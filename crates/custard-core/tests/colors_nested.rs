//! The Hyprland colors file, loaded by a real Hyprland.
//!
//! Runs a nested, headless Hyprland — its own output is disabled before it
//! opens, so nothing appears on screen — whose config ends with the line the
//! Colors page adds, then applies a second scheme live the way Apply does.
//! Ignored by default because it needs Hyprland and a Wayland session:
//!
//! cargo test -p custard-core --test colors_nested -- --ignored --nocapture

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use custard_core::colors::{named, targets};
use custard_core::hyprctl;

fn start(config: &std::path::Path) -> (std::process::Child, String) {
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
                return (child, new.instance.clone());
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let _ = child.kill();
    let _ = child.wait();
    panic!("the nested Hyprland did not come up");
}

/// The first color of a border option, as `0xAARRGGBB`, lowercased.
fn border(sig: &str, key: &str) -> String {
    let code = format!("local c = hl.get_config(\"{key}\"); return c.colors[1]");
    hyprctl::on_instance(sig, &["repl", &code]).unwrap_or_default().trim().to_lowercase()
}

#[test]
#[ignore = "starts a nested Hyprland"]
fn the_colors_file_loads_at_startup_and_live() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("config");
    let config = home.join("hypr").join("hyprland.lua");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(&config, "hl.monitor({ output = \"WAYLAND-1\", disabled = true })\n").unwrap();

    let list = targets::all(&home, &config);
    let hypr = list.iter().find(|t| t.id == "hyprland").unwrap();
    let palettes = named::builtins();
    let gruvbox = palettes.iter().find(|n| n.id == "gruvbox-dark").unwrap().scheme();
    let nord = palettes.iter().find(|n| n.id == "nord").unwrap().scheme();
    targets::write(hypr, &gruvbox, 1.0).unwrap();
    targets::wire(hypr, 0).unwrap();

    let (mut child, sig) = start(&config);
    let result = std::panic::catch_unwind(|| {
        std::thread::sleep(Duration::from_millis(800));
        let errors = hyprctl::on_instance(&sig, &["configerrors"]).unwrap_or_default();
        assert!(errors.trim().is_empty(), "config errors:\n{errors}");
        assert_eq!(border(&sig, "general.col.active_border"), "0xff83a598");
        assert_eq!(border(&sig, "general.col.inactive_border"), "0xff504945");
        let table = hyprctl::on_instance(&sig, &["repl", "return hyprcustard_colors.base0B"])
            .unwrap_or_default();
        assert_eq!(table.trim(), "#b8bb26");

        // Apply, live: rewrite the file and load it again.
        targets::write(hypr, &nord, 1.0).unwrap();
        let code = format!("dofile(\"{}\")", hypr.file.display());
        let out = hyprctl::on_instance(&sig, &["eval", &code]).unwrap();
        assert!(!out.starts_with("error"), "{out}");
        assert_eq!(border(&sig, "general.col.active_border"), "0xff81a1c1");

        // A missing file must not break the config: the line is a pcall.
        std::fs::remove_file(&hypr.file).unwrap();
        hyprctl::on_instance(&sig, &["reload", "config-only"]).unwrap();
        std::thread::sleep(Duration::from_millis(500));
        let errors = hyprctl::on_instance(&sig, &["configerrors"]).unwrap_or_default();
        assert!(errors.trim().is_empty(), "a missing colors file caused:\n{errors}");
    });
    let _ = child.kill();
    let _ = child.wait();
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}
