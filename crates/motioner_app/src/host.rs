//! The desktop implementation of [`motioner_ui::ProjectHost`] (D-0045).
//!
//! Project folders, the native folder picker, the recent list, the asset
//! resolvers and ffmpeg all live in `motioner_io` or in this crate; the
//! window only sees the trait.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use std::collections::HashMap;

use motioner_core::{ImageResolver, NodeContent};
use motioner_io::{
    AppState, AssetKind, FfmpegLocation, Project, ProjectImages, ProjectOptions, ProjectSettings,
    RecentProject,
};
use motioner_render::FontSet;
use motioner_ui::{AssetKindUi, AssetSummary, NewProjectRequest, ProjectHost, StartEntry};

/// Desktop project services.
pub struct Host {
    state: RefCell<AppState>,
    state_path: PathBuf,
    root: PathBuf,
    audio: RefCell<Option<crate::audio_out::AudioOutput>>,
    video_cache: crate::video_cache::VideoCache,
    proxies: RefCell<HashMap<String, std::sync::Arc<std::sync::atomic::AtomicU32>>>,
    proxy_done: RefCell<HashMap<String, std::time::Instant>>,
    /// The last opened project and its manifest mtime, so the per-frame video
    /// path does not re-read and validate `project.motion` 60 times a second.
    project_cache: RefCell<Option<(PathBuf, Option<std::time::SystemTime>, Project)>>,
}

impl Host {
    /// Loads the application state from `%APPDATA%\Motioner\app.motion`.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        let state_path = AppState::default_path();
        let state = AppState::load(&state_path).unwrap_or_default();
        Self {
            state: RefCell::new(state),
            state_path,
            root,
            audio: RefCell::new(None),
            video_cache: crate::video_cache::VideoCache::new(),
            proxies: RefCell::new(HashMap::new()),
            proxy_done: RefCell::new(HashMap::new()),
            project_cache: RefCell::new(None),
        }
    }

    /// The loaded application state.
    #[must_use]
    pub fn state(&self) -> AppState {
        self.state.borrow().clone()
    }

    /// Decoded-video bytes resident in the bounded cache.
    #[must_use]
    pub fn video_cache_bytes(&self) -> usize {
        self.video_cache.bytes()
    }

    /// Opens a project, reusing the last one while its manifest is unchanged.
    fn project_for(&self, composition: &Path) -> Option<Project> {
        let root = motioner_io::project_root(composition)?;
        let manifest = root.join(motioner_io::PROJECT_FILE);
        let mtime = std::fs::metadata(&manifest)
            .and_then(|metadata| metadata.modified())
            .ok();
        {
            let cache = self.project_cache.borrow();
            if let Some((path, cached_mtime, project)) = cache.as_ref() {
                if path == composition && *cached_mtime == mtime {
                    return Some(project.clone());
                }
            }
        }
        let project = Project::open(composition).ok()?;
        *self.project_cache.borrow_mut() =
            Some((composition.to_path_buf(), mtime, project.clone()));
        Some(project)
    }

    /// Starts a proxy job for `hash` once, when the file does not exist yet.
    fn ensure_proxy(&self, project: &Project, hash: &str, source: &Path) {
        if crate::video_cache::has_proxy(project, hash) {
            return;
        }
        let mut proxies = self.proxies.borrow_mut();
        if proxies.contains_key(hash) {
            return;
        }
        let duration = motioner_io::probe_video(source, &FfmpegLocation::FromEnvOrPath)
            .map_or(0.0, |info| info.duration);
        let destination = crate::video_cache::proxy_path(project, hash);
        let job =
            crate::video_cache::spawn_proxy(source.to_path_buf(), destination, duration.max(0.1));
        proxies.insert(hash.to_owned(), job);
    }

    /// Persists the application state; failures are not fatal.
    pub fn save_state(&self) {
        if let Some(parent) = self.state_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = self.state.borrow().save(&self.state_path);
    }

    fn touch(&self, path: &Path, name: &str) {
        self.state
            .borrow_mut()
            .touch_recent(path, name, std::time::SystemTime::now());
        self.save_state();
    }

    /// Recent entries, rebuilding the "missing" flag from disk.
    fn recent_entries(&self) -> Vec<StartEntry> {
        self.state
            .borrow()
            .recent
            .iter()
            .map(|entry: &RecentProject| {
                let missing = !entry.path.is_dir() && !entry.path.is_file();
                let now = std::time::SystemTime::now();
                let detail = if missing {
                    format!("missing: {}", entry.path.display())
                } else {
                    format!(
                        "last opened {}",
                        motioner_io::relative_time(&entry.opened, now)
                    )
                };
                let tooltip = if missing {
                    entry.path.display().to_string()
                } else {
                    motioner_io::full_local_date(&entry.opened)
                };
                StartEntry::recent(entry.path.clone(), entry.name.clone(), detail, missing)
                    .with_tooltip(tooltip)
            })
            .collect()
    }
}

