//! Icon and cursor themes, for every toolkit at once.
//!
//! Each toolkit reads its icon and cursor theme from somewhere different,
//! and a theme picked in one place and not another shows as GTK apps with
//! one set of icons and Qt apps with another, or a different cursor over
//! every other window. So a theme is written everywhere it is read from:
//!
//! - **Icons**: gsettings (`icon-theme`, which GTK on Wayland and the
//!   settings portal read), `gtk-icon-theme-name` in GTK 3's and 4's
//!   `settings.ini`, and `icon_theme` in qt5ct's and qt6ct's config, each
//!   only where that file already exists.
//! - **Cursors**: gsettings (`cursor-theme`, `cursor-size`), the GTK
//!   `settings.ini` keys, `~/.icons/default/index.theme` (what X apps fall
//!   back to), and Hyprland: `hyprctl setcursor` now, and a managed block
//!   setting `XCURSOR_*`/`HYPRCURSOR_*` and running `setcursor` at login.
//!   Qt on Wayland draws the cursor the compositor asks for, so it has no
//!   setting of its own. Lines in the user's config that set the cursor are
//!   offered for handing over, as the wallpaper's start lines are.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::block::{self, Markers};
use crate::colors::toolkits::{gsettings_get, gsettings_set, ini_get, ini_set};
use crate::error::{Error, Result};
use crate::model::lua_quote;
use crate::paths;
use crate::walls::startup::{lines_outside_blocks, movable, split_keep};

// ---------------------------------------------------------------------------
// Where things are
// ---------------------------------------------------------------------------

/// The folders this module reads and writes.
#[derive(Debug, Clone, PartialEq)]
pub struct Places {
    /// `$XDG_CONFIG_HOME`: GTK's and qt5ct/qt6ct's configs.
    pub config_home: PathBuf,
    /// Where themes are installed, in the order the icon theme spec looks.
    pub theme_dirs: Vec<PathBuf>,
    /// `~/.icons`, whose `default` theme names the cursor X apps fall back to.
    pub home_icons: PathBuf,
    /// Where cursor previews are written.
    pub cache: PathBuf,
}

impl Places {
    /// This user's folders. A unit test gets throwaway ones: it must never
    /// write to the real `~/.icons`.
    pub fn session() -> Places {
        if cfg!(test) {
            let root = paths::app_config_dir().join("icons-test");
            return Places {
                config_home: paths::apps_config_home(),
                theme_dirs: vec![root.join("themes")],
                home_icons: root.join("home-icons"),
                cache: root.join("cache"),
            };
        }
        let home = dirs::home_dir().unwrap_or_default();
        let data_home = std::env::var_os("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"));
        let mut theme_dirs = vec![home.join(".icons"), data_home.join("icons")];
        theme_dirs.extend(
            std::env::var("XDG_DATA_DIRS")
                .ok()
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "/usr/local/share:/usr/share".into())
                .split(':')
                .filter(|d| !d.is_empty())
                .map(|d| PathBuf::from(d).join("icons")),
        );
        theme_dirs.push(PathBuf::from("/usr/share/pixmaps"));
        Places {
            config_home: paths::apps_config_home(),
            theme_dirs,
            home_icons: home.join(".icons"),
            cache: dirs::cache_dir()
                .unwrap_or_else(|| home.join(".cache"))
                .join("hyprcustard")
                .join("cursors"),
        }
    }
}

// ---------------------------------------------------------------------------
// What is installed
// ---------------------------------------------------------------------------

/// An installed icon theme.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IconTheme {
    /// Its folder's name, which is what every setting takes.
    pub id: String,
    /// What it calls itself.
    pub name: String,
    /// Where it is, shortened with `~`.
    pub dir: String,
    /// A few of its icons, as files, for the page to show.
    pub previews: Vec<String>,
}

/// An installed cursor theme.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CursorTheme {
    /// Its folder's name.
    pub id: String,
    /// What it calls itself.
    pub name: String,
    /// Where it is, shortened with `~`.
    pub dir: String,
    /// Whether it has hyprcursor's format as well as (or instead of) X's.
    pub hyprcursor: bool,
    /// The sizes its arrow comes in.
    pub sizes: Vec<u32>,
    /// A few of its cursors, written out as PNG files.
    pub previews: Vec<String>,
}

