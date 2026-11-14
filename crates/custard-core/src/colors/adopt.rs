//! Handing an app's own colors over to the scheme.
//!
//! A waybar style or rofi theme often defines its colors itself, after the
//! line that loads hyprcustard's file, so its own win and the scheme never
//! shows; and it names them in its own words (`@color_aqua`,
//! `bar-background`), which no scheme defines. Handing over pairs each of
//! those colors with one of the scheme's, comments the user's definition
//! out, and has hyprcustard's file define the name instead — in the
//! scheme's color, with the user's own transparency.
//!
//! The pairing is kept in the user's file, in the marker on the commented
//! line, so it is there to read and to change, and anything that writes the
//! colors file (the Colors page, custard-watch) finds it the same way:
//!
//! ```css
//! /* [hyprcustard sets this: base0C] @define-color color_aqua #89B482; */
//! ```
//!
//! Giving them back takes the markers off, leaving each line exactly as it
//! was. Only colors written out are offered: one defined from another
//! (`@foreground`, `shade(@bg, 1.2)`) follows that one by itself.

use material_colors::color::Rgb;
use material_colors::hct::Hct;

/// The two kinds of file a hand-over reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// GTK CSS, as waybar reads it: `@define-color name value;`.
    Gtk,
    /// A rofi theme: `name: value;` in a top-level `* { }` block.
    Rasi,
}

impl Kind {
    /// The kind for a target id, if it has one.
    pub fn of(target: &str) -> Option<Kind> {
        match target {
            "waybar" => Some(Kind::Gtk),
            "rofi" => Some(Kind::Rasi),
            _ => None,
        }
    }
}

/// What starts a handed-over definition, before the scheme color's name.
const MARK: &str = "/* [hyprcustard sets this: ";

/// One color the user's file defines.
#[derive(Debug, Clone, PartialEq)]
pub struct Own {
    /// 1-based.
    pub line: usize,
    /// As the file spells it.
    pub name: String,
    /// As the file writes it.
    pub value: String,
    /// The color, without its transparency.
    pub rgb: Rgb,
    /// 0 (clear) to 1 (solid).
    pub alpha: f64,
    /// The scheme color it would get, or has: a role or `base00`..`base0F`.
    pub role: String,
}

/// The scheme colors a definition can be paired with, as the page offers
/// them: the surfaces from darkest to lightest (for a dark scheme), the
/// accents, then base16.
pub const CHOICES: &[&str] = &[
    "surface_container_lowest",
    "background",
    "surface_container_low",
    "surface_container",
    "surface_container_high",
    "surface_container_highest",
    "outline_variant",
    "outline",
    "on_surface_variant",
    "on_surface",
    "primary",
    "on_primary",
    "primary_container",
    "secondary",
    "secondary_container",
    "tertiary",
    "error",
    "base08",
    "base09",
    "base0A",
    "base0B",
    "base0C",
    "base0D",
    "base0E",
    "base0F",
];

/// The colors `text` defines itself and has not handed over, each with the
/// scheme color it is paired with at first.
pub fn own(kind: Kind, text: &str) -> Vec<Own> {
    let found: Vec<(usize, String, String, Rgb, f64)> = statements(kind, text)
        .into_iter()
        .filter_map(|(line, span)| {
            let (name, value) = split(kind, &text_line(text, line)[span])?;
            let (rgb, alpha) = color(&value)?;
            Some((line, name, value, rgb, alpha))
        })
        .collect();
    let dark = is_dark(&found);
    found
        .into_iter()
        .map(|(line, name, value, rgb, alpha)| {
            let role = guess(&name, rgb, dark).to_string();
            Own { line, name, value, rgb, alpha, role }
        })
        .collect()
}

/// The definitions `text` has handed over, with the scheme color each gets.
pub fn taken(kind: Kind, text: &str) -> Vec<Own> {
    text.lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let (role, original) = marked(l)?;
            let (name, value) = split(kind, original)?;
            let (rgb, alpha) = color(&value)?;
            Some(Own { line: i + 1, name, value, rgb, alpha, role: role.to_string() })
        })
        .collect()
}

