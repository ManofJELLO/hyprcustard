//! Every built-in theme's look block, loaded by a real Hyprland.
//!
//! Runs a nested, headless Hyprland per theme — its own output is disabled
//! before it opens, so nothing appears on screen — with a config holding
//! nothing but the theme's block, then asks it for config errors and reads a
//! few values back. Ignored by default because it needs Hyprland installed
//! and a Wayland session to nest in:
//!
//! cargo test -p custard-core --test themes_nested -- --ignored --nocapture

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use custard_core::hyprctl;
use custard_core::look::{self, Look};
use custard_core::motion::LiveMotion;
use custard_core::preset;

fn start(config: &std::path::Path, log: &std::path::Path) -> (std::process::Child, String) {
    let before = hyprctl::instances().unwrap_or_default();
    let mut child = Command::new("Hyprland")
        .arg("-c")
        .arg(config)
        .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
        .env_remove("HYPRLAND_CMD")
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(log).map(Stdio::from).unwrap_or(Stdio::null()))
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
    // Never leave it running, even when failing.
    let _ = child.kill();
    let _ = child.wait();
    panic!("the nested Hyprland did not come up; see {}", log.display());
}

#[test]
#[ignore = "starts a nested Hyprland"]
fn every_builtin_theme_loads_without_errors() {
    let dir = tempfile::tempdir().unwrap();
    for p in preset::builtin() {
        let (applied, problems) = preset::to_look(&p, &Look::default(), &LiveMotion::default());
        assert!(problems.is_empty(), "{}: {problems:?}", p.name);
        let block = look::emit(&applied).unwrap();
        let config = dir.path().join("hyprland.lua");
        std::fs::write(
            &config,
            format!(
                "hl.monitor({{ output = \"WAYLAND-1\", disabled = true }})\n\n{}\n",
                custard_core::block::splice_with("", &block, &look::MARKERS).unwrap()
            ),
        )
        .unwrap();

        let (mut child, sig) = start(&config, &dir.path().join("hyprland.log"));
        // Give it a moment to finish reading the config.
        std::thread::sleep(Duration::from_millis(800));
        let errors = hyprctl::on_instance(&sig, &["configerrors"]).unwrap_or_default();
        let rounding = hyprctl::on_instance(&sig, &["getoption", "decoration:rounding", "-j"])
            .unwrap_or_default();
        let windows = hyprctl::on_instance(&sig, &["animations", "-j"]).unwrap_or_default();
        let _ = child.kill();
        let _ = child.wait();

        println!("{}: errors {:?}", p.name, errors.trim());
        assert!(errors.trim().is_empty(), "{} gave config errors:\n{errors}", p.name);
        let want = p.options["decoration.rounding"].as_i64().unwrap();
        assert!(rounding.contains(&format!("\"int\": {want}")), "{}: {rounding}", p.name);
        for curve in p.curves.keys() {
            assert!(windows.contains(&format!("\"{curve}\"")), "{}: no curve {curve}", p.name);
        }
    }
}
