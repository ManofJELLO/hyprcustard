//! Starship, which has no way to load a second file.
//!
//! A prompt's colors come from one palette in `starship.toml`, chosen by a
//! top-level `palette = "name"` and defined in `[palettes.name]`. So the
//! scheme is written as a palette of its own, `hyprcustard`, kept in a
//! colors file of hyprcustard's beside the config; once the user asks,
//! their config is pointed at that palette and holds a copy of it, which
//! every later Apply refreshes. Nothing else in their config is touched,
//! and the palette they had stays where it was, so pointing `palette` back
//! at it undoes the change.
//!
//! Starship lowercases a style's words before it looks them up, so every
//! name is spelled in lowercase here: `base0d`, not `base0D`. Its own color
//! names (`red`, `bright-blue`…) can be redefined by a palette too, and are,
//! so a prompt using them follows the scheme with no edit.
//!
//! Switching palettes would leave the names the old one defined
//! (`color_orange`, `color_bg1`…) undefined, and every style using them
//! colorless. So they are copied into the new palette, each paired with a
//! scheme color — guessed the way a hand-over guesses (see [`super::adopt`])
//! — and the pairing is kept in a comment after it, there to read and to
//! change:
//!
//! ```toml
//! color_orange = "#fe8019"  # scheme: base09
//! ```
//!
//! A name with no such comment keeps whatever value it has, so a color the
//! user adds to the palette themselves stays theirs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item, Table, Value};

use super::adopt;
use super::scheme::Scheme;
use crate::error::{Error, Result};

/// The palette's name, in `[palettes.…]` and `palette = "…"`.
pub const PALETTE: &str = "hyprcustard";

/// What starts the comment that pairs a carried-over name with a scheme
/// color.
const PAIRED: &str = "# scheme:";

/// Starship's own color names, in terminal palette order.
const ANSI: [&str; 16] = [
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "purple",
    "cyan",
    "white",
    "bright-black",
    "bright-red",
    "bright-green",
    "bright-yellow",
    "bright-blue",
    "bright-purple",
    "bright-cyan",
    "bright-white",
];

/// The config Starship reads: `$STARSHIP_CONFIG`, else `starship.toml` in
/// the config home.
pub fn config_path(config_home: &Path) -> PathBuf {
    // A test must never find the user's real config.
    let env = if cfg!(test) { None } else { std::env::var_os("STARSHIP_CONFIG") };
    env.filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| config_home.join("starship.toml"))
}

/// The colors file: the palette on its own, as a TOML table.
pub fn render(scheme: &Scheme, header: &str, terminal: &[usize; 16]) -> String {
    let mut out = String::from(header);
    out.push_str(&format!("[palettes.{PALETTE}]\n"));
    // Under the table's header, so the copy in starship.toml leaves it out.
    out.push_str(
        "# Starship cannot load this file: Add it for me copies it into your\n\
         # starship.toml, and every Apply copies it again.\n",
    );
    for (k, v) in scheme.named() {
        out.push_str(&format!("{} = \"{v}\"\n", k.to_ascii_lowercase()));
    }
    for (name, n) in ANSI.iter().zip(terminal) {
        out.push_str(&format!("{name} = \"{}\"\n", scheme.base(*n)));
    }
    out
}

/// Whether a config uses the palette: chosen at the top, and defined.
pub fn wired(config: &str) -> bool {
    let Ok(doc) = config.parse::<DocumentMut>() else { return false };
    doc.get("palette").and_then(Item::as_str) == Some(PALETTE)
        && palette_table(&doc, PALETTE).is_some()
}

