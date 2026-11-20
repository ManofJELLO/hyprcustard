//! The GTK and Qt page: installed GTK 3 and Kvantum themes, set from the
//! app, with a preview of each drawn by the toolkit itself.
//!
//! - **GTK**: a theme is a folder in a themes folder with a
//!   `gtk-3.0/gtk.css`. Setting one writes gsettings and each GTK
//!   `settings.ini` that names a theme, as the Colors page's switch to
//!   adw-gtk3 does. GTK 4 apps built on libadwaita take no theme at all, so
//!   only GTK 3 is offered.
//! - **Kvantum**: a theme is a folder `<name>/<name>.kvconfig` in
//!   `~/.config/Kvantum` or a data folder's `Kvantum`. Setting one writes
//!   `kvantum.kvconfig`, and makes Kvantum the style in qt6ct and qt5ct.
//!   With the Colors page's scheme, the theme becomes the one hyprcustard's
//!   recolored copy is made from instead.
//! - **Previews** are real widgets, drawn offscreen by a child process of
//!   this program ([`GTK_PREVIEW_FLAG`], [`QT_PREVIEW_FLAG`]; the renderers
//!   are the GUI's, in its `render` module), so neither
//!   GTK nor a second Qt is ever loaded into the app, and a theme that
//!   crashes its toolkit takes only the child with it. Nothing is put on
//!   screen.

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;

use crate::colors::scheme::{self, Scheme};
use crate::colors::toolkits::{self, ini_get, ini_set, Desktop, KvantumBase, KVANTUM_THEME};
use crate::error::{Error, Result};
use crate::paths;

/// Runs this program as the GTK preview renderer: `<flag> <out.png>`, the
/// theme in `GTK_THEME`.
pub const GTK_PREVIEW_FLAG: &str = "--render-gtk-preview";
/// Runs this program as the Qt preview renderer: `<flag> <out.png> <style>
/// <font>`, Kvantum's theme in `$XDG_CONFIG_HOME/Kvantum`.
pub const QT_PREVIEW_FLAG: &str = "--render-qt-preview";

/// The size both previews are drawn at.
pub const PREVIEW_SIZE: (u32, u32) = (520, 300);

/// Previews kept in the cache; the oldest go first.
const KEEP_PREVIEWS: usize = 80;

// ---------------------------------------------------------------------------
// What is installed
// ---------------------------------------------------------------------------

/// Whether GTK 3 is installed: the only GTK that takes themes.
pub fn gtk_installed() -> bool {
    !cfg!(test) && toolkits::has_lib("libgtk-3.so.0")
}

/// Whether Kvantum is installed for Qt 6, which is what the preview and
/// most Qt apps today are drawn with.
pub fn kvantum_installed() -> bool {
    if cfg!(test) {
        return false;
    }
    let mut dirs: Vec<PathBuf> = std::env::var("QT_PLUGIN_PATH")
        .unwrap_or_default()
        .split(':')
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .collect();
    dirs.extend(
        [
            "/usr/lib/qt6/plugins",
            "/usr/lib64/qt6/plugins",
            "/usr/lib/x86_64-linux-gnu/qt6/plugins",
            "/usr/lib/aarch64-linux-gnu/qt6/plugins",
        ]
        .map(PathBuf::from),
    );
    dirs.iter().any(|d| d.join("styles/libkvantum.so").exists())
}

/// An installed GTK 3 theme.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GtkTheme {
    /// Its folder's name, which is what every setting takes.
    pub id: String,
    /// Its name as shown: the folder's, since that is what settings and the
    /// "Now" line say.
    pub name: String,
    /// What its `index.theme` calls it, when that is something else.
    pub calls_itself: Option<String>,
    /// Where it is, shortened with `~`; empty for one built into GTK.
    pub dir: String,
    /// Whether it has a dark version GTK picks when dark is preferred.
    pub dark_version: bool,
    /// Whether it also has GTK 4 files (which only matter to apps that do
    /// not use libadwaita, and only once copied into GTK 4's config).
    pub gtk4: bool,
    /// Whether it draws with GTK's named colors, so the Colors page's scheme
    /// reaches GTK 3 apps under it.
    pub follows: bool,
    /// Its background, text, view and selection colors, where it says.
    pub swatches: Vec<String>,
    /// Whether its background is dark, in the version GTK would show.
    pub dark: bool,
}

/// GTK 3's own theme, which is not a folder.
const BUILT_IN_GTK: &str = "Adwaita";

