//! Getting the right agent binary onto a host, and keeping it there.
//!
//! Matching is by filename, and the check is to run the file. If
//! `onlydiffs-agent-0.1.8-x86_64-unknown-linux-gnu-1a2b3c4d --version`
//! executes and prints `0.1.8`, that is the binary this app speaks to — no
//! handshake negotiation, no compatibility matrix, and no way for a
//! half-written upload to look like a working one.
//!
//! The trailing digest is the local binary's own content, and it is there
//! because a version number is not enough. Two builds of `0.1.8` that speak
//! different protocols are exactly what a working day produces, and naming
//! only by version means the first one uploaded wins until someone thinks to
//! bump a number — which cost an afternoon to a stale agent answering
//! `unknown variant` before this was added.
//!
//! The binary is uploaded rather than downloaded. There is no CDN to download
//! from, and uploading is what makes the feature work on a host with no
//! outbound internet at all — a jump box, an air-gapped builder — which is a
//! large share of the machines anyone wants this for.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;

use tokio::process::Command;

use crate::error::AppError;
use crate::services::ssh::connection::{shell_join, SshConnection};

/// The app's version is the agent's version. Nothing else would be true.
pub const AGENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Uploading a few megabytes over a slow link is not a 60-second operation.
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(600);

/// Distinguishes two uploads from one process, which two projects connecting to
/// one host at the same moment will produce.
static UPLOAD_SEQUENCE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Where agents live on a host, relative to the remote home directory. The same
/// `.onlydiffs` the app uses locally, so a host that is also somebody's
/// workstation has one directory rather than two.
pub const AGENT_DIR: &str = ".onlydiffs/agent";

/// The stable name the newest agent is reachable by, relative to the home
/// directory. `claude mcp add` is pointed at this once, and every install
/// afterwards moves it, so the registration never goes stale.
pub const CURRENT_LINK: &str = ".onlydiffs/agent/current";

/// The name the channel is registered under with Claude Code, on hosts and
/// locally alike.
pub const MCP_SERVER_NAME: &str = "onlydiffs";

/// FNV-1a over the binary. Not a security boundary — the file crosses an
/// authenticated connection and is verified by being run — just a cheap,
/// dependency-free way to tell one build from another.
pub(crate) fn digest(bytes: &[u8]) -> String {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let hash = bytes
        .iter()
        .fold(OFFSET, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(PRIME));
    format!("{:08x}", hash as u32 ^ (hash >> 32) as u32)
}

/// The name a given build is stored under. Every part of it is load-bearing:
/// the version is what a person reads, the triple stops a home directory shared
/// over NFS between an x86 host and an ARM one from serving the wrong binary,
/// and the digest is what makes a rebuild a different file rather than a
/// silently reused old one.
pub fn agent_filename(triple: &str, digest: &str) -> String {
    format!("onlydiffs-agent-{AGENT_VERSION}-{triple}-{digest}")
}

/// The absolute path of the agent on the host.
pub fn agent_path(home: &str, triple: &str, digest: &str) -> String {
    format!(
        "{}/{AGENT_DIR}/{}",
        home.trim_end_matches('/'),
        agent_filename(triple, digest)
    )
}

/// The bundle's `agents/` directory, as Tauri resolves it for this platform:
/// `Contents/Resources` on macOS, `/usr/lib/<app>` in a deb or AppImage, next
/// to the executable on Windows. Set once at startup, because only Tauri knows
/// the answer and only the app has Tauri.
static BUNDLED_AGENTS: OnceLock<PathBuf> = OnceLock::new();

pub fn set_bundled_agents_dir(dir: PathBuf) {
    let _ = BUNDLED_AGENTS.set(dir);
}

