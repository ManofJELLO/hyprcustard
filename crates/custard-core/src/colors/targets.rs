//! The apps a scheme is written for.
//!
//! Each gets a colors file of its own, in its own format, next to its own
//! config, defining the same documented set of names: every Material role
//! (`primary`, `on_surface`, …) and base16 (`base00` … `base0F`). Apps whose
//! colors are fixed keys — a terminal's palette, a notification's frame —
//! have those keys set too, so they change with no edit to their config.
//!
//! The file only takes effect once the app's config loads it, and that is
//! the user's file, so nothing is added to it unless they ask: each target
//! knows the one line that loads its file, whether it is already there, and
//! how to add it, with a backup, when asked.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::pins::{self, Pin};
use super::scheme::Scheme;
use super::starship;
use super::toolkits::{self, Desktop, KVANTUM_THEME};
use crate::error::{Error, Result};
use crate::look::{config_call, LookValue};
use crate::model::lua_quote;

/// Quickshell's colors file: a QML singleton, so its name is the type name.
const QS_FILE: &str = "HyprcustardColors.qml";

/// The GTK colors file's name, beside each `gtk.css`.
const GTK_FILE: &str = "hyprcustard-colors.css";

/// The names given an alpha copy, for translucent bars and popups.
const ALPHA_ROLES: &[&str] = &[
    "background",
    "surface",
    "surface_container_low",
    "surface_container",
    "surface_container_high",
];

/// One app a scheme can be written for.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    /// `hyprland`, `waybar`, …
    pub id: &'static str,
    /// What the page shows.
    pub name: &'static str,
    /// Whether its program is on PATH.
    pub installed: bool,
    /// The config that has to load the colors file.
    pub config: PathBuf,
    /// The colors file.
    pub file: PathBuf,
    /// The line that loads it, or empty when nothing needs adding.
    pub line: String,
    /// Where in the config the line goes, for the page to say.
    pub place: &'static str,
    /// Whether the config already loads the file.
    pub wired: bool,
    /// How to use the colors, beyond what the file sets by itself.
    pub usage: String,
    /// Why the colors may not show even once the file is loaded, or empty.
    pub warning: String,
    /// A change the page can offer to make that warning go away, as a
    /// button label, or empty. `colors.fix` makes it.
    pub fix: String,
    /// Whether loading the file means setting the config aside (it is backed
    /// up) and starting a new one, because the config is a whole theme
    /// whose colors would win over any line added to it.
    pub replaces: bool,
}

/// Every target, read from the disk as it is now.
///
/// `config_home` is `$XDG_CONFIG_HOME`; `hypr_config` the Hyprland config
/// the app edits, which need not be the default one.
pub fn all(config_home: &Path, hypr_config: &Path) -> Vec<Target> {
    all_in(config_home, hypr_config, &Desktop::read(config_home))
}

