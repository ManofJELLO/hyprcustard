//! Hyprland's own look: gaps, borders, rounding, opacity, shadow and blur.
//!
//! The app never edits the user's own `hl.config({...})`. Instead it writes a
//! second, managed `hl.config` call holding only the options the user changed
//! here. Hyprland merges repeated `hl.config` calls key by key, so everything
//! not overridden keeps coming from the user's config, and an override is
//! undone by simply leaving it out of the block.
//!
//! Current values are read from the running compositor rather than parsed out
//! of the file — the same approach the shader preview takes — and changes can
//! be pushed to it live with `hyprctl eval`, which is cheap enough (a few ms)
//! to follow a slider as it is dragged.

use std::collections::BTreeMap;
use std::process::Command;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::block::Markers;
use crate::error::{Error, Result};
use crate::model::lua_quote;

/// The look block's markers.
pub const MARKERS: Markers = Markers {
    begin: "-- >>> HyprCustard look (managed by hyprcustard) >>>",
    end: "-- <<< HyprCustard look (managed by hyprcustard) <<<",
    name: "HyprCustard look",
};

/// Prefix of the line carrying the overrides as JSON.
pub const STATE_PREFIX: &str = "-- custard-look-v1 ";

/// What sort of value an option holds, which decides both its control and
/// how it is written as Lua.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// `true` / `false`.
    Bool,
    /// A whole number.
    Int,
    /// A real number.
    Float,
    /// One color.
    Color,
    /// One or more colors and an angle, as a border takes.
    Gradient,
    /// Four whole numbers: top, right, bottom, left.
    Sides,
    /// An x, y pair.
    Vec2,
    /// A line of text. Only rules use it, for an animation style.
    Text,
}

/// The sections of the Look page, in the order it shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    /// Gaps and borders.
    Borders,
    /// Rounding, opacity and dimming.
    Windows,
    /// Drop shadow.
    Shadow,
    /// Background blur.
    Blur,
    /// A colored glow around windows.
    Glow,
    /// The desktop behind everything, the splash, and the cursor.
    Desktop,
    /// Animations, as a whole.
    Motion,
}

impl Group {
    /// Every group, in page order.
    pub const ALL: [Group; 7] = [
        Group::Borders,
        Group::Windows,
        Group::Shadow,
        Group::Glow,
        Group::Blur,
        Group::Desktop,
        Group::Motion,
    ];

    /// The heading the page shows.
    pub fn label(self) -> &'static str {
        match self {
            Group::Borders => "Gaps and borders",
            Group::Windows => "Windows",
            Group::Shadow => "Shadow",
            Group::Blur => "Blur",
            Group::Glow => "Glow",
            Group::Desktop => "Desktop and cursor",
            Group::Motion => "Animation",
        }
    }
}

/// One option the Look page can set.
#[derive(Debug, Clone, Copy)]
pub struct Opt {
    /// The dotted path `hl.config` and `hl.get_config` take.
    pub key: &'static str,
    /// The section it appears in.
    pub group: Group,
    /// What the row is called.
    pub label: &'static str,
    /// A line under the row, or empty.
    pub hint: &'static str,
    /// What kind of value it holds.
    pub kind: Kind,
    /// Slider range, for numbers and sides.
    pub min: f64,
    /// Slider range, for numbers and sides.
    pub max: f64,
    /// Slider step, for numbers.
    pub step: f64,
    /// Hyprland's own default, as JSON in the form [`parse`] takes. Shown when
    /// Hyprland is not running to ask.
    pub default: &'static str,
    /// For an option that, left unset, takes another's value — the unfocused
    /// shadow color is the focused one unless set — that other option's key.
    /// Empty otherwise.
    pub follows: &'static str,
}

const fn opt(
    key: &'static str,
    group: Group,
    label: &'static str,
    kind: Kind,
    (min, max, step): (f64, f64, f64),
    default: &'static str,
    hint: &'static str,
) -> Opt {
    Opt { key, group, label, hint, kind, min, max, step, default, follows: "" }
}

/// `o`, taking `key`'s value while it is not set itself.
const fn following(mut o: Opt, key: &'static str) -> Opt {
    o.follows = key;
    o
}

const NONE: (f64, f64, f64) = (0.0, 0.0, 0.0);
const UNIT: (f64, f64, f64) = (0.0, 1.0, 0.01);

