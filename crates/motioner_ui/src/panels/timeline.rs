//! Timeline: transport strip, ruler, layer rows with visibility strips and
//! key summaries, and the expanded property sub-rows of the selected layer.

use motioner_core::{Document, Frame, NodeId, Property};
use motioner_eval::sample;

use crate::icons::{self, Icon};
use crate::inspect::hex;
use crate::layout::{Hit, Rect, TransportButton};
use crate::paint::Painter;
use crate::panels::View;
use crate::text::Align;
use crate::theme::{size, type_scale};
use crate::timeline::{frame_to_x, tick_frames, tick_spacing, TickUnit, MIN_TICK_SPACING_PX};
use crate::timeline_model::{RowKind, TimelineRow};
use crate::transport::timecode;
use crate::widgets::{self, ButtonKind, KeyMark};

/// Draws the timeline.
pub fn draw(p: &mut Painter<'_>, v: &View<'_>) {
    let t = *p.theme;
    let tl = v.layout.timeline;
    p.fill(tl.rect, t.panel);
    p.hline(tl.rect.x, tl.rect.right(), tl.rect.y, t.hairline);

    header(p, v);
    ruler(p, v);
    rows(p, v);
    if v.document.is_some() {
        playhead(p, v);
    }
    retime_pill(p, v);
}

fn header(p: &mut Painter<'_>, v: &View<'_>) {
    let t = *p.theme;
    let tl = v.layout.timeline;
    let header = tl.header;
    p.hline(
        header.x,
        header.right(),
        header.bottom() - p.m.hairline(),
        t.hairline,
    );

    let pad = p.m.px(size::PAD);
    let loaded = v.document.is_some();
    let tc_style = p
        .style(
            type_scale::TITLE,
            if loaded { t.text } else { t.text_disabled },
        )
        .medium()
        .tracked(0.02);
    let tc = if loaded {
        timecode(v.frame, v.rate)
    } else {
        "--:--:--:--".to_owned()
    };
    let tc_w = p.measure(&tc, &tc_style);
    let tc_rect = Rect::new(header.x + pad, header.y, tc_w + 1.0, header.h);
    p.text_in(&tc, tc_rect, Align::Left, &tc_style);
    let counter_style = p.style(type_scale::CAPTION, t.text_muted);
    let counter = if loaded {
        format!("{} / {}", v.frame, v.duration.max(1) - 1)
    } else {
        String::new()
    };
    let counter_rect = Rect::new(
        tc_rect.right() + p.m.px(size::GRID * 3.0),
        header.y,
        p.m.px(96.0),
        header.h,
    );
    p.text_in(&counter, counter_rect, Align::Left, &counter_style);

    for (button, rect) in v.layout.transport {
        let (icon, kind) = match button {
            TransportButton::ToStart => (Icon::ToStart, ButtonKind::Ghost),
            TransportButton::StepBack => (Icon::StepBack, ButtonKind::Ghost),
            TransportButton::PlayPause => (
                if v.playing { Icon::Pause } else { Icon::Play },
                ButtonKind::Primary,
            ),
            TransportButton::StepForward => (Icon::StepForward, ButtonKind::Ghost),
            TransportButton::ToEnd => (Icon::ToEnd, ButtonKind::Ghost),
            TransportButton::Loop => (Icon::Loop, ButtonKind::Ghost),
        };
        let mut state = v.interaction(Hit::Transport(button));
        state.active = button == TransportButton::Loop && v.looping;
        state.disabled = v.document.is_none();
        widgets::icon_button(p, rect, icon, kind, state);
    }

    // Selected keys: count and the U-filter hint, right of the transport.
    let selected = v.selected_keys.len();
    if selected > 0 {
        let style = p.style(type_scale::CAPTION, t.text_muted);
        let label = if selected == 1 {
            "1 key".to_owned()
        } else {
            format!("{selected} keys")
        };
        let w = p.measure(&label, &style);
        let x = v.layout.transport[5].1.right() + p.m.px(size::PAD * 2.0);
        if x + w < header.right() - p.m.px(size::PAD * 4.0) {
            p.text_in(
                &label,
                Rect::new(x, header.y, w, header.h),
                Align::Left,
                &style,
            );
        }
    }

    let rate = v.rate;
    let seconds = v.duration as f64 * f64::from(rate.denominator()) / f64::from(rate.numerator());
    let info = format!("{seconds:.2} s \u{00b7} {} frames", v.duration);
    let info_w = p.measure(&info, &counter_style);
    let info_rect = Rect::new(
        header.right() - pad - info_w,
        header.y,
        info_w + 1.0,
        header.h,
    );
    let transport_end = v.layout.transport[5].1.right();
    if loaded && info_rect.x > transport_end + pad {
        p.text_in(&info, info_rect, Align::Left, &counter_style);
    }
}

