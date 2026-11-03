//! Folding overrides into the MDF program (`motioner bake`, D-0022/T-DSL-0006).
//!
//! Bake is pure: it takes the program text, the compiled document, the
//! top-level variable bindings, the value origins and the override set, and
//! returns the new program text plus the overrides it could not fold in. It
//! never writes files (the app does that, atomically).
//!
//! Two mechanisms, in this order:
//!
//! 1. **In place.** When the overridden value came from a private plain
//!    literal in the program (evaluated exactly once), the literal text is
//!    replaced and the override is dropped. A keyframe edits the arguments of
//!    its `.key(...)` call. This also reaches nodes created in loops and
//!    functions when their particular literal is private.
//! 2. **Generated block.** Anything else falls back to D-0022: nodes bound to
//!    a top-level variable get `var.property = value` statements appended
//!    after the marker; loop- and function-created nodes without a binding
//!    keep their overrides and a refusal reason.
//!
//! In-place edits shift the byte-offset provenance keys of later nodes, so
//! every remaining override is re-keyed by the exact edit offsets in the same
//! atomic write (D-0025). The result is verified by re-running the program:
//! when the semantics do not match, the in-place pass is discarded and the
//! old behaviour is used instead.

use std::collections::BTreeMap;

use motioner_core::{
    format_number, quote, seconds_of, CharValue, Document, Ease, EffectChannel, EffectParam,
    FrameRate, NodeContent, NodeId, OverrideAction, OverrideEntry, OverrideSet, OverrideValue,
    Property, TextAnimator, Transform, Vec2, LOCKED, NAME, VISIBLE,
};

use crate::diagnostic::Span;
use crate::origins::{Literal, ValueOrigin, ValueOrigins};

/// First line of the generated block.
pub const MARKER: &str = "# --- baked by motioner (do not edit above this line by hand) ---";

/// One override that was not baked, with the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BakeRefusal {
    /// Identity of the node the override targeted.
    pub id: String,
    /// Property of the override.
    pub property: String,
    /// Why it cannot be folded into the program.
    pub reason: String,
}

/// What [`bake`] produced.
#[derive(Debug, Clone, PartialEq)]
pub struct BakeOutcome {
    /// The program with the generated block appended and in-place edits
    /// applied (unchanged if nothing was baked).
    pub program: String,
    /// The overrides that remain in the overrides file, re-keyed to the new
    /// program when an in-place edit shifted the offsets.
    pub overrides: OverrideSet,
    /// Identities whose every override was baked.
    pub baked: Vec<String>,
    /// Overrides that could not be baked.
    pub refused: Vec<BakeRefusal>,
}

/// One literal replacement in the program.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PlannedEdit {
    span: Span,
    text: String,
}

/// Folds `overrides` into `program`, in place where that is exact.
///
/// `only` limits baking to those identities when it is not empty.
#[must_use]
pub fn bake(
    program: &str,
    document: &Document,
    bindings: &[(String, NodeId)],
    origins: &ValueOrigins,
    overrides: &OverrideSet,
    only: &[String],
) -> BakeOutcome {
    // What `program` plus every override produces, identities included.
    let mut expected = document.clone();
    overrides.apply(&mut expected);

    let hybrid = bake_once(program, document, bindings, origins, overrides, only, true);
    if is_exact(&hybrid, &expected) {
        return hybrid;
    }
    bake_once(program, document, bindings, origins, overrides, only, false)
}

/// Whether re-running the baked program with the remaining overrides gives the
/// expected composition (node identities shift with in-place edits by design,
/// so they are compared by position).
fn is_exact(outcome: &BakeOutcome, expected: &Document) -> bool {
    let rate = expected.composition().frame_rate();
    let overrides = outcome.overrides.to_mdf(rate);
    let Ok(compiled) = crate::run_with_overrides(
        &outcome.program,
        Some(&overrides),
        &crate::Limits::default(),
    ) else {
        return false;
    };
    if !compiled.warnings.is_empty() {
        return false;
    }
    semantics(&compiled.document) == semantics(expected)
}

/// A document's observable composition, ignoring provenance identities (which
/// an in-place edit shifts by design).
type Semantics = (
    NodeContent,
    Transform,
    motioner_core::BlendMode,
    Vec<motioner_core::Effect>,
    Option<motioner_core::MaskSpec>,
    bool,
    Option<usize>,
);

