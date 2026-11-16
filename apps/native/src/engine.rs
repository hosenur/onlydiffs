//! The line diff, built the way Zed's `buffer_diff` builds it.
//!
//! Zed does not parse `git diff`. It reads the two texts itself, interns their
//! lines, runs imara-diff's Histogram algorithm, and then applies git's
//! slider/indent heuristic (`postprocess_lines`) so that a change that could
//! sit in more than one place always lands in the same one. This follows the
//! same steps; it is a fresh implementation, not Zed's code.
//!
//! What it leaves out is everything Zed needs only because its diffs are
//! editable: anchors that survive typing, the `SumTree` the hunks live in, and
//! the multi-buffer. A review is read-only, so hunks are plain line ranges and
//! the display is a flat list of rows.

use std::ops::Range;

use imara_diff::{Algorithm, Diff, InternedInput, sources::lines};

/// Unchanged lines kept on each side of a hunk, as `git diff` does by default.
pub const CONTEXT_LINES: u32 = 3;

/// One change: lines `old` of the old text were replaced by lines `new` of the
/// new one. Ranges are zero-based line indices; an empty range is a pure
/// insertion or deletion at that position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub old: Range<u32>,
    pub new: Range<u32>,
}

impl Hunk {
    pub fn added_lines(&self) -> u32 {
        self.new.end - self.new.start
    }

    pub fn removed_lines(&self) -> u32 {
        self.old.end - self.old.start
    }
}

/// Splits text into lines the same way the diff tokenises it: each line keeps
/// its terminator, and a final line without one still counts. So `"a\nb"` and
/// `"a\nb\n"` are both two lines, and index `i` here is token `i` in the diff.
fn split_lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// The hunks that turn `old` into `new`.
pub fn compute_hunks(old: &str, new: &str) -> Vec<Hunk> {
    let input = InternedInput::new(lines(old), lines(new));
    let mut diff = Diff::compute(Algorithm::Histogram, &input);
    diff.postprocess_lines(&input);
    diff.hunks()
        .map(|hunk| Hunk {
            old: hunk.before,
            new: hunk.after,
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Context,
    Added,
    Removed,
}

/// One line of the rendered diff. Line numbers are one-based, as shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub kind: RowKind,
    pub old_number: Option<u32>,
    pub new_number: Option<u32>,
    /// The line without its terminator.
    pub text: String,
    /// The words that changed, as byte ranges of `text`: set on a removed
    /// line and the added line that replaced it, empty otherwise.
    pub changes: Vec<std::ops::Range<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Line(Line),
    /// Unchanged lines that are folded away between two hunks, or before the
    /// first or after the last.
    Fold { hidden: u32 },
}

/// One row of the side-by-side layout: the old version on the left, the new
/// on the right. A side is empty where the other version has a line it
/// does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitRow {
    Pair { left: Option<Line>, right: Option<Line> },
    Fold { hidden: u32 },
}

/// A file's diff, ready to draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    pub hunks: Vec<Hunk>,
    /// Stacked: removed lines above the added lines that replace them.
    pub rows: Vec<Row>,
    /// Side by side: the same diff as pairs of old and new lines.
    pub split: Vec<SplitRow>,
    pub additions: u32,
    pub deletions: u32,
}

/// A file with no changes, every line shown: what opening an unchanged file
/// from the project tree displays.
pub fn plain(text: &str) -> FileDiff {
    let rows: Vec<Row> = split_lines(text)
        .into_iter()
        .enumerate()
        .map(|(ix, line)| {
            let number = Some(ix as u32 + 1);
            Row::Line(Line {
                kind: RowKind::Context,
                old_number: number,
                new_number: number,
                text: line.trim_end_matches(['\n', '\r']).to_owned(),
                changes: Vec::new(),
            })
        })
        .collect();
    let split = side_by_side(&rows);
    FileDiff { hunks: Vec::new(), rows, split, additions: 0, deletions: 0 }
}

