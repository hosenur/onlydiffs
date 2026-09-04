//! Images pasted into the composer, on their way to a Claude session.
//!
//! The bytes never travel in the message. The channel carries 64 KB of text and
//! a screenshot is megabytes of binary, so the image is written down where the
//! session can open it and the message names the path instead.
//!
//! That is also the only form the remote case can take. A path means something
//! on exactly one machine, and it has to be the machine the session is on —
//! which is why this lives in `core` rather than in the app. The agent calls
//! this function for a repository on a host, so the path handed back is always
//! one the session on the other end can open.
//!
//! They land in the repository's git directory, which is the only place that
//! answers both halves of the problem. The working tree is out: this is an app
//! for reading what changed, and a pasted screenshot appearing as an untracked
//! file would be noise produced by the feature meant to explain it. The home
//! directory is out too — it sits outside the session's working directory,
//! which costs a permission prompt on every paste. `.git` is inside the
//! checkout, writable by definition, and invisible to `git status`.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use image::ImageFormat;
use tokio::io::AsyncWriteExt;

use crate::error::AppError;
use crate::services::repository::Repository;

/// The largest image that will be written down.
///
/// Well above a retina screenshot, which is what nearly every paste is, and far
/// enough below the protocol's frame limit that the bytes crossing to a host
/// are never the thing that breaks. The renderer knows this number too, so an
/// oversized paste is refused before it is copied rather than after.
pub const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;

/// Where the images go, relative to the repository's git directory.
const DIRECTORY: &str = "onlydiffs/pastes";

/// How long an image is kept.
///
/// It has to outlive the message that named it: a session scrolls back, and a
/// path that no longer resolves is worse than the few kilobytes it saved. But
/// nothing else ever deletes one — the composer writes a file per paste whether
/// or not the message is sent — so something has to.
const KEEP_FOR: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// How many names to try before giving up on finding a free one.
const NAME_ATTEMPTS: u32 = 64;

/// Writes a pasted image where the Claude session for `repo` can open it, and
/// answers with its absolute path *on this machine*.
pub async fn write(repo: &Repository, bytes: &[u8]) -> Result<String, AppError> {
    if bytes.is_empty() {
        return Err(AppError::Attachment("The pasted image is empty.".into()));
    }
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(AppError::Attachment(format!(
            "The image is {} and the limit is {}.",
            megabytes(bytes.len()),
            megabytes(MAX_IMAGE_BYTES)
        )));
    }

    let extension = image_extension(bytes)?;
    let directory = pastes_dir(repo).await?;
    tokio::fs::create_dir_all(&directory).await.map_err(|error| {
        AppError::Attachment(format!(
            "failed to create {}: {error}",
            directory.display()
        ))
    })?;
    restrict(&directory).await;
    prune_older_than(&directory, KEEP_FOR).await;

    let path = store(&directory, extension, bytes).await?;
    Ok(path.to_string_lossy().into_owned())
}

/// What the bytes actually are, rather than what the renderer said they were.
///
/// A `DataTransfer` item's type is whatever the application it came from
/// claimed, and it would decide the extension the session opens the file by —
/// so the format is read from the bytes here instead.
///
/// The header is parsed as well as sniffed. A truncated screenshot has a
/// perfectly good magic number and nothing behind it, and finding that out at
/// the paste is a sentence about the paste; finding it out later is a puzzle in
/// the middle of a conversation.
fn image_extension(bytes: &[u8]) -> Result<&'static str, AppError> {
    let format = image::guess_format(bytes)
        .map_err(|_| AppError::Attachment("That is not an image.".into()))?;

    let Some(extension) = extension_for(format) else {
        return Err(AppError::Attachment(format!(
            "A {format:?} image cannot be pasted into a Claude session. PNG, JPEG, GIF, and WebP can."
        )));
    };

    image::io::Reader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()
        .and_then(|reader| reader.into_dimensions().ok())
        .ok_or_else(|| {
            AppError::Attachment("The image could not be read; it may be incomplete.".into())
        })?;

    Ok(extension)
}

/// The formats a Claude Code session can open, and the extension each is
/// written under. Anything else is refused rather than saved as a file the
/// session will fail to read.
fn extension_for(format: ImageFormat) -> Option<&'static str> {
    match format {
        ImageFormat::Png => Some("png"),
        ImageFormat::Jpeg => Some("jpg"),
        ImageFormat::Gif => Some("gif"),
        ImageFormat::WebP => Some("webp"),
        _ => None,
    }
}

/// The pastes directory for this repository.
///
/// `git rev-parse --git-dir` rather than `root/.git`, because in a linked
/// worktree that is a *file* pointing somewhere else and git is the only thing
/// that knows where. The answer is relative for an ordinary checkout and
/// absolute for a worktree; `join` is right either way.
async fn pastes_dir(repo: &Repository) -> Result<PathBuf, AppError> {
    let answer = repo.git(&["rev-parse", "--git-dir"]).await?;
    let git_dir = answer.trim();
    if git_dir.is_empty() {
        return Err(AppError::Attachment(
            "git did not say where this repository keeps its own files.".into(),
        ));
    }
    Ok(repo.root().join(git_dir).join(DIRECTORY))
}

