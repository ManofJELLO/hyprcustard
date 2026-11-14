//! `QT_STYLE_OVERRIDE`, and taking it out for qt5ct/qt6ct.
//!
//! With `QT_QPA_PLATFORMTHEME=qt6ct`, the style is qt6ct's to choose: while
//! `QT_STYLE_OVERRIDE` is set, Qt takes the style from it instead, and qt6ct
//! itself warns that the setup is wrong. Users often set both — guides for
//! Kvantum say to — so this finds where the variable is set, in the places
//! a Hyprland session takes its environment from, and comments those lines
//! out with a marker that puts each back exactly.
//!
//! Only lines that set the variable and nothing else are touched; anything
//! else is reported where it is, for the user to change themselves.

use std::path::{Path, PathBuf};

/// The variable.
pub const VAR: &str = "QT_STYLE_OVERRIDE";

/// Put after a line's indent, before its text, by [`take_out`].
const MARK: &str = "[hyprcustard took this out: qt6ct chooses the style] ";

/// How a file is written, which decides how a line sets the variable and how
/// it is commented out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Hyprland's Lua config: `hl.env("VAR", "value")`.
    Lua,
    /// Hyprland's hyprlang config: `env = VAR,value`.
    Hyprlang,
    /// environment.d, uwsm's env files, shell profiles:
    /// `VAR=value` or `export VAR=value`.
    Shell,
    /// fish: `set -gx VAR value`.
    Fish,
}

impl Kind {
    fn comment(self) -> &'static str {
        match self {
            Kind::Lua => "-- ",
            _ => "# ",
        }
    }
}

/// A file that may set the variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// The file.
    pub path: PathBuf,
    /// How it is written.
    pub kind: Kind,
}

/// A line that sets the variable, or did until it was taken out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setting {
    /// The file it is in.
    pub path: PathBuf,
    /// 1-based.
    pub line: usize,
    /// What it sets the variable to.
    pub value: String,
    /// Commented out by [`take_out`].
    pub taken: bool,
}

/// The files a Hyprland session takes its environment from that exist: the
/// Hyprland config and the files it sources, environment.d, uwsm's env
/// files, and the login shells' profiles. `home` is `None` to leave the
/// profiles out.
pub fn sources(config_home: &Path, home: Option<&Path>, hypr_config: &Path) -> Vec<Source> {
    let mut out = Vec::new();
    hypr_files(hypr_config, &mut out, 0);
    if let Ok(dir) = std::fs::read_dir(config_home.join("environment.d")) {
        let mut confs: Vec<PathBuf> = dir
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "conf"))
            .collect();
        confs.sort();
        out.extend(confs.into_iter().map(|path| Source { path, kind: Kind::Shell }));
    }
    for f in ["uwsm/env", "uwsm/env-hyprland"] {
        out.push(Source { path: config_home.join(f), kind: Kind::Shell });
    }
    out.push(Source { path: config_home.join("fish/config.fish"), kind: Kind::Fish });
    if let Some(home) = home {
        for f in [".profile", ".bash_profile", ".zprofile", ".zshenv"] {
            out.push(Source { path: home.join(f), kind: Kind::Shell });
        }
    }
    out.retain(|s| s.path.is_file());
    out
}

/// The Hyprland config, then each file it sources, depth first.
fn hypr_files(path: &Path, out: &mut Vec<Source>, depth: usize) {
    if depth > 4 || out.iter().any(|s| s.path == path) || !path.is_file() {
        return;
    }
    let lua = path.extension().is_some_and(|e| e == "lua");
    let kind = if lua { Kind::Lua } else { Kind::Hyprlang };
    out.push(Source { path: path.to_path_buf(), kind });
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let dir = path.parent().unwrap_or(Path::new("."));
    for line in text.lines() {
        let line = line.trim();
        let next = if lua { lua_include(line, dir) } else { hyprlang_source(line, dir) };
        if let Some(next) = next {
            hypr_files(&next, out, depth + 1);
        }
    }
}

/// `require("a.b")` → `<dir>/a/b.lua`; `dofile("path")`, also as
/// `pcall(dofile, "path")`.
fn lua_include(line: &str, dir: &Path) -> Option<PathBuf> {
    if line.starts_with("--") {
        return None;
    }
    for (call, module) in [("require", true), ("dofile", false)] {
        let Some(at) = line.find(call) else { continue };
        let rest = line[at + call.len()..].trim_start();
        let rest = rest.strip_prefix(['(', ',']).unwrap_or(rest).trim_start();
        let Some((arg, _)) = quoted(rest) else { continue };
        return Some(if module {
            dir.join(format!("{}.lua", arg.replace('.', "/")))
        } else {
            resolve(&arg, dir)
        });
    }
    None
}

