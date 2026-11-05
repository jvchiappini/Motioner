//! Loop specialisation for the write-through planner (D-0036 section 2.2).
//!
//! A node created inside a `for` body is fed by the same literal or expression
//! on every iteration, so editing that literal would change every instance.
//! Instead the planner inserts a **guarded statement** inside the innermost
//! loop body, after the last statement that writes the node:
//!
//! ```text
//! for i in range(5):
//!     c = circle(40, fill="#ffffff")
//!     c.pos = (200 + i * 120, 540)
//!     if i == 3:
//!         c.fill = "#ff4d4d"
//! ```
//!
//! The guard uses the loop variables (`if row == 2 and col == 5:`), merges
//! repeated iterations into `if i in (1, 3):`, extends an existing guard for
//! the same node instead of duplicating it, and removes a guard that becomes
//! redundant when the value equals the shared literal again.
//!
//! When the loop variable is `_`, it is renamed; when the iteration value has
//! no literal form (or repeats), the loop is rewritten to
//! `for i, x in enumerate(xs):` and the guard uses the index. Inline nodes
//! without a binding get a readable one (`c = circle(...)`) so the guard can
//! reference them. Everything is verified by re-running the program.

use std::collections::HashSet;

use motioner_core::{
    format_number, quote, seconds_of, Ease, FrameRate, NodeContent, NodeId, OverrideAction, VISIBLE,
};

use crate::ast::{Program, Stmt, StmtKind};
use crate::code_edit::{CodeEdit, Replacement, Strategy};
use crate::origins::{
    IterationValue, LoopOrigin, NodeOrigin, OriginKind, ValueOrigin, ValueOrigins,
};
use crate::plan::{Action, Resolved};
use crate::write_text::{key_replacement, set_replacement, set_value_text};

