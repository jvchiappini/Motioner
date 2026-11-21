//! The start screen: recent projects, examples and the New project dialog.
//!
//! This module is pure state plus painting: it never reads the file system.
//! The application fills [`StartScreen::recents`] and `examples`, and handles
//! [`StartAction`] values returned by [`StartScreen::action_at`] with the
//! [`crate::ProjectHost`] (folder picker, project creation, open).

use std::path::PathBuf;

use crate::layout::Rect;
use crate::paint::Painter;
use crate::text::{Align, TextStyle};
use crate::text_edit::{EditKey, TextEdit};
use crate::theme::{size, type_scale, Metrics, Theme};
use crate::widgets::Interaction;

/// One recent project or example shown on the start screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartEntry {
    /// Project folder or example program.
    pub path: PathBuf,
    /// Display name.
    pub name: String,
    /// Secondary line: last opened, asset count or a one-line summary.
    pub detail: String,
    /// Whether the folder is gone (shown greyed with "remove").
    pub missing: bool,
    /// Hover tooltip, such as the full local opened date.
    pub tooltip: Option<String>,
}

impl StartEntry {
    /// A recent project entry.
    #[must_use]
    pub fn recent(path: PathBuf, name: String, detail: String, missing: bool) -> Self {
        Self {
            path,
            name,
            detail,
            missing,
            tooltip: None,
        }
    }

    /// Attaches a hover tooltip to the entry.
    #[must_use]
    pub fn with_tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }
}

/// Whether the start screen shows the home list or the New project dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartMode {
    /// Recent projects and examples.
    Home,
    /// The New project form.
    New,
}

/// One editable field of the New project dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FormField {
    /// Project name.
    Name,
    /// Parent folder.
    Location,
    /// Composition width.
    Width,
    /// Composition height.
    Height,
    /// Frame rate.
    Fps,
    /// Length in frames.
    Duration,
    /// Background colour.
    Background,
}

impl FormField {
    /// Every field in draw order.
    pub const ALL: [Self; 7] = [
        Self::Name,
        Self::Location,
        Self::Width,
        Self::Height,
        Self::Fps,
        Self::Duration,
        Self::Background,
    ];

    /// The field label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Location => "Location",
            Self::Width => "Width",
            Self::Height => "Height",
            Self::Fps => "FPS",
            Self::Duration => "Frames",
            Self::Background => "Background",
        }
    }
}

/// What the New project dialog asks the application to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewProjectRequest {
    /// Project name.
    pub name: String,
    /// Parent folder where `<name>.motioner/` is created.
    pub location: PathBuf,
    /// Composition width.
    pub width: u32,
    /// Composition height.
    pub height: u32,
    /// Frame rate.
    pub fps: u32,
    /// Length in frames.
    pub duration: i64,
    /// Background colour as `#rrggbb`.
    pub background: String,
}

/// What a click or key on the start screen asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartAction {
    /// Show the New project dialog.
    NewProject,
    /// Ask the application for a folder, then open it.
    OpenProject,
    /// Ask the application for a parent folder for the form.
    Browse,
    /// Create the project described by the form.
    Create,
    /// Return to the home list.
    Cancel,
    /// Open a recent project.
    OpenRecent(usize),
    /// Forget a recent project.
    ForgetRecent(usize),
    /// Open an example program.
    OpenExample(usize),
    /// Close the window.
    Quit,
}

/// The editable New project form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewProjectForm {
    /// Project name.
    pub name: TextEdit,
    /// Parent folder.
    pub location: TextEdit,
    /// Width in pixels.
    pub width: TextEdit,
    /// Height in pixels.
    pub height: TextEdit,
    /// Frame rate.
    pub fps: TextEdit,
    /// Length in frames.
    pub duration: TextEdit,
    /// Background colour.
    pub background: TextEdit,
}

impl Default for NewProjectForm {
    fn default() -> Self {
        Self {
            name: TextEdit::new("Untitled"),
            location: TextEdit::new(String::new()),
            width: TextEdit::new("1920"),
            height: TextEdit::new("1080"),
            fps: TextEdit::new("60"),
            duration: TextEdit::new("300"),
            background: TextEdit::new("#000000"),
        }
    }
}

