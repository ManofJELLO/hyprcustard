//! hyprcustard's bar: a Quickshell config that ships with the app.
//!
//! Setting it up writes the bar's QML to `~/.config/quickshell/hyprcustard`,
//! which Quickshell runs as `qs -p <that folder>`, beside any config of the
//! user's own. The folder is the app's: it is rewritten when the app updates,
//! and a list of what was written ([`STAMP`]) lets removing it delete exactly
//! those files. The bar reads its settings from `bar.json` there and follows
//! a change at once. Its colors come from the Colors page, which writes them
//! beside it as `colors.json` like any other target; the bar watches that
//! file too, rather than leaving it to Quickshell's reload of the whole
//! config, which rebuilds the bar's windows and did not always fire.
//!
//! Quickshell itself is the user's to install: the page says which package,
//! and never runs a package manager.
//!
//! Starting it at login is a managed block in the Hyprland config, like the
//! wallpaper's: other bars' start lines can be handed over (commented out,
//! marked) and given back exactly.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::block::{self, Markers};
use crate::colors::Scheme;
use crate::error::{Error, Result};
use crate::model::lua_quote;
use crate::paths;
use crate::walls::startup::{has_word, lines_outside_blocks, movable, split_keep};

/// The Quickshell config's name: its folder under `quickshell/`.
pub const CONFIG_NAME: &str = "hyprcustard";

/// The bar's settings file, beside its QML.
pub const SETTINGS_FILE: &str = "bar.json";

/// The colors the Colors page writes for the bar.
pub const COLORS_FILE: &str = "colors.json";

/// What marks the folder as the app's: the version that wrote it, then each
/// file written, one per line.
pub const STAMP: &str = ".hyprcustard-bar";

/// The bar's QML and its shape shader, as shipped.
pub const FILES: &[(&str, &[u8])] = &[
    ("shell.qml", include_str!("../bar/shell.qml").as_bytes()),
    ("Config.qml", include_str!("../bar/Config.qml").as_bytes()),
    ("Theme.qml", include_str!("../bar/Theme.qml").as_bytes()),
    ("Bar.qml", include_str!("../bar/Bar.qml").as_bytes()),
    ("Module.qml", include_str!("../bar/Module.qml").as_bytes()),
    ("Tooltip.qml", include_str!("../bar/Tooltip.qml").as_bytes()),
    ("ClockModule.qml", include_str!("../bar/ClockModule.qml").as_bytes()),
    ("WorkspacesModule.qml", include_str!("../bar/WorkspacesModule.qml").as_bytes()),
    ("WindowModule.qml", include_str!("../bar/WindowModule.qml").as_bytes()),
    ("MediaModule.qml", include_str!("../bar/MediaModule.qml").as_bytes()),
    ("TrayModule.qml", include_str!("../bar/TrayModule.qml").as_bytes()),
    ("TrayMenu.qml", include_str!("../bar/TrayMenu.qml").as_bytes()),
    ("VolumeModule.qml", include_str!("../bar/VolumeModule.qml").as_bytes()),
    ("BluetoothModule.qml", include_str!("../bar/BluetoothModule.qml").as_bytes()),
    ("NetworkModule.qml", include_str!("../bar/NetworkModule.qml").as_bytes()),
    ("CpuModule.qml", include_str!("../bar/CpuModule.qml").as_bytes()),
    ("MemoryModule.qml", include_str!("../bar/MemoryModule.qml").as_bytes()),
    ("TempModule.qml", include_str!("../bar/TempModule.qml").as_bytes()),
    ("temps.sh", include_str!("../bar/temps.sh").as_bytes()),
    ("LauncherModule.qml", include_str!("../bar/LauncherModule.qml").as_bytes()),
    ("PowerModule.qml", include_str!("../bar/PowerModule.qml").as_bytes()),
    ("Neu.qml", include_str!("../bar/Neu.qml").as_bytes()),
    ("Panel.qml", include_str!("../bar/Panel.qml").as_bytes()),
    ("Launcher.qml", include_str!("../bar/Launcher.qml").as_bytes()),
    ("PowerMenu.qml", include_str!("../bar/PowerMenu.qml").as_bytes()),
    ("neu.frag.qsb", include_bytes!("../bar/neu.frag.qsb")),
];

/// The modules, in the order they sit on the bar, with what the page calls
/// them.
pub const MODULES: &[(&str, &str)] = &[
    ("launcher", "Launcher"),
    ("clock", "Clock"),
    ("workspaces", "Workspaces"),
    ("window", "Window title"),
    ("media", "Media"),
    ("tray", "Tray"),
    ("volume", "Volume"),
    ("bluetooth", "Bluetooth"),
    ("network", "Network"),
    ("cpu", "CPU"),
    ("memory", "Memory"),
    ("cpu_temp", "CPU temperature"),
    ("gpu_temp", "GPU temperature"),
    ("power", "Power menu"),
];

/// Mixers the volume module can open, best first.
const MIXERS: &[&str] = &["pwvucontrol", "pavucontrol", "pavucontrol-qt", "hyprpwcenter"];

/// Bluetooth apps with a window of their own, best first.
const BLUETOOTH_APPS: &[&str] = &["blueman-manager", "overskride", "blueberry"];

/// Screen lockers, best first, with the command that locks at once.
const LOCKERS: &[(&str, &str)] =
    &[("hyprlock", "hyprlock"), ("swaylock", "swaylock -f"), ("gtklock", "gtklock -d")];

/// The power menu's commands when neither the page nor the machine names
/// one. Log out tries a Lua config's dispatcher first, then a .conf one's.
pub const POWER_DEFAULTS: &[(&str, &str)] = &[
    ("lock", "loginctl lock-session"),
    ("logout", "hyprctl dispatch 'hl.dsp.exit()' || hyprctl dispatch exit"),
    ("suspend", "systemctl suspend"),
    ("reboot", "systemctl reboot"),
    ("shutdown", "systemctl poweroff"),
];

