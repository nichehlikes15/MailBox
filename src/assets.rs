use std::borrow::Cow;

use anyhow::Result;
use gpui::{AssetSource, SharedString};
use rust_embed::RustEmbed;

// Embeds everything in /assets (fonts, icons, themes) into the executable
// at compile time, so the app doesn't need those files next to it.
// Paths are relative to /assets, e.g. "images/add.svg", "themes/zed.json".
#[derive(RustEmbed)]
#[folder = "assets/"]
pub struct Assets;

// Lets gpui load embedded files by path (used by `svg().path(...)`).
impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }

        Ok(Self::get(path).map(|file| file.data))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(Self::iter()
            .filter(|file| file.starts_with(path))
            .map(SharedString::from)
            .collect())
    }
}
