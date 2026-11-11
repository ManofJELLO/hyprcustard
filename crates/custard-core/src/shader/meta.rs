//! Reading a `.glsl` file for everything the GUI can show or edit.
//!
//! The plugin has no custom-uniform channel: a shader's tunable numbers are
//! `const` declarations in its source. So "editing a variable" means rewriting
//! the literal in the file, which the plugin picks up on the next frame because
//! it reloads on mtime change.
//!
//! Two sources of truth, annotations winning:
//!
//! * `// @param` / `// @color` / `// @bool` comments, which name a range and a
//!   label explicitly.
//! * plain `const` declarations, for which a range is guessed.

use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Plugin uniforms, for detecting which ones a shader declares.
pub const KNOWN_UNIFORMS: &[(&str, &str)] = &[
    ("time", "seconds since plugin start; declaring it forces continuous redraws"),
    ("plugin_alpha", "the window's total alpha (reserved, injected by the wrapper)"),
    ("resolution", "active monitor pixel size"),
    ("surface_size", "the drawn view's own size"),
    ("mouse", "pointer position in compositor coords"),
    ("is_active", "1.0 while focused"),
    ("is_floating", "1.0 while floating"),
    ("is_fullscreen", "1.0 while fullscreen"),
    ("progress", "0 to 1 across an open/close animation"),
    ("seed", "stable per-window random value"),
    ("velocity", "window velocity px/s — always 0 on a layer"),
    ("size_velocity", "resize rate px/s — always 0 on a layer"),
    ("peak_velocity", "fastest velocity this gesture — always 0 on a layer"),
    ("peak_size_velocity", "fastest resize rate this gesture — always 0 on a layer"),
    ("release_velocity", "velocity frozen when motion stopped — always 0 on a layer"),
    ("move_delta", "whole trip vector — always 0 on a layer"),
    ("move_remaining", "distance still to travel — always 0 on a layer"),
    ("size_delta", "whole resize vector — always 0 on a layer"),
    ("window_box", "the window's box in global logical coords"),
    ("window_rect", "where the drawn view sits in the sampled texture"),
    ("is_moving", "1.0 while the position is animating"),
    ("is_resizing", "1.0 while the size is animating"),
    ("is_dragging", "1.0 while physically dragging"),
    ("anim_kind", "0 none, 1 move, 2 resize, 3 workspace, 4 urgent, 5 focus, 6 unfocus"),
    ("curve", "eased progress; can overshoot below 0 or above 1"),
    ("duration", "seconds the transform will take, or -1"),
    ("settle", "0 to 1 across the post-motion tail"),
];

/// Uniforms that read zero on a layer surface, so a shader driven by them is a
/// silent no-op there.
pub const MOTION_UNIFORMS: &[&str] = &[
    "velocity",
    "size_velocity",
    "peak_velocity",
    "peak_size_velocity",
    "release_velocity",
    "move_delta",
    "move_remaining",
    "size_delta",
    "is_moving",
    "is_resizing",
    "is_dragging",
];

/// How a parameter should be presented and edited.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ParamKind {
    /// A single number on a slider.
    Scalar {
        /// Slider minimum.
        min: f32,
        /// Slider maximum.
        max: f32,
        /// Slider step.
        step: f32,
        /// True for `int`, so the UI rounds.
        integer: bool,
    },
    /// A checkbox.
    Bool,
    /// An RGB or RGBA colour.
    Color {
        /// True when the underlying type is `vec4`.
        alpha: bool,
    },
    /// Two to four numbers, each on its own slider.
    Vector {
        /// How many components.
        len: usize,
        /// Slider minimum, shared by all components.
        min: f32,
        /// Slider maximum.
        max: f32,
        /// Slider step.
        step: f32,
    },
}

/// The current value of a parameter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParamValue {
    /// A single number, also used for `bool` as 0.0 / 1.0.
    Scalar(f32),
    /// Two to four components.
    Vector(Vec<f32>),
}