/// `config` pointed at the palette, holding the one in `colors` (the
/// colors file's text), with the names of the palette it used before
/// carried over.
///
/// A config that is not valid TOML is an error rather than rewritten.
pub fn splice(config: &str, colors: &str) -> Result<String> {
    let mut doc: DocumentMut = config.parse().map_err(|e| {
        Error::other(format!("your starship.toml is not valid TOML, so it was left alone: {e}"))
    })?;
    let colors: DocumentMut = colors
        .parse()
        .map_err(|e| Error::other(format!("hyprcustard's Starship colors file is broken: {e}")))?;
    let scheme_table = palette_table(&colors, PALETTE)
        .ok_or_else(|| Error::other("hyprcustard's Starship colors file has no palette"))?;
    let scheme: Vec<(String, String)> = scheme_table
        .iter()
        .filter_map(|(k, v)| Some((k.to_string(), v.as_str()?.to_string())))
        .collect();
    let lookup: HashMap<&str, &str> =
        scheme.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

    let carried = carried(&doc, &lookup);

    let mut table = Table::new();
    let header: String = scheme_table
        .decor()
        .prefix()
        .and_then(|p| p.as_str())
        .unwrap_or("")
        .lines()
        .filter(|l| l.trim_start().starts_with('#'))
        .map(|l| format!("{l}\n"))
        .collect();
    table.decor_mut().set_prefix(format!("\n{header}"));
    for (k, v) in &scheme {
        table.insert(k, toml_edit::value(v.as_str()));
    }
    for (i, (name, value, role)) in carried.iter().enumerate() {
        let mut value = value.clone();
        let suffix = match role {
            Some(r) => format!("  {PAIRED} {r}"),
            None => value.decor().suffix().and_then(|s| s.as_str()).unwrap_or("").to_string(),
        };
        value.decor_mut().clear();
        value.decor_mut().set_prefix(" ");
        value.decor_mut().set_suffix(suffix);
        table.insert(name, Item::Value(value));
        if i == 0 {
            if let Some(mut key) = table.key_mut(name) {
                key.leaf_decor_mut().set_prefix(
                    "\n# Your own names, kept so your styles still find them. Each takes the\n\
                     # scheme color its comment names; change the name to pair it differently.\n\
                     # One with no comment keeps the value written here.\n",
                );
            }
        }
    }

    let palettes = doc.entry("palettes").or_insert_with(|| {
        let mut t = Table::new();
        t.set_implicit(true);
        Item::Table(t)
    });
    let Some(palettes) = palettes.as_table_mut() else {
        return Err(Error::other("`palettes` in your starship.toml is not a table"));
    };
    if let Some(pos) = palettes.get(PALETTE).and_then(Item::as_table).and_then(Table::position) {
        table.set_position(pos);
    }
    palettes.insert(PALETTE, Item::Table(table));

    // In place when it is there, so its line keeps its spacing and comment.
    match doc.get_mut("palette").and_then(Item::as_value_mut) {
        Some(old) => {
            let decor = old.decor().clone();
            *old = Value::from(PALETTE);
            *old.decor_mut() = decor;
        }
        None => {
            doc.insert("palette", toml_edit::value(PALETTE));
        }
    }
    Ok(doc.to_string())
}

/// The table `[palettes.<name>]`, if the document has one.
fn palette_table<'a>(doc: &'a DocumentMut, name: &str) -> Option<&'a Table> {
    doc.get("palettes")?.as_table()?.get(name)?.as_table()
}

/// The names to keep beside the scheme's, each with its value and the
/// scheme color it is paired with, if any.
///
/// From hyprcustard's palette when the config already has it — the pairing
/// read back from each comment — otherwise from the palette it uses now,
/// each paired by a guess.
fn carried(
    doc: &DocumentMut,
    scheme: &HashMap<&str, &str>,
) -> Vec<(String, Value, Option<String>)> {
    let fresh = |k: &str| !scheme.contains_key(k);
    if let Some(ours) = palette_table(doc, PALETTE) {
        return ours
            .iter()
            .filter(|(k, _)| fresh(k))
            .filter_map(|(k, item)| {
                let value = item.as_value()?;
                let role = value
                    .decor()
                    .suffix()
                    .and_then(|s| s.as_str())
                    .and_then(|s| s.trim().strip_prefix(PAIRED))
                    .map(|r| r.trim().to_ascii_lowercase())
                    .filter(|r| is_choice(r));
                let value = match &role {
                    Some(r) => Value::from(*scheme.get(r.as_str())?),
                    None => value.clone(),
                };
                Some((k.to_string(), value, role))
            })
            .collect();
    }
    let Some(theirs) =
        doc.get("palette").and_then(Item::as_str).and_then(|name| palette_table(doc, name))
    else {
        return Vec::new();
    };
    let found: Vec<(String, Value, Option<material_colors::color::Rgb>)> = theirs
        .iter()
        .filter(|(k, _)| fresh(k))
        .filter_map(|(k, item)| {
            let value = item.as_value()?;
            let rgb = value.as_str().and_then(adopt::color).map(|(rgb, _)| rgb);
            Some((k.to_string(), value.clone(), rgb))
        })
        .collect();
    let dark = adopt::dark(found.iter().filter_map(|(k, _, rgb)| Some((k.as_str(), (*rgb)?))));
    found
        .into_iter()
        .map(|(k, value, rgb)| match rgb {
            Some(rgb) => {
                let role = adopt::guess(&k, rgb, dark).to_ascii_lowercase();
                let value = scheme.get(role.as_str()).map_or(value, |v| Value::from(*v));
                (k, value, Some(role))
            }
            // A 256-color number or a color name: kept as it is.
            None => (k, value, None),
        })
        .collect()
}

