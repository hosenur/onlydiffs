//! Finding a repository's own artwork and shrinking it to a shortlist.
//!
//! This runs where the repository is. It reads up to thirty-two images, decodes
//! them, and returns at most three 256px PNG thumbnails — which is what makes
//! it worth doing on the far side of a connection rather than shipping the
//! source files across one.
//!
//! Choosing between the candidates is not here. That call needs a vision model
//! and an API key, and the key belongs on the user's own machine.

use std::cmp::Reverse;
use std::io::Cursor;
use std::path::{Component, Path, PathBuf};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use image::ImageOutputFormat;
use serde::{Deserialize, Serialize};

use crate::services::repository::Repository;

/// Groq's vision models reject a request carrying more than three images. The
/// scan lives here and the model call does not, but the limit belongs with the
/// thing that decides how many candidates to build — sending four back over a
/// wire only to drop one is a wasted thumbnail.
pub const MAX_CANDIDATES: usize = 3;
const MAX_RANKED_PATHS: usize = 32;
const MAX_SOURCE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_SVG_BYTES: u64 = 512 * 1024;
const MAX_RASTER_DIMENSION: u32 = 4096;
const PREVIEW_SIZE: u32 = 256;

/// One piece of repository artwork, already shrunk to something a vision model
/// can be shown.
///
/// The thumbnail is built where the repository is. That is the whole reason
/// this type crosses a wire rather than the files it came from: scanning may
/// read thirty-two images at four megabytes each, and what comes back is three
/// PNGs of about twenty kilobytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: String,
    pub relative_path: String,
    pub score: i32,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub data_url: String,
    pub byte_len: u64,
    pub modified_millis: u128,
    /// Distinguishes artwork committed twice over from a genuine alternative.
    pub content_hash: u64,
}

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

fn fnv(seed: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(seed, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
    })
}

fn is_safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn image_extension(path: &Path) -> Option<&str> {
    let extension = path.extension()?.to_str()?;
    match extension.to_ascii_lowercase().as_str() {
        "gif" => Some("gif"),
        "ico" => Some("ico"),
        "jpeg" | "jpg" => Some("jpeg"),
        "png" => Some("png"),
        "svg" => Some("svg"),
        "webp" => Some("webp"),
        _ => None,
    }
}

fn name_score(path: &Path) -> Option<i32> {
    image_extension(path)?;
    let stem = path.file_stem()?.to_string_lossy().to_ascii_lowercase();
    let normalized = stem.replace(['_', ' '], "-");

    let mut score = match normalized.as_str() {
        "app-icon" | "application-icon" | "icon" => 150,
        "logo-mark" | "logomark" | "mark" => 145,
        "logo" => 140,
        "favicon" => 130,
        _ if normalized.contains("app-icon") => 115,
        _ if normalized.contains("logo") => 105,
        _ if normalized.contains("icon") => 95,
        _ if normalized.contains("favicon") => 90,
        _ if normalized.contains("brand") || normalized.contains("mark") => 80,
        // Arbitrarily named mascots and product marks are still worth showing
        // the model when a repository has no conventional logo filename.
        _ => 10,
    };

    let lower_path = path.to_string_lossy().to_ascii_lowercase();
    if [
        "banner",
        "cover",
        "hero",
        "og-image",
        "open-graph",
        "screenshot",
        "social",
        "splash",
        "wordmark",
    ]
    .iter()
    .any(|term| lower_path.contains(term))
    {
        score -= 100;
    }
    if lower_path.contains("node_modules") || lower_path.contains("fixture") {
        score -= 100;
    }
    if lower_path.contains("file-icons") || lower_path.contains("test-data") {
        score -= 80;
    }
    if lower_path.contains("src-tauri/icons") || lower_path.contains("appicon") {
        score += 25;
    }

    let depth = path.components().count().saturating_sub(1) as i32;
    score -= depth.min(12) * 2;
    Some(score)
}

/// Read from the bytes rather than the path: the scanner has them in hand, and
/// a second open would be a second round trip on a repository that is not here.
fn dimensions(bytes: &[u8], extension: &str) -> (Option<u32>, Option<u32>) {
    if extension == "svg" {
        return (None, None);
    }
    match image::io::Reader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()
        .and_then(|reader| reader.into_dimensions().ok())
    {
        Some((width, height)) => (Some(width), Some(height)),
        None => (None, None),
    }
}

