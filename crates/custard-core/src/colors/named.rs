//! Named palettes: a few built in, and any base16 scheme file the user adds.
//!
//! base16 is the format nearly every popular palette is already published
//! in (the tinted-theming collection alone has hundreds), so a user scheme is
//! simply one of those files dropped into `~/.config/hyprcustard/schemes/`.

use std::path::Path;

use super::scheme::{self, luminance, Scheme};

/// A palette that can be chosen by name.
#[derive(Debug, Clone, PartialEq)]
pub struct Named {
    /// `gruvbox-dark`, or a file's stem.
    pub id: String,
    /// What the list shows.
    pub name: String,
    /// Whether it is a dark palette.
    pub dark: bool,
    /// `base00` .. `base0F` as `#rrggbb`.
    pub base16: [String; 16],
    /// Where it came from: `None` for a built-in.
    pub file: Option<String>,
}

impl Named {
    /// The full scheme.
    pub fn scheme(&self) -> Scheme {
        scheme::from_base16(&self.name, self.dark, &self.base16)
    }
}

fn builtin(id: &str, name: &str, dark: bool, b: [&str; 16]) -> Named {
    Named { id: id.into(), name: name.into(), dark, base16: b.map(|s| format!("#{s}")), file: None }
}

/// The palettes that ship with the app.
pub fn builtins() -> Vec<Named> {
    vec![
        builtin(
            "gruvbox-dark",
            "Gruvbox Dark",
            true,
            [
                "282828", "3c3836", "504945", "665c54", "bdae93", "d5c4a1", "ebdbb2", "fbf1c7",
                "fb4934", "fe8019", "fabd2f", "b8bb26", "8ec07c", "83a598", "d3869b", "d65d0e",
            ],
        ),
        builtin(
            "gruvbox-light",
            "Gruvbox Light",
            false,
            [
                "fbf1c7", "ebdbb2", "d5c4a1", "bdae93", "665c54", "504945", "3c3836", "282828",
                "9d0006", "af3a03", "b57614", "79740e", "427b58", "076678", "8f3f71", "d65d0e",
            ],
        ),
        builtin(
            "catppuccin-mocha",
            "Catppuccin Mocha",
            true,
            [
                "1e1e2e", "181825", "313244", "45475a", "585b70", "cdd6f4", "f5e0dc", "b4befe",
                "f38ba8", "fab387", "f9e2af", "a6e3a1", "94e2d5", "89b4fa", "cba6f7", "f2cdcd",
            ],
        ),
        builtin(
            "catppuccin-latte",
            "Catppuccin Latte",
            false,
            [
                "eff1f5", "e6e9ef", "ccd0da", "bcc0cc", "acb0be", "4c4f69", "dc8a78", "7287fd",
                "d20f39", "fe640b", "df8e1d", "40a02b", "179299", "1e66f5", "8839ef", "dd7878",
            ],
        ),
        builtin(
            "nord",
            "Nord",
            true,
            [
                "2e3440", "3b4252", "434c5e", "4c566a", "d8dee9", "e5e9f0", "eceff4", "8fbcbb",
                "bf616a", "d08770", "ebcb8b", "a3be8c", "88c0d0", "81a1c1", "b48ead", "5e81ac",
            ],
        ),
        builtin(
            "rose-pine",
            "Rosé Pine",
            true,
            [
                "191724", "1f1d2e", "26233a", "6e6a86", "908caa", "e0def4", "e0def4", "524f67",
                "eb6f92", "f6c177", "ebbcba", "31748f", "9ccfd8", "c4a7e7", "f6c177", "524f67",
            ],
        ),
    ]
}

/// Where user palettes live.
pub fn dir() -> std::path::PathBuf {
    crate::paths::app_config_dir().join("schemes")
}

/// Every palette: the built-ins, then the folder's, which replace a built-in
/// of the same id. A file that cannot be read is reported and skipped.
pub fn load_all() -> (Vec<Named>, Vec<String>) {
    load_from(&dir())
}

/// [`load_all`] for a given folder.
pub fn load_from(dir: &Path) -> (Vec<Named>, Vec<String>) {
    let mut all = builtins();
    let mut problems = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (all, problems);
    };
    let mut paths: Vec<_> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("yaml" | "yml")))
        .collect();
    paths.sort();
    for path in paths {
        let id = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| parse_base16(&id, &text))
        {
            Ok(mut named) => {
                named.file = Some(crate::paths::contract(&path));
                all.retain(|n| n.id != named.id);
                all.push(named);
            }
            Err(e) => problems.push(format!("{}: {e}", crate::paths::contract(&path))),
        }
    }
    (all, problems)
}

