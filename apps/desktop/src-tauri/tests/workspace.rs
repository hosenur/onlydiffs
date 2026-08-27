//! The Effect build's workspace tests, ported.
//!
//! `Workspace::new` takes the state directory and the startup repository
//! directly rather than reading them from the environment, so these run in
//! parallel without racing over process-wide environment variables.


use std::path::{Path, PathBuf};
use std::process::Command;

use onlydiffs_lib::services::workspace::Workspace;
use tempfile::TempDir;

struct Sandbox {
    root: TempDir,
}

impl Sandbox {
    fn new() -> Self {
        Self {
            root: TempDir::new().expect("temp root"),
        }
    }

    fn state_dir(&self) -> PathBuf {
        self.root.path().join("state")
    }

    /// A workspace with no repository preselected.
    fn workspace(&self) -> Workspace {
        Workspace::new(self.state_dir(), None)
    }

    fn make_repo(&self, name: &str) -> PathBuf {
        let dir = self.root.path().join(name);
        std::fs::create_dir_all(&dir).expect("create repo dir");
        let status = Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(["init", "-q"])
            .status()
            .expect("git init runs");
        assert!(status.success());
        dir
    }
}

fn as_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[test]
fn no_project_is_open_until_one_is_chosen() {
    let sandbox = Sandbox::new();
    let workspace = sandbox.workspace();

    assert!(workspace.current_project().is_none());
    let error = workspace.current_path().expect_err("no project open");
    assert_eq!(error.tag(), "NoProjectOpenError");
}

#[test]
fn opening_a_repository_makes_it_current_and_records_it() {
    let sandbox = Sandbox::new();
    let workspace = sandbox.workspace();
    let repo = sandbox.make_repo("alpha");

    let opened = workspace.open(&as_str(&repo)).expect("open");
    assert_eq!(opened.path, as_str(&repo));
    assert_eq!(opened.name, "alpha");

    assert_eq!(workspace.current_path().expect("current"), repo);
    let listed = workspace.list();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].path, as_str(&repo));
    assert_eq!(listed[0].name, "alpha");
}

#[test]
fn a_path_inside_a_checkout_opens_the_repository_root() {
    let sandbox = Sandbox::new();
    let workspace = sandbox.workspace();
    let repo = sandbox.make_repo("beta");
    let nested = repo.join("src").join("deep");
    std::fs::create_dir_all(&nested).expect("create nested dir");

    let opened = workspace.open(&as_str(&nested)).expect("open");
    assert_eq!(opened.path, as_str(&repo));
}

#[test]
fn a_folder_that_is_not_a_repository_is_rejected() {
    let sandbox = Sandbox::new();
    let workspace = sandbox.workspace();
    let plain = sandbox.root.path().join("not-a-repo");
    std::fs::create_dir_all(&plain).expect("create plain dir");

    let error = workspace.open(&as_str(&plain)).expect_err("rejected");
    assert_eq!(error.tag(), "InvalidProjectError");
    // The rejection must not become the current project.
    assert!(workspace.current_project().is_none());
}

#[test]
fn a_path_that_does_not_exist_is_rejected() {
    let sandbox = Sandbox::new();
    let workspace = sandbox.workspace();
    let missing = sandbox.root.path().join("nope");

    assert!(workspace.open(&as_str(&missing)).is_err());
}

#[test]
fn an_empty_path_is_rejected() {
    let sandbox = Sandbox::new();
    assert!(sandbox.workspace().open("   ").is_err());
}

#[test]
fn recents_are_newest_first_and_de_duplicated() {
    let sandbox = Sandbox::new();
    let workspace = sandbox.workspace();
    let one = sandbox.make_repo("one");
    let two = sandbox.make_repo("two");

    workspace.open(&as_str(&one)).expect("open one");
    workspace.open(&as_str(&two)).expect("open two");
    workspace.open(&as_str(&one)).expect("re-open one");

    let names: Vec<String> = workspace.list().into_iter().map(|p| p.name).collect();
    assert_eq!(names, vec!["one", "two"]);
}

#[test]
fn recents_survive_a_restart_and_skip_folders_that_vanished() {
    let sandbox = Sandbox::new();
    let kept = sandbox.make_repo("kept");
    let gone = sandbox.make_repo("gone");

    {
        let workspace = sandbox.workspace();
        workspace.open(&as_str(&kept)).expect("open kept");
        workspace.open(&as_str(&gone)).expect("open gone");
    }

    let stored = std::fs::read_to_string(sandbox.state_dir().join("projects.json"))
        .expect("the store was written");
    let parsed: serde_json::Value = serde_json::from_str(&stored).expect("valid json");
    assert_eq!(parsed["projects"].as_array().expect("projects").len(), 2);

    std::fs::remove_dir_all(&gone).expect("remove the vanished repo");

    // A fresh workspace reads the store back off disk.
    let restarted = sandbox.workspace();
    let names: Vec<String> = restarted.list().into_iter().map(|p| p.name).collect();
    assert_eq!(names, vec!["kept"]);
}

#[test]
fn forget_removes_a_project_from_the_history() {
    let sandbox = Sandbox::new();
    let workspace = sandbox.workspace();
    let repo = sandbox.make_repo("temporary");

    workspace.open(&as_str(&repo)).expect("open");
    workspace.forget(&as_str(&repo));
    assert!(workspace.list().is_empty());
}

#[test]
fn a_corrupt_store_file_is_treated_as_empty_history() {
    let sandbox = Sandbox::new();
    std::fs::create_dir_all(sandbox.state_dir()).expect("create state dir");
    std::fs::write(sandbox.state_dir().join("projects.json"), "{ not json")
        .expect("write corrupt store");

    assert!(sandbox.workspace().list().is_empty());
}

#[test]
fn a_startup_repository_is_opened_immediately() {
    let sandbox = Sandbox::new();
    let repo = sandbox.make_repo("preselected");

    let workspace = Workspace::new(sandbox.state_dir(), Some(as_str(&repo)));
    assert_eq!(workspace.current_path().expect("current"), repo);
}

#[test]
fn a_bad_startup_repository_leaves_the_app_on_the_landing_page() {
    let sandbox = Sandbox::new();
    let missing = sandbox.root.path().join("not-here");

    let workspace = Workspace::new(sandbox.state_dir(), Some(as_str(&missing)));
    assert!(workspace.current_project().is_none());
}

#[test]
fn the_store_never_lands_in_the_real_home_directory_during_tests() {
    let sandbox = Sandbox::new();
    let home_store = dirs::home_dir().expect("home").join(".onlydiffs");
    assert!(!sandbox.state_dir().starts_with(home_store));
}