/// `source = path`, without globs.
fn hyprlang_source(line: &str, dir: &Path) -> Option<PathBuf> {
    let rest = line.strip_prefix("source")?.trim_start().strip_prefix('=')?;
    let path = rest.split(" #").next()?.trim();
    (!path.is_empty() && !path.contains(['*', '?', '['])).then(|| resolve(path, dir))
}

fn resolve(path: &str, dir: &Path) -> PathBuf {
    let p = crate::paths::expand(path);
    if p.is_absolute() {
        p
    } else {
        dir.join(p)
    }
}

/// A quoted string at the start of `s`, and what follows it.
fn quoted(s: &str) -> Option<(String, &str)> {
    let q = s.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let end = s[1..].find(q)? + 1;
    Some((s[1..end].to_string(), &s[end + 1..]))
}

/// An unquoted or quoted shell word, with the quotes taken off.
fn word(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    if let Some(q) = quoted(s) {
        return Some(q);
    }
    let end = s.find(char::is_whitespace).unwrap_or(s.len());
    (end > 0).then(|| (s[..end].to_string(), &s[end..]))
}

/// What's left is nothing but an end of statement or a comment.
fn ends(rest: &str, kind: Kind) -> bool {
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(';').unwrap_or(rest).trim_start();
    rest.is_empty()
        || match kind {
            Kind::Lua => rest.starts_with("--"),
            _ => rest.starts_with('#'),
        }
}

/// The value a line sets the variable to, when that is all it does.
pub fn value_of(kind: Kind, line: &str) -> Option<String> {
    let line = line.trim();
    match kind {
        Kind::Lua => {
            let rest = line.strip_prefix("hl.env")?.trim_start().strip_prefix('(')?;
            let (name, rest) = quoted(rest.trim_start())?;
            let rest = rest.trim_start().strip_prefix(',')?;
            let (value, rest) = quoted(rest.trim_start())?;
            let rest = rest.trim_start().strip_prefix(')')?;
            (name == VAR && ends(rest, kind)).then_some(value)
        }
        Kind::Hyprlang => {
            let rest = line.strip_prefix("env")?.trim_start().strip_prefix('=')?;
            let (name, value) = rest.split_once(',')?;
            let value = value.split(" #").next()?.trim();
            (name.trim() == VAR).then(|| value.to_string())
        }
        Kind::Shell => {
            let rest = line.strip_prefix("export ").map(str::trim_start).unwrap_or(line);
            let rest = rest.strip_prefix(VAR)?.strip_prefix('=')?;
            if rest.is_empty() {
                return Some(String::new());
            }
            let (value, rest) = word(rest)?;
            ends(rest, kind).then_some(value)
        }
        Kind::Fish => {
            let mut rest = line.strip_prefix("set ")?;
            loop {
                let (w, after) = word(rest)?;
                if w.starts_with('-') {
                    rest = after;
                    continue;
                }
                if w != VAR {
                    return None;
                }
                let (value, after) = word(after).unwrap_or_default();
                return ends(after, kind).then_some(value);
            }
        }
    }
}

/// Split a line into its indent and the rest.
fn indent(line: &str) -> (&str, &str) {
    let at = line.len() - line.trim_start().len();
    line.split_at(at)
}

/// The text a taken-out line had, when `line` is one.
fn taken_text(kind: Kind, line: &str) -> Option<String> {
    let (lead, rest) = indent(line);
    let rest = rest.strip_prefix(kind.comment())?.strip_prefix(MARK)?;
    Some(format!("{lead}{rest}"))
}

/// Every line in a file that sets the variable, or did until taken out.
pub fn settings(source: &Source, text: &str) -> Vec<Setting> {
    text.lines()
        .enumerate()
        .filter_map(|(i, line)| {
            let (text, taken) = match taken_text(source.kind, line) {
                Some(t) => (t, true),
                None => (line.to_string(), false),
            };
            Some(Setting {
                path: source.path.clone(),
                line: i + 1,
                value: value_of(source.kind, &text)?,
                taken,
            })
        })
        .collect()
}

