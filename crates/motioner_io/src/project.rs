//! The project folder contract (D-0045).
//!
//! A project is a directory named `<name>.motioner/` whose only authoritative
//! files are `project.motion`, `compositions/*.motion`, `assets/*` and
//! `editor.motion`. `cache/`, `backups/` and `traces/` are disposable.
//!
//! The manifest is MDF constants, so the workspace parser, diagnostics and
//! quoting serve it exactly like a composition. Every write is atomic: a temp
//! file in the same directory is renamed over the destination, so a crash
//! never leaves a half-written project.
//!
//! ```
//! let dir = std::env::temp_dir().join(format!("motioner_doc_{}", std::process::id()));
//! let options = motioner_io::ProjectOptions::new("demo");
//! let project = motioner_io::Project::create(&dir, &options).unwrap();
//! assert!(project.root().join("project.motion").is_file());
//! std::fs::remove_dir_all(project.root()).ok();
//! ```

use std::fmt;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use motioner_core::FrameRate;
use motioner_dsl::ast::{Argument, Expr, ExprKind, StmtKind};
use motioner_dsl::parse;

/// Name of the manifest file that marks a directory as a project.
pub const PROJECT_FILE: &str = "project.motion";
/// Extension of a project folder.
pub const PROJECT_EXTENSION: &str = "motioner";
/// Format version understood by this build.
pub const FORMAT_VERSION: u32 = 1;
/// Directory of composition programs.
pub const COMPOSITIONS_DIR: &str = "compositions";
/// Directory of content-addressed blobs.
pub const ASSETS_DIR: &str = "assets";
/// Directory of decoded media and thumbnails.
pub const CACHE_DIR: &str = "cache";
/// Directory of rolling backups.
pub const BACKUPS_DIR: &str = "backups";
/// Directory reserved for write-through traces (D-0036).
pub const TRACES_DIR: &str = "traces";
/// Editor state file (never rendered).
pub const EDITOR_FILE: &str = "editor.motion";

/// What kind of media an asset holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssetKind {
    /// A raster image (PNG after conversion).
    Image,
    /// A static TTF/OTF face.
    Font,
    /// An audio file.
    Audio,
    /// A video file.
    Video,
}

impl AssetKind {
    /// Every kind.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Font => "font",
            Self::Audio => "audio",
            Self::Video => "video",
        }
    }

    /// The kind with this manifest name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "image" => Some(Self::Image),
            "font" => Some(Self::Font),
            "audio" => Some(Self::Audio),
            "video" => Some(Self::Video),
            _ => None,
        }
    }

    /// The kind inferred from a file extension.
    #[must_use]
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "ttf" | "otf" => Some(Self::Font),
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tga" => Some(Self::Image),
            "wav" | "mp3" | "flac" | "ogg" | "aac" | "m4a" => Some(Self::Audio),
            "mp4" | "mov" | "mkv" | "webm" | "avi" => Some(Self::Video),
            _ => None,
        }
    }
}

/// One immutable blob in the project's asset registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetEntry {
    /// Stable human key used from MDF.
    pub key: String,
    /// SHA-256 of the bytes, lower-case hex.
    pub hash: String,
    /// Original file name, including its extension.
    pub original: String,
    /// Media kind.
    pub kind: AssetKind,
    /// Import date, RFC 3339 seconds (UTC).
    pub imported: String,
}

impl AssetEntry {
    /// The blob name inside `assets/`: `<hash>.<ext>`.
    #[must_use]
    pub fn file_name(&self) -> String {
        let extension = Path::new(&self.original)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");
        if extension.is_empty() {
            self.hash.clone()
        } else {
            format!("{}.{}", self.hash, extension.to_ascii_lowercase())
        }
    }

    /// The blob path relative to the project root.
    #[must_use]
    pub fn relative_path(&self) -> String {
        format!("{ASSETS_DIR}/{}", self.file_name())
    }
}

/// One composition listed by the manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositionRef {
    /// File stem and command-line name.
    pub slug: String,
    /// Human title, shown in the editor.
    pub title: String,
}

/// The project settings stored at the top of `project.motion`.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectSettings {
    /// Project display name.
    pub name: String,
    /// Composition width in pixels.
    pub width: u32,
    /// Composition height in pixels.
    pub height: u32,
    /// Frame-rate numerator.
    pub fps_numerator: u32,
    /// Frame-rate denominator.
    pub fps_denominator: u32,
    /// Composition length in frames.
    pub duration: i64,
    /// Background colour as `#rrggbb`.
    pub background: String,
    /// Slug of the composition opened by default.
    pub main: String,
}

impl ProjectSettings {
    /// Settings for a 1080p60 five-second project named `name`.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            width: 1920,
            height: 1080,
            fps_numerator: 60,
            fps_denominator: 1,
            duration: 300,
            background: "#000000".to_owned(),
            main: "main".to_owned(),
        }
    }

    /// The frame rate as a core value.
    ///
    /// # Panics
    ///
    /// Only if a hand-built [`ProjectSettings`] has a zero denominator; the
    /// manifest parser rejects that before this point.
    #[must_use]
    pub fn frame_rate(&self) -> FrameRate {
        FrameRate::new(self.fps_numerator, self.fps_denominator).unwrap_or_else(|_| {
            // `new` only rejects a zero denominator, which the parser rejects
            // before this point; 1/1 is a safe last resort.
            FrameRate::new(1, 1).expect("1/1 is a valid rate")
        })
    }

    /// Duration in seconds.
    #[must_use]
    pub fn seconds(&self) -> f64 {
        #[allow(clippy::cast_precision_loss)] // display only
        let frames = self.duration as f64;
        self.frame_rate().as_f64().recip() * frames
    }
}

