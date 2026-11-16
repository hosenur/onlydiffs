//! The project tree: every file in the repository as nested folders, and the
//! rows the sidebar shows for whichever folders are open.
//!
//! Follows Zed's project panel: folders before files, names compared
//! naturally (`file2` before `file10`) and without regard to case, and a run
//! of folders that each hold only one folder shown as one row
//! (`apps/desktop/src-tauri`), which Zed calls auto-folding.

use std::cmp::Ordering;
use std::collections::HashSet;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Dir {
    pub name: String,
    /// Repository-relative; empty for the root.
    pub path: String,
    pub dirs: Vec<Dir>,
    pub files: Vec<File>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Dir { expanded: bool },
    File,
}

/// One line of the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The file's path, or the deepest folder's for a folded run.
    pub path: String,
    /// What is shown: a name, or `a/b/c` for a folded run of folders.
    pub label: String,
    pub depth: usize,
    pub kind: RowKind,
}

/// The tree for a list of repository-relative file paths.
pub fn build<'a>(paths: impl IntoIterator<Item = &'a str>) -> Dir {
    let mut root = Dir::default();
    for path in paths {
        let mut dir = &mut root;
        let mut parts = path.split('/').filter(|part| !part.is_empty()).peekable();
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                if !dir.files.iter().any(|f| f.name == part) {
                    dir.files.push(File { name: part.to_owned(), path: path.to_owned() });
                }
                break;
            }
            let at = match dir.dirs.iter().position(|d| d.name == part) {
                Some(at) => at,
                None => {
                    let child_path =
                        if dir.path.is_empty() { part.to_owned() } else { format!("{}/{part}", dir.path) };
                    dir.dirs.push(Dir { name: part.to_owned(), path: child_path, ..Dir::default() });
                    dir.dirs.len() - 1
                }
            };
            dir = &mut dir.dirs[at];
        }
    }
    sort(&mut root);
    root
}

fn sort(dir: &mut Dir) {
    dir.dirs.sort_by(|a, b| natural(&a.name, &b.name));
    dir.files.sort_by(|a, b| natural(&a.name, &b.name));
    for child in &mut dir.dirs {
        sort(child);
    }
}

/// Compares names the way people count: digit runs by value, everything
/// else without regard to case, and case only to break a tie.
pub fn natural(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut digits = String::new();
                    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
                        digits.push(c);
                        it.next();
                    }
                    digits
                };
                let (m, n) = (take(&mut x), take(&mut y));
                let (m_trim, n_trim) = (m.trim_start_matches('0'), n.trim_start_matches('0'));
                let order = m_trim.len().cmp(&n_trim.len()).then_with(|| m_trim.cmp(n_trim));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(c), Some(d)) => {
                let order = c.to_lowercase().cmp(d.to_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
                x.next();
                y.next();
            }
        }
    }
}

/// The rows to show: the root's contents, and inside every open folder.
pub fn rows(root: &Dir, expanded: &HashSet<String>) -> Vec<Row> {
    let mut out = Vec::new();
    walk(root, 0, expanded, &mut out);
    out
}

fn walk(dir: &Dir, depth: usize, expanded: &HashSet<String>, out: &mut Vec<Row>) {
    for child in &dir.dirs {
        // Fold a run of folders that each hold just one folder.
        let mut shown = child;
        let mut label = child.name.clone();
        while shown.files.is_empty() && shown.dirs.len() == 1 {
            shown = &shown.dirs[0];
            label.push('/');
            label.push_str(&shown.name);
        }
        let open = expanded.contains(&shown.path);
        out.push(Row { path: shown.path.clone(), label, depth, kind: RowKind::Dir { expanded: open } });
        if open {
            walk(shown, depth + 1, expanded, out);
        }
    }
    for file in &dir.files {
        out.push(Row { path: file.path.clone(), label: file.name.clone(), depth, kind: RowKind::File });
    }
}

/// Every folder above `path`: `a/b/c.rs` → `a`, `a/b`.
pub fn ancestors(path: &str) -> impl Iterator<Item = String> + '_ {
    path.match_indices('/').map(move |(at, _)| path[..at].to_owned())
}

/// Every folder in the tree, for "expand all".
pub fn all_dirs(dir: &Dir, out: &mut HashSet<String>) {
    for child in &dir.dirs {
        out.insert(child.path.clone());
        all_dirs(child, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(root: &Dir, open: &[&str]) -> Vec<String> {
        let expanded: HashSet<String> = open.iter().map(|s| s.to_string()).collect();
        rows(root, &expanded)
            .into_iter()
            .map(|r| {
                let mark = match r.kind {
                    RowKind::Dir { expanded: true } => "▾ ",
                    RowKind::Dir { expanded: false } => "▸ ",
                    RowKind::File => "",
                };
                format!("{}{mark}{}", "  ".repeat(r.depth), r.label)
            })
            .collect()
    }

    #[test]
    fn folders_come_first_and_names_sort_naturally() {
        let root = build(["b.rs", "src/x.rs", "file10.txt", "File2.txt", "a.rs", "docs/a.md"]);
        assert_eq!(shown(&root, &[]), ["▸ docs", "▸ src", "a.rs", "b.rs", "File2.txt", "file10.txt"]);
    }

    #[test]
    fn open_folders_show_their_contents_indented() {
        let root = build(["src/main.rs", "src/app/view.rs", "README.md"]);
        assert_eq!(
            shown(&root, &["src", "src/app"]),
            ["▾ src", "  ▾ app", "    view.rs", "  main.rs", "README.md"]
        );
    }

    #[test]
    fn a_run_of_single_folders_is_one_row() {
        let root = build(["apps/native/src/main.rs", "apps/native/src/app.rs", "README.md"]);
        assert_eq!(shown(&root, &[]), ["▸ apps/native/src", "README.md"]);
        assert_eq!(shown(&root, &["apps/native/src"]), ["▾ apps/native/src", "  app.rs", "  main.rs", "README.md"]);
    }

    #[test]
    fn folding_stops_where_a_folder_holds_a_file() {
        let root = build(["apps/Cargo.toml", "apps/native/src/main.rs"]);
        assert_eq!(shown(&root, &["apps"]), ["▾ apps", "  ▸ native/src", "  Cargo.toml"]);
    }

    #[test]
    fn ancestors_are_every_folder_above() {
        assert_eq!(ancestors("a/b/c.rs").collect::<Vec<_>>(), ["a", "a/b"]);
        assert_eq!(ancestors("c.rs").count(), 0);
    }
}
