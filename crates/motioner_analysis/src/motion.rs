//! `motion_report`: how elements move, exactly, from the curves (D-0040 §5).
//!
//! The report combines two sources: the authored keyframes (durations, eases,
//! velocity at keys) and the evaluated scene (what actually moves on screen,
//! including inherited motion from a parent group).

use std::collections::BTreeMap;

use motioner_core::{Axis, Document, Ease, NodeId, Property};
use motioner_eval::{ease, Evaluator, RenderPlan};

use crate::element::{frame_elements, ElementKind};
use crate::text::TextMeasure;

/// The speed at a key on both sides of it, in pixels per frame.
#[derive(Debug, Clone, PartialEq)]
pub struct VelocityJump {
    /// Channel name (`pos.x`, `pos.y`, `rotation`, `scale.x`, `opacity`).
    pub property: &'static str,
    /// The key frame where the speed changes.
    pub frame: i64,
    /// Speed at the end of the incoming segment.
    pub incoming: f64,
    /// Speed at the start of the outgoing segment.
    pub outgoing: f64,
    /// `|outgoing - incoming|`.
    pub delta: f64,
}

/// Per-element motion facts.
#[derive(Debug, Clone, PartialEq)]
pub struct ElementMotion {
    /// Node id.
    pub id: NodeId,
    /// Stable identity.
    pub identity: String,
    /// Display name.
    pub name: Option<String>,
    /// What it draws.
    pub kind: ElementKind,
    /// First sampled frame it is visible on.
    pub first_frame: i64,
    /// Last sampled frame it is visible on.
    pub last_frame: i64,
    /// Frames between the first and last key of any transform channel.
    pub duration_frames: i64,
    /// Peak speed in pixels per frame.
    pub peak_speed: f64,
    /// Peak speed relative to the element's largest box side.
    pub peak_relative: f64,
    /// Frame at which the peak happens.
    pub peak_frame: i64,
    /// Keys where the speed changes abruptly.
    pub jumps: Vec<VelocityJump>,
    /// Eases used by the element's keys, in key order (deduplicated).
    pub eases: Vec<&'static str>,
    /// Number of keys across the transform channels.
    pub keys: usize,
}

/// How the elements start relative to each other.
#[derive(Debug, Clone, PartialEq)]
pub struct Stagger {
    /// Onset frame per element, in id order.
    pub onsets: Vec<(NodeId, i64)>,
    /// Frames between the first and the last onset.
    pub span_frames: i64,
    /// Number of distinct onset frames.
    pub distinct: usize,
    /// Whether two or more elements share an onset frame.
    pub simultaneous: bool,
}

/// The whole motion report.
#[derive(Debug, Clone, PartialEq)]
pub struct MotionReport {
    /// Per-element facts, in id order.
    pub elements: Vec<ElementMotion>,
    /// Onset distribution.
    pub stagger: Stagger,
    /// Longest run of frames where nothing moves, after motion started.
    pub dead_time_frames: i64,
    /// First frame of that run.
    pub dead_time_start: i64,
    /// Most large movers seen on one frame.
    pub overload_peak: usize,
    /// Frames where more large movers moved at once than any other.
    pub overload_frames: Vec<i64>,
    /// Number of sampled frame gaps.
    pub frames_checked: usize,
    /// Frame step between samples.
    pub step: i64,
}

/// A mover counts as large when its largest side is at least this fraction of
/// the canvas' smaller side.
pub const LARGE_ELEMENT_FRACTION: f64 = 0.06;

/// Speed above which an element counts as moving, in pixels per frame.
pub const MOVING_SPEED: f64 = 0.05;

/// Speed above which a large mover counts towards overload, in pixels per
/// frame.
pub const FAST_SPEED: f64 = 4.0;

/// The transform channels the report inspects for velocity jumps.
const CHANNELS: [(&str, Property, usize); 6] = [
    ("pos.x", Property::Position, 0),
    ("pos.y", Property::Position, 1),
    ("scale.x", Property::Scale, 0),
    ("scale.y", Property::Scale, 1),
    ("rotation", Property::Rotation, 0),
    ("opacity", Property::Opacity, 0),
];