/// Every option the Look page offers, in the order it shows them.
pub static OPTIONS: &[Opt] = &[
    // --- gaps and borders ---
    opt(
        "general.gaps_in",
        Group::Borders,
        "Gaps between windows",
        Kind::Sides,
        (0.0, 50.0, 1.0),
        "[5,5,5,5]",
        "Half of the space between two windows, as each window keeps its own.",
    ),
    opt(
        "general.gaps_out",
        Group::Borders,
        "Gaps at screen edges",
        Kind::Sides,
        (0.0, 100.0, 1.0),
        "[20,20,20,20]",
        "",
    ),
    opt(
        "general.border_size",
        Group::Borders,
        "Border width",
        Kind::Int,
        (0.0, 20.0, 1.0),
        "1",
        "",
    ),
    opt(
        "general.col.active_border",
        Group::Borders,
        "Focused border",
        Kind::Gradient,
        NONE,
        r##"{"colors":["#ffffffff"],"angle":0}"##,
        "Up to ten colors, blended along the angle.",
    ),
    opt(
        "general.col.inactive_border",
        Group::Borders,
        "Other borders",
        Kind::Gradient,
        NONE,
        r##"{"colors":["#ff444444"],"angle":0}"##,
        "",
    ),
    // --- windows ---
    opt(
        "decoration.rounding",
        Group::Windows,
        "Corner rounding",
        Kind::Int,
        (0.0, 40.0, 1.0),
        "0",
        "",
    ),
    opt(
        "decoration.rounding_power",
        Group::Windows,
        "Corner shape",
        Kind::Float,
        (1.0, 10.0, 0.1),
        "2",
        "2 is a circle; higher values square the corner off into a squircle.",
    ),
    opt("decoration.active_opacity", Group::Windows, "Focused opacity", Kind::Float, UNIT, "1", ""),
    opt(
        "decoration.inactive_opacity",
        Group::Windows,
        "Unfocused opacity",
        Kind::Float,
        UNIT,
        "1",
        "",
    ),
    opt(
        "decoration.fullscreen_opacity",
        Group::Windows,
        "Fullscreen opacity",
        Kind::Float,
        UNIT,
        "1",
        "",
    ),
    opt(
        "decoration.dim_inactive",
        Group::Windows,
        "Dim unfocused windows",
        Kind::Bool,
        NONE,
        "false",
        "",
    ),
    opt("decoration.dim_strength", Group::Windows, "Dim strength", Kind::Float, UNIT, "0.5", ""),
    // --- shadow ---
    opt("decoration.shadow.enabled", Group::Shadow, "Shadows", Kind::Bool, NONE, "true", ""),
    opt("decoration.shadow.range", Group::Shadow, "Size", Kind::Int, (0.0, 100.0, 1.0), "4", ""),
    opt(
        "decoration.shadow.render_power",
        Group::Shadow,
        "Falloff",
        Kind::Int,
        (1.0, 4.0, 1.0),
        "3",
        "Higher is a harder edge.",
    ),
    opt(
        "decoration.shadow.color",
        Group::Shadow,
        "Color",
        Kind::Color,
        NONE,
        r##""#ee1a1a1a""##,
        "",
    ),
    opt(
        "decoration.shadow.offset",
        Group::Shadow,
        "Offset",
        Kind::Vec2,
        (-50.0, 50.0, 1.0),
        "[0,0]",
        "",
    ),
    following(
        opt(
            "decoration.shadow.color_inactive",
            Group::Shadow,
            "Unfocused color",
            Kind::Color,
            NONE,
            r##""#ee1a1a1a""##,
            "",
        ),
        "decoration.shadow.color",
    ),
    opt(
        "decoration.shadow.scale",
        Group::Shadow,
        "Scale",
        Kind::Float,
        UNIT,
        "1",
        "Below 1 the shadow is smaller than the window, so it shows only past the offset.",
    ),
    opt(
        "decoration.shadow.sharp",
        Group::Shadow,
        "Sharp edge",
        Kind::Bool,
        NONE,
        "false",
        "A hard-edged shadow with no falloff, like a solid offset copy of the window.",
    ),
    // --- glow ---
    opt(
        "decoration.glow.enabled",
        Group::Glow,
        "Glow",
        Kind::Bool,
        NONE,
        "false",
        "A soft colored light around windows, drawn like a shadow.",
    ),
    opt("decoration.glow.range", Group::Glow, "Size", Kind::Int, (0.0, 100.0, 1.0), "10", ""),
    opt(
        "decoration.glow.render_power",
        Group::Glow,
        "Falloff",
        Kind::Int,
        (1.0, 4.0, 1.0),
        "3",
        "Higher is a harder edge.",
    ),
    opt("decoration.glow.color", Group::Glow, "Color", Kind::Color, NONE, r##""#ee33ccff""##, ""),
    following(
        opt(
            "decoration.glow.color_inactive",
            Group::Glow,
            "Unfocused color",
            Kind::Color,
            NONE,
            r##""#ee33ccff""##,
            "",
        ),
        "decoration.glow.color",
    ),
    // --- blur ---
    opt(
        "decoration.blur.enabled",
        Group::Blur,
        "Blur",
        Kind::Bool,
        NONE,
        "true",
        "Behind transparent windows and any layer a rule asks it for.",
    ),
    opt("decoration.blur.size", Group::Blur, "Radius", Kind::Int, (1.0, 20.0, 1.0), "8", ""),
    opt(
        "decoration.blur.passes",
        Group::Blur,
        "Passes",
        Kind::Int,
        (1.0, 8.0, 1.0),
        "1",
        "Each pass blurs further and costs GPU time.",
    ),
    opt(
        "decoration.blur.noise",
        Group::Blur,
        "Noise",
        Kind::Float,
        (0.0, 1.0, 0.005),
        "0.0117",
        "",
    ),
    opt(
        "decoration.blur.contrast",
        Group::Blur,
        "Contrast",
        Kind::Float,
        (0.0, 2.0, 0.01),
        "0.8916",
        "",
    ),
    opt(
        "decoration.blur.brightness",
        Group::Blur,
        "Brightness",
        Kind::Float,
        (0.0, 2.0, 0.01),
        "0.8172",
        "",
    ),
    opt(
        "decoration.blur.vibrancy",
        Group::Blur,
        "Vibrancy",
        Kind::Float,
        UNIT,
        "0.1696",
        "Saturates the brightest colors behind the blur.",
    ),
    opt(
        "decoration.blur.vibrancy_darkness",
        Group::Blur,
        "Vibrancy in darks",
        Kind::Float,
        UNIT,
        "0",
        "",
    ),
    opt(
        "decoration.blur.xray",
        Group::Blur,
        "X-ray",
        Kind::Bool,
        NONE,
        "false",
        "Floating windows blur only the wallpaper, not the windows under them.",
    ),
    opt(
        "decoration.blur.ignore_opacity",
        Group::Blur,
        "Ignore opacity",
        Kind::Bool,
        NONE,
        "true",
        "Blur at full strength however transparent the window is.",
    ),
    opt("decoration.blur.popups", Group::Blur, "Blur popups", Kind::Bool, NONE, "false", ""),
    opt(
        "decoration.blur.special",
        Group::Blur,
        "Blur behind special workspaces",
        Kind::Bool,
        NONE,
        "false",
        "",
    ),
    opt(
        "decoration.blur.input_methods",
        Group::Blur,
        "Blur input method popups",
        Kind::Bool,
        NONE,
        "false",
        "The candidate windows of an input method, such as fcitx5's.",
    ),
    opt(
        "decoration.blur.new_optimizations",
        Group::Blur,
        "Optimized blur",
        Kind::Bool,
        NONE,
        "true",
        "Cheaper blur that skips redrawing what has not changed. Leave on unless it misbehaves.",
    ),
    // --- desktop and cursor ---
    opt(
        "general.gaps_workspaces",
        Group::Desktop,
        "Gap between workspaces",
        Kind::Int,
        (0.0, 200.0, 1.0),
        "0",
        "Space between workspaces while one slides in for another.",
    ),
    opt(
        "misc.background_color",
        Group::Desktop,
        "Background color",
        Kind::Color,
        NONE,
        r##""#ff111111""##,
        "Behind the wallpaper — what shows when there is none.",
    ),
    opt(
        "misc.disable_hyprland_logo",
        Group::Desktop,
        "Hide the Hyprland logo",
        Kind::Bool,
        NONE,
        "false",
        "The default wallpaper's logo, when no wallpaper tool is running.",
    ),
    opt(
        "misc.disable_splash_rendering",
        Group::Desktop,
        "Hide the splash text",
        Kind::Bool,
        NONE,
        "false",
        "",
    ),
    opt(
        "misc.col.splash",
        Group::Desktop,
        "Splash text color",
        Kind::Color,
        NONE,
        r##""#55ffffff""##,
        "",
    ),
    opt(
        "cursor.inactive_timeout",
        Group::Desktop,
        "Hide an idle cursor after",
        Kind::Float,
        (0.0, 30.0, 0.5),
        "0",
        "Seconds without moving; 0 never hides it.",
    ),
    opt(
        "cursor.hide_on_key_press",
        Group::Desktop,
        "Hide the cursor while typing",
        Kind::Bool,
        NONE,
        "false",
        "",
    ),
    // --- animation ---
    opt(
        "animations.enabled",
        Group::Motion,
        "Animations",
        Kind::Bool,
        NONE,
        "true",
        "Every animation at once. Curves and speeds come later.",
    ),
];

/// The option with this key, if the page offers it.
pub fn find(key: &str) -> Option<&'static Opt> {
    OPTIONS.iter().find(|o| o.key == key)
}

