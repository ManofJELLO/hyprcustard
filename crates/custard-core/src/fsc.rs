//! Fishsticks & Custard: the voice assistant, from one page.
//!
//! Fishsticks & Custard is two programs: `fscd`, the assistant itself, and
//! `fsc`, which tells a running `fscd` what to do and is what key bindings
//! call. Setting it up by hand takes four steps: trust its signing key and
//! add its pacman repository, install it, start `fscd` with Hyprland and bind
//! its keys, and edit `~/.config/fsc/config.toml`. This module does each.
//!
//! - **Install**: a [`Run`] for the job runner, so pacman's output streams
//!   into the window and sudo asks the window for the password.
//! - **Start and keys**: a managed block in the Hyprland config, written
//!   from [`FscSettings`]. Lines the user wrote by hand are found and can be
//!   handed over: commented out with a marker, so giving them back restores
//!   them exactly, the way the wallpaper's start lines are.
//! - **Its config**: read and edited with `toml_edit`, so the comments in
//!   the file stay where they are. Only what the user changes is written;
//!   a key left at its default is never added.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::block::{self, Markers};
use crate::error::{Error, Result};
use crate::hyprpm::Run;
use crate::model::lua_quote;
use crate::paths;
use crate::walls::startup::{lines_outside_blocks, lua_string, movable, split_keep};

/// The package, and the pacman repository it comes from.
pub const PACKAGE: &str = "fishsticks-and-custard";
/// The project's page.
pub const HOME_PAGE: &str = "https://github.com/ManofJELLO/fishsticks-and-custard";
/// The key the packages are signed with. A downloaded key is checked
/// against this before pacman is told to trust it.
pub const KEY_FINGERPRINT: &str = "3286286EB33C3D8E9B47D6EF0A83688698B6FA30";
/// Where the signing key is published.
pub const KEY_URL: &str = "https://raw.githubusercontent.com/ManofJELLO/fishsticks-and-custard/main/packaging/arch/fishsticks-and-custard.asc";
/// The repository's `Server` line.
pub const REPO_SERVER: &str =
    "https://github.com/ManofJELLO/fishsticks-and-custard/releases/download/repo";
/// pacman's own config.
pub const PACMAN_CONF: &str = "/etc/pacman.conf";

/// The managed block's markers.
pub const MARKERS: Markers = Markers {
    begin: "-- >>> Fishsticks & Custard (managed by hyprcustard) >>>",
    end: "-- <<< Fishsticks & Custard (managed by hyprcustard) <<<",
    name: "Fishsticks & Custard",
};

/// What a handed-over line starts with, after its indentation.
pub const MOVED: &str = "-- [moved to the Fishsticks & Custard block] ";

// ---------------------------------------------------------------------------
// Start and keys
// ---------------------------------------------------------------------------

/// Something a key can make the assistant do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Open the popup with the text field focused.
    Type,
    /// Hold to talk; a quick tap listens until you stop talking.
    Talk,
    /// Show the popup, or hide it.
    Toggle,
    /// Start a fresh conversation.
    New,
    /// Stop listening, or interrupt the reply.
    Stop,
    /// Mute or unmute the wake word.
    Mute,
}

impl Action {
    /// Every action, in the order the page lists them.
    pub const ALL: [Action; 6] =
        [Action::Type, Action::Talk, Action::Toggle, Action::New, Action::Stop, Action::Mute];

    /// What the page calls it.
    pub fn label(self) -> &'static str {
        match self {
            Action::Type => "Type to it",
            Action::Talk => "Talk (hold)",
            Action::Toggle => "Show or hide",
            Action::New => "New conversation",
            Action::Stop => "Stop",
            Action::Mute => "Mute the wake word",
        }
    }

    /// The `fsc` command a key press runs.
    fn press(self) -> &'static str {
        match self {
            Action::Type => "fsc type",
            Action::Talk => "fsc listen",
            Action::Toggle => "fsc toggle",
            Action::New => "fsc new",
            Action::Stop => "fsc stop",
            Action::Mute => "fsc mute",
        }
    }

    /// The `fsc` command letting go of the key runs, for hold-to-talk.
    fn release(self) -> Option<&'static str> {
        (self == Action::Talk).then_some("fsc release")
    }

    /// The key FSC's own Hyprland snippet suggests, and whether it is on
    /// unless someone says otherwise.
    fn default_key(self) -> (&'static str, bool) {
        match self {
            Action::Type => ("SUPER + A", true),
            Action::Talk => ("SUPER + SHIFT + A", true),
            _ => ("", false),
        }
    }
}

/// One key binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Key {
    /// What it does.
    pub action: Action,
    /// The combination, in `hl.bind` syntax: `SUPER + SHIFT + A`.
    pub key: String,
    /// Off keeps the key here without binding it.
    pub enabled: bool,
}

/// The page's choices, kept in hyprcustard's settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FscSettings {
    /// Start `fscd` with Hyprland.
    pub autostart: bool,
    /// Float the popup window, keep it on every workspace and put it at the
    /// top center, as FSC's own snippet does.
    pub window_rule: bool,
    /// One per [`Action`], in [`Action::ALL`]'s order.
    pub keys: Vec<Key>,
}

impl Default for FscSettings {
    fn default() -> Self {
        Self {
            autostart: true,
            window_rule: false,
            keys: Action::ALL
                .iter()
                .map(|&action| {
                    let (key, enabled) = action.default_key();
                    Key { action, key: key.into(), enabled }
                })
                .collect(),
        }
    }
}

impl FscSettings {
    /// Every action once, in order, with what was there kept.
    pub fn sanitized(mut self) -> Self {
        let mut keys = Vec::new();
        for action in Action::ALL {
            let found = self.keys.iter().find(|k| k.action == action).cloned();
            keys.push(found.unwrap_or_else(|| {
                let (key, enabled) = action.default_key();
                Key { action, key: key.into(), enabled }
            }));
        }
        for k in &mut keys {
            k.key = k.key.trim().to_string();
            if k.key.is_empty() {
                k.enabled = false;
            }
        }
        self.keys = keys;
        self
    }

