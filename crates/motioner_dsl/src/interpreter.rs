//! Tree-walking interpreter for the MDF v0 subset.
//!
//! Scoping follows Python without closures: a function sees its own
//! locals, then globals, then builtins; assignment inside a function
//! creates a local. Execution is deterministic: no clock, no I/O, no
//! unseeded randomness, and hash maps are only used for lookups, never
//! iterated.

use std::collections::HashMap;
use std::rc::Rc;

use motioner_core::{Document, NodeId};

use crate::ast::{
    Argument, AssignOp, BinaryOp, CompareOp, Expr, ExprKind, FunctionDef, LogicalOp, Program, Stmt,
    StmtKind, Target, UnaryOp,
};
use crate::builtins;
use crate::diagnostic::{Diagnostic, ErrorCategory, Severity, Span, StackFrame};
use crate::host::{as_integer, bind, f64_from_i64, CallArgs, CallValue, HostError, HostResult};
use crate::limits::{Budget, Limits};
use crate::origins::{literal_of, ValueOrigins};
use crate::scene::{CreationSite, Scene};
use crate::value::{Builtin, Value};
use motioner_core::{NoTextMetrics, TextMetrics};

/// Executes a parsed program and returns the document it built.
///
/// # Errors
///
/// Returns a runtime diagnostic (name, type, value, limit or unsupported)
/// with the span and call stack of the failure, or a value error if the
/// program never called `comp()`.
pub fn execute(program: &Program, limits: &Limits) -> Result<Document, Diagnostic> {
    execute_with_metrics(program, limits, &NoTextMetrics)
}

/// Executes a parsed program with text metrics installed (D-0034).
///
/// # Errors
///
/// Same as [`execute`].
pub fn execute_with_metrics(
    program: &Program,
    limits: &Limits,
    metrics: &dyn TextMetrics,
) -> Result<Document, Diagnostic> {
    execute_full_with_metrics(program, limits, metrics).map(|(document, _)| document)
}

/// Executes a parsed program and returns the document plus every top-level
/// variable bound to a node, sorted by name (bake uses the bindings to name
/// nodes in generated statements).
///
/// # Errors
///
/// Same as [execute].
pub fn execute_full(
    program: &Program,
    limits: &Limits,
) -> Result<(Document, Vec<(String, NodeId)>), Diagnostic> {
    execute_detailed(program, limits).map(|execution| (execution.document, execution.bindings))
}

/// [`execute_full`] with text metrics installed (D-0034).
///
/// # Errors
///
/// Same as [`execute`].
pub fn execute_full_with_metrics(
    program: &Program,
    limits: &Limits,
    metrics: &dyn TextMetrics,
) -> Result<(Document, Vec<(String, NodeId)>), Diagnostic> {
    execute_detailed_with_metrics(program, limits, metrics)
        .map(|execution| (execution.document, execution.bindings))
}

/// Everything a run produced: the document, the top-level node bindings and
/// the value origins the hybrid bake uses.
#[derive(Debug, Clone)]
pub struct Execution {
    /// The document the program built.
    pub document: Document,
    /// Top-level variables bound to nodes, sorted by name.
    pub bindings: Vec<(String, NodeId)>,
    /// Where every recorded value came from in the source.
    pub origins: ValueOrigins,
}

/// Executes a parsed program and returns the document, the bindings and the
/// value origins.
///
/// # Errors
///
/// Same as [execute].
pub fn execute_detailed(program: &Program, limits: &Limits) -> Result<Execution, Diagnostic> {
    execute_detailed_with_metrics(program, limits, &NoTextMetrics)
}

