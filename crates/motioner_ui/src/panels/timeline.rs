//! Timeline: transport strip, ruler, layer rows with visibility strips and
//! key summaries, and the expanded property sub-rows of the selected layer.

use std::fmt::Write as _;

use motioner_core::{Frame, FrameRate};

use crate::icons::{self, Icon};
use crate::layout::{Hit, Rect, TransportButton};
use crate::paint::{Painter, PanelKey, PanelKind, PanelRevision};
use crate::panels::View;
use crate::text::Align;
use crate::theme::{size, type_scale};
use crate::timeline::{frame_to_x, tick_spacing, TickUnit, MIN_TICK_SPACING_PX};
use crate::timeline_model::{RowKind, TimelineRow};
use crate::widgets::{self, ButtonKind, KeyMark};

/// Everything the timeline reads, except the theme and DPI (in the key).
///
/// The transport frame and the strings that change with it are deliberately
/// absent: they live in [`PanelKind::TimelineLive`]. A scrub therefore hits
/// this panel and replays its rows instead of rebuilding the whole timeline.
#[allow(clippy::too_many_lines)] // one input per line: the whole key in one place
fn revision(v: &View<'_>) -> u64 {
    let mut rev = PanelRevision::new()
        .bool(v.document.is_some())
        .i64(v.duration)
        .u64(u64::from(v.rate.numerator()))
        .u64(u64::from(v.rate.denominator()))
        .bool(v.playing)
        .bool(v.looping)
        .u64(v.selected.map_or(u64::MAX, |index| index as u64))
        .u64(v.pressed.map_or(u64::MAX, Hit::stable_code))
        .usize(v.selected_keys.len())
        .usize(v.timeline_layout.rows.len())
        .f32(v.timeline_layout.height)
        .usize(v.expanded.len())
        .bool(v.graph.is_some())
        .rect(v.layout.timeline.names)
        .rect(v.layout.timeline.tracks)
        .rect(v.layout.timeline.ruler)
        .rect(v.layout.timeline.scrub);
    rev = rev.u64(selected_keys_revision(v));
    if let Some(graph) = v.graph {
        rev = rev
            .str(&graph.id)
            .str(&graph.property)
            .i64(graph.duration)
            .f64(graph.min)
            .f64(graph.max)
            .f64(graph.zoom_x)
            .f64(graph.zoom_y)
            .f64(graph.pan_x)
            .f64(graph.pan_y)
            .str(&graph.caption)
            .usize(graph.keys.len());
        for key in &graph.keys {
            rev = rev.i64(key.frame).f64(key.value).hashable(&key.ease);
        }
    }
    for &index in v.expanded {
        rev = rev.usize(index);
    }
    let assets_revision = assets_revision(v.assets);
    for row_rect in &v.timeline_layout.rows {
        rev = rev
            .usize(row_rect.row)
            .rect(row_rect.rect)
            .rect(row_rect.name);
        if let Some(rect) = row_rect.stopwatch {
            rev = rev.rect(rect);
        }
        if let Some(rect) = row_rect.field {
            rev = rev.rect(rect);
        }
        for rect in [row_rect.prev, row_rect.key, row_rect.next, row_rect.chevron]
            .into_iter()
            .flatten()
        {
            rev = rev.rect(rect);
        }
        let Some(row) = v.timeline.rows().get(row_rect.row) else {
            continue;
        };
        rev = row_revision(rev, v, row, row_rect.row, false);
        if let Some(value) = v.timeline_values.get(row_rect.row) {
            rev = rev.str(value);
        }
        rev = rev.u64(assets_revision);
    }
    rev.finish()
}

