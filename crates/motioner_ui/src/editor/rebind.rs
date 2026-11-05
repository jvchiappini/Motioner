//! Re-binding suggestions for orphaned overrides (D-0025 addendum).
//!
//! An override keyed by a byte-offset provenance key becomes an orphan when
//! an edit above the node's creating call shifts the offsets (D-0025 keeps
//! the keys). This module suggests the live nodes an orphan most likely
//! belongs to, so the editor can offer a one-click "Rebind" instead of
//! leaving the user to read offsets.
//!
//! The ranking is pure and deterministic:
//!
//! 1. The uniform provenance shift of the program edit is inferred from the
//!    orphans and the live nodes (the delta that the largest number of orphan
//!    identities agree on).
//! 2. Candidates must accept the property and must not already be targeted by
//!    another override of the same property.
//! 3. The closest shifted provenance key wins: base call offset, then the
//!    `/c` call-site offsets and the `/i` loop indices (same creation order),
//!    then the parent's offset (same parent), then creation order.

use std::collections::{BTreeMap, BTreeSet};

use motioner_core::{ContentField, Document, Node, OverrideEntry, Property, LOCKED, NAME, VISIBLE};

use super::Editor;

/// A parsed provenance key: `@<call>` followed by `/c<call-site>` and
/// `/i<iteration>` segments.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Provenance {
    /// Byte offset of the creating call.
    call: Option<usize>,
    /// Segments after the call offset.
    segments: Vec<Segment>,
}

/// One suffix of a provenance key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Segment {
    /// A user-function call site, by byte offset.
    Call(usize),
    /// A loop iteration index.
    Loop(i64),
}

/// Reads a leading decimal number, consuming its digits.
fn take_number(rest: &mut &str) -> Option<usize> {
    let digits = rest
        .as_bytes()
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digits == 0 {
        return None;
    }
    let (head, tail) = rest.split_at(digits);
    *rest = tail;
    head.parse().ok()
}

impl Provenance {
    fn parse(id: &str) -> Self {
        let mut rest = id.strip_prefix('@').unwrap_or("");
        let call = take_number(&mut rest);
        let mut segments = Vec::new();
        while let Some(stripped) = rest.strip_prefix('/') {
            rest = stripped;
            let kind = rest.chars().next();
            rest = rest.get(1..).unwrap_or("");
            match kind {
                Some('c') => {
                    if let Some(value) = take_number(&mut rest) {
                        segments.push(Segment::Call(value));
                    }
                }
                Some('i') => {
                    if let Some(value) = take_number(&mut rest) {
                        // MDF loop indices are non-negative and small.
                        segments.push(Segment::Loop(i64::try_from(value).unwrap_or(i64::MAX)));
                    }
                }
                _ => {}
            }
        }
        Self { call, segments }
    }
}

/// Whether a node accepts an override of `property`.
fn accepts(node: &Node, property: &str) -> bool {
    if matches!(property, NAME | LOCKED | VISIBLE) {
        return true;
    }
    if Property::from_name(property).is_some() {
        return true;
    }
    ContentField::from_name(property).is_some_and(|field| node.content().accepts(field))
}

/// The uniform shift an edit above the nodes introduced, if any.
///
/// Every orphan whose node still exists in the program shifts by the same
/// amount; the delta supported by the most orphan identities wins, then the
/// smallest one.
fn infer_shift(orphans: &[OverrideEntry], nodes: &[&Node]) -> Option<isize> {
    let bases: BTreeSet<usize> = nodes
        .iter()
        .filter_map(|node| Provenance::parse(node.identity().as_str()).call)
        .collect();
    if bases.is_empty() {
        return None;
    }
    let mut support: BTreeMap<isize, usize> = BTreeMap::new();
    let mut seen: BTreeSet<usize> = BTreeSet::new();
    for orphan in orphans {
        let Some(call) = Provenance::parse(&orphan.id).call else {
            continue;
        };
        if !seen.insert(call) {
            continue;
        }
        for base in &bases {
            let delta = base.cast_signed().saturating_sub(call.cast_signed());
            *support.entry(delta).or_default() += 1;
        }
    }
    support
        .into_iter()
        .max_by_key(|(delta, count)| (*count, std::cmp::Reverse(delta.unsigned_abs())))
        .map(|(delta, _)| delta)
}