impl ParamValue {
    /// The first component, or the scalar.
    pub fn first(&self) -> f32 {
        match self {
            ParamValue::Scalar(v) => *v,
            ParamValue::Vector(v) => v.first().copied().unwrap_or(0.0),
        }
    }

    /// All components.
    pub fn components(&self) -> Vec<f32> {
        match self {
            ParamValue::Scalar(v) => vec![*v],
            ParamValue::Vector(v) => v.clone(),
        }
    }
}

/// One editable constant in a shader.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Param {
    /// The GLSL identifier.
    pub name: String,
    /// Display label.
    pub label: String,
    /// The GLSL type, e.g. `float`, `vec3`.
    pub glsl_type: String,
    /// How to edit it.
    pub kind: ParamKind,
    /// Its value as the file currently has it.
    pub value: ParamValue,
    /// True when an annotation described it, false when the range was guessed.
    pub annotated: bool,
    /// Zero-based line of the `const` declaration.
    pub line: usize,
}

/// Everything read out of one `.glsl` file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShaderInfo {
    /// Absolute path.
    pub path: String,
    /// Display name, from `// @label` or the file stem.
    pub name: String,
    /// File stem.
    pub stem: String,
    /// `// @desc`, if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// `// @duration`, if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f32>,
    /// Zero-based line of the `// @duration` comment, for in-place edits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_line: Option<usize>,
    /// Whether `// @overlay` is present.
    pub overlay: bool,
    /// Zero-based line of `// @overlay`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlay_line: Option<usize>,
    /// Plugin uniforms the shader declares.
    pub uniforms: Vec<String>,
    /// Editable constants.
    pub params: Vec<Param>,
    /// Modification time, seconds since the epoch.
    pub mtime: u64,
    /// Things worth telling the user about this shader.
    pub notes: Vec<String>,
}

impl ShaderInfo {
    /// True when the shader is written as a one-shot animation.
    pub fn is_animation(&self) -> bool {
        self.uniforms.iter().any(|u| u == "progress")
    }

    /// True when the shader's effect scales with motion, which reads zero on a
    /// layer surface.
    pub fn is_motion_driven(&self) -> bool {
        self.uniforms.iter().any(|u| MOTION_UNIFORMS.contains(&u.as_str()))
    }

    /// The duration the plugin will actually use, absent a rule override.
    pub fn effective_duration(&self) -> f32 {
        self.duration.unwrap_or(0.3)
    }
}

// ---------------------------------------------------------------------------
// Regexes
// ---------------------------------------------------------------------------

fn re_const() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r"(?x)
            ^\s*const\s+
            (?:highp\s+|mediump\s+|lowp\s+)?
            (float|int|bool|vec2|vec3|vec4|ivec2|ivec3|ivec4)\s+
            ([A-Za-z_][A-Za-z0-9_]*)\s*
            =\s*(.+?)\s*;",
        )
        .expect("const regex")
    })
}

fn re_uniform() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r"(?x)
            ^\s*uniform\s+
            (?:highp\s+|mediump\s+|lowp\s+)?
            [A-Za-z_][A-Za-z0-9_]*\s+
            ([A-Za-z_][A-Za-z0-9_]*)\s*;",
        )
        .expect("uniform regex")
    })
}

