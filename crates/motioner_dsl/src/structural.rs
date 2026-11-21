//! Structural edits as MDF code (prompt 12 phase 5, D-0036 section 3).
//!
//! Structural UI gestures write ordinary MDF statements instead of override
//! entries: adding, deleting, duplicating, renaming, reordering and parenting
//! layers, placing assets, and maintaining keyframes (add, move, remove, ease,
//! stopwatch). Every plan is verified by re-running the program; when a
//! statement is shared, the edit cannot be represented exactly and the planner
//! returns [`Plan::Fallback`] with a reason instead of guessing.
//!
//! Loops and functions reuse `crate::loop_guard`: a key removed from one loop
//! instance excludes that iteration with the same guard the value planner
//! builds, and deleting one loop instance writes `if <guard>: continue` as the
//! first statement of the body. Inserted snippets go through
//! [`crate::printer::format_snippet`] so phase 7 can give them the canonical
//! style without changing this module.

use std::collections::{BTreeSet, HashSet};

use motioner_core::{
    format_number, quote, seconds_of, Axis, Document, Ease, EditCommand, Frame, NodeContent,
    NodeId, OverrideAction, OverrideEntry, OverrideSet, OverrideValue, NAME,
};

use crate::ast::{Expr, ExprKind, Program, Stmt, StmtKind, Target};
use crate::code_edit::{CodeEdit, Replacement, Strategy};
use crate::origins::{NodeOrigin, ValueOrigin, ValueOrigins};
use crate::plan::{self, Plan, Scope};
use crate::write_text::set_value_text;

/// The layer kinds the editor can add or place from the Assets panel.
///
/// Each variant carries the minimal constructor data the UI already has. The
/// variants map one-to-one to MDF constructors; use [`Self::constructor`] to
/// get the statement text and [`Self::binding_base`] for the readable variable
/// name.
#[derive(Debug, Clone, PartialEq)]
pub enum LayerKind {
    /// A rectangle with its destination size in pixels.
    Rect {
        /// Width in pixels.
        width: f64,
        /// Height in pixels.
        height: f64,
    },
    /// A circle with its radius in pixels.
    Circle {
        /// Radius in pixels.
        radius: f64,
    },
    /// A text layer with its content.
    Text {
        /// The text to draw.
        content: String,
    },
    /// An image layer from a project-relative source.
    Image {
        /// Project-relative image path.
        source: String,
    },
    /// An audio layer from a project-relative source.
    Audio {
        /// Project-relative audio path.
        source: String,
    },
    /// A video layer from a project-relative source.
    Video {
        /// Project-relative video path.
        source: String,
    },
    /// A vector path with its corner points.
    Path {
        /// Points in local space, at least two.
        points: Vec<(f64, f64)>,
    },
    /// An empty group that can hold children.
    Group,
}

impl LayerKind {
    /// The MDF constructor statement for this kind (without a binding).
    #[must_use]
    pub fn constructor(&self) -> String {
        match self {
            Self::Rect { width, height } => format!(
                "rect({}, {})",
                format_number(*width),
                format_number(*height)
            ),
            Self::Circle { radius } => format!("circle({})", format_number(*radius)),
            Self::Text { content } => format!("text({})", quote(content)),
            Self::Image { source } => format!("image({})", quote(source)),
            Self::Audio { source } => format!("audio({})", quote(source)),
            Self::Video { source } => format!("video({})", quote(source)),
            Self::Path { points } => {
                let points = if points.len() < 2 {
                    vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)]
                } else {
                    points.clone()
                };
                let rendered: Vec<String> = points
                    .iter()
                    .map(|(x, y)| format!("({}, {})", format_number(*x), format_number(*y)))
                    .collect();
                format!("path([{}], closed=True)", rendered.join(", "))
            }
            Self::Group => "group()".to_owned(),
        }
    }

    /// The readable base name for the generated binding (`rect`, `circle`,
    /// `text`, `image`, `audio`, `video`, `path`, `group`).
    #[must_use]
    pub const fn binding_base(&self) -> &'static str {
        match self {
            Self::Rect { .. } => "rect",
            Self::Circle { .. } => "circle",
            Self::Text { .. } => "text",
            Self::Image { .. } => "image",
            Self::Audio { .. } => "audio",
            Self::Video { .. } => "video",
            Self::Path { .. } => "path",
            Self::Group => "group",
        }
    }

    /// Whether this kind is an asset layer (`image`, `audio` or `video`).
    #[must_use]
    pub const fn is_asset(&self) -> bool {
        matches!(
            self,
            Self::Image { .. } | Self::Audio { .. } | Self::Video { .. }
        )
    }
}

/// A structural gesture resolved into code terms.
#[derive(Debug, Clone, PartialEq)]
pub enum StructuralCommand {
    /// Inserts a new layer after `after` (a node identity) or at the end of
    /// the composition body. `name` overrides the readable binding base.
    AddLayer {
        /// The constructor and its data.
        kind: LayerKind,
        /// Selected node identity, or `None` for the end of the body.
        after: Option<String>,
        /// Optional readable binding/display name base.
        name: Option<String>,
    },
    /// Deletes a layer and the statements that only build it.
    Delete {
        /// Node identity.
        id: String,
    },
    /// Copies the statements that build a layer under a fresh binding.
    Duplicate {
        /// Node identity.
        id: String,
    },
    /// Writes or edits `node.name = "..."`; an empty name removes the private
    /// name assignment.
    Rename {
        /// Node identity.
        id: String,
        /// New display name; empty removes it.
        name: String,
    },
    /// Moves a top-level layer to another position in creation order:
    /// `to` is the zero-based index among the node's siblings.
    Reorder {
        /// Node identity.
        id: String,
        /// Destination position among siblings.
        to: usize,
    },
    /// Moves a layer into a group (`Some`) or back to the top level (`None`).
    Reparent {
        /// Node identity.
        id: String,
        /// Group identity, or `None` to unparent.
        parent: Option<String>,
    },
    /// Places an asset layer from the Assets panel.
    PlaceAsset {
        /// The asset kind (`Image`, `Audio` or `Video`).
        kind: LayerKind,
        /// Project-relative source path.
        source: String,
        /// First composition frame for `audio`/`video`; ignored for images.
        frame: Option<i64>,
        /// Optional position in composition pixels.
        pos: Option<(f64, f64)>,
    },
    /// Wraps several layers in a new group in one edit.
    ///
    /// The group is created after the last selected layer and every selected
    /// layer is moved into it. Planned as one combined code edit so the whole
    /// gesture is one program write and one undo entry.
    Group {
        /// Node identities to put in the group, in document order.
        ids: Vec<String>,
        /// Optional readable group name.
        name: Option<String>,
    },
    /// Dissolves a group: its direct children return to the group's parent and
    /// the empty group is deleted, in one edit.
    Ungroup {
        /// Node identity of the group.
        id: String,
    },
}

/// Plans `command` against `program`.
///
/// `document` is what the program produces now and `origins` is the run that
/// produced it. The returned [`Plan::Code`] edit is exact for the requested
/// change; anything the planner cannot make exact returns
/// [`Plan::Fallback`] with a user-facing reason.
#[must_use]
pub fn plan_structural(
    program: &str,
    origins: &ValueOrigins,
    document: &Document,
    command: &StructuralCommand,
) -> Plan {
    plan_structural_with_metrics(
        program,
        origins,
        document,
        command,
        &motioner_core::NoTextMetrics,
    )
}

/// [`plan_structural`] with the text metrics the document was produced with.
///
/// Verification re-runs the program, so a program that calls `text_width` must
/// be re-run with the same metrics or the exact edit would be rejected.
#[must_use]
pub fn plan_structural_with_metrics(
    program: &str,
    origins: &ValueOrigins,
    document: &Document,
    command: &StructuralCommand,
    metrics: &dyn motioner_core::TextMetrics,
) -> Plan {
    match command {
        StructuralCommand::Group { ids, name } => {
            plan_group(program, origins, document, ids, name.as_deref(), metrics)
        }
        StructuralCommand::Ungroup { id } => plan_ungroup(program, origins, document, id, metrics),
        other => plan_one(program, origins, document, other, metrics),
    }
}

/// One structural command that is not itself a sequence.
fn plan_one(
    program: &str,
    origins: &ValueOrigins,
    document: &Document,
    command: &StructuralCommand,
    metrics: &dyn motioner_core::TextMetrics,
) -> Plan {
    match command {
        StructuralCommand::AddLayer { kind, after, name } => plan_add_layer(
            program,
            origins,
            document,
            kind,
            after.as_deref(),
            name.as_deref(),
            metrics,
        ),
        StructuralCommand::Delete { id } => plan_delete(program, origins, document, id, metrics),
        StructuralCommand::Duplicate { id } => {
            plan_duplicate(program, origins, document, id, metrics)
        }
        StructuralCommand::Rename { id, name } => {
            plan_rename_structural(program, origins, document, id, name, metrics)
        }
        StructuralCommand::Reorder { id, to } => {
            plan_reorder(program, origins, document, id, *to, metrics)
        }
        StructuralCommand::Reparent { id, parent } => {
            plan_reparent(program, origins, document, id, parent.as_deref(), metrics)
        }
        StructuralCommand::PlaceAsset {
            kind,
            source,
            frame,
            pos,
        } => plan_place_asset(
            program, origins, document, kind, source, *frame, *pos, metrics,
        ),
        StructuralCommand::Group { .. } | StructuralCommand::Ungroup { .. } => {
            Plan::Fallback("a group command is planned as a sequence".to_owned())
        }
    }
}

/// The state after applying one exact code edit: the program text, the run it
/// produces and the origins of that run.
struct Step {
    program: String,
    document: Document,
    origins: ValueOrigins,
}

impl Step {
    fn new(program: String) -> Option<Self> {
        let compiled = crate::run_with_overrides(&program, None, &crate::Limits::default()).ok()?;
        compiled.warnings.is_empty().then_some(Self {
            program,
            document: compiled.document,
            origins: compiled.origins,
        })
    }
}