/// The icons shown for each theme: each is a list of names, the first one
/// a theme has being the one used.
const PREVIEW_ICONS: &[&[&str]] = &[
    &["folder"],
    &["user-home", "folder-home"],
    &["utilities-terminal", "terminal", "org.gnome.Terminal", "kitty"],
    &["preferences-system", "preferences-desktop", "systemsettings", "org.gnome.Settings"],
    &["web-browser", "internet-web-browser", "firefox", "chromium"],
    &["text-x-generic", "text-plain"],
];

/// The cursors shown for each theme.
const PREVIEW_CURSORS: &[&[&str]] = &[
    &["left_ptr", "default", "arrow"],
    &["hand2", "pointer", "hand1"],
    &["xterm", "text", "ibeam"],
    &["watch", "wait", "progress"],
];

/// A theme folder's `index.theme`, if it has one.
fn index(dir: &Path) -> Option<String> {
    std::fs::read_to_string(dir.join("index.theme")).ok()
}

fn key(text: &str, k: &str) -> Option<String> {
    ini_get(text, "Icon Theme", k).filter(|v| !v.is_empty())
}

/// Every theme folder, the first one found under each name winning, as the
/// spec says.
fn theme_folders(p: &Places) -> Vec<(String, PathBuf)> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for root in &p.theme_dirs {
        let Ok(entries) = std::fs::read_dir(root) else { continue };
        let mut found: Vec<(String, PathBuf)> = entries
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
            .collect();
        found.sort();
        for (id, dir) in found {
            if id == "default" || id == "hicolor" || id.starts_with('.') {
                continue;
            }
            if seen.insert(id.clone()) {
                out.push((id, dir));
            }
        }
    }
    out
}

/// The icon themes installed, with their previews.
pub fn icon_themes(p: &Places) -> Vec<IconTheme> {
    let folders = theme_folders(p);
    let mut out: Vec<IconTheme> = folders
        .iter()
        .filter_map(|(id, dir)| {
            let text = index(dir)?;
            // Hidden keeps a theme out of pickers, which Adwaita asks for
            // although it is what GTK falls back to: it stays listed.
            if key(&text, "Hidden").is_some_and(|h| h == "true") && id != "Adwaita" {
                return None;
            }
            key(&text, "Directories")?;
            let previews = PREVIEW_ICONS
                .iter()
                .filter_map(|names| names.iter().find_map(|n| find_icon(&folders, id, n, 48, 0)))
                .map(|f| f.display().to_string())
                .collect();
            Some(IconTheme {
                id: id.clone(),
                name: key(&text, "Name").unwrap_or_else(|| id.clone()),
                dir: paths::contract(dir),
                previews,
            })
        })
        .collect();
    out.sort_by_key(|t| t.name.to_lowercase());
    out
}

/// An icon's file in a theme, or in the themes it inherits from (not
/// hicolor, which every theme falls back to and which says nothing about
/// how this one looks): the size nearest `want`, scalable counting as exact.
fn find_icon(
    folders: &[(String, PathBuf)],
    theme: &str,
    name: &str,
    want: i64,
    depth: usize,
) -> Option<PathBuf> {
    if depth > 3 {
        return None;
    }
    let dir = &folders.iter().find(|(id, _)| id == theme)?.1;
    let text = index(dir)?;
    let mut best: Option<(i64, PathBuf)> = None;
    for sub in key(&text, "Directories").unwrap_or_default().split(',').map(str::trim) {
        if sub.is_empty() {
            continue;
        }
        let kind = ini_get(&text, sub, "Type").unwrap_or_default();
        let size: i64 = ini_get(&text, sub, "Size").and_then(|s| s.parse().ok()).unwrap_or(want);
        let scale: i64 = ini_get(&text, sub, "Scale").and_then(|s| s.parse().ok()).unwrap_or(1);
        if scale != 1 {
            continue;
        }
        for ext in ["svg", "png"] {
            let file = dir.join(sub).join(format!("{name}.{ext}"));
            if !file.is_file() {
                continue;
            }
            let score = if kind == "Scalable" && ext == "svg" { 0 } else { (size - want).abs() };
            if best.as_ref().is_none_or(|(s, _)| score < *s) {
                best = Some((score, file));
            }
        }
    }
    if let Some((_, f)) = best {
        return Some(f);
    }
    key(&text, "Inherits")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty() && *t != "hicolor")
        .find_map(|t| find_icon(folders, t, name, want, depth + 1))
}

