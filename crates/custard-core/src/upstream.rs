//! hyprcustard against the latest HyprWindowShade and Fishsticks & Custard.
//!
//! hyprcustard writes rules, Lua calls and config keys that belong to those
//! two projects, and mirrors their names, defaults and choices in its own
//! tables. These tests read each project's source and compare. They are
//! ignored on a plain `cargo test`: `make upstream-check` fetches the
//! versions people actually get, sets `HYPRCUSTARD_UPSTREAM_HWS` and
//! `HYPRCUSTARD_UPSTREAM_FSC` to the checkouts and runs them, and it is run
//! before every release.
//!
//! Each test gathers every difference before failing, in three kinds:
//!
//! - **gone**: hyprcustard uses something upstream no longer has.
//! - **changed**: both have it, but it differs (a default, the choices, what
//!   it means).
//! - **new**: upstream has something hyprcustard does not know about.
//!
//! Something left out on purpose goes in an `ACK` table with the reason,
//! and is no longer reported.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::Map;

use crate::fsc::{self, Action, Kind};
use crate::model::{Seconds, TagKind, TAGS};
use crate::settings::{DEFAULT_PLUGIN_NAME, DEFAULT_PLUGIN_REPO};
use crate::shader::meta::KNOWN_UNIFORMS;

// ---------------------------------------------------------------------------
// Left out on purpose
// ---------------------------------------------------------------------------

/// FSC config keys the page does not offer.
const FSC_ACK_KEYS: &[(&str, &str)] = &[
    ("claude.home", "a path to FSC's own working folder; kept in the file as it is"),
    ("claude.setting_sources", "which Claude Code settings FSC reads; kept as it is"),
    ("claude.extra_args", "raw arguments for claude; kept as it is"),
    // FSC 0.7's other agents. Which agent answers, and its model and mode,
    // are switched from FSC's tray menus or `fsc agent` / `model` / `mode`,
    // which save them; an agent's launch settings are set up once, by hand,
    // per FSC's docs. Not offered yet — a candidate for the page later.
    ("claude.memory", "FSC 0.7's old place for agent.memory, which the page edits"),
    ("claude.session_idle_minutes", "FSC 0.7's old place for agent.session_idle_minutes, which the page edits"),
    ("agent.backend", "switched from FSC's tray or `fsc agent`, which saves it"),
    ("agent.auto_allow", "an ACP agent's tool permissions; kept as it is"),
    ("agent.home", "a path to the assistant's home folder; kept as it is"),
    ("claude.permission_mode", "how much Claude asks first; FSC's mode menu sets it"),
    ("acp.command", "a custom ACP agent's launch settings; kept as they are"),
    ("acp.args", "a custom ACP agent's launch settings; kept as they are"),
    ("acp.env", "a custom ACP agent's launch settings; kept as they are"),
    ("acp.model", "chosen from FSC's model menu or `fsc model`"),
    ("acp.mode", "chosen from FSC's mode menu or `fsc mode`"),
    ("codex.command", "Codex's launch settings; kept as they are"),
    ("codex.args", "Codex's launch settings; kept as they are"),
    ("codex.env", "Codex's launch settings; kept as they are"),
    ("codex.model", "chosen from FSC's model menu or `fsc model`"),
    ("codex.mode", "chosen from FSC's mode menu or `fsc mode`"),
    ("antigravity.command", "Antigravity's launch settings; kept as they are"),
    ("antigravity.args", "Antigravity's launch settings; kept as they are"),
    ("antigravity.env", "Antigravity's launch settings; kept as they are"),
    ("antigravity.model", "chosen from FSC's model menu or `fsc model`"),
    ("antigravity.mode", "chosen from FSC's mode menu or `fsc mode`"),
    ("opencode.command", "OpenCode's launch settings; kept as they are"),
    ("opencode.args", "OpenCode's launch settings; kept as they are"),
    ("opencode.env", "OpenCode's launch settings; kept as they are"),
    ("opencode.model", "chosen from FSC's model menu or `fsc model`"),
    ("opencode.mode", "chosen from FSC's mode menu or `fsc mode`"),
];

