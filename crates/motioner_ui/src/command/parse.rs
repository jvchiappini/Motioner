//! Parsing commands from text with the MDF parser.

use std::fmt;

use motioner_core::{Ease, EffectKind, MaskMode, OverrideValue, Vec2};
use motioner_dsl::ast::{Argument, Expr, ExprKind, StmtKind, UnaryOp};

use super::{Command, CommandError, CommandKind, ErrorKind, KeySpec, Panel, Time};

/// One parsed call: its name and arguments, before typing.
#[derive(Debug, Clone)]
pub struct Call {
    /// Function name.
    pub name: String,
    args: Vec<Argument>,
}

impl Call {
    /// The call's arguments.
    pub(crate) fn into_args(self) -> Vec<Argument> {
        self.args
    }
}

/// Parses text into calls (one per line or statement) with the MDF parser.
///
/// # Errors
///
/// Returns [`ErrorKind::Parse`] when the text is not MDF, or when a
/// statement is not a plain call such as `seek(10)`.
pub fn parse_calls(text: &str) -> Result<Vec<Call>, CommandError> {
    let program = motioner_dsl::parse(text).map_err(|diagnostic| {
        CommandError::new(
            ErrorKind::Parse,
            format!("{} (line {})", diagnostic.message, diagnostic.span.line),
        )
    })?;
    let mut calls = Vec::new();
    for statement in program.body {
        let StmtKind::Expr(expr) = statement.kind else {
            return Err(CommandError::new(
                ErrorKind::Parse,
                format!(
                    "line {}: commands are calls like seek(10)",
                    statement.span.line
                ),
            ));
        };
        let ExprKind::Call { callee, args } = expr.kind else {
            return Err(CommandError::new(
                ErrorKind::Parse,
                format!("line {}: commands are calls like seek(10)", expr.span.line),
            ));
        };
        let ExprKind::Name(name) = callee.kind else {
            return Err(CommandError::new(
                ErrorKind::Parse,
                "a command name must be a plain name",
            ));
        };
        calls.push(Call {
            name: name.to_string(),
            args,
        });
    }
    Ok(calls)
}

/// Parses text into commands.
///
/// # Errors
///
/// Returns the first parse, unknown-command or argument error.
pub fn parse_commands(text: &str) -> Result<Vec<Command>, CommandError> {
    parse_calls(text)?
        .into_iter()
        .map(Command::from_call)
        .collect()
}

