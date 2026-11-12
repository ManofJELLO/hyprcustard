//! A color scheme: the Material roles and a base16 set, side by side.
//!
//! Both come out of every scheme whatever made it, so anything written for
//! another app can use either vocabulary. A wallpaper or a picked color makes
//! the Material roles directly — the same library matugen uses — and the
//! base16 set is derived from them. A named palette is the other way round:
//! it is a base16 set, and the Material roles are derived from that.

use std::collections::BTreeMap;

use material_colors::blend::harmonize;
use material_colors::color::Rgb;
use material_colors::dynamic_color::{DynamicScheme, Variant};
use material_colors::hct::Hct;
use serde::{Deserialize, Serialize};

/// Every Material role a scheme carries, in the order matugen lists them.
pub const ROLES: &[&str] = &[
    "background",
    "on_background",
    "surface",
    "surface_dim",
    "surface_bright",
    "surface_container_lowest",
    "surface_container_low",
    "surface_container",
    "surface_container_high",
    "surface_container_highest",
    "on_surface",
    "surface_variant",
    "on_surface_variant",
    "inverse_surface",
    "inverse_on_surface",
    "outline",
    "outline_variant",
    "shadow",
    "scrim",
    "surface_tint",
    "primary",
    "on_primary",
    "primary_container",
    "on_primary_container",
    "inverse_primary",
    "secondary",
    "on_secondary",
    "secondary_container",
    "on_secondary_container",
    "tertiary",
    "on_tertiary",
    "tertiary_container",
    "on_tertiary_container",
    "error",
    "on_error",
    "error_container",
    "on_error_container",
    "primary_fixed",
    "primary_fixed_dim",
    "on_primary_fixed",
    "on_primary_fixed_variant",
    "secondary_fixed",
    "secondary_fixed_dim",
    "on_secondary_fixed",
    "on_secondary_fixed_variant",
    "tertiary_fixed",
    "tertiary_fixed_dim",
    "on_tertiary_fixed",
    "on_tertiary_fixed_variant",
    "source_color",
];

/// The styles a seed color can be turned into a scheme with, as
/// `(id, label, hint)`. The ids are matugen's `--type` names without the
/// `scheme-` prefix.
pub const STYLES: &[(&str, &str, &str)] = &[
    ("tonal-spot", "Tonal spot", "Calm: the seed's hue, softly. Matugen's default."),
    ("content", "Content", "Keeps close to the wallpaper's actual colors."),
    ("fidelity", "Fidelity", "Like Content, holding the seed's own chroma."),
    ("vibrant", "Vibrant", "Saturated accents, colorful surfaces."),
    ("expressive", "Expressive", "Playful: the hues are shifted away from the seed."),
    ("rainbow", "Rainbow", "Gray surfaces, colorful accents."),
    ("fruit-salad", "Fruit salad", "Bold, with the hues rotated."),
    ("neutral", "Neutral", "Nearly gray, with a hint of the seed."),
    ("monochrome", "Monochrome", "Grays only."),
];

/// A finished scheme.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scheme {
    /// What it is called: a palette's name, or what the seed came from.
    pub name: String,
    /// Whether the surfaces are dark.
    pub dark: bool,
    /// The seed color, or a named palette's main accent, as `#rrggbb`.
    pub source: String,
    /// Every name in [`ROLES`], as `#rrggbb`.
    pub roles: BTreeMap<String, String>,
    /// `base00` to `base0F`, as `#rrggbb`.
    pub base16: Vec<String>,
}

impl Scheme {
    /// A role's color, or black for a name the scheme does not have.
    pub fn role(&self, name: &str) -> &str {
        self.roles.get(name).map(String::as_str).unwrap_or("#000000")
    }

    /// `base00` .. `base0F` by number.
    pub fn base(&self, n: usize) -> &str {
        self.base16.get(n).map(String::as_str).unwrap_or("#000000")
    }

    /// Every name a colors file defines, with its color: the roles, then the
    /// base16 set. This is the documented vocabulary of every file written.
    pub fn named(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> =
            ROLES.iter().map(|r| (r.to_string(), self.role(r).to_string())).collect();
        for (i, c) in self.base16.iter().enumerate() {
            out.push((format!("base{i:02X}"), c.clone()));
        }
        out
    }
}

/// The Material variant behind a style id, defaulting to tonal spot.
pub fn variant(style: &str) -> Variant {
    match style {
        "content" => Variant::Content,
        "fidelity" => Variant::Fidelity,
        "vibrant" => Variant::Vibrant,
        "expressive" => Variant::Expressive,
        "rainbow" => Variant::Rainbow,
        "fruit-salad" => Variant::FruitSalad,
        "neutral" => Variant::Neutral,
        "monochrome" => Variant::Monochrome,
        _ => Variant::TonalSpot,
    }
}