/// Everything `Project::create` needs.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectOptions {
    /// Settings written to the manifest.
    pub settings: ProjectSettings,
    /// Main composition title.
    pub main_title: String,
    /// Program text of the main composition.
    pub program: String,
}

impl ProjectOptions {
    /// Default 1080p60 five-second project with a small title program.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        let settings = ProjectSettings::new(name);
        let program = default_program(&settings, "Main");
        Self {
            settings,
            main_title: "Main".to_owned(),
            program,
        }
    }
}

/// A default composition program with one title layer.
#[must_use]
pub fn default_program(settings: &ProjectSettings, title: &str) -> String {
    let rate = settings.frame_rate();
    format!(
        "comp(name={:?}, width={}, height={}, fps={}/{}, duration={:.6}, background={:?})\n\
         \n\
         title = text({:?}, font=\"Inter\", weight=600, size=128, color=\"#ffffff\")\n\
         title.pos = ({:.1}, {:.1})\n",
        title,
        settings.width,
        settings.height,
        rate.numerator(),
        rate.denominator(),
        settings.seconds(),
        settings.background,
        title,
        f64::from(settings.width) * 0.5,
        f64::from(settings.height) * 0.5,
    )
}

/// Why a project could not be created, loaded, validated or saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectError {
    /// The path is neither a project folder nor one of its files.
    NotFound {
        /// The path that was tried.
        path: PathBuf,
    },
    /// A filesystem operation failed.
    Io {
        /// The path involved.
        path: PathBuf,
        /// Underlying message.
        message: String,
    },
    /// The manifest or a composition is not valid MDF.
    Parse {
        /// The file.
        path: PathBuf,
        /// 1-based line.
        line: u32,
        /// 1-based column.
        column: u32,
        /// The parser's message.
        message: String,
    },
    /// The manifest is valid MDF but not a valid project manifest.
    Invalid {
        /// The file.
        path: PathBuf,
        /// 1-based line, or `0` when the problem is structural.
        line: u32,
        /// What is wrong.
        message: String,
    },
    /// The manifest declares a format version this build cannot read.
    UnsupportedVersion {
        /// The file.
        path: PathBuf,
        /// Version found in the manifest.
        version: u64,
        /// Version this build writes.
        supported: u32,
    },
    /// A project already exists at the destination.
    Exists {
        /// The existing manifest.
        path: PathBuf,
    },
    /// The main composition is missing from the manifest or the disk.
    MissingComposition {
        /// The manifest.
        path: PathBuf,
        /// The missing slug.
        slug: String,
    },
    /// An asset blob named by the registry is missing.
    MissingAsset {
        /// The project root.
        root: PathBuf,
        /// Registry key.
        key: String,
        /// Blob path.
        file: String,
    },
    /// A blob's bytes do not match the hash recorded at import.
    HashMismatch {
        /// The blob path.
        path: PathBuf,
        /// Registry key.
        key: String,
    },
    /// A file type Motioner does not import.
    UnsupportedFile {
        /// The file.
        path: PathBuf,
        /// Its extension, when it has one.
        extension: String,
    },
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { path } => write!(
                f,
                "{} is not a Motioner project (no {PROJECT_FILE})",
                path.display()
            ),

            Self::Parse {
                path,
                line,
                column,
                message,
            } => write!(f, "{}:{line}:{column}: {message}", path.display()),
            Self::Invalid {
                path,
                line,
                message,
            } if *line > 0 => write!(f, "{}:{line}: {message}", path.display()),
            Self::Io { path, message } | Self::Invalid { path, message, .. } => {
                write!(f, "{}: {message}", path.display())
            }
            Self::UnsupportedVersion {
                path,
                version,
                supported,
            } => write!(
                f,
                "{}: project format version {version} is newer than the supported {supported}",
                path.display()
            ),
            Self::Exists { path } => write!(f, "a project already exists at {}", path.display()),
            Self::MissingComposition { path, slug } => {
                write!(f, "{}: composition {slug:?} is missing", path.display())
            }
            Self::MissingAsset { root, key, file } => write!(
                f,
                "{}: asset {key:?} blob {} is missing",
                root.display(),
                file
            ),
            Self::HashMismatch { path, key } => write!(
                f,
                "{}: asset {key:?} does not match its recorded hash",
                path.display()
            ),
            Self::UnsupportedFile { path, extension } => write!(
                f,
                "{}: unsupported asset type {}",
                path.display(),
                if extension.is_empty() {
                    "(none)"
                } else {
                    extension
                }
            ),
        }
    }
}

impl std::error::Error for ProjectError {}

impl ProjectError {
    /// A short machine-readable kind for JSON output.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::NotFound { .. } => "not_found",
            Self::Io { .. } => "io",
            Self::Parse { .. } => "parse",
            Self::Invalid { .. } => "invalid",
            Self::UnsupportedVersion { .. } => "unsupported_version",
            Self::Exists { .. } => "exists",
            Self::MissingComposition { .. } => "missing_composition",
            Self::MissingAsset { .. } => "missing_asset",
            Self::HashMismatch { .. } => "hash_mismatch",
            Self::UnsupportedFile { .. } => "unsupported_file",
        }
    }
}

/// A loaded project: its root, manifest and composition list.
#[derive(Debug, Clone)]
pub struct Project {
    root: PathBuf,
    settings: ProjectSettings,
    assets: Vec<AssetEntry>,
    compositions: Vec<CompositionRef>,
}

