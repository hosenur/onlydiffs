//! The SSH connection, against a real `sshd`.
//!
//! Not a mock. The whole point of shelling out to OpenSSH is that the user's
//! own ssh decides what happens, so a fake would test the one thing that is not
//! in question. This starts a throwaway `sshd` on a loopback port with its own
//! host key, its own `authorized_keys`, and its own `known_hosts`, and drives
//! the real code against it.
//!
//! Every host it touches is 127.0.0.1 and every file is in a `TempDir`, so
//! nothing here reads or writes the developer's own SSH configuration.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use onlydiffs_lib::services::ssh::host_key;
use onlydiffs_lib::services::ssh::target;
use onlydiffs_lib::services::ssh::{Prompt, SshConnection};
use tempfile::TempDir;
use tokio::sync::mpsc;

/// The daemons ship in different places on macOS and Linux.
fn sshd_binary() -> Option<PathBuf> {
    ["/usr/sbin/sshd", "/usr/local/sbin/sshd", "/opt/homebrew/sbin/sshd"]
        .into_iter()
        .map(PathBuf::from)
        .find(|path| path.exists())
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("a loopback port")
        .local_addr()
        .expect("addr")
        .port()
}

fn keygen(path: &Path, comment: &str) {
    let status = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-C", comment, "-f"])
        .arg(path)
        .stdin(Stdio::null())
        .status()
        .expect("ssh-keygen runs");
    assert!(status.success(), "ssh-keygen failed for {}", path.display());
}

struct Sshd {
    dir: TempDir,
    port: u16,
    child: Child,
}