/// [`all`], with the session's GTK and Qt settings given.
pub fn all_in(config_home: &Path, hypr_config: &Path, desk: &Desktop) -> Vec<Target> {
    let installed = |p: &str| crate::paths::which(p).is_some();
    let hypr_file = hypr_config.with_file_name("hyprcustard-colors.lua");
    let ghostty_dir = config_home.join("ghostty");
    let ghostty_config = ["config.ghostty", "config"]
        .iter()
        .map(|n| ghostty_dir.join(n))
        .find(|p| p.is_file())
        .unwrap_or_else(|| ghostty_dir.join("config.ghostty"));
    let dunst_base = config_home.join("dunst").join("dunstrc");
    let starship_config = starship::config_path(config_home);
    let shell_dir = quickshell_dir(config_home);

    let mut list = vec![
        Target {
            id: "hyprland",
            name: "Hyprland",
            installed: installed("Hyprland") || installed("hyprctl"),
            config: hypr_config.to_path_buf(),
            line: format!(
                "pcall(dofile, {})  -- hyprcustard colors",
                lua_quote(&hypr_file.to_string_lossy())
            ),
            file: hypr_file,
            place: "at the end of",
            wired: false,
            usage: "Sets the window borders and shadow. Every color is also in the \
                    hyprcustard_colors table, for your own settings below that line."
                .into(),
            warning: String::new(),
            fix: String::new(),
            replaces: false,
        },
        Target {
            id: "waybar",
            name: "Waybar",
            installed: installed("waybar"),
            config: config_home.join("waybar").join("style.css"),
            file: config_home.join("waybar").join("hyprcustard-colors.css"),
            line: "@import \"hyprcustard-colors.css\";".into(),
            place: "at the top of",
            wired: false,
            usage: "Use the names in your style: @primary, @on_surface, @base0D. \
                    @surface_alpha and friends follow the bar opacity. A color your \
                    style defines itself keeps your value, until you hand it over."
                .into(),
            warning: String::new(),
            fix: String::new(),
            replaces: false,
        },
        Target {
            id: "rofi",
            name: "Rofi",
            installed: installed("rofi"),
            config: config_home.join("rofi").join("config.rasi"),
            file: config_home.join("rofi").join("hyprcustard-colors.rasi"),
            line: "@import \"hyprcustard-colors.rasi\"".into(),
            // Rofi drops whatever came before an `@theme`, so the import
            // goes after one.
            place: "at the top (after any @theme line) of",
            wired: false,
            usage: "Use the names in your theme: @primary, @on-surface, @base0D \
                    (rofi spells them with dashes). The import comes after your @theme \
                    line, so where your theme defines the same name (background, for \
                    one), the scheme's color is used. Colors your config defines itself \
                    keep your value, until you hand them over."
                .into(),
            warning: String::new(),
            fix: String::new(),
            replaces: false,
        },
        Target {
            id: "quickshell",
            name: "Quickshell",
            installed: installed("quickshell") || installed("qs"),
            config: shell_dir.join("shell.qml"),
            file: shell_dir.join(QS_FILE),
            line: String::new(),
            place: "",
            wired: uses_singleton(&shell_dir),
            usage: "A QML singleton beside your shell.qml, so no line is needed: use \
                    HyprcustardColors.primary, .on_surface, .base0D in your shell's QML \
                    (files in a subfolder reach it after `import qs`). .surface_alpha and \
                    friends follow the bar opacity. Quickshell reloads itself when the file \
                    changes."
                .into(),
            warning: String::new(),
            fix: String::new(),
            replaces: false,
        },
        Target {
            id: "bar",
            name: "hyprcustard's bar",
            installed: crate::bar::installed(config_home),
            config: crate::bar::dir(config_home).join("shell.qml"),
            file: crate::bar::dir(config_home).join(crate::bar::COLORS_FILE),
            line: String::new(),
            place: "",
            wired: true,
            usage: "The bar from the Bar page, which uses these colors by itself and follows \
                    a new scheme at once. Its opacity is set on the Bar page."
                .into(),
            warning: String::new(),
            fix: String::new(),
            replaces: false,
        },
        Target {
            id: "dunst",
            name: "Dunst",
            installed: installed("dunst"),
            config: dunst_base.clone(),
            file: config_home.join("dunst").join("dunstrc.d").join("90-hyprcustard-colors.conf"),
            line: String::new(),
            place: "",
            wired: dunst_base.is_file(),
            usage: "Sets the notification colors and frames. Dunst only reads this \
                    folder when you have your own dunstrc."
                .into(),
            warning: String::new(),
            fix: String::new(),
            replaces: false,
        },
        Target {
            id: "kitty",
            name: "kitty",
            installed: installed("kitty"),
            config: config_home.join("kitty").join("kitty.conf"),
            file: config_home.join("kitty").join("hyprcustard-colors.conf"),
            line: "include hyprcustard-colors.conf".into(),
            place: "at the end of",
            wired: false,
            usage: "Sets the terminal palette, cursor, selection and tabs.".into(),
            warning: String::new(),
            fix: String::new(),
            replaces: false,
        },
        Target {
            id: "ghostty",
            name: "Ghostty",
            installed: installed("ghostty"),
            config: ghostty_config,
            file: ghostty_dir.join("hyprcustard-colors"),
            line: "config-file = ?hyprcustard-colors".into(),
            place: "at the end of",
            wired: false,
            usage: "Sets the terminal palette, cursor and selection, over any theme.".into(),
            warning: String::new(),
            fix: String::new(),
            replaces: false,
        },
        Target {
            id: "starship",
            name: "Starship",
            installed: installed("starship"),
            file: starship_config.with_file_name("hyprcustard-starship-colors.toml"),
            config: starship_config,
            line: format!("palette = \"{}\"", starship::PALETTE),
            place: "at the top, with a copy of the scheme's [palettes.hyprcustard] table, of",
            wired: false,
            usage: "Starship cannot load another file, so your starship.toml holds a copy of \
                    the palette, which every Apply refreshes; nothing else in it changes. Use \
                    the names in your styles, in lowercase as Starship reads them: fg:primary, \
                    bg:surface_container, base0d. Its own colors (red, bright-blue…) follow the \
                    scheme too. The names your old palette defined are copied in, each paired \
                    with a scheme color in a comment you can change. The old palette stays, so \
                    pointing palette back at it undoes this."
                .into(),
            warning: String::new(),
            fix: String::new(),
            replaces: false,
        },
    ];
    let pins = super::pins::find(&super::qt_style::hypr_sources(hypr_config));
    list.extend(toolkit_targets(config_home, desk, &pins));
    for t in &mut list {
        if t.id == "starship" {
            t.wired = starship::wired(&std::fs::read_to_string(&t.config).unwrap_or_default());
        } else if !t.line.is_empty() && !matches!(t.id, "qt6ct" | "qt5ct" | "kvantum") {
            t.wired = loads(&t.config, &t.file);
        }
        // An import before rofi's `@theme` is thrown away by it.
        if t.id == "rofi" && t.wired {
            let text = std::fs::read_to_string(&t.config).unwrap_or_default();
            t.wired = !imports_before_theme(&text, &t.file);
        }
    }
    list
}

