//! One color scheme for the whole desktop.
//!
//! A scheme comes from one of three places — a named palette, the wallpaper,
//! or a color the user picks — and is written for each app the user turns
//! on (see [`targets`]). The wallpaper path reads whichever wallpaper program
//! is running ([`wallpaper`]), samples it with ffmpeg ([`extract`]) and
//! builds a Material scheme from it ([`scheme`]), the way matugen does,
//! without needing matugen.
//!
//! Building and applying are separate. Building only reads, so the page can
//! show a scheme before anything changes; applying writes the files and asks
//! each app to reload. The last scheme applied is kept in
//! `~/.config/hyprcustard/colors/current.json`, which the app's own window
//! can wear.

pub mod adopt;
pub mod extract;
pub mod follow;
pub mod named;
pub mod scheme;
pub mod targets;
pub mod toolkits;
pub mod wallpaper;

use std::path::{Path, PathBuf};

use material_colors::color::Rgb;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
pub use named::Named;
pub use scheme::Scheme;

/// What the user chose on the Colors page. Stored in `settings.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorSettings {
    /// `scheme:<id>`, `wallpaper`, or `color:#rrggbb`.
    pub source: String,
    /// How a seed becomes a scheme: one of [`scheme::STYLES`].
    pub style: String,
    /// Dark surfaces rather than light, for a wallpaper or a picked color.
    /// A named palette is whichever it is.
    pub dark: bool,
    /// -1 (softer) to 1 (sharper).
    pub contrast: f64,
    /// Which of a wallpaper's colors to build on: `prominent` or `colorful`.
    pub pick: String,
    /// The monitor whose wallpaper counts; empty for the first one.
    pub monitor: String,
    /// When the wallpaper plays from a folder, read the whole folder rather
    /// than only what is showing — one scheme that suits every clip.
    pub whole_folder: bool,
    /// Opacity for bar and popup backgrounds, 0.3 to 1.
    pub opacity: f64,
    /// Targets turned off. Everything else that is installed is written.
    pub off: Vec<String>,
    /// While following the wallpaper, the shortest time between two changes
    /// of colors, in minutes, so a fast slideshow does not make the desktop
    /// flicker. 0 to 240.
    pub min_minutes: u32,
    /// How the watcher is started at login: `hyprland` or `systemd`.
    pub start_with: String,
}

impl Default for ColorSettings {
    fn default() -> Self {
        Self {
            source: "scheme:gruvbox-dark".into(),
            style: "tonal-spot".into(),
            dark: true,
            contrast: 0.0,
            pick: "prominent".into(),
            monitor: String::new(),
            whole_folder: false,
            opacity: 1.0,
            off: Vec::new(),
            min_minutes: 5,
            start_with: "hyprland".into(),
        }
    }
}

impl ColorSettings {
    /// Bring anything out of range back into it.
    pub fn sanitized(mut self) -> Self {
        self.contrast =
            if self.contrast.is_finite() { self.contrast.clamp(-1.0, 1.0) } else { 0.0 };
        self.opacity = if self.opacity.is_finite() { self.opacity.clamp(0.3, 1.0) } else { 1.0 };
        if !scheme::STYLES.iter().any(|(id, ..)| *id == self.style) {
            self.style = "tonal-spot".into();
        }
        if !matches!(self.pick.as_str(), "prominent" | "colorful") {
            self.pick = "prominent".into();
        }
        self.min_minutes = self.min_minutes.min(240);
        if !matches!(self.start_with.as_str(), "hyprland" | "systemd") {
            self.start_with = "hyprland".into();
        }
        self
    }
}

/// A scheme built and ready to look at, with how it was arrived at.
#[derive(Debug, Clone, PartialEq)]
pub struct Built {
    /// The scheme.
    pub scheme: Scheme,
    /// A wallpaper's best seed colors, best first; empty for other sources.
    pub candidates: Vec<String>,
    /// The wallpaper read, when the source is the wallpaper.
    pub wallpaper: Option<wallpaper::Wallpaper>,
    /// One line on where the colors came from, for the page.
    pub from: String,
    /// The monitors, when the wallpaper was looked for.
    pub monitors: Vec<String>,
}

