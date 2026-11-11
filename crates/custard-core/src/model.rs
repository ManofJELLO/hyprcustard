//! The configuration model.
//!
//! This is the single source of truth the GUI edits. It is serialised to JSON
//! and embedded in the managed block of `hyprland.lua` so a later run can read
//! back exactly what it wrote, without having to parse Lua.

use serde::{Deserialize, Serialize};

/// Current schema version of the embedded state blob.
pub const STATE_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Tag slots
// ---------------------------------------------------------------------------

/// One of the shader tags HyprWindowShade understands on a window rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagSlot {
    /// `+shader:` — always applied.
    Shader,
    /// `+shader_fullscreen:` — the one shader that applies while fullscreen.
    Fullscreen,
    /// `+shader_active:` — only while focused.
    Active,
    /// `+shader_inactive:` — only while unfocused.
    Inactive,
    /// `+shader_floating:`
    Floating,
    /// `+shader_tiled:`
    Tiled,
    /// `+shader_open:` — one-shot on open.
    Open,
    /// `+shader_close:` — one-shot on close.
    Close,
    /// `+shader_move:` — while being moved.
    Move,
    /// `+shader_resize:` — while being resized.
    Resize,
    /// `+shader_workspace:` — while the workspace slides.
    Workspace,
    /// `+shader_fullscreen_enter:` — opt-in, see the plugin README.
    FullscreenEnter,
    /// `+shader_fullscreen_exit:`
    FullscreenExit,
    /// `+shader_float:` — on becoming floating.
    Float,
    /// `+shader_tile:` — on becoming tiled.
    Tile,
    /// `+shader_urgent:`
    Urgent,
    /// `+shader_focus:`
    Focus,
    /// `+shader_unfocus:`
    Unfocus,
    /// `+shader_replace:1` — opt out of stacking.
    Replace,
    /// `+shader_fullscreen_stack:1` — keep the stack while fullscreen.
    FullscreenStack,
}

/// What kind of value a tag slot carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagKind {
    /// Carries a shader path, optionally with an `@seconds` override.
    Path,
    /// Carries the literal `1`.
    Flag,
}

/// Static description of a tag slot, used to drive the UI.
#[derive(Debug, Clone, Copy)]
pub struct TagInfo {
    /// The tag name as written in the config, without the leading `+` or the
    /// trailing `:`.
    pub key: &'static str,
    /// Human-readable label.
    pub label: &'static str,
    /// One line of help text.
    pub help: &'static str,
    /// Path-valued or flag-valued.
    pub kind: TagKind,
    /// Whether an `@seconds` duration override does anything for this slot.
    pub takes_duration: bool,
    /// Whether a `_default` fallback form exists.
    pub supports_default: bool,
    /// Whether the plugin README explicitly documents the `_default` form for
    /// this slot. The tag table says any path tag accepts it; the fallback
    /// section names eight. Slots outside those eight get a soft warning.
    pub default_documented: bool,
    /// UI grouping.
    pub group: TagGroup,
}

/// How tags are grouped in the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagGroup {
    /// Persistent looks.
    Persistent,
    /// Focus- and geometry-dependent looks.
    State,
    /// One-shot animations.
    Animation,
    /// Behaviour switches.
    Behavior,
}

impl TagGroup {
    /// Label for the group header.
    pub fn label(self) -> &'static str {
        match self {
            TagGroup::Persistent => "Always on",
            TagGroup::State => "Depends on state",
            TagGroup::Animation => "Animations",
            TagGroup::Behavior => "Behaviour",
        }
    }
}

macro_rules! tag_table {
    ($($slot:ident => $key:literal, $label:literal, $help:literal,
       $kind:ident, $dur:literal, $def:literal, $defdoc:literal, $group:ident;)*) => {
        /// Every tag slot, in the order the UI should present them.
        pub const TAGS: &[(TagSlot, TagInfo)] = &[
            $((TagSlot::$slot, TagInfo {
                key: $key, label: $label, help: $help,
                kind: TagKind::$kind, takes_duration: $dur,
                supports_default: $def, default_documented: $defdoc,
                group: TagGroup::$group,
            }),)*
        ];
    };
}

