//! The text being highlighted, and where each of its lines starts.
//!
//! Tree-sitter speaks byte offsets; a diff speaks line numbers. This is the
//! table between the two. The text is read-only, so a plain `String` does
//! what Zed needs a rope for.

use std::ops::Range;

pub struct Source {
    text: String,
    /// Byte offset of the start of each line. Always holds at least one entry,
    /// so an empty text is one empty line.
    line_starts: Vec<usize>,
}

impl Source {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let mut line_starts = vec![0];
        line_starts.extend(text.match_indices('\n').map(|(at, _)| at + 1));
        // A final newline ends the last line rather than starting a new one.
        if line_starts.len() > 1 && line_starts.last() == Some(&text.len()) {
            line_starts.pop();
        }
        Self { text, line_starts }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// The bytes of line `line` (zero-based), without its line ending.
    pub fn line_range(&self, line: usize) -> Range<usize> {
        let start = self.line_starts[line];
        let mut end = self.line_starts.get(line + 1).copied().unwrap_or(self.text.len());
        if self.text[start..end].ends_with('\n') {
            end -= 1;
        }
        if self.text[start..end].ends_with('\r') {
            end -= 1;
        }
        start..end
    }

    #[cfg(test)]
    pub fn line(&self, line: usize) -> &str {
        &self.text[self.line_range(line)]
    }

    /// The line that byte `offset` is on.
    /// Binary search over `line_starts`, so this is O(log lines).
    #[cfg(test)]
    pub fn line_of(&self, offset: usize) -> usize {
        self.line_starts.partition_point(|&start| start <= offset) - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_split_without_their_endings() {
        let source = Source::new("one\r\ntwo\nthree\n");
        assert_eq!(source.line_count(), 3);
        assert_eq!(source.line(0), "one");
        assert_eq!(source.line(1), "two");
        assert_eq!(source.line(2), "three");
    }

    #[test]
    fn an_empty_text_is_one_empty_line() {
        let source = Source::new("");
        assert_eq!(source.line_count(), 1);
        assert_eq!(source.line(0), "");
    }

    #[test]
    fn a_byte_knows_its_line() {
        let source = Source::new("ab\ncd\n");
        assert_eq!(source.line_of(0), 0);
        assert_eq!(source.line_of(2), 0); // the newline belongs to its line
        assert_eq!(source.line_of(3), 1);
    }
}