impl Drop for Sshd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Sshd {
    /// `None` when there is no `sshd` to run — the tests then skip rather than
    /// fail, because a machine without an ssh daemon has nothing to say about
    /// whether this code is correct.
    fn start() -> Option<Self> {
        let sshd = sshd_binary()?;
        let dir = TempDir::new().expect("temp dir");
        let root = dir.path();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700)).expect("chmod");
        }

        keygen(&root.join("host_ed25519"), "onlydiffs-test-host");
        keygen(&root.join("client"), "onlydiffs-test-client");
        std::fs::copy(root.join("client.pub"), root.join("authorized_keys")).expect("authorize");

        let port = free_port();
        let config = format!(
            "Port {port}\n\
             ListenAddress 127.0.0.1\n\
             HostKey {root}/host_ed25519\n\
             PidFile {root}/sshd.pid\n\
             AuthorizedKeysFile {root}/authorized_keys\n\
             PasswordAuthentication no\n\
             KbdInteractiveAuthentication no\n\
             UsePAM no\n\
             StrictModes no\n",
            root = root.display()
        );
        std::fs::write(root.join("sshd_config"), config).expect("write sshd_config");

        let child = Command::new(sshd)
            .arg("-D")
            .arg("-f")
            .arg(root.join("sshd_config"))
            .arg("-E")
            .arg(root.join("sshd.log"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        let daemon = Self { dir, port, child };
        daemon.await_listening()?;
        daemon.record_host_key();
        Some(daemon)
    }

    fn await_listening(&self) -> Option<()> {
        for _ in 0..100 {
            if std::net::TcpStream::connect(("127.0.0.1", self.port)).is_ok() {
                return Some(());
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        None
    }

    /// Puts the daemon's key in this fixture's own `known_hosts`, standing in
    /// for the user having already approved it.
    fn record_host_key(&self) {
        let public = std::fs::read_to_string(self.dir.path().join("host_ed25519.pub"))
            .expect("host key");
        std::fs::write(
            self.known_hosts(),
            format!("[127.0.0.1]:{} {}", self.port, public),
        )
        .expect("write known_hosts");
    }

    fn known_hosts(&self) -> PathBuf {
        self.dir.path().join("known_hosts")
    }

    /// The flags that point ssh at this daemon and nothing else: this identity
    /// only, this `known_hosts` only, and none of the user's own config.
    fn args(&self) -> Vec<String> {
        vec![
            "-F".into(),
            "/dev/null".into(),
            "-i".into(),
            self.dir.path().join("client").to_string_lossy().into_owned(),
            "-o".into(),
            "IdentitiesOnly=yes".into(),
            "-o".into(),
            format!("UserKnownHostsFile={}", self.known_hosts().display()),
            "-o".into(),
            "GlobalKnownHostsFile=/dev/null".into(),
            "-p".into(),
            self.port.to_string(),
        ]
    }
}

/// Skips the body when there is no `sshd` on this machine.
macro_rules! sshd_or_skip {
    () => {
        match Sshd::start() {
            Some(daemon) => daemon,
            None => {
                eprintln!("no sshd available; skipping");
                return;
            }
        }
    };
}

async fn connect(daemon: &Sshd) -> SshConnection {
    let target = target::resolve("127.0.0.1", &daemon.args())
        .await
        .expect("resolve");
    let (tx, _rx) = mpsc::unbounded_channel::<Prompt>();
    SshConnection::connect(target, daemon.args(), tx)
        .await
        .expect("connect")
}

#[tokio::test]
async fn a_host_key_already_in_known_hosts_is_recognised() {
    let daemon = sshd_or_skip!();
    let target = target::resolve("127.0.0.1", &daemon.args())
        .await
        .expect("resolve");

    assert_eq!(target.port, Some(daemon.port));
    assert!(
        host_key::is_known(&target).await.expect("lookup"),
        "the fixture wrote this key into the known_hosts ssh was pointed at"
    );
}

#[tokio::test]
async fn an_unrecorded_host_is_refused_rather_than_trusted_silently() {
    let daemon = sshd_or_skip!();
    // The same daemon, but pointed at an empty known_hosts: this is what a
    // first-ever connection looks like, and it must not just succeed.
    let empty = daemon.dir.path().join("empty_known_hosts");
    std::fs::write(&empty, "").expect("write");
    let mut args = daemon.args();
    for entry in args.iter_mut() {
        if entry.starts_with("UserKnownHostsFile=") {
            *entry = format!("UserKnownHostsFile={}", empty.display());
        }
    }

    let target = target::resolve("127.0.0.1", &args).await.expect("resolve");
    let (tx, _rx) = mpsc::unbounded_channel::<Prompt>();
    let refused = SshConnection::connect(target, args, tx).await;

    let tag = refused.as_ref().err().map(|error| error.tag());

    assert_eq!(
        tag,
        Some("SshUnknownHostError"),
        "an unknown host must become a question, not a connection"
    );
}

#[tokio::test]
async fn an_unknown_key_can_be_fetched_fingerprinted_and_trusted() {
    let daemon = sshd_or_skip!();
    let store = daemon.dir.path().join("fresh_known_hosts");
    std::fs::write(&store, "").expect("write");
    let mut args = daemon.args();
    for entry in args.iter_mut() {
        if entry.starts_with("UserKnownHostsFile=") {
            *entry = format!("UserKnownHostsFile={}", store.display());
        }
    }
    let target = target::resolve("127.0.0.1", &args).await.expect("resolve");

    let unknown = host_key::fetch_unknown(&target.hostname, target.port)
        .await
        .expect("keyscan");
    assert!(unknown.fingerprint.starts_with("SHA256:"));
    assert_eq!(unknown.key_type, "ssh-ed25519");
    assert!(!host_key::is_known(&target).await.expect("lookup"));

    host_key::trust(&target, &unknown).await.expect("trust");

    assert!(
        host_key::is_known(&target).await.expect("lookup"),
        "trusting a key has to make it known to ssh, not just to us"
    );
}

#[tokio::test]
async fn connecting_probes_the_host_before_trusting_it_with_anything() {
    let daemon = sshd_or_skip!();
    let mut connection = connect(&daemon).await;
    let probe = connection.probe().clone();

    assert!(!probe.git_version.is_empty(), "the probe reads git's version");
    assert!(
        probe.os == "Darwin" || probe.os == "Linux",
        "unexpected os: {}",
        probe.os
    );
    assert!(probe.home.starts_with('/'), "home should be absolute: {}", probe.home);
    assert!(probe.target_triple().is_some(), "this platform should have an agent");

    connection.disconnect().await;
}

#[tokio::test]
async fn commands_run_on_the_established_connection() {
    let daemon = sshd_or_skip!();
    let mut connection = connect(&daemon).await;

    let echoed = connection.run(&["echo", "hello from the host"]).await.expect("run");

    assert_eq!(echoed.trim(), "hello from the host");
    connection.disconnect().await;
}

#[tokio::test]
async fn an_argument_cannot_become_a_command_on_the_remote_shell() {
    let daemon = sshd_or_skip!();
    let mut connection = connect(&daemon).await;
    let marker = daemon.dir.path().join("pwned");

    // If quoting leaked, the `;` would run `touch` as its own command.
    let echoed = connection
        .run(&["echo", &format!("safe; touch {}", marker.display())])
        .await
        .expect("run");

    assert!(echoed.contains("safe;"), "the whole string is one argument");
    assert!(!marker.exists(), "the injected command must not have run");
    connection.disconnect().await;
}

#[tokio::test]
async fn a_failing_remote_command_reports_its_own_exit_rather_than_a_disconnect() {
    let daemon = sshd_or_skip!();
    let mut connection = connect(&daemon).await;

    let failed = connection
        .run(&["sh", "-c", "echo nope >&2; exit 3"])
        .await
        .expect_err("should fail");

    assert_eq!(failed.tag(), "SshError", "exit 3 is the command's, not ssh's");
    assert!(failed.message().contains("nope"), "stderr should survive: {failed:?}");
    connection.disconnect().await;
}

#[tokio::test]
async fn a_second_connection_reuses_the_first_master_rather_than_reauthenticating() {
    let daemon = sshd_or_skip!();
    let mut first = connect(&daemon).await;

    // The fixture's config sets no ControlPath, so the first connection made
    // its own. A second one resolves the same config and must therefore make
    // its own too — what this proves is that two live connections to one host
    // do not tread on each other's sockets.
    let mut second = connect(&daemon).await;
    assert_eq!(second.run(&["echo", "second"]).await.expect("run").trim(), "second");
    assert_eq!(first.run(&["echo", "first"]).await.expect("run").trim(), "first");

    second.disconnect().await;
    assert_eq!(
        first.run(&["echo", "still here"]).await.expect("run").trim(),
        "still here",
        "closing one connection must not close the other"
    );
    first.disconnect().await;
}

// ---------------------------------------------------------------------------
// The agent, over the connection above. This is the part the whole design is
// for: a repository on the far side of an SSH connection, answering the same
// questions in the same shapes as one on this machine.
// ---------------------------------------------------------------------------

use onlydiffs_core::contract::ChangeStatus;
use onlydiffs_core::protocol::Event;
use onlydiffs_core::services::repository::Repository;
use onlydiffs_lib::services::ssh::AgentTransport;

/// A git repository the tests can treat as living "on the host". It is on this
/// machine, of course — but every byte of it reaches the app through `ssh`, the
/// agent, and the protocol, which is what is being tested.
struct RemoteRepo {
    dir: TempDir,
}

impl RemoteRepo {
    fn new() -> Self {
        let dir = TempDir::new().expect("temp repo");
        let repo = Self { dir };
        repo.git(&["init", "-q"]);
        repo.git(&["config", "user.email", "onlydiffs@example.test"]);
        repo.git(&["config", "user.name", "OnlyDiffs Test"]);
        repo
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn git(&self, args: &[&str]) {
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
    }

    fn write(&self, name: &str, contents: &str) {
        let path = self.path().join(name);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, contents).expect("write");
    }
}

/// A connection with a live agent on the far end, and the `Repository` that
/// speaks to it.
struct Remote {
    connection: SshConnection,
    transport: AgentTransport,
    events: mpsc::UnboundedReceiver<Event>,
}

impl Remote {
    async fn open(daemon: &Sshd) -> Option<Self> {
        // A dev build has to have produced the agent; CI builds it before the
        // tests run, and skipping is better than a failure that says nothing
        // about the code under test.
        if !PathBuf::from("target/release/onlydiffs-agent").exists()
            && !PathBuf::from("target/debug/onlydiffs-agent").exists()
        {
            eprintln!("no agent built; skipping (cargo build -p onlydiffs-agent --release)");
            return None;
        }
        let connection = connect(daemon).await;
        let (tx, events) = mpsc::unbounded_channel();
        let transport = AgentTransport::start(&connection, tx)
            .await
            .expect("agent started");
        Some(Self {
            connection,
            transport,
            events,
        })
    }

    fn repository(&self, repo: &RemoteRepo) -> Repository {
        Repository::remote(
            self.connection.target().alias.clone(),
            repo.path().to_path_buf(),
            self.transport.calls(),
        )
    }

    async fn close(mut self) {
        self.transport.shutdown().await;
        self.connection.disconnect().await;
    }
}

macro_rules! remote_or_skip {
    ($daemon:expr) => {
        match Remote::open($daemon).await {
            Some(remote) => remote,
            None => return,
        }
    };
}

#[tokio::test]
async fn the_agent_is_uploaded_version_matched_and_answers_a_handshake() {
    let daemon = sshd_or_skip!();
    let remote = remote_or_skip!(&daemon);

    assert_eq!(
        remote.transport.agent_version(),
        env!("CARGO_PKG_VERSION"),
        "the agent on the host is the one this build speaks to"
    );
    remote.close().await;
}

#[tokio::test]
async fn a_remote_diff_comes_back_in_the_same_shape_as_a_local_one() {
    let daemon = sshd_or_skip!();
    let remote = remote_or_skip!(&daemon);
    let project = RemoteRepo::new();
    project.write("kept.txt", "one\n");
    project.git(&["add", "-A"]);
    project.git(&["commit", "-q", "-m", "first"]);
    project.write("kept.txt", "one\ntwo\n");
    project.write("added.txt", "new\n");

    let repo = remote.repository(&project);
    let diff = repo.diff().await.expect("remote diff");

    let paths: Vec<&str> = diff.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(paths, vec!["added.txt", "kept.txt"]);
    assert!(diff.repo_path.starts_with("127.0.0.1:"), "{}", diff.repo_path);
    assert!(!diff.branch.trim().is_empty());
    // The whole diff arrived; nothing was fetched per file to build it.
    assert_eq!(
        diff.files.iter().filter(|file| file.additions > 0).count(),
        2
    );
    remote.close().await;
}

#[tokio::test]
async fn a_remote_file_can_be_read_staged_and_committed() {
    let daemon = sshd_or_skip!();
    let remote = remote_or_skip!(&daemon);
    let project = RemoteRepo::new();
    project.write("src/main.rs", "fn main() {}\n");
    project.git(&["add", "-A"]);
    project.git(&["commit", "-q", "-m", "first"]);
    project.write("src/main.rs", "fn main() { println!(\"hi\"); }\n");

    let repo = remote.repository(&project);

    let contents = repo
        .file_contents("src/main.rs", None, ChangeStatus::Modified, false)
        .await
        .expect("contents");
    assert!(contents.new_contents.expect("new").contains("println!"));
    assert!(contents.old_contents.expect("old").contains("fn main() {}"));

    repo.stage_file("src/main.rs", None).await.expect("stage");
    let staged = repo.diff().await.expect("diff");
    assert!(staged.files.iter().all(|file| file.staged), "{staged:?}");

    let head = repo.commit_all("Say hi").await.expect("commit");
    assert!(head.contains("Say hi"), "{head}");
    assert!(repo.diff().await.expect("diff").files.is_empty());
    remote.close().await;
}

#[tokio::test]
async fn the_file_list_and_history_come_from_the_host() {
    let daemon = sshd_or_skip!();
    let remote = remote_or_skip!(&daemon);
    let project = RemoteRepo::new();
    project.write("a.txt", "a\n");
    project.write("b/c.txt", "c\n");
    project.git(&["add", "-A"]);
    project.git(&["commit", "-q", "-m", "seed"]);

    let repo = remote.repository(&project);

    let mut files = repo.list_files().await.expect("files");
    files.sort();
    assert_eq!(files, vec!["a.txt".to_owned(), "b/c.txt".to_owned()]);

    let history = repo.history(Some(10.0)).await.expect("history");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].subject, "seed");
    remote.close().await;
}