/// Bluetooth apps that run in a terminal.
const BLUETOOTH_TUIS: &[&str] = &["bluetui", "bluetuith"];

/// The Bar page's choices, kept in the app's settings and written to
/// [`SETTINGS_FILE`] for the bar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BarSettings {
    /// Monitors to show it on, by name. Empty is every monitor.
    pub monitors: Vec<String>,
    /// `top` or `bottom`.
    pub position: String,
    /// The bar's own height, in pixels.
    pub height: u32,
    /// Space between the bar and the screen's edge.
    pub edge_margin: u32,
    /// Space between the bar and the screen's sides.
    pub side_margin: u32,
    /// Space the bar keeps clear on its window side, past its box: windows
    /// start that much further away, and its shadow fades out there.
    pub window_gap: u32,
    /// How opaque the background is, 0 to 1.
    pub opacity: f64,
    /// The font family. Empty follows Qt's (qt6ct's or the desktop's).
    pub font: String,
    /// The text size, in pixels.
    pub font_size: u32,
    /// `auto` (Nerd Font icons when one is installed), `glyphs` or `text`.
    pub icons: String,
    /// Ids from [`MODULES`] that are shown.
    pub modules: Vec<String>,
    /// The clock, as a Qt date format.
    pub clock_format: String,
    /// Each bar lists only its own monitor's workspaces.
    pub workspaces_per_monitor: bool,
    /// Labels for workspaces 1, 2, 3...; an empty or missing one shows the
    /// workspace's name.
    pub workspace_labels: Vec<String>,
    /// The media player to show, by part of its name. Empty: whichever plays.
    pub player: String,
    /// What right-clicking the volume runs. Empty: a mixer that is installed.
    pub mixer: String,
    /// What clicking Bluetooth runs. Empty: a Bluetooth app that is installed.
    pub bluetooth: String,
    /// What the launcher puts before an app that runs in a terminal, such as
    /// `kitty -e`. Empty: a terminal that is installed.
    pub terminal: String,
    /// The power menu's Lock. Empty: the locker [`Machine`] found, else
    /// `loginctl lock-session`. Each of these is left out of the menu when
    /// the bar is given an empty command.
    pub power_lock: String,
    /// Log out. Empty: Hyprland's exit dispatcher.
    pub power_logout: String,
    /// Suspend. Empty: `systemctl suspend`.
    pub power_suspend: String,
    /// Reboot. Empty: `systemctl reboot`.
    pub power_reboot: String,
    /// Shut down. Empty: `systemctl poweroff`.
    pub power_shutdown: String,
    /// `flat`; `neumorphic`: raised and pressed-in shapes with soft
    /// shadows; or `neubrutalism`: thick outlines and hard, offset shadows.
    pub style: String,
    /// `auto` (the color scheme's own), `light` or `dark`. A scheme of the
    /// other kind is built for the bar from the same seed color.
    pub mode: String,
    /// Where the launcher and power menu open: `center`, or a side or
    /// corner (`top`, `top-left`, ... `bottom-right`).
    pub menu_position: String,
    /// Their space from the screen's edges, when not centered.
    pub menu_margin: u32,
    /// The monitor they open on, by name. Empty: the focused one.
    pub menu_monitor: String,
    /// The launcher's width, in pixels.
    pub launcher_width: u32,
    /// How many apps the launcher lists at once.
    pub launcher_rows: u32,
    /// Whether the launcher shows the apps' icons. A grid always does.
    pub launcher_icons: bool,
    /// `list`, or `grid`: icons with their names under them.
    pub launcher_layout: String,
    /// How many rows of apps the grid shows at once.
    pub launcher_grid_rows: u32,
    /// The grid's icon size, in pixels; the columns follow from it and the
    /// launcher's width.
    pub launcher_icon_size: u32,
    /// The power menu's buttons, by id from [`POWER_ITEMS`], in that order.
    pub power_items: Vec<String>,
    /// `row` or `column`.
    pub power_layout: String,
    /// Whether the power menu's buttons have icons (with a Nerd Font).
    pub power_icons: bool,
}

/// The power menu's buttons, in their order, with what the page calls them.
pub const POWER_ITEMS: &[(&str, &str)] = &[
    ("lock", "Lock"),
    ("logout", "Log out"),
    ("suspend", "Suspend"),
    ("reboot", "Reboot"),
    ("shutdown", "Shut down"),
];

/// The bar's styles.
pub const BAR_STYLES: &[&str] = &["flat", "neumorphic", "neubrutalism"];

/// Where the menus can open.
pub const MENU_POSITIONS: &[&str] = &[
    "top-left",
    "top",
    "top-right",
    "left",
    "center",
    "right",
    "bottom-left",
    "bottom",
    "bottom-right",
];

impl Default for BarSettings {
    fn default() -> Self {
        Self {
            monitors: Vec::new(),
            position: "top".into(),
            height: 36,
            edge_margin: 4,
            side_margin: 0,
            window_gap: 0,
            opacity: 0.8,
            font: String::new(),
            font_size: 11,
            icons: "auto".into(),
            modules: MODULES.iter().map(|(id, _)| id.to_string()).collect(),
            clock_format: "ddd MM/dd  h:mm AP".into(),
            workspaces_per_monitor: false,
            workspace_labels: Vec::new(),
            player: String::new(),
            mixer: String::new(),
            bluetooth: String::new(),
            terminal: String::new(),
            power_lock: String::new(),
            power_logout: String::new(),
            power_suspend: String::new(),
            power_reboot: String::new(),
            power_shutdown: String::new(),
            style: "flat".into(),
            mode: "auto".into(),
            menu_position: "center".into(),
            menu_margin: 12,
            menu_monitor: String::new(),
            launcher_width: 600,
            launcher_rows: 8,
            launcher_icons: true,
            launcher_layout: "list".into(),
            launcher_grid_rows: 4,
            launcher_icon_size: 48,
            power_items: POWER_ITEMS.iter().map(|(id, _)| id.to_string()).collect(),
            power_layout: "row".into(),
            power_icons: true,
        }
    }
}

