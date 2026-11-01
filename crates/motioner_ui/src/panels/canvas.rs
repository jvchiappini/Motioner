//! Canvas: the pasteboard and composition frame (drawn in the engine's
//! `Pure2D` pass, under the composition) and the overlays drawn on top of it
//! in the UI pass (frame border, selection, hover, safe areas, state cards).
//!
//! The canvas shows exactly the `FrameRenderer` output for the transport
//! frame: the backdrop only fills the area around and under the composition,
//! and every overlay is UI chrome above it.

use ferrous_renderer::render_2d::ShapeInstance;
use ferrous_renderer::Renderer;
use motioner_core::Color;
use motioner_render::linear_rgba;

use crate::icons::{self, Icon};
use crate::layout::Rect;
use crate::paint::Painter;
use crate::panels::View;
use crate::text::Align;
use crate::theme::{size, type_scale, Theme};

/// Depth of the pasteboard: below every composition item (`-50..50`).
const Z_PASTEBOARD: f32 = -60.0;
/// Depth of the composition's drop shadow.
const Z_SHADOW: f32 = -58.0;
/// Depth of the composition background.
const Z_BACKGROUND: f32 = -55.0;

/// Fills the canvas and the composition background in the `Pure2D` pass.
///
/// `window` is the window size in physical pixels (`Pure2D` world space has
/// its origin at the window centre and `y` up).
pub fn paint_backdrop(
    renderer: &mut Renderer,
    theme: &Theme,
    window: (f32, f32),
    canvas: Rect,
    comp: Option<(Rect, Color)>,
    shadow_px: f32,
) {
    push_rect(
        renderer,
        window,
        canvas,
        theme.pasteboard,
        0.0,
        Z_PASTEBOARD,
    );
    let Some((rect, background)) = comp else {
        return;
    };
    // A soft shadow from stacked translucent rounded rects: the 2D pass has
    // no blur, and four steps read as a smooth falloff on a dark pasteboard.
    let steps = 4;
    for step in (1..=steps).rev() {
        let grow = shadow_px * step as f32 / steps as f32;
        let mut color = theme.shadow;
        color[3] = 0.16;
        let r = Rect::new(
            rect.x - grow,
            rect.y - grow + shadow_px * 0.35,
            rect.w + grow * 2.0,
            rect.h + grow * 2.0,
        );
        push_rect(renderer, window, r, color, grow, Z_SHADOW);
    }
    push_rect(
        renderer,
        window,
        rect,
        linear_rgba(background, 1.0),
        0.0,
        Z_BACKGROUND,
    );
}

fn push_rect(
    renderer: &mut Renderer,
    window: (f32, f32),
    rect: Rect,
    color: [f32; 4],
    radius: f32,
    z: f32,
) {
    if rect.is_empty() {
        return;
    }
    let center = ferrous_renderer::glam::Vec2::new(
        rect.center_x() - window.0 * 0.5,
        window.1 * 0.5 - rect.center_y(),
    );
    let size = ferrous_renderer::glam::Vec2::new(rect.w, rect.h);
    let mut shape = ShapeInstance::rounded_rect(center, size, radius, color);
    shape.transform_c3[2] = z;
    renderer.draw_2d_shape(shape);
}

/// Draws the overlays above the composition, clipped to the canvas.
pub fn draw_overlays(p: &mut Painter<'_>, v: &View<'_>, time: f32) {
    let t = *p.theme;
    let canvas = v.layout.canvas;
    p.push_clip(canvas);

    if let Some(comp) = v.comp_rect {
        let h = p.m.hairline();
        p.stroke_rounded(
            Rect::new(comp.x - h, comp.y - h, comp.w + 2.0 * h, comp.h + 2.0 * h),
            t.border,
            0.0,
            h,
        );
        if v.safe_area {
            safe_areas(p, comp);
        }
        if v.document.is_some_and(motioner_core::Document::is_empty) {
            empty_card(p, comp, v.file_name);
        }
    }
    if let Some(bounds) = v.hover_bounds {
        if v.selection_bounds != Some(bounds) {
            p.stroke_rounded(bounds, t.hover_outline, 0.0, p.m.hairline());
        }
    }
    if let Some(bounds) = v.selection_bounds {
        selection(p, bounds, v.selection_size);
    }

    match (v.document.is_some(), v.has_error()) {
        (true, true) => error_banner(p, canvas, v),
        (false, true) => error_card(p, canvas, v),
        (false, false) => loading(p, canvas, time),
        (true, false) => {}
    }
    p.pop_clip();
}