/// The cursor themes installed. Previews are written to `p.cache`.
pub fn cursor_themes(p: &Places) -> Vec<CursorTheme> {
    let _ = std::fs::create_dir_all(&p.cache);
    let mut out: Vec<CursorTheme> = theme_folders(p)
        .into_iter()
        .filter_map(|(id, dir)| {
            let xcursor = dir.join("cursors").is_dir();
            let hyprcursor =
                dir.join("manifest.hl").is_file() || dir.join("manifest.toml").is_file();
            if !xcursor && !hyprcursor {
                return None;
            }
            let text = index(&dir).unwrap_or_default();
            let read = |names: &[&str]| {
                names.iter().find_map(|n| std::fs::read(dir.join("cursors").join(n)).ok())
            };
            let sizes = read(PREVIEW_CURSORS[0]).map(|b| xcursor_sizes(&b)).unwrap_or_default();
            let previews = PREVIEW_CURSORS
                .iter()
                .enumerate()
                .filter_map(|(i, names)| {
                    let (w, h, rgba) = xcursor_image(&read(names)?, 32)?;
                    let file = p.cache.join(format!("{id}-{i}.png"));
                    write_png(&file, w, h, &rgba).ok()?;
                    Some(file.display().to_string())
                })
                .collect();
            Some(CursorTheme {
                name: key(&text, "Name").unwrap_or_else(|| id.clone()),
                id,
                dir: paths::contract(&dir),
                hyprcursor,
                sizes,
                previews,
            })
        })
        .collect();
    out.sort_by_key(|t| t.name.to_lowercase());
    out
}

// ---------------------------------------------------------------------------
// Xcursor files
// ---------------------------------------------------------------------------

const XCURSOR_IMAGE: u32 = 0xfffd_0002;

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// The images in an Xcursor file: (nominal size, offset of its chunk).
fn xcursor_toc(b: &[u8]) -> Vec<(u32, usize)> {
    if b.get(..4) != Some(b"Xcur") {
        return Vec::new();
    }
    let Some(n) = u32_at(b, 12) else { return Vec::new() };
    (0..n.min(1024) as usize)
        .filter_map(|i| {
            let at = 16 + i * 12;
            (u32_at(b, at)? == XCURSOR_IMAGE)
                .then_some((u32_at(b, at + 4)?, u32_at(b, at + 8)? as usize))
        })
        .collect()
}

/// The sizes an Xcursor file has, smallest first.
pub fn xcursor_sizes(b: &[u8]) -> Vec<u32> {
    let mut s: Vec<u32> = xcursor_toc(b).into_iter().map(|(size, _)| size).collect();
    s.sort_unstable();
    s.dedup();
    s
}

/// The first frame at the size nearest `want`, as straight RGBA.
pub fn xcursor_image(b: &[u8], want: u32) -> Option<(u32, u32, Vec<u8>)> {
    let (_, at) = xcursor_toc(b).into_iter().min_by_key(|(size, _)| size.abs_diff(want))?;
    let width = u32_at(b, at + 16)?;
    let height = u32_at(b, at + 20)?;
    if width == 0 || height == 0 || width > 1024 || height > 1024 {
        return None;
    }
    let start = at + u32_at(b, at)? as usize;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for i in 0..(width * height) as usize {
        // Premultiplied ARGB, little-endian: B, G, R, A in memory.
        let px = b.get(start + i * 4..start + i * 4 + 4)?;
        let a = px[3];
        let un = |c: u8| {
            if a == 0 {
                0
            } else {
                ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8
            }
        };
        rgba.extend_from_slice(&[un(px[2]), un(px[1]), un(px[0]), a]);
    }
    Some((width, height, rgba))
}