/// Build the scheme the settings describe. Reads only.
///
/// For the wallpaper this runs ffmpeg, which takes a moment — call it off
/// the UI thread.
pub fn build(settings: &ColorSettings, palettes: &[Named]) -> Result<Built> {
    let s = settings.clone().sanitized();
    if let Some(id) = s.source.strip_prefix("scheme:") {
        let n = palettes
            .iter()
            .find(|n| n.id == id)
            .ok_or_else(|| Error::other(format!("there is no color scheme called `{id}`")))?;
        return Ok(Built {
            scheme: n.scheme(),
            candidates: Vec::new(),
            wallpaper: None,
            from: format!("The {} palette", n.name),
            monitors: Vec::new(),
        });
    }
    if let Some(hex) = s.source.strip_prefix("color:") {
        let seed =
            scheme::parse(hex).ok_or_else(|| Error::other(format!("{hex:?} is not a color")))?;
        return Ok(Built {
            scheme: scheme::from_seed(&scheme::hex(seed), seed, &s.style, s.dark, s.contrast),
            candidates: Vec::new(),
            wallpaper: None,
            from: format!("Built from {}", scheme::hex(seed)),
            monitors: Vec::new(),
        });
    }
    if s.source != "wallpaper" {
        return Err(Error::other(format!("unknown color source `{}`", s.source)));
    }
    build_from(&s, &wallpaper::detect())
}

/// [`build`] from the wallpaper, with what is on screen already known.
pub fn build_from(settings: &ColorSettings, found: &wallpaper::Found) -> Result<Built> {
    let s = settings.clone().sanitized();
    let w = found.for_monitor(&s.monitor).cloned().ok_or_else(|| {
        let why = if found.unreadable.is_empty() {
            "No wallpaper program is running that hyprcustard can read — it reads \
             mpvpaper, swaybg, hyprpaper, swww, awww and wpaperd."
                .to_string()
        } else {
            format!(
                "Something is drawing the background ({}), but hyprcustard cannot tell \
                 which file it is showing. It reads mpvpaper, swaybg, hyprpaper, swww, \
                 awww and wpaperd.",
                found.unreadable.join(", ")
            )
        };
        Error::other(why)
    })?;
    let (pixels, what) = match (w.folder(), w.file()) {
        (Some(dir), _) if s.whole_folder => {
            (extract::sample_folder(dir)?, display(dir) + " (every file)")
        }
        (_, Some(file)) => (extract::sample(file)?, display(file)),
        (Some(dir), None) => (extract::sample_folder(dir)?, display(dir) + " (every file)"),
        (None, None) => {
            return Err(Error::other(format!(
                "{} is showing {}, which is not there any more",
                w.program,
                crate::paths::contract(&w.path)
            )))
        }
    };
    let ranked = extract::candidates(&pixels);
    let seed = extract::pick(&ranked, &s.pick);
    let on = w.monitor.clone().map(|m| format!(" on {m}")).unwrap_or_default();
    Ok(Built {
        scheme: scheme::from_seed(&what, seed, &s.style, s.dark, s.contrast),
        candidates: ranked.into_iter().map(scheme::hex).collect(),
        from: format!("{what}, shown by {}{on}", w.program),
        wallpaper: Some(w),
        monitors: found.monitors.clone(),
    })
}

fn display(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| crate::paths::contract(p))
}

/// Where the last applied scheme is kept.
pub fn current_path() -> PathBuf {
    crate::paths::app_config_dir().join("colors").join("current.json")
}

/// The last applied scheme, if there is one.
pub fn current() -> Option<Scheme> {
    let text = std::fs::read_to_string(current_path()).ok()?;
    serde_json::from_str(&text).ok()
}

/// What applying did, target by target.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Applied {
    /// Targets whose file was written, with how the reload went, if there
    /// was one.
    pub written: Vec<(String, Option<String>)>,
    /// Written, but the app's config does not load the file yet.
    pub unwired: Vec<String>,
    /// Targets that could not be written, with why.
    pub failed: Vec<(String, String)>,
}

impl Applied {
    /// One line for a notification.
    pub fn summary(&self, scheme: &str) -> String {
        let names: Vec<&str> = self.written.iter().map(|(n, _)| n.as_str()).collect();
        let mut out = if names.is_empty() {
            format!("Saved {scheme} — no apps are turned on")
        } else {
            format!("Applied {scheme} to {}", names.join(", "))
        };
        if !self.unwired.is_empty() {
            out.push_str(&format!(
                ". {} not load the colors yet — see the Colors page",
                if self.unwired.len() == 1 {
                    format!("{} does", self.unwired[0])
                } else {
                    format!("{} do", self.unwired.join(", "))
                }
            ));
        }
        for (name, why) in &self.failed {
            out.push_str(&format!(". {name}: {why}"));
        }
        let trouble: Vec<String> = self
            .written
            .iter()
            .filter_map(|(n, r)| {
                r.as_ref().filter(|r| r.starts_with("could not")).map(|r| format!("{n} {r}"))
            })
            .collect();
        if !trouble.is_empty() {
            out.push_str(&format!(". {}", trouble.join("; ")));
        }
        out
    }
}

