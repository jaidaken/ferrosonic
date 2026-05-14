//! Themes — built-in `Default` plus TOML files from
//! `~/.config/ferrosonic/themes/`.

use std::path::Path;

use ratatui::style::Color;
use serde::Deserialize;
use tracing::{error, info, warn};

use crate::config::paths;

#[derive(Debug, Clone, Copy)]
pub struct ThemeColors {
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub artist: Color,
    pub album: Color,
    pub song: Color,
    pub muted: Color,
    pub highlight_bg: Color,
    pub highlight_fg: Color,
    pub success: Color,
    pub error: Color,
    pub playing: Color,
    pub played: Color,
    pub border_focused: Color,
    pub border_unfocused: Color,
}

#[derive(Debug, Clone)]
pub struct ThemeData {
    pub name: String,
    pub colors: ThemeColors,
    pub cava_gradient: [String; 8],
    pub cava_horizontal_gradient: [String; 8],
}

#[derive(Deserialize)]
struct ThemeFile {
    colors: ThemeFileColors,
    cava: Option<ThemeFileCava>,
}

#[derive(Deserialize)]
struct ThemeFileColors {
    primary: String,
    secondary: String,
    accent: String,
    artist: String,
    album: String,
    song: String,
    muted: String,
    highlight_bg: String,
    highlight_fg: String,
    success: String,
    error: String,
    playing: String,
    played: String,
    border_focused: String,
    border_unfocused: String,
}

#[derive(Deserialize)]
struct ThemeFileCava {
    gradient: Option<Vec<String>>,
    horizontal_gradient: Option<Vec<String>>,
}

fn hex_to_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ) {
            return Color::Rgb(r, g, b);
        }
    }
    warn!("Invalid hex color '{}', falling back to white", hex);
    Color::White
}

fn parse_gradient(values: &[String], fallback: &[&str; 8]) -> [String; 8] {
    let mut result: [String; 8] = std::array::from_fn(|i| fallback[i].to_string());
    for (i, v) in values.iter().enumerate().take(8) {
        result[i] = v.clone();
    }
    result
}

impl ThemeData {
    fn from_file_content(name: &str, content: &str) -> Result<Self, String> {
        let file: ThemeFile = toml::from_str(content)
            .map_err(|e| format!("Failed to parse theme '{}': {}", name, e))?;

        let c = &file.colors;
        let colors = ThemeColors {
            primary: hex_to_color(&c.primary),
            secondary: hex_to_color(&c.secondary),
            accent: hex_to_color(&c.accent),
            artist: hex_to_color(&c.artist),
            album: hex_to_color(&c.album),
            song: hex_to_color(&c.song),
            muted: hex_to_color(&c.muted),
            highlight_bg: hex_to_color(&c.highlight_bg),
            highlight_fg: hex_to_color(&c.highlight_fg),
            success: hex_to_color(&c.success),
            error: hex_to_color(&c.error),
            playing: hex_to_color(&c.playing),
            played: hex_to_color(&c.played),
            border_focused: hex_to_color(&c.border_focused),
            border_unfocused: hex_to_color(&c.border_unfocused),
        };

        let default_g: [&str; 8] = [
            "#59cc33", "#cccc33", "#cc8033", "#cc5533", "#cc3333", "#bb1111", "#990000", "#990000",
        ];
        let default_h: [&str; 8] = [
            "#c45161", "#e094a0", "#f2b6c0", "#f2dde1", "#cbc7d8", "#8db7d2", "#5e62a9", "#434279",
        ];

        let cava = file.cava.as_ref();
        let cava_gradient = match cava.and_then(|c| c.gradient.as_ref()) {
            Some(g) => parse_gradient(g, &default_g),
            None => std::array::from_fn(|i| default_g[i].to_string()),
        };
        let cava_horizontal_gradient = match cava.and_then(|c| c.horizontal_gradient.as_ref()) {
            Some(h) => parse_gradient(h, &default_h),
            None => std::array::from_fn(|i| default_h[i].to_string()),
        };

        Ok(ThemeData {
            name: name.to_string(),
            colors,
            cava_gradient,
            cava_horizontal_gradient,
        })
    }

    pub fn default_theme() -> Self {
        ThemeData {
            name: "Default".to_string(),
            colors: ThemeColors {
                primary: Color::Cyan,
                secondary: Color::DarkGray,
                accent: Color::Yellow,
                artist: Color::LightGreen,
                album: Color::Magenta,
                song: Color::Magenta,
                muted: Color::Gray,
                highlight_bg: Color::Rgb(102, 51, 153),
                highlight_fg: Color::White,
                success: Color::Green,
                error: Color::Red,
                playing: Color::LightGreen,
                played: Color::Red,
                border_focused: Color::Cyan,
                border_unfocused: Color::DarkGray,
            },
            cava_gradient: [
                "#59cc33".into(),
                "#cccc33".into(),
                "#cc8033".into(),
                "#cc5533".into(),
                "#cc3333".into(),
                "#bb1111".into(),
                "#990000".into(),
                "#990000".into(),
            ],
            cava_horizontal_gradient: [
                "#c45161".into(),
                "#e094a0".into(),
                "#f2b6c0".into(),
                "#f2dde1".into(),
                "#cbc7d8".into(),
                "#8db7d2".into(),
                "#5e62a9".into(),
                "#434279".into(),
            ],
        }
    }
}

pub fn load_themes() -> Vec<ThemeData> {
    let mut themes = vec![ThemeData::default_theme()];

    if let Some(dir) = paths::themes_dir() {
        if dir.is_dir() {
            let mut entries: Vec<_> = std::fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|ext| ext == "toml"))
                .collect();
            entries.sort_by_key(|e| e.file_name());

            for entry in entries {
                let path = entry.path();
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown");
                let name = titlecase_filename(stem);

                match std::fs::read_to_string(&path) {
                    Ok(content) => match ThemeData::from_file_content(&name, &content) {
                        Ok(theme) => {
                            info!("Loaded theme '{}' from {}", name, path.display());
                            themes.push(theme);
                        }
                        Err(e) => error!("{}", e),
                    },
                    Err(e) => error!("Failed to read {}: {}", path.display(), e),
                }
            }
        }
    }

    themes
}

/// `tokyo-night` → `Tokyo Night`, `rose_pine` → `Rose Pine`.
fn titlecase_filename(s: &str) -> String {
    s.split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    let upper: String = first.to_uppercase().collect();
                    upper + chars.as_str()
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Writes the built-in TOML themes, skipping any that already exist.
pub fn seed_default_themes(dir: &Path) {
    if let Err(e) = std::fs::create_dir_all(dir) {
        error!("Failed to create themes directory: {}", e);
        return;
    }

    for (filename, content) in BUILTIN_THEMES {
        let path = dir.join(filename);
        if !path.exists() {
            #[allow(clippy::disallowed_methods)]
            // allow-direct-write: seeding built-in theme, rebuildable from defaults
            let res = std::fs::write(&path, content);
            match res {
                Err(e) => error!("Failed to write theme {}: {}", filename, e),
                Ok(()) => info!("Seeded theme file: {}", filename),
            }
        }
    }
}

use super::theme_builtins::BUILTIN_THEMES;