fn plan_group(
    program: &str,
    origins: &ValueOrigins,
    document: &Document,
    ids: &[String],
    name: Option<&str>,
    metrics: &dyn motioner_core::TextMetrics,
) -> Plan {
    if ids.is_empty() {
        return Plan::Fallback("select at least one layer to group".to_owned());
    }
    // The group is created after the last selected layer in document order.
    let mut selected = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(node) = document.find(id) else {
            return Plan::Fallback(format!("no layer has the identity {id:?}"));
        };
        selected.push(node);
    }
    let last = document
        .nodes()
        .filter(|node| selected.contains(&node.id()))
        .last()
        .map(motioner_core::Node::id);
    let Some(last) = last else {
        return Plan::Fallback("select at least one layer to group".to_owned());
    };
    let last_id = document
        .node(last)
        .map(|node| node.identity().as_str().to_owned())
        .unwrap_or_default();
    let add = plan_add_layer(
        program,
        origins,
        document,
        &LayerKind::Group,
        Some(&last_id),
        name,
        metrics,
    );
    let Plan::Code(add_edit) = add else {
        return add;
    };
    let Some(step) = apply_step(program, &add_edit) else {
        return Plan::Fallback("adding the group did not compile".to_owned());
    };
    let Some(group) = step
        .document
        .nodes()
        .find(|node| document.find(node.identity().as_str()).is_none())
        .map(motioner_core::Node::id)
    else {
        return Plan::Fallback("the new group layer was not produced".to_owned());
    };
    let group_id = step
        .document
        .node(group)
        .map(|node| node.identity().as_str().to_owned())
        .unwrap_or_default();
    chain(
        step,
        add_edit,
        ids.iter().map(|id| StructuralCommand::Reparent {
            id: id.clone(),
            parent: Some(group_id.clone()),
        }),
        metrics,
    )
}

fn plan_ungroup(
    program: &str,
    origins: &ValueOrigins,
    document: &Document,
    id: &str,
    metrics: &dyn motioner_core::TextMetrics,
) -> Plan {
    let Some(node) = document.find(id) else {
        return Plan::Fallback(format!("no layer has the identity {id:?}"));
    };
    if !matches!(
        document.node(node).map(motioner_core::Node::content),
        Some(NodeContent::Group)
    ) {
        return Plan::Fallback(format!("{id:?} is not a group"));
    }
    let children: Vec<String> = document
        .nodes()
        .filter(|child| child.parent() == Some(node))
        .map(|child| child.identity().as_str().to_owned())
        .collect();
    let mut first: Option<CodeEdit> = None;
    let mut step = Step {
        program: program.to_owned(),
        document: document.clone(),
        origins: origins.clone(),
    };
    for child in &children {
        let command = StructuralCommand::Reparent {
            id: child.clone(),
            parent: None,
        };
        let plan = plan_one(
            &step.program,
            &step.origins,
            &step.document,
            &command,
            metrics,
        );
        let Plan::Code(edit) = plan else {
            return plan;
        };
        let Some(next) = apply_step(&step.program, &edit) else {
            return Plan::Fallback("removing a child from the group did not compile".to_owned());
        };
        first = Some(match first {
            None => edit,
            Some(previous) => match previous.compose(&edit) {
                Ok(combined) => combined,
                Err(error) => return Plan::Fallback(error.to_string()),
            },
        });
        step = next;
    }
    let delete = plan_one(
        &step.program,
        &step.origins,
        &step.document,
        &StructuralCommand::Delete { id: id.to_owned() },
        metrics,
    );
    let Plan::Code(delete_edit) = delete else {
        return delete;
    };
    match first {
        None => Plan::Code(delete_edit),
        Some(previous) => match previous.compose(&delete_edit) {
            Ok(combined) => Plan::Code(combined),
            Err(error) => Plan::Fallback(error.to_string()),
        },
    }
}

/// Applies `commands` in order after `first`, composing them into one edit.
fn chain(
    mut step: Step,
    first: CodeEdit,
    commands: impl IntoIterator<Item = StructuralCommand>,
    metrics: &dyn motioner_core::TextMetrics,
) -> Plan {
    let mut combined = first;
    for command in commands {
        let plan = plan_one(
            &step.program,
            &step.origins,
            &step.document,
            &command,
            metrics,
        );
        let Plan::Code(edit) = plan else {
            return plan;
        };
        let Some(next) = apply_step(&step.program, &edit) else {
            return Plan::Fallback(format!(
                "the edit for {} did not compile",
                command_name(&command)
            ));
        };
        combined = match combined.compose(&edit) {
            Ok(combined) => combined,
            Err(error) => return Plan::Fallback(error.to_string()),
        };
        step = next;
    }
    Plan::Code(combined)
}

fn command_name(command: &StructuralCommand) -> &'static str {
    match command {
        StructuralCommand::AddLayer { .. } => "add layer",
        StructuralCommand::Delete { .. } => "delete",
        StructuralCommand::Duplicate { .. } => "duplicate",
        StructuralCommand::Rename { .. } => "rename",
        StructuralCommand::Reorder { .. } => "reorder",
        StructuralCommand::Reparent { .. } => "reparent",
        StructuralCommand::PlaceAsset { .. } => "place asset",
        StructuralCommand::Group { .. } => "group",
        StructuralCommand::Ungroup { .. } => "ungroup",
    }
}

fn apply_step(program: &str, edit: &CodeEdit) -> Option<Step> {
    let after = edit.apply(program).ok()?;
    Step::new(after)
}

/// Dispatches the key and layer [`EditCommand`] variants that phase 5 turns
/// into code. Returns `None` when the command belongs to the existing value
/// strategy ladder.
pub(crate) fn plan_edit_command(
    program: &str,
    origins: &ValueOrigins,
    document: &Document,
    change: &EditCommand,
    metrics: &dyn motioner_core::TextMetrics,
) -> Option<Plan> {
    let plan = match change {
        EditCommand::RemoveKey {
            id,
            property,
            axis,
            frame,
        } => plan_remove_key(
            program, origins, document, id, property, *axis, *frame, metrics,
        ),
        EditCommand::MoveKeys(moves) => plan_move_keys(program, origins, document, moves, metrics),
        EditCommand::ToggleStopwatch {
            id,
            property,
            axis,
            frame,
            value,
            existing_keys,
        } => plan_stopwatch(
            program,
            origins,
            document,
            id,
            property,
            *axis,
            *frame,
            value,
            existing_keys,
            metrics,
        ),
        EditCommand::SetName { id, name } => {
            plan_rename_structural(program, origins, document, id, name, metrics)
        }
        EditCommand::SetLocked { .. } => Plan::Fallback(
            "'locked' is editor state, not program code; the editor stores it in editor.motion \
             (D-0036 section 3)"
                .to_owned(),
        ),
        _ => return None,
    };
    Some(plan)
}

// ---- add and place -------------------------------------------------------

fn plan_add_layer(
    program: &str,
    origins: &ValueOrigins,
    document: &Document,
    kind: &LayerKind,
    after: Option<&str>,
    name: Option<&str>,
    metrics: &dyn motioner_core::TextMetrics,
) -> Plan {
    let (insert_at, indent) = match after {
        Some(id) => {
            let Some(node) = document.find(id) else {
                return Plan::Fallback(format!("no layer has the identity {id:?}"));
            };
            match insertion_after_node(program, origins, node) {
                Ok((offset, indent)) => (Some(offset), indent),
                Err(reason) => return Plan::Fallback(reason),
            }
        }
        None => (None, String::new()),
    };
    let mut used = crate::loop_guard::program_names(program);
    let base = name.unwrap_or_else(|| kind.binding_base());
    let binding = crate::loop_guard::fresh_binding(base, &mut used);
    let constructor =
        crate::printer::format_snippet(&format!("{binding} = {}", kind.constructor()));
    let (offset, text) = match insert_at {
        Some(anchor) => (anchor, line_block(program, &indent, &[constructor])),
        None => (program.len(), end_block(program, &[constructor])),
    };
    let Some(edit) = edit(
        vec![Replacement::insert(offset, text)],
        "added a layer",
        Strategy::StructuralEdit,
    ) else {
        return Plan::Fallback("the insert range is not representable".to_owned());
    };
    let expected_index = origins
        .nodes()
        .iter()
        .filter(|origin| origin.creation.start < offset)
        .count();
    if verify_added(program, &edit, document, expected_index, kind, metrics) {
        Plan::Code(edit)
    } else {
        Plan::Fallback("the inserted constructor did not produce the expected layer".to_owned())
    }
}

#[allow(clippy::too_many_arguments)] // the command has this many facts by design
fn plan_place_asset(
    program: &str,
    _origins: &ValueOrigins,
    document: &Document,
    kind: &LayerKind,
    source: &str,
    frame: Option<i64>,
    pos: Option<(f64, f64)>,
    metrics: &dyn motioner_core::TextMetrics,
) -> Plan {
    if !kind.is_asset() {
        return Plan::Fallback(
            "asset placement needs an image, audio or video layer kind".to_owned(),
        );
    }
    let mut used = crate::loop_guard::program_names(program);
    let binding = crate::loop_guard::fresh_binding(kind.binding_base(), &mut used);
    let asset_kind = with_source(kind, source);
    let mut constructor = format!("{binding} = {}", asset_kind.constructor());
    if let Some(frame) = frame {
        if matches!(kind, LayerKind::Audio { .. } | LayerKind::Video { .. }) {
            if let Some(index) = constructor.rfind(')') {
                constructor.insert_str(index, &format!(", start={}", frame.max(0)));
            }
        }
    }
    let mut lines = vec![crate::printer::format_snippet(&constructor)];
    if let Some((x, y)) = pos {
        lines.push(crate::printer::format_snippet(&format!(
            "{binding}.pos = ({}, {})",
            format_number(x),
            format_number(y)
        )));
    }
    let offset = end_of_document(program);
    let text = end_block(program, &lines);
    let Some(edit) = edit(
        vec![Replacement::insert(offset, text)],
        "placed an asset",
        Strategy::StructuralEdit,
    ) else {
        return Plan::Fallback("the insert range is not representable".to_owned());
    };
    let expected_index = document.len();
    if verify_added(
        program,
        &edit,
        document,
        expected_index,
        &asset_kind,
        metrics,
    ) {
        Plan::Code(edit)
    } else {
        Plan::Fallback("the placed asset statement did not produce the expected layer".to_owned())
    }
}