impl Project {
    /// Creates a project at `dir` and writes the manifest and main program.
    ///
    /// A `.motioner` extension is added when `dir` has none, so
    /// `motioner new demo` creates `demo.motioner/`.
    ///
    /// # Errors
    ///
    /// [`ProjectError::Exists`] when a manifest is already there,
    /// [`ProjectError::Io`] when a directory or file cannot be created.
    pub fn create(dir: &Path, options: &ProjectOptions) -> Result<Self, ProjectError> {
        let root = normalize_project_dir(dir);
        let manifest_path = root.join(PROJECT_FILE);
        if manifest_path.exists() {
            return Err(ProjectError::Exists {
                path: manifest_path,
            });
        }
        let project = Self {
            root,
            settings: options.settings.clone(),
            assets: Vec::new(),
            compositions: vec![CompositionRef {
                slug: options.settings.main.clone(),
                title: options.main_title.clone(),
            }],
        };
        for directory in [
            ASSETS_DIR,
            COMPOSITIONS_DIR,
            CACHE_DIR,
            BACKUPS_DIR,
            TRACES_DIR,
        ] {
            create_dir(&project.root.join(directory))?;
        }
        project.save()?;
        write_atomic(
            &project.composition_path(&options.settings.main),
            &options.program,
        )?;
        write_atomic(&project.editor_path(), &EditorState::default().to_mdf())?;
        Ok(project)
    }

    /// Opens a project from a folder, its manifest or a composition file.
    ///
    /// # Errors
    ///
    /// [`ProjectError::NotFound`] for a path that is not inside a project,
    /// plus every error [`Self::load_from`] can return.
    pub fn open(path: &Path) -> Result<Self, ProjectError> {
        let root = project_root(path).ok_or_else(|| ProjectError::NotFound {
            path: path.to_path_buf(),
        })?;
        Self::load_from(&root)
    }

    /// Loads and validates a project rooted at `root`.
    ///
    /// # Errors
    ///
    /// [`ProjectError`] for unreadable or invalid manifests and for
    /// references that do not exist on disk.
    pub fn load_from(root: &Path) -> Result<Self, ProjectError> {
        clean_temp(root, &[PROJECT_FILE, EDITOR_FILE]);
        let manifest_path = root.join(PROJECT_FILE);
        let text = fs::read_to_string(&manifest_path).map_err(|error| ProjectError::Io {
            path: manifest_path.clone(),
            message: error.to_string(),
        })?;
        let (settings, assets, compositions) = parse_manifest(&manifest_path, &text)?;
        let project = Self {
            root: root.to_path_buf(),
            settings,
            assets,
            compositions,
        };
        project.validate()?;
        Ok(project)
    }

    /// The project folder.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The manifest path.
    #[must_use]
    pub fn manifest_path(&self) -> PathBuf {
        self.root.join(PROJECT_FILE)
    }

    /// The settings block.
    #[must_use]
    pub fn settings(&self) -> &ProjectSettings {
        &self.settings
    }

    /// Mutable settings; call [`Self::save`] afterwards.
    pub fn settings_mut(&mut self) -> &mut ProjectSettings {
        &mut self.settings
    }

    /// The asset registry.
    #[must_use]
    pub fn assets(&self) -> &[AssetEntry] {
        &self.assets
    }

    /// Mutable registry, for the asset module.
    pub(crate) fn assets_mut(&mut self) -> &mut Vec<AssetEntry> {
        &mut self.assets
    }

    /// Keeps the registry in canonical key order.
    pub(crate) fn sort_assets(&mut self) {
        self.assets.sort_by(|left, right| {
            left.key
                .to_ascii_lowercase()
                .cmp(&right.key.to_ascii_lowercase())
        });
    }

    /// The composition list.
    #[must_use]
    pub fn compositions(&self) -> &[CompositionRef] {
        &self.compositions
    }

    /// The folder holding composition programs.
    #[must_use]
    pub fn compositions_dir(&self) -> PathBuf {
        self.root.join(COMPOSITIONS_DIR)
    }

    /// Program path of `slug`.
    #[must_use]
    pub fn composition_path(&self, slug: &str) -> PathBuf {
        self.compositions_dir().join(format!("{slug}.motion"))
    }

    /// Overrides path of `slug` (D-0022).
    #[must_use]
    pub fn overrides_path(&self, slug: &str) -> PathBuf {
        self.compositions_dir()
            .join(format!("{slug}.overrides.motion"))
    }

    /// Folder of immutable blobs.
    #[must_use]
    pub fn assets_dir(&self) -> PathBuf {
        self.root.join(ASSETS_DIR)
    }

    /// Disposable cache folder.
    #[must_use]
    pub fn cache_dir(&self) -> PathBuf {
        self.root.join(CACHE_DIR)
    }

    /// Rolling backup folder.
    #[must_use]
    pub fn backups_dir(&self) -> PathBuf {
        self.root.join(BACKUPS_DIR)
    }

    /// Editor state file.
    #[must_use]
    pub fn editor_path(&self) -> PathBuf {
        self.root.join(EDITOR_FILE)
    }

    /// Program text of the main composition.
    ///
    /// # Errors
    ///
    /// [`ProjectError::Io`] when the file cannot be read.
    pub fn main_source(&self) -> Result<String, ProjectError> {
        let path = self.composition_path(&self.settings.main);
        fs::read_to_string(&path).map_err(|error| ProjectError::Io {
            path,
            message: error.to_string(),
        })
    }

    /// Finds a registry entry by key or original file name.
    #[must_use]
    pub fn find_asset(&self, name: &str) -> Option<&AssetEntry> {
        self.assets.iter().find(|entry| {
            entry.key.eq_ignore_ascii_case(name) || entry.original.eq_ignore_ascii_case(name)
        })
    }

    /// The registry entry for a content hash.
    #[must_use]
    pub fn asset_by_hash(&self, hash: &str) -> Option<&AssetEntry> {
        self.assets.iter().find(|entry| entry.hash == hash)
    }

