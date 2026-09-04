//! Path arithmetic shared by everything that compares a recorded working
//! directory with the repository on screen.

use std::path::{Component, Path, PathBuf};

/// Lexical `.`/`..` folding, the equivalent of Node's `path.normalize`.
///
/// Deliberately not `canonicalize`: symlinks stay unresolved, so a path the
/// user typed comes back as typed, and a working directory a session recorded
/// is compared in the form it was recorded in.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Whether `candidate` is `root` or somewhere inside it, after folding both.
///
/// Inside counts: a session opened in a subdirectory is a session on the
/// repository. The comparison is by component, so a sibling that merely
/// starts with the same characters does not match.
pub fn is_within(candidate: &Path, root: &Path) -> bool {
    let candidate = normalize(candidate);
    let root = normalize(root);
    candidate == root || candidate.starts_with(&root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_folded_before_it_is_compared() {
        assert_eq!(normalize(Path::new("/a/b/../c/./d")), PathBuf::from("/a/c/d"));
    }

    #[test]
    fn a_subdirectory_is_within_the_repository_and_a_sibling_is_not() {
        let root = Path::new("/w/repo");
        assert!(is_within(Path::new("/w/repo"), root));
        assert!(is_within(Path::new("/w/repo/api/src"), root));
        assert!(is_within(Path::new("/w/repo/api/../api/src"), root));
        assert!(!is_within(Path::new("/w/other"), root));
        assert!(!is_within(Path::new("/w/repo-two"), root));
    }
}
