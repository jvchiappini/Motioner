//! Inspector panel: editable properties of the selected layer (or the
//! composition summary), drawn from a precomputed pure layout so drawing and
//! hit testing share the same rectangles.

use crate::icons::{self, Icon};
use crate::inspect::{FieldTarget, Item, Row, RowValue};
use crate::layout::{Hit, Rect};
use crate::paint::Painter;
use crate::panels::View;
use crate::text::Align;
use crate::theme::{size, type_scale};
use crate::widgets;

/// Draws the inspector and returns the height of its content.
pub fn draw(p: &mut Painter<'_>, v: &View<'_>) -> f32 {
    let t = *p.theme;
    let panel = v.layout.inspector;
    p.fill(panel.rect, t.panel);
    p.vline(panel.rect.x, panel.rect.y, panel.rect.bottom(), t.hairline);
    widgets::panel_header(p, panel.header, "PROPERTIES", None);
    p.vline(
        panel.rect.x,
        panel.rect.y,
        panel.header.bottom(),
        t.hairline,
    );

    let body = panel.body;
    p.push_clip(body);
    for (index, item) in v.inspector_layout.items.iter().enumerate() {
        draw_item(p, v, index, item);
    }
    p.pop_clip();
    let content_h = v.inspector_layout.height;
    if content_h - v.inspector_scroll > body.h + 1.0 {
        let fade_h = p.m.px(size::ROW);
        let mut clear = t.panel;
        clear[3] = 0.0;
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

fn draw_item(p: &mut Painter<'_>, v: &View<'_>, index: usize, item: &Item) {
    let t = *p.theme;
    match item {
        Item::Header { rect, copy } => header(p, *rect, *copy, v),
        Item::Section { title, rect } => {
            let caption = widgets::caption(p, t.text_muted);
            p.text_in(&title.to_uppercase(), *rect, Align::Left, &caption);
        }
        Item::Row {
            index: row_index,
            rect,
            label,
            stopwatch,
            diamond,
            fields,
            segments,
        } => {
            let Some(row) = v.inspector.rows.get(*row_index) else {
                return;
            };
            if row_hovered(v, index) {
                p.fill_rounded(
                    rect.inset_xy(p.m.px(size::GRID), 0.0),
                    t.row_hover,
                    p.m.px(size::RADIUS_SM),
                );
            }
            if let Some(stopwatch) = stopwatch {
                draw_stopwatch(p, v, index, row, *stopwatch);
            }
            let label_style = p.style(type_scale::BODY, t.text_muted);
            p.text_in(row.label, *label, Align::Left, &label_style);
            if let Some(diamond) = diamond {
                draw_diamond(p, v, index, row, *diamond);
            }
            draw_value(p, v, row, fields, segments.as_ref(), index);
        }
    }
}

fn row_hovered(v: &View<'_>, item: usize) -> bool {
    matches!(
        v.hover,
        Some(
            Hit::InspectField { item: i, .. }
                | Hit::InspectStopwatch(i)
                | Hit::InspectDiamond(i)
                | Hit::InspectSegment { item: i, .. }
        ) if i == item
    )
}

fn draw_stopwatch(p: &mut Painter<'_>, v: &View<'_>, item: usize, row: &Row, rect: Rect) {
    let t = *p.theme;
    let Some(channel) = row.channel.as_ref() else {
        return;
    };
    if v.interaction(Hit::InspectStopwatch(item)).hovered {
        p.fill_rounded(rect.inset(-p.m.px(2.0)), t.control, p.m.px(size::RADIUS_SM));
    }
    let ink = if channel.keyed {
        t.accent
    } else {
        t.text_muted
    };
    icons::draw(p, Icon::Stopwatch, rect.inset(p.m.px(1.0)), ink, t.panel);
}

fn draw_diamond(p: &mut Painter<'_>, v: &View<'_>, item: usize, row: &Row, rect: Rect) {
    let t = *p.theme;
    let Some(channel) = row.channel.as_ref() else {
        return;
    };
    if !channel.keyed {
        return;
    }
    let ink = if channel.on_key {
        t.keyframe
    } else {
        t.text_disabled
    };
    p.diamond(rect, ink, channel.on_key);
    if v.interaction(Hit::InspectDiamond(item)).hovered {
        p.diamond(rect, t.text, false);
    }
}

fn draw_value(
    p: &mut Painter<'_>,
    v: &View<'_>,
    row: &Row,
    fields: &[(Rect, FieldTarget)],
    segments: Option<&[Rect; 3]>,
    item: usize,
) {
    match &row.value {
        RowValue::ReadOnly(text) => {
            let Some((rect, _)) = fields.first() else {
                return;
            };
            p.fill_rounded(*rect, p.theme.raised, p.m.px(size::RADIUS_SM));
            let style = p.style(type_scale::BODY, p.theme.text_disabled);
            p.text_in(
                text,
                rect.inset_xy(p.m.px(size::GRID * 2.0), 0.0),
                Align::Left,
                &style,
            );
        }
        RowValue::Pair { left, right } => {
            for (field, value) in fields.iter().zip([left, right]) {
                numeric_field(p, v, *field, value.label, &value.text, value.suffix);
            }
        }
        RowValue::Single(value) => {
            if let Some(field) = fields.first() {
                numeric_field(p, v, *field, value.label, &value.text, value.suffix);
            }
        }
        RowValue::Color { color, text, .. } => {
            if let Some(field) = fields.first() {
                color_field(p, v, *field, *color, text);
            }
        }
        RowValue::Text { text, .. } => {
            if let Some(field) = fields.first() {
                text_field(p, v, *field, text);
            }
        }
        RowValue::Align { align, .. } => {
            if let Some(segments) = segments {
                align_segments(p, v, segments, *align, item);
            }
        }
    }
}

fn align_segments(
    p: &mut Painter<'_>,
    v: &View<'_>,
    segments: &[Rect; 3],
    align: motioner_core::TextAlign,
    item: usize,
) {
    let t = *p.theme;
    let names = ["Left", "Centre", "Right"];
    let current = match align {
        motioner_core::TextAlign::Left => 0,
        motioner_core::TextAlign::Center => 1,
        motioner_core::TextAlign::Right => 2,
    };
    for (segment, rect) in segments.iter().enumerate() {
        let state = v.interaction(Hit::InspectSegment { item, segment });
        let active = segment == current;
        let fill = if active {
            t.accent
        } else if state.hovered {
            t.control_hover
        } else {
            t.raised
        };
        p.fill_rounded(*rect, fill, p.m.px(size::RADIUS_SM));
        let ink = if active { t.text } else { t.text_muted };
        let style = p.style(type_scale::CAPTION, ink);
        p.text_in(names[segment], *rect, Align::Center, &style);
    }
}

fn header(p: &mut Painter<'_>, rect: Rect, copy: Rect, v: &View<'_>) {
    let t = *p.theme;
    let Some(header) = v.inspector.header.as_ref() else {
        return;
    };
    let tile_side = p.m.px(32.0);
    let tile = Rect::new(rect.x, rect.y, tile_side, tile_side);
    p.fill_rounded(tile, t.raised, p.m.px(size::RADIUS_MD));
    if let Some(row) = v.selected.and_then(|index| v.outline.get(index)) {
        icons::draw(
            p,
            Icon::for_kind(row.kind),
            tile.inset(p.m.px(8.0)),
            t.accent,
            t.raised,
        );
    }
    let text_x = tile.right() + p.m.px(size::GRID * 2.5);
    let text_w = (rect.right() - text_x).max(0.0);
    let name_style = p.style(type_scale::EMPHASIS, t.text).medium();
    p.text_in(
        &header.name,
        Rect::new(text_x, rect.y, text_w, p.m.px(18.0)),
        Align::Left,
        &name_style,
    );
    let kind_style = p.style(type_scale::CAPTION, t.text_muted);
    p.text_in(
        header.kind,
        Rect::new(text_x, rect.y + p.m.px(17.0), text_w, p.m.px(14.0)),
        Align::Left,
        &kind_style,
    );
    let id_style = p.style(type_scale::CAPTION, t.text_disabled);
    let id_text = if header.explicit {
        format!("id {}", header.identity)
    } else {
        format!("key {}", header.identity)
    };
    p.text_in(
        &id_text,
        Rect::new(
            text_x,
            rect.y + p.m.px(34.0),
            (text_w - copy.w - p.m.px(4.0)).max(0.0),
            p.m.px(16.0),
        ),
        Align::Left,
        &id_style,
    );
    let state = v.interaction(Hit::InspectCopy);
    let fill = if state.pressed {
        t.control_pressed
    } else if state.hovered {
        t.control_hover
    } else {
        t.raised
    };
    p.fill_rounded(copy, fill, p.m.px(size::RADIUS_SM));
    let copy_style = p.style(type_scale::CAPTION, t.text_muted);
    p.text_in(
        if v.copied { "Copied" } else { "Copy" },
        copy,
        Align::Center,
        &copy_style,
    );
}

fn numeric_field(
    p: &mut Painter<'_>,
    v: &View<'_>,
    field: (Rect, FieldTarget),
    label: &str,
    text: &str,
    suffix: &str,
) {
    let (rect, target) = field;
    let editing = v.editing.filter(|editing| editing.target == target);
    field_box(
        p,
        rect,
        editing.is_some(),
        editing.is_some_and(|e| e.invalid),
    );
    let pad = p.m.px(size::GRID * 2.0);
    let mut inner = rect.inset_xy(pad, 0.0);
    if !label.is_empty() {
        let style = p.style(type_scale::CAPTION, p.theme.text_disabled);
        let label_w = p.measure(label, &style);
        p.text_in(label, inner, Align::Left, &style);
        let shift = label_w + p.m.px(size::GRID);
        inner = Rect::new(inner.x + shift, inner.y, inner.w - shift, inner.h);
    }
    if let Some(editing) = editing {
        let style = p.style(type_scale::BODY, p.theme.text);
        let shown = editing.editor.text();
        p.text_in(shown, inner, Align::Left, &style);
        let caret_x =
            inner.x + p.measure(&shown[..char_byte(shown, editing.editor.caret())], &style);
        p.vline(
            caret_x.round(),
            rect.y + p.m.px(size::GRID),
            rect.bottom() - p.m.px(size::GRID),
            p.theme.accent,
        );
        return;
    }
    let color = if v.inspector_field_error == Some(target) {
        p.theme.danger
    } else {
        p.theme.text
    };
    let suffix_w = if suffix.is_empty() {
        0.0
    } else {
        p.measure(suffix, &p.style(type_scale::CAPTION, p.theme.text_disabled)) + p.m.px(size::GRID)
    };
    let value_rect = Rect::new(inner.x, inner.y, (inner.w - suffix_w).max(0.0), inner.h);
    let style = p.style(type_scale::BODY, color);
    p.text_in(text, value_rect, Align::Left, &style);
    if !suffix.is_empty() {
        let suffix_style = p.style(type_scale::CAPTION, p.theme.text_disabled);
        p.text_in(
            suffix,
            Rect::new(inner.right() - suffix_w, inner.y, suffix_w, inner.h),
            Align::Right,
            &suffix_style,
        );
    }
}

fn char_byte(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map_or(text.len(), |(byte, _)| byte)
}

fn field_box(p: &mut Painter<'_>, rect: Rect, focused: bool, invalid: bool) {
    let t = *p.theme;
    p.fill_rounded(rect, t.raised, p.m.px(size::RADIUS_SM));
    if focused {
        p.stroke_rounded(rect, t.accent, p.m.px(size::RADIUS_SM), p.m.hairline());
    } else if invalid {
        p.stroke_rounded(rect, t.danger, p.m.px(size::RADIUS_SM), p.m.hairline());
    }
}

fn color_field(
    p: &mut Painter<'_>,
    v: &View<'_>,
    field: (Rect, FieldTarget),
    color: motioner_core::Color,
    text: &str,
) {
    let (rect, target) = field;
    let editing = v.editing.filter(|editing| editing.target == target);
    field_box(
        p,
        rect,
        editing.is_some(),
        editing.is_some_and(|e| e.invalid),
    );
    if let Some(editing) = editing {
        let style = p.style(type_scale::BODY, p.theme.text);
        p.text_in(
            editing.editor.text(),
            rect.inset_xy(p.m.px(size::GRID * 2.0), 0.0),
            Align::Left,
            &style,
        );
        return;
    }
    let pad = p.m.px(size::GRID * 1.5);
    let side = rect.h - pad * 2.0;
    let swatch = Rect::new(rect.x + pad, rect.y + pad, side, side);
    p.fill_rounded(
        swatch,
        motioner_render::linear_rgba(color, 1.0),
        p.m.px(2.0),
    );
    p.stroke_rounded(swatch, p.theme.border, p.m.px(2.0), p.m.hairline());
    let text_x = swatch.right() + p.m.px(size::GRID * 2.0);
    let inner = Rect::new(text_x, rect.y, rect.right() - text_x - pad, rect.h);
    let style = p.style(type_scale::BODY, p.theme.text_body);
    p.text_in(text, inner, Align::Left, &style);
}

fn text_field(p: &mut Painter<'_>, v: &View<'_>, field: (Rect, FieldTarget), text: &str) {
    let (rect, target) = field;
    let editing = v.editing.filter(|editing| editing.target == target);
    field_box(
        p,
        rect,
        editing.is_some(),
        editing.is_some_and(|e| e.invalid),
    );
    let pad = p.m.px(size::GRID * 2.0);
    if let Some(editing) = editing {
        let style = p.style(type_scale::BODY, p.theme.text);
        let shown = editing.editor.text();
        p.text_in(shown, rect.inset_xy(pad, 0.0), Align::Left, &style);
        let caret_x =
            rect.x + pad + p.measure(&shown[..char_byte(shown, editing.editor.caret())], &style);
        p.vline(
            caret_x.round(),
            rect.y + p.m.px(size::GRID),
            rect.bottom() - p.m.px(size::GRID),
            p.theme.accent,
        );
        return;
    }
    let style = p.style(type_scale::BODY, p.theme.text_body);
    p.text_in(text, rect.inset_xy(pad, 0.0), Align::Left, &style);
}
