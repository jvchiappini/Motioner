//! Recursive-descent parser for the MDF v0 subset (spec section 14).
//!
//! Deferred constructs (`while`, `use`, dict literals) are recognized and
//! rejected with an [`ErrorCategory::Unsupported`] diagnostic instead of a
//! confusing syntax error.

use std::rc::Rc;

use crate::ast::{
    Argument, AssignOp, BinaryOp, CompareOp, Expr, ExprKind, FunctionDef, LogicalOp, Param,
    Program, Stmt, StmtKind, Target, UnaryOp,
};
use crate::diagnostic::{Diagnostic, ErrorCategory, Span};
use crate::lexer::tokenize;
use crate::token::{Keyword, Op, Token, TokenKind};

/// Maximum nesting of blocks and expressions. Bounds parser recursion so
/// hostile input cannot overflow the stack.
///
/// This is a *syntactic* limit: it counts blocks, parentheses and unary or
/// `not` prefixes while the parser recurses. It does not bound the depth of
/// the tree built by the left-associative operator loops or by postfix
/// chains, because those loops are iterative; [`MAX_EXPR_DEPTH`] bounds the
/// resulting tree instead.
///
/// The value is a recursion budget, not a language feature: each level of
/// parentheses walks the whole precedence chain in a debug build, so a
/// larger value would risk the parser's own stack on threads with a small
/// stack. Sixty-four levels are far beyond any hand-written or generated
/// MDF program.
const MAX_NESTING: u32 = 64;

/// Maximum depth of an expression tree, counted in AST nodes.
///
/// A node's depth is one plus the depth of its deepest child; leaves have
/// depth 1 and parentheses add no node. This is an *AST* limit, separate
/// from the parser-recursion limit `MAX_NESTING`: a flat
/// `1 + 1 + ...` chain or a long `f()()()` chain grows this depth without
/// growing parser recursion, so the parser checks it before inserting a new
/// node and rejects deeper trees with an [`ErrorCategory::Limit`]
/// diagnostic.
///
/// The value is chosen so that a tree exactly this deep parses, evaluates
/// and drops on the interpreter thread created by [`crate::run`], which has
/// a 64 MiB stack (`INTERPRETER_STACK_BYTES` in `lib.rs`). A debug build
/// was verified to accept exactly this depth and to recurse no further:
/// `tests/deep_syntax.rs` runs a 4096-node `+` chain through the public
/// [`crate::run`] API, and the next deeper tree is rejected before it is
/// built, so no evaluation or drop can ever see more than this many levels.
pub const MAX_EXPR_DEPTH: u32 = 4096;

/// An expression under construction together with the depth of its subtree.
///
/// Kept private so the public [`Expr`] shape does not change.
struct ParsedExpr {
    expr: Expr,
    depth: u32,
}

/// Builds an expression node after checking its subtree depth.
///
/// `child_depths` are the depths of the node's direct children; an empty
/// slice means the node is a leaf of depth 1. `error_span` must be the
/// operator or closing token of the construction, never a default span.
fn expression_node(
    span: Span,
    kind: ExprKind,
    child_depths: &[u32],
    error_span: Span,
) -> ParseResult<ParsedExpr> {
    let depth = 1 + child_depths.iter().copied().max().unwrap_or(0);
    if depth > MAX_EXPR_DEPTH {
        return Err(Diagnostic::new(
            ErrorCategory::Limit,
            error_span,
            format!("expression is too deeply nested; the limit is {MAX_EXPR_DEPTH}"),
        ));
    }
    Ok(ParsedExpr {
        expr: Expr { kind, span },
        depth,
    })
}

/// Parses MDF source into a [`Program`].
///
/// # Errors
///
/// Returns a lexical, parse or unsupported diagnostic with the span of
/// the first problem.
pub fn parse(source: &str) -> Result<Program, Diagnostic> {
    let tokens = tokenize(source)?;
    Parser {
        tokens,
        pos: 0,
        depth: 0,
        loop_depth: 0,
        function_depth: 0,
    }
    .program()
}

type ParseResult<T> = Result<T, Diagnostic>;

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    depth: u32,
    loop_depth: u32,
    function_depth: u32,
}

impl Parser {
    // ---- token cursor -------------------------------------------------

