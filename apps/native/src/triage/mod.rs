//! Triage: which changed files can safely be skimmed, and which lockfile
//! changes deserve a careful read. A hint for the reader — nothing is hidden
//! or skipped because of it.
//!
//! Rules only (rules.rs): lockfiles, generated files, assets, whitespace.
//! Files no rule covers get no mark.

pub mod rules;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Review,
    Skim,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub level: Level,
    pub reason: String,
}

impl Verdict {
    /// For the hover text on a file's mark.
    pub fn describe(&self) -> String {
        let what = match self.level {
            Level::Review => "Needs review",
            Level::Skim => "Skim",
        };
        format!("{what}: {}", self.reason)
    }
}

/// A verdict when a rule covers the file.
pub fn judge(path: &str, before: &str, after: &str) -> Option<Verdict> {
    rules::judge(path, before, after)
}