fn ruler(p: &mut Painter<'_>, v: &View<'_>) {
    let t = *p.theme;
    let tl = v.layout.timeline;
    p.fill(tl.corner, t.raised);
    p.fill(tl.ruler, t.raised);
    p.hline(
        tl.ruler.x - tl.corner.w,
        tl.ruler.right(),
        tl.ruler.bottom() - p.m.hairline(),
        t.hairline,
    );
    let caption = widgets::caption(p, t.text_disabled);
    p.text_in(
        "LAYER",
        tl.corner.inset_xy(p.m.px(size::PAD), 0.0),
        Align::Left,
        &caption,
    );
    if v.document.is_none() {
        return;
    }

    let scrub = tl.scrub;
    let duration = v.duration.max(1);
    let spacing = tick_spacing(
        duration,
        v.rate,
        f64::from(scrub.w),
        MIN_TICK_SPACING_PX * f64::from(p.m.scale),
    );
    let head = v.document.is_some().then(|| head_rect(p, v).0);
    p.push_clip(tl.ruler);
    let label_style = p.style(type_scale::CAPTION, t.text_muted);
    let major_h = p.m.px(8.0);
    let minor_h = p.m.px(4.0);
    let bottom = tl.ruler.bottom() - p.m.hairline();
    let step = spacing.step_frames.max(1);
    let minor_step = minor_step(step);
    let mut frame = 0;
    while frame < duration {
        let x = frame_to_x(frame, duration, f64::from(scrub.x), f64::from(scrub.w)).round() as f32;
        if frame % step == 0 {
            p.vline(x, bottom - major_h, bottom, t.text_disabled);
        } else {
            p.vline(x, bottom - minor_h, bottom, t.border);
        }
        frame += minor_step;
    }
    // Labels are centred on their tick so the mark and the number agree.
    for tick in tick_frames(duration, spacing) {
        let x = frame_to_x(tick, duration, f64::from(scrub.x), f64::from(scrub.w)).round() as f32;
        let label = match spacing.unit {
            TickUnit::Frames => tick.to_string(),
            TickUnit::Seconds => {
                let seconds =
                    tick as f64 * f64::from(v.rate.denominator()) / f64::from(v.rate.numerator());
                format!("{seconds:.0}s")
            }
        };
        let w = p.measure(&label, &label_style);
        let rect = Rect::new(
            (x - w * 0.5).round(),
            tl.ruler.y,
            w + 1.0,
            tl.ruler.h - major_h,
        );
        if head.is_some_and(|h| h.overlaps(rect)) {
            continue;
        }
        p.text_in(&label, rect, Align::Center, &label_style);
    }
    // The composition end is explicit: its tick and label sit at the right
    // edge, where the last frame maps to.
    let end_frame = duration - 1;
    let end =
        frame_to_x(end_frame, duration, f64::from(scrub.x), f64::from(scrub.w)).round() as f32;
    p.vline(end, bottom - major_h, bottom, t.text_disabled);
    let seconds =
        v.duration as f64 * f64::from(v.rate.denominator()) / f64::from(v.rate.numerator());
    let end_label = format!("{seconds:.0}s");
    let w = p.measure(&end_label, &label_style);
    let rect = Rect::new(end - w, tl.ruler.y, w + 1.0, tl.ruler.h - major_h);
    if !head.is_some_and(|h| h.overlaps(rect)) {
        p.text_in(&end_label, rect, Align::Right, &label_style);
    }
    p.pop_clip();
}

