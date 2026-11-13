//! custard-watch: keeps the desktop's colors matching the wallpaper.
//!
//! Started at login when "Follow the wallpaper" is on in hyprcustard's
//! Colors page, and stopped when it is turned off. It does nothing on a
//! timer that it does not have to: it sleeps until one of these wakes it —
//!
//! * Hyprland's event socket: a wallpaper program starting, stopping or
//!   restarting puts up or takes down a background layer, and monitors come
//!   and go. Fullscreen and workspace events matter only while a change is
//!   waiting.
//! * inotify, on the folders the wallpaper comes from: every wallpaper
//!   program has to open a file to show it, so a new clip or image wakes it.
//! * The snapshot of what to follow changing, or being removed (stop).
//! * A slow fallback check, for anything the two above miss.
//!
//! When the wallpaper has changed it builds a scheme exactly as the Colors
//! page does and applies it — unless something is fullscreen, in which case
//! it waits until nothing is, or the last change was too recent, in which
//! case it waits out the minimum gap.

use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use custard_core::colors::follow::{self, Follower, Next, Status};
use custard_core::colors::{self, named, targets, wallpaper, ColorSettings};
use custard_core::settings::Settings;
use custard_core::{hyprctl, paths};
use inotify::{Inotify, WatchDescriptor, WatchMask};

/// What wakes the main loop.
enum Wake {
    /// A Hyprland event that could mean a new wallpaper.
    Wallpaper,
    /// A Hyprland event that could end a fullscreen.
    Screen,
    /// A file in a wallpaper folder was opened.
    Opened,
    /// The snapshot of what to follow changed.
    Snapshot,
    /// Hyprland went away.
    Gone,
}

const HELP: &str = "\
custard-watch — keeps the desktop's colors matching the wallpaper

Turned on and off from hyprcustard's Colors page (Follow the wallpaper),
which also starts it at login. It follows the choices saved in
~/.config/hyprcustard/colors/follow.toml, and exits when that file goes.

Options:
  --poll <seconds>   how often to look again with no event (default 120)
  --version          print the version
  --help             this
";