impl NewProjectForm {
    /// The editable text of `field`.
    #[must_use]
    pub fn field(&self, field: FormField) -> &TextEdit {
        match field {
            FormField::Name => &self.name,
            FormField::Location => &self.location,
            FormField::Width => &self.width,
            FormField::Height => &self.height,
            FormField::Fps => &self.fps,
            FormField::Duration => &self.duration,
            FormField::Background => &self.background,
        }
    }

    fn field_mut(&mut self, field: FormField) -> &mut TextEdit {
        match field {
            FormField::Name => &mut self.name,
            FormField::Location => &mut self.location,
            FormField::Width => &mut self.width,
            FormField::Height => &mut self.height,
            FormField::Fps => &mut self.fps,
            FormField::Duration => &mut self.duration,
            FormField::Background => &mut self.background,
        }
    }

    /// Parses the form into a request, naming the first invalid field.
    ///
    /// # Errors
    ///
    /// Returns a human-readable message for an empty name, an empty location,
    /// a non-positive size, rate or length, or an invalid colour.
    pub fn request(&self) -> Result<NewProjectRequest, String> {
        let name = self.name.text().trim().to_owned();
        if name.is_empty() {
            return Err("the project needs a name".to_owned());
        }
        let location = self.location.text().trim();
        if location.is_empty() {
            return Err("choose a location for the project".to_owned());
        }
        let width = positive_u32(self.width.text(), "width")?;
        let height = positive_u32(self.height.text(), "height")?;
        let fps = positive_u32(self.fps.text(), "frame rate")?;
        let duration = self
            .duration
            .text()
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| "frames must be a positive whole number".to_owned())?;
        let background = self.background.text().trim().to_owned();
        if !valid_hex_color(&background) {
            return Err("background must be #rrggbb".to_owned());
        }
        Ok(NewProjectRequest {
            name,
            location: PathBuf::from(location),
            width,
            height,
            fps,
            duration,
            background,
        })
    }
}

fn positive_u32(text: &str, what: &str) -> Result<u32, String> {
    text.trim()
        .parse::<u32>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{what} must be a positive whole number"))
}

/// Whether `text` is `#rrggbb`.
#[must_use]
pub fn valid_hex_color(text: &str) -> bool {
    text.len() == 7 && text.starts_with('#') && text[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// The start screen state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StartScreen {
    /// Recent projects, most recent first.
    pub recents: Vec<StartEntry>,
    /// Shipped examples.
    pub examples: Vec<StartEntry>,
    /// Home list or New project dialog.
    pub mode: Option<StartMode>,
    /// The New project form (kept while the dialog is closed).
    pub form: NewProjectForm,
    /// Focused form field.
    pub focused: Option<FormField>,
    /// Last creation error.
    pub error: Option<String>,
}

impl StartScreen {
    /// A start screen with `recents` and `examples`.
    #[must_use]
    pub fn new(recents: Vec<StartEntry>, examples: Vec<StartEntry>) -> Self {
        Self {
            recents,
            examples,
            mode: Some(StartMode::Home),
            form: NewProjectForm::default(),
            focused: None,
            error: None,
        }
    }

    /// The current mode (`Home` when unset).
    #[must_use]
    pub fn mode(&self) -> StartMode {
        self.mode.unwrap_or(StartMode::Home)
    }

    /// Makes `field` the focused one and selects its text.
    pub fn focus(&mut self, field: FormField) {
        self.focused = Some(field);
        self.form.field_mut(field).select_all_in_place();
    }

    /// Applies one editing key to the focused field.
    ///
    /// Returns `true` when the key changed the field.
    pub fn edit_key(&mut self, key: EditKey, shift: bool) -> bool {
        let Some(field) = self.focused else {
            return false;
        };
        self.form.field_mut(field).key(key, shift);
        true
    }

    /// Inserts typed text into the focused field.
    pub fn type_char(&mut self, ch: char) {
        let Some(field) = self.focused else {
            return;
        };
        if ch.is_control() {
            return;
        }
        self.form.field_mut(field).insert_char(ch);
        self.error = None;
    }

    /// The action under `(x, y)` in `layout`.
    #[must_use]
    pub fn action_at(&self, layout: &StartLayout, x: f32, y: f32) -> Option<StartAction> {
        match self.mode() {
            StartMode::Home => {
                if layout.new_project.contains(x, y) {
                    return Some(StartAction::NewProject);
                }
                if layout.open_project.contains(x, y) {
                    return Some(StartAction::OpenProject);
                }
                for (index, row) in layout.recents.iter().enumerate() {
                    if row.forget.contains(x, y) {
                        return Some(StartAction::ForgetRecent(index));
                    }
                    if row.card.contains(x, y) {
                        return Some(StartAction::OpenRecent(index));
                    }
                }
                for (index, row) in layout.examples.iter().enumerate() {
                    if row.contains(x, y) {
                        return Some(StartAction::OpenExample(index));
                    }
                }
                None
            }
            StartMode::New => {
                if layout.create.contains(x, y) {
                    return Some(StartAction::Create);
                }
                if layout.cancel.contains(x, y) {
                    return Some(StartAction::Cancel);
                }
                if layout.browse.contains(x, y) {
                    return Some(StartAction::Browse);
                }
                for (_field, rect) in &layout.fields {
                    if rect.contains(x, y) {
                        return Some(StartAction::Cancel);
                    }
                }
                None
            }
        }
    }
}

/// One recent-project card.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecentCard {
    /// The whole card (opens the project).
    pub card: Rect,
    /// The remove action for a missing project.
    pub forget: Rect,
}