/// Read a base16 scheme file.
///
/// Both shapes in the wild are taken: the original flat one
/// (`scheme:`, `base00: "282828"`) and tinted-theming's
/// (`name:`, `variant:`, and the colors under `palette:`). Only those keys
/// are looked at, so this is a line reader rather than a YAML parser — the
/// files are all this simple.
pub fn parse_base16(id: &str, text: &str) -> Result<Named, String> {
    let mut name = None;
    let mut variant = None;
    let mut base: [Option<String>; 16] = Default::default();
    for line in text.lines() {
        let line = line.split(" #").next().unwrap_or("").trim();
        let Some((key, value)) = line.split_once(':') else { continue };
        let key = key.trim();
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'').trim();
        match key {
            "scheme" | "name" if name.is_none() && !value.is_empty() => {
                name = Some(value.to_string())
            }
            "variant" => variant = Some(value.to_ascii_lowercase()),
            _ => {
                let Some(n) = key.strip_prefix("base").filter(|n| n.len() == 2) else { continue };
                let Ok(i) = usize::from_str_radix(n, 16) else { continue };
                if i < 16 {
                    let color = scheme::parse(value)
                        .ok_or_else(|| format!("{key} = {value:?} is not a color"))?;
                    base[i] = Some(scheme::hex(color));
                }
            }
        }
    }
    let missing: Vec<String> =
        (0..16).filter(|i| base[*i].is_none()).map(|i| format!("base{i:02X}")).collect();
    if !missing.is_empty() {
        return Err(format!("not a base16 scheme: no {}", missing.join(", ")));
    }
    let base16 = base.map(|c| c.unwrap_or_default());
    let dark = match variant.as_deref() {
        Some("light") => false,
        Some("dark") => true,
        _ => {
            luminance(
                scheme::parse(&base16[0]).unwrap_or(material_colors::color::Rgb::new(0, 0, 0)),
            ) < 0.4
        }
    };
    Ok(Named { id: id.into(), name: name.unwrap_or_else(|| id.into()), dark, base16, file: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_complete_and_unique() {
        let b = builtins();
        let mut ids: Vec<&str> = b.iter().map(|n| n.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), b.len());
        for n in &b {
            assert!(n.base16.iter().all(|c| scheme::parse(c).is_some()), "{}", n.id);
            // The dark flag agrees with the background.
            let bg = luminance(scheme::parse(&n.base16[0]).unwrap());
            assert_eq!(n.dark, bg < 0.4, "{}", n.id);
        }
    }

    #[test]
    fn reads_the_original_flat_format() {
        let text = "scheme: \"Ocean\"\nauthor: \"x\"\n".to_string()
            + &(0..16)
                .map(|i| format!("base{i:02X}: \"{:06x}\"\n", i * 0x101010))
                .collect::<String>();
        let n = parse_base16("ocean", &text).unwrap();
        assert_eq!(n.name, "Ocean");
        assert_eq!(n.base16[1], "#101010");
        assert!(n.dark);
    }

    #[test]
    fn reads_the_tinted_theming_format() {
        let mut text =
            String::from("system: \"base16\"\nname: \"Paper\"\nvariant: \"light\"\npalette:\n");
        for i in 0..16 {
            text.push_str(&format!("  base{i:02X}: \"#{:06x}\" # comment\n", 0x202020 + i));
        }
        let n = parse_base16("paper", &text).unwrap();
        assert_eq!(n.name, "Paper");
        assert!(!n.dark, "the variant line wins over the background");
        assert_eq!(n.base16[0xF], "#20202f");
    }

    #[test]
    fn a_short_or_broken_file_is_refused_by_name() {
        let e = parse_base16("x", "base00: \"000000\"\n").unwrap_err();
        assert!(e.contains("base01") && e.contains("base0F"), "{e}");
        let e = parse_base16("x", "base00: \"nope\"\n").unwrap_err();
        assert!(e.contains("base00"), "{e}");
    }

    #[test]
    fn a_folder_file_replaces_a_builtin_of_the_same_id() {
        let dir = tempfile::tempdir().unwrap();
        let text: String =
            (0..16).map(|i| format!("base{i:02X}: \"{:06x}\"\n", 0xffffff - i)).collect();
        std::fs::write(dir.path().join("nord.yaml"), &text).unwrap();
        std::fs::write(dir.path().join("broken.yaml"), "base00: 1\n").unwrap();
        let (all, problems) = load_from(dir.path());
        let nord: Vec<&Named> = all.iter().filter(|n| n.id == "nord").collect();
        assert_eq!(nord.len(), 1);
        assert!(!nord[0].dark);
        assert!(nord[0].file.is_some());
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("broken.yaml"));
    }
}