/// GTK 3, GTK 4, qt6ct, qt5ct and Kvantum. See [`toolkits`].
fn toolkit_targets(config_home: &Path, desk: &Desktop, pins: &[Pin]) -> Vec<Target> {
    let lib = toolkits::has_lib;
    let follows = desk.gtk_theme_follows();
    let mut out = Vec::new();

    for (v, id, name, libname) in
        [(3, "gtk3", "GTK 3", "libgtk-3.so.0"), (4, "gtk4", "GTK 4", "libgtk-4.so.1")]
    {
        let dir = config_home.join(format!("gtk-{v}.0"));
        let config = dir.join("gtk.css");
        let text = std::fs::read_to_string(&config).unwrap_or_default();
        let replaces = toolkits::whole_theme(&text, GTK_FILE);
        let mut usage = String::from(
            "Defines GTK's color names (window_bg_color, accent_bg_color, theme_bg_color…) \
             and every scheme name, for your own rules. Apps opened from now on use them; \
             open ones keep their colors until restarted.",
        );
        let mut warning = String::new();
        let mut fix = String::new();
        if v == 4 {
            usage.push_str(
                " Apps built on libadwaita follow the scheme; other GTK 4 apps only under \
                 the adw-gtk3 theme. Applying also sets the desktop's light or dark \
                 preference to match.",
            );
            let set = pins::live(pins, "color-scheme");
            if let Some(first) = set.first() {
                warning = format!(
                    "Your Hyprland config sets the light or dark preference to {} at {} each \
                     time Hyprland runs it, over the one Applying sets, so apps keep that \
                     whatever the scheme is.",
                    first.value,
                    pins::places(&set)
                );
                fix = "Take it out".into();
            }
        } else if !follows {
            warning = format!(
                "Your GTK theme, {}, draws with fixed colors of its own, so GTK 3 apps keep \
                 its look whatever this file says. The adw-gtk3 theme draws with these names.",
                desk.gtk_theme
            );
            if desk.adw_gtk3(true).is_some() || desk.adw_gtk3(false).is_some() {
                fix = "Switch to adw-gtk3".into();
            } else {
                warning.push_str(" Install it to use it (on Arch, the adw-gtk-theme package).");
            }
            let set = pins::live(pins, "gtk-theme");
            if !set.is_empty() && !fix.is_empty() {
                warning.push_str(&format!(
                    " Your Hyprland config sets the theme each time Hyprland runs it, at {}; \
                     switching takes that line out too, or it would switch back.",
                    pins::places(&set)
                ));
            }
        } else if let Some(first) = pins::live(pins, "gtk-theme").first() {
            // The theme follows now, but a line will put its own back, or
            // undo the light or dark version Applying chooses.
            let set = pins::live(pins, "gtk-theme");
            warning = format!(
                "Your Hyprland config sets the GTK theme to {} at {} each time Hyprland runs it",
                first.value,
                pins::places(&set)
            );
            warning.push_str(&if first.value != desk.gtk_theme {
                format!(", over {}.", desk.gtk_theme)
            } else {
                ", so the light or dark version Applying chooses does not last.".to_string()
            });
            fix = "Take it out".into();
        }
        if replaces {
            if !warning.is_empty() {
                warning.push(' ');
            }
            warning.push_str(&format!(
                "Your gtk.css is a whole theme with its own colors, which GTK {v} draws with \
                 over anything else. Loading the colors sets it aside (it is backed up) and \
                 starts a new gtk.css."
            ));
        }
        out.push(Target {
            id,
            name,
            installed: lib(libname),
            config,
            file: dir.join(GTK_FILE),
            line: format!("@import \"{GTK_FILE}\";"),
            // Last, so the scheme wins over colors gtk.css defines itself.
            place: "at the end of",
            wired: false,
            usage,
            warning,
            fix,
            replaces,
        });
    }

    for (id, name) in [("qt6ct", "Qt 6 (qt6ct)"), ("qt5ct", "Qt 5 (qt5ct)")] {
        let dir = config_home.join(id);
        let config = dir.join(format!("{id}.conf"));
        let file = dir.join("colors").join("hyprcustard.conf");
        let text = std::fs::read_to_string(&config).unwrap_or_default();
        let get = |k: &str| toolkits::ini_get(&text, "Appearance", k);
        let wired = get("color_scheme_path").is_some_and(|p| Path::new(&p) == file)
            && get("custom_palette").as_deref() == Some("true");
        let mut warning = String::new();
        if !desk.qt_uses_ct() {
            warning = format!(
                "Qt apps are not reading {id}'s settings: QT_QPA_PLATFORMTHEME is {}. Set it \
                 to {id} in your Hyprland environment for this to apply.",
                desk.qt_platform.as_deref().map_or("not set".into(), |p| format!("{p:?}"))
            );
        } else if toolkits::kvantum_in_use(config_home, desk) {
            warning = "Your Qt style is Kvantum, which paints with its own theme's colors \
                       rather than this palette; the Kvantum row recolors that."
                .into();
        }
        out.push(Target {
            id,
            name,
            installed: crate::paths::which(id).is_some(),
            line: format!("color_scheme_path={}", file.display()),
            file,
            config,
            place: "under [Appearance], with custom_palette=true, in",
            wired,
            usage: format!(
                "A color scheme in {id}'s own format, so it is also listed in {id} as \
                 hyprcustard. Qt apps opened from now on use it."
            ),
            warning,
            fix: String::new(),
            replaces: false,
        });
    }

    let kv = config_home.join("Kvantum");
    let kv_config = kv.join("kvantum.kvconfig");
    let kv_text = std::fs::read_to_string(&kv_config).unwrap_or_default();
    let base = toolkits::kvantum_base(config_home, desk);
    let mut warning = match &base {
        None => "Kvantum is on its built-in theme, which has no files to copy. Choose a theme \
                 in Kvantum Manager, and a recolored copy of it is made from then on."
            .to_string(),
        Some(_) => String::new(),
    };
    if !toolkits::kvantum_in_use(config_home, desk) {
        if !warning.is_empty() {
            warning.push(' ');
        }
        warning.push_str(
            "Qt apps are not drawing with Kvantum (choose kvantum as the style in qt6ct, or \
             without qt6ct, set QT_STYLE_OVERRIDE=kvantum).",
        );
    }
    out.push(Target {
        id: "kvantum",
        name: "Kvantum",
        installed: crate::paths::which("kvantummanager").is_some(),
        config: kv_config,
        file: kv.join(KVANTUM_THEME).join(format!("{KVANTUM_THEME}.kvconfig")),
        line: format!("theme={KVANTUM_THEME}"),
        place: "under [General] in",
        wired: toolkits::ini_get(&kv_text, "General", "theme").as_deref() == Some(KVANTUM_THEME),
        usage: match &base {
            Some(b) => format!(
                "A copy of your Kvantum theme, {}, with its colors and images recolored: close \
                 to the scheme rather than exact, since the theme's shading stays. Qt apps \
                 opened from now on use it.",
                b.name.trim_end_matches('#')
            ),
            None => "A recolored copy of your Kvantum theme.".into(),
        },
        warning,
        fix: String::new(),
        replaces: false,
    });
    out
}

/// Whether `config` mentions `file`'s name on a line that is not a comment.
fn loads(config: &Path, file: &Path) -> bool {
    let Some(name) = file.file_name().and_then(|n| n.to_str()) else { return false };
    let Ok(text) = std::fs::read_to_string(config) else { return false };
    text.lines().any(|l| {
        let t = l.trim_start();
        let comment = ["#", "//", "--", "/*"].iter().any(|c| t.starts_with(c));
        !comment && t.contains(name)
    })
}

/// The folder of the Quickshell config in use.
///
/// The one a running Quickshell was started with (`-c name` or `-p path`),
/// else the default `quickshell/shell.qml`, else the only named config,
/// else the default folder. hyprcustard's own bar is never it: that has a
/// target of its own.
fn quickshell_dir(config_home: &Path) -> PathBuf {
    let root = config_home.join("quickshell");
    let bar = crate::bar::dir(config_home);
    // A test must never find, and so write into, a real shell's folder.
    if !cfg!(test) {
        let mut pids = crate::procs::named("qs");
        pids.extend(crate::procs::named("quickshell"));
        for pid in pids {
            let Ok(raw) = std::fs::read(format!("/proc/{pid}/cmdline")) else { continue };
            let args: Vec<String> =
                raw.split(|b| *b == 0).map(|a| String::from_utf8_lossy(a).into_owned()).collect();
            if let Some(dir) = quickshell_arg(&args, &root).filter(|d| *d != bar) {
                return dir;
            }
        }
    }
    if root.join("shell.qml").is_file() {
        return root;
    }
    let named: Vec<PathBuf> = std::fs::read_dir(&root)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("shell.qml").is_file() && *p != bar)
        .collect();
    match named.as_slice() {
        [only] => only.clone(),
        _ => root,
    }
}