/// `fsc` subcommands no key or button of hyprcustard's runs.
const FSC_ACK_COMMANDS: &[(&str, &str)] = &[
    ("ask", "takes free text, so it is not a key action"),
    ("show", "the Show or hide key covers it"),
    ("hide", "the Show or hide key covers it"),
    ("status", "hyprcustard looks for the fscd process instead"),
    ("memory", "prints to a terminal; nothing for a key to do"),
    ("forget", "destructive, and belongs in FSC's own popup"),
    ("agent", "takes an agent's name; FSC's tray switches agents"),
    ("model", "takes a model id; FSC's tray switches models"),
    ("mode", "takes a mode; FSC's tray switches modes"),
    ("wake", "takes a phrase; changing it is a setting, not a key"),
];

/// Plugin uniforms that are not for shaders to declare.
const HWS_ACK_UNIFORMS: &[(&str, &str)] = &[
    ("plugin_box_size", "set for the plugin's own rounding wrapper"),
    ("plugin_round", "set for the plugin's own rounding wrapper"),
    ("plugin_round_power", "set for the plugin's own rounding wrapper"),
    ("plugin_dim", "set for the plugin's own dimming wrapper"),
];

// ---------------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------------

struct Report {
    project: &'static str,
    items: Vec<String>,
}

impl Report {
    fn new(project: &'static str) -> Self {
        Self { project, items: Vec::new() }
    }

    fn gone(&mut self, what: impl AsRef<str>) {
        self.items.push(format!("gone:    {}", what.as_ref()));
    }

    fn changed(&mut self, what: impl AsRef<str>) {
        self.items.push(format!("changed: {}", what.as_ref()));
    }

    fn new_item(&mut self, what: impl AsRef<str>) {
        self.items.push(format!("new:     {}", what.as_ref()));
    }

    fn finish(self) {
        if self.items.is_empty() {
            println!("{}: nothing to adjust", self.project);
            return;
        }
        let mut msg = format!("{}: {} to adjust\n", self.project, self.items.len());
        for i in &self.items {
            msg.push_str("  ");
            msg.push_str(i);
            msg.push('\n');
        }
        panic!("{msg}");
    }
}

fn acked(table: &[(&str, &str)], name: &str) -> bool {
    table.iter().any(|(n, _)| *n == name)
}

/// The checkout an environment variable names.
fn checkout(var: &str) -> PathBuf {
    let dir = std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("{var} is not set; run `make upstream-check`"));
    assert!(dir.is_dir(), "{var}: {} is not a folder", dir.display());
    dir
}

fn read(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join(rel))
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", root.join(rel).display()))
}

