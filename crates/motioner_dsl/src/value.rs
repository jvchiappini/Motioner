//! Runtime values of the MDF interpreter.

use std::cell::{Ref, RefCell, RefMut};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::rc::Rc;

use motioner_core::NodeId;

use crate::ast::FunctionDef;
use crate::host::{HostError, HostResult};
use crate::limits::{Budget, Limits};

/// A lazily iterated integer range produced by `range()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RangeValue {
    pub start: i64,
    pub stop: i64,
    pub step: i64,
}

impl RangeValue {
    /// Number of elements, saturating at zero.
    pub fn len(self) -> i64 {
        let span = if self.step > 0 {
            self.stop - self.start
        } else {
            self.start - self.stop
        };
        if span <= 0 {
            0
        } else {
            (span + self.step.abs() - 1) / self.step.abs()
        }
    }

    /// The element at `index`, which must be in `0..len`.
    pub fn at(self, index: i64) -> i64 {
        self.start + index * self.step
    }
}

/// Built-in functions and scene constructors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Builtin {
    Range,
    Len,
    Min,
    Max,
    Abs,
    Round,
    Str,
    Int,
    Float,
    Sin,
    Cos,
    Lerp,
    Clamp,
    Comp,
    Group,
    Rect,
    Circle,
    Text,
    Path,
    Polygon,
    Star,
    Arc,
}

impl Builtin {
    /// Every builtin with its MDF name.
    pub const ALL: [(Self, &'static str); 22] = [
        (Self::Range, "range"),
        (Self::Len, "len"),
        (Self::Min, "min"),
        (Self::Max, "max"),
        (Self::Abs, "abs"),
        (Self::Round, "round"),
        (Self::Str, "str"),
        (Self::Int, "int"),
        (Self::Float, "float"),
        (Self::Sin, "sin"),
        (Self::Cos, "cos"),
        (Self::Lerp, "lerp"),
        (Self::Clamp, "clamp"),
        (Self::Comp, "comp"),
        (Self::Group, "group"),
        (Self::Rect, "rect"),
        (Self::Circle, "circle"),
        (Self::Text, "text"),
        (Self::Path, "path"),
        (Self::Polygon, "polygon"),
        (Self::Star, "star"),
        (Self::Arc, "arc"),
    ];

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .find(|(_, candidate)| *candidate == name)
            .map(|(builtin, _)| *builtin)
    }

    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(builtin, _)| *builtin == self)
            .map_or("?", |(_, name)| name)
    }
}

/// Methods callable on scene nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NodeMethod {
    Key,
    Add,
    Id,
}

impl NodeMethod {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "key" => Some(Self::Key),
            "add" => Some(Self::Add),
            "id" => Some(Self::Id),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Add => "add",
            Self::Id => "id",
        }
    }
}

/// Shared item storage for lists and tuples.
///
/// Lists and tuples use the same storage so one rule governs them. Dropping
/// is iterative: when the last reference to a container disappears its
/// children are moved onto a heap stack instead of recursing through the
/// Rust stack, so an arbitrarily deep acyclic value cannot overflow it.
#[derive(Debug)]
pub(crate) struct Items {
    inner: RefCell<Vec<Value>>,
}

impl Items {
    fn new(items: Vec<Value>) -> Self {
        Self {
            inner: RefCell::new(items),
        }
    }

    /// Reads the items.
    pub fn borrow(&self) -> Ref<'_, Vec<Value>> {
        self.inner.borrow()
    }

    /// Writes the items; only lists expose mutation to the interpreter.
    pub fn borrow_mut(&self) -> RefMut<'_, Vec<Value>> {
        self.inner.borrow_mut()
    }

    /// Number of items.
    pub fn len(&self) -> usize {
        self.inner.borrow().len()
    }

    /// Whether the container holds no items.
    pub fn is_empty(&self) -> bool {
        self.inner.borrow().is_empty()
    }

    /// Removes every item, leaving the container empty.
    fn take_all(&mut self) -> Vec<Value> {
        std::mem::take(self.inner.get_mut())
    }
}

