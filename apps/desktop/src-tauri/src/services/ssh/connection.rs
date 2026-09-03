//! One multiplexed SSH connection to a host, and the commands that ride it.
//!
//! Authentication happens once. Everything after it — the probe, the agent
//! upload, the agent itself — reuses that authenticated channel through
//! OpenSSH's `ControlMaster`, so a host behind a hardware key or 2FA is touched
//! once per connection rather than once per command.
//!
//! The user's own master is adopted where they have one. That is not
//! politeness: a developer with `ControlPersist` configured has already paid
//! for a connection, and opening a second one would ask them for the same
//! second factor again.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use crate::error::AppError;
use crate::services::ssh::askpass::{self, AskpassServer, Prompt, ASKPASS_SOCKET_ENV};
use crate::services::ssh::host_key;
use crate::services::ssh::target::SshTarget;

/// How long the master gets to authenticate once no prompt is outstanding.
/// Generous, because a hardware key is a human pressing a button.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(120);
/// How long a single non-interactive command on an established connection gets.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(60);

/// What the app learned about a host before trusting it with anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostProbe {
    /// e.g. `git version 2.43.0`, reduced to `2.43.0`.
    pub git_version: String,
    /// `uname -s`, e.g. `Linux` or `Darwin`.
    pub os: String,
    /// `uname -m`, e.g. `x86_64` or `arm64`.
    pub arch: String,
    /// The absolute path of the remote home directory, resolved once so every
    /// later path can be built without another round trip.
    pub home: String,
}

impl HostProbe {
    /// The Rust target triple whose agent binary will run here, or `None` for a
    /// platform this app has no agent for.
    pub fn target_triple(&self) -> Option<&'static str> {
        match (self.os.as_str(), self.arch.as_str()) {
            ("Linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
            ("Linux", "aarch64" | "arm64") => Some("aarch64-unknown-linux-gnu"),
            ("Darwin", "arm64") => Some("aarch64-apple-darwin"),
            ("Darwin", "x86_64") => Some("x86_64-apple-darwin"),
            _ => None,
        }
    }
}

/// Whether we started the master, and so whether we should stop it.
enum Master {
    /// The user's own, adopted. Killing it would close connections we never
    /// opened, so this variant never does.
    Adopted,
    Owned(Child),
}

pub struct SshConnection {
    target: SshTarget,
    extra_args: Vec<String>,
    control_path: PathBuf,
    master: Master,
    probe: HostProbe,
    /// Kept alive for the connection's lifetime: ssh may prompt again when a
    /// key is removed from the agent mid-session.
    _askpass: AskpassServer,
    /// The socket directory is a `TempDir` owned by the askpass server, so this
    /// only has to outlive the connection, not be cleaned up separately.
    _helper: Arc<PathBuf>,
}

impl SshConnection {
    pub fn target(&self) -> &SshTarget {
        &self.target
    }

    pub fn probe(&self) -> &HostProbe {
        &self.probe
    }

    /// Opens a connection, prompting through `prompts` for anything ssh asks.
    ///
    /// The host key is settled first, deliberately: an unknown host must be a
    /// question with a fingerprint in it, not a silent `accept-new`.
    pub async fn connect(
        target: SshTarget,
        extra_args: Vec<String>,
        prompts: mpsc::UnboundedSender<Prompt>,
    ) -> Result<Self, AppError> {
        if !host_key::is_known(&target).await? {
            return Err(AppError::SshUnknownHost(target.hostname.clone()));
        }

        let helper = askpass::helper_binary()?;
        let askpass_server = AskpassServer::start(prompts)?;

        let (control_path, master) =
            match adopt_existing(&target, &extra_args).await {
                Some(path) => (path, Master::Adopted),
                None => {
                    let dir = askpass_server.socket_path().parent().ok_or_else(|| {
                        AppError::Ssh("askpass socket has no directory".into())
                    })?;
                    // Short, because a unix socket path is capped near 104
                    // bytes on macOS and a long temp path plus a long host
                    // would silently exceed it.
                    let path = dir.join("cm.sock");
                    let child = spawn_master(
                        &target,
                        &extra_args,
                        &path,
                        &helper,
                        askpass_server.socket_path(),
                    )?;
                    (path, Master::Owned(child))
                }
            };

        let connection = Self {
            target,
            extra_args,
            control_path,
            master,
            probe: HostProbe {
                git_version: String::new(),
                os: String::new(),
                arch: String::new(),
                home: String::new(),
            },
            _askpass: askpass_server,
            _helper: helper,
        };

        connection.await_master().await?;
        let probe = connection.run_probe().await?;
        Ok(Self { probe, ..connection })
    }