fn axis_of(index: usize) -> Axis {
    if index.is_multiple_of(2) {
        Axis::X
    } else {
        Axis::Y
    }
}

/// Accumulated motion facts of one node while the frames are sampled.
struct Track {
    first: Option<i64>,
    last: i64,
    peak_speed: f64,
    peak_relative: f64,
    peak_frame: i64,
    previous: Option<(i64, f64, f64, f64, f64)>,
    previous_opacity: f64,
}

fn channel_values(
    document: &Document,
    id: NodeId,
    property: Property,
    index: usize,
) -> Vec<(i64, f64)> {
    let Some(node) = document.node(id) else {
        return Vec::new();
    };
    let transform = node.transform();
    let axis = axis_of(index);
    match property {
        Property::Position => transform
            .position
            .axis(axis)
            .keys()
            .iter()
            .map(|key| (key.frame, key.value))
            .collect(),
        Property::Scale => transform
            .scale
            .axis(axis)
            .keys()
            .iter()
            .map(|key| (key.frame, key.value))
            .collect(),
        Property::Rotation => transform
            .rotation
            .keys()
            .iter()
            .map(|key| (key.frame, key.value))
            .collect(),
        Property::Opacity => transform
            .opacity
            .keys()
            .iter()
            .map(|key| (key.frame, key.value))
            .collect(),
        Property::Anchor => Vec::new(),
    }
}

fn channel_eases(document: &Document, id: NodeId, property: Property, index: usize) -> Vec<Ease> {
    let Some(node) = document.node(id) else {
        return Vec::new();
    };
    let transform = node.transform();
    let axis = axis_of(index);
    let keys = match property {
        Property::Position => transform.position.axis(axis).keys(),
        Property::Scale => transform.scale.axis(axis).keys(),
        Property::Rotation => transform.rotation.keys(),
        Property::Opacity => transform.opacity.keys(),
        Property::Anchor => return Vec::new(),
    };
    keys.iter().map(|key| key.ease).collect()
}

/// Numerical slope of an ease at one end of its segment.
fn derivative(curve: Ease, at_end: bool) -> f64 {
    const EPSILON: f64 = 1e-3;
    if at_end {
        (ease(curve, 1.0) - ease(curve, 1.0 - EPSILON)) / EPSILON
    } else {
        (ease(curve, EPSILON) - ease(curve, 0.0)) / EPSILON
    }
}

fn jumps_for(document: &Document, id: NodeId) -> Vec<VelocityJump> {
    let mut jumps = Vec::new();
    for (name, property, axis) in CHANNELS {
        let values = channel_values(document, id, property, axis);
        let eases = channel_eases(document, id, property, axis);
        if values.len() < 3 || eases.len() != values.len() {
            continue;
        }
        for key_index in 1..values.len() - 1 {
            let incoming_ease = eases[key_index];
            let outgoing_ease = eases[key_index + 1];
            if incoming_ease == Ease::Hold || outgoing_ease == Ease::Hold {
                continue;
            }
            let previous = values[key_index - 1];
            let current = values[key_index];
            let next = values[key_index + 1];
            #[allow(clippy::cast_precision_loss)]
            let incoming_span = (current.0 - previous.0) as f64;
            #[allow(clippy::cast_precision_loss)]
            let outgoing_span = (next.0 - current.0) as f64;
            if incoming_span <= 0.0 || outgoing_span <= 0.0 {
                continue;
            }
            let incoming =
                (current.1 - previous.1) / incoming_span * derivative(incoming_ease, true);
            let outgoing = (next.1 - current.1) / outgoing_span * derivative(outgoing_ease, false);
            let delta = (outgoing - incoming).abs();
            if delta.is_finite() {
                jumps.push(VelocityJump {
                    property: name,
                    frame: current.0,
                    incoming,
                    outgoing,
                    delta,
                });
            }
        }
    }
    jumps.sort_by(|a, b| a.frame.cmp(&b.frame).then(a.property.cmp(b.property)));
    jumps
}