/// Minor tick step: a fifth or a sixth of the major step when that divides
/// it, otherwise every frame for tiny steps.
fn minor_step(major: i64) -> i64 {
    for parts in [6, 5, 4, 2] {
        if major % parts == 0 && major / parts >= 1 {
            return major / parts;
        }
    }
    major.max(1)
}

fn rows(p: &mut Painter<'_>, v: &View<'_>) {
    let t = *p.theme;
    let tl = v.layout.timeline;
    p.fill(tl.tracks, t.app);
    p.vline(
        tl.names.right() - p.m.hairline(),
        tl.corner.y,
        tl.names.bottom(),
        t.hairline,
    );

    let area = Rect::new(tl.names.x, tl.names.y, tl.rect.w, tl.names.h);
    p.push_clip(area);
    for row_rect in &v.timeline_layout.rows {
        if row_rect.rect.bottom() < area.y || row_rect.rect.y > area.bottom() {
            continue;
        }
        let Some(row) = v.timeline.rows().get(row_rect.row) else {
            continue;
        };
        match row.kind {
            RowKind::Layer { outline } => track_row(p, v, row_rect.row, row, outline),
            RowKind::Transform {
                outline,
                property,
                axis,
            } => {
                property_row(p, v, row_rect.row, row, outline, property, axis);
            }
            RowKind::Content { outline, field } => {
                content_row(p, v, row_rect.row, row, outline, field);
            }
        }
    }
    if v.timeline.is_empty() {
        let style = p.style(type_scale::CAPTION, t.text_disabled);
        let pad = p.m.px(size::PAD);
        let rect = Rect::new(
            tl.names.x + pad,
            tl.names.y,
            tl.names.w - pad,
            p.m.px(size::ROW),
        );
        p.text_in("No tracks", rect, Align::Left, &style);
    }
    p.pop_clip();
}

fn row_background(
    p: &mut Painter<'_>,
    v: &View<'_>,
    row: usize,
    outline: usize,
    rect: Rect,
) -> bool {
    let t = *p.theme;
    let is_layer = v
        .timeline
        .rows()
        .get(row)
        .is_some_and(|entry| entry.kind.is_layer());
    let selected = v.selected == Some(outline);
    if !is_layer {
        // Property rows of an expanded layer sit on a darker names column so
        // they read as the layer's children; only the layer row itself
        // carries the selection colour.
        let names = v.layout.timeline.names;
        p.fill(
            Rect::new(rect.x, rect.y, names.right() - rect.x, rect.h),
            t.sub_row,
        );
    }
    if selected && is_layer {
        p.fill(rect, t.selection);
    } else if matches!(
        v.hover,
        Some(
            Hit::TimelineRow(r)
                | Hit::TimelineStopwatch(r)
                | Hit::TimelinePrevKey(r)
                | Hit::TimelineKeyToggle(r)
                | Hit::TimelineNextKey(r)
                | Hit::TimelineField(r)
                | Hit::TimelineDiamond { row: r, .. }
        ) if r == row
    ) {
        p.fill(rect, t.row_hover);
    }
    p.hline(
        rect.x,
        rect.right(),
        rect.bottom() - p.m.hairline(),
        t.hairline,
    );
    selected
}