/// The config folder a Quickshell command line names, if it names one.
pub(crate) fn quickshell_arg(args: &[String], root: &Path) -> Option<PathBuf> {
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        let (flag, value) = match a.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v.to_string())),
            _ => (a.as_str(), None),
        };
        let mut value = || value.clone().or_else(|| it.next().cloned());
        match flag {
            "-c" | "--config" => return value().map(|n| root.join(n)),
            "-p" | "--path" => {
                let p = PathBuf::from(value()?);
                return Some(if p.is_file() { p.parent()?.to_path_buf() } else { p });
            }
            _ => {}
        }
    }
    None
}

/// Whether any QML file of a shell names the colors singleton.
fn uses_singleton(dir: &Path) -> bool {
    let ty = QS_FILE.trim_end_matches(".qml");
    let mut todo = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = todo.pop() {
        for p in
            std::fs::read_dir(&d).into_iter().flatten().filter_map(|e| e.ok().map(|e| e.path()))
        {
            if p.is_dir() && depth < 4 {
                todo.push((p, depth + 1));
            } else if p.extension().is_some_and(|e| e == "qml")
                && p.file_name().is_some_and(|n| n != QS_FILE)
                && std::fs::read_to_string(&p).is_ok_and(|t| {
                    t.lines().any(|l| !l.trim_start().starts_with("//") && l.contains(ty))
                })
            {
                return true;
            }
        }
    }
    false
}

/// Whether every line of a rofi config naming `file` comes before its last
/// `@theme`, which discards what was defined before it.
fn imports_before_theme(text: &str, file: &Path) -> bool {
    let Some(name) = file.file_name().and_then(|n| n.to_str()) else { return false };
    let lines: Vec<&str> = text.lines().collect();
    let Some(theme) = lines.iter().rposition(|l| l.trim_start().starts_with("@theme")) else {
        return false;
    };
    !lines[theme..].iter().any(|l| {
        let t = l.trim_start();
        !t.starts_with("//") && !t.starts_with("/*") && t.contains(name)
    })
}

/// The colors file's text for a target.
pub fn render(id: &str, scheme: &Scheme, opacity: f64) -> String {
    let named = scheme.named();
    let alpha = |hex: &str| format!("{hex}{:02x}", (opacity.clamp(0.0, 1.0) * 255.0).round() as u8);
    let r = |name: &str| scheme.role(name).to_string();
    let b = |n: usize| scheme.base(n).to_string();
    let terminal = [0x0, 0x8, 0xB, 0xA, 0xD, 0xE, 0xC, 0x5, 0x3, 0x8, 0xB, 0xA, 0xD, 0xE, 0xC, 0x7];
    let header = |c: &str| {
        format!(
            "{c} Colors from hyprcustard: {}.\n\
             {c} Rewritten every time a color scheme is applied, so change the scheme\n\
             {c} in hyprcustard rather than this file.\n",
            scheme.name
        )
    };

    match id {
        "hyprland" => {
            let mut out = header("--");
            out.push_str("hyprcustard_colors = {\n");
            for (k, v) in &named {
                out.push_str(&format!("    {k} = \"{v}\",\n"));
            }
            for k in ALPHA_ROLES {
                out.push_str(&format!("    {k}_alpha = \"{}\",\n", alpha(scheme.role(k))));
            }
            out.push_str("}\n");
            let argb = |hex: &str, a: &str| format!("#{a}{}", hex.trim_start_matches('#'));
            out.push_str(&config_call(&[
                (
                    "general.col.active_border",
                    &LookValue::Gradient {
                        colors: vec![argb(&r("primary"), "ff"), argb(&r("tertiary"), "ff")],
                        angle: 45.0,
                    },
                ),
                (
                    "general.col.inactive_border",
                    &LookValue::Color(argb(&r("outline_variant"), "ff")),
                ),
                ("decoration.shadow.color", &LookValue::Color(argb(&r("shadow"), "ee"))),
            ]));
            out
        }
        "waybar" => {
            let mut out = String::from("/*\n");
            out.push_str(&header(" *"));
            out.push_str(" */\n");
            for (k, v) in &named {
                out.push_str(&format!("@define-color {k} {v};\n"));
            }
            for k in ALPHA_ROLES {
                out.push_str(&format!(
                    "@define-color {k}_alpha alpha({}, {:.2});\n",
                    scheme.role(k),
                    opacity.clamp(0.0, 1.0)
                ));
            }
            out
        }
        "rofi" => {
            let mut out = header("//");
            out.push_str("* {\n");
            for (k, v) in &named {
                out.push_str(&format!("    {}: {v};\n", k.replace('_', "-")));
            }
            for k in ALPHA_ROLES {
                out.push_str(&format!(
                    "    {}-alpha: {};\n",
                    k.replace('_', "-"),
                    alpha(scheme.role(k))
                ));
            }
            out.push_str("}\n");
            out
        }
        "bar" => {
            let mut map = serde_json::Map::new();
            map.insert("scheme".into(), scheme.name.clone().into());
            map.insert("dark".into(), scheme.dark.into());
            for (k, v) in &named {
                map.insert(k.to_string(), v.to_string().into());
            }
            // The other kind too, for the Bar page's light or dark choice.
            let flipped = super::scheme::flipped(scheme);
            let mut other = serde_json::Map::new();
            other.insert("dark".into(), flipped.dark.into());
            for (k, v) in flipped.named() {
                other.insert(k, v.into());
            }
            map.insert("other".into(), other.into());
            format!("{}\n", serde_json::to_string_pretty(&map).unwrap_or_default())
        }
        "quickshell" => {
            let mut out = String::from("pragma Singleton\n\n");
            out.push_str(&header("//"));
            out.push_str("\nimport QtQuick\nimport Quickshell\n\nSingleton {\n");
            for (k, v) in &named {
                out.push_str(&format!("    readonly property color {k}: \"{v}\"\n"));
            }
            // QML reads eight hex digits as #AARRGGBB.
            let a = format!("{:02x}", (opacity.clamp(0.0, 1.0) * 255.0).round() as u8);
            for k in ALPHA_ROLES {
                let hex = scheme.role(k).trim_start_matches('#');
                out.push_str(&format!("    readonly property color {k}_alpha: \"#{a}{hex}\"\n"));
            }
            out.push_str("}\n");
            out
        }
        "dunst" => {
            let mut out = header("#");
            let bg = alpha(&r("surface_container"));
            out.push_str(&format!(
                "[global]\n    frame_color = \"{}\"\n    separator_color = frame\n    highlight = \"{}\"\n\n",
                r("outline_variant"),
                r("primary")
            ));
            for (section, fg, frame) in [
                ("urgency_low", r("on_surface_variant"), r("outline_variant")),
                ("urgency_normal", r("on_surface"), r("primary")),
                ("urgency_critical", r("on_surface"), r("error")),
            ] {
                out.push_str(&format!(
                    "[{section}]\n    background = \"{bg}\"\n    foreground = \"{fg}\"\n    frame_color = \"{frame}\"\n\n"
                ));
            }
            out
        }
        "kitty" => {
            let mut out = header("#");
            for (k, v) in [
                ("foreground", r("on_surface")),
                ("background", r("surface")),
                ("selection_foreground", r("on_secondary_container")),
                ("selection_background", r("secondary_container")),
                ("cursor", r("primary")),
                ("cursor_text_color", r("on_primary")),
                ("url_color", r("tertiary")),
                ("active_border_color", r("primary")),
                ("inactive_border_color", r("outline_variant")),
                ("active_tab_foreground", r("on_primary")),
                ("active_tab_background", r("primary")),
                ("inactive_tab_foreground", r("on_surface_variant")),
                ("inactive_tab_background", r("surface_container")),
                ("tab_bar_background", r("surface_container_lowest")),
            ] {
                out.push_str(&format!("{k} {v}\n"));
            }
            for (i, n) in terminal.iter().enumerate() {
                out.push_str(&format!("color{i} {}\n", b(*n)));
            }
            out
        }
        "ghostty" => {
            let mut out = header("#");
            for (k, v) in [
                ("background", r("surface")),
                ("foreground", r("on_surface")),
                ("cursor-color", r("primary")),
                ("cursor-text", r("on_primary")),
                ("selection-background", r("secondary_container")),
                ("selection-foreground", r("on_secondary_container")),
            ] {
                out.push_str(&format!("{k} = {v}\n"));
            }
            for (i, n) in terminal.iter().enumerate() {
                out.push_str(&format!("palette = {i}={}\n", b(*n)));
            }
            out
        }
        "starship" => starship::render(scheme, &header("#"), &terminal),
        "gtk3" | "gtk4" => toolkits::gtk_css(scheme, id == "gtk4", &header(" *")),
        "qt6ct" => format!("{}{}", header(";"), toolkits::qt_palette(scheme, 22)),
        "qt5ct" => format!("{}{}", header(";"), toolkits::qt_palette(scheme, 21)),
        _ => String::new(),
    }
}