impl BarSettings {
    /// Within the ranges the page offers, with unknown values put right.
    pub fn sanitized(mut self) -> Self {
        if self.position != "bottom" {
            self.position = "top".into();
        }
        self.height = self.height.clamp(20, 80);
        self.edge_margin = self.edge_margin.min(200);
        self.side_margin = self.side_margin.min(2000);
        self.window_gap = self.window_gap.min(40);
        self.opacity = if self.opacity.is_finite() { self.opacity.clamp(0.0, 1.0) } else { 0.8 };
        self.font_size = self.font_size.clamp(6, 40);
        if !["auto", "glyphs", "text"].contains(&self.icons.as_str()) {
            self.icons = "auto".into();
        }
        let mut seen: Vec<String> = Vec::new();
        self.modules.retain(|m| {
            let keep = MODULES.iter().any(|(id, _)| id == m) && !seen.contains(m);
            seen.push(m.clone());
            keep
        });
        self.monitors.retain(|m| !m.trim().is_empty());
        if self.clock_format.trim().is_empty() {
            self.clock_format = BarSettings::default().clock_format;
        }
        self.workspace_labels.truncate(20);
        if !BAR_STYLES.contains(&self.style.as_str()) {
            self.style = "flat".into();
        }
        if !["auto", "light", "dark"].contains(&self.mode.as_str()) {
            self.mode = "auto".into();
        }
        if !MENU_POSITIONS.contains(&self.menu_position.as_str()) {
            self.menu_position = "center".into();
        }
        self.menu_margin = self.menu_margin.min(400);
        self.menu_monitor = self.menu_monitor.trim().to_string();
        self.launcher_width = self.launcher_width.clamp(320, 1600);
        self.launcher_rows = self.launcher_rows.clamp(3, 20);
        if self.launcher_layout != "grid" {
            self.launcher_layout = "list".into();
        }
        self.launcher_grid_rows = self.launcher_grid_rows.clamp(2, 10);
        self.launcher_icon_size = self.launcher_icon_size.clamp(24, 128);
        // Kept in the menu's own order, once each.
        let chosen = std::mem::take(&mut self.power_items);
        self.power_items = POWER_ITEMS
            .iter()
            .filter(|(id, _)| chosen.iter().any(|c| c == id))
            .map(|(id, _)| id.to_string())
            .collect();
        if self.power_layout != "column" {
            self.power_layout = "row".into();
        }
        self
    }
}

/// What the bar needs that depends on the machine: worked out when its
/// settings are written, so the bar itself never has to look.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Machine {
    /// Whether a Nerd Font is installed, for the icons.
    pub nerd_font: bool,
    /// The mixer found, as a command.
    pub mixer: Option<String>,
    /// The Bluetooth app found, as a command.
    pub bluetooth: Option<String>,
    /// The terminal found, as the command an app's own command follows.
    pub terminal: Option<String>,
    /// The screen locker found, as a command.
    pub lock: Option<String>,
}

impl Machine {
    /// Look.
    pub fn detect() -> Self {
        if cfg!(test) {
            return Self::default();
        }
        let first =
            |list: &[&str]| list.iter().find(|p| paths::which(p).is_some()).map(|p| p.to_string());
        let bluetooth = first(BLUETOOTH_APPS).or_else(|| {
            let tui = BLUETOOTH_TUIS.iter().find(|p| paths::which(p).is_some())?;
            let (term, args) = crate::hyprpm::terminal()?;
            let mut argv = vec![term.to_string_lossy().into_owned()];
            argv.extend(args);
            argv.push(tui.to_string());
            Some(argv.iter().map(|a| crate::walls::startup::quote(a)).collect::<Vec<_>>().join(" "))
        });
        let terminal = crate::hyprpm::terminal().map(|(term, args)| {
            std::iter::once(term.to_string_lossy().into_owned())
                .chain(args)
                .map(|a| crate::walls::startup::quote(&a))
                .collect::<Vec<_>>()
                .join(" ")
        });
        let lock =
            LOCKERS.iter().find(|(p, _)| paths::which(p).is_some()).map(|(_, c)| c.to_string());
        Self { nerd_font: nerd_font(), mixer: first(MIXERS), bluetooth, terminal, lock }
    }
}