/// The option with this key, or an error naming it.
pub fn require(key: &str) -> Result<&'static Opt> {
    find(key).ok_or_else(|| Error::other(format!("{key} is not a look option")))
}

/// One option's value.
///
/// Colors are held as `#aarrggbb`, which is what QML reads directly; they are
/// turned into Hyprland's `rgba(rrggbbaa)` only on the way out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LookValue {
    /// A switch.
    Bool(bool),
    /// A whole number.
    Int(i64),
    /// A real number.
    Float(f64),
    /// One color.
    Color(String),
    /// Colors blended along an angle in degrees.
    Gradient {
        /// One to ten colors.
        colors: Vec<String>,
        /// Degrees.
        angle: f64,
    },
    /// Top, right, bottom, left.
    Sides([i64; 4]),
    /// x, y.
    Vec2([f64; 2]),
    /// A line of text.
    Text(String),
}

impl LookValue {
    /// The value as the UI handles it: a bare JSON value whose shape follows
    /// the option's [`Kind`].
    pub fn to_json(&self) -> Value {
        match self {
            LookValue::Bool(b) => json!(b),
            LookValue::Int(i) => json!(i),
            LookValue::Float(f) => json!(f),
            LookValue::Color(c) => json!(c),
            LookValue::Gradient { colors, angle } => json!({ "colors": colors, "angle": angle }),
            LookValue::Sides(s) => json!(s),
            LookValue::Vec2(v) => json!(v),
            LookValue::Text(t) => json!(t),
        }
    }

    /// The value as Lua, ready to sit on the right of `key =`.
    pub fn to_lua(&self) -> String {
        match self {
            LookValue::Bool(b) => b.to_string(),
            LookValue::Int(i) => i.to_string(),
            LookValue::Float(f) => lua_number(*f),
            LookValue::Color(c) => lua_quote(&hypr_color(c)),
            LookValue::Gradient { colors, angle } => {
                let list: Vec<String> = colors.iter().map(|c| lua_quote(&hypr_color(c))).collect();
                format!("{{ colors = {{ {} }}, angle = {} }}", list.join(", "), lua_number(*angle))
            }
            LookValue::Sides([t, r, b, l]) => {
                format!("{{ top = {t}, right = {r}, bottom = {b}, left = {l} }}")
            }
            LookValue::Vec2([x, y]) => format!("{{ {}, {} }}", lua_number(*x), lua_number(*y)),
            LookValue::Text(t) => lua_quote(t),
        }
    }

