//! The write-through planner (D-0036, phases 1-2).
//!
//! `plan` resolves one [`EditCommand`] into a [`CodeEdit`] that reproduces it
//! on the program text, or a [`Plan::Fallback`] with a reason. Phase 2
//! implements the two exact strategies that do not need a loop or call guard:
//!
//! 1. **Private literal** — the value comes from a literal evaluated once for
//!    this node; replace it in place (moved from the hybrid bake, D-0027).
//! 2. **Local assignment** — the node is bound to a variable whose block runs
//!    once; write `var.property = value` (or the key call) after the node's last
//!    write in that block, editing an existing assignment instead of adding a
//!    second one.
//!
//! Every code plan is verified by re-running the program and comparing the
//! document with the expected one by position; when verification fails the
//! planner returns a fallback instead of guessing. Verification lives in
//! [`verify`], which the editor, the CLI and tests call.

use motioner_core::{
    format_number, quote, seconds_of, Axis, ContentField, Document, Ease, EditCommand, FrameRate,
    NodeId, OverrideAction, OverrideEntry, OverrideSet, OverrideValue, Property, LOCKED, NAME,
    VISIBLE,
};

use crate::code_edit::{CodeEdit, Replacement, Strategy};
use crate::origins::{BindingOrigin, Literal, NodeOrigin, ValueOrigin, ValueOrigins};
use crate::write_text::{key_replacement, set_replacement, set_value_text};

/// What the planner produced.
#[derive(Debug, Clone, PartialEq)]
pub enum Plan {
    /// An exact code edit for the program.
    Code(CodeEdit),
    /// No exact code edit; the reason is shown to the user (D-0036 section 4).
    Fallback(String),
}

impl Plan {
    /// The code edit, if the plan is exact.
    #[must_use]
    pub fn code(&self) -> Option<&CodeEdit> {
        match self {
            Self::Code(edit) => Some(edit),
            Self::Fallback(_) => None,
        }
    }

    /// The fallback reason, if the plan is not exact.
    #[must_use]
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Code(_) => None,
            Self::Fallback(reason) => Some(reason),
        }
    }
}

/// How wide a loop edit applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scope {
    /// Only the selected iteration (a guard), the default (D-0036).
    #[default]
    This,
    /// Every instance made by the same loop call (a shared literal edit).
    All,
}

/// Plans `change` against `program`. `document` is what the program produces
/// now; the plan is verified to produce the same document with the change
/// applied.
#[must_use]
pub fn plan(
    program: &str,
    origins: &ValueOrigins,
    document: &Document,
    change: &EditCommand,
) -> Plan {
    plan_with_scope(program, origins, document, change, Scope::This)
}

/// [`plan`] with the "this one / all" choice for nodes made in a loop.
#[must_use]
pub fn plan_with_scope(
    program: &str,
    origins: &ValueOrigins,
    document: &Document,
    change: &EditCommand,
    scope: Scope,
) -> Plan {
    plan_with_scope_and_metrics(
        program,
        origins,
        document,
        change,
        scope,
        &motioner_core::NoTextMetrics,
    )
}

