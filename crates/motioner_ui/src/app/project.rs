//! Project-level actions of the window: the start screen, opening and
//! closing projects, saving copies and the File menu.
//!
//! All file-system work goes through [`crate::ProjectHost`]; this module only
//! swaps sessions and keeps the painter caches honest.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use ferrous_engine::{AppContext, KeyCode, MouseButton};

use super::PreviewApp;
use crate::command::Command;
use crate::editor::Editor;
use crate::layout::MenuAction;
use crate::start::{NewProjectRequest, StartAction, StartLayout, StartMode, StartScreen};
use crate::{FileWatch, PreviewSession};

impl PreviewApp {
    /// The window title: the project name, or "Motioner" on the start screen,
    /// with a dot while a write is pending.
    #[must_use]
    pub(crate) fn window_title(&self) -> String {
        if self.start.is_some() {
            return "Motioner".to_owned();
        }
        let path = self.editor.session().path();
        let name = crate::project_display_name(path);
        if self.editor.session().is_dirty() {
            format!("• {name} — Motioner")
        } else {
            format!("{name} — Motioner")
        }
    }

    /// The project host, when the application supplied one.
    pub(crate) fn host(&self) -> Option<std::rc::Rc<dyn crate::ProjectHost>> {
        self.host.clone()
    }

    fn metrics(&self) -> Arc<dyn motioner_core::TextMetrics> {
        self.options
            .text_metrics
            .clone()
            .unwrap_or_else(|| Arc::new(motioner_core::NoTextMetrics))
    }

    /// Opens `path` (folder, manifest or program) as the current project.
    pub(crate) fn open_composition(&mut self, path: &Path) {
        if let Some(host) = self.host() {
            host.audio_stop();
        }
        self.audio_started = false;
        self.last_scrub_audio = None;
        let Some(host) = self.host() else {
            self.report("opening a project needs the desktop application");
            return;
        };
        let composition = match host.open_project(path) {
            Ok(composition) => composition,
            Err(message) => {
                self.report(&message);
                return;
            }
        };
        let mut session = PreviewSession::new(&composition);
        session.set_text_metrics(self.metrics());
        session.set_image_resolver(host.images(&composition));
        let _ = session.reload();
        let fonts = host.fonts(&composition);
        let mut families: Vec<String> =
            fonts.faces().map(|(family, _)| family.to_owned()).collect();
        families.sort();
        families.dedup();
        if let Err(error) = self.editor.replace_session(session) {
            self.report(&error.message);
            return;
        }
        self.editor.set_font_families(families);
        self.fonts = Some(fonts);
        self.watch = FileWatch::new(&composition);
        self.painter_generation = None;
        self.blur_generation = None;
        self.last_bounds = None;
        if let Some(painter) = self.painter.as_mut() {
            painter.clear_cache();
        }
        self.start = None;
        self.menu_open = false;
        if let Some(host) = self.host() {
            self.recovery = host.recovery(&composition);
            host.begin_session(&composition);
        }
        self.last_heartbeat = Instant::now();
        self.last_snapshot = Instant::now();
        self.refresh_assets();
        self.retitle();
    }

    /// Closes the project and shows the start screen.
    pub(crate) fn close_project(&mut self) {
        if let Some(host) = self.host() {
            host.audio_stop();
            host.end_session(self.editor.session().path());
        }
        self.audio_started = false;
        self.last_scrub_audio = None;
        self.recovery = None;
        let (recents, examples) = self.host().map_or_else(
            || (Vec::new(), Vec::new()),
            |host| (host.recents(), host.examples()),
        );
        self.start = Some(StartScreen::new(recents, examples));
        self.menu_open = false;
        self.editor = Editor::blank(self.metrics());
        self.asset_generation = self.asset_generation.saturating_add(1);
        self.assets.set_entries(Vec::new(), self.asset_generation);
        self.painter_generation = None;
        self.blur_generation = None;
        self.last_bounds = None;
        if let Some(painter) = self.painter.as_mut() {
            painter.clear_cache();
        }
        self.retitle();
    }

    /// Creates a project through the host and opens it.
    pub(crate) fn create_project(&mut self, request: &NewProjectRequest) {
        let Some(host) = self.host() else {
            self.report("creating a project needs the desktop application");
            return;
        };
        match host.create_project(request) {
            Ok(root) => {
                self.open_composition(&root);
                if let Some(start) = self.start.as_mut() {
                    start.mode = Some(StartMode::Home);
                    start.focused = None;
                }
            }
            Err(message) => {
                if let Some(start) = self.start.as_mut() {
                    start.error = Some(message);
                } else {
                    self.report(&message);
                }
            }
        }
    }