    /// Whether two values are the same as far as anyone could see.
    ///
    /// The compositor stores floats as `f32` and angles via radians, so what
    /// it reports back is never bit-for-bit what was set.
    pub fn same_as(&self, other: &LookValue) -> bool {
        let close = |a: f64, b: f64| (a - b).abs() < 1e-3;
        match (self, other) {
            (LookValue::Float(a), LookValue::Float(b)) => close(*a, *b),
            (
                LookValue::Gradient { colors: a, angle: x },
                LookValue::Gradient { colors: b, angle: y },
            ) => a == b && close(*x, *y),
            (LookValue::Vec2([a, b]), LookValue::Vec2([c, d])) => close(*a, *c) && close(*b, *d),
            _ => self == other,
        }
    }
}

/// A float as Lua, without the noise of `0.30000001`.
pub(crate) fn lua_number(v: f64) -> String {
    let rounded = (v * 10_000.0).round() / 10_000.0;
    if rounded == rounded.trunc() && rounded.abs() < 1e15 {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded}")
    }
}

/// `#aarrggbb` as Hyprland's `rgba(rrggbbaa)`.
pub(crate) fn hypr_color(c: &str) -> String {
    let hex = c.trim_start_matches('#');
    if hex.len() == 8 {
        format!("rgba({}{})", &hex[2..], &hex[..2])
    } else {
        format!("rgba({hex}ff)")
    }
}

/// Read a color in any of the spellings a person or a toolkit might use, and
/// return it as `#aarrggbb`.
///
/// * `#rgb`, `#rrggbb`, `#aarrggbb` — Qt's order, alpha first.
/// * `rgba(rrggbbaa)`, `rgb(rrggbb)`, and bare `rrggbbaa` / `rrggbb` —
///   Hyprland's order, alpha last. A bare value is what the page's text field
///   shows, so it reads it back the same way.
/// * `0xaarrggbb` — how `hl.get_config` reports one.
pub fn parse_color(text: &str) -> Result<String> {
    let s = text.trim().to_ascii_lowercase();
    let bad = || Error::other(format!("{text:?} is not a color"));
    let is_hex = |h: &str| !h.is_empty() && h.chars().all(|c| c.is_ascii_hexdigit());

    let (argb, rgba): (Option<String>, Option<String>) = if let Some(h) = s.strip_prefix('#') {
        match h.len() {
            3 => (Some(format!("ff{}", h.chars().flat_map(|c| [c, c]).collect::<String>())), None),
            6 => (Some(format!("ff{h}")), None),
            8 => (Some(h.to_string()), None),
            _ => return Err(bad()),
        }
    } else if let Some(h) = s.strip_prefix("0x") {
        match h.len() {
            8 => (Some(h.to_string()), None),
            6 => (Some(format!("ff{h}")), None),
            _ => return Err(bad()),
        }
    } else {
        let inner = s
            .strip_prefix("rgba(")
            .or_else(|| s.strip_prefix("rgb("))
            .and_then(|r| r.strip_suffix(')'))
            .unwrap_or(&s);
        match inner.len() {
            8 => (None, Some(inner.to_string())),
            6 => (None, Some(format!("{inner}ff"))),
            _ => return Err(bad()),
        }
    };

    let argb = match (argb, rgba) {
        (Some(a), _) => a,
        (None, Some(r)) => format!("{}{}", &r[6..], &r[..6]),
        (None, None) => return Err(bad()),
    };
    if !is_hex(&argb) {
        return Err(bad());
    }
    Ok(format!("#{argb}"))
}

/// Read a value from the UI (or from an option's default) for `opt`,
/// checking it fits and clamping numbers into the option's range.
pub fn parse(opt: &Opt, value: &Value) -> Result<LookValue> {
    parse_as(opt.key, opt.kind, (opt.min, opt.max), value)
}

/// [`parse`] for anything with a key, a kind and a range — a rule's fields
/// as well as the page's options.
pub fn parse_as(key: &str, kind: Kind, (min, max): (f64, f64), value: &Value) -> Result<LookValue> {
    let wrong = || Error::other(format!("{key} cannot be set to {value}"));
    let clamp = |v: f64| if max > min { v.clamp(min, max) } else { v };
    let int = |v: &Value| v.as_f64().map(|f| clamp(f.round()) as i64).ok_or_else(wrong);

    Ok(match kind {
        Kind::Bool => LookValue::Bool(value.as_bool().ok_or_else(wrong)?),
        Kind::Int => LookValue::Int(int(value)?),
        Kind::Float => LookValue::Float(clamp(value.as_f64().ok_or_else(wrong)?)),
        Kind::Color => LookValue::Color(parse_color(value.as_str().ok_or_else(wrong)?)?),
        Kind::Gradient => {
            let colors = value
                .get("colors")
                .and_then(Value::as_array)
                .ok_or_else(wrong)?
                .iter()
                .map(|c| c.as_str().ok_or_else(wrong).and_then(parse_color))
                .collect::<Result<Vec<_>>>()?;
            if colors.is_empty() || colors.len() > 10 {
                return Err(Error::other("a border takes between one and ten colors"));
            }
            let angle = value.get("angle").and_then(Value::as_f64).unwrap_or(0.0).rem_euclid(360.0);
            LookValue::Gradient { colors, angle }
        }
        Kind::Sides => {
            let v: Vec<i64> = match value {
                Value::Number(_) => vec![int(value)?],
                Value::Array(a) => a.iter().map(int).collect::<Result<_>>()?,
                _ => return Err(wrong()),
            };
            LookValue::Sides(match v[..] {
                [a] => [a; 4],
                [tb, rl] => [tb, rl, tb, rl],
                [t, rl, b] => [t, rl, b, rl],
                [t, r, b, l] => [t, r, b, l],
                _ => return Err(wrong()),
            })
        }
        Kind::Vec2 => {
            let a = value.as_array().filter(|a| a.len() == 2).ok_or_else(wrong)?;
            let x = a[0].as_f64().ok_or_else(wrong)?;
            let y = a[1].as_f64().ok_or_else(wrong)?;
            LookValue::Vec2([clamp(x), clamp(y)])
        }
        Kind::Text => {
            let t = value.as_str().map(str::trim).ok_or_else(wrong)?;
            if t.is_empty() || t.contains(['\n', '\r']) {
                return Err(wrong());
            }
            LookValue::Text(t.to_string())
        }
    })
}

