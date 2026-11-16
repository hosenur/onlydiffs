//! Parse a file, run its language's rules over the tree, and flatten what
//! they tag into coloured spans that never overlap, grouped by line.

use std::cmp::Reverse;
use std::ops::Range;

use tree_sitter::{Parser, QueryCursor, StreamingIterator};

use super::language::Language;
use super::source::Source;
use super::theme::HighlightId;

/// A run of one colour within a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// Bytes within the line, not the file.
    pub range: Range<usize>,
    pub id: HighlightId,
}

/// The output: for every line, its coloured spans in order. Text between
/// spans has no colour of its own.
#[derive(Debug, Default)]
pub struct Highlights {
    pub lines: Vec<Vec<Span>>,
}

/// Keeps a parser and query cursor to reuse across files.
pub struct Highlighter {
    parser: Parser,
    cursor: QueryCursor,
}

impl Default for Highlighter {
    fn default() -> Self {
        Self { parser: Parser::new(), cursor: QueryCursor::new() }
    }
}

/// One tagged node, before overlaps are settled.
struct Tag {
    range: Range<usize>,
    pattern: usize,
    id: Option<HighlightId>,
}

impl Highlighter {
    pub fn highlight(
        &mut self,
        language: &Language,
        source: &Source,
    ) -> Result<Highlights, String> {
        let text = source.text();
        self.parser
            .set_language(&language.grammar)
            .map_err(|error| format!("{}: {error}", language.name))?;
        let tree = self.parser.parse(text, None).ok_or("the parse was cancelled")?;

        let names = language.capture_names();
        let map = &language.map;
        let mut tags = Vec::new();
        let mut captures = self.cursor.captures(&language.highlights, tree.root_node(), text.as_bytes());
        while let Some((found, index)) = captures.next() {
            let capture = found.captures[*index];
            let id = map.get(capture.index);
            // A capture the theme does not colour leaves the text as it was,
            // except `@none`, which a rule uses to clear what it sits in.
            if id.is_none() && names[capture.index as usize] != "none" {
                continue;
            }
            tags.push(Tag { range: capture.node.byte_range(), pattern: found.pattern_index, id });
        }

        // Settle overlaps by painting: outer nodes first so inner ones paint
        // over them, and for the same node the later rule first so the earlier
        // rule — which by tree-sitter's convention wins — paints last.
        tags.sort_by_key(|tag| (tag.range.start, Reverse(tag.range.end), Reverse(tag.pattern)));
        // One slot per byte: 4 bytes each, so a 1 MB file costs 4 MB while it
        // is highlighted. Simple and fast enough for phase 0; a sweep over the
        // sorted tags would need no buffer at all.
        let mut paint: Vec<Option<HighlightId>> = vec![None; text.len()];
        for tag in &tags {
            paint[tag.range.clone()].fill(tag.id);
        }

        let lines = (0..source.line_count())
            .map(|line| spans(&paint, source.line_range(line)))
            .collect();
        // Code the parser cannot make sense of is marked as an error node
        // and the rest still highlights, so broken code needs no special case.
        Ok(Highlights { lines })
    }
}

/// Runs of the same colour over `range` of `paint`, relative to its start.
fn spans(paint: &[Option<HighlightId>], range: Range<usize>) -> Vec<Span> {
    let line = &paint[range];
    let mut spans: Vec<Span> = Vec::new();
    for (at, id) in line.iter().enumerate() {
        let Some(id) = *id else { continue };
        match spans.last_mut() {
            Some(last) if last.id == id && last.range.end == at => last.range.end = at + 1,
            _ => spans.push(Span { range: at..at + 1, id }),
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::syntax::registry::LanguageRegistry;
    use crate::syntax::theme::key;

    /// Each coloured piece of `line`, as (text, theme key).
    fn pieces(path: &str, text: &str, line: usize) -> Vec<(String, &'static str)> {
        let registry = LanguageRegistry::bundled();
        let language = registry.for_path(Path::new(path), "").unwrap().unwrap();
        let source = Source::new(text);
        let highlights = Highlighter::default().highlight(&language, &source).unwrap();
        let line_text = source.line(line);
        highlights.lines[line]
            .iter()
            .map(|span| (line_text[span.range.clone()].to_owned(), key(span.id)))
            .collect()
    }

    fn has(pieces: &[(String, &str)], text: &str, key: &str) -> bool {
        pieces.iter().any(|(t, k)| t == text && *k == key)
    }

    #[test]
    fn rust_gets_its_parts_coloured() {
        let p = pieces("a.rs", "fn add(a: i32) -> i32 { a + 1 }", 0);
        assert!(has(&p, "fn", "keyword"), "{p:?}");
        assert!(has(&p, "add", "function"), "{p:?}");
        assert!(has(&p, "i32", "type.builtin"), "{p:?}");
    }

    #[test]
    fn a_line_inside_a_block_comment_is_still_a_comment() {
        // The reason to highlight whole files: alone, line 1 is not a comment.
        let p = pieces("a.rs", "/* start\nlet x = 1;\nend */", 1);
        assert_eq!(p, [("let x = 1;".to_owned(), "comment")]);
    }

    #[test]
    fn an_inner_node_paints_over_the_one_around_it() {
        let p = pieces("a.rs", r#"let s = "a\nb";"#, 0);
        assert!(has(&p, "\\n", "escape"), "{p:?}");
    }

    #[test]
    fn typescript_uses_its_own_rules_and_javascripts() {
        let p = pieces("a.ts", "const n: number = parse(x);", 0);
        assert!(has(&p, "const", "keyword"), "{p:?}");
        assert!(has(&p, "number", "type.builtin"), "{p:?}");
        assert!(has(&p, "parse", "function"), "{p:?}");
    }
}
