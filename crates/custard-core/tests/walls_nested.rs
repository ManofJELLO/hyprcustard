//! Putting wallpapers up with every program hyprcustard drives, for real,
//! in a nested Hyprland.
//!
//! The nested instance's own window is disabled before it opens and it is
//! given a headless output instead, so nothing appears on screen. This
//! process's `WAYLAND_DISPLAY` and `HYPRLAND_INSTANCE_SIGNATURE` point at it,
//! which is what keeps every program started — and every process stopped —
//! inside it. awww's cache goes to a scratch folder.
//!
//! cargo test -p custard-core --test walls_nested -- --ignored --nocapture
//!
//! Each program that is not installed is skipped.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use custard_core::colors::wallpaper::{self, session_processes};
use custard_core::hyprctl;
use custard_core::walls::setter::{self, Request};

struct Nested(Child);
impl Drop for Nested {
    fn drop(&mut self) {
        for (pid, _) in session_processes(true) {
            let _ = Command::new("kill").arg(pid.to_string()).status();
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn nested(config: &Path) -> (Nested, String, String) {
    let before = hyprctl::instances().unwrap_or_default();
    let child = Command::new("Hyprland")
        .arg("-c")
        .arg(config)
        .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
        .env_remove("HYPRLAND_CMD")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start a nested Hyprland");
    let child = Nested(child);
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if let Some(i) = hyprctl::instances()
            .unwrap_or_default()
            .into_iter()
            .find(|i| !before.iter().any(|b| b.instance == i.instance))
        {
            return (child, i.instance, i.wl_socket);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    panic!("the nested Hyprland did not come up");
}

fn media(path: &Path, source: &str, video: bool) {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-nostdin", "-v", "error", "-y", "-f", "lavfi", "-i", source]);
    if video {
        cmd.args(["-t", "2", "-pix_fmt", "yuv420p"]);
    } else {
        cmd.args(["-frames:v", "1"]);
    }
    assert!(cmd.arg(path).status().unwrap().success());
}

/// The wallpaper programs running in the nested session.
fn running() -> Vec<&'static str> {
    setter::running(&session_processes(true))
}

/// Put `path` up with `program`, then wait until the session reports it.
fn put(program: &str, path: &Path, monitor: Option<&str>, conf: &Path) {
    let p = setter::get(program).unwrap();
    let req = Request {
        program: p,
        monitor: monitor.map(String::from),
        path: path.to_path_buf(),
        fit: "fill".into(),
        monitors: vec!["HEADLESS-2".into()],
        hyprpaper_conf: conf.to_path_buf(),
        mpvpaper: Vec::new(),
        mpv: Default::default(),
    };
    let start = Instant::now();
    let steps = setter::plan(&req, &session_processes(true)).unwrap();
    setter::run(&steps).unwrap_or_else(|e| panic!("{program}: {e}"));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let found = wallpaper::detect();
        let shows = found.wallpapers.iter().any(|w| {
            w.program == program && (w.path == path || w.showing.as_deref() == Some(path))
        });
        let copies = session_processes(true)
            .iter()
            .filter(|(_, argv)| wallpaper::program(argv) == p.process)
            .count();
        if shows && running() == vec![p.id] && copies == 1 {
            break;
        }
        if Instant::now() > deadline {
            panic!(
                "{program} did not end up alone showing {}: running {:?}, found {:?}",
                path.display(),
                running(),
                found.wallpapers
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let layers = hyprctl_layers();
    println!(
        "{program:<9} {:<10} up in {:>4} ms; background layers {layers:?}",
        path.file_name().unwrap().to_string_lossy(),
        start.elapsed().as_millis()
    );
}

fn hyprctl_layers() -> Vec<String> {
    let out = Command::new("hyprctl").args(["-j", "layers"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
    v.as_object()
        .into_iter()
        .flat_map(|m| m.values())
        .filter_map(|mon| mon["levels"]["0"].as_array())
        .flatten()
        .filter_map(|l| l["namespace"].as_str().map(String::from))
        .collect()
}

#[test]
#[ignore = "starts a nested Hyprland"]
fn every_program_puts_a_wallpaper_up_and_hands_over_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("hyprland.lua");
    std::fs::write(
        &config,
        "hl.monitor({ output = \"WAYLAND-1\", disabled = true })\n\
         hl.monitor({ output = \"\", mode = \"1280x720\", position = \"0x0\", scale = 1 })\n",
    )
    .unwrap();
    let (_nested, sig, wl) = nested(&config);
    // Everything from here on talks to the nested instance.
    std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", &sig);
    std::env::set_var("WAYLAND_DISPLAY", &wl);
    std::env::set_var("XDG_CACHE_HOME", dir.path().join("cache"));
    let created = Command::new("hyprctl").args(["output", "create", "headless"]).output().unwrap();
    assert!(created.status.success());
    std::thread::sleep(Duration::from_millis(500));
    let names: Vec<String> = hyprctl::monitors().unwrap().into_iter().map(|m| m.name).collect();
    println!("nested monitors: {names:?}");

    let red = dir.path().join("red.png");
    let green = dir.path().join("green.png");
    let clips = dir.path().join("clips");
    std::fs::create_dir_all(&clips).unwrap();
    let clip = clips.join("bars.mp4");
    let second = clips.join("dots.mp4");
    media(&red, "color=c=red:s=320x200", false);
    media(&green, "color=c=green:s=320x200", false);
    media(&clip, "testsrc=s=320x200:d=2", true);
    media(&second, "testsrc2=s=320x200:d=2", true);
    let conf = dir.path().join("hyprcustard-hyprpaper.conf");

    let have = |p: &str| setter::installed(&setter::get(p).unwrap());
    let mut order: Vec<(&str, PathBuf)> = Vec::new();
    if have("swaybg") {
        order.push(("swaybg", red.clone()));
        order.push(("swaybg", green.clone()));
    }
    if have("mpvpaper") {
        order.push(("mpvpaper", clip.clone()));
        order.push(("mpvpaper", clips.clone()));
    }
    if have("awww") {
        order.push(("awww", red.clone()));
        order.push(("awww", green.clone()));
    }
    if have("hyprpaper") {
        order.push(("hyprpaper", red.clone()));
        order.push(("hyprpaper", green.clone()));
    }
    if have("swaybg") {
        order.push(("swaybg", red.clone()));
    }
    assert!(!order.is_empty(), "no wallpaper program is installed");
    for (program, path) in &order {
        put(program, path, None, &conf);
        // A folder plays through: within a few seconds mpv moves from the
        // first two-second clip to the second.
        if *program == "mpvpaper" && path.is_dir() {
            let deadline = Instant::now() + Duration::from_secs(8);
            let mut seen = Vec::new();
            while Instant::now() < deadline && seen.len() < 2 {
                let now = wallpaper::detect().wallpapers.into_iter().find_map(|w| w.showing);
                if let Some(f) = now {
                    if !seen.contains(&f) {
                        seen.push(f);
                    }
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            assert_eq!(seen.len(), 2, "the folder stayed on {seen:?}");
            println!("mpvpaper  moved on from one clip to the next");
        }
    }
    if have("hyprpaper") {
        let text = std::fs::read_to_string(&conf).unwrap();
        assert!(text.contains(&format!("path = {}", red.display())), "{text}");
    }

    // A video with a program that cannot play one is refused before
    // anything is touched.
    if have("swaybg") {
        let req = Request {
            program: setter::get("swaybg").unwrap(),
            monitor: None,
            path: clip.clone(),
            fit: "fill".into(),
            monitors: names.clone(),
            hyprpaper_conf: conf.clone(),
            mpvpaper: Vec::new(),
            mpv: Default::default(),
        };
        assert!(setter::plan(&req, &session_processes(true)).is_err());
        assert_eq!(running(), vec!["swaybg"]);
    }
}
