//! From the capture names a grammar's rules use to the colours Catppuccin
//! gives them.
//!
//! Highlighting stores a `HighlightId` — a slot in `KEYS` — not a colour, so
//! the same result draws in Mocha or Latte without highlighting again.
//! Colours follow Catppuccin's style guide
//! (https://github.com/catppuccin/catppuccin/blob/main/docs/style-guide.md).

use gpui::{FontStyle, HighlightStyle, Hsla};

use crate::theme::{IconFlavour, hex};

/// A slot in `KEYS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HighlightId(pub u16);

/// The capture names coloured, most specific forms included. A capture
/// takes the longest of these that it starts with.
const KEYS: &[&str] = &[
    "attribute",
    "boolean",
    "comment",
    "constant",
    "constant.builtin",
    "constructor",
    "embedded",
    "escape",
    "function",
    "function.builtin",
    "function.macro",
    "keyword",
    "label",
    "module",
    "number",
    "operator",
    "property",
    "punctuation",
    "string",
    "string.special",
    "tag",
    "type",
    "type.builtin",
    "variable.builtin",
    "variable.parameter",
];

pub fn key(id: HighlightId) -> &'static str {
    KEYS[id.0 as usize]
}

/// Catppuccin's accents for one flavour.
struct Accents {
    rosewater: u32,
    pink: u32,
    mauve: u32,
    red: u32,
    maroon: u32,
    peach: u32,
    yellow: u32,
    green: u32,
    teal: u32,
    sky: u32,
    blue: u32,
    lavender: u32,
    text: u32,
    overlay2: u32,
}

const MOCHA: Accents = Accents {
    rosewater: 0xf5e0dc,
    pink: 0xf5c2e7,
    mauve: 0xcba6f7,
    red: 0xf38ba8,
    maroon: 0xeba0ac,
    peach: 0xfab387,
    yellow: 0xf9e2af,
    green: 0xa6e3a1,
    teal: 0x94e2d5,
    sky: 0x89dceb,
    blue: 0x89b4fa,
    lavender: 0xb4befe,
    text: 0xcdd6f4,
    overlay2: 0x9399b2,
};

const LATTE: Accents = Accents {
    rosewater: 0xdc8a78,
    pink: 0xea76cb,
    mauve: 0x8839ef,
    red: 0xd20f39,
    maroon: 0xe64553,
    peach: 0xfe640b,
    yellow: 0xdf8e1d,
    green: 0x40a02b,
    teal: 0x179299,
    sky: 0x04a5e5,
    blue: 0x1e66f5,
    lavender: 0x7287fd,
    text: 0x4c4f69,
    overlay2: 0x7c7f93,
};

/// How `id` is drawn in the theme's flavour.
pub fn style(id: HighlightId, flavour: IconFlavour) -> HighlightStyle {
    let c = match flavour {
        IconFlavour::Mocha => &MOCHA,
        IconFlavour::Latte => &LATTE,
    };
    let color = match key(id) {
        "keyword" => c.mauve,
        "string" => c.green,
        "string.special" | "escape" => c.pink,
        "comment" => c.overlay2,
        "number" | "boolean" | "constant" | "constant.builtin" => c.peach,
        "operator" => c.sky,
        "punctuation" => c.overlay2,
        "function" => c.blue,
        "function.builtin" => c.red,
        "function.macro" => c.rosewater,
        "type" | "constructor" | "module" => c.yellow,
        "type.builtin" => c.yellow,
        "attribute" => c.yellow,
        "property" => c.lavender,
        "label" | "tag" => c.blue,
        "variable.builtin" => c.red,
        "variable.parameter" => c.maroon,
        "embedded" => c.text,
        _ => c.teal,
    };
    let color: Hsla = hex(color);
    HighlightStyle {
        color: Some(color),
        font_style: (key(id) == "comment").then_some(FontStyle::Italic),
        ..HighlightStyle::default()
    }
}

/// For each capture in a language's rules, the slot that colours it, or
/// `None` to leave the text as it was.
pub struct HighlightMap(Vec<Option<HighlightId>>);

impl HighlightMap {
    /// `function.method.call` tries itself, then `function.method`, then
    /// `function`.
    pub fn new(capture_names: &[&str]) -> Self {
        let find = |name: &str| {
            let mut candidate = name;
            loop {
                if let Some(ix) = KEYS.iter().position(|key| *key == candidate) {
                    return Some(HighlightId(ix as u16));
                }
                candidate = &candidate[..candidate.rfind('.')?];
            }
        };
        Self(capture_names.iter().map(|name| find(name)).collect())
    }

    pub fn get(&self, capture: u32) -> Option<HighlightId> {
        self.0[capture as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_capture_takes_its_most_specific_key() {
        let map = HighlightMap::new(&["function.method.call", "function.builtin", "spell"]);
        assert_eq!(key(map.get(0).unwrap()), "function");
        assert_eq!(key(map.get(1).unwrap()), "function.builtin");
        assert_eq!(map.get(2), None);
    }
}
