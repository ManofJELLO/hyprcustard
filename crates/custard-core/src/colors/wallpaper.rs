//! Finding out what is on the desktop's background right now.
//!
//! There is no one place to ask: every wallpaper program keeps its own
//! state. Each supported one gets a small reader — its command line, its
//! IPC, or its config file — and everything is read rather than assumed, so
//! the same code serves whichever program and whichever monitors a user has.
//!
//! Supported: mpvpaper, swaybg, hyprpaper, swww and awww, wpaperd, and
//! waypaper's config as a last resort (waypaper itself is a front end that
//! runs one of the others). Anything else drawing a background is named, so
//! the page can say what it cannot read instead of guessing.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::extract::is_media;

/// One wallpaper a program is showing.
#[derive(Debug, Clone, PartialEq)]
pub struct Wallpaper {
    /// The program showing it.
    pub program: String,
    /// The monitor it is on, or `None` for every monitor.
    pub monitor: Option<String>,
    /// What the program was given: a file, or a folder it plays from.
    pub path: PathBuf,
    /// The file on screen now, when the program can tell and it differs from
    /// `path` — the current clip of a folder or playlist.
    pub showing: Option<PathBuf>,
}

impl Wallpaper {
    /// The one file to read colors from, if there is one.
    pub fn file(&self) -> Option<&Path> {
        self.showing.as_deref().or_else(|| self.path.is_file().then_some(self.path.as_path()))
    }

    /// The folder being played from, when `path` is one.
    pub fn folder(&self) -> Option<&Path> {
        self.path.is_dir().then_some(self.path.as_path())
    }
}

/// Everything found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Found {
    /// What is being shown, per program and monitor.
    pub wallpapers: Vec<Wallpaper>,
    /// Background surfaces from programs this cannot read, by name.
    pub unreadable: Vec<String>,
    /// The monitors, in Hyprland's order.
    pub monitors: Vec<String>,
}

impl Found {
    /// The wallpaper for `monitor` — or, with no monitor named or none
    /// matching, for the first monitor that has one.
    pub fn for_monitor(&self, monitor: &str) -> Option<&Wallpaper> {
        let on = |m: &str| {
            self.wallpapers
                .iter()
                .find(|w| w.monitor.as_deref() == Some(m))
                .or_else(|| self.wallpapers.iter().find(|w| w.monitor.is_none()))
        };
        if !monitor.is_empty() {
            if let Some(w) = on(monitor) {
                return Some(w);
            }
        }
        self.monitors.iter().find_map(|m| on(m)).or_else(|| self.wallpapers.first())
    }
}

/// Look at the running session.
pub fn detect() -> Found {
    let mut found = Found {
        monitors: crate::hyprctl::monitors()
            .map(|ms| ms.into_iter().map(|m| m.name).collect())
            .unwrap_or_default(),
        ..Found::default()
    };
    // Only this session's programs: the shader preview runs a swaybg inside
    // its own nested Hyprland, and that is not the desktop's wallpaper.
    let ours = std::env::var("WAYLAND_DISPLAY").ok();
    let procs: Vec<(u32, Vec<String>)> = processes()
        .into_iter()
        .filter(|(pid, _)| same_display(ours.as_deref(), display_of(*pid).as_deref()))
        .collect();
    let running = |name: &str| procs.iter().any(|(_, argv)| program(argv) == name);

    for (pid, argv) in &procs {
        match program(argv) {
            "mpvpaper" => {
                if let Some(mut w) = mpvpaper_args(argv) {
                    w.showing = open_media(*pid, &w.path);
                    found.wallpapers.push(w);
                }
            }
            "swaybg" => found.wallpapers.extend(swaybg_args(argv)),
            _ => {}
        }
    }
    if running("hyprpaper") {
        let active = run(&["hyprctl", "hyprpaper", "listactive"]).map(|t| hyprpaper_active(&t));
        match active {
            Some(list) if !list.is_empty() => found.wallpapers.extend(list),
            _ => found.wallpapers.extend(hyprpaper_conf(
                &std::fs::read_to_string(crate::paths::hypr_dir().join("hyprpaper.conf"))
                    .unwrap_or_default(),
            )),
        }
    }
    for daemon in ["awww", "swww"] {
        if running(&format!("{daemon}-daemon")) {
            if let Some(text) = run(&[daemon, "query"]) {
                found.wallpapers.extend(awww_query(daemon, &text));
            }
        }
    }
    if running("wpaperd") {
        if let Some(text) = run(&["wpaperctl", "all-wallpapers"]) {
            found.wallpapers.extend(wpaperd_list(&text));
        }
    }
    if found.wallpapers.is_empty() {
        let ini = crate::paths::config_home().join("waypaper").join("config.ini");
        if let Some(w) = std::fs::read_to_string(ini).ok().and_then(|t| waypaper_ini(&t)) {
            found.wallpapers.push(w);
        }
    }

    let known = |ns: &str| {
        found.wallpapers.iter().any(|w| namespace_program(ns) == Some(w.program.as_str()))
    };
    found.unreadable = background_namespaces().into_iter().filter(|ns| !known(ns)).collect();
    found
}

