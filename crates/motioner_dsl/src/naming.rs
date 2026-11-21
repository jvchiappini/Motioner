//! Default display names derived from the variable a node is bound to.
//!
//! The name is presentation only (D-0022 identity is untouched). Programs
//! override it with `node.name = "...";` and an `name` override can replace it
//! without a re-run. Write-through (D-0036) and future constructors reuse
//! [`default_name`] so a generated binding gets the same readable label as a
//! hand-written one.

use std::collections::HashSet;
use std::fmt::Write as _;

use motioner_core::{Document, NodeId};

/// One `name = <node>` assignment recorded while the program ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameNote {
    /// The variable name.
    pub binding: String,
    /// The node it was bound to.
    pub node: NodeId,
    /// Enclosing loop indices and function-call ordinals, outermost first.
    ///
    /// A `for` loop contributes its zero-based iteration index; a user
    /// function contributes its one-based invocation ordinal. The combined
    /// path keeps nested loops and repeated calls distinguishable:
    /// `dot 2 5` is the dot in outer iteration 2 and inner iteration 5, and
    /// `holder 2` is the holder created by the second call of that function.
    pub path: Vec<i64>,
}

/// The display name for a binding evaluated at `path`.
///
/// A top-level binding is the variable name (`title`). A node made inside
/// loops or a user function gets the binding plus the full index path,
/// outermost first (`dot 2 5`), so nested iterations never collide.
#[must_use]
pub fn default_name(binding: &str, path: &[i64]) -> String {
    if path.is_empty() {
        return binding.to_owned();
    }
    let mut name = String::with_capacity(binding.len() + path.len() * 4);
    name.push_str(binding);
    for index in path {
        // Writing into the reserved buffer cannot fail and does not allocate.
        let _ = write!(name, " {index}");
    }
    name
}

/// Applies the first recorded binding of each node as its display name.
///
/// Nodes that already have a name (an explicit `node.name = ...`) are left
/// alone. Later bindings of the same node (an alias such as `alias = title`)
/// do not rename it.
pub fn assign_default_names(document: &mut Document, notes: &[NameNote]) {
    let mut seen: HashSet<NodeId> = HashSet::with_capacity(notes.len());
    for note in notes {
        if !seen.insert(note.node) {
            continue;
        }
        if document
            .node(note.node)
            .is_some_and(|node| node.name().is_none())
        {
            let _ = document.set_name(note.node, &default_name(&note.binding, &note.path));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_level_bindings_use_the_variable_name() {
        assert_eq!(default_name("title", &[]), "title");
    }

    #[test]
    fn the_full_index_path_is_appended_outermost_first() {
        assert_eq!(default_name("dot", &[3]), "dot 3");
        assert_eq!(default_name("dot", &[2, 5]), "dot 2 5");
        assert_eq!(default_name("holder", &[2]), "holder 2");
    }
}