/// Sound icon + name + (for layers) chevron; returns the name rectangle.
fn name_cell(
    p: &mut Painter<'_>,
    v: &View<'_>,
    row_rect: &crate::timeline_model::RowRect,
    row: &TimelineRow,
    name: &str,
    dim: bool,
) {
    let t = *p.theme;
    let icon_side = p.m.px(14.0);
    let chevron_side = p.m.px(12.0);
    let tl = v.layout.timeline;
    let y = row_rect.rect.center_y();
    if let Some(chevron) = row_rect.chevron {
        let open = v.expanded.contains(&row.kind.outline());
        let glyph = if open {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        };
        let ink = if v.hover == Some(Hit::LayerChevron(row_rect.row)) {
            t.text
        } else {
            t.text_muted
        };
        icons::draw(p, glyph, chevron, ink, t.panel);
    }
    if row.kind.is_layer() {
        if let Some(outline) = v.outline.get(row.kind.outline()) {
            let icon = Rect::new(
                row_rect.rect.x
                    + p.m.px(size::PAD)
                    + p.m.px(size::INDENT) * row.depth as f32
                    + chevron_side
                    + p.m.px(2.0),
                (y - icon_side * 0.5).round(),
                icon_side,
                icon_side,
            );
            let _ = tl;
            icons::draw(
                p,
                Icon::for_kind(outline.kind),
                icon,
                if dim { t.text_disabled } else { t.text_muted },
                t.panel,
            );
        }
    }
    let style = p.style(
        type_scale::BODY,
        if dim {
            t.text_disabled
        } else if v.selected == Some(row.kind.outline()) {
            t.text
        } else {
            t.text_body
        },
    );
    p.text_in(name, row_rect.name, Align::Left, &style);
}

fn track_row(p: &mut Painter<'_>, v: &View<'_>, index: usize, row: &TimelineRow, outline: usize) {
    let t = *p.theme;
    let Some(rect) = v.timeline_layout.rows.get(index).map(|r| r.rect) else {
        return;
    };
    let (visible, locked) = v
        .layer_states
        .get(outline)
        .copied()
        .unwrap_or((true, false));
    row_background(p, v, index, outline, rect);
    let Some(outline_row) = v.outline.get(outline) else {
        return;
    };
    let name = outline_row.name.clone();
    let row_rect = v.timeline_layout.rows[index].clone();
    name_cell(p, v, &row_rect, row, &name, !visible || locked);

    let tl = v.layout.timeline;
    let duration = v.duration.max(1);
    let scrub = tl.scrub;
    let x_of =
        |frame: i64| frame_to_x(frame, duration, f64::from(scrub.x), f64::from(scrub.w)) as f32;
    let bar_h = p.m.px(12.0);
    let bar_y = (rect.center_y() - bar_h * 0.5).round();
    let full = Rect::new(
        x_of(0).round(),
        bar_y,
        (x_of(duration - 1) - x_of(0)).round(),
        bar_h,
    );
    // Frames where the layer is not drawn are an empty outline; visible
    // runs are a solid bar on top, so absence reads at a glance.
    p.stroke_rounded(full, t.track_bar_hidden, p.m.px(3.0), p.m.hairline());
    if visible {
        let bar_fill = if v.selected == Some(outline) {
            t.track_bar_selected
        } else {
            t.track_bar
        };
        for run in &row.visible {
            let from = x_of(run.from).max(full.x).round();
            // A run is inclusive: it reaches the start of the next frame.
            let to = x_of(run.to + 1).min(full.right()).round();
            if to > from {
                p.fill_rounded(
                    Rect::new(from, bar_y, (to - from).max(1.0), bar_h),
                    bar_fill,
                    p.m.px(3.0),
                );
            }
        }
    }
    let diamond = p.m.px(10.0);
    let marker_at = |frame: Frame| {
        Rect::new(
            (x_of(frame) - diamond * 0.5).round(),
            (rect.center_y() - diamond * 0.5).round(),
            diamond,
            diamond,
        )
    };
    let dim = !visible || locked;
    for frame in &row.inherited {
        widgets::key_marker(p, marker_at(*frame), KeyMark::Inherited, dim);
    }
    let id = layer_identity(v, outline);
    for key in &row.summary {
        let selected = id.as_deref().is_some_and(|id| {
            v.selected_keys
                .iter()
                .any(|k| k.frame == key.frame && k.id == id)
        });
        let mark = if selected {
            KeyMark::Selected
        } else if key.channels > 1 {
            KeyMark::Shared
        } else {
            KeyMark::Key
        };
        widgets::key_marker(p, marker_at(key.frame), mark, dim);
    }
}