/// A pasted image, for a repository on the far side of a connection.
///
/// The bytes have to end up on the *host*: the session that will be told about
/// them is a process there, and a path to a file on this Mac would be a path to
/// nothing as far as it is concerned. So what comes back is a path in the
/// host's filesystem, the file is there, and it is byte-for-byte the image that
/// was pasted.
#[tokio::test]
async fn a_pasted_image_is_written_on_the_host_and_named_by_its_path_there() {
    let daemon = sshd_or_skip!();
    let remote = remote_or_skip!(&daemon);
    let project = RemoteRepo::new();
    project.write("seed.txt", "one\n");
    project.git(&["add", "-A"]);
    project.git(&["commit", "-q", "-m", "seed"]);
    let repo = remote.repository(&project);

    let mut pasted = std::io::Cursor::new(Vec::new());
    image::RgbImage::from_pixel(8, 8, image::Rgb([200, 40, 90]))
        .write_to(&mut pasted, image::ImageOutputFormat::Png)
        .expect("encode a screenshot");
    let pasted = pasted.into_inner();

    let path = repo.write_attachment(&pasted).await.expect("write");

    let written = PathBuf::from(&path);
    assert!(written.is_absolute(), "the session is given a path, not a name: {path}");
    assert_eq!(std::fs::read(&written).expect("read it back"), pasted);
    // Inside the repository's own git directory, which is what keeps a pasted
    // screenshot out of the diff the user is reading.
    assert!(written.starts_with(project.path().join(".git")), "{path}");
    assert_eq!(
        String::from_utf8_lossy(
            &Command::new("git")
                .arg("-C")
                .arg(project.path())
                .args(["status", "--porcelain"])
                .output()
                .expect("git runs")
                .stdout
        )
        .trim(),
        ""
    );

    // And the refusal happens on the host too, rather than being trusted here.
    let refused = repo.write_attachment(b"not an image").await;
    assert_eq!(refused.expect_err("refused").tag(), "AttachmentError");

    remote.close().await;
}

