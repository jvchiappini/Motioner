//! Timeline: transport strip, ruler, layer rows with visibility strips and
//! key summaries, and the expanded property sub-rows of the selected layer.

use std::fmt::Write as _;

use motioner_core::{Frame, FrameRate};

use crate::icons::{self, Icon};
use crate::layout::{Hit, Rect, TransportButton};
use crate::paint::Painter;
use crate::panels::View;
use crate::text::Align;
use crate::theme::{size, type_scale};
use crate::timeline::{frame_to_x, tick_spacing, TickUnit, MIN_TICK_SPACING_PX};
use crate::timeline_model::{RowKind, TimelineRow};
use crate::widgets::{self, ButtonKind, KeyMark};

/// Timeline header and ruler strings, rewritten in place every frame.
///
/// A scrub or a playback frame changes the timecode, the counter, the playhead
/// label and (rarely) the selected-key count. Caching them keeps the ordinary
/// draw path allocation-free after the first frame.
#[derive(Debug, Default)]
pub struct ChromeText {
    /// `HH:MM:SS:FF`.
    pub timecode: String,
    /// `frame / last`.
    pub counter: String,
    /// `seconds · frames`.
    pub info: String,
    /// Selected-key count, empty when nothing is selected.
    pub selected: String,
    /// Playhead ruler label.
    pub head: String,
    /// Ruler labels and their x coordinates.
    pub ruler: Vec<(f32, String)>,
    /// Composition-end label.
    pub end_label: String,
    /// Composition-end x coordinate.
    pub end_x: f32,
}

impl ChromeText {
    /// Empties every string, for a frame with no document.
    pub fn clear(&mut self) {
        self.timecode.clear();
        self.counter.clear();
        self.info.clear();
        self.selected.clear();
        self.head.clear();
        self.ruler.clear();
        self.end_label.clear();
        self.end_x = 0.0;
    }

    /// Rewrites the strings for `frame` in `layout`.
    pub fn prepare(
        &mut self,
        layout: &crate::layout::Layout,
        duration: i64,
        rate: FrameRate,
        frame: i64,
        selected_keys: usize,
    ) {
        crate::transport::write_timecode(&mut self.timecode, frame, rate);
        self.counter.clear();
        let _ = write!(&mut self.counter, "{} / {}", frame, duration.max(1) - 1);
        let seconds = duration as f64 * f64::from(rate.denominator()) / f64::from(rate.numerator());
        self.info.clear();
        let _ = write!(&mut self.info, "{seconds:.2} s \u{00b7} {duration} frames");
        self.selected.clear();
        if selected_keys == 1 {
            self.selected.push_str("1 key");
        } else if selected_keys > 1 {
            let _ = write!(&mut self.selected, "{selected_keys} keys");
        }
        self.head.clear();
        let _ = write!(&mut self.head, "{frame}");

        let t = layout.timeline;
        let spacing = tick_spacing(
            duration.max(1),
            rate,
            f64::from(t.scrub.w),
            MIN_TICK_SPACING_PX * f64::from(layout.metrics.scale),
        );
        let step = spacing.step_frames.max(1);
        let mut tick = 0_i64;
        let mut index = 0;
        while tick < duration {
            let x = frame_to_x(tick, duration, f64::from(t.scrub.x), f64::from(t.scrub.w)).round()
                as f32;
            if index == self.ruler.len() {
                self.ruler.push((0.0, String::new()));
            }
            let slot = &mut self.ruler[index];
            slot.0 = x;
            slot.1.clear();
            match spacing.unit {
                TickUnit::Frames => {
                    let _ = write!(&mut slot.1, "{tick}");
                }
                TickUnit::Seconds => {
                    let seconds =
                        tick as f64 * f64::from(rate.denominator()) / f64::from(rate.numerator());
                    let _ = write!(&mut slot.1, "{seconds:.0}s");
                }
            }
            index += 1;
            tick = tick.saturating_add(step);
        }
        self.ruler.truncate(index);
        let end = duration.saturating_sub(1);
        self.end_x =
            frame_to_x(end, duration, f64::from(t.scrub.x), f64::from(t.scrub.w)).round() as f32;
        let seconds = duration as f64 * f64::from(rate.denominator()) / f64::from(rate.numerator());
        self.end_label.clear();
        let _ = write!(&mut self.end_label, "{seconds:.0}s");
    }
}

