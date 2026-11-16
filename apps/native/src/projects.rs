//! The projects the app knows: the same `~/.onlydiffs/projects.json` the
//! Tauri app keeps, so both list the same repositories.
//!
//! The file is edited in place as JSON, keeping every field this build does
//! not use — remote hosts, icon scan hashes — so the other app still reads
//! it whole. Entries stay in first-opened order, so opening one never moves
//! the rail; `lastOpenedAt` only picks what to restore at launch.
//!
//! Local projects only for now: entries on other machines are kept in the
//! file and left off the rail.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use serde_json::{Value, json};

const STORE_FILE: &str = "projects.json";

/// One repository on the rail.
#[derive(Clone)]
pub struct Project {
    /// The repository root, absolute.
    pub path: String,
    /// The folder's name.
    pub name: String,
    /// The project's own artwork, when the other app found some.
    pub icon: Option<Arc<gpui::Image>>,
}

pub struct Store {
    file: PathBuf,
    /// The whole file, so fields this build ignores survive a write.
    root: Value,
}

fn state_dir() -> PathBuf {
    std::env::var_os("ONLYDIFFS_STATE_DIR").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".onlydiffs")
    })
}

fn now_millis() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

impl Store {
    pub fn load() -> Self {
        Self::at(state_dir().join(STORE_FILE))
    }

    fn at(file: PathBuf) -> Self {
        let root = std::fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .filter(|value| value["projects"].is_array())
            .unwrap_or_else(|| json!({ "version": 1, "projects": [] }));
        Self { file, root }
    }

    fn entries(&self) -> &Vec<Value> {
        self.root["projects"].as_array().expect("checked on load")
    }

    fn entries_mut(&mut self) -> &mut Vec<Value> {
        self.root["projects"].as_array_mut().expect("checked on load")
    }

    fn is_local(entry: &Value) -> bool {
        entry["host"].is_null()
    }

    /// Local projects whose folder is still there, in first-opened order.
    pub fn local(&self) -> Vec<Project> {
        self.entries()
            .iter()
            .filter(|entry| Self::is_local(entry))
            .filter_map(|entry| {
                let path = entry["path"].as_str()?;
                Path::new(path).is_dir().then(|| Project {
                    path: path.to_owned(),
                    name: name_of(path),
                    icon: entry["icon"]["dataUrl"].as_str().and_then(decode_icon),
                })
            })
            .collect()
    }

    /// The local project opened last, to restore at launch.
    pub fn most_recent(&self) -> Option<String> {
        self.entries()
            .iter()
            .filter(|entry| Self::is_local(entry))
            .filter(|entry| entry["path"].as_str().is_some_and(|path| Path::new(path).is_dir()))
            .max_by_key(|entry| entry["lastOpenedAt"].as_u64().unwrap_or(0))
            .and_then(|entry| entry["path"].as_str().map(str::to_owned))
    }

    /// Records that `root` was opened: newest for the next launch, and on
    /// the rail if it was not already.
    pub fn touch(&mut self, root: &str) {
        // Strictly increasing, even for two opens in the same millisecond.
        let latest = self.entries().iter().filter_map(|e| e["lastOpenedAt"].as_u64()).max().unwrap_or(0);
        let opened_at = now_millis().max(latest + 1);
        let existing = self
            .entries_mut()
            .iter_mut()
            .find(|entry| Self::is_local(entry) && entry["path"].as_str() == Some(root));
        match existing {
            Some(entry) => entry["lastOpenedAt"] = json!(opened_at),
            None => self.entries_mut().push(json!({ "path": root, "lastOpenedAt": opened_at })),
        }
        self.save();
    }

    /// Takes `root` off the list. The repository itself is untouched.
    pub fn forget(&mut self, root: &str) {
        self.entries_mut().retain(|entry| !(Self::is_local(entry) && entry["path"].as_str() == Some(root)));
        self.save();
    }

    fn save(&self) {
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        let Ok(body) = serde_json::to_vec_pretty(&self.root) else { return };
        let partial = self.file.with_extension("json.partial");
        if std::fs::write(&partial, body).is_ok() {
            std::fs::rename(partial, &self.file).ok();
        }
    }
}

/// The last path segment: what the rail and the window title call it.
pub fn name_of(path: &str) -> String {
    path.trim_end_matches('/').rsplit('/').next().filter(|s| !s.is_empty()).unwrap_or(path).to_owned()
}

