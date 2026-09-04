//! The complete surface the renderer can reach.
//!
//! Every command answers with an `IpcResult` rather than a `Result`, so a
//! failure arrives as a value the renderer can pattern-match on instead of a
//! rejection whose tag has been stringified away. Payloads are deserialised
//! into the request structs below before a service sees them, so a renderer
//! that has been tampered with cannot hand arbitrary shapes to `git`.

use serde::Deserialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::contract::{
    AppSettings, AppTheme, ChangeStatus, ClaudeChannelStatus, CodexChannelStatus, Commit,
    ConnectedHost,
    FullFileContents, HostConnectionState, Project, ProjectLocation, RepoDiff, SshHostEntry,
    UnknownHostKeyPrompt, UpdateStatus,
};
use crate::error::{AppError, IpcResult};
use crate::services::ssh::{host_key, target};
use crate::services::{commit_message, repo_watch, updater};
use crate::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFileContentsRequest {
    path: String,
    #[serde(default)]
    old_path: Option<String>,
    status: ChangeStatus,
    staged: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageFileRequest {
    path: String,
    #[serde(default)]
    old_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GetHistoryRequest {
    #[serde(default)]
    limit: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct SendClaudeMessageRequest {
    message: String,
}

#[derive(Debug, Deserialize)]
pub struct SendCodexMessageRequest {
    message: String,
}

#[derive(Debug, Deserialize)]
pub struct CommitAllRequest {
    message: String,
}

#[derive(Debug, Deserialize)]
pub struct OpenProjectRequest {
    path: String,
}

#[derive(Debug, Deserialize)]
pub struct ForgetProjectRequest {
    path: String,
}

#[derive(Debug, Deserialize)]
pub struct SetThemeRequest {
    theme: AppTheme,
}

#[derive(Debug, Deserialize)]
pub struct SetGroqApiKeyRequest {
    /// Absent or blank clears the stored key, handing the app back to
    /// `GROQ_API_KEY` where that is set.
    #[serde(default)]
    key: Option<String>,
}

/// Resolves the open repository, or answers the renderer with the failure that
/// says there isn't one. Every command below that touches a repository starts
/// here, so "no project open" is reported in exactly one shape.
macro_rules! repository {
    ($state:expr) => {
        match $state.repository().await {
            Ok(repository) => repository,
            Err(error) => return Ok(IpcResult::Err(error)),
        }
    };
}

#[tauri::command]
pub async fn get_diff(state: State<'_, AppState>) -> Result<IpcResult<RepoDiff>, ()> {
    let repo = repository!(state);
    Ok(repo.diff().await.into())
}

#[tauri::command]
pub async fn get_file_contents(
    state: State<'_, AppState>,
    request: GetFileContentsRequest,
) -> Result<IpcResult<FullFileContents>, ()> {
    let repo = repository!(state);
    Ok(repo
        .file_contents(
            &request.path,
            request.old_path.as_deref(),
            request.status,
            request.staged,
        )
        .await
        .into())
}

#[tauri::command]
pub async fn get_history(
    state: State<'_, AppState>,
    request: GetHistoryRequest,
) -> Result<IpcResult<Vec<Commit>>, ()> {
    let repo = repository!(state);
    Ok(repo.history(request.limit).await.into())
}

#[tauri::command]
pub async fn stage_file(
    state: State<'_, AppState>,
    request: StageFileRequest,
) -> Result<IpcResult<()>, ()> {
    let repo = repository!(state);
    Ok(repo
        .stage_file(&request.path, request.old_path.as_deref())
        .await
        .into())
}

#[tauri::command]
pub async fn generate_commit_message(state: State<'_, AppState>) -> Result<IpcResult<String>, ()> {
    let repo = repository!(state);
    Ok(
        commit_message::generate(&repo, &state.settings, &state.http)
            .await
            .into(),
    )
}

#[tauri::command]
pub async fn send_claude_message(
    state: State<'_, AppState>,
    request: SendClaudeMessageRequest,
) -> Result<IpcResult<String>, ()> {
    let repo = repository!(state);
    Ok(repo.claude_send(&request.message).await.into())
}

/// Hands a message to the Codex session working in the open repository. Like
/// its Claude counterpart it refuses when no session is running there.
#[tauri::command]
pub async fn send_codex_message(
    state: State<'_, AppState>,
    request: SendCodexMessageRequest,
) -> Result<IpcResult<String>, ()> {
    let repo = repository!(state);
    Ok(repo.codex_send(&request.message).await.into())
}

#[tauri::command]
pub async fn codex_status(state: State<'_, AppState>) -> Result<IpcResult<CodexChannelStatus>, ()> {
    // Polled beside the Claude indicator and on the same terms: a project that
    // is not open, or a host that is not reachable, is "no session" rather than
    // a failure worth surfacing four times a minute.
    let Ok(repo) = state.repository().await else {
        return Ok(IpcResult::Ok(CodexChannelStatus {
            connected: false,
            sessions: 0,
        }));
    };
    Ok(IpcResult::Ok(repo.codex_status().await))
}

/// Writes a pasted image where the Claude session for the open repository can
/// open it, and answers with the path it landed at — on that repository's
/// machine, which is the only machine the path is good on.
///
/// The one command whose payload is not JSON. A screenshot is megabytes of
/// binary: base64 would cost a third again on top of a copy the renderer has
/// already made, and JSON's array-of-numbers encoding several times that. Tauri
/// carries an `ArrayBuffer` over as a raw body, so the bytes arrive as bytes.
#[tauri::command]
pub async fn attach_image(
    state: State<'_, AppState>,
    request: tauri::ipc::Request<'_>,
) -> Result<IpcResult<String>, ()> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Ok(IpcResult::Err(AppError::Attachment(
            "The pasted image did not arrive as bytes.".into(),
        )));
    };
    let repo = repository!(state);
    Ok(repo.write_attachment(bytes).await.into())
}

#[tauri::command]
pub async fn claude_status(state: State<'_, AppState>) -> Result<IpcResult<ClaudeChannelStatus>, ()> {
    // A status poll on a project that is not open, or on a host that is not
    // connected, is "no session" rather than an error: the indicator asks four
    // times a minute and has nothing to do with a failure.
    let Ok(repo) = state.repository().await else {
        return Ok(IpcResult::Ok(ClaudeChannelStatus {
            connected: false,
            sessions: 0,
            unregistered: 0,
        }));
    };
    Ok(IpcResult::Ok(repo.claude_status().await))
}

#[tauri::command]
pub async fn commit_all(
    state: State<'_, AppState>,
    request: CommitAllRequest,
) -> Result<IpcResult<String>, ()> {
    let repo = repository!(state);
    Ok(repo.commit_all(&request.message).await.into())
}

#[tauri::command]
pub async fn list_files(state: State<'_, AppState>) -> Result<IpcResult<Vec<String>>, ()> {
    let repo = repository!(state);
    Ok(repo.list_files().await.into())
}

#[tauri::command]
pub async fn list_projects(state: State<'_, AppState>) -> Result<IpcResult<Vec<Project>>, ()> {
    Ok(IpcResult::Ok(state.workspace.list()))
}

#[tauri::command]
pub async fn current_project(state: State<'_, AppState>) -> Result<IpcResult<Option<Project>>, ()> {
    Ok(IpcResult::Ok(state.workspace.current_project()))
}

#[tauri::command]
pub async fn open_project(
    app: AppHandle,
    state: State<'_, AppState>,
    request: OpenProjectRequest,
) -> Result<IpcResult<Project>, ()> {
    // Releases a watch on the host if the outgoing project was on one.
    // Without this, switching from a remote project to a local one leaves the
    // host watching and pushing changes, and the app refreshing for a
    // repository nobody is looking at.
    stop_watching(&state).await;
    let opened = state.workspace.open(&request.path);
    // Move the watch with the project. Reopening the current repository lands
    // on the same root and is a no-op inside the watcher.
    if opened.is_ok() {
        if let Ok(root) = state.workspace.current_path() {
            repo_watch::watch_repo(&app, &state.watcher, root);
        }
        crate::resolve_project_icons_in_background(app.clone());
    }
    Ok(opened.into())
}

#[tauri::command]
pub async fn forget_project(
    state: State<'_, AppState>,
    request: ForgetProjectRequest,
) -> Result<IpcResult<()>, ()> {
    state.workspace.forget(&request.path);
    Ok(IpcResult::Ok(()))
}

/// The window frame is drawn by the OS, not by the page, so an in-app theme
/// pinned against the system one leaves the title bar and its buttons out of
/// step. `set_theme` is the one knob that moves them; `None` hands the window
/// back to the system setting.
#[tauri::command]
pub async fn set_theme(app: AppHandle, request: SetThemeRequest) -> Result<IpcResult<()>, ()> {
    let theme = match request.theme {
        AppTheme::Light => Some(tauri::Theme::Light),
        AppTheme::Dark => Some(tauri::Theme::Dark),
        AppTheme::System => None,
    };
    let Some(window) = app.get_webview_window("main") else {
        return Ok(IpcResult::Ok(()));
    };
    Ok(window
        .set_theme(theme)
        .map_err(|error| AppError::WorkTree(format!("failed to set the window theme: {error}")))
        .into())
}

/// Clipboard writes go through the backend rather than `navigator.clipboard`,
/// which depends on the renderer being a secure context — it is not when the
/// app is loaded from a custom protocol.
#[tauri::command]
pub async fn write_clipboard_text(app: AppHandle, text: String) -> Result<IpcResult<()>, ()> {
    Ok(app
        .clipboard()
        .write_text(text)
        .map_err(|error| AppError::Clipboard(format!("failed to write to the clipboard: {error}")))
        .into())
}

/// Answers `available: false` rather than failing when there is no update, so
/// the renderer has one shape to read either way. A dev build always answers
/// that way — see `services::updater`.
#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<IpcResult<UpdateStatus>, ()> {
    Ok(updater::check(&app, &state).await.into())
}