    /// Resolves a document source (`"logo.png"`, `"logo"`) to a project
    /// relative blob path.
    #[must_use]
    pub fn resolve_source(&self, source: &str) -> Option<String> {
        let direct = self.find_asset(source).map(AssetEntry::relative_path);
        direct.or_else(|| {
            Path::new(source)
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| self.find_asset(name))
                .map(AssetEntry::relative_path)
        })
    }

    /// Checks version, composition and asset references.
    ///
    /// # Errors
    ///
    /// The first broken reference, named with its path.
    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.compositions.is_empty() {
            return Err(ProjectError::Invalid {
                path: self.manifest_path(),
                line: 0,
                message: "the manifest lists no composition".to_owned(),
            });
        }
        if !self
            .compositions
            .iter()
            .any(|composition| composition.slug == self.settings.main)
        {
            return Err(ProjectError::MissingComposition {
                path: self.manifest_path(),
                slug: self.settings.main.clone(),
            });
        }
        for composition in &self.compositions {
            let path = self.composition_path(&composition.slug);
            if !path.is_file() {
                return Err(ProjectError::MissingComposition {
                    path: self.manifest_path(),
                    slug: composition.slug.clone(),
                });
            }
        }
        for entry in &self.assets {
            let path = self.root.join(entry.relative_path());
            if !path.is_file() {
                return Err(ProjectError::MissingAsset {
                    root: self.root.clone(),
                    key: entry.key.clone(),
                    file: entry.relative_path(),
                });
            }
        }
        Ok(())
    }

    /// Writes the manifest atomically in its canonical order.
    ///
    /// # Errors
    ///
    /// [`ProjectError::Io`] when the file cannot be written.
    pub fn save(&self) -> Result<(), ProjectError> {
        write_atomic(&self.manifest_path(), &self.to_mdf())
    }

    /// The manifest as MDF text.
    #[must_use]
    pub fn to_mdf(&self) -> String {
        let settings = &self.settings;
        let mut text = String::new();
        let _ = writeln!(
            text,
            "project(version={}, name={}, width={}, height={}, fps={}, duration={}, background={}, main={})",
            FORMAT_VERSION,
            quote(&settings.name),
            settings.width,
            settings.height,
            fps_text(settings.fps_numerator, settings.fps_denominator),
            settings.duration,
            quote(&settings.background),
            quote(&settings.main),
        );
        let mut assets: Vec<&AssetEntry> = self.assets.iter().collect();
        assets.sort_by(|left, right| left.key.cmp(&right.key));
        for entry in assets {
            let _ = writeln!(
                text,
                "asset(key={}, hash={}, original={}, kind={}, imported={})",
                quote(&entry.key),
                quote(&entry.hash),
                quote(&entry.original),
                quote(entry.kind.name()),
                quote(&entry.imported),
            );
        }
        let mut compositions: Vec<&CompositionRef> = self.compositions.iter().collect();
        compositions.sort_by(|left, right| left.slug.cmp(&right.slug));
        for composition in compositions {
            let _ = writeln!(
                text,
                "composition(slug={}, title={})",
                quote(&composition.slug),
                quote(&composition.title),
            );
        }
        text
    }
}

/// Editor state stored in `editor.motion`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditorState {
    /// Last shown frame.
    pub last_frame: i64,
    /// Locked node identities, sorted.
    pub locked: Vec<String>,
    /// Expanded node identities, sorted.
    pub expanded: Vec<String>,
    /// Inspector scroll offset in logical pixels.
    pub inspector_scroll: u32,
}

impl EditorState {
    /// The file text.
    #[must_use]
    pub fn to_mdf(&self) -> String {
        let locked = self
            .locked
            .iter()
            .map(|id| quote(id))
            .collect::<Vec<_>>()
            .join(", ");
        let expanded = self
            .expanded
            .iter()
            .map(|id| quote(id))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "editor(last_frame={}, locked=[{}], expanded=[{}], inspector_scroll={})\n",
            self.last_frame, locked, expanded, self.inspector_scroll
        )
    }

    /// Parses editor state, falling back to defaults for an unknown file.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut state = Self::default();
        let Ok(program) = parse(text) else {
            return state;
        };
        for statement in program.body {
            if let StmtKind::Expr(Expr {
                kind: ExprKind::Call { callee, args },
                ..
            }) = statement.kind
            {
                if call_name(&callee) != Some("editor") {
                    continue;
                }
                for argument in args {
                    let Some(name) = argument.name.as_deref() else {
                        continue;
                    };
                    match (name, &argument.value.kind) {
                        ("last_frame", ExprKind::Number(value)) => {
                            #[allow(clippy::cast_possible_truncation)]
                            let frame = *value as i64;
                            state.last_frame = frame;
                        }
                        ("inspector_scroll", ExprKind::Number(value)) => {
                            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                            let scroll = value.max(0.0) as u32;
                            state.inspector_scroll = scroll;
                        }
                        ("locked", ExprKind::List(items)) => {
                            state.locked = string_list(items);
                        }
                        ("expanded", ExprKind::List(items)) => {
                            state.expanded = string_list(items);
                        }
                        _ => {}
                    }
                }
            }
        }
        state
    }

    /// Loads the editor state next to a project root.
    ///
    /// # Errors
    ///
    /// [`ProjectError::Io`] for a file that exists but cannot be read.
    pub fn load(root: &Path) -> Result<Self, ProjectError> {
        let path = root.join(EDITOR_FILE);
        match fs::read_to_string(&path) {
            Ok(text) => Ok(Self::parse(&text)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(ProjectError::Io {
                path,
                message: error.to_string(),
            }),
        }
    }

    /// Writes the editor state atomically.
    ///
    /// # Errors
    ///
    /// [`ProjectError::Io`] when the file cannot be written.
    pub fn save(&self, root: &Path) -> Result<(), ProjectError> {
        write_atomic(&root.join(EDITOR_FILE), &self.to_mdf())
    }
}