/// The identity of an outline row's node.
fn layer_identity(v: &View<'_>, outline: usize) -> Option<String> {
    let node = v.outline.get(outline)?.node;
    let document = v.document?;
    Some(document.node(node)?.identity().as_str().to_owned())
}

/// The override name of a channel: `pos.x`, `opacity`.
fn channel_name(property: Property, axis: Option<motioner_core::Axis>) -> String {
    match axis {
        Some(axis) => format!("{}.{}", property.name(), axis.name()),
        None => property.name().to_owned(),
    }
}

fn property_row(
    p: &mut Painter<'_>,
    v: &View<'_>,
    index: usize,
    row: &TimelineRow,
    outline: usize,
    property: Property,
    axis: Option<motioner_core::Axis>,
) {
    let Some(row_rect) = v.timeline_layout.rows.get(index).cloned() else {
        return;
    };
    let rect = row_rect.rect;
    row_background(p, v, index, outline, rect);
    let (visible, locked) = v
        .layer_states
        .get(outline)
        .copied()
        .unwrap_or((true, false));
    let dim = !visible || locked;
    let label = channel_label(property, axis);
    let name = channel_name(property, axis);
    let value = v
        .document
        .and_then(|document| v.outline.get(outline).map(|row| (document, row.node)))
        .map_or_else(String::new, |(document, node)| {
            value_text(document, node, property, axis, v.frame)
        });
    draw_row_controls(p, v, index, row, &row_rect, &label, &value, dim);
    draw_row_keys(p, v, row, outline, &name, rect, dim);
}

#[allow(clippy::too_many_arguments)] // one row's controls, kept flat
fn draw_row_controls(
    p: &mut Painter<'_>,
    v: &View<'_>,
    index: usize,
    row: &TimelineRow,
    row_rect: &crate::timeline_model::RowRect,
    label_text: &str,
    value: &str,
    dim: bool,
) {
    let t = *p.theme;
    let rect = row_rect.rect;
    if let Some(stopwatch) = row_rect.stopwatch {
        let state = v.interaction(Hit::TimelineStopwatch(index));
        if state.hovered {
            p.fill_rounded(
                stopwatch.inset(-p.m.px(2.0)),
                t.control,
                p.m.px(size::RADIUS_SM),
            );
        }
        let ink = if row.animated {
            t.accent
        } else {
            t.text_disabled
        };
        icons::draw(
            p,
            Icon::Stopwatch,
            stopwatch.inset(p.m.px(1.0)),
            ink,
            t.panel,
        );
    }
    let style = p.style(
        type_scale::CAPTION,
        if dim { t.text_disabled } else { t.text_muted },
    );
    let label_right = row_rect.field.map_or(rect.right(), |f| f.x);
    let label = Rect::new(
        row_rect.name.x,
        rect.y,
        (label_right - row_rect.name.x - p.m.px(size::GRID)).max(0.0),
        rect.h,
    );
    p.text_in(label_text, label, Align::Left, &style);
    if let Some(field) = row_rect.field {
        p.fill_rounded(field, t.raised, p.m.px(size::RADIUS_SM));
        let style = p.style(
            type_scale::CAPTION,
            if dim { t.text_disabled } else { t.text_body },
        );
        p.text_in(
            value,
            field.inset_xy(p.m.px(size::GRID * 1.5), 0.0),
            Align::Left,
            &style,
        );
    }
    for (offset, hit) in [
        (0, Hit::TimelinePrevKey(index)),
        (1, Hit::TimelineKeyToggle(index)),
        (2, Hit::TimelineNextKey(index)),
    ] {
        let Some(button) = [row_rect.prev, row_rect.key, row_rect.next][offset] else {
            continue;
        };
        let state = v.interaction(hit);
        let ink = if state.hovered { t.text } else { t.text_muted };
        match offset {
            0 => icons::draw(p, Icon::StepBack, button.inset(p.m.px(3.0)), ink, t.panel),
            2 => icons::draw(
                p,
                Icon::StepForward,
                button.inset(p.m.px(3.0)),
                ink,
                t.panel,
            ),
            _ => {
                let marker = Rect::new(
                    button.center_x() - p.m.px(4.0),
                    button.center_y() - p.m.px(4.0),
                    p.m.px(8.0),
                    p.m.px(8.0),
                );
                let mark = if row.keys.contains(&v.frame) {
                    KeyMark::Key
                } else {
                    KeyMark::Empty
                };
                widgets::key_marker(p, marker, mark, false);
                if state.hovered {
                    p.diamond(marker.inset(-p.m.px(2.0)), ink, false);
                }
            }
        }
    }
}

