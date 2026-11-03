//! Project image loading and the content-addressed texture cache (D-0035).
//!
//! Image nodes carry a project-relative path.  This module validates that
//! path, reads the file, decodes a PNG to straight-alpha RGBA8 and hashes the
//! **file bytes** with SHA-256.  The hash is the texture key: the same file at
//! two paths is one texture, and a changed file is a new key.
//!
//! PNG is the only required format.  The decoder expands palette, greyscale
//! and 16-bit images to RGBA8 (16-bit keeps the high byte); gAMA and iCCP are
//! ignored, so every image is treated as sRGB.  JPEG/WebP need a separate
//! dependency decision and are rejected.
//!
//! The cache is owned by the application: core, DSL and eval never read a
//! file.  Recreate the store on reload so a changed file is re-hashed.

use std::collections::HashMap;
use std::fmt;
use std::io::Cursor;
use std::path::{Component, Path, PathBuf};

use motioner_core::{ImageAsset, ImageKey, ImageResolver};
use sha2::{Digest, Sha256};

/// One decoded image, straight-alpha sRGB RGBA8.
#[derive(Debug, Clone)]
pub struct DecodedImage {
    /// SHA-256 of the file bytes.
    pub key: ImageKey,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` straight-alpha sRGB bytes.
    pub rgba8: Vec<u8>,
}

/// Why an image could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageError {
    /// An absolute path, a `..` escape or another non-project path.
    InvalidPath {
        /// The path as written in the document.
        source: String,
    },
    /// The file could not be read.
    Io {
        /// The resolved path.
        path: PathBuf,
        /// Underlying message.
        message: String,
    },
    /// The file is not a PNG this decoder understands.
    Decode {
        /// The resolved path.
        path: PathBuf,
        /// Underlying message.
        message: String,
    },
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath { source } => write!(
                f,
                "image {source:?} must be a project-relative path without '..'"
            ),
            Self::Io { path, message } => {
                write!(f, "cannot read image {}: {message}", path.display())
            }
            Self::Decode { path, message } => {
                write!(f, "cannot decode image {}: {message}", path.display())
            }
        }
    }
}

impl std::error::Error for ImageError {}

/// The project image store: path validation, PNG decoding and a byte cache.
#[derive(Debug, Default)]
pub struct ImageStore {
    root: PathBuf,
    by_key: HashMap<ImageKey, DecodedImage>,
    by_path: HashMap<String, ImageKey>,
}

impl ImageStore {
    /// A store rooted at the project directory (where `assets/` lives).
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            by_key: HashMap::new(),
            by_path: HashMap::new(),
        }
    }

    /// The project root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Loads one project-relative image, reusing the cache when possible.
    ///
    /// # Errors
    ///
    /// Returns [`ImageError`] for an invalid path, a missing/unreadable file or
    /// an undecodable PNG.
    pub fn load(&mut self, source: &str) -> Result<ImageAsset, ImageError> {
        if let Some(key) = self.by_path.get(source) {
            if let Some(image) = self.by_key.get(key) {
                return Ok(asset_of(image));
            }
        }
        let path = self.resolve_path(source)?;
        let bytes = std::fs::read(&path).map_err(|error| ImageError::Io {
            path: path.clone(),
            message: error.to_string(),
        })?;
        let digest: [u8; 32] = Sha256::digest(&bytes).into();
        let key = ImageKey::from_bytes(digest);
        if let Some(image) = self.by_key.get(&key) {
            // The same bytes at another path: one texture.
            self.by_path.insert(source.to_owned(), key);
            return Ok(asset_of(image));
        }
        let image = decode_png(&path, key, &bytes)?;
        let asset = asset_of(&image);
        self.by_key.insert(key, image);
        self.by_path.insert(source.to_owned(), key);
        Ok(asset)
    }

    /// The decoded image for `key`, when it has been loaded.
    #[must_use]
    pub fn get(&self, key: &ImageKey) -> Option<&DecodedImage> {
        self.by_key.get(key)
    }

    /// Every decoded image (cloned; the caller uploads them to the renderer).
    #[must_use]
    pub fn decoded(&self) -> Vec<DecodedImage> {
        self.by_key.values().cloned().collect()
    }

    /// Total resident decoded bytes.
    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.by_key
            .values()
            .map(|image| u64::from(image.width) * u64::from(image.height) * 4)
            .sum()
    }

    /// Validates and joins a project-relative path.
    fn resolve_path(&self, source: &str) -> Result<PathBuf, ImageError> {
        let path = Path::new(source);
        let invalid = || ImageError::InvalidPath {
            source: source.to_owned(),
        };
        if source.is_empty() || path.is_absolute() {
            return Err(invalid());
        }
        for component in path.components() {
            match component {
                Component::Normal(_) | Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(invalid())
                }
            }
        }
        Ok(self.root.join(path))
    }
}

impl ImageResolver for ImageStore {
    fn resolve(&self, source: &str) -> Result<ImageAsset, String> {
        // The trait takes `&self`; the cache is behind a clone-on-write map in
        // the application, so this path re-reads without caching.  The app
        // calls `ImageStore::load` directly (through `Document::resolve_images`
        // with a `&mut` store wrapped in a resolver below).
        let path = self
            .resolve_path(source)
            .map_err(|error| error.to_string())?;
        let bytes = std::fs::read(&path).map_err(|error| {
            ImageError::Io {
                path: path.clone(),
                message: error.to_string(),
            }
            .to_string()
        })?;
        let digest: [u8; 32] = Sha256::digest(&bytes).into();
        let key = ImageKey::from_bytes(digest);
        if let Some(image) = self.by_key.get(&key) {
            return Ok(asset_of(image));
        }
        let image = decode_png(&path, key, &bytes).map_err(|error| error.to_string())?;
        Ok(asset_of(&image))
    }
}

/// A resolver that caches through a `RefCell`, for the `&self` trait.
#[derive(Debug)]
pub struct CachedImages {
    store: std::cell::RefCell<ImageStore>,
}

impl CachedImages {
    /// Wraps a store.
    #[must_use]
    pub fn new(store: ImageStore) -> Self {
        Self {
            store: std::cell::RefCell::new(store),
        }
    }

    /// The inner store.
    #[must_use]
    pub fn into_inner(self) -> ImageStore {
        self.store.into_inner()
    }

    /// A borrow of the store.
    pub fn with<R>(&self, f: impl FnOnce(&ImageStore) -> R) -> R {
        f(&self.store.borrow())
    }
}

impl ImageResolver for CachedImages {
    fn resolve(&self, source: &str) -> Result<ImageAsset, String> {
        self.store
            .borrow_mut()
            .load(source)
            .map_err(|error| error.to_string())
    }

    fn decoded(&self) -> Vec<motioner_core::DecodedImage> {
        self.store
            .borrow()
            .by_key
            .values()
            .map(|image| motioner_core::DecodedImage {
                asset: asset_of(image),
                rgba8: image.rgba8.clone(),
            })
            .collect()
    }
}

fn asset_of(image: &DecodedImage) -> ImageAsset {
    ImageAsset {
        key: image.key,
        width: image.width,
        height: image.height,
    }
}

/// Decodes a PNG to straight-alpha RGBA8.
///
/// `normalize_to_color8` expands palettes and tRNS, lifts sub-8-bit greyscale
/// to 8 bits and strips 16-bit channels to their high byte.  The remaining
/// greyscale/RGB variants are expanded to RGBA here.
fn decode_png(path: &Path, key: ImageKey, bytes: &[u8]) -> Result<DecodedImage, ImageError> {
    let decode_error = |message: String| ImageError::Decode {
        path: path.to_path_buf(),
        message,
    };
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .map_err(|error| decode_error(error.to_string()))?;
    let mut buffer = vec![0_u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|error| decode_error(error.to_string()))?;
    let data = &buffer[..info.buffer_size()];
    let width = info.width;
    let height = info.height;
    let mut rgba8 = Vec::with_capacity(width as usize * height as usize * 4);
    match info.color_type {
        png::ColorType::Rgba => rgba8.extend_from_slice(data),
        png::ColorType::Rgb => {
            for texel in data.chunks_exact(3) {
                rgba8.extend_from_slice(&[texel[0], texel[1], texel[2], 255]);
            }
        }
        png::ColorType::Grayscale => {
            for &value in data {
                rgba8.extend_from_slice(&[value, value, value, 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for texel in data.chunks_exact(2) {
                rgba8.extend_from_slice(&[texel[0], texel[0], texel[0], texel[1]]);
            }
        }
        png::ColorType::Indexed => {
            // `normalize_to_color8` should have expanded this; keep a typed
            // error instead of guessing a palette.
            return Err(decode_error(
                "indexed PNG was not expanded to RGBA by the decoder".to_owned(),
            ));
        }
    }
    let expected = width as usize * height as usize * 4;
    if rgba8.len() != expected {
        return Err(decode_error(format!(
            "decoded {} bytes for a {width}x{height} image, expected {expected}",
            rgba8.len()
        )));
    }
    Ok(DecodedImage {
        key,
        width,
        height,
        rgba8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::write_png;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("motioner_images_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn loads_a_png_and_hashes_the_bytes() {
        let dir = temp_dir("load");
        let path = dir.join("a.png");
        write_png(&path, 2, 1, &[255, 0, 0, 255, 0, 0, 255, 255]).unwrap();
        let mut store = ImageStore::new(&dir);
        let asset = store.load("a.png").unwrap();
        assert_eq!((asset.width, asset.height), (2, 1));
        let image = store.get(&asset.key).unwrap();
        assert_eq!(image.rgba8, [255, 0, 0, 255, 0, 0, 255, 255]);
        assert_eq!(store.total_bytes(), 8);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_same_bytes_at_two_paths_are_one_key() {
        let dir = temp_dir("same");
        let bytes = [1u8, 2, 3, 255];
        write_png(&dir.join("a.png"), 1, 1, &bytes).unwrap();
        write_png(&dir.join("b.png"), 1, 1, &bytes).unwrap();
        let mut store = ImageStore::new(&dir);
        let a = store.load("a.png").unwrap();
        let b = store.load("b.png").unwrap();
        assert_eq!(a.key, b.key);
        assert_eq!(store.decoded().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn paths_must_stay_inside_the_project() {
        let dir = temp_dir("paths");
        let store = ImageStore::new(&dir);
        assert!(matches!(
            store.resolve_path("../x.png"),
            Err(ImageError::InvalidPath { .. })
        ));
        assert!(matches!(
            store.resolve_path("C:/x.png"),
            Err(ImageError::InvalidPath { .. })
        ));
        assert!(store.resolve_path("assets/x.png").is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_file_names_the_path() {
        let dir = temp_dir("missing");
        let mut store = ImageStore::new(&dir);
        let error = store.load("assets/nope.png").unwrap_err();
        let text = error.to_string();
        assert!(text.contains("nope.png"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