/// Write a target's colors file.
///
/// Kvantum's is a recolored copy of the user's Kvantum theme, so it is
/// skipped when Kvantum is on its built-in theme, which has no files — the
/// page says so.
pub fn write(target: &Target, scheme: &Scheme, opacity: f64) -> Result<()> {
    if let Some(dir) = target.file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    }
    if target.id == "kvantum" {
        let Some(config_home) = target.config.parent().and_then(Path::parent) else {
            return Ok(());
        };
        let desk = Desktop::read(config_home);
        let Some(base) = toolkits::kvantum_base(config_home, &desk) else { return Ok(()) };
        let header = format!(
            " Colors from hyprcustard: {}, over the {} Kvantum theme.\n \
             Rewritten every time a color scheme is applied.",
            scheme.name, base.name
        );
        let dir = target.file.parent().unwrap_or(Path::new("."));
        return toolkits::write_kvantum(dir, &base, scheme, &header);
    }
    let mut text = render(target.id, scheme, opacity);
    // The user's own names handed over to the scheme, defined last so they
    // win over the same names above.
    if let Some(kind) = super::adopt::Kind::of(target.id) {
        let config = std::fs::read_to_string(&target.config).unwrap_or_default();
        let taken = super::adopt::taken(kind, &config);
        text.push_str(&super::adopt::lines_for(kind, &taken, |name| color_of(scheme, name)));
    }
    crate::paths::write_atomic(&target.file, &text)?;
    // Starship cannot load the file, so a config using the palette gets the
    // new copy of it.
    if target.id == "starship" && target.wired {
        let config = std::fs::read_to_string(&target.config).unwrap_or_default();
        let next = starship::splice(&config, &text)?;
        if next != config {
            crate::paths::write_atomic(&target.config, &next)?;
        }
    }
    Ok(())
}

/// A scheme color by the name a colors file gives it: a role, or
/// `base00`..`base0F`.
pub fn color_of(scheme: &Scheme, name: &str) -> String {
    match name.strip_prefix("base").and_then(|n| usize::from_str_radix(n, 16).ok()) {
        Some(n) if n < 16 => scheme.base(n).to_string(),
        _ => scheme.role(name).to_string(),
    }
}