/// Draws the timeline.
pub fn draw(p: &mut Painter<'_>, v: &View<'_>) {
    let t = *p.theme;
    let tl = v.layout.timeline;
    p.fill(tl.rect, t.panel);
    p.hline(tl.rect.x, tl.rect.right(), tl.rect.y, t.hairline);

    header(p, v);
    ruler(p, v);
    if v.graph.is_some() {
        graph(p, v);
    } else {
        rows(p, v);
    }
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
        v.chrome.timecode.as_str()
    } else {
        "--:--:--:--"
    };
    let tc_w = p.measure(tc, &tc_style);
    let tc_rect = Rect::new(header.x + pad, header.y, tc_w + 1.0, header.h);
    p.text_in(tc, tc_rect, Align::Left, &tc_style);
    let counter_style = p.style(type_scale::CAPTION, t.text_muted);
    let counter = if loaded { &v.chrome.counter } else { "" };
    let counter_rect = Rect::new(
        tc_rect.right() + p.m.px(size::GRID * 3.0),
        header.y,
        p.m.px(96.0),
        header.h,
    );
    p.text_in(counter, counter_rect, Align::Left, &counter_style);

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
    if !v.chrome.selected.is_empty() {
        let style = p.style(type_scale::CAPTION, t.text_muted);
        let label = v.chrome.selected.as_str();
        let w = p.measure(label, &style);
        let x = v.layout.transport[5].1.right() + p.m.px(size::PAD * 2.0);
        if x + w < header.right() - p.m.px(size::PAD * 4.0) {
            p.text_in(
                label,
                Rect::new(x, header.y, w, header.h),
                Align::Left,
                &style,
            );
        }
    }

    let info = v.chrome.info.as_str();
    let info_w = p.measure(info, &counter_style);
    let info_rect = Rect::new(
        header.right() - pad - info_w,
        header.y,
        info_w + 1.0,
        header.h,
    );
    let transport_end = v.layout.transport[5].1.right();
    if loaded && info_rect.x > transport_end + pad {
        p.text_in(info, info_rect, Align::Left, &counter_style);
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
    let head = v.document.is_some().then(|| head_rect(p, v));
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
    // Labels are centred on their tick so the mark and the number agree; the
    // strings and x positions are prepared once per frame.
    for (x, label) in &v.chrome.ruler {
        let w = p.measure(label, &label_style);
        let rect = Rect::new(
            (x - w * 0.5).round(),
            tl.ruler.y,
            w + 1.0,
            tl.ruler.h - major_h,
        );
        if head.is_some_and(|h| h.overlaps(rect)) {
            continue;
        }
        p.text_in(label, rect, Align::Center, &label_style);
    }
    // The composition end is explicit: its tick and label sit at the right
    // edge, where the last frame maps to.
    let end = v.chrome.end_x;
    p.vline(end, bottom - major_h, bottom, t.text_disabled);
    let end_label = v.chrome.end_label.as_str();
    let w = p.measure(end_label, &label_style);
    let rect = Rect::new(end - w, tl.ruler.y, w + 1.0, tl.ruler.h - major_h);
    if !head.is_some_and(|h| h.overlaps(rect)) {
        p.text_in(end_label, rect, Align::Right, &label_style);
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
            RowKind::Transform { outline, .. } => {
                property_row(p, v, row_rect.row, row, outline);
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

/// Draws the graph editor in the timeline's track area.
#[allow(clippy::too_many_lines)] // one plot: grid, curve, handles, keys
fn graph(p: &mut Painter<'_>, v: &View<'_>) {
    let t = *p.theme;
    let Some(model) = v.graph else {
        return;
    };
    let tl = v.layout.timeline;
    p.fill(tl.tracks, t.app);
    let plot = v.layout.graph_plot();
    if plot.is_empty() {
        return;
    }
    p.fill_rounded(plot, t.panel, p.m.px(size::RADIUS_SM));
    let geometry = model.geometry(plot);
    // The caption lives in its own strip above the plot, so it can never hide
    // a key or a handle; the string is cached by the graph model.
    let caption = p.style(type_scale::CAPTION, t.text_disabled);
    p.text_in(
        &model.caption,
        v.layout.graph_caption(),
        Align::Left,
        &caption,
    );
    p.push_clip(plot);
    // Two horizontal and four vertical grid lines.
    for step in 0..=4 {
        let x = plot.x + plot.w * (step as f32 / 4.0);
        p.vline(x.round(), plot.y, plot.bottom(), t.hairline);
    }
    for step in 0..=2 {
        let y = plot.y + plot.h * (step as f32 / 2.0);
        p.hline(plot.x, plot.right(), y.round(), t.hairline);
    }
    // The value curve is sampled once per screen column and drawn as real
    // anti-aliased segments, so named eases read truthfully and diagonals are
    // smooth. Sampling by column (not a fixed step count) keeps the line
    // correct at every zoom.
    let ink = t.timeline_curve;
    let w = p.m.px(1.5).max(1.0);
    let right = plot.right();
    for pair in model.keys.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        if to.frame <= from.frame {
            continue;
        }
        let start = crate::graph::to_screen(&geometry, from.frame as f64, from.value);
        let end = crate::graph::to_screen(&geometry, to.frame as f64, to.value);
        let point_at = |x: i32| -> (f32, f32) {
            let progress = if (end.0 - start.0).abs() < 1e-6 {
                0.0
            } else {
                (x as f32 - start.0) / (end.0 - start.0)
            };
            let progress = f64::from(progress.clamp(0.0, 1.0));
            #[allow(clippy::cast_precision_loss)] // frames are bounded by 2^31
            let span = (to.frame - from.frame) as f64;
            let frame = from.frame as f64 + progress * span;
            let value =
                from.value + motioner_eval::ease(to.ease, progress) * (to.value - from.value);
            (x as f32, crate::graph::to_screen(&geometry, frame, value).1)
        };
        let first = start.0.max(plot.x).ceil();
        let last = end.0.min(right).floor();
        #[allow(clippy::cast_possible_truncation)] // screen columns
        let (first, last) = (first as i32, last as i32);
        if last < first {
            // A segment shorter than one column still draws if it is on
            // screen; otherwise it contributes nothing.
            if start.0 >= plot.x && end.0 <= right {
                p.line(start, end, w, ink);
            }
            continue;
        }
        let mut previous = point_at(first);
        for x in (first + 1)..=last {
            let point = point_at(x);
            p.line(previous, point, w, ink);
            previous = point;
        }
    }
    // Handles first, then keys, so a key wins a shared position.
    for handle in model.handles(plot) {
        let segment = &model.keys[handle.segment..=handle.segment + 1];
        let from = crate::graph::to_screen(&geometry, segment[0].frame as f64, segment[0].value);
        let to = crate::graph::to_screen(&geometry, segment[1].frame as f64, segment[1].value);
        let anchor = match handle.kind {
            crate::graph::HandleKind::Out => from,
            crate::graph::HandleKind::In => to,
        };
        p.line(anchor, handle.at, p.m.hairline().max(1.0), t.border);
        let side = p.m.px(8.0);
        p.fill_rounded(
            Rect::new(
                handle.at.0 - side * 0.5,
                handle.at.1 - side * 0.5,
                side,
                side,
            ),
            t.accent,
            p.m.px(2.0),
        );
    }
    let diamond = p.m.px(10.0);
    let id = model.id.as_str();
    for point in model.points(plot) {
        let rect = Rect::new(
            point.at.0 - diamond * 0.5,
            point.at.1 - diamond * 0.5,
            diamond,
            diamond,
        );
        let selected = v.selected_keys.iter().any(|key| {
            key.id.as_str() == id
                && key.frame == model.keys[point.key].frame
                && key.property == model.property
        });
        widgets::key_marker(
            p,
            rect,
            if selected {
                KeyMark::Selected
            } else {
                KeyMark::Key
            },
            false,
        );
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
    let row_rect = &v.timeline_layout.rows[index];
    name_cell(p, v, row_rect, row, &outline_row.name, !visible || locked);

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
        let selected = id.is_some_and(|id| {
            v.selected_keys
                .iter()
                .any(|k| k.frame == key.frame && k.id.as_str() == id)
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
///
/// Borrowed from the document: a selected-key test must not allocate one
/// identity `String` per visible row on every frame.
fn layer_identity<'a>(v: &'a View<'_>, outline: usize) -> Option<&'a str> {
    let node = v.outline.get(outline)?.node;
    let document = v.document?;
    Some(document.node(node)?.identity().as_str())
}

fn property_row(
    p: &mut Painter<'_>,
    v: &View<'_>,
    index: usize,
    row: &TimelineRow,
    outline: usize,
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
    let value = v.timeline_values.get(index).map_or("", String::as_str);
    draw_row_controls(p, v, index, row, &row_rect, &row.label, value, dim);
    draw_row_keys(p, v, row, outline, &row.channel, rect, dim);
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
        let selected = id.is_some_and(|id| {
            v.selected_keys
                .iter()
                .any(|key| key.frame == *frame && key.property == name && key.id.as_str() == id)
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
    let value = v.timeline_values.get(index).map_or("", String::as_str);
    draw_row_controls(p, v, index, row, &row_rect, &row.label, value, dim);
    if field.is_animatable() {
        draw_row_keys(p, v, row, outline, &row.channel, rect, dim);
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

    let head = head_rect(p, v);
    let style = p.style(type_scale::CAPTION, t.text).medium();
    p.fill_rounded(head, t.accent, p.m.px(size::RADIUS_SM));
    p.text_in(&v.chrome.head, head, Align::Center, &style);
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
    let head = head_rect(p, v);
    let width = widgets::pill_width(p, &label, &style);
    let x = (head.center_x() - width * 0.5).round();
    let margin = p.m.px(size::GRID);
    let _ = tl;
    p.layer();
    widgets::pill(p, x, head.y - margin, &label, &style, t.accent_hover);
}

/// The playhead tab on the ruler and its frame label.
fn head_rect(p: &Painter<'_>, v: &View<'_>) -> Rect {
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
    let w = (p.measure(&v.chrome.head, &style) + p.m.px(size::GRID * 3.0)).round();
    let h = p.m.px(18.0);
    let head_x = (x - w * 0.5)
        .clamp(tl.ruler.x, tl.ruler.right() - w)
        .round();
    Rect::new(head_x, (tl.ruler.center_y() - h * 0.5).round(), w, h)
}
