//! Colours for the app's own window.
//!
//! By default the app wears whatever Qt is already wearing, so it matches the
//! rest of the desktop rather than announcing itself. Gruvbox dark and light
//! are built in for anyone who would rather it did. Anything else is a TOML
//! file in `~/.config/hyprcustard/themes/`, which may override as few
//! or as many of the colours as it likes and set the window's opacity.
//!
//! The system theme is the one thing this module cannot resolve on its own:
//! reading a Qt palette needs Qt. It is named here and filled in by the UI.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// The colour roles the UI uses. Every field is a `#rrggbb` string.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Palette {
    /// Window background.
    pub bg: String,
    /// Sidebar and header background.
    pub bg_alt: String,
    /// Cards, list rows, input fields.
    pub surface: String,
    /// Hovered or raised surface.
    pub surface_hi: String,
    /// Hairlines and dividers.
    pub border: String,
    /// Primary text.
    pub fg: String,
    /// Secondary text.
    pub fg_dim: String,
    /// Disabled text and placeholders.
    pub muted: String,
    /// Primary accent: focus rings, active tabs, sliders.
    pub accent: String,
    /// Secondary accent, for a second series of things.
    pub accent_alt: String,
    /// Success.
    pub ok: String,
    /// Warning.
    pub warn: String,
    /// Error.
    pub error: String,
    /// Selected row background.
    pub selection: String,
    /// Text drawn on top of `accent`.
    pub on_accent: String,
}

/// A complete theme.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    /// Identifier used in the config: a built-in name or a file stem.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Whether this is a dark theme, which decides a few shadow choices.
    pub dark: bool,
    /// Window opacity, 0.3 to 1.0. Hyprland composites the rest.
    pub opacity: f32,
    /// Take the colours from Qt's palette instead of from `colors`.
    ///
    /// Only the built-in `system` theme sets this. The engine has no way to
    /// read a Qt palette — that is the one piece of theming the UI layer has
    /// to supply — so `colors` here is a plain stand-in for anything that
    /// reads the state document without a Qt palette to hand, such as
    /// `--print-state`.
    #[serde(default)]
    pub system: bool,
    /// The colours.
    pub colors: Palette,
}

/// Gruvbox dark.
pub fn gruvbox_dark() -> Theme {
    Theme {
        id: "gruvbox-dark".into(),
        name: "Gruvbox Dark".into(),
        dark: true,
        opacity: 1.0,
        system: false,
        colors: Palette {
            bg: "#282828".into(),
            bg_alt: "#1d2021".into(),
            surface: "#32302f".into(),
            surface_hi: "#3c3836".into(),
            border: "#504945".into(),
            fg: "#ebdbb2".into(),
            fg_dim: "#d5c4a1".into(),
            muted: "#928374".into(),
            accent: "#83a598".into(),
            accent_alt: "#d3869b".into(),
            ok: "#b8bb26".into(),
            warn: "#fabd2f".into(),
            error: "#fb4934".into(),
            selection: "#3c3836".into(),
            on_accent: "#1d2021".into(),
        },
    }
}

/// Gruvbox light.
pub fn gruvbox_light() -> Theme {
    Theme {
        id: "gruvbox-light".into(),
        name: "Gruvbox Light".into(),
        dark: false,
        opacity: 1.0,
        system: false,
        colors: Palette {
            bg: "#fbf1c7".into(),
            bg_alt: "#f2e5bc".into(),
            surface: "#f9f5d7".into(),
            surface_hi: "#ebdbb2".into(),
            border: "#d5c4a1".into(),
            fg: "#3c3836".into(),
            fg_dim: "#504945".into(),
            muted: "#7c6f64".into(),
            accent: "#076678".into(),
            accent_alt: "#8f3f71".into(),
            ok: "#79740e".into(),
            warn: "#b57614".into(),
            error: "#9d0006".into(),
            selection: "#ebdbb2".into(),
            on_accent: "#fbf1c7".into(),
        },
    }
}

/// The user's Qt colours, whatever they are.
///
/// The palette this stands for is read by the UI layer, which is the only part
/// that can see one; the colours below are only what a reader with no Qt
/// palette falls back to. See [`Theme::system`].
pub fn system() -> Theme {
    Theme {
        id: "system".into(),
        name: "System (Qt)".into(),
        dark: true,
        opacity: 1.0,
        system: true,
        colors: gruvbox_dark().colors,
    }
}

/// The themes that ship with the app, the system one first.
pub fn builtin() -> Vec<Theme> {
    vec![system(), gruvbox_dark(), gruvbox_light()]
}

// ---------------------------------------------------------------------------
// User theme files
// ---------------------------------------------------------------------------

/// The on-disk shape of a theme file. Everything except `base` is optional.
///
/// Unknown keys are refused rather than ignored: a misspelled `accent_alt` that
/// silently does nothing is a worse afternoon than one that says so by name.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeFile {
    /// Display name. Defaults to the file stem.
    pub name: Option<String>,
    /// Which built-in to start from: `gruvbox-dark` or `gruvbox-light`.
    /// Defaults to `gruvbox-dark`.
    pub base: Option<String>,
    /// Override the dark/light flag.
    pub dark: Option<bool>,
    /// Window opacity, clamped to 0.3 – 1.0.
    pub opacity: Option<f32>,
    /// Colour overrides.
    #[serde(default)]
    pub colors: PaletteFile,
}

