//! Which Catppuccin Mocha icon a file gets, by the same rules as Catppuccin's
//! VS Code theme: its whole name first, then its extensions from the longest
//! (`d.ts`, `test.ts`) to the shortest (`ts`).

use gpui::SharedString;

use crate::file_icons_table::{EXTENSIONS, FILE_NAMES, LATTE, MOCHA};
use crate::theme::IconFlavour;

const FALLBACK: &str = "_file";

/// The icon for `path` in the theme's flavour, as an asset path for `img()`.
pub fn icon_for(path: &str, flavour: IconFlavour) -> SharedString {
    format!("file-icons/{}/{}.svg", flavour.dir(), icon_name(path)).into()
}

/// A folder's icon, open or closed, as an asset path for `img()`.
pub fn folder_icon(open: bool, flavour: IconFlavour) -> SharedString {
    let name = if open { "_folder_open" } else { "_folder" };
    format!("file-icons/{}/{name}.svg", flavour.dir()).into()
}

/// The icon's name: `rust`, `typescript-def`, or `_file` for anything unknown.
pub fn icon_name(path: &str) -> &'static str {
    let name = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    if let Some(icon) = lookup(FILE_NAMES, &name) {
        return icon;
    }
    // Each dot after the first character starts a candidate extension, the
    // longest first. The first character is skipped so `.gitignore` is a
    // name, not an extension.
    name.char_indices()
        .skip(1)
        .filter(|(_, c)| *c == '.')
        .find_map(|(at, _)| lookup(EXTENSIONS, &name[at + 1..]))
        .unwrap_or(FALLBACK)
}

/// The SVG for `asset` (`file-icons/mocha/rust.svg`), if it is one of these.
pub fn load(asset: &str) -> Option<&'static [u8]> {
    let rest = asset.strip_prefix("file-icons/")?.strip_suffix(".svg")?;
    let (flavour, name) = rest.split_once('/')?;
    let icons = match flavour {
        "mocha" => MOCHA,
        "latte" => LATTE,
        _ => return None,
    };
    icons.iter().find(|(icon, _)| *icon == name).map(|(_, bytes)| *bytes)
}

fn lookup(table: &'static [(&'static str, &'static str)], key: &str) -> Option<&'static str> {
    table
        .binary_search_by(|(k, _)| (*k).cmp(key))
        .ok()
        .map(|ix| table[ix].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tables_are_sorted_for_binary_search() {
        assert!(FILE_NAMES.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(EXTENSIONS.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn files_get_their_icons() {
        assert_eq!(icon_name("src/main.rs"), "rust");
        assert_eq!(icon_name("app/page.tsx"), "typescript-react");
        assert_eq!(icon_name("Cargo.toml"), "cargo");
        assert_eq!(icon_name("README.md"), icon_name("readme.md"));
        assert_eq!(icon_name("types/env.d.ts"), "typescript-def");
        assert_eq!(icon_name("a.ts"), "typescript");
        assert_eq!(icon_name(".gitignore"), "git");
        assert_eq!(icon_name("changes.diff"), "diff");
    }

    #[test]
    fn an_unknown_file_gets_the_plain_icon() {
        assert_eq!(icon_name("notes.unknownext"), FALLBACK);
        assert_eq!(icon_name("Makefile.nope"), FALLBACK);
    }

    #[test]
    fn every_icon_the_tables_name_is_bundled() {
        for flavour in ["mocha", "latte"] {
            for (_, icon) in FILE_NAMES.iter().chain(EXTENSIONS) {
                assert!(load(&format!("file-icons/{flavour}/{icon}.svg")).is_some(), "{flavour}/{icon}");
            }
            assert!(load(&format!("file-icons/{flavour}/_file.svg")).is_some());
        }
    }
}
