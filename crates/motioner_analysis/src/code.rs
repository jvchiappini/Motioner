//! Frame ↔ code mapping from the interpreter's value origins (D-0036).

use motioner_core::NodeId;
use motioner_dsl::ValueOrigins;

/// Where a node was created and written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeLocation {
    /// The node.
    pub node: NodeId,
    /// Line of the constructor call.
    pub creation_line: u32,
    /// Line of the statement that contains it.
    pub statement_line: u32,
    /// Column of the statement (1-based).
    pub statement_column: u32,
}

/// The code location of `node`, when the run recorded it.
#[must_use]
pub fn code_for_node(origins: &ValueOrigins, node: NodeId) -> Option<CodeLocation> {
    let origin = origins.node_origin(node)?;
    Some(CodeLocation {
        node,
        creation_line: origin.creation.line,
        statement_line: origin.statement.line,
        statement_column: origin.statement.column,
    })
}

/// Every node created or written by the statement on `line`.
///
/// Mirrors `code_for_node`: a line can create several nodes (a loop body
/// runs many times, a function is called from several places), so the result
/// is a list. Only the node's creation line and the statement line that
/// contains it are considered; a value write on the same line also matches.
#[must_use]
pub fn nodes_for_code(origins: &ValueOrigins, line: u32) -> Vec<NodeId> {
    let mut nodes: Vec<NodeId> = origins
        .nodes()
        .iter()
        .filter(|origin| origin.creation.line == line || origin.statement.line == line)
        .map(|origin| origin.node)
        .collect();
    for entry in origins.entries() {
        if entry.statement.line == line && !nodes.contains(&entry.node) {
            nodes.push(entry.node);
        }
    }
    nodes.sort_by_key(|id| id.raw());
    nodes
}

/// The line span a node's statement occupies, when known.
#[must_use]
pub fn statement_span(origins: &ValueOrigins, node: NodeId) -> Option<(u32, u32)> {
    let origin = origins.node_origin(node)?;
    let start = origin.statement.line;
    let end = origin.block.span.line.max(start);
    Some((start, end))
}
