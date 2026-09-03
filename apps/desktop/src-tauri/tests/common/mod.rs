//! Shared fixture: a throwaway git repository and a `Workspace` pointed at it.

use std::path::{Path, PathBuf};
use std::process::Command;

use onlydiffs_lib::services::repository::Repository;
use onlydiffs_lib::services::workspace::Workspace;
use tempfile::TempDir;

pub struct TestRepo {
    pub repo: TempDir,
    // Not every test file uses every corner of the fixture; `dead_code` here is
    // about the crate boundary, not about the field being pointless.
    #[allow(dead_code)]
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

    #[allow(dead_code)]
    pub fn state_dir(&self) -> PathBuf {
        self.state.path().to_path_buf()
    }

    /// A workspace already opened on this repository, the way the app arrives
    /// with `ONLYDIFFS_REPO_PATH` set.
    #[allow(dead_code)]
    pub fn workspace(&self) -> Workspace {
        Workspace::new(
            self.state_dir(),
            Some(self.path().to_string_lossy().into_owned()),
        )
    }

    /// The fixture as the services see it: a root, on this machine.
    pub fn repository(&self) -> Repository {
        Repository::local(self.path().to_path_buf())
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