    fn peek(&self) -> &Token {
        // The lexer always ends with Eof and `advance` never moves past it.
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn peek_kind_at(&self, offset: usize) -> Option<&TokenKind> {
        self.tokens.get(self.pos + offset).map(|t| &t.kind)
    }

    fn advance(&mut self) -> Token {
        let token = self.peek().clone();
        if token.kind != TokenKind::Eof {
            self.pos += 1;
        }
        token
    }

    fn check_op(&self, op: Op) -> bool {
        self.peek().kind == TokenKind::Op(op)
    }

    fn eat_op(&mut self, op: Op) -> bool {
        let matched = self.check_op(op);
        if matched {
            self.advance();
        }
        matched
    }

    fn check_keyword(&self, keyword: Keyword) -> bool {
        self.peek().kind == TokenKind::Keyword(keyword)
    }

    fn expect_op(&mut self, op: Op, expected: &str) -> ParseResult<Span> {
        if self.check_op(op) {
            Ok(self.advance().span)
        } else {
            Err(self.unexpected(expected))
        }
    }

    fn expect_name(&mut self, expected: &str) -> ParseResult<(Rc<str>, Span)> {
        match &self.peek().kind {
            TokenKind::Name(name) => {
                let name = name.clone();
                Ok((name, self.advance().span))
            }
            _ => Err(self.unexpected(expected)),
        }
    }

    fn expect_kind(&mut self, kind: &TokenKind, expected: &str) -> ParseResult<()> {
        if &self.peek().kind == kind {
            self.advance();
            Ok(())
        } else {
            Err(self.unexpected(expected))
        }
    }

    fn unexpected(&self, expected: &str) -> Diagnostic {
        let token = self.peek();
        Diagnostic::new(
            ErrorCategory::Parse,
            token.span,
            format!("expected {expected}, found {}", describe(&token.kind)),
        )
    }

    /// Runs `parse` one nesting level deeper, failing past [`MAX_NESTING`].
    fn nested<T>(&mut self, parse: impl FnOnce(&mut Self) -> ParseResult<T>) -> ParseResult<T> {
        if self.depth >= MAX_NESTING {
            return Err(Diagnostic::new(
                ErrorCategory::Limit,
                self.peek().span,
                format!("code is nested more than {MAX_NESTING} levels deep"),
            ));
        }
        self.depth += 1;
        let result = parse(self);
        self.depth -= 1;
        result
    }

    // ---- statements ---------------------------------------------------

    fn program(mut self) -> ParseResult<Program> {
        let mut body = Vec::new();
        while self.peek().kind != TokenKind::Eof {
            body.push(self.statement()?);
        }
        Ok(Program { body })
    }

    fn statement(&mut self) -> ParseResult<Stmt> {
        match &self.peek().kind {
            TokenKind::Keyword(Keyword::Def) => self.def_statement(),
            TokenKind::Keyword(Keyword::If) => self.if_statement(),
            TokenKind::Keyword(Keyword::For) => self.for_statement(),
            TokenKind::Keyword(Keyword::While) => Err(unsupported(
                self.peek().span,
                "'while' loops are deferred to MDF v0.1; use 'for' with range()",
            )),
            TokenKind::Indent => Err(Diagnostic::new(
                ErrorCategory::Parse,
                self.peek().span,
                "unexpected indentation",
            )),
            _ => {
                let statement = self.simple_statement()?;
                self.expect_kind(&TokenKind::Newline, "end of line")?;
                Ok(statement)
            }
        }
    }

    fn simple_statement(&mut self) -> ParseResult<Stmt> {
        let start = self.peek().span;
        let keyword_stmt = |kind| Stmt { kind, span: start };
        match self.peek().kind {
            TokenKind::Keyword(Keyword::Return) => {
                self.advance();
                if self.function_depth == 0 {
                    return Err(parse_error(start, "'return' outside a function"));
                }
                if self.peek().kind == TokenKind::Newline {
                    return Ok(keyword_stmt(StmtKind::Return(None)));
                }
                let value = self.expression()?;
                let span = start.to(value.expr.span);
                return Ok(Stmt {
                    kind: StmtKind::Return(Some(value.expr)),
                    span,
                });
            }
            TokenKind::Keyword(keyword @ (Keyword::Break | Keyword::Continue)) => {
                self.advance();
                if self.loop_depth == 0 {
                    return Err(parse_error(
                        start,
                        "'break' and 'continue' must be inside a loop",
                    ));
                }
                let kind = if keyword == Keyword::Break {
                    StmtKind::Break
                } else {
                    StmtKind::Continue
                };
                return Ok(keyword_stmt(kind));
            }
            TokenKind::Keyword(Keyword::Pass) => {
                self.advance();
                return Ok(keyword_stmt(StmtKind::Pass));
            }
            _ => {}
        }

        let expr = self.expression()?;
        let Some(op) = self.assign_op() else {
            let span = expr.expr.span;
            return Ok(Stmt {
                kind: StmtKind::Expr(expr.expr),
                span,
            });
        };
        self.advance();
        let target = into_target(expr.expr)?;
        let value = self.expression()?;
        let span = start.to(value.expr.span);
        Ok(Stmt {
            kind: StmtKind::Assign {
                target,
                op,
                value: value.expr,
            },
            span,
        })
    }

    fn assign_op(&self) -> Option<AssignOp> {
        match self.peek().kind {
            TokenKind::Op(Op::Assign) => Some(AssignOp::Set),
            TokenKind::Op(Op::PlusAssign) => Some(AssignOp::Add),
            TokenKind::Op(Op::MinusAssign) => Some(AssignOp::Sub),
            TokenKind::Op(Op::StarAssign) => Some(AssignOp::Mul),
            TokenKind::Op(Op::SlashAssign) => Some(AssignOp::Div),
            _ => None,
        }
    }

    /// Parses `: NEWLINE INDENT statement+ DEDENT`.
    fn block(&mut self) -> ParseResult<Vec<Stmt>> {
        self.expect_op(Op::Colon, "':'")?;
        self.expect_kind(&TokenKind::Newline, "a new line after ':'")?;
        self.expect_kind(&TokenKind::Indent, "an indented block")?;
        self.nested(|p| {
            let mut body = Vec::new();
            while !matches!(p.peek().kind, TokenKind::Dedent | TokenKind::Eof) {
                body.push(p.statement()?);
            }
            p.expect_kind(&TokenKind::Dedent, "end of block")?;
            Ok(body)
        })
    }

    fn def_statement(&mut self) -> ParseResult<Stmt> {
        let start = self.advance().span;
        let (name, _) = self.expect_name("a function name")?;
        self.expect_op(Op::LParen, "'('")?;
        let mut params: Vec<Param> = Vec::new();
        let mut seen_default = false;
        while !self.check_op(Op::RParen) {
            let (name, span) = self.expect_name("a parameter name")?;
            if params.iter().any(|param| param.name == name) {
                return Err(parse_error(span, format!("duplicate parameter '{name}'")));
            }
            let default = if self.eat_op(Op::Assign) {
                Some(self.expression()?.expr)
            } else {
                None
            };
            if default.is_none() && seen_default {
                return Err(parse_error(
                    span,
                    format!("non-default parameter '{name}' follows a default parameter"),
                ));
            }
            seen_default |= default.is_some();
            params.push(Param { name, default });
            if !self.eat_op(Op::Comma) {
                break;
            }
        }
        let header_end = self.expect_op(Op::RParen, "')'")?;
        let span = start.to(header_end);

        let saved_loops = std::mem::replace(&mut self.loop_depth, 0);
        self.function_depth += 1;
        let body = self.block();
        self.function_depth -= 1;
        self.loop_depth = saved_loops;

        let function = FunctionDef {
            name,
            params,
            body: body?,
            span,
        };
        Ok(Stmt {
            kind: StmtKind::Def(Rc::new(function)),
            span,
        })
    }

    fn if_statement(&mut self) -> ParseResult<Stmt> {
        let start = self.advance().span;
        let mut branches = vec![(self.expression()?.expr, self.block()?)];
        let mut otherwise = Vec::new();
        loop {
            if self.check_keyword(Keyword::Elif) {
                self.advance();
                branches.push((self.expression()?.expr, self.block()?));
            } else if self.check_keyword(Keyword::Else) {
                self.advance();
                otherwise = self.block()?;
                break;
            } else {
                break;
            }
        }
        let mut span = start;
        for (_, body) in &branches {
            if let Some(last) = body.last() {
                span = span.to(last.span);
            }
        }
        if let Some(last) = otherwise.last() {
            span = span.to(last.span);
        }
        Ok(Stmt {
            kind: StmtKind::If {
                branches,
                otherwise,
            },
            span,
        })
    }

    fn for_statement(&mut self) -> ParseResult<Stmt> {
        let start = self.advance().span;
        let variables = self.for_variables()?;
        self.expect_kind(&TokenKind::Keyword(Keyword::In), "'in'")?;
        let iterable = self.expression()?;
        self.loop_depth += 1;
        let body = self.block()?;
        self.loop_depth -= 1;
        let span = body.last().map_or(start, |last| start.to(last.span));
        Ok(Stmt {
            kind: StmtKind::For {
                variables,
                iterable: iterable.expr,
                body,
            },
            span,
        })
    }

    /// Parses `x`, `i, x` or `(i, x)` before the `in` of a `for`.
    fn for_variables(&mut self) -> ParseResult<Vec<Rc<str>>> {
        let parenthesized = self.eat_op(Op::LParen);
        let mut variables = Vec::new();
        loop {
            let (name, span) = self.expect_name("a loop variable name")?;
            if variables.contains(&name) {
                return Err(parse_error(
                    span,
                    format!("duplicate loop variable '{name}'"),
                ));
            }
            variables.push(name);
            if !self.eat_op(Op::Comma) {
                break;
            }
            if parenthesized && self.check_op(Op::RParen) {
                break;
            }
        }
        if parenthesized {
            self.expect_op(Op::RParen, "')'")?;
        }
        Ok(variables)
    }

    // ---- expressions --------------------------------------------------

    fn expression(&mut self) -> ParseResult<ParsedExpr> {
        self.nested(Self::or_expr)
    }

    fn or_expr(&mut self) -> ParseResult<ParsedExpr> {
        let mut left = self.and_expr()?;
        while self.check_keyword(Keyword::Or) {
            let operator = self.advance().span;
            let right = self.and_expr()?;
            let child_depths = [left.depth, right.depth];
            let span = left.expr.span.to(right.expr.span);
            left = expression_node(
                span,
                ExprKind::Logical {
                    op: LogicalOp::Or,
                    left: Box::new(left.expr),
                    right: Box::new(right.expr),
                },
                &child_depths,
                operator,
            )?;
        }
        Ok(left)
    }

    fn and_expr(&mut self) -> ParseResult<ParsedExpr> {
        let mut left = self.not_expr()?;
        while self.check_keyword(Keyword::And) {
            let operator = self.advance().span;
            let right = self.not_expr()?;
            let child_depths = [left.depth, right.depth];
            let span = left.expr.span.to(right.expr.span);
            left = expression_node(
                span,
                ExprKind::Logical {
                    op: LogicalOp::And,
                    left: Box::new(left.expr),
                    right: Box::new(right.expr),
                },
                &child_depths,
                operator,
            )?;
        }
        Ok(left)
    }

    fn not_expr(&mut self) -> ParseResult<ParsedExpr> {
        if !self.check_keyword(Keyword::Not) {
            return self.comparison();
        }
        let start = self.advance().span;
        let operand = self.nested(Self::not_expr)?;
        let span = start.to(operand.expr.span);
        let child_depths = [operand.depth];
        expression_node(
            span,
            ExprKind::Not(Box::new(operand.expr)),
            &child_depths,
            start,
        )
    }

    fn comparison(&mut self) -> ParseResult<ParsedExpr> {
        let left = self.additive()?;
        let Some(op) = self.compare_op() else {
            return Ok(left);
        };
        let operator = self.advance().span;
        if op == CompareOp::NotIn {
            self.expect_kind(&TokenKind::Keyword(Keyword::In), "'in' after 'not'")?;
        }
        let right = self.additive()?;
        if self.compare_op().is_some() {
            return Err(unsupported(
                self.peek().span,
                "chained comparisons are not supported; combine them with 'and'",
            ));
        }
        let child_depths = [left.depth, right.depth];
        let span = left.expr.span.to(right.expr.span);
        expression_node(
            span,
            ExprKind::Compare {
                op,
                left: Box::new(left.expr),
                right: Box::new(right.expr),
            },
            &child_depths,
            operator,
        )
    }

    fn compare_op(&self) -> Option<CompareOp> {
        match self.peek().kind {
            TokenKind::Op(Op::EqEq) => Some(CompareOp::Eq),
            TokenKind::Op(Op::NotEq) => Some(CompareOp::NotEq),
            TokenKind::Op(Op::Lt) => Some(CompareOp::Lt),
            TokenKind::Op(Op::Le) => Some(CompareOp::Le),
            TokenKind::Op(Op::Gt) => Some(CompareOp::Gt),
            TokenKind::Op(Op::Ge) => Some(CompareOp::Ge),
            TokenKind::Keyword(Keyword::In) => Some(CompareOp::In),
            TokenKind::Keyword(Keyword::Not)
                if self.peek_kind_at(1) == Some(&TokenKind::Keyword(Keyword::In)) =>
            {
                Some(CompareOp::NotIn)
            }
            _ => None,
        }
    }

    fn additive(&mut self) -> ParseResult<ParsedExpr> {
        let mut left = self.multiplicative()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Op(Op::Plus) => BinaryOp::Add,
                TokenKind::Op(Op::Minus) => BinaryOp::Sub,
                _ => return Ok(left),
            };
            let operator = self.advance().span;
            let right = self.multiplicative()?;
            let child_depths = [left.depth, right.depth];
            let span = left.expr.span.to(right.expr.span);
            left = expression_node(
                span,
                ExprKind::Binary {
                    op,
                    left: Box::new(left.expr),
                    right: Box::new(right.expr),
                },
                &child_depths,
                operator,
            )?;
        }
    }

    fn multiplicative(&mut self) -> ParseResult<ParsedExpr> {
        let mut left = self.unary()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Op(Op::Star) => BinaryOp::Mul,
                TokenKind::Op(Op::Slash) => BinaryOp::Div,
                TokenKind::Op(Op::SlashSlash) => BinaryOp::FloorDiv,
                TokenKind::Op(Op::Percent) => BinaryOp::Mod,
                _ => return Ok(left),
            };
            let operator = self.advance().span;
            let right = self.unary()?;
            let child_depths = [left.depth, right.depth];
            let span = left.expr.span.to(right.expr.span);
            left = expression_node(
                span,
                ExprKind::Binary {
                    op,
                    left: Box::new(left.expr),
                    right: Box::new(right.expr),
                },
                &child_depths,
                operator,
            )?;
        }
    }

    fn unary(&mut self) -> ParseResult<ParsedExpr> {
        let op = match self.peek().kind {
            TokenKind::Op(Op::Minus) => UnaryOp::Neg,
            TokenKind::Op(Op::Plus) => UnaryOp::Pos,
            _ => return self.postfix(),
        };
        let start = self.advance().span;
        let operand = self.nested(Self::unary)?;
        let span = start.to(operand.expr.span);
        let child_depths = [operand.depth];
        expression_node(
            span,
            ExprKind::Unary {
                op,
                operand: Box::new(operand.expr),
            },
            &child_depths,
            start,
        )
    }

    fn postfix(&mut self) -> ParseResult<ParsedExpr> {
        let mut expr = self.primary()?;
        loop {
            if self.eat_op(Op::Dot) {
                let (name, end) = self.expect_name("an attribute name")?;
                let span = expr.expr.span.to(end);
                let child_depths = [expr.depth];
                expr = expression_node(
                    span,
                    ExprKind::Attribute {
                        object: Box::new(expr.expr),
                        name,
                    },
                    &child_depths,
                    end,
                )?;
            } else if self.eat_op(Op::LBracket) {
                let index = self.expression()?;
                let end = self.expect_op(Op::RBracket, "']'")?;
                let span = expr.expr.span.to(end);
                let child_depths = [expr.depth, index.depth];
                expr = expression_node(
                    span,
                    ExprKind::Index {
                        object: Box::new(expr.expr),
                        index: Box::new(index.expr),
                    },
                    &child_depths,
                    end,
                )?;
            } else if self.eat_op(Op::LParen) {
                let args = self.arguments()?;
                let end = self.expect_op(Op::RParen, "')'")?;
                let span = expr.expr.span.to(end);
                let mut child_depths = Vec::with_capacity(args.len() + 1);
                child_depths.push(expr.depth);
                child_depths.extend(args.iter().map(|(_, value)| value.depth));
                let arguments = args
                    .into_iter()
                    .map(|(name, value)| Argument {
                        name,
                        value: value.expr,
                    })
                    .collect();
                expr = expression_node(
                    span,
                    ExprKind::Call {
                        callee: Box::new(expr.expr),
                        args: arguments,
                    },
                    &child_depths,
                    end,
                )?;
            } else {
                return Ok(expr);
            }
        }
    }

    fn arguments(&mut self) -> ParseResult<Vec<(Option<Rc<str>>, ParsedExpr)>> {
        let mut args: Vec<(Option<Rc<str>>, ParsedExpr)> = Vec::new();
        while !self.check_op(Op::RParen) {
            let is_keyword = matches!(self.peek().kind, TokenKind::Name(_))
                && self.peek_kind_at(1) == Some(&TokenKind::Op(Op::Assign));
            if is_keyword {
                let (name, span) = self.expect_name("an argument name")?;
                self.advance();
                if args
                    .iter()
                    .any(|(existing, _)| existing.as_ref() == Some(&name))
                {
                    return Err(parse_error(
                        span,
                        format!("keyword argument '{name}' repeated"),
                    ));
                }
                let value = self.expression()?;
                args.push((Some(name), value));
            } else {
                let value = self.expression()?;
                if args.iter().any(|(name, _)| name.is_some()) {
                    return Err(parse_error(
                        value.expr.span,
                        "positional argument follows keyword argument",
                    ));
                }
                args.push((None, value));
            }
            if !self.eat_op(Op::Comma) {
                break;
            }
        }
        Ok(args)
    }

    fn primary(&mut self) -> ParseResult<ParsedExpr> {
        let token = self.peek().clone();
        let literal = |kind| {
            Ok(ParsedExpr {
                expr: Expr {
                    kind,
                    span: token.span,
                },
                depth: 1,
            })
        };
        match &token.kind {
            TokenKind::Number(value) => {
                self.advance();
                literal(ExprKind::Number(*value))
            }
            TokenKind::Str(value) => {
                self.advance();
                literal(ExprKind::Str(value.clone()))
            }
            TokenKind::Name(name) => {
                self.advance();
                literal(ExprKind::Name(name.clone()))
            }
            TokenKind::Keyword(Keyword::True) => {
                self.advance();
                literal(ExprKind::Bool(true))
            }
            TokenKind::Keyword(Keyword::False) => {
                self.advance();
                literal(ExprKind::Bool(false))
            }
            TokenKind::Keyword(Keyword::None) => {
                self.advance();
                literal(ExprKind::None)
            }
            TokenKind::Op(Op::LParen) => self.parenthesized(),
            TokenKind::Op(Op::LBracket) => {
                self.advance();
                let items = self.expression_list(Op::RBracket)?;
                let end = self.expect_op(Op::RBracket, "']'")?;
                let span = token.span.to(end);
                let child_depths: Vec<u32> = items.iter().map(|item| item.depth).collect();
                let values = items.into_iter().map(|item| item.expr).collect();
                expression_node(span, ExprKind::List(values), &child_depths, end)
            }
            TokenKind::Op(Op::LBrace) => Err(unsupported(
                token.span,
                "dict literals are deferred to MDF v0.1",
            )),
            TokenKind::Keyword(Keyword::Use) => Err(unsupported(
                token.span,
                "'use' modules are deferred to MDF v0.1",
            )),
            _ => Err(self.unexpected("an expression")),
        }
    }

    /// Parses `()`, `(expr)` or a tuple `(a, b, ...)`.
    fn parenthesized(&mut self) -> ParseResult<ParsedExpr> {
        let start = self.advance().span;
        if let Ok(end) = self.expect_op(Op::RParen, "')'") {
            return Ok(ParsedExpr {
                expr: Expr {
                    kind: ExprKind::Tuple(Vec::new()),
                    span: start.to(end),
                },
                depth: 1,
            });
        }
        let first = self.expression()?;
        if !self.eat_op(Op::Comma) {
            let end = self.expect_op(Op::RParen, "')'")?;
            return Ok(ParsedExpr {
                expr: Expr {
                    span: start.to(end),
                    ..first.expr
                },
                depth: first.depth,
            });
        }
        let mut items = vec![first];
        items.extend(self.expression_list(Op::RParen)?);
        let end = self.expect_op(Op::RParen, "')'")?;
        let span = start.to(end);
        let child_depths: Vec<u32> = items.iter().map(|item| item.depth).collect();
        let values = items.into_iter().map(|item| item.expr).collect();
        expression_node(span, ExprKind::Tuple(values), &child_depths, end)
    }

    /// Parses comma-separated expressions up to (not including) `close`,
    /// allowing a trailing comma.
    fn expression_list(&mut self, close: Op) -> ParseResult<Vec<ParsedExpr>> {
        let mut items = Vec::new();
        while !self.check_op(close) {
            items.push(self.expression()?);
            if !self.eat_op(Op::Comma) {
                break;
            }
        }
        Ok(items)
    }
}