/// The laid-out start screen.
#[derive(Debug, Clone, PartialEq)]
pub struct StartLayout {
    /// Whole window.
    pub window: Rect,
    /// DPI metrics.
    pub metrics: Metrics,
    /// Primary "New project" button.
    pub new_project: Rect,
    /// Secondary "Open project" button.
    pub open_project: Rect,
    /// Recent cards.
    pub recents: Vec<RecentCard>,
    /// Example rows.
    pub examples: Vec<Rect>,
    /// The New project dialog card.
    pub dialog: Rect,
    /// Field rectangles in [`FormField::ALL`] order.
    pub fields: Vec<(FormField, Rect)>,
    /// Browse button next to the location field.
    pub browse: Rect,
    /// Create button.
    pub create: Rect,
    /// Cancel button.
    pub cancel: Rect,
}

impl StartLayout {
    /// Lays the screen out for a window and DPI scale.
    #[must_use]
    pub fn compute(width: f32, height: f32, scale: f32, recents: usize, examples: usize) -> Self {
        let m = Metrics::new(scale);
        let window = Rect::new(0.0, 0.0, width.max(0.0), height.max(0.0));
        let pad = m.px(size::PAD * 3.0);
        let content_w = (window.w - pad * 2.0).min(m.px(1040.0));
        let left = ((window.w - content_w) * 0.5).round();
        let mut y = pad + m.px(96.0);

        let button_h = m.px(38.0);
        let new_project = Rect::new(left, y, m.px(168.0), button_h);
        let open_project = Rect::new(new_project.right() + m.px(12.0), y, m.px(168.0), button_h);
        y += button_h + m.px(52.0);

        // A two-column card grid, four rows at most.
        let columns = if content_w >= m.px(720.0) { 2 } else { 1 };
        let gap = m.px(16.0);
        let card_w = (content_w - gap * (columns as f32 - 1.0)) / columns as f32;
        let card_h = m.px(52.0);
        let rows = recents.div_ceil(columns).min(4);
        let mut cards = Vec::with_capacity(recents);
        for index in 0..rows * columns {
            let column = index % columns;
            let row = index / columns;
            let card = Rect::new(
                left + column as f32 * (card_w + gap),
                y + row as f32 * (card_h + gap),
                card_w,
                card_h,
            );
            let forget = Rect::new(
                card.right() - m.px(52.0),
                card.y + m.px(8.0),
                m.px(44.0),
                m.px(18.0),
            );
            cards.push(RecentCard { card, forget });
        }
        y += rows as f32 * (card_h + gap) + m.px(28.0);

        let example_h = m.px(26.0);
        let mut example_rows = Vec::with_capacity(examples);
        for index in 0..examples.min(6) {
            example_rows.push(Rect::new(
                left,
                y + index as f32 * example_h,
                content_w,
                example_h,
            ));
        }

        // The dialog is a centred card.
        let dialog_w = m.px(520.0).min(window.w - pad * 2.0);
        let dialog_h = m.px(430.0).min(window.h - pad * 2.0);
        let dialog = Rect::new(
            ((window.w - dialog_w) * 0.5).round(),
            ((window.h - dialog_h) * 0.5).round(),
            dialog_w,
            dialog_h,
        );
        let field_h = m.px(26.0);
        let field_pad = m.px(size::PAD * 2.0);
        let field_w = dialog.w - field_pad * 2.0;
        let mut fields = Vec::with_capacity(FormField::ALL.len());
        let mut field_y = dialog.y + m.px(58.0);
        for field in FormField::ALL {
            fields.push((
                field,
                Rect::new(dialog.x + field_pad, field_y, field_w, field_h),
            ));
            field_y += field_h + m.px(10.0);
        }
        let browse = Rect::new(
            dialog.right() - field_pad - m.px(62.0),
            dialog.y + m.px(58.0) + field_h + m.px(10.0) + m.px(3.0),
            m.px(62.0),
            m.px(20.0),
        );
        let create = Rect::new(
            dialog.right() - field_pad - m.px(88.0),
            dialog.bottom() - m.px(46.0),
            m.px(88.0),
            m.px(30.0),
        );
        let cancel = Rect::new(create.x - m.px(96.0), create.y, m.px(88.0), m.px(30.0));

        Self {
            window,
            metrics: m,
            new_project,
            open_project,
            recents: cards,
            examples: example_rows,
            dialog,
            fields,
            browse,
            create,
            cancel,
        }
    }
}

