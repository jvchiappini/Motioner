//! Project assets: import, registry mutation and media resolution (D-0045).
//!
//! Importing hashes the bytes with SHA-256, copies them to
//! `assets/<hash>.<extension>` (deduplicating by hash), assigns a stable
//! human key and rewrites `project.motion` atomically. The original file is
//! never modified and the blob is immutable.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use motioner_core::{ImageAsset, ImageResolver};
use sha2::{Digest, Sha256};

use crate::project::{
    format_utc, write_atomic_bytes, AssetEntry, AssetKind, Project, ProjectError,
};

/// Result of importing one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportOutcome {
    /// The registry entry, new or already present.
    pub entry: AssetEntry,
    /// Whether the bytes were already in the project.
    pub deduped: bool,
}

/// Why an asset could not be imported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetError {
    /// The project rejected the write.
    Project(ProjectError),
    /// The source file could not be read.
    Source {
        /// The file.
        path: PathBuf,
        /// Underlying message.
        message: String,
    },
    /// The file type is not importable.
    Unsupported {
        /// The file.
        path: PathBuf,
        /// Its extension.
        extension: String,
    },
    /// No asset has that key.
    UnknownKey {
        /// The key that was asked for.
        key: String,
    },
    /// A key cannot be empty or contain separators.
    BadKey {
        /// The rejected key.
        key: String,
    },
    /// The key is already used by another asset.
    KeyInUse {
        /// The key that is taken.
        key: String,
    },
}

impl fmt::Display for AssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Project(error) => write!(f, "{error}"),
            Self::Source { path, message } => {
                write!(f, "cannot read {}: {message}", path.display())
            }
            Self::Unsupported { path, extension } => write!(
                f,
                "{}: unsupported asset type {extension:?}",
                path.display()
            ),
            Self::UnknownKey { key } => write!(f, "no asset has the key {key:?}"),
            Self::BadKey { key } => write!(
                f,
                "asset key {key:?} must be non-empty and use letters, digits, '-' or '_'"
            ),
            Self::KeyInUse { key } => write!(f, "asset key {key:?} is already in use"),
        }
    }
}

impl std::error::Error for AssetError {}

impl From<ProjectError> for AssetError {
    fn from(error: ProjectError) -> Self {
        Self::Project(error)
    }
}

impl Project {
    /// Imports one file into `assets/` and registers it.
    ///
    /// # Errors
    ///
    /// [`AssetError::Source`] for an unreadable file,
    /// [`AssetError::Unsupported`] for a type Motioner does not import, and
    /// [`AssetError::Project`] when the blob or manifest cannot be written.
    pub fn import_file(&mut self, source: &Path) -> Result<ImportOutcome, AssetError> {
        let extension = source
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let kind =
            AssetKind::from_extension(&extension).ok_or_else(|| AssetError::Unsupported {
                path: source.to_path_buf(),
                extension: extension.clone(),
            })?;
        let bytes = std::fs::read(source).map_err(|error| AssetError::Source {
            path: source.to_path_buf(),
            message: error.to_string(),
        })?;
        self.import_bytes(
            &bytes,
            source
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("asset"),
            kind,
            SystemTime::now(),
        )
    }

    /// Imports bytes that are already in memory.
    ///
    /// # Errors
    ///
    /// [`AssetError::Project`] when the blob or manifest cannot be written.
    pub fn import_bytes(
        &mut self,
        bytes: &[u8],
        original: &str,
        kind: AssetKind,
        now: SystemTime,
    ) -> Result<ImportOutcome, AssetError> {
        let hash = sha256_hex(bytes);
        if let Some(entry) = self.asset_by_hash(&hash) {
            return Ok(ImportOutcome {
                entry: entry.clone(),
                deduped: true,
            });
        }
        let entry = AssetEntry {
            key: self.unique_key(original),
            hash,
            original: original.to_owned(),
            kind,
            imported: format_utc(now),
        };
        let blob = self.root().join(entry.relative_path());
        write_atomic_bytes(&blob, bytes)?;
        self.assets_mut().push(entry.clone());
        self.sort_assets();
        self.save()?;
        Ok(ImportOutcome {
            entry,
            deduped: false,
        })
    }

