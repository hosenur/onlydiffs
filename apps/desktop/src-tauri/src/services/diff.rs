//! Reading the repository's changes: the metadata walk, the per-file patches,
//! the full file versions behind each card, and committing.

use futures::future;
use futures::stream::{self, StreamExt};

use crate::contract::{ChangeStatus, FileChange, FullFileContents, RepoDiff};
use crate::error::AppError;
use crate::services::git;
use crate::services::workspace::Workspace;

/// How many `git diff` children may be in flight while collecting the repo.
const PATCH_CONCURRENCY: usize = 8;

/// One half of a porcelain record: a path as either its staged or its unstaged
/// change. A path edited, staged, then edited again produces both.
#[derive(Debug, Clone)]
struct Side {
    path: String,
    old_path: Option<String>,
    status: ChangeStatus,
    staged: bool,
}

/// Splits on newlines the way Rust's `str::lines` does — `\r\n` included.
fn split_lines(text: &str) -> impl Iterator<Item = &str> {
    text.split('\n').map(|line| line.strip_suffix('\r').unwrap_or(line))
}

fn is_binary(patch: &str) -> bool {
    split_lines(patch).any(|line| line.starts_with("Binary files ") || line == "GIT binary patch")
}

fn count_changes(patch: &str) -> (u32, u32) {
    let mut additions = 0;
    let mut deletions = 0;
    for line in split_lines(patch) {
        // The file headers are not content lines even though they start with +/-.
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if line.starts_with('+') {
            additions += 1;
        } else if line.starts_with('-') {
            deletions += 1;
        }
    }
    (additions, deletions)
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

async fn worktree_file(workspace: &Workspace, file_path: &str) -> Result<String, AppError> {
    let repo_path = workspace.current_path()?;
    let bytes = tokio::fs::read(repo_path.join(file_path))
        .await
        .map_err(|error| AppError::WorkTree(format!("failed to read {file_path}: {error}")))?;
    // Lossy by design, matching how the file is read everywhere else.
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

async fn file_patch(workspace: &Workspace, side: &Side) -> Result<String, AppError> {
    match (side.status, side.staged, side.old_path.as_deref()) {
        (ChangeStatus::Untracked, _, _) => {
            git::run(
                workspace,
                &["diff", "--no-index", "--", "/dev/null", &side.path],
            )
            .await
        }
        (_, true, None) => git::run(workspace, &["diff", "--cached", "--", &side.path]).await,
        (_, true, Some(old_path)) => {
            git::run(
                workspace,
                &["diff", "--cached", "-M", "--", old_path, &side.path],
            )
            .await
        }
        (_, false, _) => git::run(workspace, &["diff", "--", &side.path]).await,
    }
}

fn to_file_change(side: &Side, patch: &str, error: Option<String>) -> FileChange {
    let binary = is_binary(patch);
    let (additions, deletions) = if binary { (0, 0) } else { count_changes(patch) };

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
        additions,
        deletions,
        binary,
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

/// One row of the startup diff.
///
/// A named `async fn` rather than an inline async block: a closure returning a
/// block that borrows its argument cannot satisfy the higher-ranked bound
/// `buffered` needs, while an `async fn` call elides the lifetimes cleanly.
async fn diff_row(workspace: &Workspace, side: &Side) -> Option<FileChange> {
    match file_patch(workspace, side).await {
        Ok(patch) if patch.trim().is_empty() => None,
        Ok(patch) => Some(to_file_change(side, &patch, None)),
        // One unreadable path shouldn't blank the whole view — surface it on
        // its own row instead.
        Err(error) => Some(to_file_change(side, "", Some(error.message().to_owned()))),
    }
}

/// The patch behind one already-collected row, paired back up with it.
async fn patch_for<'a>(
    workspace: &Workspace,
    file: &'a FileChange,
) -> Result<(&'a FileChange, String), AppError> {
    let side = Side {
        path: file.path.clone(),
        old_path: file.old_path.clone(),
        status: file.status,
        staged: file.staged,
    };
    file_patch(workspace, &side).await.map(|patch| (file, patch))
}

/// Metadata for every change in the repo, untracked files included. Staged and
/// unstaged edits to the same path are returned as separate rows, because they
/// are genuinely two different patches.
pub async fn get_diff(workspace: &Workspace) -> Result<RepoDiff, AppError> {
    let repo_path = workspace.current_path()?;

    let (branch, head, status) = tokio::try_join!(
        git::run_in(&repo_path, &["rev-parse", "--abbrev-ref", "HEAD"]),
        git::run_in(&repo_path, &["log", "-1", "--pretty=%h %s"]),
        // -uall matters: without it an untracked *directory* collapses into a
        // single "?? dir/" record, and diffing a directory is not a thing.
        git::run_in(&repo_path, &["status", "--porcelain", "-z", "-uall"]),
    )?;

    let sides = parse_porcelain(&status);

    // The futures are built up front rather than by a closure inside
    // `stream::iter`: a closure mapping a reference to a future it borrows from
    // cannot satisfy the higher-ranked bound, and a plain `Vec` sidesteps it.
    let mut pending = Vec::with_capacity(sides.len());
    for side in &sides {
        pending.push(diff_row(workspace, side));
    }

    let mut files: Vec<FileChange> = stream::iter(pending)
        .buffered(PATCH_CONCURRENCY)
        .filter_map(future::ready)
        .collect()
        .await;

    files.sort_by(|a, b| a.path.cmp(&b.path).then(b.staged.cmp(&a.staged)));

    Ok(RepoDiff {
        repo_path: repo_path.to_string_lossy().into_owned(),
        branch: branch.trim().to_owned(),
        head: head.trim().to_owned(),
        files,
    })
}

/// Loads complete file versions. Kept separate from `get_diff` so startup never
/// reads every changed file — cards ask for this as they approach the viewport.
pub async fn get_file_contents(
    workspace: &Workspace,
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
            new_contents: Some(worktree_file(workspace, file_path).await?),
        });
    }

    if staged {
        let old_contents = if status == ChangeStatus::Added {
            None
        } else {
            Some(git::show_file(workspace, "HEAD", old_path.unwrap_or(file_path)).await?)
        };
        let new_contents = if status == ChangeStatus::Deleted {
            None
        } else {
            Some(git::show_file(workspace, "", file_path).await?)
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
        Some(git::show_file(workspace, "", source).await?)
    };
    let new_contents = if status == ChangeStatus::Deleted {
        None
    } else {
        Some(worktree_file(workspace, file_path).await?)
    };

    Ok(FullFileContents {
        old_contents,
        new_contents,
    })
}

