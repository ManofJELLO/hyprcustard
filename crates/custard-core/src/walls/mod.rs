//! The Wallpaper page: browsing the user's wallpapers and putting one on
//! screen with whichever wallpaper program they use.
//!
//! What is on screen now is read by [`crate::colors::wallpaper`], which the
//! Colors page shares.

pub mod library;
pub mod setter;
pub mod startup;
pub mod thumbs;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::colors::wallpaper::{Found, Wallpaper};

/// The page's choices, kept in `settings.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WallSettings {
    /// Folders the user added.
    pub folders: Vec<String>,
    /// Found folders the user took off the list.
    pub hidden: Vec<String>,
    /// The program to show wallpapers with; empty for whichever is running.
    pub program: String,
    /// `fill`, `fit` or `stretch`.
    pub fit: String,
    /// Rebuild the desktop's colors from each wallpaper picked.
    pub recolor: bool,
    /// The user's own mpvpaper command, as last seen running, so their
    /// options are kept after another program has shown wallpapers.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mpvpaper: Vec<String>,
    /// Choices for mpvpaper made on the page.
    pub mpv: MpvChoices,
}

/// Choices for mpvpaper made on the page, laid over whatever command it is
/// started with — the user's own or hyprcustard's — so the rest of it stays
/// as they wrote it. `None` leaves that part of the command alone.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MpvChoices {
    /// Seconds before the next file of a folder (mpvpaper's slideshow), or
    /// 0 for none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_every: Option<u32>,
    /// Play a folder in a random order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shuffle: Option<bool>,
}

impl Default for WallSettings {
    fn default() -> Self {
        WallSettings {
            folders: Vec::new(),
            hidden: Vec::new(),
            program: String::new(),
            fit: "fill".into(),
            recolor: false,
            mpvpaper: Vec::new(),
            mpv: MpvChoices::default(),
        }
    }
}

impl WallSettings {
    /// Unknown values replaced by the defaults.
    pub fn sanitized(mut self) -> Self {
        if !["fill", "fit", "stretch"].contains(&self.fit.as_str()) {
            self.fit = "fill".into();
        }
        if !self.program.is_empty() && setter::get(&self.program).is_none() {
            self.program.clear();
        }
        self
    }
}

/// What is on screen, and what is drawing it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scan {
    /// The wallpapers, as the Colors page reads them.
    pub found: Found,
    /// Which of [`setter::PROGRAMS`] are running in this session.
    pub running: Vec<&'static str>,
    /// Enabled systemd user services that start a wallpaper.
    pub services: Vec<String>,
    /// The wallpaper programs' processes, with their command lines.
    pub procs: Vec<(u32, Vec<String>)>,
}

/// Look at the session: a few `hyprctl` calls and a pass over `/proc`, so
/// not for the UI thread.
pub fn scan() -> Scan {
    let procs: Vec<(u32, Vec<String>)> = crate::colors::wallpaper::session_processes(true)
        .into_iter()
        .filter(|(_, argv)| {
            let name = crate::colors::wallpaper::program(argv);
            setter::PROGRAMS.iter().any(|p| p.process == name)
        })
        .collect();
    Scan {
        found: crate::colors::wallpaper::detect(),
        running: setter::running(&procs),
        services: startup::services(),
        procs,
    }
}

/// `found` with `path` on `monitor` (or every monitor), unless it shows it
/// already — for reading colors from a wallpaper just set, before its
/// program can be asked about it.
pub fn with_set(found: &Found, program: &str, monitor: Option<&str>, path: &Path) -> Found {
    if found.wallpapers.iter().any(|w| w.path == path || w.showing.as_deref() == Some(path)) {
        return found.clone();
    }
    let mut out = found.clone();
    out.wallpapers.retain(|w| monitor.is_some() && w.monitor.as_deref() != monitor);
    out.wallpapers.insert(
        0,
        Wallpaper {
            program: program.into(),
            monitor: monitor.map(String::from),
            path: path.to_path_buf(),
            showing: None,
        },
    );
    out
}

/// hyprcustard's own hyprpaper config, for the hyprpaper it starts.
pub fn hyprpaper_conf() -> PathBuf {
    crate::paths::app_config_dir().join("wallpaper").join("hyprpaper.conf")
}

/// The program to use: the one chosen, else the one running, else the
/// first one installed.
pub fn pick_program(chosen: &str, running: &[&str]) -> Option<setter::Program> {
    if let Some(p) = setter::get(chosen) {
        return Some(p);
    }
    if let Some(p) = running.first().and_then(|id| setter::get(id)) {
        return Some(p);
    }
    setter::PROGRAMS.iter().copied().find(setter::installed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default_and_sanitize() {
        let s = WallSettings { fit: "zoom".into(), program: "feh".into(), ..Default::default() };
        let s = s.sanitized();
        assert_eq!(s.fit, "fill");
        assert!(s.program.is_empty());
        let s: WallSettings = toml::from_str("recolor = true\n").unwrap();
        assert!(s.recolor);
        assert_eq!(s.fit, "fill");
    }

    #[test]
    fn a_wallpaper_just_set_is_read_before_its_program_reports_it() {
        let w = |m: Option<&str>, p: &str| Wallpaper {
            program: "swaybg".into(),
            monitor: m.map(String::from),
            path: PathBuf::from(p),
            showing: None,
        };
        let found = Found {
            wallpapers: vec![w(Some("DP-1"), "/a.png"), w(Some("HDMI-A-1"), "/b.png")],
            ..Found::default()
        };
        let one = with_set(&found, "swaybg", Some("DP-1"), Path::new("/new.png"));
        assert_eq!(one.for_monitor("DP-1").unwrap().path, PathBuf::from("/new.png"));
        assert_eq!(one.for_monitor("HDMI-A-1").unwrap().path, PathBuf::from("/b.png"));
        let all = with_set(&found, "awww", None, Path::new("/new.png"));
        assert_eq!(all.wallpapers.len(), 1);
        assert_eq!(with_set(&found, "swaybg", None, Path::new("/b.png")), found);
    }

    #[test]
    fn the_program_chosen_wins_then_the_one_running() {
        assert_eq!(pick_program("swaybg", &["mpvpaper"]).unwrap().id, "swaybg");
        assert_eq!(pick_program("", &["mpvpaper"]).unwrap().id, "mpvpaper");
    }
}