fn draw_row_keys(
    p: &mut Painter<'_>,
    v: &View<'_>,
    row: &TimelineRow,
    outline: usize,
    name: &str,
    rect: Rect,
    dim: bool,
) {
    let tl = v.layout.timeline;
    let duration = v.duration.max(1);
    let scrub = tl.scrub;
    let x_of =
        |frame: i64| frame_to_x(frame, duration, f64::from(scrub.x), f64::from(scrub.w)) as f32;
    draw_easing(p, row, x_of, rect, tl.tracks, dim);
    let diamond = p.m.px(10.0);
    let id = layer_identity(v, outline);
    for frame in &row.keys {
        let marker = Rect::new(
            (x_of(*frame) - diamond * 0.5).round(),
            (rect.center_y() - diamond * 0.5).round(),
            diamond,
            diamond,
        );
        let selected = id.as_deref().is_some_and(|id| {
            v.selected_keys
                .iter()
                .any(|key| key.frame == *frame && key.property == name && key.id == id)
        });
        let mark = if selected {
            KeyMark::Selected
        } else {
            KeyMark::Key
        };
        widgets::key_marker(p, marker, mark, dim);
    }
}
fn content_row(
    p: &mut Painter<'_>,
    v: &View<'_>,
    index: usize,
    row: &TimelineRow,
    outline: usize,
    field: motioner_core::ContentField,
) {
    let Some(row_rect) = v.timeline_layout.rows.get(index).cloned() else {
        return;
    };
    let rect = row_rect.rect;
    row_background(p, v, index, outline, rect);
    let (visible, locked) = v
        .layer_states
        .get(outline)
        .copied()
        .unwrap_or((true, false));
    let dim = !visible || locked;
    let value = v
        .document
        .and_then(|document| v.outline.get(outline).map(|row| (document, row.node)))
        .map_or_else(String::new, |(document, node)| {
            content_text(document, node, field, v.frame)
        });
    draw_row_controls(p, v, index, row, &row_rect, field.name(), &value, dim);
    if field.is_animatable() {
        draw_row_keys(p, v, row, outline, field.name(), rect, dim);
    }
}

/// The inspector's display name for a transform property.
fn property_label(property: Property) -> &'static str {
    match property {
        Property::Position => "Position",
        Property::Scale => "Scale",
        Property::Rotation => "Rotation",
        Property::Anchor => "Anchor",
        Property::Opacity => "Opacity",
    }
}

