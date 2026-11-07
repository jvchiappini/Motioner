//! The read-only code panel model: the program lines and the range the last
//! gesture wrote.
//!
//! The highlight comes from the applied [`CodeEdit`], never from re-diffing
//! text in the drawing code (D-0036). The drawing lives in `panels::code`;
//! this module owns the line and range arithmetic so the future `GUIMaker`
//! port only replaces the drawing.

use motioner_dsl::CodeEdit;

/// One line of the program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeLine {
    /// One-based line number.
    pub number: usize,
    /// The line without its terminator.
    pub text: String,
    /// Whether the last gesture wrote this line.
    pub edited: bool,
    /// Whether this line is where the last gesture deleted code.
    pub deleted: bool,
}

/// A program with the last edit's lines marked.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CodePanel {
    /// Every line of the program.
    pub lines: Vec<CodeLine>,
    /// First and last edited line (one-based, inclusive), for scrolling.
    pub highlight: Option<(usize, usize)>,
}

impl CodePanel {
    /// Builds the panel for `source` and the edit that produced it.
    ///
    /// `edit` is the exact edit applied to the previous program; its
    /// replacements are mapped to line ranges in `source`.
    #[must_use]
    pub fn build(source: &str, edit: Option<&CodeEdit>) -> Self {
        let mut lines: Vec<CodeLine> = source
            .lines()
            .enumerate()
            .map(|(index, text)| CodeLine {
                number: index + 1,
                text: text.to_owned(),
                edited: false,
                deleted: false,
            })
            .collect();
        let Some(edit) = edit else {
            return Self {
                lines,
                highlight: None,
            };
        };
        let mut ranges = Vec::new();
        let mut deletions = Vec::new();
        let mut delta = 0_isize;
        for replacement in edit.replacements() {
            let start = shift(replacement.start, delta);
            let end = start + replacement.text.len();
            if replacement.text.is_empty() {
                deletions.push(line_of(source, start));
            } else {
                ranges.push((
                    line_of(source, start),
                    line_of(source, end.saturating_sub(1)),
                ));
            }
            let removed = replacement.end.saturating_sub(replacement.start);
            delta += replacement.text.len().cast_signed() - removed.cast_signed();
        }
        for line in &mut lines {
            line.edited = ranges
                .iter()
                .any(|(first, last)| line.number >= *first && line.number <= *last);
            line.deleted = deletions.contains(&line.number);
        }
        let highlight = ranges
            .iter()
            .copied()
            .reduce(|(a, b), (c, d)| (a.min(c), b.max(d)))
            .or_else(|| {
                deletions
                    .iter()
                    .copied()
                    .reduce(usize::min)
                    .map(|line| (line, line))
            });
        Self { lines, highlight }
    }
}

fn shift(offset: usize, delta: isize) -> usize {
    usize::try_from(offset.cast_signed().saturating_add(delta)).unwrap_or(0)
}

/// The one-based line containing byte `offset`.
fn line_of(source: &str, offset: usize) -> usize {
    let end = offset.min(source.len());
    source[..end].bytes().filter(|byte| *byte == b'\n').count() + 1
}

#[cfg(test)]
mod tests {
    use super::CodePanel;
    use motioner_dsl::{CodeEdit, Replacement, Strategy};

    fn edit(replacements: Vec<Replacement>) -> CodeEdit {
        CodeEdit::new(replacements, "test", Strategy::StructuralEdit).unwrap()
    }

    #[test]
    fn no_edit_has_no_highlight() {
        let panel = CodePanel::build("a\nb\n", None);
        assert_eq!(panel.lines.len(), 2);
        assert_eq!(panel.lines[1].number, 2);
        assert!(panel.highlight.is_none());
        assert!(panel.lines.iter().all(|line| !line.edited));
    }

    #[test]
    fn an_insertion_marks_the_inserted_lines() {
        let source = "a = circle(1)\nb = circle(2)\nc = circle(3)\n";
        let at = source.find("b = ").unwrap();
        let edit = edit(vec![Replacement::insert(at, "extra = circle(0)\n")]);
        let panel = CodePanel::build(source, Some(&edit));
        assert!(panel.lines[1].edited, "{panel:?}");
        assert!(!panel.lines[0].edited, "{panel:?}");
        let (first, last) = panel.highlight.expect("a highlight");
        assert!(first <= 2 && last >= 2, "{panel:?}");
    }

    #[test]
    fn a_deletion_marks_its_line() {
        let source = "a = circle(1)\nb = circle(2)\n";
        let at = source.find("b = ").unwrap();
        let edit = edit(vec![Replacement::new(at, at + 12, "")]);
        let panel = CodePanel::build(source, Some(&edit));
        assert!(panel.lines[1].deleted, "{panel:?}");
        assert_eq!(panel.highlight, Some((2, 2)));
    }
}
