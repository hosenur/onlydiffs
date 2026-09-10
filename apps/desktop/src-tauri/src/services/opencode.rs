//! The local-only bridge to an OpenCode 2 TUI.
//!
//! OpenCode 2 owns one authenticated background service. External clients are
//! expected to discover that service from its registration file; they must not
//! start or replace it. A persisted session is not enough on its own, so this
//! bridge also requires a TUI from the registered OpenCode executable to be
//! running in the open repository.

use std::collections::HashSet;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::{Client, RequestBuilder, Response, Url};
use serde::Deserialize;
use serde_json::json;

use crate::contract::OpenCodeChannelStatus;
use crate::error::AppError;

const MAX_MESSAGE_BYTES: usize = 64 * 1024;
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const SEND_TIMEOUT: Duration = Duration::from_secs(10);

const NO_SESSION_MESSAGE: &str =
    "No OpenCode 2 session is running in this repository. Open `opencode` there first.";
const NOT_CONNECTED_MESSAGE: &str =
    "An OpenCode 2 session is running here, but its background service has no matching session.";

#[derive(Debug, Deserialize)]
struct Registration {
    #[serde(default)]
    version: Option<String>,
    url: String,
    pid: u32,
    #[serde(default)]
    password: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Health {
    healthy: bool,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    pid: Option<u32>,
}

/// Which body a service's prompt endpoint accepts.
///
/// Both versions expose `/api/session/{id}/prompt`, and both refuse anything
/// their schema does not name, but they do not agree on what goes in it: v1
/// requires the prompt's fields nested under `prompt`, v2 requires `text` at
/// the top level. One body cannot satisfy both.
///
/// Version numbers are no use for telling them apart. The v2 beta reports
/// `0.0.0-beta-19242` and v1 reports `1.18.30`, so anything that orders them
/// concludes v1 is the newer of the two and reaches for the wrong shape. What
/// actually separates them is that v2's health carries a version and a pid at
/// all; v1 answers `{"healthy":true}` and nothing more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dialect {
    /// v1: `{"prompt": {"text": ...}}`.
    Nested,
    /// v2: `{"text": ...}`.
    Flat,
}

impl Dialect {
    fn of(health: &Health) -> Self {
        if health.version.is_some() {
            Self::Flat
        } else {
            Self::Nested
        }
    }

    fn body(self, message: &str) -> serde_json::Value {
        match self {
            Self::Flat => json!({ "text": message, "delivery": "queue" }),
            Self::Nested => json!({ "prompt": { "text": message }, "delivery": "queue" }),
        }
    }
}

/// A service that answered a health probe: where to reach it, and what it
/// speaks. The dialect is settled here, at the one point that has the evidence,
/// rather than re-derived wherever a message is built.
struct Service {
    registration: Registration,
    dialect: Dialect,
}

#[derive(Debug, Deserialize)]
struct SessionList {
    data: Vec<Session>,
}

#[derive(Debug, Deserialize)]
struct Session {
    id: String,
    /// `parentID`, with the acronym in capitals — which is not what a
    /// `camelCase` rename produces. Spelling it `parentId` meant this never
    /// matched, so every child session read as a top-level one and the filter
    /// below stopped excluding them: a sub-agent's session could be picked as
    /// the newest and take the message meant for the session someone is
    /// sitting in front of.
    #[serde(default, rename = "parentID")]
    parent_id: Option<String>,
    time: SessionTime,
}

#[derive(Debug, Deserialize)]
struct SessionTime {
    updated: u64,
    #[serde(default)]
    archived: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct PromptResponse {
    data: AdmittedPrompt,
}

#[derive(Debug, Deserialize)]
struct AdmittedPrompt {
    id: String,
}

fn registration_path() -> PathBuf {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".local/state")))
        .unwrap_or_else(|| PathBuf::from(".local/state"));
    state.join("opencode/service.json")
}

fn authenticated(registration: &Registration, request: RequestBuilder) -> RequestBuilder {
    match registration.password.as_deref() {
        Some(password) => request.basic_auth("opencode", Some(password)),
        None => request,
    }
}

fn endpoint(registration: &Registration, path: &str) -> Option<Url> {
    let mut url = Url::parse(&registration.url).ok()?;
    if url.scheme() != "http" || !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    let local = match url.host_str()? {
        "localhost" => true,
        host => host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback()),
    };
    if !local {
        return None;
    }
    url.set_path(path);
    url.set_query(None);
    Some(url)
}