/// [`plan_with_scope`] with the text metrics the document was produced with.
///
/// The verification re-runs the program; a program that calls `text_width`
/// must be re-run with the same metrics, otherwise the exact strategy would be
/// rejected as a mismatch.
#[must_use]
#[allow(clippy::too_many_lines)] // the strategy ladder reads best as one sequence
pub fn plan_with_scope_and_metrics(
    program: &str,
    origins: &ValueOrigins,
    document: &Document,
    change: &EditCommand,
    scope: Scope,
    metrics: &dyn motioner_core::TextMetrics,
) -> Plan {
    if let Some(plan) =
        crate::structural::plan_edit_command(program, origins, document, change, metrics)
    {
        return plan;
    }
    let rate = document.composition().frame_rate();
    let resolved = match resolve(change) {
        Ok(resolved) => resolved,
        Err(reason) => return Plan::Fallback(reason),
    };
    let Some(node) = document.find(&resolved.entry.id) else {
        return Plan::Fallback(format!(
            "no node has the identity {:?} (orphan)",
            resolved.entry.id
        ));
    };
    let Some(expected) = (if scope == Scope::All {
        expected_document_all(origins, document, &resolved.entry, node)
    } else {
        expected_document(document, &resolved.entry)
    }) else {
        return Plan::Fallback(format!(
            "'{}' does not apply to this node",
            resolved.entry.property
        ));
    };
    let mut reason = String::from("no exact code edit");

    // An edit that leaves every iteration with the same value is a shared
    // literal edit: the guards have become dead code (D-0036 section 2.2).
    if scope == Scope::This {
        if let Some(edit) =
            automatic_shared_value(program, origins, document, &resolved, node, metrics)
        {
            return Plan::Code(edit);
        }
    }

    // A guard that became equal to the unguarded value is dead code; remove it
    // before the private-literal edit would rewrite the guarded literal.
    if let Ok(Some(edit)) = crate::loop_guard::remove_redundant(program, origins, &resolved, node) {
        if verify_edit(program, &edit, &expected, metrics) {
            return Plan::Code(edit);
        }
    }

    if let Some(edit) = private_literal(&resolved, node, origins) {
        if verify_edit(program, &edit, &expected, metrics) {
            return Plan::Code(edit);
        }
        "the private literal did not reproduce the document".clone_into(&mut reason);
    }

    if scope == Scope::All {
        match crate::loop_guard::shared_value(program, origins, &resolved, node) {
            Ok(Some(edit)) => {
                if verify_edit(program, &edit, &expected, metrics) {
                    return Plan::Code(edit);
                }
                "the shared literal did not reproduce the document".clone_into(&mut reason);
            }
            Ok(None) => {}
            Err(error) => reason = error,
        }
    }

    match crate::loop_guard::plan_loop(program, origins, &resolved, node, rate) {
        Ok(Some(edit)) => {
            if verify_edit(program, &edit, &expected, metrics) {
                return Plan::Code(edit);
            }
            "the loop guard did not reproduce the document".clone_into(&mut reason);
        }
        Ok(None) => {}
        Err(error) => reason = error,
    }

    match crate::call_site::plan_call(program, origins, &resolved, node, rate) {
        Ok(Some(edit)) => {
            if verify_edit(program, &edit, &expected, metrics) {
                return Plan::Code(edit);
            }
            "the call-site edit did not reproduce the document".clone_into(&mut reason);
        }
        Ok(None) => {}
        Err(error) => reason = error,
    }

    match local_assignment(program, origins, &resolved, node, rate) {
        Ok(Some(edit)) => {
            if verify_edit(program, &edit, &expected, metrics) {
                return Plan::Code(edit);
            }
            "the local assignment did not reproduce the document".clone_into(&mut reason);
        }
        Ok(None) => {
            if reason == "no exact code edit" {
                reason = local_assignment_reason(&resolved, origins, node);
            }
        }
        Err(error) => {
            let structural = local_assignment_reason(&resolved, origins, node);
            if reason == "no exact code edit" {
                reason = if structural.starts_with("no exact code edit") {
                    error
                } else {
                    structural
                };
            }
        }
    }

    Plan::Fallback(reason)
}

/// The shared-literal edit when the new value already matches every other
/// iteration, verified against the all-instances document.
fn automatic_shared_value(
    program: &str,
    origins: &ValueOrigins,
    document: &Document,
    resolved: &Resolved,
    node: NodeId,
    metrics: &dyn motioner_core::TextMetrics,
) -> Option<CodeEdit> {
    if !matches!(resolved.action, Action::Set) {
        return None;
    }
    let rhs = crate::loop_guard::value_text(resolved).ok()?;
    if !crate::loop_guard::siblings_have_value(
        program,
        origins,
        node,
        &resolved.entry.property,
        &rhs,
    ) {
        return None;
    }
    let edit = crate::loop_guard::shared_value(program, origins, resolved, node).ok()??;
    let expected = expected_document_all(origins, document, &resolved.entry, node)?;
    verify_edit(program, &edit, &expected, metrics).then_some(edit)
}

