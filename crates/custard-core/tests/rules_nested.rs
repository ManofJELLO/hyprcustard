//! Look rules, checked by a real Hyprland.
//!
//! Runs a nested, headless Hyprland — its own output is disabled before it
//! opens, so nothing appears on screen — and has it load a look block with a
//! rule of every kind setting every effect the page offers, then edits them
//! live the way the page does. Ignored by default because it needs Hyprland
//! installed and a Wayland session to nest in:
//!
//! cargo test -p custard-core --test rules_nested -- --ignored --nocapture

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use custard_core::hyprctl;
use custard_core::look::{self, Look};
use custard_core::rules::{self, Rule, RuleKind, EFFECTS};
use serde_json::{json, Value};

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
    let _ = child.kill();
    let _ = child.wait();
    panic!("the nested Hyprland did not come up; see {}", log.display());
}

/// A rule of `kind` setting every effect the page offers it, at its default.
fn everything(id: &str, kind: RuleKind, target: Value) -> Rule {
    let mut effects = serde_json::Map::new();
    for f in EFFECTS.iter().filter(|f| f.rule == kind) {
        effects.insert(f.key.to_string(), serde_json::from_str(f.default).unwrap());
    }
    rules::edited(&Rule::new(id, kind), &json!({ "matches": target, "effects": effects })).unwrap()
}

fn workspace_rule(sig: &str, ws: &str) -> Value {
    let all: Value =
        serde_json::from_str(&hyprctl::on_instance(sig, &["workspacerules", "-j"]).unwrap())
            .unwrap();
    all.as_array()
        .unwrap()
        .iter()
        .rfind(|r| r["workspaceString"] == ws && r["enabled"] == json!(true))
        .cloned()
        .unwrap_or(Value::Null)
}

#[test]
#[ignore = "starts a nested Hyprland"]
fn every_rule_field_loads_and_edits_live() {
    let dir = tempfile::tempdir().unwrap();
    let mut l = Look::default();
    l.rules.push(everything("lr1", RuleKind::Window, json!({ "class": "kitty", "float": false })));
    l.rules.push(everything("lr2", RuleKind::Layer, json!({ "namespace": "waybar" })));
    l.rules.push(everything("lr3", RuleKind::Workspace, json!({ "workspace": "w[tv1]" })));
    let block = look::emit(&l).unwrap();

    // The user's own rule for the same workspace, above the block.
    let theirs = "hl.workspace_rule({ workspace = \"w[tv1]\", border_size = 7 })";
    let config = dir.path().join("hyprland.lua");
    std::fs::write(
        &config,
        format!(
            "hl.monitor({{ output = \"WAYLAND-1\", disabled = true }})\n{theirs}\n\n{}\n",
            custard_core::block::splice_with("", &block, &look::MARKERS).unwrap()
        ),
    )
    .unwrap();

    let (mut child, sig) = start(&config, &dir.path().join("hyprland.log"));
    let result = std::panic::catch_unwind(|| {
        std::thread::sleep(Duration::from_millis(800));
        let errors = hyprctl::on_instance(&sig, &["configerrors"]).unwrap_or_default();
        assert!(errors.trim().is_empty(), "config errors:\n{errors}\n\n{block}");

        // Loaded: ours merged over theirs, which only we leave border_size to.
        let ws = workspace_rule(&sig, "w[tv1]");
        println!("loaded: {ws}");
        assert_eq!(ws["gapsOut"], json!([0, 0, 0, 0]));
        assert_eq!(ws["borderSize"], json!(0), "ours comes later and wins");

        // A live edit taking border_size away: theirs shows through again,
        // rather than vanishing along with ours.
        let mut edited = l.rules[2].clone();
        edited.effects.remove("border_size");
        edited.effects.insert("gaps_out".into(), look::LookValue::Sides([9, 9, 9, 9]));
        let hand = rules::in_config(&std::fs::read_to_string(&config).unwrap());
        let code = rules::live_one("lr3", Some(&edited), &rules::theirs_for(&hand, &["w[tv1]"]));
        let out = hyprctl::on_instance(&sig, &["eval", &code]).unwrap();
        assert!(!out.starts_with("error"), "{out}\n{code}");
        let ws = workspace_rule(&sig, "w[tv1]");
        println!("edited: {ws}");
        assert_eq!(ws["gapsOut"], json!([9, 9, 9, 9]));
        assert_eq!(ws["borderSize"], json!(7));

        // Every rule replaced at once, the way a resync does.
        let code = rules::live_all(&l.rules, &rules::theirs_for(&hand, &["w[tv1]"]));
        let out = hyprctl::on_instance(&sig, &["eval", &code]).unwrap();
        assert!(!out.starts_with("error"), "{out}\n{code}");
        let ws = workspace_rule(&sig, "w[tv1]");
        assert_eq!(ws["gapsOut"], json!([0, 0, 0, 0]));
        assert_eq!(ws["borderSize"], json!(0));

        // A rule Hyprland will not take is refused, not half applied.
        let mut bad = l.rules[0].clone();
        bad.matches.insert("bogus".into(), look::LookValue::Text("x".into()));
        let out = hyprctl::on_instance(&sig, &["eval", &rules::live_one("lr1", Some(&bad), &[])]);
        assert!(out.is_err() || out.unwrap().starts_with("error"));
    });
    let _ = child.kill();
    let _ = child.wait();
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}
