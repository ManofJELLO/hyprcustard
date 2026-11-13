//! GTK and Qt: what most desktop apps draw with.
//!
//! Neither toolkit has one colors file the way a terminal does, and how far
//! a scheme reaches depends on the theme the user runs, so this module both
//! writes the files and reads enough of the desktop to say honestly what
//! they will change.
//!
//! - **GTK** reads `gtk.css` from its config folder over any theme. The file
//!   written here defines GTK's named colors (libadwaita's `window_bg_color`
//!   and friends, and GTK 3's older `theme_bg_color` set) and, for GTK 4,
//!   libadwaita's CSS variables too. Apps built on libadwaita draw with
//!   those names. A GTK 3 app, or a GTK 4 one without libadwaita, follows
//!   them only when its theme draws with the names, as adw-gtk3 does; most
//!   other themes, Adwaita's GTK 3 one included, have their colors built in.
//!   A `gtk.css` that is itself a whole theme wins over all of this.
//! - **Qt** apps take their palette from qt6ct or qt5ct when
//!   `QT_QPA_PLATFORMTHEME` points at one, and the file written here is a
//!   color scheme in their format. Kvantum is a style that paints with its
//!   own theme's colors and images, so it gets a recolored copy of the
//!   user's Kvantum theme instead.
//!
//! Open apps keep the colors they started with: neither toolkit re-reads
//! these files while running.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use material_colors::color::Rgb;
use material_colors::hct::Hct;
use regex::Regex;

use super::scheme::{self, Scheme};
use crate::error::{Error, Result};

/// The Kvantum theme hyprcustard writes.
pub const KVANTUM_THEME: &str = "HyprCustard";

/// The GTK themes that draw with the names the GTK file sets.
const ADW_GTK3: &str = "adw-gtk3";

/// What the session says about GTK and Qt.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Desktop {
    /// The GTK theme in use: gsettings, else `settings.ini`, else Adwaita.
    pub gtk_theme: String,
    /// Folders GTK themes are looked for in, in GTK's order.
    pub theme_dirs: Vec<PathBuf>,
    /// Folders Kvantum's own themes are installed in.
    pub kvantum_dirs: Vec<PathBuf>,
    /// `QT_QPA_PLATFORMTHEME`, when set.
    pub qt_platform: Option<String>,
    /// `QT_STYLE_OVERRIDE`, when set.
    pub qt_style: Option<String>,
}

impl Desktop {
    /// Read it from this session.
    pub fn read(config_home: &Path) -> Desktop {
        let home = dirs::home_dir().unwrap_or_default();
        let data_home = std::env::var_os("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"));
        let data_dirs: Vec<PathBuf> = std::env::var("XDG_DATA_DIRS")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "/usr/local/share:/usr/share".into())
            .split(':')
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .collect();
        let mut theme_dirs = vec![data_home.join("themes"), home.join(".themes")];
        theme_dirs.extend(data_dirs.iter().map(|d| d.join("themes")));
        let gtk_theme = gsettings_get("gtk-theme")
            .or_else(|| {
                let text =
                    std::fs::read_to_string(config_home.join("gtk-3.0/settings.ini")).ok()?;
                ini_get(&text, "Settings", "gtk-theme-name")
            })
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "Adwaita".into());
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        Desktop {
            gtk_theme,
            theme_dirs,
            kvantum_dirs: data_dirs.iter().map(|d| d.join("Kvantum")).collect(),
            qt_platform: env("QT_QPA_PLATFORMTHEME"),
            qt_style: env("QT_STYLE_OVERRIDE"),
        }
    }

    /// The folder of an installed GTK theme.
    pub fn find_theme(&self, name: &str) -> Option<PathBuf> {
        self.theme_dirs.iter().map(|d| d.join(name)).find(|d| d.join("index.theme").is_file())
    }

    /// Whether the GTK theme in use draws with GTK's named colors, so GTK 3
    /// apps (and GTK 4 ones without libadwaita) follow the scheme.
    pub fn gtk_theme_follows(&self) -> bool {
        if self.gtk_theme.starts_with(ADW_GTK3) {
            return true;
        }
        let Some(dir) = self.find_theme(&self.gtk_theme) else { return false };
        let css = ["gtk-3.0/gtk.css", "gtk-3.0/gtk-dark.css"]
            .iter()
            .find_map(|f| std::fs::read_to_string(dir.join(f)).ok())
            .unwrap_or_default();
        draws_with_names(&css)
    }

    /// The adw-gtk3 theme to switch to, when it is installed.
    pub fn adw_gtk3(&self, dark: bool) -> Option<&'static str> {
        let name = if dark { "adw-gtk3-dark" } else { ADW_GTK3 };
        self.find_theme(name).map(|_| name)
    }

    /// Whether Qt apps read qt5ct/qt6ct's settings. Both plugins answer to
    /// either name.
    pub fn qt_uses_ct(&self) -> bool {
        matches!(self.qt_platform.as_deref(), Some("qt5ct" | "qt6ct"))
    }
}