/// Where the bundled agents live on this machine.
///
/// The bundle's resource directory first, then beside the executable, then the
/// workspace's target directory in a dev build — so `tauri dev` can connect to
/// a host without a release build first.
pub(crate) fn local_agent(triple: &str) -> Result<PathBuf, AppError> {
    let name = format!("onlydiffs-agent-{triple}");
    let exe = std::env::current_exe()
        .map_err(|error| AppError::Ssh(format!("could not locate this binary: {error}")))?;
    let mut candidates = Vec::new();
    if let Some(dir) = BUNDLED_AGENTS.get() {
        candidates.push(dir.join(&name));
    }
    if let Some(dir) = exe.parent() {
        // macOS: Contents/MacOS/onlydiffs, agents in Contents/Resources/agents.
        candidates.push(dir.join("agents").join(&name));
        candidates.push(dir.join("../Resources/agents").join(&name));
        candidates.push(dir.join(&name));
    }
    // A dev build: `cargo build -p onlydiffs-agent --target <triple>`.
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let root = PathBuf::from(manifest);
        candidates.push(root.join("target").join(triple).join("release").join("onlydiffs-agent"));
        candidates.push(root.join("target").join(triple).join("debug").join("onlydiffs-agent"));
        candidates.push(root.join("target/release/onlydiffs-agent"));
        candidates.push(root.join("target/debug/onlydiffs-agent"));
    }

    // The newest of them. A dev checkout accumulates builds — a release
    // cross-compile from last week beside a debug build from a minute ago —
    // and they all pass the version check while speaking whatever protocol
    // they were built with. The one built last is the one that matches the
    // code running here.
    candidates
        .into_iter()
        .filter(|path| path.is_file())
        .max_by_key(|path| {
            std::fs::metadata(path)
                .and_then(|meta| meta.modified())
                .unwrap_or(std::time::UNIX_EPOCH)
        })
        .ok_or_else(|| {
            AppError::Ssh(format!(
                "this build carries no agent for {triple}. A release built by CI does; a local `tauri dev` needs `cargo build -p onlydiffs-agent --release` first."
            ))
        })
}

/// Whether the host already has exactly this version.
///
/// Running the file is the check. A truncated upload, a binary for the wrong
/// architecture, and a file that is not executable all fail it, which a
/// stat-and-compare would not.
async fn already_installed(connection: &SshConnection, path: &str) -> bool {
    match connection.run(&[path, "--version"]).await {
        Ok(printed) => printed.trim() == AGENT_VERSION,
        Err(_) => false,
    }
}