fn with_source(kind: &LayerKind, source: &str) -> LayerKind {
    match kind {
        LayerKind::Image { .. } => LayerKind::Image {
            source: source.to_owned(),
        },
        LayerKind::Audio { .. } => LayerKind::Audio {
            source: source.to_owned(),
        },
        LayerKind::Video { .. } => LayerKind::Video {
            source: source.to_owned(),
        },
        other => other.clone(),
    }
}

/// The insertion offset after the node's creation block and its indentation.
fn insertion_after_node(
    program: &str,
    origins: &ValueOrigins,
    node: NodeId,
) -> Result<(usize, String), String> {
    let Some(origin) = origins.node_origin(node) else {
        return Err("the selected layer has no recorded creation site".to_owned());
    };
    if !origin.site.loops.is_empty() {
        return Err(
            "the selected layer is created inside a loop; add the layer at the end of the body"
                .to_owned(),
        );
    }
    if !origin.site.calls.is_empty() {
        return Err(
            "the selected layer is created inside a function; add the layer at the end of the body"
                .to_owned(),
        );
    }
    if origin.block.indent > 0 {
        return Err(
            "the selected layer is created inside a conditional block; add the layer at the end \
             of the body"
                .to_owned(),
        );
    }
    let mut end = origin.statement.end;
    for entry in origins.entries().iter().filter(|entry| entry.node == node) {
        if entry.block.span == origin.block.span {
            end = end.max(entry.statement.end);
        }
    }
    let indent = crate::loop_guard::line_indent_at(program, origin.statement.start);
    let offset = crate::loop_guard::line_end(program, end);
    Ok((offset, indent))
}

// ---- delete --------------------------------------------------------------

fn plan_delete(
    program: &str,
    origins: &ValueOrigins,
    document: &Document,
    id: &str,
    metrics: &dyn motioner_core::TextMetrics,
) -> Plan {
    let Some(node) = document.find(id) else {
        return Plan::Fallback(format!("no layer has the identity {id:?}"));
    };
    let Some(origin) = origins.node_origin(node) else {
        return Plan::Fallback("the layer has no recorded creation site".to_owned());
    };
    if !origin.site.loops.is_empty() {
        return plan_delete_loop_instance(program, origins, document, node, origin, metrics);
    }
    if !origin.site.calls.is_empty() {
        return Plan::Fallback(
            "the layer is created inside a function; delete it where it is authored".to_owned(),
        );
    }
    let Some(binding) = plan::binding_for(origin) else {
        return Plan::Fallback("the layer is not bound to a variable".to_owned());
    };
    if binding.reassigned_later {
        return Plan::Fallback("the layer binding is reassigned later in its block".to_owned());
    }
    let binding_name = origins.name(binding.name).to_owned();
    let ast = match crate::parse(program) {
        Ok(ast) => ast,
        Err(error) => return Plan::Fallback(error.to_string()),
    };
    let candidates = match removable_candidates(&ast, origins, node, binding, &binding_name) {
        Ok(candidates) => candidates,
        Err(reason) => return Plan::Fallback(reason),
    };
    let replacements = removal_replacements(program, &ast, &candidates);
    if replacements.is_empty() {
        return Plan::Fallback("no statement to remove".to_owned());
    }
    let Some(edit) = edit(
        replacements,
        format!("deleted {binding_name}"),
        Strategy::StructuralEdit,
    ) else {
        return Plan::Fallback("the deletion ranges overlap".to_owned());
    };
    if verify_deleted(program, &edit, document, node, metrics) {
        Plan::Code(edit)
    } else {
        Plan::Fallback(
            "removing the layer statements did not produce the expected document".to_owned(),
        )
    }
}

fn plan_delete_loop_instance(
    program: &str,
    origins: &ValueOrigins,
    document: &Document,
    node: NodeId,
    origin: &NodeOrigin,
    metrics: &dyn motioner_core::TextMetrics,
) -> Plan {
    if !origin.site.calls.is_empty() {
        return Plan::Fallback(
            "the layer is created through a function call inside a loop; deleting one instance is \
             not representable"
                .to_owned(),
        );
    }
    let Some(innermost) = origin.site.loops.last() else {
        return Plan::Fallback("the layer has no enclosing loop".to_owned());
    };
    let index = innermost.index;
    let loop_start = innermost.span.start;
    let creates_other = origins.nodes().iter().any(|candidate| {
        candidate.node != node
            && candidate.site.loops.iter().any(|loop_origin| {
                loop_origin.span.start == loop_start && loop_origin.index == index
            })
    });
    if creates_other {
        return Plan::Fallback(
            "that loop iteration creates more than one layer, so a `continue` would delete them \
             too"
            .to_owned(),
        );
    }
    let ast = match crate::parse(program) {
        Ok(ast) => ast,
        Err(error) => return Plan::Fallback(error.to_string()),
    };
    let Some(body) = crate::loop_guard::loop_body(&ast, innermost.span.start) else {
        return Plan::Fallback("the loop body could not be located".to_owned());
    };
    let Some(first) = body.first() else {
        return Plan::Fallback("the loop body is empty".to_owned());
    };
    let (condition, mut replacements) =
        match crate::loop_guard::node_guard_condition(program, origins, node) {
            Ok(result) => result,
            Err(reason) => return Plan::Fallback(reason),
        };
    let indent = crate::loop_guard::line_indent_at(program, first.span.start);
    let eol = crate::loop_guard::line_ending(program);
    let start = program[..first.span.start]
        .rfind('\n')
        .map_or(first.span.start, |index| index + 1);
    let unit = crate::loop_guard::indent_unit(program);
    let mut text = String::new();
    text.push_str(&indent);
    text.push_str("if ");
    text.push_str(&condition);
    text.push(':');
    text.push_str(eol);
    text.push_str(&indent);
    text.push_str(&unit);
    text.push_str(&crate::printer::format_snippet("continue"));
    text.push_str(eol);
    replacements.push(Replacement::insert(start, text));
    let Some(edit) = edit(
        replacements,
        format!("skipped the loop instance that creates {node}"),
        Strategy::StructuralLoopContinue,
    ) else {
        return Plan::Fallback("the loop guard ranges overlap".to_owned());
    };
    if verify_deleted(program, &edit, document, node, metrics) {
        Plan::Code(edit)
    } else {
        Plan::Fallback("the loop-instance guard did not produce the expected document".to_owned())
    }
}

// ---- duplicate -----------------------------------------------------------

fn plan_duplicate(
    program: &str,
    origins: &ValueOrigins,
    document: &Document,
    id: &str,
    metrics: &dyn motioner_core::TextMetrics,
) -> Plan {
    let Some(node) = document.find(id) else {
        return Plan::Fallback(format!("no layer has the identity {id:?}"));
    };
    let Some(origin) = origins.node_origin(node) else {
        return Plan::Fallback("the layer has no recorded creation site".to_owned());
    };
    if !origin.site.loops.is_empty() || !origin.site.calls.is_empty() {
        return Plan::Fallback(
            "the layer is created in a loop or function; duplicating one instance is not \
             representable"
                .to_owned(),
        );
    }
    if origin.block.indent > 0 {
        return Plan::Fallback(
            "the layer is created inside a conditional block; duplicating it is not representable"
                .to_owned(),
        );
    }
    let Some(binding) = plan::binding_for(origin) else {
        return Plan::Fallback("the layer is not bound to a variable".to_owned());
    };
    if binding.reassigned_later {
        return Plan::Fallback("the layer binding is reassigned later in its block".to_owned());
    }
    let binding_name = origins.name(binding.name).to_owned();
    let ast = match crate::parse(program) {
        Ok(ast) => ast,
        Err(error) => return Plan::Fallback(error.to_string()),
    };
    let candidates = match removable_candidates(&ast, origins, node, binding, &binding_name) {
        Ok(candidates) => candidates,
        Err(reason) => return Plan::Fallback(reason),
    };
    let add_calls = add_calls_for(&ast.body, &binding_name);
    let copy_starts: BTreeSet<usize> = candidates
        .iter()
        .copied()
        .filter(|start| !add_calls.iter().any(|(span, _)| span.start == *start))
        .collect();
    let Some(body) = block_body(&ast.body, origin.block.span) else {
        return Plan::Fallback("the layer's block could not be located".to_owned());
    };
    let indexes: Vec<usize> = copy_starts
        .iter()
        .filter_map(|start| {
            body.iter()
                .position(|statement| statement.span.start == *start)
        })
        .collect();
    if indexes.len() != copy_starts.len() || indexes.is_empty() {
        return Plan::Fallback(
            "the statements that build the layer are not all directly in its block".to_owned(),
        );
    }
    let Some(first) = indexes.iter().min().copied() else {
        return Plan::Fallback("the layer has no statements to copy".to_owned());
    };
    let Some(last) = indexes.iter().max().copied() else {
        return Plan::Fallback("the layer has no statements to copy".to_owned());
    };
    if last - first + 1 != indexes.len() {
        return Plan::Fallback("the statements that build the layer are not contiguous".to_owned());
    }
    let mut used = crate::loop_guard::program_names(program);
    let fresh = crate::loop_guard::fresh_binding(&binding_name, &mut used);
    let start = line_start(program, body[first].span.start);
    let end = crate::loop_guard::line_end(program, body[last].span.end);
    let copied = replace_identifier(&program[start..end], &binding_name, &fresh);
    let mut text = copied;
    let eol = crate::loop_guard::line_ending(program);
    if let Some(parent) = document
        .node(node)
        .and_then(motioner_core::Node::parent)
        .and_then(|parent| origins.node_origin(parent))
        .and_then(plan::binding_for)
    {
        let parent_name = origins.name(parent.name);
        text.push_str(&crate::printer::format_snippet(&format!(
            "{parent_name}.add({fresh})"
        )));
        text.push_str(eol);
    }
    let Some(edit) = edit(
        vec![Replacement::insert(end, text)],
        format!("duplicated {binding_name} as {fresh}"),
        Strategy::StructuralEdit,
    ) else {
        return Plan::Fallback("the copy range is not representable".to_owned());
    };
    let index = document
        .nodes()
        .position(|candidate| candidate.id() == node)
        .map_or(document.len(), |index| index + 1);
    if verify_duplicated(program, &edit, document, node, index, metrics) {
        Plan::Code(edit)
    } else {
        Plan::Fallback("the duplicated statements did not produce the expected layer".to_owned())
    }
}

