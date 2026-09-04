//! Reading the repository's changes: the metadata walk, the per-file patches,
//! the full file versions behind each card, and committing.
//!
//! The walk is a fixed number of git invocations, however many files changed.
//! `git status` names every changed path, and two `--numstat` runs — one for
//! the working tree, one for the index — give every count in one answer each.
//! It used to be one `git diff` per changed file, which on a hundred-file
//! change was a hundred processes on every refresh the watcher triggered, and
//! on macOS a process is not cheap. Patches are still produced per file, but
//! only when something asks for one: a card opening, or the commit-message
//! model being shown the whole diff.

use std::collections::HashMap;
use std::path::Path;

use futures::stream::{self, StreamExt};

use crate::contract::{ChangeStatus, FileChange, FullFileContents, RepoDiff};
use crate::error::AppError;
use crate::services::repository::Repository;

/// How many children or file reads may be in flight while collecting the repo.
const CONCURRENCY: usize = 8;

/// The most of a working-tree file the diff view will load. A file past this is
/// one no renderer is going to draw, and the bound is what lets the same read
/// cross a network without a caller having promised anything about its size.
const MAX_WORKTREE_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// How far into a file git looks for a NUL before calling it binary.
const BINARY_SNIFF_BYTES: usize = 8000;

/// One half of a porcelain record: a path as either its staged or its unstaged
/// change. A path edited, staged, then edited again produces both.
#[derive(Debug, Clone)]
struct Side {
    path: String,
    old_path: Option<String>,
    status: ChangeStatus,
    staged: bool,
}

/// What `--numstat` says about one path: added and deleted lines, or binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Counts {
    additions: u32,
    deletions: u32,
    binary: bool,
}

/// Maps one side of a porcelain XY pair to a status. `None` means that side has
/// no change at all.
fn classify(code: u8) -> Option<ChangeStatus> {
    match code {
        b' ' => None,
        b'?' => Some(ChangeStatus::Untracked),
        b'R' | b'C' => Some(ChangeStatus::Renamed),
        b'A' => Some(ChangeStatus::Added),
        b'D' => Some(ChangeStatus::Deleted),
        _ => Some(ChangeStatus::Modified),
    }
}

/// Repository-relative and staying inside the repository. Paths reach this from
/// the renderer, so `..`, a leading separator, and a drive letter are all
/// rejected before they can be handed to `git show` or joined onto the root.
fn is_safe_repo_path(value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    if value.starts_with('/') || value.starts_with('\\') {
        return false;
    }
    // Windows drive letters, e.g. `C:`.
    let bytes = value.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return false;
    }
    !value.split(['/', '\\']).any(|segment| segment == "..")
}

fn validate_path(value: &str) -> Result<(), AppError> {
    if is_safe_repo_path(value) {
        Ok(())
    } else {
        Err(AppError::InvalidPath(format!(
            "invalid repository-relative path: {value}"
        )))
    }
}