    /// Whether the block would have anything in it.
    pub fn any(&self) -> bool {
        self.autostart || self.window_rule || self.keys.iter().any(|k| k.enabled)
    }
}

/// The managed block's body.
pub fn body(s: &FscSettings) -> String {
    let mut out = String::from(
        "-- Fishsticks & Custard, set up on hyprcustard's Fishsticks & Custard page.\n\
         -- Change it there; edits made here are replaced.\n",
    );
    if s.autostart {
        out.push_str("hl.on(\"hyprland.start\", function()\n    hl.exec_cmd(\"fscd\")\nend)\n");
    }
    if s.window_rule {
        out.push_str(
            "hl.window_rule({\n    name  = \"fsc-window\",\n    \
             match = { class = \"^(dev\\\\.fsc\\\\.FishsticksAndCustard)$\" },\n    \
             float = true,\n    pin   = true,\n    \
             move  = \"(monitor_w*0.5-window_w*0.5) 80\",\n})\n",
        );
    }
    for k in s.keys.iter().filter(|k| k.enabled && !k.key.is_empty()) {
        let key = lua_quote(&k.key);
        out.push_str(&format!(
            "hl.bind({key}, hl.dsp.exec_cmd({}))\n",
            lua_quote(k.action.press())
        ));
        if let Some(release) = k.action.release() {
            out.push_str(&format!(
                "hl.bind({key}, hl.dsp.exec_cmd({}), {{ release = true }})\n",
                lua_quote(release)
            ));
        }
    }
    out
}

/// Whether the managed block is there.
pub fn managed(text: &str) -> bool {
    matches!(block::split_with(text, &MARKERS), Ok(Some(_)))
}

/// The managed block as it is in the config: the line its opening marker is
/// on (1-based) and what is between the markers.
pub fn block_text(text: &str) -> Option<(usize, String)> {
    let split = block::split_with(text, &MARKERS).ok()??;
    let line = split.before.lines().count() + 1;
    Some((line, split.inner.trim_end().to_string()))
}

/// The config with the block written from `s`, or taken out when there is
/// nothing to put in it.
pub fn apply(text: &str, s: &FscSettings) -> Result<String> {
    if s.any() {
        block::splice_with(text, &body(s), &MARKERS)
    } else {
        block::remove_with(text, &MARKERS)
    }
}

/// What one of the user's own lines does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Does {
    /// Starts `fscd`.
    Start,
    /// Binds a key to an action. `release` is the let-go half of
    /// hold-to-talk.
    Bind {
        /// The action.
        action: Action,
        /// The combination.
        key: String,
        /// Whether it runs when the key is let go.
        release: bool,
    },
    /// Runs `fsc` some other way: kept as it is, not taken over.
    Other,
}

/// A line in the user's config that starts or drives the assistant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Line {
    /// 1-based.
    pub number: usize,
    /// The line, trimmed.
    pub text: String,
    /// Whether it is one statement on a line of its own, so that commenting
    /// it out cannot break the code around it.
    pub movable: bool,
    /// What it does.
    pub does: Does,
}

/// Lines outside every managed block, not commented out, that start `fscd`
/// or run `fsc`.
pub fn user_lines(text: &str) -> Vec<Line> {
    lines_outside_blocks(text)
        .filter_map(|(number, l)| {
            let t = l.trim();
            if t.starts_with("--") || t.starts_with('#') || !t.contains("exec") {
                return None;
            }
            let does = classify(t)?;
            let movable = match does {
                Does::Start => movable(l),
                _ => one_bind(t),
            };
            Some(Line { number, text: t.to_string(), movable, does })
        })
        .collect()
}

/// Lines hyprcustard commented out when it took over.
pub fn moved_lines(text: &str) -> Vec<usize> {
    text.lines()
        .enumerate()
        .filter(|(_, l)| l.trim_start().starts_with(MOVED))
        .map(|(i, _)| i + 1)
        .collect()
}

/// What a line does, if it runs `fscd` or `fsc` at all.
fn classify(t: &str) -> Option<Does> {
    let at = t.find("exec_cmd(")?;
    let command = lua_string(t[at + "exec_cmd(".len()..].trim_start())?;
    let words: Vec<&str> = command.split_whitespace().collect();
    let program = words.first()?.rsplit('/').next()?;
    if let Some(rest) = t.strip_prefix("hl.bind(") {
        if program != "fsc" {
            return None;
        }
        let key = lua_string(rest.trim_start())?;
        let release = release_flag(t);
        let action = match (words.get(1).copied(), words.len(), release) {
            (Some("type"), 2, false) => Some(Action::Type),
            (Some("listen"), 2, false) | (Some("release"), 2, true) => Some(Action::Talk),
            (Some("toggle"), 2, false) => Some(Action::Toggle),
            (Some("new"), 2, false) => Some(Action::New),
            (Some("stop"), 2, false) => Some(Action::Stop),
            (Some("mute"), 2, false) => Some(Action::Mute),
            (Some("mute"), 3, false) if words[2] == "toggle" => Some(Action::Mute),
            _ => None,
        };
        return Some(match action {
            Some(action) => Does::Bind { action, key, release },
            None => Does::Other,
        });
    }
    match program {
        "fscd" if words.len() == 1 => Some(Does::Start),
        "fscd" | "fsc" => Some(Does::Other),
        _ => None,
    }
}

/// Whether a bind's options say it runs on release.
fn release_flag(t: &str) -> bool {
    let squeezed: String = t.chars().filter(|c| !c.is_whitespace()).collect();
    squeezed.contains("release=true")
}

/// Whether a line is one `hl.bind(...)` and nothing else, perhaps with a
/// trailing comment.
fn one_bind(t: &str) -> bool {
    let t = match (t.rfind(')'), t.rfind("--")) {
        (Some(close), Some(dash)) if dash > close => t[..dash].trim_end(),
        _ => t,
    };
    t.starts_with("hl.bind(")
        && t.ends_with(')')
        && t.matches("hl.bind(").count() == 1
        && t.matches('(').count() == t.matches(')').count()
        && !t.contains(';')
        && !t.contains("function")
}

