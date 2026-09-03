//! The OnlyDiffs backend.
//!
//! One window, one long-lived `AppState`, and the command surface in
//! `commands`. The repository under review lives in `AppState::workspace` and
//! is resolved per call, so opening a different project from the landing page
//! takes effect immediately.

mod commands;
// Re-exported rather than defined here: both live in `onlydiffs-core` so the
// agent speaks the same types, and keeping the paths means nothing above had to
// learn where they moved to.
pub use onlydiffs_core::{contract, error, protocol};
pub mod services;

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;

use services::project_icon;
use services::repository::Repository;
use services::settings::Settings;
use services::ssh::SshHosts;
use services::repo_watch;
use services::watcher::RepoWatcher;
use services::workspace::Workspace;

/// How long the renderer gets to paint before the window is shown regardless.
const FIRST_PAINT_GRACE: std::time::Duration = std::time::Duration::from_secs(3);

/// Everything the commands share. Built once, at startup.
pub struct AppState {
    pub workspace: Workspace,
    /// The hosts with a live connection. One per host, shared by every project
    /// on it, so two repositories on one build box cost one authentication.
    pub hosts: SshHosts,
    /// Everything the user set deliberately, including the Groq key both Groq
    /// features resolve through.
    pub settings: Settings,
    /// Follows the open repository and tells the renderer when it changes, so
    /// the diff on screen is the diff on disk without anyone asking.
    pub watcher: RepoWatcher,
    /// One pooled client for both outbound callers — Groq and the loopback
    /// Claude channel.
    pub http: reqwest::Client,
    /// Stops startup and newly opened projects from launching overlapping
    /// Groq image-selection requests.
    pub icon_resolution: tokio::sync::Mutex<()>,
    /// The release a check turned up, held until the user installs it so that
    /// installing applies the version they were offered.
    #[cfg(desktop)]
    pub pending_update: tokio::sync::Mutex<Option<tauri_plugin_updater::Update>>,
}

impl AppState {
    /// The open repository, resolved per call rather than held, so opening a
    /// different project takes effect immediately — and so that a project on
    /// another machine answers through a different transport without anything
    /// above here noticing which.
    pub async fn repository(&self) -> Result<Repository, crate::error::AppError> {
        let location = self.workspace.current_location()?;
        match &location.host {
            None => Ok(Repository::local(location.path.into())),
            Some(alias) => self.hosts.repository(alias, &location.path).await,
        }
    }

    fn new() -> Self {
        Self {
            workspace: Workspace::from_env(),
            hosts: SshHosts::new(),
            settings: Settings::from_env(),
            watcher: RepoWatcher::new(),
            http: reqwest::Client::new(),
            icon_resolution: tokio::sync::Mutex::new(()),
            #[cfg(desktop)]
            pending_update: tokio::sync::Mutex::new(None),
        }
    }
}

pub(crate) fn resolve_project_icons_in_background(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let _guard = state.icon_resolution.lock().await;
        project_icon::resolve_missing(
            &app,
            &state.hosts,
            &state.workspace,
            &state.settings,
            &state.http,
        )
        .await;
    });
}

/// Nothing in this app should navigate away from the bundle. Anything that
/// tries is handed to the real browser instead.
fn is_internal(url: &tauri::Url) -> bool {
    match url.scheme() {
        "tauri" | "asset" => true,
        "http" | "https" => matches!(url.host_str(), Some("localhost" | "127.0.0.1"))
            || url.host_str().is_some_and(|host| host.ends_with(".localhost")),
        _ => false,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    // Release builds only: the updater reads a signing key from the config at
    // startup, and nothing in a dev build has any use for it.
    #[cfg(all(desktop, not(debug_assertions)))]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::get_diff,
            commands::get_file_contents,
            commands::get_history,
            commands::stage_file,
            commands::generate_commit_message,
            commands::send_claude_message,
            commands::claude_status,
            commands::commit_all,
            commands::list_files,
            commands::list_projects,
            commands::current_project,
            commands::open_project,
            commands::forget_project,
            commands::set_theme,
            commands::write_clipboard_text,
            commands::check_for_update,
            commands::install_update,
            commands::get_settings,
            commands::set_groq_api_key,
            commands::list_hosts,
            commands::connect_host,
            commands::disconnect_host,
            commands::inspect_host_key,
            commands::trust_host_key,
            commands::answer_ssh_prompt,
            commands::open_remote_project,
            commands::add_ssh_host,
            commands::forget_ssh_host,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // `AppState` is built before there is a handle to emit through, so
            // the repository restored at startup starts being watched here.
            let state = app.state::<AppState>();
            if let Ok(root) = state.workspace.current_path() {
                repo_watch::watch_repo(app.handle(), &state.watcher, root);
            }

            let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("onlydiffs")
                .inner_size(1100.0, 760.0)
                .min_inner_size(720.0, 480.0)
                // The renderer shows the window once React has painted, which
                // is what keeps a half-drawn frame off the screen.
                .visible(false)
                .on_navigation(move |url| {
                    if is_internal(url) {
                        return true;
                    }
                    let _ = handle.opener().open_url(url.as_str(), None::<&str>);
                    false
                })
                .build()?;

            // Safety net for the hidden window: the renderer shows it once it
            // has painted, but if it never gets that far — a bundle that fails
            // to parse, a throw inside a provider — the app would sit invisible
            // with no way to recover. Show it anyway after a beat.
            let fallback = window.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(FIRST_PAINT_GRACE).await;
                if matches!(fallback.is_visible(), Ok(false)) {
                    let _ = fallback.show();
                }
            });

            // Icon discovery reads every remembered repository and may call
            // Groq, so it starts after the first frame rather than delaying it.
            let icon_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                resolve_project_icons_in_background(icon_app);
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building onlydiffs")
        .run(|app, event| {
            // Masters this app started are `ssh -N` processes with no terminal
            // and no parent to reap them, so quitting without this leaves one
            // per host running until the machine reboots. `kill_on_drop` does
            // not help: nothing unwinds on the way out of a GUI app.
            if let tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit = event {
                let state = app.state::<AppState>();
                tauri::async_runtime::block_on(state.hosts.disconnect_all());
            }
        });
}
