//! Value origins: where each node property value came from in the program.
//!
//! The hybrid bake (T-DSL-0006) edits a literal argument in place when that is
//! exact. To decide, the interpreter records, for every property write it
//! performs, the source span of the expression that produced the value and
//! whether that expression is a plain literal. A literal is **private** to a
//! node when it is evaluated exactly once during the run: inside a loop or a
//! function called twice the same span appears several times, so editing it
//! would change every node it fed.
//!
//! Origins v2 (D-0036) also record the statement, block, loop stack and call
//! stack of every write and every node creation, the variable a node is bound
//! to and whether the expression is a literal, a parameter, a loop variable or
//! computed. The write-through planner uses that context to insert guarded
//! statements and keyword arguments instead of opaque provenance keys.
//!
//! Names are interned: origins store a [`NameId`] and [`ValueOrigins`] owns one
//! copy of each name, so recording a write stays cheap and the result can cross
//! the interpreter thread boundary.
//!
//! Recording is deterministic (the interpreter has no clock or randomness)
//! and never changes the document.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use motioner_core::{Frame, NodeId};

use crate::ast::{Expr, ExprKind, Stmt, StmtKind, Target};
use crate::diagnostic::Span;

/// An interned identifier in a [`ValueOrigins`] table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NameId(u32);

impl NameId {
    /// Index into the owning origin table's name list.
    #[must_use]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// The MDF shape of a plain literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Literal {
    /// A number, possibly signed: `160`, `-24`.
    Number,
    /// A string literal.
    Text,
    /// A two-number pair: `(960, 480)`.
    Pair,
    /// `True` or `False`.
    Bool,
    /// `None`.
    None,
}

/// What produced a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginKind {
    /// A static write: a constructor argument or `node.property = value`.
    Set,
    /// A `node.key(...)` call at a frame.
    Key(Frame),
}

/// The syntactic origin of an expression: a literal, a parameter, a loop
/// variable, or anything computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpressionOrigin {
    /// A plain literal of the given shape.
    Literal(Literal),
    /// A reference to a parameter of the enclosing function.
    Parameter {
        /// Interned parameter name as written in the definition.
        name: NameId,
    },
    /// A reference to an enclosing loop variable.
    LoopVariable {
        /// Interned loop variable name.
        name: NameId,
    },
    /// A local, a global of non-parameter kind, or any computed expression.
    Computed,
}

/// An explicit `ease="..."` argument of a `.key()` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EaseOrigin {
    /// Span of the ease expression (the string literal).
    pub span: Span,
    /// The ease name it names.
    pub name: String,
}

/// The extent and indentation of the block that contains a statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BlockOrigin {
    /// Byte range from the first to the last statement of the block.
    pub span: Span,
    /// Indentation columns of the block's statements.
    pub indent: u16,
}

impl BlockOrigin {
    /// The block formed by `body`.
    #[must_use]
    pub(crate) fn of(body: &[Stmt]) -> Self {
        // The top-level block of a run contains the library statements and the
        // user's statements. Write-through locates blocks in the user's source,
        // so the block must span only the user's statements; a block made only
        // of library statements keeps its own range.
        let user: Vec<&Stmt> = body
            .iter()
            .filter(|statement| !crate::prelude::is_library_span(statement.span))
            .collect();
        let (first, last) = if user.is_empty() {
            (body.first(), body.last())
        } else {
            (user.first().copied(), user.last().copied())
        };
        match (first, last) {
            (Some(first), Some(last)) => Self {
                span: first.span.to(last.span),
                indent: u16::try_from(first.span.column.saturating_sub(1)).unwrap_or(u16::MAX),
            },
            _ => Self::default(),
        }
    }
}

/// A loop iteration value that can be written as a literal.
#[derive(Debug, Clone, PartialEq)]
pub enum IterationValue {
    /// A number, `range()` indices included.
    Number(f64),
    /// A string.
    Text(String),
    /// `True` or `False`.
    Bool(bool),
    /// `None`.
    None,
}

/// One enclosing `for` loop at the moment a value was produced.
#[derive(Debug, Clone, PartialEq)]
pub struct LoopOrigin {
    /// Span of the `for` statement.
    pub span: Span,
    /// Interned loop variables, outermost first; one until tuple unpacking
    /// exists.
    pub variables: Vec<NameId>,
    /// Span of the iterable expression.
    pub iterable: Span,
    /// Zero-based iteration index.
    pub index: i64,
    /// The iteration value when it is expressible as a literal.
    pub value: Option<IterationValue>,
    /// Span of the loop body.
    pub body: Span,
    /// Indentation columns of the loop body.
    pub indent: u16,
}