fn raster_data_url(bytes: &[u8], width: Option<u32>, height: Option<u32>) -> Option<String> {
    if width.is_some_and(|value| value > MAX_RASTER_DIMENSION)
        || height.is_some_and(|value| value > MAX_RASTER_DIMENSION)
    {
        return None;
    }
    let image = image::load_from_memory(bytes).ok()?;
    let thumbnail = image.thumbnail(PREVIEW_SIZE, PREVIEW_SIZE);
    let mut encoded = Cursor::new(Vec::new());
    thumbnail
        .write_to(&mut encoded, ImageOutputFormat::Png)
        .ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        BASE64.encode(encoded.into_inner())
    ))
}

fn svg_png_data_url(bytes: &[u8]) -> Option<String> {
    if bytes.len() as u64 > MAX_SVG_BYTES {
        return None;
    }
    // No resource directory and no raster-image feature means an SVG cannot
    // read sibling files or remote URLs while it is being rasterized.
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default()).ok()?;
    let size = tree.size();
    let scale = (PREVIEW_SIZE as f32 / size.width()).min(PREVIEW_SIZE as f32 / size.height());
    let width = (size.width() * scale).round().max(1.0) as u32;
    let height = (size.height() * scale).round().max(1.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let png = pixmap.encode_png().ok()?;
    Some(format!("data:image/png;base64,{}", BASE64.encode(png)))
}

async fn candidate(
    repo: &Repository,
    relative_path: &Path,
    base_score: i32,
) -> Option<Candidate> {
    if !is_safe_relative(relative_path) {
        return None;
    }
    let extension = image_extension(relative_path)?;
    let metadata = repo.metadata(relative_path).await.ok()??;
    if !metadata.is_file || metadata.len > MAX_SOURCE_BYTES {
        return None;
    }

    // The bytes come first now: dimensions are read from them rather than from
    // the path, because on another machine there is no path to open twice.
    let bytes = repo.read_file(relative_path, MAX_SOURCE_BYTES).await.ok()?;
    let (width, height) = dimensions(&bytes, extension);
    let mut score = base_score;
    if let (Some(width), Some(height)) = (width, height) {
        let ratio = width.min(height) as f32 / width.max(height) as f32;
        if ratio >= 0.9 {
            score += 35;
        } else if ratio >= 0.7 {
            score += 15;
        } else if ratio < 0.4 {
            score -= 40;
        }
        if width < 32 || height < 32 {
            score -= 30;
        }
    }

    let data_url = if extension == "svg" {
        svg_png_data_url(&bytes)?
    } else {
        raster_data_url(&bytes, width, height)?
    };

    Some(Candidate {
        id: String::new(),
        relative_path: relative_path.to_string_lossy().into_owned(),
        score,
        width,
        height,
        data_url,
        byte_len: metadata.len,
        modified_millis: metadata.modified_millis,
        content_hash: fnv(FNV_OFFSET, &bytes),
    })
}

pub async fn discover(repo: &Repository) -> Result<Vec<Candidate>, String> {
    let output = repo
        .git(&["ls-files", "-co", "--exclude-standard", "-z"])
        .await
        .map_err(|error| error.message().to_owned())?;

    let mut ranked_paths: Vec<(i32, PathBuf)> = output
        .split('\0')
        .filter(|path| !path.is_empty())
        .filter_map(|path| {
            let relative = PathBuf::from(path);
            name_score(&relative).map(|score| (score, relative))
        })
        .collect();
    ranked_paths.sort_by_key(|(score, path)| {
        (Reverse(*score), path.to_string_lossy().to_ascii_lowercase())
    });
    ranked_paths.truncate(MAX_RANKED_PATHS);

    // Projects routinely commit one mark several times over — an .ico beside
    // its .png, a web copy beside the bundled one. With three image slots per
    // request, a byte-identical repeat would cost a genuine alternative.
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut seen: Vec<u64> = Vec::new();
    for (score, path) in ranked_paths {
        if candidates.len() == MAX_CANDIDATES {
            break;
        }
        let Some(candidate) = candidate(repo, &path, score).await else {
            continue;
        };
        if seen.contains(&candidate.content_hash) {
            continue;
        }
        seen.push(candidate.content_hash);
        candidates.push(candidate);
    }
    candidates.sort_by_key(|candidate| {
        (
            Reverse(candidate.score),
            candidate.relative_path.to_ascii_lowercase(),
        )
    });
    for (index, candidate) in candidates.iter_mut().enumerate() {
        candidate.id = char::from(b'A' + index as u8).to_string();
    }
    Ok(candidates)
}

