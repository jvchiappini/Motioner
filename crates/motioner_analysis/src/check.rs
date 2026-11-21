//! `check_design`: one rule table over the exact document and frames.
//!
//! The checker combines the layout, motion and legibility reports into a
//! single issue list. Issues are aggregated by `(rule, nodes)`: one entry per
//! problem with the frames where it happens, so a 30-second composition does
//! not return thousands of near-identical rows.

use std::collections::BTreeMap;

use motioner_core::{Document, NodeId};
use motioner_eval::{Evaluator, RenderPlan};

use crate::element::frame_elements;
use crate::layout::{layout_report, LayoutReport};
use crate::legibility::{legibility_report, BackgroundSample};
use crate::motion::{motion_report_range, MotionReport};
use crate::rules::{Rules, Severity, RULES};
use crate::text::TextMeasure;

/// Limits that shape one `check_design` run.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CheckOptions {
    /// First frame; the composition start when `None`.
    pub start: Option<i64>,
    /// Last frame (inclusive); the composition end when `None`.
    pub end: Option<i64>,
    /// Thresholds.
    pub rules: Rules,
}

/// One reported problem.
#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    /// Rule id (an entry of [`RULES`]).
    pub rule: &'static str,
    /// How much it matters.
    pub severity: Severity,
    /// Nodes involved, sorted by id.
    pub nodes: Vec<NodeId>,
    /// Frames where it happens (at most [`MAX_FRAMES`] of them).
    pub frames: Vec<i64>,
    /// How many sampled frames it happened on.
    pub occurrences: usize,
    /// What was measured, exactly.
    pub measured: String,
    /// The threshold that was exceeded.
    pub threshold: String,
    /// What to change.
    pub fix: &'static str,
}

/// At most this many frames are listed per aggregated issue.
pub const MAX_FRAMES: usize = 8;

/// The result of one check.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckReport {
    /// Issues, sorted by severity (errors first) then rule.
    pub issues: Vec<Issue>,
    /// Errors.
    pub errors: usize,
    /// Warnings.
    pub warnings: usize,
    /// Informational issues.
    pub infos: usize,
    /// Frames sampled.
    pub frames_checked: usize,
    /// Frame step used for the frame-level checks.
    pub step: i64,
    /// The composition range checked.
    pub start: i64,
    /// The last frame checked.
    pub end: i64,
}

impl CheckReport {
    /// Whether the composition is acceptable (no errors).
    #[must_use]
    pub fn ok(&self) -> bool {
        self.errors == 0
    }
}

#[derive(Default)]
struct Collector {
    issues: BTreeMap<(&'static str, Vec<NodeId>), Issue>,
}

impl Collector {
    fn push(
        &mut self,
        rule: &'static str,
        nodes: Vec<NodeId>,
        frame: i64,
        measured: String,
        rules: &Rules,
    ) {
        let Some(spec) = Rules::spec(rule) else {
            return;
        };
        let key = (rule, nodes.clone());
        let entry = self.issues.entry(key).or_insert_with(|| Issue {
            rule,
            severity: spec.severity,
            nodes,
            frames: Vec::new(),
            occurrences: 0,
            measured: measured.clone(),
            threshold: rules.threshold_text(rule),
            fix: spec.fix,
        });
        entry.occurrences += 1;
        if entry.frames.len() < MAX_FRAMES {
            entry.frames.push(frame);
        }
        entry.measured = measured;
    }