fn key_count(document: &Document, id: NodeId) -> usize {
    CHANNELS
        .iter()
        .map(|(_, property, axis)| channel_values(document, id, *property, *axis).len())
        .sum()
}

fn all_eases(document: &Document, id: NodeId) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = Vec::new();
    for (_, property, axis) in CHANNELS {
        for value in channel_eases(document, id, property, axis) {
            let name = value.name();
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

fn key_extent(document: &Document, id: NodeId) -> (Option<i64>, Option<i64>) {
    let Some(node) = document.node(id) else {
        return (None, None);
    };
    let transform = node.transform();
    let mut frames: Vec<i64> = Vec::new();
    for axis in [Axis::X, Axis::Y] {
        for channel in [
            transform.position.axis(axis),
            transform.scale.axis(axis),
            transform.anchor.axis(axis),
        ] {
            frames.extend(channel.keys().iter().map(|key| key.frame));
        }
    }
    frames.extend(transform.rotation.keys().iter().map(|key| key.frame));
    frames.extend(transform.opacity.keys().iter().map(|key| key.frame));
    (frames.iter().copied().min(), frames.iter().copied().max())
}

/// Builds the motion report for `[start, end]` (inclusive), filtered by ids.
#[allow(clippy::too_many_lines)] // one pass over the sampled frames, kept linear
#[must_use]
pub fn motion_report_range(
    document: &Document,
    start: i64,
    end: i64,
    ids: Option<&[NodeId]>,
    measure: &dyn TextMeasure,
) -> MotionReport {
    let duration = document.composition().duration().max(0);
    let start = start.max(0).min(duration.saturating_sub(1));
    let end = end.max(start).min(duration.saturating_sub(1));
    let span = end - start + 1;
    let step = (span / 600).max(1);
    let mut evaluator = Evaluator::new();
    let mut plan = RenderPlan::new();

    let mut tracks: BTreeMap<NodeId, Track> = BTreeMap::new();
    let mut kinds: BTreeMap<NodeId, (ElementKind, String, Option<String>)> = BTreeMap::new();
    let mut overload_counts: Vec<(i64, usize)> = Vec::new();
    let mut moving: Vec<bool> = Vec::new();

    let mut frame = start;
    while frame <= end {
        evaluator.evaluate(document, frame, &mut plan);
        let scene = frame_elements(document, &plan, frame, measure);
        let large_threshold =
            scene.canvas.width().min(scene.canvas.height()) * LARGE_ELEMENT_FRACTION;
        let mut large_movers = 0_usize;
        let mut moved_this_frame = false;
        for element in &scene.elements {
            if element.kind == ElementKind::Group {
                continue;
            }
            if ids.is_some_and(|ids| !ids.contains(&element.id)) {
                continue;
            }
            let Some(bounds) = element.bounds else {
                continue;
            };
            let center = bounds.center();
            let size = bounds.width().max(bounds.height()).max(1.0);
            kinds
                .entry(element.id)
                .or_insert_with(|| (element.kind, element.identity.clone(), element.name.clone()));
            let track = tracks.entry(element.id).or_insert(Track {
                first: None,
                last: frame,
                peak_speed: 0.0,
                peak_relative: 0.0,
                peak_frame: frame,
                previous: None,
                previous_opacity: element.opacity,
            });
            track.first.get_or_insert(frame);
            track.last = frame;
            if let Some((previous_frame, previous_x, previous_y, previous_w, previous_h)) =
                track.previous
            {
                #[allow(clippy::cast_precision_loss)]
                let elapsed = (frame - previous_frame) as f64;
                // Movement is a change of the box: translation plus size (a
                // spin or a scale changes the axis-aligned box too).
                let translation =
                    ((center.x - previous_x).powi(2) + (center.y - previous_y).powi(2)).sqrt();
                let resized =
                    (bounds.width() - previous_w).abs() + (bounds.height() - previous_h).abs();
                let speed = (translation + resized) / elapsed.max(1.0);
                if speed > track.peak_speed {
                    track.peak_speed = speed;
                    track.peak_relative = speed / size;
                    track.peak_frame = frame;
                }
                if speed >= FAST_SPEED && size >= large_threshold {
                    large_movers += 1;
                }
                // A fade is motion for the audience even when no box moves.
                let faded = (element.opacity - track.previous_opacity).abs() > 0.01;
                if speed >= MOVING_SPEED || faded {
                    moved_this_frame = true;
                }
            }
            track.previous = Some((frame, center.x, center.y, bounds.width(), bounds.height()));
            track.previous_opacity = element.opacity;
        }
        moving.push(moved_this_frame);
        overload_counts.push((frame, large_movers));
        frame += step;
    }

    let mut elements: Vec<ElementMotion> = Vec::with_capacity(tracks.len());
    let mut onsets: Vec<(NodeId, i64)> = Vec::new();
    for (id, track) in tracks {
        let (kind, identity, name) =
            kinds
                .get(&id)
                .cloned()
                .unwrap_or((ElementKind::Shape, String::new(), None));
        let first = track.first.unwrap_or(start);
        onsets.push((id, first));
        let (first_key, last_key) = key_extent(document, id);
        let duration_frames = match (first_key, last_key) {
            (Some(first), Some(last)) => (last - first).max(0),
            _ => (track.last - first).max(0),
        };
        elements.push(ElementMotion {
            id,
            identity,
            name,
            kind,
            first_frame: first,
            last_frame: track.last,
            duration_frames,
            peak_speed: track.peak_speed,
            peak_relative: track.peak_relative,
            peak_frame: track.peak_frame,
            jumps: jumps_for(document, id),
            eases: all_eases(document, id),
            keys: key_count(document, id),
        });
    }

    let onset_values: Vec<i64> = onsets.iter().map(|(_, frame)| *frame).collect();
    let span_frames = onset_values
        .iter()
        .max()
        .zip(onset_values.iter().min())
        .map_or(0, |(last, first)| last - first);
    let mut distinct = onset_values.clone();
    distinct.sort_unstable();
    distinct.dedup();
    let max_shared = distinct
        .iter()
        .map(|candidate| {
            onset_values
                .iter()
                .filter(|frame| *frame == candidate)
                .count()
        })
        .max()
        .unwrap_or(0);

    let mut best_run = 0_usize;
    let mut best_start = 0_usize;
    let mut run = 0_usize;
    let mut run_start = 0_usize;
    for (index, is_moving) in moving.iter().enumerate() {
        if *is_moving {
            run = 0;
        } else {
            if run == 0 {
                run_start = index;
            }
            run += 1;
            if run > best_run {
                best_run = run;
                best_start = run_start;
            }
        }
    }
    #[allow(clippy::cast_possible_wrap)]
    let dead_time_frames = (best_run as i64) * step;
    #[allow(clippy::cast_possible_wrap)]
    let dead_time_start = start + (best_start as i64) * step;

    let overload_peak = overload_counts
        .iter()
        .map(|(_, count)| *count)
        .max()
        .unwrap_or(0);
    let overload_frames = overload_counts
        .iter()
        .filter(|(_, count)| *count == overload_peak && *count > 0)
        .map(|(frame, _)| *frame)
        .collect();

    MotionReport {
        elements,
        stagger: Stagger {
            onsets,
            span_frames,
            distinct: distinct.len(),
            simultaneous: max_shared > 1,
        },
        dead_time_frames,
        dead_time_start,
        overload_peak,
        overload_frames,
        frames_checked: moving.len(),
        step,
    }
}

/// [`motion_report_range`] over the whole composition.
#[must_use]
pub fn motion_report(
    document: &Document,
    ids: Option<&[NodeId]>,
    measure: &dyn TextMeasure,
) -> MotionReport {
    let duration = document.composition().duration();
    motion_report_range(document, 0, duration.saturating_sub(1), ids, measure)
}