/// Which program a background layer belongs to, by its namespace.
fn namespace_program(ns: &str) -> Option<&'static str> {
    let ns = ns.to_ascii_lowercase();
    if ns == "mpvpaper" {
        Some("mpvpaper")
    } else if ns == "hyprpaper" {
        Some("hyprpaper")
    } else if ns == "wallpaper" {
        Some("swaybg")
    } else if ns.contains("awww") {
        Some("awww")
    } else if ns.contains("swww") {
        Some("swww")
    } else if ns.contains("wpaperd") {
        Some("wpaperd")
    } else {
        None
    }
}

/// Namespaces of the layers on the background level, on any monitor.
fn background_namespaces() -> Vec<String> {
    let Some(json) = run(&["hyprctl", "-j", "layers"]) else { return Vec::new() };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) else { return Vec::new() };
    let mut out: Vec<String> = v
        .as_object()
        .into_iter()
        .flat_map(|m| m.values())
        .filter_map(|mon| mon["levels"]["0"].as_array())
        .flatten()
        .filter_map(|l| l["namespace"].as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Every process's command line.
fn processes() -> Vec<(u32, Vec<String>)> {
    let Ok(rd) = std::fs::read_dir("/proc") else { return Vec::new() };
    rd.filter_map(|e| e.ok())
        .filter_map(|e| {
            let pid: u32 = e.file_name().to_str()?.parse().ok()?;
            let raw = std::fs::read(e.path().join("cmdline")).ok()?;
            let argv: Vec<String> = raw
                .split(|b| *b == 0)
                .filter(|a| !a.is_empty())
                .map(|a| String::from_utf8_lossy(a).into_owned())
                .collect();
            (!argv.is_empty()).then_some((pid, argv))
        })
        .collect()
}

/// A process's `WAYLAND_DISPLAY`, if it has one and it can be read.
fn display_of(pid: u32) -> Option<String> {
    let env = std::fs::read(format!("/proc/{pid}/environ")).ok()?;
    env.split(|b| *b == 0).find_map(|kv| {
        let kv = String::from_utf8_lossy(kv);
        kv.strip_prefix("WAYLAND_DISPLAY=").map(str::to_string)
    })
}

/// Whether a process belongs to this session's compositor. Anything that
/// cannot be told apart is kept.
fn same_display(ours: Option<&str>, theirs: Option<&str>) -> bool {
    match (ours, theirs) {
        (Some(a), Some(b)) => a == b,
        _ => true,
    }
}

fn program(argv: &[String]) -> &str {
    argv.first().map(|a| a.rsplit('/').next().unwrap_or(a)).unwrap_or("")
}

fn run(args: &[&str]) -> Option<String> {
    let out = Command::new(args[0])
        .args(&args[1..])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The media file a process has open, preferring one inside `path` when that
/// is a folder. This is how the current clip of an mpvpaper playlist is
/// known: mpv keeps the file it is playing open.
fn open_media(pid: u32, path: &Path) -> Option<PathBuf> {
    if path.is_file() && is_media(path) {
        return None;
    }
    let rd = std::fs::read_dir(format!("/proc/{pid}/fd")).ok()?;
    let open: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| std::fs::read_link(e.path()).ok())
        .filter(|p| is_media(p) && p.is_file())
        .collect();
    open.iter().find(|p| p.starts_with(path)).or(open.first()).cloned()
}

// ---------------------------------------------------------------------------
// Readers, one per program. Each takes text, so each is tested on its own.
// ---------------------------------------------------------------------------

/// `mpvpaper [options] <output> <path>`.
pub fn mpvpaper_args(argv: &[String]) -> Option<Wallpaper> {
    let takes_value = |c: char| matches!(c, 'a' | 'n' | 'l' | 'o');
    let long_value = ["--auto-mode", "--slideshow", "--layer", "--mpv-options"];
    let mut positional = Vec::new();
    let mut i = 1;
    while i < argv.len() {
        let a = &argv[i];
        if let Some(long) = a.strip_prefix("--") {
            if !long.contains('=') && long_value.contains(&a.as_str()) {
                i += 1;
            }
        } else if a.len() > 1 && a.starts_with('-') {
            // Short flags may be bundled: `-vs`, `-pa FULL`.
            if a.chars().last().map(takes_value).unwrap_or(false) {
                i += 1;
            }
        } else {
            positional.push(a.clone());
        }
        i += 1;
    }
    let [output, path, ..] = positional.as_slice() else { return None };
    Some(Wallpaper {
        program: "mpvpaper".into(),
        monitor: all_or(output, &["ALL", "*"]),
        path: crate::paths::expand(path),
        showing: None,
    })
}

/// `swaybg -o <output> -i <image> ...`: each `-i` belongs to the latest `-o`.
pub fn swaybg_args(argv: &[String]) -> Vec<Wallpaper> {
    let mut out = Vec::new();
    let mut output: Option<String> = None;
    let mut it = argv.iter().skip(1);
    while let Some(a) = it.next() {
        let (flag, inline) = match a.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v.to_string())),
            _ => (a.as_str(), None),
        };
        let mut value = || inline.clone().or_else(|| it.next().cloned());
        match flag {
            "-o" | "--output" => output = value().and_then(|o| all_or(&o, &["*"])),
            "-i" | "--image" => {
                if let Some(p) = value() {
                    out.push(Wallpaper {
                        program: "swaybg".into(),
                        monitor: output.clone(),
                        path: crate::paths::expand(&p),
                        showing: None,
                    });
                }
            }
            "-c" | "--color" | "-m" | "--mode" => {
                value();
            }
            _ => {}
        }
    }
    out
}

