//! custard-watch, end to end, against a nested Hyprland.
//!
//! A headless Hyprland (its output disabled before it opens, so nothing
//! appears on screen), a stand-in for swaybg showing a red image, and the
//! watcher with its config folders pointed at scratch directories. The
//! watcher has to apply red at once, notice when the wallpaper becomes green
//! — through inotify, not its fallback timer — reload the nested Hyprland's
//! borders, and exit when following is turned off. Its memory and CPU use
//! are printed along the way.
//!
//! cargo test -p custard-watch --test follow_nested -- --ignored --nocapture

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use custard_core::hyprctl;

struct Kill(Vec<Child>);
impl Drop for Kill {
    fn drop(&mut self) {
        for c in &mut self.0 {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

fn nested(config: &Path) -> (Child, String, String) {
    let before = hyprctl::instances().unwrap_or_default();
    let mut child = Command::new("Hyprland")
        .arg("-c")
        .arg(config)
        .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
        .env_remove("HYPRLAND_CMD")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start a nested Hyprland");
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
    let _ = child.kill();
    let _ = child.wait();
    panic!("the nested Hyprland did not come up");
}

/// A process whose command line reads `swaybg -c ... -i <image>`, on the
/// nested compositor's display — all the watcher looks at.
fn fake_swaybg(image: &Path, display: &str) -> Child {
    Command::new("bash")
        .arg("-c")
        .arg("exec -a swaybg python3 -c 'import time; time.sleep(600)' -i \"$0\"")
        .arg(image)
        .env("WAYLAND_DISPLAY", display)
        .stdin(Stdio::null())
        .spawn()
        .unwrap()
}

fn image(path: &Path, color: &str) {
    let ok = Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg(format!("color=c={color}:s=320x200"))
        .args(["-frames:v", "1"])
        .arg(path)
        .status()
        .unwrap()
        .success();
    assert!(ok);
}

fn applied(app: &Path) -> Option<String> {
    let text = std::fs::read_to_string(app.join("colors").join("current.json")).ok()?;
    serde_json::from_str::<serde_json::Value>(&text).ok()?["name"].as_str().map(str::to_string)
}

fn wait_for(what: &str, secs: u64, mut ok: impl FnMut() -> bool) -> Duration {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(secs) {
        if ok() {
            return start.elapsed();
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("timed out waiting for {what}");
}

/// Resident memory in kB and CPU time in clock ticks.
fn usage(pid: u32) -> (u64, u64) {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap_or_default();
    let rss = status
        .lines()
        .find_map(|l| l.strip_prefix("VmRSS:"))
        .and_then(|v| v.split_whitespace().next()?.parse().ok())
        .unwrap_or(0);
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
    let after = stat.rsplit(')').next().unwrap_or("");
    let f: Vec<u64> = after.split_whitespace().filter_map(|x| x.parse().ok()).collect();
    // utime and stime are fields 14 and 15; after the `)` that is 11 and 12,
    // counting from 0 with the state letter dropped by the parse.
    (rss, f.get(10).copied().unwrap_or(0) + f.get(11).copied().unwrap_or(0))
}

#[test]
#[ignore = "starts a nested Hyprland"]
fn follows_a_changing_wallpaper_and_stops_when_told() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("xdg");
    let app = dir.path().join("app");
    let walls = dir.path().join("walls");
    for d in [&home.join("hypr"), &app.join("colors"), &walls] {
        std::fs::create_dir_all(d).unwrap();
    }
    let (red, green) = (walls.join("red.png"), walls.join("green.png"));
    image(&red, "0xc02020");
    image(&green, "0x20a040");

    let config = home.join("hypr").join("hyprland.lua");
    let colors_file = home.join("hypr").join("hyprcustard-colors.lua");
    std::fs::write(
        &config,
        format!(
            "hl.monitor({{ output = \"WAYLAND-1\", disabled = true }})\n\
             pcall(dofile, \"{}\")  -- hyprcustard colors\n",
            colors_file.display()
        ),
    )
    .unwrap();
    std::fs::write(app.join("settings.toml"), format!("config_path = \"{}\"\n", config.display()))
        .unwrap();
    std::fs::write(
        app.join("colors").join("follow.toml"),
        "source = \"wallpaper\"\nmin_minutes = 0\n",
    )
    .unwrap();

    let (hypr, sig, display) = nested(&config);
    let mut guard = Kill(vec![hypr]);
    std::thread::sleep(Duration::from_millis(600));
    guard.0.push(fake_swaybg(&red, &display));

    let watcher = Command::new(env!("CARGO_BIN_EXE_custard-watch"))
        .env("HYPRLAND_INSTANCE_SIGNATURE", &sig)
        .env("WAYLAND_DISPLAY", &display)
        .env("XDG_CONFIG_HOME", &home)
        .env("CUSTARD_CONFIG_DIR", &app)
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    let pid = watcher.id();
    guard.0.push(watcher);

    let took = wait_for("red to be applied", 30, || applied(&app).as_deref() == Some("red.png"));
    println!("red applied after {took:?}");
    let border = || {
        hyprctl::on_instance(
            &sig,
            &["repl", "return hl.get_config(\"general.col.active_border\").colors[1]"],
        )
        .unwrap_or_default()
        .trim()
        .to_lowercase()
    };
    let red_border = border();
    assert!(red_border.starts_with("0xff"), "{red_border}");

    std::thread::sleep(Duration::from_secs(2));
    let (rss, ticks) = usage(pid);
    std::thread::sleep(Duration::from_secs(5));
    let (rss_idle, ticks_idle) = usage(pid);
    println!("idle: {rss_idle} kB resident, {} CPU ticks over 5 s", ticks_idle - ticks);
    assert!(ticks_idle - ticks <= 1, "it should be asleep");
    let _ = rss;

    // The wallpaper changes: a new program shows green, and opening the file
    // is what wakes the watcher.
    let _ = guard.0[1].kill();
    let _ = guard.0[1].wait();
    guard.0[1] = fake_swaybg(&green, &display);
    std::thread::sleep(Duration::from_millis(200));
    let _ = std::fs::read(&green).unwrap();
    let took =
        wait_for("green to be applied", 30, || applied(&app).as_deref() == Some("green.png"));
    println!("green applied {took:?} after the file was opened");
    assert!(took < Duration::from_secs(20), "the event, not the 120 s fallback, woke it");
    assert_ne!(border(), red_border, "the nested Hyprland's borders were reloaded");

    // Turning following off: the snapshot goes, and the watcher exits.
    std::fs::remove_file(app.join("colors").join("follow.toml")).unwrap();
    let watcher = &mut guard.0[2];
    let took = wait_for("the watcher to exit", 10, || matches!(watcher.try_wait(), Ok(Some(_))));
    println!("exited {took:?} after following was turned off");
}