/// Every GTK 3 theme, the first folder winning for a name, sorted by name.
/// `prefer_dark`: which version's colors to show, when there are two.
pub fn gtk_themes(desk: &Desktop, prefer_dark: bool) -> Vec<GtkTheme> {
    let mut out: Vec<GtkTheme> = Vec::new();
    for dir in &desk.theme_dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let id = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            if out.iter().any(|t| t.id == id) || !path.join("gtk-3.0/gtk.css").is_file() {
                continue;
            }
            out.push(gtk_theme(&id, &path, prefer_dark));
        }
    }
    if !out.iter().any(|t| t.id == BUILT_IN_GTK) {
        out.push(GtkTheme {
            id: BUILT_IN_GTK.into(),
            name: BUILT_IN_GTK.into(),
            calls_itself: None,
            dir: String::new(),
            dark_version: true,
            gtk4: false,
            follows: false,
            swatches: if prefer_dark {
                ["#353535", "#eeeeec", "#2d2d2d", "#15539e"].map(String::from).to_vec()
            } else {
                ["#f6f5f4", "#2e3436", "#ffffff", "#3584e4"].map(String::from).to_vec()
            },
            dark: prefer_dark,
        });
    }
    out.sort_by_key(|t| t.name.to_lowercase());
    out
}

fn gtk_theme(id: &str, path: &Path, prefer_dark: bool) -> GtkTheme {
    let index = std::fs::read_to_string(path.join("index.theme")).unwrap_or_default();
    let calls_itself =
        ini_get(&index, "Desktop Entry", "Name").filter(|n| !n.is_empty() && n != id);
    let dark_version = path.join("gtk-3.0/gtk-dark.css").is_file();
    let file = if prefer_dark && dark_version { "gtk-dark.css" } else { "gtk.css" };
    let css = std::fs::read_to_string(path.join("gtk-3.0").join(file)).unwrap_or_default();
    let swatches = gtk_swatches(&css);
    let dark = swatches.first().is_some_and(|c| is_dark(c))
        || (swatches.is_empty() && id.to_lowercase().contains("dark"));
    GtkTheme {
        id: id.to_string(),
        name: id.to_string(),
        calls_itself,
        dir: paths::contract(path),
        dark_version,
        gtk4: path.join("gtk-4.0").is_dir(),
        follows: id.starts_with("adw-gtk3") || toolkits::draws_with_names(&css),
        swatches,
        dark,
    }
}

