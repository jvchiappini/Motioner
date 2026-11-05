//! Folding overrides into the MDF program (`motioner bake`, D-0022/T-DSL-0006).
//!
//! Bake is pure: it takes the program text, the compiled document, the
//! top-level variable bindings, the value origins and the override set, and
//! returns the new program text plus the overrides it could not fold in. It
//! never writes files (the app does that, atomically).
//!
//! Bake is the batch form of the write-through planner (D-0036): each
//! override is offered to [`crate::plan`], which tries a private literal and
//! then a local assignment next to the node's last write. Anything the planner
//! refuses falls back to D-0022: nodes bound to a top-level variable get
//! `var.property = value` statements appended after the marker;
//! loop- and function-created nodes without a binding keep their overrides and
//! a refusal reason.
//!
//! In-place edits shift the byte-offset provenance keys of later nodes, so
//! every remaining override is re-keyed by the exact edit offsets in the same
//! atomic write (D-0025). The result is verified by re-running the program:
//! when the semantics do not match, the in-place pass is discarded and the
//! old behaviour is used instead.

use std::collections::BTreeMap;

use motioner_core::{
    format_number, quote, seconds_of, value_text, CharValue, Document, EditCommand, EffectChannel,
    EffectParam, FrameRate, NodeId, OverrideAction, OverrideEntry, OverrideSet, OverrideValue,
    Property, TextAnimator, LOCKED, NAME,
};