fn write_png(path: &Path, w: u32, h: u32, rgba: &[u8]) -> Result<()> {
    let file = std::fs::File::create(path).map_err(|e| Error::io(path, e))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut wr| wr.write_image_data(rgba))
        .map_err(|e| Error::other(format!("could not write {}: {e}", path.display())))
}

// ---------------------------------------------------------------------------
// What is set
// ---------------------------------------------------------------------------

/// Where each theme is set, and to what.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Current {
    /// The icon theme, place by place.
    pub icons: Vec<Setting>,
    /// The cursor theme, place by place.
    pub cursors: Vec<Setting>,
    /// The cursor size gsettings or GTK has.
    pub cursor_size: Option<u32>,
}

/// One place a theme is set.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Setting {
    /// What reads it.
    pub place: String,
    /// What it says.
    pub value: String,
}

fn gtk_ini(p: &Places, v: &str) -> PathBuf {
    p.config_home.join(v).join("settings.ini")
}

fn ct_conf(p: &Places, v: &str) -> PathBuf {
    p.config_home.join(v).join(format!("{v}.conf"))
}

fn x_default(p: &Places) -> PathBuf {
    p.home_icons.join("default").join("index.theme")
}

/// Read every place.
pub fn current(p: &Places) -> Current {
    let mut c = Current::default();
    let push = |list: &mut Vec<Setting>, place: &str, value: Option<String>| {
        if let Some(value) = value.filter(|v| !v.is_empty()) {
            list.push(Setting { place: place.into(), value });
        }
    };
    let read = |f: PathBuf| std::fs::read_to_string(f).ok();
    push(&mut c.icons, "gsettings", gsettings_get("icon-theme"));
    push(&mut c.cursors, "gsettings", gsettings_get("cursor-theme"));
    for (v, label) in [("gtk-3.0", "GTK 3"), ("gtk-4.0", "GTK 4")] {
        let text = read(gtk_ini(p, v)).unwrap_or_default();
        push(&mut c.icons, label, ini_get(&text, "Settings", "gtk-icon-theme-name"));
        push(&mut c.cursors, label, ini_get(&text, "Settings", "gtk-cursor-theme-name"));
        if c.cursor_size.is_none() {
            c.cursor_size =
                ini_get(&text, "Settings", "gtk-cursor-theme-size").and_then(|s| s.parse().ok());
        }
    }
    for v in ["qt6ct", "qt5ct"] {
        let text = read(ct_conf(p, v)).unwrap_or_default();
        push(&mut c.icons, v, ini_get(&text, "Appearance", "icon_theme"));
    }
    let x = read(x_default(p)).unwrap_or_default();
    push(&mut c.cursors, "X apps (~/.icons/default)", key(&x, "Inherits"));
    if !cfg!(test) {
        push(&mut c.cursors, "Hyprland (XCURSOR_THEME)", std::env::var("XCURSOR_THEME").ok());
    }
    if let Some(size) = gsettings_get("cursor-size").and_then(|s| s.parse().ok()) {
        c.cursor_size = Some(size);
    }
    c
}

// ---------------------------------------------------------------------------
// Setting them
// ---------------------------------------------------------------------------

/// Write one INI file, backed up, when it exists and the value changes.
fn ini_write(path: &Path, section: &str, pairs: &[(&str, &str)], keep: usize) -> Result<bool> {
    let Ok(text) = std::fs::read_to_string(path) else { return Ok(false) };
    let next = ini_set(&text, section, pairs);
    if next == text {
        return Ok(false);
    }
    paths::backup(path, keep)?;
    paths::write_atomic(path, &next)?;
    Ok(true)
}

/// A theme name that is safe to write into every one of these files.
fn checked_name(name: &str) -> Result<&str> {
    let n = name.trim();
    if n.is_empty() || n.contains(['/', '\n', '\r', '"', '\'', '\\']) || n.starts_with('.') {
        return Err(Error::other(format!("`{name}` is not a theme name")));
    }
    Ok(n)
}