/// A GTK stylesheet's main colors, from its `@define-color` lines:
/// background, text, view background, selection. Names that refer to other
/// names are followed. A theme compiled into a resource file has none here.
pub fn gtk_swatches(css: &str) -> Vec<String> {
    let mut defs: Vec<(&str, &str)> = Vec::new();
    for line in css.lines() {
        let Some(rest) = line.trim_start().strip_prefix("@define-color") else { continue };
        let rest = rest.trim().trim_end_matches(';').trim();
        if let Some((name, value)) = rest.split_once(char::is_whitespace) {
            defs.push((name, value.trim()));
        }
    }
    let lookup = |start: &str| -> Option<String> {
        let mut name = start.to_string();
        for _ in 0..8 {
            let value = defs.iter().rev().find(|(n, _)| *n == name)?.1;
            match value.strip_prefix('@') {
                Some(next) => name = next.to_string(),
                None => return css_color(value),
            }
        }
        None
    };
    let pick = |names: &[&str]| names.iter().find_map(|n| lookup(n));
    [
        pick(&["theme_bg_color", "window_bg_color", "bg_color"]),
        pick(&["theme_fg_color", "window_fg_color", "fg_color"]),
        pick(&["theme_base_color", "view_bg_color", "base_color"]),
        pick(&["theme_selected_bg_color", "accent_bg_color", "selected_bg_color"]),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// A CSS color as `#rrggbb`: hex, or `rgb()`/`rgba()` with plain numbers.
fn css_color(value: &str) -> Option<String> {
    let v = value.trim();
    if let Some(hex) = v.strip_prefix('#') {
        let hex = match hex.len() {
            3 | 4 if hex.is_ascii() => hex.chars().take(3).flat_map(|c| [c, c]).collect(),
            6 | 8 => hex.get(..6)?.to_string(),
            _ => return None,
        };
        return scheme::parse(&hex).map(|_| format!("#{}", hex.to_lowercase()));
    }
    let inner = v.strip_prefix("rgba(").or_else(|| v.strip_prefix("rgb("))?.strip_suffix(')')?;
    let parts: Vec<u8> = inner
        .split(',')
        .take(3)
        .map(|p| p.trim().parse::<f64>().ok().map(|n| n as u8))
        .collect::<Option<_>>()?;
    (parts.len() == 3).then(|| format!("#{:02x}{:02x}{:02x}", parts[0], parts[1], parts[2]))
}

fn is_dark(hex: &str) -> bool {
    scheme::parse(hex.trim_start_matches('#')).is_some_and(|c| {
        let l =
            0.2126 * f64::from(c.red) + 0.7152 * f64::from(c.green) + 0.0722 * f64::from(c.blue);
        l < 128.0
    })
}

/// Whether GTK 3 is told to prefer its themes' dark versions.
pub fn gtk_prefers_dark(config_home: &Path) -> bool {
    let text =
        std::fs::read_to_string(config_home.join("gtk-3.0/settings.ini")).unwrap_or_default();
    ini_get(&text, "Settings", "gtk-application-prefer-dark-theme")
        .is_some_and(|v| matches!(v.trim(), "1" | "true"))
}

/// An installed Kvantum theme.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KvantumTheme {
    /// Its folder's name, which is what `kvantum.kvconfig` takes.
    pub id: String,
    /// Where it is, shortened with `~`.
    pub dir: String,
    /// Whether it is in the user's own Kvantum folder.
    pub mine: bool,
    /// Its window, view, text and selection colors.
    pub swatches: Vec<String>,
    /// Whether its windows are dark.
    pub dark: bool,
}

/// Every Kvantum theme but hyprcustard's own copy, the user's folder winning
/// for a name, sorted by name.
pub fn kvantum_themes(config_home: &Path, desk: &Desktop) -> Vec<KvantumTheme> {
    let user = config_home.join("Kvantum");
    let mut out: Vec<KvantumTheme> = Vec::new();
    for dir in std::iter::once(&user).chain(desk.kvantum_dirs.iter()) {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let id = entry.file_name().to_string_lossy().into_owned();
            if id == KVANTUM_THEME || out.iter().any(|t| t.id == id) {
                continue;
            }
            let Some(base) = toolkits::kvantum_named(config_home, desk, &id) else { continue };
            let text = std::fs::read_to_string(&base.kvconfig).unwrap_or_default();
            let swatches = kvantum_swatches(&text);
            out.push(KvantumTheme {
                dark: swatches.first().is_some_and(|c| is_dark(c)),
                id,
                dir: paths::contract(&entry.path()),
                mine: dir == &user,
                swatches,
            });
        }
    }
    out.sort_by_key(|t| t.id.to_lowercase());
    out
}

/// A `.kvconfig`'s window, view, text and selection colors.
pub fn kvantum_swatches(kvconfig: &str) -> Vec<String> {
    ["window.color", "text.color", "base.color", "highlight.color"]
        .iter()
        .filter_map(|k| ini_get(kvconfig, "GeneralColors", k))
        .filter_map(|v| css_color(&v))
        .collect()
}

// ---------------------------------------------------------------------------
// Now in use, and setting
// ---------------------------------------------------------------------------

/// What Qt is set to draw with.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct QtNow {
    /// `kvantum.kvconfig`'s theme; empty for Kvantum's built-in one.
    pub theme: String,
    /// When that is hyprcustard's recolored copy, the theme it is made from.
    pub base: String,
    /// qt6ct's style, and qt5ct's, where each is set up.
    pub styles: Vec<(String, String)>,
    /// `QT_QPA_PLATFORMTHEME`.
    pub platform: Option<String>,
    /// `QT_STYLE_OVERRIDE`.
    pub style_override: Option<String>,
}