/// One project in the recent list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentProject {
    /// Absolute folder path.
    pub path: PathBuf,
    /// Display name.
    pub name: String,
    /// Last opened, RFC 3339 seconds (UTC).
    pub opened: String,
}

/// Window placement stored with the application state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowPlacement {
    /// Window width in physical pixels.
    pub width: u32,
    /// Window height in physical pixels.
    pub height: u32,
    /// Outer x position.
    pub x: i32,
    /// Outer y position.
    pub y: i32,
    /// Whether the window was maximised.
    pub maximized: bool,
}

impl Default for WindowPlacement {
    fn default() -> Self {
        Self {
            width: 1600,
            height: 900,
            x: 0,
            y: 0,
            maximized: false,
        }
    }
}

/// Application-level state shared by every project (D-0045).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppState {
    /// Window placement.
    pub window: WindowPlacement,
    /// Recent projects, most recent first (at most [`Self::MAX_RECENT`]).
    pub recent: Vec<RecentProject>,
}

impl AppState {
    /// Maximum number of remembered projects.
    pub const MAX_RECENT: usize = 12;

    /// The default state file: `%APPDATA%\Motioner\app.motion` on Windows,
    /// `$XDG_CONFIG_HOME/motioner/app.motion` (or `~/.config/...`) elsewhere.
    #[must_use]
    pub fn default_path() -> PathBuf {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("Motioner").join("app.motion");
        }
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .unwrap_or_default();
        base.join("motioner").join("app.motion")
    }

    /// Loads the state, returning defaults when the file does not exist.
    ///
    /// # Errors
    ///
    /// [`ProjectError::Io`] for a file that exists but cannot be read.
    pub fn load(path: &Path) -> Result<Self, ProjectError> {
        match fs::read_to_string(path) {
            Ok(text) => Ok(Self::parse(&text)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(ProjectError::Io {
                path: path.to_path_buf(),
                message: error.to_string(),
            }),
        }
    }

    /// Writes the state atomically.
    ///
    /// # Errors
    ///
    /// [`ProjectError::Io`] when the file cannot be written.
    pub fn save(&self, path: &Path) -> Result<(), ProjectError> {
        write_atomic(path, &self.to_mdf())
    }

    /// Records `path` as opened now, moving it to the front.
    pub fn touch_recent(&mut self, path: &Path, name: &str, now: SystemTime) {
        let path = path.to_path_buf();
        self.recent.retain(|entry| entry.path != path);
        self.recent.insert(
            0,
            RecentProject {
                path,
                name: name.to_owned(),
                opened: format_utc(now),
            },
        );
        self.recent.truncate(Self::MAX_RECENT);
    }

    /// Removes a recent entry by path.
    pub fn remove_recent(&mut self, path: &Path) {
        self.recent.retain(|entry| entry.path != path);
    }

    /// The state as MDF text.
    #[must_use]
    pub fn to_mdf(&self) -> String {
        let window = self.window;
        let mut text = format!(
            "app(version={}, window_width={}, window_height={}, window_x={}, window_y={}, maximized={})\n",
            FORMAT_VERSION,
            window.width,
            window.height,
            window.x,
            window.y,
            if window.maximized { "True" } else { "False" },
        );
        for entry in &self.recent {
            let _ = writeln!(
                text,
                "recent(path={}, name={}, opened={})",
                quote(&path_text(&entry.path)),
                quote(&entry.name),
                quote(&entry.opened),
            );
        }
        text
    }

    /// Parses state text; unknown calls are ignored so an older app still
    /// opens a newer state file.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut state = Self::default();
        let Ok(program) = parse(text) else {
            return state;
        };
        for statement in program.body {
            let StmtKind::Expr(Expr {
                kind: ExprKind::Call { callee, args },
                ..
            }) = statement.kind
            else {
                continue;
            };
            match call_name(&callee) {
                Some("app") => {
                    for argument in args {
                        apply_window(&mut state.window, &argument);
                    }
                }
                Some("recent") => {
                    let mut path = None;
                    let mut name = None;
                    let mut opened = None;
                    for argument in args {
                        let Some(key) = argument.name.as_deref() else {
                            continue;
                        };
                        if let ExprKind::Str(value) = &argument.value.kind {
                            match key {
                                "path" => path = Some(PathBuf::from(value.as_ref())),
                                "name" => name = Some(value.to_string()),
                                "opened" => opened = Some(value.to_string()),
                                _ => {}
                            }
                        }
                    }
                    if let (Some(path), Some(name)) = (path, name) {
                        state.recent.push(RecentProject {
                            path,
                            name,
                            opened: opened.unwrap_or_default(),
                        });
                    }
                }
                _ => {}
            }
        }
        state.recent.truncate(Self::MAX_RECENT);
        state
    }
}

fn apply_window(window: &mut WindowPlacement, argument: &Argument) {
    let Some(name) = argument.name.as_deref() else {
        return;
    };
    match (&argument.value.kind, name) {
        (ExprKind::Number(value), "window_width") => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                window.width = value.max(0.0) as u32;
            }
        }
        (ExprKind::Number(value), "window_height") => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                window.height = value.max(0.0) as u32;
            }
        }
        (ExprKind::Number(value), "window_x") => {
            #[allow(clippy::cast_possible_truncation)]
            {
                window.x = *value as i32;
            }
        }
        (ExprKind::Number(value), "window_y") => {
            #[allow(clippy::cast_possible_truncation)]
            {
                window.y = *value as i32;
            }
        }
        (ExprKind::Bool(value), "maximized") => window.maximized = *value,
        _ => {}
    }
}

