//! `bake`: folding the overrides into the program through `motioner_dsl`.

use std::time::Duration;

use super::{Editor, Effect, Outcome};
use crate::command::{CommandError, ErrorKind};
use crate::edit::UndoStack;

impl Editor {
    pub(super) fn bake(&mut self, ids: &[String]) -> Result<Outcome, CommandError> {
        if !self.session.writes_enabled() {
            return Ok(Outcome {
                summary: "bake is disabled while writes are turned off".to_owned(),
                effect: Effect::None,
            });
        }
        self.save_now()?;
        let source = self.session.source().to_owned();
        let overrides_path = self.session.overrides_path().to_owned();
        let program_path = self.session.path().to_owned();
        let overrides_text = match std::fs::read_to_string(&overrides_path) {
            Ok(text) => Some(text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(CommandError::new(ErrorKind::Io, error.to_string())),
        };
        let compiled = motioner_dsl::run_with_overrides(
            &source,
            overrides_text.as_deref(),
            &motioner_dsl::Limits::default(),
        )
        .map_err(|d| CommandError::new(ErrorKind::NoDocument, d.message))?;
        let outcome = motioner_dsl::bake(
            &source,
            &compiled.document,
            &compiled.bindings,
            &compiled.origins,
            &compiled.overrides,
            ids,
        );
        let rate = compiled.document.composition().frame_rate();
        let io = |error: std::io::Error| CommandError::new(ErrorKind::Io, error.to_string());
        if outcome.program != source {
            crate::write_atomic(&program_path, &outcome.program).map_err(io)?;
        }
        crate::write_atomic(&overrides_path, &outcome.overrides.to_mdf(rate)).map_err(io)?;
        // The program changed under the history: old snapshots would point
        // at overrides that are now in the program.
        self.undo = UndoStack::default_limit();
        self.reload(Duration::ZERO);
        let mut summary = Vec::new();
        for id in &outcome.baked {
            summary.push(format!("baked {id}"));
        }
        for refusal in &outcome.refused {
            summary.push(format!(
                "refused {} '{}': {}",
                refusal.id, refusal.property, refusal.reason
            ));
        }
        if summary.is_empty() {
            summary.push("nothing to bake".to_owned());
        }
        summary.push(format!("{} override(s) left", outcome.overrides.len()));
        Ok(Outcome {
            summary: summary.join("; "),
            effect: Effect::Reloaded,
        })
    }
}