/// Diffs `old` against `new` and lays the result out as rows, keeping
/// `context` unchanged lines around each hunk and folding the rest.
pub fn diff_texts(old: &str, new: &str, context: u32) -> FileDiff {
    let hunks = compute_hunks(old, new);
    let old_lines = split_lines(old);
    let new_lines = split_lines(new);
    let mut rows = layout(&old_lines, &new_lines, &hunks, context);
    mark_word_changes(&mut rows);
    FileDiff {
        additions: hunks.iter().map(Hunk::added_lines).sum(),
        deletions: hunks.iter().map(Hunk::removed_lines).sum(),
        hunks,
        split: side_by_side(&rows),
        rows,
    }
}

/// Fills in `changes` for each removed line and the added line it pairs
/// with — the same pairing the side-by-side layout uses.
fn mark_word_changes(rows: &mut [Row]) {
    let mut at = 0;
    while at < rows.len() {
        let is = |row: &Row, kind: RowKind| matches!(row, Row::Line(l) if l.kind == kind);
        if !is(&rows[at], RowKind::Removed) {
            at += 1;
            continue;
        }
        let removed_start = at;
        while at < rows.len() && is(&rows[at], RowKind::Removed) {
            at += 1;
        }
        let added_start = at;
        while at < rows.len() && is(&rows[at], RowKind::Added) {
            at += 1;
        }
        let pairs = (added_start - removed_start).min(at - added_start);
        for offset in 0..pairs {
            let (Row::Line(old), Row::Line(new)) = (&rows[removed_start + offset], &rows[added_start + offset]) else {
                continue;
            };
            let (old_changes, new_changes) = word_changes(&old.text, &new.text);
            if let Row::Line(old) = &mut rows[removed_start + offset] {
                old.changes = old_changes;
            }
            if let Row::Line(new) = &mut rows[added_start + offset] {
                new.changes = new_changes;
            }
        }
    }
}

/// Words, runs of spaces, and single other characters: what a word diff
/// compares.
fn tokens(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        let same_kind = |d: char| {
            if c.is_alphanumeric() || c == '_' {
                d.is_alphanumeric() || d == '_'
            } else if c.is_whitespace() {
                d.is_whitespace()
            } else {
                false
            }
        };
        let mut end = start + c.len_utf8();
        while let Some(&(at, d)) = chars.peek() {
            if !same_kind(d) {
                break;
            }
            end = at + d.len_utf8();
            chars.next();
        }
        out.push(start..end);
    }
    out
}

/// Past this many tokens a line is long enough that marking words stops
/// helping, and the comparison stops being cheap.
const MAX_WORD_TOKENS: usize = 300;

