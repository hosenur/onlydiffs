//! The Effect build's workspace tests, ported.
//!
//! `Workspace::new` takes the state directory and the startup repository
//! directly rather than reading them from the environment, so these run in
//! parallel without racing over process-wide environment variables.


use std::path::{Path, PathBuf};
use std::process::Command;

use onlydiffs_lib::contract::ProjectLocation;
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
fn project_order_is_fixed_and_de_duplicated() {
    let sandbox = Sandbox::new();
    let workspace = sandbox.workspace();
    let one = sandbox.make_repo("one");
    let two = sandbox.make_repo("two");

    workspace.open(&as_str(&one)).expect("open one");
    workspace.open(&as_str(&two)).expect("open two");
    workspace.open(&as_str(&two)).expect("re-open two");

    let names: Vec<String> = workspace.list().into_iter().map(|p| p.name).collect();
    assert_eq!(names, vec!["one", "two"]);
}

#[test]
fn every_opened_repository_stays_in_the_history() {
    let sandbox = Sandbox::new();
    let workspace = sandbox.workspace();

    for index in 0..25 {
        let repo = sandbox.make_repo(&format!("project-{index}"));
        workspace.open(&as_str(&repo)).expect("open project");
    }

    assert_eq!(workspace.list().len(), 25);
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
fn history_from_before_project_icons_is_preserved() {
    let sandbox = Sandbox::new();
    let repo = sandbox.make_repo("legacy");
    std::fs::create_dir_all(sandbox.state_dir()).expect("create state dir");
    let stored = serde_json::json!({
        "version": 1,
        "projects": [{
            "path": as_str(&repo),
            "lastOpenedAt": 1
        }]
    });
    std::fs::write(
        sandbox.state_dir().join("projects.json"),
        serde_json::to_string(&stored).expect("serialize store"),
    )
    .expect("write legacy store");

    let workspace = sandbox.workspace();
    assert_eq!(workspace.list()[0].name, "legacy");
    assert!(workspace.list()[0].icon.is_none());
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
fn the_most_recent_repository_reopens_on_the_next_launch() {
    let sandbox = Sandbox::new();
    let first = sandbox.make_repo("first");
    let last = sandbox.make_repo("last");

    {
        let workspace = sandbox.workspace();
        workspace.open(&as_str(&first)).expect("open first");
        workspace.open(&as_str(&last)).expect("open last");
    }

    let relaunched = sandbox.workspace();
    assert_eq!(relaunched.current_path().expect("current"), last);
}

#[test]
fn a_vanished_recent_does_not_block_the_one_below_it() {
    let sandbox = Sandbox::new();
    let kept = sandbox.make_repo("kept");
    let gone = sandbox.make_repo("gone");

    {
        let workspace = sandbox.workspace();
        workspace.open(&as_str(&kept)).expect("open kept");
        workspace.open(&as_str(&gone)).expect("open gone");
    }
    std::fs::remove_dir_all(&gone).expect("remove the vanished repo");

    let relaunched = sandbox.workspace();
    assert_eq!(relaunched.current_path().expect("current"), kept);
}

#[test]
fn a_named_startup_repository_wins_over_the_most_recent_one() {
    let sandbox = Sandbox::new();
    let recent = sandbox.make_repo("recent");
    let pinned = sandbox.make_repo("pinned");

    {
        let workspace = sandbox.workspace();
        workspace.open(&as_str(&recent)).expect("open recent");
    }

    let workspace = Workspace::new(sandbox.state_dir(), Some(as_str(&pinned)));
    assert_eq!(workspace.current_path().expect("current"), pinned);
}

#[test]
fn a_bad_startup_repository_leaves_the_app_on_the_landing_page() {
    let sandbox = Sandbox::new();
    let recent = sandbox.make_repo("recent");
    let missing = sandbox.root.path().join("not-here");

    {
        let workspace = sandbox.workspace();
        workspace.open(&as_str(&recent)).expect("open recent");
    }

    // Naming a repository that cannot be opened is not an invitation to open a
    // different one instead.
    let workspace = Workspace::new(sandbox.state_dir(), Some(as_str(&missing)));
    assert!(workspace.current_project().is_none());
}

#[test]
fn the_store_never_lands_in_the_real_home_directory_during_tests() {
    let sandbox = Sandbox::new();
    let home_store = dirs::home_dir().expect("home").join(".onlydiffs");
    assert!(!sandbox.state_dir().starts_with(home_store));
}

#[test]
fn a_resolved_icon_reaches_the_project_list_and_survives_a_restart() {
    let sandbox = Sandbox::new();
    let repo = sandbox.make_repo("charted");
    {
        let workspace = sandbox.workspace();
        workspace.open(&as_str(&repo)).expect("open repo");
        workspace.record_icon_scan(
            &ProjectLocation::local(as_str(&repo)),
            "abc123".to_owned(),
            Some(("assets/logo.png".to_owned(), "data:image/png;base64,AA".to_owned())),
        );

        let icon = workspace.list()[0].icon.clone().expect("icon on the project");
        assert_eq!(icon.source_path, "assets/logo.png");
    }

    // A second workspace over the same state directory is what the next launch
    // sees: the artwork is already there, so nothing is sent to Groq again.
    let restarted = sandbox.workspace();
    let icon = restarted.list()[0].icon.clone().expect("icon after restart");
    assert_eq!(icon.data_url, "data:image/png;base64,AA");
    assert!(restarted.project_icon_jobs().is_empty());
}

#[test]
fn a_scan_that_found_nothing_is_remembered_so_it_is_not_re_sent() {
    let sandbox = Sandbox::new();
    let repo = sandbox.make_repo("bare");
    let workspace = sandbox.workspace();
    workspace.open(&as_str(&repo)).expect("open repo");
    workspace.record_icon_scan(
        &ProjectLocation::local(as_str(&repo)),
        "no-artwork".to_owned(),
        None,
    );

    let restarted = sandbox.workspace();
    let jobs = restarted.project_icon_jobs();

    // The repository is still queued — its artwork could appear at any commit —
    // but it carries the hash that stops the scan short of a Groq request.
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].previous_scan_hash.as_deref(), Some("no-artwork"));
    assert!(restarted.list()[0].icon.is_none());
}

#[test]
fn the_open_project_is_the_first_icon_resolved() {
    let sandbox = Sandbox::new();
    let first = sandbox.make_repo("first");
    let second = sandbox.make_repo("second");
    let third = sandbox.make_repo("third");
    let workspace = sandbox.workspace();
    for repo in [&first, &second, &third] {
        workspace.open(&as_str(repo)).expect("open repo");
    }
    workspace.open(&as_str(&second)).expect("re-open second");

    let jobs = workspace.project_icon_jobs();

    // Whichever project the user is looking at gets its icon first, even though
    // the list itself stays in first-opened order.
    assert_eq!(jobs[0].location, ProjectLocation::local(as_str(&second)));
    assert_eq!(jobs.len(), 3);
}
