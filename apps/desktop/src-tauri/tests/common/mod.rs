//! Shared fixture: a throwaway git repository and a `Workspace` pointed at it.

use std::path::{Path, PathBuf};
use std::process::Command;

use onlydiffs_lib::services::workspace::Workspace;
use tempfile::TempDir;

pub struct TestRepo {
    pub repo: TempDir,
    pub state: TempDir,
}

impl TestRepo {
    pub fn new() -> Self {
        let repo = TempDir::new().expect("temp repo");
        let state = TempDir::new().expect("temp state");
        let fixture = Self { repo, state };
        fixture.git(&["init", "-q"]);
        fixture.git(&["config", "user.email", "onlydiffs@example.test"]);
        fixture.git(&["config", "user.name", "OnlyDiffs Test"]);
        fixture
    }

    pub fn path(&self) -> &Path {
        self.repo.path()
    }

    pub fn state_dir(&self) -> PathBuf {
        self.state.path().to_path_buf()
    }

    /// A workspace already opened on this repository, the way the app arrives
    /// with `ONLYDIFFS_REPO_PATH` set.
    pub fn workspace(&self) -> Workspace {
        Workspace::new(
            self.state_dir(),
            Some(self.path().to_string_lossy().into_owned()),
        )
    }

    pub fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(self.path())
            .args(args)
            .output()
            .expect("git runs");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    pub fn write(&self, name: &str, contents: &str) {
        std::fs::write(self.path().join(name), contents).expect("write fixture file");
    }
}