/// An option's default, as a value.
pub fn default_of(opt: &Opt) -> LookValue {
    let json: Value = serde_json::from_str(opt.default).expect("look defaults are valid JSON");
    parse(opt, &json).expect("look defaults fit their own options")
}

// ---------------------------------------------------------------------------
// Overrides, and the block they are written into
// ---------------------------------------------------------------------------

/// The options the user has set here, by key. Anything absent comes from
/// their own config.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Look {
    /// Schema version of this blob.
    #[serde(default = "one")]
    pub version: u32,
    /// The overrides.
    #[serde(default)]
    pub values: BTreeMap<String, LookValue>,
    /// Easing curves defined or redefined here, by name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub curves: BTreeMap<String, crate::motion::Points>,
    /// Animation leaves set here, by name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub animations: BTreeMap<String, crate::motion::AnimSetting>,
    /// Window, layer and workspace rules, in the order they are written.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<crate::rules::Rule>,
}

fn one() -> u32 {
    1
}

impl Default for Look {
    fn default() -> Self {
        Look {
            version: one(),
            values: BTreeMap::new(),
            curves: BTreeMap::new(),
            animations: BTreeMap::new(),
            rules: Vec::new(),
        }
    }
}

impl Look {
    /// True when nothing is overridden, so there is no block to write.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
            && self.curves.is_empty()
            && self.animations.is_empty()
            && self.rules.is_empty()
    }

    /// How many things are set here, for the page to count.
    pub fn len(&self) -> usize {
        self.values.len() + self.curves.len() + self.animations.len() + self.rules.len()
    }

    /// Everything set here as Lua, as it goes into the file: the
    /// `hl.config` call, the curves and animations, then the rules. Empty
    /// when nothing is set.
    pub fn lua(&self) -> String {
        let mut out = self.settings_lua();
        let rules = crate::rules::block_lua(&self.rules);
        if !rules.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&rules);
        }
        out
    }

    /// The options, curves and animations as Lua — everything but the rules,
    /// which go on the desktop differently from how they go into the file.
    pub fn settings_lua(&self) -> String {
        let mut out = String::new();
        if !self.values.is_empty() {
            out.push_str(&config_call(&self.ordered()));
        }
        let motion = crate::motion::lua(&self.curves, &self.animations);
        if !motion.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&motion);
        }
        out
    }

    /// Keys in page order, then any the page no longer offers (left by a
    /// newer or older version) alphabetically, so nothing written is lost.
    fn ordered(&self) -> Vec<(&str, &LookValue)> {
        let mut out: Vec<(&str, &LookValue)> =
            OPTIONS.iter().filter_map(|o| self.values.get(o.key).map(|v| (o.key, v))).collect();
        out.extend(
            self.values.iter().filter(|(k, _)| find(k).is_none()).map(|(k, v)| (k.as_str(), v)),
        );
        out
    }
}

/// One `hl.config({...})` call setting `values`, nested the way Hyprland's
/// Lua config expects: `decoration.blur.size` becomes
/// `decoration = { blur = { size = ... } }`.
pub fn config_call(values: &[(&str, &LookValue)]) -> String {
    enum Node {
        Leaf(String),
        Branch(Vec<(String, Node)>),
    }
    fn insert(into: &mut Vec<(String, Node)>, path: &[&str], lua: String) {
        let (head, rest) = (path[0], &path[1..]);
        if rest.is_empty() {
            into.push((head.to_string(), Node::Leaf(lua)));
            return;
        }
        let at = match into.iter().position(|(k, n)| k == head && matches!(n, Node::Branch(_))) {
            Some(i) => i,
            None => {
                into.push((head.to_string(), Node::Branch(Vec::new())));
                into.len() - 1
            }
        };
        if let Node::Branch(children) = &mut into[at].1 {
            insert(children, rest, lua);
        }
    }
    fn render(out: &mut String, nodes: &[(String, Node)], depth: usize) {
        let pad = "    ".repeat(depth);
        for (key, node) in nodes {
            match node {
                Node::Leaf(lua) => out.push_str(&format!("{pad}{key} = {lua},\n")),
                Node::Branch(children) => {
                    out.push_str(&format!("{pad}{key} = {{\n"));
                    render(out, children, depth + 1);
                    out.push_str(&format!("{pad}}},\n"));
                }
            }
        }
    }

    let mut root = Vec::new();
    for (key, value) in values {
        let path: Vec<&str> = key.split('.').collect();
        insert(&mut root, &path, value.to_lua());
    }
    let mut out = String::from("hl.config({\n");
    render(&mut out, &root, 1);
    out.push_str("})\n");
    out
}

/// The body of the look block: a header, the state line, and the call.
pub fn emit(look: &Look) -> Result<String> {
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S %:z");
    let mut out = format!(
        "-- Generated by hyprcustard {} on {stamp}.\n\
         -- Only what was changed in the app is here; everything else still comes\n\
         -- from your own hl.config above. This block has to stay below it to win.\n\
         -- Edits inside this block are replaced the next time the app saves.\n",
        env!("CARGO_PKG_VERSION")
    );
    out.push_str(STATE_PREFIX);
    out.push_str(&serde_json::to_string(look)?);
    out.push_str("\n\n");
    out.push_str(&look.lua());
    Ok(out)
}