/// Draws the start screen.
pub fn draw(
    p: &mut Painter<'_>,
    screen: &StartScreen,
    layout: &StartLayout,
    hover: Option<StartAction>,
    pressed: Option<StartAction>,
) {
    let t = *p.theme;
    p.fill(layout.window, t.pasteboard);
    // A faint accent wash behind the wordmark.
    let glow = Rect::new(
        layout.window.center_x() - p.m.px(260.0),
        p.m.px(20.0),
        p.m.px(520.0),
        p.m.px(96.0),
    );
    let mut accent = t.accent;
    accent[3] = 0.05;
    p.fill_rounded(glow, accent, p.m.px(24.0));

    match screen.mode() {
        StartMode::Home => draw_home(p, screen, layout, hover, pressed),
        StartMode::New => {
            draw_home(p, screen, layout, None, None);
            draw_dialog(p, screen, layout, hover, pressed);
        }
    }
    if let Some(StartAction::OpenRecent(index)) = hover {
        if let (Some(entry), Some(card)) = (screen.recents.get(index), layout.recents.get(index)) {
            if let Some(tooltip) = &entry.tooltip {
                p.layer();
                crate::widgets::tooltip(p, card.card, layout.window, tooltip, None);
            }
        }
    }
}

fn interaction(
    hover: Option<StartAction>,
    pressed: Option<StartAction>,
    is: StartAction,
) -> Interaction {
    Interaction {
        hovered: hover.as_ref() == Some(&is),
        pressed: pressed.as_ref() == Some(&is),
        disabled: false,
        active: false,
    }
}