async fn discover(client: &Client) -> Option<Service> {
    let raw = tokio::fs::read_to_string(registration_path()).await.ok()?;
    let registration: Registration = serde_json::from_str(&raw).ok()?;
    let health_url = endpoint(&registration, "/api/health")?;
    let response = authenticated(&registration, client.get(health_url))
        .timeout(PROBE_TIMEOUT)
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let health: Health = response.json().await.ok()?;
    if !health.healthy
        || health.pid.is_some_and(|pid| pid != registration.pid)
        || matches!((&health.version, &registration.version), (Some(actual), Some(expected)) if actual != expected)
    {
        return None;
    }
    Some(Service {
        dialect: Dialect::of(&health),
        registration,
    })
}

async fn matching_session(
    client: &Client,
    registration: &Registration,
    directories: &[PathBuf],
) -> Result<Option<Session>, AppError> {
    let mut newest: Option<Session> = None;
    let mut queried = HashSet::new();

    for directory in directories {
        let Some(directory) = directory.to_str() else {
            continue;
        };
        if !queried.insert(directory.to_owned()) {
            continue;
        }
        let Some(url) = endpoint(registration, "/api/session") else {
            return Err(AppError::OpenCodeChannel(
                "OpenCode registered an invalid service URL.".into(),
            ));
        };
        let response = authenticated(
            registration,
            client
                .get(url)
                .query(&[("directory", directory), ("order", "desc"), ("limit", "50")]),
        )
        .timeout(PROBE_TIMEOUT)
        .send()
        .await
        .map_err(|error| {
            AppError::OpenCodeChannel(format!("Could not reach OpenCode 2: {error}"))
        })?;
        if !response.status().is_success() {
            return Err(response_error(response, "list OpenCode 2 sessions").await);
        }
        let listed: SessionList = response.json().await.map_err(|error| {
            AppError::OpenCodeChannel(format!(
                "OpenCode 2 returned an invalid session list: {error}"
            ))
        })?;
        for session in listed
            .data
            .into_iter()
            .filter(|session| session.parent_id.is_none() && session.time.archived.is_none())
        {
            if newest
                .as_ref()
                .map_or(true, |current| session.time.updated > current.time.updated)
            {
                newest = Some(session);
            }
        }
    }
    Ok(newest)
}

async fn response_error(response: Response, operation: &str) -> AppError {
    let status = response.status();
    let detail = response
        .text()
        .await
        .ok()
        .and_then(|body| serde_json::from_str::<serde_json::Value>(&body).ok())
        .and_then(|body| body.get("message")?.as_str().map(str::to_owned));
    AppError::OpenCodeChannel(
        detail.unwrap_or_else(|| format!("OpenCode 2 could not {operation} (HTTP {status}).")),
    )
}

/// Whether a live local OpenCode 2 TUI has a service session for this repository.
pub async fn status(root: &Path, client: &Client) -> OpenCodeChannelStatus {
    let Some(service) = discover(client).await else {
        return OpenCodeChannelStatus {
            connected: false,
            sessions: 0,
        };
    };
    let directories = tui_sessions(root, service.registration.pid).await;
    let sessions = directories.len();
    let connected = sessions > 0
        && matching_session(client, &service.registration, &directories)
            .await
            .is_ok_and(|session| session.is_some());
    OpenCodeChannelStatus {
        connected,
        sessions,
    }
}

/// Durably admits a queued prompt to the newest matching OpenCode 2 session.
pub async fn send(root: &Path, raw_message: &str, client: &Client) -> Result<String, AppError> {
    let fail = AppError::OpenCodeChannel;
    let message = raw_message.trim();
    if message.is_empty() {
        return Err(fail("Message cannot be empty.".into()));
    }
    if message.len() > MAX_MESSAGE_BYTES {
        return Err(fail(format!(
            "Message is too large (maximum {MAX_MESSAGE_BYTES} bytes)."
        )));
    }

    let service = discover(client)
        .await
        .ok_or_else(|| fail("OpenCode 2's background service is not available.".into()))?;
    let directories = tui_sessions(root, service.registration.pid).await;
    if directories.is_empty() {
        return Err(fail(NO_SESSION_MESSAGE.into()));
    }
    let session = matching_session(client, &service.registration, &directories)
        .await?
        .ok_or_else(|| fail(NOT_CONNECTED_MESSAGE.into()))?;
    let Some(url) = endpoint(
        &service.registration,
        &format!("/api/session/{}/prompt", session.id),
    ) else {
        return Err(fail("OpenCode registered an invalid service URL.".into()));
    };
    let response = authenticated(
        &service.registration,
        client.post(url).json(&service.dialect.body(message)),
    )
    .timeout(SEND_TIMEOUT)
    .send()
    .await
    .map_err(|error| fail(format!("Could not send to OpenCode 2: {error}")))?;
    if !response.status().is_success() {
        return Err(response_error(response, "queue the message").await);
    }
    let admitted: PromptResponse = response.json().await.map_err(|error| {
        fail(format!(
            "OpenCode 2 returned an invalid prompt acknowledgement: {error}"
        ))
    })?;
    Ok(admitted.data.id)
}

