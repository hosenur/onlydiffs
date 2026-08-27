//! The OnlyDiffs backend.
//!
//! One window, one long-lived `AppState`, and the command surface in
//! `commands`. The repository under review lives in `AppState::workspace` and
//! is resolved per call, so opening a different project from the landing page
//! takes effect immediately.

mod commands;
// Public so the integration tests in `tests/` can drive the services directly,
// the way the Effect build's tests drove the layers.
pub mod contract;
pub mod error;
pub mod services;

use tauri::{WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;

use services::workspace::Workspace;

/// How long the renderer gets to paint before the window is shown regardless.
const FIRST_PAINT_GRACE: std::time::Duration = std::time::Duration::from_secs(3);

/// Everything the commands share. Built once, at startup.
pub struct AppState {
    pub workspace: Workspace,
    /// One pooled client for both outbound callers — Groq and the loopback
    /// Claude channel.
    pub http: reqwest::Client,
}

impl AppState {
    fn new() -> Self {
        Self {
            workspace: Workspace::from_env(),
            http: reqwest::Client::new(),
        }
    }
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
    tauri::Builder::default()
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
        ])
        .setup(|app| {
            let handle = app.handle().clone();
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

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running onlydiffs");
}
