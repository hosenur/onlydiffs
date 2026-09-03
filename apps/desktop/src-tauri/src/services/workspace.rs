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

use crate::contract::{Project, ProjectIcon, ProjectLocation};
use crate::error::AppError;

const STORE_FILE: &str = "projects.json";
const STORE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredProject {
    path: String,
    /// The SSH alias this project is on. Absent for a project on this machine,
    /// which is what keeps a store written by an older build readable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    host: Option<String>,
    last_opened_at: u64,
    #[serde(default)]
    icon: Option<StoredProjectIcon>,
    #[serde(default)]
    icon_scan_hash: Option<String>,
}

impl StoredProject {
    fn location(&self) -> ProjectLocation {
        ProjectLocation {
            host: self.host.clone(),
            path: self.path.clone(),
        }
    }

    /// Whether this entry is the one being asked about. Path alone is not
    /// enough: the same path on two hosts is two projects.
    fn is(&self, location: &ProjectLocation) -> bool {
        self.path == location.path && self.host == location.host
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredProjectIcon {
    source_path: String,
    data_url: String,
}

#[derive(Debug, Clone)]
pub struct ProjectIconJob {
    pub location: ProjectLocation,
    pub previous_scan_hash: Option<String>,
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

fn describe(location: &ProjectLocation, icon: Option<&StoredProjectIcon>) -> Project {
    // Last segment of the *remote* path for a remote project: the host's path
    // style is what named it, and a Windows-style root is not something this
    // app produces on either side.
    let name = location
        .path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|segment| !segment.is_empty())
        .unwrap_or(&location.path)
        .to_owned();
    Project {
        path: location.display(),
        name,
        host: location.host.clone(),
        root: location.path.clone(),
        icon: icon.map(|icon| ProjectIcon {
            source_path: icon.source_path.clone(),
            data_url: icon.data_url.clone(),
        }),
    }
}

pub struct Workspace {
    store_path: PathBuf,
    current: Mutex<Option<ProjectLocation>>,
    recents: Mutex<Vec<StoredProject>>,
}

impl Workspace {
    /// `state_dir` is where the recents file lives; `initial_repo` is opened
    /// straight away when present, which is how the app lands on a repository
    /// instead of the landing page. With nothing named, the most recent entry
    /// stands in, so quitting and relaunching returns to where you left off.
    pub fn new(state_dir: PathBuf, initial_repo: Option<String>) -> Self {
        let store_path = state_dir.join(STORE_FILE);
        let workspace = Self {
            recents: Mutex::new(read_store(&store_path)),
            store_path,
            current: Mutex::new(None),
        };
        let requested = initial_repo.filter(|path| !path.trim().is_empty());
        // Missing folders are skipped, so the fallback never reopens a
        // checkout that has since been deleted or unmounted.
        let restored = || workspace.most_recent_path();
        if let Some(path) = requested.or_else(restored) {
            // A bad value in the environment, or a repository that stopped
            // being one, is not worth failing startup over; the landing page
            // is a perfectly good fallback.
            let _ = workspace.open(&path);
        }
        workspace
    }

    /// The two environment knobs: `ONLYDIFFS_STATE_DIR` redirects the recents
    /// file, and `ONLYDIFFS_REPO_PATH` pins the repository opened at startup,
    /// overriding the most-recent one that would otherwise be restored.
    pub fn from_env() -> Self {
        Self::new(
            super::state_dir(),
            std::env::var("ONLYDIFFS_REPO_PATH").ok(),
        )
    }

    /// Validates a path on this machine and, if it checks out, opens it.
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

        self.adopt(ProjectLocation::local(root.to_string_lossy().into_owned()))
    }

    /// Opens a repository root that has already been resolved — by
    /// `find_repo_root` here, or by the agent on the machine it is on.
    ///
    /// The remote half of `open`. Validation happens where the path is, because
    /// this side has no way to stat it.
    pub fn adopt(&self, location: ProjectLocation) -> Result<Project, AppError> {
        *self.current.lock().expect("workspace lock") = Some(location.clone());
        {
            let mut recents = self.recents.lock().expect("recents lock");
            // The timestamp controls startup restoration, not display order.
            // Keep it monotonic even when two opens land in the same millisecond.
            let opened_at = now_millis().max(
                recents
                    .iter()
                    .map(|entry| entry.last_opened_at)
                    .max()
                    .unwrap_or(0)
                    .saturating_add(1),
            );
            if let Some(entry) = recents.iter_mut().find(|entry| entry.is(&location)) {
                entry.last_opened_at = opened_at;
            } else {
                recents.push(StoredProject {
                    path: location.path.clone(),
                    host: location.host.clone(),
                    last_opened_at: opened_at,
                    icon: None,
                    icon_scan_hash: None,
                });
            }
        }
        self.write_store();

        Ok(self.describe(&location))
    }

    /// Where the active repository is. Everything that reaches a repository
    /// starts here.
    pub fn current_location(&self) -> Result<ProjectLocation, AppError> {
        self.current
            .lock()
            .expect("workspace lock")
            .clone()
            .ok_or_else(|| AppError::NoProjectOpen("No project is open.".into()))
    }

    /// The active repository's root, when it is on this machine.
    pub fn current_path(&self) -> Result<PathBuf, AppError> {
        let location = self.current_location()?;
        if location.is_remote() {
            return Err(AppError::NoProjectOpen(
                "The open project is on another machine.".into(),
            ));
        }
        Ok(PathBuf::from(location.path))
    }

    pub fn current_project(&self) -> Option<Project> {
        let location = self.current.lock().expect("workspace lock").clone()?;
        Some(self.describe(&location))
    }

    /// Projects stay in first-opened order so switching one never moves its UI.
    ///
    /// A local entry whose folder is gone is dropped. A remote one never is:
    /// its host may simply be asleep, and forgetting a project because a laptop
    /// was on a train would be the wrong answer to a temporary question.
    pub fn list(&self) -> Vec<Project> {
        self.recents
            .lock()
            .expect("recents lock")
            .iter()
            .filter(|entry| entry.host.is_some() || Path::new(&entry.path).exists())
            .map(|entry| describe(&entry.location(), entry.icon.as_ref()))
            .collect()
    }

    fn describe(&self, location: &ProjectLocation) -> Project {
        let recents = self.recents.lock().expect("recents lock");
        let icon = recents
            .iter()
            .find(|entry| entry.is(location))
            .and_then(|entry| entry.icon.as_ref());
        describe(location, icon)
    }

    pub fn project_icon_jobs(&self) -> Vec<ProjectIconJob> {
        let current = self.current.lock().expect("workspace lock").clone();
        let mut jobs: Vec<ProjectIconJob> = self
            .recents
            .lock()
            .expect("recents lock")
            .iter()
            .filter(|entry| {
                entry.icon.is_none() && (entry.host.is_some() || Path::new(&entry.path).exists())
            })
            .map(|entry| ProjectIconJob {
                location: entry.location(),
                previous_scan_hash: entry.icon_scan_hash.clone(),
            })
            .collect();
        jobs.sort_by_key(|job| match &current {
            Some(location) => location != &job.location,
            None => true,
        });
        jobs
    }

    pub fn record_icon_scan(
        &self,
        location: &ProjectLocation,
        scan_hash: String,
        icon: Option<(String, String)>,
    ) {
        let mut recents = self.recents.lock().expect("recents lock");
        let Some(entry) = recents.iter_mut().find(|entry| entry.is(location)) else {
            return;
        };
        entry.icon_scan_hash = Some(scan_hash);
        if let Some((source_path, data_url)) = icon {
            entry.icon = Some(StoredProjectIcon {
                source_path,
                data_url,
            });
        }
        drop(recents);
        self.write_store();
    }

    /// The most recently opened project on *this* machine.
    ///
    /// Startup restores a local project only. A remote one would mean dialling
    /// a host, authenticating, and possibly prompting, before the window has
    /// drawn anything — which is not what launching an app should do.
    fn most_recent_path(&self) -> Option<String> {
        self.recents
            .lock()
            .expect("recents lock")
            .iter()
            .filter(|entry| entry.host.is_none() && Path::new(&entry.path).exists())
            .max_by_key(|entry| entry.last_opened_at)
            .map(|entry| entry.path.clone())
    }

    pub fn forget(&self, display: &str) {
        self.recents
            .lock()
            .expect("recents lock")
            .retain(|entry| entry.location().display() != display);
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