/// Whether `program` (plus `overrides`, when given) produces `expected`.
///
/// Identities are compared by node position, never by provenance text: an
/// in-place edit legitimately renumbers the keys of later nodes (D-0027).
#[must_use]
pub fn verify(program: &str, overrides: Option<&str>, expected: &Document) -> bool {
    verify_with_metrics(program, overrides, expected, &motioner_core::NoTextMetrics)
}

/// [`verify`] with the text metrics the document was produced with.
///
/// A program that measures text must be re-run with the same metrics, or the
/// verifier would compare against different text bounds and refuse an exact
/// edit.
#[must_use]
pub fn verify_with_metrics(
    program: &str,
    overrides: Option<&str>,
    expected: &Document,
    metrics: &dyn motioner_core::TextMetrics,
) -> bool {
    let Ok(compiled) = crate::run_with_overrides_and_metrics(
        program,
        overrides,
        &crate::Limits::default(),
        metrics,
    ) else {
        return false;
    };
    if !compiled.warnings.is_empty() {
        return false;
    }
    semantics(&compiled.document) == semantics(expected)
}

/// One resolved change: its override entry and the action-specific data.
pub(crate) struct Resolved {
    pub(crate) entry: OverrideEntry,
    pub(crate) action: Action,
}

pub(crate) enum Action {
    Set,
    Key { frame: i64, ease: Ease },
    Visible(bool),
}

fn resolve(change: &EditCommand) -> Result<Resolved, String> {
    match change {
        EditCommand::SetValue {
            id,
            property,
            value,
        } => {
            validate_assignable(property)?;
            Ok(Resolved {
                entry: OverrideEntry::new(
                    id.clone(),
                    property.clone(),
                    OverrideAction::Set(value.clone()),
                ),
                action: Action::Set,
            })
        }
        EditCommand::SetKey {
            id,
            property,
            axis,
            frame,
            value,
            ease,
        } => {
            let property = property_name(property, *axis);
            validate_keyable(&property)?;
            Ok(Resolved {
                entry: OverrideEntry::new(
                    id.clone(),
                    property,
                    OverrideAction::Key {
                        frame: *frame,
                        value: value.clone(),
                        ease: *ease,
                    },
                ),
                action: Action::Key {
                    frame: *frame,
                    ease: *ease,
                },
            })
        }
        EditCommand::SetVisible { id, visible } => Ok(Resolved {
            entry: OverrideEntry::new(id.clone(), VISIBLE, OverrideAction::Visible(*visible)),
            action: Action::Visible(*visible),
        }),
        EditCommand::RemoveKey { .. }
        | EditCommand::MoveKeys(_)
        | EditCommand::ToggleStopwatch { .. }
        | EditCommand::SetName { .. } => Err(format!(
            "{} is planned as a structural code edit",
            command_name(change)
        )),
        EditCommand::SetLocked { .. } => {
            Err("'locked' is editor state stored in editor.motion, not program code".to_owned())
        }
        EditCommand::Reassign { .. } | EditCommand::Remove { .. } => Err(format!(
            "{} changes overrides, not the program",
            command_name(change)
        )),
    }
}

fn command_name(change: &EditCommand) -> &'static str {
    match change {
        EditCommand::SetValue { .. } => "set value",
        EditCommand::SetKey { .. } => "set key",
        EditCommand::RemoveKey { .. } => "remove key",
        EditCommand::MoveKeys(_) => "move keys",
        EditCommand::ToggleStopwatch { .. } => "toggle stopwatch",
        EditCommand::SetVisible { .. } => "set visible",
        EditCommand::SetLocked { .. } => "set locked",
        EditCommand::SetName { .. } => "set name",
        EditCommand::Reassign { .. } => "reassign",
        EditCommand::Remove { .. } => "remove override",
    }
}