/// Visibility of one timeline row, without its rectangle (in the key).
///
/// `with_hover` is off for the enclosing panel: its key must not change when
/// the pointer moves between rows, or a hover would miss the whole timeline.
/// The row key itself always includes the pointer state.
fn row_revision(
    mut rev: PanelRevision,
    v: &View<'_>,
    row: &TimelineRow,
    row_index: usize,
    with_hover: bool,
) -> PanelRevision {
    let outline = row.kind.outline();
    let (visible, locked) = v
        .layer_states
        .get(outline)
        .copied()
        .unwrap_or((true, false));
    rev = rev
        .usize(outline)
        .u64(u64::from(row.kind.is_layer()))
        .usize(row.depth as usize)
        .bool(row.animated)
        .bool(row.on_key)
        .str(&row.label)
        .str(&row.channel)
        .usize(row.visible.len())
        .usize(row.summary.len())
        .usize(row.inherited.len())
        .usize(row.keys.len())
        .hashable(&row.keys)
        .hashable(&row.eases)
        .bool(v.selected == Some(outline))
        .bool(v.expanded.contains(&outline));
    if with_hover {
        rev = rev
            .u64(row_hit_code(v.hover, row_index))
            .u64(row_hit_code(v.pressed, row_index));
    }
    rev = rev
        .i64(v.duration)
        .u64(u64::from(v.rate.numerator()))
        .u64(u64::from(v.rate.denominator()))
        .u64(selected_keys_revision(v))
        .bool(visible)
        .bool(locked);
    if let Some(outline_row) = v.outline.get(outline) {
        rev = rev
            .str(&outline_row.name)
            .str(&outline_row.identity)
            .u64(outline_row.kind as u64);
    } else {
        rev = rev.bool(false);
    }
    rev = clip_revision(rev, v, row.node);
    // Layer rows do not change with the playhead; property rows show the
    // value at the playhead and the key toggle of that frame.
    if !row.kind.is_layer() {
        rev = rev.i64(v.frame);
    }
    for run in &row.visible {
        rev = rev.i64(run.from).i64(run.to);
    }
    for key in &row.summary {
        rev = rev.i64(key.frame).u64(u64::from(key.channels));
    }
    for frame in &row.inherited {
        rev = rev.i64(*frame);
    }
    rev
}

/// The cache key of one timeline row.
fn row_key(
    p: &Painter<'_>,
    v: &View<'_>,
    row_rect: &crate::timeline_model::RowRect,
    row: &TimelineRow,
    assets: u64,
) -> PanelKey {
    let mut rev = row_revision(PanelRevision::new(), v, row, row_rect.row, true)
        .usize(row_rect.row)
        .rect(row_rect.rect)
        .rect(row_rect.name)
        .rect(v.layout.timeline.rect)
        .rect(v.layout.timeline.scrub);
    if let Some(rect) = row_rect.stopwatch {
        rev = rev.rect(rect);
    }
    if let Some(rect) = row_rect.field {
        rev = rev.rect(rect);
    }
    for rect in [row_rect.prev, row_rect.key, row_rect.next, row_rect.chevron]
        .into_iter()
        .flatten()
    {
        rev = rev.rect(rect);
    }
    if let Some(value) = v.timeline_values.get(row_rect.row) {
        rev = rev.str(value);
    }
    rev = rev.u64(assets);
    p.panel_key(PanelKind::Row, rev.finish(), row_rect.rect)
}

/// Hash of the audio/video values the clip strips read from the document.
fn clip_revision(rev: PanelRevision, v: &View<'_>, node: motioner_core::NodeId) -> PanelRevision {
    let Some(document) = v.document else {
        return rev.bool(false);
    };
    let Some(content) = document.node(node).map(motioner_core::Node::content) else {
        return rev.bool(false);
    };
    match content {
        motioner_core::NodeContent::Audio(audio) => rev
            .bool(true)
            .str(&audio.source)
            .f64(audio.in_point.value())
            .f64(audio.out_point.value())
            .f64(audio.gain.value())
            .f64(audio.pan.value())
            .i64(audio.start)
            .f64(audio.fade_in)
            .f64(audio.fade_out),
        motioner_core::NodeContent::Video(video) => rev
            .bool(true)
            .str(&video.source)
            .f64(video.in_point.value())
            .f64(video.out_point.value())
            .f64(video.gain.value())
            .i64(video.start)
            .f64(video.speed)
            .f64(video.width.value())
            .f64(video.height.value())
            .bool(video.asset.is_some()),
        _ => rev.bool(false),
    }
}