tag_table! {
    Shader => "shader", "Always",
        "Applied regardless of focus. The base layer of the stack.",
        Path, false, true, true, Persistent;
    Active => "shader_active", "While focused",
        "Applies only when the window has focus.",
        Path, false, true, true, State;
    Inactive => "shader_inactive", "While unfocused",
        "Applies only when the window does not have focus. Cheaper than Always \
         for effects you do not want while working in the window.",
        Path, false, true, true, State;
    Floating => "shader_floating", "While floating",
        "Applies only while the window is floating.",
        Path, false, true, true, State;
    Tiled => "shader_tiled", "While tiled",
        "Applies only while the window is tiled.",
        Path, false, true, true, State;
    Fullscreen => "shader_fullscreen", "While fullscreen",
        "The one shader that applies while fullscreen. Without it, a fullscreen \
         window renders unshaded unless you also set 'Keep stack fullscreen'.",
        Path, false, true, true, State;
    Open => "shader_open", "On open",
        "Plays once as the window opens, on top of the window's normal shader.",
        Path, true, true, true, Animation;
    Close => "shader_close", "On close",
        "Plays once as the window closes. Must end fully transparent.",
        Path, true, true, true, Animation;
    Move => "shader_move", "While moving",
        "Plays while the window is being moved. Hyprland's windowsMove animation \
         is the clock, so a duration override does nothing here.",
        Path, false, true, false, Animation;
    Resize => "shader_resize", "While resizing",
        "Plays while the window is being resized. Drive it from size_velocity — \
         velocity is near zero during a corner drag.",
        Path, false, true, false, Animation;
    Workspace => "shader_workspace", "Workspace slide",
        "Plays while the window's workspace slides in or out.",
        Path, false, true, false, Animation;
    FullscreenEnter => "shader_fullscreen_enter", "Entering fullscreen",
        "Opt-in. Setting it also suppresses the generic move/resize shader for \
         that transition.",
        Path, true, true, false, Animation;
    FullscreenExit => "shader_fullscreen_exit", "Leaving fullscreen",
        "Opt-in, same as entering.",
        Path, true, true, false, Animation;
    Float => "shader_float", "Becoming floating",
        "Plays when the window is toggled to floating.",
        Path, true, true, false, Animation;
    Tile => "shader_tile", "Becoming tiled",
        "Plays when the window is toggled back to tiled.",
        Path, true, true, false, Animation;
    Urgent => "shader_urgent", "On urgent",
        "One-shot cue when the window is marked urgent.",
        Path, true, true, false, Animation;
    Focus => "shader_focus", "On gaining focus",
        "One-shot cue when the window gains focus. Distinct from 'While focused'.",
        Path, true, true, false, Animation;
    Unfocus => "shader_unfocus", "On losing focus",
        "One-shot cue when the window loses focus.",
        Path, true, true, false, Animation;
    Replace => "shader_replace", "Opt out of stacking",
        "First matching tag wins and the rest are ignored, the pre-stacking \
         behaviour. Has no _default form.",
        Flag, false, false, false, Behavior;
    FullscreenStack => "shader_fullscreen_stack", "Keep stack fullscreen",
        "Keeps the window's normal shader stack while fullscreen instead of \
         dropping everything. Has no _default form.",
        Flag, false, false, false, Behavior;
}

impl TagSlot {
    /// Look up the static description of this slot.
    pub fn info(self) -> &'static TagInfo {
        TAGS.iter().find(|(s, _)| *s == self).map(|(_, i)| i).expect("every TagSlot is in TAGS")
    }

    /// The config key, e.g. `shader_inactive`.
    pub fn key(self) -> &'static str {
        self.info().key
    }

    /// Parse a config key (with or without a `_default` suffix) into a slot.
    ///
    /// Returns the slot and whether the `_default` suffix was present.
    pub fn parse_key(key: &str) -> Option<(TagSlot, bool)> {
        let key = key.trim_start_matches('+').trim_end_matches(':');
        if let Some((slot, info)) = TAGS.iter().find(|(_, i)| i.key == key) {
            let _ = info;
            return Some((*slot, false));
        }
        let base = key.strip_suffix("_default")?;
        TAGS.iter().find(|(_, i)| i.key == base && i.supports_default).map(|(s, _)| (*s, true))
    }
}

// ---------------------------------------------------------------------------
// Shader references
// ---------------------------------------------------------------------------