#[allow(clippy::too_many_lines)] // one immediate-mode home screen
fn draw_home(
    p: &mut Painter<'_>,
    screen: &StartScreen,
    layout: &StartLayout,
    hover: Option<StartAction>,
    pressed: Option<StartAction>,
) {
    let t = *p.theme;
    let title = p.style(30.0, t.text).medium();
    p.text_in(
        "Motioner",
        Rect::new(layout.window.x, p.m.px(28.0), layout.window.w, p.m.px(44.0)),
        Align::Center,
        &title,
    );
    let subtitle = p.style(type_scale::EMPHASIS, t.text_muted);
    p.text_in(
        "Program-first motion graphics and video",
        Rect::new(layout.window.x, p.m.px(74.0), layout.window.w, p.m.px(22.0)),
        Align::Center,
        &subtitle,
    );

    let primary = interaction(hover, pressed, StartAction::NewProject);
    button(
        p,
        layout.new_project,
        "New project",
        ButtonKind::Primary,
        primary,
    );
    let secondary = interaction(hover, pressed, StartAction::OpenProject);
    button(
        p,
        layout.open_project,
        "Open project",
        ButtonKind::Secondary,
        secondary,
    );

    let section = p.style(type_scale::CAPTION, t.text_muted).tracked(0.08);
    let header_y = layout.new_project.bottom() + p.m.px(30.0);
    p.text_in(
        "RECENT PROJECTS",
        Rect::new(
            layout.new_project.x,
            header_y,
            layout.new_project.w * 4.0,
            p.m.px(16.0),
        ),
        Align::Left,
        &section,
    );
    if screen.recents.is_empty() {
        let hint = p.style(type_scale::BODY, t.text_muted);
        p.text_in(
            "No recent projects yet. Create one to get started.",
            Rect::new(
                layout.new_project.x,
                header_y + p.m.px(22.0),
                layout.window.w,
                p.m.px(20.0),
            ),
            Align::Left,
            &hint,
        );
    }
    for (index, entry) in screen.recents.iter().enumerate() {
        let Some(card) = layout.recents.get(index) else {
            break;
        };
        let action = StartAction::OpenRecent(index);
        let state = interaction(hover, pressed, action);
        let base = if state.pressed {
            t.raised
        } else if state.hovered {
            t.row_hover
        } else {
            t.panel
        };
        p.fill_rounded(card.card, base, p.m.px(6.0));
        p.stroke_rounded(card.card, t.hairline, p.m.px(6.0), p.m.px(1.0));
        // A small accent tile stands in for the stored thumbnail.
        let swatch = Rect::new(
            card.card.x + p.m.px(10.0),
            card.card.y + p.m.px(10.0),
            p.m.px(32.0),
            p.m.px(32.0),
        );
        let mut tile_color = t.accent;
        tile_color[3] = if entry.missing { 0.18 } else { 0.85 };
        p.fill_rounded(swatch, tile_color, p.m.px(4.0));
        let name_style = p
            .style(
                type_scale::EMPHASIS,
                if entry.missing { t.text_muted } else { t.text },
            )
            .medium();
        p.text_in(
            &entry.name,
            Rect::new(
                swatch.right() + p.m.px(10.0),
                card.card.y + p.m.px(9.0),
                card.card.w - p.m.px(84.0),
                p.m.px(18.0),
            ),
            Align::Left,
            &name_style,
        );
        let detail = p.style(type_scale::CAPTION, t.text_muted);
        p.text_in(
            &entry.detail,
            Rect::new(
                swatch.right() + p.m.px(10.0),
                card.card.y + p.m.px(27.0),
                card.card.w - p.m.px(84.0),
                p.m.px(16.0),
            ),
            Align::Left,
            &detail,
        );
        if entry.missing {
            let remove = interaction(hover, pressed, StartAction::ForgetRecent(index));
            button(p, card.forget, "remove", ButtonKind::Ghost, remove);
        }
    }

    let examples_header = layout
        .examples
        .first()
        .map_or(layout.window.bottom() - p.m.px(40.0), |first| {
            first.y - p.m.px(30.0)
        });
    p.text_in(
        "EXAMPLES",
        Rect::new(
            layout.new_project.x,
            examples_header,
            layout.new_project.w * 3.0,
            p.m.px(16.0),
        ),
        Align::Left,
        &section,
    );
    for (index, entry) in screen.examples.iter().enumerate() {
        let Some(rect) = layout.examples.get(index) else {
            break;
        };
        let state = interaction(hover, pressed, StartAction::OpenExample(index));
        if state.hovered {
            p.fill_rounded(*rect, t.row_hover, p.m.px(4.0));
        }
        let name = p.style(type_scale::BODY, t.text_body).medium();
        p.text_in(
            &entry.name,
            Rect::new(rect.x + p.m.px(8.0), rect.y, rect.w * 0.45, rect.h),
            Align::Left,
            &name,
        );
        let detail = p.style(type_scale::CAPTION, t.text_muted);
        p.text_in(
            &entry.detail,
            Rect::new(
                rect.x + rect.w * 0.45,
                rect.y,
                rect.w * 0.55 - p.m.px(8.0),
                rect.h,
            ),
            Align::Right,
            &detail,
        );
    }

    let footer = p.style(type_scale::CAPTION, t.text_muted);
    p.text_in(
        "Ctrl+N new  ·  Ctrl+O open  ·  Esc close",
        Rect::new(
            layout.window.x,
            layout.window.bottom() - p.m.px(28.0),
            layout.window.w,
            p.m.px(18.0),
        ),
        Align::Center,
        &footer,
    );
}

