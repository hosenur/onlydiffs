//! Syntax highlighting on tree-sitter: text and a path in, coloured spans per
//! line out. Moved here from the ~/Playground/highlight experiment.
//!
//! ```text
//! path ─▶ LanguageRegistry ─▶ Language (grammar + highlights.scm + map)
//! text ─▶ Source ─▶ tree-sitter Tree ─▶ captures ─▶ Highlights (per line)
//! ```
//!
//! A diff highlights the whole old and new files, not lines one by one, so a
//! line in the middle of a block comment or string is still coloured as one.
//! Start reading at `Highlighter::highlight` in highlight.rs.

mod highlight;
mod language;
mod registry;
mod source;
pub mod theme;

use std::path::Path;
use std::sync::{Arc, OnceLock};

pub use highlight::{Highlights, Span};

use highlight::Highlighter;
use registry::LanguageRegistry;
use source::Source;

/// Files past this are left plain: highlighting would take longer than the
/// read is worth.
const MAX_BYTES: usize = 2 * 1024 * 1024;

fn registry() -> &'static LanguageRegistry {
    static REGISTRY: OnceLock<LanguageRegistry> = OnceLock::new();
    REGISTRY.get_or_init(LanguageRegistry::bundled)
}

/// `text` as the file at `path` highlights, or `None` for a language this
/// build does not know, a file too big, or one the grammar failed on.
pub fn highlight(path: &str, text: &str) -> Option<Arc<Highlights>> {
    if text.is_empty() || text.len() > MAX_BYTES {
        return None;
    }
    let first_line = text.lines().next().unwrap_or("");
    let language = registry().for_path(Path::new(path), first_line).ok()??;
    let source = Source::new(text);
    Highlighter::default().highlight(&language, &source).ok().map(Arc::new)
}