/// Plans a loop-guarded write for `node`, or `Ok(None)` when the node is not
/// created in a loop (the caller tries the next strategy).
#[allow(clippy::too_many_lines)] // one branch per action plus the merge/remove paths
pub(crate) fn plan_loop(
    program: &str,
    origins: &ValueOrigins,
    resolved: &Resolved,
    node: NodeId,
    rate: FrameRate,
) -> Result<Option<CodeEdit>, String> {
    let Some(origin) = origins.node_origin(node) else {
        return Ok(None);
    };
    if origin.site.loops.is_empty() {
        return Ok(None);
    }
    if !origin.site.calls.is_empty() {
        return Err(
            "the node is created in a function; call-site specialisation comes later".to_owned(),
        );
    }
    let loops = &origin.site.loops;
    let write = value_origin(resolved, origins, node);
    if let Some(write) = write {
        if !write.site.calls.is_empty() {
            return Err(
                "the value is written in a function; call-site specialisation comes later"
                    .to_owned(),
            );
        }
        if write.site.loops.is_empty() {
            return Err(
                "the value is written after the loop, so a guard inside it cannot override it"
                    .to_owned(),
            );
        }
        if write
            .site
            .loops
            .last()
            .map(|loop_origin| loop_origin.span.start)
            != loops.last().map(|loop_origin| loop_origin.span.start)
        {
            return Err("the value is written in a different loop".to_owned());
        }
    }

    let program_ast = crate::parse(program).map_err(|error| error.to_string())?;
    let innermost = loops
        .last()
        .ok_or_else(|| "the node has no enclosing loop".to_owned())?;
    let body = loop_body(&program_ast, innermost.span.start)
        .ok_or_else(|| "the loop body could not be located".to_owned())?;
    let rhs = value_text(resolved)?;
    let property = resolved.entry.property.as_str();

    // Removing a guard that now equals the unguarded value is the first exact
    // outcome: the value already comes from the shared literal.
    if let Some(edit) = remove_redundant(program, origins, resolved, node)? {
        return Ok(Some(edit));
    }

    let mut used = program_names(program);
    let (condition, mut replacements) = guard_condition(program, origins, loops, &mut used)?;

    let (binding, binding_edit) = match binding_name(origins, origin) {
        Some(name) if name != "_" => (name.to_owned(), None),
        _ => inline_binding(
            program,
            &program_ast,
            origin.creation.start,
            node,
            &mut used,
        )?,
    };
    if let Some(edit) = binding_edit {
        replacements.push(edit);
    }

    let statement = write_statement(resolved, property, &rhs, &binding, rate);
    let Some(write_span) = last_write_span(origins, node, origin.statement.start, body) else {
        return Err("the node's last write could not be located".to_owned());
    };
    let after = write_span.start;
    let insert_at = line_end(program, write_span.end);
    let eol = line_ending(program);

    // Extend an existing guard for the same node: same condition appends, a
    // different iteration of the same value merges into an `in` tuple.
    if let Some((edit, condition_edit)) = existing_guard(
        program, body, after, &condition, &statement, &binding, property, &rhs,
    ) {
        if let Some(condition_edit) = condition_edit {
            replacements.push(condition_edit);
        }
        if let Some(edit) = edit {
            replacements.push(edit);
        }
        return Ok(Some(
            CodeEdit::new(
                replacements,
                format!(
                    "{} {} under a loop guard for {binding}",
                    property,
                    action_label(&resolved.action)
                ),
                Strategy::LoopGuard,
            )
            .map_err(|error| error.to_string())?,
        ));
    }

    let indent = line_indent_at(program, body[0].span.start);
    let unit = indent_unit(program);
    let mut text = String::new();
    if insert_at == program.len() && !program.ends_with('\n') {
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
    replacements.push(Replacement::insert(insert_at, text));

    CodeEdit::new(
        replacements,
        format!(
            "{} {} under `if {condition}` for {binding}",
            property,
            action_label(&resolved.action)
        ),
        Strategy::LoopGuard,
    )
    .map(Some)
    .map_err(|error| error.to_string())
}

/// Edits the literal shared by every iteration ("all" scope).
pub(crate) fn shared_value(
    program: &str,
    origins: &ValueOrigins,
    resolved: &Resolved,
    node: NodeId,
) -> Result<Option<CodeEdit>, String> {
    let Some(origin) = origins.node_origin(node) else {
        return Ok(None);
    };
    if origin.site.loops.is_empty() {
        return Ok(None);
    }
    let ast = crate::parse(program).map_err(|error| error.to_string())?;
    let Some(unguarded) = unguarded_origin(&ast, origins, node, &resolved.entry.property) else {
        return Ok(None);
    };
    if !unguarded.site.calls.is_empty() || unguarded.site.loops.is_empty() {
        return Ok(None);
    }
    if unguarded.literal.is_none() {
        return Ok(None);
    }
    if !literal_is_confined(origins, unguarded.span.start, origin.creation.start) {
        return Err("the literal is also used outside this loop call".to_owned());
    }

    let mut replacements = literal_replacements(resolved, unguarded)?;
    let innermost = &origin.site.loops[origin.site.loops.len() - 1];
    for sibling in origins
        .nodes()
        .iter()
        .filter(|sibling| sibling.creation.start == origin.creation.start)
    {
        if let Some(removals) = remove_guards(
            program,
            origins,
            sibling.node,
            &resolved.entry.property,
            innermost,
        )? {
            replacements.extend(removals.replacements().iter().cloned());
        }
    }
    CodeEdit::new(
        replacements,
        format!(
            "{} {} for every loop instance of {}",
            resolved.entry.property,
            action_label(&resolved.action),
            resolved.entry.id
        ),
        Strategy::LoopSharedValue,
    )
    .map(Some)
    .map_err(|error| error.to_string())
}

fn value_origin<'a>(
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

pub(crate) fn value_text(resolved: &Resolved) -> Result<String, String> {
    let value = match &resolved.entry.action {
        OverrideAction::Set(value) | OverrideAction::Key { value, .. } => value,
        OverrideAction::Visible(visible) => {
            return Ok(if *visible { "True" } else { "False" }.to_owned())
        }
        OverrideAction::Locked(_) | OverrideAction::Unkey { .. } => {
            return Err("the change has no value".to_owned())
        }
    };
    Ok(set_value_text(&resolved.entry.property, value))
}

fn write_statement(
    resolved: &Resolved,
    property: &str,
    rhs: &str,
    binding: &str,
    rate: FrameRate,
) -> String {
    match resolved.action {
        Action::Set | Action::Visible(_) => format!("{binding}.{property} = {rhs}"),
        Action::Key { frame, ease } => {
            let time = format_number(seconds_of(frame, rate));
            let ease = if ease == Ease::Linear {
                String::new()
            } else {
                format!(", ease={}", ease.mdf_literal())
            };
            format!("{binding}.key({}, {time}, {rhs}{ease})", quote(property))
        }
    }
}

fn action_label(action: &Action) -> &'static str {
    match action {
        Action::Set | Action::Visible(_) => "set",
        Action::Key { .. } => "key",
    }
}