impl Command {
    /// Types one parsed call.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorKind::UnknownCommand`] for an unknown name and
    /// [`ErrorKind::BadArgument`] for wrong arguments.
    #[allow(clippy::too_many_lines)] // one arm per command kind
    pub fn from_call(call: Call) -> Result<Self, CommandError> {
        let Some(kind) = CommandKind::from_name(&call.name) else {
            return Err(CommandError::new(
                ErrorKind::UnknownCommand,
                format!("unknown command '{}'", call.name),
            ));
        };
        let mut a = Args::new(kind, call.args);
        let command = match kind {
            CommandKind::Play => Self::Play,
            CommandKind::Pause => Self::Pause,
            CommandKind::TogglePlay => Self::TogglePlay,
            CommandKind::Step => Self::Step(a.int("frames")?),
            CommandKind::Seek => Self::Seek(a.int("frame")?),
            CommandKind::GoToStart => Self::GoToStart,
            CommandKind::GoToEnd => Self::GoToEnd,
            CommandKind::Loop => Self::Loop(a.boolean("on")?),
            CommandKind::PrevKey => Self::PrevKey {
                id: a.string("id")?,
                property: a.string("property")?,
            },
            CommandKind::NextKey => Self::NextKey {
                id: a.string("id")?,
                property: a.string("property")?,
            },
            CommandKind::Select => Self::Select(a.string("id")?),
            CommandKind::Deselect => Self::Deselect,
            CommandKind::SelectRelative => Self::SelectRelative(a.int("rows")?),
            CommandKind::Expand => Self::Expand {
                id: a.string("id")?,
                open: a.optional_boolean("open")?.unwrap_or(true),
            },
            CommandKind::ExpandAll => Self::ExpandAll(a.boolean("open")?),
            CommandKind::SelectKeys => Self::SelectKeys {
                keys: a.keys("keys")?,
                add: a.optional_boolean("add")?.unwrap_or(false),
            },
            CommandKind::ClearKeys => Self::ClearKeys,
            CommandKind::SafeArea => Self::SafeArea(a.boolean("on")?),
            CommandKind::Snap => Self::Snap(a.boolean("on")?),
            CommandKind::OnlyAnimated => Self::OnlyAnimated(a.boolean("on")?),
            CommandKind::Zoom => Self::Zoom(a.number("factor")?),
            CommandKind::ZoomFit => Self::ZoomFit,
            CommandKind::ZoomBy => Self::ZoomBy {
                steps: a.int("steps")?,
                x: a.optional_number("x")?,
                y: a.optional_number("y")?,
            },
            CommandKind::Pan => Self::Pan {
                dx: a.number("dx")?,
                dy: a.number("dy")?,
            },
            CommandKind::PanelSize => Self::PanelSize {
                panel: a.panel()?,
                size: a.number("size")?,
            },
            CommandKind::Scroll => Self::Scroll {
                panel: a.panel()?,
                offset: a.number("offset")?,
            },
            CommandKind::CopyId => Self::CopyId,
            CommandKind::Reload => Self::Reload,
            CommandKind::Quit => Self::Quit,
            CommandKind::Set => Self::Set {
                id: a.string("id")?,
                property: a.string("property")?,
                value: a.value("value")?,
            },
            CommandKind::Edit => Self::Edit {
                id: a.string("id")?,
                property: a.string("property")?,
                value: a.value("value")?,
            },
            CommandKind::Key => Self::Key {
                id: a.string("id")?,
                property: a.string("property")?,
                time: a.time("time")?,
                value: a.value("value")?,
                ease: a.optional_ease("ease")?,
            },
            CommandKind::Unkey => Self::Unkey {
                id: a.string("id")?,
                property: a.string("property")?,
                time: a.time("time")?,
            },
            CommandKind::MoveKey => Self::MoveKey {
                id: a.string("id")?,
                property: a.string("property")?,
                from: a.time("from_time")?,
                to: a.time("to_time")?,
            },
            CommandKind::Ease => Self::Ease {
                id: a.string("id")?,
                property: a.string("property")?,
                time: a.time("time")?,
                ease: a.ease("name")?,
            },
            CommandKind::SetEase => Self::SetEase(a.ease("name")?),
            CommandKind::Stopwatch => Self::Stopwatch {
                id: a.string("id")?,
                property: a.string("property")?,
            },
            CommandKind::ToggleKey => Self::ToggleKey {
                id: a.string("id")?,
                property: a.string("property")?,
            },
            CommandKind::AddKeys => Self::AddKeys,
            CommandKind::DeleteKeys => Self::DeleteKeys,
            CommandKind::RetimeKeys => Self::RetimeKeys(a.int("frames")?),
            CommandKind::Visible => Self::Visible {
                id: a.string("id")?,
                visible: a.boolean("on")?,
            },
            CommandKind::Lock => Self::Lock {
                id: a.string("id")?,
                locked: a.boolean("on")?,
            },
            CommandKind::Rename => Self::Rename {
                id: a.string("id")?,
                name: a.string("name")?,
            },
            CommandKind::AssignId => Self::AssignId {
                from: a.string("from")?,
                to: a.string("to")?,
            },
            CommandKind::DeleteOrphan => Self::DeleteOrphan {
                id: a.string("id")?,
                property: a.string("property")?,
                time: a.optional_time("time")?,
            },
            CommandKind::Undo => Self::Undo,
            CommandKind::Redo => Self::Redo,
            CommandKind::EffectAdd => Self::EffectAdd {
                id: a.string("id")?,
                kind: effect_kind(&a.string("kind")?)?,
            },
            CommandKind::EffectRemove => Self::EffectRemove {
                id: a.string("id")?,
                kind: effect_kind(&a.string("kind")?)?,
            },
            CommandKind::EffectMove => Self::EffectMove {
                id: a.string("id")?,
                kind: effect_kind(&a.string("kind")?)?,
                index: usize::try_from(a.int("index")?).map_err(|_| {
                    CommandError::new(ErrorKind::BadArgument, "'index' must be zero or above")
                })?,
            },
            CommandKind::MaskSet => Self::MaskSet {
                id: a.string("id")?,
                source: a.string("source")?,
                mode: match a.optional_string("mode")? {
                    Some(name) => mask_mode(&name)?,
                    None => MaskMode::Alpha,
                },
                invert: a.optional_boolean("invert")?.unwrap_or(false),
            },
            CommandKind::MaskClear => Self::MaskClear {
                id: a.string("id")?,
            },
            CommandKind::Bake => Self::Bake(a.rest_strings()?),
        };
        a.finish()?;
        Ok(command)
    }
}

/// Parses an effect kind name from a command argument.
fn mask_mode(name: &str) -> Result<MaskMode, CommandError> {
    MaskMode::from_name(name).ok_or_else(|| {
        CommandError::new(
            ErrorKind::BadArgument,
            format!("unknown mask mode {name:?}; expected alpha or luma"),
        )
    })
}

fn effect_kind(name: &str) -> Result<EffectKind, CommandError> {
    EffectKind::from_name(name).ok_or_else(|| {
        CommandError::new(
            ErrorKind::BadArgument,
            format!("unknown effect kind {name:?}; expected drop_shadow, blur or glow"),
        )
    })
}