/// The stable code of a hit when it belongs to timeline row `index`.
///
/// A row must rebuild when the pointer moves between its sub-widgets (body,
/// stopwatch, key buttons, field, diamond, chevron), not only when it enters
/// another row.
fn row_hit_code(hit: Option<Hit>, index: usize) -> u64 {
    match hit {
        Some(
            hit @ (Hit::TimelineRow(row)
            | Hit::TimelineStopwatch(row)
            | Hit::TimelinePrevKey(row)
            | Hit::TimelineKeyToggle(row)
            | Hit::TimelineNextKey(row)
            | Hit::TimelineField(row)
            | Hit::TimelineDiamond { row, .. }
            | Hit::LayerChevron(row)),
        ) if row == index => Hit::stable_code(hit),
        _ => u64::MAX,
    }
}

/// Hash of the selected keys, so a selection change repaints their diamonds.
fn selected_keys_revision(v: &View<'_>) -> u64 {
    let mut rev = PanelRevision::new().usize(v.selected_keys.len());
    for key in v.selected_keys {
        rev = rev.str(&key.id).str(&key.property).i64(key.frame);
    }
    rev.finish()
}

/// Hash of the asset summaries the audio/video strips read.
fn assets_revision(assets: &crate::AssetsModel) -> u64 {
    let mut rev = PanelRevision::new()
        .usize(assets.entries.len())
        .usize(assets.entries.as_ptr() as usize)
        .u64(assets.view() as u64)
        .u64(assets.selected.map_or(u64::MAX, |index| index as u64));
    for entry in &assets.entries {
        rev = rev
            .str(&entry.key)
            .str(&entry.original)
            .u64(entry.kind as u64)
            .u64(entry.size_bytes)
            .f64(entry.duration.unwrap_or(-1.0))
            .usize(entry.used_by)
            .usize(entry.peaks.len())
            .usize(entry.peak_levels.len())
            .usize(entry.thumbs.len())
            .usize(entry.thumbs.as_ptr() as usize);
        for peak in &entry.peaks {
            rev = rev.f32(*peak);
        }
        for level in &entry.peak_levels {
            rev = rev.usize(level.len());
            for peak in level {
                rev = rev.f32(*peak);
            }
        }
        for thumb in &entry.thumbs {
            rev = rev
                .u64(u64::from(thumb.width))
                .u64(u64::from(thumb.height))
                .f64(thumb.seconds)
                .usize(thumb.rgba8.len())
                .usize(thumb.rgba8.as_ptr() as usize);
        }
    }
    rev.finish()
}

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
    let key = p.panel_key(PanelKind::Timeline, revision(v), v.layout.timeline.rect);
    p.panel_hover(key, v.hover_key(PanelKind::Timeline), |p| draw_body(p, v));
    draw_live(p, v);
}