fn binding_name<'a>(origins: &'a ValueOrigins, origin: &'a NodeOrigin) -> Option<&'a str> {
    origin
        .bindings
        .iter()
        .find(|binding| !binding.reassigned_later)
        .map(|binding| origins.name(binding.name))
}

/// Adds `name = ` before an inline constructor statement and returns the
/// readable binding plus that insertion.
fn inline_binding(
    program: &str,
    ast: &Program,
    creation_start: usize,
    node: NodeId,
    used: &mut HashSet<String>,
) -> Result<(String, Option<Replacement>), String> {
    if !has_constructor_statement(&ast.body, creation_start) {
        return Err(
            "the node is created in a loop without a simple binding; assign it to a variable first"
                .to_owned(),
        );
    }
    let name = fresh_node_name(program, node, used);
    Ok((
        name.clone(),
        Some(Replacement::insert(creation_start, format!("{name} = "))),
    ))
}

fn has_constructor_statement(body: &[Stmt], creation_start: usize) -> bool {
    body.iter().any(|statement| match &statement.kind {
        StmtKind::Expr(expr) => expr.span.start == creation_start,
        StmtKind::If {
            branches,
            otherwise,
        } => {
            branches
                .iter()
                .any(|(_, body)| has_constructor_statement(body, creation_start))
                || has_constructor_statement(otherwise, creation_start)
        }
        StmtKind::For { body, .. } => has_constructor_statement(body, creation_start),
        StmtKind::Def(function) => has_constructor_statement(&function.body, creation_start),
        _ => false,
    })
}

/// A readable, unused binding name from the node's kind or display name.
fn fresh_node_name(program: &str, node: NodeId, used: &mut HashSet<String>) -> String {
    let document = crate::run(program, &crate::Limits::default()).ok();
    let base = document
        .as_ref()
        .and_then(|document| document.node(node))
        .and_then(|node| node.name().map(sanitize_name))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| kind_name(program, node));
    fresh(&base, used)
}

fn kind_name(program: &str, node: NodeId) -> String {
    let kind = crate::run(program, &crate::Limits::default())
        .ok()
        .and_then(|document| document.node(node).map(|node| content_name(node.content())))
        .unwrap_or("layer");
    kind.to_owned()
}

fn content_name(content: &NodeContent) -> &'static str {
    match content {
        NodeContent::Group => "group",
        NodeContent::Rect { .. } => "rect",
        NodeContent::Circle { .. } => "circle",
        NodeContent::Path { .. } => "path",
        NodeContent::Text(_) => "text",
        NodeContent::Image(_) => "image",
        NodeContent::Audio(_) => "audio",
        NodeContent::Video(_) => "video",
    }
}

fn sanitize_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for character in name.chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            out.push(character.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_owned()
}

