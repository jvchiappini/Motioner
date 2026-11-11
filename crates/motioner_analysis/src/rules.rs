//! The `check_design` rule table (D-0040 §5) and its thresholds.
//!
//! Thresholds live here and nowhere else. `docs/design-rules.md` documents
//! every entry of [`RULES`] with a good and a bad example, and the tests that
//! exercise them read the same numbers.

use crate::motion::{FAST_SPEED, LARGE_ELEMENT_FRACTION, MOVING_SPEED};

/// How much a rule matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// The frame is broken: text is unreadable, off screen or overlapping.
    Error,
    /// The motion or layout looks wrong.
    Warning,
    /// A suggestion; the composition is still acceptable.
    Info,
}

impl Severity {
    /// Stable lower-case name used in tool results.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }
}

/// One documented rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleSpec {
    /// Stable id (`contrast`, `safe_area`, ...).
    pub id: &'static str,
    /// Short human title.
    pub title: &'static str,
    /// Default severity.
    pub severity: Severity,
    /// What is measured and why it matters.
    pub description: &'static str,
    /// What to change when it fires.
    pub fix: &'static str,
}

/// Every rule the checker implements, in report order.
pub const RULES: [RuleSpec; 14] = [
    RuleSpec {
        id: "blank_frame",
        title: "Blank frame",
        severity: Severity::Error,
        description: "A sampled frame draws no visible element.",
        fix: "Cover the composition: add content, a background, or start the scene earlier.",
    },
    RuleSpec {
        id: "off_screen",
        title: "Element off screen",
        severity: Severity::Error,
        description: "An element's box does not intersect the canvas.",
        fix: "Place the element inside the composition or animate it in from an edge.",
    },
    RuleSpec {
        id: "safe_area",
        title: "Text outside title safe",
        severity: Severity::Warning,
        description: "A text box leaves the inner 90 % of the canvas.",
        fix: "Keep text inside the title-safe rectangle (10 % margin).",
    },
    RuleSpec {
        id: "text_overlap",
        title: "Text overlaps text",
        severity: Severity::Error,
        description: "Two text boxes share more than 5 % of the smaller box.",
        fix: "Separate the lines with layout (row/column/stack) or move one in time.",
    },
    RuleSpec {
        id: "text_on_busy",
        title: "Text over busy content",
        severity: Severity::Warning,
        description: "Text sits over an image or a gradient, where contrast varies.",
        fix: "Move the text onto a surface, add a scrim, or place it beside the image.",
    },
    RuleSpec {
        id: "contrast",
        title: "Low contrast",
        severity: Severity::Error,
        description: "Text contrast against the real background is below WCAG AA.",
        fix: "Use a lighter or darker text token, a surface behind the text, or a scrim.",
    },
    RuleSpec {
        id: "reading_time",
        title: "Too little reading time",
        severity: Severity::Warning,
        description: "Text stays on screen less than 0.5 s + 0.3 s per word.",
        fix: "Hold the text longer, slow the exit, or shorten it.",
    },
    RuleSpec {
        id: "velocity_jump",
        title: "Velocity discontinuity",
        severity: Severity::Warning,
        description: "Speed changes abruptly at a key whose ease is not hold.",
        fix: "Match the eases on both sides of the key or use a single curve.",
    },
    RuleSpec {
        id: "peak_speed",
        title: "Too fast",
        severity: Severity::Warning,
        description: "An element moves faster than the speed budget.",
        fix: "Lengthen the move, reduce the distance, or lower the hold time.",
    },
    RuleSpec {
        id: "linear_ease",
        title: "Mechanical linear ease",
        severity: Severity::Info,
        description: "A short move uses linear easing where a curve reads better.",
        fix: "Use the theme's entrance/exit ease (quad_out/quad_in).",
    },
    RuleSpec {
        id: "stagger",
        title: "No stagger",
        severity: Severity::Info,
        description: "Three or more elements start on exactly the same frame.",
        fix: "Stagger the onsets by 2-6 frames with `stagger` or explicit `at`.",
    },
    RuleSpec {
        id: "dead_time",
        title: "Dead time",
        severity: Severity::Info,
        description: "Nothing moves for longer than the dead-time budget.",
        fix: "Break the hold with an emphasis, a cut, or a shorter scene.",
    },
    RuleSpec {
        id: "overload",
        title: "Too many large movers",
        severity: Severity::Warning,
        description: "More large elements move fast at once than the budget allows.",
        fix: "Stagger the moves or move fewer large elements at the same time.",
    },
    RuleSpec {
        id: "type_hierarchy",
        title: "Unclear type hierarchy",
        severity: Severity::Info,
        description: "Several texts share one size, or too many sizes compete.",
        fix: "Use two or three sizes from the theme's type scale.",
    },
];

