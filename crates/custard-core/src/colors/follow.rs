//! Following the wallpaper: what `custard-watch` reads, how it decides, and
//! how it is started and stopped.
//!
//! The watcher follows a *snapshot* of the Colors page's choices —
//! `colors/follow.toml` — written when following is turned on and again on
//! each Apply. Trying other palettes on the page therefore never disturbs
//! it, and removing the snapshot is what tells it to stop.
//!
//! It is started at login one of two ways: a small managed block in the
//! Hyprland config (the default, and right for nearly everyone), or a
//! systemd user service for sessions run under `graphical-session.target`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::wallpaper::Wallpaper;
use super::ColorSettings;
use crate::block::Markers;
use crate::error::{Error, Result};
use crate::paths;

/// The watcher's program name, which is also its `comm` in `/proc`.
pub const PROGRAM: &str = "custard-watch";

/// The block in the Hyprland config that starts the watcher at login.
pub const MARKERS: Markers = Markers {
    begin: "-- >>> HyprCustard colors (managed by hyprcustard) >>>",
    end: "-- <<< HyprCustard colors (managed by hyprcustard) <<<",
    name: "HyprCustard colors",
};

// ---------------------------------------------------------------------------
// The snapshot
// ---------------------------------------------------------------------------

/// Where the choices being followed are kept.
pub fn snapshot_path() -> PathBuf {
    paths::app_config_dir().join("colors").join("follow.toml")
}

/// The choices being followed, or `None` when not following.
pub fn following() -> Option<ColorSettings> {
    let text = std::fs::read_to_string(snapshot_path()).ok()?;
    toml::from_str::<ColorSettings>(&text).ok().map(ColorSettings::sanitized)
}

/// Follow these choices from now on.
pub fn write_snapshot(choices: &ColorSettings) -> Result<()> {
    let path = snapshot_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    }
    let text = toml::to_string_pretty(choices)
        .map_err(|e| Error::other(format!("could not encode the colors to follow: {e}")))?;
    paths::write_atomic(
        &path,
        &format!(
            "# What custard-watch follows: the Colors page's choices as of the last Apply.\n\
             # Written by hyprcustard; turn following off on the Colors page.\n{text}"
        ),
    )
}

/// Stop following. Not following already is fine.
pub fn clear_snapshot() -> Result<()> {
    let path = snapshot_path();
    match std::fs::remove_file(&path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(Error::io(&path, e)),
        _ => Ok(()),
    }
}

// ---------------------------------------------------------------------------
// Deciding
// ---------------------------------------------------------------------------

/// What identifies a wallpaper for following: when this changes, the
/// colors may need to.
///
/// With the whole folder sampled, the clip on show does not matter — every
/// clip gets the same scheme — so only the folder counts.
pub fn key(w: &Wallpaper, whole_folder: bool) -> String {
    let showing = if whole_folder && w.folder().is_some() {
        String::new()
    } else {
        w.showing.as_ref().map(|p| p.display().to_string()).unwrap_or_default()
    };
    format!("{}|{}|{}|{showing}", w.program, w.monitor.as_deref().unwrap_or("*"), w.path.display())
}

/// What the watcher should do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// Nothing has changed.
    Idle,
    /// Build and apply now.
    Apply,
    /// A change is waiting for the minimum gap since the last one.
    Wait(Duration),
    /// A change is waiting for nothing to be fullscreen.
    Paused,
}

/// The watcher's memory: what it last saw and when it last changed colors.
#[derive(Debug, Clone)]
pub struct Follower {
    /// The shortest time between two changes of colors.
    pub min_gap: Duration,
    seen: Option<String>,
    last_change: Option<Instant>,
    pending: bool,
}

impl Follower {
    /// A follower that has seen nothing yet.
    pub fn new(min_gap: Duration) -> Self {
        Self { min_gap, seen: None, last_change: None, pending: false }
    }

    /// Note what the wallpaper is now. A different one than last time
    /// leaves a change pending; none at all (the wallpaper program not up
    /// yet, at login) leaves things as they are.
    pub fn saw(&mut self, key: Option<String>) {
        if let Some(k) = key {
            if self.seen.as_ref() != Some(&k) {
                self.seen = Some(k);
                self.pending = true;
            }
        }
    }

    /// Whether a change is waiting. Only then is it worth asking Hyprland
    /// whether anything is fullscreen.
    pub fn pending(&self) -> bool {
        self.pending
    }