/// The project root of `path` when it is a folder, a manifest or a file
/// inside a project.
#[must_use]
pub fn project_root(path: &Path) -> Option<PathBuf> {
    let is_manifest = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == PROJECT_FILE);
    if path.is_dir() {
        return path
            .join(PROJECT_FILE)
            .is_file()
            .then(|| path.to_path_buf());
    }
    if is_manifest {
        return path.parent().map(Path::to_path_buf);
    }
    let mut current = path.parent();
    while let Some(directory) = current {
        if directory.join(PROJECT_FILE).is_file() {
            return Some(directory.to_path_buf());
        }
        current = directory.parent();
    }
    None
}

/// Adds the `.motioner` extension when `dir` has none.
#[must_use]
pub fn normalize_project_dir(dir: &Path) -> PathBuf {
    match dir.extension().and_then(|extension| extension.to_str()) {
        Some(PROJECT_EXTENSION) => dir.to_path_buf(),
        _ => dir.with_extension(PROJECT_EXTENSION),
    }
}

/// The UTC timestamp of `time` as RFC 3339 seconds.
#[must_use]
pub fn format_utc(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    format_utc_seconds(seconds)
}

/// Formats a Unix timestamp as `YYYY-MM-DDTHH:MM:SSZ` (proleptic Gregorian).
#[must_use]
pub fn format_utc_seconds(seconds: u64) -> String {
    #[allow(clippy::cast_possible_wrap)]
    let seconds = seconds as i64;
    let days = seconds.div_euclid(86_400);
    let of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = of_day / 3600;
    let minute = (of_day % 3600) / 60;
    let second = of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 to a date.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    {
        (year, m as u32, d as u32)
    }
}

/// Prints an fps as a whole number when possible, else `"num/den"`.
fn fps_text(numerator: u32, denominator: u32) -> String {
    if denominator == 1 {
        numerator.to_string()
    } else {
        quote(&format!("{numerator}/{denominator}"))
    }
}

/// `#rrggbb` quoting (colour strings stay strings in the manifest).
fn quote(text: &str) -> String {
    motioner_core::quote(text)
}

/// The text form of a path with forward slashes.
#[must_use]
pub fn path_text(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn call_name(callee: &Expr) -> Option<&str> {
    match &callee.kind {
        ExprKind::Name(name) => Some(name),
        _ => None,
    }
}

fn string_list(items: &[Expr]) -> Vec<String> {
    let mut values: Vec<String> = items
        .iter()
        .filter_map(|item| match &item.kind {
            ExprKind::Str(value) => Some(value.to_string()),
            _ => None,
        })
        .collect();
    values.sort();
    values
}

fn parse_manifest(
    path: &Path,
    text: &str,
) -> Result<(ProjectSettings, Vec<AssetEntry>, Vec<CompositionRef>), ProjectError> {
    let program = parse(text).map_err(|diagnostic| ProjectError::Parse {
        path: path.to_path_buf(),
        line: diagnostic.span.line,
        column: diagnostic.span.column,
        message: diagnostic.message,
    })?;
    let mut settings = None;
    let mut assets = Vec::new();
    let mut compositions = Vec::new();
    for statement in program.body {
        let line = statement.span.line;
        let StmtKind::Expr(Expr {
            kind: ExprKind::Call { callee, args },
            ..
        }) = statement.kind
        else {
            return Err(ProjectError::Invalid {
                path: path.to_path_buf(),
                line,
                message: "the manifest only holds constant calls".to_owned(),
            });
        };
        match call_name(&callee) {
            Some("project") => {
                if settings.is_some() {
                    return Err(ProjectError::Invalid {
                        path: path.to_path_buf(),
                        line,
                        message: "project(...) appears more than once".to_owned(),
                    });
                }
                settings = Some(project_settings(path, line, &args)?);
            }
            Some("asset") => assets.push(asset_entry(path, line, &args)?),
            Some("composition") => compositions.push(composition_ref(path, line, &args)?),
            Some(other) => {
                return Err(ProjectError::Invalid {
                    path: path.to_path_buf(),
                    line,
                    message: format!("unknown manifest call {other}(...)"),
                })
            }
            None => {
                return Err(ProjectError::Invalid {
                    path: path.to_path_buf(),
                    line,
                    message: "manifest calls must be plain names".to_owned(),
                })
            }
        }
    }
    let settings = settings.ok_or_else(|| ProjectError::Invalid {
        path: path.to_path_buf(),
        line: 0,
        message: "the manifest has no project(...) block".to_owned(),
    })?;
    let mut keys = std::collections::HashSet::new();
    for entry in &assets {
        if !keys.insert(entry.key.to_ascii_lowercase()) {
            return Err(ProjectError::Invalid {
                path: path.to_path_buf(),
                line: 0,
                message: format!("duplicate asset key {:?}", entry.key),
            });
        }
    }
    Ok((settings, assets, compositions))
}

fn project_settings(
    path: &Path,
    line: u32,
    args: &[Argument],
) -> Result<ProjectSettings, ProjectError> {
    let mut version = None;
    let mut settings = ProjectSettings::new("project");
    for argument in args {
        let Some(name) = argument.name.as_deref() else {
            return Err(invalid_arg(
                path,
                line,
                "project(...) takes keyword arguments",
            ));
        };
        match (name, &argument.value.kind) {
            ("version", ExprKind::Number(value)) => version = Some(number_u64(*value)),
            ("name", ExprKind::Str(value)) => settings.name = value.to_string(),
            ("width", ExprKind::Number(value)) => {
                settings.width = number_u32(path, line, "width", *value)?;
            }
            ("height", ExprKind::Number(value)) => {
                settings.height = number_u32(path, line, "height", *value)?;
            }
            ("fps", ExprKind::Number(value)) => {
                settings.fps_numerator = number_u32(path, line, "fps", *value)?;
                settings.fps_denominator = 1;
            }
            ("fps", ExprKind::Str(value)) => {
                let (numerator, denominator) = parse_rate(path, line, value)?;
                settings.fps_numerator = numerator;
                settings.fps_denominator = denominator;
            }
            ("duration", ExprKind::Number(value)) => {
                #[allow(clippy::cast_possible_truncation)]
                let duration = *value as i64;
                if duration <= 0 {
                    return Err(invalid_arg(path, line, "duration must be positive"));
                }
                settings.duration = duration;
            }
            ("background", ExprKind::Str(value)) => settings.background = value.to_string(),
            ("main", ExprKind::Str(value)) => settings.main = value.to_string(),
            (other, _) => {
                return Err(invalid_arg(
                    path,
                    line,
                    &format!("unknown project field {other:?}"),
                ))
            }
        }
    }
    let version = version.ok_or_else(|| invalid_arg(path, line, "project(...) needs version"))?;
    if version != u64::from(FORMAT_VERSION) {
        return Err(ProjectError::UnsupportedVersion {
            path: path.to_path_buf(),
            version,
            supported: FORMAT_VERSION,
        });
    }
    if settings.width == 0 || settings.height == 0 {
        return Err(invalid_arg(path, line, "width and height must be positive"));
    }
    Ok(settings)
}

fn asset_entry(path: &Path, line: u32, args: &[Argument]) -> Result<AssetEntry, ProjectError> {
    let mut key = None;
    let mut hash = None;
    let mut original = None;
    let mut kind = None;
    let mut imported = None;
    for argument in args {
        let Some(name) = argument.name.as_deref() else {
            return Err(invalid_arg(
                path,
                line,
                "asset(...) takes keyword arguments",
            ));
        };
        match (name, &argument.value.kind) {
            ("key", ExprKind::Str(value)) => key = Some(value.to_string()),
            ("hash", ExprKind::Str(value)) => hash = Some(value.to_string()),
            ("original", ExprKind::Str(value)) => original = Some(value.to_string()),
            ("kind", ExprKind::Str(value)) => {
                kind = AssetKind::from_name(value);
                if kind.is_none() {
                    return Err(invalid_arg(
                        path,
                        line,
                        &format!("unknown asset kind {value:?}"),
                    ));
                }
            }
            ("imported", ExprKind::Str(value)) => imported = Some(value.to_string()),
            (other, _) => {
                return Err(invalid_arg(
                    path,
                    line,
                    &format!("unknown asset field {other:?}"),
                ))
            }
        }
    }
    let entry = AssetEntry {
        key: key.ok_or_else(|| invalid_arg(path, line, "asset(...) needs key"))?,
        hash: hash.ok_or_else(|| invalid_arg(path, line, "asset(...) needs hash"))?,
        original: original.ok_or_else(|| invalid_arg(path, line, "asset(...) needs original"))?,
        kind: kind.ok_or_else(|| invalid_arg(path, line, "asset(...) needs kind"))?,
        imported: imported.unwrap_or_default(),
    };
    if entry.hash.len() != 64 || !entry.hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(invalid_arg(path, line, "asset hash must be 64 hex digits"));
    }
    Ok(entry)
}