fn safe_areas(p: &mut Painter<'_>, comp: Rect) {
    let mut color = crate::theme::palette::WHITE.linear();
    color[3] = 0.18;
    let h = p.m.hairline();
    for fraction in [0.9_f32, 0.8] {
        let inset_x = (comp.w * (1.0 - fraction) * 0.5).round();
        let inset_y = (comp.h * (1.0 - fraction) * 0.5).round();
        p.stroke_rounded(comp.inset_xy(inset_x, inset_y), color, 0.0, h);
    }
    let arm = p.m.px(8.0);
    let (cx, cy) = (comp.center_x().round(), comp.center_y().round());
    p.hline(cx - arm, cx + arm + h, cy, color);
    p.vline(cx, cy - arm, cy + arm + h, color);
}

fn selection(p: &mut Painter<'_>, bounds: Rect, size_label: Option<(f64, f64)>) {
    let t = *p.theme;
    let h = p.m.hairline();
    p.stroke_rounded(bounds, t.accent, 0.0, h);
    let handle = p.m.px(7.0);
    for (x, y) in [
        (bounds.x, bounds.y),
        (bounds.right(), bounds.y),
        (bounds.x, bounds.bottom()),
        (bounds.right(), bounds.bottom()),
    ] {
        let r = Rect::new(
            (x - handle * 0.5).round(),
            (y - handle * 0.5).round(),
            handle,
            handle,
        );
        p.fill(r, crate::theme::palette::WHITE.linear());
        p.stroke_rounded(r, t.accent, 0.0, h);
    }
    if let Some((w, hgt)) = size_label {
        let text = format!("{} × {}", w.round(), hgt.round());
        let style = p.style(type_scale::CAPTION, t.text).medium();
        let width = crate::widgets::pill_width(p, &text, &style);
        let x = bounds.center_x() - width * 0.5;
        let cy = bounds.bottom() + p.m.px(size::GRID * 2.0) + p.m.px(10.0);
        crate::widgets::pill(p, x, cy, &text, &style, t.accent);
    }
}

fn empty_card(p: &mut Painter<'_>, comp: Rect, file_name: &str) {
    let t = *p.theme;
    let line = p.m.px(20.0);
    let title_style = p.style(type_scale::EMPHASIS, t.text_muted).medium();
    let hint_style = p.style(type_scale::CAPTION, t.text_disabled);
    let cy = comp.center_y();
    p.text_in(
        "Empty composition",
        Rect::new(comp.x, cy - line, comp.w, line),
        Align::Center,
        &title_style,
    );
    let hint = format!("Add layers in {file_name} and save");
    p.text_in(
        &hint,
        Rect::new(comp.x, cy, comp.w, line),
        Align::Center,
        &hint_style,
    );
}

fn error_banner(p: &mut Painter<'_>, canvas: Rect, v: &View<'_>) {
    let t = *p.theme;
    let Some(diagnostic) = v.diagnostics.first() else {
        return;
    };
    // Bottom of the canvas, like a toast: it must not cover the top edge of
    // the composition, which is where titles usually sit.
    let pad = p.m.px(size::PAD);
    let h = p.m.px(36.0);
    let rect = Rect::new(
        canvas.x + pad,
        canvas.bottom() - pad - h,
        canvas.w - pad * 2.0,
        h,
    );
    p.fill_rounded(rect, t.panel, p.m.px(size::RADIUS_MD));
    p.fill_rounded(rect, t.danger_soft, p.m.px(size::RADIUS_MD));
    p.stroke_rounded(rect, t.danger, p.m.px(size::RADIUS_MD), p.m.hairline());
    let icon_side = p.m.px(16.0);
    let icon = Rect::new(
        rect.x + pad,
        (rect.center_y() - icon_side * 0.5).round(),
        icon_side,
        icon_side,
    );
    icons::draw(p, Icon::Error, icon, t.danger, t.panel);
    let note = "Showing the last good version";
    let note_style = p.style(type_scale::CAPTION, t.text_muted);
    let note_w = p.measure(note, &note_style);
    let text_x = icon.right() + p.m.px(size::GRID * 2.0);
    let message_style = p.style(type_scale::BODY, t.text);
    let message = Rect::new(
        text_x,
        rect.y,
        (rect.right() - pad * 2.0 - note_w - text_x).max(0.0),
        rect.h,
    );
    p.text_in(
        &diagnostic.to_string(),
        message,
        Align::Left,
        &message_style,
    );
    p.text_in(
        note,
        Rect::new(rect.right() - pad - note_w, rect.y, note_w + 1.0, rect.h),
        Align::Left,
        &note_style,
    );
}