fn fresh(base: &str, used: &mut HashSet<String>) -> String {
    if used.insert(base.to_owned()) {
        return base.to_owned();
    }
    for suffix in 2.. {
        let candidate = format!("{base}_{suffix}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("an unused suffix always exists")
}

/// Every name used by the program, so generated names never shadow one.
fn program_names(program: &str) -> HashSet<String> {
    match crate::tokenize(program) {
        Ok(tokens) => tokens
            .into_iter()
            .filter_map(|token| match token.kind {
                crate::TokenKind::Name(name) => Some(name.to_string()),
                _ => None,
            })
            .collect(),
        Err(_) => HashSet::new(),
    }
}

/// Builds the guard condition for the whole loop stack and the header edits it
/// needs (`_` rename, `enumerate` rewrite).
fn guard_condition(
    program: &str,
    origins: &ValueOrigins,
    loops: &[LoopOrigin],
    used: &mut HashSet<String>,
) -> Result<(String, Vec<Replacement>), String> {
    let mut conditions: Vec<String> = Vec::with_capacity(loops.len());
    let mut replacements = Vec::new();
    for loop_origin in loops {
        let (condition, edits) = loop_condition(program, origins, loop_origin, used)?;
        conditions.push(condition);
        replacements.extend(edits);
    }
    Ok((conditions.join(" and "), replacements))
}

fn loop_condition(
    program: &str,
    origins: &ValueOrigins,
    loop_origin: &LoopOrigin,
    used: &mut HashSet<String>,
) -> Result<(String, Vec<Replacement>), String> {
    let names: Vec<&str> = loop_origin
        .variables
        .iter()
        .map(|id| origins.name(*id))
        .collect();
    let iterable = &program[loop_origin.iterable.start..loop_origin.iterable.end];
    let enumerating = iterable.trim_start().starts_with("enumerate(");
    if names.len() > 1 {
        if enumerating && names.len() == 2 {
            return Ok((format!("{} == {}", names[0], loop_origin.index), Vec::new()));
        }
        return Err(
            "the loop unpacks a tuple that is not an enumerate index; rewrite it with enumerate()"
                .to_owned(),
        );
    }
    let name = names[0];
    if enumerating {
        return Err("the loop variable receives an enumerate pair; unpack it first".to_owned());
    }
    let Some(value) = &loop_origin.value else {
        return Ok(enumerate_condition(loop_origin, name, used));
    };
    if name == "_" {
        if value_is_unique(origins, loop_origin, value) {
            let fresh_name = fresh_loop_name(used);
            let header = Replacement::new(
                loop_origin.span.start,
                loop_origin.iterable.start,
                format!("for {fresh_name} in "),
            );
            return Ok((
                format!("{fresh_name} == {}", literal_text(value)),
                vec![header],
            ));
        }
        return Ok(enumerate_condition(loop_origin, name, used));
    }
    if value_is_number(value)
        && (iterable.trim_start().starts_with("range(")
            || value_is_unique(origins, loop_origin, value))
    {
        return Ok((format!("{name} == {}", literal_text(value)), Vec::new()));
    }
    if value_is_unique(origins, loop_origin, value) {
        return Ok((format!("{name} == {}", literal_text(value)), Vec::new()));
    }
    Ok(enumerate_condition(loop_origin, name, used))
}

/// `for i, x in enumerate(xs):` and a guard on the fresh index.
fn enumerate_condition(
    loop_origin: &LoopOrigin,
    name: &str,
    used: &mut HashSet<String>,
) -> (String, Vec<Replacement>) {
    let index_name = fresh_loop_name(used);
    let mut replacements = vec![Replacement::new(
        loop_origin.span.start,
        loop_origin.iterable.start,
        format!("for {index_name}, {name} in enumerate("),
    )];
    replacements.push(Replacement::insert(loop_origin.iterable.end, ")"));
    (
        format!("{index_name} == {}", loop_origin.index),
        replacements,
    )
}

fn fresh_loop_name(used: &mut HashSet<String>) -> String {
    for candidate in ["i", "j", "k", "index", "row", "col", "item", "n"] {
        if used.insert(candidate.to_owned()) {
            return candidate.to_owned();
        }
    }
    fresh("i", used)
}

fn literal_text(value: &IterationValue) -> String {
    match value {
        IterationValue::Number(number) => format_number(*number),
        IterationValue::Text(text) => quote(text),
        IterationValue::Bool(true) => "True".to_owned(),
        IterationValue::Bool(false) => "False".to_owned(),
        IterationValue::None => "None".to_owned(),
    }
}

fn value_is_number(value: &IterationValue) -> bool {
    matches!(value, IterationValue::Number(_))
}

/// Whether exactly one iteration of `loop_origin` produced `value`.
fn value_is_unique(
    origins: &ValueOrigins,
    loop_origin: &LoopOrigin,
    value: &IterationValue,
) -> bool {
    let mut indices = HashSet::new();
    let mut record = |loops: &[LoopOrigin]| {
        if let Some(candidate) = loops
            .iter()
            .find(|candidate| candidate.span.start == loop_origin.span.start)
        {
            if candidate.value.as_ref() == Some(value) {
                indices.insert(candidate.index);
            }
        }
    };
    for entry in origins.entries() {
        record(&entry.site.loops);
    }
    for origin in origins.nodes() {
        record(&origin.site.loops);
    }
    indices.len() == 1
}

/// Whether the value the program computes without a guard already equals
/// `rhs`, so a guard would be redundant.
fn unguarded_matches(
    program: &str,
    ast: &Program,
    origins: &ValueOrigins,
    node: NodeId,
    property: &str,
    rhs: &str,
) -> bool {
    unguarded_origin(ast, origins, node, property)
        .is_some_and(|origin| program[origin.span.start..origin.span.end].trim() == rhs)
}

/// The last origin of `node`/`property` that is not inside an `if` branch.
fn unguarded_origin<'a>(
    ast: &Program,
    origins: &'a ValueOrigins,
    node: NodeId,
    property: &str,
) -> Option<&'a ValueOrigin> {
    origins.entries().iter().rev().find(|entry| {
        entry.node == node
            && entry.property == property
            && !is_inside_if(&ast.body, entry.statement.start)
    })
}