/// [`execute_detailed`] with text metrics installed (D-0034).
///
/// # Errors
///
/// Same as [`execute`].
pub fn execute_detailed_with_metrics(
    program: &Program,
    limits: &Limits,
    metrics: &dyn TextMetrics,
) -> Result<Execution, Diagnostic> {
    let mut interpreter = Interpreter {
        limits,
        metrics,
        globals: HashMap::new(),
        frames: Vec::new(),
        loops: Vec::new(),
        budget: Budget::new(limits),
        scene: Scene::new(limits.max_nodes),
    };
    interpreter.exec_block(&program.body)?;
    let bindings = interpreter.bindings();
    let origins = interpreter.scene.origins().clone();
    let document = interpreter.scene.into_document().ok_or_else(|| {
        let start = Span {
            line: 1,
            column: 1,
            ..Span::default()
        };
        Diagnostic::new(
            ErrorCategory::Value,
            start,
            "the program never called comp(...); every MDF program must create a composition",
        )
    })?;
    Ok(Execution {
        document,
        bindings,
        origins,
    })
}

/// How a statement finished.
enum Flow {
    Normal,
    Break,
    Continue,
    Return(Value),
}

struct CallFrame {
    function: Rc<str>,
    call_span: Span,
    locals: HashMap<Rc<str>, Value>,
}

struct Interpreter<'l> {
    limits: &'l Limits,
    metrics: &'l dyn TextMetrics,
    globals: HashMap<Rc<str>, Value>,
    frames: Vec<CallFrame>,
    /// Iteration index of every active loop, outermost first.
    loops: Vec<i64>,
    budget: Budget,
    scene: Scene,
}

type Exec<T> = Result<T, Diagnostic>;

