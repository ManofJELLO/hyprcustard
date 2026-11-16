//! Lines in the Hyprland config that set GTK's desktop settings with
//! `gsettings`, and taking them out.
//!
//! Setups often run `gsettings set org.gnome.desktop.interface gtk-theme …`
//! from the Hyprland config, with `exec` (each time Hyprland starts or
//! reloads) or `exec-once` (each time it starts). gsettings keeps a value
//! once it is set, so such a line only needs to run once; left in, it puts
//! its value back over what hyprcustard sets — the GTK theme, the light or
//! dark preference — the next time it runs. This finds those lines and
//! comments them out with a marker, so each can be put back exactly.
//!
//! Only lines that run that one command and nothing else are read.

use std::path::PathBuf;

use super::qt_style::{ends, indent, map_lines, quoted, word, Kind, Source};

/// The gsettings schema GTK's settings are in.
const SCHEMA: &str = "org.gnome.desktop.interface";

/// Put after a line's indent, before its text, by [`take_out`].
const MARK: &str = "[hyprcustard took this out: it reset what hyprcustard sets] ";

/// A line that sets a key, or did until it was taken out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    /// The file it is in.
    pub path: PathBuf,
    /// 1-based.
    pub line: usize,
    /// The gsettings key, `gtk-theme` or `color-scheme`, say.
    pub key: String,
    /// What it sets the key to.
    pub value: String,
    /// Commented out by [`take_out`].
    pub taken: bool,
}

/// The key and value a line sets, when running `gsettings set` on GTK's
/// schema is all it does.
pub fn pinned(kind: Kind, line: &str) -> Option<(String, String)> {
    let line = line.trim();
    let cmd = match kind {
        Kind::Lua => {
            let rest = line.strip_prefix("hl.exec_cmd")?.trim_start().strip_prefix('(')?;
            let (cmd, rest) = quoted(rest.trim_start())?;
            let rest = rest.trim_start().strip_prefix(')')?;
            ends(rest, kind).then_some(cmd)?
        }
        Kind::Hyprlang => {
            let (k, v) = line.split_once('=')?;
            if !matches!(k.trim(), "exec" | "exec-once") {
                return None;
            }
            v.split(" #").next()?.trim().to_string()
        }
        Kind::Shell | Kind::Fish => return None,
    };
    let mut words = Vec::new();
    let mut rest = cmd.as_str();
    while let Some((w, after)) = word(rest) {
        words.push(w);
        rest = after;
    }
    match words.as_slice() {
        [bin, set, schema, key, value]
            if (bin == "gsettings" || bin.ends_with("/gsettings"))
                && set == "set"
                && schema == SCHEMA
                && !value.contains([';', '&', '|']) =>
        {
            Some((key.clone(), value.clone()))
        }
        _ => None,
    }
}

/// The text a taken-out line had, when `line` is one.
fn taken_text(kind: Kind, line: &str) -> Option<String> {
    let (lead, rest) = indent(line);
    let rest = rest.strip_prefix(kind.comment())?.strip_prefix(MARK)?;
    Some(format!("{lead}{rest}"))
}

/// Every line in a file that sets a key, or did until taken out.
pub fn pins(source: &Source, text: &str) -> Vec<Pin> {
    text.lines()
        .enumerate()
        .filter_map(|(i, line)| {
            let (text, taken) = match taken_text(source.kind, line) {
                Some(t) => (t, true),
                None => (line.to_string(), false),
            };
            let (key, value) = pinned(source.kind, &text)?;
            Some(Pin { path: source.path.clone(), line: i + 1, key, value, taken })
        })
        .collect()
}

/// Every pin in every source.
pub fn find(sources: &[Source]) -> Vec<Pin> {
    sources
        .iter()
        .flat_map(|s| pins(s, &std::fs::read_to_string(&s.path).unwrap_or_default()))
        .collect()
}

/// The pins of `key` still in force.
pub fn live<'a>(all: &'a [Pin], key: &str) -> Vec<&'a Pin> {
    all.iter().filter(|p| !p.taken && p.key == key).collect()
}