/// A shader path plus an optional `@seconds` duration override.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShaderRef {
    /// Absolute path to the `.glsl` file.
    pub path: String,
    /// `@seconds` override appended to the path. `None` means the shader's own
    /// `// @duration` (or the plugin's 0.3s default) decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f32>,
}

impl ShaderRef {
    /// A reference with no duration override.
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into(), duration: None }
    }

    /// Render as it appears in the config: `/path/x.glsl` or `/path/x.glsl@0.6`.
    pub fn to_value(&self) -> String {
        match self.duration {
            Some(d) => format!("{}@{}", self.path, trim_float(d)),
            None => self.path.clone(),
        }
    }

    /// Parse `/path/x.glsl@0.6` back into a reference.
    pub fn parse(value: &str) -> Self {
        // Only split on an `@` that is followed by a number, so a path
        // containing `@` (rare, but legal) survives intact.
        if let Some(idx) = value.rfind('@') {
            let (head, tail) = value.split_at(idx);
            if let Ok(secs) = tail[1..].parse::<f32>() {
                return Self { path: head.to_string(), duration: Some(secs) };
            }
        }
        Self::new(value)
    }
}

/// Format a float without a trailing `.0`, which reads better in a config.
pub fn trim_float(v: f32) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

/// The value carried by one tag on a rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TagValue {
    /// A shader path with an optional duration.
    Path(ShaderRef),
    /// A switch, emitted as `:1`.
    ///
    /// A struct variant rather than a newtype because serde's internally
    /// tagged representation cannot carry a bare `bool`.
    Flag {
        /// Present and on. A tag that is off is removed rather than set false.
        on: bool,
    },
}

/// One tag on one window rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    /// Which slot this fills.
    pub slot: TagSlot,
    /// Whether to emit the `_default` fallback form.
    #[serde(default)]
    pub is_default: bool,
    /// The value.
    pub value: TagValue,
}

impl Tag {
    /// Render the whole tag string, e.g. `+shader_close_default:/p/x.glsl@1.0`.
    pub fn to_tag_string(&self) -> String {
        let key = if self.is_default && self.slot.info().supports_default {
            format!("{}_default", self.slot.key())
        } else {
            self.slot.key().to_string()
        };
        match &self.value {
            TagValue::Path(r) => format!("+{}:{}", key, r.to_value()),
            TagValue::Flag { .. } => format!("+{key}:1"),
        }
    }
}

// ---------------------------------------------------------------------------
// Window rules
// ---------------------------------------------------------------------------

/// The match criteria of a window rule.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Match {
    /// `class = "..."` — a Lua pattern / regex as Hyprland understands it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    /// `title = "..."`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// `initialClass = "..."`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_class: Option<String>,
    /// `initialTitle = "..."`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_title: Option<String>,
}

impl Match {
    /// True when nothing is set, which would match every window.
    pub fn is_empty(&self) -> bool {
        self.class.is_none()
            && self.title.is_none()
            && self.initial_class.is_none()
            && self.initial_title.is_none()
    }

    /// A short human description for list rows.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if let Some(v) = &self.class {
            parts.push(format!("class {v}"));
        }
        if let Some(v) = &self.title {
            parts.push(format!("title {v}"));
        }
        if let Some(v) = &self.initial_class {
            parts.push(format!("initialClass {v}"));
        }
        if let Some(v) = &self.initial_title {
            parts.push(format!("initialTitle {v}"));
        }
        if parts.is_empty() {
            "every window".into()
        } else {
            parts.join(", ")
        }
    }
}

/// One `hl.window_rule` entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowRule {
    /// Stable id so the UI can address a rule across reorders.
    pub id: String,
    /// The `name =` field. Optional, but useful in Hyprland's rule listing.
    #[serde(default)]
    pub name: String,
    /// Disabled rules are kept in state but not emitted.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// What the rule matches.
    #[serde(default)]
    pub match_: Match,
    /// The tags it applies.
    #[serde(default)]
    pub tags: Vec<Tag>,
}

fn yes() -> bool {
    true
}