/// Read the overrides back out of a look block body.
pub fn extract(inner: &str) -> Result<Look> {
    for line in inner.lines() {
        if let Some(json) = line.trim().strip_prefix(STATE_PREFIX) {
            return Ok(serde_json::from_str(json.trim())?);
        }
    }
    Ok(Look::default())
}

/// The overrides in a whole `hyprland.lua`, or none when it has no look block.
pub fn read(document: &str) -> Result<Look> {
    match crate::block::split_with(document, &MARKERS)? {
        Some(s) => extract(&s.inner),
        None => Ok(Look::default()),
    }
}

// ---------------------------------------------------------------------------
// The running compositor
// ---------------------------------------------------------------------------

/// Run a Lua string in the compositor with `hyprctl eval` / `repl`.
///
/// Errors come back on stdout with exit status 7, so stdout is the message.
fn hyprctl_lua(verb: &str, code: &str) -> Result<String> {
    let out = Command::new("hyprctl").args([verb, code]).output().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => {
            Error::Hyprctl("hyprctl is not on PATH — is Hyprland installed?".into())
        }
        _ => Error::Hyprctl(e.to_string()),
    })?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() && !stdout.starts_with("error:") {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    let msg = if !stdout.is_empty() { stdout } else { stderr };
    Err(Error::Hyprctl(clean_lua_error(&msg)))
}

/// Strip the echo of the whole chunk out of a Lua error, which otherwise
/// repeats the entire `hl.config({...})` back at the user.
fn clean_lua_error(msg: &str) -> String {
    let msg = msg.trim().trim_start_matches("error:").trim();
    match msg.find(":1: ") {
        Some(i) => msg[i + 4..].to_string(),
        None => msg.to_string(),
    }
}

/// Set options on the running compositor without touching any file.
pub fn apply_live(values: &[(&str, &LookValue)]) -> Result<()> {
    if values.is_empty() {
        return Ok(());
    }
    hyprctl_lua("eval", config_call(values).trim_end()).map(|_| ())
}

/// Every override at once — what the desktop should look like according to
/// the app, on top of whatever the files say.
///
/// `theirs` is the user's own workspace rules sharing a workspace with one
/// of the app's, run again so switching the app's off does not take theirs
/// with it (see [`crate::rules`]).
pub fn apply_all_live(look: &Look, theirs: &[String]) -> Result<()> {
    eval(look.settings_lua().trim_end())?;
    eval(&crate::rules::live_all(&look.rules, theirs))
}

/// Run Lua in the compositor. Empty code is a no-op.
pub fn eval(code: &str) -> Result<()> {
    if code.trim().is_empty() {
        return Ok(());
    }
    hyprctl_lua("eval", code).map(|_| ())
}

/// Read every option the page offers from the running compositor.
///
/// One `hyprctl repl` call, answering a line per option. An option the
/// compositor does not know (an older Hyprland) is simply missing from the
/// result.
pub fn read_live() -> Result<BTreeMap<String, LookValue>> {
    let keys: Vec<String> = OPTIONS.iter().map(|o| lua_quote(o.key)).collect();
    let code = format!(
        "local function n(x) return string.format('%.6g', x) end \
         local function hex(s) return string.format('%08x', tonumber(s)) end \
         local out = {{}} \
         for _, k in ipairs({{ {keys} }}) do \
           local v = hl.get_config(k) local t = type(v) local s = nil \
           if t == 'boolean' then s = 'b ' .. tostring(v) \
           elseif t == 'number' then \
             if math.type(v) == 'integer' then s = 'i ' .. v else s = 'f ' .. n(v) end \
           elseif t == 'table' then \
             if v.colors then \
               local c = {{}} for i, x in ipairs(v.colors) do c[i] = hex(x) end \
               s = 'g ' .. n(v.angle or 0) .. ' ' .. table.concat(c, ' ') \
             elseif v.top then s = 's ' .. v.top .. ' ' .. v.right .. ' ' .. v.bottom .. ' ' .. v.left \
             elseif v.x then s = 'v ' .. n(v.x) .. ' ' .. n(v.y) end \
           elseif t == 'string' and tonumber(v) then s = 'g 0 ' .. hex(v) \
           end \
           if s then out[#out + 1] = k .. ' ' .. s end \
         end \
         return table.concat(out, '\\n')",
        keys = keys.join(", ")
    );
    let mut live = parse_live(&hyprctl_lua("repl", &code)?);

    // An unset "follows" option reads back as white, not as the value it is
    // actually using. Only `getoption` says whether it was set.
    for o in OPTIONS.iter().filter(|o| !o.follows.is_empty()) {
        if !crate::hyprctl::option_is_set(&o.key.replace('.', ":")).unwrap_or(true) {
            live.remove(o.key);
        }
    }
    Ok(live)
}

