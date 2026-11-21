//! A small line-based unified diff for the CLI and the live control endpoint.
//!
//! The planner produces byte-range edits; people and agents read line diffs.
//! One implementation formats the code change for `motioner edit --dry-run`
//! and for the control reply, so the two can never disagree (D-0036).

/// Lines of context shown around each change.
const CONTEXT: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Equal,
    Delete,
    Insert,
}

/// The lines of a unified diff between `before` and `after`.
///
/// `path` is the project-relative file name shown in the `---`/`+++` header.
/// Returns the empty string when the two texts are equal.
#[must_use]
pub fn unified(before: &str, after: &str, path: &str) -> String {
    if before == after {
        return String::new();
    }
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    let ops = diff(&old, &new);
    let mut out = format!("--- a/{path}\n+++ b/{path}\n");
    for hunk in hunks(&ops, &old, &new) {
        out.push_str(&hunk);
    }
    out
}

/// The shortest edit script between `old` and `new` (LCS dynamic programming;
/// programs are small, so the quadratic table is fine and deterministic).
fn diff(old: &[&str], new: &[&str]) -> Vec<Op> {
    let n = old.len();
    let m = new.len();
    let mut table = vec![vec![0_u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i][j] = if old[i] == new[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    let mut ops = Vec::with_capacity(n + m);
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if old[i] == new[j] {
            ops.push(Op::Equal);
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            ops.push(Op::Delete);
            i += 1;
        } else {
            ops.push(Op::Insert);
            j += 1;
        }
    }
    while i < n {
        ops.push(Op::Delete);
        i += 1;
    }
    while j < m {
        ops.push(Op::Insert);
        j += 1;
    }
    ops
}

/// Renders `ops` as unified hunks with context.
fn hunks(ops: &[Op], old: &[&str], new: &[&str]) -> Vec<String> {
    let changed: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, op)| !matches!(op, Op::Equal))
        .map(|(index, _)| index)
        .collect();
    let mut out = Vec::new();
    let mut index = 0;
    while index < changed.len() {
        let start = changed[index].saturating_sub(CONTEXT);
        let mut end = changed[index] + 1;
        index += 1;
        while index < changed.len() && changed[index].saturating_sub(CONTEXT) <= end {
            end = changed[index] + 1;
            index += 1;
        }
        let end = (end + CONTEXT).min(ops.len());
        out.push(render(ops, old, new, start, end));
    }
    out
}

/// One `@@` hunk for `ops[start..end]`.
fn render(ops: &[Op], old: &[&str], new: &[&str], start: usize, end: usize) -> String {
    // Positions of each op in the old and new files (1-based for the header).
    let mut old_line = 0;
    let mut new_line = 0;
    for op in &ops[..start] {
        match op {
            Op::Equal => {
                old_line += 1;
                new_line += 1;
            }
            Op::Delete => old_line += 1,
            Op::Insert => new_line += 1,
        }
    }
    let old_before = old_line + 1;
    let new_before = new_line + 1;
    let mut body = String::new();
    let mut old_len = 0;
    let mut new_len = 0;
    for op in &ops[start..end] {
        match op {
            Op::Equal => {
                body.push(' ');
                body.push_str(old.get(old_line).copied().unwrap_or(""));
                old_line += 1;
                old_len += 1;
                new_line += 1;
                new_len += 1;
            }
            Op::Delete => {
                body.push('-');
                body.push_str(old.get(old_line).copied().unwrap_or(""));
                old_line += 1;
                old_len += 1;
            }
            Op::Insert => {
                body.push('+');
                body.push_str(new.get(new_line).copied().unwrap_or(""));
                new_line += 1;
                new_len += 1;
            }
        }
        body.push('\n');
    }
    format!("@@ -{old_before},{old_len} +{new_before},{new_len} @@\n{body}")
}

#[cfg(test)]
mod tests {
    use super::unified;

    #[test]
    fn identical_texts_have_no_diff() {
        assert_eq!(unified("a\nb\n", "a\nb\n", "p.motion"), "");
    }

    #[test]
    fn a_replacement_is_one_hunk_with_the_path() {
        let diff = unified("a\nb\nc\n", "a\nB\nc\n", "p.motion");
        assert!(
            diff.starts_with("--- a/p.motion\n+++ b/p.motion\n"),
            "{diff}"
        );
        assert!(diff.contains("@@ -1,3 +1,3 @@"), "{diff}");
        assert!(diff.contains("-b\n+B\n"), "{diff}");
        assert!(diff.contains(" a\n"), "{diff}");
    }

    #[test]
    fn an_insertion_shows_a_plus_line() {
        let diff = unified("a\nc\n", "a\nb\nc\n", "p.motion");
        assert!(diff.contains("+b\n"), "{diff}");
        assert!(diff.contains("@@ -1,2 +1,3 @@"), "{diff}");
    }

    #[test]
    fn distant_changes_make_two_hunks() {
        let mut before = String::new();
        let mut after = String::new();
        for index in 0..30 {
            let line = format!("line {index}\n");
            before.push_str(&line);
            after.push_str(if index == 2 || index == 27 {
                "changed\n"
            } else {
                &line
            });
        }
        let diff = unified(&before, &after, "p.motion");
        assert_eq!(diff.matches("@@ -").count(), 2, "{diff}");
    }
}
