//! The app's own settings, which cannot live in the file the app edits.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::paths;

/// Settings stored in `~/.config/hyprcustard/settings.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The Hyprland Lua config to edit.
    pub config_path: String,
    /// How many timestamped backups to keep per edited file.
    pub backups_to_keep: usize,
    /// Run `hyprctl reload` after saving the config.
    pub reload_after_save: bool,
    /// Ask the plugin to drop its shader cache after editing a `.glsl`.
    ///
    /// Off by default: the plugin already reloads a shader when its mtime
    /// changes, so this is only useful when that does not take.
    pub reload_shaders_after_edit: bool,
    /// Write a shader edit as soon as a slider settles, rather than on Apply.
    pub live_shader_edits: bool,
    /// Show Look page edits on the desktop as they are made, before saving.
    pub live_look_edits: bool,
    /// The repository the plugin page installs from with `hyprpm add`.
    ///
    /// Configurable because a fork, or a local clone, is a perfectly ordinary
    /// thing to be running.
    pub plugin_repo_url: String,
    /// The plugin's name in hyprpm, which `enable` and `disable` take.
    pub plugin_name: String,
    /// The Colors page's choices.
    pub colors: crate::colors::ColorSettings,
}

/// Where the plugin is installed from unless someone says otherwise.
pub const DEFAULT_PLUGIN_REPO: &str = "https://github.com/ManofJELLO/HyprWindowShade";

/// The plugin's name as hyprpm lists it.
pub const DEFAULT_PLUGIN_NAME: &str = "HyprWindowShade";

impl Default for Settings {
    fn default() -> Self {
        Self {
            config_path: paths::default_lua_config().to_string_lossy().into_owned(),
            backups_to_keep: 10,
            reload_after_save: false,
            reload_shaders_after_edit: false,
            live_shader_edits: true,
            live_look_edits: true,
            plugin_repo_url: DEFAULT_PLUGIN_REPO.to_string(),
            plugin_name: DEFAULT_PLUGIN_NAME.to_string(),
            colors: crate::colors::ColorSettings::default(),
        }
    }
}

impl Settings {
    /// Where the settings file lives.
    pub fn path() -> std::path::PathBuf {
        paths::app_config_dir().join("settings.toml")
    }

    /// Load settings, falling back to defaults when the file is absent.
    ///
    /// A malformed file is reported rather than silently replaced, so a typo
    /// does not quietly reset someone's config path.
    pub fn load() -> (Self, Option<String>) {
        let path = Self::path();
        match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str::<Settings>(&text) {
                Ok(s) => (s.sanitized(), None),
                Err(e) => (
                    Settings::default(),
                    Some(format!(
                        "{} could not be read ({e}); using defaults and leaving the file alone",
                        paths::contract(&path)
                    )),
                ),
            },
            Err(_) => (Settings::default(), None),
        }
    }

    /// Write settings back out.
    pub fn save(&self) -> Result<()> {
        let path = Self::path();
        let text = toml::to_string_pretty(self)
            .map_err(|e| Error::other(format!("could not encode settings: {e}")))?;
        paths::write_atomic(&path, &text)
    }

    fn sanitized(mut self) -> Self {
        self.backups_to_keep = self.backups_to_keep.clamp(0, 200);
        if self.config_path.trim().is_empty() {
            self.config_path = Settings::default().config_path;
        }
        if self.plugin_repo_url.trim().is_empty() {
            self.plugin_repo_url = DEFAULT_PLUGIN_REPO.to_string();
        }
        if self.plugin_name.trim().is_empty() {
            self.plugin_name = DEFAULT_PLUGIN_NAME.to_string();
        }
        self.colors = self.colors.sanitized();
        self
    }

    /// The config path with `~` expanded.
    pub fn resolved_config_path(&self) -> std::path::PathBuf {
        paths::expand(&self.config_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_point_at_the_hyprland_lua_config() {
        let s = Settings::default();
        assert!(s.config_path.ends_with("hyprland.lua"));
        assert_eq!(s.backups_to_keep, 10);
    }

    #[test]
    fn a_partial_file_keeps_the_other_defaults() {
        let s: Settings = toml::from_str("backups_to_keep = 3\n").unwrap();
        assert_eq!(s.backups_to_keep, 3);
        assert!(s.live_shader_edits);
    }

    #[test]
    fn absurd_values_are_clamped() {
        let s: Settings = toml::from_str("backups_to_keep = 99999\n").unwrap();
        assert_eq!(s.sanitized().backups_to_keep, 200);
    }

    #[test]
    fn settings_round_trip_through_toml() {
        let mut s = Settings::default();
        s.reload_after_save = true;
        let text = toml::to_string_pretty(&s).unwrap();
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), s);
    }
}
