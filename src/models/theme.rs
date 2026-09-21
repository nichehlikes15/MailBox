use crate::assets::Assets;
use serde::Deserialize;
use std::sync::OnceLock;

/// Colours are parsed from hex once when the theme loads, so render code can
/// use them directly (`rgb(theme.text)`) instead of re-parsing strings for
/// every element on every frame.
#[derive(Clone, Debug)]
pub struct Theme {
    pub name: String,
    pub background: u32,
    pub surface: u32,
    pub surface_hover: u32,
    pub border: u32,
    pub text: u32,
    pub text_muted: u32,
    pub text_inactive: u32,
    pub selected: u32,
    pub selected_text: u32,
    pub selected_option: u32,
}

#[derive(Debug, Deserialize)]
struct ThemeFile {
    #[serde(default)]
    name: String,
    style: Vec<ThemeColors>,
}

#[derive(Clone, Debug)]
pub struct ThemeInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
struct ThemeColors {
    background: String,
    surface: String,
    #[serde(rename = "surface-hover")]
    surface_hover: String,
    border: String,
    text: String,
    #[serde(rename = "text-muted")]
    text_muted: String,
    #[serde(rename = "text-inactive")]
    text_inactive: String,
    selected: String,
    #[serde(rename = "selected-text")]
    selected_text: String,
    #[serde(rename = "selected-option")]
    selected_option: String,
}

// Built the first time `Theme::available()` is called, then reused.
static AVAILABLE: OnceLock<Vec<ThemeInfo>> = OnceLock::new();

impl Theme {
    pub fn load() -> Self {
        match Self::default_name() {
            Some(name) => Self::load_named(&name),
            None => {
                eprintln!("No themes found in assets/themes, using built-in fallback");
                Self::fallback()
            }
        }
    }

    /// Loads a theme by id. Never panics: an unknown or broken theme falls back
    /// to the default theme, and failing that to a built-in dark palette.
    pub fn load_named(name: &str) -> Self {
        let colors = Self::read_asset(&format!("themes/{name}.json"))
            .and_then(|file| file.style.into_iter().next());

        match colors {
            Some(colors) => Self::from_colors(name, colors),
            None => {
                eprintln!("Theme '{name}' is missing or invalid");
                match Self::default_name() {
                    Some(default) if default != name => Self::load_named(&default),
                    _ => Self::fallback(),
                }
            }
        }
    }

    /// Themes are embedded at compile time, so this list never changes; it's
    /// built once instead of re-parsing every theme file on each call.
    pub fn available() -> Vec<ThemeInfo> {
        AVAILABLE
            .get_or_init(|| {
                let mut themes = Assets::iter()
                    .filter_map(|path| path.strip_prefix("themes/").map(str::to_owned))
                    .filter_map(|path| {
                        let id = path.strip_suffix(".json")?.to_string();
                        let file = Self::read_asset(&format!("themes/{id}.json"))?;
                        if file.style.is_empty() {
                            return None;
                        }
                        Some(ThemeInfo {
                            name: if file.name.is_empty() {
                                id.clone()
                            } else {
                                file.name
                            },
                            id,
                        })
                    })
                    .collect::<Vec<_>>();
                themes.sort_by(|left, right| left.name.cmp(&right.name));
                themes
            })
            .clone()
    }

    pub fn default_name() -> Option<String> {
        let themes = Self::available();
        themes
            .iter()
            .find(|theme| theme.id == "zed")
            .or_else(|| themes.first())
            .map(|theme| theme.id.clone())
    }

    fn from_colors(name: &str, colors: ThemeColors) -> Self {
        Self {
            name: name.to_string(),
            background: parse_color(&colors.background),
            surface: parse_color(&colors.surface),
            surface_hover: parse_color(&colors.surface_hover),
            border: parse_color(&colors.border),
            text: parse_color(&colors.text),
            text_muted: parse_color(&colors.text_muted),
            text_inactive: parse_color(&colors.text_inactive),
            selected: parse_color(&colors.selected),
            selected_text: parse_color(&colors.selected_text),
            selected_option: parse_color(&colors.selected_option),
        }
    }

    // Used only if no theme file can be loaded at all, so the app still starts
    // instead of crashing.
    fn fallback() -> Self {
        Self {
            name: "fallback".to_string(),
            background: 0x1e1e1e,
            surface: 0x252526,
            surface_hover: 0x2d2d2d,
            border: 0x3c3c3c,
            text: 0xe6e6e6,
            text_muted: 0xb0b0b0,
            text_inactive: 0x707070,
            selected: 0x094771,
            selected_text: 0xffffff,
            selected_option: 0x37373d,
        }
    }

    fn read_asset(path: &str) -> Option<ThemeFile> {
        let asset = Assets::get(path)?;
        serde_json::from_slice(&asset.data)
            .map_err(|error| eprintln!("Failed to parse theme {path}: {error}"))
            .ok()
    }
}

// "#1e1e1e" -> 0x1e1e1e. A bad value becomes bright magenta so it's easy to
// spot on screen, instead of crashing the app like the old `panic!` did.
fn parse_color(value: &str) -> u32 {
    u32::from_str_radix(value.trim().trim_start_matches('#'), 16).unwrap_or_else(|_| {
        eprintln!("Invalid theme color: {value}");
        0xff00ff
    })
}
