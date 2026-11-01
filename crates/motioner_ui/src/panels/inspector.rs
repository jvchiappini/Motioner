//! Inspector panel: read-only properties of the selected layer (or of the
//! composition), in aligned label/value columns.

use crate::icons::{self, Icon};
use crate::inspect::{self, kind_label, Section, Value};
use crate::layout::Rect;
use crate::paint::Painter;
use crate::panels::{InspectorContent, View};
use crate::text::Align;
use crate::theme::{size, type_scale};
use crate::widgets;

/// Draws the inspector panel and returns the height of its content, which
/// bounds how far it can scroll.
pub fn draw(p: &mut Painter<'_>, v: &View<'_>) -> f32 {
    let t = *p.theme;
    let panel = v.layout.inspector;
    p.fill(panel.rect, t.panel);
    p.vline(panel.rect.x, panel.rect.y, panel.rect.bottom(), t.hairline);
    widgets::panel_header(p, panel.header, "PROPERTIES", None);
    // The header's own hairline must not cover the left separator.
    p.vline(
        panel.rect.x,
        panel.rect.y,
        panel.header.bottom(),
        t.hairline,
    );

    let body = panel.body;
    p.push_clip(body);
    let pad = p.m.px(size::PAD);
    let top = body.y - v.inspector_scroll;
    let y = top + p.m.px(size::GRID * 3.0);
    let bottom = match v.inspector {
        InspectorContent::Layer {
            name,
            kind,
            sections,
        } => {
            let y = layer_header(p, body, y, name, *kind);
            draw_sections(p, body, y, sections)
        }
        InspectorContent::Composition(sections) => {
            let y = draw_sections(p, body, y, sections);
            let hint = p.style(type_scale::CAPTION, t.text_disabled);
            let rect = Rect::new(
                body.x + pad,
                y + p.m.px(size::GRID),
                body.w - pad * 2.0,
                p.m.px(18.0),
            );
            p.text_in("Select a layer to inspect it", rect, Align::Left, &hint);
            rect.bottom()
        }
        InspectorContent::Empty => {
            let hint = p.style(type_scale::BODY, t.text_disabled);
            let rect = Rect::new(body.x + pad, y, body.w - pad * 2.0, p.m.px(18.0));
            p.text_in("No composition loaded", rect, Align::Left, &hint);
            rect.bottom()
        }
    };
    p.pop_clip();
    let content_h = bottom - top + p.m.px(size::PAD);
    // Fade the last rows into the panel when more content is below.
    if content_h - v.inspector_scroll > body.h + 1.0 {
        let fade_h = p.m.px(size::ROW);
        let mut clear = t.panel;
        clear[3] = 0.0;
        // A new layer: quads of a layer are drawn before its text, so the
        // fade must start one to cover the field labels too.
        p.layer();
        p.gradient_v(
            Rect::new(
                body.x + p.m.hairline(),
                body.bottom() - fade_h,
                body.w,
                fade_h,
            ),
            clear,
            t.panel,
        );
    }
    content_h
}

/// Kind icon tile, layer name and kind label. Returns the next free `y`.
fn layer_header(p: &mut Painter<'_>, body: Rect, y: f32, name: &str, kind: crate::NodeKind) -> f32 {
    let t = *p.theme;
    let pad = p.m.px(size::PAD);
    let tile_side = p.m.px(32.0);
    let tile = Rect::new(body.x + pad, y, tile_side, tile_side);
    p.fill_rounded(tile, t.raised, p.m.px(size::RADIUS_MD));
    icons::draw(
        p,
        Icon::for_kind(kind),
        tile.inset(p.m.px(8.0)),
        t.accent,
        t.raised,
    );
    let text_x = tile.right() + p.m.px(size::GRID * 2.5);
    let text_w = body.right() - pad - text_x;
    let name_style = p.style(type_scale::EMPHASIS, t.text).medium();
    p.text_in(
        name,
        Rect::new(text_x, y, text_w, tile_side * 0.55),
        Align::Left,
        &name_style,
    );
    let kind_style = p.style(type_scale::CAPTION, t.text_muted);
    p.text_in(
        kind_label(kind),
        Rect::new(text_x, y + tile_side * 0.5, text_w, tile_side * 0.5),
        Align::Left,
        &kind_style,
    );
    let bottom = tile.bottom() + p.m.px(size::GRID * 4.0);
    p.hline(body.x, body.right(), bottom - p.m.hairline(), t.hairline);
    bottom + p.m.px(size::GRID * 3.0)
}

/// Draws the sections from `y`; returns the next free `y`.
fn draw_sections(p: &mut Painter<'_>, body: Rect, mut y: f32, sections: &[Section]) -> f32 {
    let t = *p.theme;
    let pad = p.m.px(size::PAD);
    let row_h = p.m.px(size::ROW);
    let field_h = p.m.px(24.0);
    let label_w = p.m.px(76.0);
    let gap = p.m.px(size::GRID * 2.0);
    let value_x = body.x + pad + label_w;
    let value_w = body.right() - pad - value_x;
    for (index, section) in sections.iter().enumerate() {
        if index > 0 {
            p.hline(body.x, body.right(), y, t.hairline);
            y += p.m.px(size::GRID * 3.0);
        }
        let caption = widgets::caption(p, t.text_muted);
        let title = section.title.to_uppercase();
        p.text_in(
            &title,
            Rect::new(body.x + pad, y, body.w - pad * 2.0, p.m.px(20.0)),
            Align::Left,
            &caption,
        );
        y += p.m.px(24.0);
        for prop in &section.props {
            let row = Rect::new(body.x + pad, y, body.w - pad * 2.0, row_h);
            let label_style = p.style(type_scale::BODY, t.text_muted);
            let label_rect = Rect::new(row.x, row.y, label_w - gap, row.h);
            p.text_in(prop.label, label_rect, Align::Left, &label_style);
            if prop.animated {
                let side = p.m.px(10.0);
                let lw = p.measure(prop.label, &label_style);
                let marker = Rect::new(
                    (row.x + lw + p.m.px(size::GRID * 1.5)).min(label_rect.right() - side),
                    (row.center_y() - side * 0.5).round(),
                    side,
                    side,
                );
                icons::draw(p, Icon::Diamond, marker, t.keyframe, t.panel);
            }
            let field = Rect::new(
                value_x,
                (row.center_y() - field_h * 0.5).round(),
                value_w,
                field_h,
            );
            match &prop.value {
                Value::Pair { first, second } => {
                    let half = ((field.w - gap) * 0.5).floor();
                    widgets::field(
                        p,
                        Rect::new(field.x, field.y, half, field.h),
                        first.0,
                        &first.1,
                    );
                    widgets::field(
                        p,
                        Rect::new(field.right() - half, field.y, half, field.h),
                        second.0,
                        &second.1,
                    );
                }
                Value::Text(text) => widgets::field(p, field, "", text),
                Value::Color(color) => {
                    widgets::color_field(p, field, *color, &inspect::hex(*color));
                }
                Value::None => {
                    p.fill_rounded(field, t.raised, p.m.px(size::RADIUS_SM));
                    let style = p.style(type_scale::BODY, t.text_disabled);
                    p.text_in(
                        "None",
                        field.inset_xy(p.m.px(size::GRID * 2.0), 0.0),
                        Align::Left,
                        &style,
                    );
                }
            }
            y += row_h;
        }
        y += p.m.px(size::GRID * 2.0);
    }
    y
}