/// Comment the user's lines out and write the block in their place.
///
/// The lines' keys and whether they start `fscd` are taken into `s`, which
/// is returned, so the block does what the lines did. Lines that run `fsc`
/// some other way are left alone. Refused, with nothing changed, when a
/// line to move shares its line with other code.
pub fn hand_over(text: &str, s: &FscSettings) -> Result<(String, FscSettings)> {
    let lines: Vec<Line> = user_lines(text).into_iter().filter(|l| l.does != Does::Other).collect();
    let stuck: Vec<String> =
        lines.iter().filter(|l| !l.movable).map(|l| l.number.to_string()).collect();
    if !stuck.is_empty() {
        return Err(Error::other(format!(
            "{} {} of hyprland.lua runs Fishsticks & Custard alongside other code, so it \
             cannot be commented out on its own. Put it on a line of its own, or remove it \
             yourself, and try again",
            if stuck.len() == 1 { "Line" } else { "Lines" },
            stuck.join(", ")
        )));
    }

    let mut next = s.clone().sanitized();
    if !lines.is_empty() {
        // What the lines did, and nothing else: a key they did not bind
        // stays off rather than appearing beside them.
        next.autostart = false;
        for k in &mut next.keys {
            k.enabled = false;
        }
    }
    for l in &lines {
        match &l.does {
            Does::Start => next.autostart = true,
            Does::Bind { action, key, release } => {
                // The press half names the key; a lone release line does too.
                if let Some(k) = next.keys.iter_mut().find(|k| k.action == *action) {
                    if !*release || !k.enabled {
                        k.key = key.clone();
                        k.enabled = true;
                    }
                }
            }
            Does::Other => {}
        }
    }

    let numbers: Vec<usize> = lines.iter().map(|l| l.number).collect();
    let mut out = String::new();
    for (i, l) in split_keep(text).into_iter().enumerate() {
        if numbers.contains(&(i + 1)) {
            let indent = &l[..l.len() - l.trim_start().len()];
            out.push_str(&format!("{indent}{MOVED}{}", &l[indent.len()..]));
        } else {
            out.push_str(l);
        }
    }
    let written = if next.any() { block::splice_with(&out, &body(&next), &MARKERS)? } else { out };
    Ok((written, next))
}

/// Take the block out and put the user's lines back as they were.
pub fn give_back(text: &str) -> Result<String> {
    let text = block::remove_with(text, &MARKERS)?;
    Ok(split_keep(&text)
        .into_iter()
        .map(|l| {
            let indent = &l[..l.len() - l.trim_start().len()];
            match l[indent.len()..].strip_prefix(MOVED) {
                Some(rest) => format!("{indent}{rest}"),
                None => l.to_string(),
            }
        })
        .collect())
}

/// Hyprland's modifier bits, as `hyprctl binds -j` reports them.
fn modmask(key: &str) -> Option<(u32, String)> {
    let mut mask = 0;
    let mut last = None;
    for part in key.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        let bit = match part.to_ascii_uppercase().as_str() {
            "SHIFT" => 1,
            "CAPS" => 2,
            "CTRL" | "CONTROL" => 4,
            "ALT" | "MOD1" => 8,
            "MOD2" => 16,
            "MOD3" => 32,
            "SUPER" | "WIN" | "LOGO" | "MOD4" | "META" => 64,
            "MOD5" => 128,
            _ => 0,
        };
        if bit == 0 {
            last = Some(part.to_string());
        } else {
            mask |= bit;
        }
    }
    last.map(|k| (mask, k))
}

/// For each enabled key, the other lines in the Hyprland config that bind
/// it: every `hl.bind` outside this block that does not run `fsc`.
///
/// Read from the file rather than asked of Hyprland: a Lua bind reports
/// only `HL.Dispatcher(exec_cmd) 53`, which says nothing about what it runs,
/// and the file is what the next reload will do anyway.
pub fn clashes(s: &FscSettings, config: &str) -> Vec<Value> {
    let mut ours = false;
    let mut binds: Vec<(usize, (u32, String), &str)> = Vec::new();
    for (i, l) in config.lines().enumerate() {
        let t = l.trim();
        if t == MARKERS.begin {
            ours = true;
            continue;
        }
        if t == MARKERS.end {
            ours = false;
            continue;
        }
        let Some(rest) = t.strip_prefix("hl.bind(") else { continue };
        if ours || classify(t).is_some() {
            continue;
        }
        if let Some(mm) = lua_string(rest.trim_start()).as_deref().and_then(modmask) {
            binds.push((i + 1, mm, t));
        }
    }
    let mut out = Vec::new();
    for k in s.keys.iter().filter(|k| k.enabled) {
        let Some((mask, key)) = modmask(&k.key) else { continue };
        let taken: Vec<String> = binds
            .iter()
            .filter(|(_, (m, bk), _)| *m == mask && bk.eq_ignore_ascii_case(&key))
            .map(|(n, _, t)| format!("line {n}: {t}"))
            .collect();
        if !taken.is_empty() {
            out.push(json!({ "action": k.action, "key": k.key, "taken": taken }));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// What is installed
// ---------------------------------------------------------------------------

/// What this machine has, read when the page asks.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Facts {
    /// The version `fsc --version` reports, when it is installed.
    pub installed: Option<String>,
    /// Whether pacman is here, which is what the install button uses.
    pub pacman: bool,
    /// Whether pacman.conf has the repository.
    pub repo_configured: bool,
    /// Whether pacman trusts the signing key.
    pub key_trusted: bool,
    /// Whether `fscd` is running.
    pub running: bool,
    /// Where the Claude Code CLI is, if it is on PATH.
    pub claude: Option<String>,
    /// Where the speech models go.
    pub models_dir: String,
    /// The model files the current config needs and does not have.
    pub missing_models: Vec<String>,
    /// The whisper models already downloaded, by name.
    pub whisper_models: Vec<String>,
}

/// Where FSC keeps its config.
pub fn config_path() -> PathBuf {
    paths::apps_config_home().join("fsc").join("config.toml")
}

/// Where FSC keeps its speech models.
pub fn models_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| paths::expand("~/.local/share")).join("fsc").join("models")
}

