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
use material_colors::palette::TonalPalette;
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

/// The cast of a wallpaper's neutral areas, for surfaces to take on: its
/// hue, and how strong it is in HCT chroma.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cast {
    /// HCT hue, 0 to 360.
    pub hue: f64,
    /// HCT chroma; near 0 for a gray.
    pub chroma: f64,
}

/// A family of hue a wallpaper has (see [`super::extract::hues`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hue {
    /// HCT hue, 0 to 360.
    pub hue: f64,
    /// Its typical HCT chroma.
    pub chroma: f64,
    /// Its share of the picture's color, 0 to 1.
    pub share: f64,
}

/// What a seed scheme is built with besides the seed, style and contrast.
#[derive(Debug, Clone, PartialEq)]
pub struct Shaping {
    /// Tint the surfaces with this rather than the seed's hue.
    pub cast: Option<Cast>,
    /// How strongly surfaces are tinted: 1 is the style's own, 0 gray, up to
    /// [`MAX_TINT`].
    pub tint: f64,
    /// The wallpaper's hue families, to build base16's accents from. Empty
    /// keeps base16's own hues.
    pub hues: Vec<Hue>,
}

impl Default for Shaping {
    fn default() -> Self {
        Self { cast: None, tint: 1.0, hues: Vec::new() }
    }
}

/// The chroma at which a wallpaper's cast tints surfaces fully. Below it the
/// tint fades, so a wallpaper with truly gray shadows gets gray surfaces
/// rather than a hue read out of noise.
const FULL_CAST: f64 = 4.0;

/// The most a scheme's surfaces can be tinted, as a multiple of the style's
/// own chroma.
pub const MAX_TINT: f64 = 4.0;

/// Build a scheme from one seed color.
///
/// `contrast` runs from -1 (softer) through 0 (as designed) to 1 (sharper).
pub fn from_seed(name: &str, seed: Rgb, style: &str, dark: bool, contrast: f64) -> Scheme {
    from_seed_with(name, seed, style, dark, contrast, &Shaping::default())
}

/// [`from_seed`], shaped further by `with`.
///
/// Material gives every surface the seed's hue at low chroma, so an orange
/// seed makes brown backgrounds even when the wallpaper's darks are cool.
/// With a cast the accents still come from the seed, but the neutral
/// palettes take the cast's hue. Their chroma stays the style's own, faded
/// when the cast is weaker than [`FULL_CAST`].
///
/// The tint scales the surfaces' chroma, cast or not: 1 is the style's own,
/// 0 gray, up to [`MAX_TINT`]. Material keeps dark surfaces so faintly
/// tinted that every style and seed reads as the same near-black in an app;
/// more tint lets the wallpaper's color into backgrounds, bars and sidebars.
///
/// With hues, base16's accents are made from them (see
/// [`wallpaper_accents`]) rather than from base16's own eight hues.
pub fn from_seed_with(
    name: &str,
    seed: Rgb,
    style: &str,
    dark: bool,
    contrast: f64,
    with: &Shaping,
) -> Scheme {
    let mut s =
        DynamicScheme::by_variant(seed, &variant(style), dark, Some(contrast.clamp(-1.0, 1.0)));
    let (cast, tint) = (with.cast, with.tint);
    let tint = if tint.is_finite() { tint.clamp(0.0, MAX_TINT) } else { 1.0 };
    let (hue, variant_hue, keep) = match cast {
        Some(c) => (c.hue, c.hue, (c.chroma / FULL_CAST).clamp(0.0, 1.0)),
        None => (s.neutral_palette.hue(), s.neutral_variant_palette.hue(), 1.0),
    };
    if cast.is_some() || tint != 1.0 {
        s.neutral_palette = TonalPalette::of(hue, s.neutral_palette.chroma() * keep * tint);
        s.neutral_variant_palette =
            TonalPalette::of(variant_hue, s.neutral_variant_palette.chroma() * keep * tint);
    }
    let three = main_hues(seed, &with.hues);
    // Secondary and tertiary take the wallpaper's next two hues, at the
    // style's own strength, so the style still shows.
    if let Some(f) = three.get(1) {
        s.secondary_palette = TonalPalette::of(f.hue, s.secondary_palette.chroma());
    }
    if let Some(f) = three.get(2) {
        s.tertiary_palette = TonalPalette::of(f.hue, s.tertiary_palette.chroma());
    }
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
        base16: base16_from_material(&s, seed, style, dark, &three),
    }
}

