//! The colours and fonts every window draws with: Catppuccin Mocha for dark,
//! Catppuccin Latte for light (https://catppuccin.com/palette), and the
//! fonts chosen in settings.

use gpui::{App, Hsla, WindowAppearance};

use crate::settings::{MONO_FONTS, Settings, ThemeChoice, UI_FONTS};

/// One Catppuccin flavour, by Catppuccin's own colour names.
struct Flavour {
    red: u32,
    peach: u32,
    yellow: u32,
    green: u32,
    blue: u32,
    text: u32,
    overlay1: u32,
    surface1: u32,
    surface0: u32,
    base: u32,
    mantle: u32,
    crust: u32,
}

const MOCHA: Flavour = Flavour {
    red: 0xf38ba8,
    peach: 0xfab387,
    yellow: 0xf9e2af,
    green: 0xa6e3a1,
    blue: 0x89b4fa,
    text: 0xcdd6f4,
    overlay1: 0x7f849c,
    surface1: 0x45475a,
    surface0: 0x313244,
    base: 0x1e1e2e,
    mantle: 0x181825,
    crust: 0x11111b,
};

const LATTE: Flavour = Flavour {
    red: 0xd20f39,
    peach: 0xfe640b,
    yellow: 0xdf8e1d,
    green: 0x40a02b,
    blue: 0x1e66f5,
    text: 0x4c4f69,
    overlay1: 0x8c8fa1,
    surface1: 0xbcc0cc,
    surface0: 0xccd0da,
    base: 0xeff1f5,
    mantle: 0xe6e9ef,
    crust: 0xdce0e8,
};

pub fn hex(color: u32) -> Hsla {
    gpui::rgb(color).into()
}

pub fn hex_alpha(color: u32, alpha: f32) -> Hsla {
    Hsla { a: alpha, ..hex(color) }
}

/// Which file-icon set matches the theme: Catppuccin draws its icons per
/// flavour, darker for Latte's light background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconFlavour {
    Mocha,
    Latte,
}

impl IconFlavour {
    pub fn dir(self) -> &'static str {
        match self {
            IconFlavour::Mocha => "mocha",
            IconFlavour::Latte => "latte",
        }
    }
}

/// The handful of colours a window uses, and its two fonts.
#[derive(Clone, Copy)]
pub struct Palette {
    pub background: Hsla,
    pub sidebar: Hsla,
    pub border: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub selected: Hsla,
    pub hover: Hsla,
    pub added_bg: Hsla,
    pub added_text: Hsla,
    pub removed_bg: Hsla,
    pub removed_text: Hsla,
    pub fold_bg: Hsla,
    pub danger: Hsla,
    pub warning: Hsla,
    /// A modified file's name, and a folder holding changes, in the tree.
    pub modified: Hsla,
    /// Indent guides in the tree.
    pub guide: Hsla,
    /// The one coloured control, Send, and what is written on it.
    pub accent: Hsla,
    pub on_accent: Hsla,
    /// Menus float over the base colours, so they need a distinct edge.
    pub menu_border: Hsla,
    pub selection: Hsla,
    pub ui_font: &'static str,
    pub mono_font: &'static str,
    pub icons: IconFlavour,
}

impl Palette {
    /// The palette the settings ask for, given the window's appearance for
    /// "Match System".
    pub fn current(appearance: WindowAppearance, cx: &App) -> Self {
        let settings = cx.global::<Settings>();
        let dark = match settings.theme {
            ThemeChoice::Dark => true,
            ThemeChoice::Light => false,
            ThemeChoice::System => {
                matches!(appearance, WindowAppearance::Dark | WindowAppearance::VibrantDark)
            }
        };
        let ui_font = Settings::resolve(UI_FONTS, &settings.ui_font, cx);
        let mono_font = Settings::resolve(MONO_FONTS, &settings.mono_font, cx);
        if dark {
            Self::from(&MOCHA, IconFlavour::Mocha, ui_font, mono_font)
        } else {
            Self::from(&LATTE, IconFlavour::Latte, ui_font, mono_font)
        }
    }

    fn from(f: &Flavour, icons: IconFlavour, ui_font: &'static str, mono_font: &'static str) -> Self {
        let light = icons == IconFlavour::Latte;
        Self {
            background: hex(f.base),
            sidebar: hex(f.mantle),
            border: hex(f.crust),
            text: hex(f.text),
            muted: hex(f.overlay1),
            selected: hex(f.surface0),
            hover: hex_alpha(f.surface0, 0.6),
            // Latte's colours are darker and need less tint to read.
            added_bg: hex_alpha(f.green, if light { 0.14 } else { 0.12 }),
            added_text: hex(f.green),
            removed_bg: hex_alpha(f.red, if light { 0.12 } else { 0.12 }),
            removed_text: hex(f.red),
            fold_bg: hex(f.mantle),
            danger: hex(f.red),
            warning: hex(f.peach),
            modified: hex(f.yellow),
            guide: hex_alpha(f.surface1, 0.5),
            accent: hex(f.blue),
            on_accent: hex(if light { f.base } else { f.crust }),
            menu_border: hex(f.surface1),
            selection: hex_alpha(f.blue, 0.3),
            ui_font,
            mono_font,
            icons,
        }
    }
}