/// The model files a config needs, by file name.
pub fn needed_models(config: &Map<String, Value>) -> Vec<String> {
    let get = |section: &str, key: &str| {
        config
            .get(&format!("{section}.{key}"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| default_of(section, key).as_str().unwrap_or("").to_string())
    };
    let voice = get("voice", "piper_voice");
    vec![
        format!("ggml-{}.bin", get("voice", "whisper_model")),
        "silero_vad.onnx".into(),
        format!("{voice}.onnx"),
        format!("{voice}.onnx.json"),
    ]
}

/// Read what is installed. `values` is FSC's config as [`config_values`]
/// read it, for which models it needs.
pub fn facts(values: &Map<String, Value>) -> Facts {
    let dir = models_dir();
    let mut whisper_models: Vec<String> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            Some(name.strip_prefix("ggml-")?.strip_suffix(".bin")?.to_string())
        })
        .collect();
    whisper_models.sort();
    let claude_bin = values
        .get("claude.bin")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| "claude".into());
    let pacman = paths::which("pacman").is_some();
    Facts {
        installed: version(),
        pacman,
        repo_configured: std::fs::read_to_string(PACMAN_CONF).is_ok_and(|t| has_repo(&t, PACKAGE)),
        key_trusted: pacman && key_trusted(),
        running: !crate::procs::named("fscd").is_empty(),
        claude: paths::which(&claude_bin)
            .or_else(|| {
                let p = paths::expand(&claude_bin);
                p.is_file().then_some(p)
            })
            .map(|p| paths::contract(&p)),
        models_dir: paths::contract(&dir),
        missing_models: needed_models(values)
            .into_iter()
            .filter(|f| !dir.join(f).exists())
            .collect(),
        whisper_models,
    }
}

/// Whether pacman.conf has a section for the repository, not commented out.
pub fn has_repo(conf: &str, repo: &str) -> bool {
    let header = format!("[{repo}]");
    conf.lines().any(|l| l.trim() == header)
}

fn version() -> Option<String> {
    let out = std::process::Command::new("fsc")
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    text.split_whitespace().nth(1).map(str::to_string)
}

