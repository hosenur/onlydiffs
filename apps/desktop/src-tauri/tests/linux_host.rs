//! The whole feature, against a machine that is not this one.
//!
//! `tests/ssh.rs` proves the protocol and the transport against a local
//! `sshd`, but the host it connects to is this Mac — same architecture, same
//! libc, same git. What that cannot prove is the thing most likely to break in
//! the field: that an agent cross-compiled here runs on someone's Debian build
//! box, that the probe picks the right triple for it, and that a diff collected
//! by a Linux binary comes back in the shape the app expects.
//!
//! Opt-in, because it needs Docker and a cross-compiled agent — neither of
//! which every checkout has:
//!
//! ```sh
//! cd apps/desktop
//! ./scripts/build-agents.sh            # needs zig + cargo-zigbuild
//! ./scripts/linux-test-host.sh up
//! ONLYDIFFS_LINUX_HOST_DIR=… cargo test --manifest-path src-tauri/Cargo.toml --test linux_host
//! ```

use std::path::{Path, PathBuf};

use onlydiffs_core::services::repository::Repository;
use onlydiffs_lib::services::ssh::{target, AgentTransport, Prompt, SshConnection};
use tokio::sync::mpsc;

/// `None` when the host is not up, so the suite is quiet rather than red on a
/// machine that has not been asked to run this.
fn host_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var("ONLYDIFFS_LINUX_HOST_DIR").ok()?);
    dir.join("client").is_file().then_some(dir)
}

fn port() -> String {
    std::env::var("ONLYDIFFS_LINUX_HOST_PORT").unwrap_or_else(|_| "2223".into())
}

/// This host and nothing else: this identity, this `known_hosts`, and none of
/// the developer's own ssh config.
fn args(dir: &Path) -> Vec<String> {
    vec![
        "-F".into(),
        "/dev/null".into(),
        "-i".into(),
        dir.join("client").to_string_lossy().into_owned(),
        "-o".into(),
        "IdentitiesOnly=yes".into(),
        "-o".into(),
        format!("UserKnownHostsFile={}", dir.join("kh").display()),
        "-o".into(),
        "GlobalKnownHostsFile=/dev/null".into(),
        "-p".into(),
        port(),
    ]
}

#[tokio::test]
async fn a_linux_host_is_probed_uploaded_to_and_reviewed_from_macos() {
    let Some(dir) = host_dir() else {
        eprintln!("no Linux test host; skipping (scripts/linux-test-host.sh up)");
        return;
    };

    let resolved = target::resolve("root@127.0.0.1", &args(&dir))
        .await
        .expect("resolve");
    let (tx, _rx) = mpsc::unbounded_channel::<Prompt>();
    let mut connection = SshConnection::connect(resolved, args(&dir), tx)
        .await
        .expect("connect");

    // The probe is what decides which of the four bundled agents is uploaded.
    let probe = connection.probe().clone();
    assert_eq!(probe.os, "Linux");
    assert_eq!(probe.arch, "x86_64");
    assert_eq!(probe.target_triple(), Some("x86_64-unknown-linux-gnu"));
    assert!(probe.home.starts_with('/'));

    let (events, _incoming) = mpsc::unbounded_channel();
    let transport = AgentTransport::start(&connection, events)
        .await
        .expect("the cross-compiled agent runs on this host");
    assert_eq!(transport.agent_version(), env!("CARGO_PKG_VERSION"));

    // A repository created on the host, reviewed from here.
    connection
        .run_script(
            "rm -rf /tmp/onlydiffs-test && mkdir -p /tmp/onlydiffs-test && \
             cd /tmp/onlydiffs-test && git init -q . && printf 'one\\n' > a.txt && \
             git add -A && git commit -q -m seed && \
             printf 'one\\ntwo\\n' > a.txt && printf 'new\\n' > b.txt",
        )
        .await
        .expect("seed the repository");

    let repo = Repository::remote(
        "linux-box".into(),
        "/tmp/onlydiffs-test".into(),
        transport.calls(),
    );

    let diff = repo.diff().await.expect("diff");
    let paths: Vec<&str> = diff.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(paths, vec!["a.txt", "b.txt"]);
    assert_eq!(diff.repo_path, "linux-box:/tmp/onlydiffs-test");

    repo.stage_file("a.txt", None).await.expect("stage");
    let head = repo
        .commit_all("Reviewed from macOS, committed on Linux")
        .await
        .expect("commit");
    assert!(head.contains("Reviewed from macOS"), "{head}");

    assert_eq!(repo.history(Some(5.0)).await.expect("history").len(), 2);

    transport.shutdown().await;
    connection.disconnect().await;
}

