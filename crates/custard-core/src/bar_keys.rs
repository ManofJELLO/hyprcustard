//! The keys that open the bar's launcher and power menu.
//!
//! The launcher and the power menu live in the bar's Quickshell, and open
//! with `qs ipc -p <bar folder> call launcher toggle` (or `power`). A user's
//! own key binding for a launcher (rofi, wofi, fuzzel...) or a power menu
//! can be handed over: its line is commented out with a marker, and a
//! managed block binds the same key, with the same options, to the bar's
//! menu. The command it replaced stays in the new binding as a fallback,
//! `qs ipc ... || <their command>`, which runs while the bar is not up
//! (`qs ipc` exits non-zero when no instance is running). Giving it back
//! takes the marker off and the binding out of the block, exactly.
//!
//! The block is rebuilt from the marked lines each time, so it always holds
//! one binding per handed-over line, in the file's order.

use std::path::Path;

use serde::Serialize;

use crate::bar;
use crate::block::{self, Markers};
use crate::error::{Error, Result};
use crate::model::lua_quote;
use crate::walls::startup::{has_word, lines_outside_blocks, lua_string, quote, split_keep};

/// The block's markers.
pub const MARKERS: Markers = Markers {
    begin: "-- >>> HyprCustard keys (managed by hyprcustard) >>>",
    end: "-- <<< HyprCustard keys (managed by hyprcustard) <<<",
    name: "HyprCustard keys",
};

/// What a handed-over line starts with, after its indentation.
pub const MOVED: &str = "-- [moved to the HyprCustard keys block] ";

/// Programs that are app launchers. rofi and wofi count only when they
/// show apps (`drun`, `run` or `combi`), not their window or other modes.
const LAUNCHERS: &[&str] = &[
    "fuzzel",
    "tofi-drun",
    "tofi-run",
    "walker",
    "anyrun",
    "bemenu-run",
    "j4-dmenu-desktop",
    "dmenu_run",
    "nwg-drawer",
    "ulauncher",
    "sherlock",
    "vicinae",
    "kickoff",
    "onagre",
    "albert",
];

/// Menus that need a mode named before they are an app launcher.
const MODAL_LAUNCHERS: &[&str] = &["rofi", "wofi"];

/// Power menus by name; anything with "power" in its command counts too.
const POWER_MENUS: &[&str] = &["wlogout", "wleave", "nwg-bar"];

/// Which of the bar's menus a key opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// The app launcher.
    Launcher,
    /// The power menu.
    Power,
}

impl Kind {
    /// The IPC target, which is also the id the page uses.
    pub fn target(self) -> &'static str {
        match self {
            Kind::Launcher => "launcher",
            Kind::Power => "power",
        }
    }

    /// The kind a page id names.
    pub fn from_id(id: &str) -> Option<Kind> {
        match id {
            "launcher" => Some(Kind::Launcher),
            "power" => Some(Kind::Power),
            _ => None,
        }
    }
}

/// A key binding in the user's config that opens a launcher or power menu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Bind {
    /// 1-based.
    pub number: usize,
    /// The line, trimmed, without the marker when it was handed over.
    pub text: String,
    /// What it opens.
    pub kind: Kind,
    /// Whether it can be handed over: one `hl.bind(...)` on a line of its
    /// own, at the top level of the file (so the names it uses, such as
    /// `mainMod`, are still in scope at the end of the file).
    pub movable: bool,
}

/// The command that opens a menu in a running bar.
pub fn ipc_command(program: &Path, config_home: &Path, kind: Kind) -> String {
    let name = program.file_name().map(|n| n.to_string_lossy().into_owned());
    format!(
        "{} ipc -p {} call {} toggle",
        name.as_deref().unwrap_or("qs"),
        quote(&bar::dir(config_home).to_string_lossy()),
        kind.target()
    )
}