/// Read what Qt is set to draw with.
pub fn qt_now(config_home: &Path, desk: &Desktop) -> QtNow {
    let kv =
        std::fs::read_to_string(config_home.join("Kvantum/kvantum.kvconfig")).unwrap_or_default();
    let theme = ini_get(&kv, "General", "theme").unwrap_or_default();
    let base = if theme == KVANTUM_THEME {
        std::fs::read_to_string(config_home.join("Kvantum").join(KVANTUM_THEME).join("base"))
            .map(|b| b.trim().to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let styles = ["qt6ct", "qt5ct"]
        .iter()
        .filter_map(|ct| {
            let text =
                std::fs::read_to_string(config_home.join(ct).join(format!("{ct}.conf"))).ok()?;
            Some((ct.to_string(), ini_get(&text, "Appearance", "style").unwrap_or_default()))
        })
        .collect();
    QtNow {
        theme,
        base,
        styles,
        platform: desk.qt_platform.clone(),
        style_override: desk.qt_style.clone(),
    }
}

impl QtNow {
    /// Whether Qt apps draw with Kvantum now.
    pub fn uses_kvantum(&self) -> bool {
        let is_kv = |s: &str| s.to_ascii_lowercase().starts_with("kvantum");
        if let Some(o) = &self.style_override {
            return is_kv(o);
        }
        matches!(self.platform.as_deref(), Some("qt5ct" | "qt6ct"))
            && self.styles.iter().any(|(ct, s)| ct == "qt6ct" && is_kv(s))
    }
}

/// Set GTK 3's theme everywhere it is read from.
pub fn use_gtk(config_home: &Path, id: &str, keep: usize) -> Result<()> {
    toolkits::set_gtk_theme(config_home, id, keep)
}

/// Point Kvantum at `theme`, and make Kvantum the style in qt6ct and qt5ct
/// where either is set up and has another. `kvantum-dark` becomes plain
/// `kvantum`: with it Kvantum swaps a theme for its `…Dark` sibling, which
/// is not the theme chosen. Returns the qt5ct/qt6ct configs changed.
pub fn use_kvantum(config_home: &Path, theme: &str, keep: usize) -> Result<Vec<String>> {
    let path = config_home.join("Kvantum/kvantum.kvconfig");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    if ini_get(&text, "General", "theme").as_deref() != Some(theme) {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        }
        paths::backup(&path, keep)?;
        paths::write_atomic(&path, &ini_set(&text, "General", &[("theme", theme)]))?;
    }
    let mut changed = Vec::new();
    for ct in ["qt6ct", "qt5ct"] {
        let conf = config_home.join(ct).join(format!("{ct}.conf"));
        let Ok(text) = std::fs::read_to_string(&conf) else { continue };
        if ini_get(&text, "Appearance", "style").as_deref() == Some("kvantum") {
            continue;
        }
        paths::backup(&conf, keep)?;
        paths::write_atomic(&conf, &ini_set(&text, "Appearance", &[("style", "kvantum")]))?;
        changed.push(paths::contract(&conf));
    }
    Ok(changed)
}

/// Make hyprcustard's recolored copy from `base`, and point Kvantum at it.
pub fn use_kvantum_recolored(
    config_home: &Path,
    base: &KvantumBase,
    s: &Scheme,
    keep: usize,
) -> Result<Vec<String>> {
    let dir = config_home.join("Kvantum").join(KVANTUM_THEME);
    toolkits::write_kvantum(&dir, base, s, &kvantum_header(s, base))?;
    use_kvantum(config_home, KVANTUM_THEME, keep)
}

fn kvantum_header(s: &Scheme, base: &KvantumBase) -> String {
    format!(
        " Colors from hyprcustard: {}, over the {} Kvantum theme.\n \
         Rewritten every time a color scheme is applied.",
        s.name, base.name
    )
}

// ---------------------------------------------------------------------------
// Previews
// ---------------------------------------------------------------------------

/// Where previews are kept.
pub fn preview_dir() -> PathBuf {
    if cfg!(test) {
        return paths::app_config_dir().join("toolkit-previews");
    }
    dirs::cache_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".cache"))
        .join("hyprcustard")
        .join("toolkit-previews")
}

/// What a preview is drawn from.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// A GTK 3 theme.
    Gtk {
        /// Its name.
        theme: String,
        /// Its dark version.
        dark: bool,
    },
    /// A Kvantum theme.
    Qt {
        /// Its files.
        base: KvantumBase,
        /// The scheme to recolor it with, or none for it as it is.
        recolor: Option<Scheme>,
        /// Qt's font, as qt6ct sets it; empty for Qt's own.
        font: String,
    },
}

/// One preview to draw. Made on the app's thread; [`PreviewJob::run`] is
/// slow and belongs on another.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewJob {
    /// The PNG, named for everything that changes how it looks, so one
    /// already there is up to date.
    pub out: PathBuf,
    /// What it is drawn from.
    pub source: Source,
}