/// Whether `role`, lowercased, is one a name can be paired with.
fn is_choice(role: &str) -> bool {
    adopt::CHOICES.iter().any(|c| c.eq_ignore_ascii_case(role))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colors::named::builtins;

    const TERMINAL: [usize; 16] =
        [0x0, 0x8, 0xB, 0xA, 0xD, 0xE, 0xC, 0x5, 0x3, 0x8, 0xB, 0xA, 0xD, 0xE, 0xC, 0x7];

    fn colors() -> String {
        render(&builtins()[0].scheme(), "# Colors from hyprcustard: Gruvbox Dark.\n", &TERMINAL)
    }

    /// The shape of the gruvbox-rainbow preset Starship ships.
    const PRESET: &str = "\"$schema\" = 'https://starship.rs/config-schema.json'

format = \"\"\"
[](color_orange)\\
$directory\\
[](fg:color_yellow bg:color_aqua)\\
$line_break$character\"\"\"

palette = 'gruvbox_dark'

[palettes.gruvbox_dark]
color_fg0 = '#fbf1c7'
color_bg1 = '#3c3836'
color_orange = '#d65d0e'
color_aqua = '#689d6a'
color_yellow = '#d79921'
mine = '208'

[os]
disabled = false
style = \"bg:color_orange fg:color_fg0\"
";

    #[test]
    fn the_palette_is_spelled_the_way_starship_looks_names_up() {
        let text = colors();
        assert!(text.contains("[palettes.hyprcustard]\n"));
        assert!(text.contains("base0d = \"#83a598\"\n"), "{text}");
        assert!(text.contains("on_surface = "));
        assert!(text.contains("bright-red = \"#fb4934\"\n"));
        assert!(!text.contains("base0D"));
        text.parse::<DocumentMut>().unwrap();
    }

    #[test]
    fn splicing_points_the_config_at_the_palette_and_keeps_its_names() {
        let out = splice(PRESET, &colors()).unwrap();
        assert!(wired(&out));
        let doc: DocumentMut = out.parse().unwrap();
        // Everything else as it was, the old palette included.
        assert!(out.starts_with("\"$schema\" = 'https://starship.rs/config-schema.json'\n"));
        assert!(out.contains("[](fg:color_yellow bg:color_aqua)\\\n"));
        assert!(out.contains("[palettes.gruvbox_dark]\ncolor_fg0 = '#fbf1c7'\n"));
        assert!(out.contains("[os]\ndisabled = false\n"));
        assert_eq!(doc["palette"].as_str(), Some(PALETTE));
        assert!(out.contains("palette = \"hyprcustard\"\n"), "{out}");
        // Before [os]: right after the palette it follows.
        let ours = out.find("[palettes.hyprcustard]").unwrap();
        assert!(out.find("[palettes.gruvbox_dark]").unwrap() < ours);
        assert!(ours < out.find("[os]").unwrap(), "{out}");
        assert!(out.contains("# Colors from hyprcustard: Gruvbox Dark.\n[palettes.hyprcustard]"));

        let p = palette_table(&doc, PALETTE).unwrap();
        assert_eq!(p["base0d"].as_str(), Some("#83a598"));
        // Named by hue, so the scheme's orange and cyan.
        assert_eq!(p["color_orange"].as_str(), Some("#fe8019"));
        assert!(out.contains("color_orange = \"#fe8019\"  # scheme: base09\n"), "{out}");
        assert!(out.contains("color_aqua = \"#8ec07c\"  # scheme: base0c\n"), "{out}");
        // A 256-color number is no color to pair: kept as written.
        assert!(out.contains("mine = '208'\n"), "{out}");
    }

    #[test]
    fn splicing_again_keeps_the_pairings_and_the_users_own_additions() {
        let once = splice(PRESET, &colors()).unwrap();
        let changed = once
            .replace("# scheme: base09", "# scheme: base0E")
            .replace("mine = '208'", "mine = '208'\nextra = \"#123456\"");
        let nord = builtins().into_iter().find(|n| n.scheme().name.contains("Nord")).unwrap();
        let header = "# Colors from hyprcustard: Nord.\n";
        let again = splice(&changed, &render(&nord.scheme(), header, &TERMINAL)).unwrap();
        let doc: DocumentMut = again.parse().unwrap();
        let p = palette_table(&doc, PALETTE).unwrap();
        let s = nord.scheme();
        assert_eq!(p["color_orange"].as_str(), Some(s.base(0xE)));
        assert!(again.contains("# scheme: base0e\n"));
        assert_eq!(p["base0d"].as_str(), Some(s.base(0xD)));
        assert_eq!(p["extra"].as_str(), Some("#123456"));
        assert_eq!(again.matches("[palettes.hyprcustard]").count(), 1);
        assert!(again.contains("# Colors from hyprcustard: Nord.\n"));
        assert!(!again.contains("Gruvbox Dark.\n[palettes.hyprcustard]"));
        // A third time changes nothing.
        let third = splice(&again, &render(&s, header, &TERMINAL)).unwrap();
        assert_eq!(third, again);
    }

    #[test]
    fn a_config_with_no_palette_gets_one_and_a_broken_one_is_left_alone() {
        let out = splice(
            "add_newline = false\n\n[character]\nsuccess_symbol = \"[>](green)\"\n",
            &colors(),
        )
        .unwrap();
        assert!(out.starts_with("add_newline = false\npalette = \"hyprcustard\"\n"), "{out}");
        assert!(wired(&out));
        assert!(!out.contains("Your own names"));
        assert!(splice("palette = [", &colors()).is_err());
        assert!(!wired(""));
        assert!(!wired("palette = \"hyprcustard\"\n"));
    }
}
