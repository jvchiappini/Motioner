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
use crate::widgets::{self, KeyMark};

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
    if content_h > body.h + 1.0 {
        scrollbar(p, body, content_h, v.inspector_scroll);
    }
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

/// A thin scroll thumb on the right edge of `body` showing which part of
/// `content_h` is visible.
fn scrollbar(p: &mut Painter<'_>, body: Rect, content_h: f32, scroll: f32) {
    let t = *p.theme;
    let track_w = p.m.px(3.0);
    let inset = p.m.px(2.0);
    let track = Rect::new(
        body.right() - track_w - inset,
        body.y + inset,
        track_w,
        body.h - inset * 2.0,
    );
    let ratio = (body.h / content_h).clamp(0.05, 1.0);
    let thumb_h = (track.h * ratio).round().max(p.m.px(16.0));
    let travel = (track.h - thumb_h).max(0.0);
    let max_scroll = (content_h - body.h).max(1.0);
    let y = track.y + travel * (scroll / max_scroll).clamp(0.0, 1.0);
    p.layer();
    p.fill_rounded(
        Rect::new(track.x, y.round(), track_w, thumb_h),
        t.control_hover,
        track_w * 0.5,
    );
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
            p.text_in(&row.label, *label, Align::Left, &label_style);
            if let Some(diamond) = diamond {
                draw_diamond(p, v, index, row, *diamond);
            }
            draw_value(p, v, row, fields, segments, index);
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
        t.text_disabled
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
    let mark = if channel.on_key {
        KeyMark::Key
    } else {
        KeyMark::Empty
    };
    widgets::key_marker(p, rect, mark, false);
    if v.interaction(Hit::InspectDiamond(item)).hovered {
        p.diamond(rect.inset(-p.m.px(2.0)), t.text, false);
    }
}

fn draw_value(
    p: &mut Painter<'_>,
    v: &View<'_>,
    row: &Row,
    fields: &[(Rect, FieldTarget)],
    segments: &[Rect],
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
                numeric_field(p, v, *field, &value.label, &value.text, value.suffix);
            }
        }
        RowValue::Single(value) => {
            if let Some(field) = fields.first() {
                numeric_field(p, v, *field, &value.label, &value.text, value.suffix);
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
        RowValue::Align { align, .. } => align_segments(p, v, segments, *align, item),
        RowValue::Ease { current } => ease_tiles(p, v, segments, *current, item),
        RowValue::Blend { mode, .. } => blend_tiles(p, v, segments, *mode, item),
        RowValue::EffectOps { .. } => label_tiles(p, v, segments, &["Up", "Dn", "Del"], item),
        RowValue::EffectAdd => {
            icon_tiles(
                p,
                v,
                segments,
                &[Icon::Shadow, Icon::Blur, Icon::Glow],
                item,
            );
        }
        RowValue::TextAnimAdd => {
            label_tiles(p, v, segments, &["Fade", "Rise", "Colour"], item);
        }
        RowValue::TextAnimRemove { .. } => label_tiles(p, v, segments, &["Del"], item),
        RowValue::MaskSource { current, .. } => {
            let label = current.as_deref().unwrap_or("Pick source");
            label_tiles(p, v, segments, &[label], item);
        }
        RowValue::Cycle { current, .. } => {
            label_tiles(p, v, segments, &[current], item);
        }
        RowValue::Toggle { on, .. } => {
            let active = usize::from(*on);
            active_tiles(p, v, segments, &["Off", "On"], active, item);
        }
        RowValue::MaskMode { mode, .. } => {
            let active = match mode {
                motioner_core::MaskMode::Alpha => 0,
                motioner_core::MaskMode::Luma => 1,
            };
            active_tiles(p, v, segments, &["Alpha", "Luma"], active, item);
        }
        RowValue::MaskInvert { invert, .. } => {
            let active = usize::from(*invert);
            active_tiles(p, v, segments, &["Off", "On", "Clear"], active, item);
        }
    }
}

/// Three equal tiles with a centered label; the hovered one is highlighted.
fn label_tiles(p: &mut Painter<'_>, v: &View<'_>, tiles: &[Rect], labels: &[&str], item: usize) {
    let t = *p.theme;
    for (segment, (rect, label)) in tiles.iter().zip(labels).enumerate() {
        let state = v.interaction(Hit::InspectSegment { item, segment });
        let fill = if state.hovered {
            t.control_hover
        } else {
            t.raised
        };
        p.fill_rounded(*rect, fill, p.m.px(size::RADIUS_SM));
        let style = p.style(type_scale::BODY, t.text_muted);
        p.text_in(label, *rect, Align::Center, &style);
    }
}

/// Tiles with a centered icon; hover is highlighted. Icons keep the row from
/// truncating labels at the minimum inspector width, and the tooltip names
/// each effect.
fn icon_tiles(p: &mut Painter<'_>, v: &View<'_>, tiles: &[Rect], icons: &[Icon], item: usize) {
    let t = *p.theme;
    for (segment, (rect, icon)) in tiles.iter().zip(icons).enumerate() {
        let state = v.interaction(Hit::InspectSegment { item, segment });
        let fill = if state.hovered {
            t.control_hover
        } else {
            t.raised
        };
        p.fill_rounded(*rect, fill, p.m.px(size::RADIUS_SM));
        icons::draw(
            p,
            *icon,
            rect.inset(p.m.px(4.0)),
            if state.hovered { t.text } else { t.text_muted },
            t.raised,
        );
    }
}