impl PreviewJob {
    /// A GTK 3 theme's preview. `theme_dir` is `None` for Adwaita.
    pub fn gtk(
        config_home: &Path,
        theme: &str,
        theme_dir: Option<&Path>,
        dark: bool,
    ) -> PreviewJob {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        ("gtk", theme, dark).hash(&mut h);
        let mut files = vec![
            config_home.join("gtk-3.0/gtk.css"),
            config_home.join("gtk-3.0/settings.ini"),
            config_home.join("gtk-3.0/hyprcustard-colors.css"),
        ];
        if let Some(d) = theme_dir {
            files.push(d.join("gtk-3.0/gtk.css"));
            files.push(d.join("gtk-3.0/gtk-dark.css"));
            files.push(d.join("gtk-3.0/gtk.gresource"));
        }
        stamp(&files, &mut h);
        PreviewJob {
            out: preview_dir().join(format!("gtk-{:016x}.png", h.finish())),
            source: Source::Gtk { theme: theme.to_string(), dark },
        }
    }

    /// A Kvantum theme's preview, recolored with `recolor` when given.
    pub fn qt(config_home: &Path, base: KvantumBase, recolor: Option<Scheme>) -> PreviewJob {
        let ct = std::fs::read_to_string(config_home.join("qt6ct/qt6ct.conf")).unwrap_or_default();
        let font = ini_get(&ct, "Fonts", "general")
            .map(|f| f.trim_matches('"').to_string())
            .filter(|f| !f.starts_with('@'))
            .unwrap_or_default();
        let mut h = std::collections::hash_map::DefaultHasher::new();
        ("qt", &base.name, &font).hash(&mut h);
        if let Some(s) = &recolor {
            serde_json::to_string(s).unwrap_or_default().hash(&mut h);
        }
        stamp(&[base.kvconfig.clone(), base.svg.clone()], &mut h);
        PreviewJob {
            out: preview_dir().join(format!("qt-{:016x}.png", h.finish())),
            source: Source::Qt { base, recolor, font },
        }
    }

    /// Whether it is drawn already.
    pub fn ready(&self) -> bool {
        self.out.is_file()
    }

    /// Draw it with `program`, this app's own executable, unless it is
    /// drawn already. Gives up after twenty seconds.
    pub fn run(&self, program: &Path) -> Result<PathBuf> {
        if self.ready() {
            return Ok(self.out.clone());
        }
        let dir = preview_dir();
        std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        // Files of this run's own: two runs for the same preview (a quick
        // second click) must not take each other's half-drawn picture.
        let run = format!("{}-{}", std::process::id(), RUNS.fetch_add(1, Ordering::Relaxed));
        let stem = self.out.file_stem().and_then(|s| s.to_str()).unwrap_or("preview");
        let part = dir.join(format!("{stem}.{run}.part.png"));
        let log = dir.join(format!("{stem}.{run}.log"));
        // The renderer's complaints go to a file, not a pipe: a theme with
        // thousands of CSS warnings would fill a pipe nobody reads until the
        // end, and the renderer would stall until it was killed.
        let log_file = std::fs::File::create(&log).map_err(|e| Error::io(&log, e))?;
        let mut cmd = Command::new(program);
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(log_file);
        let mut scratch = None;
        match &self.source {
            Source::Gtk { theme, dark } => {
                let value = if *dark { format!("{theme}:dark") } else { theme.clone() };
                cmd.arg(GTK_PREVIEW_FLAG).arg(&part).env("GTK_THEME", value);
                cmd.env("NO_AT_BRIDGE", "1").env("GTK_A11Y", "none");
                // A connection to the session's display, which GTK needs to
                // start; the window is offscreen and never shown on it.
                if std::env::var_os("WAYLAND_DISPLAY").is_some() {
                    cmd.env("GDK_BACKEND", "wayland");
                }
            }
            Source::Qt { base, recolor, font } => {
                let home = qt_scratch(&dir, base, recolor.as_ref())?;
                cmd.arg(QT_PREVIEW_FLAG).arg(&part).arg("kvantum").arg(font);
                cmd.env("XDG_CONFIG_HOME", &home).env("QT_QPA_PLATFORM", "offscreen");
                cmd.env_remove("QT_QPA_PLATFORMTHEME").env_remove("QT_STYLE_OVERRIDE");
                scratch = Some(home);
            }
        }
        let result = wait(cmd, Duration::from_secs(20), &log);
        if let Some(home) = scratch {
            let _ = std::fs::remove_dir_all(home);
        }
        let _ = std::fs::remove_file(&log);
        if let Err(e) = result {
            let _ = std::fs::remove_file(&part);
            return Err(e);
        }
        if !part.is_file() {
            return Err(Error::other("the preview was not drawn"));
        }
        std::fs::rename(&part, &self.out).map_err(|e| Error::io(&self.out, e))?;
        prune(&dir);
        Ok(self.out.clone())
    }
}