/// Whether a GTK stylesheet's rules refer to the named colors rather than
/// having their own built in: twenty uses outside `@define-color` lines.
fn draws_with_names(css: &str) -> bool {
    static NAMES: OnceLock<Regex> = OnceLock::new();
    let re = NAMES.get_or_init(|| {
        Regex::new(
            r"@(theme_bg_color|theme_base_color|theme_selected_bg_color|window_bg_color|view_bg_color|accent_bg_color|headerbar_bg_color)\b",
        )
        .unwrap()
    });
    css.lines()
        .filter(|l| !l.trim_start().starts_with("@define-color"))
        .map(|l| re.find_iter(l).count())
        .sum::<usize>()
        >= 20
}

/// Whether a `gtk.css` is a whole theme rather than a few tweaks: it pulls
/// in a theme's stylesheet, or carries a hundred colors of its own. GTK
/// draws with it over anything else, so its colors win.
pub fn whole_theme(css: &str, own_file: &str) -> bool {
    let imports_theme = css.lines().any(|l| {
        let t = l.trim_start();
        t.starts_with("@import") && !t.contains(own_file) && t.contains(".css")
    });
    static HEX: OnceLock<Regex> = OnceLock::new();
    let hex = HEX.get_or_init(|| Regex::new(r"#[0-9a-fA-F]{6}\b").unwrap());
    imports_theme || hex.find_iter(css).count() >= 100
}

// ---------------------------------------------------------------------------
// GTK
// ---------------------------------------------------------------------------