fn composition_ref(
    path: &Path,
    line: u32,
    args: &[Argument],
) -> Result<CompositionRef, ProjectError> {
    let mut slug = None;
    let mut title = None;
    for argument in args {
        let Some(name) = argument.name.as_deref() else {
            return Err(invalid_arg(
                path,
                line,
                "composition(...) takes keyword arguments",
            ));
        };
        match (name, &argument.value.kind) {
            ("slug", ExprKind::Str(value)) => slug = Some(value.to_string()),
            ("title", ExprKind::Str(value)) => title = Some(value.to_string()),
            (other, _) => {
                return Err(invalid_arg(
                    path,
                    line,
                    &format!("unknown composition field {other:?}"),
                ))
            }
        }
    }
    let slug = slug.ok_or_else(|| invalid_arg(path, line, "composition(...) needs slug"))?;
    if slug.is_empty()
        || !slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(invalid_arg(
            path,
            line,
            "a composition slug uses letters, digits, '-' and '_'",
        ));
    }
    Ok(CompositionRef {
        title: title.unwrap_or_else(|| slug.clone()),
        slug,
    })
}

fn parse_rate(path: &Path, line: u32, text: &str) -> Result<(u32, u32), ProjectError> {
    let (numerator, denominator) = text
        .split_once('/')
        .ok_or_else(|| invalid_arg(path, line, "fps must be a number or \"num/den\""))?;
    let numerator = numerator
        .trim()
        .parse::<u32>()
        .map_err(|_| invalid_arg(path, line, "invalid fps numerator"))?;
    let denominator = denominator
        .trim()
        .parse::<u32>()
        .map_err(|_| invalid_arg(path, line, "invalid fps denominator"))?;
    if numerator == 0 || denominator == 0 {
        return Err(invalid_arg(path, line, "fps parts must be positive"));
    }
    Ok((numerator, denominator))
}

fn number_u64(value: f64) -> u64 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        value.max(0.0) as u64
    }
}