impl Drop for Items {
    fn drop(&mut self) {
        let mut stack = self.take_all();
        while let Some(value) = stack.pop() {
            if let Value::List(items) | Value::Tuple(items) = value {
                if let Ok(mut items) = Rc::try_unwrap(items) {
                    let mut children = items.take_all();
                    stack.append(&mut children);
                }
            }
        }
    }
}

/// A dynamically typed MDF value. Cloning is cheap: aggregates share
/// their storage.
///
/// A list may never contain itself, directly or indirectly. Assignment
/// into a list rejects a value that reaches the target list (see
/// [`Value::contains_shared_list`]), so display, equality and drop can
/// assume a value graph is acyclic.
#[derive(Debug, Clone)]
pub(crate) enum Value {
    None,
    Bool(bool),
    Number(f64),
    Str(Rc<str>),
    List(Rc<Items>),
    Tuple(Rc<Items>),
    Range(RangeValue),
    Function(Rc<FunctionDef>),
    Builtin(Builtin),
    Node(NodeId),
    Method(NodeId, NodeMethod),
}

impl Value {
    /// Wraps items as a mutable list.
    pub fn list(items: Vec<Value>) -> Self {
        Self::List(Rc::new(Items::new(items)))
    }

    /// Wraps items as an immutable tuple.
    pub fn tuple(items: Vec<Value>) -> Self {
        Self::Tuple(Rc::new(Items::new(items)))
    }