/// Named colors, as `(name, color)`.
type Names = Vec<(&'static str, String)>;

/// GTK's named colors, as `(name, color)`: libadwaita's set, then GTK 3's
/// older one. Colors are `#rrggbb` or `rgba(…)`.
fn gtk_names(s: &Scheme) -> (Names, Names) {
    let r = |n: &str| s.role(n).to_string();
    let shade = |a: f64| {
        let c = scheme::parse(s.role("shadow")).unwrap_or(Rgb::new(0, 0, 0));
        format!("rgba({}, {}, {}, {a})", c.red, c.green, c.blue)
    };
    let ok = s.base(0xB).to_string();
    let warn = s.base(0xA).to_string();
    let on_base = s.base(0x0).to_string();
    let adw = vec![
        ("accent_color", r("primary")),
        ("accent_bg_color", r("primary")),
        ("accent_fg_color", r("on_primary")),
        ("destructive_color", r("error")),
        ("destructive_bg_color", r("error")),
        ("destructive_fg_color", r("on_error")),
        ("success_color", ok.clone()),
        ("success_bg_color", ok),
        ("success_fg_color", on_base.clone()),
        ("warning_color", warn.clone()),
        ("warning_bg_color", warn),
        ("warning_fg_color", on_base),
        ("error_color", r("error")),
        ("error_bg_color", r("error")),
        ("error_fg_color", r("on_error")),
        ("window_bg_color", r("surface")),
        ("window_fg_color", r("on_surface")),
        ("view_bg_color", r("surface_container_lowest")),
        ("view_fg_color", r("on_surface")),
        ("headerbar_bg_color", r("surface_container")),
        ("headerbar_fg_color", r("on_surface")),
        ("headerbar_border_color", r("on_surface")),
        ("headerbar_backdrop_color", r("surface")),
        ("headerbar_shade_color", shade(0.36)),
        ("headerbar_darker_shade_color", shade(0.9)),
        ("sidebar_bg_color", r("surface_container_low")),
        ("sidebar_fg_color", r("on_surface")),
        ("sidebar_backdrop_color", r("surface")),
        ("sidebar_shade_color", shade(0.25)),
        ("sidebar_border_color", r("outline_variant")),
        ("secondary_sidebar_bg_color", r("surface")),
        ("secondary_sidebar_fg_color", r("on_surface")),
        ("secondary_sidebar_backdrop_color", r("surface")),
        ("secondary_sidebar_shade_color", shade(0.25)),
        ("secondary_sidebar_border_color", r("outline_variant")),
        ("card_bg_color", r("surface_container_low")),
        ("card_fg_color", r("on_surface")),
        ("card_shade_color", shade(0.36)),
        ("thumbnail_bg_color", r("surface_container_high")),
        ("thumbnail_fg_color", r("on_surface")),
        ("dialog_bg_color", r("surface_container_high")),
        ("dialog_fg_color", r("on_surface")),
        ("popover_bg_color", r("surface_container_high")),
        ("popover_fg_color", r("on_surface")),
        ("popover_shade_color", shade(0.25)),
        ("shade_color", shade(0.36)),
        ("scrollbar_outline_color", shade(0.5)),
    ];
    let gtk3 = vec![
        ("theme_bg_color", r("surface")),
        ("theme_fg_color", r("on_surface")),
        ("theme_base_color", r("surface_container_lowest")),
        ("theme_text_color", r("on_surface")),
        ("theme_selected_bg_color", r("primary")),
        ("theme_selected_fg_color", r("on_primary")),
        ("theme_unfocused_bg_color", r("surface")),
        ("theme_unfocused_fg_color", r("on_surface")),
        ("theme_unfocused_base_color", r("surface_container_lowest")),
        ("theme_unfocused_text_color", r("on_surface")),
        ("theme_unfocused_selected_bg_color", r("primary")),
        ("theme_unfocused_selected_fg_color", r("on_primary")),
        ("insensitive_bg_color", r("surface_container")),
        ("insensitive_fg_color", r("on_surface_variant")),
        ("insensitive_base_color", r("surface_container_low")),
        ("unfocused_insensitive_color", r("on_surface_variant")),
        ("borders", r("outline_variant")),
        ("unfocused_borders", r("outline_variant")),
        ("content_view_bg", r("surface_container_lowest")),
        ("text_view_bg", r("surface_container_lowest")),
    ];
    (adw, gtk3)
}

/// The GTK colors file: the named colors GTK's themes use, every scheme
/// name, and for GTK 4 libadwaita's CSS variables as well.
pub fn gtk_css(s: &Scheme, gtk4: bool, header: &str) -> String {
    let (adw, gtk3) = gtk_names(s);
    let mut out = format!("/*\n{header} */\n");
    out.push_str("\n/* GTK's own names, which themes draw with. */\n");
    for (k, v) in adw.iter().chain(&gtk3) {
        out.push_str(&format!("@define-color {k} {v};\n"));
    }
    out.push_str("\n/* The scheme's names, for your own rules. */\n");
    for (k, v) in s.named() {
        out.push_str(&format!("@define-color {k} {v};\n"));
    }
    if gtk4 {
        // libadwaita 1.6 and later draw with these.
        out.push_str("\n:root {\n");
        for (k, v) in &adw {
            out.push_str(&format!("  --{}: {v};\n", k.replace('_', "-")));
        }
        out.push_str("}\n");
    }
    out
}

/// Move GTK to adw-gtk3, light or dark: gsettings, and each `settings.ini`
/// that names a theme. Returns the theme it replaced.
pub fn use_adw_gtk3(config_home: &Path, desk: &Desktop, dark: bool, keep: usize) -> Result<String> {
    let name = desk.adw_gtk3(dark).ok_or_else(|| {
        Error::other("adw-gtk3 is not installed (on Arch it is the adw-gtk-theme package)")
    })?;
    set_gtk_theme(config_home, name, keep)?;
    Ok(desk.gtk_theme.clone())
}

/// Set the GTK theme everywhere GTK reads it from. `keep` backups of each
/// `settings.ini` changed are kept; none are taken for zero.
fn set_gtk_theme(config_home: &Path, name: &str, keep: usize) -> Result<()> {
    gsettings_set("gtk-theme", name);
    for v in ["gtk-3.0", "gtk-4.0"] {
        let path = config_home.join(v).join("settings.ini");
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        if ini_get(&text, "Settings", "gtk-theme-name").is_none_or(|t| t == name) {
            continue;
        }
        crate::paths::backup(&path, keep)?;
        crate::paths::write_atomic(
            &path,
            &ini_set(&text, "Settings", &[("gtk-theme-name", name)]),
        )?;
    }
    Ok(())
}

/// After a scheme is applied to GTK: when the theme is adw-gtk3, use its
/// variant that matches the scheme; and set the desktop's light or dark
/// preference, which libadwaita apps (and browsers, and Qt) follow.
pub fn gtk_follow(config_home: &Path, dark: bool) -> Option<String> {
    let desk = Desktop::read(config_home);
    if desk.gtk_theme.starts_with(ADW_GTK3) {
        if let Some(want) = desk.adw_gtk3(dark) {
            if want != desk.gtk_theme {
                if let Err(e) = set_gtk_theme(config_home, want, 0) {
                    return Some(format!("could not switch to {want}: {e}"));
                }
            }
        }
    }
    let current = gsettings_get("color-scheme")?;
    let want = if dark { "prefer-dark" } else { "default" };
    if (current == "prefer-dark") != dark {
        gsettings_set("color-scheme", want);
    }
    None
}

// ---------------------------------------------------------------------------
// Qt: qt5ct and qt6ct
// ---------------------------------------------------------------------------

/// A qt5ct/qt6ct color scheme. Qt 5 has 21 palette roles; Qt 6.6 added a
/// 22nd, the accent.
pub fn qt_palette(s: &Scheme, roles: usize) -> String {
    let r = |n: &str| s.role(n);
    let (light, midlight, dark) = if s.dark {
        (r("surface_bright"), r("surface_container_highest"), r("surface_container_lowest"))
    } else {
        (r("surface_container_lowest"), r("surface_container_low"), r("surface_dim"))
    };
    // In QPalette::ColorRole order.
    let active = [
        r("on_surface"),                // WindowText
        r("surface_container_high"),    // Button
        light,                          // Light
        midlight,                       // Midlight
        dark,                           // Dark
        r("outline_variant"),           // Mid
        r("on_surface"),                // Text
        "#ffffff",                      // BrightText
        r("on_surface"),                // ButtonText
        r("surface_container_lowest"),  // Base
        r("surface"),                   // Window
        r("shadow"),                    // Shadow
        r("primary"),                   // Highlight
        r("on_primary"),                // HighlightedText
        r("primary"),                   // Link
        r("tertiary"),                  // LinkVisited
        r("surface_container_low"),     // AlternateBase
        "#000000",                      // NoRole
        r("surface_container_highest"), // ToolTipBase
        r("on_surface"),                // ToolTipText
        r("on_surface_variant"),        // PlaceholderText
        r("primary"),                   // Accent
    ];
    let argb = |c: &str| format!("#ff{}", c.trim_start_matches('#'));
    let row = |dim: bool| {
        active
            .iter()
            .take(roles)
            .enumerate()
            .map(|(i, c)| {
                // Disabled text is drawn in the outline color.
                if dim && matches!(i, 0 | 6 | 8 | 13 | 20) {
                    argb(r("outline"))
                } else {
                    argb(c)
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "[ColorScheme]\nactive_colors={}\ndisabled_colors={}\ninactive_colors={}\n",
        row(false),
        row(true),
        row(false)
    )
}

// ---------------------------------------------------------------------------
// Kvantum
// ---------------------------------------------------------------------------

/// The Kvantum theme a recolored copy is made from.
#[derive(Debug, Clone, PartialEq)]
pub struct KvantumBase {
    /// Its name.
    pub name: String,
    /// Its `.kvconfig`.
    pub kvconfig: PathBuf,
    /// Its `.svg`.
    pub svg: PathBuf,
}

/// The theme to copy: the one Kvantum is set to — or, once it is set to
/// hyprcustard's, the one that copy was made from. Its dark variant when
/// Qt's style asks for one and there is one. `None` for Kvantum's built-in
/// theme, which has no files to copy.
pub fn kvantum_base(config_home: &Path, desk: &Desktop) -> Option<KvantumBase> {
    let user = config_home.join("Kvantum");
    let text = std::fs::read_to_string(user.join("kvantum.kvconfig")).unwrap_or_default();
    let mut name = ini_get(&text, "General", "theme").filter(|t| !t.is_empty())?;
    if name == KVANTUM_THEME {
        name =
            std::fs::read_to_string(user.join(KVANTUM_THEME).join("base")).ok()?.trim().to_string();
    }
    let dirs: Vec<PathBuf> =
        std::iter::once(user.clone()).chain(desk.kvantum_dirs.clone()).collect();
    let find = |n: &str, ext: &str| {
        dirs.iter().map(|d| d.join(n).join(format!("{n}.{ext}"))).find(|p| p.is_file())
    };
    let style = desk.qt_style.clone().or_else(|| {
        let ct = std::fs::read_to_string(config_home.join("qt6ct/qt6ct.conf")).ok()?;
        ini_get(&ct, "Appearance", "style")
    });
    if style.as_deref() == Some("kvantum-dark") && !name.ends_with("Dark") {
        let dark = format!("{name}Dark");
        if find(&dark, "kvconfig").is_some() {
            name = dark;
        }
    }
    let kvconfig = find(&name, "kvconfig")?;
    // A theme edited in Kvantum Manager is saved as `<name>#`, with only its
    // settings; the images are still the original's.
    let svg = find(&name, "svg").or_else(|| find(name.trim_end_matches('#'), "svg"))?;
    Some(KvantumBase { name, kvconfig, svg })
}

/// Whether Qt is drawing with Kvantum.
pub fn kvantum_in_use(config_home: &Path, desk: &Desktop) -> bool {
    let style = desk.qt_style.clone().or_else(|| {
        let ct = std::fs::read_to_string(config_home.join("qt6ct/qt6ct.conf")).ok()?;
        ini_get(&ct, "Appearance", "style")
    });
    style.is_some_and(|s| s.to_ascii_lowercase().starts_with("kvantum"))
}

/// How a base theme's colors are carried over to a scheme.
///
/// Lightness is mapped along the line through two anchors — the theme's
/// window color goes to the scheme's surface, its text color to the
/// scheme's text — so every shade keeps its place between them, and a dark
/// theme turns light under a light scheme. Grays take the surface's tint;
/// colors take the scheme's accent, or its error color for reds. Pure black
/// and white (shadows, glints) are left alone.
struct Recolor {
    from: (f64, f64),
    to: (f64, f64),
    neutral: Hct,
    accent: Hct,
    error: Hct,
}

impl Recolor {
    fn new(window: Rgb, text: Rgb, s: &Scheme) -> Recolor {
        let tone = |c: Rgb| Hct::new(c).get_tone();
        let hct = |n: &str| Hct::new(scheme::parse(s.role(n)).unwrap_or(Rgb::new(0, 0, 0)));
        Recolor {
            from: (tone(window), tone(text)),
            to: (hct("surface").get_tone(), hct("on_surface").get_tone()),
            neutral: hct("surface"),
            accent: hct("primary"),
            error: hct("error"),
        }
    }

    fn map(&self, c: Rgb) -> Rgb {
        let h = Hct::new(c);
        let t = h.get_tone();
        if !(2.0..=98.0).contains(&t) {
            return c;
        }
        let span = self.from.1 - self.from.0;
        let k = if span.abs() < 10.0 { 1.0 } else { (self.to.1 - self.to.0) / span };
        let tone = (self.to.0 + (t - self.from.0) * k).clamp(0.0, 100.0);
        let like = if h.get_chroma() < 16.0 {
            &self.neutral
        } else {
            let hue = h.get_hue();
            if !(50.0..345.0).contains(&hue) {
                &self.error
            } else {
                &self.accent
            }
        };
        let chroma =
            if h.get_chroma() < 16.0 { like.get_chroma().min(16.0) } else { like.get_chroma() };
        Rgb::from(Hct::from(like.get_hue(), chroma, tone))
    }

    /// Every `#rrggbb`, `#rrggbbaa` and (after a colon) `#rgb` in `text`,
    /// mapped; alpha is kept.
    fn text(&self, text: &str) -> String {
        static HEX: OnceLock<Regex> = OnceLock::new();
        let re = HEX.get_or_init(|| {
            Regex::new(r"(:\s*)?#([0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{3})\b").unwrap()
        });
        re.replace_all(text, |c: &regex::Captures| {
            let lead = c.get(1).map_or("", |m| m.as_str());
            let h = &c[2];
            if h.len() == 3 && lead.is_empty() {
                return c[0].to_string();
            }
            let (rgb, alpha) = if h.len() == 8 { (&h[..6], &h[6..]) } else { (h, "") };
            match scheme::parse(rgb) {
                Some(col) => format!("{lead}{}{alpha}", scheme::hex(self.map(col))),
                None => c[0].to_string(),
            }
        })
        .into_owned()
    }
}

/// Kvantum's palette keys given the scheme's colors exactly; every other
/// color in the theme is carried over by [`Recolor`].
fn kvantum_general(s: &Scheme, key: &str) -> Option<String> {
    let role = match key {
        "window.color" => "surface",
        "base.color" => "surface_container_lowest",
        "alt.base.color" => "surface_container_low",
        "button.color" => "surface_container_high",
        "text.color" | "window.text.color" | "button.text.color" | "tooltip.text.color" => {
            "on_surface"
        }
        "disabled.text.color" => "outline",
        "highlight.color" | "inactive.highlight.color" => "secondary_container",
        "highlight.text.color" => "on_secondary_container",
        "link.color" => "primary",
        "link.visited.color" => "tertiary",
        "progress.indicator.text.color" => "on_primary",
        _ => return None,
    };
    Some(s.role(role).to_string())
}

/// A base theme's `.kvconfig` and `.svg`, recolored for a scheme.
pub fn kvantum_theme(base_kvconfig: &str, base_svg: &str, s: &Scheme) -> (String, String) {
    let window = ini_get(base_kvconfig, "GeneralColors", "window.color")
        .and_then(|v| scheme::parse(v.trim_start_matches('#').get(..6)?));
    let text = ini_get(base_kvconfig, "GeneralColors", "text.color")
        .and_then(|v| scheme::parse(v.trim_start_matches('#').get(..6)?));
    let (window, text) = match (window, text) {
        (Some(w), Some(t)) => (w, t),
        _ => (Rgb::new(0x20, 0x20, 0x20), Rgb::new(0xe0, 0xe0, 0xe0)),
    };
    let rc = Recolor::new(window, text, s);
    let mut section = String::new();
    let mut out = String::with_capacity(base_kvconfig.len());
    for line in base_kvconfig.split_inclusive('\n') {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            section = t[1..t.len() - 1].to_string();
            out.push_str(line);
            continue;
        }
        let exact =
            (section == "GeneralColors").then(|| t.split_once('=')).flatten().and_then(|(k, v)| {
                let color = kvantum_general(s, k.trim())?;
                // Keep the theme's alpha, as in `#665c54cc`.
                let v = v.trim().trim_start_matches('#');
                let alpha = if v.len() == 8 { &v[6..] } else { "" };
                Some(format!("{}={color}{alpha}\n", k.trim()))
            });
        match exact {
            Some(l) => out.push_str(&l),
            None => out.push_str(&rc.text(line)),
        }
    }
    (out, rc.text(base_svg))
}

/// Write hyprcustard's Kvantum theme, recolored from the base theme, into
/// `dir`. Nothing is written when there is no base to copy.
pub fn write_kvantum(dir: &Path, base: &KvantumBase, s: &Scheme, header: &str) -> Result<()> {
    let kv = std::fs::read_to_string(&base.kvconfig).map_err(|e| Error::io(&base.kvconfig, e))?;
    let svg = std::fs::read_to_string(&base.svg).map_err(|e| Error::io(&base.svg, e))?;
    let (kv, svg) = kvantum_theme(&kv, &svg, s);
    let comment: String = header.lines().map(|l| format!(";{l}\n")).collect();
    crate::paths::write_atomic(&dir.join("base"), &format!("{}\n", base.name))?;
    crate::paths::write_atomic(&dir.join(format!("{KVANTUM_THEME}.svg")), &svg)?;
    crate::paths::write_atomic(
        &dir.join(format!("{KVANTUM_THEME}.kvconfig")),
        &format!("{comment}{kv}"),
    )
}

// ---------------------------------------------------------------------------
// INI files and gsettings
// ---------------------------------------------------------------------------

/// A key's value in an INI section.
pub fn ini_get(text: &str, section: &str, key: &str) -> Option<String> {
    let mut here = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            here = &t[1..t.len() - 1] == section;
        } else if here {
            if let Some((k, v)) = t.split_once('=') {
                if k.trim() == key {
                    return Some(v.trim().to_string());
                }
            }
        }
    }
    None
}

/// `text` with keys set in a section: replaced where they are, added at the
/// end of the section otherwise, and the section added when missing.
pub fn ini_set(text: &str, section: &str, pairs: &[(&str, &str)]) -> String {
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let header = |l: &str| {
        let t = l.trim();
        t.starts_with('[') && t.ends_with(']')
    };
    let start = lines.iter().position(|l| l.trim() == format!("[{section}]"));
    let Some(start) = start else {
        let mut out = text.to_string();
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!("[{section}]\n"));
        for (k, v) in pairs {
            out.push_str(&format!("{k}={v}\n"));
        }
        return out;
    };
    let mut end =
        lines[start + 1..].iter().position(|l| header(l)).map_or(lines.len(), |i| start + 1 + i);
    // Added keys go after the section's last setting, not after the blank
    // line that separates it from the next one.
    while end > start + 1 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    for (k, v) in pairs {
        let found = lines[start + 1..end]
            .iter()
            .position(|l| l.split_once('=').is_some_and(|(lk, _)| lk.trim() == *k));
        match found {
            Some(i) => lines[start + 1 + i] = format!("{k}={v}"),
            None => {
                lines.insert(end, format!("{k}={v}"));
                end += 1;
            }
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// A key of `org.gnome.desktop.interface`, unquoted. Never read in tests.
fn gsettings_get(key: &str) -> Option<String> {
    if cfg!(test) || crate::paths::which("gsettings").is_none() {
        return None;
    }
    let out = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", key])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().trim_matches('\'').to_string())
}

/// Set a key of `org.gnome.desktop.interface`. Never in tests.
fn gsettings_set(key: &str, value: &str) {
    if cfg!(test) || crate::paths::which("gsettings").is_none() {
        return;
    }
    let _ = Command::new("gsettings")
        .args(["set", "org.gnome.desktop.interface", key, value])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colors::named::builtins;

    fn gruvbox() -> Scheme {
        builtins()[0].scheme()
    }

    fn light() -> Scheme {
        scheme::from_seed("t", Rgb::new(0x67, 0x50, 0xa4), "tonal-spot", false, 0.0)
    }

    #[test]
    fn the_gtk_file_sets_the_names_themes_use() {
        let s = gruvbox();
        let css3 = gtk_css(&s, false, " * x\n");
        let css4 = gtk_css(&s, true, " * x\n");
        for name in [
            "window_bg_color",
            "accent_bg_color",
            "theme_bg_color",
            "theme_selected_bg_color",
            "primary",
            "base0D",
        ] {
            assert!(css3.contains(&format!("@define-color {name} ")), "{name}");
        }
        assert!(css3.contains(&format!("@define-color window_bg_color {};", s.role("surface"))));
        assert!(!css3.contains(":root"), "GTK 3 does not know CSS variables");
        assert!(css4.contains(&format!("--window-bg-color: {};", s.role("surface"))));
        assert!(css4.contains("--headerbar-shade-color: rgba("));
    }

    #[test]
    fn a_whole_theme_in_gtk_css_is_told_from_a_few_tweaks() {
        assert!(!whole_theme(
            "@import \"hyprcustard-colors.css\";\nlabel { color: #ff0000; }\n",
            "hyprcustard-colors.css"
        ));
        assert!(whole_theme(
            "@import url(\"/home/u/.themes/Nord/gtk-4.0/gtk.css\");\n",
            "hyprcustard-colors.css"
        ));
        let big: String = (0..150).map(|i| format!(".c{i} {{ color: #{i:06x}; }}\n")).collect();
        assert!(whole_theme(&big, "hyprcustard-colors.css"));
    }

    #[test]
    fn a_theme_drawn_with_names_follows_and_one_with_its_own_colors_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let mk = |name: &str, css: &str| {
            let d = dir.path().join(name);
            std::fs::create_dir_all(d.join("gtk-3.0")).unwrap();
            std::fs::write(d.join("index.theme"), "[Desktop Entry]\n").unwrap();
            std::fs::write(d.join("gtk-3.0/gtk.css"), css).unwrap();
        };
        mk("Named", &".a { background: @theme_bg_color; }\n".repeat(25));
        mk("Fixed", &".a { background: #282828; }\n".repeat(25));
        mk("adw-gtk3-dark", "");
        let mut desk = Desktop { theme_dirs: vec![dir.path().to_path_buf()], ..Default::default() };
        for (theme, follows) in
            [("Named", true), ("Fixed", false), ("Adwaita", false), ("adw-gtk3-dark", true)]
        {
            desk.gtk_theme = theme.into();
            assert_eq!(desk.gtk_theme_follows(), follows, "{theme}");
        }
        assert_eq!(desk.adw_gtk3(true), Some("adw-gtk3-dark"));
        assert_eq!(desk.adw_gtk3(false), None);
    }

    #[test]
    fn qt_palettes_have_one_color_per_role() {
        for (roles, s) in [(21, gruvbox()), (22, light())] {
            let p = qt_palette(&s, roles);
            for key in ["active_colors", "disabled_colors", "inactive_colors"] {
                let v = ini_get(&p, "ColorScheme", key).unwrap();
                let colors: Vec<&str> = v.split(", ").collect();
                assert_eq!(colors.len(), roles);
                assert!(colors.iter().all(|c| c.len() == 9 && c.starts_with("#ff")), "{v}");
            }
            let active = ini_get(&p, "ColorScheme", "active_colors").unwrap();
            // Window is the tenth role after nine others.
            assert_eq!(
                active.split(", ").nth(10).unwrap(),
                format!("#ff{}", &s.role("surface")[1..])
            );
        }
    }

    #[test]
    fn ini_keys_are_replaced_in_place_or_added_to_their_section() {
        let text = "[Appearance]\ncolor_scheme_path=/a.conf\nstyle=Fusion\n\n[Fonts]\nfixed=x\n";
        let out = ini_set(
            text,
            "Appearance",
            &[("color_scheme_path", "/b.conf"), ("custom_palette", "true")],
        );
        assert_eq!(
            out,
            "[Appearance]\ncolor_scheme_path=/b.conf\nstyle=Fusion\ncustom_palette=true\n\n[Fonts]\nfixed=x\n"
        );
        assert_eq!(ini_get(&out, "Fonts", "fixed").as_deref(), Some("x"));
        assert_eq!(ini_get(&out, "Fonts", "style"), None);
        assert_eq!(ini_set("", "General", &[("theme", "T")]), "[General]\ntheme=T\n");
        assert_eq!(
            ini_set("[Other]\na=1", "General", &[("theme", "T")]),
            "[Other]\na=1\n\n[General]\ntheme=T\n"
        );
    }

    fn kvantum_dirs() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("config");
        let theme = home.join("Kvantum/Mine");
        std::fs::create_dir_all(&theme).unwrap();
        std::fs::write(
            theme.join("Mine.kvconfig"),
            "[%General]\nauthor=me\n\n[GeneralColors]\nwindow.color=#232323\ntext.color=#ebdbb2\nhighlight.color=#665c54cc\nmid.color=#3f3f3f\n\n[PanelButtonCommand]\ntext.normal.color=#ebdbb2\n",
        )
        .unwrap();
        std::fs::write(
            theme.join("Mine.svg"),
            "<svg><rect style=\"fill:#232323;stroke:#cc241d\"/><rect fill=\"#458588\"/><use href=\"#bad\"/><rect style=\"fill:#000000\"/></svg>",
        )
        .unwrap();
        std::fs::write(home.join("Kvantum/kvantum.kvconfig"), "[General]\ntheme=Mine\n").unwrap();
        (dir, home)
    }

    #[test]
    fn the_kvantum_copy_takes_the_scheme_and_remembers_its_base() {
        let (_d, home) = kvantum_dirs();
        let desk = Desktop::default();
        let base = kvantum_base(&home, &desk).unwrap();
        assert_eq!(base.name, "Mine");
        let s = gruvbox();
        let out = home.join("Kvantum").join(KVANTUM_THEME);
        write_kvantum(&out, &base, &s, " Colors from hyprcustard.").unwrap();
        let kv = std::fs::read_to_string(out.join("HyprCustard.kvconfig")).unwrap();
        assert!(kv.starts_with("; Colors from hyprcustard."));
        assert_eq!(ini_get(&kv, "GeneralColors", "window.color").unwrap(), s.role("surface"));
        let hl = ini_get(&kv, "GeneralColors", "highlight.color").unwrap();
        assert_eq!(hl, format!("{}cc", s.role("secondary_container")), "the alpha is kept");
        assert_ne!(ini_get(&kv, "PanelButtonCommand", "text.normal.color").unwrap(), "#ebdbb2");
        let svg = std::fs::read_to_string(out.join("HyprCustard.svg")).unwrap();
        // The theme's window color lands on the scheme's surface, give or
        // take rounding through HCT.
        let fill = &svg[svg.find("fill:").unwrap() + 5..][..7];
        let (a, b) = (scheme::parse(fill).unwrap(), scheme::parse(s.role("surface")).unwrap());
        let near = |x: u8, y: u8| x.abs_diff(y) <= 2;
        assert!(near(a.red, b.red) && near(a.green, b.green) && near(a.blue, b.blue), "{svg}");
        assert!(svg.contains("href=\"#bad\""), "an id is not a color: {svg}");
        assert!(svg.contains("fill:#000000"), "black is left alone");

        // Once Kvantum is set to the copy, the copy is remade from the same base.
        std::fs::write(home.join("Kvantum/kvantum.kvconfig"), "[General]\ntheme=HyprCustard\n")
            .unwrap();
        assert_eq!(kvantum_base(&home, &desk).unwrap().name, "Mine");
    }

    #[test]
    fn a_light_scheme_turns_a_dark_kvantum_theme_light() {
        let (_d, home) = kvantum_dirs();
        let base = kvantum_base(&home, &Desktop::default()).unwrap();
        let kv = std::fs::read_to_string(&base.kvconfig).unwrap();
        let svg = std::fs::read_to_string(&base.svg).unwrap();
        let s = light();
        let (kv, _) = kvantum_theme(&kv, &svg, &s);
        let mid = scheme::parse(&ini_get(&kv, "GeneralColors", "mid.color").unwrap()).unwrap();
        assert!(scheme::luminance(mid) > 0.5, "a dark gray became a light one");
    }

    #[test]
    fn the_dark_variant_is_copied_when_qt_asks_for_dark() {
        let (_d, home) = kvantum_dirs();
        let dark = home.join("Kvantum/MineDark");
        std::fs::create_dir_all(&dark).unwrap();
        std::fs::write(dark.join("MineDark.kvconfig"), "[GeneralColors]\n").unwrap();
        std::fs::write(dark.join("MineDark.svg"), "<svg/>").unwrap();
        let desk = Desktop { qt_style: Some("kvantum-dark".into()), ..Default::default() };
        assert_eq!(kvantum_base(&home, &desk).unwrap().name, "MineDark");
        assert_eq!(kvantum_base(&home, &Desktop::default()).unwrap().name, "Mine");
    }

    #[test]
    fn kvantums_built_in_theme_has_nothing_to_copy() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(kvantum_base(dir.path(), &Desktop::default()), None);
    }
}