impl WindowRule {
    /// A new empty rule with a fresh id.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: String::new(),
            enabled: true,
            match_: Match::default(),
            tags: Vec::new(),
        }
    }

    /// The tag filling a given slot, if any.
    pub fn tag(&self, slot: TagSlot) -> Option<&Tag> {
        self.tags.iter().find(|t| t.slot == slot)
    }

    /// Set or clear a slot.
    pub fn set_tag(&mut self, tag: Option<Tag>, slot: TagSlot) {
        self.tags.retain(|t| t.slot != slot);
        if let Some(t) = tag {
            self.tags.push(t);
        }
        self.tags
            .sort_by_key(|t| TAGS.iter().position(|(s, _)| *s == t.slot).unwrap_or(usize::MAX));
    }
}

// ---------------------------------------------------------------------------
// Layers
// ---------------------------------------------------------------------------

/// Shader and animation assignments for one layer namespace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerEntry {
    /// Stable id.
    pub id: String,
    /// The layer namespace, or `*` for the catch-all.
    pub namespace: String,
    /// Disabled entries are kept but not emitted.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// `layershader(ns, path)`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shader: Option<ShaderRef>,
    /// `layeropenanim(ns, path[@sec])`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_anim: Option<ShaderRef>,
    /// `layercloseanim(ns, path[@sec])`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub close_anim: Option<ShaderRef>,
}

impl LayerEntry {
    /// A new empty entry for a namespace.
    pub fn new(id: impl Into<String>, namespace: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            namespace: namespace.into(),
            enabled: true,
            shader: None,
            open_anim: None,
            close_anim: None,
        }
    }

    /// True when the entry would emit nothing.
    pub fn is_empty(&self) -> bool {
        self.shader.is_none() && self.open_anim.is_none() && self.close_anim.is_none()
    }
}

// ---------------------------------------------------------------------------
// Startup shaders and keybinds
// ---------------------------------------------------------------------------

/// A plugin call, used both for startup actions and for keybinds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    /// `togglewindowshader(path)` — the focused window.
    ToggleWindowShader {
        /// Shader to toggle.
        shader: ShaderRef,
    },
    /// `classshader(class, path|"clear")`
    ClassShader {
        /// Window class to target.
        class: String,
        /// `None` emits `"clear"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shader: Option<ShaderRef>,
    },
    /// `toggleclassshader(class, path)`
    ToggleClassShader {
        /// Window class to target.
        class: String,
        /// Shader to toggle.
        shader: ShaderRef,
    },
    /// `layershader(namespace, path|"clear")`
    LayerShader {
        /// Layer namespace.
        namespace: String,
        /// `None` emits `"clear"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shader: Option<ShaderRef>,
    },
    /// `togglelayershader(namespace, path)`
    ToggleLayerShader {
        /// Layer namespace.
        namespace: String,
        /// Shader to toggle.
        shader: ShaderRef,
    },
    /// `reloadshaders()`
    ReloadShaders,
}

impl Action {
    /// The Lua call text, assuming `ns` is the plugin table.
    pub fn to_lua_call(&self, ns: &str) -> String {
        let q = lua_quote;
        match self {
            Action::ToggleWindowShader { shader } => {
                format!("{ns}.togglewindowshader({})", q(&shader.to_value()))
            }
            Action::ClassShader { class, shader } => format!(
                "{ns}.classshader({}, {})",
                q(class),
                q(shader.as_ref().map(|s| s.to_value()).unwrap_or_else(|| "clear".into()).as_str())
            ),
            Action::ToggleClassShader { class, shader } => {
                format!("{ns}.toggleclassshader({}, {})", q(class), q(&shader.to_value()))
            }
            Action::LayerShader { namespace, shader } => format!(
                "{ns}.layershader({}, {})",
                q(namespace),
                q(shader.as_ref().map(|s| s.to_value()).unwrap_or_else(|| "clear".into()).as_str())
            ),
            Action::ToggleLayerShader { namespace, shader } => {
                format!("{ns}.togglelayershader({}, {})", q(namespace), q(&shader.to_value()))
            }
            Action::ReloadShaders => format!("{ns}.reloadshaders()"),
        }
    }

