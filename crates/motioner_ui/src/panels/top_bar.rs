//! Top bar: app mark, file name with a state dot, composition pills, the
//! safe-area toggle and the zoom readout.

use crate::icons::{self, Icon};
use crate::layout::{Hit, Rect, TopBarToggle};
use crate::paint::Painter;
use crate::panels::View;
use crate::text::Align;
use crate::theme::{size, type_scale};
use crate::widgets::{self, ButtonKind};

/// Draws the top bar.
pub fn draw(p: &mut Painter<'_>, v: &View<'_>) {
    let t = *p.theme;
    let bar = v.layout.top_bar;
    p.fill(bar, t.panel);
    p.hline(
        bar.x,
        bar.right(),
        bar.bottom() - p.m.hairline(),
        t.hairline,
    );

    let pad = p.m.px(size::PAD);
    let cy = bar.center_y();

    // App mark: an accent tile with a play glyph.
    let mark_side = p.m.px(20.0);
    let mark = Rect::new(
        bar.x + pad,
        (cy - mark_side * 0.5).round(),
        mark_side,
        mark_side,
    );
    p.fill_rounded(mark, t.accent, p.m.px(5.0));
    let glyph = mark.inset(p.m.px(4.0));
    icons::draw(p, Icon::Play, glyph, t.text, t.accent);

    let brand = p.style(type_scale::EMPHASIS, t.text).medium();
    let brand_x = mark.right() + p.m.px(size::GRID * 2.0);
    let brand_w = p.measure("Motioner", &brand);
    p.text_in(
        "Motioner",
        Rect::new(brand_x, bar.y, brand_w + 1.0, bar.h),
        Align::Left,
        &brand,
    );

    // Separator, then the file with its state dot.
    let sep_x = (brand_x + brand_w + p.m.px(size::PAD)).round();
    let sep_h = p.m.px(16.0);
    p.vline(sep_x, cy - sep_h * 0.5, cy + sep_h * 0.5, t.hairline);
    let file_style = p.style(type_scale::EMPHASIS, t.text_body);
    let file_x = sep_x + p.m.px(size::PAD);
    let file_w = p.measure(v.file_name, &file_style).min(bar.w * 0.25);
    p.text_in(
        v.file_name,
        Rect::new(file_x, bar.y, file_w + 1.0, bar.h),
        Align::Left,
        &file_style,
    );
    let dot_r = p.m.px(3.0);
    let dot_x = file_x + file_w + p.m.px(size::GRID * 2.0) + dot_r;
    let dot_color = if v.has_error() {
        t.danger
    } else {
        let mut c = t.success;
        c[3] = 0.35 + 0.65 * v.reload_pulse;
        c
    };
    if v.reload_pulse > 0.0 && !v.has_error() {
        let halo = dot_r * (1.0 + 2.0 * (1.0 - v.reload_pulse));
        let mut c = t.success;
        c[3] = 0.25 * v.reload_pulse;
        p.circle(dot_x, cy, halo, c);
    }
    p.circle(dot_x, cy, dot_r, dot_color);

    if let Some(document) = v.document {
        comp_pills(p, v, document, dot_x);
    }
    right_side(p, v);
}

/// Size, rate and length pills, centred on the canvas column but never
/// before `min_x`.
fn comp_pills(p: &mut Painter<'_>, v: &View<'_>, document: &motioner_core::Document, min_x: f32) {
    let t = *p.theme;
    let cy = v.layout.top_bar.center_y();
    {
        let comp = document.composition();
        let rate = comp.frame_rate();
        let fps = if rate.denominator() == 1 {
            format!("{} fps", rate.numerator())
        } else {
            format!("{:.3} fps", rate.as_f64())
        };
        let seconds = comp.duration() as f64 / rate.as_f64();
        let pills = [
            format!("{} × {}", comp.width(), comp.height()),
            fps,
            format!("{seconds:.1} s"),
        ];
        let style = p.style(type_scale::CAPTION, t.text_muted);
        let gap = p.m.px(size::GRID * 1.5);
        let total: f32 = pills
            .iter()
            .map(|s| widgets::pill_width(p, s, &style))
            .sum::<f32>()
            + gap * (pills.len() - 1) as f32;
        let canvas = v.layout.canvas;
        let mut x = (canvas.center_x() - total * 0.5).max(min_x + p.m.px(size::PAD * 2.0));
        for text in &pills {
            let rect = widgets::pill(p, x, cy, text, &style, t.raised);
            x = rect.right() + gap;
        }
    }
}

/// The snapping and safe-area toggles and the zoom readout.
fn right_side(p: &mut Painter<'_>, v: &View<'_>) {
    let t = *p.theme;
    let toggle = v.layout.safe_area_toggle;
    let mut state = v.interaction(Hit::Toggle(TopBarToggle::SafeArea));
    state.active = v.safe_area;
    widgets::icon_button(p, toggle, Icon::SafeArea, ButtonKind::Ghost, state);
    let snap = v.layout.snap_toggle;
    let mut state = v.interaction(Hit::SnapToggle);
    state.active = v.snap;
    widgets::icon_button(p, snap, Icon::Snap, ButtonKind::Ghost, state);

    let zoom_text = if v.comp_rect.is_some() {
        format!("{:.0}%", v.zoom * 100.0)
    } else {
        "—".to_owned()
    };
    let style = p.style(type_scale::CAPTION, t.text_body);
    let zoom = v.layout.zoom_pill;
    let state = v.interaction(Hit::ZoomPill);
    p.fill_rounded(
        zoom,
        if state.hovered { t.row_hover } else { t.raised },
        zoom.h * 0.5,
    );
    p.text_in(&zoom_text, zoom, Align::Center, &style);
}