    /// Removes an asset from the registry and deletes its blob.
    ///
    /// # Errors
    ///
    /// [`AssetError::UnknownKey`] when the key is not registered, and
    /// [`AssetError::Project`] when the manifest cannot be saved.
    pub fn remove_asset(&mut self, key: &str) -> Result<AssetEntry, AssetError> {
        let index = self
            .assets()
            .iter()
            .position(|entry| entry.key.eq_ignore_ascii_case(key))
            .ok_or_else(|| AssetError::UnknownKey {
                key: key.to_owned(),
            })?;
        let entry = self.assets_mut().remove(index);
        let blob = self.root().join(entry.relative_path());
        match std::fs::remove_file(&blob) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(AssetError::Project(ProjectError::Io {
                    path: blob,
                    message: error.to_string(),
                }))
            }
        }
        self.save()?;
        Ok(entry)
    }

    /// Renames an asset's human key.
    ///
    /// The original file name and the blob stay untouched, so every document
    /// that refers to the asset by file name keeps working.
    ///
    /// # Errors
    ///
    /// [`AssetError::UnknownKey`] for a missing key,
    /// [`AssetError::BadKey`] for a key with separators and
    /// [`AssetError::KeyInUse`] for a duplicate.
    pub fn rename_asset(&mut self, key: &str, name: &str) -> Result<AssetEntry, AssetError> {
        if !valid_key(name) {
            return Err(AssetError::BadKey {
                key: name.to_owned(),
            });
        }
        let index = self
            .assets()
            .iter()
            .position(|entry| entry.key.eq_ignore_ascii_case(key))
            .ok_or_else(|| AssetError::UnknownKey {
                key: key.to_owned(),
            })?;
        if self
            .assets()
            .iter()
            .enumerate()
            .any(|(other, entry)| other != index && entry.key.eq_ignore_ascii_case(name))
        {
            return Err(AssetError::KeyInUse {
                key: name.to_owned(),
            });
        }
        let assets = self.assets_mut();
        name.clone_into(&mut assets[index].key);
        self.sort_assets();
        let entry = self.assets()[index].clone();
        self.save()?;
        Ok(entry)
    }

    /// Lists the registered keys, sorted.
    #[must_use]
    pub fn asset_keys(&self) -> Vec<String> {
        self.assets()
            .iter()
            .map(|entry| entry.key.clone())
            .collect()
    }

    /// A key derived from `original`, suffixed until it is unique.
    fn unique_key(&self, original: &str) -> String {
        let stem = Path::new(original)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("asset");
        let base: String = stem
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let base = if base.is_empty() {
            "asset".to_owned()
        } else {
            base
        };
        if !self
            .assets()
            .iter()
            .any(|entry| entry.key.eq_ignore_ascii_case(&base))
        {
            return base;
        }
        for suffix in 2.. {
            let candidate = format!("{base}-{suffix}");
            if !self
                .assets()
                .iter()
                .any(|entry| entry.key.eq_ignore_ascii_case(&candidate))
            {
                return candidate;
            }
        }
        unreachable!("an unbounded suffix always finds a free key")
    }
}

/// Whether `name` is a legal asset key.
#[must_use]
pub fn valid_key(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Lower-case hex SHA-256 of `bytes`.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut text = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(text, "{byte:02x}");
    }
    text
}

/// Resolves image sources through the project registry, then as project
/// relative paths (today's single-file behaviour).
///
/// The resolver caches decoded bytes so `Document::resolve_images` and the
/// renderer's upload set share one decode per blob.
pub struct ProjectImages {
    store: RefCell<crate::images::ImageStore>,
    aliases: HashMap<String, String>,
}