fn property_name(property: &str, axis: Option<Axis>) -> String {
    match axis {
        Some(axis) => format!("{property}.{}", axis.name()),
        None => property.to_owned(),
    }
}

/// Rejects properties the program cannot assign with `var.property = value`.
fn validate_assignable(property: &str) -> Result<(), String> {
    if property == NAME || property == LOCKED || property == VISIBLE {
        return Err(format!(
            "'{property}' is editor state or uses its own strategy"
        ));
    }
    if property.contains('.') {
        return Err(format!(
            "'{property}' is a vector axis; assign the vector or key the axis"
        ));
    }
    if property == "blend"
        || Property::from_name(property).is_some()
        || ContentField::from_name(property).is_some()
    {
        return Ok(());
    }
    Err(format!(
        "'{property}' is not an assignable program property"
    ))
}

/// Rejects keys the program cannot write through `node.key(...)`.
fn validate_keyable(property: &str) -> Result<(), String> {
    let base = property.split_once('.').map_or(property, |(base, _)| base);
    if Property::from_name(base).is_some() || ContentField::from_name(base).is_some() {
        Ok(())
    } else {
        Err(format!(
            "'{property}' is not an animatable transform property or content field"
        ))
    }
}

/// The document after applying one override entry.
fn expected_document(document: &Document, entry: &OverrideEntry) -> Option<Document> {
    let mut expected = document.clone();
    let mut set = OverrideSet::new();
    set.insert(entry.clone());
    set.apply(&mut expected).is_empty().then_some(expected)
}

/// The document after applying `entry` to every node made by the same loop
/// call as `node` ("all" scope).
fn expected_document_all(
    origins: &ValueOrigins,
    document: &Document,
    entry: &OverrideEntry,
    node: NodeId,
) -> Option<Document> {
    let creation = origins.node_origin(node)?.creation.start;
    let mut expected = document.clone();
    let mut set = OverrideSet::new();
    for origin in origins
        .nodes()
        .iter()
        .filter(|origin| origin.creation.start == creation)
    {
        let Some(target) = document.node(origin.node) else {
            continue;
        };
        let mut sibling = entry.clone();
        target.identity().as_str().clone_into(&mut sibling.id);
        set.push(sibling);
    }
    (!set.is_empty() && set.apply(&mut expected).is_empty()).then_some(expected)
}

fn verify_edit(
    program: &str,
    edit: &CodeEdit,
    expected: &Document,
    metrics: &dyn motioner_core::TextMetrics,
) -> bool {
    let Ok(after) = edit.apply(program) else {
        return false;
    };
    verify_with_metrics(&after, None, expected, metrics)
}

/// Strategy 1: replace a private literal, its key value and its ease.
fn private_literal(resolved: &Resolved, node: NodeId, origins: &ValueOrigins) -> Option<CodeEdit> {
    let entry = &resolved.entry;
    let replacements = match (&resolved.action, &entry.action) {
        (Action::Set, OverrideAction::Set(value)) => {
            let origin = origins.set_origin(node, &entry.property)?;
            if !origins.is_private(origin) {
                return None;
            }
            vec![Replacement::new(
                origin.span.start,
                origin.span.end,
                set_replacement(origin, &entry.property, value)?,
            )]
        }
        (Action::Key { ease, .. }, OverrideAction::Key { frame, value, .. }) => {
            let origin = origins.key_origin(node, &entry.property, *frame)?;
            if !origins.is_private(origin) {
                return None;
            }
            let mut replacements = vec![Replacement::new(
                origin.span.start,
                origin.span.end,
                key_replacement(origin, value)?,
            )];
            match &origin.ease {
                Some(current) if current.name != ease.mdf_value() => {
                    replacements.push(Replacement::new(
                        current.span.start,
                        current.span.end,
                        ease.mdf_literal(),
                    ));
                }
                None if *ease != Ease::Linear => return None,
                Some(_) | None => {}
            }
            replacements
        }
        (Action::Visible(visible), OverrideAction::Visible(_)) => {
            let origin = origins.set_origin(node, VISIBLE)?;
            if !origins.is_private(origin) || origin.literal != Some(Literal::Bool) {
                return None;
            }
            vec![Replacement::new(
                origin.span.start,
                origin.span.end,
                if *visible { "True" } else { "False" }.to_owned(),
            )]
        }
        _ => return None,
    };
    CodeEdit::new(
        replacements,
        format!(
            "{} {} as a private literal",
            entry.property,
            action_label(&resolved.action)
        ),
        Strategy::PrivateLiteral,
    )
    .ok()
}