/// How far a candidate's provenance key is from an orphan's shifted key.
fn provenance_gap(candidate: &Provenance, orphan: &Provenance, shift: Option<isize>) -> usize {
    const FAR: usize = usize::MAX / 4;
    let Some(shift) = shift else {
        return 0;
    };
    let shifted = |offset: usize| -> usize {
        let value = offset.cast_signed().saturating_add(shift);
        usize::try_from(value).unwrap_or(0)
    };
    let mut gap = match (candidate.call, orphan.call) {
        (Some(candidate), Some(orphan)) => candidate.abs_diff(shifted(orphan)),
        _ => FAR,
    };
    if candidate.segments.len() != orphan.segments.len() {
        return gap + FAR;
    }
    for (candidate, orphan) in candidate.segments.iter().zip(&orphan.segments) {
        match (candidate, orphan) {
            (Segment::Call(candidate), Segment::Call(orphan)) => {
                gap += candidate.abs_diff(shifted(*orphan));
            }
            (Segment::Loop(candidate), Segment::Loop(orphan)) => {
                if candidate != orphan {
                    gap += FAR;
                }
            }
            _ => gap += FAR,
        }
    }
    gap
}

/// The ranked candidate identities for `entry`.
fn suggestions_for(
    document: &Document,
    entry: &OverrideEntry,
    nodes: &[&Node],
    targeted: &BTreeSet<(String, String)>,
    shift: Option<isize>,
) -> Vec<String> {
    let orphan = Provenance::parse(&entry.id);
    let mut candidates: Vec<(usize, usize, usize, &str)> = nodes
        .iter()
        .filter(|node| accepts(node, &entry.property))
        .filter(|node| {
            !targeted.contains(&(node.identity().as_str().to_owned(), entry.property.clone()))
        })
        .enumerate()
        .map(|(order, node)| {
            let identity = node.identity().as_str();
            let gap = provenance_gap(&Provenance::parse(identity), &orphan, shift);
            let parent_gap = node
                .parent()
                .and_then(|parent| document.node(parent))
                .map_or(usize::MAX / 4, |parent| {
                    provenance_gap(
                        &Provenance::parse(parent.identity().as_str()),
                        &orphan,
                        shift,
                    )
                });
            (gap, parent_gap, order, identity)
        })
        .collect();
    candidates.sort_by_key(|candidate| (candidate.0, candidate.1, candidate.2));
    candidates
        .into_iter()
        .map(|(_, _, _, identity)| identity.to_owned())
        .collect()
}

impl Editor {
    /// Every orphan with its candidate nodes, best first.
    ///
    /// Candidates are the live nodes that accept the orphan's property and
    /// are not already targeted by another override of the same property,
    /// ranked by their shift-corrected provenance key.
    #[must_use]
    pub fn orphan_suggestions(&self) -> Vec<(OverrideEntry, Vec<String>)> {
        let orphans = self.orphans();
        let Some(document) = self.document() else {
            return orphans
                .into_iter()
                .map(|entry| (entry, Vec::new()))
                .collect();
        };
        let nodes: Vec<&Node> = document.nodes().collect();
        let shift = infer_shift(&orphans, &nodes);
        let targeted: BTreeSet<(String, String)> = self
            .session
            .overrides()
            .entries()
            .iter()
            .map(|entry| (entry.id.clone(), entry.property.clone()))
            .collect();
        orphans
            .into_iter()
            .map(|entry| {
                let suggestions = suggestions_for(document, &entry, &nodes, &targeted, shift);
                (entry, suggestions)
            })
            .collect()
    }