fn value_text(
    document: &Document,
    node: NodeId,
    property: Property,
    axis: Option<motioner_core::Axis>,
    frame: Frame,
) -> String {
    let Some(node) = document.node(node) else {
        return String::new();
    };
    let t = node.transform();
    let scalar = |value: &motioner_core::Animatable<f64>| round(sample(value, frame));
    match (property, axis) {
        (Property::Position, Some(axis)) => scalar(t.position.axis(axis)),
        (Property::Scale, Some(axis)) => {
            format!("{}%", round(sample(t.scale.axis(axis), frame) * 100.0))
        }
        (Property::Anchor, Some(axis)) => scalar(t.anchor.axis(axis)),
        (Property::Position, None) => {
            let v = motioner_eval::sample_vec2(&t.position, frame);
            format!("{}, {}", round(v.x), round(v.y))
        }
        (Property::Scale, None) => {
            let v = motioner_eval::sample_vec2(&t.scale, frame);
            format!("{}%, {}%", round(v.x * 100.0), round(v.y * 100.0))
        }
        (Property::Anchor, None) => {
            let v = motioner_eval::sample_vec2(&t.anchor, frame);
            format!("{}, {}", round(v.x), round(v.y))
        }
        (Property::Rotation, _) => format!("{}\u{00b0}", scalar(&t.rotation)),
        (Property::Opacity, _) => format!("{}%", round(sample(&t.opacity, frame) * 100.0)),
    }
}

/// The timeline label of a channel: `Position X`, `Scale Y`, `Rotation`.
fn channel_label(property: Property, axis: Option<motioner_core::Axis>) -> String {
    match axis {
        Some(axis) => format!(
            "{} {}",
            property_label(property),
            axis.name().to_uppercase()
        ),
        None => property_label(property).to_owned(),
    }
}

fn content_text(
    document: &Document,
    node: NodeId,
    field: motioner_core::ContentField,
    frame: Frame,
) -> String {
    match field {
        motioner_core::ContentField::Fill | motioner_core::ContentField::Stroke => {
            match crate::editor::channels::sampled_colour(document, node, field, frame) {
                crate::editor::channels::SampledColour::Colour(colour) => hex(colour),
                crate::editor::channels::SampledColour::Absent => "none".to_owned(),
                crate::editor::channels::SampledColour::NotColour => "n/a".to_owned(),
            }
        }
        motioner_core::ContentField::StrokeWidth
        | motioner_core::ContentField::Width
        | motioner_core::ContentField::Height
        | motioner_core::ContentField::Radius
        | motioner_core::ContentField::Size => {
            crate::editor::channels::sampled_number(document, node, field, frame)
                .map_or_else(|| "n/a".to_owned(), round)
        }
        motioner_core::ContentField::Color => {
            match crate::editor::channels::sampled_colour(document, node, field, frame) {
                crate::editor::channels::SampledColour::Colour(colour) => hex(colour),
                _ => String::new(),
            }
        }
        motioner_core::ContentField::Text | motioner_core::ContentField::Align => {
            match document
                .node(node)
                .and_then(|node| node.content().static_value(field))
            {
                Some(motioner_core::ContentValue::Text(text)) => text,
                Some(motioner_core::ContentValue::Align(align)) => match align {
                    motioner_core::TextAlign::Left => "left",
                    motioner_core::TextAlign::Center => "center",
                    motioner_core::TextAlign::Right => "right",
                }
                .to_owned(),
                _ => "n/a".to_owned(),
            }
        }
    }
}