    /// Saves a copy of the project under `destination` (picked when absent).
    pub(crate) fn save_as(&mut self, destination: Option<PathBuf>) {
        let Some(host) = self.host() else {
            return;
        };
        let destination = destination.or_else(|| {
            host.pick_folder(
                "Save a copy of the project in",
                self.editor.session().path().parent(),
            )
        });
        let Some(destination) = destination else {
            return;
        };
        let composition = self.editor.session().path().to_path_buf();
        match host.save_as(&composition, &destination) {
            Ok(copy) => {
                self.open_composition(&copy);
            }
            Err(message) => self.report(&message),
        }
    }

    /// Shows a message in the status bar (or the start screen error line).
    fn report(&mut self, message: &str) {
        if let Some(start) = self.start.as_mut() {
            start.error = Some(message.to_owned());
        } else {
            self.toast = Some((message.to_owned(), Instant::now()));
        }
    }

    /// Updates the native window title when it changed.
    pub(crate) fn retitle(&mut self) {
        let title = self.window_title();
        if title != self.last_title {
            self.last_title = title;
            self.title_dirty = true;
        }
    }

    // ---- start screen ------------------------------------------------------

    /// The start screen layout for the current window.
    fn start_layout(&self, ctx: &AppContext<'_>, screen: &StartScreen) -> StartLayout {
        let (width, height) = ctx.window_size;
        StartLayout::compute(
            width as f32,
            height as f32,
            self.scale(ctx),
            screen.recents.len(),
            screen.examples.len(),
        )
    }

    pub(crate) fn update_start(&mut self, ctx: &mut AppContext<'_>) {
        let Some(mut screen) = self.start.take() else {
            return;
        };
        let layout = self.start_layout(ctx, &screen);
        let modifiers = super::modifiers(ctx);
        let typing = screen.focused.is_some();

        if ctx.input.just_pressed(KeyCode::Escape) {
            if screen.mode() == StartMode::New {
                screen.mode = Some(StartMode::Home);
                screen.focused = None;
                screen.error = None;
            } else {
                self.quit = true;
            }
        }
        if ctx.input.just_pressed(KeyCode::Enter) && screen.mode() == StartMode::New {
            self.start = Some(screen);
            self.handle_start_action(StartAction::Create, ctx);
            return;
        }
        if modifiers.ctrl && ctx.input.just_pressed(KeyCode::KeyN) {
            screen.mode = Some(StartMode::New);
            screen.focused = Some(crate::FormField::Name);
        }
        if modifiers.ctrl && ctx.input.just_pressed(KeyCode::KeyO) {
            self.start = Some(screen);
            self.handle_start_action(StartAction::OpenProject, ctx);
            return;
        }
        if typing {
            let shift = modifiers.shift;
            let key = [
                (KeyCode::Backspace, crate::EditKey::Backspace),
                (KeyCode::Delete, crate::EditKey::Delete),
                (KeyCode::ArrowLeft, crate::EditKey::Left),
                (KeyCode::ArrowRight, crate::EditKey::Right),
                (KeyCode::Home, crate::EditKey::Home),
                (KeyCode::End, crate::EditKey::End),
            ]
            .into_iter()
            .find(|(code, _)| ctx.input.just_pressed(*code))
            .map(|(_, key)| key);
            if let Some(key) = key {
                screen.edit_key(key, shift);
            }
            for ch in ctx.input.typed_chars() {
                screen.type_char(*ch);
            }
        }

        let (mx, my) = ctx.input.mouse_pos_f32();
        if let Some(action) = screen.action_at(&layout, mx, my) {
            self.start_hover = Some(action);
        } else {
            self.start_hover = None;
        }
        if ctx.input.button_just_pressed(MouseButton::Left) {
            if let Some(action) = self.start_hover {
                self.start = Some(screen);
                self.handle_start_action(action, ctx);
                return;
            }
        }
        self.start = Some(screen);
        self.retitle();
    }