impl std::fmt::Debug for ProjectImages {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectImages")
            .field("store", &self.store)
            .field("aliases", &self.aliases)
            .finish()
    }
}

impl ProjectImages {
    /// Builds a resolver for `project`.
    #[must_use]
    pub fn new(project: &Project) -> Self {
        let mut aliases = HashMap::new();
        for entry in project.assets() {
            if entry.kind != AssetKind::Image {
                continue;
            }
            let relative = converted_image(project, entry).unwrap_or_else(|| entry.relative_path());
            aliases.insert(entry.key.to_ascii_lowercase(), relative.clone());
            aliases.insert(entry.original.to_ascii_lowercase(), relative.clone());
            aliases.insert(
                format!("assets/{}", entry.original).to_ascii_lowercase(),
                relative.clone(),
            );
            aliases.insert(entry.hash.clone(), relative);
        }
        Self {
            store: RefCell::new(crate::images::ImageStore::new(project.root())),
            aliases,
        }
    }

    /// A resolver for single-file mode: project-relative paths only.
    #[must_use]
    pub fn loose(root: impl Into<PathBuf>) -> Self {
        Self {
            store: RefCell::new(crate::images::ImageStore::new(root)),
            aliases: HashMap::new(),
        }
    }

    /// The decoded images, for the renderer's upload set.
    #[must_use]
    pub fn decoded(&self) -> Vec<crate::images::DecodedImage> {
        self.store.borrow().decoded()
    }

    /// The inner store.
    #[must_use]
    pub fn into_inner(self) -> crate::images::ImageStore {
        self.store.into_inner()
    }
}

impl ImageResolver for ProjectImages {
    fn resolve(&self, source: &str) -> Result<ImageAsset, String> {
        let path = self
            .aliases
            .get(&source.to_ascii_lowercase())
            .cloned()
            .unwrap_or_else(|| source.to_owned());
        self.store
            .borrow_mut()
            .load(&path)
            .map_err(|error| error.to_string())
    }

    fn decoded(&self) -> Vec<motioner_core::DecodedImage> {
        self.store
            .borrow()
            .decoded()
            .into_iter()
            .map(|image| motioner_core::DecodedImage {
                asset: ImageAsset {
                    key: image.key,
                    width: image.width,
                    height: image.height,
                },
                rgba8: image.rgba8,
            })
            .collect()
    }
}

/// The converted PNG in `cache/` for a non-PNG image, when it exists.
#[must_use]
pub fn converted_image(project: &Project, entry: &AssetEntry) -> Option<String> {
    let extension = Path::new(&entry.original)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if extension == "png" {
        return None;
    }
    let relative = format!("cache/{}.png", entry.hash);
    project.root().join(&relative).is_file().then_some(relative)
}

/// Every project font face, in key order, read through the registry.
///
/// # Errors
///
/// Returns the first unreadable blob as [`ProjectError::Io`].
pub fn project_fonts(project: &Project) -> Result<Vec<crate::ProjectFont>, ProjectError> {
    let mut fonts = Vec::new();
    for entry in project
        .assets()
        .iter()
        .filter(|entry| entry.kind == AssetKind::Font)
    {
        let path = project.root().join(entry.relative_path());
        let bytes = std::fs::read(&path).map_err(|error| ProjectError::Io {
            path,
            message: error.to_string(),
        })?;
        fonts.push(crate::fonts::font_from_original(&entry.original, bytes));
    }
    Ok(fonts)
}