fn into_target(expr: Expr) -> ParseResult<Target> {
    match expr.kind {
        ExprKind::Name(name) => Ok(Target::Name(name)),
        ExprKind::Attribute { object, name } => Ok(Target::Attribute {
            object: *object,
            name,
        }),
        ExprKind::Index { object, index } => Ok(Target::Index {
            object: *object,
            index: *index,
        }),
        _ => Err(parse_error(
            expr.span,
            "cannot assign to this expression; assign to a name, attribute or item",
        )),
    }
}

fn parse_error(span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(ErrorCategory::Parse, span, message)
}

fn unsupported(span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(ErrorCategory::Unsupported, span, message)
}

fn describe(kind: &TokenKind) -> String {
    match kind {
        TokenKind::Name(name) => format!("name '{name}'"),
        TokenKind::Number(value) => format!("number {value}"),
        TokenKind::Str(_) => "a string".to_owned(),
        TokenKind::Keyword(keyword) => {
            format!("keyword '{}'", format!("{keyword:?}").to_lowercase())
        }
        TokenKind::Op(op) => format!("'{}'", op_symbol(*op)),
        TokenKind::Newline => "end of line".to_owned(),
        TokenKind::Indent => "indentation".to_owned(),
        TokenKind::Dedent => "end of block".to_owned(),
        TokenKind::Eof => "end of file".to_owned(),
    }
}

