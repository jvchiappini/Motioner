//! Shared helpers: deterministic text metrics and a solid background sampler.

#![allow(dead_code)] // each test binary uses a different subset

use motioner_analysis::{Box2, LineExtent, TextMeasure};
use motioner_core::{Color, Document, TextBounds, TextMeasurement, TextMetrics, TextRequest};
use motioner_dsl::{run_with_metrics, Limits};

/// Fixed-width metrics: no font files, deterministic on every machine.
pub struct Metrics;

impl Metrics {
    fn width(text: &str, size: f64) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let count = text.chars().count() as f64;
        size * 0.5 * count
    }
}

impl TextMetrics for Metrics {
    fn measure(&self, request: &TextRequest<'_>) -> Option<TextMeasurement> {
        Some(TextMeasurement {
            width: Self::width(request.text, request.size),
            line_count: 1,
            height: request.size,
            ink: TextBounds {
                left: 0.0,
                top: -request.size * 0.8,
                right: request.size * 0.5,
                bottom: request.size * 0.2,
            },
            line_advances: vec![Self::width(request.text, request.size)],
        })
    }
}

impl TextMeasure for Metrics {
    fn line(&self, text: &str, size: f64) -> Option<LineExtent> {
        Some(LineExtent {
            advance: Self::width(text, size),
            ascender: size * 0.8,
            descender: size * 0.2,
        })
    }
}

/// A sampler that reports one colour everywhere.
pub struct SolidSampler(pub Color);

impl motioner_analysis::BackgroundSample for SolidSampler {
    fn behind(&self, _frame: i64, _rect: Box2) -> Option<Color> {
        Some(self.0)
    }
}

/// A sampler that never has pixels (contrast stays unmeasured).
pub struct NoSampler;

impl motioner_analysis::BackgroundSample for NoSampler {
    fn behind(&self, _frame: i64, _rect: Box2) -> Option<Color> {
        None
    }
}

/// Runs a full MDF program with the prelude and fixed metrics.
pub fn build(source: &str) -> Document {
    run_with_metrics(source, &Limits::default(), &Metrics)
        .unwrap_or_else(|error| panic!("{}", error.render(source, "test")))
}