/// Key bindings outside every managed block that open a launcher or a
/// power menu, not commented out.
pub fn user_binds(text: &str) -> Vec<Bind> {
    lines_outside_blocks(text)
        .filter_map(|(number, line)| {
            let t = line.trim();
            if t.starts_with("--") {
                return None;
            }
            let kind = classify(text, number, t)?;
            Some(Bind { number, text: t.to_string(), kind, movable: movable(line) })
        })
        .collect()
}

/// Bindings the app commented out when it took them over.
pub fn moved_binds(text: &str) -> Vec<Bind> {
    text.lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let rest = l.trim_start().strip_prefix(MOVED)?.trim();
            // One that no longer reads as a menu still counts as a launcher,
            // so it is never stranded behind the marker.
            let kind = classify(text, i + 1, rest).unwrap_or(Kind::Launcher);
            Some(Bind { number: i + 1, text: rest.to_string(), kind, movable: true })
        })
        .collect()
}

/// The block as it is in the config: the line its opening marker is on and
/// what is between the markers.
pub fn block_text(text: &str) -> Option<(usize, String)> {
    let split = block::split_with(text, &MARKERS).ok()??;
    let line = split.before.lines().count() + 1;
    Some((line, split.inner.trim_end().to_string()))
}

/// Comment out the user's bindings of `kind` and bind the same keys to the
/// bar's menu. Refused, with nothing changed, when one cannot be moved on
/// its own, or when there is none.
pub fn hand_over(text: &str, kind: Kind, program: &Path, config_home: &Path) -> Result<String> {
    let binds: Vec<Bind> = user_binds(text).into_iter().filter(|b| b.kind == kind).collect();
    if binds.is_empty() {
        return Err(Error::other(format!(
            "No key in your config opens a {} to hand over",
            match kind {
                Kind::Launcher => "launcher",
                Kind::Power => "power menu",
            }
        )));
    }
    let stuck: Vec<String> =
        binds.iter().filter(|b| !b.movable).map(|b| b.number.to_string()).collect();
    if !stuck.is_empty() {
        return Err(Error::other(format!(
            "{} {} is not a single hl.bind(...) on its own line at the top of the file, \
             so it cannot be commented out on its own. Bind the key to the command shown \
             on this page yourself instead",
            if stuck.len() == 1 { "Line" } else { "Lines" },
            stuck.join(", ")
        )));
    }
    let numbers: Vec<usize> = binds.iter().map(|b| b.number).collect();
    let mut out = String::new();
    for (i, l) in split_keep(text).into_iter().enumerate() {
        if numbers.contains(&(i + 1)) {
            out.push_str(MOVED);
        }
        out.push_str(l);
    }
    rebuild(&out, program, config_home)
}

/// Put the user's bindings of `kind` back as they were, and take theirs out
/// of the block (the block itself when it is left empty).
pub fn give_back(text: &str, kind: Kind, program: &Path, config_home: &Path) -> Result<String> {
    let numbers: Vec<usize> =
        moved_binds(text).into_iter().filter(|b| b.kind == kind).map(|b| b.number).collect();
    let out: String = split_keep(text)
        .into_iter()
        .enumerate()
        .map(|(i, l)| {
            if numbers.contains(&(i + 1)) {
                let indent = &l[..l.len() - l.trim_start().len()];
                if let Some(rest) = l[indent.len()..].strip_prefix(MOVED) {
                    return format!("{indent}{rest}");
                }
            }
            l.to_string()
        })
        .collect();
    rebuild(&out, program, config_home)
}

