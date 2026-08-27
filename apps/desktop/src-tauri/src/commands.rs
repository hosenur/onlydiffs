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
    AppTheme, ChangeStatus, ClaudeChannelStatus, Commit, FullFileContents, Project, RepoDiff,
};
use crate::error::{AppError, IpcResult};
use crate::services::{claude_channel, commit_message, diff, file_tree, history};
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

#[tauri::command]
pub async fn get_diff(state: State<'_, AppState>) -> Result<IpcResult<RepoDiff>, ()> {
    Ok(diff::get_diff(&state.workspace).await.into())
}

#[tauri::command]
pub async fn get_file_contents(
    state: State<'_, AppState>,
    request: GetFileContentsRequest,
) -> Result<IpcResult<FullFileContents>, ()> {
    Ok(diff::get_file_contents(
        &state.workspace,
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
    Ok(history::get_history(&state.workspace, request.limit)
        .await
        .into())
}

#[tauri::command]
pub async fn stage_file(
    state: State<'_, AppState>,
    request: StageFileRequest,
) -> Result<IpcResult<()>, ()> {
    Ok(
        diff::stage_file(&state.workspace, &request.path, request.old_path.as_deref())
            .await
            .into(),
    )
}

#[tauri::command]
pub async fn generate_commit_message(state: State<'_, AppState>) -> Result<IpcResult<String>, ()> {
    Ok(commit_message::generate(&state.workspace, &state.http)
        .await
        .into())
}

#[tauri::command]
pub async fn send_claude_message(
    state: State<'_, AppState>,
    request: SendClaudeMessageRequest,
) -> Result<IpcResult<String>, ()> {
    Ok(
        claude_channel::send(&state.workspace, &state.http, &request.message)
            .await
            .into(),
    )
}

#[tauri::command]
pub async fn claude_status(state: State<'_, AppState>) -> Result<IpcResult<ClaudeChannelStatus>, ()> {
    Ok(IpcResult::Ok(claude_channel::status(&state.workspace).await))
}

#[tauri::command]
pub async fn commit_all(
    state: State<'_, AppState>,
    request: CommitAllRequest,
) -> Result<IpcResult<String>, ()> {
    Ok(diff::commit_all(&state.workspace, &request.message)
        .await
        .into())
}

#[tauri::command]
pub async fn list_files(state: State<'_, AppState>) -> Result<IpcResult<Vec<String>>, ()> {
    Ok(file_tree::list_files(&state.workspace).await.into())
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
    state: State<'_, AppState>,
    request: OpenProjectRequest,
) -> Result<IpcResult<Project>, ()> {
    Ok(state.workspace.open(&request.path).into())
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