/// `hyprctl hyprpaper listactive`: `MONITOR = /path`, one per line.
pub fn hyprpaper_active(text: &str) -> Vec<Wallpaper> {
    text.lines()
        .filter_map(|l| l.split_once('='))
        .map(|(m, p)| (m.trim(), p.trim()))
        .filter(|(_, p)| !p.is_empty())
        .map(|(m, p)| Wallpaper {
            program: "hyprpaper".into(),
            monitor: all_or(m, &[""]),
            path: crate::paths::expand(p),
            showing: None,
        })
        .collect()
}

/// `hyprpaper.conf`, in both its shapes: `wallpaper = MONITOR,path` and the
/// newer `wallpaper { monitor = ... path = ... }` block.
pub fn hyprpaper_conf(text: &str) -> Vec<Wallpaper> {
    let mut out = Vec::new();
    let mut block: Option<(String, String)> = None;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if let Some((m, p)) = block.as_mut() {
            if line.starts_with('}') {
                if !p.is_empty() {
                    out.push(Wallpaper {
                        program: "hyprpaper".into(),
                        monitor: all_or(m, &[""]),
                        path: crate::paths::expand(p),
                        showing: None,
                    });
                }
                block = None;
            } else if let Some((k, v)) = line.split_once('=') {
                match k.trim() {
                    "monitor" => *m = v.trim().to_string(),
                    "path" => *p = v.trim().to_string(),
                    _ => {}
                }
            }
            continue;
        }
        if line.starts_with("wallpaper") && line.ends_with('{') {
            block = Some((String::new(), String::new()));
        } else if let Some((k, v)) = line.split_once('=') {
            if k.trim() == "wallpaper" {
                if let Some((m, p)) = v.split_once(',') {
                    out.push(Wallpaper {
                        program: "hyprpaper".into(),
                        monitor: all_or(m.trim(), &[""]),
                        path: crate::paths::expand(p.trim()),
                        showing: None,
                    });
                }
            }
        }
    }
    out
}