/// Strategy 4: a static value or key written next to the node's last write.
#[allow(clippy::too_many_lines)] // one branch per static/key/visible strategy
fn local_assignment(
    program: &str,
    origins: &ValueOrigins,
    resolved: &Resolved,
    node: NodeId,
    rate: FrameRate,
) -> Result<Option<CodeEdit>, String> {
    let Some(origin) = origins.node_origin(node) else {
        return Err("the node has no recorded creation site".to_owned());
    };
    if !origin.site.loops.is_empty() {
        return Err("the node is created in a loop; loop specialisation comes later".to_owned());
    }
    if !origin.site.calls.is_empty() {
        return Err(
            "the node is created in a function; call-site specialisation comes later".to_owned(),
        );
    }
    if value_origin(resolved, origins, node).is_some_and(|write| !write.site.calls.is_empty()) {
        return Err(
            "the value is written inside a function; the call-site edit was not exact".to_owned(),
        );
    }
    let Some(binding) = binding_for(origin) else {
        return Err("the node is not bound to a variable, or the binding is reassigned".to_owned());
    };
    if binding.block != origin.block {
        return Err("the binding lives in a different block".to_owned());
    }
    let binding_name = origins.name(binding.name).to_owned();
    let statement_end = last_write_end(origins, node, binding, &origin.statement);
    let replacements = match &resolved.action {
        Action::Set => {
            let Some(value) = value_of(&resolved.entry) else {
                return Err("the resolved change has no value".to_owned());
            };
            match existing_set(
                origins,
                node,
                &resolved.entry.property,
                binding,
                &origin.statement,
            ) {
                Some(existing) => vec![Replacement::new(
                    existing.span.start,
                    existing.span.end,
                    set_value_text(&resolved.entry.property, value),
                )],
                None => vec![insert_assignment(
                    program,
                    statement_end,
                    &format!(
                        "{binding_name}.{} = {}",
                        resolved.entry.property,
                        set_value_text(&resolved.entry.property, value)
                    ),
                )],
            }
        }
        Action::Key { frame, ease } => {
            let Some(value) = value_of(&resolved.entry) else {
                return Err("the resolved change has no value".to_owned());
            };
            match existing_key(
                origins,
                node,
                &resolved.entry.property,
                *frame,
                binding,
                &origin.statement,
            ) {
                Some(existing) => {
                    let mut replacements = vec![Replacement::new(
                        existing.span.start,
                        existing.span.end,
                        set_value_text(&resolved.entry.property, value),
                    )];
                    match &existing.ease {
                        Some(current) if current.name != ease.mdf_value() => {
                            replacements.push(Replacement::new(
                                current.span.start,
                                current.span.end,
                                ease.mdf_literal(),
                            ));
                        }
                        None if *ease != Ease::Linear => {
                            return Err("the existing key has no ease argument to edit".to_owned())
                        }
                        Some(_) | None => {}
                    }
                    replacements
                }
                None => vec![insert_assignment(
                    program,
                    statement_end,
                    &key_text(&binding_name, resolved, value, *frame, *ease, rate),
                )],
            }
        }
        Action::Visible(visible) => {
            match existing_set(origins, node, VISIBLE, binding, &origin.statement) {
                Some(existing) => vec![Replacement::new(
                    existing.span.start,
                    existing.span.end,
                    if *visible { "True" } else { "False" }.to_owned(),
                )],
                None => vec![insert_assignment(
                    program,
                    statement_end,
                    &format!(
                        "{binding_name}.visible = {}",
                        if *visible { "True" } else { "False" }
                    ),
                )],
            }
        }
    };
    CodeEdit::new(
        replacements,
        format!(
            "{} {} as a local assignment",
            resolved.entry.property,
            action_label(&resolved.action)
        ),
        Strategy::LocalAssignment,
    )
    .map(Some)
    .map_err(|error| error.to_string())
}

