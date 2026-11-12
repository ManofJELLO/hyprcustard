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

use super::scheme::Scheme;
use crate::error::{Error, Result};
use crate::look::{config_call, LookValue};
use crate::model::lua_quote;

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
    pub usage: &'static str,
}

/// Every target, read from the disk as it is now.
///
/// `config_home` is `$XDG_CONFIG_HOME`; `hypr_config` the Hyprland config
/// the app edits, which need not be the default one.
pub fn all(config_home: &Path, hypr_config: &Path) -> Vec<Target> {
    let installed = |p: &str| crate::paths::which(p).is_some();
    let hypr_file = hypr_config.with_file_name("hyprcustard-colors.lua");
    let ghostty_dir = config_home.join("ghostty");
    let ghostty_config = ["config.ghostty", "config"]
        .iter()
        .map(|n| ghostty_dir.join(n))
        .find(|p| p.is_file())
        .unwrap_or_else(|| ghostty_dir.join("config.ghostty"));
    let dunst_base = config_home.join("dunst").join("dunstrc");

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
                    hyprcustard_colors table, for your own settings below that line.",
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
                    style defines itself keeps your value.",
        },
        Target {
            id: "rofi",
            name: "Rofi",
            installed: installed("rofi"),
            config: config_home.join("rofi").join("config.rasi"),
            file: config_home.join("rofi").join("hyprcustard-colors.rasi"),
            line: "@import \"hyprcustard-colors.rasi\"".into(),
            place: "at the top of",
            wired: false,
            usage: "Use the names in your theme: @primary, @on-surface, @base0D \
                    (rofi spells them with dashes). A color your theme defines itself \
                    keeps your value.",
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
                    folder when you have your own dunstrc.",
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
            usage: "Sets the terminal palette, cursor, selection and tabs.",
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
            usage: "Sets the terminal palette, cursor and selection, over any theme.",
        },
    ];
    for t in &mut list {
        if !t.line.is_empty() {
            t.wired = loads(&t.config, &t.file);
        }
    }
    list
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
        _ => String::new(),
    }
}

/// Write a target's colors file.
pub fn write(target: &Target, scheme: &Scheme, opacity: f64) -> Result<()> {
    if let Some(dir) = target.file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    }
    crate::paths::write_atomic(&target.file, &render(target.id, scheme, opacity))
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
    let wired = if target.place == "at the top of" {
        format!("{}\n{text}", target.line)
    } else {
        let sep = if text.is_empty() || text.ends_with('\n') { "" } else { "\n" };
        format!("{text}{sep}\n{}\n", target.line)
    };
    crate::paths::write_atomic(&target.config, &wired)?;
    Ok(backup)
}

/// Tell a running app to pick its new colors up.
///
/// Returns what happened, for the message: `None` when the app was not
/// running or reads its colors fresh each time anyway.
pub fn reload(target: &Target) -> Option<String> {
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
        "waybar" => signal("waybar", "USR2"),
        "kitty" => signal("kitty", "USR1"),
        "ghostty" => signal("ghostty", "USR2"),
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
    let Ok(rd) = std::fs::read_dir("/proc") else { return Vec::new() };
    rd.filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_str().is_some_and(|s| s.chars().all(|c| c.is_ascii_digit())))
        .filter(|e| std::fs::read_to_string(e.path().join("comm")).is_ok_and(|c| c.trim() == name))
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .collect()
}

fn running(name: &str) -> bool {
    !pids(name).is_empty()
}

fn signal(name: &str, sig: &str) -> Option<String> {
    let pids = pids(name);
    if pids.is_empty() {
        return None;
    }
    let ok = Command::new("kill")
        .arg("-s")
        .arg(sig)
        .args(&pids)
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
            assert!(t.file.is_file(), "{}", t.id);
        }
        let h = target(&list, "hyprland");
        assert!(h.line.contains(&*h.file.to_string_lossy()));
        assert!(h.line.starts_with("pcall(dofile, \""));
    }
}