/// The header readouts, ruler labels and playhead: the parts that follow the
/// transport frame.
///
/// Each range is its own small cache entry so a scrub rebuilds a thin playhead
/// line and a few labels while the main timeline (its rows included) hits.
fn draw_live(p: &mut Painter<'_>, v: &View<'_>) {
    let tl = v.layout.timeline;
    // The live keys change on every scrub frame, so their rectangles are kept
    // tight: a stale entry damages its own key rect, never the whole timeline.
    let live_band = p.m.px(28.0);
    let header_rect = Rect::new(
        tl.header.x,
        (tl.header.center_y() - live_band * 0.5).round(),
        tl.header.w,
        live_band,
    );
    let header_key = p.panel_key(
        PanelKind::TimelineLive,
        header_live_revision(v),
        header_rect,
    );
    p.panel(header_key, |p| header_live(p, v));
    if v.document.is_none() {
        return;
    }
    let duration = v.duration.max(1);
    let x = frame_to_x(
        v.frame,
        duration,
        f64::from(tl.scrub.x),
        f64::from(tl.scrub.w),
    )
    .round() as f32;
    let line = p.m.hairline().max(p.m.px(1.5).floor()).max(1.0);
    let playhead_rect = Rect::new(
        (x - line).floor(),
        tl.ruler.y,
        (line * 2.0).max(2.0),
        tl.ruler.h + tl.tracks.h,
    );
    let playhead_key = p.panel_key(PanelKind::TimelineLive, playhead_revision(v), playhead_rect);
    p.panel(playhead_key, |p| playhead_line(p, v));
    let ruler_key = p.panel_key(PanelKind::TimelineLive, ruler_live_revision(v), tl.ruler);
    p.panel(ruler_key, |p| ruler_live(p, v));
    if v.retime_delta.is_some() {
        let pill_rect = Rect::new(
            tl.ruler.x,
            tl.ruler.y - p.m.px(24.0),
            tl.ruler.w,
            tl.ruler.h + p.m.px(24.0),
        );
        let pill_key = p.panel_key(PanelKind::TimelineLive, retime_revision(v), pill_rect);
        p.panel(pill_key, |p| retime_pill(p, v));
    }
}

/// The frame-dependent header strings.
fn header_live_revision(v: &View<'_>) -> u64 {
    PanelRevision::new()
        .bool(v.document.is_some())
        .str(&v.chrome.timecode)
        .str(&v.chrome.counter)
        .str(&v.chrome.info)
        .str(&v.chrome.selected)
        .finish()
}

/// The ruler labels and the playhead tab, which the head can hide.
fn ruler_live_revision(v: &View<'_>) -> u64 {
    let mut rev = PanelRevision::new()
        .bool(v.document.is_some())
        .i64(v.frame)
        .i64(v.duration)
        .str(&v.chrome.head)
        .str(&v.chrome.end_label)
        .f32(v.chrome.end_x)
        .rect(v.layout.timeline.ruler);
    for (x, label) in &v.chrome.ruler {
        rev = rev.f32(*x).str(label);
    }
    rev.finish()
}

/// The playhead line, which spans the ruler and the tracks.
fn playhead_revision(v: &View<'_>) -> u64 {
    PanelRevision::new()
        .bool(v.document.is_some())
        .i64(v.frame)
        .i64(v.duration)
        .rect(v.layout.timeline.scrub)
        .finish()
}

/// The retime delta pill above the playhead.
fn retime_revision(v: &View<'_>) -> u64 {
    PanelRevision::new()
        .bool(v.document.is_some())
        .i64(v.frame)
        .i64(v.duration)
        .bool(v.retime_delta.is_some())
        .u64(v.retime_delta.map_or(0, |delta| delta as u64))
        .str(&v.chrome.head)
        .finish()
}

fn draw_body(p: &mut Painter<'_>, v: &View<'_>) {
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
}

/// The transport strip: its separator and the buttons.
///
/// The timecode and counter are painted by [`header_live`], so a scrub does not
/// rebuild the transport.
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
}

/// The frame-dependent header readouts: timecode, counter, selected-key count
/// and duration.
fn header_live(p: &mut Painter<'_>, v: &View<'_>) {
    let t = *p.theme;
    let tl = v.layout.timeline;
    let header = tl.header;
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

/// The ruler background, the corner caption and the tick marks.
///
/// Labels, the composition-end label and the playhead tab follow the transport
/// frame; [`ruler_live`] paints them.
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
    p.push_clip(tl.ruler);
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
    // The composition end is explicit: its tick sits at the right edge, where
    // the last frame maps to. Its label is frame-adjacent (the head can cover
    // it) and lives in [`ruler_live`].
    let end = v.chrome.end_x;
    p.vline(end, bottom - major_h, bottom, t.text_disabled);
    p.pop_clip();
}