    /// What to do, given the time and whether anything is fullscreen.
    pub fn decide(&self, now: Instant, fullscreen: bool) -> Next {
        if !self.pending {
            return Next::Idle;
        }
        if fullscreen {
            return Next::Paused;
        }
        if let Some(t) = self.last_change {
            let since = now.saturating_duration_since(t);
            if since < self.min_gap {
                return Next::Wait(self.min_gap - since);
            }
        }
        Next::Apply
    }

    /// The pending change was dealt with. `changed` is whether the colors
    /// actually changed — a wallpaper that gives the same scheme as before
    /// does not start the minimum gap.
    pub fn done(&mut self, now: Instant, changed: bool) {
        self.pending = false;
        if changed {
            self.last_change = Some(now);
        }
    }
}

// ---------------------------------------------------------------------------
// What it is doing, for the page
// ---------------------------------------------------------------------------

/// The watcher's report on itself, rewritten when its state changes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Status {
    /// Its process id.
    pub pid: u32,
    /// `watching`, `waiting`, `paused` or `error`.
    pub state: String,
    /// A sentence about it.
    pub detail: String,
    /// The scheme it last applied.
    pub last_applied: Option<String>,
    /// When, as local time.
    pub at: Option<String>,
}

/// Where the status is kept: the runtime directory, which is memory, so a
/// status change never touches the disk.
pub fn status_path() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(d) if !d.is_empty() && !cfg!(test) => PathBuf::from(d).join("custard-watch.json"),
        _ => paths::app_config_dir().join("colors").join("watch-status.json"),
    }
}

/// The watcher's last report, if one is running.
pub fn read_status() -> Option<Status> {
    let s: Status = serde_json::from_str(&std::fs::read_to_string(status_path()).ok()?).ok()?;
    running_pids().contains(&s.pid).then_some(s)
}

/// Write the watcher's report.
pub fn write_status(s: &Status) {
    let path = status_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string(s) {
        let _ = paths::write_atomic(&path, &text);
    }
}

// ---------------------------------------------------------------------------
// Starting and stopping
// ---------------------------------------------------------------------------

/// The watcher program: beside the running executable first — so a build
/// tree or a package finds its own — then on PATH.
pub fn binary() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join(PROGRAM)))
        .filter(|p| p.is_file())
        .or_else(|| paths::which(PROGRAM))
}

/// Ids of running watchers, other than this process.
pub fn running_pids() -> Vec<u32> {
    let me = std::process::id();
    let Ok(rd) = std::fs::read_dir("/proc") else { return Vec::new() };
    rd.filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| *pid != me)
        .filter(|pid| {
            std::fs::read_to_string(format!("/proc/{pid}/comm")).is_ok_and(|c| c.trim() == PROGRAM)
        })
        .collect()
}

/// Start the watcher now, in the background. It outlives the app.
pub fn start(binary: &Path) -> Result<()> {
    if cfg!(test) || !running_pids().is_empty() {
        return Ok(());
    }
    let mut child = Command::new(binary)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Error::other(format!("could not start {}: {e}", binary.display())))?;
    // Reap it if it exits while the app is still open, rather than leaving
    // a zombie behind.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Stop every running watcher.
