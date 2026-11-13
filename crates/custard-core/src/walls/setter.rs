//! Putting a wallpaper on screen with the program the user has.
//!
//! Nothing here replaces the user's wallpaper program; it drives it. Programs
//! with a way to be told (awww, swww, hyprpaper) are told. The ones without
//! (swaybg, mpvpaper) are started again with the new file, keeping every
//! other option the user gave them, and the old copy is stopped only once
//! the new one is up — so a mistake leaves the old wallpaper where it was.
//!
//! Working out what to do ([`plan`]) is separate from doing it ([`run`]), so
//! every program's steps are tested without touching anyone's desktop.

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use super::MpvChoices;
use crate::colors::wallpaper::{mpvpaper_args, mpvpaper_positions, program, resolve, swaybg_args};
use crate::error::{Error, Result};

/// A wallpaper program this can drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Program {
    /// Its name, as the page and the settings know it.
    pub id: &'static str,
    /// The command that is run to show a wallpaper.
    pub binary: &'static str,
    /// The long-running process that draws it.
    pub process: &'static str,
    /// Whether it plays videos.
    pub videos: bool,
    /// Whether it plays a whole folder.
    pub folders: bool,
    /// Whether it reads GIFs, animated or as a still.
    pub gifs: bool,
}

/// Every program this can drive, in the order one is picked when the user
/// has none running: the light, image-only ones first.
pub const PROGRAMS: [Program; 5] = [
    Program {
        id: "awww",
        binary: "awww",
        process: "awww-daemon",
        videos: false,
        folders: false,
        gifs: true,
    },
    Program {
        id: "swww",
        binary: "swww",
        process: "swww-daemon",
        videos: false,
        folders: false,
        gifs: true,
    },
    // Its image library has no GIF reader.
    Program {
        id: "hyprpaper",
        binary: "hyprpaper",
        process: "hyprpaper",
        videos: false,
        folders: false,
        gifs: false,
    },
    // gdk-pixbuf shows a GIF's first frame.
    Program {
        id: "swaybg",
        binary: "swaybg",
        process: "swaybg",
        videos: false,
        folders: false,
        gifs: true,
    },
    Program {
        id: "mpvpaper",
        binary: "mpvpaper",
        process: "mpvpaper",
        videos: true,
        folders: true,
        gifs: true,
    },
];

/// Wallpaper processes this cannot drive but stops when another program
/// takes over the desktop, so two do not draw at once.
const OTHERS: [&str; 1] = ["wpaperd"];

/// The program called `id`.
pub fn get(id: &str) -> Option<Program> {
    PROGRAMS.iter().copied().find(|p| p.id == id)
}

/// Whether `p` is installed.
pub fn installed(p: &Program) -> bool {
    crate::paths::which(p.binary).is_some() && crate::paths::which(p.process).is_some()
}

/// Refuse a program too old to drive the way [`plan`] does. Only
/// hyprpaper has such a line: 0.8 changed its IPC (no more `preload`, a fit
/// mode in `wallpaper`) and its config (`wallpaper { ... }` blocks).
pub fn check_version(p: &Program) -> Result<()> {
    if p.id != "hyprpaper" || cfg!(test) {
        return Ok(());
    }
    let out = Command::new("hyprpaper")
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    match parse_version(&out) {
        Some(v) if v < (0, 8) => Err(Error::other(format!(
            "hyprcustard drives hyprpaper 0.8 or newer, and this is {}.{}. Update hyprpaper, \
             or choose another program under \"Show it with\"",
            v.0, v.1
        ))),
        // Unreadable: assume it is new, as every current one is.
        _ => Ok(()),
    }
}

/// `(major, minor)` from the first `v1.2.3`-looking word.
fn parse_version(text: &str) -> Option<(u32, u32)> {
    text.split_whitespace().find_map(|w| {
        let mut parts = w.trim_start_matches('v').split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        Some((major, minor))
    })
}

/// Which of [`PROGRAMS`] are running, among `procs`.
pub fn running(procs: &[(u32, Vec<String>)]) -> Vec<&'static str> {
    PROGRAMS
        .iter()
        .filter(|p| procs.iter().any(|(_, argv)| program(argv) == p.process))
        .map(|p| p.id)
        .collect()
}

/// What the user asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// The program to show it with.
    pub program: Program,
    /// One monitor, or `None` for all of them.
    pub monitor: Option<String>,
    /// A file, or a folder for a program that plays one.
    pub path: PathBuf,
    /// `fill`, `fit` or `stretch`.
    pub fit: String,
    /// Every monitor, in Hyprland's order.
    pub monitors: Vec<String>,
    /// hyprcustard's own hyprpaper config, used when it starts hyprpaper.
    pub hyprpaper_conf: PathBuf,
    /// The user's own mpvpaper command, seen running earlier or read from
    /// their login line, for when no mpvpaper is running to copy it from.
    /// Empty when there is none.
    pub mpvpaper: Vec<String>,
    /// The page's choices for mpvpaper, laid over the command it uses.
    pub mpv: MpvChoices,
}