/// Every setting in every source.
pub fn find(sources: &[Source]) -> Vec<Setting> {
    sources
        .iter()
        .flat_map(|s| settings(s, &std::fs::read_to_string(&s.path).unwrap_or_default()))
        .collect()
}

/// Comment out each line that sets the variable.
pub fn take_out(kind: Kind, text: &str) -> String {
    map_lines(text, |line| {
        value_of(kind, line)?;
        let (lead, rest) = indent(line);
        Some(format!("{lead}{}{MARK}{rest}", kind.comment()))
    })
}

/// Put each taken-out line back as it was.
pub fn put_back(kind: Kind, text: &str) -> String {
    map_lines(text, |line| taken_text(kind, line))
}

/// Replace lines, keeping the file's line endings and final newline.
fn map_lines(text: &str, f: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len() + 64);
    for piece in text.split_inclusive('\n') {
        let (line, end) = match piece.strip_suffix("\r\n") {
            Some(l) => (l, "\r\n"),
            None => match piece.strip_suffix('\n') {
                Some(l) => (l, "\n"),
                None => (piece, ""),
            },
        };
        out.push_str(&f(line).unwrap_or_else(|| line.to_string()));
        out.push_str(end);
    }
    out
}

/// Where the variable stands, for the Qt rows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Override {
    /// Its value in this session, when set.
    pub running: Option<String>,
    /// Lines that set it.
    pub set: Vec<Setting>,
    /// Lines hyprcustard took out.
    pub taken: Vec<Setting>,
}

impl Override {
    /// Read the lines from `sources`; `running` is the session's value.
    pub fn read(running: Option<String>, sources: &[Source]) -> Override {
        let (taken, set) = find(sources).into_iter().partition(|s| s.taken);
        Override { running, set, taken }
    }

    /// Whether there is anything to say about it.
    pub fn matters(&self) -> bool {
        self.running.is_some() || !self.set.is_empty() || !self.taken.is_empty()
    }
}

/// What the Qt rows say about the variable, and the button they show.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Advice {
    /// Empty when there is nothing to say.
    pub note: String,
    /// A problem, rather than a note on what was done.
    pub warn: bool,
    /// `"take"` (take it out), `"put"` (put it back), or empty.
    pub action: &'static str,
}

/// Where the lines are, as `file:line` with `~` for home.
fn places(set: &[Setting]) -> String {
    let all: Vec<String> =
        set.iter().map(|s| format!("{}:{}", crate::paths::contract(&s.path), s.line)).collect();
    all.join(", ")
}