/// The part of one of hyprcustard's own sources before its tests, so names
/// that only appear in test fixtures do not count as used.
fn ours(source: &'static str) -> &'static str {
    source.split("#[cfg(test)]").next().unwrap_or(source)
}

fn caps1(re: &str, text: &str) -> BTreeSet<String> {
    Regex::new(re).unwrap().captures_iter(text).map(|c| c[1].to_string()).collect()
}

/// `SomeName` the way serde's or clap's `rename_all` writes it.
fn rename(ident: &str, rule: &str) -> String {
    let sep = match rule {
        "kebab-case" => Some('-'),
        "snake_case" => Some('_'),
        _ => None,
    };
    let mut out = String::new();
    for (i, ch) in ident.chars().enumerate() {
        if ch.is_ascii_uppercase() && i > 0 {
            if let Some(s) = sep {
                out.push(s);
            }
        }
        out.push(ch.to_ascii_lowercase());
    }
    match rule {
        "lowercase" | "kebab-case" | "snake_case" => out,
        _ => ident.to_string(),
    }
}

/// The variants of every `enum` in a Rust source, as written on the wire.
/// `default_rule` applies when the enum has no `rename_all` (clap's
/// subcommands and value enums are kebab-case by default).
fn rust_enums(source: &str, default_rule: &str) -> BTreeMap<String, Vec<String>> {
    let head = Regex::new(r"(?m)^\s*(?:pub\s+)?enum\s+(\w+)\s*\{").unwrap();
    let rename_all = Regex::new(r#"rename_all\s*=\s*"([\w-]+)""#).unwrap();
    let rename_one =
        Regex::new(r#"#\[(?:serde|command|value)\([^\]]*\b(?:rename|name)\s*=\s*"([^"]+)""#)
            .unwrap();
    let variant = Regex::new(r"^\s*([A-Z]\w*)\s*(?:[,({]|$)").unwrap();
    let mut out = BTreeMap::new();
    for c in head.captures_iter(source) {
        let start = c.get(0).unwrap().end();
        // The attributes just above the enum.
        let above = &source[..c.get(0).unwrap().start()];
        let attrs: Vec<&str> = above
            .lines()
            .rev()
            .take_while(|l| {
                let t = l.trim();
                t.starts_with("#[") || t.starts_with("///") || t.is_empty()
            })
            .collect();
        let rule = attrs
            .iter()
            .find_map(|l| rename_all.captures(l).map(|c| c[1].to_string()))
            .unwrap_or_else(|| default_rule.to_string());
        // The body, to the brace that closes it.
        let mut depth = 1;
        let mut end = start;
        for (i, ch) in source[start..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = start + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        let mut names = Vec::new();
        let mut pending: Option<String> = None;
        let mut nested = 0;
        for line in source[start..end].lines() {
            let t = line.trim();
            if nested == 0 {
                if let Some(r) = rename_one.captures(t) {
                    pending = Some(r[1].to_string());
                } else if let Some(v) = variant.captures(line) {
                    names.push(pending.take().unwrap_or_else(|| rename(&v[1], &rule)));
                }
            }
            nested += t.matches('{').count() as i32 - t.matches('}').count() as i32;
        }
        out.insert(c[1].to_string(), names);
    }
    out
}

// ---------------------------------------------------------------------------
// Fishsticks & Custard
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs an FSC checkout; run `make upstream-check`"]
fn fishsticks_and_custard() {
    let root = checkout("HYPRCUSTARD_UPSTREAM_FSC");
    let mut r = Report::new("Fishsticks & Custard");

    fsc_config(&root, &mut r);
    fsc_commands(&root, &mut r);
    fsc_desktop(&root, &mut r);
    fsc_packaging(&root, &mut r);

    r.finish();
}

/// The settings page against `config.example.toml` (FSC's documented
/// defaults) and the types in `config.rs`.
fn fsc_config(root: &Path, r: &mut Report) {
    let example: toml::Table =
        read(root, "packaging/config.example.toml").parse().expect("config.example.toml parses");
    let rust = read(root, "crates/fsc-core/src/config.rs");
    let enums = rust_enums(&rust, "PascalCase");

    // Each section's struct, from `pub claude: ClaudeConfig`.
    let section_re = Regex::new(r"pub\s+(\w+)\s*:\s*(\w+Config)\s*,").unwrap();
    let sections: BTreeMap<String, String> =
        section_re.captures_iter(&rust).map(|c| (c[1].to_string(), c[2].to_string())).collect();
    // Each field's type, per struct.
    let mut types: BTreeMap<String, String> = BTreeMap::new();
    let field_re = Regex::new(r"pub\s+(\w+)\s*:\s*([\w<>:]+)\s*,").unwrap();
    for (section, strukt) in &sections {
        let body = Regex::new(&format!(r"(?s)pub\s+struct\s+{strukt}\s*\{{(.*?)\n\}}"))
            .unwrap()
            .captures(&rust)
            .map(|c| c[1].to_string())
            .unwrap_or_default();
        for c in field_re.captures_iter(&body) {
            types.insert(format!("{section}.{}", &c[1]), c[2].to_string());
        }
    }

    let mut upstream: BTreeMap<String, toml::Value> = BTreeMap::new();
    for (section, table) in &example {
        if let Some(t) = table.as_table() {
            for (k, v) in t {
                upstream.insert(format!("{section}.{k}"), v.clone());
            }
        }
    }
    // Keys only in config.rs (commented out in the example, like `home`).
    for name in types.keys() {
        upstream.entry(name.clone()).or_insert(toml::Value::String(String::new()));
    }

    for f in fsc::FIELDS {
        let name = format!("{}.{}", f.section, f.key);
        let Some(theirs) = upstream.get(&name) else {
            r.gone(format!("config key {name}: the page offers it, FSC no longer reads it"));
            continue;
        };
        let mine: toml::Table = format!("v = {}", f.default).parse().expect("our default parses");
        let mine = &mine["v"];
        if example.get(f.section).and_then(|t| t.get(f.key)).is_some() && mine != theirs {
            r.changed(format!("config key {name}: default is {theirs}, the page says {mine}"));
        }
        // An optional key (FSC falls back to another, or to its default)
        // is edited as the value it holds.
        let ty = types.get(&name).map(String::as_str).unwrap_or("");
        let ty = ty.strip_prefix("Option<").and_then(|t| t.strip_suffix('>')).unwrap_or(ty);
        let kind_ok = match f.kind {
            Kind::Bool => ty == "bool",
            Kind::Int(..) => matches!(ty, "u8" | "u16" | "u32" | "u64" | "i32" | "i64" | "usize"),
            Kind::Float(..) => matches!(ty, "f32" | "f64"),
            Kind::Text => ty == "String",
            Kind::Choice(mine) => match enums.get(ty) {
                Some(theirs) => {
                    let a: BTreeSet<&str> = mine.iter().copied().collect();
                    let b: BTreeSet<&str> = theirs.iter().map(String::as_str).collect();
                    if a != b {
                        r.changed(format!(
                            "config key {name}: choices are {:?}, the page offers {:?}",
                            b, a
                        ));
                    }
                    true
                }
                None => false,
            },
        };
        if !kind_ok && !ty.is_empty() {
            r.changed(format!(
                "config key {name}: FSC reads it as {ty}, the page edits it as {:?}",
                f.kind
            ));
        }
    }

    for name in upstream.keys() {
        let offered = fsc::FIELDS.iter().any(|f| format!("{}.{}", f.section, f.key) == *name);
        if !offered && !acked(FSC_ACK_KEYS, name) {
            r.new_item(format!("config key {name}: FSC reads it, the page does not offer it"));
        }
    }
}

/// The `fsc` and `fscd` commands hyprcustard runs or binds, against the CLI.
fn fsc_commands(root: &Path, r: &mut Report) {
    let cli = read(root, "crates/fsc-cli/src/main.rs");
    let enums = rust_enums(&cli, "kebab-case");
    let commands: BTreeSet<String> =
        enums.get("Cmd").cloned().unwrap_or_default().into_iter().collect();
    assert!(!commands.is_empty(), "found no `enum Cmd` in fsc-cli");

    let src = ours(include_str!("fsc.rs"));
    let mut used: BTreeSet<String> = BTreeSet::new();
    for a in Action::ALL {
        used.insert(a.press().trim_start_matches("fsc ").to_string());
        if let Some(rel) = a.release() {
            used.insert(rel.trim_start_matches("fsc ").to_string());
        }
    }
    used.extend(caps1(r#""fsc"\.into\(\),\s*"([a-z][a-z-]*)""#, src));
    used.extend(caps1(r#"Command::new\("fsc"\)\s*\.arg\("([a-z][a-z-]*)"\)"#, src));
    used.extend(caps1(r#"\(Some\("([a-z][a-z-]*)"\)"#, src));

    for c in &used {
        if !commands.contains(c) {
            r.gone(format!("fsc {c}: hyprcustard runs it, fsc no longer has it"));
        }
    }
    for c in &commands {
        if !used.contains(c) && !acked(FSC_ACK_COMMANDS, c) {
            r.new_item(format!("fsc {c}: not offered as a key or button"));
        }
    }

    // `fsc mute toggle`, as hand-written lines say it.
    let mute: BTreeSet<String> =
        enums.get("MuteMode").cloned().unwrap_or_default().into_iter().collect();
    for m in caps1(r#"words\[2\] == "([a-z-]+)""#, src) {
        if !mute.contains(&m) {
            r.gone(format!("fsc mute {m}: hyprcustard reads it, fsc no longer takes it"));
        }
    }
    if !Regex::new(r#"#\[command\([^\]]*\bversion\b"#).unwrap().is_match(&cli) {
        r.gone("fsc --version: hyprcustard reads the installed version with it");
    }

    let app = read(root, "crates/fsc-app/src/main.rs");
    for flag in caps1(r#""fscd"\.into\(\),\s*"(--[a-z-]+)""#, src) {
        if !app.contains(&format!("\"{flag}\"")) {
            r.gone(format!("fscd {flag}: hyprcustard runs it, fscd no longer takes it"));
        }
    }
}

/// The popup's window class and the keys FSC's own snippet suggests.
fn fsc_desktop(root: &Path, r: &mut Report) {
    let app = read(root, "crates/fsc-app/src/main.rs");
    match Regex::new(r#"APP_ID:\s*&str\s*=\s*"([^"]+)""#).unwrap().captures(&app) {
        Some(c) if &c[1] != fsc::WINDOW_CLASS => r.changed(format!(
            "window class is {}, the window rule matches {}",
            &c[1],
            fsc::WINDOW_CLASS
        )),
        Some(_) => {}
        None => r.gone("APP_ID: not found in fsc-app; check the window rule's class"),
    }

    let lua = read(root, "packaging/hyprland/fsc.lua");
    let bind = Regex::new(
        r#"(?m)^\s*hl\.bind\("([^"]+)",\s*hl\.dsp\.exec_cmd\("fsc ([a-z-]+)"\)(,\s*\{\s*release\s*=\s*true\s*\})?\)"#,
    )
    .unwrap();
    let mut suggested: BTreeMap<String, String> = BTreeMap::new();
    for c in bind.captures_iter(&lua) {
        if c.get(3).is_none() {
            suggested.insert(c[2].to_string(), c[1].to_string());
        }
    }
    for a in Action::ALL {
        let cmd = a.press().trim_start_matches("fsc ").to_string();
        let (key, on) = a.default_key();
        match (suggested.get(&cmd), on) {
            (Some(k), true) if k != key => {
                r.changed(format!("default key for fsc {cmd}: FSC suggests {k}, the page {key}"))
            }
            (Some(k), false) => {
                r.new_item(format!("default key for fsc {cmd}: FSC suggests {k}, the page none"))
            }
            (None, true) => {
                r.changed(format!("default key for fsc {cmd}: FSC suggests none, the page {key}"))
            }
            _ => {}
        }
    }

    // The model files the page counts, by pattern.
    let models = read(root, "crates/fsc-core/src/models.rs");
    let theirs = caps1(r#"models_dir\(\)\.join\((?:format!\()?"([^"]+)""#, &models);
    let whisper = fsc::FIELDS.iter().find(|f| f.key == "whisper_model").unwrap().default;
    let piper = fsc::FIELDS.iter().find(|f| f.key == "piper_voice").unwrap().default;
    let mine: BTreeSet<String> = fsc::needed_models(&Map::new())
        .into_iter()
        .map(|m| m.replace(whisper.trim_matches('"'), "{}").replace(piper.trim_matches('"'), "{}"))
        .collect();
    for m in mine.difference(&theirs) {
        r.gone(format!("model file {m}: the page counts it, FSC no longer downloads it"));
    }
    for m in theirs.difference(&mine) {
        r.new_item(format!("model file {m}: FSC downloads it, the page does not count it"));
    }
}

/// The package, its signing key and its repository.
fn fsc_packaging(root: &Path, r: &mut Report) {
    let pkgbuild = read(root, "packaging/arch/PKGBUILD");
    if !pkgbuild.lines().any(|l| l.trim() == format!("pkgname={}", fsc::PACKAGE)) {
        r.changed(format!("package name: PKGBUILD does not build {}", fsc::PACKAGE));
    }
    let fpr = read(root, "packaging/arch/signing-key.fpr");
    if fpr.trim() != fsc::KEY_FINGERPRINT {
        r.changed(format!(
            "signing key: FSC signs with {}, hyprcustard trusts {}",
            fpr.trim(),
            fsc::KEY_FINGERPRINT
        ));
    }
    let key_path = fsc::KEY_URL.split("/main/").nth(1).unwrap_or("");
    if !root.join(key_path).is_file() {
        r.gone(format!("signing key file {key_path}: hyprcustard downloads it"));
    }
    let tag = fsc::REPO_SERVER.rsplit('/').next().unwrap_or("");
    let tags = std::process::Command::new("git")
        .args(["-C", &root.to_string_lossy(), "tag", "-l", tag])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    if tags != tag {
        r.gone(format!("pacman repository: no `{tag}` release to serve it from"));
    }
}

// ---------------------------------------------------------------------------
// HyprWindowShade
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs a HyprWindowShade checkout; run `make upstream-check`"]
fn hyprwindowshade() {
    let root = checkout("HYPRCUSTARD_UPSTREAM_HWS");
    let mut r = Report::new("HyprWindowShade");

    hws_tags(&root, &mut r);
    hws_lua(&root, &mut r);
    hws_shaders(&root, &mut r);
    hws_manifest(&root, &mut r);

    r.finish();
}

/// What `@<seconds>` on a tag sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Suffix {
    None,
    Duration,
    Settle,
}

impl Suffix {
    fn what(self) -> &'static str {
        match self {
            Suffix::None => "nothing",
            Suffix::Duration => "the duration",
            Suffix::Settle => "the settle tail",
        }
    }
}

/// The window-rule tags, against the plugin's parser in Hooks.cpp.
fn hws_tags(root: &Path, r: &mut Report) {
    let hooks = read(root, "Hooks.cpp");
    let re = Regex::new(
        r#"(!isDefault\s*&&\s*)?key\s*==\s*"(shader[a-z_]*)"\)\s*(?:(assign|assignAnim)\(\s*dst\.\w+\s*(?:,\s*dst\.(\w+))?)?"#,
    )
    .unwrap();
    // name -> (has a _default form, a path, what @sec sets)
    let mut theirs: BTreeMap<String, (bool, bool, Suffix)> = BTreeMap::new();
    for c in re.captures_iter(&hooks) {
        let suffix = match c.get(4).map(|m| m.as_str()) {
            Some(s) if s.ends_with("Duration") => Suffix::Duration,
            Some(s) if s.ends_with("Settle") => Suffix::Settle,
            _ => Suffix::None,
        };
        theirs.insert(c[2].to_string(), (c.get(1).is_none(), c.get(3).is_some(), suffix));
    }
    assert!(theirs.contains_key("shader"), "found no tag parser in Hooks.cpp");

    for (_, info) in TAGS {
        let Some(&(default, path, suffix)) = theirs.get(info.key) else {
            r.gone(format!(
                "tag {}: the rules page writes it, the plugin no longer reads it",
                info.key
            ));
            continue;
        };
        if info.supports_default != default {
            r.changed(format!(
                "tag {}: the plugin {} a _default form, the page says it {}",
                info.key,
                if default { "takes" } else { "has no" },
                if info.supports_default { "does" } else { "does not" }
            ));
        }
        let mine_path = matches!(info.kind, TagKind::Path);
        if mine_path != path {
            r.changed(format!(
                "tag {}: the plugin takes {}, the page writes {}",
                info.key,
                if path { "a shader path" } else { "a flag" },
                if mine_path { "a shader path" } else { "a flag" }
            ));
        }
        let mine = match info.seconds {
            Seconds::None => Suffix::None,
            Seconds::Duration => Suffix::Duration,
            Seconds::Settle => Suffix::Settle,
        };
        if mine != suffix {
            r.changed(format!(
                "tag {}: @<seconds> on it sets {}, the page treats it as {}",
                info.key,
                suffix.what(),
                mine.what()
            ));
        }
    }
    for name in theirs.keys() {
        if !TAGS.iter().any(|(_, i)| i.key == name) {
            r.new_item(format!(
                "tag {name}: the plugin reads it, the rules page does not offer it"
            ));
        }
    }
}

/// The Lua functions the emitter calls, against the ones main.cpp registers.
fn hws_lua(root: &Path, r: &mut Report) {
    let main = read(root, "main.cpp");
    let reg = Regex::new(r#"addLuaFunction\(\s*\w+\s*,\s*"(\w+)"\s*,\s*"(\w+)""#).unwrap();
    let mut theirs = BTreeSet::new();
    let mut namespaces = BTreeSet::new();
    for c in reg.captures_iter(&main) {
        namespaces.insert(c[1].to_string());
        theirs.insert(c[2].to_string());
    }
    assert!(!theirs.is_empty(), "found no addLuaFunction in main.cpp");

    let emit = ours(include_str!("emit.rs"));
    for ns in caps1(r"hl\.plugin\.(\w+)\.", emit) {
        if !namespaces.contains(&ns) {
            r.changed(format!("Lua namespace hl.plugin.{ns}: the plugin registers {namespaces:?}"));
        }
    }
    let mine = caps1(r"\{NS_VAR\}\.(\w+)\(", emit);
    for f in mine.difference(&theirs) {
        r.gone(format!("Lua {f}(): hyprcustard writes it, the plugin no longer registers it"));
    }
    for f in theirs.difference(&mine) {
        r.new_item(format!("Lua {f}(): the plugin registers it, hyprcustard never writes it"));
    }
}

/// Shader directives and uniforms, against ShaderEngine.cpp.
fn hws_shaders(root: &Path, r: &mut Report) {
    let engine = read(root, "ShaderEngine.cpp");

    let mut directives = BTreeSet::new();
    for line in engine.lines().filter(|l| l.contains("std::regex")) {
        directives.extend(caps1(r"@([a-z_]+)", line));
    }
    let meta = ours(include_str!("shader/meta.rs"));
    let arm =
        Regex::new(r#"(?m)^\s*("[a-z]+"(?:\s*\|\s*"[a-z]+")*)(?:\s+if\s[^=]*)?\s*=>"#).unwrap();
    let mut known = BTreeSet::new();
    for c in arm.captures_iter(meta) {
        known.extend(caps1(r#""([a-z]+)""#, &c[1]));
    }
    for d in &directives {
        if !known.contains(d) {
            r.new_item(format!(
                "directive // @{d}: the plugin reads it, the shader editor does not"
            ));
        }
    }

    let theirs = caps1(r#"UniformLocation\(\s*\w+\s*,\s*"(\w+)""#, &engine);
    let mine: BTreeSet<String> = KNOWN_UNIFORMS.iter().map(|(n, _)| n.to_string()).collect();
    for u in mine.difference(&theirs) {
        r.gone(format!("uniform {u}: the shader editor lists it, the plugin no longer sets it"));
    }
    for u in theirs.difference(&mine) {
        if !acked(HWS_ACK_UNIFORMS, u) {
            r.new_item(format!(
                "uniform {u}: the plugin sets it, the shader editor does not list it"
            ));
        }
    }
}

/// The hyprpm manifest's names.
fn hws_manifest(root: &Path, r: &mut Report) {
    let manifest: toml::Table = read(root, "hyprpm.toml").parse().expect("hyprpm.toml parses");
    let repo = manifest
        .get("repository")
        .and_then(|t| t.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !DEFAULT_PLUGIN_REPO.ends_with(&format!("/{repo}")) {
        r.changed(format!("hyprpm repository is {repo}, hyprcustard adds {DEFAULT_PLUGIN_REPO}"));
    }
    let plugins: Vec<&String> = manifest.keys().filter(|k| *k != "repository").collect();
    if !plugins.iter().any(|p| *p == DEFAULT_PLUGIN_NAME) {
        r.changed(format!(
            "hyprpm plugins are {plugins:?}, hyprcustard enables {DEFAULT_PLUGIN_NAME}"
        ));
    }
}

mod tests {
    use super::*;

    #[test]
    fn renames_like_serde() {
        assert_eq!(rename("TopLeft", "kebab-case"), "top-left");
        assert_eq!(rename("PoliceBox", "kebab-case"), "police-box");
        assert_eq!(rename("Window", "lowercase"), "window");
        assert_eq!(rename("FetchModels", "kebab-case"), "fetch-models");
    }

    #[test]
    fn reads_enums() {
        let src = r#"
#[derive(Subcommand)]
enum Cmd {
    /// Show it
    Toggle,
    Ask {
        #[arg(required = true)]
        text: Vec<String>,
    },
    FetchModels,
}

#[serde(rename_all = "lowercase")]
pub enum Speak {
    Voice,
    #[serde(rename = "all")]
    Always,
}
"#;
        let e = rust_enums(src, "kebab-case");
        assert_eq!(e["Cmd"], vec!["toggle", "ask", "fetch-models"]);
        assert_eq!(e["Speak"], vec!["voice", "all"]);
    }
}
