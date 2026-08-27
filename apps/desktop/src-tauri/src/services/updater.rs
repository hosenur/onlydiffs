//! Finding a newer release, and replacing this bundle with it.
//!
//! The renderer never talks to the updater plugin directly, the way it never
//! talks to the clipboard plugin directly — both arrive as commands. What the
//! plugin hands back from a check is a handle that knows which artifact to
//! download, so it is kept in `AppState` until the user acts on the offer;
//! installing then applies exactly the version the palette advertised, rather
//! than whatever happens to be newest by the time they click.

use crate::contract::UpdateStatus;
use crate::error::AppError;
use crate::AppState;

use tauri::AppHandle;

#[cfg(all(desktop, not(debug_assertions)))]
use tauri_plugin_updater::UpdaterExt;

#[cfg(all(desktop, not(debug_assertions)))]
pub async fn check(app: &AppHandle, state: &AppState) -> Result<UpdateStatus, AppError> {
    let found = app
        .updater()
        .map_err(|error| AppError::Updater(format!("the updater is unavailable: {error}")))?
        .check()
        .await
        .map_err(|error| AppError::Updater(format!("could not check for updates: {error}")))?;

    let mut pending = state.pending_update.lock().await;

    let Some(update) = found else {
        // An offer that has since been withdrawn — a release pulled, say —
        // should stop being installable.
        *pending = None;
        return Ok(UpdateStatus::none());
    };

    let status = UpdateStatus {
        available: true,
        version: Some(update.version.clone()),
        notes: update.body.clone(),
    };
    *pending = Some(update);
    Ok(status)
}

/// In development there is nothing to update *to*: the running tree is by
/// definition ahead of the last release, so a check could only ever offer to
/// replace it with something older. On mobile there is no updater at all.
#[cfg(not(all(desktop, not(debug_assertions))))]
pub async fn check(_app: &AppHandle, _state: &AppState) -> Result<UpdateStatus, AppError> {
    Ok(UpdateStatus::none())
}

#[cfg(all(desktop, not(debug_assertions)))]
pub async fn install(app: &AppHandle, state: &AppState) -> Result<(), AppError> {
    // Borrowed rather than taken: a download that fails halfway should leave
    // the offer standing, so the user can try again without waiting for the
    // next launch. Holding the lock across the download costs nothing — the
    // only other caller is the once-per-launch check.
    let pending = state.pending_update.lock().await;
    let update = pending.as_ref().ok_or_else(|| {
        AppError::Updater("no update is ready to install — check for one first".to_owned())
    })?;

    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|error| AppError::Updater(format!("could not install the update: {error}")))?;

    // Nothing after this line runs: the process is replaced by the new bundle.
    app.restart()
}

#[cfg(not(all(desktop, not(debug_assertions))))]
pub async fn install(_app: &AppHandle, _state: &AppState) -> Result<(), AppError> {
    Err(AppError::Updater(
        "updates are only installable in a release build".to_owned(),
    ))
}