/// Typed access to a call's arguments, positional first, then keywords.
pub(crate) struct Args {
    signature: &'static str,
    positional: std::collections::VecDeque<Expr>,
    keywords: Vec<(String, Expr)>,
}

impl Args {
    fn new(kind: CommandKind, args: Vec<Argument>) -> Self {
        Self::with_signature(kind.signature(), args)
    }

    /// Arguments of a call with this signature (used by requests too).
    pub(crate) fn with_signature(signature: &'static str, args: Vec<Argument>) -> Self {
        let mut positional = std::collections::VecDeque::new();
        let mut keywords = Vec::new();
        for arg in args {
            match arg.name {
                Some(name) => keywords.push((name.to_string(), arg.value)),
                None => positional.push_back(arg.value),
            }
        }
        Self {
            signature,
            positional,
            keywords,
        }
    }

    fn error(&self, message: impl fmt::Display) -> CommandError {
        CommandError::new(
            ErrorKind::BadArgument,
            format!("{message}; usage: {}", self.signature),
        )
    }

    /// The next argument: positional, or the keyword `name`.
    fn take(&mut self, name: &str) -> Option<Expr> {
        if let Some(index) = self.keywords.iter().position(|(key, _)| key == name) {
            return Some(self.keywords.remove(index).1);
        }
        self.positional.pop_front()
    }

    fn required(&mut self, name: &str) -> Result<Expr, CommandError> {
        self.take(name)
            .ok_or_else(|| self.error(format!("missing argument '{name}'")))
    }

    pub(crate) fn string(&mut self, name: &str) -> Result<String, CommandError> {
        let expr = self.required(name)?;
        match expr.kind {
            ExprKind::Str(text) => Ok(text.to_string()),
            _ => Err(self.error(format!("'{name}' must be a string"))),
        }
    }

    pub(crate) fn optional_string(&mut self, name: &str) -> Result<Option<String>, CommandError> {
        match self.take(name) {
            None
            | Some(Expr {
                kind: ExprKind::None,
                ..
            }) => Ok(None),
            Some(Expr {
                kind: ExprKind::Str(text),
                ..
            }) => Ok(Some(text.to_string())),
            Some(_) => Err(self.error(format!("'{name}' must be a string"))),
        }
    }

    pub(crate) fn number(&mut self, name: &str) -> Result<f64, CommandError> {
        let expr = self.required(name)?;
        number_of(&expr).ok_or_else(|| self.error(format!("'{name}' must be a number")))
    }

    fn optional_number(&mut self, name: &str) -> Result<Option<f64>, CommandError> {
        match self.take(name) {
            None
            | Some(Expr {
                kind: ExprKind::None,
                ..
            }) => Ok(None),
            Some(expr) => number_of(&expr)
                .map(Some)
                .ok_or_else(|| self.error(format!("'{name}' must be a number"))),
        }
    }

    pub(crate) fn int(&mut self, name: &str) -> Result<i64, CommandError> {
        let value = self.number(name)?;
        if value.fract() != 0.0 || !value.is_finite() || value.abs() > 1e15 {
            return Err(self.error(format!("'{name}' must be a whole number")));
        }
        // Checked above: whole and well inside the i64 range.
        #[allow(clippy::cast_possible_truncation)]
        Ok(value as i64)
    }

    fn boolean(&mut self, name: &str) -> Result<bool, CommandError> {
        let expr = self.required(name)?;
        bool_of(&expr).ok_or_else(|| self.error(format!("'{name}' must be true or false")))
    }

    fn optional_boolean(&mut self, name: &str) -> Result<Option<bool>, CommandError> {
        match self.take(name) {
            None => Ok(None),
            Some(expr) => bool_of(&expr)
                .map(Some)
                .ok_or_else(|| self.error(format!("'{name}' must be true or false"))),
        }
    }

    fn time(&mut self, name: &str) -> Result<Time, CommandError> {
        let expr = self.required(name)?;
        time_of(&expr).ok_or_else(|| self.error(format!("'{name}' must be seconds or frame(N)")))
    }

    fn optional_time(&mut self, name: &str) -> Result<Option<Time>, CommandError> {
        match self.take(name) {
            None
            | Some(Expr {
                kind: ExprKind::None,
                ..
            }) => Ok(None),
            Some(expr) => time_of(&expr)
                .map(Some)
                .ok_or_else(|| self.error(format!("'{name}' must be seconds or frame(N)"))),
        }
    }