    /// The MDF type name used in error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Bool(_) => "bool",
            Self::Number(_) => "number",
            Self::Str(_) => "string",
            Self::List(_) => "list",
            Self::Tuple(_) => "tuple",
            Self::Range(_) => "range",
            Self::Function(_) | Self::Builtin(_) | Self::Method(..) => "function",
            Self::Node(_) => "node",
        }
    }

    /// Python truthiness.
    pub fn is_truthy(&self) -> bool {
        match self {
            Self::None => false,
            Self::Bool(b) => *b,
            Self::Number(n) => *n != 0.0,
            Self::Str(s) => !s.is_empty(),
            Self::List(items) | Self::Tuple(items) => !items.is_empty(),
            Self::Range(range) => range.len() > 0,
            Self::Function(_) | Self::Builtin(_) | Self::Node(_) | Self::Method(..) => true,
        }
    }

    /// Structural equality; values of different types are unequal.
    ///
    /// # Errors
    ///
    /// Returns a limit error when either value nests deeper than
    /// [`Limits::max_value_depth`] or when the step budget is exhausted, so
    /// comparison is always bounded. The depth check runs before any
    /// structural short-circuit, so even comparing a container with itself
    /// reports an over-deep value.
    pub fn equals(&self, other: &Self, limits: &Limits, budget: &mut Budget) -> HostResult<bool> {
        self.check_depth(limits, budget)?;
        other.check_depth(limits, budget)?;
        self.equals_at(other, 0, limits, budget)
    }

    /// Fails when this value nests deeper than the configured limit.
    ///
    /// The walk stops at `max_value_depth + 1`, so its own recursion is
    /// bounded; `deepest` keeps a shared subtree from being re-walked at a
    /// shallower depth. Each visited item charges one step.
    fn check_depth(&self, limits: &Limits, budget: &mut Budget) -> HostResult<()> {
        let mut deepest: HashMap<*const Items, usize> = HashMap::new();
        self.check_depth_at(0, limits, budget, &mut deepest)
    }

    fn check_depth_at(
        &self,
        depth: usize,
        limits: &Limits,
        budget: &mut Budget,
        deepest: &mut HashMap<*const Items, usize>,
    ) -> HostResult<()> {
        let (Self::List(items) | Self::Tuple(items)) = self else {
            return Ok(());
        };
        let depth = next_depth(depth, limits)?;
        let pointer = Rc::as_ptr(items);
        if deepest.get(&pointer).is_some_and(|seen| *seen >= depth) {
            return Ok(());
        }
        deepest.insert(pointer, depth);
        let items = items.borrow();
        for child in items.iter() {
            budget.charge(1)?;
            child.check_depth_at(depth, limits, budget, deepest)?;
        }
        Ok(())
    }

    fn equals_at(
        &self,
        other: &Self,
        depth: usize,
        limits: &Limits,
        budget: &mut Budget,
    ) -> HostResult<bool> {
        match (self, other) {
            (Self::None, Self::None) => Ok(true),
            (Self::Bool(a), Self::Bool(b)) => Ok(a == b),
            // MDF `==` is IEEE-754 equality by design (spec/mdf-v0.md section 4).
            #[allow(clippy::float_cmp)]
            (Self::Number(a), Self::Number(b)) => Ok(a == b),
            (Self::Str(a), Self::Str(b)) => {
                budget.charge_string(a.len().min(b.len()))?;
                Ok(a == b)
            }
            (Self::List(a), Self::List(b)) | (Self::Tuple(a), Self::Tuple(b)) => {
                if Rc::ptr_eq(a, b) {
                    return Ok(true);
                }
                let depth = next_depth(depth, limits)?;
                items_equal(&a.borrow(), &b.borrow(), depth, limits, budget)
            }
            (Self::Range(a), Self::Range(b)) => Ok(a == b),
            (Self::Function(a), Self::Function(b)) => Ok(Rc::ptr_eq(a, b)),
            (Self::Builtin(a), Self::Builtin(b)) => Ok(a == b),
            (Self::Node(a), Self::Node(b)) => Ok(a == b),
            (Self::Method(a, m), Self::Method(b, n)) => Ok(a == b && m == n),
            _ => Ok(false),
        }
    }

    /// The `str()` form of the value.
    ///
    /// Formatting appends into one buffer and fails as soon as the buffer
    /// passes [`Limits::max_string_len`], so a shared value cannot blow up
    /// memory before a limit is checked.
    ///
    /// # Errors
    ///
    /// Returns a limit error when the value nests deeper than
    /// [`Limits::max_value_depth`], when the produced string exceeds
    /// [`Limits::max_string_len`], or when the step budget is exhausted.
    pub fn display(&self, limits: &Limits, budget: &mut Budget) -> HostResult<String> {
        let mut out = String::new();
        self.write_display(&mut out, 0, limits, budget)?;
        Ok(out)
    }

    fn write_display(
        &self,
        out: &mut String,
        depth: usize,
        limits: &Limits,
        budget: &mut Budget,
    ) -> HostResult<()> {
        match self {
            Self::None => push_checked(out, "None", limits),
            Self::Bool(true) => push_checked(out, "True", limits),
            Self::Bool(false) => push_checked(out, "False", limits),
            Self::Number(n) => push_checked(out, &format_number(*n), limits),
            Self::Str(s) => push_checked(out, s, limits),
            Self::List(items) => {
                let depth = next_depth(depth, limits)?;
                write_items(out, &items.borrow(), depth, limits, budget, "[", "]")
            }
            Self::Tuple(items) => {
                let depth = next_depth(depth, limits)?;
                let items = items.borrow();
                if items.len() == 1 {
                    push_checked(out, "(", limits)?;
                    budget.charge(1)?;
                    items[0].write_repr(out, depth, limits, budget)?;
                    push_checked(out, ",)", limits)
                } else {
                    write_items(out, &items, depth, limits, budget, "(", ")")
                }
            }
            Self::Range(r) => push_checked(
                out,
                &format!("range({}, {}, {})", r.start, r.stop, r.step),
                limits,
            ),
            Self::Function(f) => push_checked(out, &format!("<function {}>", f.name), limits),
            Self::Builtin(b) => push_checked(out, &format!("<builtin {}>", b.name()), limits),
            Self::Node(id) => push_checked(out, &format!("<node {id}>"), limits),
            Self::Method(id, m) => {
                push_checked(out, &format!("<method {} of node {id}>", m.name()), limits)
            }
        }
    }

    /// The form used inside containers: strings are quoted.
    fn write_repr(
        &self,
        out: &mut String,
        depth: usize,
        limits: &Limits,
        budget: &mut Budget,
    ) -> HostResult<()> {
        match self {
            Self::Str(s) => {
                // `String` writes are infallible; the depth and length
                // limits are enforced around the append.
                let _ = write!(out, "{s:?}");
                limits.check_string_len(out.len())
            }
            other => other.write_display(out, depth, limits, budget),
        }
    }

    /// Whether `target` is reachable from this value through lists and
    /// tuples. Scalars are skipped and each container is visited once, so
    /// the walk is iterative and safe on arbitrarily deep acyclic values.
    /// Each item visited charges one step.
    ///
    /// # Errors
    ///
    /// Returns a limit error when the step budget is exhausted.
    pub(crate) fn contains_shared_list(
        &self,
        target: &Rc<Items>,
        budget: &mut Budget,
    ) -> HostResult<bool> {
        let target_ptr = Rc::as_ptr(target);
        let mut visited: HashSet<*const Items> = HashSet::new();
        let mut stack = vec![self.clone()];
        while let Some(value) = stack.pop() {
            budget.charge(1)?;
            let (Self::List(items) | Self::Tuple(items)) = value else {
                continue;
            };
            if Rc::as_ptr(&items) == target_ptr {
                return Ok(true);
            }
            if visited.insert(Rc::as_ptr(&items)) {
                stack.extend(items.borrow().iter().cloned());
            }
        }
        Ok(false)
    }
}

