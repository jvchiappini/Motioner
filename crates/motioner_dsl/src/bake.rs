//! Folding overrides into the MDF program (`motioner bake`, D-0022).
//!
//! Bake is pure: it takes the program text, the compiled document, the
//! top-level variable bindings and the override set, and returns the new
//! program text plus the overrides it could not fold in. It never writes
//! files (the app does that, atomically).
//!
//! Only nodes reachable from a top-level variable binding can be expressed in
//! the program, because generated statements are `var.property = value`.
//! Nodes created inside a loop or a function without a binding are refused
//! with a reason; their overrides stay in the file.

use motioner_core::{
    format_number, quote, seconds_of, Document, FrameRate, NodeId, OverrideAction, OverrideEntry,
    OverrideSet, OverrideValue, Property, NAME,
};

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
    /// The program with the generated block appended (unchanged if nothing
    /// was baked).
    pub program: String,
    /// The overrides that remain in the overrides file.
    pub overrides: OverrideSet,
    /// Identities whose every override was baked.
    pub baked: Vec<String>,
    /// Overrides that could not be baked.
    pub refused: Vec<BakeRefusal>,
}

/// Folds `overrides` into `program` for every node bound to a top-level
/// variable.
///
/// `only` limits baking to those identities when it is not empty.
#[must_use]
pub fn bake(
    program: &str,
    document: &Document,
    bindings: &[(String, NodeId)],
    overrides: &OverrideSet,
    only: &[String],
) -> BakeOutcome {
    let rate = document.composition().frame_rate();
    let mut statements = Vec::new();
    let mut remaining = OverrideSet::new();
    let mut baked: Vec<String> = Vec::new();
    let mut refused = Vec::new();

    for entry in overrides.entries() {
        if !only.is_empty() && !only.iter().any(|id| id == &entry.id) {
            remaining.push(entry.clone());
            continue;
        }
        match bake_entry(document, bindings, entry, rate) {
            Ok(statement) => {
                statements.push(statement);
                if !baked.contains(&entry.id) {
                    baked.push(entry.id.clone());
                }
            }
            Err(reason) => {
                refused.push(BakeRefusal {
                    id: entry.id.clone(),
                    property: entry.property.clone(),
                    reason,
                });
                remaining.push(entry.clone());
            }
        }
    }

    let program = if statements.is_empty() {
        program.to_owned()
    } else {
        append_block(program, &statements)
    };
    BakeOutcome {
        program,
        overrides: remaining,
        baked,
        refused,
    }
}

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
        OverrideAction::Set(value) => Ok(format!(
            "{variable}.{} = {}",
            entry.property,
            set_value_text(&entry.property, value)
        )),
        OverrideAction::Key { frame, value, ease } => {
            let (base, axis) = motioner_core::split_property(&entry.property);
            let Some(property) = Property::from_name(base) else {
                return Err(format!(
                    "'{}' is not an animatable transform property",
                    entry.property
                ));
            };
            if axis.is_some() && !property.is_vector() {
                return Err(format!("'{}' has no axes", entry.property));
            }
            let time = format_number(seconds_of(*frame, rate));
            let ease = if *ease == motioner_core::Ease::Linear {
                String::new()
            } else {
                format!(", ease={}", quote(ease.name()))
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

fn set_value_text(property: &str, value: &OverrideValue) -> String {
    if let OverrideValue::Text(text) = value {
        let optional = matches!(property, "fill" | "stroke" | "color");
        if optional && text.eq_ignore_ascii_case("none") {
            return "None".to_owned();
        }
    }
    value_text(value)
}

fn value_text(value: &OverrideValue) -> String {
    match value {
        OverrideValue::Number(n) => format_number(*n),
        OverrideValue::Vec2(v) => format!("({}, {})", format_number(v.x), format_number(v.y)),
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

    /// Compiles `source` and keeps the bindings bake needs.
    fn compile(source: &str) -> crate::Compiled {
        let program = crate::parse(source).unwrap();
        let (document, bindings) =
            crate::execute_full(&program, &crate::Limits::default()).unwrap();
        crate::Compiled {
            document,
            overrides: OverrideSet::new(),
            warnings: Vec::new(),
            bindings,
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

    #[test]
    fn bake_is_exact_for_a_bound_title_and_group() {
        let source = hello_with_bound_spinner();
        let compiled = compile(&source);
        let title = compiled.document.roots()[0];
        let title_id = compiled
            .document
            .node(title)
            .unwrap()
            .identity()
            .as_str()
            .to_owned();
        let rect = compiled
            .bindings
            .iter()
            .find(|(name, _)| name == "spinner1")
            .map(|(_, id)| *id)
            .expect("hello.motion binds the spinner group");
        let rect_id = compiled
            .document
            .node(rect)
            .unwrap()
            .identity()
            .as_str()
            .to_owned();

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
        let baked = bake(&source, &compiled.document, &compiled.bindings, &set, &[]);
        assert_eq!(baked.baked.len(), 2, "{baked:?}");
        assert!(baked.refused.is_empty(), "{baked:?}");
        assert!(baked.overrides.is_empty());

        let expected = {
            let mut document = compiled.document.clone();
            set.apply(&mut document);
            document
        };
        let actual = crate::run(&baked.program, &crate::Limits::default()).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn bake_is_idempotent() {
        let compiled = compiled();
        let title = compiled.document.roots()[0];
        let id = compiled
            .document
            .node(title)
            .unwrap()
            .identity()
            .as_str()
            .to_owned();
        let set = overrides(&[(
            &id,
            "rotation",
            OverrideAction::Set(OverrideValue::Number(12.0)),
        )]);
        let first = bake(HELLO, &compiled.document, &compiled.bindings, &set, &[]);
        assert!(first.overrides.is_empty());
        let second = bake(
            &first.program,
            &compiled.document,
            &compiled.bindings,
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
        let (document, bindings) =
            crate::execute_full(&program, &crate::Limits::default()).unwrap();
        let circle = document.roots()[0];
        let id = document
            .node(circle)
            .unwrap()
            .identity()
            .as_str()
            .to_owned();
        let set = overrides(&[(
            &id,
            "pos",
            OverrideAction::Set(OverrideValue::Vec2(Vec2::new(1.0, 2.0))),
        )]);
        let baked = bake(source, &document, &bindings, &set, &[]);
        assert_eq!(baked.program, source, "nothing was appended");
        assert_eq!(baked.refused.len(), 1);
        assert!(baked.refused[0].reason.contains("top-level variable"));
        assert_eq!(baked.overrides.len(), 1, "the override is kept");
    }

    #[test]
    fn only_limits_the_bake_to_named_ids() {
        let compiled = compiled();
        let title = compiled.document.roots()[0];
        let a = compiled
            .document
            .node(title)
            .unwrap()
            .identity()
            .as_str()
            .to_owned();
        let (other, other_id) = compiled.bindings[0].clone();
        let _ = other;
        let set = overrides(&[
            (
                &a,
                "rotation",
                OverrideAction::Set(OverrideValue::Number(1.0)),
            ),
            (
                &other_id.to_string(),
                "opacity",
                OverrideAction::Set(OverrideValue::Number(0.25)),
            ),
        ]);
        let baked = bake(
            HELLO,
            &compiled.document,
            &compiled.bindings,
            &set,
            std::slice::from_ref(&a),
        );
        assert_eq!(baked.baked, vec![a]);
        assert_eq!(baked.overrides.len(), 1);
    }

    #[test]
    fn axis_keys_bake_exactly() {
        let compiled = compiled();
        let title = compiled.document.roots()[0];
        let id = compiled
            .document
            .node(title)
            .unwrap()
            .identity()
            .as_str()
            .to_owned();
        let set = overrides(&[(
            &id,
            "pos.x",
            OverrideAction::Key {
                frame: 60,
                value: OverrideValue::Number(500.0),
                ease: Ease::QuadOut,
            },
        )]);
        let baked = bake(HELLO, &compiled.document, &compiled.bindings, &set, &[]);
        assert!(baked.refused.is_empty(), "{baked:?}");
        assert!(
            baked
                .program
                .contains(".key(\"pos.x\", 1, 500, ease=\"quad_out\")"),
            "{}",
            baked.program
        );
        let mut expected = compiled.document.clone();
        set.apply(&mut expected);
        let actual = crate::run(&baked.program, &crate::Limits::default()).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn names_and_unkeys_are_refused() {
        let compiled = compiled();
        let title = compiled.document.roots()[0];
        let id = compiled
            .document
            .node(title)
            .unwrap()
            .identity()
            .as_str()
            .to_owned();
        for action in [
            (
                NAME,
                OverrideAction::Set(OverrideValue::Text("Title".to_owned())),
            ),
            ("rotation", OverrideAction::Unkey { frame: 10 }),
        ] {
            let set = overrides(&[(&id, action.0, action.1)]);
            let baked = bake(HELLO, &compiled.document, &compiled.bindings, &set, &[]);
            assert_eq!(baked.refused.len(), 1, "{baked:?}");
            assert_eq!(baked.overrides.len(), 1);
            assert!(baked.program == HELLO);
        }
    }

    #[test]
    fn a_baked_block_carries_the_marker() {
        let compiled = compiled();
        let title = compiled.document.roots()[0];
        let id = compiled
            .document
            .node(title)
            .unwrap()
            .identity()
            .as_str()
            .to_owned();
        let set = overrides(&[(
            &id,
            "opacity",
            OverrideAction::Set(OverrideValue::Number(0.75)),
        )]);
        let baked = bake(HELLO, &compiled.document, &compiled.bindings, &set, &[]);
        assert!(baked.program.contains(MARKER));
        assert!(baked.program.trim_end().ends_with(".opacity = 0.75"));
        let _ = rate();
    }
}