pub fn stop() {
    if cfg!(test) {
        return;
    }
    let pids: Vec<String> = running_pids().iter().map(u32::to_string).collect();
    if pids.is_empty() {
        return;
    }
    let _ = Command::new("kill")
        .arg("-TERM")
        .args(&pids)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// The Hyprland block that starts the watcher at login.
pub fn startup_block(binary: &Path) -> String {
    format!(
        "-- Keeps your colors matching the wallpaper. Turn it off on hyprcustard's\n\
         -- Colors page rather than here; this block is rewritten from there.\n\
         hl.on(\"hyprland.start\", function()\n    hl.exec_cmd({})\nend)\n",
        crate::model::lua_quote(&binary.to_string_lossy())
    )
}

/// Where the systemd user unit goes.
pub fn unit_path() -> PathBuf {
    paths::apps_config_home().join("systemd").join("user").join(format!("{PROGRAM}.service"))
}

/// The systemd user unit.
pub fn unit_text(binary: &Path) -> String {
    format!(
        "# Written by hyprcustard. Turn it off on the Colors page rather than here.\n\
         [Unit]\n\
         Description=Keep the desktop's colors matching the wallpaper (hyprcustard)\n\
         PartOf=graphical-session.target\n\
         After=graphical-session.target\n\n\
         [Service]\n\
         ExecStart={}\n\
         Restart=on-failure\n\
         RestartSec=10\n\n\
         [Install]\n\
         WantedBy=graphical-session.target\n",
        binary.display()
    )
}

/// Whether this session runs under systemd's graphical session target, so
/// a user service would start with it.
pub fn systemd_available() -> bool {
    Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", "graphical-session.target"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Install and start the user service.
pub fn enable_systemd(binary: &Path) -> Result<()> {
    let path = unit_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    }
    paths::write_atomic(&path, &unit_text(binary))?;
    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", "--now", &format!("{PROGRAM}.service")])
}

/// Stop and remove the user service, if it is installed.
pub fn disable_systemd() -> Result<()> {
    let path = unit_path();
    if !path.exists() {
        return Ok(());
    }
    let _ = systemctl(&["disable", "--now", &format!("{PROGRAM}.service")]);
    std::fs::remove_file(&path).map_err(|e| Error::io(&path, e))?;
    let _ = systemctl(&["daemon-reload"]);
    Ok(())
}

fn systemctl(args: &[&str]) -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    let out = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| Error::other(format!("could not run systemctl: {e}")))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(Error::other(format!(
            "systemctl --user {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

/// Held by every test that touches the snapshot, which is one file shared
/// by all of them.
#[cfg(test)]
pub(crate) static SNAPSHOT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: Duration = Duration::from_secs(300);

    #[test]
    fn a_new_wallpaper_is_applied_at_once_the_first_time() {
        let mut f = Follower::new(MIN);
        let t = Instant::now();
        assert_eq!(f.decide(t, false), Next::Idle);
        f.saw(None);
        assert_eq!(f.decide(t, false), Next::Idle, "no wallpaper program yet");
        f.saw(Some("a".into()));
        assert_eq!(f.decide(t, false), Next::Apply);
        f.done(t, true);
        f.saw(Some("a".into()));
        assert_eq!(f.decide(t, false), Next::Idle, "the same wallpaper again");
    }

    #[test]
    fn a_second_change_waits_out_the_minimum_gap() {
        let mut f = Follower::new(MIN);
        let t = Instant::now();
        f.saw(Some("a".into()));
        f.done(t, true);
        f.saw(Some("b".into()));
        let later = t + Duration::from_secs(60);
        assert_eq!(f.decide(later, false), Next::Wait(Duration::from_secs(240)));
        assert_eq!(f.decide(t + MIN, false), Next::Apply);
    }

    #[test]
    fn fullscreen_holds_a_change_back_until_it_ends() {
        let mut f = Follower::new(Duration::ZERO);
        let t = Instant::now();
        f.saw(Some("a".into()));
        assert_eq!(f.decide(t, true), Next::Paused);
        assert!(f.pending());
        assert_eq!(f.decide(t, false), Next::Apply);
    }

    #[test]
    fn an_unchanged_scheme_does_not_start_the_gap() {
        let mut f = Follower::new(MIN);
        let t = Instant::now();
        f.saw(Some("a".into()));
        f.done(t, false);
        f.saw(Some("b".into()));
        assert_eq!(f.decide(t, false), Next::Apply);
    }

    #[test]
    fn the_key_ignores_the_clip_when_the_whole_folder_counts() {
        let dir = tempfile::tempdir().unwrap();
        let w = |clip: &str| Wallpaper {
            program: "mpvpaper".into(),
            monitor: None,
            path: dir.path().to_path_buf(),
            showing: Some(dir.path().join(clip)),
        };
        assert_ne!(key(&w("a.mp4"), false), key(&w("b.mp4"), false));
        assert_eq!(key(&w("a.mp4"), true), key(&w("b.mp4"), true));
    }

    #[test]
    fn the_snapshot_round_trips_and_clears() {
        let _lock = SNAPSHOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_snapshot().unwrap();
        assert!(following().is_none());
        let s = ColorSettings { source: "wallpaper".into(), min_minutes: 12, ..Default::default() };
        write_snapshot(&s).unwrap();
        assert_eq!(following().unwrap(), s);
        clear_snapshot().unwrap();
        clear_snapshot().unwrap();
        assert!(following().is_none());
    }

    #[test]
    fn the_startup_block_and_unit_name_the_binary() {
        let b = Path::new("/opt/x/custard-watch");
        assert!(startup_block(b).contains("hl.exec_cmd(\"/opt/x/custard-watch\")"));
        assert!(startup_block(b).contains("hl.on(\"hyprland.start\""));
        assert!(unit_text(b).contains("ExecStart=/opt/x/custard-watch\n"));
        assert!(unit_text(b).contains("WantedBy=graphical-session.target"));
    }
}
