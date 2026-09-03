//! The Effect build's diff and history tests, ported.
//!
//! They are what proves the porcelain walk and the startup/lazy split behave
//! the same after the move to Rust.

mod common;

use common::TestRepo;
use onlydiffs_lib::contract::ChangeStatus;
use onlydiffs_lib::services::{diff, history};

#[tokio::test]
async fn complete_file_contents_are_loaded_separately_from_the_startup_diff() {
    let fixture = TestRepo::new();

    let original: String = (1..=40)
        .map(|index| {
            if index == 25 {
                "UNCHANGED_MARKER".to_owned()
            } else {
                format!("original line {index}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";

    fixture.write("example.ts", &original);
    fixture.git(&["add", "example.ts"]);
    fixture.git(&["commit", "-q", "-m", "fixture"]);

    fixture.write(
        "example.ts",
        &original.replace("original line 1\n", "changed line 1\n"),
    );

    let repo = fixture.repository();
    let repo_diff = diff::get_diff(&repo).await.expect("diff");
    assert_eq!(repo_diff.files.len(), 1);

    let file = &repo_diff.files[0];
    assert_eq!(file.path, "example.ts");
    assert!(!file.staged);
    assert_eq!(file.status, ChangeStatus::Modified);
    assert_eq!(file.additions, 1);
    assert_eq!(file.deletions, 1);

    let contents = diff::get_file_contents(
        &repo,
        &file.path,
        file.old_path.as_deref(),
        file.status,
        file.staged,
    )
    .await
    .expect("file contents");

    // The unchanged region is present on both sides, which the patch alone
    // would not give the renderer.
    let old = contents.old_contents.expect("old side");
    let new = contents.new_contents.expect("new side");
    assert!(old.contains("UNCHANGED_MARKER"));
    assert!(new.contains("UNCHANGED_MARKER"));
    assert!(old.contains("original line 1\n"));
    assert!(new.contains("changed line 1\n"));

    diff::stage_file(&repo, &file.path, file.old_path.as_deref())
        .await
        .expect("stage");

    let staged = diff::get_diff(&repo).await.expect("diff after staging");
    assert_eq!(staged.files.len(), 1);
    assert!(staged.files[0].staged);
}

#[tokio::test]
async fn a_path_edited_staged_then_edited_again_yields_two_rows() {
    let fixture = TestRepo::new();
    fixture.write("both.txt", "one\n");
    fixture.git(&["add", "."]);
    fixture.git(&["commit", "-q", "-m", "fixture"]);

    fixture.write("both.txt", "two\n");
    fixture.git(&["add", "both.txt"]);
    fixture.write("both.txt", "three\n");

    let repo_diff = diff::get_diff(&fixture.repository()).await.expect("diff");
    assert_eq!(repo_diff.files.len(), 2);
    // Staged first, per the sort.
    let staged: Vec<bool> = repo_diff.files.iter().map(|file| file.staged).collect();
    assert_eq!(staged, vec![true, false]);
    let ids: Vec<&str> = repo_diff.files.iter().map(|file| file.id.as_str()).collect();
    assert_eq!(ids, vec!["staged:both.txt", "unstaged:both.txt"]);
}

#[tokio::test]
async fn the_commit_message_diff_includes_every_worktree_half() {
    let fixture = TestRepo::new();
    fixture.write("staged.txt", "before staged\n");
    fixture.write("unstaged.txt", "before unstaged\n");
    fixture.git(&["add", "."]);
    fixture.git(&["commit", "-q", "-m", "fixture"]);

    fixture.write("staged.txt", "after staged\n");
    fixture.git(&["add", "staged.txt"]);
    fixture.write("unstaged.txt", "after unstaged\n");
    fixture.write("untracked.txt", "new untracked\n");

    let document = diff::commit_message_diff(&fixture.repository())
        .await
        .expect("commit message diff");

    assert!(document.contains("### staged: staged.txt"));
    assert!(document.contains("+after staged"));
    assert!(document.contains("### unstaged: unstaged.txt"));
    assert!(document.contains("+after unstaged"));
    assert!(document.contains("### untracked: untracked.txt"));
    assert!(document.contains("+new untracked"));
}

#[tokio::test]
async fn a_rename_is_reported_with_the_path_it_moved_from() {
    let fixture = TestRepo::new();
    fixture.write("before.txt", "same contents\n");
    fixture.git(&["add", "."]);
    fixture.git(&["commit", "-q", "-m", "fixture"]);
    fixture.git(&["mv", "before.txt", "after.txt"]);

    let repo_diff = diff::get_diff(&fixture.repository()).await.expect("diff");
    let renamed = repo_diff
        .files
        .iter()
        .find(|file| file.path == "after.txt")
        .expect("the renamed row");

    assert_eq!(renamed.status, ChangeStatus::Renamed);
    assert_eq!(renamed.old_path.as_deref(), Some("before.txt"));
    assert!(renamed.staged);
}

#[tokio::test]
async fn history_returns_commits_newest_first() {
    let fixture = TestRepo::new();
    fixture.write("a.txt", "one\n");
    fixture.git(&["add", "."]);
    fixture.git(&["commit", "-q", "-m", "first commit"]);
    fixture.write("a.txt", "two\n");
    fixture.git(&["commit", "-q", "-a", "-m", "second commit"]);

    let commits = history::get_history(&fixture.repository(), Some(10.0))
        .await
        .expect("history");

    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0].subject, "second commit");
    assert_eq!(commits[1].subject, "first commit");
    assert_eq!(commits[0].author, "OnlyDiffs Test");
    assert_eq!(commits[0].author_email, "onlydiffs@example.test");
    assert!(!commits[0].is_merge);
    assert!(!commits[0].short_hash.is_empty());
}

#[tokio::test]
async fn a_path_escaping_the_repository_is_rejected_before_git_sees_it() {
    let fixture = TestRepo::new();
    fixture.write("a.txt", "one\n");
    fixture.git(&["add", "."]);
    fixture.git(&["commit", "-q", "-m", "fixture"]);

    let result = diff::get_file_contents(
        &fixture.repository(),
        "../outside.txt",
        None,
        ChangeStatus::Modified,
        false,
    )
    .await;

    let error = result.expect_err("the traversal must be rejected");
    assert_eq!(error.tag(), "InvalidPathError");
}