/// Puts the agent on the host if it is not already there, and answers with its
/// absolute path.
pub async fn ensure(connection: &SshConnection) -> Result<String, AppError> {
    let probe = connection.probe();
    let triple = probe.target_triple().ok_or_else(|| {
        AppError::Ssh(format!(
            "OnlyDiffs has no agent for {} {}. Supported hosts are Linux on x86_64 or aarch64, and macOS on Apple silicon or Intel.",
            probe.os, probe.arch
        ))
    })?;

    let source = local_agent(triple)?;
    let bytes = tokio::fs::read(&source).await.map_err(|error| {
        AppError::Ssh(format!("could not read {}: {error}", source.display()))
    })?;
    let destination = agent_path(&probe.home, triple, &digest(&bytes));
    if already_installed(connection, &destination).await {
        settle(connection, &probe.home, triple, &destination).await?;
        return Ok(destination);
    }

    let directory = format!("{}/{AGENT_DIR}", probe.home.trim_end_matches('/'));
    // 0700: the agent is executed by this user and by nobody else, on a host
    // that may well have other people on it.
    connection
        .run_script(&format!("mkdir -p {} && chmod 700 {}", shell_join(&[&directory]), shell_join(&[&directory])))
        .await?;

    // Uploaded under a temporary name and moved into place, so a connection
    // that drops mid-transfer leaves a partial file that nothing will run
    // rather than a partial file at the name the version check trusts.
    //
    // The name is unique per upload, not just per destination. Two projects on
    // one host connecting at the same moment would otherwise write the same
    // `.partial` over each other, and the `mv` that follows would publish
    // whichever interleaving won — which showed up as intermittently corrupt
    // agents the first time two connections raced.
    let staging = format!(
        "{destination}.partial.{}.{}",
        std::process::id(),
        UPLOAD_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    upload(connection, &bytes, &staging).await?;
    connection
        .run_script(&format!(
            "chmod 700 {staging} && mv -f {staging} {destination}",
            staging = shell_join(&[&staging]),
            destination = shell_join(&[&destination])
        ))
        .await?;

    // `mv` is atomic within a filesystem, so a concurrent upload of the same
    // build either has not landed yet or has landed completely; either way what
    // is at `destination` is a whole binary.
    if !already_installed(connection, &destination).await {
        return Err(AppError::Ssh(format!(
            "the agent uploaded to {} would not run. The host may be {triple} in name only.",
            connection.target().alias
        )));
    }
    settle(connection, &probe.home, triple, &destination).await?;
    Ok(destination)
}

/// What follows a verified install: `current` moves to it, and every other
/// build for this platform goes.
///
/// The link is replaced by a rename, so there is never a moment with no
/// `current`. The prune keeps only the file just verified; older versions have
/// nothing pointing at them any more and were four megabytes each — a host
/// this app had been used against for a day was holding six.
async fn settle(
    connection: &SshConnection,
    home: &str,
    triple: &str,
    destination: &str,
) -> Result<(), AppError> {
    let home = home.trim_end_matches('/');
    let directory = format!("{home}/{AGENT_DIR}");
    let link = format!("{home}/{CURRENT_LINK}");
    // Unique per call, not per process: two projects on one host connect at
    // the same moment from one app, and a shared staging name had one `mv`
    // taking the other's link out from under it.
    let staging = format!(
        "{link}.{}.{}",
        std::process::id(),
        UPLOAD_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let keep = destination.rsplit('/').next().unwrap_or(destination).to_owned();
    connection
        .run_script(&format!(
            "ln -sfn {destination} {staging} && mv -f {staging} {link} && \
             find {directory} -maxdepth 1 -type f -name {pattern} ! -name {keep} -delete",
            destination = shell_join(&[destination]),
            staging = shell_join(&[&staging]),
            link = shell_join(&[&link]),
            directory = shell_join(&[&directory]),
            pattern = shell_join(&[&format!("onlydiffs-agent-*-{triple}-*")]),
            keep = shell_join(&[&keep]),
        ))
        .await?;
    Ok(())
}

/// Registers the agent's channel mode with Claude Code on the host, so a
/// session opened there can be sent to. Answers whether it managed to.
///
/// `claude` is routinely somewhere only a login shell can find — `~/.bun/bin`,
/// `~/.local/bin` — so the usual places are tried by name before a login shell
/// is asked. Not registering is not a failure of the connection: the host is
/// still reviewable, it just cannot be talked to until someone registers by
/// hand, and the host list says so.
pub async fn register_claude_channel(connection: &SshConnection) -> bool {
    let script = format!(
        r#"c=$(command -v claude 2>/dev/null)
if [ -z "$c" ]; then
  for p in "$HOME/.bun/bin/claude" "$HOME/.local/bin/claude" "$HOME/.claude/local/claude" /usr/local/bin/claude /opt/homebrew/bin/claude; do
    if [ -x "$p" ]; then c="$p"; break; fi
  done
fi
if [ -z "$c" ] && [ -n "$SHELL" ]; then c=$("$SHELL" -lc 'command -v claude' 2>/dev/null </dev/null | tail -1); fi
if [ -z "$c" ]; then echo onlydiffs-no-claude; exit 0; fi
if "$c" mcp get {name} 2>/dev/null | grep -q "{link}"; then echo onlydiffs-registered; exit 0; fi
"$c" mcp remove --scope user {name} >/dev/null 2>&1 || true
if "$c" mcp add --scope user {name} -- "$HOME/{link}" channel >/dev/null 2>&1; then echo onlydiffs-registered; else echo onlydiffs-register-failed; fi"#,
        name = MCP_SERVER_NAME,
        link = CURRENT_LINK,
    );
    match connection.run_script(&script).await {
        Ok(output) => output.contains("onlydiffs-registered"),
        Err(_) => false,
    }
}

/// Sends the binary over the connection that is already open.
///
/// `cat > file` through the multiplexed connection rather than `scp`: it needs
/// no second authentication, no `scp` on the host, and works identically
/// whether the host has OpenSSH 8 or 9 — `scp`'s protocol changed to SFTP in
/// between and the old flag spelling is deprecated on some builds and absent on
/// others.
async fn upload(
    connection: &SshConnection,
    bytes: &[u8],
    destination: &str,
) -> Result<(), AppError> {
    let bytes = bytes.to_vec();
    let mut command = connection.command(&format!("cat > {}", shell_join(&[destination])));
    command.stdin(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| AppError::Ssh(format!("could not start the upload: {error}")))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| AppError::Ssh("the upload had no stdin".into()))?;

    let write = async move {
        use tokio::io::AsyncWriteExt;
        stdin.write_all(&bytes).await?;
        stdin.shutdown().await
    };

    let finished = async {
        let (written, output) = tokio::join!(write, child.wait_with_output());
        written.map_err(|error| AppError::Ssh(format!("the upload failed: {error}")))?;
        output.map_err(|error| AppError::Ssh(format!("the upload failed: {error}")))
    };

    let output = match tokio::time::timeout(UPLOAD_TIMEOUT, finished).await {
        Ok(result) => result?,
        Err(_) => {
            return Err(AppError::Ssh(format!(
                "uploading the agent to {} did not finish within {}s.",
                connection.target().alias,
                UPLOAD_TIMEOUT.as_secs()
            )))
        }
    };

    if !output.status.success() {
        return Err(AppError::Ssh(format!(
            "could not write the agent to {}: {}",
            destination,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

/// A `Command` that runs the agent and speaks the protocol on its stdio.
///
/// `exec` matters: without it the login shell stays in the process tree between
/// `ssh` and the agent, and a signal — or a closed stdin — reaches the shell
/// rather than the process that needs to hear it.
pub fn serve_command(connection: &SshConnection, agent_path: &str) -> Command {
    let mut command = connection.command(&format!("exec {} serve", shell_join(&[agent_path])));
    command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    command
}

#[cfg(test)]
mod tests {
    use super::{agent_filename, agent_path, digest, AGENT_VERSION};

    const TRIPLE: &str = "x86_64-unknown-linux-gnu";

    #[test]
    fn the_filename_carries_the_version_the_platform_and_the_build() {
        let name = agent_filename(TRIPLE, "1a2b3c4d");

        assert!(name.contains(AGENT_VERSION), "{name}");
        assert!(name.contains(TRIPLE), "{name}");
        assert!(name.ends_with("1a2b3c4d"), "{name}");
    }

    #[test]
    fn two_architectures_sharing_a_home_directory_do_not_share_a_binary() {
        // An NFS home mounted on both an x86 builder and an ARM one is a real
        // arrangement, and the wrong binary there fails in a confusing way.
        assert_ne!(
            agent_path("/home/me", TRIPLE, "1a2b3c4d"),
            agent_path("/home/me", "aarch64-unknown-linux-gnu", "1a2b3c4d")
        );
    }

    #[test]
    fn rebuilding_without_bumping_the_version_still_uploads() {
        // The bug this exists to prevent: a same-version rebuild that changed
        // the protocol, reused because the name had not changed.
        assert_ne!(
            agent_path("/home/me", TRIPLE, &digest(b"old build")),
            agent_path("/home/me", TRIPLE, &digest(b"new build"))
        );
    }

    #[test]
    fn an_unchanged_binary_keeps_its_name_so_it_is_uploaded_once() {
        assert_eq!(digest(b"same bytes"), digest(b"same bytes"));
        assert_eq!(digest(b"same bytes").len(), 8);
    }

    #[test]
    fn a_trailing_slash_on_the_home_directory_does_not_double_up() {
        assert_eq!(
            agent_path("/home/me/", TRIPLE, "1a2b3c4d"),
            agent_path("/home/me", TRIPLE, "1a2b3c4d")
        );
        assert!(!agent_path("/home/me/", TRIPLE, "1a2b3c4d").contains("//"));
    }
}
