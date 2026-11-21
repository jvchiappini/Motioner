//! Status bar: errors first, then override warnings, then frame-time
//! readouts.

use crate::icons::{self, Icon};
use crate::layout::{Hit, Rect};
use crate::paint::{Painter, PanelKind, PanelRevision};
use crate::panels::View;
use crate::text::Align;
use crate::theme::{size, type_scale};

/// Everything the status bar reads, except the theme and DPI (in the key).
fn revision(v: &View<'_>) -> u64 {
    let mut rev = PanelRevision::new()
        .bool(v.document.is_some())
        .bool(v.motion_blur_preview)
        .usize(v.video_bytes)
        .f32(v.stats.ui_fps)
        .f32(v.stats.eval_ms)
        .u64(v.stats.dropped)
        .u64(v.stats.allocations)
        .f32(v.stats.blur_ms)
        .u64(v.pressed.map_or(u64::MAX, Hit::stable_code))
        .usize(v.diagnostics.len())
        .usize(v.warnings.len())
        .bool(v.recovery.is_some());
    for diagnostic in v.diagnostics {
        rev = rev
            .str(&diagnostic.message)
            .u64(u64::from(diagnostic.line))
            .u64(u64::from(diagnostic.column))
            .bool(diagnostic.warning);
    }
    for warning in v.warnings {
        rev = rev
            .str(&warning.message)
            .u64(u64::from(warning.line))
            .u64(u64::from(warning.column))
            .bool(warning.warning);
    }
    if let Some(recovery) = v.recovery {
        rev = rev.str(&recovery.name).str(&recovery.detail);
    }
    rev.finish()
}

/// Draws the status bar.
pub fn draw(p: &mut Painter<'_>, v: &View<'_>) {
    let key = p.panel_key(PanelKind::Status, revision(v), v.layout.status_bar);
    p.panel_hover(key, v.hover_key(PanelKind::Status), |p| draw_body(p, v));
}

#[allow(clippy::too_many_lines)] // one immediate-mode strip
fn draw_body(p: &mut Painter<'_>, v: &View<'_>) {
    let t = *p.theme;
    let bar = v.layout.status_bar;
    p.fill(bar, t.app);
    p.hline(bar.x, bar.right(), bar.y, t.hairline);

    let pad = p.m.px(size::PAD);
    let icon_side = p.m.px(12.0);
    let icon = Rect::new(
        bar.x + pad,
        (bar.center_y() - icon_side * 0.5).round(),
        icon_side,
        icon_side,
    );
    let text_x = icon.right() + p.m.px(size::GRID * 1.5);

    if let Some(recovery) = v.recovery {
        icons::draw(p, Icon::Warning, icon, t.warning, t.app);
        let style = p.style(type_scale::CAPTION, t.warning);
        let message = format!(
            "Crash detected: backup {} ({}) is newer",
            recovery.name, recovery.detail
        );
        let room = Rect::new(
            text_x,
            bar.y,
            (v.layout.recovery_restore.x - text_x - p.m.px(size::PAD)).max(0.0),
            bar.h,
        );
        p.text_in(&message, room, Align::Left, &style);
        for (rect, label, hit) in [
            (
                v.layout.recovery_restore,
                "Restore",
                crate::Hit::RecoverRestore,
            ),
            (
                v.layout.recovery_dismiss,
                "Dismiss",
                crate::Hit::RecoverDismiss,
            ),
        ] {
            let state = v.interaction(hit);
            p.fill_rounded(
                rect,
                if state.hovered { t.row_hover } else { t.raised },
                p.m.px(size::RADIUS_SM),
            );
            let style = p.style(type_scale::CAPTION, t.text_body);
            p.text_in(label, rect, Align::Center, &style);
        }
        return;
    }

    // Right side first, so the message can be ellipsized before it.
    let style = p.style(type_scale::CAPTION, t.text_muted);
    let mut readout = if crate::alloc::enabled() {
        format!(
            "Eval {:.2} ms   \u{00b7}   UI {:.0} fps   \u{00b7}   Dropped {}   \u{00b7}   Alloc {}",
            v.stats.eval_ms, v.stats.ui_fps, v.stats.dropped, v.stats.allocations
        )
    } else {
        format!(
            "Eval {:.2} ms   \u{00b7}   UI {:.0} fps   \u{00b7}   Dropped {}",
            v.stats.eval_ms, v.stats.ui_fps, v.stats.dropped
        )
    };
    if v.motion_blur_preview && v.stats.blur_ms > 0.0 {
        use std::fmt::Write as _;
        let _ = write!(&mut readout, "   \u{00b7}   Blur {:.0} ms", v.stats.blur_ms);
    }
    if v.video_bytes > 0 {
        use std::fmt::Write as _;
        let _ = write!(
            &mut readout,
            "   \u{00b7}   Video {:.0} MB",
            v.video_bytes as f64 / (1024.0 * 1024.0)
        );
    }
    let readout_w = p.measure(&readout, &style);
    let readout_x = bar.right() - pad - readout_w;
    p.text_in(
        &readout,
        Rect::new(readout_x, bar.y, readout_w + 1.0, bar.h),
        Align::Left,
        &style,
    );

    let room = Rect::new(
        text_x,
        bar.y,
        (readout_x - pad * 2.0 - text_x).max(0.0),
        bar.h,
    );
    let warnings = v.warnings.len();
    if let Some(diagnostic) = v.diagnostics.first() {
        icons::draw(p, Icon::Error, icon, t.danger, t.app);
        let count = v.diagnostics.len();
        let message = if count > 1 {
            format!("{diagnostic}   (+{} more)", count - 1)
        } else {
            diagnostic.to_string()
        };
        let style = p.style(type_scale::CAPTION, t.danger);
        p.text_in(&message, room, Align::Left, &style);
    } else if let Some(warning) = v.warnings.first() {
        icons::draw(p, Icon::Warning, icon, t.warning, t.app);
        let message = if warnings > 1 {
            format!("{warning}   (+{} more)", warnings - 1)
        } else {
            warning.to_string()
        };
        let style = p.style(type_scale::CAPTION, t.warning);
        p.text_in(&message, room, Align::Left, &style);
    } else {
        icons::draw(p, Icon::Check, icon, t.success, t.app);
        let message = if v.document.is_some() {
            "No issues"
        } else {
            "Loading"
        };
        p.text_in(message, room, Align::Left, &style);
    }
}