impl Interpreter<'_> {
    // ---- errors and scopes ---------------------------------------------

    fn diagnostic(
        &self,
        category: ErrorCategory,
        span: Span,
        message: impl Into<String>,
    ) -> Diagnostic {
        Diagnostic {
            category,
            severity: Severity::Error,
            message: message.into(),
            span,
            stack: self
                .frames
                .iter()
                .map(|frame| StackFrame {
                    function: frame.function.to_string(),
                    call_span: frame.call_span,
                })
                .collect(),
        }
    }

    fn host_error(&self, span: Span, error: HostError) -> Diagnostic {
        self.diagnostic(error.category, span, error.message)
    }

    /// The provenance site of a creating call at `span`.
    fn site(&self, span: Span) -> CreationSite {
        CreationSite {
            call_start: span.start,
            callers: self
                .frames
                .iter()
                .map(|frame| frame.call_span.start)
                .collect(),
            loops: self.loops.clone(),
        }
    }

    /// Top-level variable bindings that point at nodes, sorted by name.
    fn bindings(&self) -> Vec<(String, NodeId)> {
        let mut bindings: Vec<(String, NodeId)> = self
            .globals
            .iter()
            .filter_map(|(name, value)| match value {
                Value::Node(id) => Some((name.to_string(), *id)),
                _ => None,
            })
            .collect();
        bindings.sort_by(|a, b| a.0.cmp(&b.0));
        bindings
    }

    fn lookup(&self, name: &str, span: Span) -> Exec<Value> {
        let local = self.frames.last().and_then(|frame| frame.locals.get(name));
        if let Some(value) = local.or_else(|| self.globals.get(name)) {
            return Ok(value.clone());
        }
        Builtin::from_name(name).map(Value::Builtin).ok_or_else(|| {
            self.diagnostic(
                ErrorCategory::Name,
                span,
                format!("name '{name}' is not defined"),
            )
        })
    }

    fn store(&mut self, name: Rc<str>, value: Value) {
        match self.frames.last_mut() {
            Some(frame) => frame.locals.insert(name, value),
            None => self.globals.insert(name, value),
        };
    }

    // ---- statements ----------------------------------------------------

    fn exec_block(&mut self, body: &[Stmt]) -> Exec<Flow> {
        for statement in body {
            let flow = self.exec(statement)?;
            if !matches!(flow, Flow::Normal) {
                return Ok(flow);
            }
        }
        Ok(Flow::Normal)
    }

    fn exec(&mut self, statement: &Stmt) -> Exec<Flow> {
        if let Err(error) = self.budget.charge(1) {
            return Err(self.host_error(statement.span, error));
        }
        match &statement.kind {
            StmtKind::Expr(expr) => {
                self.eval(expr)?;
                Ok(Flow::Normal)
            }
            StmtKind::Assign { target, op, value } => {
                self.assign(target, *op, value, statement.span)?;
                Ok(Flow::Normal)
            }
            StmtKind::Def(function) => {
                self.store(function.name.clone(), Value::Function(function.clone()));
                Ok(Flow::Normal)
            }
            StmtKind::Return(value) => {
                let value = match value {
                    Some(expr) => self.eval(expr)?,
                    None => Value::None,
                };
                Ok(Flow::Return(value))
            }
            StmtKind::If {
                branches,
                otherwise,
            } => {
                for (condition, body) in branches {
                    if self.eval(condition)?.is_truthy() {
                        return self.exec_block(body);
                    }
                }
                self.exec_block(otherwise)
            }
            StmtKind::For {
                variable,
                iterable,
                body,
            } => self.exec_for(variable, iterable, body),
            StmtKind::Break => Ok(Flow::Break),
            StmtKind::Continue => Ok(Flow::Continue),
            StmtKind::Pass => Ok(Flow::Normal),
        }
    }

    fn exec_for(&mut self, variable: &Rc<str>, iterable: &Expr, body: &[Stmt]) -> Exec<Flow> {
        let sequence = self.eval(iterable)?;
        if let Value::Range(range) = sequence {
            // Ranges are iterated lazily so `range(10**7)` needs no memory.
            let values = (0..range.len()).map(|i| Value::Number(f64_from_i64(range.at(i))));
            return self.run_loop(variable, body, values);
        }
        let items = builtins::items(&sequence, self.limits, &mut self.budget)
            .map_err(|e| self.host_error(iterable.span, e))?;
        self.run_loop(variable, body, items.into_iter())
    }

    fn run_loop(
        &mut self,
        variable: &Rc<str>,
        body: &[Stmt],
        values: impl Iterator<Item = Value>,
    ) -> Exec<Flow> {
        for (index, value) in values.enumerate() {
            self.store(variable.clone(), value);
            // The loop index is part of every node's provenance key, so the
            // same iteration of the same call yields the same identity.
            self.loops.push(i64::try_from(index).unwrap_or(i64::MAX));
            let flow = self.exec_block(body);
            self.loops.pop();
            match flow? {
                Flow::Break => break,
                Flow::Return(value) => return Ok(Flow::Return(value)),
                Flow::Normal | Flow::Continue => {}
            }
        }
        Ok(Flow::Normal)
    }

    fn assign(&mut self, target: &Target, op: AssignOp, value: &Expr, span: Span) -> Exec<()> {
        let evaluated = self.eval(value)?;
        let combine = |this: &mut Self, current: Value, evaluated: Value| -> Exec<Value> {
            match augmented(op) {
                None => Ok(evaluated),
                Some(binary) => match this.binary(binary, &current, &evaluated) {
                    Ok(value) => Ok(value),
                    Err(error) => Err(this.host_error(span, error)),
                },
            }
        };
        match target {
            Target::Name(name) => {
                let current = if op == AssignOp::Set {
                    Value::None
                } else {
                    self.lookup(name, span)?
                };
                let new = combine(self, current, evaluated)?;
                self.store(name.clone(), new);
            }
            Target::Attribute { object, name } => {
                let object = self.eval(object)?;
                let Value::Node(id) = object else {
                    return Err(self.diagnostic(
                        ErrorCategory::Type,
                        span,
                        format!("cannot set attributes on a {}", object.type_name()),
                    ));
                };
                let current = if op == AssignOp::Set {
                    Value::None
                } else {
                    self.scene
                        .get_attribute(id, name)
                        .map_err(|e| self.host_error(span, e))?
                };
                let new = combine(self, current, evaluated)?;
                let literal = (op == AssignOp::Set).then(|| literal_of(value)).flatten();
                let result = self
                    .scene
                    .set_attribute(id, name, &new, value.span, literal);
                result.map_err(|e| self.host_error(span, e))?;
            }
            Target::Index { object, index } => {
                let object = self.eval(object)?;
                let index = self.eval(index)?;
                let Value::List(items) = object else {
                    return Err(self.diagnostic(
                        ErrorCategory::Type,
                        span,
                        format!("a {} does not support item assignment", object.type_name()),
                    ));
                };
                let len = items.borrow().len();
                let slot = resolve_index(&index, len).map_err(|e| self.host_error(span, e))?;
                let current = items.borrow()[slot].clone();
                let new = combine(self, current, evaluated)?;
                let cycle = new.contains_shared_list(&items, &mut self.budget);
                if cycle.map_err(|e| self.host_error(span, e))? {
                    return Err(self
                        .host_error(span, HostError::value_error("a list cannot contain itself")));
                }
                items.borrow_mut()[slot] = new;
            }
        }
        Ok(())
    }

    // ---- expressions ---------------------------------------------------

    fn eval(&mut self, expr: &Expr) -> Exec<Value> {
        let span = expr.span;
        let host =
            |this: &Self, result: HostResult<Value>| result.map_err(|e| this.host_error(span, e));
        match &expr.kind {
            ExprKind::Number(n) => Ok(Value::Number(*n)),
            ExprKind::Str(s) => {
                host(
                    self,
                    self.limits.check_string_len(s.len()).map(|()| Value::None),
                )?;
                Ok(Value::Str(s.clone()))
            }
            ExprKind::Bool(b) => Ok(Value::Bool(*b)),
            ExprKind::None => Ok(Value::None),
            ExprKind::Name(name) => self.lookup(name, span),
            ExprKind::List(items) => {
                host(
                    self,
                    self.limits
                        .check_list_len(items.len())
                        .map(|()| Value::None),
                )?;
                if let Err(error) = self.budget.charge_items(items.len()) {
                    return Err(self.host_error(span, error));
                }
                let values = self.eval_all(items)?;
                Ok(Value::list(values))
            }
            ExprKind::Tuple(items) => {
                host(
                    self,
                    self.limits
                        .check_list_len(items.len())
                        .map(|()| Value::None),
                )?;
                if let Err(error) = self.budget.charge_items(items.len()) {
                    return Err(self.host_error(span, error));
                }
                Ok(Value::tuple(self.eval_all(items)?))
            }
            ExprKind::Unary { op, operand } => {
                let value = self.eval(operand)?;
                let Value::Number(n) = value else {
                    return Err(self.diagnostic(
                        ErrorCategory::Type,
                        span,
                        format!("unary minus/plus needs a number, got {}", value.type_name()),
                    ));
                };
                Ok(Value::Number(if *op == UnaryOp::Neg { -n } else { n }))
            }
            ExprKind::Binary { op, left, right } => {
                let left = self.eval(left)?;
                let right = self.eval(right)?;
                let result = self.binary(*op, &left, &right);
                host(self, result)
            }
            ExprKind::Compare { op, left, right } => {
                let left = self.eval(left)?;
                let right = self.eval(right)?;
                let result = compare(*op, &left, &right, self.limits, &mut self.budget);
                host(self, result)
            }
            ExprKind::Logical { op, left, right } => {
                let left = self.eval(left)?;
                let short_circuits = match op {
                    LogicalOp::And => !left.is_truthy(),
                    LogicalOp::Or => left.is_truthy(),
                };
                if short_circuits {
                    Ok(left)
                } else {
                    self.eval(right)
                }
            }
            ExprKind::Not(operand) => Ok(Value::Bool(!self.eval(operand)?.is_truthy())),
            ExprKind::Call { callee, args } => self.call(callee, args, span),
            ExprKind::Attribute { object, name } => {
                let object = self.eval(object)?;
                let Value::Node(id) = object else {
                    return Err(self.diagnostic(
                        ErrorCategory::Name,
                        span,
                        format!("a {} has no attribute '{name}'", object.type_name()),
                    ));
                };
                host(self, self.scene.get_attribute(id, name))
            }
            ExprKind::Index { object, index } => {
                let object = self.eval(object)?;
                let index = self.eval(index)?;
                let result = index_value(&object, &index, &mut self.budget);
                host(self, result)
            }
        }
    }

    fn eval_all(&mut self, exprs: &[Expr]) -> Exec<Vec<Value>> {
        exprs.iter().map(|expr| self.eval(expr)).collect()
    }

    fn binary(&mut self, op: BinaryOp, left: &Value, right: &Value) -> HostResult<Value> {
        match (op, left, right) {
            (_, Value::Number(a), Value::Number(b)) => arithmetic(op, *a, *b),
            (BinaryOp::Add, Value::Str(a), Value::Str(b)) => {
                let bytes = combined_len(a.len(), b.len())?;
                self.limits.check_string_len(bytes)?;
                self.budget.charge_string(bytes)?;
                Ok(Value::Str(Rc::from(format!("{a}{b}"))))
            }
            (BinaryOp::Add, Value::List(a), Value::List(b)) => {
                let len = combined_len(a.borrow().len(), b.borrow().len())?;
                self.limits.check_list_len(len)?;
                self.budget.charge_items(len)?;
                let joined: Vec<Value> = a
                    .borrow()
                    .iter()
                    .chain(b.borrow().iter())
                    .cloned()
                    .collect();
                Ok(Value::list(joined))
            }
            (BinaryOp::Add, Value::Tuple(a), Value::Tuple(b)) => {
                let len = combined_len(a.borrow().len(), b.borrow().len())?;
                self.limits.check_list_len(len)?;
                self.budget.charge_items(len)?;
                Ok(Value::tuple(
                    a.borrow()
                        .iter()
                        .chain(b.borrow().iter())
                        .cloned()
                        .collect(),
                ))
            }
            _ => Err(HostError::type_error(format!(
                "unsupported operand types for {}: {} and {}",
                op.symbol(),
                left.type_name(),
                right.type_name()
            ))),
        }
    }

    fn call(&mut self, callee: &Expr, args: &[Argument], span: Span) -> Exec<Value> {
        let function = self.eval(callee)?;
        let mut call_args = CallArgs::default();
        for arg in args {
            let value = self.eval(&arg.value)?;
            let call_value = CallValue {
                value,
                span: arg.value.span,
                literal: literal_of(&arg.value),
            };
            match &arg.name {
                Some(name) => call_args.keywords.push((name.clone(), call_value)),
                None => call_args.positional.push(call_value),
            }
        }
        let result = match function {
            Value::Function(def) => return self.call_function(&def, call_args, span),
            Value::Builtin(
                builtin @ (Builtin::Comp
                | Builtin::Group
                | Builtin::Rect
                | Builtin::Circle
                | Builtin::Text
                | Builtin::Path
                | Builtin::Polygon
                | Builtin::Star
                | Builtin::Arc
                | Builtin::Image
                | Builtin::Stagger),
            ) => self.scene.call(builtin, call_args, &self.site(span)),
            Value::Builtin(builtin) => builtins::call(
                builtin,
                call_args,
                self.limits,
                &mut self.budget,
                self.metrics,
            ),
            Value::Method(id, method) => self.scene.call_method(id, method, call_args),
            other => Err(HostError::type_error(format!(
                "a {} is not callable",
                other.type_name()
            ))),
        };
        result.map_err(|e| self.host_error(span, e))
    }

    fn call_function(
        &mut self,
        function: &Rc<FunctionDef>,
        args: CallArgs,
        span: Span,
    ) -> Exec<Value> {
        if self.frames.len() >= self.limits.max_call_depth {
            return Err(self.diagnostic(
                ErrorCategory::Limit,
                span,
                format!(
                    "call depth limit of {} exceeded; check for runaway recursion",
                    self.limits.max_call_depth
                ),
            ));
        }
        let names: Vec<&str> = function.params.iter().map(|p| &**p).collect();
        let bound = bind(&function.name, &names, names.len(), args)
            .map_err(|e| self.host_error(span, e))?;
        let locals = function
            .params
            .iter()
            .cloned()
            .zip(
                bound
                    .into_values()
                    .into_iter()
                    .map(|v| v.unwrap_or(Value::None)),
            )
            .collect();
        self.frames.push(CallFrame {
            function: function.name.clone(),
            call_span: span,
            locals,
        });
        let flow = self.exec_block(&function.body);
        self.frames.pop();
        match flow? {
            Flow::Return(value) => Ok(value),
            Flow::Normal | Flow::Break | Flow::Continue => Ok(Value::None),
        }
    }
}

