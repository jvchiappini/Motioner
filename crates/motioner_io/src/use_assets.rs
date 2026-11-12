//! The file-system resolver behind `use()` (D-0051).
//!
//! Resolution is sandboxed to the project root: absolute paths, parent
//! escapes and paths that resolve outside the root are rejected before any
//! read. SVG files are parsed here (the DSL never touches the file system);
//! `.motion` module sources are returned for the interpreter to execute.

use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};

use motioner_dsl::{AssetResolver, LoadedAsset};

use crate::svg::parse_svg;

/// A resolver rooted at a project folder (or a loose file's directory).
#[derive(Debug, Clone)]
pub struct ProjectAssets {
    root: PathBuf,
}

impl ProjectAssets {
    /// A resolver that reads under `root`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The sandbox root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn resolve(&self, path: &str, base: &str) -> Result<(PathBuf, String), String> {
        let candidate = Path::new(path);
        if candidate.is_absolute() {
            return Err(format!("absolute paths are not allowed ({path})"));
        }
        let mut parts: Vec<String> = Vec::new();
        if !base.is_empty() {
            for component in Path::new(base).components() {
                match component {
                    Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
                    Component::CurDir => {}
                    _ => return Err("the module directory escapes the project".to_owned()),
                }
            }
        }
        for component in candidate.components() {
            match component {
                Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
                Component::CurDir => {}
                Component::ParentDir => {
                    if parts.pop().is_none() {
                        return Err(format!("parent escapes are not allowed ({path})"));
                    }
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(format!("absolute paths are not allowed ({path})"));
                }
            }
        }
        let mut relative = PathBuf::new();
        for part in &parts {
            relative.push(part);
        }
        Ok((self.root.join(&relative), parts.join("/")))
    }
}

impl AssetResolver for ProjectAssets {
    fn load(&self, path: &str, base: &str) -> Result<LoadedAsset, String> {
        let (full, key) = self.resolve(path, base)?;
        let extension = Path::new(path)
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        match extension.as_str() {
            "svg" => {
                let text = std::fs::read_to_string(&full)
                    .map_err(|error| format!("cannot read {}: {error}", full.display()))?;
                let name = Path::new(path)
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("symbol");
                let symbol = parse_svg(&text, name)
                    .map_err(|error| format!("{}: {error}", full.display()))?;
                Ok(LoadedAsset::Symbol {
                    key,
                    symbol: std::sync::Arc::new(symbol),
                })
            }
            "motion" => {
                let source = std::fs::read_to_string(&full)
                    .map_err(|error| format!("cannot read {}: {error}", full.display()))?;
                let dir = Path::new(&key)
                    .parent()
                    .map(|parent| parent.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();
                Ok(LoadedAsset::Module { key, source, dir })
            }
            other => {
                let mut message = String::from("use() of '");
                let _ = write!(message, "{path}'");
                if other.is_empty() {
                    message.push_str(" has no extension");
                } else {
                    let _ = write!(message, " has unsupported extension '{other}'");
                }
                Err(message)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_paths_and_parent_escapes_are_rejected() {
        let resolver = ProjectAssets::new(".");
        assert!(resolver.resolve("C:/x.svg", "").is_err());
        assert!(resolver.resolve("../x.svg", "").is_err());
        assert!(resolver.resolve("assets/../../x.svg", "").is_err());
        assert!(resolver.resolve("/etc/passwd", "").is_err());
    }

    #[test]
    fn nested_paths_resolve_under_the_root() {
        let resolver = ProjectAssets::new("project");
        let (full, key) = resolver.resolve("../assets/mascot.svg", "rigs").unwrap();
        assert!(key.ends_with("assets/mascot.svg"));
        assert!(full.ends_with("project/assets/mascot.svg"));
    }
}