/// The ruler labels, the composition-end label and the playhead tab.
fn ruler_live(p: &mut Painter<'_>, v: &View<'_>) {
    let t = *p.theme;
    let tl = v.layout.timeline;
    if v.document.is_none() {
        return;
    }
    let head = head_rect(p, v);
    let major_h = p.m.px(8.0);
    p.push_clip(tl.ruler);
    let label_style = p.style(type_scale::CAPTION, t.text_muted);
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
        if head.overlaps(rect) {
            continue;
        }
        p.text_in(label, rect, Align::Center, &label_style);
    }
    let end = v.chrome.end_x;
    let end_label = v.chrome.end_label.as_str();
    let w = p.measure(end_label, &label_style);
    let rect = Rect::new(end - w, tl.ruler.y, w + 1.0, tl.ruler.h - major_h);
    if !head.overlaps(rect) {
        p.text_in(end_label, rect, Align::Right, &label_style);
    }
    // The tab is drawn last so it covers any label the head sits on, matching
    // the old single-panel order.
    let style = p.style(type_scale::CAPTION, t.text).medium();
    p.fill_rounded(head, t.accent, p.m.px(size::RADIUS_SM));
    p.text_in(&v.chrome.head, head, Align::Center, &style);
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
    let assets = assets_revision(v.assets);
    for row_rect in &v.timeline_layout.rows {
        if row_rect.rect.bottom() < area.y || row_rect.rect.y > area.bottom() {
            continue;
        }
        let Some(row) = v.timeline.rows().get(row_rect.row) else {
            continue;
        };
        let key = row_key(p, v, row_rect, row, assets);
        p.row(key, |p| match row.kind {
            RowKind::Layer { outline } => track_row(p, v, row_rect.row, row, outline),
            RowKind::Transform { outline, .. } => {
                property_row(p, v, row_rect.row, row, outline);
            }
            RowKind::Content { outline, field } => {
                content_row(p, v, row_rect.row, row, outline, field);
            }
        });
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
                icons::for_kind(outline.kind),
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
    if outline_row.kind == crate::outline::NodeKind::Audio {
        audio_strip(p, v, outline_row, full, bar_y, bar_h, &x_of);
    } else if outline_row.kind == crate::outline::NodeKind::Video {
        video_strip(p, v, outline_row, bar_y, bar_h, &x_of);
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

/// Draws the waveform of an audio layer inside its clip bar.
#[allow(clippy::too_many_arguments)]
fn audio_strip(
    p: &mut Painter<'_>,
    v: &View<'_>,
    outline_row: &crate::OutlineRow,
    full: Rect,
    bar_y: f32,
    bar_h: f32,
    x_of: &impl Fn(i64) -> f32,
) {
    let t = *p.theme;
    let Some(audio) = v
        .document
        .and_then(|document| document.node(outline_row.node))
        .and_then(|node| match node.content() {
            motioner_core::NodeContent::Audio(audio) => Some(audio),
            _ => None,
        })
    else {
        return;
    };
    let entry = v
        .assets
        .entries
        .iter()
        .find(|entry| entry.original == audio.source || entry.key == audio.source);
    let range = crate::timeline_model::audio_clip_range(
        audio,
        v.rate,
        entry.and_then(|entry| entry.duration),
        v.duration,
    );
    let from = x_of(range.start).max(full.x).round();
    let to = x_of(range.end).min(full.right()).round();
    if to <= from {
        return;
    }
    let strip = Rect::new(from, bar_y, to - from, bar_h);
    let Some(entry) = entry else {
        return;
    };
    let Some(duration) = entry.duration.filter(|duration| *duration > 0.0) else {
        return;
    };
    if entry.peak_levels.is_empty() || strip.w <= 0.0 {
        return;
    }
    let columns = strip.w.max(1.0) as usize;
    let peaks = crate::assets::level_for_columns(&entry.peak_levels, columns);
    if peaks.is_empty() {
        return;
    }
    // The clip bar is already accent-tinted; a second accent layer was
    // invisible. The envelope is a light ink over that bar.
    let mut colour = t.text;
    colour[3] = 0.72;
    let centre = strip.center_y();
    let fps = v.rate.as_f64();
    let in_point = audio.in_point.value().max(0.0);
    let span = (range.end - range.start).max(1) as f64;
    let mut x = strip.x;
    while x < strip.right() {
        let t0 = (f64::from(x - strip.x) / f64::from(strip.w)).clamp(0.0, 1.0);
        let t1 = (f64::from(x + 1.0 - strip.x) / f64::from(strip.w)).clamp(0.0, 1.0);
        let seconds0 = in_point + t0 * span / fps;
        let seconds1 = in_point + t1 * span / fps;
        let index0 = (seconds0 / duration * peaks.len() as f64).floor().max(0.0) as usize;
        let index1 = (seconds1 / duration * peaks.len() as f64).ceil() as usize;
        let peak = peaks[index0.min(peaks.len() - 1)..index1.clamp(index0 + 1, peaks.len())]
            .iter()
            .fold(0.0_f32, |peak, value| peak.max(*value));
        // Silence is empty, not a dimmer clip: zero peaks draw nothing.
        if peak > 0.0 {
            let height = strip.h * peak.clamp(0.0, 1.0);
            p.fill(
                Rect::new(x, centre - height * 0.5, 1.0, height.max(1.0)),
                colour,
            );
        }
        x += 1.0;
    }
    p.hline(strip.x, strip.right(), centre.round(), t.hairline);
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

/// Draws the thumbnail strip of a video layer inside its clip bar.
fn video_strip(
    p: &mut Painter<'_>,
    v: &View<'_>,
    outline_row: &crate::OutlineRow,
    bar_y: f32,
    bar_h: f32,
    x_of: &impl Fn(i64) -> f32,
) {
    let t = *p.theme;
    let Some(video) = v
        .document
        .and_then(|document| document.node(outline_row.node))
        .and_then(|node| match node.content() {
            motioner_core::NodeContent::Video(video) => Some(video),
            _ => None,
        })
    else {
        return;
    };
    let entry = v
        .assets
        .entries
        .iter()
        .find(|entry| entry.original == video.source || entry.key == video.source);
    let Some(entry) = entry else {
        return;
    };
    let range = crate::timeline_model::video_clip_range(video, v.rate, entry.duration, v.duration);
    let from = x_of(range.start).round();
    let to = x_of(range.end).round();
    if to <= from || entry.thumbs.is_empty() {
        return;
    }
    let strip = Rect::new(from, bar_y, to - from, bar_h);
    let in_point = video.in_point.value().max(0.0);
    let out_point = video.out_point.value();
    let duration = entry
        .duration
        .filter(|duration| *duration > 0.0)
        .unwrap_or(1.0);
    let end = if out_point > in_point {
        out_point
    } else {
        duration
    };
    let span = (end - in_point).max(0.001);
    #[allow(clippy::cast_possible_truncation)]
    let thumb_w = strip.w * (span / duration) as f32 / entry.thumbs.len() as f32;
    p.push_clip(strip);
    for (index, thumb) in entry.thumbs.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let seconds = duration * (index as f64 + 0.5) / entry.thumbs.len() as f64;
        if seconds < in_point || seconds > end {
            continue;
        }
        let x = strip.x + strip.w * ((seconds - in_point) / span) as f32 - thumb_w * 0.5;
        p.image_grid(
            Rect::new(x, strip.y, thumb_w.max(1.0), strip.h),
            thumb.width,
            thumb.height,
            &thumb.rgba8,
            12,
            6,
            0.9,
        );
    }
    p.pop_clip();
    p.stroke_rounded(strip, t.accent, p.m.px(3.0), p.m.hairline());
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

/// The playhead line across the ruler and the tracks.
///
/// The head tab is painted by [`ruler_live`] (above the ruler clip) so it can
/// cover the labels the head sits on.
fn playhead_line(p: &mut Painter<'_>, v: &View<'_>) {
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