/// Set the icon theme everywhere it is read. Returns what was changed.
pub fn set_icon_theme(p: &Places, name: &str, keep: usize) -> Result<Vec<String>> {
    let name = checked_name(name)?;
    let mut changed = Vec::new();
    if paths::which("gsettings").is_some() || cfg!(test) {
        gsettings_set("icon-theme", name);
        changed.push("gsettings".to_string());
    }
    for (v, label) in [("gtk-3.0", "GTK 3"), ("gtk-4.0", "GTK 4")] {
        if ini_write(&gtk_ini(p, v), "Settings", &[("gtk-icon-theme-name", name)], keep)? {
            changed.push(label.into());
        }
    }
    for v in ["qt6ct", "qt5ct"] {
        if ini_write(&ct_conf(p, v), "Appearance", &[("icon_theme", name)], keep)? {
            changed.push(v.into());
        }
    }
    Ok(changed)
}

/// Set the cursor theme on the desktop: everywhere but the Hyprland config,
/// which is [`apply_block`]'s. Returns what was changed.
pub fn set_cursor_theme(p: &Places, name: &str, size: u32, keep: usize) -> Result<Vec<String>> {
    let name = checked_name(name)?;
    if !(8..=256).contains(&size) {
        return Err(Error::other(format!("a cursor {size} pixels big is not one to set")));
    }
    let size_s = size.to_string();
    let mut changed = Vec::new();
    if paths::which("gsettings").is_some() || cfg!(test) {
        gsettings_set("cursor-theme", name);
        gsettings_set("cursor-size", &size_s);
        changed.push("gsettings".to_string());
    }
    for (v, label) in [("gtk-3.0", "GTK 3"), ("gtk-4.0", "GTK 4")] {
        let pairs = [("gtk-cursor-theme-name", name), ("gtk-cursor-theme-size", size_s.as_str())];
        if ini_write(&gtk_ini(p, v), "Settings", &pairs, keep)? {
            changed.push(label.into());
        }
    }
    // What X apps fall back to. Kept to the one line when it exists, since
    // other tools write the file too.
    let x = x_default(p);
    let text = std::fs::read_to_string(&x).unwrap_or_default();
    let next = if text.trim().is_empty() {
        format!("[Icon Theme]\nName=Default\nComment=Default Cursor Theme\nInherits={name}\n")
    } else {
        ini_set(&text, "Icon Theme", &[("Inherits", name)])
    };
    if next != text {
        if x.exists() {
            paths::backup(&x, keep)?;
        }
        std::fs::create_dir_all(x.parent().expect("has a parent")).map_err(|e| Error::io(&x, e))?;
        paths::write_atomic(&x, &next)?;
        changed.push("X apps".into());
    }
    if !cfg!(test) && crate::hyprctl::is_running() {
        crate::hyprctl::run_args(&["setcursor", name, &size_s])?;
        changed.push("Hyprland, now".into());
    }
    Ok(changed)
}

// ---------------------------------------------------------------------------
// Hyprland's config
// ---------------------------------------------------------------------------

/// The cursor block's markers.
pub const MARKERS: Markers = Markers {
    begin: "-- >>> HyprCustard cursor (managed by hyprcustard) >>>",
    end: "-- <<< HyprCustard cursor (managed by hyprcustard) <<<",
    name: "HyprCustard cursor",
};

/// What a handed-over line starts with, after its indentation.
pub const MOVED: &str = "-- [moved to the HyprCustard cursor block] ";

const VARS: [&str; 4] = ["XCURSOR_THEME", "XCURSOR_SIZE", "HYPRCURSOR_THEME", "HYPRCURSOR_SIZE"];

/// The block's body: the variables apps start with, and the cursor
/// Hyprland itself draws, set again at login.
pub fn body(name: &str, size: u32, hyprcursor: bool) -> String {
    let mut out = String::from(
        "-- The cursor hyprcustard set. Pick another on its Icons page, or use\n\
         -- \"Give them back\" there to bring back the lines this replaced.\n",
    );
    let env = |k: &str, v: &str| format!("hl.env({}, {})\n", lua_quote(k), lua_quote(v));
    out.push_str(&env("XCURSOR_THEME", name));
    out.push_str(&env("XCURSOR_SIZE", &size.to_string()));
    if hyprcursor {
        out.push_str(&env("HYPRCURSOR_THEME", name));
    }
    out.push_str(&env("HYPRCURSOR_SIZE", &size.to_string()));
    out.push_str(&format!(
        "hl.on(\"hyprland.start\", function()\n    hl.exec_cmd({})\nend)\n",
        lua_quote(&format!("hyprctl setcursor {name} {size}"))
    ));
    out
}

