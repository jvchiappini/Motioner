//! Design and media analysis for Motioner (D-0049).
//!
//! Every function here is pure: it reads a [`motioner_core::Document`], an
//! evaluated [`motioner_eval::RenderPlan`] or decoded samples and returns
//! measurements. The crate never opens a file, never touches the GPU and
//! never depends on the editor, so `check_design` and the media analysers can
//! run in tests and in training without a window.
//!
//! Two capabilities are injected instead of implemented here:
//!
//! - text measurement ([`TextMeasure`]), because glyph advances come from the
//!   renderer's font set;
//! - background sampling ([`BackgroundSample`]), because contrast is measured
//!   against what is really behind a text, which only a render can tell.
//!
//! Thresholds live in one table ([`Rules`]) so the docs, the tests and the
//! agent see the same numbers.

mod check;
mod code;
mod describe;
mod element;
mod geom;
mod hit;
mod layout;
mod legibility;
mod media;
mod motion;
mod rules;
mod sample;
/// Text measurement injected by the renderer (D-0035).
pub mod text;

pub use check::{check_design, rule_ids, CheckOptions, CheckReport, Issue, MAX_FRAMES};
pub use code::{code_for_node, nodes_for_code, statement_span, CodeLocation};
pub use describe::{describe_frame, FrameDescription};
pub use element::{element_of, frame_elements, Element, ElementKind, SceneFrame, TextElement};
pub use geom::Box2;
pub use hit::{hit_test, Hit};
pub use layout::{
    layout_report, AlignmentGroup, BusyOverlap, LayoutReport, OffScreen, SafeViolation,
    TextOverlap, TypeLevel,
};
pub use legibility::{
    contrast_ratio, is_large_text, legibility_report, relative_luminance, BackgroundSample,
    LegibilityReport, TextLegibility,
};
pub use media::{
    analyze_audio, analyze_image, analyze_video, AudioAnalysis, EmptyRegion, GrayFrame,
    ImageAnalysis, MediaOptions, PaletteColor, RgbaImage, VideoAnalysis,
};
pub use motion::{
    motion_report, motion_report_range, ElementMotion, MotionReport, Stagger, VelocityJump,
    FAST_SPEED, LARGE_ELEMENT_FRACTION, MOVING_SPEED,
};
pub use rules::{RuleSpec, Rules, Severity, RULES};
pub use sample::{sample_property, sample_series};
pub use text::{text_local_box, LineExtent, TextMeasure, LINE_STEP_RATIO};