/// The user's own mpvpaper command among `procs`: the first that is not one
/// hyprcustard started with its own options.
pub fn users_mpvpaper(procs: &[(u32, Vec<String>)]) -> Option<Vec<String>> {
    procs
        .iter()
        .map(|(_, argv)| argv)
        .find(|argv| program(argv) == "mpvpaper" && mpvpaper_args(argv).is_some() && !ours(argv))
        .cloned()
}

/// One thing to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// Start a program that keeps running.
    Start(Vec<String>),
    /// Run a command and wait for it; it failing is the request failing.
    Run(Vec<String>),
    /// Run a command until it succeeds, for up to three seconds: a daemon
    /// that was just started getting ready.
    Await(Vec<String>),
    /// Write a file.
    Write(PathBuf, String),
    /// Wait this many milliseconds, then make sure what was started is still
    /// running.
    Settle(u64),
    /// Stop these processes.
    Stop(Vec<u32>),
}

/// The steps to show `req`, given this session's processes.
///
/// When another wallpaper program is running, every monitor moves to the
/// one asked for and the other is stopped afterwards.
pub fn plan(req: &Request, procs: &[(u32, Vec<String>)]) -> Result<Vec<Step>> {
    let p = req.program;
    let name = p.id;
    if !req.path.exists() {
        return Err(Error::other(format!("{} is not there", crate::paths::contract(&req.path))));
    }
    if req.path.is_dir() && !p.folders {
        return Err(Error::other(format!(
            "{name} shows one file at a time; only mpvpaper plays a whole folder"
        )));
    }
    if super::library::gif(&req.path) && !p.gifs {
        return Err(Error::other(format!(
            "{name} cannot read GIFs. Choose another program under \"Show it with\""
        )));
    }
    if super::library::video(&req.path) && !p.videos {
        return Err(Error::other(format!(
            "{name} shows images, not videos. Choose mpvpaper under \"Show it with\" to play one"
        )));
    }

    let others: Vec<u32> = procs
        .iter()
        .filter(|(_, argv)| {
            let prog = program(argv);
            prog != p.process
                && (PROGRAMS.iter().any(|q| q.process == prog) || OTHERS.contains(&prog))
        })
        .map(|(pid, _)| *pid)
        .collect();
    let mine: Vec<(u32, &Vec<String>)> = procs
        .iter()
        .filter(|(_, argv)| program(argv) == p.process)
        .map(|(pid, argv)| (*pid, argv))
        .collect();
    // Taking over from another program takes over every monitor.
    let monitor = if others.is_empty() { req.monitor.clone() } else { None };
    let path = req.path.to_string_lossy().into_owned();

    let mut steps = match p.id {
        "awww" | "swww" => {
            let mut s = Vec::new();
            if mine.is_empty() {
                s.push(Step::Start(vec![p.process.into()]));
                s.push(Step::Await(vec![p.binary.into(), "query".into()]));
            }
            let resize = match req.fit.as_str() {
                "fit" => "fit",
                "stretch" => "stretch",
                _ => "crop",
            };
            let mut cmd: Vec<String> =
                vec![p.binary.into(), "img".into(), path, "--resize".into(), resize.into()];
            if let Some(m) = &monitor {
                cmd.extend(["-o".into(), m.clone()]);
            }
            s.push(Step::Run(cmd));
            s
        }
        "hyprpaper" => {
            let fit = hyprpaper_fit(&req.fit);
            let mon = monitor.clone().unwrap_or_default();
            if mine.is_empty() {
                // Started with a config of hyprcustard's own that already
                // names the file, so it does not show another first.
                vec![
                    Step::Write(
                        req.hyprpaper_conf.clone(),
                        hyprpaper_conf_text(&[(monitor.clone(), req.path.clone())], &req.fit),
                    ),
                    Step::Start(vec![
                        "hyprpaper".into(),
                        "-c".into(),
                        req.hyprpaper_conf.to_string_lossy().into_owned(),
                    ]),
                    Step::Await(vec!["hyprctl".into(), "hyprpaper".into(), "listactive".into()]),
                ]
            } else {
                vec![Step::Run(vec![
                    "hyprctl".into(),
                    "hyprpaper".into(),
                    "wallpaper".into(),
                    format!("{mon},{path},{fit}"),
                ])]
            }
        }
        "swaybg" => {
            let mut entries: Vec<(Option<String>, String)> = Vec::new();
            for (pid, argv) in &mine {
                for w in swaybg_args(argv) {
                    let path = resolve(&w.path, &PathBuf::from(format!("/proc/{pid}")));
                    entries.push((w.monitor, path.to_string_lossy().into_owned()));
                }
            }
            let entries = per_monitor(entries, monitor.as_deref(), &path, &req.monitors);
            let mode = match req.fit.as_str() {
                "fit" => "fit",
                "stretch" => "stretch",
                _ => "fill",
            };
            let mut argv: Vec<String> = vec!["swaybg".into()];
            for (m, image) in entries {
                argv.extend(["-o".into(), m.unwrap_or_else(|| "*".into())]);
                argv.extend(["-i".into(), image, "-m".into(), mode.into()]);
            }
            let mut s = vec![Step::Start(argv)];
            if !mine.is_empty() {
                s.push(Step::Settle(500));
                s.push(Step::Stop(mine.iter().map(|(pid, _)| *pid).collect()));
            }
            s
        }
        "mpvpaper" => {
            let template = monitor
                .as_ref()
                .and_then(|m| {
                    mine.iter().find(|(_, a)| {
                        mpvpaper_args(a).and_then(|w| w.monitor).as_deref() == Some(m)
                    })
                })
                .or(mine.first())
                .map(|(_, a)| (*a).clone())
                // Options hyprcustard chose itself are chosen again for this
                // pick: a file's `loop` would keep a folder on its first clip.
                .filter(|a| !ours(a))
                // Another program showing wallpapers since does not lose
                // the user's options.
                .or_else(|| {
                    let theirs = &req.mpvpaper;
                    (mpvpaper_args(theirs).is_some() && !ours(theirs)).then(|| theirs.clone())
                })
                .unwrap_or_else(|| mpvpaper_default(&req.fit, req.path.is_dir()));
            let template = with_choices(&template, &req.mpv, req.path.is_dir());
            let mut s = Vec::new();
            let mut stop = Vec::new();
            match &monitor {
                None => {
                    s.push(Step::Start(mpvpaper_with(&template, "ALL", &path)));
                    stop.extend(mine.iter().map(|(pid, _)| *pid));
                }
                Some(m) => {
                    for (pid, argv) in &mine {
                        let Some(w) = mpvpaper_args(argv) else { continue };
                        match w.monitor.as_deref() {
                            Some(x) if x == m => stop.push(*pid),
                            Some(_) => {}
                            // One process on every monitor: the others keep
                            // what it plays, each in a process of its own.
                            None => {
                                let old = resolve(&w.path, &PathBuf::from(format!("/proc/{pid}")));
                                let old = old.to_string_lossy().into_owned();
                                for other in req.monitors.iter().filter(|o| *o != m) {
                                    s.push(Step::Start(mpvpaper_with(argv, other, &old)));
                                }
                                stop.push(*pid);
                            }
                        }
                    }
                    s.push(Step::Start(mpvpaper_with(&template, m, &path)));
                }
            }
            if !stop.is_empty() {
                // mpv takes a moment to open a large video.
                s.push(Step::Settle(800));
                s.push(Step::Stop(stop));
            }
            s
        }
        other => return Err(Error::other(format!("hyprcustard cannot drive {other}"))),
    };
    if !others.is_empty() {
        if !matches!(steps.last(), Some(Step::Settle(_)) | Some(Step::Stop(_))) {
            steps.push(Step::Settle(400));
        }
        steps.push(Step::Stop(others));
    }
    Ok(steps)
}