fn key_trusted() -> bool {
    std::process::Command::new("pacman-key")
        .args(["--list-keys", KEY_FINGERPRINT])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

// ---------------------------------------------------------------------------
// Jobs
// ---------------------------------------------------------------------------

/// Install, or update: trust the key and add the repository if they are
/// not already there, then install with a full system upgrade, which is the
/// only kind of install Arch supports once the package lists are synced.
///
/// Arguments: fingerprint, key URL, repository, server, pacman.conf.
const INSTALL: &str = r#"set -eu
fpr="$1"; key_url="$2"; repo="$3"; server="$4"; conf="$5"
if pacman-key --list-keys "$fpr" >/dev/null 2>&1; then
    echo "The packaging key is already trusted."
else
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    echo "Fetching the packaging key from $key_url"
    curl -fsSL "$key_url" -o "$tmp/key.asc"
    got=$(gpg --batch --with-colons --show-keys "$tmp/key.asc" 2>/dev/null \
        | awk -F: '$1 == "fpr" { print $10; exit }')
    if [ "$got" != "$fpr" ]; then
        echo "The key downloaded is ${got:-unreadable}, not the expected $fpr. Nothing was changed." >&2
        exit 1
    fi
    echo "Trusting key $fpr"
    sudo -A pacman-key --add "$tmp/key.asc"
    sudo -A pacman-key --lsign-key "$fpr"
fi
if grep -qx "[[:space:]]*\[$repo\][[:space:]]*" "$conf"; then
    echo "$conf already has [$repo]."
else
    echo "Adding [$repo] to $conf (a copy of it is kept as $conf.hyprcustard.bak)"
    sudo -A cp "$conf" "$conf.hyprcustard.bak"
    printf '\n[%s]\nServer = %s\n' "$repo" "$server" | sudo -A tee -a "$conf" >/dev/null
fi
echo "Installing $repo, and upgrading the system with it"
sudo -A pacman -Syu --needed --noconfirm "$repo"
"#;

/// A job the page can run: `install`, `remove`, `models` or `selftest`.
pub fn job(op: &str) -> Result<Run> {
    let sh = |label: &str, script: &str, args: &[&str]| {
        let mut v = vec!["/bin/sh".to_string(), "-c".into(), script.to_string(), "fsc-job".into()];
        v.extend(args.iter().map(|a| a.to_string()));
        Run { label: label.to_string(), args: v }
    };
    Ok(match op {
        "install" => sh(
            "Installing Fishsticks & Custard",
            INSTALL,
            &[KEY_FINGERPRINT, KEY_URL, PACKAGE, REPO_SERVER, PACMAN_CONF],
        ),
        "remove" => Run {
            label: "Removing Fishsticks & Custard".into(),
            args: ["sudo", "-A", "pacman", "-Rns", "--noconfirm", PACKAGE]
                .map(String::from)
                .to_vec(),
        },
        "models" => Run {
            label: "Downloading the speech models".into(),
            args: vec!["fsc".into(), "fetch-models".into()],
        },
        "selftest" => Run {
            label: "Testing speech: Piper speaks, whisper listens".into(),
            args: vec!["fscd".into(), "--self-test".into()],
        },
        other => return Err(Error::other(format!("unknown Fishsticks & Custard job `{other}`"))),
    })
}

/// Stop `fscd` and start it again, so it reads its config: it only does so
/// when it starts.
pub fn restart() -> Result<String> {
    let was_running = !crate::procs::named("fscd").is_empty();
    if was_running {
        let _ = std::process::Command::new("fsc")
            .arg("quit")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
        while !crate::procs::named("fscd").is_empty() {
            if std::time::Instant::now() > deadline {
                return Err(Error::other(
                    "fscd did not quit when asked; it is still running with the old settings",
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    start()?;
    Ok(if was_running { "Restarted fscd".into() } else { "Started fscd".into() })
}

/// Start `fscd` on its own, so it outlives this window.
pub fn start() -> Result<()> {
    let mut command = match paths::which("setsid") {
        Some(setsid) => {
            let mut c = std::process::Command::new(setsid);
            c.args(["-f", "fscd"]);
            c
        }
        None => {
            use std::os::unix::process::CommandExt;
            let mut c = std::process::Command::new("fscd");
            c.process_group(0);
            c
        }
    };
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| Error::other(format!("could not start fscd: {e}")))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Its config
// ---------------------------------------------------------------------------

/// What kind of value a setting takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// true or false.
    Bool,
    /// A whole number in a range.
    Int(i64, i64),
    /// A number in a range.
    Float(f64, f64),
    /// Any text.
    Text,
    /// One of a few words.
    Choice(&'static [&'static str]),
}

/// One setting in FSC's config.
#[derive(Debug, Clone, Copy)]
pub struct Field {
    /// Its table: `claude`, `ui` or `voice`.
    pub section: &'static str,
    /// Its key in that table.
    pub key: &'static str,
    /// What the page calls it.
    pub label: &'static str,
    /// A line under it.
    pub hint: &'static str,
    /// What it takes.
    pub kind: Kind,
    /// FSC's default, as TOML would write it.
    pub default: &'static str,
}

const fn f(
    section: &'static str,
    key: &'static str,
    label: &'static str,
    hint: &'static str,
    kind: Kind,
    default: &'static str,
) -> Field {
    Field { section, key, label, hint, kind, default }
}

const ANCHORS: &[&str] = &[
    "top",
    "top-left",
    "top-right",
    "bottom",
    "bottom-left",
    "bottom-right",
    "left",
    "right",
    "center",
];

/// The settings the page offers, and FSC's defaults for them, from
/// fsc-core's `config.rs`. Settings it leaves out (`claude.home`,
/// `setting_sources`, `extra_args`) are kept in the file as they are.
pub const FIELDS: &[Field] = &[
    f("claude", "model", "Model", "An alias (sonnet, opus, haiku) or a full model id", Kind::Text, "\"sonnet\""),
    f("claude", "bin", "Claude Code", "The claude program, by name or path", Kind::Text, "\"claude\""),
    f("claude", "session_idle_minutes", "New session after", "Minutes without an exchange; 0 never starts one", Kind::Int(0, 1440), "20"),
    f("claude", "memory", "Remember between sessions", "Keeps a short list of facts about you in memory.md", Kind::Bool, "true"),
    f("ui", "surface", "Shown as", "window: placed and decorated by Hyprland's rules. layer: pinned to an edge. headless: voice and tray only", Kind::Choice(&["window", "layer", "headless"]), "\"window\""),
    f("ui", "anchor", "Pinned to", "Layer only", Kind::Choice(ANCHORS), "\"top\""),
    f("ui", "margin", "Distance from top or bottom", "Layer only, in pixels", Kind::Int(0, 4000), "80"),
    f("ui", "margin_x", "Distance from the side", "Layer only, in pixels", Kind::Int(0, 4000), "24"),
    f("ui", "monitor", "Monitor", "Layer only: an output name such as DP-1; empty is the focused one", Kind::Text, "\"\""),
    f("ui", "opacity", "Opacity", "Layer only; a window takes Hyprland's opacity rule", Kind::Float(0.1, 1.0), "0.92"),
    f("ui", "gradient", "Fade", "Layer only: most see-through behind the visualizer, solid behind text", Kind::Bool, "false"),
    f("ui", "width", "Width", "In pixels", Kind::Int(200, 4000), "680"),
    f("ui", "max_height", "Conversation height", "Pixels before it scrolls", Kind::Int(80, 4000), "360"),
    f("ui", "font", "Font", "A font family", Kind::Text, "\"0xProto Nerd Font\""),
    f("ui", "accent", "Accent", "system follows the desktop, or a hex color such as #D71921", Kind::Text, "\"system\""),
    f("ui", "theme", "Tray icon light or dark", "auto follows the desktop", Kind::Choice(&["auto", "dark", "light"]), "\"auto\""),
    f("ui", "tray_icon", "Tray icon", "A red dot in its corner means it is recording", Kind::Choice(&["police-box", "fishbone"]), "\"police-box\""),
    f("ui", "visualiser", "Visualizer", "spectrum: frequency bars. loudness: level over time", Kind::Choice(&["spectrum", "loudness"]), "\"spectrum\""),
    f("ui", "bands", "Spectrum bars", "", Kind::Int(1, 64), "16"),
    f("ui", "block_percent", "Bar block size", "Each bar lights in blocks of this many percent", Kind::Int(1, 50), "10"),
    f("ui", "notify_when_hidden", "Notify when hidden", "A notification when a reply finishes while the popup is hidden", Kind::Bool, "true"),
    f("voice", "whisper_model", "Speech recognition model", "A whisper.cpp model; Download models fetches it", Kind::Text, "\"large-v3-turbo-q5_0\""),
    f("voice", "language", "Language", "en, another language code, or auto", Kind::Text, "\"en\""),
    f("voice", "gpu", "Recognize on the GPU", "whisper on Vulkan", Kind::Bool, "true"),
    f("voice", "piper_voice", "Voice", "Any voice from huggingface.co/rhasspy/piper-voices", Kind::Text, "\"en_GB-alan-medium\""),
    f("voice", "speech_rate", "Speaking speed", "Above 1 is faster", Kind::Float(0.5, 2.0), "1.0"),
    f("voice", "speak", "Speak replies", "voice: to spoken questions only. always. never", Kind::Choice(&["voice", "always", "never"]), "\"voice\""),
    f("voice", "input_device", "Microphone", "Part of its name, such as C920; empty is the system default", Kind::Text, "\"\""),
    f("voice", "end_silence_ms", "Pause that ends a sentence", "Milliseconds", Kind::Int(100, 5000), "800"),
    f("voice", "no_speech_timeout_ms", "Give up listening after", "Milliseconds with no speech", Kind::Int(1000, 60000), "6000"),
    f("voice", "max_utterance_secs", "Longest question", "Seconds", Kind::Int(1, 300), "30"),
    f("voice", "hold_mic_while_speaking", "Keep the mic open while speaking", "Needed for Bluetooth headsets", Kind::Bool, "true"),
    f("voice", "conversation", "Conversation mode", "After a spoken reply, listen again until the popup closes", Kind::Bool, "true"),
    f("voice", "hang_up_after_secs", "Hang up after", "Seconds of silence in a conversation; 0 waits for the popup to close", Kind::Int(0, 3600), "60"),
    f("voice", "wake_word", "Wake phrase", "Listen for it whenever the mic is free", Kind::Bool, "true"),
    f("voice", "wake_phrase", "The phrase", "", Kind::Text, "\"fishsticks and custard\""),
];

fn field(name: &str) -> Option<&'static Field> {
    FIELDS.iter().find(|f| format!("{}.{}", f.section, f.key) == name)
}

/// A field's default, as JSON.
pub fn default_of(section: &str, key: &str) -> Value {
    FIELDS
        .iter()
        .find(|f| f.section == section && f.key == key)
        .map(|f| toml_to_json(f.default))
        .unwrap_or(Value::Null)
}

fn toml_to_json(literal: &str) -> Value {
    let doc: toml::Value = toml::from_str(&format!("v = {literal}")).expect("a default parses");
    serde_json::to_value(&doc["v"]).unwrap_or(Value::Null)
}

/// The settings `text` sets, by `section.key`, for the fields the page
/// offers. A key the file does not set is absent.
pub fn config_values(text: &str) -> Result<Map<String, Value>> {
    let doc: toml::Table = toml::from_str(text)
        .map_err(|e| Error::other(format!("FSC's config.toml could not be read: {e}")))?;
    let mut out = Map::new();
    for f in FIELDS {
        if let Some(v) = doc.get(f.section).and_then(|t| t.get(f.key)) {
            out.insert(format!("{}.{}", f.section, f.key), serde_json::to_value(v)?);
        }
    }
    Ok(out)
}

/// The fields as the page shows them.
pub fn fields_view(values: &Map<String, Value>) -> Value {
    Value::Array(
        FIELDS
            .iter()
            .map(|f| {
                let name = format!("{}.{}", f.section, f.key);
                let (kind, extra) = match f.kind {
                    Kind::Bool => ("bool", json!({})),
                    Kind::Int(lo, hi) => ("int", json!({ "min": lo, "max": hi })),
                    Kind::Float(lo, hi) => ("float", json!({ "min": lo, "max": hi })),
                    Kind::Text => ("text", json!({})),
                    Kind::Choice(c) => ("choice", json!({ "choices": c })),
                };
                let mut v = json!({
                    "name": name,
                    "section": f.section,
                    "key": f.key,
                    "label": f.label,
                    "hint": f.hint,
                    "kind": kind,
                    "default": toml_to_json(f.default),
                    "value": values.get(&name).cloned().unwrap_or(Value::Null),
                });
                if let (Value::Object(o), Value::Object(e)) = (&mut v, extra) {
                    o.extend(e);
                }
                v
            })
            .collect(),
    )
}

/// Check a value against its field, as the TOML value to write.
fn checked(f: &Field, v: &Value) -> Result<toml_edit::Value> {
    let name = format!("{}.{}", f.section, f.key);
    let bad = || Error::other(format!("{} ({name}) cannot be {v}", f.label));
    Ok(match f.kind {
        Kind::Bool => toml_edit::Value::from(v.as_bool().ok_or_else(bad)?),
        Kind::Int(lo, hi) => {
            let n = v.as_i64().or_else(|| v.as_f64().map(|x| x.round() as i64)).ok_or_else(bad)?;
            if !(lo..=hi).contains(&n) {
                return Err(Error::other(format!("{} goes from {lo} to {hi}", f.label)));
            }
            toml_edit::Value::from(n)
        }
        Kind::Float(lo, hi) => {
            let n = v.as_f64().ok_or_else(bad)?;
            if !(lo..=hi).contains(&n) {
                return Err(Error::other(format!("{} goes from {lo} to {hi}", f.label)));
            }
            // Two decimals is what any of these is set to; more is a
            // slider's rounding error.
            toml_edit::Value::from((n * 100.0).round() / 100.0)
        }
        Kind::Text => {
            let s = v.as_str().ok_or_else(bad)?;
            if s.contains(['\n', '\r']) {
                return Err(bad());
            }
            toml_edit::Value::from(s.trim())
        }
        Kind::Choice(choices) => {
            let s = v.as_str().ok_or_else(bad)?;
            if !choices.contains(&s) {
                return Err(Error::other(format!(
                    "{} is one of {}, not {s}",
                    f.label,
                    choices.join(", ")
                )));
            }
            toml_edit::Value::from(s)
        }
    })
}

/// `text` with `changes` made: `section.key` to a value, or to null to take
/// the key out so FSC's default applies.
///
/// Comments, blank lines and keys the page does not know stay as they are.
/// A key the file does not set yet is only added when it differs from the
/// default; one it already sets keeps its place and its comment.
pub fn edit_config(text: &str, changes: &Map<String, Value>) -> Result<String> {
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .map_err(|e| Error::other(format!("FSC's config.toml could not be read: {e}")))?;
    for (name, v) in changes {
        let f = field(name).ok_or_else(|| Error::other(format!("no FSC setting `{name}`")))?;
        if v.is_null() {
            if let Some(t) = doc.get_mut(f.section).and_then(|t| t.as_table_like_mut()) {
                t.remove(f.key);
            }
            continue;
        }
        let new = checked(f, v)?;
        let present = doc.get(f.section).and_then(|t| t.get(f.key)).is_some();
        if !present && same(&new, &toml_to_json(f.default)) {
            continue;
        }
        if doc.get(f.section).is_none() {
            doc.insert(f.section, toml_edit::table());
        }
        let table = doc[f.section].as_table_like_mut().ok_or_else(|| {
            Error::other(format!("[{}] in FSC's config is not a table", f.section))
        })?;
        match table.get_mut(f.key).and_then(|i| i.as_value_mut()) {
            Some(old) => {
                let decor = old.decor().clone();
                *old = new;
                *old.decor_mut() = decor;
            }
            None => {
                table.insert(f.key, toml_edit::Item::Value(new));
            }
        }
    }
    Ok(doc.to_string())
}

fn same(v: &toml_edit::Value, default: &Value) -> bool {
    match v {
        toml_edit::Value::Boolean(b) => default.as_bool() == Some(*b.value()),
        toml_edit::Value::Integer(i) => default.as_i64() == Some(*i.value()),
        toml_edit::Value::Float(x) => {
            default.as_f64().is_some_and(|d| (d - x.value()).abs() < 1e-9)
        }
        toml_edit::Value::String(s) => default.as_str() == Some(s.value().as_str()),
        _ => false,
    }
}

/// Read FSC's config, which need not exist.
pub fn read_config(path: &Path) -> Result<String> {
    match std::fs::read_to_string(path) {
        Ok(t) => Ok(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(Error::io(path, e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The lines this was written for, as they are in a real config.
    const CONFIG: &str = "\
hl.on(\"hyprland.start\", function()
    hl.exec_cmd(\"waybar\")
    hl.exec_cmd(\"fscd\")
end)
hl.bind(\"SUPER + Q\", hl.dsp.exec_cmd(\"kitty\"))
hl.bind(\"SUPER + A\", hl.dsp.exec_cmd(\"fsc type\"))
hl.bind(\"SUPER + SHIFT + A\", hl.dsp.exec_cmd(\"fsc listen\"))
hl.bind(\"SUPER + SHIFT + A\", hl.dsp.exec_cmd(\"fsc release\"), { release = true })
-- hl.bind(\"SUPER + Z\", hl.dsp.exec_cmd(\"fsc new\"))
hl.bind(\"SUPER + X\", hl.dsp.exec_cmd(\"fsc ask hello\"))
";

    #[test]
    fn finds_the_lines_that_run_it() {
        let lines = user_lines(CONFIG);
        let numbers: Vec<usize> = lines.iter().map(|l| l.number).collect();
        assert_eq!(numbers, vec![3, 6, 7, 8, 10]);
        assert_eq!(lines[0].does, Does::Start);
        assert_eq!(
            lines[3].does,
            Does::Bind { action: Action::Talk, key: "SUPER + SHIFT + A".into(), release: true }
        );
        assert_eq!(lines[4].does, Does::Other, "a command with no key of its own is kept");
        assert!(lines.iter().all(|l| l.movable));
    }

    #[test]
    fn hand_over_and_give_back_round_trip() {
        let (taken, s) = hand_over(CONFIG, &FscSettings::default()).unwrap();
        assert!(managed(&taken));
        assert!(s.autostart && !s.window_rule);
        let on: Vec<(Action, &str)> =
            s.keys.iter().filter(|k| k.enabled).map(|k| (k.action, k.key.as_str())).collect();
        assert_eq!(on, vec![(Action::Type, "SUPER + A"), (Action::Talk, "SUPER + SHIFT + A")]);
        assert_eq!(moved_lines(&taken), vec![3, 6, 7, 8]);
        assert!(taken.contains(&format!("    {MOVED}hl.exec_cmd(\"fscd\")")));
        // Only the line with no action of its own is still live.
        let live: Vec<usize> = user_lines(&taken).iter().map(|l| l.number).collect();
        assert_eq!(live, vec![10]);
        let (_, inner) = block_text(&taken).unwrap();
        assert!(inner.contains("hl.exec_cmd(\"fscd\")"));
        assert!(inner.contains(
            "hl.bind(\"SUPER + SHIFT + A\", hl.dsp.exec_cmd(\"fsc release\"), { release = true })"
        ));
        assert_eq!(give_back(&taken).unwrap(), CONFIG);
    }

    #[test]
    fn a_line_shared_with_other_code_is_never_commented_out() {
        let text = "hl.on(\"hyprland.start\", function() hl.exec_cmd(\"fscd\") end)\n";
        let lines = user_lines(text);
        assert_eq!(lines.len(), 1);
        assert!(!lines[0].movable);
        let e = hand_over(text, &FscSettings::default()).unwrap_err().to_string();
        assert!(e.starts_with("Line 1 of hyprland.lua"), "{e}");
        assert!(!one_bind("hl.bind(\"A\", f) hl.bind(\"B\", g)"));
        assert!(one_bind("hl.bind(\"SUPER + A\", hl.dsp.exec_cmd(\"fsc type\")) -- mine"));
    }

    #[test]
    fn the_block_follows_the_settings_and_goes_when_empty() {
        let mut s = FscSettings::default();
        s.window_rule = true;
        let on = apply("x = 1\n", &s).unwrap();
        assert!(on.starts_with("x = 1\n") && managed(&on));
        assert!(on.contains("class = \"^(dev\\\\.fsc\\\\.FishsticksAndCustard)$\""));
        assert!(on.contains("hl.bind(\"SUPER + A\", hl.dsp.exec_cmd(\"fsc type\"))"));
        assert_eq!(apply(&on, &s).unwrap(), on, "the same settings twice are one block");
        let off = FscSettings {
            autostart: false,
            window_rule: false,
            keys: FscSettings::default()
                .keys
                .into_iter()
                .map(|k| Key { enabled: false, ..k })
                .collect(),
        };
        assert_eq!(apply(&on, &off).unwrap(), "x = 1\n");
    }

    #[test]
    fn settings_always_hold_every_action_once() {
        let s: FscSettings = toml::from_str(
            "autostart = false\n[[keys]]\naction = \"mute\"\nkey = \"SUPER + M\"\nenabled = true\n\
             [[keys]]\naction = \"new\"\nkey = \"  \"\nenabled = true\n",
        )
        .unwrap();
        let s = s.sanitized();
        let actions: Vec<Action> = s.keys.iter().map(|k| k.action).collect();
        assert_eq!(actions, Action::ALL.to_vec());
        assert_eq!(s.keys[5].key, "SUPER + M");
        assert!(!s.keys[3].enabled, "a key with no combination cannot be on");
        assert_eq!(s.keys[0].key, "SUPER + A", "a missing action gets its default");
    }

    #[test]
    fn clashes_name_the_other_lines_on_a_key() {
        let config = format!(
            "{CONFIG}hl.bind(\"super + a\", hl.dsp.exec_cmd(\"rofi -show drun\"))\n\
             -- hl.bind(\"SUPER + A\", hl.dsp.exec_cmd(\"old\"))\n"
        );
        let c = clashes(&FscSettings::default(), &config);
        assert_eq!(c.len(), 1, "{c:?}");
        assert_eq!(c[0]["key"], "SUPER + A");
        assert_eq!(
            c[0]["taken"][0],
            "line 11: hl.bind(\"super + a\", hl.dsp.exec_cmd(\"rofi -show drun\"))"
        );
        // The block's own binds are not clashes either.
        let (taken, s) = hand_over(CONFIG, &FscSettings::default()).unwrap();
        assert!(clashes(&s, &taken).is_empty());
        assert_eq!(modmask("CTRL + ALT + Delete"), Some((12, "Delete".into())));
    }

    /// The user's real file, comments and all.
    const FSC_CONFIG: &str = "\
# Fishsticks & Custard. All keys and defaults: packaging/config.example.toml in the repo.

[voice]
# Listen on the webcam mic; replies play on the default output (the Nothing Ear
# headphones), which can then stay in high-quality A2DP mode.
input_device = \"C920\"
";

    #[test]
    fn editing_keeps_the_comments_and_writes_only_changes() {
        let mut changes = Map::new();
        changes.insert("voice.input_device".into(), json!("Yeti"));
        changes.insert("voice.gpu".into(), json!(true)); // the default: not added
        changes.insert("ui.width".into(), json!(720));
        let out = edit_config(FSC_CONFIG, &changes).unwrap();
        assert!(out.starts_with(&FSC_CONFIG[..FSC_CONFIG.find("input_device").unwrap()]));
        assert!(out.contains("input_device = \"Yeti\"\n"));
        assert!(!out.contains("gpu"));
        assert!(out.contains("[ui]\nwidth = 720\n"));

        let values = config_values(&out).unwrap();
        assert_eq!(values["voice.input_device"], "Yeti");
        assert_eq!(values["ui.width"], 720);
        assert!(!values.contains_key("voice.gpu"));

        // Null takes a key out, so FSC's default applies again.
        let mut back = Map::new();
        back.insert("ui.width".into(), Value::Null);
        let out = edit_config(&out, &back).unwrap();
        assert!(!config_values(&out).unwrap().contains_key("ui.width"));
        assert!(out.contains("# Listen on the webcam mic"));
    }

    #[test]
    fn values_are_checked_against_their_field() {
        let one = |k: &str, v: Value| {
            let mut m = Map::new();
            m.insert(k.into(), v);
            edit_config("", &m)
        };
        assert!(one("ui.surface", json!("floating")).is_err());
        assert!(one("ui.width", json!(50)).is_err());
        assert!(one("voice.gpu", json!("yes")).is_err());
        assert!(one("voice.wake_phrase", json!("a\nb")).is_err());
        assert!(one("ui.nope", json!(1)).is_err());
        assert_eq!(one("ui.opacity", json!(0.8549)).unwrap(), "[ui]\nopacity = 0.85\n");
        assert_eq!(
            one("ui.anchor", json!("bottom-right")).unwrap(),
            "[ui]\nanchor = \"bottom-right\"\n"
        );
    }

    #[test]
    fn every_default_parses_and_fits_its_field() {
        for f in FIELDS {
            let v = toml_to_json(f.default);
            assert!(checked(f, &v).is_ok(), "{}.{} = {}", f.section, f.key, f.default);
        }
        assert_eq!(
            needed_models(&Map::new()),
            vec![
                "ggml-large-v3-turbo-q5_0.bin",
                "silero_vad.onnx",
                "en_GB-alan-medium.onnx",
                "en_GB-alan-medium.onnx.json"
            ]
        );
    }

    #[test]
    fn a_commented_out_repository_does_not_count() {
        assert!(has_repo("[core]\n\n[fishsticks-and-custard]\nServer = x\n", PACKAGE));
        assert!(!has_repo("#[fishsticks-and-custard]\n", PACKAGE));
    }

    /// The install script, run with stand-ins for curl, gpg, pacman-key and
    /// sudo: a key with the wrong fingerprint stops it before sudo is
    /// asked for anything.
    #[test]
    fn the_install_script_refuses_a_key_it_does_not_expect() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let log = dir.path().join("log");
        let conf = dir.path().join("pacman.conf");
        std::fs::write(&conf, "[core]\nInclude = x\n").unwrap();
        let stub = |name: &str, body: &str| {
            use std::os::unix::fs::PermissionsExt;
            let p = bin.join(name);
            std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        let log_s = log.display().to_string();
        stub("pacman-key", "exit 1");
        stub("curl", "while [ $# -gt 1 ]; do shift; done; echo key > \"$1\"");
        stub("sudo", &format!("echo \"sudo $*\" >> {log_s}; [ \"$1\" = -A ] && shift; case \"$1\" in tee) cat >> \"$3\";; esac"));

        let run = |fpr_seen: &str| {
            stub("gpg", &format!("echo 'fpr:::::::::{fpr_seen}:'"));
            let _ = std::fs::remove_file(&log);
            let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
            std::process::Command::new("/bin/sh")
                .args([
                    "-c",
                    INSTALL,
                    "fsc-job",
                    KEY_FINGERPRINT,
                    "https://k",
                    PACKAGE,
                    "https://r",
                ])
                .arg(&conf)
                .env("PATH", path)
                .output()
                .unwrap()
        };

        let wrong = run("0000000000000000000000000000000000000000");
        assert!(!wrong.status.success());
        assert!(String::from_utf8_lossy(&wrong.stderr).contains("not the expected"));
        assert!(!log.exists(), "sudo was never asked");

        let right = run(KEY_FINGERPRINT);
        assert!(right.status.success(), "{}", String::from_utf8_lossy(&right.stderr));
        let calls = std::fs::read_to_string(&log).unwrap();
        assert!(calls.contains(&format!("sudo -A pacman-key --lsign-key {KEY_FINGERPRINT}")));
        assert!(calls.contains(&format!("sudo -A pacman -Syu --needed --noconfirm {PACKAGE}")));
        assert!(has_repo(&std::fs::read_to_string(&conf).unwrap(), PACKAGE));
    }
}