/// Whether fontconfig knows a Nerd Font.
fn nerd_font() -> bool {
    Command::new("fc-list")
        .args([":", "family"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains("nerd font"))
        .unwrap_or(false)
}

/// The bar's folder.
pub fn dir(config_home: &Path) -> PathBuf {
    config_home.join("quickshell").join(CONFIG_NAME)
}

/// Quickshell's program, `qs` or `quickshell`.
pub fn program() -> Option<PathBuf> {
    paths::which("qs").or_else(|| paths::which("quickshell"))
}

/// The command that starts the bar. It names the folder by path: with a
/// `shell.qml` of the user's own directly in `~/.config/quickshell`,
/// Quickshell takes that whole folder as one config and `-c hyprcustard`
/// finds nothing.
pub fn start_command(program: &Path, config_home: &Path) -> String {
    let name = program.file_name().map(|n| n.to_string_lossy().into_owned());
    format!(
        "{} -p {}",
        name.as_deref().unwrap_or("qs"),
        crate::walls::startup::quote(&dir(config_home).to_string_lossy())
    )
}

/// Whether the bar is set up: the folder holds the app's stamp.
pub fn installed(config_home: &Path) -> bool {
    dir(config_home).join(STAMP).is_file()
}

/// The version that wrote the bar's files, and the files it wrote.
fn read_stamp(config_home: &Path) -> Option<(String, Vec<String>)> {
    let text = std::fs::read_to_string(dir(config_home).join(STAMP)).ok()?;
    let mut lines = text.lines();
    let version = lines.next()?.trim().to_string();
    Some((version, lines.map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()))
}

/// The version that wrote the installed files, if they are installed.
pub fn installed_version(config_home: &Path) -> Option<String> {
    read_stamp(config_home).map(|(v, _)| v)
}

/// Write the bar's QML and stamp, leaving its settings and colors alone, and
/// take out any file an older version wrote that this one no longer ships.
fn write_files(config_home: &Path) -> Result<()> {
    let dir = dir(config_home);
    std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
    let old: Vec<String> = read_stamp(config_home).map(|(_, f)| f).unwrap_or_default();
    for (name, bytes) in write_order(|name| dir.join(name).exists()) {
        paths::write_atomic_bytes(&dir.join(name), bytes)?;
    }
    for gone in old.iter().filter(|o| !FILES.iter().any(|(n, _)| n == o)) {
        // Only a plain file name from the stamp, never a path.
        if !gone.contains('/') && gone != ".." {
            let _ = std::fs::remove_file(dir.join(gone));
        }
    }
    let mut stamp = format!("{}\n", crate::VERSION);
    for (name, _) in FILES {
        stamp.push_str(name);
        stamp.push('\n');
    }
    paths::write_atomic(&dir.join(STAMP), &stamp)
}

/// The order to write [`FILES`] in, so a running bar never loads a mix it
/// cannot use. Quickshell reloads as soon as a file it has loaded changes,
/// even in the middle of an update. So the files it has not seen yet go
/// first (writing them sets off nothing, and they are there when a changed
/// file names them), then the ones it has, and `shell.qml` last, so the
/// final reload sees every file as it now is.
fn write_order(exists: impl Fn(&str) -> bool) -> Vec<(&'static str, &'static [u8])> {
    let mut files: Vec<(&str, &[u8])> = FILES.to_vec();
    files.sort_by_key(|(name, _)| (*name == "shell.qml", exists(name)));
    files
}

/// Set the bar up: its QML, its settings and its colors.
///
/// Refused when the folder holds a Quickshell config that is not the app's,
/// which would be overwritten.
pub fn install(
    config_home: &Path,
    settings: &BarSettings,
    machine: &Machine,
    scheme: &Scheme,
) -> Result<()> {
    let dir = dir(config_home);
    if !installed(config_home) && dir.join("shell.qml").exists() {
        return Err(Error::other(format!(
            "{} already holds a Quickshell config that is not hyprcustard's. Move it \
             elsewhere and try again",
            paths::contract(&dir)
        )));
    }
    write_files(config_home)?;
    write_settings(config_home, settings, machine)?;
    write_colors(config_home, scheme)
}

/// Rewrite the QML when the app is newer than the files. Returns whether it
/// did. An older app leaves a newer bar alone: it would put back older
/// files, and take out the ones only the newer version has.
pub fn update_if_stale(config_home: &Path) -> Result<bool> {
    match installed_version(config_home) {
        Some(v) if newer(crate::VERSION, &v) => write_files(config_home).map(|()| true),
        _ => Ok(false),
    }
}

/// Whether version `a` comes after `b`, by their dotted numbers. A version
/// that is not numbers counts as older than any that is.
fn newer(a: &str, b: &str) -> bool {
    let parse = |v: &str| -> Option<Vec<u64>> {
        v.trim().trim_start_matches('v').split('.').map(|p| p.parse().ok()).collect()
    };
    match (parse(a), parse(b)) {
        (Some(a), Some(b)) => a > b,
        (Some(_), None) => true,
        _ => false,
    }
}

/// The colors file, for a scheme.
pub fn write_colors(config_home: &Path, scheme: &Scheme) -> Result<()> {
    paths::write_atomic(
        &dir(config_home).join(COLORS_FILE),
        &crate::colors::targets::render("bar", scheme, 1.0),
    )
}

/// What [`SETTINGS_FILE`] holds: the settings, with what is left to the
/// machine filled in.
pub fn settings_json(settings: &BarSettings, machine: &Machine) -> serde_json::Value {
    let glyphs = match settings.icons.as_str() {
        "glyphs" => true,
        "text" => false,
        _ => machine.nerd_font,
    };
    let pick = |mine: &str, found: &Option<String>| {
        if mine.trim().is_empty() {
            found.clone().unwrap_or_default()
        } else {
            mine.trim().to_string()
        }
    };
    json!({
        "monitors": settings.monitors,
        "position": settings.position,
        "height": settings.height,
        "edge_margin": settings.edge_margin,
        "side_margin": settings.side_margin,
        "window_gap": settings.window_gap,
        "opacity": settings.opacity,
        "font": settings.font,
        "font_size": settings.font_size,
        "glyphs": glyphs,
        "modules": settings.modules,
        "clock_format": settings.clock_format,
        "workspaces_per_monitor": settings.workspaces_per_monitor,
        "workspace_labels": settings.workspace_labels,
        "player": settings.player,
        "mixer": pick(&settings.mixer, &machine.mixer),
        "bluetooth": pick(&settings.bluetooth, &machine.bluetooth),
        "terminal": pick(&settings.terminal, &machine.terminal),
        "power": power_commands(settings, machine),
        "style": settings.style,
        "mode": settings.mode,
        "menu_position": settings.menu_position,
        "menu_margin": settings.menu_margin,
        "menu_monitor": settings.menu_monitor,
        "launcher_width": settings.launcher_width,
        "launcher_rows": settings.launcher_rows,
        "launcher_icons": settings.launcher_icons,
        "launcher_layout": settings.launcher_layout,
        "launcher_grid_rows": settings.launcher_grid_rows,
        "launcher_icon_size": settings.launcher_icon_size,
        "power_items": settings.power_items,
        "power_layout": settings.power_layout,
        "power_icons": settings.power_icons,
    })
}

/// The power menu's commands, each the page's, else the machine's, else
/// the usual one.
pub fn power_commands(settings: &BarSettings, machine: &Machine) -> serde_json::Value {
    let fallback = |id: &str| {
        POWER_DEFAULTS
            .iter()
            .find(|(k, _)| *k == id)
            .map(|(_, c)| c.to_string())
            .unwrap_or_default()
    };
    let pick = |mine: &str, id: &str| {
        let mine = mine.trim();
        if !mine.is_empty() {
            return mine.to_string();
        }
        match id {
            "lock" => machine.lock.clone().unwrap_or_else(|| fallback(id)),
            _ => fallback(id),
        }
    };
    json!({
        "lock": pick(&settings.power_lock, "lock"),
        "logout": pick(&settings.power_logout, "logout"),
        "suspend": pick(&settings.power_suspend, "suspend"),
        "reboot": pick(&settings.power_reboot, "reboot"),
        "shutdown": pick(&settings.power_shutdown, "shutdown"),
    })
}

/// Write [`SETTINGS_FILE`], which a running bar follows at once.
pub fn write_settings(config_home: &Path, settings: &BarSettings, machine: &Machine) -> Result<()> {
    let text = serde_json::to_string_pretty(&settings_json(settings, machine))?;
    paths::write_atomic(&dir(config_home).join(SETTINGS_FILE), &format!("{text}\n"))
}

/// Take the bar's files out: exactly the ones the stamp lists, its settings,
/// colors and stamp, then the folder if that leaves it empty. Anything else
/// the user put there stays.
pub fn uninstall(config_home: &Path) -> Result<()> {
    let dir = dir(config_home);
    let Some((_, files)) = read_stamp(config_home) else { return Ok(()) };
    let ours = files
        .iter()
        .map(String::as_str)
        .chain([SETTINGS_FILE, COLORS_FILE, STAMP])
        .filter(|n| !n.contains('/') && *n != "..");
    for name in ours {
        let p = dir.join(name);
        if p.exists() {
            std::fs::remove_file(&p).map_err(|e| Error::io(&p, e))?;
        }
    }
    // Fails, harmlessly, when the user left something of their own there.
    let _ = std::fs::remove_dir(&dir);
    Ok(())
}

/// Running Quickshells that run the bar.
pub fn running_pids(config_home: &Path) -> Vec<u32> {
    let root = config_home.join("quickshell");
    let mine = dir(config_home);
    let mut pids = crate::procs::named("qs");
    pids.extend(crate::procs::named("quickshell"));
    pids.into_iter()
        .filter(|pid| {
            let Ok(raw) = std::fs::read(format!("/proc/{pid}/cmdline")) else { return false };
            let args: Vec<String> =
                raw.split(|b| *b == 0).map(|a| String::from_utf8_lossy(a).into_owned()).collect();
            crate::colors::targets::quickshell_arg(&args, &root).is_some_and(|d| d == mine)
        })
        .collect()
}

/// Ask Hyprland to start the bar. Hyprland starts it, so it gets the
/// session's environment rather than the app's, and outlives the app. It
/// is not running yet when this returns: see [`Control`].
pub fn start(program: &Path, config_home: &Path) -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    if !crate::hyprctl::is_running() {
        return Err(Error::other(
            "Hyprland is not running, so there is nothing to start the bar in",
        ));
    }
    crate::look::eval(&format!("hl.exec_cmd({})", lua_quote(&start_command(program, config_home))))
}