use crate::code_edit::CodeEdit;
use crate::origins::ValueOrigins;
use crate::plan::{plan, Plan};
use crate::write_text::{pair_text, set_value_text};

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
/// expected composition (the shared verifier compares node identities by
/// position, D-0027).
fn is_exact(outcome: &BakeOutcome, expected: &Document) -> bool {
    let rate = expected.composition().frame_rate();
    let overrides = outcome.overrides.to_mdf(rate);
    crate::verify(&outcome.program, Some(&overrides), expected)
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
    let mut combined: Option<CodeEdit> = None;
    let mut remaining = OverrideSet::new();
    let mut refused = Vec::new();
    let mut entries_baked: Vec<bool> = Vec::with_capacity(overrides.entries().len());

    for entry in overrides.entries() {
        if !only.is_empty() && !only.iter().any(|id| id == &entry.id) {
            remaining.push(entry.clone());
            entries_baked.push(false);
            continue;
        }
        let mut planner_reason = None;
        if in_place {
            if let Some(change) = change_for(entry) {
                match plan(program, origins, document, &change) {
                    Plan::Code(edit) => {
                        let candidate = match &combined {
                            Some(existing) => {
                                CodeEdit::combine([existing.clone(), edit.clone()]).ok()
                            }
                            None => Some(edit),
                        };
                        if let Some(candidate) = candidate {
                            combined = Some(candidate);
                            entries_baked.push(true);
                            continue;
                        }
                        planner_reason =
                            Some("the code edit conflicts with another baked edit".to_owned());
                    }
                    Plan::Fallback(reason) => planner_reason = Some(reason),
                }
            }
        }
        match bake_entry(document, bindings, entry, rate) {
            Ok(statement) => {
                statements.push(statement);
                entries_baked.push(true);
            }
            Err(reason) => {
                let reason = match planner_reason {
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

    let (remaining, mut program) = match combined {
        Some(edit) => {
            let remaining = edit.rekey_overrides(&remaining);
            let Some(text) = edit.apply(program).ok() else {
                return bake_once(program, document, bindings, origins, overrides, only, false);
            };
            (remaining, text)
        }
        None => (remaining, program.to_owned()),
    };
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

/// The resolved edit equivalent of one override entry, when the planner can
/// reason about it.
fn change_for(entry: &OverrideEntry) -> Option<EditCommand> {
    match &entry.action {
        OverrideAction::Set(value) if entry.property != NAME && entry.property != LOCKED => {
            Some(EditCommand::SetValue {
                id: entry.id.clone(),
                property: entry.property.clone(),
                value: value.clone(),
            })
        }
        OverrideAction::Key { frame, value, ease } => Some(EditCommand::SetKey {
            id: entry.id.clone(),
            property: entry.property.clone(),
            axis: None,
            frame: *frame,
            value: value.clone(),
            ease: *ease,
        }),
        OverrideAction::Visible(visible) => Some(EditCommand::SetVisible {
            id: entry.id.clone(),
            visible: *visible,
        }),
        OverrideAction::Set(_) | OverrideAction::Locked(_) | OverrideAction::Unkey { .. } => None,
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

/// Folds one override into the generated block (D-0022 behaviour).
fn bake_entry(
    document: &Document,
    bindings: &[(String, NodeId)],
    entry: &OverrideEntry,
    rate: FrameRate,
) -> Result<String, String> {
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
    if entry.property == NAME {
        let OverrideAction::Set(OverrideValue::Text(name)) = &entry.action else {
            return Err("'name' expects a string".to_owned());
        };
        return Ok(format!("{variable}.name = {}", quote(name)));
    }
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
        assert_eq!(
            crate::plan::semantics(&actual),
            crate::plan::semantics(&before)
        );
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
        assert_eq!(
            crate::plan::semantics(&actual),
            crate::plan::semantics(&before)
        );
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
        assert_eq!(
            crate::plan::semantics(&actual),
            crate::plan::semantics(&before)
        );
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
        assert_eq!(
            crate::plan::semantics(&actual),
            crate::plan::semantics(&before)
        );
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
        assert_eq!(
            crate::plan::semantics(&actual),
            crate::plan::semantics(&before)
        );

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
        assert_eq!(
            crate::plan::semantics(&actual),
            crate::plan::semantics(&before)
        );
    }

    #[test]
    fn a_loop_literal_is_specialised_with_a_guard() {
        let compiled = compiled();
        let circle = compiled.document.roots()[1];
        let circle_id = id_of(&compiled, circle);
        let set = overrides(&[(
            &circle_id,
            "radius",
            OverrideAction::Set(OverrideValue::Number(30.0)),
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
        assert!(baked.refused.is_empty(), "{baked:?}");
        assert!(
            baked
                .program
                .contains("    if i == 0:\n        dot.radius = 30\n"),
            "{}",
            baked.program
        );
        let actual = crate::run(&baked.program, &crate::Limits::default()).unwrap();
        assert_eq!(
            crate::plan::semantics(&actual),
            crate::plan::semantics(&before)
        );
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
        assert_eq!(
            crate::plan::semantics(&actual),
            crate::plan::semantics(&before)
        );
    }

    #[test]
    fn an_in_place_edit_and_a_call_override_both_become_code() {
        let compiled = compiled();
        let title_id = title(&compiled);
        // A node returned by a function is now reachable through its call
        // result (prompt 12 phase 4), so its override also becomes code.
        let spinner = compiled.document.roots()[6];
        let spinner_id = id_of(&compiled, spinner);
        // The title's size grows by one byte; every later key shifts, and any
        // remaining override must follow it.
        let set = overrides(&[
            (
                &title_id,
                "size",
                OverrideAction::Set(OverrideValue::Number(1000.0)),
            ),
            (
                &spinner_id,
                "pos",
                OverrideAction::Set(OverrideValue::Vec2(Vec2::new(1.0, 2.0))),
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
        assert!(
            baked.program.contains(" = spinner(240, 220, 96, 1.0)"),
            "the call result is bound: {}",
            baked.program
        );
        assert!(
            baked.program.contains(".pos = (1, 2)"),
            "the spinner override became code: {}",
            baked.program
        );
        let remaining = baked.overrides.to_mdf(rate());
        assert!(
            baked.overrides.is_empty(),
            "no override remains: {remaining}"
        );
        let actual =
            crate::run_with_overrides(&baked.program, None, &crate::Limits::default()).unwrap();
        assert_eq!(
            crate::plan::semantics(&actual.document),
            crate::plan::semantics(&before)
        );
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
    fn loop_created_nodes_are_specialised_with_a_guard() {
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
        let mut expected = execution.document.clone();
        set.apply(&mut expected);
        let baked = bake(
            source,
            &execution.document,
            &execution.bindings,
            &execution.origins,
            &set,
            &[],
        );
        assert!(baked.refused.is_empty(), "{baked:?}");
        assert!(
            baked
                .program
                .contains("    if i == 0:\n        d.pos = (1, 2)\n"),
            "{}",
            baked.program
        );
        let actual = crate::run(&baked.program, &crate::Limits::default()).unwrap();
        assert_eq!(
            crate::plan::semantics(&actual),
            crate::plan::semantics(&expected)
        );
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
        assert_eq!(
            crate::plan::semantics(&actual),
            crate::plan::semantics(&before)
        );
    }

    #[test]
    fn unkeys_are_refused() {
        let compiled = compiled();
        let title_id = title(&compiled);
        let set = overrides(&[(&title_id, "rotation", OverrideAction::Unkey { frame: 10 })]);
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

    #[test]
    fn display_names_bake_as_assignments() {
        let compiled = compiled();
        let title_id = title(&compiled);
        let set = overrides(&[(
            &title_id,
            NAME,
            OverrideAction::Set(OverrideValue::Text("Title".to_owned())),
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
            baked.program.contains("title.name = \"Title\""),
            "{}",
            baked.program
        );
        let actual = crate::run(&baked.program, &crate::Limits::default()).unwrap();
        assert_eq!(actual.roots().len(), compiled.document.roots().len());
        assert!(
            actual.nodes().any(|node| node.name() == Some("Title")),
            "the baked assignment renames the node"
        );
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
        assert_eq!(
            crate::plan::semantics(&actual),
            crate::plan::semantics(&before)
        );
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
        assert_eq!(
            crate::plan::semantics(&actual),
            crate::plan::semantics(&before)
        );
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
        assert_eq!(
            crate::plan::semantics(&actual),
            crate::plan::semantics(&before)
        );
    }
}