/// A parameter of an enclosing user function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParameterOrigin {
    /// Interned parameter name.
    pub name: NameId,
    /// Span of its default expression, when the language has defaults.
    ///
    /// MDF v0 has required parameters only, so this is `None` today; the
    /// field is reserved for the call-site specialisation of D-0036.
    pub default: Option<Span>,
}

/// One enclosing user-function call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallOrigin {
    /// Span of the call expression at the call site.
    pub call_site: Span,
    /// Interned function name.
    pub function: NameId,
    /// Span of the `def` header.
    pub definition: Span,
    /// Parameters in definition order.
    pub parameters: Vec<ParameterOrigin>,
    /// Whether this invocation returned the node being described.
    pub returns_node: bool,
}

/// The variable a node is bound to, and whether that binding is then reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BindingOrigin {
    /// Interned variable name.
    pub name: NameId,
    /// Span of the assignment that created the binding.
    pub statement: Span,
    /// The block that contains that assignment.
    pub block: BlockOrigin,
    /// Whether the same name is assigned again later in that block.
    pub reassigned_later: bool,
}

/// Where a statement or value came from: the enclosing block, loop stack and
/// call stack at that moment.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OriginSite {
    /// Enclosing loops, outermost first.
    pub loops: Vec<LoopOrigin>,
    /// Enclosing user-function calls, outermost first.
    pub calls: Vec<CallOrigin>,
}

/// The expression that produced a value, with its literal shape and syntax
/// class.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValueSource {
    /// Span of the expression.
    pub span: Span,
    /// The literal shape, when the expression is a plain literal.
    pub literal: Option<Literal>,
    /// Literal, parameter reference, loop variable or computed.
    pub expression: ExpressionOrigin,
}

/// One recorded property value.
#[derive(Debug, Clone, PartialEq)]
pub struct ValueOrigin {
    /// The node the value belongs to.
    pub node: NodeId,
    /// The property as written (`pos`, `pos.x`, `radius`, `visible`...).
    pub property: String,
    /// Span of the expression that produced the value.
    pub span: Span,
    /// The literal shape, when the expression is a plain literal.
    pub literal: Option<Literal>,
    /// Whether the value is static or a keyframe.
    pub kind: OriginKind,
    /// For a key, its explicit ease argument.
    pub ease: Option<EaseOrigin>,
    /// Literal, parameter reference, loop variable or computed.
    pub expression: ExpressionOrigin,
    /// Span of the statement that wrote the property.
    pub statement: Span,
    /// The block that contains the statement.
    pub block: BlockOrigin,
    /// Loop and call context of the write.
    pub site: Arc<OriginSite>,
}

/// Where a node was created and how it was reached.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeOrigin {
    /// The created node.
    pub node: NodeId,
    /// Span of the constructor call.
    pub creation: Span,
    /// Span of the statement that contains the call.
    pub statement: Span,
    /// The block that contains the statement.
    pub block: BlockOrigin,
    /// Loop and call context of the creation.
    pub site: Arc<OriginSite>,
    /// Every variable the node is bound to, one per block that binds it.
    pub bindings: Vec<BindingOrigin>,
}

/// The most recent entry index for one node property, split by write kind.
#[derive(Debug, Clone, Default, PartialEq)]
struct PropertyIndex {
    /// Latest `Set` entry, if any.
    set: Option<usize>,
    /// Latest `Key` entry per frame.
    keys: HashMap<Frame, usize>,
}

/// Every value origin of one run.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ValueOrigins {
    entries: Vec<ValueOrigin>,
    nodes: Vec<NodeOrigin>,
    names: Vec<String>,
    name_ids: HashMap<String, NameId>,
    /// Entry index by node, so a lookup does not scan the entry list.
    entries_by_node: HashMap<NodeId, Vec<usize>>,
    /// Latest entry per node property and kind.
    properties: HashMap<NodeId, HashMap<String, PropertyIndex>>,
    /// How many entries share each expression span start.
    span_counts: HashMap<usize, usize>,
    /// Creation-origin index by node.
    node_index: HashMap<NodeId, usize>,
}

impl ValueOrigins {
    /// An empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The recorded origins, in write order.
    #[must_use]
    pub fn entries(&self) -> &[ValueOrigin] {
        &self.entries
    }