/// Stop every copy of the bar: asked to first, then made to if one is
/// still there after two seconds. Slow, so not on the window's thread.
pub fn stop(config_home: &Path) -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    for (signal, wait) in [("-TERM", 2000), ("-KILL", 1000)] {
        let pids: Vec<String> = running_pids(config_home).iter().map(u32::to_string).collect();
        if pids.is_empty() {
            return Ok(());
        }
        let _ = Command::new("kill")
            .arg(signal)
            .args(&pids)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let gone = wait_for(wait, || running_pids(config_home).is_empty());
        if gone {
            return Ok(());
        }
    }
    Err(Error::other("the bar did not stop, even when made to"))
}

/// Poll `done` every 25 ms for up to `ms` milliseconds.
fn wait_for(ms: u64, mut done: impl FnMut() -> bool) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(ms);
    loop {
        if done() {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

/// Starting, stopping or restarting the bar, made on the window's thread
/// and run on another: each waits until the bar has really started or
/// stopped, so the page never says one thing while the other is true.
#[derive(Debug, Clone, PartialEq)]
pub struct Control {
    /// `start`, `stop`, `restart` or `remove` (which stops it first).
    pub op: String,
    /// Quickshell, for starting.
    pub program: std::path::PathBuf,
    /// `$XDG_CONFIG_HOME`.
    pub config_home: std::path::PathBuf,
}

impl Control {
    /// Do it. The message says what happened.
    pub fn run(&self) -> Result<String> {
        let home = &self.config_home;
        match self.op.as_str() {
            "stop" => stop(home).map(|()| "Stopped the bar".into()),
            "remove" => stop(home).map(|()| String::new()),
            "start" if !cfg!(test) && !running_pids(home).is_empty() => {
                Ok("The bar is already running".into())
            }
            "start" => self.start_and_see().map(|()| "Started the bar".into()),
            "restart" => {
                // A copy that would not stop must not get a second beside it.
                stop(home)?;
                self.start_and_see().map(|()| "Restarted the bar".into())
            }
            other => Err(Error::other(format!("the bar cannot {other}"))),
        }
    }

    fn start_and_see(&self) -> Result<()> {
        start(&self.program, &self.config_home)?;
        if cfg!(test) || wait_for(5000, || !running_pids(&self.config_home).is_empty()) {
            return Ok(());
        }
        Err(Error::other(format!(
            "Hyprland was asked to start the bar, but it has not appeared after five seconds. \
             Run {} in a terminal to see why",
            start_command(&self.program, &self.config_home)
        )))
    }
}

// ---------------------------------------------------------------------------
// Starting at login
// ---------------------------------------------------------------------------

/// The login block's markers.
pub const MARKERS: Markers = Markers {
    begin: "-- >>> HyprCustard bar (managed by hyprcustard) >>>",
    end: "-- <<< HyprCustard bar (managed by hyprcustard) <<<",
    name: "HyprCustard bar",
};

/// What a handed-over line starts with, after its indentation.
pub const MOVED: &str = "-- [moved to the HyprCustard bar block] ";

/// Programs whose start line means another bar or shell.
const BARS: &[&str] = &[
    "waybar",
    "qs",
    "quickshell",
    "hyprpanel",
    "ags",
    "astal",
    "eww",
    "nwg-panel",
    "yambar",
    "ironbar",
    "sfwbar",
    "gBar",
    "ashell",
];

/// The block's body.
pub fn body(command: &str) -> String {
    format!(
        "-- Starts hyprcustard's bar at login. Turn it off on hyprcustard's Bar page,\n\
         -- where \"Give them back\" also brings back any lines this replaced.\n\
         hl.on(\"hyprland.start\", function()\n    hl.exec_cmd({})\nend)\n",
        lua_quote(command)
    )
}

/// A line in the user's config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Line {
    /// 1-based.
    pub number: usize,
    /// The line, trimmed.
    pub text: String,
    /// Whether it is one start command on its own line.
    pub movable: bool,
}

/// Lines outside every managed block, not commented out and not key
/// bindings, that start a bar or shell.
pub fn user_lines(text: &str) -> Vec<Line> {
    lines_outside_blocks(text)
        .filter(|(_, l)| {
            let t = l.trim_start();
            !t.starts_with("--")
                && !t.starts_with('#')
                && t.contains("exec")
                && !t.contains("bind")
                && BARS.iter().any(|w| has_word(t, w))
        })
        .map(|(number, l)| Line { number, text: l.trim().to_string(), movable: movable(l) })
        .collect()
}

/// Lines the app commented out when it took over.
pub fn moved_lines(text: &str) -> Vec<Line> {
    text.lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let rest = l.trim_start().strip_prefix(MOVED)?;
            Some(Line { number: i + 1, text: rest.trim().to_string(), movable: true })
        })
        .collect()
}

