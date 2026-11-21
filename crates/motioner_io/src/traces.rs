//! Edit traces: one JSON record per committed edit (D-0036 section 6).
//!
//! A trace is the training material of the Motioner code model: the program
//! before and after, the D-0024 command that changed it, the planner strategy
//! and the diff. Records are local, appended to `<project>/traces/<date>.jsonl`
//! and never uploaded. The schema is versioned and reserves the agent fields
//! (`conversation`, `tool_calls`) so a future in-app agent produces the same
//! records.
//!
//! The writer and parser are hand-written on purpose: the trace is a documented
//! export, not a document format (D-0005), and keeping it dependency-free makes
//! it stable for external training tools.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::assets::sha256_hex;
use crate::project::TRACES_DIR;

/// The schema version written by this build.
pub const TRACE_VERSION: i64 = 1;

/// One committed edit.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TraceRecord {
    /// Schema version.
    pub version: i64,
    /// When the edit happened (RFC 3339, local offset when available).
    pub time: String,
    /// Project-relative program file.
    pub file: String,
    /// SHA-256 of the program before the edit.
    pub before_hash: String,
    /// The program text before the edit.
    pub before: String,
    /// The command in its D-0024 text form.
    pub command: String,
    /// Planner strategy (`private-literal`, `fallback`, ...).
    pub strategy: String,
    /// Unified diff of the program.
    pub diff: String,
    /// SHA-256 of the program after the edit.
    pub after_hash: String,
    /// Playhead frame when the edit ran.
    pub frame: i64,
    /// Selected node identity, when one was selected.
    pub selection: Option<String>,
    /// Reserved for the agent panel: the conversation around the edit.
    pub conversation: Option<String>,
    /// Reserved for the agent panel: the tool calls around the edit.
    pub tool_calls: Vec<String>,
}

impl TraceRecord {
    /// A record stamped with the current time and the hashes filled in.
    #[must_use]
    pub fn new(before: &str, after: &str, command: &str, strategy: &str, diff: &str) -> Self {
        let time = OffsetDateTime::now_local()
            .unwrap_or_else(|_| OffsetDateTime::now_utc())
            .format(&Rfc3339)
            .unwrap_or_default();
        Self {
            version: TRACE_VERSION,
            time,
            file: String::new(),
            before_hash: sha256_hex(before.as_bytes()),
            before: before.to_owned(),
            command: command.to_owned(),
            strategy: strategy.to_owned(),
            diff: diff.to_owned(),
            after_hash: sha256_hex(after.as_bytes()),
            frame: 0,
            selection: None,
            conversation: None,
            tool_calls: Vec::new(),
        }
    }

    /// Whether the record has the fields a replay needs.
    #[must_use]
    pub fn is_replayable(&self) -> bool {
        self.version == TRACE_VERSION
            && !self.before.is_empty()
            && !self.command.is_empty()
            && !self.after_hash.is_empty()
    }

    /// The record as one JSON line (no trailing newline).
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push('{');
        let mut first = true;
        let mut field = |out: &mut String, name: &str| {
            if !first {
                out.push(',');
            }
            first = false;
            write_string(out, name);
            out.push(':');
        };
        field(&mut out, "version");
        let _ = write!(out, "{}", self.version);
        field(&mut out, "time");
        write_string(&mut out, &self.time);
        field(&mut out, "file");
        write_string(&mut out, &self.file);
        field(&mut out, "before_sha256");
        write_string(&mut out, &self.before_hash);
        field(&mut out, "before");
        write_string(&mut out, &self.before);
        field(&mut out, "command");
        write_string(&mut out, &self.command);
        field(&mut out, "strategy");
        write_string(&mut out, &self.strategy);
        field(&mut out, "diff");
        write_string(&mut out, &self.diff);
        field(&mut out, "after_sha256");
        write_string(&mut out, &self.after_hash);
        field(&mut out, "frame");
        let _ = write!(out, "{}", self.frame);
        field(&mut out, "selection");
        if let Some(selection) = &self.selection {
            write_string(&mut out, selection);
        } else {
            out.push_str("null");
        }
        field(&mut out, "conversation");
        if let Some(conversation) = &self.conversation {
            write_string(&mut out, conversation);
        } else {
            out.push_str("null");
        }
        field(&mut out, "tool_calls");
        out.push('[');
        for (index, call) in self.tool_calls.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            write_string(&mut out, call);
        }
        out.push_str("]}");
        out
    }

    /// Parses one JSONL line, ignoring unknown fields.
    ///
    /// Returns `None` for malformed JSON or a mismatched schema version.
    #[must_use]
    pub fn parse(line: &str) -> Option<Self> {
        let fields = parse_object(line.trim())?;
        let mut record = Self {
            version: fields.number("version").unwrap_or(0),
            ..Self::default()
        };
        if record.version != TRACE_VERSION {
            return None;
        }
        record.time = fields.string("time").unwrap_or_default();
        record.file = fields.string("file").unwrap_or_default();
        record.before_hash = fields.string("before_sha256").unwrap_or_default();
        record.before = fields.string("before").unwrap_or_default();
        record.command = fields.string("command").unwrap_or_default();
        record.strategy = fields.string("strategy").unwrap_or_default();
        record.diff = fields.string("diff").unwrap_or_default();
        record.after_hash = fields.string("after_sha256").unwrap_or_default();
        record.frame = fields.number("frame").unwrap_or(0);
        record.selection = fields.string_opt("selection");
        record.conversation = fields.string_opt("conversation");
        record.tool_calls = fields.strings("tool_calls");
        Some(record)
    }
}