/// A line in the user's config that sets the cursor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Line {
    /// 1-based.
    pub number: usize,
    /// The line, trimmed.
    pub text: String,
    /// Whether it is one statement on its own line.
    pub movable: bool,
}

/// Lines outside every managed block, not commented out, that set a cursor
/// variable or run `hyprctl setcursor`.
pub fn user_lines(text: &str) -> Vec<Line> {
    lines_outside_blocks(text)
        .filter_map(|(number, l)| {
            let t = l.trim();
            if t.starts_with("--") || t.starts_with('#') {
                return None;
            }
            let env = (t.starts_with("hl.env(") || t.starts_with("env"))
                && VARS.iter().any(|v| t.contains(v));
            let setcursor = t.contains("exec") && t.contains("hyprctl setcursor");
            if !env && !setcursor {
                return None;
            }
            let movable =
                if env { one_call(t, "hl.env(") || t.starts_with("env") } else { movable(l) };
            Some(Line { number, text: t.to_string(), movable })
        })
        .collect()
}

fn one_call(t: &str, opener: &str) -> bool {
    let t = match (t.rfind(')'), t.rfind("--")) {
        (Some(close), Some(dash)) if dash > close => t[..dash].trim_end(),
        _ => t,
    };
    t.starts_with(opener)
        && t.ends_with(')')
        && t.matches(opener).count() == 1
        && t.matches('(').count() == t.matches(')').count()
        && !t.contains(';')
        && !t.contains("function")
}

/// Lines hyprcustard commented out when it took over.
pub fn moved_lines(text: &str) -> Vec<usize> {
    text.lines()
        .enumerate()
        .filter(|(_, l)| l.trim_start().starts_with(MOVED))
        .map(|(i, _)| i + 1)
        .collect()
}

/// Whether the block is there.
pub fn managed(text: &str) -> bool {
    matches!(block::split_with(text, &MARKERS), Ok(Some(_)))
}

/// Write the block. Refused while the user's own lines set the cursor:
/// both would, and which wins would depend on where they sit.
pub fn apply_block(text: &str, body: &str) -> Result<String> {
    if !managed(text) && !user_lines(text).is_empty() {
        return Err(Error::other(
            "Your config sets the cursor itself; take those lines over first",
        ));
    }
    block::splice_with(text, body, &MARKERS)
}

/// Comment the user's cursor lines out and write the block.
pub fn hand_over(text: &str, body: &str) -> Result<String> {
    let lines = user_lines(text);
    let stuck: Vec<String> =
        lines.iter().filter(|l| !l.movable).map(|l| l.number.to_string()).collect();
    if !stuck.is_empty() {
        return Err(Error::other(format!(
            "{} {} of hyprland.lua sets the cursor alongside other code, so it cannot be \
             commented out on its own. Put it on a line of its own, or remove it yourself, \
             and try again",
            if stuck.len() == 1 { "Line" } else { "Lines" },
            stuck.join(", ")
        )));
    }
    let numbers: Vec<usize> = lines.iter().map(|l| l.number).collect();
    let mut out = String::new();
    for (i, l) in split_keep(text).into_iter().enumerate() {
        if numbers.contains(&(i + 1)) {
            let indent = &l[..l.len() - l.trim_start().len()];
            out.push_str(&format!("{indent}{MOVED}{}", &l[indent.len()..]));
        } else {
            out.push_str(l);
        }
    }
    block::splice_with(&out, body, &MARKERS)
}