    fn into_sorted(self) -> Vec<Issue> {
        let mut issues: Vec<Issue> = self.issues.into_values().collect();
        issues.sort_by(|a, b| {
            a.severity
                .cmp(&b.severity)
                .then(a.rule.cmp(b.rule))
                .then_with(|| {
                    a.nodes
                        .first()
                        .copied()
                        .map(NodeId::raw)
                        .cmp(&b.nodes.first().copied().map(NodeId::raw))
                })
        });
        issues
    }
}

fn node_ids(nodes: &[NodeId]) -> Vec<NodeId> {
    let mut nodes = nodes.to_vec();
    nodes.sort_by_key(|id| id.raw());
    nodes.dedup();
    nodes
}

fn frame_step(span: i64) -> i64 {
    (span / 240).max(1)
}

/// Runs every rule over `[start, end]`.
///
/// A `None` background sampler leaves contrast unmeasured instead of failing:
/// the text rules that do not need pixels still run.
#[allow(clippy::too_many_lines)] // one function per rule family, kept linear
#[must_use]
pub fn check_design(
    document: &Document,
    options: &CheckOptions,
    measure: &dyn TextMeasure,
    background: Option<&dyn BackgroundSample>,
) -> CheckReport {
    let rules = &options.rules;
    let duration = document.composition().duration().max(0);
    let default_start = 0;
    let default_end = duration.saturating_sub(1);
    let start = options
        .start
        .unwrap_or(default_start)
        .max(0)
        .min(default_end);
    let end = options
        .end
        .unwrap_or(default_end)
        .max(start)
        .min(default_end);
    let span = end - start + 1;
    let step = frame_step(span);
    let mut collector = Collector::default();

    // Frame-level checks: blank runs and layout.
    let mut evaluator = Evaluator::new();
    let mut plan = RenderPlan::new();
    let mut blank_frames: Vec<i64> = Vec::new();
    let mut first_content: Option<i64> = None;
    let mut last_content: Option<i64> = None;
    let mut frames_checked = 0_usize;
    let mut frame = start;
    while frame <= end {
        evaluator.evaluate(document, frame, &mut plan);
        let scene = frame_elements(document, &plan, frame, measure);
        frames_checked += 1;
        let visible = scene.elements.iter().any(|element| {
            element.opacity >= rules.min_visible_opacity && element.bounds.is_some()
        });
        if visible {
            first_content.get_or_insert(frame);
            last_content = Some(frame);
        } else {
            blank_frames.push(frame);
        }
        let layout = layout_report(&scene, rules);
        collect_layout(&mut collector, &layout, frame, rules);
        frame += step;
    }
    let interior_blank: Vec<i64> = blank_frames
        .iter()
        .copied()
        .filter(|frame| {
            first_content.is_none_or(|first| *frame > first)
                && last_content.is_none_or(|last| *frame < last)
        })
        .collect();
    if interior_blank.is_empty() {
        if first_content.is_none() {
            collector.push(
                "blank_frame",
                Vec::new(),
                start,
                format!("every sampled frame is blank ({frames_checked} sampled)"),
                rules,
            );
        }
    } else {
        for frame in &interior_blank {
            collector.push(
                "blank_frame",
                Vec::new(),
                *frame,
                "no visible element on this frame".to_owned(),
                rules,
            );
        }
    }

    // Motion, legibility and the frame-level checks above are one pass each.
    let motion = motion_report_range(document, start, end, None, measure);
    collect_motion(
        &mut collector,
        &motion,
        rules,
        document.composition().frame_rate().as_f64(),
    );

    let legibility = legibility_report(document, start, end, rules, measure, background);
    for text in &legibility.texts {
        if !text.holds_long_enough() {
            collector.push(
                "reading_time",
                vec![text.id],
                text.sample_frame,
                format!(
                    "{:.2} s on screen, {:.2} s needed for {} word(s)",
                    text.visible_seconds, text.required_seconds, text.words
                ),
                rules,
            );
        }
        if let Some(ratio) = text.contrast {
            if ratio + 1e-9 < text.required_contrast {
                collector.push(
                    "contrast",
                    vec![text.id],
                    text.sample_frame,
                    format!("{ratio:.1}:1 against the real background"),
                    rules,
                );
            }
        }
    }

    let issues = collector.into_sorted();
    let errors = issues
        .iter()
        .filter(|issue| issue.severity == Severity::Error)
        .count();
    let warnings = issues
        .iter()
        .filter(|issue| issue.severity == Severity::Warning)
        .count();
    let infos = issues
        .iter()
        .filter(|issue| issue.severity == Severity::Info)
        .count();
    CheckReport {
        issues,
        errors,
        warnings,
        infos,
        frames_checked,
        step,
        start,
        end,
    }
}

fn collect_layout(collector: &mut Collector, layout: &LayoutReport, frame: i64, rules: &Rules) {
    for overlap in &layout.text_overlaps {
        collector.push(
            "text_overlap",
            node_ids(&[overlap.a, overlap.b]),
            frame,
            format!("{:.0} % of the smaller text box", overlap.fraction * 100.0),
            rules,
        );
    }
    for overlap in &layout.busy_overlaps {
        collector.push(
            "text_on_busy",
            node_ids(&[overlap.text, overlap.busy]),
            frame,
            format!("{:.0} % of the text box", overlap.fraction * 100.0),
            rules,
        );
    }
    for violation in &layout.safe_violations {
        collector.push(
            "safe_area",
            vec![violation.id],
            frame,
            format!("{:.0} px outside title safe", violation.outside_pixels),
            rules,
        );
    }
    for off in &layout.off_screen {
        collector.push(
            "off_screen",
            vec![off.id],
            frame,
            "box does not intersect the canvas".to_owned(),
            rules,
        );
    }
    // Type hierarchy is a per-frame statement; report it once per size set.
    let texts = layout
        .type_levels
        .iter()
        .map(|level| level.count)
        .sum::<usize>();
    let same_size = layout
        .type_levels
        .first()
        .is_some_and(|level| level.count >= 3 && level.count == texts);
    if texts >= 2 && (same_size || layout.type_levels.len() > rules.max_type_levels) {
        collector.push(
            "type_hierarchy",
            Vec::new(),
            frame,
            if same_size {
                format!("{texts} texts share one size")
            } else {
                format!("{} distinct sizes", layout.type_levels.len())
            },
            rules,
        );
    }
    if layout.off_screen.is_empty() {
        // Quiet branch: nothing to add, the collector only keeps problems.
    }
}

fn collect_motion(collector: &mut Collector, motion: &MotionReport, rules: &Rules, fps: f64) {
    for element in &motion.elements {
        for jump in &element.jumps {
            if jump.delta > rules.max_velocity_jump {
                collector.push(
                    "velocity_jump",
                    vec![element.id],
                    jump.frame,
                    format!("{} changes by {:.1} px/frame", jump.property, jump.delta),
                    rules,
                );
            }
        }
        if element.peak_speed > rules.max_peak_speed {
            collector.push(
                "peak_speed",
                vec![element.id],
                element.peak_frame,
                format!("{:.1} px/frame", element.peak_speed),
                rules,
            );
        }
        let linear = element.eases.len() == 1 && element.eases.first() == Some(&"linear");
        if linear && element.keys > 1 && element.keys < rules.linear_ease_min_keys {
            collector.push(
                "linear_ease",
                vec![element.id],
                element.first_frame,
                format!("{} linear keys", element.keys),
                rules,
            );
        }
    }
    if motion.elements.len() >= 3
        && motion.stagger.span_frames < rules.min_stagger_span
        && motion.stagger.simultaneous
    {
        collector.push(
            "stagger",
            Vec::new(),
            motion
                .elements
                .first()
                .map_or(0, |element| element.first_frame),
            format!(
                "{} elements across {} onset frame(s)",
                motion.elements.len(),
                motion.stagger.distinct
            ),
            rules,
        );
    }
    #[allow(clippy::cast_precision_loss)]
    let dead_seconds = motion.dead_time_frames as f64 / fps.max(1.0);
    if dead_seconds > rules.max_dead_time {
        collector.push(
            "dead_time",
            Vec::new(),
            motion.dead_time_start,
            format!("{dead_seconds:.2} s without motion"),
            rules,
        );
    }
    if motion.overload_peak > rules.max_large_movers {
        for frame in &motion.overload_frames {
            collector.push(
                "overload",
                Vec::new(),
                *frame,
                format!("{} large movers at once", motion.overload_peak),
                rules,
            );
        }
    }
}

/// The ids of every rule, in report order.
#[must_use]
pub fn rule_ids() -> Vec<&'static str> {
    RULES.iter().map(|rule| rule.id).collect()
}