/// Write a scheme for every target turned on, reload the apps that load it,
/// and keep it as the current scheme.
pub fn apply(
    built: &Scheme,
    settings: &ColorSettings,
    list: &[targets::Target],
) -> Result<Applied> {
    let path = current_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    }
    crate::paths::write_atomic(&path, &serde_json::to_string_pretty(built)?)?;

    let mut out = Applied::default();
    for t in list.iter().filter(|t| t.installed && !settings.off.iter().any(|o| o == t.id)) {
        if let Err(e) = targets::write(t, built, settings.opacity) {
            out.failed.push((t.name.to_string(), e.to_string()));
            continue;
        }
        if t.wired {
            out.written.push((t.name.to_string(), targets::reload(t, built)));
        } else {
            out.written.push((t.name.to_string(), None));
            out.unwired.push(t.name.to_string());
        }
    }
    Ok(out)
}

/// A seed color from text, for a picked color.
pub fn seed(text: &str) -> Option<Rgb> {
    scheme::parse(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_named_source_builds_without_touching_anything() {
        let b = build(&ColorSettings::default(), &named::builtins()).unwrap();
        assert_eq!(b.scheme.name, "Gruvbox Dark");
        assert!(b.candidates.is_empty());
        assert!(b.wallpaper.is_none());
    }

    #[test]
    fn a_picked_color_builds_in_the_chosen_mode() {
        let s = ColorSettings { source: "color:#6750a4".into(), dark: false, ..Default::default() };
        let b = build(&s, &[]).unwrap();
        assert!(!b.scheme.dark);
        assert_eq!(b.scheme.source, "#6750a4");
    }

    #[test]
    fn bad_sources_are_explained() {
        let e = build(&ColorSettings { source: "scheme:nope".into(), ..Default::default() }, &[])
            .unwrap_err();
        assert!(e.to_string().contains("nope"));
        let e = build(&ColorSettings { source: "color:blue".into(), ..Default::default() }, &[])
            .unwrap_err();
        assert!(e.to_string().contains("blue"));
        assert!(build(&ColorSettings { source: "moon".into(), ..Default::default() }, &[]).is_err());
    }

    #[test]
    fn settings_are_kept_in_range() {
        let s = ColorSettings {
            contrast: 9.0,
            opacity: 0.0,
            style: "plaid".into(),
            pick: "loudest".into(),
            ..Default::default()
        }
        .sanitized();
        assert_eq!((s.contrast, s.opacity), (1.0, 0.3));
        assert_eq!(s.style, "tonal-spot");
        assert_eq!(s.pick, "prominent");
    }

    #[test]
    fn apply_writes_only_installed_targets_that_are_on() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("config");
        let mut list = targets::all(&home, &home.join("hypr").join("hyprland.lua"));
        for t in &mut list {
            // Pretend every app is installed and none is running or wired,
            // so nothing is signaled.
            t.installed = true;
            t.wired = false;
        }
        let settings = ColorSettings { off: vec!["kitty".into()], ..Default::default() };
        let scheme = named::builtins()[0].scheme();
        let done = apply(&scheme, &settings, &list).unwrap();
        assert!(done.written.iter().all(|(n, r)| n != "kitty" && r.is_none()));
        assert_eq!(done.written.len(), list.len() - 1);
        assert!(!list.iter().find(|t| t.id == "kitty").unwrap().file.exists());
        assert!(list.iter().find(|t| t.id == "waybar").unwrap().file.is_file());
        assert_eq!(current().unwrap(), scheme);
        assert!(done.summary("Gruvbox Dark").contains("not load the colors yet"));
    }

    #[test]
    fn the_summary_reads_naturally() {
        let a = Applied {
            written: vec![("Hyprland".into(), Some("reloaded".into())), ("Waybar".into(), None)],
            unwired: vec!["Waybar".into()],
            failed: vec![],
        };
        assert_eq!(
            a.summary("Nord"),
            "Applied Nord to Hyprland, Waybar. Waybar does not load the colors yet — see the Colors page"
        );
    }
}