fn error_card(p: &mut Painter<'_>, canvas: Rect, v: &View<'_>) {
    let t = *p.theme;
    let Some(diagnostic) = v.diagnostics.first() else {
        return;
    };
    let w = p.m.px(440.0).min(canvas.w - p.m.px(size::PAD * 2.0));
    let h = p.m.px(148.0);
    let rect = Rect::new(
        (canvas.center_x() - w * 0.5).round(),
        (canvas.center_y() - h * 0.5).round(),
        w,
        h,
    );
    p.shadow(rect, p.m.px(10.0), p.m.px(10.0), p.m.px(6.0), t.shadow);
    p.fill_rounded(rect, t.raised, p.m.px(10.0));
    p.stroke_rounded(rect, t.border, p.m.px(10.0), p.m.hairline());
    let pad = p.m.px(size::PAD * 2.0);
    let inner = rect.inset(pad);
    let icon_side = p.m.px(24.0);
    let icon = Rect::new(inner.x, inner.y, icon_side, icon_side);
    icons::draw(p, Icon::Error, icon, t.danger, t.raised);
    let text_x = icon.right() + p.m.px(size::PAD);
    let text_w = inner.right() - text_x;
    let line = p.m.px(20.0);
    let title_style = p.style(type_scale::TITLE, t.text).medium();
    let title = format!("Couldn't load {}", v.file_name);
    p.text_in(
        &title,
        Rect::new(text_x, inner.y + p.m.px(2.0), text_w, line),
        Align::Left,
        &title_style,
    );
    let where_style = p.style(type_scale::BODY, t.danger);
    let location = if diagnostic.line == 0 {
        "File error".to_owned()
    } else {
        format!("Line {}, column {}", diagnostic.line, diagnostic.column)
    };
    let mut y = inner.y + line + p.m.px(size::GRID * 2.0);
    p.text_in(
        &location,
        Rect::new(text_x, y, text_w, line),
        Align::Left,
        &where_style,
    );
    y += line;
    let body_style = p.style(type_scale::BODY, t.text_body);
    p.text_in(
        &diagnostic.message,
        Rect::new(text_x, y, text_w, line),
        Align::Left,
        &body_style,
    );
    y += line + p.m.px(size::GRID * 2.0);
    let hint_style = p.style(type_scale::CAPTION, t.text_muted);
    p.text_in(
        "Fix the file and save. Motioner reloads it automatically.",
        Rect::new(text_x, y, text_w, line),
        Align::Left,
        &hint_style,
    );
}

fn loading(p: &mut Painter<'_>, canvas: Rect, time: f32) {
    let t = *p.theme;
    let r = p.m.px(3.0);
    let gap = p.m.px(10.0);
    let cy = canvas.center_y() - p.m.px(10.0);
    for i in 0..3 {
        let phase = (time * 3.0 - i as f32 * 0.6).sin() * 0.5 + 0.5;
        let mut c = t.text_muted;
        c[3] = 0.25 + 0.75 * phase;
        p.circle(canvas.center_x() + (i as f32 - 1.0) * gap, cy, r, c);
    }
    let style = p.style(type_scale::CAPTION, t.text_muted);
    let line = p.m.px(18.0);
    p.text_in(
        "Loading composition",
        Rect::new(canvas.x, cy + p.m.px(12.0), canvas.w, line),
        Align::Center,
        &style,
    );
}