    /// Runs one start-screen action.
    pub(crate) fn handle_start_action(&mut self, action: StartAction, ctx: &mut AppContext<'_>) {
        let Some(mut screen) = self.start.take() else {
            return;
        };
        match action {
            StartAction::NewProject => {
                screen.mode = Some(StartMode::New);
                screen.error = None;
                screen.focused = Some(crate::FormField::Name);
                self.start = Some(screen);
            }
            StartAction::OpenProject => {
                self.start = Some(screen);
                if let Some(host) = self.host() {
                    if let Some(folder) = host.pick_folder("Open a Motioner project", None) {
                        self.open_composition(&folder);
                    }
                }
            }
            StartAction::Browse => {
                if let Some(host) = self.host() {
                    if let Some(folder) =
                        host.pick_folder("Choose where to create the project", None)
                    {
                        screen
                            .form
                            .location
                            .set_text(crate::project_path_text(&folder));
                    }
                }
                self.start = Some(screen);
            }
            StartAction::Create => match screen.form.request() {
                Ok(request) => {
                    self.start = Some(screen);
                    self.create_project(&request);
                }
                Err(message) => {
                    screen.error = Some(message);
                    self.start = Some(screen);
                }
            },
            StartAction::Cancel => {
                screen.mode = Some(StartMode::Home);
                screen.focused = None;
                screen.error = None;
                self.start = Some(screen);
            }
            StartAction::OpenRecent(index) => {
                let Some(entry) = screen.recents.get(index).cloned() else {
                    self.start = Some(screen);
                    return;
                };
                self.start = Some(screen);
                self.open_composition(&entry.path);
            }
            StartAction::ForgetRecent(index) => {
                if let Some(entry) = screen.recents.get(index).cloned() {
                    if let Some(host) = self.host() {
                        host.forget_recent(&entry.path);
                    }
                    screen.recents.remove(index);
                }
                self.start = Some(screen);
            }
            StartAction::OpenExample(index) => {
                let Some(entry) = screen.examples.get(index).cloned() else {
                    self.start = Some(screen);
                    return;
                };
                self.start = Some(screen);
                self.open_composition(&entry.path);
            }
            StartAction::Quit => ctx.request_exit(),
        }
    }

    /// Draws the start screen.
    pub(crate) fn draw_start(&mut self, dc: &mut ferrous_engine::DrawContext<'_, '_>) {
        let window = (dc.ctx.window_size.0 as f32, dc.ctx.window_size.1 as f32);
        let scale = self
            .layout
            .map_or_else(|| self.scale(dc.ctx), |layout| layout.metrics.scale);
        let Some(screen) = self.start.as_ref() else {
            return;
        };
        let layout = StartLayout::compute(
            window.0,
            window.1,
            scale,
            screen.recents.len(),
            screen.examples.len(),
        );
        let mut p = crate::paint::Painter::new(dc.gui, dc.font, &self.theme, layout.metrics);
        p.layer();
        crate::start::draw(
            &mut p,
            screen,
            &layout,
            self.start_hover,
            self.start_pressed,
        );
        let _ = window;
    }

    // ---- assets ------------------------------------------------------------

    /// Rebuilds the asset panel from the project registry.
    pub(crate) fn refresh_assets(&mut self) {
        self.asset_generation = self.asset_generation.saturating_add(1);
        let entries = self
            .host()
            .map_or_else(Vec::new, |host| host.assets(self.editor.session().path()));
        self.assets.set_entries(entries, self.asset_generation);
    }

    /// Imports one file, or every file the picker returns when `path` is
    /// empty.
    pub(crate) fn import_assets(&mut self, path: &str) {
        let Some(host) = self.host() else {
            self.report("importing needs the desktop application");
            return;
        };
        let composition = self.editor.session().path().to_path_buf();
        let sources = if path.is_empty() {
            host.pick_files("Import media into the project")
        } else {
            vec![PathBuf::from(path)]
        };
        if sources.is_empty() {
            return;
        }
        let mut imported = 0_usize;
        let mut failure = None;
        let mut last_key = None;
        for source in sources {
            match host.import_asset(&composition, &source) {
                Ok(summary) => {
                    imported += 1;
                    last_key = Some(summary.key);
                }
                Err(message) => failure = Some(message),
            }
        }
        self.last_imported = last_key;
        if let Some(message) = failure {
            self.report(&message);
        } else if imported > 0 {
            self.toast = Some((format!("imported {imported} asset(s)"), Instant::now()));
        }
        let fonts = host.fonts(&composition);
        let mut families: Vec<String> =
            fonts.faces().map(|(family, _)| family.to_owned()).collect();
        families.sort();
        families.dedup();
        self.editor.set_font_families(families);
        self.fonts = Some(fonts);
        self.refresh_assets();
    }

    /// Removes one asset and its blob.
    pub(crate) fn remove_asset(&mut self, key: &str) {
        let Some(host) = self.host() else {
            return;
        };
        let composition = self.editor.session().path().to_path_buf();
        match host.remove_asset(&composition, key) {
            Ok(()) => {
                self.toast = Some((format!("removed {key}"), Instant::now()));
                self.refresh_assets();
            }
            Err(message) => self.report(&message),
        }
    }

