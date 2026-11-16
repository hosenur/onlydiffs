//! The languages this build knows, and which one a file is.
//!
//! A language is only built — its highlight query compiled — the first time
//! a file needs it.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use super::language::{FileMatcher, Language};

struct Entry {
    matcher: FileMatcher,
    build: fn() -> Result<Language, String>,
    built: OnceLock<Result<Arc<Language>, String>>,
}

pub struct LanguageRegistry {
    entries: Vec<Entry>,
}

macro_rules! matcher {
    ($ext:expr) => {
        FileMatcher { extensions: $ext, file_names: &[], shebangs: &[] }
    };
    ($ext:expr, names: $names:expr) => {
        FileMatcher { extensions: $ext, file_names: $names, shebangs: &[] }
    };
    ($ext:expr, shebangs: $bangs:expr) => {
        FileMatcher { extensions: $ext, file_names: &[], shebangs: $bangs }
    };
}

const RUST: FileMatcher = matcher!(&["rs"]);
const JAVASCRIPT: FileMatcher =
    matcher!(&["js", "mjs", "cjs", "jsx"], shebangs: &["node", "bun", "deno"]);
const TYPESCRIPT: FileMatcher = matcher!(&["ts", "mts", "cts"]);
const TSX: FileMatcher = matcher!(&["tsx"]);
const PYTHON: FileMatcher = matcher!(&["py", "pyi"], shebangs: &["python", "python3"]);
const JSON: FileMatcher =
    matcher!(&["json", "jsonc"], names: &["package-lock.json", ".prettierrc", ".babelrc"]);
const GO: FileMatcher = matcher!(&["go"]);

impl LanguageRegistry {
    /// The languages compiled into this build.
    pub fn bundled() -> Self {
        fn entry(matcher: FileMatcher, build: fn() -> Result<Language, String>) -> Entry {
            Entry { matcher, build, built: OnceLock::new() }
        }
        Self {
            entries: vec![
                entry(RUST, || {
                    Language::new("Rust", tree_sitter_rust::LANGUAGE, &[tree_sitter_rust::HIGHLIGHTS_QUERY])
                }),
                entry(JAVASCRIPT, || {
                    Language::new(
                        "JavaScript",
                        tree_sitter_javascript::LANGUAGE,
                        &[tree_sitter_javascript::HIGHLIGHT_QUERY, tree_sitter_javascript::JSX_HIGHLIGHT_QUERY],
                    )
                }),
                // TypeScript's rules extend JavaScript's: its own go first so
                // they win where both tag a node.
                entry(TYPESCRIPT, || {
                    Language::new(
                        "TypeScript",
                        tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
                        &[tree_sitter_typescript::HIGHLIGHTS_QUERY, tree_sitter_javascript::HIGHLIGHT_QUERY],
                    )
                }),
                entry(TSX, || {
                    Language::new(
                        "TSX",
                        tree_sitter_typescript::LANGUAGE_TSX,
                        &[
                            tree_sitter_typescript::HIGHLIGHTS_QUERY,
                            tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
                            tree_sitter_javascript::HIGHLIGHT_QUERY,
                        ],
                    )
                }),
                entry(PYTHON, || {
                    Language::new("Python", tree_sitter_python::LANGUAGE, &[tree_sitter_python::HIGHLIGHTS_QUERY])
                }),
                entry(JSON, || {
                    Language::new("JSON", tree_sitter_json::LANGUAGE, &[tree_sitter_json::HIGHLIGHTS_QUERY])
                }),
                entry(GO, || {
                    Language::new("Go", tree_sitter_go::LANGUAGE, &[tree_sitter_go::HIGHLIGHTS_QUERY])
                }),
            ],
        }
    }

    /// The language for `path`: by exact file name, then extension, then the
    /// interpreter on a `#!` first line. `None` means plain text.
    pub fn for_path(&self, path: &Path, first_line: &str) -> Result<Option<Arc<Language>>, String> {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let interpreter = shebang_interpreter(first_line);

        let found = self
            .entries
            .iter()
            .find(|e| e.matcher.file_names.contains(&name))
            .or_else(|| {
                self.entries
                    .iter()
                    .find(|e| e.matcher.extensions.iter().any(|x| x.eq_ignore_ascii_case(extension)))
            })
            .or_else(|| {
                let interpreter = interpreter?;
                self.entries.iter().find(|e| e.matcher.shebangs.contains(&interpreter))
            });
        found.map(|entry| self.build(entry)).transpose()
    }

    /// A language by name, ignoring case.
    #[cfg(test)]
    pub fn by_name(&self, wanted: &str) -> Result<Option<Arc<Language>>, String> {
        for entry in &self.entries {
            let language = self.build(entry)?;
            if language.name.eq_ignore_ascii_case(wanted) {
                return Ok(Some(language));
            }
        }
        Ok(None)
    }

    #[cfg(test)]
    pub fn names(&self) -> Result<Vec<&'static str>, String> {
        self.entries.iter().map(|e| self.build(e).map(|l| l.name)).collect()
    }

    fn build(&self, entry: &Entry) -> Result<Arc<Language>, String> {
        entry.built.get_or_init(|| (entry.build)().map(Arc::new)).clone()
    }
}

/// `#!/usr/bin/env python3` → `python3`; `#!/bin/bash` → `bash`.
fn shebang_interpreter(first_line: &str) -> Option<&str> {
    let rest = first_line.strip_prefix("#!")?;
    let mut words = rest.split_whitespace();
    let program = words.next()?.rsplit('/').next()?;
    if program == "env" {
        words.find(|word| !word.starts_with('-'))
    } else {
        Some(program)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name_for(path: &str, first_line: &str) -> Option<&'static str> {
        LanguageRegistry::bundled()
            .for_path(Path::new(path), first_line)
            .unwrap()
            .map(|l| l.name)
    }

    #[test]
    fn every_bundled_query_compiles() {
        let names = LanguageRegistry::bundled().names().unwrap();
        assert_eq!(names.len(), 7);
    }

    #[test]
    fn files_are_placed_by_extension_name_and_shebang() {
        assert_eq!(name_for("src/main.rs", ""), Some("Rust"));
        assert_eq!(name_for("app/page.tsx", ""), Some("TSX"));
        assert_eq!(name_for("a.ts", ""), Some("TypeScript"));
        assert_eq!(name_for("package-lock.json", ""), Some("JSON"));
        assert_eq!(name_for("bin/run", "#!/usr/bin/env python3"), Some("Python"));
        assert_eq!(name_for("bin/run", "#!/usr/bin/env -S node --flag"), Some("JavaScript"));
        assert_eq!(name_for("README", ""), None);
    }
}