/// Comment out each line that sets one of `keys`.
pub fn take_out(kind: Kind, text: &str, keys: &[&str]) -> String {
    map_lines(text, |line| {
        let (key, _) = pinned(kind, line)?;
        if !keys.contains(&key.as_str()) {
            return None;
        }
        let (lead, rest) = indent(line);
        Some(format!("{lead}{}{MARK}{rest}", kind.comment()))
    })
}

/// Where the lines are, as `file:line` with `~` for home.
pub fn places(pins: &[&Pin]) -> String {
    let all: Vec<String> =
        pins.iter().map(|p| format!("{}:{}", crate::paths::contract(&p.path), p.line)).collect();
    all.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_that_only_runs_gsettings_set_is_read_in_each_kind() {
        let some = |k: &str, v: &str| Some((k.to_string(), v.to_string()));
        let cases = [
            (
                Kind::Lua,
                r#"hl.exec_cmd("gsettings set org.gnome.desktop.interface gtk-theme 'Gruvbox-Dark'")"#,
                some("gtk-theme", "Gruvbox-Dark"),
            ),
            (
                Kind::Lua,
                r#"  hl.exec_cmd('gsettings set org.gnome.desktop.interface color-scheme "prefer-dark"') -- dark"#,
                some("color-scheme", "prefer-dark"),
            ),
            (
                Kind::Lua,
                r#"-- hl.exec_cmd("gsettings set org.gnome.desktop.interface gtk-theme 'X'")"#,
                None,
            ),
            (
                Kind::Lua,
                r#"hl.exec_cmd("gsettings set org.gnome.desktop.interface gtk-theme X && waybar")"#,
                None,
            ),
            (
                Kind::Lua,
                r#"hl.exec_cmd("gsettings set org.gnome.desktop.wm.preferences button-layout ''")"#,
                None,
            ),
            (
                Kind::Hyprlang,
                "exec = gsettings set org.gnome.desktop.interface gtk-theme \"Gruvbox-Dark\"",
                some("gtk-theme", "Gruvbox-Dark"),
            ),
            (
                Kind::Hyprlang,
                "exec-once=/usr/bin/gsettings set org.gnome.desktop.interface color-scheme prefer-dark # why",
                some("color-scheme", "prefer-dark"),
            ),
            (Kind::Hyprlang, "exec = gsettings set org.gnome.desktop.interface gtk-theme X; y", None),
            (Kind::Hyprlang, "bind = SUPER, T, exec, gsettings set org.gnome.desktop.interface gtk-theme X", None),
            (Kind::Shell, "gsettings set org.gnome.desktop.interface gtk-theme X", None),
        ];
        for (kind, line, want) in cases {
            assert_eq!(pinned(kind, line), want, "{kind:?}: {line}");
        }
    }

    #[test]
    fn taking_out_one_key_leaves_the_others_and_is_read_back() {
        let text = "hl.exec_cmd(\"gsettings set org.gnome.desktop.interface icon-theme 'Icons'\")\n\
                    hl.exec_cmd(\"gsettings set org.gnome.desktop.interface gtk-theme 'Gruvbox-Dark'\")\n  \
                    hl.exec_cmd(\"gsettings set org.gnome.desktop.interface color-scheme 'prefer-dark'\")\n";
        let source = Source { path: "f".into(), kind: Kind::Lua };
        let out = take_out(Kind::Lua, text, &["gtk-theme", "color-scheme"]);
        let p = pins(&source, &out);
        let read: Vec<(&str, &str, bool)> =
            p.iter().map(|p| (p.key.as_str(), p.value.as_str(), p.taken)).collect();
        assert_eq!(
            read,
            [
                ("icon-theme", "Icons", false),
                ("gtk-theme", "Gruvbox-Dark", true),
                ("color-scheme", "prefer-dark", true),
            ]
        );
        // The indent stays where it was.
        assert!(out.contains(&format!("\n  -- {MARK}hl.exec_cmd")), "{out}");
        assert_eq!(live(&p, "gtk-theme").len(), 0);
        assert_eq!(live(&p, "icon-theme").len(), 1);
        // Twice is the same as once.
        assert_eq!(take_out(Kind::Lua, &out, &["gtk-theme", "color-scheme"]), out);
    }
}
