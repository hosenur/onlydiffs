//! Host key verification, asked as a question rather than switched off.
//!
//! The tempting shortcut is `StrictHostKeyChecking=no` or `accept-new`, which
//! makes the first connection succeed and quietly discards the only protection
//! SSH has against a machine-in-the-middle on that first connection. This does
//! what `ssh` itself does when it has a terminal: look the host up in
//! `known_hosts`, and if it is not there, show the fingerprint and ask.
//!
//! Every step shells out to the OpenSSH tools rather than parsing key formats
//! here. `known_hosts` supports hashed hostnames, `@revoked` and `@cert-authority`
//! markers, and per-host key types; `ssh-keygen -F` already knows all of it.

use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::error::AppError;
use crate::services::ssh::target::SshTarget;

/// `ssh-keyscan` talks to the host, so this is a network timeout.
const SCAN_TIMEOUT: Duration = Duration::from_secs(15);

/// A host key offered by a machine we have not seen before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownHostKey {
    /// The `host` or `[host]:port` form `known_hosts` is keyed by.
    pub host: String,
    /// e.g. `ssh-ed25519`.
    pub key_type: String,
    /// The `SHA256:…` fingerprint, which is what the user compares.
    pub fingerprint: String,
    /// The exact `known_hosts` line, kept verbatim so trusting it appends
    /// precisely what was shown rather than something re-derived.
    line: String,
}

impl UnknownHostKey {
    pub fn known_hosts_line(&self) -> &str {
        &self.line
    }
}

/// How `known_hosts` names a host: bare, or bracketed when the port is not 22.
pub(crate) fn known_hosts_host(hostname: &str, port: Option<u16>) -> String {
    match port {
        Some(port) if port != 22 => format!("[{hostname}]:{port}"),
        _ => hostname.to_owned(),
    }
}