    fn value(&mut self, name: &str) -> Result<OverrideValue, CommandError> {
        let expr = self.required(name)?;
        if let Some(number) = number_of(&expr) {
            return Ok(OverrideValue::Number(number));
        }
        match expr.kind {
            ExprKind::Str(text) => Ok(OverrideValue::Text(text.to_string())),
            ExprKind::Tuple(items) if items.len() == 2 => {
                match (number_of(&items[0]), number_of(&items[1])) {
                    (Some(x), Some(y)) => Ok(OverrideValue::Vec2(Vec2::new(x, y))),
                    _ => Err(self.error("a pair must hold two numbers")),
                }
            }
            _ => Err(self.error(format!(
                "'{name}' must be a number, an (x, y) pair or a string"
            ))),
        }
    }

    fn ease(&mut self, name: &str) -> Result<Ease, CommandError> {
        let text = self.string(name)?;
        Ease::from_name(&text).ok_or_else(|| {
            let names: Vec<&str> = Ease::ALL.iter().map(|(_, name)| *name).collect();
            self.error(format!(
                "unknown ease {text:?}; use one of {}",
                names.join(", ")
            ))
        })
    }

    fn optional_ease(&mut self, name: &str) -> Result<Option<Ease>, CommandError> {
        if self.keywords.iter().any(|(key, _)| key == name) || !self.positional.is_empty() {
            self.ease(name).map(Some)
        } else {
            Ok(None)
        }
    }

    fn panel(&mut self) -> Result<Panel, CommandError> {
        let text = self.string("panel")?;
        Panel::from_name(&text).ok_or_else(|| {
            self.error(format!(
                "unknown panel {text:?}; use \"layers\", \"inspector\" or \"timeline\""
            ))
        })
    }

    fn keys(&mut self, name: &str) -> Result<Vec<KeySpec>, CommandError> {
        let expr = self.required(name)?;
        let ExprKind::List(items) = expr.kind else {
            return Err(self.error(format!("'{name}' must be a list of (id, property, time)")));
        };
        let mut keys = Vec::with_capacity(items.len());
        for item in items {
            let ExprKind::Tuple(parts) = item.kind else {
                return Err(self.error("each key is an (id, property, time) tuple"));
            };
            let [id, property, time] = parts.as_slice() else {
                return Err(self.error("each key is an (id, property, time) tuple"));
            };
            let (ExprKind::Str(id), ExprKind::Str(property)) = (&id.kind, &property.kind) else {
                return Err(self.error("a key's id and property are strings"));
            };
            let time =
                time_of(time).ok_or_else(|| self.error("a key's time is seconds or frame(N)"))?;
            keys.push(KeySpec {
                id: id.to_string(),
                property: property.to_string(),
                time,
            });
        }
        Ok(keys)
    }

    fn rest_strings(&mut self) -> Result<Vec<String>, CommandError> {
        let mut out = Vec::new();
        while let Some(expr) = self.positional.pop_front() {
            match expr.kind {
                ExprKind::Str(text) => out.push(text.to_string()),
                _ => return Err(self.error("ids must be strings")),
            }
        }
        Ok(out)
    }

    /// Fails when arguments were left over.
    pub(crate) fn finish(self) -> Result<(), CommandError> {
        if let Some((name, _)) = self.keywords.first() {
            return Err(self.error(format!("unexpected keyword '{name}'")));
        }
        if !self.positional.is_empty() {
            return Err(self.error("too many arguments"));
        }
        Ok(())
    }
}

/// A number literal, with an optional sign.
fn number_of(expr: &Expr) -> Option<f64> {
    match &expr.kind {
        ExprKind::Number(value) => Some(*value),
        ExprKind::Unary { op, operand } => {
            let value = number_of(operand)?;
            Some(match op {
                UnaryOp::Neg => -value,
                UnaryOp::Pos => value,
            })
        }
        _ => None,
    }
}

/// `true`/`false` (MDF `True`/`False` too).
fn bool_of(expr: &Expr) -> Option<bool> {
    match &expr.kind {
        ExprKind::Bool(value) => Some(*value),
        ExprKind::Name(name) if name.as_ref() == "true" => Some(true),
        ExprKind::Name(name) if name.as_ref() == "false" => Some(false),
        _ => None,
    }
}

/// Seconds, or `frame(N)`.
fn time_of(expr: &Expr) -> Option<Time> {
    if let Some(seconds) = number_of(expr) {
        return Some(Time::Seconds(seconds));
    }
    let ExprKind::Call { callee, args } = &expr.kind else {
        return None;
    };
    let ExprKind::Name(name) = &callee.kind else {
        return None;
    };
    if name.as_ref() != "frame" || args.len() != 1 || args[0].name.is_some() {
        return None;
    }
    let value = number_of(&args[0].value)?;
    if value.fract() != 0.0 || value.abs() > 1e15 {
        return None;
    }
    // Checked above: whole and inside the i64 range.
    #[allow(clippy::cast_possible_truncation)]
    Some(Time::Frame(value as i64))
}