/// Optional overrides for each colour role.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)]
pub struct PaletteFile {
    pub bg: Option<String>,
    pub bg_alt: Option<String>,
    pub surface: Option<String>,
    pub surface_hi: Option<String>,
    pub border: Option<String>,
    pub fg: Option<String>,
    pub fg_dim: Option<String>,
    pub muted: Option<String>,
    pub accent: Option<String>,
    pub accent_alt: Option<String>,
    pub ok: Option<String>,
    pub warn: Option<String>,
    pub error: Option<String>,
    pub selection: Option<String>,
    pub on_accent: Option<String>,
}

/// Where user themes live.
pub fn theme_dir() -> std::path::PathBuf {
    crate::paths::app_config_dir().join("themes")
}

/// Parse a theme file's text into a theme with the given id.
pub fn from_toml(id: &str, text: &str) -> Result<Theme> {
    let file: ThemeFile = toml::from_str(text)?;

    let mut theme = match file.base.as_deref() {
        Some("gruvbox-light") => gruvbox_light(),
        Some("gruvbox-dark") | None => gruvbox_dark(),
        Some(other) => {
            return Err(Error::other(format!(
                "theme `{id}` has base = \"{other}\", which is not a built-in theme \
                 (use gruvbox-dark or gruvbox-light)"
            )))
        }
    };

    theme.id = id.to_string();
    theme.name = file.name.unwrap_or_else(|| crate::model::base_name(id).to_string());
    if let Some(d) = file.dark {
        theme.dark = d;
    }
    if let Some(o) = file.opacity {
        theme.opacity = o.clamp(0.3, 1.0);
    }

    let c = &file.colors;
    let mut bad = Vec::new();
    let mut set = |slot: &mut String, value: &Option<String>, role: &str| {
        if let Some(v) = value {
            if is_hex_color(v) {
                *slot = normalize_hex(v);
            } else {
                bad.push(format!("{role} = \"{v}\""));
            }
        }
    };

    set(&mut theme.colors.bg, &c.bg, "bg");
    set(&mut theme.colors.bg_alt, &c.bg_alt, "bg_alt");
    set(&mut theme.colors.surface, &c.surface, "surface");
    set(&mut theme.colors.surface_hi, &c.surface_hi, "surface_hi");
    set(&mut theme.colors.border, &c.border, "border");
    set(&mut theme.colors.fg, &c.fg, "fg");
    set(&mut theme.colors.fg_dim, &c.fg_dim, "fg_dim");
    set(&mut theme.colors.muted, &c.muted, "muted");
    set(&mut theme.colors.accent, &c.accent, "accent");
    set(&mut theme.colors.accent_alt, &c.accent_alt, "accent_alt");
    set(&mut theme.colors.ok, &c.ok, "ok");
    set(&mut theme.colors.warn, &c.warn, "warn");
    set(&mut theme.colors.error, &c.error, "error");
    set(&mut theme.colors.selection, &c.selection, "selection");
    set(&mut theme.colors.on_accent, &c.on_accent, "on_accent");

    if !bad.is_empty() {
        return Err(Error::other(format!(
            "theme `{id}`: not a colour — {}. Use #rgb, #rrggbb or #aarrggbb.",
            bad.join(", ")
        )));
    }

    Ok(theme)
}

/// Load every theme: the built-ins, then any `.toml` in the theme directory.
///
/// A broken theme file is skipped and reported rather than stopping the others.
pub fn load_all() -> (Vec<Theme>, Vec<String>) {
    let mut themes = builtin();
    let mut problems = Vec::new();

    let dir = theme_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return (themes, problems);
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().map(|e| e != "toml").unwrap_or(true) {
            continue;
        }
        let id = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        match std::fs::read_to_string(&path) {
            Ok(text) => match from_toml(&id, &text) {
                Ok(t) => {
                    themes.retain(|x| x.id != t.id);
                    themes.push(t);
                }
                Err(e) => problems.push(e.to_string()),
            },
            Err(e) => problems.push(format!("{}: {e}", crate::paths::contract(&path))),
        }
    }

    (themes, problems)
}

/// Find a theme by id, falling back to Gruvbox dark.
pub fn resolve(id: &str) -> Theme {
    let (themes, _) = load_all();
    themes.into_iter().find(|t| t.id == id).unwrap_or_else(system)
}

/// Write a documented example theme file, so there is something to copy.
pub fn write_example() -> Result<std::path::PathBuf> {
    let dir = theme_dir();
    std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
    let path = dir.join("example.toml");
    crate::paths::write_atomic(&path, EXAMPLE_THEME)?;
    Ok(path)
}

/// The contents of the example theme file.
pub const EXAMPLE_THEME: &str = r##"# A theme for hyprcustard.
#
# Drop this in ~/.config/hyprcustard/themes/ under any name; the file
# stem is the theme's id. Everything except `base` is optional — anything you
# leave out keeps the base theme's value.