/// The wallpapers on each monitor after `monitor` gets `path`: one entry for
/// every monitor when it is all of them, otherwise the others kept as they
/// were — and an "every monitor" entry split up so it can keep them.
fn per_monitor(
    entries: Vec<(Option<String>, String)>,
    monitor: Option<&str>,
    path: &str,
    monitors: &[String],
) -> Vec<(Option<String>, String)> {
    let Some(m) = monitor else { return vec![(None, path.to_string())] };
    let mut out: Vec<(Option<String>, String)> = Vec::new();
    for (mon, image) in entries {
        match mon {
            Some(x) if x == m => {}
            Some(x) => out.push((Some(x), image)),
            None => {
                for o in monitors.iter().filter(|o| *o != m) {
                    if !out.iter().any(|(x, _)| x.as_deref() == Some(o)) {
                        out.push((Some(o.clone()), image.clone()));
                    }
                }
            }
        }
    }
    out.push((Some(m.to_string()), path.to_string()));
    out
}

/// An mpvpaper command line with its output and path replaced, and every
/// option kept.
pub fn mpvpaper_with(argv: &[String], output: &str, path: &str) -> Vec<String> {
    let mut out = argv.to_vec();
    let pos = mpvpaper_positions(argv);
    match pos.as_slice() {
        [o, p, ..] => {
            out[*o] = output.to_string();
            out[*p] = path.to_string();
        }
        _ => {
            out.truncate(1);
            out.extend([output.to_string(), path.to_string()]);
        }
    }
    out
}