fn is_inside_if(body: &[Stmt], offset: usize) -> bool {
    for statement in body {
        match &statement.kind {
            StmtKind::If {
                branches,
                otherwise,
            } => {
                for (_, branch) in branches {
                    if contains_offset(branch, offset) || is_inside_if(branch, offset) {
                        return true;
                    }
                }
                if contains_offset(otherwise, offset) || is_inside_if(otherwise, offset) {
                    return true;
                }
            }
            StmtKind::For { body, .. } => {
                if is_inside_if(body, offset) {
                    return true;
                }
            }
            StmtKind::Def(function) if is_inside_if(&function.body, offset) => {
                return true;
            }
            _ => {}
        }
    }
    false
}

fn contains_offset(body: &[Stmt], offset: usize) -> bool {
    body.iter()
        .any(|statement| statement.span.start <= offset && offset < statement.span.end)
}

fn literal_is_confined(origins: &ValueOrigins, span_start: usize, creation_start: usize) -> bool {
    origins
        .entries()
        .iter()
        .filter(|entry| entry.span.start == span_start)
        .all(|entry| {
            origins
                .node_origin(entry.node)
                .is_some_and(|origin| origin.creation.start == creation_start)
        })
}

fn literal_replacements(
    resolved: &Resolved,
    origin: &ValueOrigin,
) -> Result<Vec<Replacement>, String> {
    let value = match &resolved.entry.action {
        OverrideAction::Set(value) | OverrideAction::Key { value, .. } => value,
        OverrideAction::Visible(visible) => {
            return Ok(vec![Replacement::new(
                origin.span.start,
                origin.span.end,
                if *visible { "True" } else { "False" },
            )])
        }
        OverrideAction::Locked(_) | OverrideAction::Unkey { .. } => {
            return Err("the change has no value".to_owned())
        }
    };
    let text = match resolved.action {
        Action::Key { .. } => key_replacement(origin, value),
        Action::Set | Action::Visible(_) => {
            set_replacement(origin, &resolved.entry.property, value)
        }
    }
    .ok_or_else(|| "the literal cannot hold the new value".to_owned())?;
    let mut replacements = vec![Replacement::new(origin.span.start, origin.span.end, text)];
    if let Action::Key { ease, .. } = resolved.action {
        if let Some(current) = &origin.ease {
            if current.name != ease.mdf_value() {
                replacements.push(Replacement::new(
                    current.span.start,
                    current.span.end,
                    ease.mdf_literal(),
                ));
            }
        }
    }
    Ok(replacements)
}