// ---- rename --------------------------------------------------------------

#[allow(clippy::too_many_lines)] // one branch per stored/guarded/removed name
pub(crate) fn plan_rename_structural(
    program: &str,
    origins: &ValueOrigins,
    document: &Document,
    id: &str,
    name: &str,
    metrics: &dyn motioner_core::TextMetrics,
) -> Plan {
    let Some(node) = document.find(id) else {
        return Plan::Fallback(format!("no layer has the identity {id:?}"));
    };
    let Some(origin) = origins.node_origin(node) else {
        return Plan::Fallback("the layer has no recorded creation site".to_owned());
    };
    if !origin.site.calls.is_empty() {
        return Plan::Fallback(
            "the layer is created inside a function; rename it at its call site".to_owned(),
        );
    }
    let Some(binding) = plan::binding_for(origin) else {
        return Plan::Fallback("the layer is not bound to a variable".to_owned());
    };
    let binding_name = origins.name(binding.name).to_owned();
    let existing = plan::existing_set(origins, node, NAME, binding, &origin.statement);
    let index = document
        .nodes()
        .position(|candidate| candidate.id() == node);
    let replacements = if name.is_empty() {
        if !origin.site.loops.is_empty() {
            return Plan::Fallback(
                "removing the name of a loop instance is not representable".to_owned(),
            );
        }
        let Some(existing) = existing else {
            return Plan::Fallback(
                "the layer has no name assignment to remove; its name comes from its binding"
                    .to_owned(),
            );
        };
        if !origins.is_private(existing) {
            return Plan::Fallback(
                "the name assignment is shared, so removing it would rename other layers"
                    .to_owned(),
            );
        }
        let ast = match crate::parse(program) {
            Ok(ast) => ast,
            Err(error) => return Plan::Fallback(error.to_string()),
        };
        removal_replacements(program, &ast, &BTreeSet::from([existing.statement.start]))
    } else if let Some(existing) = existing {
        if !origins.is_private(existing) {
            return Plan::Fallback(
                "the name assignment is shared, so editing it would rename other layers".to_owned(),
            );
        }
        vec![Replacement::new(
            existing.span.start,
            existing.span.end,
            quote(name),
        )]
    } else if !origin.site.loops.is_empty() {
        match guarded_name_assignment(program, origins, node, origin, binding, name) {
            Ok(replacements) => replacements,
            Err(reason) => return Plan::Fallback(reason),
        }
    } else {
        let end = plan::last_write_end(origins, node, binding, &origin.statement);
        let indent = crate::loop_guard::line_indent_at(program, origin.statement.start);
        let statement =
            crate::printer::format_snippet(&format!("{binding_name}.name = {}", quote(name)));
        let offset = crate::loop_guard::line_end(program, end);
        vec![Replacement::insert(
            offset,
            line_block(program, &indent, &[statement]),
        )]
    };
    let Some(edit) = edit(
        replacements,
        format!("renamed {binding_name}"),
        Strategy::StructuralEdit,
    ) else {
        return Plan::Fallback("the rename ranges overlap".to_owned());
    };
    let Some(after) = run_checked_after(program, &edit, metrics) else {
        return Plan::Fallback("the renamed program did not run cleanly".to_owned());
    };
    if !plan::verify_with_metrics(
        &apply_edit(program, &edit).unwrap_or_default(),
        None,
        document,
        metrics,
    ) {
        return Plan::Fallback("the rename changed the rendered document".to_owned());
    }
    let actual = index
        .and_then(|index| after.nodes().nth(index))
        .and_then(|node| node.name().map(str::to_owned));
    let expected = if name.is_empty() {
        Some(binding_name.clone())
    } else {
        Some(name.to_owned())
    };
    if actual == expected {
        Plan::Code(edit)
    } else {
        Plan::Fallback("the renamed layer does not have the requested display name".to_owned())
    }
}

fn guarded_name_assignment(
    program: &str,
    origins: &ValueOrigins,
    node: NodeId,
    origin: &NodeOrigin,
    binding: &crate::origins::BindingOrigin,
    name: &str,
) -> Result<Vec<Replacement>, String> {
    let (condition, mut header) = crate::loop_guard::node_guard_condition(program, origins, node)?;
    let end = plan::last_write_end(origins, node, binding, &origin.statement);
    let indent = crate::loop_guard::line_indent_at(program, end);
    let unit = crate::loop_guard::indent_unit(program);
    let eol = crate::loop_guard::line_ending(program);
    let binding_name = origins.name(binding.name);
    let statement =
        crate::printer::format_snippet(&format!("{binding_name}.name = {}", quote(name)));
    let mut text = String::new();
    let offset = crate::loop_guard::line_end(program, end);
    if offset == program.len() && !program.ends_with('\n') {
        text.push_str(eol);
    }
    text.push_str(&indent);
    text.push_str("if ");
    text.push_str(&condition);
    text.push(':');
    text.push_str(eol);
    text.push_str(&indent);
    text.push_str(&unit);
    text.push_str(&statement);
    text.push_str(eol);
    header.push(Replacement::insert(offset, text));
    Ok(header)
}

// ---- reorder -------------------------------------------------------------

#[allow(clippy::too_many_lines)] // one branch per source range and insertion point
fn plan_reorder(
    program: &str,
    origins: &ValueOrigins,
    document: &Document,
    id: &str,
    to: usize,
    metrics: &dyn motioner_core::TextMetrics,
) -> Plan {
    let Some(node) = document.find(id) else {
        return Plan::Fallback(format!("no layer has the identity {id:?}"));
    };
    let Some(origin) = origins.node_origin(node) else {
        return Plan::Fallback("the layer has no recorded creation site".to_owned());
    };
    if !origin.site.loops.is_empty() || !origin.site.calls.is_empty() {
        return Plan::Fallback(
            "the layer is created in a loop or function; reordering is not representable"
                .to_owned(),
        );
    }
    if origin.block.indent > 0 {
        return Plan::Fallback(
            "the layer is created inside a conditional block; reordering is not representable"
                .to_owned(),
        );
    }
    if document
        .node(node)
        .and_then(motioner_core::Node::parent)
        .is_some()
    {
        return Plan::Fallback("the layer is inside a group; reorder its group instead".to_owned());
    }
    let Some(binding) = plan::binding_for(origin) else {
        return Plan::Fallback("the layer is not bound to a variable".to_owned());
    };
    let ast = match crate::parse(program) {
        Ok(ast) => ast,
        Err(error) => return Plan::Fallback(error.to_string()),
    };
    let Some(body) = block_body(&ast.body, origin.block.span) else {
        return Plan::Fallback("the layer's block could not be located".to_owned());
    };
    let starts = node_statement_starts(origins, node, origin, binding);
    let (source_start, source_end) = match contiguous_source(program, body, &starts) {
        Ok(range) => range,
        Err(reason) => return Plan::Fallback(reason),
    };
    let all_siblings: Vec<NodeId> = document
        .nodes()
        .filter(|candidate| candidate.parent().is_none())
        .map(motioner_core::Node::id)
        .collect();
    let current = all_siblings.iter().position(|candidate| *candidate == node);
    if current == Some(to) || (to >= all_siblings.len() && current == Some(all_siblings.len() - 1))
    {
        return Plan::Fallback("the layer is already at that position".to_owned());
    }
    let siblings: Vec<NodeId> = all_siblings
        .iter()
        .copied()
        .filter(|candidate| *candidate != node)
        .filter(|candidate| origins.node_origin(*candidate).is_some())
        .collect();
    let target = to.min(siblings.len());
    let insert_at = if target < siblings.len() {
        let Some(sibling_origin) = origins.node_origin(siblings[target]) else {
            return Plan::Fallback("a sibling layer has no recorded creation site".to_owned());
        };
        line_start(program, sibling_origin.statement.start)
    } else if let Some(last_sibling) = siblings.last().copied() {
        let Some(last_origin) = origins.node_origin(last_sibling) else {
            return Plan::Fallback("a sibling layer has no recorded creation site".to_owned());
        };
        let Some(last_binding) = plan::binding_for(last_origin) else {
            return Plan::Fallback("a sibling layer is not bound to a variable".to_owned());
        };
        crate::loop_guard::line_end(
            program,
            plan::last_write_end(origins, last_sibling, last_binding, &last_origin.statement),
        )
    } else {
        program.len()
    };
    if insert_at >= source_start && insert_at < source_end {
        return Plan::Fallback("the layer is already at that position".to_owned());
    }
    if insert_at == source_start {
        return Plan::Fallback("the layer is already at that position".to_owned());
    }
    let text = program[source_start..source_end].to_owned();
    let Some(edit) = edit(
        vec![
            Replacement::new(source_start, source_end, ""),
            Replacement::insert(insert_at, text),
        ],
        format!("reordered {id}"),
        Strategy::StructuralEdit,
    ) else {
        return Plan::Fallback("the move ranges overlap".to_owned());
    };
    let mut expected: Vec<Shape> = document.nodes().map(shape).collect();
    let index = document
        .nodes()
        .position(|candidate| candidate.id() == node)
        .unwrap_or(0);
    let removed = expected.remove(index);
    expected.insert(to.min(expected.len()), removed);
    if let Some(after) = run_checked_after(program, &edit, metrics) {
        let actual: Vec<Shape> = after.nodes().map(shape).collect();
        if actual == expected {
            return Plan::Code(edit);
        }
    }
    Plan::Fallback("moving the layer did not produce the expected order".to_owned())
}

// ---- parent --------------------------------------------------------------