/// Each file's size and time, so an edited theme gets a new preview.
fn stamp(files: &[PathBuf], h: &mut impl Hasher) {
    for f in files {
        let meta = std::fs::metadata(f).ok();
        let modified =
            meta.as_ref().and_then(|m| m.modified().ok()).unwrap_or(SystemTime::UNIX_EPOCH);
        (f, meta.map(|m| m.len()), modified).hash(h);
    }
}

/// A config folder for the Qt preview: Kvantum set to the theme, the
/// user's own Kvantum themes linked in so Kvantum finds them, and the
/// recolored copy written into it when there is one.
fn qt_scratch(dir: &Path, base: &KvantumBase, recolor: Option<&Scheme>) -> Result<PathBuf> {
    let n = RUNS.fetch_add(1, Ordering::Relaxed);
    let home = dir.join(format!("qt-config-{}-{n}", std::process::id()));
    let kv = home.join("Kvantum");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&kv).map_err(|e| Error::io(&kv, e))?;
    let theme = match recolor {
        Some(s) => {
            toolkits::write_kvantum(&kv.join(KVANTUM_THEME), base, s, &kvantum_header(s, base))?;
            KVANTUM_THEME.to_string()
        }
        None => {
            // The theme's own folder, wherever it is, and every folder of the
            // user's (an edited `Name#` theme takes its images from `Name`).
            let mut folders: Vec<PathBuf> =
                base.kvconfig.parent().into_iter().map(Path::to_path_buf).collect();
            if let Ok(user) = paths::config_home().join("Kvantum").read_dir() {
                folders.extend(user.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
            }
            for f in folders {
                let Some(name) = f.file_name() else { continue };
                let link = kv.join(name);
                if !link.exists() {
                    std::os::unix::fs::symlink(&f, &link).map_err(|e| Error::io(&link, e))?;
                }
            }
            base.name.clone()
        }
    };
    paths::write_atomic(&kv.join("kvantum.kvconfig"), &format!("[General]\ntheme={theme}\n"))?;
    Ok(home)
}

/// Runs started by this process, to name each one's files.
static RUNS: AtomicU32 = AtomicU32::new(0);

/// Run a renderer whose stderr goes to `log`, killing it if it takes longer
/// than `limit`.
fn wait(mut cmd: Command, limit: Duration, log: &Path) -> Result<()> {
    let mut child =
        cmd.spawn().map_err(|e| Error::other(format!("could not start the preview: {e}")))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    return Ok(());
                }
                let err =
                    String::from_utf8_lossy(&std::fs::read(log).unwrap_or_default()).into_owned();
                let last = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
                return Err(Error::other(if last.is_empty() {
                    format!("the preview stopped ({status})")
                } else {
                    format!("the preview stopped: {last}")
                }));
            }
            Ok(None) if start.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::other("the preview took too long and was stopped"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(30)),
            Err(e) => return Err(Error::other(format!("the preview: {e}"))),
        }
    }
}