fn number_u32(path: &Path, line: u32, field: &str, value: f64) -> Result<u32, ProjectError> {
    if !(1.0..=f64::from(u32::MAX)).contains(&value) || value.fract() != 0.0 {
        return Err(invalid_arg(
            path,
            line,
            &format!("{field} must be a positive whole number"),
        ));
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(value as u32)
}

fn invalid_arg(path: &Path, line: u32, message: &str) -> ProjectError {
    ProjectError::Invalid {
        path: path.to_path_buf(),
        line,
        message: message.to_owned(),
    }
}

/// Removes `<name>.tmp` siblings left by an interrupted atomic write.
fn clean_temp(root: &Path, names: &[&str]) {
    for name in names {
        let _ = fs::remove_file(root.join(format!(".{name}.tmp")));
    }
}

fn create_dir(path: &Path) -> Result<(), ProjectError> {
    fs::create_dir_all(path).map_err(|error| ProjectError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

/// Writes `text` through a sibling temp file and a rename.
///
/// # Errors
///
/// [`ProjectError::Io`] when the directory or file cannot be written.
pub fn write_atomic(path: &Path, text: &str) -> Result<(), ProjectError> {
    write_atomic_bytes(path, text.as_bytes())
}

/// Writes `bytes` through a sibling temp file and a rename.
///
/// # Errors
///
/// [`ProjectError::Io`] when the directory or file cannot be written.
pub fn write_atomic_bytes(path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            create_dir(parent)?;
        }
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let temp = path.with_file_name(format!(".{file_name}.tmp"));
    fs::write(&temp, bytes).map_err(|error| ProjectError::Io {
        path: temp.clone(),
        message: error.to_string(),
    })?;
    if let Err(error) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(ProjectError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("motioner_project_{}_{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn creates_a_project_and_round_trips_the_manifest() {
        let dir = temp_dir("create");
        let options = ProjectOptions::new("demo");
        let project = Project::create(&dir, &options).unwrap();
        assert!(project
            .root()
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("create.motioner")));
        assert!(project.composition_path("main").is_file());
        assert!(project.cache_dir().is_dir());
        let reloaded = Project::open(project.root()).unwrap();
        assert_eq!(reloaded.settings(), project.settings());
        assert_eq!(reloaded.to_mdf(), project.to_mdf());
        let first = fs::read_to_string(reloaded.manifest_path()).unwrap();
        reloaded.save().unwrap();
        let second = fs::read_to_string(reloaded.manifest_path()).unwrap();
        assert_eq!(first, second);
        let _ = fs::remove_dir_all(project.root());
    }

    #[test]
    fn opens_from_a_composition_file() {
        let dir = temp_dir("composition");
        let project = Project::create(&dir, &ProjectOptions::new("demo")).unwrap();
        let opened = Project::open(&project.composition_path("main")).unwrap();
        assert_eq!(opened.root(), project.root());
        let _ = fs::remove_dir_all(project.root());
    }

    #[test]
    fn rejects_unknown_fields_and_newer_versions() {
        let dir = temp_dir("bad");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(PROJECT_FILE);
        fs::write(
            &path,
            "project(version=1, name=\"x\", width=1920, height=1080, fps=60, duration=10, background=\"#000000\", main=\"main\", extra=1)\n",
        )
        .unwrap();
        let error = Project::load_from(&dir).unwrap_err();
        assert!(error.to_string().contains("extra"), "{error}");

        fs::write(
            &path,
            "project(version=9, name=\"x\", width=1920, height=1080, fps=60, duration=10, background=\"#000000\", main=\"main\")\n",
        )
        .unwrap();
        assert!(matches!(
            Project::load_from(&dir).unwrap_err(),
            ProjectError::UnsupportedVersion { version: 9, .. }
        ));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validation_names_the_missing_asset() {
        let dir = temp_dir("missing");
        let mut project = Project::create(&dir, &ProjectOptions::new("demo")).unwrap();
        project.assets.push(AssetEntry {
            key: "logo".to_owned(),
            hash: "a".repeat(64),
            original: "logo.png".to_owned(),
            kind: AssetKind::Image,
            imported: "2026-11-03T00:00:00Z".to_owned(),
        });
        project.save().unwrap();
        let error = Project::load_from(project.root()).unwrap_err();
        assert!(matches!(error, ProjectError::MissingAsset { .. }));
        let _ = fs::remove_dir_all(project.root());
    }

    #[test]
    fn editor_state_round_trips() {
        let state = EditorState {
            last_frame: 120,
            locked: vec!["a".to_owned(), "b".to_owned()],
            expanded: vec!["c".to_owned()],
            inspector_scroll: 42,
        };
        let text = state.to_mdf();
        assert_eq!(EditorState::parse(&text), state);
    }

    #[test]
    fn app_state_round_trips_and_caps_recent() {
        let mut state = AppState::default();
        for index in 0..AppState::MAX_RECENT + 4 {
            state.touch_recent(
                Path::new(&format!("C:/work/p{index}.motioner")),
                &format!("p{index}"),
                SystemTime::UNIX_EPOCH,
            );
        }
        assert_eq!(state.recent.len(), AppState::MAX_RECENT);
        let text = state.to_mdf();
        let parsed = AppState::parse(&text);
        assert_eq!(parsed, state);
        assert_eq!(parsed.recent[0].name, "p15");
    }

    #[test]
    fn timestamps_are_utc_rfc3339() {
        assert_eq!(format_utc_seconds(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_utc_seconds(1_700_000_000), "2023-11-14T22:13:20Z");
    }

    #[test]
    fn a_leftover_temp_file_is_ignored_on_open() {
        let dir = temp_dir("leftover");
        let project = Project::create(&dir, &ProjectOptions::new("demo")).unwrap();
        std::fs::write(project.root().join(".project.motion.tmp"), "half").unwrap();
        let reloaded = Project::open(project.root()).unwrap();
        assert_eq!(reloaded.settings().name, "demo");
        assert!(!project.root().join(".project.motion.tmp").exists());
        let _ = fs::remove_dir_all(project.root());
    }

    #[test]
    fn atomic_writes_leave_no_temp_file() {
        let dir = temp_dir("atomic");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("project.motion");
        write_atomic(&path, "one").unwrap();
        write_atomic(&path, "two").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "two");
        assert!(!dir.join(".project.motion.tmp").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
