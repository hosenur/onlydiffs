//! Everything one language needs: its parser, its highlighting rules, and
//! which files it applies to.

use tree_sitter::Query;
use tree_sitter_language::LanguageFn;

use super::theme::HighlightMap;

pub struct Language {
    pub name: &'static str,
    pub grammar: tree_sitter::Language,
    /// The grammar's own `highlights.scm`: patterns that tag syntax nodes with
    /// capture names such as `@function.method` or `@string`.
    pub highlights: Query,
    /// Its capture names onto the theme's keys, settled once.
    pub map: HighlightMap,
}

/// Which files a language claims.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileMatcher {
    /// Without the dot: `["rs"]`.
    pub extensions: &'static [&'static str],
    /// Whole names, for files an extension cannot place: `["Cargo.lock"]`.
    pub file_names: &'static [&'static str],
    /// Interpreters named on a `#!` first line: `["python3", "python"]`.
    pub shebangs: &'static [&'static str],
}

impl Language {
    /// `queries` are joined in order. When two patterns tag the same node the
    /// earlier one wins, which is how a language extends another — TypeScript
    /// puts its own rules after JavaScript's.
    pub fn new(
        name: &'static str,
        grammar: LanguageFn,
        queries: &[&str],
    ) -> Result<Self, String> {
        let grammar = tree_sitter::Language::new(grammar);
        let source = queries.join("\n");
        let highlights = Query::new(&grammar, &source)
            .map_err(|error| format!("{name} highlight query: {error}"))?;
        let map = HighlightMap::new(highlights.capture_names());
        Ok(Self { name, grammar, highlights, map })
    }

    pub fn capture_names(&self) -> &[&str] {
        self.highlights.capture_names()
    }
}