async fn worktree_file(repo: &Repository, file_path: &str) -> Result<String, AppError> {
    let bytes = repo
        .read_file(Path::new(file_path), MAX_WORKTREE_FILE_BYTES)
        .await?;
    // Lossy by design, matching how the file is read everywhere else.
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// The patch for one side. Per file, so only asked for when a patch is wanted.
async fn file_patch(repo: &Repository, side: &Side) -> Result<String, AppError> {
    match (side.status, side.staged, side.old_path.as_deref()) {
        (ChangeStatus::Untracked, _, _) => {
            repo.git(&["diff", "--no-index", "--", "/dev/null", &side.path])
                .await
        }
        (_, true, None) => repo.git(&["diff", "--cached", "--", &side.path]).await,
        (_, true, Some(old_path)) => {
            repo.git(&["diff", "--cached", "-M", "--", old_path, &side.path])
                .await
        }
        (_, false, _) => repo.git(&["diff", "--", &side.path]).await,
    }
}

fn to_file_change(side: &Side, counts: Counts, error: Option<String>) -> FileChange {
    FileChange {
        id: format!(
            "{}:{}",
            if side.staged { "staged" } else { "unstaged" },
            side.path
        ),
        path: side.path.clone(),
        old_path: side.old_path.clone(),
        status: side.status,
        staged: side.staged,
        additions: counts.additions,
        deletions: counts.deletions,
        binary: counts.binary,
        error,
    }
}

/// Walks `git status --porcelain -z` into one entry per changed side.
fn parse_porcelain(status: &str) -> Vec<Side> {
    // With -z each record is "XY PATH\0"; renames and copies add the original
    // path as the next NUL-terminated field.
    let records: Vec<&str> = status.split('\0').filter(|record| !record.is_empty()).collect();
    let mut sides = Vec::new();

    let mut index = 0;
    while index < records.len() {
        let record = records[index];
        index += 1;
        if record.len() < 4 || !record.is_char_boundary(3) {
            continue;
        }

        let bytes = record.as_bytes();
        let index_code = bytes[0];
        let work_tree_code = bytes[1];
        let file_path = &record[3..];

        let mut old_path = None;
        if matches!(index_code, b'R' | b'C') || matches!(work_tree_code, b'R' | b'C') {
            if index >= records.len() {
                continue;
            }
            old_path = Some(records[index].to_owned());
            index += 1;
        }

        let index_status = if index_code == b'?' {
            None
        } else {
            classify(index_code)
        };

        if let Some(status) = index_status {
            sides.push(Side {
                path: file_path.to_owned(),
                old_path: old_path.clone(),
                status,
                staged: true,
            });
        }
        if let Some(status) = classify(work_tree_code) {
            sides.push(Side {
                path: file_path.to_owned(),
                old_path,
                status,
                staged: false,
            });
        }
    }

    sides
}

/// Walks `git diff --numstat -z` into counts keyed by the path as it is now.
///
/// Each record is `added TAB deleted TAB path NUL`. A rename or copy is
/// `added TAB deleted TAB NUL old NUL new NUL` instead — the path field empty
/// and the two names following — and a binary file has `-` for both counts.
fn parse_numstat(output: &str) -> HashMap<String, Counts> {
    let mut counts = HashMap::new();
    let mut fields = output.split('\0');
    while let Some(record) = fields.next() {
        if record.is_empty() {
            continue;
        }
        let mut parts = record.splitn(3, '\t');
        let (Some(added), Some(deleted), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let path = if path.is_empty() {
            // A rename: skip the old name, keep the new one.
            let _old = fields.next();
            match fields.next() {
                Some(new_path) if !new_path.is_empty() => new_path,
                _ => continue,
            }
        } else {
            path
        };
        let binary = added == "-" || deleted == "-";
        counts.insert(
            path.to_owned(),
            Counts {
                additions: if binary { 0 } else { added.parse().unwrap_or(0) },
                deletions: if binary { 0 } else { deleted.parse().unwrap_or(0) },
                binary,
            },
        );
    }
    counts
}

/// What `git diff --no-index /dev/null file` would have counted, without the
/// process: every line is an addition, and a NUL near the top makes it binary.
fn count_untracked(bytes: &[u8]) -> Counts {
    let head = &bytes[..bytes.len().min(BINARY_SNIFF_BYTES)];
    if head.contains(&0) {
        return Counts {
            additions: 0,
            deletions: 0,
            binary: true,
        };
    }
    let mut lines = bytes.iter().filter(|byte| **byte == b'\n').count();
    // A final line with no newline is still a line, as git counts it.
    if !bytes.is_empty() && bytes.last() != Some(&b'\n') {
        lines += 1;
    }
    Counts {
        additions: u32::try_from(lines).unwrap_or(u32::MAX),
        deletions: 0,
        binary: false,
    }
}

/// One untracked row, read rather than diffed.
///
/// A named `async fn` rather than an inline async block: a closure returning a
/// block that borrows its argument cannot satisfy the higher-ranked bound
/// `buffered` needs, while an `async fn` call elides the lifetimes cleanly.
async fn untracked_row(repo: &Repository, side: &Side) -> FileChange {
    match repo
        .read_file(Path::new(&side.path), MAX_WORKTREE_FILE_BYTES)
        .await
    {
        Ok(bytes) => to_file_change(side, count_untracked(&bytes), None),
        // One unreadable path shouldn't blank the whole view — surface it on
        // its own row instead.
        Err(error) => to_file_change(
            side,
            Counts {
                additions: 0,
                deletions: 0,
                binary: false,
            },
            Some(error.message().to_owned()),
        ),
    }
}

/// The patch behind one already-collected row, paired back up with it.
async fn patch_for<'a>(
    repo: &Repository,
    file: &'a FileChange,
) -> Result<(&'a FileChange, String), AppError> {
    let side = Side {
        path: file.path.clone(),
        old_path: file.old_path.clone(),
        status: file.status,
        staged: file.staged,
    };
    file_patch(repo, &side).await.map(|patch| (file, patch))
}

/// Metadata for every change in the repo, untracked files included. Staged and
/// unstaged edits to the same path are returned as separate rows, because they
/// are genuinely two different patches.
///
/// Five git invocations, whatever the size of the change. A tracked path that
/// `git status` lists but neither `--numstat` mentions — a mode change and
/// nothing else — has no patch to show and is left out, as it always was.
pub async fn get_diff(repo: &Repository) -> Result<RepoDiff, AppError> {
    let (branch, head, status, unstaged, staged) = tokio::try_join!(
        repo.git(&["rev-parse", "--abbrev-ref", "HEAD"]),
        repo.git(&["log", "-1", "--pretty=%h %s"]),
        // -uall matters: without it an untracked *directory* collapses into a
        // single "?? dir/" record, and diffing a directory is not a thing.
        repo.git(&["status", "--porcelain", "-z", "-uall"]),
        repo.git(&["diff", "--numstat", "-z"]),
        repo.git(&["diff", "--cached", "--numstat", "-z", "-M"]),
    )?;

    let unstaged = parse_numstat(&unstaged);
    let staged = parse_numstat(&staged);
    let sides = parse_porcelain(&status);

    let mut files: Vec<FileChange> = Vec::with_capacity(sides.len());
    let mut untracked = Vec::new();
    for side in &sides {
        match side.status {
            ChangeStatus::Untracked => untracked.push(side),
            _ => {
                let counted = if side.staged { &staged } else { &unstaged };
                if let Some(counts) = counted.get(&side.path) {
                    files.push(to_file_change(side, *counts, None));
                }
            }
        }
    }

    // The futures are built up front rather than by a closure inside
    // `stream::iter`: a closure mapping a reference to a future it borrows from
    // cannot satisfy the higher-ranked bound, and a plain `Vec` sidesteps it.
    let mut pending = Vec::with_capacity(untracked.len());
    for side in untracked {
        pending.push(untracked_row(repo, side));
    }
    let read: Vec<FileChange> = stream::iter(pending).buffered(CONCURRENCY).collect().await;
    files.extend(read);

    files.sort_by(|a, b| a.path.cmp(&b.path).then(b.staged.cmp(&a.staged)));

    Ok(RepoDiff {
        repo_path: repo.display_path(),
        branch: branch.trim().to_owned(),
        head: head.trim().to_owned(),
        files,
    })
}

/// Loads complete file versions. Kept separate from `get_diff` so startup never
/// reads every changed file — cards ask for this as they approach the viewport.
pub async fn get_file_contents(
    repo: &Repository,
    file_path: &str,
    old_path: Option<&str>,
    status: ChangeStatus,
    staged: bool,
) -> Result<FullFileContents, AppError> {
    validate_path(file_path)?;
    if let Some(old_path) = old_path {
        validate_path(old_path)?;
    }

    if status == ChangeStatus::Untracked {
        return Ok(FullFileContents {
            old_contents: None,
            new_contents: Some(worktree_file(repo, file_path).await?),
        });
    }

    if staged {
        let old_contents = if status == ChangeStatus::Added {
            None
        } else {
            Some(repo.show_file("HEAD", old_path.unwrap_or(file_path)).await?)
        };
        let new_contents = if status == ChangeStatus::Deleted {
            None
        } else {
            Some(repo.show_file("", file_path).await?)
        };
        return Ok(FullFileContents {
            old_contents,
            new_contents,
        });
    }

    let old_contents = if status == ChangeStatus::Added {
        None
    } else {
        let source = if status == ChangeStatus::Renamed {
            old_path.unwrap_or(file_path)
        } else {
            file_path
        };
        Some(repo.show_file("", source).await?)
    };
    let new_contents = if status == ChangeStatus::Deleted {
        None
    } else {
        Some(worktree_file(repo, file_path).await?)
    };

    Ok(FullFileContents {
        old_contents,
        new_contents,
    })
}

/// Stages the current file, including the previous path when the change is a
/// rename so Git records both halves of the move.
pub async fn stage_file(
    repo: &Repository,
    file_path: &str,
    old_path: Option<&str>,
) -> Result<(), AppError> {
    validate_path(file_path)?;
    if let Some(old_path) = old_path {
        validate_path(old_path)?;
    }

    let mut args = vec!["add", "-A", "--", file_path];
    if let Some(old_path) = old_path {
        if old_path != file_path {
            args.push(old_path);
        }
    }
    repo.git(&args).await?;
    Ok(())
}

/// Stages everything and commits it in one step.
///
/// `git add -A` then `git commit` rather than `commit -a`, because `-a` ignores
/// untracked files and this is the "commit all" the user means. The message
/// goes through `-m` as a single argument, so nothing in it is interpreted as a
/// flag or reaches a shell.
pub async fn commit_all(repo: &Repository, message: &str) -> Result<String, AppError> {
    let subject = message.trim();
    if subject.is_empty() {
        return Err(AppError::InvalidPath("A commit needs a message.".into()));
    }

    let status = repo.git(&["status", "--porcelain"]).await?;
    if status.trim().is_empty() {
        return Err(AppError::Git(
            "Nothing to commit — the working tree is clean.".into(),
        ));
    }

    repo.git(&["add", "-A"]).await?;
    repo.git(&["commit", "-m", subject]).await?;
    let head = repo.git(&["log", "-1", "--pretty=%h %s"]).await?;
    Ok(head.trim().to_owned())
}

/// The complete staged, unstaged, and untracked diff as one annotated document
/// — what the commit-message model is shown.
///
/// This is the one place every patch is wanted at once, and it is a background
/// call, so the per-file `git diff` is paid here rather than on every refresh.
pub async fn commit_message_diff(repo: &Repository) -> Result<String, AppError> {
    let clean = || AppError::Git("Working tree is clean; there is no diff to summarize.".into());

    let repo_diff = get_diff(repo).await?;
    if repo_diff.files.is_empty() {
        return Err(clean());
    }

    let mut pending = Vec::with_capacity(repo_diff.files.len());
    for file in &repo_diff.files {
        pending.push(patch_for(repo, file));
    }

    let patches: Vec<(&FileChange, String)> = stream::iter(pending)
        .buffered(CONCURRENCY)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<_, _>>()?;

    let mut diff = String::new();
    for (file, patch) in patches {
        if patch.trim().is_empty() {
            continue;
        }
        let section = match (file.status, file.staged) {
            (ChangeStatus::Untracked, _) => "untracked",
            (_, true) => "staged",
            (_, false) => "unstaged",
        };
        diff.push_str(&format!("### {section}: {}\n", file.path));
        diff.push_str(&patch);
        if !patch.ends_with('\n') {
            diff.push('\n');
        }
        diff.push('\n');
    }

    if diff.trim().is_empty() {
        return Err(clean());
    }
    Ok(diff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numstat_records_are_read_including_renames_and_binaries() {
        let output = "3\t1\tsrc/a.rs\0-\t-\timg/logo.png\0\
                      2\t0\t\0old/name.txt\0new/name.txt\0\
                      0\t5\tgone.txt\0";

        let counts = parse_numstat(output);

        assert_eq!(counts["src/a.rs"], Counts { additions: 3, deletions: 1, binary: false });
        assert_eq!(counts["img/logo.png"], Counts { additions: 0, deletions: 0, binary: true });
        assert_eq!(counts["new/name.txt"], Counts { additions: 2, deletions: 0, binary: false });
        assert!(!counts.contains_key("old/name.txt"), "a rename is keyed by where it is now");
        assert_eq!(counts["gone.txt"].deletions, 5);
        assert_eq!(counts.len(), 4);
    }

    #[test]
    fn an_untracked_file_is_counted_the_way_git_would_count_it() {
        assert_eq!(count_untracked(b"one\ntwo\n").additions, 2);
        // The last line without a newline is still a line.
        assert_eq!(count_untracked(b"one\ntwo").additions, 2);
        assert_eq!(count_untracked(b"").additions, 0);
        assert!(!count_untracked(b"text").binary);
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.push(0);
        let counted = count_untracked(&png);
        assert!(counted.binary);
        assert_eq!(counted.additions, 0);
    }

    #[test]
    fn a_path_that_leaves_the_repository_is_refused() {
        assert!(is_safe_repo_path("src/main.rs"));
        assert!(!is_safe_repo_path("../outside"));
        assert!(!is_safe_repo_path("/etc/passwd"));
        assert!(!is_safe_repo_path("C:\\windows"));
        assert!(!is_safe_repo_path(""));
    }
}