/// Appends `record` to `<root>/traces/<date>.jsonl`.
///
/// # Errors
///
/// An I/O error when the folder or the file cannot be written.
pub fn append(root: &Path, record: &TraceRecord) -> std::io::Result<PathBuf> {
    let dir = root.join(TRACES_DIR);
    std::fs::create_dir_all(&dir)?;
    let day = record
        .time
        .get(..10)
        .filter(|day| !day.is_empty())
        .unwrap_or("undated");
    let path = dir.join(format!("{day}.jsonl"));
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    file.write_all(record.to_json().as_bytes())?;
    file.write_all(b"\n")?;
    Ok(path)
}

/// Every trace under `<root>/traces`, oldest file first.
///
/// # Errors
///
/// An I/O error when the folder exists but cannot be read.
pub fn read_all(root: &Path) -> std::io::Result<Vec<TraceRecord>> {
    let dir = root.join(TRACES_DIR);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "jsonl")
        })
        .collect();
    paths.sort();
    let mut records = Vec::new();
    for path in paths {
        let text = std::fs::read_to_string(&path)?;
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            if let Some(record) = TraceRecord::parse(line) {
                records.push(record);
            }
        }
    }
    Ok(records)
}

fn write_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// The decoded fields of one flat JSON object.
struct Fields(Vec<(String, Value)>);

enum Value {
    String(String),
    Number(i64),
    Strings(Vec<String>),
    Other,
}

/// Parses `["a", "b"]` (the reserved `tool_calls` field).
fn parse_string_array(
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
) -> Option<Vec<String>> {
    if chars.next()?.1 != '[' {
        return None;
    }
    let mut out = Vec::new();
    loop {
        skip_space(chars);
        match chars.peek()?.1 {
            ']' => {
                chars.next();
                return Some(out);
            }
            '"' => out.push(parse_string(chars)?.1),
            _ => return None,
        }
        skip_space(chars);
        match chars.next()?.1 {
            ',' => {}
            ']' => return Some(out),
            _ => return None,
        }
    }
}

impl Fields {
    fn string(&self, name: &str) -> Option<String> {
        self.0
            .iter()
            .find(|(key, _)| key == name)
            .and_then(|(_, value)| match value {
                Value::String(text) => Some(text.clone()),
                Value::Number(_) | Value::Strings(_) | Value::Other => None,
            })
    }

    fn string_opt(&self, name: &str) -> Option<String> {
        self.string(name)
    }

    fn number(&self, name: &str) -> Option<i64> {
        self.0
            .iter()
            .find(|(key, _)| key == name)
            .and_then(|(_, value)| match value {
                Value::Number(number) => Some(*number),
                Value::String(_) | Value::Strings(_) | Value::Other => None,
            })
    }

    fn strings(&self, name: &str) -> Vec<String> {
        self.0
            .iter()
            .find(|(key, _)| key == name)
            .and_then(|(_, value)| match value {
                Value::Strings(items) => Some(items.clone()),
                Value::String(_) | Value::Number(_) | Value::Other => None,
            })
            .unwrap_or_default()
    }
}

