//! Keeping a wallpaper at the next login.
//!
//! Setting a wallpaper changes the screen now; whatever the user's config
//! starts at login still starts next time. So the page shows the lines that
//! start a wallpaper program, and offers to hand them over: they are
//! commented out — marked, so they can be given back exactly — and a managed
//! block starts what is on screen instead, rewritten every time a wallpaper
//! is picked.

use crate::block::{self, Markers};
use crate::colors::wallpaper::program;
use crate::error::Result;

/// The managed block's markers.
pub const MARKERS: Markers = Markers {
    begin: "-- >>> HyprCustard wallpaper (managed by hyprcustard) >>>",
    end: "-- <<< HyprCustard wallpaper (managed by hyprcustard) <<<",
    name: "HyprCustard wallpaper",
};

/// What a handed-over line starts with, after its indentation.
pub const MOVED: &str = "-- [moved to the HyprCustard wallpaper block] ";

/// Words that mark a line as starting a wallpaper.
const WORDS: [&str; 8] =
    ["mpvpaper", "swaybg", "hyprpaper", "awww-daemon", "swww-daemon", "awww", "swww", "wpaperd"];

/// A line in the user's config.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// 1-based.
    pub number: usize,
    /// The line, trimmed.
    pub text: String,
    /// Whether the line is one start command and nothing else, so that
    /// commenting it out cannot break the code around it.
    pub movable: bool,
}

/// Lines that start a wallpaper program at login, outside any managed block
/// and not commented out. Key bindings are left alone: a key that changes
/// the wallpaper is not what starts one.
pub fn user_lines(text: &str) -> Vec<Line> {
    lines_outside_blocks(text)
        .filter(|(_, l)| {
            let t = l.trim_start();
            !t.starts_with("--")
                && !t.starts_with('#')
                && t.contains("exec")
                && !t.contains("bind")
                && WORDS.iter().any(|w| has_word(t, w))
        })
        .map(|(number, l)| Line { number, text: l.trim().to_string(), movable: movable(l) })
        .collect()
}

/// Whether a line is a single start command: `hl.exec_cmd("...")`, perhaps
/// with a trailing comment, or a hyprlang `exec-once = ...`. Anything sharing
/// the line — a `function()` opening, an `end)`, a second call, a table —
/// would be commented out with it.
fn movable(line: &str) -> bool {
    let t = line.trim();
    if t.starts_with("exec-once") || t.starts_with("exec ") || t.starts_with("exec=") {
        return true;
    }
    // A trailing comment, when it comes after the call's last parenthesis.
    let t = match (t.rfind(')'), t.rfind("--")) {
        (Some(close), Some(dash)) if dash > close => t[..dash].trim_end(),
        _ => t,
    };
    let opens = t.matches('(').count();
    t.starts_with("hl.exec_cmd(")
        && t.ends_with(')')
        && t.matches("exec_cmd(").count() == 1
        && opens == t.matches(')').count()
        && !t.contains(['{', '}', ';'])
        && !t.contains("function")
}

/// Lines hyprcustard commented out when it took over.
pub fn moved_lines(text: &str) -> Vec<Line> {
    text.lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let rest = l.trim_start().strip_prefix(MOVED)?;
            Some(Line { number: i + 1, text: rest.trim().to_string(), movable: true })
        })
        .collect()
}

/// The mpvpaper command the user's config starts at login, from their own
/// line or one handed over to hyprcustard, as arguments.
pub fn users_mpvpaper(text: &str) -> Option<Vec<String>> {
    user_lines(text)
        .into_iter()
        .chain(moved_lines(text))
        .filter_map(|l| command_of(&l.text))
        .find(|argv| program(argv) == "mpvpaper")
}

/// The command a start line runs, split as `sh` would: the string in
/// `hl.exec_cmd("...")`, or what follows `exec-once =`. `None` for any
/// other shape, or one this cannot read for certain.
pub fn command_of(line: &str) -> Option<Vec<String>> {
    let t = line.trim();
    let cmd = if let Some(rest) = t.strip_prefix("hl.exec_cmd(") {
        lua_string(rest.trim_start())?
    } else {
        let rest = t.strip_prefix("exec-once").or_else(|| t.strip_prefix("exec"))?;
        rest.trim_start().strip_prefix('=')?.trim().to_string()
    };
    shell_split(&cmd).filter(|a| !a.is_empty())
}