/// `data:image/png;base64,…` → an image GPUI can draw.
fn decode_icon(data_url: &str) -> Option<Arc<gpui::Image>> {
    let (head, body) = data_url.split_once(',')?;
    let format = if head.contains("image/png") {
        gpui::ImageFormat::Png
    } else if head.contains("image/jpeg") {
        gpui::ImageFormat::Jpeg
    } else if head.contains("image/webp") {
        gpui::ImageFormat::Webp
    } else {
        return None;
    };
    let bytes = base64::engine::general_purpose::STANDARD.decode(body).ok()?;
    Some(Arc::new(gpui::Image::from_bytes(format, bytes)))
}

/// What a typed path means: `~/x`, an absolute path, or a path relative to
/// the home folder; any folder inside a checkout opens the whole repository.
pub fn resolve(input: &str) -> Result<String, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("Enter a path to a git repository.".into());
    }
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let path = match input.strip_prefix("~") {
        Some(rest) => home.join(rest.trim_start_matches('/')),
        None if Path::new(input).is_absolute() => PathBuf::from(input),
        None => home.join(input),
    };
    if !path.is_dir() {
        return Err(format!("No such folder: {}", path.display()));
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(&path)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|error| format!("Could not run git: {error}"))?;
    if !output.status.success() {
        return Err(format!("Not a git repository: {}", path.display()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Two letters for a folder name, as the other app draws them: `omo-switch`
/// gives OS, `MyProject` gives MP, `onlydiffs` gives ON.
pub fn initials(name: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut previous_lower = false;
    for c in name.chars() {
        if !c.is_alphanumeric() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            previous_lower = false;
            continue;
        }
        // camelCase starts a new word at the capital.
        if c.is_uppercase() && previous_lower && !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
        previous_lower = c.is_lowercase() || c.is_numeric();
        current.push(c);
    }
    if !current.is_empty() {
        words.push(current);
    }
    let letters: String = match words.as_slice() {
        [] => "?".into(),
        [one] => one.chars().take(2).collect(),
        [first, second, ..] => first.chars().take(1).chain(second.chars().take(1)).collect(),
    };
    letters.to_uppercase()
}

/// A stable pick from `count` colours for a project, from its path — so a
/// repository keeps its colour between launches.
pub fn tint(path: &str, count: usize) -> usize {
    let hash = path.bytes().fold(0u32, |hash, byte| hash.wrapping_mul(31).wrapping_add(u32::from(byte)));
    hash as usize % count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_follow_the_other_apps_rules() {
        assert_eq!(initials("omo-switch"), "OS");
        assert_eq!(initials("MyProject"), "MP");
        assert_eq!(initials("onlydiffs"), "ON");
        assert_eq!(initials("---"), "?");
    }

    #[test]
    fn the_store_keeps_order_and_fields_it_does_not_use() {
        let dir = std::env::temp_dir().join(format!("onlydiffs-store-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a");
        let b = dir.join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let file = dir.join("projects.json");
        let (a, b) = (a.to_string_lossy().into_owned(), b.to_string_lossy().into_owned());
        std::fs::write(
            &file,
            serde_json::to_vec(&json!({ "version": 1, "projects": [
                { "path": a, "lastOpenedAt": 5, "iconScanHash": "keep-me" },
                { "path": "/srv/x", "host": "box", "lastOpenedAt": 9 },
                { "path": b, "lastOpenedAt": 7 },
            ]}))
            .unwrap(),
        )
        .unwrap();

        let mut store = Store::at(file.clone());
        assert_eq!(store.local().iter().map(|p| p.path.clone()).collect::<Vec<_>>(), [a.clone(), b.clone()]);
        assert_eq!(store.most_recent(), Some(b.clone()));

        store.touch(&a);
        assert_eq!(store.most_recent(), Some(a.clone()));
        // Opening does not move it on the rail.
        assert_eq!(store.local()[0].path, a);

        store.forget(&b);
        let reread = Store::at(file);
        let saved = reread.entries();
        assert_eq!(saved.len(), 2, "the remote entry stays");
        assert_eq!(saved[0]["iconScanHash"], "keep-me");
        assert_eq!(saved[1]["host"], "box");
        std::fs::remove_dir_all(dir).ok();
    }
}