fn plan_reparent(
    program: &str,
    origins: &ValueOrigins,
    document: &Document,
    id: &str,
    parent: Option<&str>,
    metrics: &dyn motioner_core::TextMetrics,
) -> Plan {
    let Some(node) = document.find(id) else {
        return Plan::Fallback(format!("no layer has the identity {id:?}"));
    };
    let Some(origin) = origins.node_origin(node) else {
        return Plan::Fallback("the layer has no recorded creation site".to_owned());
    };
    let Some(binding) = plan::binding_for(origin) else {
        return Plan::Fallback("the layer is not bound to a variable".to_owned());
    };
    if !origin.site.loops.is_empty() || !origin.site.calls.is_empty() {
        return Plan::Fallback(
            "the layer is created in a loop or function; parenting is not representable".to_owned(),
        );
    }
    let binding_name = origins.name(binding.name).to_owned();
    let ast = match crate::parse(program) {
        Ok(ast) => ast,
        Err(error) => return Plan::Fallback(error.to_string()),
    };
    let adds = add_calls_for(&ast.body, &binding_name);
    if adds.iter().any(|(_, sole)| !sole) {
        return Plan::Fallback(
            "the layer is added by a shared `add()` call; split it first".to_owned(),
        );
    }
    let candidates: BTreeSet<usize> = adds.iter().map(|(span, _)| span.start).collect();
    let mut replacements = removal_replacements(program, &ast, &candidates);
    let mut target_binding = None;
    if let Some(parent_id) = parent {
        let Some(parent_node) = document.find(parent_id) else {
            return Plan::Fallback(format!("no layer has the identity {parent_id:?}"));
        };
        if !matches!(
            document.node(parent_node).map(motioner_core::Node::content),
            Some(NodeContent::Group)
        ) {
            return Plan::Fallback(format!("{parent_id:?} is not a group"));
        }
        let Some(parent_origin) = origins.node_origin(parent_node) else {
            return Plan::Fallback("the parent group has no recorded creation site".to_owned());
        };
        let Some(parent_binding) = plan::binding_for(parent_origin) else {
            return Plan::Fallback("the parent group is not bound to a variable".to_owned());
        };
        if !parent_origin.site.loops.is_empty() || !parent_origin.site.calls.is_empty() {
            return Plan::Fallback(
                "the parent group is created in a loop or function; parenting is not representable"
                    .to_owned(),
            );
        }
        let parent_name = origins.name(parent_binding.name).to_owned();
        let end = plan::last_write_end(origins, node, binding, &origin.statement).max(
            plan::last_write_end(
                origins,
                parent_node,
                parent_binding,
                &parent_origin.statement,
            ),
        );
        let statement =
            crate::printer::format_snippet(&format!("{parent_name}.add({binding_name})"));
        let offset = crate::loop_guard::line_end(program, end);
        replacements.push(Replacement::insert(
            offset,
            end_block(program, &[statement]),
        ));
        target_binding = Some(parent_node);
    } else if replacements.is_empty() {
        return Plan::Fallback("the layer has no parent to remove".to_owned());
    }
    let Some(edit) = edit(
        replacements,
        format!("reparented {binding_name}"),
        Strategy::StructuralEdit,
    ) else {
        return Plan::Fallback("the parenting ranges overlap".to_owned());
    };
    let child_index = document
        .nodes()
        .position(|candidate| candidate.id() == node)
        .unwrap_or(0);
    let parent_index = target_binding.and_then(|parent| {
        document
            .nodes()
            .position(|candidate| candidate.id() == parent)
    });
    if let Some(after) = run_checked_after(program, &edit, metrics) {
        let actual_parent = after
            .nodes()
            .nth(child_index)
            .and_then(motioner_core::Node::parent)
            .and_then(|parent| after.nodes().position(|candidate| candidate.id() == parent));
        if actual_parent == parent_index {
            return Plan::Code(edit);
        }
    }
    Plan::Fallback("the add() statement did not produce the expected parent".to_owned())
}

// ---- keys ----------------------------------------------------------------

#[allow(clippy::too_many_arguments)] // the command has this many facts by design
fn plan_stopwatch(
    program: &str,
    origins: &ValueOrigins,
    document: &Document,
    id: &str,
    property: &str,
    axis: Option<Axis>,
    frame: Frame,
    value: &OverrideValue,
    existing_keys: &[Frame],
    metrics: &dyn motioner_core::TextMetrics,
) -> Plan {
    if existing_keys.is_empty() {
        let command = EditCommand::SetKey {
            id: id.to_owned(),
            property: property.to_owned(),
            axis,
            frame,
            value: value.clone(),
            ease: Ease::Linear,
        };
        return plan::plan_with_scope_and_metrics(
            program,
            origins,
            document,
            &command,
            Scope::This,
            metrics,
        );
    }
    plan_stopwatch_off(
        program,
        origins,
        document,
        id,
        property,
        axis,
        value,
        existing_keys,
        metrics,
    )
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)] // the command's facts
fn plan_stopwatch_off(
    program: &str,
    origins: &ValueOrigins,
    document: &Document,
    id: &str,
    property: &str,
    axis: Option<Axis>,
    value: &OverrideValue,
    existing_keys: &[Frame],
    metrics: &dyn motioner_core::TextMetrics,
) -> Plan {
    let property = property_name(property, axis);
    let Some(node) = document.find(id) else {
        return Plan::Fallback(format!("no layer has the identity {id:?}"));
    };
    let Some(origin) = origins.node_origin(node) else {
        return Plan::Fallback("the layer has no recorded creation site".to_owned());
    };
    if !origin.site.calls.is_empty() {
        return Plan::Fallback(
            "the layer is created inside a function; stopping the stopwatch is not representable"
                .to_owned(),
        );
    }
    if !origin.site.loops.is_empty() {
        return Plan::Fallback(
            "the layer is created in a loop; stopping the stopwatch for one instance is not \
             representable"
                .to_owned(),
        );
    }
    let Some(binding) = plan::binding_for(origin) else {
        return Plan::Fallback("the layer is not bound to a variable".to_owned());
    };
    let ast = match crate::parse(program) {
        Ok(ast) => ast,
        Err(error) => return Plan::Fallback(error.to_string()),
    };
    let mut removal_starts = BTreeSet::new();
    let mut entries = OverrideSet::new();
    for key in existing_keys {
        let Some(origin) = origins.key_origin(node, &property, *key) else {
            return Plan::Fallback(format!(
                "no code writes the key at frame {key} of '{property}'"
            ));
        };
        if origin.block.span != binding.block.span {
            return Plan::Fallback(
                "the channel has keys in another block; stopping the stopwatch is not representable"
                    .to_owned(),
            );
        }
        removal_starts.insert(origin.statement.start);
        entries.insert(OverrideEntry::new(
            id.to_owned(),
            property.clone(),
            OverrideAction::Unkey { frame: *key },
        ));
    }
    entries.insert(OverrideEntry::new(
        id.to_owned(),
        property.clone(),
        OverrideAction::Set(value.clone()),
    ));
    let Some(expected) = expected_document(document, &entries) else {
        return Plan::Fallback("the stopwatch change does not apply to this layer".to_owned());
    };
    let mut replacements = removal_replacements(program, &ast, &removal_starts);
    let mut end = origin.statement.end;
    for entry in origins.entries().iter().filter(|entry| entry.node == node) {
        if entry.block.span == binding.block.span
            && !removal_starts.contains(&entry.statement.start)
        {
            end = end.max(entry.statement.end);
        }
    }
    let indent = crate::loop_guard::line_indent_at(program, origin.statement.start);
    let offset = crate::loop_guard::line_end(program, end);
    let statement = crate::printer::format_snippet(&format!(
        "{}.{property} = {}",
        origins.name(binding.name),
        set_value_text(&property, value)
    ));
    replacements.push(Replacement::insert(
        offset,
        line_block(program, &indent, &[statement]),
    ));
    let Some(edit) = edit(
        replacements,
        format!("stopped the stopwatch for '{property}'"),
        Strategy::StructuralEdit,
    ) else {
        return Plan::Fallback("the stopwatch ranges overlap".to_owned());
    };
    if verify_edit_expected(program, &edit, &expected, metrics) {
        Plan::Code(edit)
    } else {
        Plan::Fallback("stopping the stopwatch did not produce the expected document".to_owned())
    }
}

#[allow(clippy::too_many_arguments)] // the command has this many facts by design
fn plan_remove_key(
    program: &str,
    origins: &ValueOrigins,
    document: &Document,
    id: &str,
    property: &str,
    axis: Option<Axis>,
    frame: Frame,
    metrics: &dyn motioner_core::TextMetrics,
) -> Plan {
    let property = property_name(property, axis);
    let Some(node) = document.find(id) else {
        return Plan::Fallback(format!("no layer has the identity {id:?}"));
    };
    let Some(origin) = origins.key_origin(node, &property, frame) else {
        return Plan::Fallback(format!(
            "no code writes the key at frame {frame} of '{property}'"
        ));
    };
    let mut entries = OverrideSet::new();
    entries.insert(OverrideEntry::new(
        id.to_owned(),
        property.clone(),
        OverrideAction::Unkey { frame },
    ));
    let Some(expected) = expected_document(document, &entries) else {
        return Plan::Fallback("the key removal does not apply to this layer".to_owned());
    };
    let ast = match crate::parse(program) {
        Ok(ast) => ast,
        Err(error) => return Plan::Fallback(error.to_string()),
    };
    if !is_bare_key_statement(program, &ast, origin) {
        return Plan::Fallback(
            "the key call is part of another expression; remove it manually".to_owned(),
        );
    }
    let replacements = if origin.site.loops.is_empty() {
        if !origin.site.calls.is_empty() && origins.count(origin) > 1 {
            return Plan::Fallback(
                "the key is written by a shared function call; remove it at the definition"
                    .to_owned(),
            );
        }
        removal_replacements(program, &ast, &BTreeSet::from([origin.statement.start]))
    } else if inside_if(&ast.body, origin.statement.start) {
        removal_replacements(program, &ast, &BTreeSet::from([origin.statement.start]))
    } else {
        let (condition, mut header) =
            match crate::loop_guard::node_guard_condition(program, origins, node) {
                Ok(result) => result,
                Err(reason) => return Plan::Fallback(reason),
            };
        match wrap_excluding(program, origin.statement, &condition) {
            Ok(replacement) => {
                header.push(replacement);
                header
            }
            Err(reason) => return Plan::Fallback(reason),
        }
    };
    let Some(edit) = edit(
        replacements,
        format!("removed the key at frame {frame}"),
        Strategy::StructuralEdit,
    ) else {
        return Plan::Fallback("the removal ranges overlap".to_owned());
    };
    if verify_edit_expected(program, &edit, &expected, metrics) {
        Plan::Code(edit)
    } else {
        Plan::Fallback("removing the key did not produce the expected document".to_owned())
    }
}