/// Whether this host already has a key in the files ssh will consult for it.
///
/// `ssh-keygen -F` exits 0 when it finds one, and understands hashed hostnames,
/// `@revoked` and `@cert-authority` markers, and per-host key types — none of
/// which is worth reimplementing here. A missing file is simply "not known",
/// not a failure: that is a first-ever connection.
pub async fn is_known(target: &SshTarget) -> Result<bool, AppError> {
    let host = known_hosts_host(&target.hostname, target.port);
    for file in target.known_hosts_files() {
        let output = Command::new("ssh-keygen")
            .args(["-F", &host, "-f"])
            .arg(file)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .output()
            .await
            .map_err(|error| {
                AppError::Ssh(format!(
                    "could not run ssh-keygen: {error}. Is OpenSSH installed?"
                ))
            })?;
        if output.status.success() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Picks the strongest key the host offered. `ssh-keyscan` prints one line per
/// type; preferring Ed25519 then ECDSA then RSA matches ssh's own default
/// `HostKeyAlgorithms` order, so the key shown is the key that will be used.
fn preferred_line(scan: &str) -> Option<String> {
    let lines: Vec<&str> = scan
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();

    for wanted in [
        "ssh-ed25519",
        "ecdsa-sha2-nistp256",
        "ecdsa-sha2-nistp384",
        "ecdsa-sha2-nistp521",
        "rsa-sha2-512",
        "rsa-sha2-256",
        "ssh-rsa",
    ] {
        if let Some(line) = lines.iter().find(|line| {
            line.split_whitespace().nth(1).is_some_and(|kind| kind == wanted)
        }) {
            return Some((*line).to_owned());
        }
    }
    lines.first().map(|line| (*line).to_owned())
}

/// Parses `ssh-keygen -lf -` output: `<bits> <fingerprint> <comment> (<TYPE>)`.
pub(crate) fn parse_fingerprint(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .find(|field| field.starts_with("SHA256:"))
        .map(str::to_owned)
}

/// Fetches the host's key and computes its fingerprint, so the user has
/// something to compare against what the host's operator told them.
pub async fn fetch_unknown(hostname: &str, port: Option<u16>) -> Result<UnknownHostKey, AppError> {
    let mut scan = Command::new("ssh-keyscan");
    if let Some(port) = port {
        scan.args(["-p", &port.to_string()]);
    }
    // `-T` bounds keyscan's own connect; the outer timeout covers DNS and a
    // host that accepts the connection and then says nothing.
    scan.args(["-T", "10", hostname])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let scanned = match tokio::time::timeout(SCAN_TIMEOUT, scan.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            return Err(AppError::Ssh(format!(
                "could not run ssh-keyscan: {error}. Is OpenSSH installed?"
            )))
        }
        Err(_) => {
            return Err(AppError::Ssh(format!(
                "{hostname} did not offer a host key within {}s.",
                SCAN_TIMEOUT.as_secs()
            )))
        }
    };

    let line = preferred_line(&String::from_utf8_lossy(&scanned.stdout)).ok_or_else(|| {
        let detail = String::from_utf8_lossy(&scanned.stderr);
        AppError::Ssh(format!(
            "{hostname} offered no host key. {}",
            detail.trim()
        ))
    })?;

    let key_type = line
        .split_whitespace()
        .nth(1)
        .unwrap_or("unknown")
        .to_owned();

    let mut fingerprinter = Command::new("ssh-keygen")
        .args(["-l", "-f", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| AppError::Ssh(format!("could not run ssh-keygen: {error}")))?;
    if let Some(mut stdin) = fingerprinter.stdin.take() {
        let _ = stdin.write_all(line.as_bytes()).await;
        let _ = stdin.write_all(b"\n").await;
        let _ = stdin.shutdown().await;
    }
    let fingerprinted = fingerprinter
        .wait_with_output()
        .await
        .map_err(|error| AppError::Ssh(format!("ssh-keygen failed: {error}")))?;

    let fingerprint = parse_fingerprint(&String::from_utf8_lossy(&fingerprinted.stdout))
        .ok_or_else(|| {
            AppError::Ssh(format!("could not fingerprint the host key offered by {hostname}."))
        })?;

    Ok(UnknownHostKey {
        host: known_hosts_host(hostname, port),
        key_type,
        fingerprint,
        line,
    })
}

/// Appends an approved key to the first `known_hosts` file ssh would consult,
/// so every other ssh client on the machine trusts it too — this is the user's
/// real store, not a private one this app keeps to itself.
pub async fn trust(target: &SshTarget, key: &UnknownHostKey) -> Result<(), AppError> {
    let path = target.known_hosts_for_writing().cloned().ok_or_else(|| {
        AppError::Ssh(format!(
            "{} has no writable known_hosts file configured, so there is nowhere to record its key.",
            target.alias
        ))
    })?;

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|error| {
            AppError::Ssh(format!("could not create {}: {error}", parent.display()))
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = tokio::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).await;
        }
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await
        .map_err(|error| AppError::Ssh(format!("could not open {}: {error}", path.display())))?;

    // A file that does not end in a newline would otherwise splice our entry
    // onto the last one and corrupt both.
    let needs_newline = tokio::fs::read(&path)
        .await
        .map(|body| !body.is_empty() && !body.ends_with(b"\n"))
        .unwrap_or(false);
    let mut entry = String::new();
    if needs_newline {
        entry.push('\n');
    }
    entry.push_str(key.known_hosts_line().trim_end());
    entry.push('\n');

    file.write_all(entry.as_bytes())
        .await
        .map_err(|error| AppError::Ssh(format!("could not write {}: {error}", path.display())))?;
    file.flush()
        .await
        .map_err(|error| AppError::Ssh(format!("could not write {}: {error}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::{known_hosts_host, parse_fingerprint, preferred_line};

    #[test]
    fn a_non_default_port_is_bracketed_the_way_known_hosts_expects() {
        assert_eq!(known_hosts_host("build.example", None), "build.example");
        assert_eq!(known_hosts_host("build.example", Some(22)), "build.example");
        assert_eq!(
            known_hosts_host("build.example", Some(2222)),
            "[build.example]:2222"
        );
    }

    #[test]
    fn the_strongest_offered_key_is_the_one_shown() {
        // keyscan prints in the host's order, which is not preference order.
        let scan = "# build.example:22 SSH-2.0-OpenSSH_9.6\n\
                    build.example ssh-rsa AAAArsa\n\
                    build.example ssh-ed25519 AAAAed\n";

        assert_eq!(
            preferred_line(scan).as_deref(),
            Some("build.example ssh-ed25519 AAAAed")
        );
    }

    #[test]
    fn comments_are_not_host_keys() {
        assert_eq!(preferred_line("# nothing but a banner\n"), None);
    }

    #[test]
    fn an_unranked_key_type_is_still_better_than_no_key() {
        let scan = "build.example ssh-dss AAAAdss\n";

        assert_eq!(preferred_line(scan).as_deref(), Some("build.example ssh-dss AAAAdss"));
    }

    #[test]
    fn the_fingerprint_is_lifted_out_of_ssh_keygens_line() {
        let line = "256 SHA256:abc123DEF/456+xyz build.example (ED25519)\n";

        assert_eq!(
            parse_fingerprint(line).as_deref(),
            Some("SHA256:abc123DEF/456+xyz")
        );
    }

    #[test]
    fn a_line_without_a_fingerprint_yields_none() {
        assert_eq!(parse_fingerprint("not a fingerprint at all"), None);
    }
}