/// Build a scheme from one seed color.
///
/// `contrast` runs from -1 (softer) through 0 (as designed) to 1 (sharper).
pub fn from_seed(name: &str, seed: Rgb, style: &str, dark: bool, contrast: f64) -> Scheme {
    let s = DynamicScheme::by_variant(seed, &variant(style), dark, Some(contrast.clamp(-1.0, 1.0)));
    let mut roles = BTreeMap::new();
    for name in ROLES {
        let rgb = if *name == "source_color" { seed } else { role_of(&s, name) };
        roles.insert(name.to_string(), hex(rgb));
    }
    Scheme {
        name: name.to_string(),
        dark,
        source: hex(seed),
        roles,
        base16: base16_from_material(&s, seed, dark),
    }
}

fn role_of(s: &DynamicScheme, name: &str) -> Rgb {
    match name {
        "background" => s.background(),
        "on_background" => s.on_background(),
        "surface" => s.surface(),
        "surface_dim" => s.surface_dim(),
        "surface_bright" => s.surface_bright(),
        "surface_container_lowest" => s.surface_container_lowest(),
        "surface_container_low" => s.surface_container_low(),
        "surface_container" => s.surface_container(),
        "surface_container_high" => s.surface_container_high(),
        "surface_container_highest" => s.surface_container_highest(),
        "on_surface" => s.on_surface(),
        "surface_variant" => s.surface_variant(),
        "on_surface_variant" => s.on_surface_variant(),
        "inverse_surface" => s.inverse_surface(),
        "inverse_on_surface" => s.inverse_on_surface(),
        "outline" => s.outline(),
        "outline_variant" => s.outline_variant(),
        "shadow" => s.shadow(),
        "scrim" => s.scrim(),
        "surface_tint" => s.surface_tint(),
        "primary" => s.primary(),
        "on_primary" => s.on_primary(),
        "primary_container" => s.primary_container(),
        "on_primary_container" => s.on_primary_container(),
        "inverse_primary" => s.inverse_primary(),
        "secondary" => s.secondary(),
        "on_secondary" => s.on_secondary(),
        "secondary_container" => s.secondary_container(),
        "on_secondary_container" => s.on_secondary_container(),
        "tertiary" => s.tertiary(),
        "on_tertiary" => s.on_tertiary(),
        "tertiary_container" => s.tertiary_container(),
        "on_tertiary_container" => s.on_tertiary_container(),
        "error" => s.error(),
        "on_error" => s.on_error(),
        "error_container" => s.error_container(),
        "on_error_container" => s.on_error_container(),
        "primary_fixed" => s.primary_fixed(),
        "primary_fixed_dim" => s.primary_fixed_dim(),
        "on_primary_fixed" => s.on_primary_fixed(),
        "on_primary_fixed_variant" => s.on_primary_fixed_variant(),
        "secondary_fixed" => s.secondary_fixed(),
        "secondary_fixed_dim" => s.secondary_fixed_dim(),
        "on_secondary_fixed" => s.on_secondary_fixed(),
        "on_secondary_fixed_variant" => s.on_secondary_fixed_variant(),
        "tertiary_fixed" => s.tertiary_fixed(),
        "tertiary_fixed_dim" => s.tertiary_fixed_dim(),
        "on_tertiary_fixed" => s.on_tertiary_fixed(),
        "on_tertiary_fixed_variant" => s.on_tertiary_fixed_variant(),
        _ => Rgb::new(0, 0, 0),
    }
}

/// base16 out of a Material scheme.
///
/// The eight grays are tones of the scheme's neutral palette, so they carry
/// its tint. The eight accents keep base16's meaning — base08 is a red,
/// base0D a blue — because terminals and editors depend on that, but each
/// is pulled a little toward the seed so they sit together.
fn base16_from_material(s: &DynamicScheme, seed: Rgb, dark: bool) -> Vec<String> {
    let grays: [i32; 8] =
        if dark { [6, 12, 22, 50, 80, 90, 95, 99] } else { [98, 94, 87, 50, 30, 10, 6, 0] };
    let mut out: Vec<String> = grays.iter().map(|t| hex(s.neutral_palette.tone(*t))).collect();
    // red, orange, yellow, green, cyan, blue, magenta, brown — in HCT hues.
    let accents: [(f64, f64); 8] = [
        (25.0, 60.0),
        (55.0, 55.0),
        (95.0, 50.0),
        (145.0, 45.0),
        (195.0, 40.0),
        (265.0, 45.0),
        (330.0, 45.0),
        (50.0, 25.0),
    ];
    let tone = if dark { 75.0 } else { 40.0 };
    for (hue, chroma) in accents {
        let design: Rgb = Hct::from(hue, chroma, tone).into();
        out.push(hex(harmonize(design, seed)));
    }
    out
}