fn re_directive() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"^\s*//\s*@([A-Za-z_][A-Za-z0-9_]*)\s*(.*?)\s*$").expect("directive regex")
    })
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Read a shader's metadata out of its source text.
pub fn parse(path: &str, source: &str, mtime: u64) -> ShaderInfo {
    let stem = std::path::Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string());

    let lines: Vec<&str> = source.lines().collect();

    let mut info = ShaderInfo {
        path: path.to_string(),
        name: prettify(&stem),
        stem,
        description: None,
        duration: None,
        duration_line: None,
        overlay: false,
        overlay_line: None,
        uniforms: Vec::new(),
        params: Vec::new(),
        mtime,
        notes: Vec::new(),
    };

    // Pending annotations that apply to the next const declaration.
    let mut pending: Option<Annotation> = None;
    // Named annotations, resolved against consts once everything is read.
    let mut named: Vec<(String, Annotation)> = Vec::new();

    for (i, raw) in lines.iter().enumerate() {
        if let Some(caps) = re_directive().captures(raw) {
            let key = caps[1].to_ascii_lowercase();
            let rest = caps[2].trim().to_string();
            match key.as_str() {
                "duration" => {
                    match rest.split_whitespace().next().and_then(|t| t.parse::<f32>().ok()) {
                        Some(v) => {
                            info.duration = Some(v.clamp(0.0, 5.0));
                            info.duration_line = Some(i);
                            if v > 5.0 {
                                info.notes.push(format!(
                                    "// @duration {v} is over the plugin's 5 second cap and will \
                                     be clamped to 5."
                                ));
                            }
                        }
                        None => info
                            .notes
                            .push(format!("line {}: // @duration has no number after it", i + 1)),
                    }
                }
                "overlay" => {
                    info.overlay = true;
                    info.overlay_line = Some(i);
                }
                "desc" | "description" if !rest.is_empty() => {
                    info.description = Some(rest);
                }
                "label" | "title" if !rest.is_empty() => {
                    info.name = rest.trim_matches('"').to_string();
                }
                "param" | "color" | "colour" | "bool" => match Annotation::parse(&key, &rest) {
                    Ok(ann) => match &ann.name {
                        Some(n) => named.push((n.clone(), ann)),
                        None => pending = Some(ann),
                    },
                    Err(e) => info.notes.push(format!("line {}: {e}", i + 1)),
                },
                _ => {}
            }
            continue;
        }

        if let Some(caps) = re_uniform().captures(raw) {
            let name = caps[1].to_string();
            if KNOWN_UNIFORMS.iter().any(|(u, _)| *u == name) && !info.uniforms.contains(&name) {
                info.uniforms.push(name);
            }
            continue;
        }

        if let Some(caps) = re_const().captures(raw) {
            let ty = caps[1].to_string();
            let name = caps[2].to_string();
            let literal = caps[3].to_string();
            let ann = pending.take();
            match build_param(&ty, &name, &literal, i, ann) {
                Ok(p) => info.params.push(p),
                Err(e) => info.notes.push(format!("line {}: {e}", i + 1)),
            }
            continue;
        }

        // A blank line or an ordinary comment does not consume a pending
        // annotation; anything else does, so a stray annotation cannot attach
        // itself to a const far below.
        let t = raw.trim();
        if !t.is_empty() && !t.starts_with("//") {
            pending = None;
        }
    }

    // Apply named annotations to the consts they name.
    for (name, ann) in named {
        match info.params.iter_mut().find(|p| p.name == name) {
            Some(p) => ann.apply(p),
            None => {
                info.notes.push(format!("// @param names {name}, but no const {name} was found"))
            }
        }
    }

    // A shader's mathematical constants are not tuning knobs. `PI` is inferred
    // as a 0–10 slider like any other float, and dragging it quietly breaks the
    // shader's maths — so they are dropped unless an annotation says otherwise,
    // which is the author declaring it really is a parameter.
    let dropped: Vec<String> = info
        .params
        .iter()
        .filter(|p| !p.annotated && is_math_constant(&p.name))
        .map(|p| p.name.clone())
        .collect();
    if !dropped.is_empty() {
        info.params.retain(|p| p.annotated || !is_math_constant(&p.name));
        info.notes.push(format!(
            "{} left alone: a mathematical constant is not a parameter. Annotate it with \
             // @param if you really want a slider.",
            dropped.join(", ")
        ));
    }

    if info.is_animation() && info.duration.is_none() {
        info.notes.push(
            "declares the progress uniform but has no // @duration, so the plugin falls back to \
             0.3s and warns once per edit"
                .into(),
        );
    }

    info
}