fn augmented(op: AssignOp) -> Option<BinaryOp> {
    match op {
        AssignOp::Set => None,
        AssignOp::Add => Some(BinaryOp::Add),
        AssignOp::Sub => Some(BinaryOp::Sub),
        AssignOp::Mul => Some(BinaryOp::Mul),
        AssignOp::Div => Some(BinaryOp::Div),
    }
}

/// Adds two lengths as `usize`, failing instead of overflowing.
fn combined_len(a: usize, b: usize) -> HostResult<usize> {
    a.checked_add(b)
        .ok_or_else(|| HostError::limit("collection length limit exceeded"))
}

fn arithmetic(op: BinaryOp, a: f64, b: f64) -> HostResult<Value> {
    let needs_divisor = matches!(op, BinaryOp::Div | BinaryOp::FloorDiv | BinaryOp::Mod);
    if needs_divisor && b == 0.0 {
        return Err(HostError::value_error("division by zero"));
    }
    let result = match op {
        BinaryOp::Add => a + b,
        BinaryOp::Sub => a - b,
        BinaryOp::Mul => a * b,
        BinaryOp::Div => a / b,
        BinaryOp::FloorDiv => (a / b).floor(),
        // Python modulo: the result takes the sign of the divisor.
        BinaryOp::Mod => a - b * (a / b).floor(),
    };
    Ok(Value::Number(result))
}