impl ProjectHost for Host {
    fn recents(&self) -> Vec<StartEntry> {
        self.recent_entries()
    }

    fn examples(&self) -> Vec<StartEntry> {
        let dir = self.root.join("examples");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("motion"))
            })
            .collect();
        files.sort();
        files
            .into_iter()
            .map(|path| {
                let name = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("example")
                    .to_owned();
                StartEntry::recent(path, name, "example program".to_owned(), false)
            })
            .collect()
    }

    fn pick_folder(&self, title: &str, start: Option<&Path>) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new().set_title(title);
        if let Some(start) = start.filter(|path| path.is_dir()) {
            dialog = dialog.set_directory(start);
        }
        dialog.pick_folder()
    }

    fn create_project(&self, request: &NewProjectRequest) -> Result<PathBuf, String> {
        let dir = request.location.join(format!("{}.motioner", request.name));
        let mut settings = ProjectSettings::new(request.name.clone());
        settings.width = request.width;
        settings.height = request.height;
        settings.fps_numerator = request.fps;
        settings.fps_denominator = 1;
        settings.duration = request.duration;
        settings.background.clone_from(&request.background);
        let mut options = ProjectOptions::new(request.name.clone());
        options.program = motioner_io::default_program(&settings, "Main");
        options.settings = settings;
        let project = Project::create(&dir, &options).map_err(|error| error.to_string())?;
        self.touch(project.root(), &request.name);
        Ok(project.root().to_path_buf())
    }

    fn open_project(&self, path: &Path) -> Result<PathBuf, String> {
        match Project::open(path) {
            Ok(project) => {
                let name = project.settings().name.clone();
                let composition = project.composition_path(&project.settings().main);
                self.touch(project.root(), &name);
                Ok(composition)
            }
            Err(motioner_io::ProjectError::NotFound { .. })
                if path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("motion")) =>
            {
                Ok(path.to_path_buf())
            }
            Err(error) => Err(error.to_string()),
        }
    }

    fn save_project(&self, composition: &Path) -> Result<PathBuf, String> {
        let project = Project::open(composition).map_err(|error| error.to_string())?;
        project.save().map_err(|error| error.to_string())?;
        Ok(project.root().to_path_buf())
    }

    fn save_as(&self, composition: &Path, destination: &Path) -> Result<PathBuf, String> {
        let project = Project::open(composition).map_err(|error| error.to_string())?;
        let name = destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("copy");
        let target = if destination
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension == "motioner")
        {
            destination.to_path_buf()
        } else {
            destination.join(format!("{name}.motioner"))
        };
        if target.exists() {
            return Err(format!("{} already exists", target.display()));
        }
        copy_dir(project.root(), &target).map_err(|error| error.to_string())?;
        let copy = Project::open(&target).map_err(|error| error.to_string())?;
        self.touch(copy.root(), &copy.settings().name);
        Ok(copy.composition_path(&copy.settings().main))
    }

    fn forget_recent(&self, path: &Path) {
        self.state.borrow_mut().remove_recent(path);
        self.save_state();
    }

    fn reveal(&self, composition: &Path) {
        let folder = Project::open(composition).map_or_else(
            |_| composition.parent().map(Path::to_path_buf),
            |project| Some(project.root().to_path_buf()),
        );
        if let Some(folder) = folder {
            let _ = std::process::Command::new("explorer").arg(folder).spawn();
        }
    }

    fn fonts(&self, composition: &Path) -> FontSet {
        let mut fonts = FontSet::bundled();
        if let Ok(project) = Project::open(composition) {
            if let Ok(faces) = motioner_io::project_fonts(&project) {
                for face in faces {
                    fonts.push_face(face.family, face.weight, face.bytes);
                }
            }
            let dir = project.root().join("assets/fonts");
            if let Ok(faces) = motioner_io::load_project_fonts(&dir) {
                for face in faces {
                    fonts.push_face(face.family, face.weight, face.bytes);
                }
            }
            return fonts;
        }
        let root = composition.parent().unwrap_or(Path::new("."));
        if let Ok(faces) = motioner_io::load_project_fonts(&root.join("assets/fonts")) {
            for face in faces {
                fonts.push_face(face.family, face.weight, face.bytes);
            }
        }
        fonts
    }

    fn images(&self, composition: &Path) -> Rc<dyn ImageResolver> {
        match Project::open(composition) {
            Ok(project) => Rc::new(ProjectImages::new(&project)),
            Err(_) => Rc::new(ProjectImages::loose(
                composition.parent().unwrap_or(Path::new(".")),
            )),
        }
    }

    fn videos(&self, composition: &Path) -> Option<Rc<dyn motioner_core::VideoResolver>> {
        match Project::open(composition) {
            Ok(project) => Some(Rc::new(ProjectImages::new(&project))),
            Err(_) => Some(Rc::new(ProjectImages::loose(
                composition.parent().unwrap_or(Path::new(".")),
            ))),
        }
    }

    fn asset_resolver(&self, composition: &Path) -> Rc<dyn motioner_dsl::AssetResolver> {
        // Same root rule as the CLI: the project root, or the application's
        // working directory for a loose program.
        let root = motioner_io::project_root(composition).unwrap_or_else(|| self.root.clone());
        Rc::new(motioner_io::ProjectAssets::new(root))
    }

    fn assets(&self, composition: &Path) -> Vec<AssetSummary> {
        let Ok(project) = Project::open(composition) else {
            return Vec::new();
        };
        let used = used_keys(&project);
        project
            .assets()
            .iter()
            .map(|entry| summary(&project, entry, used.get(&entry.key).copied().unwrap_or(0)))
            .collect()
    }

    fn import_asset(&self, composition: &Path, source: &Path) -> Result<AssetSummary, String> {
        let mut project = Project::open(composition).map_err(|error| error.to_string())?;
        let outcome = project
            .import_file(source)
            .map_err(|error| error.to_string())?;
        let entry = outcome.entry;
        let ffmpeg = FfmpegLocation::FromEnvOrPath;
        if entry.kind == AssetKind::Image {
            let extension = Path::new(&entry.original)
                .extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if extension != "png" {
                let target = project.cache_dir().join(format!("{}.png", entry.hash));
                motioner_io::first_frame_png(
                    &project.assets_dir().join(entry.file_name()),
                    &target,
                    &ffmpeg,
                )
                .map_err(|error| error.to_string())?;
            }
        }
        if entry.kind == AssetKind::Audio {
            let _ = self.waveform(composition, &entry.key, 2048);
            let _ = motioner_io::decode_to_cache(
                &project.assets_dir().join(entry.file_name()),
                &project.cache_dir(),
                &entry.hash,
                &ffmpeg,
            );
        }
        let used = used_keys(&project);
        Ok(summary(
            &project,
            &entry,
            used.get(&entry.key).copied().unwrap_or(0),
        ))
    }

    fn remove_asset(&self, composition: &Path, key: &str) -> Result<(), String> {
        let mut project = Project::open(composition).map_err(|error| error.to_string())?;
        project
            .remove_asset(key)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn rename_asset(&self, composition: &Path, key: &str, name: &str) -> Result<(), String> {
        let mut project = Project::open(composition).map_err(|error| error.to_string())?;
        project
            .rename_asset(key, name)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn edit_mode(&self, composition: &Path) -> bool {
        let Ok(project) = Project::open(composition) else {
            return false;
        };
        motioner_io::EditorState::load(project.root()).is_ok_and(|state| state.overrides_mode)
    }

    fn set_edit_mode(&self, composition: &Path, overrides: bool) -> Result<(), String> {
        let project = Project::open(composition).map_err(|error| error.to_string())?;
        let mut state =
            motioner_io::EditorState::load(project.root()).map_err(|error| error.to_string())?;
        state.overrides_mode = overrides;
        state
            .save(project.root())
            .map_err(|error| error.to_string())
    }

    fn traces_enabled(&self, composition: &Path) -> bool {
        if std::env::var("MOTIONER_TRACES").is_ok_and(|value| value == "1") {
            return true;
        }
        let setting = traces_setting_raw(composition);
        if cfg!(debug_assertions) {
            setting.unwrap_or(true)
        } else {
            setting.unwrap_or(false)
        }
    }

    fn traces_setting(&self, composition: &Path) -> bool {
        traces_setting_raw(composition).unwrap_or(false)
    }

    fn set_traces(&self, composition: &Path, on: bool) -> Result<(), String> {
        let project = Project::open(composition).map_err(|error| error.to_string())?;
        let mut state =
            motioner_io::EditorState::load(project.root()).map_err(|error| error.to_string())?;
        state.traces = Some(on);
        state
            .save(project.root())
            .map_err(|error| error.to_string())
    }

    fn record_trace(
        &self,
        composition: &Path,
        trace: &motioner_ui::TraceInput<'_>,
    ) -> Result<(), String> {
        let project = Project::open(composition).map_err(|error| error.to_string())?;
        let mut record = motioner_io::TraceRecord::new(
            trace.before,
            trace.after,
            trace.command,
            trace.strategy,
            trace.diff,
        );
        trace.file.clone_into(&mut record.file);
        record.frame = trace.frame;
        record.selection = trace.selection.map(str::to_owned);
        motioner_io::append_trace(project.root(), &record)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn place_asset(
        &self,
        composition: &Path,
        key: &str,
        frame: Option<i64>,
        pos: Option<(f64, f64)>,
    ) -> Result<(), String> {
        let project = Project::open(composition).map_err(|error| error.to_string())?;
        let entry = project
            .find_asset(key)
            .ok_or_else(|| format!("no asset has the key {key:?}"))?;
        match entry.kind {
            AssetKind::Image | AssetKind::Audio | AssetKind::Video => {}
            AssetKind::Font => return Err("fonts are used by text layers, not placed".to_owned()),
        }
        let path = project.composition_path(&project.settings().main);
        let source = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let (x, y) = pos.unwrap_or_else(|| {
            (
                f64::from(project.settings().width) * 0.5,
                f64::from(project.settings().height) * 0.5,
            )
        });
        let variable = unique_variable(&source, key);
        let mut text = source;
        if !text.ends_with('\n') {
            text.push('\n');
        }
        {
            use std::fmt::Write as _;
            match entry.kind {
                AssetKind::Image => {
                    let _ = write!(
                        text,
                        "\n# asset {:?} ({})\n{variable} = image({:?})\n{variable}.pos = ({x:.1}, {y:.1})\n",
                        entry.key, entry.original, entry.original,
                    );
                }
                AssetKind::Audio => {
                    let start = frame.unwrap_or(0);
                    let _ = write!(
                        text,
                        "\n# asset {:?} ({})\n{variable} = audio({:?}, start={start})\n",
                        entry.key, entry.original, entry.original,
                    );
                }
                AssetKind::Video => {
                    let start = frame.unwrap_or(0);
                    let _ = write!(
                        text,
                        "\n# asset {:?} ({})\n{variable} = video({:?}, start={start})\n{variable}.pos = ({x:.1}, {y:.1})\n",
                        entry.key, entry.original, entry.original,
                    );
                }
                AssetKind::Font => {}
            }
            if let Some(frame) = frame {
                let _ = writeln!(text, "# placed at frame {frame}");
            }
        }
        motioner_io::write_atomic(&path, &text).map_err(|error| error.to_string())
    }

    fn unused_assets(&self, composition: &Path) -> Result<Vec<String>, String> {
        let project = Project::open(composition).map_err(|error| error.to_string())?;
        let used = used_keys(&project);
        Ok(project
            .assets()
            .iter()
            .filter(|entry| !used.contains_key(&entry.key))
            .map(|entry| entry.key.clone())
            .collect())
    }

    fn remove_unused_assets(&self, composition: &Path) -> Result<usize, String> {
        let mut project = Project::open(composition).map_err(|error| error.to_string())?;
        let used = used_keys(&project);
        let unused: Vec<String> = project
            .assets()
            .iter()
            .filter(|entry| !used.contains_key(&entry.key))
            .map(|entry| entry.key.clone())
            .collect();
        let count = unused.len();
        for key in unused {
            project
                .remove_asset(&key)
                .map_err(|error| error.to_string())?;
        }
        Ok(count)
    }

    fn pick_files(&self, title: &str) -> Vec<PathBuf> {
        rfd::FileDialog::new()
            .set_title(title)
            .add_filter(
                "Media",
                &[
                    "png", "jpg", "jpeg", "webp", "gif", "bmp", "tga", "ttf", "otf", "wav", "mp3",
                    "flac", "ogg", "m4a", "mp4", "mov", "mkv", "webm", "avi",
                ],
            )
            .pick_files()
            .unwrap_or_default()
    }

    fn begin_session(&self, composition: &Path) {
        if let Ok(project) = Project::open(composition) {
            let _ = project.begin_session();
        }
    }

    fn end_session(&self, composition: &Path) {
        if let Ok(project) = Project::open(composition) {
            project.end_session();
        }
    }

    fn heartbeat(&self, composition: &Path) {
        if let Ok(project) = Project::open(composition) {
            let _ = project.heartbeat();
        }
    }

    fn snapshot(&self, composition: &Path) -> Result<String, String> {
        let project = Project::open(composition).map_err(|error| error.to_string())?;
        project
            .snapshot(std::time::SystemTime::now())
            .map(|info| info.name)
            .map_err(|error| error.to_string())
    }

    fn recovery(&self, composition: &Path) -> Option<motioner_ui::RecoveryInfo> {
        let project = Project::open(composition).ok()?;
        let backup = project.pending_recovery().ok()??;
        Some(motioner_ui::RecoveryInfo {
            name: backup.name.clone(),
            detail: format!("{} files, {} bytes", backup.files, backup.bytes),
        })
    }

    fn restore(&self, composition: &Path) -> Result<(), String> {
        let project = Project::open(composition).map_err(|error| error.to_string())?;
        let backup = project
            .pending_recovery()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "no newer backup to restore".to_owned())?;
        project.restore(&backup).map_err(|error| error.to_string())
    }

    fn audio_play(
        &self,
        composition: &Path,
        document: &motioner_core::Document,
        from: motioner_core::Frame,
        muted: &[String],
    ) -> Result<(), String> {
        self.audio.borrow_mut().take();
        let project = Project::open(composition).map_err(|error| error.to_string())?;
        let output = crate::audio_out::AudioOutput::start(&project, document, from, muted, false)?;
        *self.audio.borrow_mut() = Some(output);
        Ok(())
    }

    fn audio_stop(&self) {
        self.audio.borrow_mut().take();
    }

    fn audio_position(&self) -> Option<f64> {
        self.audio
            .borrow()
            .as_ref()
            .and_then(crate::audio_out::AudioOutput::position)
    }

    fn audio_ended(&self) -> bool {
        self.audio
            .borrow()
            .as_ref()
            .is_some_and(crate::audio_out::AudioOutput::ended)
    }

    fn audio_scrub(
        &self,
        composition: &Path,
        document: &motioner_core::Document,
        from: motioner_core::Frame,
        muted: &[String],
    ) {
        self.audio.borrow_mut().take();
        let Ok(project) = Project::open(composition) else {
            return;
        };
        if let Ok(output) =
            crate::audio_out::AudioOutput::start(&project, document, from, muted, true)
        {
            *self.audio.borrow_mut() = Some(output);
        }
    }

    fn video_frame(
        &self,
        composition: &Path,
        source: &str,
        source_frame: i64,
        preview: bool,
    ) -> motioner_ui::VideoFrameState {
        let Some(project) = self.project_for(composition) else {
            return motioner_ui::VideoFrameState::Unavailable;
        };
        let Some(entry) = crate::video_cache::video_entry(&project, source) else {
            return motioner_ui::VideoFrameState::Unavailable;
        };
        if preview {
            self.ensure_proxy(&project, &entry.hash, &entry.path);
        }
        match crate::video_cache::decode(&project, &entry, &self.video_cache, source_frame, preview)
        {
            crate::video_cache::FrameLookup::Ready(image) => {
                motioner_ui::VideoFrameState::Ready(image)
            }
            crate::video_cache::FrameLookup::Pending => motioner_ui::VideoFrameState::Pending,
            crate::video_cache::FrameLookup::Unavailable => {
                motioner_ui::VideoFrameState::Unavailable
            }
        }
    }

    fn video_cache_bytes(&self) -> usize {
        Host::video_cache_bytes(self)
    }

    fn proxy_progress(&self, composition: &Path, source: &str) -> Option<f32> {
        let project = self.project_for(composition)?;
        let entry = crate::video_cache::video_entry(&project, source)?;
        if crate::video_cache::has_proxy(&project, &entry.hash) {
            // Keep the completed indicator on screen briefly: the job can
            // finish between two UI frames.
            let mut done = self.proxy_done.borrow_mut();
            let started = *done
                .entry(entry.hash.clone())
                .or_insert_with(std::time::Instant::now);
            return (started.elapsed() < std::time::Duration::from_secs(3)).then_some(1.0);
        }
        let proxies = self.proxies.borrow();
        let job = proxies.get(&entry.hash)?;
        #[allow(clippy::cast_precision_loss)]
        Some(job.load(std::sync::atomic::Ordering::Relaxed) as f32 / 10_000.0)
    }

    fn video_thumbnails(
        &self,
        composition: &Path,
        source: &str,
        count: usize,
    ) -> Vec<motioner_core::DecodedImage> {
        let Ok(project) = Project::open(composition) else {
            return Vec::new();
        };
        let Some(entry) = crate::video_cache::video_entry(&project, source) else {
            return Vec::new();
        };
        crate::video_cache::thumbnails(&project, &entry, count)
    }

    fn waveform(&self, composition: &Path, key: &str, buckets: usize) -> Vec<f32> {
        let Ok(project) = Project::open(composition) else {
            return Vec::new();
        };
        let Some(entry) = project.find_asset(key) else {
            return Vec::new();
        };
        let cache = project.cache_dir().join(format!("{}.peaks", entry.hash));
        if let Ok(text) = std::fs::read_to_string(&cache) {
            if let Some(peaks) = motioner_io::peaks_from_text(&text) {
                return resample(&peaks.peaks, buckets);
            }
        }
        let source = project.assets_dir().join(entry.file_name());
        match motioner_io::audio_peaks(&source, buckets, &FfmpegLocation::FromEnvOrPath) {
            Ok(peaks) => {
                let _ = std::fs::create_dir_all(project.cache_dir());
                let _ = std::fs::write(&cache, motioner_io::peaks_to_text(&peaks));
                peaks.peaks
            }
            Err(_) => Vec::new(),
        }
    }
}

/// One asset summary for the panel.
fn summary(project: &Project, entry: &motioner_io::AssetEntry, used_by: usize) -> AssetSummary {
    let path = project.assets_dir().join(entry.file_name());
    let size_bytes = std::fs::metadata(&path).map_or(0, |metadata| metadata.len());
    let cached_peaks = if entry.kind == AssetKind::Audio {
        let cache = project.cache_dir().join(format!("{}.peaks", entry.hash));
        std::fs::read_to_string(&cache)
            .ok()
            .and_then(|text| motioner_io::peaks_from_text(&text))
    } else {
        None
    };
    let duration = if entry.kind == AssetKind::Video {
        motioner_io::probe_video(&path, &FfmpegLocation::FromEnvOrPath)
            .ok()
            .map(|info| info.duration)
    } else {
        cached_peaks.as_ref().map(|peaks| peaks.duration)
    };
    let kind = match entry.kind {
        AssetKind::Image => AssetKindUi::Image,
        AssetKind::Font => AssetKindUi::Font,
        AssetKind::Audio => AssetKindUi::Audio,
        AssetKind::Video => AssetKindUi::Video,
    };
    let thumbs = if entry.kind == AssetKind::Video {
        crate::video_cache::video_entry(project, &entry.key)
            .map(|video| {
                crate::video_cache::thumbnails(project, &video, 6)
                    .into_iter()
                    .enumerate()
                    .map(|(index, frame)| motioner_ui::VideoThumb {
                        width: frame.asset.width,
                        height: frame.asset.height,
                        rgba8: frame.rgba8,
                        #[allow(clippy::cast_precision_loss)]
                        seconds: duration.unwrap_or(0.0) * (index as f64 + 0.5) / 6.0,
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    AssetSummary {
        key: entry.key.clone(),
        original: entry.original.clone(),
        kind,
        size_bytes,
        duration,
        used_by,
        peaks: cached_peaks.map(|peaks| peaks.peaks).unwrap_or_default(),
        peak_levels: Vec::new(),
        thumbs,
    }
}

/// Number of layers per asset key across every composition.
fn used_keys(project: &Project) -> HashMap<String, usize> {
    let mut used = HashMap::new();
    for composition in project.compositions() {
        let path = project.composition_path(&composition.slug);
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        let assets = motioner_io::ProjectAssets::new(project.root());
        let Ok(document) = motioner_dsl::run_with_assets(
            &source,
            &motioner_dsl::Limits::default(),
            &motioner_core::NoTextMetrics,
            &assets,
        ) else {
            continue;
        };
        for node in document.nodes() {
            let name = match node.content() {
                NodeContent::Image(image) => image.source.as_str(),
                NodeContent::Audio(audio) => audio.source.as_str(),
                NodeContent::Video(video) => video.source.as_str(),
                _ => continue,
            };
            if let Some(entry) = project.find_asset(name) {
                *used.entry(entry.key.clone()).or_insert(0) += 1;
            }
        }
    }
    used
}

/// A variable name derived from the key that the program does not use yet.
fn unique_variable(source: &str, key: &str) -> String {
    let base: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let base = if base.is_empty() {
        "asset".to_owned()
    } else {
        base
    };
    let mut candidate = format!("{base}_layer");
    let mut suffix = 2;
    while source.contains(&format!("{candidate} ")) || source.contains(&format!("{candidate}=")) {
        candidate = format!("{base}_layer_{suffix}");
        suffix += 1;
    }
    candidate
}

/// Resamples peaks to `buckets` bars for a narrower panel.
fn resample(peaks: &[f32], buckets: usize) -> Vec<f32> {
    if peaks.len() == buckets || peaks.is_empty() {
        return peaks.to_vec();
    }
    (0..buckets)
        .map(|bucket| {
            let start = bucket * peaks.len() / buckets;
            let end = ((bucket + 1) * peaks.len() / buckets).max(start + 1);
            peaks[start..end.min(peaks.len())]
                .iter()
                .copied()
                .fold(0.0, f32::max)
        })
        .collect()
}

/// Recursively copies `from` into `to`.
fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let destination = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&source, &destination)?;
        } else {
            std::fs::copy(&source, &destination)?;
        }
    }
    Ok(())
}

/// The project's stored trace setting, before the debug/release default.
fn traces_setting_raw(composition: &Path) -> Option<bool> {
    let project = Project::open(composition).ok()?;
    let state = motioner_io::EditorState::load(project.root()).ok()?;
    state.traces
}