/// Stages the current file, including the previous path when the change is a
/// rename so Git records both halves of the move.
pub async fn stage_file(
    workspace: &Workspace,
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
    git::run(workspace, &args).await?;
    Ok(())
}

/// Stages everything and commits it in one step.
///
/// `git add -A` then `git commit` rather than `commit -a`, because `-a` ignores
/// untracked files and this is the "commit all" the user means. The message
/// goes through `-m` as a single argument, so nothing in it is interpreted as a
/// flag or reaches a shell.
pub async fn commit_all(workspace: &Workspace, message: &str) -> Result<String, AppError> {
    let subject = message.trim();
    if subject.is_empty() {
        return Err(AppError::InvalidPath("A commit needs a message.".into()));
    }

    let status = git::run(workspace, &["status", "--porcelain"]).await?;
    if status.trim().is_empty() {
        return Err(AppError::Git(
            "Nothing to commit — the working tree is clean.".into(),
        ));
    }

    git::run(workspace, &["add", "-A"]).await?;
    git::run(workspace, &["commit", "-m", subject]).await?;
    let head = git::run(workspace, &["log", "-1", "--pretty=%h %s"]).await?;
    Ok(head.trim().to_owned())
}

/// The complete staged, unstaged, and untracked diff as one annotated document
/// — what the commit-message model is shown.
pub async fn commit_message_diff(workspace: &Workspace) -> Result<String, AppError> {
    let clean = || AppError::Git("Working tree is clean; there is no diff to summarize.".into());

    let repo_diff = get_diff(workspace).await?;
    if repo_diff.files.is_empty() {
        return Err(clean());
    }

    let mut pending = Vec::with_capacity(repo_diff.files.len());
    for file in &repo_diff.files {
        pending.push(patch_for(workspace, file));
    }

    let patches: Vec<(&FileChange, String)> = stream::iter(pending)
        .buffered(PATCH_CONCURRENCY)
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