/// Parses the flat top-level object the writer produces.
fn parse_object(text: &str) -> Option<Fields> {
    let mut chars = text.char_indices().peekable();
    if chars.next()?.1 != '{' {
        return None;
    }
    let mut fields = Vec::new();
    loop {
        skip_space(&mut chars);
        match chars.peek()?.1 {
            '}' => {
                chars.next();
                return Some(Fields(fields));
            }
            '"' => {}
            _ => return None,
        }
        let (_, key) = parse_string(&mut chars)?;
        skip_space(&mut chars);
        if chars.next()?.1 != ':' {
            return None;
        }
        skip_space(&mut chars);
        let value = match chars.peek()?.1 {
            '"' => Value::String(parse_string(&mut chars)?.1),
            '-' | '0'..='9' => Value::Number(parse_number(&mut chars)?),
            '[' => Value::Strings(parse_string_array(&mut chars)?),
            '{' => {
                skip_container(&mut chars, '{', '}')?;
                Value::Other
            }
            _ => {
                skip_scalar(&mut chars);
                Value::Other
            }
        };
        fields.push((key, value));
        skip_space(&mut chars);
        match chars.peek().map(|(_, c)| *c) {
            Some(',') => {
                chars.next();
            }
            Some('}') => {}
            _ => return None,
        }
    }
}

fn skip_space(chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>) {
    while chars.peek().is_some_and(|(_, c)| c.is_whitespace()) {
        chars.next();
    }
}

fn parse_string(
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
) -> Option<(usize, String)> {
    let (start, quote) = chars.next()?;
    if quote != '"' {
        return None;
    }
    let mut out = String::new();
    while let Some((_, c)) = chars.next() {
        match c {
            '"' => return Some((start, out)),
            '\\' => {
                let (_, escaped) = chars.next()?;
                match escaped {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'u' => {
                        let mut code = 0_u32;
                        for _ in 0..4 {
                            let (_, digit) = chars.next()?;
                            code = code * 16 + digit.to_digit(16)?;
                        }
                        out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                    }
                    _ => return None,
                }
            }
            c => out.push(c),
        }
    }
    None
}

fn parse_number(chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>) -> Option<i64> {
    let mut text = String::new();
    while let Some((_, c)) = chars.peek() {
        if c.is_ascii_digit() || *c == '-' {
            text.push(*c);
            chars.next();
        } else {
            break;
        }
    }
    text.parse().ok()
}

fn skip_scalar(chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>) {
    while let Some((_, c)) = chars.peek() {
        if *c == ',' || *c == '}' || c.is_whitespace() {
            break;
        }
        chars.next();
    }
}

fn skip_container(
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    open: char,
    close: char,
) -> Option<()> {
    if chars.next()?.1 != open {
        return None;
    }
    let mut depth = 1_usize;
    while let Some((_, c)) = chars.next() {
        if c == '"' {
            // Skip the string body; the opening quote was already consumed.
            let mut escaped = false;
            for (_, c) in chars.by_ref() {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    break;
                }
            }
        } else if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{append, read_all, TraceRecord, TRACE_VERSION};

    fn record() -> TraceRecord {
        let before = "comp(name=\"P\", duration=1.0)\na = circle(10)\n";
        let after = "comp(name=\"P\", duration=1.0)\na = circle(20)\n";
        let mut record = TraceRecord {
            version: TRACE_VERSION,
            time: "2026-11-06T10:00:00Z".to_owned(),
            file: "main.motion".to_owned(),
            before_hash: crate::assets::sha256_hex(before.as_bytes()),
            before: before.to_owned(),
            command: "edit(\"a\", \"radius\", 20)".to_owned(),
            strategy: "private-literal".to_owned(),
            diff: "--- a/main.motion\n+++ b/main.motion\n".to_owned(),
            after_hash: crate::assets::sha256_hex(after.as_bytes()),
            frame: 12,
            selection: Some("@12".to_owned()),
            conversation: None,
            tool_calls: Vec::new(),
        };
        record.conversation = Some("make it bigger".to_owned());
        record
    }

    #[test]
    fn a_record_round_trips_through_json() {
        let record = record();
        let line = record.to_json();
        assert_eq!(TraceRecord::parse(&line), Some(record));
    }

    #[test]
    fn escaping_keeps_quotes_newlines_and_backslashes() {
        let mut record = record();
        record.before = "a = text(\"x\\ny\")\n".to_owned();
        record.tool_calls = vec!["read_program()".to_owned()];
        let line = record.to_json();
        assert!(!line.contains('\n'), "one record is one line: {line}");
        assert_eq!(TraceRecord::parse(&line), Some(record));
    }

    #[test]
    fn append_writes_one_file_per_day_and_reads_them_back() {
        let dir = std::env::temp_dir().join(format!("motioner_traces_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = append(&dir, &record()).unwrap();
        assert_eq!(path.file_name().unwrap(), "2026-11-06.jsonl");
        append(&dir, &record()).unwrap();
        let records = read_all(&dir).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0], record());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_or_future_records_are_skipped() {
        assert!(TraceRecord::parse("not json").is_none());
        let mut line = record().to_json();
        line = line.replace("\"version\":1", "\"version\":99");
        assert!(TraceRecord::parse(&line).is_none());
    }
}