/// Which words differ between a removed line and its replacement, as byte
/// ranges in each. Changed words separated only by spaces are joined into
/// one range, as Pierre's `word-alt` does, so a changed phrase reads as one
/// mark. When nothing is shared the whole line changed and there is nothing
/// worth marking.
pub fn word_changes(old: &str, new: &str) -> (Vec<std::ops::Range<usize>>, Vec<std::ops::Range<usize>>) {
    let (a, b) = (tokens(old), tokens(new));
    if a.len() > MAX_WORD_TOKENS || b.len() > MAX_WORD_TOKENS {
        return (Vec::new(), Vec::new());
    }
    let (ta, tb): (Vec<&str>, Vec<&str>) =
        (a.iter().map(|r| &old[r.clone()]).collect(), b.iter().map(|r| &new[r.clone()]).collect());
    // Longest common subsequence of tokens, by table.
    let mut table = vec![vec![0u16; tb.len() + 1]; ta.len() + 1];
    for i in (0..ta.len()).rev() {
        for j in (0..tb.len()).rev() {
            table[i][j] = if ta[i] == tb[j] { table[i + 1][j + 1] + 1 } else { table[i + 1][j].max(table[i][j + 1]) };
        }
    }
    if table[0][0] == 0 {
        return (Vec::new(), Vec::new());
    }
    let (mut keep_a, mut keep_b) = (vec![false; ta.len()], vec![false; tb.len()]);
    let (mut i, mut j) = (0, 0);
    while i < ta.len() && j < tb.len() {
        if ta[i] == tb[j] {
            keep_a[i] = true;
            keep_b[j] = true;
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    let ranges = |tokens: &[std::ops::Range<usize>], keep: &[bool], text: &str| {
        let mut out: Vec<std::ops::Range<usize>> = Vec::new();
        for (ix, range) in tokens.iter().enumerate() {
            if keep[ix] {
                continue;
            }
            match out.last_mut() {
                // Join to the previous mark across kept whitespace alone.
                Some(last)
                    if text[last.end..range.start].chars().all(char::is_whitespace) =>
                {
                    last.end = range.end
                }
                _ => out.push(range.clone()),
            }
        }
        // Leading or trailing spaces alone are not a change worth showing.
        out.retain(|r| !text[r.clone()].trim().is_empty());
        out
    };
    (ranges(&a, &keep_a, old), ranges(&b, &keep_b, new))
}

/// The stacked rows as side-by-side pairs. An unchanged line sits on both
/// sides. In each run of changes the removed lines are paired in order with
/// the added lines that follow them — the first removed beside the first
/// added — and whichever side runs out first is left empty below, so a line
/// and its replacement sit level.
pub fn side_by_side(rows: &[Row]) -> Vec<SplitRow> {
    let mut out = Vec::new();
    let mut removed: Vec<Line> = Vec::new();
    let mut added: Vec<Line> = Vec::new();
    let flush = |out: &mut Vec<SplitRow>, removed: &mut Vec<Line>, added: &mut Vec<Line>| {
        let count = removed.len().max(added.len());
        let mut left = removed.drain(..);
        let mut right = added.drain(..);
        for _ in 0..count {
            out.push(SplitRow::Pair { left: left.next(), right: right.next() });
        }
    };
    for row in rows {
        match row {
            Row::Fold { hidden } => {
                flush(&mut out, &mut removed, &mut added);
                out.push(SplitRow::Fold { hidden: *hidden });
            }
            Row::Line(line) => match line.kind {
                RowKind::Removed => {
                    // A removal after additions starts a new run.
                    if !added.is_empty() {
                        flush(&mut out, &mut removed, &mut added);
                    }
                    removed.push(line.clone());
                }
                RowKind::Added => added.push(line.clone()),
                RowKind::Context => {
                    flush(&mut out, &mut removed, &mut added);
                    out.push(SplitRow::Pair { left: Some(line.clone()), right: Some(line.clone()) });
                }
            },
        }
    }
    flush(&mut out, &mut removed, &mut added);
    out
}

fn strip_terminator(line: &str) -> String {
    line.trim_end_matches(['\n', '\r']).to_owned()
}

fn layout(old_lines: &[&str], new_lines: &[&str], hunks: &[Hunk], context: u32) -> Vec<Row> {
    let mut rows = Vec::new();
    // The next old/new line not yet emitted. Unchanged regions advance both
    // together, so the gap between them stays constant outside a hunk.
    let mut old_at: u32 = 0;
    let mut new_at: u32 = 0;
    let old_len = old_lines.len() as u32;

    let push_context = |rows: &mut Vec<Row>, old_ix: u32, new_ix: u32| {
        rows.push(Row::Line(Line {
            kind: RowKind::Context,
            old_number: Some(old_ix + 1),
            new_number: Some(new_ix + 1),
            text: strip_terminator(old_lines[old_ix as usize]),
            changes: Vec::new(),
        }));
    };

    for (index, hunk) in hunks.iter().enumerate() {
        // Unchanged lines between the previous position and this hunk: show
        // up to `context` after the previous hunk and before this one, and
        // fold whatever is left in the middle. Before the first hunk there is
        // no previous one to trail.
        let gap = hunk.old.start - old_at;
        let lead = if index == 0 { 0 } else { context.min(gap) };
        let trail = context.min(gap - lead);
        let hidden = gap - lead - trail;

        for offset in 0..lead {
            push_context(&mut rows, old_at + offset, new_at + offset);
        }
        if hidden > 0 {
            rows.push(Row::Fold { hidden });
        }
        for offset in (gap - trail)..gap {
            push_context(&mut rows, old_at + offset, new_at + offset);
        }

        for old_ix in hunk.old.clone() {
            rows.push(Row::Line(Line {
                kind: RowKind::Removed,
                old_number: Some(old_ix + 1),
                new_number: None,
                text: strip_terminator(old_lines[old_ix as usize]),
                changes: Vec::new(),
            }));
        }
        for new_ix in hunk.new.clone() {
            rows.push(Row::Line(Line {
                kind: RowKind::Added,
                old_number: None,
                new_number: Some(new_ix + 1),
                text: strip_terminator(new_lines[new_ix as usize]),
                changes: Vec::new(),
            }));
        }

        old_at = hunk.old.end;
        new_at = hunk.new.end;
    }

    // After the last hunk: `context` lines, then fold the rest.
    if !hunks.is_empty() {
        let remaining = old_len - old_at;
        let shown = context.min(remaining);
        for offset in 0..shown {
            push_context(&mut rows, old_at + offset, new_at + offset);
        }
        if remaining > shown {
            rows.push(Row::Fold {
                hidden: remaining - shown,
            });
        }
    }

    rows
}

#[cfg(test)]
mod tests {
    /// Each side-by-side row as "left | right" by text, `·` for an empty side.
    fn pairs(diff: &super::FileDiff) -> Vec<String> {
        diff.split
            .iter()
            .map(|row| match row {
                super::SplitRow::Fold { hidden } => format!("⋯{hidden}"),
                super::SplitRow::Pair { left, right } => {
                    let side = |line: &Option<super::Line>| line.as_ref().map_or("·".to_owned(), |l| l.text.clone());
                    format!("{} | {}", side(left), side(right))
                }
            })
            .collect()
    }

    #[test]
    fn only_the_changed_words_are_marked() {
        let (old, new) = super::word_changes("let timeout = 5;", "let timeout = 30;");
        assert_eq!(old, [14..15]);
        assert_eq!(new, [14..16]);
        // A changed phrase is one mark, not one per word.
        let (old, new) = super::word_changes("a fast red car", "a slow blue car");
        assert_eq!((&"a fast red car"[old[0].clone()], &"a slow blue car"[new[0].clone()]), ("fast red", "slow blue"));
        // Nothing shared: the whole line is the change.
        assert_eq!(super::word_changes("abc", "xyz"), (vec![], vec![]));
    }

    #[test]
    fn a_diff_marks_words_on_paired_lines_only() {
        let diff = super::diff_texts("a\nconst N = 5;\nz\n", "a\nconst N = 1;\nextra\nz\n", 3);
        let marked: Vec<_> = diff
            .rows
            .iter()
            .filter_map(|row| match row {
                super::Row::Line(l) if !l.changes.is_empty() => Some((l.kind, l.text[l.changes[0].clone()].to_owned())),
                _ => None,
            })
            .collect();
        assert_eq!(marked, [(super::RowKind::Removed, "5".to_owned()), (super::RowKind::Added, "1".to_owned())]);
    }

    #[test]
    fn a_changed_line_sits_beside_its_replacement() {
        let diff = super::diff_texts("a\nb\nc\n", "a\nB\nc\n", 3);
        assert_eq!(pairs(&diff), ["a | a", "b | B", "c | c"]);
    }

    #[test]
    fn an_uneven_change_leaves_the_short_side_empty() {
        let diff = super::diff_texts("a\nb\nz\n", "a\nx\ny\nw\nz\n", 3);
        assert_eq!(pairs(&diff), ["a | a", "b | x", "· | y", "· | w", "z | z"]);
        let diff = super::diff_texts("a\nb\nc\nz\n", "a\nz\n", 3);
        assert_eq!(pairs(&diff), ["a | a", "b | ·", "c | ·", "z | z"]);
    }

    #[test]
    fn folds_carry_over() {
        let old: String = (1..=20).map(|n| format!("{n}\n")).collect();
        let new = old.replace("10\n", "ten\n");
        let diff = super::diff_texts(&old, &new, 1);
        assert_eq!(pairs(&diff), ["⋯8", "9 | 9", "10 | ten", "11 | 11", "⋯9"]);
    }

    #[test]
    fn a_plain_file_shows_every_line_numbered() {
        let file = plain("a\nb\n");
        assert_eq!(file.rows.len(), 2);
        assert!(matches!(&file.rows[1], Row::Line(l) if l.text == "b" && l.new_number == Some(2)));
    }

    use super::*;

    fn kinds(diff: &FileDiff) -> Vec<String> {
        diff.rows
            .iter()
            .map(|row| match row {
                Row::Line(line) => {
                    let mark = match line.kind {
                        RowKind::Context => ' ',
                        RowKind::Added => '+',
                        RowKind::Removed => '-',
                    };
                    format!("{mark}{}", line.text)
                }
                Row::Fold { hidden } => format!("~{hidden}"),
            })
            .collect()
    }

    #[test]
    fn identical_texts_have_no_rows() {
        let diff = diff_texts("a\nb\n", "a\nb\n", CONTEXT_LINES);
        assert!(diff.hunks.is_empty());
        assert!(diff.rows.is_empty());
    }

    #[test]
    fn a_changed_line_is_a_removal_then_an_addition() {
        let diff = diff_texts("a\nb\nc\n", "a\nB\nc\n", CONTEXT_LINES);
        assert_eq!(kinds(&diff), [" a", "-b", "+B", " c"]);
        assert_eq!((diff.additions, diff.deletions), (1, 1));
    }

    #[test]
    fn line_numbers_follow_each_side() {
        let diff = diff_texts("a\nb\nc\n", "a\nx\ny\nc\n", CONTEXT_LINES);
        let numbers: Vec<_> = diff
            .rows
            .iter()
            .filter_map(|row| match row {
                Row::Line(line) => Some((line.old_number, line.new_number)),
                Row::Fold { .. } => None,
            })
            .collect();
        assert_eq!(
            numbers,
            [
                (Some(1), Some(1)),
                (Some(2), None),
                (None, Some(2)),
                (None, Some(3)),
                (Some(3), Some(4)),
            ]
        );
    }

    #[test]
    fn distant_unchanged_lines_fold() {
        let old: String = (1..=20).map(|n| format!("{n}\n")).collect();
        let new = old.replace("10\n", "ten\n");
        let diff = diff_texts(&old, &new, 3);
        assert_eq!(
            kinds(&diff),
            ["~6", " 7", " 8", " 9", "-10", "+ten", " 11", " 12", " 13", "~7"]
        );
    }

    #[test]
    fn close_hunks_share_their_context() {
        let old: String = (1..=10).map(|n| format!("{n}\n")).collect();
        let new = old.replace("3\n", "three\n").replace("6\n", "six\n");
        let diff = diff_texts(&old, &new, 3);
        // Two unchanged lines between the hunks: shown, not folded.
        assert_eq!(
            kinds(&diff),
            [" 1", " 2", "-3", "+three", " 4", " 5", "-6", "+six", " 7", " 8", " 9", "~1"]
        );
    }

    #[test]
    fn a_new_file_is_all_additions() {
        let diff = diff_texts("", "one\ntwo", CONTEXT_LINES);
        assert_eq!(kinds(&diff), ["+one", "+two"]);
    }

    #[test]
    fn a_deleted_file_is_all_removals() {
        let diff = diff_texts("one\ntwo\n", "", CONTEXT_LINES);
        assert_eq!(kinds(&diff), ["-one", "-two"]);
    }

    #[test]
    fn a_missing_final_newline_is_a_change() {
        let diff = diff_texts("a\nb\n", "a\nb", CONTEXT_LINES);
        assert_eq!(kinds(&diff), [" a", "-b", "+b"]);
    }

    #[test]
    fn crlf_terminators_are_not_shown() {
        let diff = diff_texts("a\r\nb\r\n", "a\r\nc\r\n", CONTEXT_LINES);
        assert_eq!(kinds(&diff), [" a", "-b", "+c"]);
    }

    #[test]
    fn the_indent_heuristic_keeps_a_new_block_whole() {
        // The inserted lines could equally be placed one line earlier
        // (blank line first). Git's indent heuristic, which
        // `postprocess_lines` applies, puts the blank line last so the hunk
        // reads as one whole function.
        let old = "fn a() {\n    1\n}\n\nfn c() {\n    3\n}\n";
        let new = "fn a() {\n    1\n}\n\nfn b() {\n    2\n}\n\nfn c() {\n    3\n}\n";
        let diff = diff_texts(old, new, 0);
        assert_eq!(diff.hunks, [Hunk { old: 4..4, new: 4..8 }]);
    }
}