fn main() {
    let mut poll = Duration::from_secs(120);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--help" | "-h" => {
                print!("{HELP}");
                return;
            }
            "--version" => {
                println!("custard-watch {}", custard_core::VERSION);
                return;
            }
            "--poll" => match args.next().and_then(|v| v.parse::<u64>().ok()) {
                Some(s) if s > 0 => poll = Duration::from_secs(s),
                _ => fail("--poll takes a number of seconds"),
            },
            other => fail(&format!("unknown option {other}; see --help")),
        }
    }

    if !follow::running_pids().is_empty() {
        eprintln!("custard-watch is already running");
        return;
    }
    let Some(mut snap) = follow::following() else {
        eprintln!(
            "not following the wallpaper (no {}); nothing to do",
            paths::contract(&follow::snapshot_path())
        );
        return;
    };
    let socket = find_session().unwrap_or_else(|| fail("no Hyprland session to watch"));

    let (tx, rx) = mpsc::channel();
    listen_to_hyprland(socket, tx.clone());
    let inotify = Inotify::init().unwrap_or_else(|e| fail(&format!("inotify: {e}")));
    let mut watches = inotify.watches();
    let snapshot_dir = follow::snapshot_path().parent().map(Path::to_path_buf).unwrap_or_default();
    let snapshot_wd = watches
        .add(&snapshot_dir, WatchMask::CLOSE_WRITE | WatchMask::MOVED_TO | WatchMask::DELETE)
        .unwrap_or_else(|e| fail(&format!("cannot watch {}: {e}", snapshot_dir.display())));
    listen_to_files(inotify, snapshot_wd, tx.clone());

    let mut follower = Follower::new(minutes(snap.min_minutes));
    let mut folders: Vec<(PathBuf, WatchDescriptor)> = Vec::new();
    let mut status =
        Status { pid: std::process::id(), state: "watching".into(), ..Status::default() };
    let mut look = true;
    // Set when follow.toml changed: read it again before anything else. A
    // wake that says so is never dropped, whichever wait it arrives in.
    let mut reread = false;

    loop {
        if reread {
            reread = false;
            match follow::following() {
                Some(s) => {
                    follower.min_gap = minutes(s.min_minutes);
                    snap = s;
                    look = true;
                }
                None => {
                    eprintln!("no longer following the wallpaper; exiting");
                    let _ = std::fs::remove_file(follow::status_path());
                    return;
                }
            }
        }
        if look {
            let found = wallpaper::detect();
            let w = found.for_monitor(&snap.monitor);
            follower.saw(w.map(|w| follow::key(w, snap.whole_folder)));
            // Watch where the wallpaper comes from, and nowhere else.
            let want: Vec<PathBuf> = w.map(watched_folders).unwrap_or_default();
            folders.retain(|(dir, wd)| {
                let keep = want.contains(dir);
                if !keep {
                    let _ = watches.remove(wd.clone());
                }
                keep
            });
            for dir in want {
                if !folders.iter().any(|(d, _)| *d == dir) {
                    if let Ok(wd) = watches.add(&dir, WatchMask::OPEN) {
                        folders.push((dir, wd));
                    }
                }
            }
            look = false;
        }

        let fullscreen = follower.pending() && hyprctl::fullscreen_on_screen();
        let wait = match follower.decide(Instant::now(), fullscreen) {
            Next::Apply => {
                let changed = apply(&snap, &mut status);
                follower.done(Instant::now(), changed);
                // ffmpeg just opened the wallpaper itself; those opens are
                // not news. Anything else that came in meanwhile still is.
                let d = drain(rx.try_iter());
                if d.gone {
                    return quit();
                }
                look |= d.wallpaper;
                reread |= d.snapshot;
                continue;
            }
            Next::Paused => {
                report(&mut status, "paused", "Waiting for nothing to be fullscreen");
                // Fullscreen events wake it sooner; this is the backstop.
                Duration::from_secs(30)
            }
            Next::Wait(d) => {
                report(
                    &mut status,
                    "waiting",
                    &format!("Waiting {} before changing colors again", span(d)),
                );
                d
            }
            Next::Idle => {
                report(&mut status, "watching", "Watching the wallpaper");
                poll
            }
        };

        match rx.recv_timeout(wait) {
            Ok(Wake::Wallpaper) => look = true,
            Ok(Wake::Screen) => {}
            Ok(Wake::Opened) => {
                // A program opening a file tends to come in bursts: let it
                // settle, then look once.
                std::thread::sleep(Duration::from_millis(500));
                let d = drain(rx.try_iter());
                if d.gone {
                    return quit();
                }
                reread |= d.snapshot;
                look = true;
            }
            Ok(Wake::Snapshot) => reread = true,
            Ok(Wake::Gone) | Err(RecvTimeoutError::Disconnected) => return quit(),
            Err(RecvTimeoutError::Timeout) => look = true,
        }
    }
}

/// What a burst of wakes, taken all at once, comes to. File opens and
/// screen changes are dropped — the caller is about to look anyway — but a
/// changed snapshot or an exit never is.
#[derive(Debug, Default, PartialEq)]
struct Drained {
    wallpaper: bool,
    snapshot: bool,
    gone: bool,
}

fn drain(wakes: impl Iterator<Item = Wake>) -> Drained {
    let mut d = Drained::default();
    for w in wakes {
        match w {
            Wake::Opened | Wake::Screen => {}
            Wake::Wallpaper => d.wallpaper = true,
            Wake::Snapshot => d.snapshot = true,
            Wake::Gone => d.gone = true,
        }
    }
    d
}

/// Leave, taking the status file along: it describes a watcher that is no
/// longer there.
fn quit() {
    let _ = std::fs::remove_file(follow::status_path());
}

/// Build the scheme for the wallpaper as it is now and apply it. Returns
/// whether the colors changed.
fn apply(snap: &ColorSettings, status: &mut Status) -> bool {
    let (palettes, _) = named::load_all();
    let built = match colors::build(snap, &palettes) {
        Ok(b) => b,
        Err(e) => {
            report(status, "error", &e.to_string());
            return false;
        }
    };
    if colors::current().as_ref() == Some(&built.scheme) {
        report(status, "watching", "Watching the wallpaper");
        return false;
    }
    let (settings, _) = Settings::load();
    let list = targets::all(&paths::apps_config_home(), &settings.resolved_config_path());
    match colors::apply(&built.scheme, snap, &list) {
        Ok(done) => {
            status.last_applied = Some(built.scheme.name.clone());
            status.at = Some(chrono::Local::now().format("%H:%M").to_string());
            status.state = String::new();
            report(status, "watching", &done.summary(&built.scheme.name));
            true
        }
        Err(e) => {
            report(status, "error", &e.to_string());
            false
        }
    }
}