/// Build a scheme from a base16 set, deriving the Material roles.
///
/// The derivation keeps to the palette's own colors wherever a role has an
/// obvious counterpart, and mixes two of them where it does not, so a named
/// palette still looks like itself in every app.
pub fn from_base16(name: &str, dark: bool, base16: &[String; 16]) -> Scheme {
    let b = |n: usize| parse(&base16[n]).unwrap_or(Rgb::new(0, 0, 0));
    let (bg, fg) = (b(0x0), b(0x5));
    let container = |c: Rgb| mix(c, bg, 0.30);
    let on_container = |c: Rgb| mix(c, fg, 0.35);
    let black = Rgb::new(0, 0, 0);

    let accent = |c: Rgb| [c, bg, container(c), on_container(c), c, mix(c, bg, 0.75), bg, b(0x1)];
    let [primary, on_primary, primary_container, on_primary_container, ..] = accent(b(0xD));
    let [secondary, on_secondary, secondary_container, on_secondary_container, ..] = accent(b(0xC));
    let [tertiary, on_tertiary, tertiary_container, on_tertiary_container, ..] = accent(b(0xE));
    let [error, on_error, error_container, on_error_container, ..] = accent(b(0x8));

    let pairs: Vec<(&str, Rgb)> = vec![
        ("background", bg),
        ("on_background", fg),
        ("surface", bg),
        ("surface_dim", mix(bg, black, if dark { 0.80 } else { 0.92 })),
        ("surface_bright", b(0x2)),
        ("surface_container_lowest", mix(bg, black, if dark { 0.75 } else { 0.97 })),
        ("surface_container_low", mix(b(0x1), bg, 0.5)),
        ("surface_container", b(0x1)),
        ("surface_container_high", b(0x2)),
        ("surface_container_highest", mix(b(0x3), b(0x2), 0.4)),
        ("on_surface", fg),
        ("surface_variant", b(0x2)),
        ("on_surface_variant", b(0x4)),
        ("inverse_surface", fg),
        ("inverse_on_surface", bg),
        ("outline", b(0x3)),
        ("outline_variant", b(0x2)),
        ("shadow", black),
        ("scrim", black),
        ("surface_tint", primary),
        ("primary", primary),
        ("on_primary", on_primary),
        ("primary_container", primary_container),
        ("on_primary_container", on_primary_container),
        ("inverse_primary", mix(primary, bg, 0.6)),
        ("secondary", secondary),
        ("on_secondary", on_secondary),
        ("secondary_container", secondary_container),
        ("on_secondary_container", on_secondary_container),
        ("tertiary", tertiary),
        ("on_tertiary", on_tertiary),
        ("tertiary_container", tertiary_container),
        ("on_tertiary_container", on_tertiary_container),
        ("error", error),
        ("on_error", on_error),
        ("error_container", error_container),
        ("on_error_container", on_error_container),
        ("primary_fixed", primary),
        ("primary_fixed_dim", mix(primary, bg, 0.8)),
        ("on_primary_fixed", bg),
        ("on_primary_fixed_variant", b(0x1)),
        ("secondary_fixed", secondary),
        ("secondary_fixed_dim", mix(secondary, bg, 0.8)),
        ("on_secondary_fixed", bg),
        ("on_secondary_fixed_variant", b(0x1)),
        ("tertiary_fixed", tertiary),
        ("tertiary_fixed_dim", mix(tertiary, bg, 0.8)),
        ("on_tertiary_fixed", bg),
        ("on_tertiary_fixed_variant", b(0x1)),
        ("source_color", primary),
    ];
    Scheme {
        name: name.to_string(),
        dark,
        source: hex(primary),
        roles: pairs.into_iter().map(|(k, v)| (k.to_string(), hex(v))).collect(),
        base16: base16.iter().map(|c| hex(parse(c).unwrap_or(black))).collect(),
    }
}

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

/// `#rrggbb`.
pub fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.red, c.green, c.blue)
}

/// Read `#rrggbb`, `rrggbb`, `#rgb` or `#aarrggbb` (the alpha is dropped).
pub fn parse(text: &str) -> Option<Rgb> {
    let h = text.trim().trim_start_matches('#');
    if !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let h = match h.len() {
        3 => h.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 => h.to_string(),
        8 => h[2..].to_string(),
        _ => return None,
    };
    u32::from_str_radix(&h, 16).ok().map(Rgb::from_u32)
}

/// `a` weighted by `t` against `b`, in plain sRGB — `t = 1` is all `a`.
pub fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let m = |x: u8, y: u8| (x as f64 * t + y as f64 * (1.0 - t)).round().clamp(0.0, 255.0) as u8;
    Rgb::new(m(a.red, b.red), m(a.green, b.green), m(a.blue, b.blue))
}