/// The three hues a wallpaper's scheme is built from: the seed's own family
/// first, as primary, then the biggest of the rest, as secondary and
/// tertiary. Every other hue in the picture is left out, as if it were not
/// there: matching to all of them made every wallpaper's palette alike.
///
/// The seed's family is the one within [`SEED_REACH`] of it. When it has
/// none (a small bright subject picked as the most colorful color), the
/// seed stands for itself.
fn main_hues(seed: Rgb, hues: &[Hue]) -> Vec<Hue> {
    if hues.is_empty() {
        return Vec::new();
    }
    let mut by_share = hues.to_vec();
    by_share.sort_by(|a, b| b.share.total_cmp(&a.share));
    let s = Hct::new(seed);
    let own = by_share
        .iter()
        .enumerate()
        .filter(|(_, f)| hue_distance(f.hue, s.get_hue()) <= SEED_REACH)
        .min_by(|(_, a), (_, b)| {
            hue_distance(a.hue, s.get_hue()).total_cmp(&hue_distance(b.hue, s.get_hue()))
        })
        .map(|(i, _)| i);
    let primary = match own {
        Some(i) => by_share.remove(i),
        None => Hue { hue: s.get_hue(), chroma: s.get_chroma(), share: SEED_SHARE },
    };
    std::iter::once(primary).chain(by_share).take(3).collect()
}

/// How far from the seed, in degrees, a hue family counts as the seed's.
const SEED_REACH: f64 = 30.0;

/// The share a seed with no family of its own is counted as having.
const SEED_SHARE: f64 = 0.05;

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

/// base16's accents as designed — red, orange, yellow, green, cyan, blue,
/// magenta, brown — as HCT (hue, chroma).
const ACCENTS: [(f64, f64); 8] = [
    (25.0, 60.0),
    (55.0, 55.0),
    (95.0, 50.0),
    (145.0, 45.0),
    (195.0, 40.0),
    (265.0, 45.0),
    (330.0, 45.0),
    (50.0, 25.0),
];

/// base16 out of a Material scheme.
///
/// The eight grays are tones of the scheme's neutral palette, so they carry
/// its tint. Without `hues` the eight accents keep base16's meaning —
/// base08 is a red, base0D a blue — each pulled a little toward the seed so
/// they sit together. With them, the accents come from the wallpaper (see
/// [`wallpaper_accents`]). Either way their strength follows the style (see
/// [`accent_strength`]).
fn base16_from_material(
    s: &DynamicScheme,
    seed: Rgb,
    style: &str,
    dark: bool,
    hues: &[Hue],
) -> Vec<String> {
    let grays: [i32; 8] =
        if dark { [6, 12, 22, 50, 80, 90, 95, 99] } else { [98, 94, 87, 50, 30, 10, 6, 0] };
    let mut out: Vec<String> = grays.iter().map(|t| hex(s.neutral_palette.tone(*t))).collect();
    let strength = accent_strength(style, seed);
    let accents = if hues.is_empty() {
        classic_accents(seed, strength, dark)
    } else {
        wallpaper_accents(hues, seed, strength, dark)
    };
    out.extend(accents.into_iter().map(hex));
    out
}

/// The tone accents are drawn at.
fn accent_tone(dark: bool) -> f64 {
    if dark {
        75.0
    } else {
        40.0
    }
}