/// Validates that `source` is a relative path without a parent escape.
#[must_use]
pub fn safe_relative(source: &str) -> bool {
    let path = Path::new(source);
    !source.is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectOptions;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("motioner_assets_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn png_bytes() -> Vec<u8> {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "motioner_assets_src_{}_{}.png",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        crate::write_png(&path, 1, 1, &[255, 0, 0, 255]).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        bytes
    }

    #[test]
    fn imports_and_dedupes_by_hash() {
        let dir = temp_dir("import");
        let mut project = Project::create(&dir, &ProjectOptions::new("demo")).unwrap();
        let bytes = png_bytes();
        let first = project
            .import_bytes(&bytes, "logo.png", AssetKind::Image, SystemTime::UNIX_EPOCH)
            .unwrap();
        assert!(!first.deduped);
        assert_eq!(first.entry.key, "logo");
        assert!(project.root().join(first.entry.relative_path()).is_file());
        let second = project
            .import_bytes(&bytes, "copy.png", AssetKind::Image, SystemTime::UNIX_EPOCH)
            .unwrap();
        assert!(second.deduped);
        assert_eq!(second.entry.key, "logo");
        assert_eq!(project.assets().len(), 1);
        let _ = std::fs::remove_dir_all(project.root());
    }

    #[test]
    fn keys_get_a_suffix_on_a_clash() {
        let dir = temp_dir("clash");
        let mut project = Project::create(&dir, &ProjectOptions::new("demo")).unwrap();
        let a = project
            .import_bytes(
                &png_bytes(),
                "logo.png",
                AssetKind::Image,
                SystemTime::UNIX_EPOCH,
            )
            .unwrap();
        let b = project
            .import_bytes(
                &[1, 2, 3],
                "logo.png",
                AssetKind::Image,
                SystemTime::UNIX_EPOCH,
            )
            .unwrap();
        assert_eq!(a.entry.key, "logo");
        assert_eq!(b.entry.key, "logo-2");
        let _ = std::fs::remove_dir_all(project.root());
    }

    #[test]
    fn rename_and_remove_update_the_registry() {
        let dir = temp_dir("mutate");
        let mut project = Project::create(&dir, &ProjectOptions::new("demo")).unwrap();
        project
            .import_bytes(
                &png_bytes(),
                "logo.png",
                AssetKind::Image,
                SystemTime::UNIX_EPOCH,
            )
            .unwrap();
        let renamed = project.rename_asset("logo", "brand").unwrap();
        assert_eq!(renamed.key, "brand");
        assert!(project.rename_asset("logo", "x").is_err());
        assert!(project.rename_asset("brand", "bad key").is_err());
        project.remove_asset("brand").unwrap();
        assert!(project.assets().is_empty());
        let _ = std::fs::remove_dir_all(project.root());
    }

    #[test]
    fn project_images_resolve_by_key_original_and_path() {
        let dir = temp_dir("resolve");
        let mut project = Project::create(&dir, &ProjectOptions::new("demo")).unwrap();
        let bytes = png_bytes();
        project
            .import_bytes(&bytes, "logo.png", AssetKind::Image, SystemTime::UNIX_EPOCH)
            .unwrap();
        let images = ProjectImages::new(&project);
        let by_name = images.resolve("logo.png").unwrap();
        let by_key = images.resolve("logo").unwrap();
        assert_eq!(by_name.key, by_key.key);
        assert_eq!((by_name.width, by_name.height), (1, 1));
        // A project-relative path still works.
        std::fs::write(project.root().join("loose.png"), &bytes).unwrap();
        assert!(images.resolve("loose.png").is_ok());
        assert!(images.resolve("../nope.png").is_err());
        let _ = std::fs::remove_dir_all(project.root());
    }

    #[test]
    fn project_fonts_use_the_original_name() {
        let dir = temp_dir("fonts");
        let mut project = Project::create(&dir, &ProjectOptions::new("demo")).unwrap();
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/fonts/Inter-Bold.ttf"
        ));
        project
            .import_bytes(
                bytes,
                "Mono-Bold.ttf",
                AssetKind::Font,
                SystemTime::UNIX_EPOCH,
            )
            .unwrap();
        let fonts = project_fonts(&project).unwrap();
        assert_eq!(fonts.len(), 1);
        assert_eq!(fonts[0].family, "Mono");
        assert_eq!(fonts[0].weight, 700);
        let _ = std::fs::remove_dir_all(project.root());
    }
}