/// Installs the release the last check turned up and relaunches into it, so a
/// successful call never returns.
#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<IpcResult<()>, ()> {
    Ok(updater::install(&app, &state).await.into())
}

/// The settings page's whole read. Resolving the key can reach for the login
/// shell, so this is a command rather than something read at startup.
#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<IpcResult<AppSettings>, ()> {
    Ok(IpcResult::Ok(state.settings.snapshot().await))
}

/// Saves a Groq key and answers with the settings as they now stand, so the
/// page never has to guess what its own write produced.
#[tauri::command]
pub async fn set_groq_api_key(
    app: AppHandle,
    state: State<'_, AppState>,
    request: SetGroqApiKeyRequest,
) -> Result<IpcResult<AppSettings>, ()> {
    if let Err(error) = state.settings.set_groq_key(request.key.as_deref()) {
        return Ok(IpcResult::Err(error));
    }
    // Every project still on the cube fallback was skipped for want of a key.
    // Now that there is one, they are worth another look — and the icons
    // appearing behind the settings page is the clearest confirmation there is
    // that the key works.
    crate::resolve_project_icons_in_background(app);
    Ok(IpcResult::Ok(state.settings.snapshot().await))
}

#[derive(Debug, Deserialize)]
pub struct HostRequest {
    /// The SSH destination as the user typed it: `build-box`, `me@10.0.0.4`,
    /// an alias from their config.
    alias: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRemoteProjectRequest {
    alias: String,
    /// A path on the host. Resolved to a repository root there, because this
    /// side has no way to stat it.
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerSshPromptRequest {
    id: u64,
    /// `None` cancels, which ssh reads as a refusal rather than a retry.
    #[serde(default)]
    answer: Option<String>,
}

/// Every host the user has added, and whether each is reachable right now.
///
/// Remembered and connected are deliberately one list. A host that is asleep is
/// still a host you have — hiding it until it answers would mean the machine
/// you want disappears exactly when it is not available.
#[tauri::command]
pub async fn list_hosts(state: State<'_, AppState>) -> Result<IpcResult<Vec<ConnectedHost>>, ()> {
    let mut hosts = state.hosts.list().await;
    for entry in state.settings.ssh_hosts() {
        if hosts.iter().any(|host| host.alias == entry.alias) {
            continue;
        }
        hosts.push(ConnectedHost {
            alias: entry.alias,
            // Everything else is what a connection reports, and there is no
            // connection. Resolving it would mean reading the ssh config for a
            // list that is rendered on every launch.
            hostname: String::new(),
            user: None,
            port: None,
            state: HostConnectionState::Disconnected,
            git_version: None,
            platform: None,
            agent_version: None,
            channel_registered: None,
        });
    }
    hosts.sort_by(|a, b| a.alias.cmp(&b.alias));
    Ok(IpcResult::Ok(hosts))
}

/// Opens a connection, authenticating if it has to.
///
/// A host with no key in `known_hosts` comes back as `SshUnknownHostError`
/// rather than connecting. The renderer answers that with `inspect_host_key`
/// and, if the user approves the fingerprint, `trust_host_key` — which is the
/// same first-connection question ssh itself asks, asked somewhere it can be
/// seen.
#[tauri::command]
pub async fn connect_host(
    app: AppHandle,
    state: State<'_, AppState>,
    request: HostRequest,
) -> Result<IpcResult<ConnectedHost>, ()> {
    let args = state.settings.ssh_args(&request.alias);
    Ok(state.hosts.connect(&app, &request.alias, args).await.into())
}

#[tauri::command]
pub async fn disconnect_host(
    app: AppHandle,
    state: State<'_, AppState>,
    request: HostRequest,
) -> Result<IpcResult<()>, ()> {
    state.hosts.disconnect(&app, &request.alias).await;
    Ok(IpcResult::Ok(()))
}

/// Fetches the host key of a machine we have not seen before, so the user has a
/// fingerprint to compare before anything is trusted.
#[tauri::command]
pub async fn inspect_host_key(
    state: State<'_, AppState>,
    request: HostRequest,
) -> Result<IpcResult<UnknownHostKeyPrompt>, ()> {
    let args = state.settings.ssh_args(&request.alias);
    let resolved = match target::resolve(&request.alias, &args).await {
        Ok(target) => target,
        Err(error) => return Ok(IpcResult::Err(error)),
    };
    let key = match host_key::fetch_unknown(&resolved.hostname, resolved.port).await {
        Ok(key) => key,
        Err(error) => return Ok(IpcResult::Err(error)),
    };
    Ok(IpcResult::Ok(UnknownHostKeyPrompt {
        alias: resolved.alias,
        hostname: resolved.hostname,
        port: resolved.port,
        key_type: key.key_type,
        fingerprint: key.fingerprint,
    }))
}

/// Records an approved host key in the user's own `known_hosts`, so every ssh
/// client on the machine trusts it — this app keeps no private store.
#[tauri::command]
pub async fn trust_host_key(
    state: State<'_, AppState>,
    request: HostRequest,
) -> Result<IpcResult<()>, ()> {
    let args = state.settings.ssh_args(&request.alias);
    let resolved = match target::resolve(&request.alias, &args).await {
        Ok(target) => target,
        Err(error) => return Ok(IpcResult::Err(error)),
    };
    let key = match host_key::fetch_unknown(&resolved.hostname, resolved.port).await {
        Ok(key) => key,
        Err(error) => return Ok(IpcResult::Err(error)),
    };
    Ok(host_key::trust(&resolved, &key).await.into())
}

/// The user's answer to something ssh asked.
#[tauri::command]
pub async fn answer_ssh_prompt(
    state: State<'_, AppState>,
    request: AnswerSshPromptRequest,
) -> Result<IpcResult<()>, ()> {
    state.hosts.answer_prompt(request.id, request.answer).await;
    Ok(IpcResult::Ok(()))
}

/// Opens a repository on a connected host.
///
/// The path is resolved on the host, by the agent, using the same walk up to a
/// `.git` that the local picker does — so pasting any path inside a checkout
/// opens its root either way.
#[tauri::command]
pub async fn open_remote_project(
    app: AppHandle,
    state: State<'_, AppState>,
    request: OpenRemoteProjectRequest,
) -> Result<IpcResult<Project>, ()> {
    let repo = match state.hosts.repository(&request.alias, "/").await {
        Ok(repo) => repo,
        Err(error) => return Ok(IpcResult::Err(error)),
    };
    let root = match repo.resolve_repository(&request.path).await {
        Ok(Some(root)) => root,
        Ok(None) => {
            return Ok(IpcResult::Err(AppError::InvalidProject(format!(
                "Not a git repository on {}: no .git found at or above {}.",
                request.alias, request.path
            ))))
        }
        Err(error) => return Ok(IpcResult::Err(error)),
    };

    // Stop watching whatever was open before taking up the new one, so a
    // project switch never leaves two watches running on a host.
    stop_watching(&state).await;

    let opened = state.workspace.adopt(ProjectLocation {
        host: Some(request.alias.clone()),
        path: root.clone(),
    });
    if opened.is_ok() {
        if let Ok(repo) = state.hosts.repository(&request.alias, &root).await {
            // A watch that cannot be established is not worth failing an open
            // over: the app still works, it just stops updating on its own.
            let _ = repo.set_watched(true).await;
        }
        crate::resolve_project_icons_in_background(app);
    }
    Ok(opened.into())
}

/// Asks whatever host the previously open project was on to stop watching it.
async fn stop_watching(state: &State<'_, AppState>) {
    let Ok(previous) = state.workspace.current_location() else {
        return;
    };
    let Some(alias) = previous.host.as_deref() else {
        return;
    };
    if let Ok(repo) = state.hosts.repository(alias, &previous.path).await {
        let _ = repo.set_watched(false).await;
    }
}

#[derive(Debug, Deserialize)]
pub struct AddSshHostRequest {
    /// The command the user already uses, e.g. `ssh user@example -p 2222`, or
    /// just a host. Whatever ssh understands.
    command: String,
}

/// Remembers an SSH destination, so it is offered next launch.
///
/// Takes the whole command rather than an alias. The options in it — a port, an
/// identity, a jump host — are kept and replayed on every later connection,
/// which is what makes a host on a non-standard port work without an edit to
/// `~/.ssh/config`. Adding is not connecting: a laptop that is asleep should
/// not stop its entry existing.
#[tauri::command]
pub async fn add_ssh_host(
    state: State<'_, AppState>,
    request: AddSshHostRequest,
) -> Result<IpcResult<SshHostEntry>, ()> {
    let parsed = match target::parse_ssh_command(&request.command) {
        Ok(parsed) => parsed,
        Err(error) => return Ok(IpcResult::Err(error)),
    };
    let entry = SshHostEntry {
        alias: parsed.destination,
        args: parsed.args,
    };
    if let Err(error) = state.settings.add_ssh_host(entry.clone()) {
        return Ok(IpcResult::Err(error));
    }
    Ok(IpcResult::Ok(entry))
}

/// Forgets an SSH destination, disconnecting it first if it is connected.
#[tauri::command]
pub async fn forget_ssh_host(
    app: AppHandle,
    state: State<'_, AppState>,
    request: HostRequest,
) -> Result<IpcResult<AppSettings>, ()> {
    state.hosts.disconnect(&app, &request.alias).await;
    if let Err(error) = state.settings.forget_ssh_host(&request.alias) {
        return Ok(IpcResult::Err(error));
    }
    Ok(IpcResult::Ok(state.settings.snapshot().await))
}