/// Every tunable threshold.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rules {
    /// Title-safe fraction of the canvas (inner 90 %).
    pub title_safe: f64,
    /// Text/text overlap fraction that counts as a collision.
    pub text_overlap_fraction: f64,
    /// Text/busy overlap fraction that counts as a collision.
    pub text_busy_fraction: f64,
    /// WCAG AA contrast for normal text.
    pub min_contrast: f64,
    /// Em size at which text counts as large for contrast purposes.
    pub large_text_px: f64,
    /// WCAG AA contrast for large text.
    pub min_contrast_large: f64,
    /// Reading time floor, in seconds.
    pub reading_base: f64,
    /// Reading time per word, in seconds.
    pub reading_per_word: f64,
    /// Peak speed budget, in pixels per frame.
    pub max_peak_speed: f64,
    /// Velocity jump budget at a key, in pixels per frame.
    pub max_velocity_jump: f64,
    /// Dead-time budget, in seconds.
    pub max_dead_time: f64,
    /// Largest number of large movers at once.
    pub max_large_movers: usize,
    /// Minimum distinct-onset span for three or more elements, in frames.
    pub min_stagger_span: i64,
    /// Key count from which a linear channel is treated as a ticker loop.
    pub linear_ease_min_keys: usize,
    /// Pixel tolerance when grouping aligned boxes.
    pub alignment_tolerance: f64,
    /// Largest number of distinct type sizes that still reads as a hierarchy.
    pub max_type_levels: usize,
    /// Smallest box side that counts as a large mover, as a canvas fraction.
    pub large_element_fraction: f64,
    /// Speed that counts as movement, in pixels per frame.
    pub moving_speed: f64,
    /// Speed that counts towards overload, in pixels per frame.
    pub fast_speed: f64,
    /// Opacity below which an element is not visible for the checkers.
    pub min_visible_opacity: f64,
    /// Smallest box area (as a canvas fraction) that off-screen applies to.
    ///
    /// Decorative particles and elements scaled to zero are exempt; text is
    /// always checked.
    pub offscreen_min_area_fraction: f64,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            title_safe: 0.9,
            text_overlap_fraction: 0.05,
            text_busy_fraction: 0.15,
            min_contrast: 4.5,
            large_text_px: 24.0,
            min_contrast_large: 3.0,
            reading_base: 0.5,
            reading_per_word: 0.3,
            max_peak_speed: 64.0,
            max_velocity_jump: 10.0,
            max_dead_time: 2.0,
            max_large_movers: 6,
            min_stagger_span: 2,
            linear_ease_min_keys: 6,
            alignment_tolerance: 2.0,
            max_type_levels: 5,
            large_element_fraction: LARGE_ELEMENT_FRACTION,
            moving_speed: MOVING_SPEED,
            fast_speed: FAST_SPEED,
            min_visible_opacity: 0.02,
            offscreen_min_area_fraction: 0.005,
        }
    }
}

impl Rules {
    /// The default rule set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The documented spec of `id`.
    #[must_use]
    pub fn spec(id: &str) -> Option<&'static RuleSpec> {
        RULES.iter().find(|rule| rule.id == id)
    }

    /// The threshold text shown next to a measured value in reports.
    #[must_use]
    pub fn threshold_text(&self, id: &str) -> String {
        match id {
            "contrast" => format!(
                ">= {:.1}:1 (>= {:.1}:1 at {:.0} px+)",
                self.min_contrast, self.min_contrast_large, self.large_text_px
            ),
            "reading_time" => format!(
                ">= {:.1} s + {:.1} s/word",
                self.reading_base, self.reading_per_word
            ),
            "text_overlap" => format!(
                "<= {:.0} % of the smaller box",
                self.text_overlap_fraction * 100.0
            ),
            "text_on_busy" => format!(
                "<= {:.0} % of the text box",
                self.text_busy_fraction * 100.0
            ),
            "safe_area" => format!("inside the inner {:.0} %", self.title_safe * 100.0),
            "peak_speed" => format!("<= {:.0} px/frame", self.max_peak_speed),
            "velocity_jump" => format!("<= {:.0} px/frame", self.max_velocity_jump),
            "dead_time" => format!("<= {:.1} s", self.max_dead_time),
            "overload" => format!("<= {} large movers at once", self.max_large_movers),
            "stagger" => format!(">= {} frames between onsets", self.min_stagger_span),
            "type_hierarchy" => format!("<= {} sizes", self.max_type_levels),
            _ => String::new(),
        }
    }
}