    /// Waits for the control socket to answer. `ssh -O check` is the only
    /// honest readiness signal: the master prints nothing on success, and
    /// watching its stderr would mean parsing free text in several languages.
    async fn await_master(&self) -> Result<(), AppError> {
        let deadline = tokio::time::Instant::now() + CONNECT_TIMEOUT;
        loop {
            if control_socket_alive(&self.target, &self.extra_args, &self.control_path).await {
                return Ok(());
            }
            if let Master::Owned(_) = &self.master {
                // A master that exited without a live socket has failed, and
                // its stderr is the only thing that says why.
                if let Some(failure) = self.master_failure().await {
                    return Err(failure);
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(AppError::Ssh(format!(
                    "{} did not accept a connection within {}s.",
                    self.target.alias,
                    CONNECT_TIMEOUT.as_secs()
                )));
            }
            tokio::time::sleep(Duration::from_millis(120)).await;
        }
    }

    /// The master's exit and stderr, if it has given up.
    async fn master_failure(&self) -> Option<AppError> {
        let Master::Owned(child) = &self.master else {
            return None;
        };
        // `try_wait` needs &mut; the child is behind &self here, so this asks
        // the OS directly instead of reaping.
        let pid = child.id()?;
        let alive = unsafe { libc::kill(pid as libc::pid_t, 0) } == 0;
        if alive {
            return None;
        }
        Some(AppError::Ssh(format!(
            "ssh could not connect to {}. Run `ssh {}` in a terminal to see why.",
            self.target.alias, self.target.alias
        )))
    }

    /// Runs one command on the established connection.
    ///
    /// `--` is not enough to make this safe, because the remote side runs a
    /// shell: every argument is quoted for `sh` before it is sent.
    pub async fn run(&self, argv: &[&str]) -> Result<String, AppError> {
        let script = shell_join(argv);
        self.run_script(&script).await
    }

    /// Runs a shell fragment on the host. Callers that need pipelines or
    /// redirection use this; everything else uses `run`, which quotes for them.
    pub async fn run_script(&self, script: &str) -> Result<String, AppError> {
        let output = self
            .command(script)
            .output();

        let output = match tokio::time::timeout(COMMAND_TIMEOUT, output).await {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => return Err(AppError::Ssh(format!("could not run ssh: {error}"))),
            Err(_) => {
                return Err(AppError::Ssh(format!(
                    "a command on {} did not finish within {}s.",
                    self.target.alias,
                    COMMAND_TIMEOUT.as_secs()
                )))
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let code = output.status.code().unwrap_or(-1);
            // 255 is ssh's own "the connection failed", as opposed to the
            // remote command having exited non-zero.
            return Err(if code == 255 {
                AppError::SshDisconnected(format!(
                    "the connection to {} dropped: {}",
                    self.target.alias,
                    stderr.trim()
                ))
            } else {
                AppError::Ssh(format!(
                    "`{script}` failed on {} ({code}): {}",
                    self.target.alias,
                    stderr.trim()
                ))
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// An `ssh` invocation on this connection, ready to spawn. Public so the
    /// agent transport can take the stdio of one rather than its output.
    pub fn command(&self, script: &str) -> Command {
        let mut command = Command::new("ssh");
        command
            .args(&self.extra_args)
            .args(base_args())
            .arg("-o")
            .arg(format!("ControlPath={}", self.control_path.display()))
            // The master owns authentication; a command that finds no master
            // should fail rather than quietly open a second connection and ask
            // for a password the user has already given.
            .args(["-o", "ControlMaster=no", "-o", "BatchMode=yes"]);
        if let Some(port) = self.target.port {
            command.args(["-p", &port.to_string()]);
        }
        command
            .arg(self.target.destination())
            .arg(script)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        command
    }

    /// Everything the app needs to know before it will upload anything.
    async fn run_probe(&self) -> Result<HostProbe, AppError> {
        // One round trip, four answers, NUL-separated so no value can be
        // confused with the separator. `command -v` rather than `which`, which
        // is not POSIX and lies about builtins on some shells.
        let script = "command -v git >/dev/null 2>&1 || { echo onlydiffs-no-git >&2; exit 78; }; \
                      printf '%s\\0%s\\0%s\\0%s' \"$(git --version)\" \"$(uname -s)\" \"$(uname -m)\" \"$HOME\"";
        let output = self.run_script(script).await.map_err(|error| {
            if error.message().contains("onlydiffs-no-git") || error.message().contains("(78)") {
                AppError::Ssh(format!(
                    "git is not on the PATH of a non-interactive shell on {}. \
                     Check with: ssh {} 'command -v git'",
                    self.target.alias, self.target.alias
                ))
            } else {
                error
            }
        })?;

        let mut fields = output.split('\0');
        let git_version = fields
            .next()
            .unwrap_or("")
            .trim()
            .trim_start_matches("git version ")
            .to_owned();
        let os = fields.next().unwrap_or("").trim().to_owned();
        let arch = fields.next().unwrap_or("").trim().to_owned();
        let home = fields.next().unwrap_or("").trim().to_owned();

        if git_version.is_empty() || os.is_empty() || home.is_empty() {
            return Err(AppError::Ssh(format!(
                "{} answered the probe with something unexpected. A login shell that prints a banner on stdout will do this.",
                self.target.alias
            )));
        }

        Ok(HostProbe {
            git_version,
            os,
            arch,
            home,
        })
    }

    /// Closes the connection. Only ever stops a master this app started.
    pub async fn disconnect(&mut self) {
        if let Master::Owned(_) = self.master {
            let mut exit = Command::new("ssh");
            exit.args(&self.extra_args)
                .args(["-O", "exit", "-o"])
                .arg(format!("ControlPath={}", self.control_path.display()))
                .arg(self.target.destination())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true);
            let _ = exit.output().await;
        }
        if let Master::Owned(child) = &mut self.master {
            let _ = child.kill().await;
        }
    }
}

/// Options every invocation shares. `BatchMode` is set per call site, because
/// the master is the one connection allowed to ask a question.
fn base_args() -> [&'static str; 6] {
    [
        "-o",
        "ConnectTimeout=10",
        "-o",
        "ServerAliveInterval=15",
        "-o",
        "ServerAliveCountMax=3",
    ]
}

/// Quotes an argv for `sh`, since ssh hands the remote side one string.
///
/// Single quotes with `'\''` for embedded quotes is the only form that needs no
/// knowledge of the remote shell's escapes — inside single quotes, `sh` treats
/// every byte literally.
pub fn shell_join(argv: &[&str]) -> String {
    argv.iter()
        .map(|arg| format!("'{}'", arg.replace('\'', "'\\''")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether a control socket is live. `ssh -O check` asks the master itself, so
/// a stale socket file left by a killed master answers honestly.
async fn control_socket_alive(
    target: &SshTarget,
    extra_args: &[String],
    control_path: &Path,
) -> bool {
    Command::new("ssh")
        .args(extra_args)
        .args(["-O", "check", "-o"])
        .arg(format!("ControlPath={}", control_path.display()))
        .arg(target.destination())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// The user's own master, if they have one and it is alive.
async fn adopt_existing(target: &SshTarget, extra_args: &[String]) -> Option<PathBuf> {
    let path = target.control_path.clone()?;
    control_socket_alive(target, extra_args, &path)
        .await
        .then_some(path)
}

/// Starts a master that exists only to hold the authenticated connection open.
fn spawn_master(
    target: &SshTarget,
    extra_args: &[String],
    control_path: &Path,
    helper: &Path,
    askpass_socket: &Path,
) -> Result<Child, AppError> {
    let mut command = Command::new("ssh");
    command
        .args(extra_args)
        .args(base_args())
        .args([
            // No remote command, and no session: this process is the channel.
            "-N",
            // The socket dies with this process rather than outliving it, so a
            // connection the app forgot about cannot linger.
            "-o",
            "ControlPersist=no",
            "-o",
            "ControlMaster=yes",
            // The host key was settled before we got here, so anything else is
            // a genuine mismatch and must fail rather than prompt.
            "-o",
            "StrictHostKeyChecking=yes",
            "-o",
        ])
        .arg(format!("ControlPath={}", control_path.display()));
    if let Some(port) = target.port {
        command.args(["-p", &port.to_string()]);
    }
    command
        .arg(target.destination())
        // No stdin at all is what makes `SSH_ASKPASS_REQUIRE=force` take
        // effect; with a terminal available ssh would prompt there instead.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("SSH_ASKPASS_REQUIRE", "force")
        .env("SSH_ASKPASS", helper)
        .env(ASKPASS_SOCKET_ENV, askpass_socket)
        // The helper is this binary; without this it would try to open a window.
        .env("ONLYDIFFS_ASKPASS_HELPER", "1")
        .kill_on_drop(true);

    command
        .spawn()
        .map_err(|error| AppError::Ssh(format!("could not start ssh: {error}. Is OpenSSH installed?")))
}

#[cfg(test)]
mod tests {
    use super::{shell_join, HostProbe};

    fn probe(os: &str, arch: &str) -> HostProbe {
        HostProbe {
            git_version: "2.43.0".into(),
            os: os.into(),
            arch: arch.into(),
            home: "/home/me".into(),
        }
    }

    #[test]
    fn a_path_with_a_quote_in_it_survives_the_remote_shell() {
        // The remote side runs `sh -c`, so this is the boundary that decides
        // whether a filename can become a command.
        assert_eq!(
            shell_join(&["git", "add", "--", "it's a file.txt"]),
            r#"'git' 'add' '--' 'it'\''s a file.txt'"#
        );
    }

    #[test]
    fn a_semicolon_is_an_argument_rather_than_a_separator() {
        assert_eq!(
            shell_join(&["git", "show", "HEAD:; rm -rf ~"]),
            r#"'git' 'show' 'HEAD:; rm -rf ~'"#
        );
    }

    #[test]
    fn an_empty_argument_stays_an_argument() {
        assert_eq!(shell_join(&["git", "show", ""]), "'git' 'show' ''");
    }

    #[test]
    fn the_hosts_platform_picks_the_agent_that_will_run_on_it() {
        assert_eq!(probe("Linux", "x86_64").target_triple(), Some("x86_64-unknown-linux-gnu"));
        assert_eq!(probe("Linux", "aarch64").target_triple(), Some("aarch64-unknown-linux-gnu"));
        // macOS reports arm64 where Rust spells it aarch64.
        assert_eq!(probe("Darwin", "arm64").target_triple(), Some("aarch64-apple-darwin"));
    }

    #[test]
    fn a_platform_with_no_agent_is_refused_rather_than_guessed_at() {
        assert_eq!(probe("FreeBSD", "amd64").target_triple(), None);
        assert_eq!(probe("Linux", "riscv64").target_triple(), None);
    }
}
