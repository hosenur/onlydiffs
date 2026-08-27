//! Every file in the repository, as flat repo-relative paths.
//!
//! One `git ls-files` covers the whole tree: `-c` tracked, `-o` untracked,
//! `--exclude-standard` to honour `.gitignore`, `-z` because a path may contain
//! a newline. That is a single process for the entire repository — walking the
//! filesystem instead would mean reimplementing `.gitignore` and being slower
//! for it.
//!
//! Deliberately uncached. It costs ~10ms on a small repository, the renderer
//! asks for it once per load, and a cache would go stale the moment a file was
//! created.

use crate::error::AppError;
use crate::services::git;
use crate::services::workspace::Workspace;

pub async fn list_files(workspace: &Workspace) -> Result<Vec<String>, AppError> {
    let output = git::run(workspace, &["ls-files", "-co", "--exclude-standard", "-z"]).await?;
    Ok(output
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect())
}