/// Take the block out and put the user's lines back as they were.
pub fn give_back(text: &str) -> Result<String> {
    let text = block::remove_with(text, &MARKERS)?;
    Ok(split_keep(&text)
        .into_iter()
        .map(|l| {
            let indent = &l[..l.len() - l.trim_start().len()];
            match l[indent.len()..].strip_prefix(MOVED) {
                Some(rest) => format!("{indent}{rest}"),
                None => l.to_string(),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn places(root: &Path) -> Places {
        Places {
            config_home: root.join("config"),
            theme_dirs: vec![root.join("local"), root.join("system")],
            home_icons: root.join("home-icons"),
            cache: root.join("cache"),
        }
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// An Xcursor file with one frame per size, each pixel the same.
    fn xcursor(sizes: &[u32], argb: u32) -> Vec<u8> {
        let mut b = b"Xcur".to_vec();
        b.extend(16u32.to_le_bytes());
        b.extend(0x10000u32.to_le_bytes());
        b.extend((sizes.len() as u32).to_le_bytes());
        let mut at = 16 + 12 * sizes.len();
        let mut chunks = Vec::new();
        for &s in sizes {
            b.extend(XCURSOR_IMAGE.to_le_bytes());
            b.extend(s.to_le_bytes());
            b.extend((at as u32).to_le_bytes());
            let mut c = Vec::new();
            for v in [36, XCURSOR_IMAGE, s, 1, s, s, 0, 0, 50] {
                c.extend(v.to_le_bytes());
            }
            for _ in 0..s * s {
                c.extend(argb.to_le_bytes());
            }
            at += c.len();
            chunks.push(c);
        }
        for c in chunks {
            b.extend(c);
        }
        b
    }

    #[test]
    fn finds_icon_and_cursor_themes_with_previews() {
        let dir = tempfile::tempdir().unwrap();
        let p = places(dir.path());
        let local = p.theme_dirs[0].clone();
        let system = p.theme_dirs[1].clone();
        write(
            &local.join("Gruv/index.theme"),
            "[Icon Theme]\nName=Gruvbox Plus\nInherits=Base,hicolor\nDirectories=48x48/places,scalable/apps\n\
             [48x48/places]\nSize=48\nType=Fixed\n[scalable/apps]\nSize=64\nType=Scalable\n",
        );
        write(&local.join("Gruv/48x48/places/folder.png"), "png");
        write(&local.join("Gruv/scalable/apps/utilities-terminal.svg"), "<svg/>");
        write(
            &system.join("Base/index.theme"),
            "[Icon Theme]\nName=Base\nDirectories=32/mimetypes\n[32/mimetypes]\nSize=32\n",
        );
        write(&system.join("Base/32/mimetypes/text-x-generic.svg"), "<svg/>");
        // The same name lower down the search order is not a second theme.
        write(&system.join("Gruv/index.theme"), "[Icon Theme]\nName=Old\nDirectories=x\n");
        // A cursor-only theme, and the folder X's default lives in.
        write(&local.join("Pointy/index.theme"), "[Icon Theme]\nName=Pointy\n");
        write(&local.join("Pointy/cursors/left_ptr"), "");
        std::fs::write(local.join("Pointy/cursors/left_ptr"), xcursor(&[24, 32, 48], 0x80ff0000))
            .unwrap();
        write(&local.join("default/index.theme"), "[Icon Theme]\nInherits=Pointy\n");

        let icons = icon_themes(&p);
        let names: Vec<&str> = icons.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["Base", "Gruvbox Plus"]);
        let gruv = &icons[1];
        assert!(gruv.previews[0].ends_with("48x48/places/folder.png"));
        assert!(gruv.previews[1].ends_with("utilities-terminal.svg"));
        assert!(gruv.previews[2].ends_with("Base/32/mimetypes/text-x-generic.svg"), "inherited");

        let cursors = cursor_themes(&p);
        assert_eq!(cursors.len(), 1);
        assert_eq!(cursors[0].id, "Pointy");
        assert_eq!(cursors[0].sizes, vec![24, 32, 48]);
        assert_eq!(cursors[0].previews.len(), 1);
        assert!(Path::new(&cursors[0].previews[0]).is_file());
    }

    #[test]
    fn xcursor_frames_come_out_unpremultiplied() {
        // Half-transparent red, premultiplied: 0x80 alpha, 0x80 red.
        let (w, h, rgba) = xcursor_image(&xcursor(&[24, 32], 0x8080_0000), 30).unwrap();
        assert_eq!((w, h), (32, 32));
        assert_eq!(&rgba[..4], &[255, 0, 0, 0x80]);
        assert_eq!(xcursor_image(b"nope", 24), None);
        assert!(xcursor_sizes(b"Xcur").is_empty());
    }

    #[test]
    fn themes_are_written_where_each_file_already_is() {
        let dir = tempfile::tempdir().unwrap();
        let p = places(dir.path());
        let gtk3 = p.config_home.join("gtk-3.0/settings.ini");
        write(&gtk3, "[Settings]\ngtk-theme-name=adw-gtk3-dark\ngtk-icon-theme-name=Old\n");
        let qt6 = p.config_home.join("qt6ct/qt6ct.conf");
        write(&qt6, "[Appearance]\nicon_theme=Old\nstyle=Fusion\n");

        let changed = set_icon_theme(&p, "Papirus-Dark", 0).unwrap();
        assert_eq!(changed, vec!["gsettings", "GTK 3", "qt6ct"]);
        assert!(std::fs::read_to_string(&gtk3)
            .unwrap()
            .contains("gtk-icon-theme-name=Papirus-Dark"));
        assert_eq!(
            std::fs::read_to_string(&qt6).unwrap(),
            "[Appearance]\nicon_theme=Papirus-Dark\nstyle=Fusion\n"
        );
        assert!(!p.config_home.join("gtk-4.0/settings.ini").exists(), "not created");

        let changed = set_cursor_theme(&p, "phinger-cursors-dark", 32, 0).unwrap();
        assert!(changed.contains(&"X apps".to_string()));
        let gtk = std::fs::read_to_string(&gtk3).unwrap();
        assert!(
            gtk.contains("gtk-cursor-theme-name=phinger-cursors-dark\ngtk-cursor-theme-size=32")
        );
        assert!(std::fs::read_to_string(x_default(&p))
            .unwrap()
            .contains("Inherits=phinger-cursors-dark"));

        let now = current(&p);
        assert!(now.icons.iter().any(|s| s.place == "qt6ct" && s.value == "Papirus-Dark"));
        assert_eq!(now.cursor_size, Some(32));

        assert!(set_icon_theme(&p, "../etc", 0).is_err());
        assert!(set_cursor_theme(&p, "x", 2, 0).is_err());
    }

    /// The lines from a real config.
    const CONFIG: &str = "\
hl.env(\"XCURSOR_SIZE\", \"24\")
hl.env(\"XCURSOR_THEME\", \"Capitaine-Cursors-Gruvbox\")
hl.env(\"HYPRCURSOR_SIZE\", \"24\")
hl.env(\"QT_QPA_PLATFORMTHEME\", \"qt6ct\")
hl.on(\"hyprland.start\", function()
    hl.exec_cmd(\"hyprctl setcursor Capitaine-Cursors-Gruvbox 24\")
end)
";

    #[test]
    fn cursor_lines_are_taken_over_and_given_back() {
        let lines = user_lines(CONFIG);
        let numbers: Vec<usize> = lines.iter().map(|l| l.number).collect();
        assert_eq!(numbers, vec![1, 2, 3, 6]);
        assert!(lines.iter().all(|l| l.movable));
        let body = body("phinger-cursors-dark", 32, false);
        assert!(apply_block(CONFIG, &body).is_err(), "not beside the user's own");

        let taken = hand_over(CONFIG, &body).unwrap();
        assert!(managed(&taken) && user_lines(&taken).is_empty());
        assert_eq!(moved_lines(&taken), vec![1, 2, 3, 6]);
        assert!(taken.contains("hl.env(\"XCURSOR_THEME\", \"phinger-cursors-dark\")"));
        assert!(!taken.contains("hl.env(\"HYPRCURSOR_THEME\""), "an X-only theme");
        assert!(taken.contains("hl.exec_cmd(\"hyprctl setcursor phinger-cursors-dark 32\")"));
        let again = apply_block(&taken, &self::body("Adwaita", 24, true)).unwrap();
        assert!(again.contains("hl.env(\"HYPRCURSOR_THEME\", \"Adwaita\")"));
        assert_eq!(give_back(&again).unwrap(), CONFIG);
    }
}