/// Turn [`read_live`]'s answer into values, coercing each to its option's
/// kind. Lines that do not fit are skipped rather than failing the lot.
fn parse_live(text: &str) -> BTreeMap<String, LookValue> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let mut bits = line.split_whitespace();
        let (Some(key), Some(tag)) = (bits.next(), bits.next()) else { continue };
        let Some(opt) = find(key) else { continue };
        let rest: Vec<&str> = bits.collect();
        let num = |i: usize| rest.get(i).and_then(|s| s.parse::<f64>().ok());

        let value = match (tag, opt.kind) {
            ("b", Kind::Bool) => rest.first().map(|s| LookValue::Bool(*s == "true")),
            ("i" | "f", Kind::Bool) => num(0).map(|v| LookValue::Bool(v != 0.0)),
            ("i" | "f", Kind::Int) => num(0).map(|v| LookValue::Int(v.round() as i64)),
            ("i" | "f", Kind::Float) => num(0).map(LookValue::Float),
            ("i" | "f", Kind::Sides) => num(0).map(|v| LookValue::Sides([v as i64; 4])),
            ("g", Kind::Gradient) => {
                let colors: Option<Vec<String>> =
                    rest[1..].iter().map(|c| parse_color(&format!("0x{c}")).ok()).collect();
                match (num(0), colors) {
                    (Some(angle), Some(colors)) if !colors.is_empty() => {
                        Some(LookValue::Gradient { colors, angle })
                    }
                    _ => None,
                }
            }
            ("g", Kind::Color) => {
                rest.get(1).and_then(|c| parse_color(&format!("0x{c}")).ok()).map(LookValue::Color)
            }
            ("s", Kind::Sides) => match (num(0), num(1), num(2), num(3)) {
                (Some(t), Some(r), Some(b), Some(l)) => {
                    Some(LookValue::Sides([t as i64, r as i64, b as i64, l as i64]))
                }
                _ => None,
            },
            ("v", Kind::Vec2) => match (num(0), num(1)) {
                (Some(x), Some(y)) => Some(LookValue::Vec2([x, y])),
                _ => None,
            },
            _ => None,
        };
        if let Some(v) = value {
            out.insert(key.to_string(), v);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The view
// ---------------------------------------------------------------------------

/// The Look page's state: every option with the value to show, and where
/// that value comes from.
///
/// `live` is what the compositor reported, `saved` what the file's look block
/// holds. The shown value is the unsaved override if there is one, then the
/// compositor's, then Hyprland's default.
pub fn view(look: &Look, saved: &Look, live: &BTreeMap<String, LookValue>, running: bool) -> Value {
    let options: Vec<Value> = OPTIONS
        .iter()
        .map(|o| {
            let own = look.values.get(o.key);
            let reported = live.get(o.key);
            // Unset, and so showing whatever it follows.
            let following = !o.follows.is_empty() && own.is_none() && reported.is_none();
            let shown = |key: &str| look.values.get(key).or_else(|| live.get(key)).cloned();
            let value = if following {
                shown(o.follows).or_else(|| find(o.follows).map(default_of))
            } else {
                own.or(reported).cloned()
            }
            .unwrap_or_else(|| default_of(o));
            json!({
                "key": o.key,
                "group": o.group,
                "label": o.label,
                "hint": o.hint,
                "kind": o.kind,
                "min": o.min,
                "max": o.max,
                "step": o.step,
                "value": value.to_json(),
                // Set here, whether saved yet or not.
                "overridden": own.is_some(),
                // Differs from what the file says.
                "changed": own != saved.values.get(o.key),
                // Hyprland did not answer for it: running an older version,
                // or not running at all.
                "unknown": reported.is_none() && !following,
                // Not set anywhere, so it takes another option's value.
                "following": following,
                "follows": o.follows,
            })
        })
        .collect();

    json!({
        "running": running,
        "dirty": look != saved,
        "count": look.values.len(),
        "groups": Group::ALL.iter().map(|g| json!({ "id": g, "label": g.label() })).collect::<Vec<_>>(),
        "options": options,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_parses_for_its_own_option() {
        for o in OPTIONS {
            let _ = default_of(o);
        }
    }

    #[test]
    fn keys_are_unique() {
        let mut keys: Vec<&str> = OPTIONS.iter().map(|o| o.key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), OPTIONS.len());
    }

    #[test]
    fn colors_read_in_every_spelling() {
        assert_eq!(parse_color("#abc").unwrap(), "#ffaabbcc");
        assert_eq!(parse_color("#33ccff").unwrap(), "#ff33ccff");
        assert_eq!(parse_color("#ee33ccff").unwrap(), "#ee33ccff");
        assert_eq!(parse_color("rgba(33ccffee)").unwrap(), "#ee33ccff");
        assert_eq!(parse_color("rgb(33ccff)").unwrap(), "#ff33ccff");
        assert_eq!(parse_color("33CCFFEE").unwrap(), "#ee33ccff");
        assert_eq!(parse_color("0xFFEBDBB2").unwrap(), "#ffebdbb2");
        assert!(parse_color("#12345").is_err());
        assert!(parse_color("rgba(zzzzzzzz)").is_err());
        assert!(parse_color("").is_err());
    }

    #[test]
    fn colors_go_out_in_hyprlands_order() {
        assert_eq!(hypr_color("#ee33ccff"), "rgba(33ccffee)");
    }

    #[test]
    fn numbers_are_clamped_to_the_option() {
        let o = find("decoration.rounding").unwrap();
        assert_eq!(parse(o, &json!(999)).unwrap(), LookValue::Int(40));
        assert_eq!(parse(o, &json!(3.6)).unwrap(), LookValue::Int(4));
        let o = find("decoration.active_opacity").unwrap();
        assert_eq!(parse(o, &json!(-1)).unwrap(), LookValue::Float(0.0));
    }

    #[test]
    fn sides_take_css_shorthand() {
        let o = find("general.gaps_out").unwrap();
        assert_eq!(parse(o, &json!(4)).unwrap(), LookValue::Sides([4; 4]));
        assert_eq!(parse(o, &json!([1, 4])).unwrap(), LookValue::Sides([1, 4, 1, 4]));
        assert_eq!(parse(o, &json!([1, 4, 4, 4])).unwrap(), LookValue::Sides([1, 4, 4, 4]));
        assert!(parse(o, &json!([1, 2, 3, 4, 5])).is_err());
    }

    #[test]
    fn a_gradient_needs_one_to_ten_colors() {
        let o = find("general.col.active_border").unwrap();
        assert!(parse(o, &json!({ "colors": [], "angle": 0 })).is_err());
        let v = parse(o, &json!({ "colors": ["#ff33ccff", "00ff99ee"], "angle": 405 })).unwrap();
        assert_eq!(
            v,
            LookValue::Gradient {
                colors: vec!["#ff33ccff".into(), "#ee00ff99".into()],
                angle: 45.0
            }
        );
    }

    #[test]
    fn the_wrong_shape_is_refused() {
        let o = find("decoration.blur.enabled").unwrap();
        assert!(parse(o, &json!(1)).is_err());
        assert!(require("decoration.nope").is_err());
    }

    #[test]
    fn the_call_nests_dotted_keys() {
        let mut look = Look::default();
        look.values.insert("decoration.blur.size".into(), LookValue::Int(6));
        look.values.insert("decoration.rounding".into(), LookValue::Int(8));
        look.values.insert(
            "general.col.active_border".into(),
            LookValue::Gradient {
                colors: vec!["#ee33ccff".into(), "#ee00ff99".into()],
                angle: 45.0,
            },
        );
        look.values.insert("decoration.blur.noise".into(), LookValue::Float(0.30000001));
        look.values.insert("general.gaps_out".into(), LookValue::Sides([1, 4, 4, 4]));
        assert_eq!(
            config_call(&look.ordered()),
            "hl.config({\n\
             \x20   general = {\n\
             \x20       gaps_out = { top = 1, right = 4, bottom = 4, left = 4 },\n\
             \x20       col = {\n\
             \x20           active_border = { colors = { \"rgba(33ccffee)\", \"rgba(00ff99ee)\" }, angle = 45 },\n\
             \x20       },\n\
             \x20   },\n\
             \x20   decoration = {\n\
             \x20       rounding = 8,\n\
             \x20       blur = {\n\
             \x20           size = 6,\n\
             \x20           noise = 0.3,\n\
             \x20       },\n\
             \x20   },\n\
             })\n"
        );
    }

    #[test]
    fn the_block_round_trips_through_its_state_line() {
        let mut look = Look::default();
        look.values.insert("decoration.shadow.color".into(), LookValue::Color("#661d1d1d".into()));
        look.values.insert("decoration.shadow.offset".into(), LookValue::Vec2([2.0, -3.5]));
        let body = emit(&look).unwrap();
        assert!(body.contains(r#""version":1"#));
        assert!(body.contains(r#"color = "rgba(1d1d1d66)","#));
        assert!(body.contains("offset = { 2, -3.5 },"));
        let doc = crate::block::splice_with("-- mine\n", &body, &MARKERS).unwrap();
        assert!(doc.starts_with("-- mine\n\n-- >>> HyprCustard look"));
        assert_eq!(read(&doc).unwrap(), look);
    }

    #[test]
    fn the_look_block_and_the_shade_block_do_not_disturb_each_other() {
        let doc = crate::block::splice("-- mine\n", "-- shade body\n").unwrap();
        let doc = crate::block::splice_with(&doc, "-- look body\n", &MARKERS).unwrap();
        let doc = crate::block::splice(&doc, "-- shade body 2\n").unwrap();
        assert!(doc.contains("-- shade body 2\n"));
        assert!(doc.contains("-- look body\n"));
        let doc = crate::block::remove_with(&doc, &MARKERS).unwrap();
        assert!(!doc.contains("look body"));
        assert!(doc.contains("-- shade body 2\n"));
        assert_eq!(read(&doc).unwrap(), Look::default());
    }

    #[test]
    fn live_answers_are_coerced_to_each_kind() {
        let text = "general.gaps_in s 2 2 2 2\n\
                    general.border_size i 0\n\
                    general.col.active_border g 0 ffebdbb2\n\
                    decoration.rounding_power f 3\n\
                    decoration.rounding i 6\n\
                    decoration.dim_inactive b true\n\
                    decoration.shadow.color g 0 661d1d1d\n\
                    decoration.shadow.offset v 0 0\n\
                    decoration.blur.noise f 0.08\n\
                    not.an.option i 3\n\
                    decoration.blur.size g nonsense";
        let live = parse_live(text);
        assert_eq!(live["general.gaps_in"], LookValue::Sides([2; 4]));
        assert_eq!(live["general.border_size"], LookValue::Int(0));
        assert_eq!(
            live["general.col.active_border"],
            LookValue::Gradient { colors: vec!["#ffebdbb2".into()], angle: 0.0 }
        );
        assert_eq!(live["decoration.rounding_power"], LookValue::Float(3.0));
        assert_eq!(live["decoration.dim_inactive"], LookValue::Bool(true));
        assert_eq!(live["decoration.shadow.color"], LookValue::Color("#661d1d1d".into()));
        assert_eq!(live["decoration.shadow.offset"], LookValue::Vec2([0.0, 0.0]));
        assert!(!live.contains_key("not.an.option"));
        assert!(!live.contains_key("decoration.blur.size"));
    }

    #[test]
    fn lua_errors_lose_the_echoed_chunk() {
        let raw = "error: return hl.config({ decoration = { nope = 1 } });:1: unknown config key \
                   'decoration.nope'";
        assert_eq!(clean_lua_error(raw), "unknown config key 'decoration.nope'");
    }

    #[test]
    fn reported_floats_count_as_the_same() {
        assert!(LookValue::Float(0.08).same_as(&LookValue::Float(0.079999998)));
        assert!(!LookValue::Float(0.08).same_as(&LookValue::Float(0.09)));
    }
}