fn is_tui(arguments: &[String]) -> bool {
    let Some(executable) = arguments.first() else {
        return false;
    };
    let is_opencode = Path::new(executable)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "opencode" || name.starts_with("opencode2"));
    if !is_opencode {
        return false;
    }
    const NON_TUI_COMMANDS: [&str; 20] = [
        "acp", "agent", "api", "attach", "auth", "debug", "export", "github", "import", "mcp",
        "migrate", "models", "pr", "run", "serve", "service", "stats", "uninstall", "upgrade",
        "web",
    ];
    !arguments
        .iter()
        .skip(1)
        .any(|argument| NON_TUI_COMMANDS.contains(&argument.as_str()))
}

async fn tui_sessions(root: &Path, service_pid: u32) -> Vec<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        linux_tui_sessions(root, service_pid).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        lsof_tui_sessions(root, service_pid).await
    }
}

#[cfg(target_os = "linux")]
async fn linux_tui_sessions(root: &Path, service_pid: u32) -> Vec<PathBuf> {
    let Ok(service_executable) = tokio::fs::read_link(format!("/proc/{service_pid}/exe")).await
    else {
        return Vec::new();
    };
    let mut found = Vec::new();
    let Ok(mut entries) = tokio::fs::read_dir("/proc").await else {
        return found;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name();
        let Some(pid) = name
            .to_str()
            .filter(|name| name.bytes().all(|byte| byte.is_ascii_digit()))
        else {
            continue;
        };
        let Ok(executable) = tokio::fs::read_link(format!("/proc/{pid}/exe")).await else {
            continue;
        };
        if executable != service_executable {
            continue;
        }
        let Ok(raw) = tokio::fs::read(format!("/proc/{pid}/cmdline")).await else {
            continue;
        };
        let arguments: Vec<String> = raw
            .split(|byte| *byte == 0)
            .filter(|argument| !argument.is_empty())
            .map(|argument| String::from_utf8_lossy(argument).into_owned())
            .collect();
        if !is_tui(&arguments) {
            continue;
        }
        let Ok(cwd) = tokio::fs::read_link(format!("/proc/{pid}/cwd")).await else {
            continue;
        };
        if onlydiffs_core::services::paths::is_within(&cwd, root) {
            found.push(cwd);
        }
    }
    found
}