    /// Renames one asset key.
    pub(crate) fn rename_asset(&mut self, key: &str, name: &str) {
        let Some(host) = self.host() else {
            return;
        };
        let composition = self.editor.session().path().to_path_buf();
        match host.rename_asset(&composition, key, name) {
            Ok(()) => self.refresh_assets(),
            Err(message) => self.report(&message),
        }
    }

    /// Appends the layer for an asset and reloads the program.
    pub(crate) fn place_asset(&mut self, key: &str, frame: Option<i64>, pos: Option<(f64, f64)>) {
        let Some(host) = self.host() else {
            return;
        };
        let composition = self.editor.session().path().to_path_buf();
        match host.place_asset(&composition, key, frame, pos) {
            Ok(()) => {
                self.toast = Some((format!("placed {key}"), Instant::now()));
                self.run(&Command::Reload);
                self.refresh_assets();
            }
            Err(message) => self.report(&message),
        }
    }

    /// Removes every asset no composition references.
    pub(crate) fn remove_unused_assets(&mut self) {
        let Some(host) = self.host() else {
            return;
        };
        let composition = self.editor.session().path().to_path_buf();
        match host.remove_unused_assets(&composition) {
            Ok(count) => {
                self.toast = Some((format!("removed {count} unused asset(s)"), Instant::now()));
                self.refresh_assets();
            }
            Err(message) => self.report(&message),
        }
    }

    /// Restores the backup offered after a crash and reloads.
    pub(crate) fn restore_backup(&mut self) {
        let Some(host) = self.host() else {
            return;
        };
        let composition = self.editor.session().path().to_path_buf();
        match host.restore(&composition) {
            Ok(()) => {
                self.recovery = None;
                self.toast = Some(("restored the backup".to_owned(), Instant::now()));
                self.run(&Command::Reload);
            }
            Err(message) => self.report(&message),
        }
    }

    /// Refreshes the session lock heartbeat and takes a periodic snapshot.
    pub(crate) fn tick_session(&mut self) {
        let Some(host) = self.host() else {
            return;
        };
        let composition = self.editor.session().path().to_path_buf();
        if self.editor.session().document().is_none() {
            return;
        }
        if self.last_heartbeat.elapsed() >= std::time::Duration::from_secs(30) {
            host.heartbeat(&composition);
            self.last_heartbeat = Instant::now();
        }
        if self.last_snapshot.elapsed() >= std::time::Duration::from_mins(2) {
            if let Err(message) = host.snapshot(&composition) {
                self.report(&message);
            }
            self.last_snapshot = Instant::now();
        }
    }

    /// Runs a File-menu action.
    pub(crate) fn dispatch_menu(&mut self, action: MenuAction) {
        self.menu_open = false;
        match action {
            MenuAction::New => {
                let (recents, examples) = self.host().map_or_else(
                    || (Vec::new(), Vec::new()),
                    |host| (host.recents(), host.examples()),
                );
                let mut screen = StartScreen::new(recents, examples);
                screen.mode = Some(StartMode::New);
                screen.focused = Some(crate::FormField::Name);
                self.start = Some(screen);
            }
            MenuAction::Open => {
                if let Some(host) = self.host() {
                    if let Some(folder) = host.pick_folder("Open a Motioner project", None) {
                        self.open_composition(&folder);
                    }
                }
            }
            MenuAction::Save => {
                self.run(&Command::Save);
                if let Some(host) = self.host() {
                    let composition = self.editor.session().path().to_path_buf();
                    if let Err(message) = host.save_project(&composition) {
                        self.report(&message);
                    }
                }
            }
            MenuAction::SaveAs => self.save_as(None),
            MenuAction::Import => {
                self.run(&Command::ImportAsset {
                    path: String::new(),
                });
            }
            MenuAction::Reveal => {
                if let Some(host) = self.host() {
                    host.reveal(self.editor.session().path());
                }
            }
            MenuAction::Close => {
                self.run(&Command::CloseProject);
            }
        }
    }

    /// Handles File-menu clicks before the editor's own hit testing.
    ///
    /// Returns `true` when the click was consumed by the menu.
    pub(crate) fn menu_click(&mut self, ctx: &AppContext<'_>, layout: &crate::Layout) -> bool {
        if !ctx.input.button_just_pressed(MouseButton::Left) {
            return false;
        }
        let (mx, my) = ctx.input.mouse_pos_f32();
        if layout.file_button.contains(mx, my) {
            self.menu_open = !self.menu_open;
            return true;
        }
        if !self.menu_open {
            return false;
        }
        if let Some(action) = layout.menu_hit(mx, my) {
            self.dispatch_menu(action);
            return true;
        }
        self.menu_open = false;
        false
    }
}