fn action_label(action: &Action) -> &'static str {
    match action {
        Action::Set | Action::Visible(_) => "set",
        Action::Key { .. } => "key",
    }
}

pub(crate) fn binding_for(origin: &NodeOrigin) -> Option<&BindingOrigin> {
    origin
        .bindings
        .iter()
        .find(|binding| !binding.reassigned_later)
}

pub(crate) fn last_write_end(
    origins: &ValueOrigins,
    node: NodeId,
    binding: &BindingOrigin,
    creation_statement: &crate::Span,
) -> usize {
    let mut end = creation_statement.end;
    for entry in origins.entries().iter().filter(|entry| entry.node == node) {
        if entry.block.span == binding.block.span && entry.statement.end > end {
            end = entry.statement.end;
        }
    }
    end
}

pub(crate) fn existing_set<'a>(
    origins: &'a ValueOrigins,
    node: NodeId,
    property: &str,
    binding: &BindingOrigin,
    creation_statement: &crate::Span,
) -> Option<&'a ValueOrigin> {
    origins.entries().iter().rev().find(|entry| {
        entry.node == node
            && entry.property == property
            && matches!(entry.kind, crate::origins::OriginKind::Set)
            && entry.block.span == binding.block.span
            && entry.statement.start != creation_statement.start
    })
}

pub(crate) fn existing_key<'a>(
    origins: &'a ValueOrigins,
    node: NodeId,
    property: &str,
    frame: i64,
    binding: &BindingOrigin,
    creation_statement: &crate::Span,
) -> Option<&'a ValueOrigin> {
    origins.entries().iter().rev().find(|entry| {
        entry.node == node
            && entry.property == property
            && entry.kind == crate::origins::OriginKind::Key(frame)
            && entry.block.span == binding.block.span
            && entry.statement.start != creation_statement.start
    })
}

fn value_of(entry: &OverrideEntry) -> Option<&OverrideValue> {
    match &entry.action {
        OverrideAction::Set(value) | OverrideAction::Key { value, .. } => Some(value),
        OverrideAction::Visible(_) | OverrideAction::Locked(_) | OverrideAction::Unkey { .. } => {
            None
        }
    }
}

fn key_text(
    variable: &str,
    resolved: &Resolved,
    value: &OverrideValue,
    frame: i64,
    ease: Ease,
    rate: FrameRate,
) -> String {
    let time = format_number(seconds_of(frame, rate));
    let ease = if ease == Ease::Linear {
        String::new()
    } else {
        format!(", ease={}", ease.mdf_literal())
    };
    format!(
        "{variable}.key({}, {time}, {}{ease})",
        quote(&resolved.entry.property),
        set_value_text(&resolved.entry.property, value)
    )
}

fn insert_assignment(program: &str, statement_end: usize, statement: &str) -> Replacement {
    let eol = line_ending(program);
    let offset = line_end(program, statement_end);
    let indent = line_indent(program, statement_end);
    let mut text = String::new();
    if offset == program.len() && !program.ends_with('\n') {
        text.push_str(eol);
    }
    text.push_str(&indent);
    text.push_str(&crate::printer::format_snippet(statement));
    text.push_str(eol);
    Replacement::insert(offset, text)
}