/// Writes the bytes under a name nothing else holds.
///
/// `create_new` rather than looking first and writing after: two windows on one
/// repository, or an app and an agent sharing a host, can arrive in the same
/// millisecond, and the loser of that race should be given the next name rather
/// than somebody else's file.
async fn store(directory: &Path, extension: &str, bytes: &[u8]) -> Result<PathBuf, AppError> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis())
        .unwrap_or(0);

    for attempt in 0..NAME_ATTEMPTS {
        let path = directory.join(match attempt {
            0 => format!("{stamp}.{extension}"),
            n => format!("{stamp}-{n}.{extension}"),
        });
        let mut file = match tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .await
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(AppError::Attachment(format!(
                    "failed to write {}: {error}",
                    path.display()
                )))
            }
        };
        file.write_all(bytes).await.map_err(|error| {
            AppError::Attachment(format!("failed to write {}: {error}", path.display()))
        })?;
        file.flush().await.map_err(|error| {
            AppError::Attachment(format!("failed to write {}: {error}", path.display()))
        })?;
        return Ok(path);
    }

    Err(AppError::Attachment(
        "could not find a free name for the pasted image.".into(),
    ))
}

/// Drops images nothing can still be talking about.
///
/// Best-effort throughout: a paste is not the place to report that a week-old
/// file could not be unlinked.
async fn prune_older_than(directory: &Path, keep: Duration) {
    let Ok(mut entries) = tokio::fs::read_dir(directory).await else {
        return;
    };
    let now = SystemTime::now();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(meta) = entry.metadata().await else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let expired = meta
            .modified()
            .ok()
            .and_then(|at| now.duration_since(at).ok())
            .is_some_and(|age| age > keep);
        if expired {
            let _ = tokio::fs::remove_file(entry.path()).await;
        }
    }
}

/// A screenshot of a private repository is as sensitive as the repository, and
/// `.git` itself is world-readable on most machines.
#[cfg(unix)]
async fn restrict(directory: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = tokio::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700)).await;
}

#[cfg(not(unix))]
async fn restrict(_directory: &Path) {}

fn megabytes(bytes: usize) -> String {
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageOutputFormat;

    fn repository() -> (tempfile::TempDir, Repository) {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["init", "-q"])
            .status()
            .expect("git runs");
        assert!(status.success(), "git init failed");
        let repo = Repository::local(dir.path().to_path_buf());
        (dir, repo)
    }

    fn encoded(format: ImageOutputFormat) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        image::RgbImage::from_pixel(4, 4, image::Rgb([10, 20, 30]))
            .write_to(&mut out, format)
            .expect("encode");
        out.into_inner()
    }

    #[tokio::test]
    async fn a_pasted_image_lands_inside_the_git_directory_rather_than_the_working_tree() {
        let (dir, repo) = repository();
        let png = encoded(ImageOutputFormat::Png);

        let path = write(&repo, &png).await.expect("write");

        let written = PathBuf::from(&path);
        assert!(written.starts_with(dir.path().join(".git")), "{path}");
        assert_eq!(written.extension().and_then(|e| e.to_str()), Some("png"));
        assert_eq!(std::fs::read(&written).expect("read back"), png);
        // The whole reason for the location: it is not a change to review.
        let status = repo.git(&["status", "--porcelain"]).await.expect("status");
        assert_eq!(status.trim(), "", "a paste is not a change to the repository");
    }

    #[tokio::test]
    async fn the_extension_comes_from_the_bytes_rather_than_from_what_was_claimed() {
        let (_dir, repo) = repository();

        let path = write(&repo, &encoded(ImageOutputFormat::Jpeg(90)))
            .await
            .expect("write");

        assert!(path.ends_with(".jpg"), "{path}");
    }

    #[tokio::test]
    async fn two_pastes_in_the_same_millisecond_get_their_own_files() {
        let (_dir, repo) = repository();
        let png = encoded(ImageOutputFormat::Png);

        let first = write(&repo, &png).await.expect("first");
        let second = write(&repo, &png).await.expect("second");

        assert_ne!(first, second);
        assert!(PathBuf::from(&first).is_file());
        assert!(PathBuf::from(&second).is_file());
    }

    #[tokio::test]
    async fn something_that_is_not_an_image_is_refused() {
        let (_dir, repo) = repository();

        let refused = write(&repo, b"just some text, pasted").await;

        assert_eq!(refused.expect_err("refused").tag(), "AttachmentError");
    }

    #[tokio::test]
    async fn a_header_with_nothing_behind_it_is_refused_here_rather_than_in_the_session() {
        let (_dir, repo) = repository();
        let png = encoded(ImageOutputFormat::Png);

        // Enough for `guess_format` to say PNG, not enough to be one.
        let refused = write(&repo, &png[..8]).await;

        assert!(refused.is_err(), "a truncated image should not be written");
    }

    #[tokio::test]
    async fn an_image_past_the_limit_is_refused_before_it_is_written() {
        let (_dir, repo) = repository();
        let mut oversized = encoded(ImageOutputFormat::Png);
        oversized.resize(MAX_IMAGE_BYTES + 1, 0);

        let refused = write(&repo, &oversized).await;

        assert!(refused.expect_err("refused").message().contains("limit"));
    }

    #[tokio::test]
    async fn an_empty_paste_is_refused() {
        let (_dir, repo) = repository();

        assert!(write(&repo, &[]).await.is_err());
    }

    #[tokio::test]
    async fn old_images_are_dropped_and_fresh_ones_are_kept() {
        let (_dir, repo) = repository();
        let path = PathBuf::from(write(&repo, &encoded(ImageOutputFormat::Png)).await.expect("write"));
        let directory = path.parent().expect("parent").to_path_buf();

        prune_older_than(&directory, KEEP_FOR).await;
        assert!(path.is_file(), "an image written a moment ago is still wanted");

        // Anything with an age at all is past a zero-length window.
        prune_older_than(&directory, Duration::ZERO).await;
        assert!(!path.exists(), "an expired image should have been dropped");
    }
}