/// Names a shader conventionally gives a mathematical constant.
///
/// These are spelled out rather than guessed at from the value, so that what
/// the app leaves alone is predictable: a `const float SPEED = 3.14159;` is
/// still yours to drag.
const MATH_CONSTANTS: &[&str] = &[
    "pi",
    "m_pi",
    "two_pi",
    "twopi",
    "half_pi",
    "halfpi",
    "inv_pi",
    "invpi",
    "tau",
    "m_tau",
    "e",
    "m_e",
    "phi",
    "golden",
    "golden_ratio",
    "eps",
    "epsilon",
    "sqrt2",
    "sqrt3",
    "deg2rad",
    "rad2deg",
    "degrees_to_radians",
    "radians_to_degrees",
];

/// Whether a `const` is named like a mathematical constant.
fn is_math_constant(name: &str) -> bool {
    let lower = name.trim_matches('_').to_ascii_lowercase();
    MATH_CONSTANTS.contains(&lower.as_str())
}

/// Turn `reading_mode` into `Reading mode`.
fn prettify(stem: &str) -> String {
    let spaced = stem.replace(['_', '-'], " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => spaced,
    }
}

// ---------------------------------------------------------------------------
// Annotations
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct Annotation {
    name: Option<String>,
    min: Option<f32>,
    max: Option<f32>,
    step: Option<f32>,
    label: Option<String>,
    force_color: bool,
    force_bool: bool,
}

impl Annotation {
    /// Parse the text after `// @param` (or `@color` / `@bool`).
    ///
    /// Accepted shapes:
    ///
    /// ```text
    /// // @param 0 1 0.01 "Dim amount"
    /// // @param AMOUNT 0 1 0.01 "Dim amount"
    /// // @color "Tint"
    /// // @bool "Invert"
    /// ```
    fn parse(key: &str, rest: &str) -> Result<Self, String> {
        let (words, label) = split_label(rest);
        let mut ann = Annotation {
            label,
            force_color: key == "color" || key == "colour",
            force_bool: key == "bool",
            ..Default::default()
        };

        let mut it = words.into_iter().peekable();

        // An identifier first means the named form.
        if let Some(first) = it.peek() {
            if first.parse::<f32>().is_err() {
                let name = it.next().unwrap();
                if !is_identifier(&name) {
                    return Err(format!("`{name}` is not a valid GLSL identifier"));
                }
                ann.name = Some(name);
            }
        }

        let nums: Vec<f32> = it
            .map(|w| w.parse::<f32>().map_err(|_| format!("`{w}` is not a number")))
            .collect::<Result<_, _>>()?;

        match nums.len() {
            0 => {}
            1 => return Err("give a min and a max, or neither".into()),
            2 => {
                ann.min = Some(nums[0]);
                ann.max = Some(nums[1]);
            }
            _ => {
                ann.min = Some(nums[0]);
                ann.max = Some(nums[1]);
                ann.step = Some(nums[2]);
            }
        }

        if let (Some(a), Some(b)) = (ann.min, ann.max) {
            if b <= a {
                return Err(format!("max ({b}) must be greater than min ({a})"));
            }
        }

        Ok(ann)
    }

    fn apply(&self, p: &mut Param) {
        p.annotated = true;
        if let Some(l) = &self.label {
            p.label = l.clone();
        }
        if self.force_bool {
            p.kind = ParamKind::Bool;
            return;
        }
        if self.force_color {
            let alpha = p.glsl_type == "vec4";
            p.kind = ParamKind::Color { alpha };
            return;
        }
        let (min, max) = match (self.min, self.max) {
            (Some(a), Some(b)) => (a, b),
            _ => return,
        };
        let step = self.step.unwrap_or_else(|| nice_step(min, max));
        p.kind = match &p.kind {
            ParamKind::Scalar { integer, .. } => {
                ParamKind::Scalar { min, max, step, integer: *integer }
            }
            ParamKind::Vector { len, .. } => ParamKind::Vector { len: *len, min, max, step },
            ParamKind::Color { .. } => ParamKind::Scalar { min, max, step, integer: false },
            ParamKind::Bool => ParamKind::Bool,
        };
    }
}

/// Split `0 1 0.01 "Dim amount"` into words and the quoted label.
fn split_label(rest: &str) -> (Vec<String>, Option<String>) {
    if let Some(start) = rest.find('"') {
        let head = &rest[..start];
        let tail = &rest[start + 1..];
        let label = tail.find('"').map(|e| tail[..e].to_string());
        (head.split_whitespace().map(String::from).collect(), label)
    } else {
        (rest.split_whitespace().map(String::from).collect(), None)
    }
}

