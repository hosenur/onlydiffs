//! Which repository the app is looking at, and which ones it has looked at
//! before.
//!
//! This is the one piece of genuinely mutable state in the backend: everything
//! else derives the path from here on each call, so opening a project takes
//! effect immediately without rebuilding anything.
//!
//! Every method is synchronous. The work is a stat or a small file rewrite, and
//! keeping it sync means the mutexes are never held across an await point.

use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::contract::Project;
use crate::error::AppError;

/// Where the recents list is kept, relative to the user's home directory.
const STORE_DIR: &str = ".onlydiffs";
const STORE_FILE: &str = "projects.json";
const MAX_RECENTS: usize = 20;
const STORE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredProject {
    path: String,
    last_opened_at: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Store {
    version: u32,
    projects: Vec<StoredProject>,
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// `git -C` does no `~` expansion, so do it here rather than hand it through.
fn expand_home(value: &str) -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    if value == "~" {
        return home;
    }
    match value.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => PathBuf::from(value),
    }
}

/// Lexical `.`/`..` folding, the equivalent of Node's `path.normalize`.
/// Deliberately not `canonicalize`: symlinks stay unresolved, so the path the
/// user typed is the path they get back.
pub(crate) fn normalize(path: &Path) -> PathBuf {
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

/// Turns whatever the user typed into an absolute path. A relative path is
/// resolved against the home directory rather than the process's working
/// directory, which for a packaged app is wherever Finder launched it from.
fn to_absolute(input: &str) -> PathBuf {
    let expanded = expand_home(input.trim());
    if expanded.is_absolute() {
        normalize(&expanded)
    } else {
        normalize(&dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")).join(expanded))
    }
}

/// Finds the repository root at or above `dir`, so pasting any path inside a
/// checkout opens the repository rather than being rejected.
fn find_repo_root(dir: &Path) -> Option<PathBuf> {
    let mut candidate = dir.to_path_buf();
    loop {
        if candidate.join(".git").exists() {
            return Some(candidate);
        }
        if !candidate.pop() {
            return None;
        }
    }
}

fn describe(repo_path: &Path) -> Project {
    let display = repo_path.to_string_lossy().into_owned();
    let name = repo_path
        .file_name()
        .map(|segment| segment.to_string_lossy().into_owned())
        .filter(|segment| !segment.is_empty())
        .unwrap_or_else(|| display.clone());
    Project { path: display, name }
}

pub struct Workspace {
    store_path: PathBuf,
    current: Mutex<Option<PathBuf>>,
    recents: Mutex<Vec<StoredProject>>,
}

impl Workspace {
    /// `state_dir` is where the recents file lives; `initial_repo` is opened
    /// straight away when present, which is how the app lands on a repository
    /// instead of the landing page.
    pub fn new(state_dir: PathBuf, initial_repo: Option<String>) -> Self {
        let store_path = state_dir.join(STORE_FILE);
        let workspace = Self {
            recents: Mutex::new(read_store(&store_path)),
            store_path,
            current: Mutex::new(None),
        };
        if let Some(path) = initial_repo {
            if !path.trim().is_empty() {
                // A bad value in the environment is not worth failing startup
                // over; the landing page is a perfectly good fallback.
                let _ = workspace.open(&path);
            }
        }
        workspace
    }

    /// The two environment knobs: `ONLYDIFFS_STATE_DIR` redirects the recents
    /// file, and `ONLYDIFFS_REPO_PATH` opens a repository at startup instead of
    /// landing on the project picker.
    pub fn from_env() -> Self {
        let state_dir = std::env::var("ONLYDIFFS_STATE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(STORE_DIR)
            });
        Self::new(state_dir, std::env::var("ONLYDIFFS_REPO_PATH").ok())
    }

    /// Validates a path and, if it checks out, makes it the current project.
    pub fn open(&self, input: &str) -> Result<Project, AppError> {
        if input.trim().is_empty() {
            return Err(AppError::InvalidProject(
                "Enter a path to a git repository.".into(),
            ));
        }

        let absolute = to_absolute(input);
        if !absolute.is_dir() {
            return Err(AppError::InvalidProject(format!(
                "No such folder: {}",
                absolute.display()
            )));
        }

        let root = find_repo_root(&absolute).ok_or_else(|| {
            AppError::InvalidProject(format!(
                "Not a git repository (no .git found at or above {}).",
                absolute.display()
            ))
        })?;

        let display = root.to_string_lossy().into_owned();
        *self.current.lock().expect("workspace lock") = Some(root.clone());
        {
            let mut recents = self.recents.lock().expect("recents lock");
            recents.retain(|entry| entry.path != display);
            recents.insert(
                0,
                StoredProject {
                    path: display,
                    last_opened_at: now_millis(),
                },
            );
            recents.truncate(MAX_RECENTS);
        }
        self.write_store();

        Ok(describe(&root))
    }

    /// The active repository. Everything that shells out to git needs this.
    pub fn current_path(&self) -> Result<PathBuf, AppError> {
        self.current
            .lock()
            .expect("workspace lock")
            .clone()
            .ok_or_else(|| AppError::NoProjectOpen("No project is open.".into()))
    }

    pub fn current_project(&self) -> Option<Project> {
        self.current
            .lock()
            .expect("workspace lock")
            .as_deref()
            .map(describe)
    }

    /// Recents, newest first, with entries that no longer exist dropped.
    pub fn list(&self) -> Vec<Project> {
        let mut entries = self.recents.lock().expect("recents lock").clone();
        entries.sort_by(|a, b| b.last_opened_at.cmp(&a.last_opened_at));
        entries
            .into_iter()
            .filter(|entry| Path::new(&entry.path).exists())
            .map(|entry| describe(Path::new(&entry.path)))
            .collect()
    }

    pub fn forget(&self, repo_path: &str) {
        self.recents
            .lock()
            .expect("recents lock")
            .retain(|entry| entry.path != repo_path);
        self.write_store();
    }

    fn write_store(&self) {
        let projects = self.recents.lock().expect("recents lock").clone();
        let store = Store {
            version: STORE_VERSION,
            projects,
        };
        let Ok(body) = serde_json::to_string_pretty(&store) else {
            return;
        };
        if let Some(parent) = self.store_path.parent() {
            // Losing the history file is not worth failing an open over.
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&self.store_path, body);
    }
}

/// A missing or corrupt file just means "no history yet".
fn read_store(store_path: &Path) -> Vec<StoredProject> {
    std::fs::read_to_string(store_path)
        .ok()
        .and_then(|body| serde_json::from_str::<Store>(&body).ok())
        .filter(|store| store.version == STORE_VERSION)
        .map(|store| store.projects)
        .unwrap_or_default()
}
