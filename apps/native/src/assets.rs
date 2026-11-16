//! Files compiled into the binary, served to GPUI by path.
//!
//! The logos come from svgl.app (https://svgl.app); they are the trademarks
//! of their owners, shown only to name the agent a message goes to. Geist and
//! Geist Mono are under the SIL Open Font License 1.1 (assets/fonts/OFL.txt).
//! The file icons are Catppuccin's, MIT (assets/file-icons/LICENSE), served
//! by `file_icons`.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

const FILES: &[(&str, &[u8])] = &[
    ("icons/claude.svg", include_bytes!("../assets/icons/claude.svg")),
    ("icons/codex.svg", include_bytes!("../assets/icons/codex.svg")),
    // The other app's logo, at the top of the project rail.
    ("platypus.png", include_bytes!("../assets/platypus.png")),
];

/// Geist and Geist Mono in the weights the app uses.
pub const FONTS: &[&[u8]] = &[
    include_bytes!("../assets/fonts/Geist-Regular.ttf"),
    include_bytes!("../assets/fonts/Geist-Medium.ttf"),
    include_bytes!("../assets/fonts/Geist-SemiBold.ttf"),
    // The preview tab's title.
    include_bytes!("../assets/fonts/Geist-Italic.ttf"),
    include_bytes!("../assets/fonts/GeistMono-Regular.ttf"),
    include_bytes!("../assets/fonts/GeistMono-Medium.ttf"),
    include_bytes!("../assets/fonts/GeistMono-SemiBold.ttf"),
];

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(icon) = crate::file_icons::load(path) {
            return Ok(Some(Cow::Borrowed(icon)));
        }
        Ok(FILES
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(FILES
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| SharedString::from(*name))
            .collect())
    }
}