/// Tiles with a centered label where `active` is filled with the accent.
fn active_tiles(
    p: &mut Painter<'_>,
    v: &View<'_>,
    tiles: &[Rect],
    labels: &[&str],
    active: usize,
    item: usize,
) {
    let t = *p.theme;
    for (segment, (rect, label)) in tiles.iter().zip(labels).enumerate() {
        let state = v.interaction(Hit::InspectSegment { item, segment });
        let current = segment == active;
        let fill = if current {
            t.selection
        } else if state.hovered {
            t.control_hover
        } else {
            t.raised
        };
        p.fill_rounded(*rect, fill, p.m.px(size::RADIUS_SM));
        if current {
            p.stroke_rounded(*rect, t.accent, p.m.px(size::RADIUS_SM), p.m.hairline());
        }
        let style = p.style(
            type_scale::BODY,
            if current {
                t.accent_hover
            } else {
                t.text_muted
            },
        );
        p.text_in(label, *rect, Align::Center, &style);
    }
}

/// The ease grid: one tile per ease with its curve and name; the current
/// ease is filled with the accent.
fn ease_tiles(
    p: &mut Painter<'_>,
    v: &View<'_>,
    tiles: &[Rect],
    current: Option<motioner_core::Ease>,
    item: usize,
) {
    let t = *p.theme;
    for (segment, (rect, (ease, _))) in tiles.iter().zip(motioner_core::Ease::ALL).enumerate() {
        let state = v.interaction(Hit::InspectSegment { item, segment });
        let active = current == Some(ease);
        let fill = if active {
            t.selection
        } else if state.hovered {
            t.control_hover
        } else {
            t.raised
        };
        p.fill_rounded(*rect, fill, p.m.px(size::RADIUS_SM));
        if active {
            p.stroke_rounded(*rect, t.accent, p.m.px(size::RADIUS_SM), p.m.hairline());
        }
        let pad = p.m.px(size::GRID * 2.0);
        let label_h = p.m.px(14.0);
        let curve = Rect::new(
            rect.x + pad,
            rect.y + p.m.px(size::GRID * 1.5),
            rect.w - pad * 2.0,
            rect.h - label_h - p.m.px(size::GRID * 2.5),
        );
        let ink = if active { t.accent_hover } else { t.text_muted };
        ease_curve(p, curve, ease, ink);
        let style = p.style(
            type_scale::CAPTION,
            if active { t.text } else { t.text_muted },
        );
        p.text_in(
            ease_label(ease),
            Rect::new(
                rect.x,
                rect.bottom() - label_h - p.m.px(2.0),
                rect.w,
                label_h,
            ),
            Align::Center,
            &style,
        );
    }
}

/// A short display name for an ease tile.
fn ease_label(ease: motioner_core::Ease) -> &'static str {
    use motioner_core::Ease;
    match ease {
        Ease::Linear => "Linear",
        Ease::QuadIn => "Quad in",
        Ease::QuadOut => "Quad out",
        Ease::QuadInOut => "Quad in-out",
        Ease::CubicIn => "Cubic in",
        Ease::CubicOut => "Cubic out",
        Ease::CubicInOut => "Cubic in-out",
        Ease::BackIn => "Back in",
        Ease::BackOut => "Back out",
        Ease::Hold => "Hold",
        Ease::ElasticOut => "Elastic out",
        Ease::BounceOut => "Bounce out",
        Ease::ExpoInOut => "Expo in-out",
        Ease::SineInOut => "Sine in-out",
        Ease::Bezier(_) => "Bezier",
    }
}

/// A small progress-over-time curve of `ease` inside `rect`.
fn ease_curve(p: &mut Painter<'_>, rect: Rect, ease: motioner_core::Ease, ink: [f32; 4]) {
    // Back eases overshoot by about a tenth; leave room for it at both ends.
    let low = rect.bottom() - rect.h * 0.12;
    let span = rect.h * 0.76;
    let w = p.m.px(1.5).max(1.0);
    let steps = rect.w.ceil().max(1.0) as i32;
    let mut previous: Option<f32> = None;
    for step in 0..=steps {
        let progress = f64::from(step) / f64::from(steps);
        let x = rect.x + rect.w * progress as f32;
        let y = low - span * motioner_eval::ease(ease, progress) as f32;
        let (top, bottom) = previous.map_or((y, y), |prev| (prev.min(y), prev.max(y)));
        p.fill(
            Rect::new(x - w * 0.5, top - w * 0.5, w, bottom - top + w),
            ink,
        );
        previous = Some(y);
    }
}

fn align_segments(
    p: &mut Painter<'_>,
    v: &View<'_>,
    segments: &[Rect],
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

/// The blend mode grid: one tile per mode, four per row; the active mode is
/// filled with the accent.
fn blend_tiles(
    p: &mut Painter<'_>,
    v: &View<'_>,
    tiles: &[Rect],
    current: motioner_core::BlendMode,
    item: usize,
) {
    let t = *p.theme;
    for (segment, (rect, mode)) in tiles.iter().zip(motioner_core::BlendMode::ALL).enumerate() {
        let state = v.interaction(Hit::InspectSegment { item, segment });
        let active = current == mode;
        let fill = if active {
            t.selection
        } else if state.hovered {
            t.control_hover
        } else {
            t.raised
        };
        p.fill_rounded(*rect, fill, p.m.px(size::RADIUS_SM));
        if active {
            p.stroke_rounded(*rect, t.accent, p.m.px(size::RADIUS_SM), p.m.hairline());
        }
        let label = crate::inspect::BLEND_LABELS[segment];
        let style = p.style(
            type_scale::CAPTION,
            if active { t.text } else { t.text_muted },
        );
        p.text_in(label, *rect, Align::Center, &style);
    }
}