/// mpvpaper's command for someone who has not run it before: paused while a
/// window covers it, silent, and filling the screen as asked. A file loops;
/// a folder plays through and starts again, each image staying five
/// minutes (mpv's default is one second).
fn mpvpaper_default(fit: &str, folder: bool) -> Vec<String> {
    let mut opts = String::from(if folder {
        "no-audio loop-playlist image-display-duration=300"
    } else {
        "no-audio loop"
    });
    match fit {
        "fit" => {}
        "stretch" => opts.push_str(" keepaspect=no"),
        _ => opts.push_str(" panscan=1.0"),
    }
    ["mpvpaper", "-p", "-o", &opts, "ALL", ""].map(String::from).to_vec()
}

/// Whether an mpvpaper command line has the options [`mpvpaper_default`]
/// gives, for any fit and either kind of pick.
/// The page's own choices ([`with_choices`]) are left out of the
/// comparison, so one started with them is still hyprcustard's.
fn ours(argv: &[String]) -> bool {
    let argv = with_shuffle(&without_slideshow(argv), false);
    ["fill", "fit", "stretch"].iter().any(|fit| {
        [false, true].iter().any(|folder| {
            let d = mpvpaper_default(fit, *folder);
            argv.len() == d.len() && argv[1..4] == d[1..4]
        })
    })
}

/// The command mpvpaper would be started with for a folder now: the
/// user's own running, else the one remembered, else hyprcustard's — with
/// the page's choices laid over it. For the page to show.
pub fn mpvpaper_for_folder(
    procs: &[(u32, Vec<String>)],
    remembered: &[String],
    fit: &str,
    choices: &MpvChoices,
) -> Vec<String> {
    let template = users_mpvpaper(procs)
        .or_else(|| {
            (mpvpaper_args(remembered).is_some() && !ours(remembered)).then(|| remembered.to_vec())
        })
        .unwrap_or_else(|| mpvpaper_default(fit, true));
    with_choices(&template, choices, true)
}

/// The page's choices laid over an mpvpaper command: the slideshow timer
/// and shuffling, which only mean anything for a folder.
pub fn with_choices(argv: &[String], c: &MpvChoices, folder: bool) -> Vec<String> {
    let mut out = argv.to_vec();
    if !folder {
        return out;
    }
    if let Some(secs) = c.change_every {
        out = without_slideshow(&out);
        if secs > 0 {
            if let Some(&at) = mpvpaper_positions(&out).first() {
                out.splice(at..at, ["-n".to_string(), secs.to_string()]);
            }
        }
    }
    if let Some(on) = c.shuffle {
        out = with_shuffle(&out, on);
    }
    out
}

/// The seconds mpvpaper's slideshow moves on after, if it has one.
pub fn slideshow(argv: &[String]) -> Option<u32> {
    let pos = mpvpaper_positions(argv);
    let mut i = 1;
    while i < argv.len() {
        let a = argv[i].as_str();
        if !pos.contains(&i) {
            if let Some(v) = a.strip_prefix("--slideshow=") {
                return v.parse().ok();
            }
            if a == "--slideshow"
                || (!a.starts_with("--") && a.starts_with('-') && a.ends_with('n'))
            {
                return argv.get(i + 1).and_then(|v| v.parse().ok());
            }
        }
        i += 1;
    }
    None
}

/// Whether mpv is asked to shuffle.
pub fn shuffled(argv: &[String]) -> bool {
    mpv_options_at(argv)
        .map(|i| argv[i].split_whitespace().any(|t| is_shuffle(t) && !t.ends_with("=no")))
        .unwrap_or(false)
}

/// The command without its slideshow option, every other argument kept.
fn without_slideshow(argv: &[String]) -> Vec<String> {
    let pos = mpvpaper_positions(argv);
    let mut out = vec![argv[0].clone()];
    let mut i = 1;
    while i < argv.len() {
        let a = &argv[i];
        if pos.contains(&i) {
            out.push(a.clone());
        } else if a == "-n" || a == "--slideshow" {
            i += 1;
        } else if a.starts_with("--slideshow=") {
        } else if !a.starts_with("--") && a.len() > 2 && a.starts_with('-') && a.ends_with('n') {
            // `-pn 30`: the rest of the bundle stays.
            out.push(a[..a.len() - 1].to_string());
            i += 1;
        } else {
            out.push(a.clone());
        }
        i += 1;
    }
    out
}