fn round(value: f64) -> String {
    let value = (value * 10.0).round() / 10.0;
    let value = if value == 0.0 { 0.0 } else { value };
    if value.fract().abs() < 1e-9 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

/// A solid line between consecutive keys whose rise follows the segment's
/// easing (progress 0 at the left key, 1 at the right one), so the shape of
/// the motion is readable on the row itself.
fn draw_easing(
    p: &mut Painter<'_>,
    row: &TimelineRow,
    x_of: impl Fn(i64) -> f32,
    rect: Rect,
    tracks: Rect,
    dim: bool,
) {
    if row.keys.len() < 2 {
        return;
    }
    let t = *p.theme;
    let ink = if dim {
        t.text_disabled
    } else {
        t.timeline_curve
    };
    let band = (rect.h * 0.5).round();
    let bottom = (rect.center_y() + band * 0.5).round();
    let w = p.m.px(1.5).max(1.0);
    let inset = p.m.px(7.0);
    for (index, pair) in row.keys.windows(2).enumerate() {
        let (from, to) = (pair[0], pair[1]);
        if to <= from {
            continue;
        }
        let ease = row
            .eases
            .get(index + 1)
            .copied()
            .unwrap_or(motioner_core::Ease::Linear);
        // Keep the line clear of the diamonds at both ends.
        let x0 = x_of(from) + inset;
        let x1 = x_of(to) - inset;
        if x1 <= x0 || x1 < tracks.x || x0 > tracks.right() {
            continue;
        }
        let steps = (x1 - x0).ceil().max(1.0) as i32;
        let mut previous: Option<f32> = None;
        for step in 0..=steps {
            let progress = f64::from(step) / f64::from(steps);
            let x = x0 + (x1 - x0) * progress as f32;
            let eased = (motioner_eval::ease(ease, progress) as f32).clamp(-0.25, 1.25);
            let y = bottom - band * eased;
            // Join each sample to the previous one so steep parts stay solid.
            let (top, low) = previous.map_or((y, y), |prev| (prev.min(y), prev.max(y)));
            p.fill(
                Rect::new(x - w * 0.5, top - w * 0.5, w, (low - top) + w),
                ink,
            );
            previous = Some(y);
        }
    }
}

fn playhead(p: &mut Painter<'_>, v: &View<'_>) {
    let t = *p.theme;
    let tl = v.layout.timeline;
    let duration = v.duration.max(1);
    let scrub = tl.scrub;
    let x = frame_to_x(v.frame, duration, f64::from(scrub.x), f64::from(scrub.w)).round() as f32;
    let area = Rect::new(tl.ruler.x, tl.ruler.y, tl.ruler.w, tl.ruler.h + tl.tracks.h);
    p.push_clip(area);
    let w = p.m.hairline().max(p.m.px(1.5).floor()).max(1.0);
    p.fill(
        Rect::new(x - (w * 0.5).floor(), tl.ruler.y, w, area.h),
        t.accent,
    );

    let (head, label) = head_rect(p, v);
    let style = p.style(type_scale::CAPTION, t.text).medium();
    p.fill_rounded(head, t.accent, p.m.px(size::RADIUS_SM));
    p.text_in(&label, head, Align::Center, &style);
    p.pop_clip();
}

/// The retime delta pill above the playhead while dragging keys.
fn retime_pill(p: &mut Painter<'_>, v: &View<'_>) {
    let Some(delta) = v.retime_delta else {
        return;
    };
    let t = *p.theme;
    let tl = v.layout.timeline;
    let style = p.style(type_scale::CAPTION, t.text).medium();
    let label = if delta > 0 {
        format!("+{delta}f")
    } else {
        format!("{delta}f")
    };
    let (head, _) = head_rect(p, v);
    let width = widgets::pill_width(p, &label, &style);
    let x = (head.center_x() - width * 0.5).round();
    let margin = p.m.px(size::GRID);
    let _ = tl;
    p.layer();
    widgets::pill(p, x, head.y - margin, &label, &style, t.accent_hover);
}

/// The playhead tab on the ruler and its frame label.
fn head_rect(p: &Painter<'_>, v: &View<'_>) -> (Rect, String) {
    let tl = v.layout.timeline;
    let duration = v.duration.max(1);
    let x = frame_to_x(
        v.frame,
        duration,
        f64::from(tl.scrub.x),
        f64::from(tl.scrub.w),
    )
    .round() as f32;
    let style = p.style(type_scale::CAPTION, p.theme.text).medium();
    let label = v.frame.to_string();
    let w = (p.measure(&label, &style) + p.m.px(size::GRID * 3.0)).round();
    let h = p.m.px(18.0);
    let head_x = (x - w * 0.5)
        .clamp(tl.ruler.x, tl.ruler.right() - w)
        .round();
    (
        Rect::new(head_x, (tl.ruler.center_y() - h * 0.5).round(), w, h),
        label,
    )
}