/// The Lua string literal `s` starts with, unescaped.
fn lua_string(s: &str) -> Option<String> {
    let mut chars = s.chars();
    let quote = chars.next().filter(|q| *q == '"' || *q == '\'')?;
    let mut out = String::new();
    while let Some(c) = chars.next() {
        match c {
            c if c == quote => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                c @ ('"' | '\'' | '\\') => out.push(c),
                // Anything rarer is not a start line worth guessing at.
                _ => return None,
            },
            c => out.push(c),
        }
    }
    None
}

/// Split a command line as `sh` would, for the quoting a start line uses:
/// single and double quotes and backslashes. `None` for anything that needs
/// a shell to read: an unclosed quote, a variable, a pipe or a `;`.
fn shell_split(s: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' => {
                if in_word {
                    out.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            '\'' => {
                in_word = true;
                loop {
                    match chars.next()? {
                        '\'' => break,
                        c => word.push(c),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => word.push(chars.next()?),
                        '$' | '`' => return None,
                        c => word.push(c),
                    }
                }
            }
            '\\' => {
                in_word = true;
                word.push(chars.next()?);
            }
            '$' | '`' | '|' | ';' | '&' | '<' | '>' | '(' | ')' => return None,
            c => {
                in_word = true;
                word.push(c);
            }
        }
    }
    if in_word {
        out.push(word);
    }
    Some(out)
}

/// The managed block as it is in the config: the line its opening marker
/// is on (1-based) and what is between the markers, for the page to show.
pub fn block_text(text: &str) -> Option<(usize, String)> {
    let split = block::split_with(text, &MARKERS).ok()??;
    let line = split.before.lines().count() + 1;
    Some((line, split.inner.trim_end().to_string()))
}

/// Whether the managed block is there.
pub fn managed(text: &str) -> bool {
    matches!(block::split_with(text, &MARKERS), Ok(Some(_)))
}