fn draw_dialog(
    p: &mut Painter<'_>,
    screen: &StartScreen,
    layout: &StartLayout,
    hover: Option<StartAction>,
    pressed: Option<StartAction>,
) {
    let t = *p.theme;
    // Dim the list behind the modal.
    let mut shade = t.pasteboard;
    shade[3] = 0.72;
    p.fill(layout.window, shade);
    p.shadow(
        layout.dialog,
        p.m.px(10.0),
        p.m.px(24.0),
        p.m.px(8.0),
        [0.0, 0.0, 0.0, 0.55],
    );
    p.fill_rounded(layout.dialog, t.panel, p.m.px(10.0));
    p.stroke_rounded(layout.dialog, t.hairline, p.m.px(10.0), p.m.px(1.0));

    let heading = p.style(type_scale::TITLE, t.text).medium();
    p.text_in(
        "New project",
        Rect::new(
            layout.dialog.x + p.m.px(20.0),
            layout.dialog.y + p.m.px(14.0),
            layout.dialog.w - p.m.px(40.0),
            p.m.px(30.0),
        ),
        Align::Left,
        &heading,
    );

    for (field, rect) in &layout.fields {
        let label = p.style(type_scale::CAPTION, t.text_muted);
        p.text_in(
            field.label(),
            Rect::new(rect.x, rect.y - p.m.px(14.0), rect.w * 0.4, p.m.px(14.0)),
            Align::Left,
            &label,
        );
        let value = screen.form.field(*field);
        let focused = screen.focused == Some(*field);
        text_field(p, *rect, value, focused, &t);
    }

    let browse = interaction(hover, pressed, StartAction::Browse);
    button(p, layout.browse, "Browse", ButtonKind::Secondary, browse);
    let cancel = interaction(hover, pressed, StartAction::Cancel);
    button(p, layout.cancel, "Cancel", ButtonKind::Secondary, cancel);
    let create = interaction(hover, pressed, StartAction::Create);
    button(p, layout.create, "Create", ButtonKind::Primary, create);

    if let Some(error) = &screen.error {
        let style = p.style(type_scale::CAPTION, t.danger);
        p.text_in(
            error,
            Rect::new(
                layout.dialog.x + p.m.px(20.0),
                layout.dialog.bottom() - p.m.px(74.0),
                layout.dialog.w - p.m.px(40.0),
                p.m.px(18.0),
            ),
            Align::Left,
            &style,
        );
    }
}

/// Which surface a start-screen button uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ButtonKind {
    /// Filled with the accent.
    Primary,
    /// Raised surface with a hairline.
    Secondary,
    /// Transparent until hovered.
    Ghost,
}