fn semantics(document: &Document) -> Vec<Semantics> {
    let index = |id: NodeId| document.nodes().position(|node| node.id() == id);
    document
        .nodes()
        .map(|node| {
            (
                node.content().clone(),
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

/// One bake pass; `in_place` selects the hybrid or the historical behaviour.
#[allow(clippy::too_many_lines)] // one pass over the entries, each with its fallback
fn bake_once(
    program: &str,
    document: &Document,
    bindings: &[(String, NodeId)],
    origins: &ValueOrigins,
    overrides: &OverrideSet,
    only: &[String],
    in_place: bool,
) -> BakeOutcome {
    let rate = document.composition().frame_rate();
    let mut statements = Vec::new();
    let mut edits: Vec<PlannedEdit> = Vec::new();
    let mut remaining = OverrideSet::new();
    let mut refused = Vec::new();
    let mut entries_baked: Vec<bool> = Vec::with_capacity(overrides.entries().len());

    for entry in overrides.entries() {
        if !only.is_empty() && !only.iter().any(|id| id == &entry.id) {
            remaining.push(entry.clone());
            entries_baked.push(false);
            continue;
        }
        if in_place {
            if let Some(planned) = plan_in_place(entry, document, origins) {
                if planned.iter().all(|edit| !conflicts(&edits, edit)) {
                    for edit in planned {
                        if !edits.iter().any(|existing| existing.span == edit.span) {
                            edits.push(edit);
                        }
                    }
                    entries_baked.push(true);
                    continue;
                }
            }
        }
        match bake_entry(document, bindings, entry, rate) {
            Ok(statement) => {
                statements.push(statement);
                entries_baked.push(true);
            }
            Err(reason) => {
                let reason = match in_place
                    .then(|| in_place_hint(entry, document, origins))
                    .flatten()
                {
                    Some(hint) => format!("{reason}; {hint}"),
                    None => reason,
                };
                refused.push(BakeRefusal {
                    id: entry.id.clone(),
                    property: entry.property.clone(),
                    reason,
                });
                remaining.push(entry.clone());
                entries_baked.push(false);
            }
        }
    }

    // Re-key every override that stays: the in-place edits shift the offsets
    // of anything created after them (D-0025).
    let remaining = rekey(remaining, &edits);
    let mut program = apply_edits(program, &edits);
    if !statements.is_empty() {
        program = append_block(&program, &statements);
    }

    let baked = fully_baked(overrides, &entries_baked);
    BakeOutcome {
        program,
        overrides: remaining,
        baked,
        refused,
    }
}

/// Identities for which every override entry was baked, in first-seen order.
fn fully_baked(overrides: &OverrideSet, entries_baked: &[bool]) -> Vec<String> {
    let mut total: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for (entry, baked) in overrides.entries().iter().zip(entries_baked) {
        let counters = total.entry(entry.id.as_str()).or_default();
        counters.0 += 1;
        counters.1 += usize::from(*baked);
    }
    let mut order = Vec::new();
    for entry in overrides.entries() {
        if total
            .get(entry.id.as_str())
            .is_some_and(|(count, baked)| count == baked)
            && !order.contains(&entry.id)
        {
            order.push(entry.id.clone());
        }
    }
    order
}

/// Two edits conflict when they overlap or replace the same span differently.
fn conflicts(edits: &[PlannedEdit], edit: &PlannedEdit) -> bool {
    edits.iter().any(|existing| {
        (existing.span == edit.span && existing.text != edit.text)
            || (existing.span.start < edit.span.end && edit.span.start < existing.span.end)
    })
}

/// The edits that fold one override in place, or `None` when it cannot be
/// done exactly.
fn plan_in_place(
    entry: &OverrideEntry,
    document: &Document,
    origins: &ValueOrigins,
) -> Option<Vec<PlannedEdit>> {
    if matches!(entry.property.as_str(), NAME | LOCKED) {
        return None;
    }
    let node = document.find(&entry.id)?;
    match &entry.action {
        OverrideAction::Set(value) => {
            let origin = origins.set_origin(node, &entry.property)?;
            if !origins.is_private(origin) {
                return None;
            }
            Some(vec![PlannedEdit {
                span: origin.span,
                text: set_replacement(origin, &entry.property, value)?,
            }])
        }
        OverrideAction::Key { frame, value, ease } => {
            let origin = origins.key_origin(node, &entry.property, *frame)?;
            if !origins.is_private(origin) {
                return None;
            }
            let mut planned = vec![PlannedEdit {
                span: origin.span,
                text: key_replacement(origin, value)?,
            }];
            match &origin.ease {
                Some(current) => {
                    if current.name != ease.mdf_value() {
                        planned.push(PlannedEdit {
                            span: current.span,
                            text: ease.mdf_literal(),
                        });
                    }
                }
                None if *ease != Ease::Linear => return None,
                None => {}
            }
            Some(planned)
        }
        OverrideAction::Visible(visible) => {
            let origin = origins.set_origin(node, VISIBLE)?;
            if !origins.is_private(origin) || origin.literal != Some(Literal::Bool) {
                return None;
            }
            Some(vec![PlannedEdit {
                span: origin.span,
                text: if *visible { "True" } else { "False" }.to_owned(),
            }])
        }
        OverrideAction::Locked(_) | OverrideAction::Unkey { .. } => None,
    }
}

/// A short explanation for the refusal when an origin existed but was not
/// private.
fn in_place_hint(
    entry: &OverrideEntry,
    document: &Document,
    origins: &ValueOrigins,
) -> Option<String> {
    let node = document.find(&entry.id)?;
    if matches!(entry.property.as_str(), NAME | LOCKED) {
        return None;
    }
    let origin = match &entry.action {
        OverrideAction::Key { frame, .. } => origins.key_origin(node, &entry.property, *frame),
        OverrideAction::Set(_) | OverrideAction::Visible(_) => {
            origins.set_origin(node, &entry.property)
        }
        OverrideAction::Locked(_) | OverrideAction::Unkey { .. } => None,
    }?;
    if origin.literal.is_none() {
        return Some(
            "the value is computed in the program, so it cannot be edited in place".to_owned(),
        );
    }
    if !origins.is_private(origin) {
        return Some(format!(
            "the literal is shared (evaluated {} times), so editing it would change other nodes",
            origins.count(origin)
        ));
    }
    None
}

/// The literal text for a static override.
fn set_replacement(origin: &ValueOrigin, property: &str, value: &OverrideValue) -> Option<String> {
    match (origin.literal?, value) {
        (Literal::Number, OverrideValue::Number(number)) => Some(format_number(*number)),
        (Literal::Text, OverrideValue::Text(text)) => Some(optional_none(property, text)),
        (Literal::None, OverrideValue::Text(text)) if is_optional(property) => {
            Some(optional_none(property, text))
        }
        (Literal::Pair, OverrideValue::Vec2(pair)) => Some(pair_text(*pair)),
        _ => None,
    }
}

/// The literal text for a key value.
fn key_replacement(origin: &ValueOrigin, value: &OverrideValue) -> Option<String> {
    match (origin.literal?, value) {
        (Literal::Number, OverrideValue::Number(number)) => Some(format_number(*number)),
        (Literal::Pair, OverrideValue::Vec2(pair)) => Some(pair_text(*pair)),
        (Literal::Text, OverrideValue::Text(text)) => Some(optional_none(&origin.property, text)),
        (Literal::None, OverrideValue::Text(text)) if is_optional(&origin.property) => {
            Some(optional_none(&origin.property, text))
        }
        _ => None,
    }
}

/// `None` for an optional colour field cleared with `"none"`, else the quoted
/// string.
fn optional_none(property: &str, text: &str) -> String {
    if is_optional(property) && text.eq_ignore_ascii_case("none") {
        "None".to_owned()
    } else {
        quote(text)
    }
}

/// Whether a content field accepts `None`.
fn is_optional(property: &str) -> bool {
    matches!(property, "fill" | "stroke")
}

fn pair_text(pair: Vec2) -> String {
    format!("({}, {})", format_number(pair.x), format_number(pair.y))
}

/// Applies the planned edits to the source, from the end backwards.
fn apply_edits(program: &str, edits: &[PlannedEdit]) -> String {
    let mut sorted = edits.to_vec();
    sorted.sort_by_key(|edit| edit.span.start);
    let mut out = String::with_capacity(program.len() + edits.len() * 4);
    let mut cursor = 0;
    for edit in sorted {
        let start = edit.span.start.min(program.len());
        let end = edit.span.end.min(program.len());
        if start < cursor || end < start {
            continue;
        }
        out.push_str(&program[cursor..start]);
        out.push_str(&edit.text);
        cursor = end;
    }
    out.push_str(&program[cursor..]);
    out
}

/// Re-keys an override set after in-place edits shifted the byte offsets.
fn rekey(set: OverrideSet, edits: &[PlannedEdit]) -> OverrideSet {
    if edits.is_empty() {
        return set;
    }
    let mut out = OverrideSet::new();
    for entry in set.entries() {
        let mut entry = entry.clone();
        entry.id = shift_identity(&entry.id, edits);
        out.push(entry);
    }
    out
}

/// Shifts the byte offsets embedded in a provenance key. Explicit identities
/// (`.id("name")`) are unchanged.
fn shift_identity(id: &str, edits: &[PlannedEdit]) -> String {
    let mut rest = id;
    let mut out = String::with_capacity(id.len());
    if let Some(after) = rest.strip_prefix('@') {
        let (digits, tail) = split_digits(after);
        let Ok(offset) = digits.parse::<usize>() else {
            return id.to_owned();
        };
        out.push('@');
        out.push_str(&shift_offset(offset, edits).to_string());
        rest = tail;
    }
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("/c") {
            let (digits, tail) = split_digits(after);
            let Ok(offset) = digits.parse::<usize>() else {
                return id.to_owned();
            };
            out.push_str("/c");
            out.push_str(&shift_offset(offset, edits).to_string());
            rest = tail;
        } else if let Some(after) = rest.strip_prefix("/i") {
            let (digits, tail) = split_digits(after);
            out.push_str("/i");
            out.push_str(digits);
            rest = tail;
        } else {
            // Not a provenance key (an explicit `.id()`); leave it alone.
            return id.to_owned();
        }
    }
    out
}

fn split_digits(text: &str) -> (&str, &str) {
    let digits = text
        .as_bytes()
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    text.split_at(digits)
}

/// The offset an edit moved an original byte position to.
fn shift_offset(offset: usize, edits: &[PlannedEdit]) -> usize {
    let mut value = offset.cast_signed();
    for edit in edits {
        if edit.span.end <= offset {
            let old = edit.span.end.saturating_sub(edit.span.start);
            value += edit.text.len().cast_signed() - old.cast_signed();
        }
    }
    usize::try_from(value).unwrap_or(offset)
}

/// Folds one override into the generated block (D-0022 behaviour).
fn bake_entry(
    document: &Document,
    bindings: &[(String, NodeId)],
    entry: &OverrideEntry,
    rate: FrameRate,
) -> Result<String, String> {
    if entry.property == NAME {
        return Err("display names are editor-only and have no program statement".to_owned());
    }
    let node = document
        .find(&entry.id)
        .ok_or_else(|| "no node has this identity (orphan)".to_owned())?;
    let Some(variable) = bindings
        .iter()
        .find(|(_, id)| *id == node)
        .map(|(name, _)| name.as_str())
    else {
        return Err(
            "the node is not bound to a top-level variable (created in a loop or a function)"
                .to_owned(),
        );
    };
    match &entry.action {
        OverrideAction::Set(value) => {
            if entry.property == motioner_core::EFFECTS {
                let OverrideValue::Text(list) = value else {
                    return Err("'effects' expects a list of effect kinds".to_owned());
                };
                return Ok(format!("{variable}.effect_stack({})", quote(list)));
            }
            if entry.property == motioner_core::MASK {
                let OverrideValue::Text(text) = value else {
                    return Err("'mask' expects '<source>:<mode>:<invert>'".to_owned());
                };
                return mask_set_statement(document, bindings, variable, text);
            }
            if entry.property == TextAnimator::OVERRIDE_PROPERTY {
                let OverrideValue::Text(text) = value else {
                    return Err("'text_animators' expects the encoded animator list".to_owned());
                };
                return text_animators_statement(variable, text, rate);
            }
            if let Some(channel) = EffectChannel::from_name(&entry.property) {
                return effect_set_statement(document, node, variable, channel, value);
            }
            Ok(format!(
                "{variable}.{} = {}",
                entry.property,
                set_value_text(&entry.property, value)
            ))
        }
        OverrideAction::Key { frame, value, ease } => {
            let (base, axis) = motioner_core::split_property(&entry.property);
            if Property::from_name(base).is_none()
                && !motioner_core::ContentField::from_name(base)
                    .is_some_and(motioner_core::ContentField::is_animatable)
                && EffectChannel::from_name(&entry.property).is_none()
            {
                return Err(format!(
                    "'{}' is not an animatable transform property, content field or effect parameter",
                    entry.property
                ));
            }
            if axis.is_some()
                && !Property::from_name(base).is_some_and(motioner_core::Property::is_vector)
            {
                return Err(format!("'{}' has no axes", entry.property));
            }
            let time = format_number(seconds_of(*frame, rate));
            let ease = if *ease == motioner_core::Ease::Linear {
                String::new()
            } else {
                format!(", ease={}", ease.mdf_literal())
            };
            Ok(format!(
                "{variable}.key({}, {time}, {}{ease})",
                quote(&entry.property),
                value_text(value)
            ))
        }
        OverrideAction::Unkey { .. } => Err(
            "bake cannot remove a keyframe from the program; edit the program instead".to_owned(),
        ),
        OverrideAction::Visible(visible) => Ok(format!(
            "{variable}.visible = {}",
            if *visible { "True" } else { "False" }
        )),
        OverrideAction::Locked(_) => {
            Err("locks are editor-only and have no program statement".to_owned())
        }
    }
}

/// `var.mask(source[, mode="luma"][, invert=True])` for a mask override.
///
/// A cleared mask writes `var.mask(None)`, the explicit form of "this node is
/// not masked".  The source has to be a top-level variable for the same
/// reason every other reference does: the program must be able to name it.
fn mask_set_statement(
    document: &Document,
    bindings: &[(String, NodeId)],
    variable: &str,
    text: &str,
) -> Result<String, String> {
    let (source_identity, mode, invert) = match motioner_core::MaskSpec::parse_override(text) {
        Ok(None) => return Ok(format!("{variable}.mask(None)")),
        Ok(Some(parsed)) => parsed,
        Err(reason) => return Err(format!("'mask' cannot take {text:?}: {reason}")),
    };
    let source = document
        .find(source_identity)
        .ok_or_else(|| format!("no node has the identity {source_identity:?}"))?;
    let Some(source_variable) = bindings
        .iter()
        .find(|(_, id)| *id == source)
        .map(|(name, _)| name.as_str())
    else {
        return Err(
            "the mask source is not bound to a top-level variable (created in a loop or a function)"
                .to_owned(),
        );
    };
    let mut call = format!("{variable}.mask({source_variable}");
    if mode != motioner_core::MaskMode::Alpha {
        use std::fmt::Write as _;
        let _ = write!(call, ", mode={}", quote(mode.name()));
    }
    if invert {
        call.push_str(", invert=True");
    }
    call.push(')');
    Ok(call)
}

/// `var.clear_chars()` plus one `var.animate_chars(...)` per animator.
///
/// The override replaces the whole list, so the first statement clears whatever
/// the program authored before appending the baked animators.
fn text_animators_statement(variable: &str, text: &str, rate: FrameRate) -> Result<String, String> {
    let animators = TextAnimator::decode_list(text)
        .map_err(|reason| format!("'text_animators' cannot take {text:?}: {reason}"))?;
    let mut out = format!("{variable}.clear_chars()");
    for animator in animators {
        use std::fmt::Write as _;
        let _ = write!(
            out,
            "\n{variable}.animate_chars({}, {}, {}, {}, {}, {}, ease={}, order={})",
            quote(animator.property.name()),
            char_value_text(animator.from),
            char_value_text(animator.to),
            format_number(seconds_of(animator.start, rate)),
            format_number(seconds_f64(animator.stagger, rate)),
            format_number(seconds_f64(animator.duration, rate)),
            quote(animator.ease.name()),
            quote(animator.order.name())
        );
    }
    Ok(out)
}

/// Seconds of a frame count expressed as `f64` (stagger and duration).
fn seconds_f64(frames: f64, rate: FrameRate) -> f64 {
    frames * f64::from(rate.denominator()) / f64::from(rate.numerator())
}

/// An animator endpoint as MDF source text.
fn char_value_text(value: CharValue) -> String {
    match value {
        CharValue::Number(number) => format_number(number),
        CharValue::Vec2(vector) => {
            format!("({}, {})", format_number(vector.x), format_number(vector.y))
        }
        CharValue::Color(color) => quote(&color.to_hex()),
    }
}

/// `var.effect("kind", param=value)` for an effect parameter override.
///
/// An offset axis is folded into the pair with the other axis read from the
/// baked document, because the DSL takes `offset=(x, y)` as one value.
fn effect_set_statement(
    document: &Document,
    node: NodeId,
    variable: &str,
    channel: EffectChannel,
    value: &OverrideValue,
) -> Result<String, String> {
    let kind = quote(channel.kind.name());
    if channel.param == EffectParam::Offset {
        let OverrideValue::Number(number) = value else {
            return Err(format!("'{}' expects a number", channel.name()));
        };
        let other_axis = if channel.axis == Some(motioner_core::Axis::X) {
            motioner_core::Axis::Y
        } else {
            motioner_core::Axis::X
        };
        let other = document
            .node(node)
            .and_then(|node| node.effect(channel.kind))
            .and_then(|effect| effect.channel_ref(EffectParam::Offset, Some(other_axis)))
            .map_or(0.0, |channel| match channel {
                motioner_core::EffectChannelRef::Number(value) => value.value(),
                motioner_core::EffectChannelRef::Color(_) => 0.0,
            });
        let pair = if channel.axis == Some(motioner_core::Axis::X) {
            (*number, other)
        } else {
            (other, *number)
        };
        return Ok(format!(
            "{variable}.effect({kind}, offset=({}, {}))",
            format_number(pair.0),
            format_number(pair.1)
        ));
    }
    let value = match value {
        OverrideValue::Text(text) => quote(text),
        OverrideValue::Number(number) => format_number(*number),
        OverrideValue::Vec2(pair) => pair_text(*pair),
    };
    Ok(format!(
        "{variable}.effect({kind}, {}={value})",
        channel.param.name()
    ))
}

fn set_value_text(property: &str, value: &OverrideValue) -> String {
    if let OverrideValue::Text(text) = value {
        if is_optional(property) && text.eq_ignore_ascii_case("none") {
            return "None".to_owned();
        }
    }
    value_text(value)
}

fn value_text(value: &OverrideValue) -> String {
    match value {
        OverrideValue::Number(n) => format_number(*n),
        OverrideValue::Vec2(v) => pair_text(*v),
        OverrideValue::Text(text) => quote(text),
    }
}

fn append_block(program: &str, statements: &[String]) -> String {
    let mut out = String::with_capacity(program.len() + 256);
    out.push_str(program.trim_end());
    out.push_str("\n\n");
    out.push_str(MARKER);
    out.push('\n');
    for statement in statements {
        out.push_str(statement);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use motioner_core::{Ease, FrameRate, OverrideValue, Vec2};

    const HELLO: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/hello.motion"
    ));

    /// Compiles `source` and keeps the bindings and origins bake needs.
    fn compile(source: &str) -> crate::Compiled {
        let program = crate::parse(source).unwrap();
        let execution = crate::execute_detailed(&program, &crate::Limits::default()).unwrap();
        crate::Compiled {
            document: execution.document,
            overrides: OverrideSet::new(),
            warnings: Vec::new(),
            bindings: execution.bindings,
            origins: execution.origins,
        }
    }

    /// `hello.motion` with the first spinner bound to a variable, so bake can
    /// name it. The example itself is never modified.
    fn hello_with_bound_spinner() -> String {
        HELLO.replace(
            "spinner(240, 220, 96, 1.0)",
            "spinner1 = spinner(240, 220, 96, 1.0)",
        )
    }

    fn compiled() -> crate::Compiled {
        compile(HELLO)
    }

    fn rate() -> FrameRate {
        FrameRate::new(60, 1).unwrap()
    }

    fn overrides(entries: &[(&str, &str, OverrideAction)]) -> OverrideSet {
        let mut set = OverrideSet::new();
        for (id, property, action) in entries {
            set.insert(OverrideEntry::new(*id, *property, action.clone()));
        }
        set
    }

    fn id_of(compiled: &crate::Compiled, node: NodeId) -> String {
        compiled
            .document
            .node(node)
            .unwrap()
            .identity()
            .as_str()
            .to_owned()
    }

    fn title(compiled: &crate::Compiled) -> String {
        id_of(compiled, compiled.document.roots()[0])
    }

    /// The document the program plus every override produces.
    fn expected(compiled: &crate::Compiled, set: &OverrideSet) -> Document {
        let mut document = compiled.document.clone();
        set.apply(&mut document);
        document
    }

    #[test]
    fn bake_is_exact_for_a_bound_title_and_group() {
        let source = hello_with_bound_spinner();
        let compiled = compile(&source);
        let title_id = title(&compiled);
        let rect = compiled
            .bindings
            .iter()
            .find(|(name, _)| name == "spinner1")
            .map(|(_, id)| *id)
            .expect("hello.motion binds the spinner group");
        let rect_id = id_of(&compiled, rect);

        let set = overrides(&[
            (
                &title_id,
                "pos",
                OverrideAction::Set(OverrideValue::Vec2(Vec2::new(960.0, 500.0))),
            ),
            (
                &title_id,
                "opacity",
                OverrideAction::Key {
                    frame: 60,
                    value: OverrideValue::Number(0.5),
                    ease: Ease::QuadOut,
                },
            ),
            (
                &rect_id,
                "scale",
                OverrideAction::Set(OverrideValue::Number(1.25)),
            ),
            (&rect_id, "visible", OverrideAction::Visible(false)),
        ]);
        let before = expected(&compiled, &set);
        let baked = bake(
            &source,
            &compiled.document,
            &compiled.bindings,
            &compiled.origins,
            &set,
            &[],
        );
        assert!(baked.refused.is_empty(), "{baked:?}");
        assert!(baked.overrides.is_empty(), "{baked:?}");
        let actual = crate::run(&baked.program, &crate::Limits::default()).unwrap();
        assert_eq!(semantics(&actual), semantics(&before));
    }

    #[test]
    fn private_literals_are_edited_in_place() {
        let compiled = compiled();
        let title_id = title(&compiled);
        let set = overrides(&[
            (
                &title_id,
                "size",
                OverrideAction::Set(OverrideValue::Number(220.0)),
            ),
            (
                &title_id,
                "color",
                OverrideAction::Set(OverrideValue::Text("#00FF88".to_owned())),
            ),
        ]);
        let before = expected(&compiled, &set);
        let baked = bake(
            HELLO,
            &compiled.document,
            &compiled.bindings,
            &compiled.origins,
            &set,
            &[],
        );
        assert!(baked.overrides.is_empty(), "{baked:?}");
        assert!(baked.program.contains("size=220"), "{}", baked.program);
        assert!(baked.program.contains("color=\"#00FF88\""));
        assert!(!baked.program.contains(MARKER), "nothing was appended");
        let actual = crate::run(&baked.program, &crate::Limits::default()).unwrap();
        assert_eq!(semantics(&actual), semantics(&before));
    }

    const EFFECTS_SOURCE: &str = "comp(name=\"T\", width=200, height=100, fps=60, duration=2.0)\ncard = rect(40, 40)\ncard.effect(\"drop_shadow\", opacity=0.5, blur=4)\ncard.effect(\"blur\", radius=2)\n";

    #[test]
    fn bake_folds_effect_parameters_and_the_stack() {
        let compiled = compile(EFFECTS_SOURCE);
        let id = id_of(&compiled, compiled.document.roots()[0]);
        let set = overrides(&[
            (
                &id,
                "blur.radius",
                OverrideAction::Set(OverrideValue::Number(6.0)),
            ),
            (
                &id,
                "drop_shadow.offset.x",
                OverrideAction::Set(OverrideValue::Number(7.0)),
            ),
            (
                &id,
                motioner_core::EFFECTS,
                OverrideAction::Set(OverrideValue::Text("glow,blur".to_owned())),
            ),
        ]);
        let before = expected(&compiled, &set);
        let baked = bake(
            EFFECTS_SOURCE,
            &compiled.document,
            &compiled.bindings,
            &compiled.origins,
            &set,
            &[],
        );
        assert!(baked.overrides.is_empty(), "{baked:?}");
        assert!(
            baked.program.contains("effect_stack(\"glow,blur\")"),
            "{}",
            baked.program
        );
        assert!(
            baked.program.contains("effect(\"blur\", radius=6)"),
            "{}",
            baked.program
        );
        let actual = crate::run(&baked.program, &crate::Limits::default()).unwrap();
        assert_eq!(semantics(&actual), semantics(&before));
    }

    #[test]
    fn bake_folds_a_keyed_effect_parameter() {
        let compiled = compile(EFFECTS_SOURCE);
        let id = id_of(&compiled, compiled.document.roots()[0]);
        let set = overrides(&[(
            &id,
            "blur.radius",
            OverrideAction::Key {
                frame: 60,
                value: OverrideValue::Number(9.0),
                ease: Ease::Linear,
            },
        )]);
        let before = expected(&compiled, &set);
        let baked = bake(
            EFFECTS_SOURCE,
            &compiled.document,
            &compiled.bindings,
            &compiled.origins,
            &set,
            &[],
        );
        assert!(baked.overrides.is_empty(), "{baked:?}");
        assert!(
            baked.program.contains("key(\"blur.radius\", 1, 9)"),
            "{}",
            baked.program
        );
        let actual = crate::run(&baked.program, &crate::Limits::default()).unwrap();
        assert_eq!(semantics(&actual), semantics(&before));
    }

    const MASKS_SOURCE: &str = "comp(name=\"T\", width=200, height=100, fps=60, duration=2.0)\ncard = rect(40, 40)\ncutout = rect(20, 20)\n";

    #[test]
    fn bake_folds_a_mask_and_its_clear() {
        let compiled = compile(MASKS_SOURCE);
        let card = id_of(&compiled, compiled.document.roots()[0]);
        let cutout = id_of(&compiled, compiled.document.roots()[1]);
        let set = overrides(&[(
            &card,
            motioner_core::MASK,
            OverrideAction::Set(OverrideValue::Text(format!("{cutout}:luma:true"))),
        )]);
        let before = expected(&compiled, &set);
        let baked = bake(
            MASKS_SOURCE,
            &compiled.document,
            &compiled.bindings,
            &compiled.origins,
            &set,
            &[],
        );
        assert!(baked.overrides.is_empty(), "{baked:?}");
        assert!(
            baked
                .program
                .contains("card.mask(cutout, mode=\"luma\", invert=True)"),
            "{}",
            baked.program
        );
        let actual = crate::run(&baked.program, &crate::Limits::default()).unwrap();
        assert_eq!(semantics(&actual), semantics(&before));

        // Clearing an existing mask folds as the explicit `mask(None)`.
        let masked = format!("{MASKS_SOURCE}card.mask(cutout, mode=\"luma\", invert=True)\n");
        let compiled = compile(&masked);
        let card = id_of(&compiled, compiled.document.roots()[0]);
        let set = overrides(&[(
            &card,
            motioner_core::MASK,
            OverrideAction::Set(OverrideValue::Text("none".to_owned())),
        )]);
        let before = expected(&compiled, &set);
        let baked = bake(
            &masked,
            &compiled.document,
            &compiled.bindings,
            &compiled.origins,
            &set,
            &[],
        );
        assert!(baked.overrides.is_empty(), "{baked:?}");
        assert!(
            baked.program.contains("card.mask(None)"),
            "{}",
            baked.program
        );
        let actual = crate::run(&baked.program, &crate::Limits::default()).unwrap();
        assert_eq!(semantics(&actual), semantics(&before));
    }

    #[test]
    fn a_shared_loop_literal_is_refused() {
        let compiled = compiled();
        let circle = compiled.document.roots()[1];
        let circle_id = id_of(&compiled, circle);
        let set = overrides(&[(
            &circle_id,
            "radius",
            OverrideAction::Set(OverrideValue::Number(30.0)),
        )]);
        let baked = bake(
            HELLO,
            &compiled.document,
            &compiled.bindings,
            &compiled.origins,
            &set,
            &[],
        );
        assert_eq!(baked.program, HELLO, "the loop body is untouched");
        assert_eq!(baked.refused.len(), 1, "{baked:?}");
        assert!(baked.refused[0].reason.contains("shared"), "{baked:?}");
        assert_eq!(baked.overrides.len(), 1);
    }

    #[test]
    fn a_key_value_and_ease_are_edited_in_place() {
        let compiled = compiled();
        let title_id = title(&compiled);
        let set = overrides(&[(
            &title_id,
            "opacity",
            OverrideAction::Key {
                frame: 48,
                value: OverrideValue::Number(0.25),
                ease: Ease::BackOut,
            },
        )]);
        let before = expected(&compiled, &set);
        let baked = bake(
            HELLO,
            &compiled.document,
            &compiled.bindings,
            &compiled.origins,
            &set,
            &[],
        );
        assert!(baked.overrides.is_empty(), "{baked:?}");
        assert!(
            baked
                .program
                .contains("key(\"opacity\", 0.8, 0.25, ease=\"back_out\")"),
            "{}",
            baked.program
        );
        let actual = crate::run(&baked.program, &crate::Limits::default()).unwrap();
        assert_eq!(semantics(&actual), semantics(&before));
    }

    #[test]
    fn shifting_an_in_place_edit_rekeys_the_remaining_overrides() {
        let compiled = compiled();
        let title_id = title(&compiled);
        let circle = compiled.document.roots()[1];
        let circle_id = id_of(&compiled, circle);
        // The title's size grows by one byte; every later key shifts, and the
        // circle's rotation override must follow it.
        let set = overrides(&[
            (
                &title_id,
                "size",
                OverrideAction::Set(OverrideValue::Number(1000.0)),
            ),
            (
                &circle_id,
                "rotation",
                OverrideAction::Set(OverrideValue::Number(45.0)),
            ),
        ]);
        let before = expected(&compiled, &set);
        let baked = bake(
            HELLO,
            &compiled.document,
            &compiled.bindings,
            &compiled.origins,
            &set,
            &[],
        );
        assert!(baked.program.contains("size=1000"), "{}", baked.program);
        let remaining = baked.overrides.to_mdf(rate());
        assert!(
            remaining.contains("override(\"@") && remaining.contains("\"rotation\", 45"),
            "the circle's override stays, re-keyed: {remaining}"
        );
        let left = baked.overrides.len();
        assert!(left >= 1);
        let actual =
            crate::run_with_overrides(&baked.program, Some(&remaining), &crate::Limits::default())
                .unwrap();
        assert_eq!(semantics(&actual.document), semantics(&before));
    }

    #[test]
    fn bake_is_idempotent() {
        let compiled = compiled();
        let title_id = title(&compiled);
        let set = overrides(&[(
            &title_id,
            "rotation",
            OverrideAction::Set(OverrideValue::Number(12.0)),
        )]);
        let first = bake(
            HELLO,
            &compiled.document,
            &compiled.bindings,
            &compiled.origins,
            &set,
            &[],
        );
        assert!(first.overrides.is_empty());
        let recompiled = compile(&first.program);
        let second = bake(
            &first.program,
            &recompiled.document,
            &recompiled.bindings,
            &recompiled.origins,
            &first.overrides,
            &[],
        );
        assert_eq!(
            second.program, first.program,
            "baking twice changes nothing"
        );
        assert!(second.refused.is_empty());
    }

    #[test]
    fn loop_created_nodes_are_refused_with_a_reason() {
        let source = "comp(name=\"L\", duration=2.0)\nfor i in range(3):\n    d = circle(radius=8)\n    d.pos = (10 + i * 20, 50)\n";
        let program = crate::parse(source).unwrap();
        let execution = crate::execute_detailed(&program, &crate::Limits::default()).unwrap();
        let circle = execution.document.roots()[0];
        let id = id_of(
            &crate::Compiled {
                document: execution.document.clone(),
                overrides: OverrideSet::new(),
                warnings: Vec::new(),
                bindings: execution.bindings.clone(),
                origins: execution.origins.clone(),
            },
            circle,
        );
        let set = overrides(&[(
            &id,
            "pos",
            OverrideAction::Set(OverrideValue::Vec2(Vec2::new(1.0, 2.0))),
        )]);
        let baked = bake(
            source,
            &execution.document,
            &execution.bindings,
            &execution.origins,
            &set,
            &[],
        );
        assert_eq!(baked.program, source, "nothing was appended");
        assert_eq!(baked.refused.len(), 1);
        assert!(baked.refused[0].reason.contains("top-level variable"));
        assert_eq!(baked.overrides.len(), 1, "the override is kept");
    }

    #[test]
    fn only_limits_the_bake_to_named_ids() {
        let compiled = compiled();
        let a = title(&compiled);
        let (other, other_id) = compiled.bindings[0].clone();
        let _ = other;
        let other_id = id_of(&compiled, other_id);
        let set = overrides(&[
            (
                &a,
                "rotation",
                OverrideAction::Set(OverrideValue::Number(1.0)),
            ),
            (
                &other_id,
                "opacity",
                OverrideAction::Set(OverrideValue::Number(0.25)),
            ),
        ]);
        let baked = bake(
            HELLO,
            &compiled.document,
            &compiled.bindings,
            &compiled.origins,
            &set,
            std::slice::from_ref(&a),
        );
        assert_eq!(baked.baked, vec![a]);
        assert_eq!(baked.overrides.len(), 1);
    }

    #[test]
    fn axis_keys_bake_exactly() {
        let compiled = compiled();
        let title_id = title(&compiled);
        let set = overrides(&[(
            &title_id,
            "pos.x",
            OverrideAction::Key {
                frame: 60,
                value: OverrideValue::Number(500.0),
                ease: Ease::QuadOut,
            },
        )]);
        let baked = bake(
            HELLO,
            &compiled.document,
            &compiled.bindings,
            &compiled.origins,
            &set,
            &[],
        );
        assert!(baked.refused.is_empty(), "{baked:?}");
        assert!(
            baked.program.contains(".key(\"pos.x\", 1, 500"),
            "{}",
            baked.program
        );
        let before = expected(&compiled, &set);
        let actual = crate::run(&baked.program, &crate::Limits::default()).unwrap();
        assert_eq!(semantics(&actual), semantics(&before));
    }

    #[test]
    fn names_and_unkeys_are_refused() {
        let compiled = compiled();
        let title_id = title(&compiled);
        for action in [
            (
                NAME,
                OverrideAction::Set(OverrideValue::Text("Title".to_owned())),
            ),
            ("rotation", OverrideAction::Unkey { frame: 10 }),
        ] {
            let set = overrides(&[(&title_id, action.0, action.1)]);
            let baked = bake(
                HELLO,
                &compiled.document,
                &compiled.bindings,
                &compiled.origins,
                &set,
                &[],
            );
            assert_eq!(baked.refused.len(), 1, "{baked:?}");
            assert_eq!(baked.overrides.len(), 1);
            assert!(baked.program == HELLO);
        }
    }

    #[test]
    fn a_baked_block_carries_the_marker() {
        let compiled = compiled();
        let (other, other_id) = compiled.bindings[0].clone();
        let _ = other;
        let other_id = id_of(&compiled, other_id);
        let set = overrides(&[(
            &other_id,
            "fill",
            OverrideAction::Set(OverrideValue::Text("#123456".to_owned())),
        )]);
        let baked = bake(
            HELLO,
            &compiled.document,
            &compiled.bindings,
            &compiled.origins,
            &set,
            &[],
        );
        // A private literal is edited in place; a computed or shared one is
        // appended. Either way nothing is left over and the result is exact.
        assert!(baked.overrides.is_empty(), "{baked:?}");
        let before = expected(&compiled, &set);
        let actual = crate::run(&baked.program, &crate::Limits::default()).unwrap();
        assert_eq!(semantics(&actual), semantics(&before));
        let _ = rate();
    }

    #[test]
    fn content_keys_are_baked_in_place() {
        let source = "comp(name=\"C\", duration=2.0)\n\
                      r = rect(10, 10, 0, fill=\"#FF0000\")\n\
                      r.key(\"fill\", 1.0, \"#00FF00\", ease=\"quad_out\")\n\
                      r.key(\"width\", 1.0, 40)\n";
        let compiled = compile(source);
        let id = id_of(&compiled, compiled.document.roots()[0]);
        let set = overrides(&[
            (
                &id,
                "fill",
                OverrideAction::Key {
                    frame: 60,
                    value: OverrideValue::Text("#0000FF".to_owned()),
                    ease: Ease::CubicOut,
                },
            ),
            (
                &id,
                "width",
                OverrideAction::Key {
                    frame: 60,
                    value: OverrideValue::Number(80.0),
                    ease: Ease::Linear,
                },
            ),
        ]);
        let before = expected(&compiled, &set);
        let baked = bake(
            source,
            &compiled.document,
            &compiled.bindings,
            &compiled.origins,
            &set,
            &[],
        );
        assert!(baked.refused.is_empty(), "{baked:?}");
        assert!(baked.overrides.is_empty(), "{baked:?}");
        assert!(baked.program.contains("\"#0000FF\""), "{}", baked.program);
        assert!(
            baked.program.contains("key(\"width\", 1.0, 80"),
            "{}",
            baked.program
        );
        let actual = crate::run(&baked.program, &crate::Limits::default()).unwrap();
        assert_eq!(semantics(&actual), semantics(&before));
    }

    #[test]
    fn a_path_trim_key_is_baked_in_place() {
        let source = "comp(\"P\", 100, 100, 60, 2.0, \"#000000\")\n\
                      p = path([(0, 0), (50, 0)], stroke=\"#FFFFFF\")\n\
                      p.id(\"p\")\n\
                      p.key(\"trim_end\", 0.0, 0.0)\n\
                      p.key(\"trim_end\", 1.0, 1.0, ease=\"cubic_out\")\n";
        let compiled = compile(source);
        let set = overrides(&[(
            "p",
            "trim_end",
            OverrideAction::Key {
                frame: 0,
                value: OverrideValue::Number(0.25),
                ease: Ease::Linear,
            },
        )]);
        let before = expected(&compiled, &set);
        let baked = bake(
            source,
            &compiled.document,
            &compiled.bindings,
            &compiled.origins,
            &set,
            &[],
        );
        assert!(baked.overrides.is_empty(), "{baked:?}");
        assert!(
            baked.program.contains("key(\"trim_end\", 0.0, 0.25"),
            "{}",
            baked.program
        );
        let actual = crate::run(&baked.program, &crate::Limits::default()).unwrap();
        assert_eq!(semantics(&actual), semantics(&before));
    }
}