    /// The candidate identities for one orphan, best first.
    #[must_use]
    pub fn suggestions_for(&self, entry: &OverrideEntry) -> Vec<String> {
        let Some(document) = self.document() else {
            return Vec::new();
        };
        let nodes: Vec<&Node> = document.nodes().collect();
        let shift = infer_shift(&self.orphans(), &nodes);
        let targeted: BTreeSet<(String, String)> = self
            .session
            .overrides()
            .entries()
            .iter()
            .map(|entry| (entry.id.clone(), entry.property.clone()))
            .collect();
        suggestions_for(document, entry, &nodes, &targeted, shift)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::tests::hello_editor;
    use crate::parse_commands;
    use std::time::Duration;

    fn run(editor: &mut Editor, text: &str) {
        for command in parse_commands(text).unwrap() {
            editor
                .execute(&command, Duration::ZERO)
                .unwrap_or_else(|error| panic!("{command}: {error}"));
        }
    }

    /// Everything a re-bound document must keep, ignoring provenance keys
    /// (which shift with the program edit by design, D-0025).
    fn semantics(
        document: &Document,
    ) -> Vec<(motioner_core::NodeContent, motioner_core::Transform, bool)> {
        document
            .nodes()
            .map(|node| {
                (
                    node.content().clone(),
                    node.transform().clone(),
                    node.is_visible(),
                )
            })
            .collect()
    }

    #[test]
    fn a_shifted_program_suggests_the_same_nodes() {
        let (mut editor, dir) = hello_editor("rebind");
        run(
            &mut editor,
            "edit(\"@281\", \"rotation\", 12)\nvisible(\"@569/i1\", false)",
        );
        let before = editor.session().document().unwrap().clone();
        editor.save_now().unwrap();
        let path = dir.0.join("hello.motion");
        let source = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, format!("# shifted\n{source}")).unwrap();
        assert!(editor.reload(Duration::ZERO));
        assert_eq!(editor.orphans().len(), 2, "{:?}", editor.orphans());

        let title = editor.session().document().unwrap().roots()[0];
        let title_id = editor.node_identity(title);
        let suggestions = editor.orphan_suggestions();
        let rotation = suggestions
            .iter()
            .find(|(entry, _)| entry.property == "rotation")
            .expect("rotation orphan");
        assert_eq!(rotation.1.first(), Some(&title_id), "{rotation:?}");
        let visible = suggestions
            .iter()
            .find(|(entry, _)| entry.property == "visible")
            .expect("visible orphan");
        assert!(
            visible.1.first().is_some_and(|id| id.ends_with("/i1")),
            "the loop iteration is part of the creation order: {visible:?}"
        );

        for (entry, list) in suggestions {
            let best = list.first().expect("a suggestion").clone();
            run(
                &mut editor,
                &format!("assign_id(\"{}\", \"{best}\")", entry.id),
            );
        }
        assert_eq!(
            semantics(editor.session().document().unwrap()),
            semantics(&before)
        );
    }

    #[test]
    fn candidates_already_holding_the_property_are_skipped() {
        let (mut editor, dir) = hello_editor("rebind_taken");
        run(&mut editor, "edit(\"@281\", \"rotation\", 12)");
        editor.save_now().unwrap();
        let path = dir.0.join("hello.motion");
        let source = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, format!("# shifted\n{source}")).unwrap();
        assert!(editor.reload(Duration::ZERO));
        let (orphan, list) = editor
            .orphan_suggestions()
            .into_iter()
            .find(|(entry, _)| entry.property == "rotation")
            .unwrap();
        // A fresh override of the same property on the suggested node makes
        // it ineligible, so the next candidate is proposed instead.
        let first = list[0].clone();
        run(&mut editor, &format!("set(\"{first}\", \"rotation\", 3)"));
        let again = editor.suggestions_for(&orphan);
        assert_ne!(again.first(), Some(&first), "{again:?}");
    }
}