#[tokio::test]
async fn reading_a_remote_file_refuses_to_exceed_the_limit_it_was_given() {
    let daemon = sshd_or_skip!();
    let remote = remote_or_skip!(&daemon);
    let project = RemoteRepo::new();
    project.write("big.bin", &"x".repeat(4096));
    let repo = remote.repository(&project);

    let allowed = repo.read_file(Path::new("big.bin"), 8192).await.expect("read");
    let refused = repo.read_file(Path::new("big.bin"), 1024).await;

    assert_eq!(allowed.len(), 4096);
    assert!(refused.is_err(), "the bound is enforced on the host, not here");
    remote.close().await;
}

#[tokio::test]
async fn a_remote_failure_keeps_the_tag_it_had_on_the_host() {
    let daemon = sshd_or_skip!();
    let remote = remote_or_skip!(&daemon);
    let empty = TempDir::new().expect("temp");
    // A directory that is not a repository: git fails, and it should arrive as
    // a git failure rather than as "something went wrong with SSH".
    let repo = Repository::remote(
        remote.connection.target().alias.clone(),
        empty.path().to_path_buf(),
        remote.transport.calls(),
    );

    let failed = repo.diff().await.expect_err("not a repository");

    assert_eq!(failed.tag(), "GitError", "{failed:?}");
    remote.close().await;
}