impl Override {
    /// What to say. `uses_ct`: Qt reads qt5ct/qt6ct's settings;
    /// `ct_styles`: qt6ct and qt5ct, where set up, with the style each
    /// chooses.
    pub fn advice(&self, uses_ct: bool, ct_styles: &[(&str, Option<String>)]) -> Advice {
        let running = self.running.as_deref();
        if !self.set.is_empty() {
            // Without qt5ct/qt6ct, the variable is how a style is chosen.
            if !uses_ct {
                return Advice::default();
            }
            let value = &self.set[0].value;
            let (same, differ): (Vec<&str>, Vec<&str>) = {
                let (a, b): (Vec<_>, Vec<_>) =
                    ct_styles.iter().partition(|(_, s)| s.as_deref() == Some(value.as_str()));
                (a.iter().map(|(n, _)| *n).collect(), b.iter().map(|(n, _)| *n).collect())
            };
            let whose = |names: &[&str]| {
                names.iter().map(|n| format!("{n}'s")).collect::<Vec<_>>().join(" and ")
            };
            let mut note = format!(
                "{VAR} is set to {value} in {}. qt6ct warns about it, and Qt apps ignore the \
                 style chosen in qt6ct while it is set.",
                places(&self.set)
            );
            if ct_styles.is_empty() {
                note.push_str(&format!(
                    " Once it is out, choose {value} as the style in qt6ct to keep the look."
                ));
            } else if differ.is_empty() {
                note.push_str(&format!(
                    " {} style is already {value}, so taking it out changes nothing you can see.",
                    whose(&same)
                ));
            } else {
                note.push_str(&format!(
                    " Taking it out also sets {} style to {value}, so apps look the same.",
                    whose(&differ)
                ));
            }
            return Advice { note, warn: true, action: "take" };
        }
        if !self.taken.is_empty() {
            let note = match running {
                Some(_) => format!(
                    "{VAR} is taken out of {}. It is gone once you log out and back in.",
                    places(&self.taken)
                ),
                None => format!("hyprcustard took {VAR} out of {}.", places(&self.taken)),
            };
            return Advice { note, warn: false, action: "put" };
        }
        match running {
            Some(value) if uses_ct => Advice {
                note: format!(
                    "{VAR} is set to {value}, and qt6ct warns about it, but not in your \
                     Hyprland config, environment.d, uwsm or shell profiles. Take it out where \
                     it is set (/etc/environment, perhaps) and choose the style in qt6ct."
                ),
                warn: true,
                action: "",
            },
            _ => Advice::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(line: usize, taken: bool) -> Setting {
        Setting { path: "/x/envvars.conf".into(), line, value: "kvantum-dark".into(), taken }
    }

    #[test]
    fn the_rows_offer_what_fits_where_the_variable_stands() {
        let k = || Some("kvantum-dark".to_string());
        let set = Override { running: k(), set: vec![at(12, false)], taken: vec![] };
        let a = set.advice(true, &[("qt6ct", k()), ("qt5ct", k())]);
        assert_eq!((a.warn, a.action), (true, "take"));
        assert!(a.note.contains("/x/envvars.conf:12"), "{}", a.note);
        assert!(a.note.contains("qt6ct's and qt5ct's style is already kvantum-dark"), "{}", a.note);
        let a = set.advice(true, &[("qt6ct", k()), ("qt5ct", Some("Fusion".into()))]);
        assert!(a.note.contains("also sets qt5ct's style to kvantum-dark"), "{}", a.note);
        assert!(set.advice(true, &[]).note.contains("choose kvantum-dark as the style"));
        // Without qt6ct the variable is how the style is chosen.
        assert_eq!(set.advice(false, &[]), Advice::default());

        let taken = Override { running: k(), set: vec![], taken: vec![at(12, true)] };
        let a = taken.advice(true, &[("qt6ct", k())]);
        assert_eq!((a.warn, a.action), (false, "put"));
        assert!(a.note.contains("log out"), "{}", a.note);
        let gone = Override { running: None, ..taken };
        assert!(!gone.advice(true, &[]).note.contains("log out"));

        let elsewhere = Override { running: k(), set: vec![], taken: vec![] };
        let a = elsewhere.advice(true, &[]);
        assert_eq!((a.warn, a.action), (true, ""));
        assert_eq!(Override::default().advice(true, &[]), Advice::default());
    }

    #[test]
    fn a_line_that_only_sets_the_variable_is_read_in_each_kind() {
        let cases = [
            (Kind::Lua, r#"hl.env("QT_STYLE_OVERRIDE", "kvantum-dark")"#, Some("kvantum-dark")),
            (Kind::Lua, r#"  hl.env('QT_STYLE_OVERRIDE','kvantum') -- dark"#, Some("kvantum")),
            (Kind::Lua, r#"hl.env("QT_QPA_PLATFORMTHEME", "qt6ct")"#, None),
            (Kind::Lua, r#"hl.env("QT_STYLE_OVERRIDE", "a") hl.env("X", "b")"#, None),
            (Kind::Lua, r#"-- hl.env("QT_STYLE_OVERRIDE", "kvantum")"#, None),
            (Kind::Hyprlang, "env = QT_STYLE_OVERRIDE,kvantum", Some("kvantum")),
            (Kind::Hyprlang, "env=QT_STYLE_OVERRIDE, kvantum # why", Some("kvantum")),
            (Kind::Hyprlang, "# env = QT_STYLE_OVERRIDE,kvantum", None),
            (Kind::Shell, "QT_STYLE_OVERRIDE=kvantum-dark", Some("kvantum-dark")),
            (Kind::Shell, "export QT_STYLE_OVERRIDE=\"kvantum\"", Some("kvantum")),
            (Kind::Shell, "export QT_STYLE_OVERRIDE=kvantum; export X=1", None),
            (Kind::Shell, "QT_STYLE_OVERRIDE_X=1", None),
            (Kind::Fish, "set -gx QT_STYLE_OVERRIDE kvantum", Some("kvantum")),
            (Kind::Fish, "set --export --global QT_STYLE_OVERRIDE 'kvantum'", Some("kvantum")),
            (Kind::Fish, "set -gx QT_QPA_PLATFORMTHEME qt6ct", None),
        ];
        for (kind, line, want) in cases {
            assert_eq!(value_of(kind, line).as_deref(), want, "{kind:?}: {line}");
        }
    }

    #[test]
    fn taking_out_and_putting_back_leaves_the_file_as_it_was() {
        let cases = [
            (
                Kind::Lua,
                "hl.env(\"QT_QPA_PLATFORMTHEME\", \"qt6ct\")\n  \
                 hl.env(\"QT_STYLE_OVERRIDE\", \"kvantum-dark\")\nhl.env(\"X\", \"1\")\n",
            ),
            (Kind::Shell, "QT_QPA_PLATFORMTHEME=qt6ct\r\nQT_STYLE_OVERRIDE=kvantum-dark\r\n"),
            (Kind::Hyprlang, "env = QT_STYLE_OVERRIDE,kvantum"),
        ];
        for (kind, text) in cases {
            let source = Source { path: "f".into(), kind };
            let out = take_out(kind, text);
            assert_ne!(out, text);
            let s = settings(&source, &out);
            assert_eq!(s.len(), 1, "{out}");
            assert!(s[0].taken);
            // The taken-out line no longer sets anything, to the program
            // that reads the file.
            assert!(out.lines().all(|l| value_of(kind, l).is_none()), "{out}");
            assert_eq!(put_back(kind, &out), text);
            // Twice is the same as once.
            assert_eq!(take_out(kind, &out), out);
        }
        let lua = take_out(Kind::Lua, "  hl.env(\"QT_STYLE_OVERRIDE\", \"kvantum-dark\")\n");
        assert_eq!(lua, format!("  -- {MARK}hl.env(\"QT_STYLE_OVERRIDE\", \"kvantum-dark\")\n"));
    }

    #[test]
    fn the_session_files_are_found_through_the_configs_sources() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("config");
        let home = dir.path().join("home");
        let hypr = cfg.join("hypr");
        std::fs::create_dir_all(hypr.join("conf")).unwrap();
        std::fs::create_dir_all(cfg.join("environment.d")).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(
            hypr.join("hyprland.lua"),
            format!(
                "require(\"conf.env\")\npcall(dofile, \"{}\")  -- colors\n",
                hypr.join("colors.lua").display()
            ),
        )
        .unwrap();
        std::fs::write(hypr.join("colors.lua"), "hl.env(\"QT_STYLE_OVERRIDE\", \"fusion\")\n")
            .unwrap();
        std::fs::write(
            hypr.join("conf/env.lua"),
            "hl.env(\"QT_STYLE_OVERRIDE\", \"kvantum-dark\")\n",
        )
        .unwrap();
        std::fs::write(
            cfg.join("environment.d/envvars.conf"),
            "A=1\nQT_STYLE_OVERRIDE=kvantum-dark\n",
        )
        .unwrap();
        std::fs::write(cfg.join("environment.d/notes.txt"), "QT_STYLE_OVERRIDE=x\n").unwrap();
        std::fs::write(home.join(".profile"), "export QT_STYLE_OVERRIDE=kvantum\n").unwrap();

        let with_home = sources(&cfg, Some(&home), &hypr.join("hyprland.lua"));
        let found = find(&with_home);
        let at: Vec<(String, usize)> = found
            .iter()
            .map(|s| (s.path.strip_prefix(dir.path()).unwrap().display().to_string(), s.line))
            .collect();
        assert_eq!(
            at,
            [
                ("config/hypr/conf/env.lua".to_string(), 1),
                ("config/hypr/colors.lua".to_string(), 1),
                ("config/environment.d/envvars.conf".to_string(), 2),
                ("home/.profile".to_string(), 1),
            ]
        );
        let without = sources(&cfg, None, &hypr.join("hyprland.lua"));
        assert_eq!(find(&without).len(), 3);

        let o = Override::read(Some("kvantum-dark".into()), &without);
        assert!(o.matters());
        assert_eq!((o.set.len(), o.taken.len()), (3, 0));
    }

    #[test]
    fn a_config_that_sources_itself_is_read_once() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("hyprland.conf");
        std::fs::write(
            &cfg,
            format!("source = {}\nenv = QT_STYLE_OVERRIDE,kvantum\n", cfg.display()),
        )
        .unwrap();
        let s = sources(dir.path(), None, &cfg);
        assert_eq!(s.len(), 1);
        assert_eq!(find(&s).len(), 1);
    }
}