/// Comment out the definitions named in `pairs`, each marked with the scheme
/// color it gets. Names not defined in `text`, or paired with something that
/// is not in [`CHOICES`], are left alone.
pub fn hand_over(kind: Kind, text: &str, pairs: &[(String, String)]) -> String {
    let mine = own(kind, text);
    let spans = statements(kind, text);
    let mut lines: Vec<String> = text.split('\n').map(String::from).collect();
    for o in &mine {
        let Some((_, role)) = pairs.iter().find(|(n, _)| *n == o.name) else { continue };
        if !CHOICES.contains(&role.as_str()) {
            continue;
        }
        let Some((_, span)) = spans.iter().find(|(l, _)| *l == o.line) else { continue };
        let l = &lines[o.line - 1];
        let wrapped =
            format!("{}{MARK}{role}] {} */{}", &l[..span.start], &l[span.clone()], &l[span.end..]);
        lines[o.line - 1] = wrapped;
    }
    lines.join("\n")
}

/// Take every marker out, leaving each line as it was before.
pub fn give_back(text: &str) -> String {
    text.split('\n')
        .map(|l| match l.find(MARK) {
            Some(start) => {
                let rest = &l[start + MARK.len()..];
                let Some(close) = rest.find("] ") else { return l.to_string() };
                let inner = &rest[close + 2..];
                let Some(end) = inner.find(" */") else { return l.to_string() };
                format!("{}{}{}", &l[..start], &inner[..end], &inner[end + 3..])
            }
            None => l.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The lines hyprcustard's colors file adds for the handed-over names, each
/// in its scheme color with the user's own transparency.
pub fn lines_for(kind: Kind, taken: &[Own], color_of: impl Fn(&str) -> String) -> String {
    if taken.is_empty() {
        return String::new();
    }
    let mut out = match kind {
        Kind::Gtk => String::from("\n/* Your own names, handed over to the scheme. */\n"),
        Kind::Rasi => String::from("\n// Your own names, handed over to the scheme.\n* {\n"),
    };
    for t in taken {
        let value = with_alpha(&color_of(&t.role), t.alpha);
        match kind {
            Kind::Gtk => out.push_str(&format!("@define-color {} {value};\n", t.name)),
            Kind::Rasi => out.push_str(&format!("    {}: {value};\n", t.name)),
        }
    }
    if kind == Kind::Rasi {
        out.push_str("}\n");
    }
    out
}

/// `#rrggbb`, or `rgba(r, g, b, a)` when it is not solid.
fn with_alpha(hex: &str, alpha: f64) -> String {
    let Some(c) = super::scheme::parse(hex) else { return hex.to_string() };
    if alpha >= 0.999 {
        return super::scheme::hex(c);
    }
    let a = format!("{:.3}", alpha.clamp(0.0, 1.0));
    let a = a.trim_end_matches('0').trim_end_matches('.');
    format!("rgba({}, {}, {}, {a})", c.red, c.green, c.blue)
}

// ---------------------------------------------------------------------------
// Reading the files
// ---------------------------------------------------------------------------

fn text_line(text: &str, line: usize) -> &str {
    text.split('\n').nth(line - 1).unwrap_or("")
}

/// Every definition statement, as (line, byte range in that line): one
/// statement on a line of its own, outside any comment, ending in `;`.
fn statements(kind: Kind, text: &str) -> Vec<(usize, std::ops::Range<usize>)> {
    let mut out = Vec::new();
    let mut in_comment = false;
    // Rasi: whether we are in a top-level `* { }` block, and how deep.
    let mut depth = 0usize;
    let mut global = false;
    for (i, l) in text.split('\n').enumerate() {
        let t = l.trim_start();
        let indent = l.len() - t.len();
        if in_comment {
            if l.contains("*/") {
                in_comment = false;
            }
            continue;
        }
        if t.starts_with("/*") {
            if !t.contains("*/") {
                in_comment = true;
            }
            continue;
        }
        if t.starts_with("//") || t.is_empty() {
            continue;
        }
        match kind {
            Kind::Gtk => {
                if t.starts_with("@define-color") {
                    if let Some(end) = t.find(';') {
                        out.push((i + 1, indent..indent + end + 1));
                    }
                }
            }
            Kind::Rasi => {
                let code = t.split("/*").next().unwrap_or(t).split("//").next().unwrap_or(t);
                let opens = code.matches('{').count();
                let closes = code.matches('}').count();
                if depth == 0 && opens == 1 && closes == 0 {
                    global = code.trim_end().trim_end_matches('{').trim() == "*";
                } else if depth == 1 && global && opens == 0 && closes == 0 {
                    if let (Some(colon), Some(end)) = (code.find(':'), code.find(';')) {
                        if colon < end {
                            out.push((i + 1, indent..indent + end + 1));
                        }
                    }
                }
                depth = (depth + opens).saturating_sub(closes);
                if depth == 0 {
                    global = false;
                }
            }
        }
    }
    out
}

/// A statement's name and value.
fn split(kind: Kind, statement: &str) -> Option<(String, String)> {
    let s = statement.trim().strip_suffix(';')?.trim();
    let (name, value) = match kind {
        Kind::Gtk => {
            let rest = s.strip_prefix("@define-color")?.trim_start();
            let at = rest.find(char::is_whitespace)?;
            (&rest[..at], rest[at..].trim())
        }
        Kind::Rasi => {
            let (n, v) = s.split_once(':')?;
            (n.trim(), v.trim())
        }
    };
    let ok = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
    (!name.is_empty() && name.chars().all(ok)).then(|| (name.to_string(), value.to_string()))
}

/// A handed-over line's scheme color and its original statement.
fn marked(line: &str) -> Option<(&str, &str)> {
    let rest = &line[line.find(MARK)? + MARK.len()..];
    let (role, rest) = rest.split_once("] ")?;
    let end = rest.find(" */")?;
    Some((role, &rest[..end]))
}

/// A color written out: `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb(…)` or
/// `rgba(…)`. `None` for anything else — a name, a reference, a function of
/// another color.
fn color(value: &str) -> Option<(Rgb, f64)> {
    let v = value.trim();
    if let Some(h) = v.strip_prefix('#') {
        if !h.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let full: String = match h.len() {
            3 | 4 => h.chars().flat_map(|c| [c, c]).collect(),
            6 | 8 => h.to_string(),
            _ => return None,
        };
        let n = |i: usize| u8::from_str_radix(&full[i..i + 2], 16).ok();
        let alpha = if full.len() == 8 { n(6)? as f64 / 255.0 } else { 1.0 };
        return Some((Rgb::new(n(0)?, n(2)?, n(4)?), alpha));
    }
    let inner = v.strip_prefix("rgba(").or_else(|| v.strip_prefix("rgb("))?.strip_suffix(')')?;
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    if !(3..=4).contains(&parts.len()) {
        return None;
    }
    let channel = |p: &str| -> Option<u8> {
        match p.strip_suffix('%') {
            Some(pc) => Some((pc.parse::<f64>().ok()? * 2.55).round().clamp(0.0, 255.0) as u8),
            None => Some(p.parse::<f64>().ok()?.round().clamp(0.0, 255.0) as u8),
        }
    };
    let alpha = match parts.get(3) {
        Some(a) => match a.strip_suffix('%') {
            Some(pc) => pc.parse::<f64>().ok()? / 100.0,
            None => a.parse::<f64>().ok()?,
        },
        None => 1.0,
    };
    Some((
        Rgb::new(channel(parts[0])?, channel(parts[1])?, channel(parts[2])?),
        alpha.clamp(0.0, 1.0),
    ))
}

// ---------------------------------------------------------------------------
// Pairing
// ---------------------------------------------------------------------------

/// Whether the file's own colors are a dark theme: its background, if it
/// names one, else most of its grays.
fn is_dark(found: &[(usize, String, String, Rgb, f64)]) -> bool {
    let tone = |c: Rgb| Hct::new(c).get_tone();
    if let Some((.., c, _)) = found.iter().find(|(_, n, ..)| {
        let n = n.to_ascii_lowercase();
        n == "background" || n == "bg" || n == "base"
    }) {
        return tone(*c) < 50.0;
    }
    let grays: Vec<f64> =
        found.iter().filter(|(.., c, _)| !is_colorful(*c)).map(|(.., c, _)| tone(*c)).collect();
    grays.is_empty() || grays.iter().filter(|t| **t < 50.0).count() * 2 >= grays.len()
}

/// The scheme color a definition starts out paired with: by its name when
/// the name says, else by the color itself — a gray by how light it is, a
/// color by its hue.
pub fn guess(name: &str, rgb: Rgb, dark: bool) -> &'static str {
    let n = name.to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| n.contains(w));
    if let Some(exact) = CHOICES.iter().find(|c| n == **c || n == c.replace('_', "-")) {
        return exact;
    }
    // Words for a hue, before anything else: `color_aqua` is aqua, whatever
    // shade of green the theme gave it.
    for (words, base) in [
        (&["red", "urgent", "critical", "error"][..], "base08"),
        (&["orange"][..], "base09"),
        (&["yellow", "gold", "warn"][..], "base0A"),
        (&["green", "ok", "success"][..], "base0B"),
        (&["aqua", "cyan", "teal"][..], "base0C"),
        (&["blue"][..], "base0D"),
        (&["purple", "magenta", "violet", "pink"][..], "base0E"),
        (&["brown"][..], "base0F"),
    ] {
        if has(words) {
            return base;
        }
    }
    let colorful = is_colorful(rgb);
    if colorful && has(&["accent", "primary", "selection", "selected", "highlight", "active"]) {
        return "primary";
    }
    // Names that mean a gray, whatever tint the theme gave it.
    if has(&["foreground", "fg", "text", "white"]) {
        return if dark { "on_surface" } else { "background" };
    }
    if has(&["black"]) {
        return if dark { "surface_container_lowest" } else { "on_surface" };
    }
    if has(&["grey", "gray", "muted", "comment"]) {
        return "outline";
    }
    if has(&["border"]) {
        return "outline_variant";
    }
    if colorful {
        let h = hue(rgb);
        return match h {
            h if !(15.0..345.0).contains(&h) => "base08",
            h if h < 40.0 => "base09",
            h if h < 70.0 => "base0A",
            h if h < 160.0 => "base0B",
            h if h < 200.0 => "base0C",
            h if h < 260.0 => "base0D",
            _ => "base0E",
        };
    }
    // A gray by how far it is from the background end of the theme.
    let tone = Hct::new(rgb).get_tone();
    let from_bg = if dark { tone } else { 100.0 - tone };
    match from_bg {
        t if t < 13.0 => "surface_container_lowest",
        t if t < 20.0 => "background",
        t if t < 26.0 => "surface_container",
        t if t < 30.0 => "surface_container_high",
        t if t < 36.0 => "surface_container_highest",
        t if t < 45.0 => "outline_variant",
        t if t < 58.0 => "outline",
        t if t < 75.0 => "on_surface_variant",
        _ => "on_surface",
    }
}

/// Whether a color reads as a color rather than a tinted gray. HCT chroma,
/// not HSL saturation, which calls a pale cream as colorful as a red.
fn is_colorful(c: Rgb) -> bool {
    Hct::new(c).get_chroma() >= 16.0
}

fn channels(c: Rgb) -> (f64, f64, f64) {
    (c.red as f64 / 255.0, c.green as f64 / 255.0, c.blue as f64 / 255.0)
}

/// HSL hue, 0 to 360.
fn hue(c: Rgb) -> f64 {
    let (r, g, b) = channels(c);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let d = max - min;
    if d == 0.0 {
        return 0.0;
    }
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    h * 60.0
}

#[cfg(test)]
mod tests {
    use super::*;

    // Trimmed from a real waybar style.
    const WAYBAR: &str = "\
@import \"hyprcustard-colors.css\";

/* Colors */
@define-color background            rgba(40,40,40,0.8); /* #282828;*/
/* @define-color hard_background       #1B1B1B; */
@define-color hard_background       rgba(27,27,27,1.0);
@define-color second_background     #45403D;
@define-color foreground            #EBDBB2;
@define-color selection_foreground  #D8A657;
@define-color color_blue            #7DAEA3;
@define-color color_red        \t    #EA6962;
@define-color color_aqua            #89B482;
@define-color color_grey            #7C6F64;
@define-color color_white           #A89984;
@define-color derived               shade(@background, 1.2);
window#waybar { background-color: @background; color: @foreground; }
";

    // Trimmed from a real rofi config.
    const ROFI: &str = "\
@import \"hyprcustard-colors.rasi\"
configuration {
    modi: \"window,run,drun\";
    terminal: \"ghostty\";
}
* {
    bar-background:              rgba(40,40,40,0.8);   /* waybar @background */
    shadow-1:                    rgba(27,27,27,0.44);
    foreground:                  #EBDBB2;
    blue:                        #7DAEA3FF;
    aqua:                        #89B482;
    background-color:            transparent;
    text-color:                  @foreground;
    spacing:                     0;
}
window { border-color: #ff0000; }
";

    fn pairs(own: &[Own]) -> Vec<(String, String)> {
        own.iter().map(|o| (o.name.clone(), o.role.clone())).collect()
    }

    #[test]
    fn waybar_colors_are_found_and_paired() {
        let own = own(Kind::Gtk, WAYBAR);
        let names: Vec<&str> = own.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "background",
                "hard_background",
                "second_background",
                "foreground",
                "selection_foreground",
                "color_blue",
                "color_red",
                "color_aqua",
                "color_grey",
                "color_white"
            ],
            "commented, derived and rule lines are left out"
        );
        let role = |n: &str| own.iter().find(|o| o.name == n).unwrap().role.as_str();
        assert_eq!(role("background"), "background");
        assert_eq!(role("hard_background"), "surface_container_lowest");
        assert_eq!(role("second_background"), "surface_container_high");
        assert_eq!(role("foreground"), "on_surface");
        assert_eq!(role("selection_foreground"), "primary");
        assert_eq!(role("color_blue"), "base0D");
        assert_eq!(role("color_red"), "base08");
        assert_eq!(role("color_aqua"), "base0C", "named aqua, though the hue is green");
        assert_eq!(role("color_grey"), "outline");
        assert_eq!(role("color_white"), "on_surface");
        assert!((own[0].alpha - 0.8).abs() < 1e-9);
    }

    #[test]
    fn rofis_global_block_is_read_and_nothing_else() {
        let own = own(Kind::Rasi, ROFI);
        let names: Vec<&str> = own.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(names, ["bar-background", "shadow-1", "foreground", "blue", "aqua"]);
        let shadow = &own[1];
        assert_eq!(shadow.role, "surface_container_lowest");
        assert!((shadow.alpha - 0.44).abs() < 1e-9);
        assert!((own[3].alpha - 1.0).abs() < 1e-9, "#rrggbbFF is solid");
    }

    #[test]
    fn handing_over_and_back_round_trips_exactly() {
        for (kind, text) in [(Kind::Gtk, WAYBAR), (Kind::Rasi, ROFI)] {
            let mine = own(kind, text);
            let taken_text = hand_over(kind, text, &pairs(&mine));
            assert!(own(kind, &taken_text).is_empty(), "{kind:?}: all handed over");
            let back = taken(kind, &taken_text);
            assert_eq!(back.len(), mine.len());
            for (a, b) in mine.iter().zip(&back) {
                assert_eq!((&a.name, &a.value, &a.role), (&b.name, &b.value, &b.role));
            }
            assert_eq!(give_back(&taken_text), text, "{kind:?}");
        }
        // The trailing comment stays outside the marker, so nothing nests.
        let one =
            hand_over(Kind::Gtk, WAYBAR, &[("background".into(), "surface_container".into())]);
        assert!(one.contains(
            "/* [hyprcustard sets this: surface_container] @define-color background            \
             rgba(40,40,40,0.8); */ /* #282828;*/"
        ));
        // Anything not offered is not taken.
        let bad = hand_over(Kind::Gtk, WAYBAR, &[("background".into(), "nonsense".into())]);
        assert_eq!(bad, WAYBAR);
    }

    #[test]
    fn the_colors_file_defines_the_names_in_scheme_colors() {
        let taken_text = hand_over(Kind::Rasi, ROFI, &pairs(&own(Kind::Rasi, ROFI)));
        let t = taken(Kind::Rasi, &taken_text);
        let lines = lines_for(Kind::Rasi, &t, |role| {
            if role == "background" {
                "#102030".into()
            } else {
                "#ffffff".into()
            }
        });
        assert!(lines.contains("    bar-background: rgba(16, 32, 48, 0.8);"), "{lines}");
        assert!(lines.contains("    foreground: #ffffff;"), "{lines}");
        assert!(lines.trim_end().ends_with('}'));
        assert_eq!(lines_for(Kind::Gtk, &[], |_| String::new()), "");
    }

    #[test]
    fn colors_are_read_in_every_written_form() {
        let solid = |v: &str| color(v).map(|(c, a)| (super::super::scheme::hex(c), a));
        assert_eq!(solid("#abc"), Some(("#aabbcc".into(), 1.0)));
        assert_eq!(solid("#11223380").map(|(h, _)| h), Some("#112233".into()));
        assert_eq!(solid("rgb(255, 0, 0)"), Some(("#ff0000".into(), 1.0)));
        assert_eq!(solid("rgba(100%,0%,0%,50%)"), Some(("#ff0000".into(), 0.5)));
        assert_eq!(solid("@foreground"), None);
        assert_eq!(solid("transparent"), None);
        assert_eq!(solid("shade(@bg, 1.2)"), None);
    }
}
