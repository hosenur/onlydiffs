//! Whether a registered Claude session is still there.
//!
//! The registration file outlives the process that wrote it — cleanup runs on
//! `exit`, `SIGINT`, and `SIGTERM`, and none of those fire for `kill -9`. This
//! is the check that keeps a crashed session from being reported as connected
//! until someone types a message at it.

use std::process::{Command, Stdio};

use onlydiffs_lib::services::claude_channel::is_process_alive;

#[test]
fn this_process_is_alive() {
    assert!(is_process_alive(std::process::id() as i64));
}

#[test]
fn a_process_that_has_exited_is_not() {
    let mut child = Command::new("sleep")
        .arg("30")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn sleep");
    let pid = child.id() as i64;
    assert!(is_process_alive(pid), "the child should be running");

    child.kill().expect("kill child");
    // Reap it. Until the parent waits, the pid stays valid as a zombie — which
    // is the honest answer for a process that has not been collected yet, and
    // not the state a crashed Claude session leaves behind.
    child.wait().expect("wait for child");

    assert!(!is_process_alive(pid), "a reaped child should read as gone");
}

#[test]
fn nonsense_pids_are_not_alive() {
    assert!(!is_process_alive(0));
    assert!(!is_process_alive(-1));
    // Past any pid the platform will hand out, so it cannot collide with a
    // real process on the machine running these tests.
    assert!(!is_process_alive(i64::from(i32::MAX) + 1));
}