fn is_identifier(s: &str) -> bool {
    let mut cs = s.chars();
    matches!(cs.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// ---------------------------------------------------------------------------
// Building a parameter from a const declaration
// ---------------------------------------------------------------------------

fn build_param(
    ty: &str,
    name: &str,
    literal: &str,
    line: usize,
    ann: Option<Annotation>,
) -> Result<Param, String> {
    let value = parse_literal(ty, literal)?;
    let label = prettify(&name.to_ascii_lowercase());

    let kind = match ty {
        "bool" => ParamKind::Bool,
        "float" | "int" => {
            let integer = ty == "int";
            let (min, max) = guess_range(&value.components(), integer);
            let step = if integer { 1.0 } else { nice_step(min, max) };
            ParamKind::Scalar { min, max, step, integer }
        }
        "vec3" | "vec4" if looks_like_color(name, &value) => {
            ParamKind::Color { alpha: ty == "vec4" }
        }
        _ => {
            let comps = value.components();
            let len = comps.len();
            let integer = ty.starts_with("ivec");
            let (min, max) = guess_range(&comps, integer);
            let step = if integer { 1.0 } else { nice_step(min, max) };
            ParamKind::Vector { len, min, max, step }
        }
    };

    let mut p = Param {
        name: name.to_string(),
        label,
        glsl_type: ty.to_string(),
        kind,
        value,
        annotated: false,
        line,
    };
    if let Some(a) = ann {
        a.apply(&mut p);
    }
    Ok(p)
}

/// Parse `0.5`, `vec3(0.1, 0.2, 0.3)`, `vec3(0.5)`, `true`.
fn parse_literal(ty: &str, literal: &str) -> Result<ParamValue, String> {
    let lit = literal.trim();
    match ty {
        "bool" => match lit {
            "true" => Ok(ParamValue::Scalar(1.0)),
            "false" => Ok(ParamValue::Scalar(0.0)),
            other => Err(format!("`{other}` is not a bool literal")),
        },
        "float" | "int" => lit
            .trim_end_matches(['f', 'F'])
            .parse::<f32>()
            .map(ParamValue::Scalar)
            .map_err(|_| format!("`{lit}` is not a number this app can edit")),
        _ => {
            let want = match ty {
                "vec2" | "ivec2" => 2,
                "vec3" | "ivec3" => 3,
                _ => 4,
            };
            let open = lit.find('(').ok_or_else(|| format!("`{lit}` is not a {ty} literal"))?;
            let close = lit.rfind(')').ok_or_else(|| format!("`{lit}` is not a {ty} literal"))?;
            if close < open {
                return Err(format!("`{lit}` is not a {ty} literal"));
            }
            let inner = &lit[open + 1..close];
            let parts: Vec<f32> = inner
                .split(',')
                .map(|p| {
                    p.trim()
                        .trim_end_matches(['f', 'F'])
                        .parse::<f32>()
                        .map_err(|_| format!("`{}` is not a plain number", p.trim()))
                })
                .collect::<Result<_, _>>()?;
            match parts.len() {
                // vec3(0.5) means all components.
                1 => Ok(ParamValue::Vector(vec![parts[0]; want])),
                n if n == want => Ok(ParamValue::Vector(parts)),
                n => Err(format!("{ty} literal has {n} components, expected {want} or 1")),
            }
        }
    }
}

fn looks_like_color(name: &str, value: &ParamValue) -> bool {
    let lower = name.to_ascii_lowercase();
    let named = ["color", "colour", "tint", "rgb", "rgba", "hue"].iter().any(|k| lower.contains(k));
    let in_unit_range = value.components().iter().all(|c| (0.0..=1.0).contains(c));
    named && in_unit_range
}

/// Pick a plausible slider range for a value with no annotation.
///
/// A negative component means the range has to straddle zero, or the current
/// value would sit outside its own slider.
fn guess_range(comps: &[f32], integer: bool) -> (f32, f32) {
    let peak = comps.iter().fold(0.0f32, |a, b| a.max(b.abs()));
    let signed = comps.iter().any(|c| *c < 0.0);

    let hi = if integer {
        (peak * 4.0).max(16.0).ceil()
    } else if peak <= 1.0 {
        1.0
    } else if peak <= 10.0 {
        10.0
    } else if peak <= 100.0 {
        100.0
    } else {
        (peak * 2.0).ceil()
    };

    if signed {
        (-hi, hi)
    } else {
        (0.0, hi)
    }
}

/// A step that gives roughly 100 stops across the range, rounded to something
/// a person would have typed.
fn nice_step(min: f32, max: f32) -> f32 {
    let span = (max - min).abs();
    if span <= 0.0 {
        return 0.01;
    }
    let rough = span / 100.0;
    let mag = 10f32.powf(rough.log10().floor());
    let norm = rough / mag;
    let snapped = if norm <= 1.0 {
        1.0
    } else if norm <= 2.0 {
        2.0
    } else if norm <= 5.0 {
        5.0
    } else {
        10.0
    };
    snapped * mag
}

/// Format one float as a GLSL literal.
///
/// Uses the shortest decimal that reads back as the same `f32`, rather than a
/// fixed number of decimal places: a slider whose step is finer than a
/// thousandth — which [`nice_step`] will happily produce for a narrow range —
/// would otherwise have its value quantised away on the trip through the file.
fn glsl_float(v: f32) -> String {
    let s = format!("{v}");
    // A GLSL float literal wants a decimal point, or it is an int. Infinities
    // and NaN have no literal at all, so they are left as-is to be rejected on
    // the way back in rather than written as something that looks valid.
    if s.contains('.') || s.contains('e') || s.contains("inf") || s.contains("NaN") {
        s
    } else {
        format!("{s}.0")
    }
}

/// Render a value back as a GLSL literal of the given type.
pub fn render_literal(ty: &str, value: &ParamValue) -> String {
    match ty {
        "bool" => if value.first() != 0.0 { "true" } else { "false" }.to_string(),
        "int" => format!("{}", value.first().round() as i64),
        "float" => glsl_float(value.first()),
        t if t.starts_with("ivec") => {
            let comps: Vec<String> =
                value.components().iter().map(|c| format!("{}", c.round() as i64)).collect();
            format!("{t}({})", comps.join(", "))
        }
        t => {
            let comps: Vec<String> = value.components().iter().map(|c| glsl_float(*c)).collect();
            format!("{t}({})", comps.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"#version 320 es
precision highp float;
// @label Reading mode
// @desc Warm paper tint for long reads
// @duration 0.4
// @overlay

in vec2 v_texcoord;
out vec4 fragColor;
uniform sampler2D tex;
uniform float progress;
uniform float is_active;
uniform vec2 surface_size;

// @param 0.0 1.0 0.01 "Dim amount"
const float DIM = 0.6;
// @param STRENGTH 0 4 "Warmth"
const float STRENGTH = 1.5;
const vec3 PAPER_COLOR = vec3(0.98, 0.94, 0.86);
const int SAMPLES = 8;
const bool INVERT = false;
const vec2 OFFSET = vec2(0.01, -0.02);

void main() {
    fragColor = texture(tex, v_texcoord);
}
"#;

    fn info() -> ShaderInfo {
        parse("/s/reading_mode.glsl", SRC, 0)
    }

    #[test]
    fn directives_are_read() {
        let i = info();
        assert_eq!(i.name, "Reading mode");
        assert_eq!(i.description.as_deref(), Some("Warm paper tint for long reads"));
        assert_eq!(i.duration, Some(0.4));
        assert!(i.overlay);
        assert_eq!(i.duration_line, Some(4));
    }

    #[test]
    fn only_known_uniforms_are_listed() {
        let i = info();
        assert!(i.uniforms.contains(&"progress".to_string()));
        assert!(i.uniforms.contains(&"is_active".to_string()));
        assert!(i.uniforms.contains(&"surface_size".to_string()));
        // `tex` is required plumbing, not a plugin-populated uniform.
        assert!(!i.uniforms.contains(&"tex".to_string()));
    }

    #[test]
    fn preceding_annotation_attaches_to_the_next_const() {
        let i = info();
        let p = i.params.iter().find(|p| p.name == "DIM").unwrap();
        assert!(p.annotated);
        assert_eq!(p.label, "Dim amount");
        assert_eq!(p.kind, ParamKind::Scalar { min: 0.0, max: 1.0, step: 0.01, integer: false });
        assert_eq!(p.value, ParamValue::Scalar(0.6));
    }

    #[test]
    fn named_annotation_finds_its_const() {
        let i = info();
        let p = i.params.iter().find(|p| p.name == "STRENGTH").unwrap();
        assert!(p.annotated);
        assert_eq!(p.label, "Warmth");
        match p.kind {
            ParamKind::Scalar { min, max, .. } => {
                assert_eq!((min, max), (0.0, 4.0));
            }
            _ => panic!("expected a scalar"),
        }
    }

    #[test]
    fn a_vec3_named_color_becomes_a_colour_picker() {
        let i = info();
        let p = i.params.iter().find(|p| p.name == "PAPER_COLOR").unwrap();
        assert_eq!(p.kind, ParamKind::Color { alpha: false });
        assert!(!p.annotated);
    }

    #[test]
    fn ints_bools_and_vectors_are_inferred() {
        let i = info();
        let s = i.params.iter().find(|p| p.name == "SAMPLES").unwrap();
        assert!(matches!(s.kind, ParamKind::Scalar { integer: true, .. }));

        let b = i.params.iter().find(|p| p.name == "INVERT").unwrap();
        assert_eq!(b.kind, ParamKind::Bool);
        assert_eq!(b.value, ParamValue::Scalar(0.0));

        let v = i.params.iter().find(|p| p.name == "OFFSET").unwrap();
        assert!(matches!(v.kind, ParamKind::Vector { len: 2, .. }));
        assert_eq!(v.value, ParamValue::Vector(vec![0.01, -0.02]));
    }

    #[test]
    fn a_progress_shader_without_duration_is_flagged() {
        let src = "uniform float progress;\nvoid main() {}\n";
        let i = parse("/s/x.glsl", src, 0);
        assert!(i.is_animation());
        assert!(i.notes.iter().any(|n| n.contains("@duration")));
    }

    #[test]
    fn motion_driven_shaders_are_detected() {
        let src = "uniform vec2 peak_velocity;\n";
        assert!(parse("/s/w.glsl", src, 0).is_motion_driven());
    }

    #[test]
    fn a_stray_annotation_does_not_attach_across_code() {
        let src = "// @param 0 1 \"Nope\"\nvoid main() {}\nconst float A = 0.5;\n";
        let i = parse("/s/x.glsl", src, 0);
        let p = &i.params[0];
        assert!(!p.annotated);
    }

    #[test]
    fn a_bad_annotation_becomes_a_note_not_a_panic() {
        let src = "// @param 5 1 \"Backwards\"\nconst float A = 0.5;\n";
        let i = parse("/s/x.glsl", src, 0);
        assert!(i.notes.iter().any(|n| n.contains("greater than")));
        assert_eq!(i.params.len(), 1);
    }

    #[test]
    fn vec_shorthand_expands() {
        assert_eq!(
            parse_literal("vec3", "vec3(0.5)").unwrap(),
            ParamValue::Vector(vec![0.5, 0.5, 0.5])
        );
    }

    #[test]
    fn expressions_are_left_alone() {
        // A const the app cannot safely rewrite must not become a slider.
        let src = "const float A = 1.0 / 3.0;\n";
        let i = parse("/s/x.glsl", src, 0);
        assert!(i.params.is_empty());
        assert!(!i.notes.is_empty());
    }

    #[test]
    fn literals_render_as_glsl() {
        assert_eq!(render_literal("float", &ParamValue::Scalar(1.0)), "1.0");
        assert_eq!(render_literal("float", &ParamValue::Scalar(0.25)), "0.25");
        assert_eq!(render_literal("int", &ParamValue::Scalar(7.4)), "7");
        assert_eq!(render_literal("bool", &ParamValue::Scalar(1.0)), "true");
        assert_eq!(
            render_literal("vec3", &ParamValue::Vector(vec![0.0, 0.5, 1.0])),
            "vec3(0.0, 0.5, 1.0)"
        );
    }

    #[test]
    fn a_value_finer_than_a_thousandth_survives_the_round_trip() {
        // nice_step gives a range of 0–0.05 a step of 0.0005, so rendering at
        // three decimal places would quantise most of that slider away.
        for v in [0.0005f32, 0.00025, 0.12345, 1.0e-6, 123.456_79] {
            let rendered = render_literal("float", &ParamValue::Scalar(v));
            let back = parse_literal("float", &rendered).unwrap();
            assert_eq!(back, ParamValue::Scalar(v), "{v} rendered as {rendered}");
        }
    }

    #[test]
    fn vector_components_keep_their_precision_too() {
        let v = ParamValue::Vector(vec![0.0001, -0.00025, 0.5]);
        let rendered = render_literal("vec3", &v);
        assert_eq!(parse_literal("vec3", &rendered).unwrap(), v, "{rendered}");
    }

    #[test]
    fn mathematical_constants_are_not_offered_as_sliders() {
        // pulse.glsl in the wild declares exactly this and nothing else, so its
        // only "parameter" was the one you must not touch.
        let src = "uniform float progress;\n// @duration 0.35\nconst float PI = 3.14159265;\n";
        let i = parse("/s/pulse.glsl", src, 0);
        assert!(i.params.is_empty(), "{:?}", i.params);
        assert!(i.notes.iter().any(|n| n.contains("PI")), "{:?}", i.notes);
    }

    #[test]
    fn the_usual_spellings_are_all_caught() {
        let src =
            "const float TAU = 6.28;\nconst float M_PI = 3.14;\nconst float EPSILON = 1e-6;\n\
                   const float _PI_ = 3.14;\nconst float golden = 1.618;\n";
        assert!(parse("/s/x.glsl", src, 0).params.is_empty());
    }

    #[test]
    fn an_annotation_overrides_the_constant_check() {
        // If the author says it is a parameter, it is one.
        let src = "// @param 3.0 3.3 0.01 \"Pi, slightly wrong\"\nconst float PI = 3.14159;\n";
        let i = parse("/s/x.glsl", src, 0);
        assert_eq!(i.params.len(), 1);
        assert_eq!(i.params[0].label, "Pi, slightly wrong");
    }

    #[test]
    fn a_constant_shaped_value_under_another_name_is_still_editable() {
        let src = "const float SPEED = 3.14159;\n";
        let i = parse("/s/x.glsl", src, 0);
        assert_eq!(i.params.len(), 1);
        assert_eq!(i.params[0].name, "SPEED");
    }

    #[test]
    fn steps_are_round_numbers() {
        assert_eq!(nice_step(0.0, 1.0), 0.01);
        assert_eq!(nice_step(0.0, 10.0), 0.1);
        assert_eq!(nice_step(0.0, 100.0), 1.0);
    }

    #[test]
    fn a_negative_default_gets_a_range_that_contains_it() {
        let src = "const float A = -0.4;\n";
        let i = parse("/s/x.glsl", src, 0);
        match i.params[0].kind {
            ParamKind::Scalar { min, max, .. } => {
                assert_eq!((min, max), (-1.0, 1.0));
            }
            _ => panic!("expected a scalar"),
        }
    }

    #[test]
    fn inferred_vector_range_covers_negative_components() {
        let i = info();
        let v = i.params.iter().find(|p| p.name == "OFFSET").unwrap();
        match v.kind {
            ParamKind::Vector { min, max, .. } => {
                assert!(min <= -0.02 && max >= 0.01);
            }
            _ => panic!("expected a vector"),
        }
    }
}