/// Comment out the user's lines and put `body` in the managed block.
///
/// Refused, with nothing changed, when a line that starts a wallpaper also
/// holds other code: commenting it out would break the config.
pub fn hand_over(text: &str, body: &str) -> Result<String> {
    let lines = user_lines(text);
    let stuck: Vec<String> =
        lines.iter().filter(|l| !l.movable).map(|l| l.number.to_string()).collect();
    if !stuck.is_empty() {
        return Err(crate::error::Error::other(format!(
            "{} {} of hyprland.lua starts the wallpaper alongside other code, so it cannot \
             be commented out on its own. Put the start command on a line of its own, or \
             remove it yourself, and try again",
            if stuck.len() == 1 { "Line" } else { "Lines" },
            stuck.join(", ")
        )));
    }
    let numbers: Vec<usize> = lines.into_iter().map(|l| l.number).collect();
    let mut out: Vec<String> = Vec::new();
    for (i, l) in split_keep(text).into_iter().enumerate() {
        if numbers.contains(&(i + 1)) {
            let indent = &l[..l.len() - l.trim_start().len()];
            out.push(format!("{indent}{MOVED}{}", &l[indent.len()..]));
        } else {
            out.push(l.to_string());
        }
    }
    block::splice_with(&out.concat(), body, &MARKERS)
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

/// Rewrite the block's body, if there is a block.
pub fn update(text: &str, body: &str) -> Result<String> {
    if managed(text) {
        block::splice_with(text, body, &MARKERS)
    } else {
        Ok(text.to_string())
    }
}

/// The block's body: start each command at login.
pub fn body(commands: &[String]) -> String {
    let mut s = String::from(
        "-- The wallpaper hyprcustard set. Pick another on its Wallpaper page, or\n\
         -- use \"Give it back\" there to bring back the lines this replaced.\n\
         hl.on(\"hyprland.start\", function()\n",
    );
    for c in commands {
        s.push_str(&format!("    hl.exec_cmd({})\n", crate::model::lua_quote(c)));
    }
    s.push_str("end)\n");
    s
}

/// The shell commands that start what `program_id` is showing now, from its
/// processes. hyprpaper is started with hyprcustard's own config, which the
/// caller writes.
pub fn commands(
    program_id: &str,
    procs: &[(u32, Vec<String>)],
    hyprpaper_conf: &std::path::Path,
) -> Vec<String> {
    let Some(p) = super::setter::get(program_id) else { return Vec::new() };
    if p.id == "hyprpaper" {
        return vec![format!("hyprpaper -c {}", quote(&hyprpaper_conf.to_string_lossy()))];
    }
    let mut out: Vec<String> = procs
        .iter()
        .filter(|(_, argv)| program(argv) == p.process)
        .map(|(_, argv)| argv.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" "))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Quote one argument for `sh`.
pub fn quote(arg: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c);
    if !arg.is_empty() && arg.chars().all(safe) {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

/// Enabled systemd user services that look like they start a wallpaper.
pub fn services() -> Vec<String> {
    if cfg!(test) {
        return Vec::new();
    }
    let Ok(out) = std::process::Command::new("systemctl")
        .args(["--user", "list-unit-files", "--state=enabled", "--no-legend", "--plain"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .filter(|u| u.ends_with(".service") && WORDS.iter().any(|w| u.contains(w)))
        .map(str::to_string)
        .collect()
}

fn has_word(line: &str, word: &str) -> bool {
    let part = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    line.match_indices(word).any(|(i, _)| {
        let before = line[..i].chars().next_back();
        let after = line[i + word.len()..].chars().next();
        !before.is_some_and(part) && !after.is_some_and(part)
    })
}

/// Lines, with their 1-based numbers, that are not inside a managed block.
fn lines_outside_blocks(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut inside = false;
    text.lines().enumerate().filter_map(move |(i, l)| {
        let t = l.trim_start();
        if t.starts_with("-- >>> ") {
            inside = true;
            return None;
        }
        if t.starts_with("-- <<< ") {
            inside = false;
            return None;
        }
        (!inside).then_some((i + 1, l))
    })
}

/// Lines with their endings kept, so the file goes back together exactly.
fn split_keep(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_start_line_is_read_as_its_arguments() {
        let v = |s: &[&str]| s.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        let lua = r#"hl.exec_cmd("mpvpaper -p -a FULL -o 'no-audio --loop-file shuffle --video-zoom=.45' ALL ~/wallpaper/1920/")"#;
        assert_eq!(
            command_of(lua),
            Some(v(&[
                "mpvpaper",
                "-p",
                "-a",
                "FULL",
                "-o",
                "no-audio --loop-file shuffle --video-zoom=.45",
                "ALL",
                "~/wallpaper/1920/"
            ]))
        );
        let escaped = r#"hl.exec_cmd("mpvpaper -o \"no-audio loop\" DP-1 /w/a\\ b.mp4") -- mine"#;
        assert_eq!(
            command_of(escaped),
            Some(v(&["mpvpaper", "-o", "no-audio loop", "DP-1", "/w/a b.mp4"]))
        );
        assert_eq!(
            command_of("exec-once = swaybg -i ~/a.png -m fill"),
            Some(v(&["swaybg", "-i", "~/a.png", "-m", "fill"]))
        );
        // Anything that needs a shell is not guessed at.
        assert_eq!(command_of(r#"hl.exec_cmd("mpvpaper ALL $WALL")"#), None);
        assert_eq!(command_of("exec-once = sleep 1; mpvpaper ALL ~/w"), None);
        assert_eq!(command_of(r#"hl.exec_cmd("mpvpaper -o 'open ALL ~/w")"#), None);
        assert_eq!(command_of("hl.exec_cmd(cmd)"), None);
    }

    #[test]
    fn the_users_mpvpaper_comes_from_their_line_or_the_one_handed_over() {
        let mine = "hl.exec_cmd(\"waybar\")\nhl.exec_cmd(\"mpvpaper -o 'no-audio' ALL ~/w/\")\n";
        let argv = users_mpvpaper(mine).unwrap();
        assert_eq!(argv[2], "no-audio");
        let moved = format!("    {MOVED}hl.exec_cmd(\"mpvpaper -p ALL ~/w/\")\n");
        assert_eq!(users_mpvpaper(&moved).unwrap()[1], "-p");
        assert_eq!(users_mpvpaper("hl.exec_cmd(\"swaybg -i ~/a.png\")\n"), None);
    }

    const CONFIG: &str = "\
hl.on(\"hyprland.start\", function()
    hl.exec_cmd(\"waybar\")
    hl.exec_cmd(\"mpvpaper -p -o 'no-audio' ALL ~/w/\")
--    hl.exec_cmd(\"swaybg -i ~/old.png\")
end)
hl.bind(\"SUPER + W\", hl.dsp.exec_cmd(\"awww img ~/next.png\"))
hl.exec_cmd(\"hyprctl setcursor x 24\")
-- >>> HyprWindowShade (managed by hyprwindowshade-gui) >>>
hws.togglelayershader(\"mpvpaper\", \"x\")
hl.exec_cmd(\"swaybg\")
-- <<< HyprWindowShade (managed by hyprwindowshade-gui) <<<
";

    #[test]
    fn finds_only_live_lines_that_start_a_wallpaper() {
        let lines = user_lines(CONFIG);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0].number, 3);
        assert!(lines[0].text.starts_with("hl.exec_cmd(\"mpvpaper"));
        assert!(!has_word("hl.exec_cmd(\"mpvpaper-helper\")", "mpvpaper"));
        assert!(has_word("exec-once = swww-daemon", "swww-daemon"));
    }

    #[test]
    fn a_line_with_other_code_on_it_is_never_commented_out() {
        assert!(movable("    hl.exec_cmd(\"swaybg -i x.png\")"));
        assert!(movable("hl.exec_cmd(\"mpvpaper ALL /w/\")  -- the clips (shuffled)"));
        assert!(movable("exec-once = swww-daemon"));
        for bad in [
            "hl.on(\"hyprland.start\", function() hl.exec_cmd(\"swaybg -i x.png\")",
            "hl.exec_cmd(\"swaybg -i x.png\") end)",
            "hl.exec_cmd(\"swaybg\") hl.exec_cmd(\"waybar\")",
            "local cmds = { hl.exec_cmd(\"swaybg\") }",
            "hl.exec_cmd(",
        ] {
            assert!(!movable(bad), "{bad}");
        }
        let one_line =
            "hl.on(\"hyprland.start\", function() hl.exec_cmd(\"swaybg -i x.png\")\nend)\n";
        let lines = user_lines(one_line);
        assert_eq!(lines.len(), 1);
        assert!(!lines[0].movable);
        let e = hand_over(one_line, "x").unwrap_err().to_string();
        assert!(e.starts_with("Line 1 of hyprland.lua"), "{e}");
    }

    #[test]
    fn hand_over_and_give_back_round_trip() {
        let body = body(&["swaybg -o '*' -i /new.png -m fill".into()]);
        let taken = hand_over(CONFIG, &body).unwrap();
        assert!(managed(&taken));
        assert!(user_lines(&taken).is_empty());
        let moved = moved_lines(&taken);
        assert_eq!(moved.len(), 1);
        assert!(moved[0].text.contains("mpvpaper"));
        assert!(taken.contains(&format!("    {MOVED}hl.exec_cmd(\"mpvpaper")));
        assert!(taken.contains("hl.exec_cmd(\"swaybg -o '*' -i /new.png -m fill\")"));
        let (line, text) = block_text(&taken).unwrap();
        assert_eq!(taken.lines().nth(line - 1), Some(MARKERS.begin));
        assert!(text.contains("swaybg -o '*' -i /new.png") && !text.contains(MARKERS.end));
        assert_eq!(block_text(CONFIG), None);

        let updated = update(&taken, &self::body(&["awww-daemon".into()])).unwrap();
        assert!(updated.contains("awww-daemon") && !updated.contains("/new.png"));
        assert_eq!(update(CONFIG, "x").unwrap(), CONFIG, "no block, nothing written");

        assert_eq!(give_back(&updated).unwrap(), CONFIG);
    }

    #[test]
    fn commands_come_from_the_running_processes() {
        let procs = vec![
            (
                1,
                vec![
                    "mpvpaper".into(),
                    "-o".into(),
                    "no-audio loop".into(),
                    "ALL".into(),
                    "/w/it's.mp4".into(),
                ],
            ),
            (2, vec!["waybar".into()]),
        ];
        let conf = std::path::Path::new("/c/hp.conf");
        assert_eq!(
            commands("mpvpaper", &procs, conf),
            vec!["mpvpaper -o 'no-audio loop' ALL '/w/it'\\''s.mp4'".to_string()]
        );
        assert_eq!(commands("hyprpaper", &procs, conf), vec!["hyprpaper -c /c/hp.conf"]);
        assert!(commands("swaybg", &procs, conf).is_empty());
    }
}