#[cfg(not(target_os = "linux"))]
async fn lsof_tui_sessions(root: &Path, service_pid: u32) -> Vec<PathBuf> {
    use tokio::process::Command;

    let Ok(service) = Command::new("ps")
        .args(["-p", &service_pid.to_string(), "-o", "comm="])
        .output()
        .await
    else {
        return Vec::new();
    };
    let executable = String::from_utf8_lossy(&service.stdout).trim().to_owned();
    if executable.is_empty() {
        return Vec::new();
    }
    let Ok(listing) = Command::new("ps")
        .args(["-eo", "pid=,comm=,args="])
        .output()
        .await
    else {
        return Vec::new();
    };
    let pids: Vec<String> = String::from_utf8_lossy(&listing.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let pid = fields.next()?;
            let command = fields.next()?;
            let arguments: Vec<String> = fields.map(str::to_owned).collect();
            (command == executable && is_tui(&arguments)).then(|| pid.to_owned())
        })
        .collect();
    if pids.is_empty() {
        return Vec::new();
    }
    let Ok(output) = Command::new("lsof")
        .args(["-a", "-d", "cwd", "-Fn", "-p", &pids.join(",")])
        .output()
        .await
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix('n'))
        .map(PathBuf::from)
        .filter(|cwd| onlydiffs_core::services::paths::is_within(cwd, root))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_tui_without_counting_v2_service_commands() {
        assert!(is_tui(&["opencode".into()]));
        assert!(is_tui(&[
            "/home/u/.opencode/bin/opencode2".into(),
            "--continue".into()
        ]));
        assert!(!is_tui(&[
            "opencode2".into(),
            "serve".into(),
            "--service".into()
        ]));
        assert!(!is_tui(&[
            "opencode".into(),
            "run".into(),
            "fix this".into()
        ]));
        assert!(!is_tui(&["vim".into(), "opencode.md".into()]));
    }

    #[test]
    fn accepts_only_loopback_http_endpoints() {
        let registration = |url: &str| Registration {
            version: None,
            url: url.into(),
            pid: 1,
            password: None,
        };
        assert!(endpoint(&registration("http://127.0.0.1:4096"), "/api/health").is_some());
        assert!(endpoint(&registration("http://[::1]:4096"), "/api/health").is_some());
        assert!(endpoint(&registration("https://example.com"), "/api/health").is_none());
    }

    #[tokio::test]
    async fn an_empty_message_is_refused_before_service_discovery() {
        let refused = send(Path::new("/repository"), "  ", &Client::new()).await;

        assert_eq!(
            refused.expect_err("refused").tag(),
            "OpenCodeChannelError"
        );
    }

    #[tokio::test]
    async fn an_oversized_message_is_refused_before_service_discovery() {
        let refused = send(
            Path::new("/repository"),
            &"x".repeat(MAX_MESSAGE_BYTES + 1),
            &Client::new(),
        )
        .await;

        assert!(refused.expect_err("refused").message().contains("too large"));
    }

    /// The service spells the field `parentID`. `camelCase` produces
    /// `parentId`, which matched nothing — so `parent_id` was always `None`,
    /// every child read as top-level, and the newest of them won. A sub-agent's
    /// session is newer than the one it was spawned from about as often as not.
    #[test]
    fn the_session_a_message_goes_to_is_the_one_with_no_parent() {
        let listed: SessionList = serde_json::from_str(
            r#"{"data":[
                {"id":"ses_parent","time":{"updated":2}},
                {"id":"ses_child","parentID":"ses_parent","time":{"updated":3}}
            ]}"#,
        )
        .expect("a session list");

        assert_eq!(listed.data[0].parent_id, None);
        assert_eq!(
            listed.data[1].parent_id.as_deref(),
            Some("ses_parent"),
            "a child session has to be recognisable as one"
        );

        // The choice `matching_session` makes: newest of those with no parent.
        let newest = listed
            .data
            .iter()
            .filter(|session| session.parent_id.is_none() && session.time.archived.is_none())
            .max_by_key(|session| session.time.updated)
            .expect("a top-level session");
        assert_eq!(
            newest.id, "ses_parent",
            "the newer child must not take the message"
        );
    }

    /// The two services answer the same route with bodies neither will accept
    /// from the other, and their version numbers cannot be used to tell which
    /// is which: v2's beta reports `0.0.0-beta-19242` and v1 reports `1.18.30`,
    /// so ordering them makes v1 look like the newer one. The health response
    /// is what actually separates them.
    #[test]
    fn the_dialect_follows_the_health_rather_than_the_version_number() {
        let v1: Health = serde_json::from_str(r#"{"healthy":true}"#).expect("v1 health");
        let v2: Health =
            serde_json::from_str(r#"{"healthy":true,"version":"0.0.0-beta-19242","pid":485003}"#)
                .expect("v2 health");

        // Why the version string is not the signal: compared as anyone would
        // compare them, v1's is the greater of the two.
        assert!(
            "1.18.30" > "0.0.0-beta-19242",
            "the version numbers order v1 above v2"
        );
        assert_eq!(Dialect::of(&v1), Dialect::Nested);
        assert_eq!(Dialect::of(&v2), Dialect::Flat);
    }

    /// Both schemas require their own key and refuse additional properties, so
    /// these are the only two bodies either service will take.
    #[test]
    fn each_dialect_builds_the_body_its_service_requires() {
        assert_eq!(
            Dialect::Flat.body("hello"),
            json!({ "text": "hello", "delivery": "queue" })
        );
        assert_eq!(
            Dialect::Nested.body("hello"),
            json!({ "prompt": { "text": "hello" }, "delivery": "queue" })
        );
    }
}