/// `swww query` / `awww query`: one line per monitor, ending in
/// `currently displaying: image: /path` (or `color: ...`, which is skipped).
pub fn awww_query(program: &str, text: &str) -> Vec<Wallpaper> {
    text.lines()
        .filter_map(|l| {
            let (head, image) = l.split_once("currently displaying: image:")?;
            let monitor = head.trim_start_matches(|c: char| c == ':' || c.is_whitespace());
            let monitor = monitor.split(':').next()?.trim();
            Some(Wallpaper {
                program: program.into(),
                monitor: all_or(monitor, &[""]),
                path: PathBuf::from(image.trim()),
                showing: None,
            })
        })
        .collect()
}

/// `wpaperctl all-wallpapers`: `MONITOR: /path`, one per line.
pub fn wpaperd_list(text: &str) -> Vec<Wallpaper> {
    text.lines()
        .filter_map(|l| l.split_once(':'))
        .map(|(m, p)| (m.trim(), p.trim()))
        .filter(|(m, p)| !m.is_empty() && p.starts_with('/'))
        .map(|(m, p)| Wallpaper {
            program: "wpaperd".into(),
            monitor: Some(m.to_string()),
            path: PathBuf::from(p),
            showing: None,
        })
        .collect()
}

/// waypaper's `config.ini`: its `wallpaper` and `monitors` keys.
pub fn waypaper_ini(text: &str) -> Option<Wallpaper> {
    let get = |key: &str| {
        text.lines().find_map(|l| {
            let (k, v) = l.split_once('=')?;
            (k.trim() == key).then(|| v.trim().to_string())
        })
    };
    let path = get("wallpaper").filter(|p| !p.is_empty())?;
    let monitor = get("monitors").and_then(|m| all_or(&m, &["All", "all", ""]));
    Some(Wallpaper {
        program: "waypaper".into(),
        monitor,
        path: crate::paths::expand(&path),
        showing: None,
    })
}