/// Removes a guard when the selected value equals the unguarded value again
/// ("this" scope): the guarded statement is dead code.
pub(crate) fn remove_redundant(
    program: &str,
    origins: &ValueOrigins,
    resolved: &Resolved,
    node: NodeId,
) -> Result<Option<CodeEdit>, String> {
    if !matches!(resolved.action, Action::Set | Action::Visible(_)) {
        return Ok(None);
    }
    let Some(origin) = origins.node_origin(node) else {
        return Ok(None);
    };
    if origin.site.loops.is_empty() {
        return Ok(None);
    }
    let ast = crate::parse(program).map_err(|error| error.to_string())?;
    let rhs = value_text(resolved)?;
    let property = resolved.entry.property.as_str();
    if !unguarded_matches(program, &ast, origins, node, property, &rhs) {
        return Ok(None);
    }
    let innermost = &origin.site.loops[origin.site.loops.len() - 1];
    remove_guards(program, origins, node, property, innermost)
}

/// Whether every other instance of the same loop call already ends with
/// `rhs`, so editing the shared literal changes only the selected node and
/// the guards are redundant.
pub(crate) fn siblings_have_value(
    program: &str,
    origins: &ValueOrigins,
    node: NodeId,
    property: &str,
    rhs: &str,
) -> bool {
    let Some(origin) = origins.node_origin(node) else {
        return false;
    };
    let mut found = false;
    for sibling in origins
        .nodes()
        .iter()
        .filter(|sibling| sibling.creation.start == origin.creation.start && sibling.node != node)
    {
        found = true;
        let Some(entry) = origins
            .entries()
            .iter()
            .rev()
            .find(|entry| entry.node == sibling.node && entry.property == property)
        else {
            return false;
        };
        if program[entry.span.start..entry.span.end].trim() != rhs {
            return false;
        }
    }
    found
}

/// The top-level statement of the innermost loop that last writes `node`.
fn last_write_span(
    origins: &ValueOrigins,
    node: NodeId,
    creation_statement_start: usize,
    body: &[Stmt],
) -> Option<crate::Span> {
    let mut best = containing_span(body, creation_statement_start);
    for entry in origins.entries().iter().filter(|entry| entry.node == node) {
        if let Some(span) = containing_span(body, entry.statement.start) {
            if best.is_none_or(|current| span.end > current.end) {
                best = Some(span);
            }
        }
    }
    best
}

fn containing_span(body: &[Stmt], offset: usize) -> Option<crate::Span> {
    body.iter()
        .find(|statement| statement.span.start <= offset && offset < statement.span.end)
        .map(|statement| statement.span)
}

fn loop_body(program: &Program, span_start: usize) -> Option<&[Stmt]> {
    find_loop_body(&program.body, span_start)
}

fn find_loop_body(body: &[Stmt], span_start: usize) -> Option<&[Stmt]> {
    for statement in body {
        if let StmtKind::For { body: inner, .. } = &statement.kind {
            if statement.span.start == span_start {
                return Some(inner);
            }
            if let Some(found) = find_loop_body(inner, span_start) {
                return Some(found);
            }
        }
        let nested = match &statement.kind {
            StmtKind::If {
                branches,
                otherwise,
            } => branches
                .iter()
                .find_map(|(_, branch)| find_loop_body(branch, span_start))
                .or_else(|| find_loop_body(otherwise, span_start)),
            StmtKind::Def(function) => find_loop_body(&function.body, span_start),
            _ => None,
        };
        if nested.is_some() {
            return nested;
        }
    }
    None
}

