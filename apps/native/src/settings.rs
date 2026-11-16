//! The user's choices: theme, fonts, and how diffs are laid out.
//!
//! One `Settings` value lives in GPUI's globals, so every window reads the
//! same choices and re-renders when they change. It is saved as JSON in
//! `~/.onlydiffs/native-settings.json` after every change and read back at
//! start; a missing or unreadable file means the defaults.

use std::path::PathBuf;

use gpui::{App, Global};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ThemeChoice {
    /// Catppuccin Mocha.
    #[default]
    Dark,
    /// Catppuccin Latte.
    Light,
    /// Mocha or Latte, following macOS.
    System,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 3] = [ThemeChoice::Dark, ThemeChoice::Light, ThemeChoice::System];

    pub fn label(self) -> &'static str {
        match self {
            ThemeChoice::Dark => "Dark — Catppuccin Mocha",
            ThemeChoice::Light => "Light — Catppuccin Latte",
            ThemeChoice::System => "Match System",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum DiffLayout {
    /// Removed lines above the added lines that replace them.
    #[default]
    Stacked,
    /// The old version on the left, the new on the right.
    SideBySide,
}

impl DiffLayout {
    pub const ALL: [DiffLayout; 2] = [DiffLayout::Stacked, DiffLayout::SideBySide];

    pub fn label(self) -> &'static str {
        match self {
            DiffLayout::Stacked => "Stacked",
            DiffLayout::SideBySide => "Side by side",
        }
    }
}

/// A font the app can be set to: the family name GPUI loads, and what the
/// settings list calls it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontChoice {
    pub family: &'static str,
    pub label: &'static str,
    /// Shipped inside the app, so always there.
    pub bundled: bool,
}

const fn font(family: &'static str, label: &'static str) -> FontChoice {
    FontChoice { family, label, bundled: false }
}

/// Interface fonts to offer, the bundled one first. Others are offered only
/// when installed.
pub const UI_FONTS: &[FontChoice] = &[
    FontChoice { family: "Geist", label: "Geist", bundled: true },
    FontChoice { family: ".SystemUIFont", label: "SF Pro (System)", bundled: true },
    font("Inter", "Inter"),
    font("Helvetica Neue", "Helvetica Neue"),
    font("Avenir Next", "Avenir Next"),
    font("IBM Plex Sans", "IBM Plex Sans"),
];

/// Code fonts to offer, the bundled one first.
pub const MONO_FONTS: &[FontChoice] = &[
    FontChoice { family: "Geist Mono", label: "Geist Mono", bundled: true },
    font("SF Mono", "SF Mono"),
    FontChoice { family: "Menlo", label: "Menlo", bundled: true },
    font("JetBrains Mono", "JetBrains Mono"),
    font("Fira Code", "Fira Code"),
    font("Berkeley Mono", "Berkeley Mono"),
    font("IBM Plex Mono", "IBM Plex Mono"),
    font("Cascadia Code", "Cascadia Code"),
    font("Iosevka", "Iosevka"),
];

/// The fonts from `list` that this Mac can draw.
pub fn available(list: &'static [FontChoice], cx: &App) -> Vec<FontChoice> {
    // Listing every installed font is slow and the palette is worked out on
    // every frame, so the list is read once per run.
    static INSTALLED: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    let installed = INSTALLED.get_or_init(|| cx.text_system().all_font_names());
    list.iter()
        .filter(|choice| choice.bundled || installed.iter().any(|name| name == choice.family))
        .copied()
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: ThemeChoice,
    pub ui_font: String,
    pub mono_font: String,
    pub diff_layout: DiffLayout,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::Dark,
            ui_font: UI_FONTS[0].family.to_owned(),
            mono_font: MONO_FONTS[0].family.to_owned(),
            diff_layout: DiffLayout::Stacked,
        }
    }
}

impl Global for Settings {}

impl Settings {
    fn path() -> Option<PathBuf> {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".onlydiffs/native-settings.json"))
    }

    /// The saved settings, or the defaults.
    pub fn load() -> Self {
        Self::path()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn save(&self) {
        let Some(path) = Self::path() else { return };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            // Write beside it and rename, so a crash mid-write cannot leave
            // half a file to be read as "use the defaults".
            let partial = path.with_extension("json.partial");
            if std::fs::write(&partial, json).is_ok() {
                std::fs::rename(partial, path).ok();
            }
        }
    }

    /// The font family to draw `wanted` in: itself when it is one of the
    /// offered fonts, else the bundled default. A saved name for a font that
    /// was since uninstalled falls back rather than drawing in a stand-in.
    pub fn resolve(list: &'static [FontChoice], wanted: &str, cx: &App) -> &'static str {
        available(list, cx)
            .into_iter()
            .find(|choice| choice.family == wanted)
            .map_or(list[0].family, |choice| choice.family)
    }
}

/// Changes the settings, saves them, and lets every window know.
pub fn update(cx: &mut App, change: impl FnOnce(&mut Settings)) {
    let mut settings = cx.global::<Settings>().clone();
    change(&mut settings);
    settings.save();
    cx.set_global(settings);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_settings_read_back_and_missing_fields_take_defaults() {
        let settings = Settings {
            theme: ThemeChoice::Light,
            ui_font: "Inter".into(),
            mono_font: "Menlo".into(),
            diff_layout: DiffLayout::SideBySide,
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), settings);
        let partial: Settings = serde_json::from_str(r#"{"theme":"system"}"#).unwrap();
        assert_eq!(partial.theme, ThemeChoice::System);
        assert_eq!(partial.mono_font, "Geist Mono");
    }
}