    /// Every node creation, in creation order.
    #[must_use]
    pub fn nodes(&self) -> &[NodeOrigin] {
        &self.nodes
    }

    /// The creation origin of `node`, if it was recorded.
    #[must_use]
    pub fn node_origin(&self, node: NodeId) -> Option<&NodeOrigin> {
        self.node_index
            .get(&node)
            .and_then(|index| self.nodes.get(*index))
    }

    /// The name an interned id refers to.
    ///
    /// # Panics
    ///
    /// Panics only when `id` comes from a different [`ValueOrigins`] table,
    /// which is a programming error.
    #[must_use]
    pub fn name(&self, id: NameId) -> &str {
        &self.names[id.index()]
    }

    /// Interns `name`, returning the stable id for this run.
    pub(crate) fn intern_name(&mut self, name: &str) -> NameId {
        if let Some(id) = self.name_ids.get(name) {
            return *id;
        }
        let id = NameId(u32::try_from(self.names.len()).unwrap_or(u32::MAX));
        self.names.push(name.to_owned());
        self.name_ids.insert(name.to_owned(), id);
        id
    }

    /// Records `origin`.
    ///
    /// Every write is kept, in program order: the planner needs the unguarded
    /// value of a property even after a guarded write replaces it, and the
    /// privacy check must count every evaluation of the same span. The latest
    /// write still wins for [`Self::set_origin`] and [`Self::key_origin`],
    /// which search from the end.
    pub fn record(&mut self, origin: ValueOrigin) {
        let index = self.entries.len();
        let node = origin.node;
        let span_start = origin.span.start;
        let kind = origin.kind;
        let property = origin.property.clone();
        self.entries.push(origin);
        *self.span_counts.entry(span_start).or_insert(0) += 1;
        self.entries_by_node.entry(node).or_default().push(index);
        let slot = self
            .properties
            .entry(node)
            .or_default()
            .entry(property)
            .or_default();
        match kind {
            OriginKind::Set => slot.set = Some(index),
            OriginKind::Key(frame) => {
                slot.keys.insert(frame, index);
            }
        }
    }

    /// Records the creation of `origin`.
    pub(crate) fn record_node(&mut self, origin: NodeOrigin) {
        let index = self.nodes.len();
        self.node_index.insert(origin.node, index);
        self.nodes.push(origin);
    }

    /// Binds `node` to the variable named in `binding`.
    pub(crate) fn attach_binding(&mut self, node: NodeId, binding: BindingOrigin) {
        let Some(index) = self.node_index.get(&node).copied() else {
            return;
        };
        let origin = &mut self.nodes[index];
        let duplicate = origin.bindings.iter().any(|existing| {
            existing.statement.start == binding.statement.start
                && existing.name == binding.name
                && existing.block.span.start == binding.block.span.start
        });
        if !duplicate {
            origin.bindings.push(binding);
        }
    }

    /// Marks the call entries that returned the node in `returned`.
    ///
    /// `returned` holds one `(node, call-site span)` pair per function return
    /// that produced a node; a node can be returned by several call sites.
    pub(crate) fn mark_returned(&mut self, returned: &[(NodeId, Span)]) {
        for (node, call_site) in returned {
            if let Some(index) = self.node_index.get(node).copied() {
                let origin = &mut self.nodes[index];
                for call in &mut Arc::make_mut(&mut origin.site).calls {
                    if call.call_site.start == call_site.start {
                        call.returns_node = true;
                    }
                }
            }
            let Some(indices) = self.entries_by_node.get(node) else {
                continue;
            };
            for index in indices {
                let origin = &mut self.entries[*index];
                for call in &mut Arc::make_mut(&mut origin.site).calls {
                    if call.call_site.start == call_site.start {
                        call.returns_node = true;
                    }
                }
            }
        }
    }

    /// Completes the binding facts that need the whole program: whether a
    /// bound variable is assigned again later in its block.
    ///
    /// `body` is the user's statements: origins only ever point at them, and
    /// scanning the library too would make every run pay for it.
    pub(crate) fn finalize(&mut self, body: &[crate::ast::Stmt]) {
        let mut reassigned: HashSet<usize> = HashSet::new();
        collect_assigned_later(body, &mut reassigned);
        for origin in &mut self.nodes {
            for binding in &mut origin.bindings {
                if reassigned.contains(&binding.statement.start) {
                    binding.reassigned_later = true;
                }
            }
        }
    }