    /// A short human description.
    pub fn summary(&self) -> String {
        match self {
            Action::ToggleWindowShader { shader } => {
                format!("toggle {} on focused window", base_name(&shader.path))
            }
            Action::ClassShader { class, shader } => match shader {
                Some(s) => format!("force {} on class {class}", base_name(&s.path)),
                None => format!("clear class shader on {class}"),
            },
            Action::ToggleClassShader { class, shader } => {
                format!("toggle {} on class {class}", base_name(&shader.path))
            }
            Action::LayerShader { namespace, shader } => match shader {
                Some(s) => format!("force {} on layer {namespace}", base_name(&s.path)),
                None => format!("clear layer shader on {namespace}"),
            },
            Action::ToggleLayerShader { namespace, shader } => {
                format!("toggle {} on layer {namespace}", base_name(&shader.path))
            }
            Action::ReloadShaders => "reload all shaders".into(),
        }
    }
}

/// The file name of a path, for display.
pub fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// A keybind that runs a plugin action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bind {
    /// Stable id.
    pub id: String,
    /// Disabled binds are kept but not emitted.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// The key combination, in `hl.bind` syntax, e.g. `SUPER + W`.
    pub key: String,
    /// What it does.
    pub action: Action,
}

/// An action run once at `hyprland.start`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StartupAction {
    /// Stable id.
    pub id: String,
    /// Disabled actions are kept but not emitted.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// What to run.
    pub action: Action,
}

// ---------------------------------------------------------------------------
// The whole config
// ---------------------------------------------------------------------------

/// How the generated block loads the plugin at session start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum LoadMode {
    /// Emit nothing; the user loads the plugin themselves.
    #[default]
    None,
    /// `hl.exec_cmd("hyprpm reload -n")` inside `hyprland.start`.
    Hyprpm,
    /// `hyprctl plugin load <path>` inside `hyprland.start`.
    Manual,
}

/// Everything the GUI manages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// Schema version of this blob.
    #[serde(default = "default_version")]
    pub version: u32,
    /// Directory scanned for `.glsl` files.
    pub shader_dir: String,
    /// Whether to emit a `local hws_shaders = "..."` and build paths from it.
    #[serde(default = "yes")]
    pub use_shader_dir_variable: bool,
    /// How (or whether) to load the plugin at session start.
    #[serde(default)]
    pub load_mode: LoadMode,
    /// Path to the `.so`, used when `load_mode` is `Manual`.
    #[serde(default)]
    pub plugin_so_path: String,
    /// Seconds to wait before applying startup actions.
    ///
    /// Zero applies them directly inside the `hyprland.start` handler. A
    /// non-zero value re-issues them through a shell-delayed `hyprctl dispatch`
    /// instead, which is the escape hatch for a plugin that finishes loading
    /// after the handler has already run.
    #[serde(default)]
    pub startup_delay_secs: f32,
    /// Window rules.
    #[serde(default)]
    pub rules: Vec<WindowRule>,
    /// Layer namespaces.
    #[serde(default)]
    pub layers: Vec<LayerEntry>,
    /// Actions run at session start.
    #[serde(default)]
    pub startup: Vec<StartupAction>,
    /// Keybinds.
    #[serde(default)]
    pub binds: Vec<Bind>,
    /// Name or path of the GUI's own colour theme.
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_version() -> u32 {
    STATE_VERSION
}

fn default_theme() -> String {
    // The desktop's own colours, until someone picks otherwise.
    "system".into()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            shader_dir: crate::paths::default_shader_dir().to_string_lossy().into_owned(),
            use_shader_dir_variable: true,
            load_mode: LoadMode::None,
            plugin_so_path: String::new(),
            startup_delay_secs: 0.0,
            rules: Vec::new(),
            layers: Vec::new(),
            startup: Vec::new(),
            binds: Vec::new(),
            theme: default_theme(),
        }
    }
}

impl Config {
    /// Mint an id that is not already used by any rule, layer, bind or startup
    /// action.
    pub fn next_id(&self, prefix: &str) -> String {
        let mut n = 1;
        loop {
            let candidate = format!("{prefix}{n}");
            let taken = self.rules.iter().any(|r| r.id == candidate)
                || self.layers.iter().any(|l| l.id == candidate)
                || self.binds.iter().any(|b| b.id == candidate)
                || self.startup.iter().any(|s| s.id == candidate);
            if !taken {
                return candidate;
            }
            n += 1;
        }
    }