/// The folders whose files the wallpaper program opens.
fn watched_folders(w: &wallpaper::Wallpaper) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for p in [Some(w.path.as_path()), w.showing.as_deref()].into_iter().flatten() {
        let dir =
            if p.is_dir() { Some(p.to_path_buf()) } else { p.parent().map(Path::to_path_buf) };
        if let Some(d) = dir {
            if !out.contains(&d) {
                out.push(d);
            }
        }
    }
    out
}

/// Write the status if it changed.
fn report(status: &mut Status, state: &str, detail: &str) {
    if status.state == state && status.detail == detail {
        return;
    }
    status.state = state.into();
    status.detail = detail.into();
    follow::write_status(status);
}

/// Find this session's Hyprland, filling in what a process started outside
/// it (a systemd service, say) may not have been given, and return its
/// event socket.
fn find_session() -> Option<PathBuf> {
    let runtime = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?);
    let sig = match std::env::var("HYPRLAND_INSTANCE_SIGNATURE") {
        Ok(s) if !s.is_empty() => s,
        _ => {
            // The most recently started instance.
            let newest = std::fs::read_dir(runtime.join("hypr"))
                .ok()?
                .filter_map(|e| e.ok())
                .filter(|e| e.path().join(".socket2.sock").exists())
                .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())?;
            let s = newest.file_name().to_string_lossy().into_owned();
            std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", &s);
            s
        }
    };
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        if let Some(i) = hyprctl::instances().ok()?.into_iter().find(|i| i.instance == sig) {
            std::env::set_var("WAYLAND_DISPLAY", i.wl_socket);
        }
    }
    let socket = runtime.join("hypr").join(&sig).join(".socket2.sock");
    socket.exists().then_some(socket)
}

/// Forward the Hyprland events that matter, on a thread of their own.
fn listen_to_hyprland(socket: PathBuf, tx: Sender<Wake>) {
    let stream = UnixStream::connect(&socket)
        .unwrap_or_else(|e| fail(&format!("cannot connect to {}: {e}", socket.display())));
    std::thread::spawn(move || {
        for line in BufReader::new(stream).lines() {
            let Ok(line) = line else { break };
            let name = line.split(">>").next().unwrap_or("");
            let wake = match name {
                "openlayer" | "closelayer" | "monitoradded" | "monitoraddedv2"
                | "monitorremoved" | "monitorremovedv2" | "configreloaded" => Wake::Wallpaper,
                "fullscreen" | "workspace" | "workspacev2" | "focusedmon" | "focusedmonv2"
                | "closewindow" | "activespecial" | "activespecialv2" => Wake::Screen,
                _ => continue,
            };
            if tx.send(wake).is_err() {
                return;
            }
        }
        let _ = tx.send(Wake::Gone);
    });
}

/// Forward file events, on a thread of their own.
fn listen_to_files(mut inotify: Inotify, snapshot: WatchDescriptor, tx: Sender<Wake>) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            let Ok(events) = inotify.read_events_blocking(&mut buf) else { return };
            for e in events {
                let wake = if e.wd == snapshot {
                    match e.name.and_then(|n| n.to_str()) {
                        Some("follow.toml") => Wake::Snapshot,
                        _ => continue,
                    }
                } else {
                    Wake::Opened
                };
                if tx.send(wake).is_err() {
                    return;
                }
            }
        }
    });
}

fn minutes(m: u32) -> Duration {
    Duration::from_secs(u64::from(m) * 60)
}

/// `4 minutes`, `30 seconds`.
fn span(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 90 {
        format!("{} minutes", s.div_ceil(60))
    } else {
        format!("{s} seconds")
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("custard-watch: {msg}");
    std::process::exit(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_keeps_a_changed_snapshot_and_an_exit() {
        let d = drain([Wake::Opened, Wake::Snapshot, Wake::Opened, Wake::Screen].into_iter());
        assert_eq!(d, Drained { snapshot: true, ..Drained::default() });
        let d = drain([Wake::Wallpaper, Wake::Gone].into_iter());
        assert!(d.wallpaper && d.gone);
        assert_eq!(drain([Wake::Opened].into_iter()), Drained::default());
    }
}