/// Appends to an existing guard or merges its condition into an `in` tuple.
#[allow(clippy::too_many_arguments)] // one call site, each argument is a fact the merge needs
fn existing_guard(
    program: &str,
    body: &[Stmt],
    after: usize,
    condition: &str,
    statement: &str,
    binding: &str,
    property: &str,
    rhs: &str,
) -> Option<(Option<Replacement>, Option<Replacement>)> {
    let mut merge: Option<(Option<Replacement>, Option<Replacement>)> = None;
    for candidate in body
        .iter()
        .filter(|statement| statement.span.start >= after)
    {
        let StmtKind::If { branches, .. } = &candidate.kind else {
            continue;
        };
        let Some((condition_expr, branch)) = branches.first() else {
            continue;
        };
        let text = &program[condition_expr.span.start..condition_expr.span.end];
        if text == condition {
            if branch_has_statement(program, branch, statement) {
                return Some((None, None));
            }
            let edit = append_to_branch(program, branch, statement)?;
            return Some((Some(edit), None));
        }
        if merge.is_none() {
            if let Some(merged) = merge_condition(text, condition) {
                if branch_mentions(program, branch, binding, property, rhs) {
                    let condition_edit = Replacement::new(
                        condition_expr.span.start,
                        condition_expr.span.end,
                        merged,
                    );
                    if branch_has_statement(program, branch, statement) {
                        return Some((None, Some(condition_edit)));
                    }
                    let edit = append_to_branch(program, branch, statement)?;
                    merge = Some((Some(edit), Some(condition_edit)));
                }
            }
        }
    }
    merge
}

/// Whether `branch` already contains exactly `statement`.
fn branch_has_statement(program: &str, branch: &[Stmt], statement: &str) -> bool {
    branch
        .iter()
        .any(|inner| program[inner.span.start..inner.span.end].trim() == statement.trim())
}

fn append_to_branch(program: &str, branch: &[Stmt], statement: &str) -> Option<Replacement> {
    let last = branch.last()?;
    let offset = line_end(program, last.span.end);
    let indent = line_indent_at(program, last.span.start);
    let eol = line_ending(program);
    let mut text = String::new();
    if offset == program.len() && !program.ends_with('\n') {
        text.push_str(eol);
    }
    text.push_str(&indent);
    text.push_str(statement);
    text.push_str(eol);
    Some(Replacement::insert(offset, text))
}

fn branch_mentions(
    program: &str,
    branch: &[Stmt],
    binding: &str,
    property: &str,
    rhs: &str,
) -> bool {
    let Some(first) = branch.first() else {
        return false;
    };
    let Some(last) = branch.last() else {
        return false;
    };
    let text = &program[first.span.start..last.span.end];
    let target = format!("{binding}.{property}");
    text.contains(&target) && text.contains(rhs)
}

/// `i == 3` plus `i == 1` becomes `i in (1, 3)`.
fn merge_condition(existing: &str, condition: &str) -> Option<String> {
    let (existing_var, existing_values) = simple_condition(existing)?;
    let (new_var, new_values) = simple_condition(condition)?;
    if existing_var != new_var {
        return None;
    }
    let mut values = existing_values;
    values.extend(new_values);
    values.sort_by(compare_guard_values);
    values.dedup();
    if values.len() < 2 {
        return None;
    }
    let rendered: Vec<&str> = values.iter().map(String::as_str).collect();
    Some(format!("{existing_var} in ({})", rendered.join(", ")))
}

