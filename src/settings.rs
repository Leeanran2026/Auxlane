use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const THEMES: [&str; 3] = ["system", "light", "dark"];
const ACCENTS: [&str; 4] = ["blue", "teal", "purple", "amber"];
const LANGUAGES: [&str; 4] = ["system", "en", "zh-CN", "zh-TW"];
const REFRESH_SECONDS: [u64; 4] = [2, 3, 5, 10];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_accent")]
    pub accent: String,
    #[serde(default = "default_refresh")]
    pub refresh_seconds: u64,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub favorites: Vec<String>,
}

fn default_theme() -> String {
    "system".to_string()
}

fn default_accent() -> String {
    "blue".to_string()
}

fn default_refresh() -> u64 {
    3
}

fn default_language() -> String {
    "system".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            accent: default_accent(),
            refresh_seconds: default_refresh(),
            language: default_language(),
            favorites: Vec::new(),
        }
    }
}

impl Settings {
    pub fn sanitize(&mut self) {
        if !THEMES.contains(&self.theme.as_str()) {
            self.theme = default_theme();
        }
        if !ACCENTS.contains(&self.accent.as_str()) {
            self.accent = default_accent();
        }
        if !LANGUAGES.contains(&self.language.as_str()) {
            self.language = default_language();
        }
        if !REFRESH_SECONDS.contains(&self.refresh_seconds) {
            self.refresh_seconds = default_refresh();
        }

        let mut favorites = Vec::new();
        for id in self.favorites.drain(..) {
            if id.is_empty() || favorites.contains(&id) {
                continue;
            }
            favorites.push(id);
            if favorites.len() == 3 {
                break;
            }
        }
        self.favorites = favorites;
    }

    pub fn refresh_seconds(&self) -> u64 {
        if REFRESH_SECONDS.contains(&self.refresh_seconds) {
            self.refresh_seconds
        } else {
            default_refresh()
        }
    }
}

fn config_path() -> PathBuf {
    if let Ok(path) = std::env::var("AUXLANE_SETTINGS") {
        return PathBuf::from(path);
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Auxlane")
        .join("settings.json")
}

pub fn load() -> Settings {
    let mut settings = fs::read_to_string(config_path())
        .ok()
        .and_then(|contents| serde_json::from_str::<Settings>(&contents).ok())
        .unwrap_or_default();
    settings.sanitize();
    settings
}

pub fn save(settings: &Settings) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(contents) = serde_json::to_string_pretty(settings) {
        let _ = fs::write(path, contents);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        let settings = Settings::default();
        assert_eq!(settings.theme, "system");
        assert_eq!(settings.accent, "blue");
        assert_eq!(settings.refresh_seconds(), 3);
        assert_eq!(settings.language, "system");
    }

    #[test]
    fn invalid_values_fall_back() {
        let mut settings = Settings {
            theme: "neon".into(),
            accent: "pink".into(),
            refresh_seconds: 7,
            language: "fr".into(),
            favorites: vec![],
        };
        settings.sanitize();
        assert_eq!(settings.theme, "system");
        assert_eq!(settings.accent, "blue");
        assert_eq!(settings.refresh_seconds(), 3);
        assert_eq!(settings.language, "system");
    }

    #[test]
    fn favorites_are_unique_and_limited_to_three() {
        let mut settings = Settings {
            favorites: vec!["a".into(), "a".into(), "b".into(), "c".into(), "d".into()],
            ..Settings::default()
        };
        settings.sanitize();
        assert_eq!(settings.favorites, vec!["a", "b", "c"]);
    }
}