/// Increments the container depth and rejects values past the limit.
fn next_depth(depth: usize, limits: &Limits) -> HostResult<usize> {
    let depth = depth.saturating_add(1);
    if depth > limits.max_value_depth {
        return Err(HostError::limit(format!(
            "value nesting limit of {} levels exceeded",
            limits.max_value_depth
        )));
    }
    Ok(depth)
}

fn items_equal(
    a: &[Value],
    b: &[Value],
    depth: usize,
    limits: &Limits,
    budget: &mut Budget,
) -> HostResult<bool> {
    if a.len() != b.len() {
        return Ok(false);
    }
    for (x, y) in a.iter().zip(b) {
        budget.charge(1)?;
        if !x.equals_at(y, depth, limits, budget)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Appends `text` and fails as soon as the buffer passes the string cap.
fn push_checked(out: &mut String, text: &str, limits: &Limits) -> HostResult<()> {
    out.push_str(text);
    limits.check_string_len(out.len())
}

/// Writes `open`, the comma-separated representations of `items`, and
/// `close` into `out`, charging one step per item visited.
fn write_items(
    out: &mut String,
    items: &[Value],
    depth: usize,
    limits: &Limits,
    budget: &mut Budget,
    open: &str,
    close: &str,
) -> HostResult<()> {
    push_checked(out, open, limits)?;
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            push_checked(out, ", ", limits)?;
        }
        budget.charge(1)?;
        item.write_repr(out, depth, limits, budget)?;
    }
    push_checked(out, close, limits)
}

/// Formats a number like Python: integral values without a fraction.
pub(crate) fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e16 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_lengths_match_python() {
        let len = |start, stop, step| RangeValue { start, stop, step }.len();
        assert_eq!(len(0, 5, 1), 5);
        assert_eq!(len(0, 5, 2), 3);
        assert_eq!(len(5, 0, -2), 3);
        assert_eq!(len(5, 0, 1), 0);
    }

    #[test]
    fn displays_like_python() {
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        let tuple = Value::tuple(vec![
            Value::Number(1.0),
            Value::Number(2.5),
            Value::Str(Rc::from("a")),
        ]);
        assert_eq!(
            tuple.display(&limits, &mut budget).unwrap(),
            "(1, 2.5, \"a\")"
        );
        assert_eq!(
            Value::Number(-0.0).display(&limits, &mut budget).unwrap(),
            "-0"
        );
        assert_eq!(
            Value::Bool(true).display(&limits, &mut budget).unwrap(),
            "True"
        );
    }
}