/// Relative luminance, 0 for black to 1 for white.
pub fn luminance(c: Rgb) -> f64 {
    let lin = |v: u8| {
        let v = v as f64 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.red) + 0.7152 * lin(c.green) + 0.0722 * lin(c.blue)
}

/// How colorful a color is, in HCT chroma.
pub fn chroma(c: Rgb) -> f64 {
    Hct::new(c).get_chroma()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gruvbox() -> [String; 16] {
        [
            "282828", "3c3836", "504945", "665c54", "bdae93", "d5c4a1", "ebdbb2", "fbf1c7",
            "fb4934", "fe8019", "fabd2f", "b8bb26", "8ec07c", "83a598", "d3869b", "d65d0e",
        ]
        .map(|s| format!("#{s}"))
    }

    #[test]
    fn a_seed_scheme_has_every_role_and_sixteen_bases() {
        let s = from_seed("t", Rgb::from_u32(0x88d0ec), "tonal-spot", true, 0.0);
        for r in ROLES {
            assert!(parse(s.role(r)).is_some(), "{r}");
        }
        assert_eq!(s.roles.len(), ROLES.len());
        assert_eq!(s.base16.len(), 16);
        assert_eq!(s.role("source_color"), "#88d0ec");
    }

    #[test]
    fn dark_and_light_put_text_and_surface_on_the_right_sides() {
        let seed = Rgb::from_u32(0x6750a4);
        let d = from_seed("d", seed, "tonal-spot", true, 0.0);
        let l = from_seed("l", seed, "tonal-spot", false, 0.0);
        let lum = |s: &Scheme, r: &str| luminance(parse(s.role(r)).unwrap());
        assert!(lum(&d, "surface") < 0.1 && lum(&d, "on_surface") > 0.6);
        assert!(lum(&l, "surface") > 0.8 && lum(&l, "on_surface") < 0.1);
        // base16's grays run the same way: base00 the background, base05 text.
        let b = |s: &Scheme, n| luminance(parse(s.base(n)).unwrap());
        assert!(b(&d, 0) < b(&d, 5));
        assert!(b(&l, 0) > b(&l, 5));
    }

    #[test]
    fn matches_matugen_for_the_same_seed() {
        // matugen 4.2: `matugen color hex 88d0ec --dry-run -j hex` (tonal
        // spot, dark). Its base16 set is not compared: matugen's keeps none
        // of base16's hue meanings, which is why this one is built here.
        let s = from_seed("t", Rgb::from_u32(0x88d0ec), "tonal-spot", true, 0.0);
        for (role, want) in [
            ("primary", "#88d0ec"),
            ("surface", "#0f1416"),
            ("on_surface", "#dee3e6"),
            ("secondary", "#b3cad5"),
            ("tertiary", "#c3c3eb"),
            ("outline", "#8a9296"),
            ("surface_container", "#1b2023"),
        ] {
            assert_eq!(s.role(role), want, "{role}");
        }
    }

    #[test]
    fn base16_accents_keep_their_hue_family() {
        let s = from_seed("t", Rgb::from_u32(0x2e7d32), "tonal-spot", true, 0.0);
        let hue = |n| Hct::new(parse(s.base(n)).unwrap()).get_hue();
        // base08 is red, base0B green, base0D blue: harmonizing moves a hue
        // by at most 15 degrees.
        let near = |h: f64, want: f64| {
            let d = (h - want).rem_euclid(360.0);
            d.min(360.0 - d) <= 16.0
        };
        assert!(near(hue(0x8), 25.0), "{}", hue(0x8));
        assert!(near(hue(0xB), 145.0), "{}", hue(0xB));
        assert!(near(hue(0xD), 265.0), "{}", hue(0xD));
    }

    #[test]
    fn a_named_palette_keeps_its_own_colors() {
        let s = from_base16("Gruvbox", true, &gruvbox());
        assert_eq!(s.role("surface"), "#282828");
        assert_eq!(s.role("on_surface"), "#d5c4a1");
        assert_eq!(s.role("primary"), "#83a598");
        assert_eq!(s.role("error"), "#fb4934");
        assert_eq!(s.base(0xB), "#b8bb26");
        assert_eq!(s.roles.len(), ROLES.len());
    }

    #[test]
    fn named_lists_roles_then_bases() {
        let s = from_base16("Gruvbox", true, &gruvbox());
        let n = s.named();
        assert_eq!(n.len(), ROLES.len() + 16);
        assert_eq!(n[0].0, "background");
        assert_eq!(n.last().unwrap(), &("base0F".to_string(), "#d65d0e".to_string()));
    }

    #[test]
    fn hex_reading() {
        assert_eq!(parse("#abc").map(hex), Some("#aabbcc".into()));
        assert_eq!(parse("ff112233").map(hex), Some("#112233".into()));
        assert_eq!(parse("#12345"), None);
        assert_eq!(parse("#gggggg"), None);
    }
}