    /// How many times the value expression of `origin` was evaluated in this
    /// run. `1` means the literal is private to its node.
    #[must_use]
    pub fn count(&self, origin: &ValueOrigin) -> usize {
        self.span_counts
            .get(&origin.span.start)
            .copied()
            .unwrap_or(0)
    }

    /// The static origin of `node`'s `property`, if recorded.
    #[must_use]
    pub fn set_origin(&self, node: NodeId, property: &str) -> Option<&ValueOrigin> {
        let slot = self.properties.get(&node)?.get(property)?;
        self.entries.get(slot.set?)
    }

    /// The key origin of `node`'s `property` at `frame`, if recorded.
    #[must_use]
    pub fn key_origin(&self, node: NodeId, property: &str, frame: Frame) -> Option<&ValueOrigin> {
        let slot = self.properties.get(&node)?.get(property)?;
        self.entries.get(*slot.keys.get(&frame)?)
    }

    /// Whether every origin with this span start is private to one node.
    #[must_use]
    pub fn is_private(&self, origin: &ValueOrigin) -> bool {
        self.count(origin) == 1
    }
}

/// Marks the statements whose assigned name is assigned again later in the
/// same block.
///
/// One reverse pass per block: a name seen later is already in `assigned`, so
/// every earlier assignment to it is a reassignment. Nested blocks are walked
/// separately, exactly as before, so a name reused in another block does not
/// leak across.
fn collect_assigned_later(body: &[Stmt], reassigned: &mut HashSet<usize>) {
    let mut assigned: HashSet<&str> = HashSet::new();
    for statement in body.iter().rev() {
        if let StmtKind::Assign {
            target: Target::Name(name),
            ..
        } = &statement.kind
        {
            if !assigned.insert(&**name) {
                reassigned.insert(statement.span.start);
            }
        }
    }
    for statement in body {
        match &statement.kind {
            StmtKind::If {
                branches,
                otherwise,
            } => {
                for (_, body) in branches {
                    collect_assigned_later(body, reassigned);
                }
                collect_assigned_later(otherwise, reassigned);
            }
            StmtKind::For { body, .. } => collect_assigned_later(body, reassigned),
            StmtKind::Def(function) => collect_assigned_later(&function.body, reassigned),
            _ => {}
        }
    }
}

/// The literal shape of `expr`, if it is a plain literal.
///
/// Strings must not be interpolated (MDF has no f-strings), numbers may be
/// signed, and a pair is a tuple or list of exactly two plain numbers.
#[must_use]
pub fn literal_of(expr: &Expr) -> Option<Literal> {
    match &expr.kind {
        ExprKind::Number(_) => Some(Literal::Number),
        ExprKind::Str(_) => Some(Literal::Text),
        ExprKind::Bool(_) => Some(Literal::Bool),
        ExprKind::None => Some(Literal::None),
        ExprKind::Unary { operand, .. } => {
            matches!(operand.kind, ExprKind::Number(_)).then_some(Literal::Number)
        }
        ExprKind::Tuple(items) | ExprKind::List(items) => {
            let plain = |item: &Expr| match &item.kind {
                ExprKind::Number(_) => true,
                ExprKind::Unary { operand, .. } => matches!(operand.kind, ExprKind::Number(_)),
                _ => false,
            };
            (items.len() == 2 && items.iter().all(plain)).then_some(Literal::Pair)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn first_value(source: &str) -> Option<Literal> {
        let program = parse(source).unwrap();
        let statement = program.body.first()?;
        let crate::ast::StmtKind::Expr(expr) = &statement.kind else {
            return None;
        };
        literal_of(expr)
    }

    #[test]
    fn literals_are_recognised() {
        assert_eq!(first_value("160\n"), Some(Literal::Number));
        assert_eq!(first_value("-24\n"), Some(Literal::Number));
        assert_eq!(first_value("\"hello\"\n"), Some(Literal::Text));
        assert_eq!(first_value("True\n"), Some(Literal::Bool));
        assert_eq!(first_value("None\n"), Some(Literal::None));
        assert_eq!(first_value("(960, -480)\n"), Some(Literal::Pair));
        assert_eq!(first_value("[1, 2]\n"), Some(Literal::Pair));
    }

    #[test]
    fn expressions_and_shared_values_are_not_literals() {
        assert_eq!(first_value("480 + i * 240\n"), None);
        assert_eq!(first_value("size\n"), None);
        assert_eq!(first_value("(960, y)\n"), None);
        assert_eq!(first_value("f()\n"), None);
    }
}