    /// Every distinct shader path referenced anywhere in the config.
    pub fn referenced_shaders(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut push = |p: &str| {
            if !out.iter().any(|x| x == p) {
                out.push(p.to_string());
            }
        };
        for r in &self.rules {
            for t in &r.tags {
                if let TagValue::Path(s) = &t.value {
                    push(&s.path);
                }
            }
        }
        for l in &self.layers {
            for s in [&l.shader, &l.open_anim, &l.close_anim].into_iter().flatten() {
                push(&s.path);
            }
        }
        let action_shader = |a: &Action| -> Option<String> {
            match a {
                Action::ToggleWindowShader { shader }
                | Action::ToggleClassShader { shader, .. }
                | Action::ToggleLayerShader { shader, .. } => Some(shader.path.clone()),
                Action::ClassShader { shader, .. } | Action::LayerShader { shader, .. } => {
                    shader.as_ref().map(|s| s.path.clone())
                }
                Action::ReloadShaders => None,
            }
        };
        for b in &self.binds {
            if let Some(p) = action_shader(&b.action) {
                push(&p);
            }
        }
        for s in &self.startup {
            if let Some(p) = action_shader(&s.action) {
                push(&p);
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Lua string quoting
// ---------------------------------------------------------------------------

/// Quote a string as a Lua literal, escaping what needs escaping.
pub fn lua_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // Padded to three digits: Lua reads up to three, so a bare `\7`
            // followed by a digit would be parsed as a different character.
            c if (c as u32) < 0x20 || c == '\x7f' => out.push_str(&format!("\\{:03}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_ref_round_trips_duration() {
        let r = ShaderRef::parse("/p/x.glsl@0.6");
        assert_eq!(r.path, "/p/x.glsl");
        assert_eq!(r.duration, Some(0.6));
        assert_eq!(r.to_value(), "/p/x.glsl@0.6");
    }

    #[test]
    fn shader_ref_without_duration() {
        let r = ShaderRef::parse("/p/x.glsl");
        assert_eq!(r.duration, None);
        assert_eq!(r.to_value(), "/p/x.glsl");
    }

    #[test]
    fn at_sign_in_path_is_not_a_duration() {
        let r = ShaderRef::parse("/home/a@b/x.glsl");
        assert_eq!(r.path, "/home/a@b/x.glsl");
        assert_eq!(r.duration, None);
    }

    #[test]
    fn tag_keys_parse_with_and_without_default() {
        assert_eq!(TagSlot::parse_key("shader"), Some((TagSlot::Shader, false)));
        assert_eq!(TagSlot::parse_key("+shader_close_default:"), Some((TagSlot::Close, true)));
        // shader_replace has no _default form.
        assert_eq!(TagSlot::parse_key("shader_replace_default"), None);
        assert_eq!(TagSlot::parse_key("nonsense"), None);
    }

    #[test]
    fn fullscreen_and_fullscreen_stack_do_not_collide() {
        assert_eq!(TagSlot::parse_key("shader_fullscreen"), Some((TagSlot::Fullscreen, false)));
        assert_eq!(
            TagSlot::parse_key("shader_fullscreen_stack"),
            Some((TagSlot::FullscreenStack, false))
        );
        assert_eq!(
            TagSlot::parse_key("shader_fullscreen_enter"),
            Some((TagSlot::FullscreenEnter, false))
        );
    }

    #[test]
    fn tag_string_rendering() {
        let t = Tag {
            slot: TagSlot::Close,
            is_default: true,
            value: TagValue::Path(ShaderRef { path: "/p/smoke.glsl".into(), duration: Some(1.0) }),
        };
        assert_eq!(t.to_tag_string(), "+shader_close_default:/p/smoke.glsl@1");

        let f =
            Tag { slot: TagSlot::Replace, is_default: true, value: TagValue::Flag { on: true } };
        // is_default is ignored for a slot that has no _default form.
        assert_eq!(f.to_tag_string(), "+shader_replace:1");
    }

    #[test]
    fn quoting_escapes() {
        assert_eq!(lua_quote(r#"a"b\c"#), r#""a\"b\\c""#);
    }

    #[test]
    fn a_control_character_escape_cannot_swallow_the_next_digit() {
        // `"\70"` would be one character, not a bell followed by a zero.
        assert_eq!(lua_quote("\u{7}0"), r#""\0070""#);
        assert_eq!(lua_quote("\u{1}"), r#""\001""#);
    }

    #[test]
    fn every_slot_has_info() {
        for (slot, _) in TAGS {
            let _ = slot.info();
        }
        assert_eq!(TAGS.len(), 20);
    }
}