/// Every binding back, and the block gone: for when the bar is removed.
pub fn give_back_all(text: &str) -> Result<String> {
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

/// Write the block from the marked lines, or take it out when there are none.
fn rebuild(text: &str, program: &Path, config_home: &Path) -> Result<String> {
    let moved = moved_binds(text);
    if moved.is_empty() {
        return block::remove_with(text, &MARKERS);
    }
    let mut body = String::from(
        "-- hyprcustard's launcher and power menu, on the keys these replaced. Each\n\
         -- runs the command it replaced while the bar is not running. \"Give it back\"\n\
         -- on hyprcustard's Bar page puts the original line back.\n",
    );
    for b in &moved {
        let line =
            rewrite(&b.text, &ipc_command(program, config_home, b.kind)).ok_or_else(|| {
                Error::other(format!("line {} could not be read as a key binding", b.number))
            })?;
        body.push_str(&line);
        body.push('\n');
    }
    block::splice_with(text, &body, &MARKERS)
}

/// The binding with its command replaced by `ipc`, falling back to what it
/// ran before: `exec_cmd("<ipc> || " .. (<their argument>))`.
fn rewrite(line: &str, ipc: &str) -> Option<String> {
    let (start, end) = exec_arg(line)?;
    let arg = line[start..end].trim();
    Some(format!(
        "{}{} .. ({arg}){}",
        &line[..start],
        lua_quote(&format!("{ipc} || ")),
        &line[end..]
    ))
}

/// What kind of menu a binding opens, if either.
fn classify(text: &str, number: usize, line: &str) -> Option<Kind> {
    if !line.starts_with("hl.bind(") {
        return None;
    }
    let (start, end) = exec_arg(line)?;
    let arg = line[start..end].trim();
    let command = resolve(text, number, arg).unwrap_or_else(|| arg.to_string());
    let lower = command.to_lowercase();
    if lower.contains("power") || POWER_MENUS.iter().any(|p| has_word(&lower, p)) {
        return Some(Kind::Power);
    }
    let modal = MODAL_LAUNCHERS.iter().any(|p| has_word(&lower, p))
        && ["drun", "run", "combi"].iter().any(|m| has_word(&lower, m));
    if modal || LAUNCHERS.iter().any(|p| has_word(&lower, p)) {
        return Some(Kind::Launcher);
    }
    None
}

/// The byte range of `hl.dsp.exec_cmd(...)`'s argument in a line.
fn exec_arg(line: &str) -> Option<(usize, usize)> {
    const CALL: &str = "exec_cmd(";
    if line.matches(CALL).count() != 1 {
        return None;
    }
    let start = line.find(CALL)? + CALL.len();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (i, c) in line[start..].char_indices() {
        if let Some(q) = quote {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                c if c == q => quote = None,
                _ => {}
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '(' => depth += 1,
            ')' if depth == 0 => return Some((start, start + i)),
            ')' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// The command a binding's argument stands for: a string, or a name given
/// a string by `local name = "..."` on an earlier line.
fn resolve(text: &str, number: usize, arg: &str) -> Option<String> {
    if let Some(s) = lua_string(arg) {
        return Some(s);
    }
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    if arg.is_empty() || !arg.chars().all(ident) {
        return None;
    }
    let before: Vec<&str> = text.lines().take(number.saturating_sub(1)).collect();
    before.into_iter().rev().find_map(|l| {
        let t = l.trim();
        let t = t.strip_prefix("local ").unwrap_or(t).trim_start();
        let rest = t.strip_prefix(arg)?.trim_start().strip_prefix('=')?;
        lua_string(rest.trim())
    })
}

/// One `hl.bind(...)` on a line of its own, not indented, nothing after it
/// but a comment.
fn movable(line: &str) -> bool {
    if line.starts_with(char::is_whitespace) {
        return false;
    }
    let t = line.trim_end();
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

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = "local mainMod = \"SUPER\"\n\
        local menu        = \"rofi -show drun\"\n\
        local powermenu   = \"fish -c '~/.config/rofi/powermenu/powermenu.sh'\"\n\
        hl.bind(mainMod .. \" + Q\", hl.dsp.exec_cmd(\"kitty\"))\n\
        hl.bind(mainMod .. \" + space\",      hl.dsp.exec_cmd(menu))\n\
        hl.bind(mainMod .. \" + TAB\", hl.dsp.exec_cmd(\"rofi -show window\"))\n\
        hl.bind(mainMod .. \" + BACKSPACE\",  hl.dsp.exec_cmd(powermenu)) -- power\n\
        -- hl.bind(mainMod .. \" + D\", hl.dsp.exec_cmd(\"wofi --show drun\"))\n";

    fn home() -> &'static Path {
        Path::new("/home/u/.config")
    }

    fn qs() -> &'static Path {
        Path::new("/usr/bin/qs")
    }

    #[test]
    fn launchers_and_power_menus_are_found_through_their_variables() {
        let binds = user_binds(CONFIG);
        let found: Vec<(usize, Kind)> = binds.iter().map(|b| (b.number, b.kind)).collect();
        assert_eq!(found, [(5, Kind::Launcher), (7, Kind::Power)], "{binds:#?}");
        assert!(binds.iter().all(|b| b.movable));
    }

    #[test]
    fn handing_over_keeps_the_key_and_falls_back_to_the_old_command() {
        let out = hand_over(CONFIG, Kind::Launcher, qs(), home()).unwrap();
        assert!(out.contains(&format!(
            "{MOVED}hl.bind(mainMod .. \" + space\",      hl.dsp.exec_cmd(menu))"
        )));
        let (_, inner) = block_text(&out).unwrap();
        assert!(
            inner.contains(
                "hl.bind(mainMod .. \" + space\",      hl.dsp.exec_cmd(\"qs ipc -p \
                 /home/u/.config/quickshell/hyprcustard call launcher toggle || \" .. (menu)))"
            ),
            "{inner}"
        );
        assert!(user_binds(&out).iter().all(|b| b.kind == Kind::Power), "the launcher is moved");

        // Then the power menu too: both in the block, in the file's order,
        // the trailing comment kept.
        let both = hand_over(&out, Kind::Power, qs(), home()).unwrap();
        let (_, inner) = block_text(&both).unwrap();
        let launcher = inner.find("call launcher").unwrap();
        let power = inner.find("call power toggle || \" .. (powermenu))) -- power").unwrap();
        assert!(launcher < power, "{inner}");
        assert!(user_binds(&both).is_empty());

        // Giving one back leaves the other; giving both back is the original.
        let one = give_back(&both, Kind::Launcher, qs(), home()).unwrap();
        assert_eq!(moved_binds(&one).len(), 1);
        assert!(!block_text(&one).unwrap().1.contains("call launcher"));
        assert_eq!(give_back(&one, Kind::Power, qs(), home()).unwrap(), CONFIG);
        assert_eq!(give_back_all(&both).unwrap(), CONFIG);
    }

    #[test]
    fn a_binding_that_cannot_move_alone_is_refused_untouched() {
        let text = "do\n  hl.bind(\"SUPER + R\", hl.dsp.exec_cmd(\"fuzzel\"))\nend\n";
        let binds = user_binds(text);
        assert_eq!(binds.len(), 1);
        assert!(!binds[0].movable, "indented: its names may not be in scope later");
        assert!(hand_over(text, Kind::Launcher, qs(), home()).is_err());
        assert!(hand_over(CONFIG, Kind::Power, qs(), home()).is_ok());
        assert!(hand_over("", Kind::Power, qs(), home()).is_err(), "nothing to hand over");
    }

    #[test]
    fn the_ipc_command_quotes_the_folder() {
        assert_eq!(
            ipc_command(
                Path::new("/usr/bin/quickshell"),
                Path::new("/home/a b/.config"),
                Kind::Power
            ),
            "quickshell ipc -p '/home/a b/.config/quickshell/hyprcustard' call power toggle"
        );
    }

    #[test]
    fn strings_in_the_argument_do_not_end_it() {
        let line = "hl.bind(\"SUPER + R\", hl.dsp.exec_cmd(\"fuzzel --prompt ')'\"), { repeating = true })";
        let (s, e) = exec_arg(line).unwrap();
        assert_eq!(&line[s..e], "\"fuzzel --prompt ')'\"");
        assert_eq!(classify("", 1, line), Some(Kind::Launcher));
    }
}