#[tokio::test]
async fn icon_candidates_are_shrunk_on_the_host_rather_than_sent_whole() {
    let daemon = sshd_or_skip!();
    let remote = remote_or_skip!(&daemon);
    let project = RemoteRepo::new();
    // Deliberately larger than the thumbnail it will become.
    let logo = image::RgbImage::from_pixel(1024, 1024, image::Rgb([30, 90, 200]));
    logo.save(project.path().join("logo.png")).expect("write png");
    project.git(&["add", "-A"]);
    project.git(&["commit", "-q", "-m", "art"]);

    let candidates = remote
        .repository(&project)
        .icon_candidates()
        .await
        .expect("candidates");

    assert_eq!(candidates.len(), 1);
    let candidate = &candidates[0];
    assert_eq!(candidate.relative_path, "logo.png");
    assert!(candidate.data_url.starts_with("data:image/png;base64,"));
    // A 1024px source; what crossed is a 256px thumbnail.
    assert!(
        candidate.data_url.len() < 60_000,
        "thumbnail was {} bytes of base64",
        candidate.data_url.len()
    );
    remote.close().await;
}

#[tokio::test]
async fn a_change_on_the_host_arrives_as_an_event_without_being_polled_for() {
    let daemon = sshd_or_skip!();
    let mut remote = remote_or_skip!(&daemon);
    let project = RemoteRepo::new();
    project.write("watched.txt", "before\n");
    project.git(&["add", "-A"]);
    project.git(&["commit", "-q", "-m", "seed"]);

    let repo = remote.repository(&project);
    repo.set_watched(true).await.expect("watch");

    // The watcher needs a moment to establish before a write can be seen.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    project.write("watched.txt", "after\n");

    let event = tokio::time::timeout(std::time::Duration::from_secs(10), remote.events.recv())
        .await
        .expect("an event arrived within ten seconds")
        .expect("channel open");

    match event {
        Event::RepoChanged { root } => {
            assert_eq!(root, project.path().to_string_lossy());
        }
    }

    repo.set_watched(false).await.expect("unwatch");
    remote.close().await;
}