/// base16's own eight hues, each harmonized toward the seed.
fn classic_accents(seed: Rgb, strength: f64, dark: bool) -> [Rgb; 8] {
    let mut i = 0;
    ACCENTS.map(|(hue, chroma)| {
        // Red is also the error color, which Material keeps red in every
        // style, so it never fades below the neutral style's.
        let k = if i == 0 { strength.max(ACCENT_STRENGTH_NEUTRAL) } else { strength };
        i += 1;
        classic(hue, chroma, seed, k, dark)
    })
}

fn classic(hue: f64, chroma: f64, seed: Rgb, strength: f64, dark: bool) -> Rgb {
    harmonize(Hct::from(hue, chroma * strength, accent_tone(dark)).into(), seed)
}

/// How far, in degrees, a wallpaper's hue may be from red and still stand
/// in for base08.
const RED_REACH: f64 = 45.0;

/// The degrees of hue distance a family's whole share of the picture is
/// worth when slots choose among families: a big one wins slots a little
/// further from it than a small one.
const SHARE_PULL: f64 = 40.0;

/// The most tone the slots sharing one family spread either side of the
/// accent tone.
const TONE_SPREAD: f64 = 15.0;

fn hue_distance(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// `to - from`, the short way round, -180 to 180.
fn hue_offset(from: f64, to: f64) -> f64 {
    let d = (to - from).rem_euclid(360.0);
    if d > 180.0 {
        d - 360.0
    } else {
        d
    }
}

/// base16's accents out of the wallpaper's main hue families (see
/// [`main_hues`]).
///
/// Each slot takes the family nearest its own hue, a big family reaching a
/// little further than a small one, so the picture's colors fill the
/// palette and a hue it does not have is left out: with no cyan in it,
/// base0C becomes its nearest — a teal from the blues, or a green. Slots
/// that land on the same family are told apart by tone, and lean a few
/// degrees toward their own hue.
///
/// base08 is the exception: it is also the error color, so it stays a red
/// when the picture has none near red.
///
/// Each family's own chroma sets the accents' strength, kept within reach
/// of base16's design, and the style scales it as it does the classic ones.
fn wallpaper_accents(hues: &[Hue], seed: Rgb, strength: f64, dark: bool) -> [Rgb; 8] {
    let pick: Vec<usize> = ACCENTS
        .iter()
        .map(|(hue, _)| {
            (0..hues.len())
                .min_by(|&a, &b| {
                    let cost = |f: &Hue| hue_distance(*hue, f.hue) - SHARE_PULL * f.share;
                    cost(&hues[a]).total_cmp(&cost(&hues[b]))
                })
                .unwrap_or(0)
        })
        .collect();
    let red_stays = hue_distance(ACCENTS[0].0, hues[pick[0]].hue) > RED_REACH;
    // Within a family, the slot nearest it keeps the base tone and the
    // others step away from it, alternately lighter and darker, the steps
    // shrinking as more slots share it so all stay within the range.
    let mut tone_offset = [0.0f64; 8];
    for (f, family) in hues.iter().enumerate() {
        let mut slots: Vec<usize> =
            (0..8).filter(|&i| pick[i] == f && !(i == 0 && red_stays)).collect();
        let distance = |i: usize| hue_distance(ACCENTS[i].0, family.hue);
        slots.sort_by(|&a, &b| distance(a).total_cmp(&distance(b)));
        let reach = slots.len().saturating_sub(1).div_ceil(2).max(1) as f64;
        let step = (TONE_SPREAD / reach).min(9.0);
        for (rank, &i) in slots.iter().enumerate() {
            let side = if rank % 2 == 1 { 1.0 } else { -1.0 };
            tone_offset[i] = rank.div_ceil(2) as f64 * side * step;
        }
    }
    let base = accent_tone(dark);
    // Lighter means away from the background: up when dark, down when light.
    let away = if dark { 1.0 } else { -1.0 };
    let mut out = [Rgb::new(0, 0, 0); 8];
    for (i, (slot_hue, design)) in ACCENTS.into_iter().enumerate() {
        let f = &hues[pick[i]];
        let k = if i == 0 { strength.max(ACCENT_STRENGTH_NEUTRAL) } else { strength };
        if i == 0 && red_stays {
            out[i] = classic(slot_hue, design, seed, k, dark);
            continue;
        }
        let lean = hue_offset(f.hue, slot_hue).clamp(-15.0, 15.0);
        // base16's design chroma, against its typical 50, scales the
        // family's: brown stays a muted one.
        let chroma = f.chroma.clamp(24.0, 72.0) * design / 50.0 * k;
        let tone = base + away * tone_offset[i];
        out[i] = Hct::from((f.hue + lean).rem_euclid(360.0), chroma, tone).into();
    }
    out
}

/// The neutral style's accent strength, and the least base08 gets.
const ACCENT_STRENGTH_NEUTRAL: f64 = 0.4;

/// How strong base16's accents are for a style, against tonal spot's 1.
///
/// Terminals and prompts draw almost everything colored with these, so they
/// have to follow the style for the style to show there: monochrome makes
/// grays, neutral muted colors, vibrant and fruit salad saturated ones.
/// Content and fidelity follow the seed's own chroma, as their Material
/// palettes do. The screen's gamut still caps the result.
fn accent_strength(style: &str, seed: Rgb) -> f64 {
    match style {
        "monochrome" => 0.0,
        "neutral" => ACCENT_STRENGTH_NEUTRAL,
        "vibrant" => 1.5,
        "fruit-salad" => 1.4,
        "expressive" | "rainbow" => 1.25,
        "content" | "fidelity" => (chroma(seed) / 48.0).clamp(ACCENT_STRENGTH_NEUTRAL, 1.5),
        _ => 1.0,
    }
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

/// `c` with its hue turned by `degrees`, its chroma and tone kept as far as
/// the screen's colors allow.
pub fn turn_hue(c: Rgb, degrees: f64) -> Rgb {
    if degrees == 0.0 {
        return c;
    }
    let h = Hct::new(c);
    Hct::from((h.get_hue() + degrees).rem_euclid(360.0), h.get_chroma(), h.get_tone()).into()
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
    fn base16_accents_follow_the_style() {
        let seed = Rgb::from_u32(0xa45400);
        let accent_chroma = |style: &str| {
            let s = from_seed("t", seed, style, true, 0.0);
            (0x9..=0xE).map(|n| chroma(parse(s.base(n)).unwrap())).sum::<f64>() / 6.0
        };
        let (mono, neutral, spot, vibrant) = (
            accent_chroma("monochrome"),
            accent_chroma("neutral"),
            accent_chroma("tonal-spot"),
            accent_chroma("vibrant"),
        );
        assert!(mono < 3.0, "{mono}");
        assert!(mono < neutral && neutral < spot && spot < vibrant, "{neutral} {spot} {vibrant}");
        // Red stays a red even in monochrome: it is the error color.
        let m = from_seed("t", seed, "monochrome", true, 0.0);
        assert!(chroma(parse(m.base(0x8)).unwrap()) > 15.0);
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
    fn a_cast_tints_the_surfaces_but_not_the_accents() {
        let seed = Rgb::from_u32(0xdd882d);
        let plain = from_seed("p", seed, "tonal-spot", true, 0.0);
        let teal = Cast { hue: 190.0, chroma: 6.0 };
        let cast = from_seed_with(
            "c",
            seed,
            "tonal-spot",
            true,
            0.0,
            &Shaping { cast: Some(teal), ..Default::default() },
        );
        assert_eq!(plain.role("primary"), cast.role("primary"));
        let hue = |s: &Scheme, r: &str| Hct::new(parse(s.role(r)).unwrap()).get_hue();
        assert!(hue(&plain, "surface_container") < 90.0);
        assert!((150.0..230.0).contains(&hue(&cast, "surface_container")));
        // A gray cast makes gray surfaces, not ones tinted by noise.
        let gray = Cast { hue: 300.0, chroma: 0.0 };
        let g = from_seed_with(
            "g",
            seed,
            "tonal-spot",
            true,
            0.0,
            &Shaping { cast: Some(gray), ..Default::default() },
        );
        assert!(chroma(parse(g.role("surface_container")).unwrap()) < 1.0);
    }

    #[test]
    fn tint_scales_the_surfaces_and_leaves_the_accents() {
        let seed = Rgb::from_u32(0xa45400);
        let at = |tint: f64| {
            from_seed_with(
                "t",
                seed,
                "tonal-spot",
                true,
                0.0,
                &Shaping { tint, ..Default::default() },
            )
        };
        let (plain, none, strong) = (at(1.0), at(0.0), at(3.0));
        assert_eq!(plain, from_seed("t", seed, "tonal-spot", true, 0.0));
        let c = |s: &Scheme, r: &str| chroma(parse(s.role(r)).unwrap());
        assert!(c(&none, "surface_container") < 1.5);
        assert!(c(&strong, "surface_container") > 2.0 * c(&plain, "surface_container"));
        assert!(c(&strong, "outline") > c(&plain, "outline"));
        assert_eq!(plain.role("primary"), strong.role("primary"));
        // The hue stays the style's when there is no cast.
        let hue = |s: &Scheme| Hct::new(parse(s.role("surface_container")).unwrap()).get_hue();
        assert!((hue(&strong) - hue(&plain)).abs() < 10.0);
        // Out of range is brought back into it.
        assert_eq!(at(f64::NAN), plain);
        assert_eq!(at(99.0), at(MAX_TINT));
    }

    #[test]
    fn wallpaper_accents_keep_to_the_wallpapers_hues() {
        let seed = Rgb::from_u32(0x2684e7);
        let blue = Hue { hue: 260.0, chroma: 34.0, share: 0.9 };
        let orange = Hue { hue: 50.0, chroma: 40.0, share: 0.1 };
        let with = |hues: Vec<Hue>| Shaping { hues, ..Default::default() };
        let s = from_seed_with("t", seed, "tonal-spot", true, 0.0, &with(vec![blue, orange]));
        let hue = |s: &Scheme, n| Hct::new(parse(s.base(n)).unwrap()).get_hue();
        let near = |h: f64, want: f64, by: f64| hue_distance(h, want) <= by;
        // No cyan or green in the picture, so none in the palette: those
        // slots come from its blue, leaning a little toward their own hue.
        assert!(near(hue(&s, 0xC), 260.0, 20.0), "{}", hue(&s, 0xC));
        assert!(near(hue(&s, 0xD), 260.0, 10.0), "{}", hue(&s, 0xD));
        // Orange and brown take the orange.
        assert!(near(hue(&s, 0x9), 50.0, 20.0), "{}", hue(&s, 0x9));
        // Red is near enough the orange to come from it.
        assert!(near(hue(&s, 0x8), 50.0, 20.0), "{}", hue(&s, 0x8));
        // Slots sharing a family differ.
        let accents: std::collections::BTreeSet<&str> = (8..16).map(|n| s.base(n)).collect();
        assert_eq!(accents.len(), 8, "{:?}", s.base16);

        // With only blue, red stays a red: it is the error color.
        let b = from_seed_with("t", seed, "tonal-spot", true, 0.0, &with(vec![blue]));
        assert!(near(hue(&b, 0x8), 25.0, 16.0), "{}", hue(&b, 0x8));
        assert!((9..16).all(|n| near(hue(&b, n), 260.0, 20.0)), "{:?}", b.base16);
        // Their tones stay usable as text on the background.
        for n in 9..16 {
            let t = Hct::new(parse(b.base(n)).unwrap()).get_tone();
            assert!((58.0..=92.0).contains(&t), "base{n:02X} tone {t}");
        }
        // The style still sets their strength; no hues keeps the classic set.
        let mono = from_seed_with("t", seed, "monochrome", true, 0.0, &with(vec![blue]));
        assert!(chroma(parse(mono.base(0xD)).unwrap()) < 3.0);
        let classic = from_seed_with("t", seed, "tonal-spot", true, 0.0, &with(vec![]));
        assert_eq!(classic, from_seed("t", seed, "tonal-spot", true, 0.0));
        // Primary is the seed's either way; secondary takes the picture's
        // next hue, and with only two hues tertiary stays the style's own.
        assert_eq!(classic.role("primary"), s.role("primary"));
        let role_hue = |s: &Scheme, r| Hct::new(parse(s.role(r)).unwrap()).get_hue();
        assert!(near(role_hue(&s, "secondary"), 50.0, 12.0), "{}", role_hue(&s, "secondary"));
        assert_eq!(classic.role("tertiary"), s.role("tertiary"));
    }

    #[test]
    fn only_the_three_main_hues_are_used() {
        let seed = Rgb::from_u32(0x2684e7);
        let hues = vec![
            Hue { hue: 260.0, chroma: 34.0, share: 0.45 },
            Hue { hue: 145.0, chroma: 30.0, share: 0.25 },
            Hue { hue: 50.0, chroma: 40.0, share: 0.15 },
            Hue { hue: 330.0, chroma: 50.0, share: 0.10 },
            Hue { hue: 195.0, chroma: 30.0, share: 0.05 },
        ];
        let s = from_seed_with(
            "t",
            seed,
            "tonal-spot",
            true,
            0.0,
            &Shaping { hues: hues.clone(), ..Default::default() },
        );
        let hue = |c: &str| Hct::new(parse(c).unwrap()).get_hue();
        let near = |h: f64, want: f64, by: f64| hue_distance(h, want) <= by;
        assert!(near(hue(s.role("secondary")), 145.0, 12.0), "{}", hue(s.role("secondary")));
        assert!(near(hue(s.role("tertiary")), 50.0, 12.0), "{}", hue(s.role("tertiary")));
        // Every accent but the error red comes from blue, green or orange:
        // the magenta and cyan in the picture are left out.
        for n in 9..16 {
            let h = hue(s.base(n));
            assert!(
                [260.0, 145.0, 50.0].iter().any(|&f| near(h, f, 16.0)),
                "base{n:02X} hue {h}: {:?}",
                s.base16
            );
        }
        // No red among the three, so base08 stays red for errors.
        assert!(near(hue(s.base(8)), 25.0, 16.0), "{}", hue(s.base(8)));

        // A seed with no family of its own is primary, and the two biggest
        // families follow it.
        let three = main_hues(Rgb::from_u32(0xff2a6d), &hues);
        assert_eq!(three.len(), 3);
        assert!((three[1].hue - 260.0).abs() < 1e-9 && (three[2].hue - 145.0).abs() < 1e-9);
        // The seed's own family is taken out of the running, not repeated.
        let three = main_hues(Rgb::from_u32(0x30a040), &hues);
        assert!((three[0].hue - 145.0).abs() < 1e-9, "{three:?}");
        assert!((three[1].hue - 260.0).abs() < 1e-9 && (three[2].hue - 50.0).abs() < 1e-9);
    }

    #[test]
    fn turning_the_hue_keeps_tone() {
        let c = Rgb::from_u32(0xdd882d);
        assert_eq!(turn_hue(c, 0.0), c);
        let (a, b) = (Hct::new(c), Hct::new(turn_hue(c, 25.0)));
        assert!((b.get_hue() - a.get_hue() - 25.0).abs() < 2.0);
        assert!((b.get_tone() - a.get_tone()).abs() < 1.0);
        assert!(
            (Hct::new(turn_hue(c, -90.0)).get_hue() - (a.get_hue() + 270.0) % 360.0).abs() < 2.0
        );
    }

    #[test]
    fn hex_reading() {
        assert_eq!(parse("#abc").map(hex), Some("#aabbcc".into()));
        assert_eq!(parse("ff112233").map(hex), Some("#112233".into()));
        assert_eq!(parse("#12345"), None);
        assert_eq!(parse("#gggggg"), None);
    }
}