#[allow(clippy::too_many_lines)] // one branch per local/loop/call key
fn plan_move_keys(
    program: &str,
    origins: &ValueOrigins,
    document: &Document,
    moves: &[motioner_core::MoveKey],
    metrics: &dyn motioner_core::TextMetrics,
) -> Plan {
    if moves.is_empty() {
        return Plan::Fallback("there are no keys to move".to_owned());
    }
    let rate = document.composition().frame_rate();
    let mut entries = OverrideSet::new();
    for mover in moves {
        entries.insert(OverrideEntry::new(
            mover.key.id.clone(),
            mover.key.property.clone(),
            OverrideAction::Unkey {
                frame: mover.key.frame,
            },
        ));
        entries.insert(OverrideEntry::new(
            mover.key.id.clone(),
            mover.key.property.clone(),
            OverrideAction::Key {
                frame: mover.to,
                value: mover.value.clone(),
                ease: mover.ease,
            },
        ));
    }
    let Some(expected) = expected_document(document, &entries) else {
        return Plan::Fallback("the key move does not apply to this layer".to_owned());
    };
    let ast = match crate::parse(program) {
        Ok(ast) => ast,
        Err(error) => return Plan::Fallback(error.to_string()),
    };
    let mut parts = Vec::new();
    for mover in moves {
        let Some(node) = document.find(&mover.key.id) else {
            return Plan::Fallback(format!("no layer has the identity {:?}", mover.key.id));
        };
        let Some(origin) = origins.key_origin(node, &mover.key.property, mover.key.frame) else {
            return Plan::Fallback(format!(
                "no code writes the key at frame {} of '{}'",
                mover.key.frame, mover.key.property
            ));
        };
        if !is_bare_key_statement(program, &ast, origin) {
            return Plan::Fallback(
                "the key call is part of another expression; retime it manually".to_owned(),
            );
        }
        let shared = origins.count(origin) > 1;
        if !origin.site.loops.is_empty() && shared {
            match move_loop_key(program, origins, node, origin, mover, rate) {
                Ok(part) => parts.push(part),
                Err(reason) => return Plan::Fallback(reason),
            }
        } else if !origin.site.calls.is_empty() && shared {
            return Plan::Fallback(
                "the key is written by a shared function call; retime it at the definition"
                    .to_owned(),
            );
        } else {
            match edit_key_call(program, &ast, origin, mover, rate) {
                Ok(part) => parts.push(part),
                Err(reason) => return Plan::Fallback(reason),
            }
        }
    }
    let Some(edit) = combine(parts, "moved keys") else {
        return Plan::Fallback("the key move ranges overlap".to_owned());
    };
    if verify_edit_expected(program, &edit, &expected, metrics) {
        Plan::Code(edit)
    } else {
        Plan::Fallback("moving the keys did not produce the expected document".to_owned())
    }
}

fn move_loop_key(
    program: &str,
    origins: &ValueOrigins,
    node: NodeId,
    origin: &ValueOrigin,
    mover: &motioner_core::MoveKey,
    rate: motioner_core::FrameRate,
) -> Result<CodeEdit, String> {
    let (condition, mut header) = crate::loop_guard::node_guard_condition(program, origins, node)?;
    let to = mover.to;
    let same_frame = to == mover.key.frame;
    let Some(origin_node) = origins.node_origin(node) else {
        return Err("the layer has no recorded creation site".to_owned());
    };
    let Some(binding) = plan::binding_for(origin_node) else {
        return Err("the layer is not bound to a variable".to_owned());
    };
    let statement = key_statement(
        origins.name(binding.name),
        &mover.key.property,
        to,
        &mover.value,
        mover.ease,
        rate,
    );
    let statement = crate::printer::format_snippet(&statement);
    let end = plan::last_write_end(origins, node, binding, &origin_node.statement);
    let indent = crate::loop_guard::line_indent_at(program, end);
    let unit = crate::loop_guard::indent_unit(program);
    let eol = crate::loop_guard::line_ending(program);
    let offset = crate::loop_guard::line_end(program, end);
    let mut text = String::new();
    if offset == program.len() && !program.ends_with('\n') {
        text.push_str(eol);
    }
    text.push_str(&indent);
    text.push_str("if ");
    text.push_str(&condition);
    text.push(':');
    text.push_str(eol);
    text.push_str(&indent);
    text.push_str(&unit);
    text.push_str(&statement);
    text.push_str(eol);
    header.push(Replacement::insert(offset, text));
    if !same_frame {
        header.push(wrap_excluding(program, origin.statement, &condition)?);
    }
    let mut combined = header;
    combined.dedup();
    CodeEdit::new(
        combined,
        format!("retimed a loop key of {node}"),
        Strategy::StructuralEdit,
    )
    .map_err(|error| error.to_string())
}

fn edit_key_call(
    program: &str,
    ast: &Program,
    origin: &ValueOrigin,
    mover: &motioner_core::MoveKey,
    rate: motioner_core::FrameRate,
) -> Result<CodeEdit, String> {
    let call = crate::call_site::find_call(ast, origin.statement.start)
        .ok_or_else(|| "the key call could not be located".to_owned())?;
    let ExprKind::Call { args, .. } = &call.kind else {
        return Err("the recorded key site is not a call".to_owned());
    };
    if args.len() < 3 {
        return Err("the key call has no property, time and value arguments".to_owned());
    }
    let mut replacements = Vec::new();
    let from = mover.key.frame;
    if mover.to != from {
        replacements.push(Replacement::new(
            args[1].value.span.start,
            args[1].value.span.end,
            format_number(seconds_of(mover.to, rate)),
        ));
    }
    let value = set_value_text(&mover.key.property, &mover.value);
    if program[args[2].value.span.start..args[2].value.span.end].trim() != value {
        replacements.push(Replacement::new(
            args[2].value.span.start,
            args[2].value.span.end,
            value,
        ));
    }
    match args
        .iter()
        .find(|argument| argument.name.as_deref() == Some("ease"))
    {
        Some(ease_arg) => {
            if program[ease_arg.value.span.start..ease_arg.value.span.end].trim()
                != mover.ease.mdf_literal()
            {
                replacements.push(Replacement::new(
                    ease_arg.value.span.start,
                    ease_arg.value.span.end,
                    mover.ease.mdf_literal(),
                ));
            }
        }
        None if mover.ease != Ease::Linear => {
            replacements.extend(crate::call_site::append_argument(
                program,
                call,
                args,
                &format!("ease={}", mover.ease.mdf_literal()),
            )?);
        }
        None => {}
    }
    CodeEdit::new(
        replacements,
        format!("moved the key at frame {from} to {}", mover.to),
        Strategy::StructuralEdit,
    )
    .map_err(|error| error.to_string())
}

fn is_bare_key_statement(program: &str, ast: &Program, origin: &ValueOrigin) -> bool {
    let Some(statement) = crate::call_site::statement_containing(&ast.body, origin.statement.start)
    else {
        return false;
    };
    let StmtKind::Expr(expr) = &statement.kind else {
        return false;
    };
    if expr.span.start != origin.statement.start {
        return false;
    }
    let ExprKind::Call { callee, .. } = &expr.kind else {
        return false;
    };
    let ExprKind::Attribute { name, .. } = &callee.kind else {
        return false;
    };
    &**name == "key" && program[statement.span.start..statement.span.end].contains(".key(")
}

// ---- shared helpers ------------------------------------------------------

/// A node's observable content without its identity or display name.
type Shape = (
    NodeContent,
    motioner_core::Transform,
    motioner_core::BlendMode,
    Vec<motioner_core::Effect>,
    Option<motioner_core::MaskSpec>,
    bool,
);

fn shape(node: &motioner_core::Node) -> Shape {
    (
        node.content().clone(),
        node.transform().clone(),
        node.blend(),
        node.effects().to_vec(),
        node.mask(),
        node.is_visible(),
    )
}

fn edit(
    replacements: Vec<Replacement>,
    summary: impl Into<String>,
    strategy: Strategy,
) -> Option<CodeEdit> {
    CodeEdit::new(replacements, summary, strategy).ok()
}

fn combine(parts: Vec<CodeEdit>, summary: &str) -> Option<CodeEdit> {
    let combined = CodeEdit::combine(parts).ok()?;
    CodeEdit::new(
        combined.replacements().to_vec(),
        summary,
        Strategy::StructuralEdit,
    )
    .ok()
}

fn apply_edit(program: &str, edit: &CodeEdit) -> Result<String, String> {
    edit.apply(program).map_err(|error| error.to_string())
}

fn run_checked_after(
    program: &str,
    edit: &CodeEdit,
    metrics: &dyn motioner_core::TextMetrics,
) -> Option<Document> {
    let after = apply_edit(program, edit).ok()?;
    let compiled =
        crate::run_with_overrides_and_metrics(&after, None, &crate::Limits::default(), metrics)
            .ok()?;
    compiled.warnings.is_empty().then_some(compiled.document)
}

fn verify_edit_expected(
    program: &str,
    edit: &CodeEdit,
    expected: &Document,
    metrics: &dyn motioner_core::TextMetrics,
) -> bool {
    let Ok(after) = edit.apply(program) else {
        return false;
    };
    plan::verify_with_metrics(&after, None, expected, metrics)
}

fn verify_added(
    program: &str,
    edit: &CodeEdit,
    document: &Document,
    expected_index: usize,
    kind: &LayerKind,
    metrics: &dyn motioner_core::TextMetrics,
) -> bool {
    let Some(after) = run_checked_after(program, edit, metrics) else {
        return false;
    };
    if after.len() != document.len() + 1 {
        return false;
    }
    let Some(inserted) = after.nodes().nth(expected_index) else {
        return false;
    };
    if !matches_kind(inserted.content(), kind) {
        return false;
    }
    let before: Vec<Shape> = document.nodes().map(shape).collect();
    let actual: Vec<Shape> = after
        .nodes()
        .enumerate()
        .filter(|(index, _)| *index != expected_index)
        .map(|(_, node)| shape(node))
        .collect();
    before == actual
}

fn verify_deleted(
    program: &str,
    edit: &CodeEdit,
    document: &Document,
    node: NodeId,
    metrics: &dyn motioner_core::TextMetrics,
) -> bool {
    let Some(after) = run_checked_after(program, edit, metrics) else {
        return false;
    };
    if after.len() + 1 != document.len() {
        return false;
    }
    let index = document
        .nodes()
        .position(|candidate| candidate.id() == node)
        .unwrap_or(usize::MAX);
    let expected: Vec<Shape> = document
        .nodes()
        .enumerate()
        .filter(|(position, _)| *position != index)
        .map(|(_, node)| shape(node))
        .collect();
    let actual: Vec<Shape> = after.nodes().map(shape).collect();
    expected == actual
}