fn button(p: &mut Painter<'_>, rect: Rect, label: &str, kind: ButtonKind, state: Interaction) {
    let t = *p.theme;
    let (fill, ink) = match kind {
        ButtonKind::Primary => {
            if state.disabled {
                (t.raised, t.text_disabled)
            } else if state.hovered {
                (t.accent_hover, t.pasteboard)
            } else {
                (t.accent, t.pasteboard)
            }
        }
        ButtonKind::Secondary => {
            if state.hovered {
                (t.row_hover, t.text)
            } else {
                (t.raised, t.text_body)
            }
        }
        ButtonKind::Ghost => (t.panel, t.text_muted),
    };
    p.fill_rounded(rect, fill, p.m.px(size::RADIUS_MD));
    if kind != ButtonKind::Primary {
        p.stroke_rounded(rect, t.hairline, p.m.px(size::RADIUS_MD), p.m.px(1.0));
    }
    let style = p.style(type_scale::BODY, ink).medium();
    p.text_in(label, rect, Align::Center, &style);
}

fn text_field(p: &mut Painter<'_>, rect: Rect, value: &TextEdit, focused: bool, theme: &Theme) {
    p.fill_rounded(rect, theme.raised, p.m.px(size::RADIUS_SM));
    let border = if focused {
        theme.accent
    } else {
        theme.hairline
    };
    p.stroke_rounded(rect, border, p.m.px(size::RADIUS_SM), p.m.px(1.0));
    let style = TextStyle::new(type_scale::BODY, theme.text);
    let inner = Rect::new(
        rect.x + p.m.px(8.0),
        rect.y,
        (rect.w - p.m.px(16.0)).max(0.0),
        rect.h,
    );
    p.text_in(value.text(), inner, Align::Left, &style);
    if focused {
        let before: String = value.text().chars().take(value.caret()).collect();
        let caret_x = inner.x + p.measure(&before, &style);
        p.fill(
            Rect::new(
                caret_x,
                rect.y + p.m.px(5.0),
                p.m.px(1.0),
                rect.h - p.m.px(10.0),
            ),
            theme.accent,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_keeps_every_target_inside_the_window() {
        let layout = StartLayout::compute(1280.0, 800.0, 1.0, 12, 8);
        for card in &layout.recents {
            assert!(layout.window.contains(card.card.x, card.card.y));
            assert!(card.card.right() <= layout.window.right() + 0.5);
        }
        assert!(layout.window.contains(layout.dialog.x, layout.dialog.y));
        assert!(layout.dialog.right() <= layout.window.right());
        assert!(layout.dialog.bottom() <= layout.window.bottom());
    }

    #[test]
    fn a_form_parses_into_a_request_and_names_the_bad_field() {
        let mut form = NewProjectForm {
            location: TextEdit::new("C:/work"),
            ..NewProjectForm::default()
        };
        let request = form.request().unwrap();
        assert_eq!(request.name, "Untitled");
        assert_eq!((request.width, request.height), (1920, 1080));
        form.fps = TextEdit::new("0");
        assert!(form.request().unwrap_err().contains("frame rate"));
        form.fps = TextEdit::new("60");
        form.background = TextEdit::new("black");
        assert!(form.request().unwrap_err().contains("background"));
    }

    #[test]
    fn actions_map_to_their_rectangles() {
        let mut screen = StartScreen::new(
            vec![StartEntry::recent(
                PathBuf::from("C:/work/demo.motioner"),
                "demo".to_owned(),
                "today".to_owned(),
                true,
            )],
            vec![],
        );
        let layout = StartLayout::compute(1400.0, 900.0, 1.0, 1, 0);
        assert_eq!(
            screen.action_at(
                &layout,
                layout.new_project.center_x(),
                layout.new_project.center_y()
            ),
            Some(StartAction::NewProject)
        );
        let card = layout.recents[0];
        assert_eq!(
            screen.action_at(&layout, card.forget.center_x(), card.forget.center_y()),
            Some(StartAction::ForgetRecent(0))
        );
        screen.mode = Some(StartMode::New);
        assert_eq!(
            screen.action_at(&layout, layout.create.center_x(), layout.create.center_y()),
            Some(StartAction::Create)
        );
    }

    #[test]
    fn typing_goes_to_the_focused_field() {
        let mut screen = StartScreen::new(Vec::new(), Vec::new());
        screen.mode = Some(StartMode::New);
        screen.focus(FormField::Name);
        assert!(screen.edit_key(EditKey::Backspace, false));
        screen.type_char('D');
        assert_eq!(screen.form.name.text(), "D");
    }
}