/// `None` when `name` is one of the ways of saying "every monitor".
fn all_or(name: &str, all: &[&str]) -> Option<String> {
    let name = name.trim();
    (!all.contains(&name)).then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(s: &[&str]) -> Vec<String> {
        s.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn mpvpaper_skips_option_values_and_bundled_flags() {
        let w = mpvpaper_args(&argv(&[
            "mpvpaper",
            "-p",
            "-a",
            "FULL",
            "-o",
            "no-audio --loop-file shuffle",
            "ALL",
            "/w/clips/",
        ]))
        .unwrap();
        assert_eq!(w.monitor, None);
        assert_eq!(w.path, PathBuf::from("/w/clips/"));

        let w = mpvpaper_args(&argv(&["/usr/bin/mpvpaper", "-vs", "-n", "60", "DP-2", "/a.mp4"]))
            .unwrap();
        assert_eq!(w.monitor.as_deref(), Some("DP-2"));
        assert_eq!(w.path, PathBuf::from("/a.mp4"));

        let w = mpvpaper_args(&argv(&["mpvpaper", "--mpv-options", "loop", "HDMI-A-1", "/b.mkv"]))
            .unwrap();
        assert_eq!(w.monitor.as_deref(), Some("HDMI-A-1"));
        assert!(mpvpaper_args(&argv(&["mpvpaper", "-d"])).is_none());
    }

    #[test]
    fn swaybg_pairs_images_with_outputs() {
        let ws = swaybg_args(&argv(&[
            "swaybg",
            "-o",
            "DP-1",
            "-i",
            "/a.png",
            "-m",
            "fill",
            "--output=HDMI-A-1",
            "--image",
            "/b.jpg",
            "-o",
            "*",
            "-c",
            "000000",
        ]));
        assert_eq!(ws.len(), 2);
        assert_eq!(ws[0].monitor.as_deref(), Some("DP-1"));
        assert_eq!(ws[1].monitor.as_deref(), Some("HDMI-A-1"));
        assert_eq!(ws[1].path, PathBuf::from("/b.jpg"));
        let all = swaybg_args(&argv(&["swaybg", "-i", "/c.png"]));
        assert_eq!(all[0].monitor, None);
    }

    #[test]
    fn hyprpaper_ipc_and_both_config_shapes() {
        let ws = hyprpaper_active("DP-1 = /p/a.png\n = /p/all.png\n");
        assert_eq!(ws[0].monitor.as_deref(), Some("DP-1"));
        assert_eq!(ws[1].monitor, None);

        let old =
            hyprpaper_conf("preload = /x.png\nwallpaper = DP-2,/x.png\nwallpaper = ,/y.png\n");
        assert_eq!(old.len(), 2);
        assert_eq!(old[0].monitor.as_deref(), Some("DP-2"));
        assert_eq!(old[1].monitor, None);

        let new = hyprpaper_conf(
            "preload = ~/p.jpg\nwallpaper {\n    monitor = \n    path = /p.jpg\n    }\n\
             #wallpaper = ,/commented.jpg\nwallpaper {\n monitor = DP-3\n path = /q.jpg # note\n}\n",
        );
        assert_eq!(new.len(), 2, "{new:?}");
        assert_eq!(new[0].monitor, None);
        assert_eq!(new[0].path, PathBuf::from("/p.jpg"));
        assert_eq!(new[1].monitor.as_deref(), Some("DP-3"));
    }

    #[test]
    fn awww_lines_with_and_without_a_leading_colon() {
        let text = "DP-1: 2560x1440, scale: 1, currently displaying: image: /w/a.png\n\
                    : HDMI-A-1: 1920x1080, scale: 1, currently displaying: image: /w/b c.gif\n\
                    eDP-1: 1920x1080, scale: 1, currently displaying: color: 000000\n";
        let ws = awww_query("awww", text);
        assert_eq!(ws.len(), 2);
        assert_eq!(ws[0].monitor.as_deref(), Some("DP-1"));
        assert_eq!(ws[1].monitor.as_deref(), Some("HDMI-A-1"));
        assert_eq!(ws[1].path, PathBuf::from("/w/b c.gif"));
    }

    #[test]
    fn wpaperd_and_waypaper() {
        let ws = wpaperd_list("DP-1: /w/a.png\nnot a line\n");
        assert_eq!(ws.len(), 1);
        let w =
            waypaper_ini("[Settings]\nbackend = swaybg\nmonitors = All\nwallpaper = /w/z.png\n")
                .unwrap();
        assert_eq!(w.monitor, None);
        assert_eq!(w.path, PathBuf::from("/w/z.png"));
        assert!(waypaper_ini("[Settings]\nwallpaper = \n").is_none());
    }

    #[test]
    fn choosing_a_monitor() {
        let w = |m: Option<&str>, p: &str| Wallpaper {
            program: "x".into(),
            monitor: m.map(str::to_string),
            path: PathBuf::from(p),
            showing: None,
        };
        let found = Found {
            wallpapers: vec![w(Some("HDMI-A-1"), "/h"), w(Some("DP-1"), "/d")],
            unreadable: vec![],
            monitors: vec!["DP-1".into(), "HDMI-A-1".into()],
        };
        assert_eq!(found.for_monitor("HDMI-A-1").unwrap().path, PathBuf::from("/h"));
        // Unset or unknown: the first monitor in Hyprland's order.
        assert_eq!(found.for_monitor("").unwrap().path, PathBuf::from("/d"));
        assert_eq!(found.for_monitor("gone").unwrap().path, PathBuf::from("/d"));

        let everywhere = Found { wallpapers: vec![w(None, "/all")], ..found.clone() };
        assert_eq!(everywhere.for_monitor("DP-1").unwrap().path, PathBuf::from("/all"));
        assert!(Found::default().for_monitor("").is_none());
    }

    #[test]
    fn programs_from_another_compositor_are_left_out() {
        assert!(same_display(Some("wayland-1"), Some("wayland-1")));
        assert!(!same_display(Some("wayland-1"), Some("wayland-2")));
        assert!(same_display(None, Some("wayland-2")), "nothing to compare with");
        assert!(same_display(Some("wayland-1"), None), "unreadable is kept");
    }
}