fn verify_duplicated(
    program: &str,
    edit: &CodeEdit,
    document: &Document,
    node: NodeId,
    expected_index: usize,
    metrics: &dyn motioner_core::TextMetrics,
) -> bool {
    let Some(after) = run_checked_after(program, edit, metrics) else {
        return false;
    };
    if after.len() != document.len() + 1 {
        return false;
    }
    let Some(original) = document.node(node) else {
        return false;
    };
    let Some(copy) = after.nodes().nth(expected_index) else {
        return false;
    };
    if shape(copy) != shape(original) {
        return false;
    }
    let expected: Vec<Shape> = document.nodes().map(shape).collect();
    let actual: Vec<Shape> = after
        .nodes()
        .enumerate()
        .filter(|(index, _)| *index != expected_index)
        .map(|(_, node)| shape(node))
        .collect();
    expected == actual
}

fn matches_kind(content: &NodeContent, kind: &LayerKind) -> bool {
    matches!(
        (kind, content),
        (LayerKind::Rect { .. }, NodeContent::Rect { .. })
            | (LayerKind::Circle { .. }, NodeContent::Circle { .. })
            | (LayerKind::Text { .. }, NodeContent::Text(_))
            | (LayerKind::Image { .. }, NodeContent::Image(_))
            | (LayerKind::Audio { .. }, NodeContent::Audio(_))
            | (LayerKind::Video { .. }, NodeContent::Video(_))
            | (LayerKind::Path { .. }, NodeContent::Path { .. })
            | (LayerKind::Group, NodeContent::Group)
    )
}

fn expected_document(document: &Document, entries: &OverrideSet) -> Option<Document> {
    let mut expected = document.clone();
    entries.apply(&mut expected).is_empty().then_some(expected)
}

fn property_name(property: &str, axis: Option<Axis>) -> String {
    match axis {
        Some(axis) => format!("{property}.{}", axis.name()),
        None => property.to_owned(),
    }
}

fn key_statement(
    binding: &str,
    property: &str,
    frame: Frame,
    value: &OverrideValue,
    ease: Ease,
    rate: motioner_core::FrameRate,
) -> String {
    let time = format_number(seconds_of(frame, rate));
    let ease = if ease == Ease::Linear {
        String::new()
    } else {
        format!(", ease={}", ease.mdf_literal())
    };
    format!(
        "{binding}.key({}, {time}, {}{ease})",
        quote(property),
        set_value_text(property, value)
    )
}

// ---- text helpers --------------------------------------------------------

fn end_of_document(program: &str) -> usize {
    program.len()
}

fn line_start(program: &str, offset: usize) -> usize {
    program[..offset].rfind('\n').map_or(0, |index| index + 1)
}

fn line_block(program: &str, indent: &str, lines: &[String]) -> String {
    let eol = crate::loop_guard::line_ending(program);
    let mut text = String::new();
    for line in lines {
        text.push_str(indent);
        text.push_str(line);
        text.push_str(eol);
    }
    text
}

fn end_block(program: &str, lines: &[String]) -> String {
    let eol = crate::loop_guard::line_ending(program);
    let mut text = String::new();
    if !program.ends_with('\n') && !program.is_empty() {
        text.push_str(eol);
    }
    for line in lines {
        text.push_str(line);
        text.push_str(eol);
    }
    text
}

fn removal_replacements(
    program: &str,
    ast: &Program,
    starts: &BTreeSet<usize>,
) -> Vec<Replacement> {
    let mut spans = Vec::new();
    for start in starts {
        if let Some(span) = removal_span(&ast.body, *start) {
            if !spans.contains(&span) {
                spans.push(span);
            }
        }
    }
    spans
        .into_iter()
        .map(|span| {
            Replacement::new(
                line_start(program, span.start),
                crate::loop_guard::line_end(program, span.end),
                "",
            )
        })
        .collect()
}

fn removal_span(body: &[Stmt], start: usize) -> Option<crate::Span> {
    for statement in body {
        if statement.span.start == start {
            return Some(statement.span);
        }
        let nested = match &statement.kind {
            StmtKind::If {
                branches,
                otherwise,
            } => {
                let mut found = None;
                for (_, branch) in branches {
                    if contains_offset(branch, start) {
                        let alone = branches.len() == 1
                            && otherwise.is_empty()
                            && branch.len() == 1
                            && branch[0].span.start == start;
                        found = Some(if alone {
                            statement.span
                        } else {
                            removal_span(branch, start)?
                        });
                        break;
                    }
                }
                if found.is_none() && contains_offset(otherwise, start) {
                    found = removal_span(otherwise, start);
                }
                found
            }
            StmtKind::For { body, .. } => contains_offset(body, start)
                .then(|| removal_span(body, start))
                .flatten(),
            StmtKind::Def(function) => contains_offset(&function.body, start)
                .then(|| removal_span(&function.body, start))
                .flatten(),
            _ => None,
        };
        if nested.is_some() {
            return nested;
        }
    }
    None
}

fn contains_offset(body: &[Stmt], offset: usize) -> bool {
    body.iter()
        .any(|statement| statement.span.start <= offset && offset < statement.span.end)
}

fn block_body(body: &[Stmt], block: crate::Span) -> Option<&[Stmt]> {
    if body
        .first()
        .is_some_and(|first| block.start == first.span.start)
        && body.last().is_some_and(|last| block.end == last.span.end)
    {
        return Some(body);
    }
    for statement in body {
        let nested = match &statement.kind {
            StmtKind::If {
                branches,
                otherwise,
            } => {
                let mut found = None;
                for (_, branch) in branches {
                    if let Some(body) = block_body(branch, block) {
                        found = Some(body);
                        break;
                    }
                }
                found.or_else(|| block_body(otherwise, block))
            }
            StmtKind::For { body, .. } => block_body(body, block),
            StmtKind::Def(function) => block_body(&function.body, block),
            _ => None,
        };
        if nested.is_some() {
            return nested;
        }
    }
    None
}

fn node_statement_starts(
    origins: &ValueOrigins,
    node: NodeId,
    origin: &NodeOrigin,
    binding: &crate::origins::BindingOrigin,
) -> BTreeSet<usize> {
    let mut starts = BTreeSet::new();
    starts.insert(origin.statement.start);
    for entry in origins.entries().iter().filter(|entry| entry.node == node) {
        if entry.block.span == binding.block.span {
            starts.insert(entry.statement.start);
        }
    }
    starts
}

fn contiguous_source(
    program: &str,
    body: &[Stmt],
    starts: &BTreeSet<usize>,
) -> Result<(usize, usize), String> {
    let indexes: Vec<usize> = starts
        .iter()
        .filter_map(|start| {
            body.iter()
                .position(|statement| statement.span.start == *start)
        })
        .collect();
    if indexes.is_empty() || indexes.len() != starts.len() {
        return Err(
            "the statements that build the layer are not all directly in its block".to_owned(),
        );
    }
    let Some(first) = indexes.iter().min().copied() else {
        return Err("the layer has no statements to move".to_owned());
    };
    let Some(last) = indexes.iter().max().copied() else {
        return Err("the layer has no statements to move".to_owned());
    };
    if last - first + 1 != indexes.len() {
        return Err("the statements that build the layer are not contiguous".to_owned());
    }
    Ok((
        line_start(program, body[first].span.start),
        crate::loop_guard::line_end(program, body[last].span.end),
    ))
}

fn wrap_excluding(
    program: &str,
    statement: crate::Span,
    condition: &str,
) -> Result<Replacement, String> {
    let start = line_start(program, statement.start);
    let end = crate::loop_guard::line_end(program, statement.end);
    let line = &program[start..end];
    if !line.trim_end().ends_with('\n') && end != program.len() {
        return Err("the key call spans several lines; retime it manually".to_owned());
    }
    let body = line.trim_end_matches(['\r', '\n']);
    if !body.contains(&program[statement.start..statement.end]) {
        return Err("the key call spans several lines; retime it manually".to_owned());
    }
    let indent = crate::loop_guard::line_indent_at(program, statement.start);
    let unit = crate::loop_guard::indent_unit(program);
    let eol = crate::loop_guard::line_ending(program);
    let inner = body
        .strip_prefix(&indent)
        .ok_or_else(|| "the key line indentation could not be read".to_owned())?;
    let mut text = String::new();
    text.push_str(&indent);
    text.push_str("if ");
    text.push_str(&negated_condition(condition));
    text.push(':');
    text.push_str(eol);
    text.push_str(&indent);
    text.push_str(&unit);
    text.push_str(inner);
    text.push_str(eol);
    Ok(Replacement::new(start, end, text))
}

fn negated_condition(condition: &str) -> String {
    if !condition.contains(" and ") && !condition.contains(" or ") {
        if let Some((variable, value)) = condition.split_once(" == ") {
            return format!("{variable} != {value}");
        }
        if let Some((variable, values)) = condition.split_once(" in (") {
            if let Some(values) = values.strip_suffix(')') {
                return format!("{variable} not in ({values})");
            }
        }
    }
    format!("not ({condition})")
}

fn inside_if(body: &[Stmt], offset: usize) -> bool {
    for statement in body {
        match &statement.kind {
            StmtKind::If {
                branches,
                otherwise,
            } => {
                for (_, branch) in branches {
                    if contains_offset(branch, offset) || inside_if(branch, offset) {
                        return true;
                    }
                }
                if contains_offset(otherwise, offset) || inside_if(otherwise, offset) {
                    return true;
                }
            }
            StmtKind::For { body, .. } if inside_if(body, offset) => return true,
            StmtKind::Def(function) if inside_if(&function.body, offset) => return true,
            _ => {}
        }
    }
    false
}

