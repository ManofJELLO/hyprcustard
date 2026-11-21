//! hyprcustard's bar, run by a real Quickshell in a real Hyprland.
//!
//! Sets the bar up in a scratch folder, starts a nested, headless Hyprland
//! (its own output is disabled before it opens, so nothing appears on
//! screen), runs the bar there with `qs -p`, and checks that it loads with no
//! QML errors, sits where its settings say, and follows a change to them
//! without restarting. Ignored by default because it needs Hyprland,
//! Quickshell and a Wayland session:
//!
//! cargo test -p custard-core --test bar_nested -- --ignored --nocapture
//!
//! With `CUSTARD_SHOT_DIR` set, it saves a screenshot of each step there.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use custard_core::bar::{self, BarSettings, Machine};
use custard_core::colors::{self, named};
use custard_core::hyprctl;

/// One nested Hyprland at a time: each finds its own by what instance is new,
/// so two starting together could take the same one.
static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Nested {
    hyprland: Child,
    sig: String,
    socket: String,
}

impl Drop for Nested {
    fn drop(&mut self) {
        let _ = self.hyprland.kill();
        let _ = self.hyprland.wait();
    }
}

fn start_hyprland(config: &Path) -> Nested {
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
        if let Ok(now) = hyprctl::instances() {
            if let Some(new) = now.iter().find(|i| !before.iter().any(|b| b.instance == i.instance))
            {
                let nested = Nested {
                    hyprland: child,
                    sig: new.instance.clone(),
                    socket: new.wl_socket.clone(),
                };
                hyprctl::on_instance(&nested.sig, &["output", "create", "headless"]).unwrap();
                std::thread::sleep(Duration::from_millis(500));
                return nested;
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let _ = child.kill();
    let _ = child.wait();
    panic!("the nested Hyprland did not come up");
}

/// The bar's layer surfaces in the nested instance: (x, y, w, h) each.
fn bar_layers(sig: &str) -> Vec<(i64, i64, i64, i64)> {
    let out = hyprctl::on_instance(sig, &["layers", "-j"]).unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap_or_default();
    let mut found = Vec::new();
    for (_, monitor) in v.as_object().into_iter().flatten() {
        for (_, level) in monitor["levels"].as_object().into_iter().flatten() {
            for layer in level.as_array().into_iter().flatten() {
                if layer["namespace"] == "quickshell" {
                    let n = |k: &str| layer[k].as_i64().unwrap_or(-1);
                    found.push((n("x"), n("y"), n("w"), n("h")));
                }
            }
        }
    }
    found
}

fn wait_for<T>(secs: u64, mut f: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline {
        if let Some(v) = f() {
            return Some(v);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
}

/// One row of the nested screen, each pixel as `#rrggbb`.
fn row(socket: &str, y: usize) -> Vec<String> {
    let out = Command::new("grim")
        .args(["-t", "ppm", "-"])
        .env("WAYLAND_DISPLAY", socket)
        .output()
        .expect("grim");
    // P6, width, height, max value, then RGB triples.
    let mut fields = Vec::new();
    let mut i = 0;
    while fields.len() < 4 {
        while out.stdout[i].is_ascii_whitespace() {
            i += 1;
        }
        let start = i;
        while !out.stdout[i].is_ascii_whitespace() {
            i += 1;
        }
        fields.push(String::from_utf8_lossy(&out.stdout[start..i]).into_owned());
    }
    let width: usize = fields[1].parse().unwrap();
    let at = i + 1 + 3 * y * width;
    out.stdout[at..at + 3 * width]
        .chunks(3)
        .map(|p| format!("#{:02x}{:02x}{:02x}", p[0], p[1], p[2]))
        .collect()
}

/// One pixel of the nested screen, as `#rrggbb`.
fn pixel(socket: &str, x: usize, y: usize) -> String {
    row(socket, y).swap_remove(x)
}

fn shot(socket: &str, name: &str) {
    let Some(dir) = std::env::var_os("CUSTARD_SHOT_DIR").map(PathBuf::from) else { return };
    let _ = Command::new("grim")
        .arg(dir.join(format!("{name}.png")))
        .env("WAYLAND_DISPLAY", socket)
        .status();
}

#[test]
#[ignore = "starts a nested Hyprland and Quickshell"]
fn the_bar_loads_sits_where_it_is_told_and_follows_its_settings() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let qs = bar::program().expect("Quickshell is installed");
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("config");
    let config = dir.path().join("nested.lua");
    std::fs::write(
        &config,
        "hl.monitor({ output = \"WAYLAND-1\", disabled = true })\n\
         hl.monitor({ output = \"\", mode = \"1600x500\", position = \"0x0\", scale = 1 })\n",
    )
    .unwrap();

    let scheme = named::builtins().iter().find(|n| n.id == "nord").unwrap().scheme();
    let machine = Machine { nerd_font: true, ..Default::default() };
    // Opaque, so a pixel of the bar is the scheme's background exactly.
    let mut settings = BarSettings { opacity: 1.0, ..Default::default() };
    bar::install(&home, &settings, &machine, &scheme).unwrap();

    let nested = start_hyprland(&config);
    let log_path = dir.path().join("qs.log");
    let log = std::fs::File::create(&log_path).unwrap();
    let mut qs = Command::new(&qs)
        .arg("-p")
        .arg(bar::dir(&home))
        .env("WAYLAND_DISPLAY", &nested.socket)
        .env("HYPRLAND_INSTANCE_SIGNATURE", &nested.sig)
        .env("QT_QPA_PLATFORM", "wayland")
        .stdin(Stdio::null())
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .spawn()
        .expect("start Quickshell");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let top = wait_for(10, || bar_layers(&nested.sig).first().copied())
            .expect("the bar maps a layer surface");
        shot(&nested.socket, "bar-top");
        let h = i64::from(settings.height) + 8;
        assert_eq!((top.1, top.3), (i64::from(settings.edge_margin), h), "at the top: {top:?}");
        assert_eq!(top.2, 1600, "the whole width");

        // A change to its settings moves it, without a restart.
        settings.position = "bottom".into();
        settings.side_margin = 100;
        bar::write_settings(&home, &settings, &machine).unwrap();
        let bottom = wait_for(5, || bar_layers(&nested.sig).first().copied().filter(|l| l.1 > 200))
            .expect("the bar moves to the bottom");
        shot(&nested.socket, "bar-bottom");
        assert_eq!(bottom.1 + bottom.3, 500 - i64::from(settings.edge_margin), "{bottom:?}");
        assert_eq!((bottom.0, bottom.2), (100, 1400), "{bottom:?}");

        // Space kept clear on the windows' side: the surface grows by it, and
        // at the bottom the box stays on the screen's edge with the space above.
        settings.window_gap = 6;
        bar::write_settings(&home, &settings, &machine).unwrap();
        let tall = i64::from(settings.height) + 8 + 6;
        let gapped =
            wait_for(5, || bar_layers(&nested.sig).first().copied().filter(|l| l.3 == tall))
                .unwrap_or_else(|| {
                    panic!("the bar did not grow to {tall}: {:?}", bar_layers(&nested.sig))
                });
        assert_eq!(gapped.1 + gapped.3, 500 - i64::from(settings.edge_margin), "{gapped:?}");
        // Hyprland animates the layer to its new place, so wait for it to land:
        // the box's first and last rows are the background, the gap above it
        // is not.
        let bg = scheme.role("background").to_string();
        let box_top = (gapped.1 + 6 + 4) as usize;
        let box_last = box_top + settings.height as usize - 1;
        let landed = || {
            let (first, last) =
                (pixel(&nested.socket, 800, box_top + 1), pixel(&nested.socket, 800, box_last - 1));
            let gap = pixel(&nested.socket, 800, box_top - 6);
            (first == bg && last == bg && gap != bg).then_some(())
        };
        wait_for(5, landed).unwrap_or_else(|| {
            shot(&nested.socket, "bar-gap");
            panic!("the box is not below a clear gap of 6px")
        });
        shot(&nested.socket, "bar-gap");
        settings.window_gap = 0;
        bar::write_settings(&home, &settings, &machine).unwrap();
        wait_for(5, || bar_layers(&nested.sig).first().copied().filter(|l| l.3 == tall - 6))
            .expect("the bar shrinks back");

        // Words instead of icons, and fewer modules: still no errors.
        settings.icons = "text".into();
        settings.modules = vec!["clock".into(), "cpu".into(), "memory".into()];
        bar::write_settings(&home, &settings, &Machine { nerd_font: false, ..machine.clone() })
            .unwrap();
        std::thread::sleep(Duration::from_millis(1500));
        shot(&nested.socket, "bar-text");

        // The green edge opens the CPU, memory and temperature group, so it
        // stays when CPU, the module that usually carries it, is turned off.
        let edge_row = (bottom.1 + bottom.3 / 2) as usize;
        let green = scheme.base(0xB).to_string();
        let greens = |socket: &str| -> Vec<usize> {
            row(socket, edge_row)
                .iter()
                .enumerate()
                .filter(|(_, p)| **p == green)
                .map(|(x, _)| x)
                .collect()
        };
        let with_cpu = greens(&nested.socket);
        assert_eq!(with_cpu.len(), 3, "one 3px green edge, before CPU's: {with_cpu:?}");
        settings.modules = vec!["clock".into(), "memory".into()];
        bar::write_settings(&home, &settings, &Machine { nerd_font: false, ..machine.clone() })
            .unwrap();
        // Memory now starts where CPU did, so the edge moves right.
        wait_for(5, || {
            let now = greens(&nested.socket);
            (now.len() == 3 && now[0] > with_cpu[0]).then_some(())
        })
        .unwrap_or_else(|| {
            shot(&nested.socket, "bar-no-cpu");
            panic!("no green edge before memory with CPU off: {:?}", greens(&nested.socket))
        });
        shot(&nested.socket, "bar-no-cpu");

        // A new scheme shows at once, the bar's own surface kept: the middle
        // of the bar, empty with no window open, is the scheme's background.
        let (mx, my) = (800, (bottom.1 + bottom.3 / 2) as usize);
        assert_eq!(pixel(&nested.socket, mx, my), scheme.role("background"));
        let gruvbox = named::builtins().iter().find(|n| n.id == "gruvbox-dark").unwrap().scheme();
        bar::write_colors(&home, &gruvbox).unwrap();
        let want = gruvbox.role("background").to_string();
        wait_for(5, || (pixel(&nested.socket, mx, my) == want).then_some(()))
            .unwrap_or_else(|| panic!("the bar did not turn {want}"));
        shot(&nested.socket, "bar-recolored");
        assert_eq!(bar_layers(&nested.sig).len(), 1, "still one bar");

        // Asking for light turns a dark scheme's bar light at once, from the
        // other kind colors.json carries; following the colors turns it back.
        settings.mode = "light".into();
        bar::write_settings(&home, &settings, &machine).unwrap();
        let light = colors::scheme::flipped(&gruvbox).role("background").to_string();
        wait_for(5, || (pixel(&nested.socket, mx, my) == light).then_some(()))
            .unwrap_or_else(|| panic!("the bar did not turn light ({light})"));
        shot(&nested.socket, "bar-light");
        settings.mode = "auto".into();
        bar::write_settings(&home, &settings, &machine).unwrap();
        wait_for(5, || (pixel(&nested.socket, mx, my) == want).then_some(()))
            .unwrap_or_else(|| panic!("the bar did not go back to {want}"));

        // The launcher and the power menu open and close over IPC, centered
        // on the screen, and only one at a time of each.
        settings.icons = "glyphs".into();
        settings.modules = BarSettings::default().modules;
        bar::write_settings(&home, &settings, &machine).unwrap();
        let program = bar::program().unwrap();
        let ipc = |target: &str, call: &str| {
            let out = Command::new(&program)
                .args(["ipc", "-p"])
                .arg(bar::dir(&home))
                .args(["call", target, call])
                // It looks for the bar on this display only.
                .env("WAYLAND_DISPLAY", &nested.socket)
                .output()
                .unwrap();
            assert!(out.status.success(), "qs ipc call {target} {call}: {out:?}");
        };
        for (target, name) in [("launcher", "launcher"), ("power", "power-menu")] {
            ipc(target, "toggle");
            let menu = wait_for(5, || {
                let layers = bar_layers(&nested.sig);
                (layers.len() == 2).then(|| layers.into_iter().find(|l| l.2 < 1400).unwrap())
            })
            .unwrap_or_else(|| panic!("the {name} did not open: {:?}", bar_layers(&nested.sig)));
            assert!((menu.0 + menu.2 / 2 - 800).abs() <= 1, "{name} centered across: {menu:?}");
            // Centered in the room the bar leaves: here the bar is at the
            // bottom, so the menu is above it and clear of it.
            let bar_top = bar_layers(&nested.sig).into_iter().map(|l| l.1).max().unwrap();
            assert!(
                (menu.1 + menu.3 / 2 - bar_top / 2).abs() <= 1,
                "{name} centered down: {menu:?}"
            );
            std::thread::sleep(Duration::from_millis(600));
            shot(&nested.socket, name);
            if target == "launcher" {
                let out = Command::new(&program)
                    .args(["ipc", "-p"])
                    .arg(bar::dir(&home))
                    .args(["call", "launcher", "search", "term"])
                    .env("WAYLAND_DISPLAY", &nested.socket)
                    .output()
                    .unwrap();
                assert!(out.status.success(), "{out:?}");
                std::thread::sleep(Duration::from_millis(400));
                shot(&nested.socket, "launcher-search");
            }
            ipc(target, "open");
            assert_eq!(bar_layers(&nested.sig).len(), 2, "opening it again adds none");
            ipc(target, "toggle");
            wait_for(5, || (bar_layers(&nested.sig).len() == 1).then_some(()))
                .unwrap_or_else(|| panic!("the {name} did not close"));
        }

        // Neumorphic, with the menus in the top right corner: the bar grows
        // to make room for its shadow, and each menu sits a margin from the
        // corner (the bar is at the bottom, so nothing is above them).
        settings.style = "neumorphic".into();
        settings.menu_position = "top-right".into();
        settings.menu_margin = 20;
        settings.power_layout = "column".into();
        settings.power_items = ["lock", "reboot", "shutdown"].map(String::from).to_vec();
        settings.launcher_icons = false;
        settings.launcher_rows = 5;
        bar::write_settings(&home, &settings, &machine).unwrap();
        let grown = i64::from(settings.height) + 2 * 10;
        wait_for(5, || bar_layers(&nested.sig).first().copied().filter(|l| l.3 == grown))
            .unwrap_or_else(|| {
                panic!("the bar did not grow to {grown}: {:?}", bar_layers(&nested.sig))
            });
        std::thread::sleep(Duration::from_millis(500));
        shot(&nested.socket, "bar-neumorphic");
        for (target, name) in
            [("launcher", "launcher-neumorphic"), ("power", "power-menu-neumorphic")]
        {
            ipc(target, "toggle");
            let menu = wait_for(5, || {
                let layers = bar_layers(&nested.sig);
                (layers.len() == 2).then(|| layers.into_iter().find(|l| l.2 < 1400).unwrap())
            })
            .unwrap_or_else(|| panic!("the {name} did not open"));
            assert_eq!(
                (menu.0 + menu.2, menu.1),
                (1600 - 20, 20),
                "{name} in the corner: {menu:?}"
            );
            std::thread::sleep(Duration::from_millis(600));
            shot(&nested.socket, name);
            ipc(target, "toggle");
            wait_for(5, || (bar_layers(&nested.sig).len() == 1).then_some(()))
                .unwrap_or_else(|| panic!("the {name} did not close"));
        }
    }));

    let _ = qs.kill();
    let _ = qs.wait();
    drop(nested);
    let text = std::fs::read_to_string(&log_path).unwrap_or_default();
    println!("--- quickshell log ---\n{text}");
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
    let bad: Vec<&str> = text
        .lines()
        .filter(|l| l.contains("ERROR") || (l.contains("WARN") && l.contains(".qml")))
        .collect();
    assert!(bad.is_empty(), "QML problems:\n{}", bad.join("\n"));
}

