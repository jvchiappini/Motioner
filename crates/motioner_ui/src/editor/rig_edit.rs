//! Rig write-through entry point (prompt 16b E2).
//!
//! The DSL plans a byte-range edit, this module compiles the result, compares
//! the rig manifest with the exact expectation and only then installs the
//! program as one undo step. A mismatch is a refusal: the editor never keeps
//! a rig edit that does not reproduce the intended manifest.

use std::collections::HashSet;

use motioner_core::{Document, NodeContent, NodeId, OverrideSet, Transform};
use motioner_dsl::{apply_to_manifest, plan_rig_edit, Limits, Plan, RigEdit, Strategy};

use crate::command::{CommandError, ErrorKind};
use crate::editor::Editor;
use crate::Outcome;

type Shape = (Vec2Like, Vec<(NodeId, NodeContent, Transform, bool)>);

/// The composition fields a rig edit must never change.
#[derive(Debug, Clone, PartialEq)]
struct Vec2Like {
    name: String,
    width: u32,
    height: u32,
    rate: motioner_core::FrameRate,
    duration: motioner_core::Frame,
    background: motioner_core::Color,
}

impl Vec2Like {
    fn of(document: &Document) -> Self {
        let composition = document.composition();
        Self {
            name: composition.name().to_owned(),
            width: composition.width(),
            height: composition.height(),
            rate: composition.frame_rate(),
            duration: composition.duration(),
            background: composition.background(),
        }
    }
}

impl Editor {
    /// Plans, verifies and installs one rig edit as a single undo step.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::Refused`] when no exact code edit exists, the rig lives in
    /// a used module, the result does not compile, or the manifest differs
    /// from the expected one. [`ErrorKind::NoDocument`] when nothing loaded.
    pub fn apply_rig_edit(&mut self, edit: &RigEdit) -> Result<Outcome, CommandError> {
        if self.edit_mode == crate::EditMode::Overrides {
            return Err(CommandError::new(
                ErrorKind::Refused,
                "rig edits change the rig declarations, which overrides cannot express; \
                 switch the project to code mode",
            ));
        }
        let document = self.require_document()?.clone();
        let origins = self.session.origins().cloned().ok_or_else(|| {
            CommandError::new(ErrorKind::NoDocument, "the program has no recorded origins")
        })?;
        let source = self.session.source().to_owned();
        let code = match plan_rig_edit(&source, &origins, &document, edit) {
            Plan::Code(code) => code,
            Plan::Fallback(reason) => return Err(CommandError::new(ErrorKind::Refused, reason)),
        };
        let new_source = code
            .apply(&source)
            .map_err(|error| CommandError::new(ErrorKind::Refused, error.to_string()))?;
        let before = self.session.overrides().clone();
        let remaining = code.rekey_overrides(&before);
        let fps = document.composition().frame_rate().as_f64();
        let mut expected = document.rig_manifest().clone();
        apply_to_manifest(&mut expected, edit, fps)
            .map_err(|reason| CommandError::new(ErrorKind::Refused, reason))?;
        let compiled = match self.compile_preview(&new_source, &remaining) {
            Ok(compiled) => compiled,
            Err(message) => {
                return Err(CommandError::new(ErrorKind::BadValue, message));
            }
        };
        if compiled.document.rig_manifest() != &expected {
            return Err(CommandError::new(
                ErrorKind::Refused,
                "the rig edit did not reproduce the expected manifest",
            ));
        }
        if shape(&document) != shape(&compiled.document) {
            return Err(CommandError::new(
                ErrorKind::Refused,
                "the rig edit changed nodes outside the rig subtree",
            ));
        }
        self.undo.record(crate::edit::UndoState {
            source: source.clone(),
            overrides: before,
        });
        if !self
            .session
            .install_compiled(&new_source, &remaining, compiled)
        {
            return Err(self.compile_error());
        }
        self.last_code = Some((source, new_source));
        self.last_edit = Some(code);
        self.last_commit = Some(crate::editor::CommitSummary::code(vec![Strategy::RigEdit]));
        self.refresh_models();
        Ok(Outcome::done("rig edit written as code"))
    }

    fn compile_preview(
        &self,
        source: &str,
        set: &OverrideSet,
    ) -> Result<motioner_dsl::Compiled, String> {
        let rate = self.session.composition().map_or_else(
            || motioner_core::FrameRate::new(60, 1).expect("60 fps is valid"),
            motioner_core::Composition::frame_rate,
        );
        let overrides = set.to_mdf(rate);
        let argument = (!overrides.trim().is_empty()).then_some(overrides.as_str());
        motioner_dsl::run_with_overrides_and_metrics_and_assets(
            source,
            argument,
            &Limits::default(),
            self.session.text_metrics(),
            self.session.asset_resolver().as_ref(),
        )
        .map_err(|diagnostic| diagnostic.message)
    }
}

/// Composition settings plus every node outside the rig subtrees.
fn shape(document: &Document) -> Shape {
    let mut excluded: HashSet<NodeId> = HashSet::new();
    for rig in &document.rig_manifest().rigs {
        let mut roots: Vec<NodeId> = Vec::new();
        if let Some(root) = document.find(&rig.id) {
            roots.push(root);
        }
        for part in &rig.parts {
            if let Some(node) = document.find(&part.node) {
                roots.push(node);
            }
        }
        for root in roots {
            collect_subtree(document, root, &mut excluded);
        }
    }
    let nodes = document
        .nodes()
        .filter(|node| !excluded.contains(&node.id()))
        .map(|node| {
            (
                node.id(),
                node.content().clone(),
                node.transform().clone(),
                node.is_visible(),
            )
        })
        .collect();
    (Vec2Like::of(document), nodes)
}

fn collect_subtree(document: &Document, node: NodeId, out: &mut HashSet<NodeId>) {
    if !out.insert(node) {
        return;
    }
    if let Some(node) = document.node(node) {
        for child in node.children() {
            collect_subtree(document, *child, out);
        }
    }
}