/// Every statement start that only builds `node` in its block, including
/// `parent.add(child)` calls whose only argument is the node.
fn removable_candidates(
    ast: &Program,
    origins: &ValueOrigins,
    node: NodeId,
    binding: &crate::origins::BindingOrigin,
    binding_name: &str,
) -> Result<BTreeSet<usize>, String> {
    let mut starts = BTreeSet::new();
    let Some(origin) = origins.node_origin(node) else {
        return Err("the layer has no recorded creation site".to_owned());
    };
    starts.insert(origin.statement.start);
    for entry in origins.entries().iter().filter(|entry| entry.node == node) {
        if entry.block.span == binding.block.span {
            starts.insert(entry.statement.start);
        }
    }
    for (span, sole) in add_calls_for(&ast.body, binding_name) {
        if sole {
            starts.insert(span.start);
        } else {
            return Err("the layer is added by a shared `add()` call; split it first".to_owned());
        }
    }
    let other_nodes: HashSet<&str> = origins
        .nodes()
        .iter()
        .filter(|candidate| candidate.node != node)
        .flat_map(|candidate| candidate.bindings.iter())
        .map(|binding| origins.name(binding.name))
        .collect();
    for span in statements_mentioning(&ast.body, binding_name) {
        if !starts.contains(&span.start) {
            return Err(format!(
                "a statement at line {} also uses {binding_name}; it is shared",
                span.line
            ));
        }
        let Some(statement) = find_statement(&ast.body, span.start) else {
            continue;
        };
        if is_sole_add_call(statement, binding_name) {
            continue;
        }
        if direct_names(statement)
            .into_iter()
            .any(|name| other_nodes.contains(name))
        {
            return Err(format!(
                "the statement at line {} also writes another layer; it is shared",
                span.line
            ));
        }
    }
    Ok(starts)
}

fn statements_mentioning(body: &[Stmt], name: &str) -> Vec<crate::Span> {
    let mut spans = Vec::new();
    collect_mentions(body, name, &mut spans);
    spans
}

fn collect_mentions(body: &[Stmt], name: &str, spans: &mut Vec<crate::Span>) {
    for statement in body {
        if statement_mentions(statement, name) {
            spans.push(statement.span);
        }
        match &statement.kind {
            StmtKind::If {
                branches,
                otherwise,
            } => {
                for (_, branch) in branches {
                    collect_mentions(branch, name, spans);
                }
                collect_mentions(otherwise, name, spans);
            }
            StmtKind::For { body, .. } => collect_mentions(body, name, spans),
            StmtKind::Def(function) => collect_mentions(&function.body, name, spans),
            _ => {}
        }
    }
}

fn statement_mentions(statement: &Stmt, name: &str) -> bool {
    match &statement.kind {
        StmtKind::Expr(expr) | StmtKind::Return(Some(expr)) => expr_mentions(expr, name),
        StmtKind::Assign { target, value, .. } => {
            target_mentions(target, name) || expr_mentions(value, name)
        }
        StmtKind::If { branches, .. } => branches
            .iter()
            .any(|(condition, _)| expr_mentions(condition, name)),
        StmtKind::For { iterable, .. } => expr_mentions(iterable, name),
        StmtKind::Def(_)
        | StmtKind::Return(None)
        | StmtKind::Break
        | StmtKind::Continue
        | StmtKind::Pass => false,
    }
}

fn target_mentions(target: &Target, name: &str) -> bool {
    match target {
        Target::Name(candidate) => &**candidate == name,
        Target::Attribute { object, .. } => expr_mentions(object, name),
        Target::Index { object, index } => {
            expr_mentions(object, name) || expr_mentions(index, name)
        }
    }
}

fn expr_mentions(expr: &Expr, name: &str) -> bool {
    match &expr.kind {
        ExprKind::Name(candidate) => &**candidate == name,
        ExprKind::List(items) | ExprKind::Tuple(items) => {
            items.iter().any(|item| expr_mentions(item, name))
        }
        ExprKind::Unary { operand, .. } | ExprKind::Not(operand) => expr_mentions(operand, name),
        ExprKind::Binary { left, right, .. }
        | ExprKind::Compare { left, right, .. }
        | ExprKind::Logical { left, right, .. } => {
            expr_mentions(left, name) || expr_mentions(right, name)
        }
        ExprKind::Call { callee, args } => {
            expr_mentions(callee, name)
                || args
                    .iter()
                    .any(|argument| expr_mentions(&argument.value, name))
        }
        ExprKind::Attribute { object, .. } => expr_mentions(object, name),
        ExprKind::Index { object, index } => {
            expr_mentions(object, name) || expr_mentions(index, name)
        }
        ExprKind::Number(_) | ExprKind::Str(_) | ExprKind::Bool(_) | ExprKind::None => false,
    }
}

fn direct_names(statement: &Stmt) -> Vec<&str> {
    let mut names = Vec::new();
    match &statement.kind {
        StmtKind::Expr(expr) | StmtKind::Return(Some(expr)) => collect_names(expr, &mut names),
        StmtKind::Assign { target, value, .. } => {
            collect_target_names(target, &mut names);
            collect_names(value, &mut names);
        }
        StmtKind::If { branches, .. } => {
            for (condition, _) in branches {
                collect_names(condition, &mut names);
            }
        }
        StmtKind::For { iterable, .. } => collect_names(iterable, &mut names),
        StmtKind::Def(_)
        | StmtKind::Return(None)
        | StmtKind::Break
        | StmtKind::Continue
        | StmtKind::Pass => {}
    }
    names
}

fn collect_target_names<'a>(target: &'a Target, names: &mut Vec<&'a str>) {
    match target {
        Target::Name(name) => names.push(name),
        Target::Attribute { object, .. } => collect_names(object, names),
        Target::Index { object, index } => {
            collect_names(object, names);
            collect_names(index, names);
        }
    }
}

fn collect_names<'a>(expr: &'a Expr, names: &mut Vec<&'a str>) {
    match &expr.kind {
        ExprKind::Name(name) => names.push(name),
        ExprKind::List(items) | ExprKind::Tuple(items) => {
            for item in items {
                collect_names(item, names);
            }
        }
        ExprKind::Unary { operand, .. } | ExprKind::Not(operand) => collect_names(operand, names),
        ExprKind::Binary { left, right, .. }
        | ExprKind::Compare { left, right, .. }
        | ExprKind::Logical { left, right, .. } => {
            collect_names(left, names);
            collect_names(right, names);
        }
        ExprKind::Call { callee, args } => {
            collect_names(callee, names);
            for argument in args {
                collect_names(&argument.value, names);
            }
        }
        ExprKind::Attribute { object, .. } => collect_names(object, names),
        ExprKind::Index { object, index } => {
            collect_names(object, names);
            collect_names(index, names);
        }
        ExprKind::Number(_) | ExprKind::Str(_) | ExprKind::Bool(_) | ExprKind::None => {}
    }
}

fn find_statement(body: &[Stmt], start: usize) -> Option<&Stmt> {
    for statement in body {
        if statement.span.start == start {
            return Some(statement);
        }
        let nested = match &statement.kind {
            StmtKind::If {
                branches,
                otherwise,
            } => {
                let mut found = None;
                for (_, branch) in branches {
                    if let Some(statement) = find_statement(branch, start) {
                        found = Some(statement);
                        break;
                    }
                }
                found.or_else(|| find_statement(otherwise, start))
            }
            StmtKind::For { body, .. } => find_statement(body, start),
            StmtKind::Def(function) => find_statement(&function.body, start),
            _ => None,
        };
        if nested.is_some() {
            return nested;
        }
    }
    None
}

fn is_sole_add_call(statement: &Stmt, name: &str) -> bool {
    let StmtKind::Expr(expr) = &statement.kind else {
        return false;
    };
    let ExprKind::Call { callee, args } = &expr.kind else {
        return false;
    };
    let ExprKind::Attribute { name: method, .. } = &callee.kind else {
        return false;
    };
    &**method == "add"
        && args.len() == 1
        && args[0].name.is_none()
        && matches!(&args[0].value.kind, ExprKind::Name(candidate) if &**candidate == name)
}

/// `add()` calls that reference `name`, with whether the node is their only
/// argument.
fn add_calls_for(body: &[Stmt], name: &str) -> Vec<(crate::Span, bool)> {
    let mut calls = Vec::new();
    collect_add_calls(body, name, &mut calls);
    calls
}

fn collect_add_calls(body: &[Stmt], name: &str, calls: &mut Vec<(crate::Span, bool)>) {
    for statement in body {
        if let StmtKind::Expr(expr) = &statement.kind {
            if let ExprKind::Call { callee, args } = &expr.kind {
                if let ExprKind::Attribute { name: method, .. } = &callee.kind {
                    if &**method == "add"
                        && args
                            .iter()
                            .any(|argument| expr_mentions(&argument.value, name))
                    {
                        let sole = args.len() == 1
                            && args[0].name.is_none()
                            && matches!(&args[0].value.kind, ExprKind::Name(candidate) if &**candidate == name);
                        calls.push((expr.span, sole));
                    }
                }
            }
        }
        match &statement.kind {
            StmtKind::If {
                branches,
                otherwise,
            } => {
                for (_, branch) in branches {
                    collect_add_calls(branch, name, calls);
                }
                collect_add_calls(otherwise, name, calls);
            }
            StmtKind::For { body, .. } => collect_add_calls(body, name, calls),
            StmtKind::Def(function) => collect_add_calls(&function.body, name, calls),
            _ => {}
        }
    }
}

fn replace_identifier(text: &str, from: &str, to: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.char_indices().peekable();
    while let Some((index, character)) = chars.next() {
        if character == '"' || character == '\'' {
            out.push(character);
            let mut escaped = false;
            for (_, current) in chars.by_ref() {
                out.push(current);
                if escaped {
                    escaped = false;
                } else if current == '\\' {
                    escaped = true;
                } else if current == character {
                    break;
                }
            }
        } else if character == '#' {
            out.push(character);
            for (_, current) in chars.by_ref() {
                out.push(current);
                if current == '\n' {
                    break;
                }
            }
        } else if is_identifier_start(character) {
            let mut end = index + character.len_utf8();
            while let Some(&(next, candidate)) = chars.peek() {
                if is_identifier_continue(candidate) {
                    end = next + candidate.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            let word = &text[index..end];
            if word == from {
                out.push_str(to);
            } else {
                out.push_str(word);
            }
        } else {
            out.push(character);
        }
    }
    out
}

fn is_identifier_start(character: char) -> bool {
    character.is_ascii_alphabetic() || character == '_'
}

fn is_identifier_continue(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}