/// Make a target's config load its colors file, backing the config up first.
///
/// Returns the backup's path, if there was a file to back up. Already loading
/// it is not an error; nothing is changed.
pub fn wire(target: &Target, keep: usize) -> Result<Option<PathBuf>> {
    if target.wired {
        return Ok(None);
    }
    if let Some(dir) = target.config.parent() {
        std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    }
    if matches!(target.id, "qt6ct" | "qt5ct" | "kvantum") {
        // Settings rather than a line: set them in their section.
        if target.id == "kvantum" && !target.file.is_file() {
            return Err(Error::other(
                "there is no recolored Kvantum theme yet: apply a scheme first (and choose a \
                 theme in Kvantum Manager if Kvantum is on its built-in one)",
            ));
        }
        let backup = crate::paths::backup(&target.config, keep)?;
        let text = std::fs::read_to_string(&target.config).unwrap_or_default();
        let file = target.file.to_string_lossy();
        let next = if target.id == "kvantum" {
            toolkits::ini_set(&text, "General", &[("theme", KVANTUM_THEME)])
        } else {
            toolkits::ini_set(
                &text,
                "Appearance",
                &[("color_scheme_path", &file), ("custom_palette", "true")],
            )
        };
        crate::paths::write_atomic(&target.config, &next)?;
        return Ok(backup);
    }
    if target.replaces {
        // A whole theme in gtk.css: kept as a backup, and a new gtk.css
        // that only loads the colors takes its place.
        let backup = crate::paths::backup(&target.config, keep.max(1))?;
        crate::paths::write_atomic(&target.config, &format!("{}\n", target.line))?;
        return Ok(backup);
    }
    if target.id == "starship" {
        let Ok(colors) = std::fs::read_to_string(&target.file) else {
            return Err(Error::other(
                "there is no palette to copy into starship.toml yet: apply a scheme first",
            ));
        };
        let text = std::fs::read_to_string(&target.config).unwrap_or_default();
        let next = starship::splice(&text, &colors)?;
        let backup = crate::paths::backup(&target.config, keep)?;
        crate::paths::write_atomic(&target.config, &next)?;
        return Ok(backup);
    }
    if target.id == "quickshell" || target.id == "bar" {
        return Err(Error::other(
            "Quickshell needs no line: use HyprcustardColors in your shell's QML",
        ));
    }
    if target.id == "dunst" {
        // Dunst reads drop-ins only beside the dunstrc it is using, so a user
        // still on the system one gets their own copy of it.
        let system = ["/etc/xdg/dunst/dunstrc", "/etc/dunst/dunstrc"]
            .iter()
            .find_map(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_else(|| {
                "# Your dunst settings. hyprcustard's colors are in dunstrc.d/.\n".into()
            });
        crate::paths::write_atomic(&target.config, &system)?;
        return Ok(None);
    }
    let backup = crate::paths::backup(&target.config, keep)?;
    let text = std::fs::read_to_string(&target.config).unwrap_or_default();
    let wired = if target.id == "rofi" {
        after_theme(&text, &target.line)
    } else if target.place == "at the top of" {
        format!("{}\n{text}", target.line)
    } else {
        let sep = if text.is_empty() || text.ends_with('\n') { "" } else { "\n" };
        format!("{text}{sep}\n{}\n", target.line)
    };
    crate::paths::write_atomic(&target.config, &wired)?;
    Ok(backup)
}

/// `line` put after the last `@theme` line of a rofi config, or at the top
/// when there is none.
fn after_theme(text: &str, line: &str) -> String {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    match lines.iter().rposition(|l| l.trim_start().starts_with("@theme")) {
        None => format!("{line}\n{text}"),
        Some(i) => {
            let mut out: String = lines[..=i].concat();
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(line);
            out.push('\n');
            out.push_str(&lines[i + 1..].concat());
            out
        }
    }
}

/// Tell a running app to pick its new colors up.
///
/// Returns what happened, for the message: `None` when the app was not
/// running or reads its colors fresh each time anyway.
pub fn reload(target: &Target, scheme: &Scheme) -> Option<String> {
    // A test must never signal someone's real waybar or run Lua in their
    // real compositor.
    if cfg!(test) {
        return None;
    }
    match target.id {
        "hyprland" => crate::hyprctl::is_running().then(|| {
            let code = format!("dofile({})", lua_quote(&target.file.to_string_lossy()));
            match crate::look::eval(&code) {
                Ok(()) => "reloaded".to_string(),
                Err(e) => format!("could not reload: {e}"),
            }
        }),
        "waybar" => signal("waybar", SIGUSR2),
        "kitty" => signal("kitty", SIGUSR1),
        "ghostty" => signal("ghostty", SIGUSR2),
        // GTK reads gtk.css once, as an app starts; what can change now is
        // the theme variant and the light or dark preference.
        "gtk3" | "gtk4" => target
            .config
            .parent()
            .and_then(Path::parent)
            .and_then(|home| toolkits::gtk_follow(home, scheme.dark)),
        "dunst" => running("dunst").then(|| {
            let ok = Command::new("dunstctl")
                .arg("reload")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok { "reloaded" } else { "could not reload (dunstctl reload failed)" }.to_string()
        }),
        _ => None,
    }
}

/// The ids of this user's processes running `name`.
fn pids(name: &str) -> Vec<String> {
    crate::procs::named(name).iter().map(u32::to_string).collect()
}

fn running(name: &str) -> bool {
    !pids(name).is_empty()
}

const SIGUSR1: u32 = 10;
const SIGUSR2: u32 = 12;

/// How long a program that has only just started may take to set up its
/// reload handler before it is left to read its colors as it starts.
const HANDLER_GRACE: Duration = Duration::from_secs(5);

/// Send `signo` to every `name`, but only to those that handle it.
///
/// Both signals end a process that has not set up a handler yet. At login
/// the watcher applies within a second of Hyprland starting, while the
/// terminals started alongside it are still coming up — and a signal then
/// closed them. A process this young is waited for briefly; one that still
/// has no handler, or an older one that never had (a version without
/// reload), is left alone.
fn signal(name: &str, signo: u32) -> Option<String> {
    let all = crate::procs::named(name);
    if all.is_empty() {
        return None;
    }
    let deadline = Instant::now() + HANDLER_GRACE;
    let ready = loop {
        let (ready, waiting): (Vec<u32>, Vec<u32>) =
            all.iter().partition(|pid| crate::procs::catches(**pid, signo));
        let young = waiting.iter().any(|pid| {
            crate::procs::alive(*pid) && crate::procs::age(*pid).is_some_and(|a| a < HANDLER_GRACE)
        });
        if !young || Instant::now() >= deadline {
            break ready;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    if ready.is_empty() {
        return Some("still starting; it reads the new colors as it does".into());
    }
    let ok = Command::new("kill")
        .arg(format!("-{signo}"))
        .args(ready.iter().map(u32::to_string))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    Some(if ok { "reloaded".into() } else { format!("could not signal {name}") })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colors::named::builtins;

    fn gruvbox() -> Scheme {
        builtins()[0].scheme()
    }

    fn setup() -> (tempfile::TempDir, Vec<Target>) {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("config");
        let hypr = home.join("hypr").join("hyprland.lua");
        (dir, all(&home, &hypr))
    }

    fn target<'a>(list: &'a [Target], id: &str) -> &'a Target {
        list.iter().find(|t| t.id == id).unwrap()
    }

    #[test]
    fn every_target_renders_every_name_its_format_carries() {
        let s = gruvbox();
        for id in ["hyprland", "waybar", "rofi"] {
            let text = render(id, &s, 0.8);
            for name in ["primary", "on_surface", "base0D", "surface_alpha"] {
                let spelled = if id == "rofi" { name.replace('_', "-") } else { name.to_string() };
                assert!(text.contains(&spelled), "{id} lacks {spelled}");
            }
        }
        assert!(render("waybar", &s, 0.8).contains("@define-color primary #83a598;"));
        assert!(render("rofi", &s, 0.8).contains("    surface-alpha: #282828cc;"));
        assert!(render("dunst", &s, 0.8).contains("background = \"#3c3836cc\""));
        let kitty = render("kitty", &s, 1.0);
        assert!(kitty.contains("color1 #fb4934") && kitty.contains("color15 #fbf1c7"));
        let ghostty = render("ghostty", &s, 1.0);
        assert!(ghostty.contains("palette = 4=#83a598"));
    }

    #[test]
    fn quickshell_gets_a_qml_singleton() {
        let qml = render("quickshell", &gruvbox(), 0.8);
        assert!(qml.starts_with("pragma Singleton\n"));
        assert!(qml.contains("Singleton {\n"));
        assert!(qml.contains("    readonly property color primary: \"#83a598\"\n"));
        assert!(qml.contains("    readonly property color base0D: "));
        // Alpha first, as QML reads it.
        assert!(qml.contains("    readonly property color surface_alpha: \"#cc282828\"\n"));
    }

    #[test]
    fn quickshell_config_folder_is_found() {
        let root = Path::new("/c/quickshell");
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(quickshell_arg(&args(&["qs"]), root), None);
        assert_eq!(quickshell_arg(&args(&["qs", "-c", "bar"]), root), Some(root.join("bar")));
        assert_eq!(quickshell_arg(&args(&["qs", "--config=bar"]), root), Some(root.join("bar")));
        assert_eq!(
            quickshell_arg(&args(&["quickshell", "-d", "-p", "/x/y"]), root),
            Some(PathBuf::from("/x/y"))
        );

        let (dir, _) = setup();
        let home = dir.path().join("config");
        let qs = home.join("quickshell");
        // Nothing there: the default folder.
        assert_eq!(quickshell_dir(&home), qs);
        // One named config: that one.
        std::fs::create_dir_all(qs.join("mine")).unwrap();
        std::fs::write(qs.join("mine").join("shell.qml"), "ShellRoot {}\n").unwrap();
        assert_eq!(quickshell_dir(&home), qs.join("mine"));
        // A default config wins.
        std::fs::write(qs.join("shell.qml"), "ShellRoot {}\n").unwrap();
        assert_eq!(quickshell_dir(&home), qs);
    }

    #[test]
    fn quickshell_is_wired_once_its_qml_uses_the_singleton() {
        let (dir, _) = setup();
        let home = dir.path().join("config");
        let qs = home.join("quickshell");
        std::fs::create_dir_all(qs.join("modules")).unwrap();
        std::fs::write(qs.join("shell.qml"), "ShellRoot {}\n").unwrap();
        std::fs::write(qs.join(QS_FILE), "Singleton {}\n").unwrap();
        std::fs::write(qs.join("modules").join("Bar.qml"), "// HyprcustardColors later\n").unwrap();
        let t =
            || target(&all(&home, &home.join("hypr").join("hyprland.lua")), "quickshell").clone();
        assert!(!t().wired);
        std::fs::write(qs.join("modules").join("Bar.qml"), "color: HyprcustardColors.primary\n")
            .unwrap();
        assert!(t().wired);
        assert!(wire(&t(), 0).is_ok());
        let mut off = t();
        off.wired = false;
        assert!(wire(&off, 0).is_err());
    }

    #[test]
    fn the_hyprland_file_sets_borders_in_the_shape_hyprland_takes() {
        let text = render("hyprland", &gruvbox(), 1.0);
        assert!(text.contains("hyprcustard_colors = {"));
        assert!(
            text.contains("active_border = { colors = { \"rgba(83a598ff)\", \"rgba(d3869bff)\" }, angle = 45 }"),
            "{text}"
        );
        assert!(text.contains("inactive_border = \"rgba(504945ff)\""));
    }

    #[test]
    fn wiring_adds_the_line_once_where_it_belongs() {
        let (_dir, list) = setup();
        let waybar = target(&list, "waybar");
        std::fs::create_dir_all(waybar.config.parent().unwrap()).unwrap();
        std::fs::write(&waybar.config, "* { color: @fg; }\n").unwrap();
        assert!(!all_again(&list, "waybar").wired);
        wire(waybar, 0).unwrap();
        let text = std::fs::read_to_string(&waybar.config).unwrap();
        assert!(text.starts_with("@import \"hyprcustard-colors.css\";\n* {"), "{text}");
        let again = all_again(&list, "waybar");
        assert!(again.wired);
        wire(&again, 0).unwrap();
        assert_eq!(std::fs::read_to_string(&waybar.config).unwrap(), text);

        let kitty = target(&list, "kitty");
        std::fs::create_dir_all(kitty.config.parent().unwrap()).unwrap();
        std::fs::write(&kitty.config, "font_size 11").unwrap();
        wire(kitty, 0).unwrap();
        assert_eq!(
            std::fs::read_to_string(&kitty.config).unwrap(),
            "font_size 11\n\ninclude hyprcustard-colors.conf\n"
        );
    }

    #[test]
    fn rofi_imports_after_its_theme_line() {
        let (_dir, list) = setup();
        let rofi = target(&list, "rofi");
        std::fs::create_dir_all(rofi.config.parent().unwrap()).unwrap();
        let line = "@import \"hyprcustard-colors.rasi\"";
        // No @theme: the top.
        std::fs::write(&rofi.config, "configuration { modi: \"drun\"; }\n").unwrap();
        wire(rofi, 0).unwrap();
        assert!(std::fs::read_to_string(&rofi.config).unwrap().starts_with(line));

        // An import above the @theme, as the first version wrote it, does not
        // count: the theme line discards it.
        let theirs = format!(
            "{line}\nconfiguration {{ modi: \"drun\"; }}\n@theme \"~/.config/rofi/mine.rasi\"\n"
        );
        std::fs::write(&rofi.config, &theirs).unwrap();
        let again = all_again(&list, "rofi");
        assert!(!again.wired);
        wire(&again, 0).unwrap();
        let text = std::fs::read_to_string(&rofi.config).unwrap();
        assert_eq!(text, format!("{theirs}{line}\n"));
        assert!(all_again(&list, "rofi").wired);

        assert_eq!(
            after_theme("@theme \"x\"\nelement { }", line),
            format!("@theme \"x\"\n{line}\nelement {{ }}")
        );
    }

    /// The same target read from disk again.
    fn all_again(list: &[Target], id: &str) -> Target {
        let hypr = &target(list, "hyprland").config;
        let home = hypr.parent().unwrap().parent().unwrap();
        target(&all(home, hypr), id).clone()
    }

    #[test]
    fn a_commented_out_line_does_not_count_as_loading() {
        let (_dir, list) = setup();
        let k = target(&list, "kitty");
        std::fs::create_dir_all(k.config.parent().unwrap()).unwrap();
        std::fs::write(&k.config, "# include hyprcustard-colors.conf\n").unwrap();
        assert!(!all_again(&list, "kitty").wired);
    }

    #[test]
    fn ghostty_uses_whichever_config_name_exists() {
        let (dir, list) = setup();
        assert!(target(&list, "ghostty").config.ends_with("config.ghostty"));
        let g = dir.path().join("config").join("ghostty");
        std::fs::create_dir_all(&g).unwrap();
        std::fs::write(g.join("config"), "theme = x\n").unwrap();
        assert!(all_again(&list, "ghostty").config.ends_with("ghostty/config"));
    }

    #[test]
    fn dunst_is_wired_by_having_a_dunstrc_of_ones_own() {
        let (_dir, list) = setup();
        let d = target(&list, "dunst");
        assert!(!d.wired);
        assert!(d.line.is_empty());
        wire(d, 0).unwrap();
        assert!(d.config.is_file());
        assert!(all_again(&list, "dunst").wired);
    }

    #[test]
    fn written_files_land_beside_their_configs() {
        let (_dir, list) = setup();
        for t in &list {
            write(t, &gruvbox(), 1.0).unwrap();
            // Kvantum's is a copy of a Kvantum theme, and there is none here.
            assert_eq!(t.file.is_file(), t.id != "kvantum", "{}", t.id);
        }
        let h = target(&list, "hyprland");
        assert!(h.line.contains(&*h.file.to_string_lossy()));
        assert!(h.line.starts_with("pcall(dofile, \""));
    }

    #[test]
    fn qt_is_wired_by_pointing_its_settings_at_the_file() {
        let (_dir, list) = setup();
        let q = target(&list, "qt6ct");
        std::fs::create_dir_all(q.config.parent().unwrap()).unwrap();
        std::fs::write(&q.config, "[Appearance]\ncolor_scheme_path=/usr/share/qt6ct/colors/darker.conf\ncustom_palette=false\nstyle=Fusion\n").unwrap();
        let q = all_again(&list, "qt6ct");
        assert!(!q.wired);
        write(&q, &gruvbox(), 1.0).unwrap();
        wire(&q, 0).unwrap();
        let text = std::fs::read_to_string(&q.config).unwrap();
        assert!(text.contains(&format!("color_scheme_path={}\n", q.file.display())), "{text}");
        assert!(text.contains("custom_palette=true\nstyle=Fusion\n"), "{text}");
        assert!(all_again(&list, "qt6ct").wired);
        assert!(!all_again(&list, "qt5ct").wired);
    }

    #[test]
    fn kvantum_is_wired_only_once_its_theme_exists() {
        let (_dir, list) = setup();
        let k = target(&list, "kvantum");
        assert!(wire(k, 0).is_err(), "nothing to point Kvantum at yet");
        let theme = k.config.parent().unwrap().join("Mine");
        std::fs::create_dir_all(&theme).unwrap();
        std::fs::write(
            theme.join("Mine.kvconfig"),
            "[GeneralColors]\nwindow.color=#202020\ntext.color=#e0e0e0\n",
        )
        .unwrap();
        std::fs::write(theme.join("Mine.svg"), "<svg/>").unwrap();
        std::fs::write(&k.config, "[General]\ntheme=Mine\n").unwrap();
        let k = all_again(&list, "kvantum");
        assert!(k.usage.contains("Mine"));
        write(&k, &gruvbox(), 1.0).unwrap();
        assert!(k.file.is_file());
        wire(&k, 0).unwrap();
        assert_eq!(std::fs::read_to_string(&k.config).unwrap(), "[General]\ntheme=HyprCustard\n");
        let k = all_again(&list, "kvantum");
        assert!(k.wired);
        // Applying again still copies Mine, not the copy.
        write(&k, &gruvbox(), 1.0).unwrap();
        assert_eq!(std::fs::read_to_string(k.file.with_file_name("base")).unwrap(), "Mine\n");
    }

    #[test]
    fn a_gtk_css_that_is_a_whole_theme_is_set_aside_rather_than_added_to() {
        let (_dir, list) = setup();
        let g = target(&list, "gtk4");
        assert!(!g.replaces);
        std::fs::create_dir_all(g.config.parent().unwrap()).unwrap();
        let theme: String = (0..120).map(|i| format!(".c{i} {{ color: #{i:06x}; }}\n")).collect();
        std::fs::write(&g.config, &theme).unwrap();
        let g = all_again(&list, "gtk4");
        assert!(g.replaces && !g.wired);
        assert!(g.warning.contains("whole theme"));
        let backup = wire(&g, 0).unwrap().expect("the theme is backed up even when keeping none");
        assert_eq!(std::fs::read_to_string(backup).unwrap(), theme);
        assert_eq!(
            std::fs::read_to_string(&g.config).unwrap(),
            "@import \"hyprcustard-colors.css\";\n"
        );
        let g = all_again(&list, "gtk4");
        assert!(g.wired && !g.replaces);

        // A few tweaks get the line at the end, after them.
        let g3 = target(&list, "gtk3");
        std::fs::create_dir_all(g3.config.parent().unwrap()).unwrap();
        std::fs::write(&g3.config, "label { font-weight: bold; }\n").unwrap();
        wire(&all_again(&list, "gtk3"), 0).unwrap();
        assert_eq!(
            std::fs::read_to_string(&g3.config).unwrap(),
            "label { font-weight: bold; }\n\n@import \"hyprcustard-colors.css\";\n"
        );
    }

    #[test]
    fn the_bar_gets_the_other_kind_of_scheme_too() {
        let gruvbox = crate::colors::named::builtins()
            .into_iter()
            .find(|n| n.id == "gruvbox-dark")
            .unwrap()
            .scheme();
        let json: serde_json::Value = serde_json::from_str(&render("bar", &gruvbox, 1.0)).unwrap();
        assert_eq!(json["dark"], true);
        assert_eq!(json["background"], "#282828");
        assert_eq!(json["other"]["dark"], false);
        let bg =
            crate::colors::scheme::parse(json["other"]["background"].as_str().unwrap()).unwrap();
        assert!(crate::colors::scheme::luminance(bg) > 0.5, "the other one is light");
        assert!(json["other"]["base0B"].as_str().unwrap().starts_with('#'));
    }
}
