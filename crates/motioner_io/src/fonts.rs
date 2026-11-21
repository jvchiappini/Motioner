//! Project font discovery (D-0034).
//!
//! `assets/fonts/` holds static TTF/OTF faces named `<Family>-<Style>.ttf`.
//! This module reads them into plain byte records; the renderer decides how to
//! register them and the UI lists them in the inspector.

use std::fmt;
use std::path::{Path, PathBuf};

/// One static face read from the project font directory.
#[derive(Debug, Clone)]
pub struct ProjectFont {
    /// Family name inferred from the file name (`Inter`).
    pub family: String,
    /// Weight inferred from the style suffix (`SemiBold` → 600).
    pub weight: u16,
    /// Raw TTF/OTF bytes.
    pub bytes: Vec<u8>,
}

/// A font directory could not be read.
#[derive(Debug)]
pub struct FontLoadError {
    /// File or directory that failed.
    pub path: PathBuf,
    /// Underlying message.
    pub message: String,
}

impl fmt::Display for FontLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "cannot read font {}: {}",
            self.path.display(),
            self.message
        )
    }
}

impl std::error::Error for FontLoadError {}

/// Reads every `.ttf`/`.otf` file in `dir`, sorted by file name.
///
/// A missing directory is not an error: it means the project adds no faces.
/// `OFL.txt`-style licence files and other extensions are ignored.
///
/// # Errors
///
/// Returns [`FontLoadError`] when an entry cannot be read.
pub fn load_project_fonts(dir: &Path) -> Result<Vec<ProjectFont>, FontLoadError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(FontLoadError {
                path: dir.to_path_buf(),
                message: error.to_string(),
            })
        }
    };
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| FontLoadError {
            path: dir.to_path_buf(),
            message: error.to_string(),
        })?;
        let path = entry.path();
        let is_font = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ttf") || ext.eq_ignore_ascii_case("otf"));
        if is_font {
            paths.push(path);
        }
    }
    paths.sort();
    let mut fonts = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = std::fs::read(&path).map_err(|error| FontLoadError {
            path: path.clone(),
            message: error.to_string(),
        })?;
        let (family, weight) = parse_name(&path);
        fonts.push(ProjectFont {
            family,
            weight,
            bytes,
        });
    }
    Ok(fonts)
}

/// Builds a face from bytes and its original file name.
///
/// Project fonts live at `assets/<hash>.ttf`, so the family and weight come
/// from the name recorded in the registry, not from the blob name (D-0045).
#[must_use]
pub fn font_from_original(original: &str, bytes: Vec<u8>) -> ProjectFont {
    let (family, weight) = parse_name(Path::new(original));
    ProjectFont {
        family,
        weight,
        bytes,
    }
}

/// Infers `(family, weight)` from `Inter-SemiBold.ttf`.
fn parse_name(path: &Path) -> (String, u16) {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Font");
    let (family, style) = stem.rsplit_once('-').unwrap_or((stem, ""));
    let weight = match style.to_ascii_lowercase().replace([' ', '_'], "").as_str() {
        "" | "regular" | "book" | "normal" => 400,
        "thin" => 100,
        "extralight" | "ultralight" => 200,
        "light" => 300,
        "medium" => 500,
        "semibold" | "demibold" => 600,
        "bold" => 700,
        "extrabold" | "ultrabold" => 800,
        "black" | "heavy" => 900,
        other => other.parse().unwrap_or(400),
    };
    (family.to_owned(), weight)
}

#[cfg(test)]
mod tests {
    use super::parse_name;

    #[test]
    fn infers_family_and_weight() {
        assert_eq!(
            parse_name(std::path::Path::new("Inter-SemiBold.ttf")),
            ("Inter".to_owned(), 600)
        );
        assert_eq!(
            parse_name(std::path::Path::new("InterDisplay-Bold.otf")),
            ("InterDisplay".to_owned(), 700)
        );
        assert_eq!(
            parse_name(std::path::Path::new("Mono.ttf")),
            ("Mono".to_owned(), 400)
        );
    }
}