/// The command with mpv's `shuffle` added to or taken out of its options.
fn with_shuffle(argv: &[String], on: bool) -> Vec<String> {
    let mut out = argv.to_vec();
    match mpv_options_at(&out) {
        Some(i) => {
            let mut tokens: Vec<&str> =
                out[i].split_whitespace().filter(|t| !is_shuffle(t)).collect();
            if on {
                tokens.push("shuffle");
            }
            out[i] = tokens.join(" ");
        }
        None if on => {
            if let Some(&at) = mpvpaper_positions(&out).first() {
                out.splice(at..at, ["-o".to_string(), "shuffle".to_string()]);
            }
        }
        None => {}
    }
    out
}

fn is_shuffle(token: &str) -> bool {
    let t = token.trim_start_matches('-');
    t == "shuffle" || t.starts_with("shuffle=")
}

/// The index of the string mpvpaper passes on to mpv (`-o "..."`).
fn mpv_options_at(argv: &[String]) -> Option<usize> {
    let pos = mpvpaper_positions(argv);
    (1..argv.len()).find_map(|i| {
        let a = argv[i].as_str();
        let takes = a == "--mpv-options"
            || (!a.starts_with("--") && a.starts_with('-') && a.ends_with('o'));
        (!pos.contains(&i) && takes && i + 1 < argv.len()).then_some(i + 1)
    })
}

fn hyprpaper_fit(fit: &str) -> &'static str {
    match fit {
        "fit" => "contain",
        "stretch" => "fill",
        _ => "cover",
    }
}

/// A hyprpaper config showing `wallpapers`, in hyprpaper 0.8's shape.
pub fn hyprpaper_conf_text(wallpapers: &[(Option<String>, PathBuf)], fit: &str) -> String {
    let mut s = String::from(
        "# Written by hyprcustard for the hyprpaper it starts at login. It is\n\
         # rewritten whenever you pick a wallpaper on hyprcustard's Wallpaper page.\n\
         splash = false\n",
    );
    for (m, p) in wallpapers {
        s.push_str(&format!(
            "\nwallpaper {{\n    monitor = {}\n    path = {}\n    fit_mode = {}\n}}\n",
            m.as_deref().unwrap_or(""),
            p.display(),
            hyprpaper_fit(fit)
        ));
    }
    s
}