fn simple_condition(condition: &str) -> Option<(String, Vec<String>)> {
    let condition = condition.trim();
    if let Some((variable, rest)) = condition.split_once(" == ") {
        return Some((variable.trim().to_owned(), vec![rest.trim().to_owned()]));
    }
    let (variable, rest) = condition.split_once(" in (")?;
    let inner = rest.strip_suffix(')')?;
    let values: Vec<String> = inner
        .split(',')
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect();
    (!values.is_empty()).then(|| (variable.trim().to_owned(), values))
}

fn compare_guard_values(a: &String, b: &String) -> std::cmp::Ordering {
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => x.total_cmp(&y),
        _ => a.cmp(b),
    }
}

/// Removes guarded writes of `property` for `node` inside `loop_origin`.
fn remove_guards(
    program: &str,
    origins: &ValueOrigins,
    node: NodeId,
    property: &str,
    loop_origin: &LoopOrigin,
) -> Result<Option<CodeEdit>, String> {
    let ast = crate::parse(program).map_err(|error| error.to_string())?;
    let Some(body) = loop_body(&ast, loop_origin.span.start) else {
        return Ok(None);
    };
    let statements: Vec<usize> = origins
        .entries()
        .iter()
        .filter(|entry| {
            entry.node == node
                && entry.property == property
                && matches!(entry.kind, OriginKind::Set | OriginKind::Key(_))
                && entry.statement.start >= loop_origin.body.start
                && entry.statement.start <= loop_origin.body.end
        })
        .map(|entry| entry.statement.start)
        .collect();
    let mut replacements = Vec::new();
    for statement in statements {
        if let Some(replacement) = removal_for(program, body, statement) {
            replacements.push(replacement);
        }
    }
    if replacements.is_empty() {
        return Ok(None);
    }
    CodeEdit::new(
        replacements,
        format!("removed a redundant guard for {property}"),
        Strategy::LoopGuard,
    )
    .map(Some)
    .map_err(|error| error.to_string())
}

/// The removal of the statement at `offset` when it sits inside an `if`
/// branch, or of the whole `if` when that statement is alone in it.
fn removal_for(program: &str, body: &[Stmt], offset: usize) -> Option<Replacement> {
    for statement in body {
        let StmtKind::If { branches, .. } = &statement.kind else {
            continue;
        };
        for (_, branch) in branches {
            if let Some(index) = branch.iter().position(|inner| inner.span.start == offset) {
                if branch.len() == 1 {
                    return Some(removal(program, statement.span.start, statement.span.end));
                }
                let inner = &branch[index];
                return Some(removal(program, inner.span.start, inner.span.end));
            }
        }
    }
    None
}

fn removal(program: &str, start: usize, end: usize) -> Replacement {
    let line_start = program[..start].rfind('\n').map_or(0, |index| index + 1);
    let line_end = program[end..]
        .find('\n')
        .map_or(program.len(), |index| end + index + 1);
    Replacement::new(line_start, line_end, "")
}

// ---- shared text helpers ------------------------------------------------

pub(crate) fn line_ending(program: &str) -> &'static str {
    if program.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

pub(crate) fn line_end(program: &str, offset: usize) -> usize {
    program[offset..]
        .find('\n')
        .map_or(program.len(), |index| offset + index + 1)
}

pub(crate) fn line_indent_at(program: &str, offset: usize) -> String {
    let line_start = program[..offset].rfind('\n').map_or(0, |index| index + 1);
    program[line_start..]
        .chars()
        .take_while(|character| *character == ' ' || *character == '\t')
        .collect()
}

/// The smallest positive indentation in the file, or four spaces.
pub(crate) fn indent_unit(program: &str) -> String {
    let mut best: Option<usize> = None;
    for line in program.lines() {
        let spaces = line
            .chars()
            .take_while(|character| *character == ' ')
            .count();
        if spaces > 0 && line.chars().nth(spaces).is_some_and(|c| c != ' ') {
            best = Some(best.map_or(spaces, |current| current.min(spaces)));
        }
        if line.starts_with('\t') {
            return "\t".to_owned();
        }
    }
    " ".repeat(best.unwrap_or(4))
}