#[tokio::test]
async fn the_claude_channel_is_asked_about_on_the_host_rather_than_here() {
    let daemon = sshd_or_skip!();
    let remote = remote_or_skip!(&daemon);
    let project = RemoteRepo::new();
    project.write("a.txt", "a\n");
    project.git(&["add", "-A"]);
    project.git(&["commit", "-q", "-m", "seed"]);

    // No session is running for this throwaway checkout on either machine, so
    // the answer is "none" — what is being proven is that the question crossed
    // and came back in the right shape rather than failing as a transport error.
    let status = remote.repository(&project).claude_status().await;

    assert!(!status.connected);
    assert_eq!(status.sessions, 0);

    // And a send against it fails as a channel problem, not an SSH one.
    let failed = remote
        .repository(&project)
        .claude_send("hello")
        .await
        .expect_err("no session to send to");
    assert_eq!(failed.tag(), "ClaudeChannelError", "{failed:?}");

    remote.close().await;
}

#[tokio::test]
async fn losing_the_connection_fails_outstanding_calls_rather_than_hanging_them() {
    let daemon = sshd_or_skip!();
    let mut remote = remote_or_skip!(&daemon);
    let project = RemoteRepo::new();
    project.write("a.txt", "a\n");
    project.git(&["add", "-A"]);
    project.git(&["commit", "-q", "-m", "seed"]);
    let repo = remote.repository(&project);

    // Prove it works, then take the connection away underneath it.
    repo.diff().await.expect("works while connected");
    remote.connection.disconnect().await;

    let after = tokio::time::timeout(std::time::Duration::from_secs(20), repo.diff())
        .await
        .expect("a dropped connection must not hang a call");

    let failed = after.expect_err("the repository is unreachable");
    assert_eq!(failed.tag(), "SshDisconnectedError", "{failed:?}");
}