/// Keep the newest previews only.
fn prune(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut pngs: Vec<(SystemTime, PathBuf)> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "png"))
        // Another run's picture, still being drawn.
        .filter(|p| !p.to_string_lossy().ends_with(".part.png"))
        .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
        .collect();
    if pngs.len() <= KEEP_PREVIEWS {
        return;
    }
    pngs.sort();
    for (_, p) in &pngs[..pngs.len() - KEEP_PREVIEWS] {
        let _ = std::fs::remove_file(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A renderer standing in for this program: `script` gets the same
    /// arguments, the picture's path second.
    fn fake_renderer(dir: &Path, script: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("render.sh");
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn two_runs_for_the_same_preview_both_end_with_it() {
        let dir = scratch();
        let program = fake_renderer(&dir, "sleep 0.3; printf png > \"$2\"");
        let job = PreviewJob::gtk(&dir, "Same", None, false);
        let _ = std::fs::remove_file(&job.out);
        let (a, b) = (job.clone(), job.clone());
        let (pa, pb) = (program.clone(), program);
        let first = std::thread::spawn(move || a.run(&pa));
        std::thread::sleep(Duration::from_millis(50));
        let second = std::thread::spawn(move || b.run(&pb));
        assert!(first.join().unwrap().is_ok());
        assert!(second.join().unwrap().is_ok(), "the second run must not lose its picture");
        assert!(job.ready());
        let _ = std::fs::remove_file(&job.out);
    }

    #[test]
    fn a_renderer_that_says_a_lot_is_not_stalled_and_its_last_word_is_kept() {
        let dir = scratch();
        // Far more than a pipe holds, then success.
        let program =
            fake_renderer(&dir, "head -c 400000 /dev/zero | tr '\\0' 'w' >&2; printf png > \"$2\"");
        let job = PreviewJob::gtk(&dir, "Loud", None, false);
        let _ = std::fs::remove_file(&job.out);
        let start = Instant::now();
        job.run(&program).unwrap();
        assert!(start.elapsed() < Duration::from_secs(5), "{:?}", start.elapsed());
        let _ = std::fs::remove_file(&job.out);

        let program = fake_renderer(
            &dir,
            "echo noise >&2; echo 'GTK could not open the display' >&2; exit 1",
        );
        let job = PreviewJob::gtk(&dir, "Broken", None, false);
        let e = job.run(&program).unwrap_err().to_string();
        assert!(e.ends_with("GTK could not open the display"), "{e}");
        let left: Vec<_> = std::fs::read_dir(preview_dir())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            // This run's, not those of a test running beside this one.
            .filter(|n| n.starts_with(job.out.file_stem().unwrap().to_str().unwrap()))
            .filter(|n| n.contains(".part.") || n.ends_with(".log"))
            .collect();
        assert!(left.is_empty(), "nothing of a run is left behind: {left:?}");
    }

    #[test]
    fn swatches_follow_names_and_read_each_way_of_writing_a_color() {
        let css = "@define-color window_bg_color #242424;\n\
                   @define-color theme_bg_color @window_bg_color;\n\
                   @define-color theme_fg_color rgba(255, 255, 255, 0.9);\n\
                   @define-color theme_base_color #FFF;\n\
                   @define-color theme_selected_bg_color @accent;\n\
                   @define-color accent #3584e4cc;\n";
        assert_eq!(gtk_swatches(css), vec!["#242424", "#ffffff", "#ffffff", "#3584e4"]);
        // Names that go round in a circle, or nowhere, give nothing.
        let css = "@define-color theme_bg_color @a;\n@define-color a @theme_bg_color;\n\
                   @define-color theme_fg_color @nowhere;\n";
        assert!(gtk_swatches(css).is_empty());
        assert!(gtk_swatches("@import url(\"resource:///x/gtk.css\");").is_empty());
        // Not ASCII where the digits should be: no color, and no panic.
        assert_eq!(css_color("#abcde\u{e9}x"), None);
        assert_eq!(css_color("#\u{e9}a"), None);
    }

    #[test]
    fn kvantum_swatches_come_from_its_general_colors() {
        let kv = "[%General]\nauthor=x\n[GeneralColors]\nwindow.color=#2e3440\n\
                  base.color=#3b4252\ntext.color=#eceff4\nhighlight.color=#88c0d0ff\n";
        assert_eq!(kvantum_swatches(kv), vec!["#2e3440", "#eceff4", "#3b4252", "#88c0d0"]);
        assert!(is_dark("#2e3440") && !is_dark("#eceff4"));
    }

    fn scratch() -> PathBuf {
        let dir =
            paths::app_config_dir().join(format!("toolkits-{:?}", std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn themes_are_found_once_each_and_only_with_gtk_3_files() {
        let root = scratch();
        let (mine, system) = (root.join("mine"), root.join("system"));
        for (dir, id, css) in [
            (&mine, "Nice-Dark", "@define-color theme_bg_color #202020;"),
            (&system, "Nice-Dark", "@define-color theme_bg_color #ffffff;"),
            (&system, "Plain", "@define-color theme_bg_color #fafafa;"),
        ] {
            std::fs::create_dir_all(dir.join(id).join("gtk-3.0")).unwrap();
            std::fs::write(dir.join(id).join("gtk-3.0/gtk.css"), css).unwrap();
        }
        std::fs::create_dir_all(system.join("Openbox-only/openbox-3")).unwrap();
        std::fs::write(system.join("Plain/index.theme"), "[Desktop Entry]\nName=Kripton\n")
            .unwrap();
        std::fs::write(
            mine.join("Nice-Dark/gtk-3.0/gtk-dark.css"),
            "@define-color theme_bg_color #101010;",
        )
        .unwrap();
        let desk = Desktop { theme_dirs: vec![mine.clone(), system], ..Default::default() };
        let themes = gtk_themes(&desk, false);
        let ids: Vec<&str> = themes.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["Adwaita", "Nice-Dark", "Plain"]);
        let nice = &themes[1];
        assert_eq!(nice.swatches[0], "#202020", "the first folder wins");
        assert!(nice.dark_version && nice.dark);
        assert_eq!(gtk_themes(&desk, true)[1].swatches[0], "#101010", "its dark version's colors");
        assert!(!themes[2].dark);
        assert_eq!(
            (themes[2].name.as_str(), themes[2].calls_itself.as_deref()),
            ("Plain", Some("Kripton"))
        );
    }

    #[test]
    fn using_a_kvantum_theme_sets_it_and_kvantum_as_the_style() {
        let home = scratch();
        std::fs::create_dir_all(home.join("qt6ct")).unwrap();
        std::fs::write(
            home.join("qt6ct/qt6ct.conf"),
            "[Appearance]\nstyle=kvantum-dark\nicon_theme=X\n",
        )
        .unwrap();
        std::fs::create_dir_all(home.join("qt5ct")).unwrap();
        std::fs::write(home.join("qt5ct/qt5ct.conf"), "[Appearance]\nstyle=kvantum\n").unwrap();
        let changed = use_kvantum(&home, "KvArcDark", 0).unwrap();
        assert_eq!(changed.len(), 1, "qt5ct already had it: {changed:?}");
        let kv = std::fs::read_to_string(home.join("Kvantum/kvantum.kvconfig")).unwrap();
        assert_eq!(ini_get(&kv, "General", "theme").as_deref(), Some("KvArcDark"));
        let ct = std::fs::read_to_string(home.join("qt6ct/qt6ct.conf")).unwrap();
        assert_eq!(ini_get(&ct, "Appearance", "style").as_deref(), Some("kvantum"));
        assert_eq!(ini_get(&ct, "Appearance", "icon_theme").as_deref(), Some("X"), "the rest kept");
        let desk = Desktop { qt_platform: Some("qt6ct".into()), ..Default::default() };
        let now = qt_now(&home, &desk);
        assert_eq!(now.theme, "KvArcDark");
        assert!(now.uses_kvantum());
        assert!(!QtNow { style_override: Some("Fusion".into()), ..now.clone() }.uses_kvantum());
    }

    #[test]
    fn the_recolored_copy_remembers_its_base_and_previews_are_named_for_what_they_show() {
        let home = scratch();
        let theme = home.join("Kvantum/Mine");
        std::fs::create_dir_all(&theme).unwrap();
        std::fs::write(
            theme.join("Mine.kvconfig"),
            "[GeneralColors]\nwindow.color=#202020\ntext.color=#e0e0e0\n",
        )
        .unwrap();
        std::fs::write(theme.join("Mine.svg"), "<svg><rect fill=\"#202020\"/></svg>").unwrap();
        let desk = Desktop::default();
        let base = toolkits::kvantum_named(&home, &desk, "Mine").unwrap();
        let nord =
            crate::colors::named::builtins().iter().find(|n| n.id == "nord").unwrap().scheme();
        use_kvantum_recolored(&home, &base, &nord, 0).unwrap();
        let now = qt_now(&home, &desk);
        assert_eq!((now.theme.as_str(), now.base.as_str()), (KVANTUM_THEME, "Mine"));
        // hyprcustard's own copy is not offered as a theme of its own.
        let ids: Vec<String> = kvantum_themes(&home, &desk).into_iter().map(|t| t.id).collect();
        assert_eq!(ids, ["Mine"]);

        let plain = PreviewJob::qt(&home, base.clone(), None);
        let recolored = PreviewJob::qt(&home, base.clone(), Some(nord));
        assert_ne!(plain.out, recolored.out);
        assert_eq!(
            plain.out,
            PreviewJob::qt(&home, base, None).out,
            "the same look, the same file"
        );
        let light = PreviewJob::gtk(&home, "Plain", None, false);
        assert_ne!(light.out, PreviewJob::gtk(&home, "Plain", None, true).out);
    }
}