fn line_ending(program: &str) -> &'static str {
    if program.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn line_end(program: &str, offset: usize) -> usize {
    program[offset..]
        .find('\n')
        .map_or(program.len(), |index| offset + index + 1)
}

fn line_indent(program: &str, statement_end: usize) -> String {
    let line_start = program[..statement_end]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let indent: String = program[line_start..statement_end]
        .chars()
        .take_while(|character| *character == ' ' || *character == '\t')
        .collect();
    indent
}

/// Why strategy 4 could not be used, for the fallback UI.
fn local_assignment_reason(resolved: &Resolved, origins: &ValueOrigins, node: NodeId) -> String {
    let Some(origin) = origins.node_origin(node) else {
        return "the node has no recorded creation site".to_owned();
    };
    let literal_hint = value_origin(resolved, origins, node).and_then(|value| {
        if value.literal.is_none() {
            Some("the value is computed in the program, so it cannot be edited in place".to_owned())
        } else if !origins.is_private(value) {
            Some(format!(
                "the literal is shared (evaluated {} times), so editing it would change other nodes",
                origins.count(value)
            ))
        } else {
            None
        }
    });
    let structural = if !origin.site.loops.is_empty() {
        "the node is created in a loop; loop specialisation comes later".to_owned()
    } else if !origin.site.calls.is_empty() {
        "the node is created in a function; call-site specialisation comes later".to_owned()
    } else if value_origin(resolved, origins, node)
        .is_some_and(|write| !write.site.calls.is_empty())
    {
        "the value is written inside a function; the call-site edit was not exact".to_owned()
    } else if origin.bindings.is_empty()
        || origin
            .bindings
            .iter()
            .all(|binding| binding.reassigned_later)
    {
        "the node binding is reassigned later in its block".to_owned()
    } else {
        format!(
            "no exact code edit for {} {}",
            resolved.entry.property,
            action_label(&resolved.action)
        )
    };
    match literal_hint {
        Some(hint) => format!("{hint}; {structural}"),
        None => structural,
    }
}

pub(crate) fn value_origin<'a>(
    resolved: &Resolved,
    origins: &'a ValueOrigins,
    node: NodeId,
) -> Option<&'a ValueOrigin> {
    match &resolved.action {
        Action::Set => origins.set_origin(node, &resolved.entry.property),
        Action::Key { frame, .. } => origins.key_origin(node, &resolved.entry.property, *frame),
        Action::Visible(_) => origins.set_origin(node, VISIBLE),
    }
}

/// The observable composition, ignoring provenance identities (which an
/// in-place edit shifts by design) and display names (presentation only,
/// D-0044: a generated binding legitimately renames a node).
pub(crate) type Semantics = (
    motioner_core::NodeContent,
    motioner_core::Transform,
    motioner_core::BlendMode,
    Vec<motioner_core::Effect>,
    Option<motioner_core::MaskSpec>,
    bool,
    Option<usize>,
);

pub(crate) fn semantics(document: &Document) -> Vec<Semantics> {
    let index = |id: NodeId| document.nodes().position(|node| node.id() == id);
    document
        .nodes()
        .map(|node| {
            (
                normalized_content(node.content()),
                node.transform().clone(),
                node.blend(),
                node.effects().to_vec(),
                node.mask(),
                node.is_visible(),
                node.parent().and_then(index),
            )
        })
        .collect()
}

/// The program-controlled content, with resolver-owned media fields cleared.
///
/// `resolve_images`/`resolve_videos` fill the asset key and natural size from
/// the file system. The verifier re-runs the program without a resolver, so
/// comparing those fields would reject every exact edit to a program that
/// contains an image or a video. The source path, box and fit stay compared.
fn normalized_content(content: &motioner_core::NodeContent) -> motioner_core::NodeContent {
    let mut content = content.clone();
    match &mut content {
        motioner_core::NodeContent::Image(image) => image.asset = None,
        motioner_core::NodeContent::Video(video) => video.asset = None,
        _ => {}
    }
    content
}