/// An update rewrites the files of a bar that is running. Quickshell reloads
/// as each loaded file changes, so it sees the update half written; it must
/// still end up on the new files, not stuck on a failed load ("Neu is not a
/// type") with the old bar left running, as happened going to 0.12.1.
#[test]
#[ignore = "starts a nested Hyprland and Quickshell, and reads v0.12.0's bar from git"]
fn a_running_bar_updated_in_place_ends_up_on_the_new_files() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let program = bar::program().expect("Quickshell is installed");
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("config");
    let config = dir.path().join("nested.lua");
    std::fs::write(
        &config,
        "hl.monitor({ output = \"WAYLAND-1\", disabled = true })\n\
         hl.monitor({ output = \"\", mode = \"1600x500\", position = \"0x0\", scale = 1 })\n",
    )
    .unwrap();
    let scheme = named::builtins().iter().find(|n| n.id == "nord").unwrap().scheme();
    bar::install(&home, &BarSettings::default(), &Machine::default(), &scheme).unwrap();

    // Put v0.12.0's bar in its place, as that version left it.
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let git = |args: &[&str]| {
        let out = Command::new("git").current_dir(&repo).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
        out.stdout
    };
    let listed =
        String::from_utf8(git(&["ls-tree", "--name-only", "v0.12.0", "crates/custard-core/bar/"]))
            .unwrap();
    let old: Vec<String> = listed
        .lines()
        .filter(|p| p.ends_with(".qml"))
        .map(|p| p.rsplit('/').next().unwrap().to_string())
        .collect();
    let d = bar::dir(&home);
    for (name, _) in bar::FILES {
        if !old.iter().any(|o| o == name) {
            std::fs::remove_file(d.join(name)).unwrap();
        }
    }
    for name in &old {
        let text = git(&["show", &format!("v0.12.0:crates/custard-core/bar/{name}")]);
        std::fs::write(d.join(name), text).unwrap();
    }
    std::fs::write(d.join(bar::STAMP), format!("0.12.0\n{}\n", old.join("\n"))).unwrap();

    let nested = start_hyprland(&config);
    let log_path = dir.path().join("qs.log");
    let log = std::fs::File::create(&log_path).unwrap();
    let mut qs = Command::new(&program)
        .arg("-p")
        .arg(&d)
        .env("WAYLAND_DISPLAY", &nested.socket)
        .env("HYPRLAND_INSTANCE_SIGNATURE", &nested.sig)
        .env("QT_QPA_PLATFORM", "wayland")
        .stdin(Stdio::null())
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .spawn()
        .expect("start Quickshell");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        wait_for(10, || bar_layers(&nested.sig).first().copied()).expect("the old bar runs");
        assert!(bar::update_if_stale(&home).unwrap(), "0.12.0 is older, so it is rewritten");
        // Settled: the last load finished, and nothing has changed since.
        std::thread::sleep(Duration::from_secs(4));
        let text = std::fs::read_to_string(&log_path).unwrap_or_default();
        let last = text
            .lines()
            .rfind(|l| {
                l.contains("Configuration Loaded") || l.contains("Failed to load configuration")
            })
            .unwrap_or("");
        assert!(last.contains("Configuration Loaded"), "the last load failed:\n{text}");
        assert!(
            text.matches("Configuration Loaded").count() >= 2,
            "it reloaded onto the update:\n{text}"
        );
        assert_eq!(bar_layers(&nested.sig).len(), 1, "one bar");
    }));

    let _ = qs.kill();
    let _ = qs.wait();
    drop(nested);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}