fn compare(
    op: CompareOp,
    left: &Value,
    right: &Value,
    limits: &Limits,
    budget: &mut Budget,
) -> HostResult<Value> {
    let result = match op {
        CompareOp::Eq => left.equals(right, limits, budget)?,
        CompareOp::NotEq => !left.equals(right, limits, budget)?,
        CompareOp::Lt => builtins::compare(left, right, budget)?.is_lt(),
        CompareOp::Le => builtins::compare(left, right, budget)?.is_le(),
        CompareOp::Gt => builtins::compare(left, right, budget)?.is_gt(),
        CompareOp::Ge => builtins::compare(left, right, budget)?.is_ge(),
    };
    Ok(Value::Bool(result))
}

/// Resolves a Python-style index (negative counts from the end).
fn resolve_index(index: &Value, len: usize) -> HostResult<usize> {
    let Value::Number(n) = index else {
        return Err(HostError::type_error(format!(
            "indices must be numbers, got {}",
            index.type_name()
        )));
    };
    let i = as_integer(*n).ok_or_else(|| HostError::type_error("indices must be whole numbers"))?;
    let len_i = i64::try_from(len).map_err(|_| HostError::limit("collection is too large"))?;
    let resolved = if i < 0 { i + len_i } else { i };
    usize::try_from(resolved)
        .ok()
        .filter(|&r| r < len)
        .ok_or_else(|| {
            HostError::value_error(format!("index {i} is out of range for length {len}"))
        })
}

fn index_value(object: &Value, index: &Value, budget: &mut Budget) -> HostResult<Value> {
    match object {
        Value::List(items) | Value::Tuple(items) => {
            let items = items.borrow();
            Ok(items[resolve_index(index, items.len())?].clone())
        }
        Value::Str(s) => {
            budget.charge_items(s.len())?;
            let count = s.chars().count();
            let i = resolve_index(index, count)?;
            Ok(Value::Str(Rc::from(
                s.chars().nth(i).map(String::from).unwrap_or_default(),
            )))
        }
        Value::Range(range) => {
            let len = usize::try_from(range.len()).unwrap_or(usize::MAX);
            let i = resolve_index(index, len)?;
            let i = i64::try_from(i).map_err(|_| HostError::limit("index is too large"))?;
            Ok(Value::Number(f64_from_i64(range.at(i))))
        }
        other => Err(HostError::type_error(format!(
            "a {} cannot be indexed",
            other.type_name()
        ))),
    }
}