/// Carry out `steps`. If something fails before the old wallpaper is
/// stopped, it is left running.
pub fn run(steps: &[Step]) -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    let mut started: Vec<(String, Child)> = Vec::new();
    let check = |started: &mut Vec<(String, Child)>| -> Result<()> {
        for (name, child) in started.iter_mut() {
            if let Ok(Some(status)) = child.try_wait() {
                return Err(Error::other(format!(
                    "{name} stopped as soon as it started ({status}), so the wallpaper \
                     was left as it was"
                )));
            }
        }
        Ok(())
    };
    let result = (|| {
        for step in steps {
            match step {
                Step::Start(argv) => {
                    let child = Command::new(&argv[0])
                        .args(&argv[1..])
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        // Its own process group, so a Ctrl-C meant for
                        // hyprcustard in a terminal does not reach it.
                        .process_group(0)
                        .spawn()
                        .map_err(|e| Error::other(format!("could not start {}: {e}", argv[0])))?;
                    started.push((argv[0].clone(), child));
                }
                Step::Run(argv) => {
                    let out = command(argv)?;
                    if !out.0 {
                        return Err(Error::other(format!("{} said: {}", argv[0], out.1)));
                    }
                }
                Step::Await(argv) => {
                    let deadline = Instant::now() + Duration::from_secs(3);
                    loop {
                        if command(argv).map(|o| o.0).unwrap_or(false) {
                            break;
                        }
                        check(&mut started)?;
                        if Instant::now() > deadline {
                            return Err(Error::other(format!(
                                "{} did not get ready in time",
                                argv[0]
                            )));
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
                Step::Write(path, text) => {
                    if let Some(dir) = path.parent() {
                        std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
                    }
                    crate::paths::write_atomic(path, text)?;
                }
                Step::Settle(ms) => {
                    std::thread::sleep(Duration::from_millis(*ms));
                    check(&mut started)?;
                }
                Step::Stop(pids) => {
                    for pid in pids {
                        let _ = Command::new("kill")
                            .args(["-TERM", &pid.to_string()])
                            .stdin(Stdio::null())
                            .stderr(Stdio::null())
                            .status();
                    }
                }
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            // Reap them if they exit while the app is open.
            std::thread::spawn(move || {
                for (_, mut c) in started {
                    let _ = c.wait();
                }
            });
            Ok(())
        }
        Err(e) => {
            for (_, mut c) in started {
                let _ = c.kill();
                let _ = c.wait();
            }
            Err(e)
        }
    }
}

/// Run a command; whether it succeeded, and what it printed.
fn command(argv: &[String]) -> Result<(bool, String)> {
    let out = Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| Error::other(format!("could not run {}: {e}", argv[0])))?;
    let text =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    let text = text.trim().trim_start_matches("error:").trim().to_string();
    // hyprctl reports a refused hyprpaper request on stdout, with an
    // `error:` prefix.
    let ok = out.status.success() && !String::from_utf8_lossy(&out.stdout).starts_with("error");
    Ok((ok, text.lines().next().unwrap_or("").to_string()))
}

/// Whether a path is something `p` can show.
pub fn can_show(p: &Program, path: &Path) -> bool {
    if path.is_dir() {
        p.folders
    } else {
        (p.videos || !super::library::video(path)) && (p.gifs || !super::library::gif(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(s: &str) -> Vec<String> {
        s.split(' ').map(String::from).collect()
    }

    fn req(program: &str, monitor: Option<&str>, path: &Path) -> Request {
        Request {
            program: get(program).unwrap(),
            monitor: monitor.map(String::from),
            path: path.to_path_buf(),
            fit: "fill".into(),
            monitors: vec!["DP-1".into(), "HDMI-A-1".into()],
            hyprpaper_conf: PathBuf::from("/cfg/hyprpaper.conf"),
            mpvpaper: Vec::new(),
            mpv: MpvChoices::default(),
        }
    }

    fn files() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("new.png");
        let mp4 = dir.path().join("new.mp4");
        std::fs::write(&png, b"x").unwrap();
        std::fs::write(&mp4, b"x").unwrap();
        (dir, png, mp4)
    }

    #[test]
    fn refuses_what_a_program_cannot_show() {
        let (dir, png, mp4) = files();
        let e = plan(&req("swaybg", None, &mp4), &[]).unwrap_err().to_string();
        assert!(e.contains("mpvpaper"), "{e}");
        let e = plan(&req("awww", None, dir.path()), &[]).unwrap_err().to_string();
        assert!(e.contains("whole folder"), "{e}");
        let e = plan(&req("awww", None, &dir.path().join("gone.png")), &[]).unwrap_err();
        assert!(e.to_string().contains("not there"));
        assert!(plan(&req("mpvpaper", None, dir.path()), &[]).is_ok());
        assert!(plan(&req("hyprpaper", None, &png), &[]).is_ok());
    }

    #[test]
    fn awww_starts_its_daemon_only_when_needed() {
        let (_d, png, _) = files();
        let p = png.to_string_lossy().into_owned();
        let steps = plan(&req("awww", Some("DP-1"), &png), &[]).unwrap();
        assert_eq!(steps[0], Step::Start(argv("awww-daemon")));
        assert_eq!(steps[1], Step::Await(argv("awww query")));
        assert_eq!(
            steps[2],
            Step::Run(vec![
                "awww".into(),
                "img".into(),
                p.clone(),
                "--resize".into(),
                "crop".into(),
                "-o".into(),
                "DP-1".into()
            ])
        );
        let running = vec![(7, argv("awww-daemon"))];
        let steps = plan(&req("awww", None, &png), &running).unwrap();
        assert_eq!(steps.len(), 1);
    }

    #[test]
    fn hyprpaper_is_told_or_started_with_its_own_config() {
        let (_d, png, _) = files();
        let p = png.to_string_lossy().into_owned();
        let running = vec![(3, argv("hyprpaper"))];
        let steps = plan(&req("hyprpaper", Some("DP-1"), &png), &running).unwrap();
        assert_eq!(
            steps,
            vec![Step::Run(vec![
                "hyprctl".into(),
                "hyprpaper".into(),
                "wallpaper".into(),
                format!("DP-1,{p},cover")
            ])]
        );
        let steps = plan(&req("hyprpaper", None, &png), &[]).unwrap();
        let Step::Write(path, text) = &steps[0] else { panic!("{steps:?}") };
        assert_eq!(path, &PathBuf::from("/cfg/hyprpaper.conf"));
        assert!(text.contains(&format!("path = {p}")) && text.contains("monitor = \n"));
        assert_eq!(steps[1], Step::Start(argv("hyprpaper -c /cfg/hyprpaper.conf")));
    }

    #[test]
    fn swaybg_keeps_the_other_monitor_and_stops_the_old_copy_last() {
        let (_d, png, _) = files();
        let p = png.to_string_lossy().into_owned();
        let running = vec![(9, argv("swaybg -i /old.png -m fill"))];
        let steps = plan(&req("swaybg", Some("DP-1"), &png), &running).unwrap();
        let Step::Start(a) = &steps[0] else { panic!() };
        assert_eq!(
            a.join(" "),
            format!("swaybg -o HDMI-A-1 -i /old.png -m fill -o DP-1 -i {p} -m fill")
        );
        assert_eq!(steps[1], Step::Settle(500));
        assert_eq!(steps[2], Step::Stop(vec![9]));

        let steps = plan(&req("swaybg", None, &png), &running).unwrap();
        let Step::Start(a) = &steps[0] else { panic!() };
        assert_eq!(a.join(" "), format!("swaybg -o * -i {p} -m fill"));
    }

    #[test]
    fn mpvpaper_keeps_the_users_options() {
        let (dir, png, _) = files();
        let p = png.to_string_lossy().into_owned();
        let mut theirs = ["mpvpaper", "-p", "-a", "FULL", "-o"].map(String::from).to_vec();
        theirs.push("no-audio --loop-file shuffle".into());
        theirs.extend(["ALL".into(), "/w/clips/".into()]);
        let running = vec![(5, theirs.clone())];

        let steps = plan(&req("mpvpaper", None, &png), &running).unwrap();
        let Step::Start(a) = &steps[0] else { panic!() };
        assert_eq!(&a[..6], &theirs[..6]);
        assert_eq!(&a[6..], &["ALL".to_string(), p.clone()]);
        assert_eq!(steps[1], Step::Settle(800));
        assert_eq!(steps[2], Step::Stop(vec![5]));

        // One monitor: the other keeps the folder in a process of its own.
        let steps = plan(&req("mpvpaper", Some("DP-1"), &png), &running).unwrap();
        let starts: Vec<String> = steps
            .iter()
            .filter_map(|s| match s {
                Step::Start(a) => Some(a[6..].join(" ")),
                _ => None,
            })
            .collect();
        assert_eq!(starts, vec!["HDMI-A-1 /w/clips/".to_string(), format!("DP-1 {p}")]);
        assert_eq!(steps.last(), Some(&Step::Stop(vec![5])));

        // A folder plays as a folder.
        let d = dir.path().to_string_lossy().into_owned();
        let steps = plan(&req("mpvpaper", None, dir.path()), &running).unwrap();
        let Step::Start(a) = &steps[0] else { panic!() };
        assert_eq!(a.last(), Some(&d));

        // Nobody's options to keep: sensible ones.
        let steps = plan(&req("mpvpaper", None, &png), &[]).unwrap();
        let Step::Start(a) = &steps[0] else { panic!() };
        assert_eq!(a.join(" "), format!("mpvpaper -p -o no-audio loop panscan=1.0 ALL {p}"));
        assert_eq!(steps.len(), 1);
        // A folder plays through rather than repeating its first file —
        // also after a file hyprcustard started with its own options.
        let steps = plan(&req("mpvpaper", None, dir.path()), &[]).unwrap();
        let Step::Start(a) = &steps[0] else { panic!() };
        let folder_opts = "no-audio loop-playlist image-display-duration=300 panscan=1.0";
        assert_eq!(a[3], folder_opts);
        let ours_running = vec![(8, mpvpaper_with(&mpvpaper_default("fill", false), "ALL", &p))];
        let steps = plan(&req("mpvpaper", None, dir.path()), &ours_running).unwrap();
        let Step::Start(a) = &steps[0] else { panic!() };
        assert_eq!(a[3], folder_opts);
        // The user's own are kept, whatever they are.
        let steps = plan(&req("mpvpaper", None, dir.path()), &running).unwrap();
        let Step::Start(a) = &steps[0] else { panic!() };
        assert_eq!(a[5], "no-audio --loop-file shuffle");

        // With another program showing wallpapers now, the options
        // remembered from before are used, not hyprcustard's.
        assert_eq!(users_mpvpaper(&running), Some(theirs.clone()));
        assert_eq!(users_mpvpaper(&ours_running), None);
        let hyprpaper = vec![(9, argv("hyprpaper -c /cfg/hyprpaper.conf"))];
        let mut asked = req("mpvpaper", None, dir.path());
        asked.mpvpaper = theirs.clone();
        let steps = plan(&asked, &hyprpaper).unwrap();
        let Step::Start(a) = &steps[0] else { panic!() };
        assert_eq!(&a[..6], &theirs[..6]);
        assert_eq!(&a[6..], &["ALL".to_string(), d.clone()]);
        assert_eq!(steps.last(), Some(&Step::Stop(vec![9])));
        // Remembered options that are hyprcustard's own are not kept.
        asked.mpvpaper = mpvpaper_with(&mpvpaper_default("fill", false), "ALL", &p);
        let steps = plan(&asked, &hyprpaper).unwrap();
        let Step::Start(a) = &steps[0] else { panic!() };
        assert_eq!(a[3], folder_opts);
    }

    #[test]
    fn the_timer_and_shuffle_are_laid_over_any_command() {
        let v = |s: &[&str]| s.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        let theirs = v(&[
            "mpvpaper",
            "-p",
            "-a",
            "FULL",
            "-o",
            "no-audio --loop-file shuffle --video-zoom=.45",
            "ALL",
            "/w/1920",
        ]);
        assert_eq!(slideshow(&theirs), None);
        assert!(shuffled(&theirs));

        // Five minutes, in order: the timer goes in before the output,
        // shuffle comes out, and the rest is theirs.
        let c = MpvChoices { change_every: Some(300), shuffle: Some(false) };
        let got = with_choices(&theirs, &c, true);
        assert_eq!(
            got,
            v(&[
                "mpvpaper",
                "-p",
                "-a",
                "FULL",
                "-o",
                "no-audio --loop-file --video-zoom=.45",
                "-n",
                "300",
                "ALL",
                "/w/1920"
            ])
        );
        assert_eq!(slideshow(&got), Some(300));
        assert!(!shuffled(&got));
        assert_eq!(with_choices(&got, &c, true), got, "laid over twice, the same");
        // Off takes the timer out; None leaves the command alone.
        let off = with_choices(&got, &MpvChoices { change_every: Some(0), shuffle: None }, true);
        assert_eq!(slideshow(&off), None);
        assert_eq!(with_choices(&theirs, &MpvChoices::default(), true), theirs);
        // A single file is not a slideshow.
        assert_eq!(with_choices(&theirs, &c, false), theirs);

        // Bundled and long forms are read and taken out.
        let bundled = v(&["mpvpaper", "-pn", "45", "ALL", "/w"]);
        assert_eq!(slideshow(&bundled), Some(45));
        assert_eq!(without_slideshow(&bundled), v(&["mpvpaper", "-p", "ALL", "/w"]));
        let long = v(&["mpvpaper", "--slideshow=90", "ALL", "/w"]);
        assert_eq!(slideshow(&long), Some(90));
        assert_eq!(without_slideshow(&long), v(&["mpvpaper", "ALL", "/w"]));
        // No mpv options yet: shuffle brings its own.
        let bare = v(&["mpvpaper", "ALL", "/w"]);
        assert_eq!(with_shuffle(&bare, true), v(&["mpvpaper", "-o", "shuffle", "ALL", "/w"]));

        // hyprcustard's own stay its own with the choices laid over them.
        let mine = with_choices(&mpvpaper_default("fill", true), &c, true);
        let mine = with_choices(&mine, &MpvChoices { shuffle: Some(true), ..c }, true);
        assert!(ours(&mine));
        assert!(!ours(&got));
    }

    #[test]
    fn taking_over_from_another_program_covers_every_monitor_and_stops_it() {
        let (_d, png, _) = files();
        let running = vec![(5, argv("mpvpaper ALL /w/a.mp4")), (6, argv("wpaperd"))];
        let steps = plan(&req("awww", Some("DP-1"), &png), &running).unwrap();
        let Step::Run(cmd) = &steps[2] else { panic!("{steps:?}") };
        assert!(!cmd.contains(&"-o".to_string()), "every monitor: {cmd:?}");
        assert_eq!(steps[3], Step::Settle(400));
        assert_eq!(steps[4], Step::Stop(vec![5, 6]));
    }

    #[test]
    fn hyprpaper_versions() {
        assert_eq!(parse_version("hyprpaper v0.8.5\n"), Some((0, 8)));
        assert_eq!(parse_version("hyprpaper v0.7.6"), Some((0, 7)));
        assert_eq!(parse_version("hyprpaper 1.0"), Some((1, 0)));
        assert_eq!(parse_version("no idea"), None);
    }

    #[test]
    fn what_each_program_can_show() {
        let (dir, png, mp4) = files();
        let mpv = get("mpvpaper").unwrap();
        let bg = get("swaybg").unwrap();
        assert!(can_show(&mpv, &mp4) && can_show(&mpv, dir.path()) && can_show(&mpv, &png));
        assert!(can_show(&bg, &png) && !can_show(&bg, &mp4) && !can_show(&bg, dir.path()));

        // hyprpaper has no GIF reader; the others show one.
        let gif = dir.path().join("loop.GIF");
        std::fs::write(&gif, b"GIF89a").unwrap();
        let hp = get("hyprpaper").unwrap();
        assert!(!can_show(&hp, &gif) && can_show(&hp, &png));
        assert!(can_show(&bg, &gif) && can_show(&get("awww").unwrap(), &gif));
        let e = plan(&req("hyprpaper", None, &gif), &[]).unwrap_err().to_string();
        assert!(e.contains("cannot read GIFs"), "{e}");
    }
}