name = "Example"
base = "gruvbox-dark"     # gruvbox-dark or gruvbox-light
dark = true               # affects shadows and a few contrast choices
opacity = 0.96            # 0.3 – 1.0; Hyprland composites the rest

[colors]
bg         = "#282828"    # window background
bg_alt     = "#1d2021"    # sidebar and header
surface    = "#32302f"    # cards, rows, inputs
surface_hi = "#3c3836"    # hover
border     = "#504945"    # hairlines
fg         = "#ebdbb2"    # primary text
fg_dim     = "#d5c4a1"    # secondary text
muted      = "#928374"    # placeholders, disabled
accent     = "#83a598"    # focus rings, sliders, active tab
accent_alt = "#d3869b"    # second accent
ok         = "#b8bb26"
warn       = "#fabd2f"
error      = "#fb4934"
selection  = "#3c3836"
on_accent  = "#1d2021"    # text drawn on the accent colour
"##;

// ---------------------------------------------------------------------------
// Colour helpers
// ---------------------------------------------------------------------------

/// True for `#rgb`, `#rrggbb` and `#aarrggbb`.
pub fn is_hex_color(s: &str) -> bool {
    let Some(hex) = s.strip_prefix('#') else { return false };
    matches!(hex.len(), 3 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
}

/// Expand `#abc` to `#aabbcc`; leave longer forms alone, lowercased.
pub fn normalize_hex(s: &str) -> String {
    let hex = s.trim_start_matches('#').to_lowercase();
    if hex.len() == 3 {
        let mut out = String::with_capacity(7);
        out.push('#');
        for c in hex.chars() {
            out.push(c);
            out.push(c);
        }
        out
    } else {
        format!("#{hex}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_distinct_and_named() {
        let b = builtin();
        let ids: Vec<&str> = b.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["system", "gruvbox-dark", "gruvbox-light"]);
        assert!(b.iter().all(|t| !t.name.is_empty()));
        // The two Gruvboxes are a dark/light pair and must not look alike.
        assert!(b[1].dark);
        assert!(!b[2].dark);
        assert_ne!(b[1].colors.bg, b[2].colors.bg);
    }

    #[test]
    fn only_the_system_theme_defers_to_qt() {
        assert!(system().system);
        assert!(!gruvbox_dark().system);
        assert!(!gruvbox_light().system);
        // A theme file always names its own colours, so it never defers.
        assert!(!from_toml("mine", "base = \"gruvbox-light\"\n").unwrap().system);
    }

    #[test]
    fn the_system_theme_is_the_default_and_the_fallback() {
        assert_eq!(crate::model::Config::default().theme, "system");
        assert_eq!(resolve("no-such-theme").id, "system");
    }

    #[test]
    fn a_file_overrides_only_what_it_names() {
        let t = from_toml("mine", "base = \"gruvbox-light\"\n[colors]\naccent = \"#ff0000\"\n")
            .unwrap();
        assert_eq!(t.colors.accent, "#ff0000");
        // Untouched roles keep the base value.
        assert_eq!(t.colors.bg, gruvbox_light().colors.bg);
        assert!(!t.dark);
        assert_eq!(t.id, "mine");
    }

    #[test]
    fn short_hex_is_expanded() {
        let t = from_toml("m", "[colors]\nbg = \"#abc\"\n").unwrap();
        assert_eq!(t.colors.bg, "#aabbcc");
    }

    #[test]
    fn opacity_is_clamped() {
        assert_eq!(from_toml("m", "opacity = 4.0\n").unwrap().opacity, 1.0);
        assert_eq!(from_toml("m", "opacity = 0.0\n").unwrap().opacity, 0.3);
    }

    #[test]
    fn a_bad_colour_is_reported_with_its_role() {
        let err = from_toml("m", "[colors]\naccent = \"blue\"\n").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("accent"));
        assert!(msg.contains("blue"));
    }

    #[test]
    fn a_misspelled_colour_key_is_reported_rather_than_ignored() {
        let err = from_toml("mine", "[colors]\naccent_altt = \"#ffffff\"\n").unwrap_err();
        assert!(err.to_string().contains("accent_altt"), "{err}");
    }

    #[test]
    fn a_misspelled_top_level_key_is_reported_too() {
        let err = from_toml("mine", "opacty = 0.9\n").unwrap_err();
        assert!(err.to_string().contains("opacty"), "{err}");
    }

    #[test]
    fn an_unknown_base_is_reported() {
        assert!(from_toml("m", "base = \"nord\"\n").is_err());
    }

    #[test]
    fn the_example_theme_parses() {
        let t = from_toml("example", EXAMPLE_THEME).unwrap();
        assert_eq!(t.name, "Example");
        assert_eq!(t.opacity, 0.96);
    }

    #[test]
    fn hex_validation() {
        assert!(is_hex_color("#fff"));
        assert!(is_hex_color("#ff00aa"));
        assert!(is_hex_color("#80ff00aa"));
        assert!(!is_hex_color("#ff00a"));
        assert!(!is_hex_color("fff"));
        assert!(!is_hex_color("#gggggg"));
    }
}
