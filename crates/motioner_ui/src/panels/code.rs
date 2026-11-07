//! The read-only code panel: program lines with numbers and the range the
//! last gesture wrote highlighted.
//!
//! The model (lines and highlight ranges) lives in [`crate::CodePanel`]; this
//! module only paints it.

use crate::layout::Rect;
use crate::paint::Painter;
use crate::panels::View;
use crate::text::Align;
use crate::theme::{size, type_scale};

/// The line height in logical pixels.
const LINE: f32 = 17.0;

/// Draws the code panel.
pub fn draw(p: &mut Painter<'_>, v: &View<'_>) {
    let t = *p.theme;
    let panel = v.layout.code_panel();
    if panel.w <= 0.0 || panel.h <= 0.0 {
        return;
    }
    p.shadow(
        panel,
        p.m.px(6.0),
        p.m.px(16.0),
        p.m.px(4.0),
        [0.0, 0.0, 0.0, 0.6],
    );
    p.fill_rounded(panel, t.panel, p.m.px(6.0));
    p.stroke_rounded(panel, t.hairline, p.m.px(6.0), p.m.px(1.0));
    let title = p.style(type_scale::CAPTION, t.text_muted).medium();
    p.text_in(
        &format!("CODE — {}", v.file_name),
        Rect::new(
            panel.x + p.m.px(size::PAD),
            panel.y + p.m.px(size::GRID),
            panel.w - p.m.px(size::PAD * 6.0),
            p.m.px(16.0),
        ),
        Align::Left,
        &title,
    );
    let close = v.layout.code_close();
    p.fill_rounded(close, t.raised, p.m.px(size::RADIUS_SM));
    let close_style = p.style(type_scale::CAPTION, t.text_body);
    p.text_in("×", close, Align::Center, &close_style);

    let body = Rect::new(
        panel.x + p.m.px(size::PAD),
        panel.y + p.m.px(LINE + size::GRID * 2.0),
        panel.w - p.m.px(size::PAD * 2.0),
        panel.h - p.m.px(LINE + size::GRID * 3.0),
    );
    if body.h <= 0.0 {
        return;
    }
    p.push_clip(body);
    let line_h = p.m.px(LINE);
    let number_style = p.style(type_scale::CAPTION, t.text_disabled);
    let code_style = p.style(type_scale::CAPTION, t.text_body);
    let edited_style = p.style(type_scale::CAPTION, t.text);
    let numbers_w = p.m.px(34.0);
    // Scroll so the first edited line is visible.
    let first = v
        .code_panel
        .highlight
        .map_or(0, |(line, _)| line.saturating_sub(1));
    let visible = (body.h / line_h).floor() as usize;
    let start = first.saturating_sub(visible.saturating_sub(1));
    for (offset, line) in v.code_panel.lines.iter().skip(start).enumerate() {
        let y = body.y + offset as f32 * line_h;
        if y + line_h > body.bottom() {
            break;
        }
        if line.edited {
            p.fill(Rect::new(body.x, y, body.w, line_h), t.selection);
        }
        p.text_in(
            &line.number.to_string(),
            Rect::new(body.x + p.m.px(2.0), y, numbers_w, line_h),
            Align::Right,
            &number_style,
        );
        let style = if line.edited {
            &edited_style
        } else {
            &code_style
        };
        p.text_in(
            &line.text,
            Rect::new(body.x + numbers_w + p.m.px(size::GRID), y, body.w, line_h),
            Align::Left,
            style,
        );
        if line.deleted {
            p.text_in(
                "deleted here",
                Rect::new(body.x + numbers_w, y, body.w, line_h),
                Align::Right,
                &p.style(type_scale::CAPTION, t.warning),
            );
        }
    }
    p.pop_clip();
}