pub fn scan_hash(candidates: &[Candidate]) -> String {
    let mut hash = FNV_OFFSET;
    for candidate in candidates {
        hash = fnv(hash, candidate.relative_path.as_bytes());
        hash = fnv(hash, &candidate.byte_len.to_le_bytes());
        hash = fnv(hash, &candidate.modified_millis.to_le_bytes());
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::{image_extension, is_safe_relative, name_score, scan_hash, svg_png_data_url, Candidate};
    use std::path::Path;

    fn candidate(id: &str, content_hash: u64) -> Candidate {
        Candidate {
            id: id.to_owned(),
            relative_path: "icon.png".into(),
            score: 100,
            width: Some(64),
            height: Some(64),
            data_url: String::new(),
            byte_len: 1,
            modified_millis: 1,
            content_hash,
        }
    }

    #[test]
    fn project_artwork_names_are_ranked_but_screenshots_are_penalized() {
        let icon = name_score(Path::new("src-tauri/icons/icon.png")).expect("icon candidate");
        let logo = name_score(Path::new("public/logo.svg")).expect("logo candidate");
        let screenshot =
            name_score(Path::new("docs/logo-screenshot.png")).expect("screenshot candidate");

        assert!(icon > logo);
        let arbitrary = name_score(Path::new("assets/platypus.png")).expect("image candidate");

        assert!(logo > screenshot);
        assert!(screenshot < arbitrary);
    }

    #[test]
    fn only_supported_image_extensions_are_candidates() {
        assert_eq!(image_extension(Path::new("favicon.ico")), Some("ico"));
        assert_eq!(image_extension(Path::new("mark.webp")), Some("webp"));
        assert_eq!(image_extension(Path::new("logo.svg")), Some("svg"));
        assert_eq!(image_extension(Path::new("logo.pdf")), None);
    }

    #[test]
    fn svg_candidates_are_rasterized_for_the_vision_model() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="10" height="10" fill="red"/></svg>"#;
        let data_url = svg_png_data_url(svg).expect("rasterized svg");
        assert!(data_url.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn candidate_paths_cannot_escape_the_repository() {
        assert!(is_safe_relative(Path::new("assets/icon.png")));
        assert!(!is_safe_relative(Path::new("../icon.png")));
        assert!(!is_safe_relative(Path::new("/tmp/icon.png")));
    }

    #[test]
    fn scan_hash_changes_with_candidate_metadata() {
        let make = |len| Candidate {
            byte_len: len,
            ..candidate("A", 7)
        };

        assert_ne!(scan_hash(&[make(1)]), scan_hash(&[make(2)]));
    }

    #[test]
    fn a_repository_with_no_artwork_keeps_a_stable_scan_hash() {
        assert_eq!(scan_hash(&[]), scan_hash(&[]));
    }
}

#[cfg(test)]
mod discovery {
    use super::{discover, Repository, MAX_CANDIDATES};
    use std::path::Path;
    use std::process::Command;

    pub(super) fn git(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .expect("git runs");
        assert!(output.status.success(), "git {args:?} failed");
    }

    /// A flat colour, so two files written with the same shade encode to the
    /// same bytes and stand in for artwork committed twice over.
    pub(super) fn write_png(repo: &Path, relative_path: &str, shade: u8) {
        let absolute = repo.join(relative_path);
        std::fs::create_dir_all(absolute.parent().expect("parent")).expect("create dir");
        image::RgbImage::from_pixel(64, 64, image::Rgb([shade, shade, shade]))
            .save(&absolute)
            .expect("write png");
    }

    #[tokio::test]
    async fn repeated_artwork_never_takes_a_second_image_slot() {
        let repo = tempfile::TempDir::new().expect("temp repo");
        git(repo.path(), &["init", "-q"]);
        // One mark under three conventional names, and two other images that
        // rank below all of them.
        write_png(repo.path(), "icon.png", 10);
        write_png(repo.path(), "build/app-icon.png", 10);
        write_png(repo.path(), "public/logo.png", 10);
        write_png(repo.path(), "assets/favicon.png", 90);
        write_png(repo.path(), "assets/brand.png", 180);

        let candidates = discover(&Repository::local(repo.path().to_path_buf())).await.expect("discover");
        let paths: Vec<&str> = candidates
            .iter()
            .map(|candidate| candidate.relative_path.as_str())
            .collect();

        assert_eq!(candidates.len(), MAX_CANDIDATES);
        // The duplicates would otherwise have crowded out both alternatives.
        assert_eq!(paths, ["icon.png", "assets/favicon.png", "assets/brand.png"]);
    }

    #[tokio::test]
    async fn a_repository_with_no_artwork_yields_no_candidates() {
        let repo = tempfile::TempDir::new().expect("temp repo");
        git(repo.path(), &["init", "-q"]);
        std::fs::write(repo.path().join("README.md"), "# no artwork").expect("write readme");

        assert!(discover(&Repository::local(repo.path().to_path_buf())).await.expect("discover").is_empty());
    }
}
