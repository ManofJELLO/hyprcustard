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
        .map(|(number, l)| Line { number, text: l.trim().to_string() })
        .collect()
}

/// Lines hyprcustard commented out when it took over.
pub fn moved_lines(text: &str) -> Vec<Line> {
    text.lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let rest = l.trim_start().strip_prefix(MOVED)?;
            Some(Line { number: i + 1, text: rest.trim().to_string() })
        })
        .collect()
}

/// Whether the managed block is there.
pub fn managed(text: &str) -> bool {
    matches!(block::split_with(text, &MARKERS), Ok(Some(_)))
}

/// Comment out the user's lines and put `body` in the managed block.
pub fn hand_over(text: &str, body: &str) -> Result<String> {
    let numbers: Vec<usize> = user_lines(text).into_iter().map(|l| l.number).collect();
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