fn op_symbol(op: Op) -> &'static str {
    match op {
        Op::Plus => "+",
        Op::Minus => "-",
        Op::Star => "*",
        Op::Slash => "/",
        Op::SlashSlash => "//",
        Op::Percent => "%",
        Op::Assign => "=",
        Op::PlusAssign => "+=",
        Op::MinusAssign => "-=",
        Op::StarAssign => "*=",
        Op::SlashAssign => "/=",
        Op::EqEq => "==",
        Op::NotEq => "!=",
        Op::Lt => "<",
        Op::Le => "<=",
        Op::Gt => ">",
        Op::Ge => ">=",
        Op::LParen => "(",
        Op::RParen => ")",
        Op::LBracket => "[",
        Op::RBracket => "]",
        Op::LBrace => "{",
        Op::RBrace => "}",
        Op::Comma => ",",
        Op::Colon => ":",
        Op::Dot => ".",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(source: &str) -> Program {
        parse(source).unwrap_or_else(|e| panic!("{e}\n{source}"))
    }

    fn only_expr(source: &str) -> ExprKind {
        match parse_ok(source).body.remove(0).kind {
            StmtKind::Expr(expr) => expr.kind,
            other => panic!("not an expression: {other:?}"),
        }
    }

    #[test]
    fn respects_operator_precedence() {
        let ExprKind::Logical { op, left, .. } = only_expr("not a == 1 + 2 * -3 or b") else {
            panic!("expected 'or' at the root");
        };
        assert_eq!(op, LogicalOp::Or);
        let ExprKind::Not(inner) = left.kind else {
            panic!("expected 'not'");
        };
        let ExprKind::Compare { right, .. } = inner.kind else {
            panic!("expected comparison");
        };
        let ExprKind::Binary { op, right, .. } = right.kind else {
            panic!("expected '+'");
        };
        assert_eq!(op, BinaryOp::Add);
        assert!(matches!(
            right.kind,
            ExprKind::Binary {
                op: BinaryOp::Mul,
                ..
            }
        ));
    }

    #[test]
    fn parses_calls_with_keywords_attributes_and_indexing() {
        let ExprKind::Call { callee, args } = only_expr("a.b[0](1, size=2,)") else {
            panic!("expected call");
        };
        assert!(matches!(callee.kind, ExprKind::Index { .. }));
        assert_eq!(args.len(), 2);
        assert_eq!(args[1].name.as_deref(), Some("size"));
    }

    #[test]
    fn parses_tuples_lists_and_grouping() {
        assert!(matches!(only_expr("(1, 2)"), ExprKind::Tuple(items) if items.len() == 2));
        assert!(matches!(only_expr("(1,)"), ExprKind::Tuple(items) if items.len() == 1));
        assert!(matches!(only_expr("()"), ExprKind::Tuple(items) if items.is_empty()));
        assert!(matches!(only_expr("(1)"), ExprKind::Number(_)));
        assert!(matches!(only_expr("[1, [2], 3,]"), ExprKind::List(items) if items.len() == 3));
    }

    #[test]
    fn parses_statements_and_blocks() {
        let source = "\
def f(a, b):
    for i in range(a):
        if i == b:
            break
        elif i < b:
            continue
        else:
            pass
    return a

x = f(3, 1)
x += 2
x.pos = (1, 2)
x[0] = 1
";
        let program = parse_ok(source);
        assert_eq!(program.body.len(), 5);
        let StmtKind::Def(def) = &program.body[0].kind else {
            panic!("expected def");
        };
        assert_eq!(&*def.name, "f");
        assert_eq!(def.params.len(), 2);
        assert!(matches!(
            program.body[2].kind,
            StmtKind::Assign {
                op: AssignOp::Add,
                ..
            }
        ));
        assert!(matches!(
            program.body[3].kind,
            StmtKind::Assign {
                target: Target::Attribute { .. },
                ..
            }
        ));
    }

    #[test]
    fn rejects_bad_syntax_with_spans() {
        for (source, category, line, column) in [
            ("x = = 1", ErrorCategory::Parse, 1, 5),
            ("f(a=1, 2)", ErrorCategory::Parse, 1, 8),
            ("1 = x", ErrorCategory::Parse, 1, 1),
            ("if x\n    y", ErrorCategory::Parse, 1, 5),
            ("return 1", ErrorCategory::Parse, 1, 1),
            ("break", ErrorCategory::Parse, 1, 1),
            (
                "def f():\n    for i in x:\n        def g():\n            break",
                ErrorCategory::Parse,
                4,
                13,
            ),
            ("while x:\n    pass", ErrorCategory::Unsupported, 1, 1),
            ("m = use('a')", ErrorCategory::Unsupported, 1, 5),
            ("d = {}", ErrorCategory::Unsupported, 1, 5),
            ("a < b < c", ErrorCategory::Unsupported, 1, 7),
            ("    x = 1", ErrorCategory::Parse, 1, 1),
            ("def f(a, a):\n    pass", ErrorCategory::Parse, 1, 10),
        ] {
            let error = parse(source).unwrap_err();
            assert_eq!(error.category, category, "{source}: {error}");
            assert_eq!(
                (error.span.line, error.span.column),
                (line, column),
                "{source}: {error}"
            );
        }
    }

    fn flat_add(terms: u32) -> String {
        let mut source = String::from("1");
        for _ in 1..terms {
            source.push_str(" + 1");
        }
        source
    }

    #[test]
    fn deep_nesting_is_a_limit_error() {
        let source = format!("x = {}1{}", "(".repeat(500), ")".repeat(500));
        let error = parse(&source).unwrap_err();
        assert_eq!(error.category, ErrorCategory::Limit);
        assert!(error.message.contains("nested more than"), "{error}");
        let source = format!("x = {}1", "-".repeat(500));
        let error = parse(&source).unwrap_err();
        assert_eq!(error.category, ErrorCategory::Limit);
        assert!(error.message.contains("nested more than"), "{error}");
    }

    #[test]
    fn expression_depth_limit_is_enforced() {
        let at_limit = format!("x = {}\n", flat_add(MAX_EXPR_DEPTH));
        assert!(parse(&at_limit).is_ok());

        let past_limit = format!("x = {}\n", flat_add(MAX_EXPR_DEPTH + 1));
        let error = parse(&past_limit).unwrap_err();
        assert_eq!(error.category, ErrorCategory::Limit);
        assert!(error.message.contains("too deeply nested"), "{error}");
        assert!(error.span.line >= 1 && error.span.column >= 1, "{error}");
    }
}
