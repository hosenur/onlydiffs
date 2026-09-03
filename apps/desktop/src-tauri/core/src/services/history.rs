//! Commit history for the open repository.

use crate::contract::Commit;
use crate::error::AppError;
use crate::services::git;
use crate::services::workspace::Workspace;

const DEFAULT_LIMIT: i64 = 100;
const MAX_LIMIT: i64 = 5000;

/// `\x1f` between fields, `\x1e` between records: neither can appear in a
/// subject or a ref name, unlike newlines and tabs.
const PRETTY_FORMAT: &str =
    "--pretty=format:%H%x1f%h%x1f%an%x1f%ae%x1f%ar%x1f%aI%x1f%P%x1f%D%x1f%s%x1e";

/// The limit arrives from the renderer and is interpolated into an argument, so
/// it is normalised to a plain integer before it gets near the command line.
fn normalize_limit(limit: Option<f64>) -> i64 {
    match limit {
        Some(value) if value.is_finite() => (value.trunc() as i64).clamp(1, MAX_LIMIT),
        _ => DEFAULT_LIMIT,
    }
}

/// Commit history reachable from HEAD, newest first.
pub async fn get_history(
    workspace: &Workspace,
    limit: Option<f64>,
) -> Result<Vec<Commit>, AppError> {
    let count = normalize_limit(limit);
    let log = git::run(
        workspace,
        &["log", &format!("--max-count={count}"), PRETTY_FORMAT],
    )
    .await?;

    let mut commits = Vec::new();
    for raw_record in log.split('\u{1e}') {
        let record = raw_record.trim_start_matches('\n');
        if record.trim().is_empty() {
            continue;
        }

        let fields: Vec<&str> = record.split('\u{1f}').collect();
        if fields.len() < 9 {
            continue;
        }

        commits.push(Commit {
            hash: fields[0].to_owned(),
            short_hash: fields[1].to_owned(),
            author: fields[2].to_owned(),
            author_email: fields[3].to_owned(),
            relative_date: fields[4].to_owned(),
            date: fields[5].to_owned(),
            is_merge: fields[6].split_whitespace().count() > 1,
            refs: fields[7].to_owned(),
            subject: fields[8].to_owned(),
        });
    }

    Ok(commits)
}