/// Whether the block is there.
pub fn managed(text: &str) -> bool {
    matches!(block::split_with(text, &MARKERS), Ok(Some(_)))
}

/// The block as it is in the config: the line its opening marker is on and
/// what is between the markers.
pub fn block_text(text: &str) -> Option<(usize, String)> {
    let split = block::split_with(text, &MARKERS).ok()??;
    let line = split.before.lines().count() + 1;
    Some((line, split.inner.trim_end().to_string()))
}

/// The config with the block written, the user's lines left as they are.
pub fn add_block(text: &str, command: &str) -> Result<String> {
    block::splice_with(text, &body(command), &MARKERS)
}

/// Comment the user's bar lines out and write the block.
///
/// Refused, with nothing changed, when such a line holds other code too.
pub fn hand_over(text: &str, command: &str) -> Result<String> {
    let lines = user_lines(text);
    let stuck: Vec<String> =
        lines.iter().filter(|l| !l.movable).map(|l| l.number.to_string()).collect();
    if !stuck.is_empty() {
        return Err(Error::other(format!(
            "{} {} of hyprland.lua starts a bar alongside other code, so it cannot be \
             commented out on its own. Put the start command on a line of its own, or \
             remove it yourself, and try again",
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
    add_block(&out, command)
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
mod control_tests {
    use super::*;

    #[test]
    fn only_a_newer_app_rewrites_the_bar() {
        assert!(newer("0.11.0", "0.10.1"));
        assert!(newer("0.10.10", "0.10.9"), "by number, not by text");
        assert!(!newer("0.10.1", "0.11.0"), "an older app leaves a newer bar alone");
        assert!(!newer("0.10.1", "0.10.1"));
        assert!(newer("0.10.1", "unknown"));
        assert!(!newer("dev", "0.10.1"));
    }

    #[test]
    fn an_update_writes_new_files_first_and_the_shell_last() {
        let had = ["shell.qml", "Bar.qml", "Theme.qml"];
        let order: Vec<&str> =
            write_order(|n| had.contains(&n)).into_iter().map(|(n, _)| n).collect();
        assert_eq!(order.len(), FILES.len());
        assert_eq!(order.last(), Some(&"shell.qml"));
        let bar = order.iter().position(|n| *n == "Bar.qml").unwrap();
        let neu = order.iter().position(|n| *n == "Neu.qml").unwrap();
        assert!(neu < bar, "a file Bar.qml names is there before Bar.qml changes: {order:?}");
    }

    #[test]
    fn menu_choices_are_kept_in_range_and_in_the_menus_order() {
        let s = BarSettings {
            style: "glass".into(),
            menu_position: "middle".into(),
            launcher_rows: 99,
            launcher_width: 10,
            launcher_layout: "tiles".into(),
            launcher_grid_rows: 0,
            launcher_icon_size: 1000,
            power_items: ["shutdown", "lock", "nope", "lock"].map(String::from).to_vec(),
            power_layout: "grid".into(),
            ..Default::default()
        }
        .sanitized();
        assert_eq!((s.style.as_str(), s.menu_position.as_str()), ("flat", "center"));
        assert_eq!((s.launcher_rows, s.launcher_width), (20, 320));
        assert_eq!(s.launcher_layout, "list");
        assert_eq!((s.launcher_grid_rows, s.launcher_icon_size), (2, 128));
        assert_eq!(s.power_items, ["lock", "shutdown"]);
        assert_eq!(s.power_layout, "row");
        for style in BAR_STYLES {
            let s = BarSettings { style: style.to_string(), ..Default::default() }.sanitized();
            assert_eq!(s.style, *style);
        }
    }

    #[test]
    fn modules_are_kept_once_each_in_the_order_given() {
        let s = BarSettings {
            modules: ["clock", "cpu", "clock", "nope", "cpu", "tray"].map(String::from).to_vec(),
            ..Default::default()
        }
        .sanitized();
        assert_eq!(s.modules, ["clock", "cpu", "tray"]);
    }

    #[test]
    fn light_or_dark_is_one_of_the_three() {
        let s = BarSettings { mode: "dim".into(), ..Default::default() }.sanitized();
        assert_eq!(s.mode, "auto");
        let s = BarSettings { mode: "light".into(), ..Default::default() }.sanitized();
        assert_eq!(s.mode, "light");
    }

    /// A fake /sys: hwmon folders given as (driver, [(label, millidegrees)]),
    /// a label of "" having no label file.
    fn sysfs(hwmons: &[(&str, &[(&str, i32)])]) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        for (i, (name, temps)) in hwmons.iter().enumerate() {
            let h = root.path().join(format!("class/hwmon/hwmon{i}"));
            std::fs::create_dir_all(&h).unwrap();
            std::fs::write(h.join("name"), format!("{name}\n")).unwrap();
            for (n, (label, milli)) in temps.iter().enumerate() {
                let n = n + 1;
                std::fs::write(h.join(format!("temp{n}_input")), format!("{milli}\n")).unwrap();
                if !label.is_empty() {
                    std::fs::write(h.join(format!("temp{n}_label")), format!("{label}\n")).unwrap();
                }
            }
        }
        root
    }

    /// What temps.sh finds there: (how, the path under the root, the sensor).
    fn find(root: &Path, kind: &str) -> Option<(String, String, String)> {
        let script = root.join("temps.sh");
        std::fs::write(&script, include_str!("../bar/temps.sh")).unwrap();
        let out =
            std::process::Command::new("sh").arg(&script).arg(kind).arg(root).output().unwrap();
        assert!(out.status.success());
        let text = String::from_utf8(out.stdout).unwrap();
        let f: Vec<&str> = text.trim_end_matches('\n').split('\t').collect();
        (f.len() == 4).then(|| {
            let under = f[1].strip_prefix(root.to_str().unwrap()).unwrap_or(f[1]);
            (f[0].to_string(), under.to_string(), f[3].to_string())
        })
    }

    #[test]
    fn the_cpu_sensor_is_the_package_or_die_one() {
        let amd = sysfs(&[
            ("amdgpu", &[("edge", 40000)]),
            ("k10temp", &[("Tctl", 72000), ("Tdie", 62000)]),
        ]);
        let (how, path, sensor) = find(amd.path(), "cpu").unwrap();
        assert_eq!((how.as_str(), sensor.as_str()), ("file", "k10temp Tdie"));
        assert_eq!(path, "/class/hwmon/hwmon1/temp2_input");

        let intel = sysfs(&[(
            "coretemp",
            &[("Core 0", 50000), ("Package id 0", 55000), ("Core 1", 51000)],
        )]);
        let (_, path, sensor) = find(intel.path(), "cpu").unwrap();
        assert_eq!(
            (path.as_str(), sensor.as_str()),
            ("/class/hwmon/hwmon0/temp2_input", "coretemp Package id 0")
        );

        let arm = sysfs(&[("cpu_thermal", &[("", 48000)])]);
        assert_eq!(find(arm.path(), "cpu").unwrap().2, "cpu_thermal");
    }

    #[test]
    fn the_gpu_sensor_prefers_the_discrete_card() {
        let two = sysfs(&[
            ("amdgpu", &[("edge", 45000)]),
            ("nvme", &[("Composite", 40000)]),
            ("amdgpu", &[("edge", 33000), ("junction", 50000), ("mem", 68000)]),
        ]);
        let (how, path, sensor) = find(two.path(), "gpu").unwrap();
        assert_eq!((how.as_str(), sensor.as_str()), ("file", "amdgpu edge"));
        assert_eq!(path, "/class/hwmon/hwmon2/temp1_input");

        let apu = sysfs(&[("amdgpu", &[("edge", 45000)])]);
        assert_eq!(find(apu.path(), "gpu").unwrap().1, "/class/hwmon/hwmon0/temp1_input");
    }

    #[test]
    fn no_sensor_finds_nothing() {
        let none = sysfs(&[("nvme", &[("Composite", 40000)]), ("acpitz", &[("", 30000)])]);
        assert_eq!(find(none.path(), "cpu"), None);
        assert_eq!(find(none.path(), "gpu"), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scheme() -> Scheme {
        crate::colors::build(
            &crate::colors::ColorSettings::default(),
            &crate::colors::named::builtins(),
        )
        .unwrap()
        .scheme
    }

    #[test]
    fn installing_writes_every_file_and_removing_takes_only_those_out() {
        let home = tempfile::tempdir().unwrap();
        let d = dir(home.path());
        install(home.path(), &BarSettings::default(), &Machine::default(), &scheme()).unwrap();
        assert!(installed(home.path()));
        for (name, bytes) in FILES {
            assert_eq!(&std::fs::read(d.join(name)).unwrap(), bytes);
        }
        let colors: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(d.join(COLORS_FILE)).unwrap()).unwrap();
        assert!(colors["on_surface"].as_str().unwrap().starts_with('#'));
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(d.join(SETTINGS_FILE)).unwrap()).unwrap();
        assert_eq!(json["position"], "top");
        assert_eq!(installed_version(home.path()).as_deref(), Some(crate::VERSION));

        std::fs::write(d.join("Mine.qml"), "// the user's").unwrap();
        uninstall(home.path()).unwrap();
        assert!(!installed(home.path()));
        assert!(d.join("Mine.qml").exists(), "a file of the user's stays");
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 1);
        std::fs::remove_file(d.join("Mine.qml")).unwrap();
        install(home.path(), &BarSettings::default(), &Machine::default(), &scheme()).unwrap();
        uninstall(home.path()).unwrap();
        assert!(!d.exists(), "an empty folder goes too");
    }

    #[test]
    fn a_config_of_the_users_in_the_folder_is_not_overwritten() {
        let home = tempfile::tempdir().unwrap();
        let d = dir(home.path());
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("shell.qml"), "// mine").unwrap();
        let err = install(home.path(), &BarSettings::default(), &Machine::default(), &scheme());
        assert!(err.is_err());
        assert_eq!(std::fs::read_to_string(d.join("shell.qml")).unwrap(), "// mine");
    }

    #[test]
    fn an_older_install_is_rewritten_and_loses_files_no_longer_shipped() {
        let home = tempfile::tempdir().unwrap();
        let d = dir(home.path());
        install(home.path(), &BarSettings::default(), &Machine::default(), &scheme()).unwrap();
        assert!(!update_if_stale(home.path()).unwrap());
        std::fs::write(d.join("Old.qml"), "old").unwrap();
        std::fs::write(d.join("Bar.qml"), "stale").unwrap();
        std::fs::write(d.join(STAMP), "0.0.1\nOld.qml\nBar.qml\n").unwrap();
        let settings = std::fs::read_to_string(d.join(SETTINGS_FILE)).unwrap();
        assert!(update_if_stale(home.path()).unwrap());
        assert!(!d.join("Old.qml").exists());
        assert_ne!(std::fs::read_to_string(d.join("Bar.qml")).unwrap(), "stale");
        assert_eq!(std::fs::read_to_string(d.join(SETTINGS_FILE)).unwrap(), settings);
    }

    #[test]
    fn the_machine_fills_in_what_the_settings_leave_open() {
        let m = Machine {
            nerd_font: false,
            mixer: Some("pavucontrol".into()),
            bluetooth: Some("blueman-manager".into()),
            terminal: Some("foot -e".into()),
            lock: Some("hyprlock".into()),
        };
        let mut s = BarSettings::default();
        let j = settings_json(&s, &m);
        assert_eq!(
            (j["glyphs"].as_bool(), j["mixer"].as_str()),
            (Some(false), Some("pavucontrol"))
        );
        s.icons = "glyphs".into();
        s.mixer = "my-mixer --tab 3".into();
        let j = settings_json(&s, &m);
        assert_eq!(
            (j["glyphs"].as_bool(), j["mixer"].as_str()),
            (Some(true), Some("my-mixer --tab 3"))
        );
        assert_eq!(j["bluetooth"], "blueman-manager");
        assert_eq!(j["terminal"], "foot -e");
        assert_eq!(j["power"]["lock"], "hyprlock");
        assert_eq!(j["power"]["reboot"], "systemctl reboot");
        s.power_reboot = " my-reboot ".into();
        let j = settings_json(&s, &Machine::default());
        assert_eq!(j["power"]["reboot"], "my-reboot");
        assert_eq!(j["power"]["lock"], "loginctl lock-session");
    }

    #[test]
    fn settings_are_kept_in_range() {
        let s = BarSettings {
            position: "left".into(),
            height: 5,
            opacity: f64::NAN,
            icons: "emoji".into(),
            modules: vec!["clock".into(), "nonsense".into()],
            clock_format: " ".into(),
            ..Default::default()
        }
        .sanitized();
        assert_eq!((s.position.as_str(), s.height, s.opacity), ("top", 20, 0.8));
        assert_eq!(s.icons, "auto");
        assert_eq!(s.modules, vec!["clock".to_string()]);
        assert!(!s.clock_format.trim().is_empty());
    }

    #[test]
    fn other_bars_are_found_handed_over_and_given_back() {
        let mine = "\
hl.on(\"hyprland.start\", function()
    hl.exec_cmd(\"qs\")
    hl.exec_cmd(\"waybar -c ~/.config/waybar/top.jsonc\")
    hl.exec_cmd(\"dunst\")
end)
hl.bind(\"SUPER + B\", hl.dsp.exec_cmd(\"pkill waybar || waybar\"))
-- hl.exec_cmd(\"eww open bar\")
";
        let found: Vec<usize> = user_lines(mine).iter().map(|l| l.number).collect();
        assert_eq!(found, vec![2, 3], "not dunst, a binding or a comment");
        let over = hand_over(mine, "qs -c hyprcustard").unwrap();
        assert!(managed(&over));
        assert!(user_lines(&over).is_empty(), "the block's own line is not one of theirs");
        assert_eq!(moved_lines(&over).len(), 2);
        assert!(over.contains(&format!("    {MOVED}hl.exec_cmd(\"qs\")")));
        assert_eq!(give_back(&over).unwrap(), mine);
    }

    #[test]
    fn a_line_holding_other_code_is_refused_whole() {
        let mine = "hl.on(\"hyprland.start\", function() hl.exec_cmd(\"waybar\") end)\n";
        assert!(hand_over(mine, "qs -c hyprcustard").is_err());
        let both = add_block(mine, "qs -c hyprcustard").unwrap();
        assert!(both.starts_with(mine) && managed(&both));
        assert_eq!(give_back(&both).unwrap(), mine);
    }

    #[test]
    fn the_start_command_names_the_config() {
        let home = Path::new("/home/u/.config");
        assert_eq!(
            start_command(Path::new("/usr/bin/qs"), home),
            "qs -p /home/u/.config/quickshell/hyprcustard"
        );
        assert_eq!(
            start_command(Path::new("/usr/bin/quickshell"), Path::new("/home/a b/.config")),
            "quickshell -p '/home/a b/.config/quickshell/hyprcustard'"
        );
        assert!(body("qs -c hyprcustard").contains("hl.exec_cmd(\"qs -c hyprcustard\")"));
    }
}
