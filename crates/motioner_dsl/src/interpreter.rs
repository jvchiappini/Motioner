//! Tree-walking interpreter for the MDF v0 subset.
//!
//! Scoping follows Python without closures: a function sees its own
//! locals, then globals, then builtins; assignment inside a function
//! creates a local. Execution is deterministic: no clock, no I/O, no
//! unseeded randomness, and hash maps are only used for lookups, never
//! iterated.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use motioner_core::{Document, NodeId};

use crate::ast::{
    Argument, AssignOp, BinaryOp, CompareOp, Expr, ExprKind, FunctionDef, LogicalOp, Program, Stmt,
    StmtKind, Target, UnaryOp,
};
use crate::builtins;
use crate::diagnostic::{Diagnostic, ErrorCategory, Severity, Span, StackFrame};
use crate::host::{as_integer, bind, f64_from_i64, CallArgs, CallValue, HostError, HostResult};
use crate::limits::{Budget, Limits};
use crate::naming::{assign_default_names, NameNote};
use crate::origins::{
    literal_of, BindingOrigin, BlockOrigin, CallOrigin, ExpressionOrigin, IterationValue, Literal,
    LoopOrigin, NameId, OriginSite, ParameterOrigin, ValueOrigins, ValueSource,
};
use crate::scene::{CreationSite, Scene, WriteSite};
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
        blocks: Vec::new(),
        index_path: Vec::new(),
        call_counts: HashMap::new(),
        returned: Vec::new(),
        site: Arc::new(OriginSite::default()),
        site_dirty: true,
        budget: Budget::new(limits),
        scene: Scene::new(limits.max_nodes),
        name_notes: Vec::new(),
    };
    interpreter.exec_block(&program.body)?;
    let bindings = interpreter.bindings();
    let mut origins = interpreter.scene.origins().clone();
    origins.finalize(program);
    origins.mark_returned(&interpreter.returned);
    let mut document = interpreter.scene.into_document().ok_or_else(|| {
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
    assign_default_names(&mut document, &interpreter.name_notes);
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
    function_id: NameId,
    call_span: Span,
    definition: Span,
    parameters: Vec<(Rc<str>, NameId)>,
    locals: HashMap<Rc<str>, Value>,
}

struct LoopFrame {
    span: Span,
    variables: Rc<[(Rc<str>, NameId)]>,
    iterable: Span,
    body: Span,
    indent: u16,
    index: i64,
    value: Option<IterationValue>,
}

struct BlockFrame {
    origin: BlockOrigin,
    statement: Span,
}

struct Interpreter<'l> {
    limits: &'l Limits,
    metrics: &'l dyn TextMetrics,
    globals: HashMap<Rc<str>, Value>,
    frames: Vec<CallFrame>,
    /// Enclosing loops at runtime, outermost first.
    loops: Vec<LoopFrame>,
    /// Enclosing statement blocks at runtime, outermost first.
    blocks: Vec<BlockFrame>,
    /// Display-name path: active loop iterations and function-call ordinals,
    /// outermost first.
    index_path: Vec<i64>,
    /// How many times each user function has been called in this run.
    call_counts: HashMap<Rc<str>, i64>,
    /// `(node, call-site span)` pairs for every function return that produced
    /// a node; origins are marked after the run.
    returned: Vec<(NodeId, Span)>,
    /// The origin context of the statement being executed, rebuilt only when
    /// a write needs it and the loop or call stack changed.
    site: Arc<OriginSite>,
    /// Whether [`Self::site`] must rebuild the cached context.
    site_dirty: bool,
    budget: Budget,
    scene: Scene,
    name_notes: Vec<NameNote>,
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

    /// Rebuilds the origin context after the statement, loop stack or call
    /// stack changes. Origins share the result through [`Arc`], so recording a
    /// write does not rebuild or clone the stacks.
    fn ensure_site(&mut self) {
        if !self.site_dirty {
            return;
        }
        self.site_dirty = false;
        let loops = self
            .loops
            .iter()
            .map(|frame| LoopOrigin {
                span: frame.span,
                variables: frame.variables.iter().map(|(_, id)| *id).collect(),
                iterable: frame.iterable,
                index: frame.index,
                value: frame.value.clone(),
                body: frame.body,
                indent: frame.indent,
            })
            .collect();
        let calls = self
            .frames
            .iter()
            .map(|frame| CallOrigin {
                call_site: frame.call_span,
                function: frame.function_id,
                definition: frame.definition,
                parameters: frame
                    .parameters
                    .iter()
                    .map(|(_, id)| ParameterOrigin {
                        name: *id,
                        default: None,
                    })
                    .collect(),
                returns_node: false,
            })
            .collect();
        self.site = Arc::new(OriginSite { loops, calls });
    }

    /// The statement currently executing inside the innermost block.
    fn current_statement(&self) -> Span {
        self.blocks
            .last()
            .map_or_else(Span::default, |frame| frame.statement)
    }

    /// The block currently executing.
    fn current_block(&self) -> BlockOrigin {
        self.blocks
            .last()
            .map_or_else(BlockOrigin::default, |frame| frame.origin)
    }

    /// The provenance site of a creating call at `span`.
    fn site(&mut self, span: Span) -> CreationSite {
        self.ensure_site();
        CreationSite {
            creation: span,
            statement: self.current_statement(),
            block: self.current_block(),
            site: Arc::clone(&self.site),
        }
    }

    /// The write context of the statement currently executing.
    fn write_site(&mut self) -> WriteSite {
        self.ensure_site();
        WriteSite {
            statement: self.current_statement(),
            block: self.current_block(),
            site: Arc::clone(&self.site),
        }
    }

    /// Classifies `expr` syntactically against the current scope.
    fn expression_origin(&self, expr: &Expr, literal: Option<Literal>) -> ExpressionOrigin {
        if let Some(literal) = literal {
            return ExpressionOrigin::Literal(literal);
        }
        if let ExprKind::Name(name) = &expr.kind {
            if let Some(frame) = self.frames.last() {
                if let Some((_, id)) = frame
                    .parameters
                    .iter()
                    .find(|(parameter, _)| parameter.as_ref() == &**name)
                {
                    return ExpressionOrigin::Parameter { name: *id };
                }
            }
            if let Some((_, id)) = self.loops.iter().find_map(|loop_frame| {
                loop_frame
                    .variables
                    .iter()
                    .find(|(variable, _)| variable.as_ref() == &**name)
            }) {
                return ExpressionOrigin::LoopVariable { name: *id };
            }
        }
        ExpressionOrigin::Computed
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
        self.blocks.push(BlockFrame {
            origin: BlockOrigin::of(body),
            statement: Span::default(),
        });
        let mut result = Ok(Flow::Normal);
        for statement in body {
            if let Some(frame) = self.blocks.last_mut() {
                frame.statement = statement.span;
            }
            match self.exec(statement) {
                Ok(Flow::Normal) => {}
                Ok(flow) => {
                    result = Ok(flow);
                    break;
                }
                Err(error) => {
                    result = Err(error);
                    break;
                }
            }
        }
        self.blocks.pop();
        result
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
            } => self.exec_for(variable, iterable, body, statement.span),
            StmtKind::Break => Ok(Flow::Break),
            StmtKind::Continue => Ok(Flow::Continue),
            StmtKind::Pass => Ok(Flow::Normal),
        }
    }

    fn exec_for(
        &mut self,
        variable: &Rc<str>,
        iterable: &Expr,
        body: &[Stmt],
        span: Span,
    ) -> Exec<Flow> {
        let sequence = self.eval(iterable)?;
        if let Value::Range(range) = sequence {
            // Ranges are iterated lazily so `range(10**7)` needs no memory.
            let values = (0..range.len()).map(|i| Value::Number(f64_from_i64(range.at(i))));
            return self.run_loop(variable, body, span, iterable.span, values);
        }
        let items = builtins::items(&sequence, self.limits, &mut self.budget)
            .map_err(|e| self.host_error(iterable.span, e))?;
        self.run_loop(variable, body, span, iterable.span, items.into_iter())
    }

    fn run_loop(
        &mut self,
        variable: &Rc<str>,
        body: &[Stmt],
        span: Span,
        iterable: Span,
        values: impl Iterator<Item = Value>,
    ) -> Exec<Flow> {
        let body_origin = BlockOrigin::of(body);
        let variable_id = self.scene.intern_name(variable);
        let variables: Rc<[(Rc<str>, NameId)]> = Rc::from(vec![(variable.clone(), variable_id)]);
        for (index, value) in values.enumerate() {
            let iteration = i64::try_from(index).unwrap_or(i64::MAX);
            let iteration_value = iteration_value_of(&value);
            self.store(variable.clone(), value);
            // The loop index is part of every node's provenance key, so the
            // same iteration of the same call yields the same identity. It is
            // also part of the display-name path (outermost first).
            self.loops.push(LoopFrame {
                span,
                variables: Rc::clone(&variables),
                iterable,
                body: body_origin.span,
                indent: body_origin.indent,
                index: iteration,
                value: iteration_value,
            });
            self.index_path.push(iteration);
            self.site_dirty = true;
            let flow = self.exec_block(body);
            self.index_path.pop();
            self.loops.pop();
            self.site_dirty = true;
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
                self.store(name.clone(), new.clone());
                if let Value::Node(node) = new {
                    self.name_notes.push(NameNote {
                        binding: name.to_string(),
                        node,
                        path: self.index_path.clone(),
                    });
                    let block = self
                        .blocks
                        .last()
                        .map_or_else(BlockOrigin::default, |frame| frame.origin);
                    let name_id = self.scene.intern_name(name);
                    self.scene.attach_binding(
                        node,
                        BindingOrigin {
                            name: name_id,
                            statement: span,
                            block,
                            reassigned_later: false,
                        },
                    );
                }
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
                let source = ValueSource {
                    span: value.span,
                    literal,
                    expression: if op == AssignOp::Set {
                        self.expression_origin(value, literal)
                    } else {
                        ExpressionOrigin::Computed
                    },
                };
                let write = self.write_site();
                let result = self.scene.set_attribute(id, name, &new, source, &write);
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
            let literal = literal_of(&arg.value);
            let call_value = CallValue {
                value,
                span: arg.value.span,
                literal,
                origin: self.expression_origin(&arg.value, literal),
            };
            match &arg.name {
                Some(name) => call_args.keywords.push((name.clone(), call_value)),
                None => call_args.positional.push(call_value),
            }
        }
        let site = self.site(span);
        let write = self.write_site();
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
                | Builtin::Audio
                | Builtin::Video
                | Builtin::Stagger),
            ) => self.scene.call(builtin, call_args, &site),
            Value::Builtin(builtin) => builtins::call(
                builtin,
                call_args,
                self.limits,
                &mut self.budget,
                self.metrics,
            ),
            Value::Method(id, method) => self.scene.call_method(id, method, call_args, &write),
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
        let function_id = self.scene.intern_name(&function.name);
        let parameters: Vec<(Rc<str>, NameId)> = function
            .params
            .iter()
            .map(|name| (Rc::clone(name), self.scene.intern_name(name)))
            .collect();
        self.frames.push(CallFrame {
            function: function.name.clone(),
            function_id,
            call_span: span,
            definition: function.span,
            parameters,
            locals,
        });
        self.site_dirty = true;
        // The invocation ordinal is the display-name path entry for bindings
        // created inside this call: two calls of `spinner` give `holder 1` and
        // `holder 2` instead of two nodes both named `holder`.
        let ordinal = {
            let count = self.call_counts.entry(function.name.clone()).or_insert(0);
            *count += 1;
            *count
        };
        self.index_path.push(ordinal);
        let flow = self.exec_block(&function.body);
        self.index_path.pop();
        self.frames.pop();
        self.site_dirty = true;
        let value = match flow? {
            Flow::Return(value) => value,
            Flow::Normal | Flow::Break | Flow::Continue => Value::None,
        };
        if let Value::Node(node) = &value {
            self.returned.push((*node, span));
        }
        Ok(value)
    }
}

/// The iteration value as a literal when its type can be written literally.
fn iteration_value_of(value: &Value) -> Option<IterationValue> {
    match value {
        Value::Number(number) => Some(IterationValue::Number(*number)),
        Value::Str(text) => Some(IterationValue::Text(text.to_string())),
        Value::Bool(flag) => Some(IterationValue::Bool(*flag)),
        Value::None => Some(IterationValue::None),
        _ => None,
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
