//! A tiny JSON writer for tool output (`query`, `state`, `ctl` replies).
//!
//! Output only: Motioner never reads JSON, and the document format stays MDF
//! (D-0005). Objects keep the order fields are written in, so the output is
//! deterministic and stable (D-0024).

use std::fmt::Write as _;

/// Builds one JSON value into a string.
#[derive(Debug, Default)]
pub struct Json {
    out: String,
    /// Whether the next item in each open container needs a leading comma.
    comma: Vec<bool>,
    /// A key was just written, so the next value follows its colon.
    after_key: bool,
}

impl Json {
    /// An empty writer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The finished text.
    #[must_use]
    pub fn finish(self) -> String {
        self.out
    }

    fn separator(&mut self) {
        if std::mem::take(&mut self.after_key) {
            return;
        }
        if let Some(needs) = self.comma.last_mut() {
            if *needs {
                self.out.push(',');
            }
            *needs = true;
        }
    }

    /// Opens an object (as a value).
    pub fn begin_object(&mut self) -> &mut Self {
        self.separator();
        self.out.push('{');
        self.comma.push(false);
        self
    }

    /// Closes the innermost object.
    pub fn end_object(&mut self) -> &mut Self {
        self.comma.pop();
        self.out.push('}');
        self
    }

    /// Opens an array (as a value).
    pub fn begin_array(&mut self) -> &mut Self {
        self.separator();
        self.out.push('[');
        self.comma.push(false);
        self
    }

    /// Closes the innermost array.
    pub fn end_array(&mut self) -> &mut Self {
        self.comma.pop();
        self.out.push(']');
        self
    }

    /// Writes an object key; the next call writes its value.
    pub fn key(&mut self, key: &str) -> &mut Self {
        self.separator();
        escape_into(&mut self.out, key);
        self.out.push(':');
        self.after_key = true;
        self
    }

    /// A string value.
    pub fn string(&mut self, value: &str) -> &mut Self {
        self.value_prefix();
        escape_into(&mut self.out, value);
        self
    }

    /// A number value (`null` when not finite). Whole numbers print without
    /// a fraction.
    pub fn number(&mut self, value: f64) -> &mut Self {
        self.value_prefix();
        if value.is_finite() {
            let _ = write!(self.out, "{value}");
        } else {
            self.out.push_str("null");
        }
        self
    }

    /// An integer value.
    pub fn int(&mut self, value: i64) -> &mut Self {
        self.value_prefix();
        let _ = write!(self.out, "{value}");
        self
    }

    /// A boolean value.
    pub fn boolean(&mut self, value: bool) -> &mut Self {
        self.value_prefix();
        self.out.push_str(if value { "true" } else { "false" });
        self
    }

    /// `null`.
    pub fn null(&mut self) -> &mut Self {
        self.value_prefix();
        self.out.push_str("null");
        self
    }

    /// A string or `null`.
    pub fn opt_string(&mut self, value: Option<&str>) -> &mut Self {
        match value {
            Some(text) => self.string(text),
            None => self.null(),
        }
    }

    /// Embeds already-encoded JSON as a value.
    pub fn raw(&mut self, json: &str) -> &mut Self {
        self.value_prefix();
        self.out.push_str(json);
        self
    }

    fn value_prefix(&mut self) {
        self.separator();
    }
}

/// Writes `text` as a quoted JSON string.
fn escape_into(out: &mut String, text: &str) {
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

#[cfg(test)]
mod tests {
    use super::Json;

    #[test]
    fn writes_nested_values_with_commas_in_the_right_places() {
        let mut j = Json::new();
        j.begin_object();
        j.key("a").int(1);
        j.key("b").begin_array();
        j.number(1.5).string("x\"y\n").boolean(false).null();
        j.begin_object().key("z").number(2.0).end_object();
        j.end_array();
        j.key("c").opt_string(None);
        j.end_object();
        assert_eq!(
            j.finish(),
            r#"{"a":1,"b":[1.5,"x\"y\n",false,null,{"z":2}],"c":null}"#
        );
    }

    #[test]
    fn non_finite_numbers_are_null_and_control_characters_are_escaped() {
        let mut j = Json::new();
        j.begin_array().number(f64::NAN).string("\u{1}").end_array();
        assert_eq!(j.finish(), r#"[null,"\u0001"]"#);
    }
}