/// The Claude channel, against a session on the *host*.
///
/// This is the question the whole remoting raises and the one a status
/// indicator cannot answer for you: a Claude Code session reviewing a checkout
/// on a build box is a process on that build box, with its channel socket in
/// that machine's home directory and a registration naming a path in that
/// machine's filesystem. Reading the registry from this Mac would find nothing
/// — and, worse, finding something would mean sending a message about the
/// wrong repository to a session that has never seen it.
///
/// So the agent does it: reads the host's `~/.onlydiffs/claude-channels`,
/// matches the registration's `cwd` against the repository root *there*,
/// connects to the socket *there*, and writes. What this test stands up is a
/// stand-in for the channel server's side of that contract — a real process on
/// a real unix socket, writing down what it received.
#[tokio::test]
async fn a_claude_session_on_the_host_receives_a_message_sent_from_here() {
    let Some(dir) = host_dir() else {
        eprintln!("no Linux test host; skipping (scripts/linux-test-host.sh up)");
        return;
    };

    let resolved = target::resolve("root@127.0.0.1", &args(&dir))
        .await
        .expect("resolve");
    let (tx, _rx) = mpsc::unbounded_channel::<Prompt>();
    let mut connection = SshConnection::connect(resolved, args(&dir), tx)
        .await
        .expect("connect");

    let root = "/tmp/onlydiffs-claude";

    // A repository, and a process standing in for the channel server a Claude
    // Code session runs in it: the same registration the server writes, and
    // the same one-message-per-connection socket it serves.
    connection
        .run_script(&format!(
            r#"set -e
rm -rf {root} ~/.onlydiffs/claude-channels
mkdir -p {root} ~/.onlydiffs/claude-channels
cd {root} && git init -q . && printf 'one
' > a.txt && git add -A && git commit -q -m seed
cat > /tmp/fake-claude.py <<'PY'
import json, os, socket, time

directory = os.path.expanduser("~/.onlydiffs/claude-channels")
path = os.path.join(directory, "session.sock")
server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
server.bind(path)
server.listen(4)
with open(os.path.join(directory, "session.json"), "w") as f:
    json.dump({{"schemaVersion": 2, "pid": os.getpid(), "cwd": "{root}",
               "socket": path, "startedAt": int(time.time() * 1000)}}, f)
while True:
    conn, _ = server.accept()
    body = b""
    while True:
        chunk = conn.recv(4096)
        if not chunk:
            break
        body += chunk
    if body.strip():
        with open("/tmp/claude-received.txt", "a") as f:
            f.write(body.decode() + "\n")
        conn.sendall(b'{{"messageId": "msg-from-the-host"}}\n')
    conn.close()
PY
nohup python3 /tmp/fake-claude.py >/tmp/fake-claude.log 2>&1 &
for _ in $(seq 1 40); do
  [ -s ~/.onlydiffs/claude-channels/session.json ] && exit 0
  sleep 0.25
done
echo "the stand-in never registered" >&2; exit 1"#
        ))
        .await
        .expect("stand up a session on the host");

    let (events, _incoming) = mpsc::unbounded_channel();
    let transport = AgentTransport::start(&connection, events)
        .await
        .expect("agent");
    let repo = Repository::remote("linux-box".into(), root.into(), transport.calls());

    // The registry it read is the host's, and the socket it connected to is a
    // process on the host — neither exists on this machine.
    let status = repo.claude_status().await;
    assert!(status.connected, "the session on the host should be found");
    assert_eq!(status.sessions, 1);

    let id = repo
        .claude_send("apps/desktop/src/main.tsx:42 why is this hidden?")
        .await
        .expect("the message reaches the session");
    assert_eq!(id, "msg-from-the-host");

    // And it actually arrived, rather than merely being accepted.
    let delivered = connection
        .run(&["cat", "/tmp/claude-received.txt"])
        .await
        .expect("read what the session received");
    assert!(
        delivered.contains("why is this hidden?"),
        "the host received: {delivered:?}"
    );

    // A repository with no session of its own is not offered the neighbour's.
    let other = Repository::remote("linux-box".into(), "/tmp".into(), transport.calls());
    assert!(
        !other.claude_status().await.connected,
        "a registration is matched against its own repository root"
    );

    connection
        .run_script("pkill -f fake-claude.py || true")
        .await
        .ok();
    transport.shutdown().await;
    connection.disconnect().await;
}
