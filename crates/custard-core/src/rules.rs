//! Look rules: the look changed for some windows, layers or workspaces only.
//!
//! A rule here is one `hl.window_rule`, `hl.layer_rule` or
//! `hl.workspace_rule` call written into the look block, after the global
//! look. Each is stored as data — what it matches and what it changes — so
//! the page can edit it, and checked by Hyprland itself before it is kept.
//!
//! Four facts about Hyprland 0.57 shape how they are put on the desktop
//! live, all found by trying it:
//!
//! * A rule call returns a handle whose `set_enabled(false)` takes the rule
//!   back off every window at once, and Lua globals live from one
//!   `hyprctl eval` to the next. So the block keeps every handle in one
//!   global table, and a live edit switches the old rule off and adds the new.
//! * Window and layer rules with the same `name` merge, and re-adding a name
//!   that was switched off switches the old one back on. So the app never
//!   names its rules; a comment above each carries the label instead.
//! * Workspace rules for the same workspace merge whatever their name, but
//!   only into one that is still on. Switching ours off therefore also
//!   switches off the user's own rule for the same workspace, so a live edit
//!   runs theirs again, verbatim, before adding ours back on top.
//! * Gaps given positionally, `{ 1, 2, 3, 4 }`, are silently read as zero.
//!   They are always written with named sides.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::look::{self, hypr_color, lua_number, Kind, LookValue};
use crate::model::lua_quote;
use crate::{Error, Result};

/// The Lua global holding a handle for every rule the app has added.
pub const HANDLES: &str = "hyprcustard_rules";

/// What a rule applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleKind {
    /// Application windows.
    Window,
    /// Layer surfaces: bars, launchers, notifications, the wallpaper.
    Layer,
    /// Every window on some workspaces.
    Workspace,
}

impl RuleKind {
    /// Every kind, in the order the page shows them.
    pub const ALL: [RuleKind; 3] = [RuleKind::Window, RuleKind::Layer, RuleKind::Workspace];

    /// The Lua function that makes one.
    pub fn function(self) -> &'static str {
        match self {
            RuleKind::Window => "hl.window_rule",
            RuleKind::Layer => "hl.layer_rule",
            RuleKind::Workspace => "hl.workspace_rule",
        }
    }
}

/// One thing a rule can match on or change.
#[derive(Debug, Clone, Copy)]
pub struct Field {
    /// Which kind of rule has it.
    pub rule: RuleKind,
    /// The key in the rule's table.
    pub key: &'static str,
    /// What the page calls it.
    pub label: &'static str,
    /// A line under it, or empty.
    pub hint: &'static str,
    /// Its value's shape.
    pub kind: Kind,
    /// Slider range.
    pub min: f64,
    /// Slider range.
    pub max: f64,
    /// Slider step.
    pub step: f64,
    /// The value it starts at when added, as JSON.
    pub default: &'static str,
}

#[allow(clippy::too_many_arguments)]
const fn field(
    rule: RuleKind,
    key: &'static str,
    label: &'static str,
    hint: &'static str,
    kind: Kind,
    (min, max, step): (f64, f64, f64),
    default: &'static str,
) -> Field {
    Field { rule, key, label, hint, kind, min, max, step, default }
}

const NONE: (f64, f64, f64) = (0.0, 0.0, 0.0);
use RuleKind::{Layer as L, Window as W, Workspace as S};

/// What a rule can match on. A workspace rule's one "match" is the workspace
/// itself, which Hyprland takes as a top-level field.
pub const MATCHES: &[Field] = &[
    field(
        W,
        "class",
        "Class",
        "The app's class, as a regular expression. kitty, firefox, ^(steam)$.",
        Kind::Text,
        NONE,
        "\"\"",
    ),
    field(
        W,
        "title",
        "Title",
        "The window's title, as a regular expression.",
        Kind::Text,
        NONE,
        "\"\"",
    ),
    field(W, "initial_class", "Class at launch", "", Kind::Text, NONE, "\"\""),
    field(W, "initial_title", "Title at launch", "", Kind::Text, NONE, "\"\""),
    field(W, "tag", "Tag", "Windows given this tag by another rule.", Kind::Text, NONE, "\"\""),
    field(
        W,
        "float",
        "Floating",
        "On for floating windows only, off for tiled ones only.",
        Kind::Bool,
        NONE,
        "false",
    ),
    field(W, "fullscreen", "Fullscreen", "", Kind::Bool, NONE, "true"),
    field(W, "pin", "Pinned", "", Kind::Bool, NONE, "true"),
    field(W, "focus", "Focused", "", Kind::Bool, NONE, "true"),
    field(W, "group", "In a group", "", Kind::Bool, NONE, "true"),
    field(W, "modal", "Dialogs", "Modal dialogs, such as a save prompt.", Kind::Bool, NONE, "true"),
    field(W, "xwayland", "XWayland", "", Kind::Bool, NONE, "true"),
    field(
        W,
        "workspace",
        "On workspace",
        "1, name:web, or a selector such as w[tv1].",
        Kind::Text,
        NONE,
        "\"1\"",
    ),
    field(
        W,
        "content",
        "Content",
        "none, photo, video or game, when the app says.",
        Kind::Text,
        NONE,
        "\"video\"",
    ),
    field(
        L,
        "namespace",
        "Namespace",
        "The layer's name: waybar, rofi, notifications.",
        Kind::Text,
        NONE,
        "\"\"",
    ),
    field(
        S,
        "workspace",
        "Workspace",
        "1, name:web, special, or a selector: w[tv1] is one tiled window, f[1] a fullscreen one.",
        Kind::Text,
        NONE,
        "\"\"",
    ),
];

/// What a rule can change — the look-related effects only. A rule moved in
/// from the user's config keeps any others as they were written.
pub const EFFECTS: &[Field] = &[
    field(
        W,
        "opacity",
        "Opacity",
        "Replaces the global opacity for these windows.",
        Kind::Vec2,
        (0.0, 1.0, 0.01),
        "[0.9, 0.8]",
    ),
    field(W, "border_size", "Border size", "", Kind::Int, (0.0, 20.0, 1.0), "3"),
    field(
        W,
        "border_color",
        "Border color",
        "Used focused or not.",
        Kind::Gradient,
        NONE,
        r##"{"colors":["#ffff9e3b"],"angle":0}"##,
    ),
    field(W, "rounding", "Rounding", "", Kind::Int, (0.0, 40.0, 1.0), "0"),
    field(
        W,
        "rounding_power",
        "Rounding curve",
        "2 is a circle; higher is squarer.",
        Kind::Float,
        (1.0, 10.0, 0.1),
        "2",
    ),
    field(W, "no_shadow", "No shadow", "", Kind::Bool, NONE, "true"),
    field(W, "no_glow", "No glow", "", Kind::Bool, NONE, "true"),
    field(W, "no_blur", "No blur", "", Kind::Bool, NONE, "true"),
    field(W, "no_dim", "Never dimmed", "Not dimmed when it loses focus.", Kind::Bool, NONE, "true"),
    field(W, "dim_around", "Dim everything behind", "", Kind::Bool, NONE, "true"),
    field(
        W,
        "xray",
        "Blur shows only the wallpaper",
        "Other windows behind are not blurred in.",
        Kind::Bool,
        NONE,
        "true",
    ),
    field(W, "opaque", "Always opaque", "", Kind::Bool, NONE, "true"),
    field(
        W,
        "decorate",
        "Decorations",
        "Off draws no border, shadow or rounding.",
        Kind::Bool,
        NONE,
        "false",
    ),
    field(W, "no_anim", "No animations", "", Kind::Bool, NONE, "true"),
    field(
        W,
        "animation",
        "Animation style",
        "popin 80%, slide, gnomed …",
        Kind::Text,
        NONE,
        "\"popin 80%\"",
    ),
    field(L, "blur", "Blur behind", "", Kind::Bool, NONE, "true"),
    field(L, "blur_popups", "Blur its popups too", "", Kind::Bool, NONE, "true"),
    field(
        L,
        "ignore_alpha",
        "Blur skips below this opacity",
        "Keeps blur off see-through edges and shadows.",
        Kind::Float,
        (0.0, 1.0, 0.01),
        "0.5",
    ),
    field(L, "dim_around", "Dim everything behind", "", Kind::Bool, NONE, "true"),
    field(L, "xray", "Blur shows only the wallpaper", "", Kind::Bool, NONE, "true"),
    field(L, "no_anim", "No animations", "", Kind::Bool, NONE, "true"),
    field(
        L,
        "animation",
        "Animation style",
        "slide top, fade, popin 90% …",
        Kind::Text,
        NONE,
        "\"slide top\"",
    ),
    field(S, "gaps_in", "Gaps between windows", "", Kind::Sides, (0.0, 60.0, 1.0), "0"),
    field(S, "gaps_out", "Gaps at the edges", "", Kind::Sides, (0.0, 100.0, 1.0), "0"),
    field(S, "border_size", "Border size", "", Kind::Int, (0.0, 20.0, 1.0), "0"),
    field(S, "no_border", "No borders", "", Kind::Bool, NONE, "true"),
    field(S, "no_rounding", "No rounding", "", Kind::Bool, NONE, "true"),
    field(S, "no_shadow", "No shadows", "", Kind::Bool, NONE, "true"),
    field(
        S,
        "decorate",
        "Decorations",
        "Off draws no border, shadow or rounding.",
        Kind::Bool,
        NONE,
        "false",
    ),
];

/// A match field by kind and key.
pub fn match_field(rule: RuleKind, key: &str) -> Option<&'static Field> {
    MATCHES.iter().find(|f| f.rule == rule && f.key == key)
}

/// An effect by kind and key.
pub fn effect(rule: RuleKind, key: &str) -> Option<&'static Field> {
    EFFECTS.iter().find(|f| f.rule == rule && f.key == key)
}

/// Read a value for a field from the UI.
pub fn parse_value(f: &Field, value: &Value) -> Result<LookValue> {
    look::parse_as(f.key, f.kind, (f.min, f.max), value)
}

fn default_of(f: &Field) -> LookValue {
    let json: Value = serde_json::from_str(f.default).expect("rule defaults are valid JSON");
    match parse_value(f, &json) {
        Ok(v) => v,
        // An empty text default: the page asks for it.
        Err(_) => LookValue::Text(String::new()),
    }
}

// ---------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------

/// One rule as the app holds it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    /// Stable id, also its key in the handle table.
    pub id: String,
    /// Window, layer or workspace.
    pub kind: RuleKind,
    /// The user's name for it, written as a comment above it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// A rule switched off is kept but not written.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// What it matches, by key. A workspace rule holds its workspace here.
    #[serde(default)]
    pub matches: BTreeMap<String, LookValue>,
    /// What it changes, by key.
    #[serde(default)]
    pub effects: BTreeMap<String, LookValue>,
    /// Fields of a rule moved in from the user's config that are not about
    /// looks — `float = true`, `size = { 200, 300 }` — as the Lua they were
    /// written as, kept so moving the rule changes nothing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<(String, String)>,
}

fn yes() -> bool {
    true
}

impl Rule {
    /// A new, empty rule.
    pub fn new(id: impl Into<String>, kind: RuleKind) -> Self {
        Rule {
            id: id.into(),
            kind,
            label: String::new(),
            enabled: true,
            matches: BTreeMap::new(),
            effects: BTreeMap::new(),
            extra: Vec::new(),
        }
    }

    /// The workspace a workspace rule is for.
    pub fn workspace(&self) -> Option<&str> {
        match (self.kind, self.matches.get("workspace")) {
            (RuleKind::Workspace, Some(LookValue::Text(w))) => Some(w),
            _ => None,
        }
    }

    /// Why Hyprland could not take it as it stands, if it could not.
    pub fn incomplete(&self) -> Option<&'static str> {
        let has =
            |k: &str| matches!(self.matches.get(k), Some(LookValue::Text(t)) if !t.is_empty());
        match self.kind {
            RuleKind::Window if self.matches.is_empty() => Some("Say which windows it is for"),
            RuleKind::Layer if !has("namespace") => Some("Say which layer it is for"),
            RuleKind::Workspace if !has("workspace") => Some("Say which workspace it is for"),
            _ if self.effects.is_empty() && self.extra.is_empty() => Some("It changes nothing yet"),
            _ => None,
        }
    }

    /// Whether it goes into the config.
    pub fn written(&self) -> bool {
        self.enabled && self.incomplete().is_none()
    }

    /// The call making it, on one line or laid out for the file.
    pub fn call(&self, pretty: bool) -> String {
        let mut fields: Vec<(String, String)> = Vec::new();
        let matches = ordered(self.kind, MATCHES, &self.matches);
        if self.kind == RuleKind::Workspace {
            for (k, v) in matches {
                fields.push((k, v.to_lua()));
            }
        } else {
            let inner: Vec<String> =
                matches.iter().map(|(k, v)| format!("{k} = {}", v.to_lua())).collect();
            fields.push(("match".into(), format!("{{ {} }}", inner.join(", "))));
        }
        for (k, v) in ordered(self.kind, EFFECTS, &self.effects) {
            fields.push((k.clone(), effect_lua(&k, v)));
        }
        fields.extend(self.extra.iter().cloned());

        if pretty {
            let mut out = format!("{}({{\n", self.kind.function());
            for (k, v) in fields {
                out.push_str(&format!("    {k} = {v},\n"));
            }
            out.push_str("})");
            out
        } else {
            let body: Vec<String> = fields.iter().map(|(k, v)| format!("{k} = {v}")).collect();
            format!("{}({{ {} }})", self.kind.function(), body.join(", "))
        }
    }

    /// A short description of what it matches, for its heading.
    pub fn target(&self) -> String {
        let mut parts = Vec::new();
        for f in MATCHES.iter().filter(|f| f.rule == self.kind) {
            match self.matches.get(f.key) {
                Some(LookValue::Text(t)) if !t.is_empty() => {
                    parts.push(if matches!(f.key, "class" | "namespace" | "workspace") {
                        t.clone()
                    } else {
                        format!("{} {t}", f.label.to_lowercase())
                    })
                }
                Some(LookValue::Bool(b)) => parts.push(match (f.key, b) {
                    ("float", true) => "floating".into(),
                    ("float", false) => "tiled".into(),
                    (_, true) => f.label.to_lowercase(),
                    (_, false) => format!("not {}", f.label.to_lowercase()),
                }),
                _ => {}
            }
        }
        if parts.is_empty() {
            match self.kind {
                RuleKind::Window => "every window".into(),
                _ => "nothing yet".into(),
            }
        } else {
            parts.join(", ")
        }
    }

    /// A short description of what it changes.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        for (k, v) in &self.effects {
            let label = effect(self.kind, k).map(|f| f.label).unwrap_or(k);
            parts.push(match v {
                LookValue::Bool(true) => label.to_string(),
                LookValue::Bool(false) => format!("{label} off"),
                LookValue::Vec2([a, i]) => {
                    format!("{label} {:.0}% / {:.0}%", a * 100.0, i * 100.0)
                }
                LookValue::Int(n) => format!("{label} {n}"),
                LookValue::Float(f) => format!("{label} {}", lua_number(*f)),
                LookValue::Text(t) => format!("{label} {t}"),
                LookValue::Sides(s) if s.iter().all(|x| *x == s[0]) => format!("{label} {}", s[0]),
                LookValue::Sides(s) => format!("{label} {} {} {} {}", s[0], s[1], s[2], s[3]),
                _ => label.to_string(),
            });
        }
        for (k, _) in &self.extra {
            parts.push(k.replace('_', " "));
        }
        parts.join(" · ")
    }

    /// The rule as the page edits it.
    pub fn view(&self) -> Value {
        let fields = |all: &[Field], have: &BTreeMap<String, LookValue>| -> Vec<Value> {
            let mut out: Vec<Value> = all
                .iter()
                .filter(|f| f.rule == self.kind)
                .filter_map(|f| have.get(f.key).map(|v| field_view(f, Some(v))))
                .collect();
            // Kept from a moved rule, but not something the page edits.
            out.extend(
                have.iter()
                    .filter(|(k, _)| !all.iter().any(|f| f.rule == self.kind && f.key == *k))
                    .map(|(k, v)| json!({ "key": k, "label": k, "kind": "fixed", "value": v.to_lua() })),
            );
            out
        };
        json!({
            "id": self.id,
            "kind": self.kind,
            "label": self.label,
            "enabled": self.enabled,
            "target": self.target(),
            "summary": self.summary(),
            "incomplete": self.incomplete(),
            "matches": fields(MATCHES, &self.matches),
            "effects": fields(EFFECTS, &self.effects),
            "extra": self.extra.iter().map(|(k, v)| json!({ "key": k, "value": v })).collect::<Vec<_>>(),
            "lua": self.call(true),
        })
    }
}

/// The fields of `have` in the catalog's order, then any it does not list.
fn ordered<'a>(
    kind: RuleKind,
    all: &[Field],
    have: &'a BTreeMap<String, LookValue>,
) -> Vec<(String, &'a LookValue)> {
    let known = |k: &str| all.iter().any(|f| f.rule == kind && f.key == k);
    let mut out: Vec<(String, &LookValue)> = all
        .iter()
        .filter(|f| f.rule == kind)
        .filter_map(|f| have.get(f.key).map(|v| (f.key.to_string(), v)))
        .collect();
    out.extend(have.iter().filter(|(k, _)| !known(k)).map(|(k, v)| (k.clone(), v)));
    out
}

/// An effect's value as Lua. Two take a string where the rest take the value
/// itself: opacity is `"0.9 override 0.8 override"`, so it replaces the
/// global opacity rather than multiplying it, and a border color is the
/// gradient spelled out the way a `.conf` would.
fn effect_lua(key: &str, v: &LookValue) -> String {
    match (key, v) {
        ("opacity", LookValue::Vec2([a, i])) => {
            lua_quote(&format!("{} override {} override", lua_number(*a), lua_number(*i)))
        }
        (_, LookValue::Gradient { colors, angle }) => {
            let mut s: Vec<String> = colors.iter().map(|c| hypr_color(c)).collect();
            if colors.len() > 1 {
                s.push(format!("{}deg", lua_number(*angle)));
            }
            lua_quote(&s.join(" "))
        }
        _ => v.to_lua(),
    }
}

fn field_view(f: &Field, value: Option<&LookValue>) -> Value {
    json!({
        "key": f.key,
        "label": f.label,
        "hint": f.hint,
        "kind": f.kind,
        "min": f.min,
        "max": f.max,
        "step": f.step,
        "value": value.cloned().unwrap_or_else(|| default_of(f)).to_json(),
        "parts": if f.key == "opacity" { json!(["Focused", "Unfocused"]) } else { json!([]) },
        "overridden": true,
    })
}

/// Every field the page can offer, by kind.
pub fn catalog() -> Value {
    static CATALOG: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    CATALOG.get_or_init(build_catalog).clone()
}

fn build_catalog() -> Value {
    let list = |all: &[Field], rule: RuleKind| -> Vec<Value> {
        all.iter().filter(|f| f.rule == rule).map(|f| field_view(f, None)).collect()
    };
    let mut out = serde_json::Map::new();
    for k in RuleKind::ALL {
        out.insert(
            serde_json::to_value(k).unwrap().as_str().unwrap().to_string(),
            json!({ "matches": list(MATCHES, k), "effects": list(EFFECTS, k) }),
        );
    }
    Value::Object(out)
}

fn is_ident(k: &str) -> bool {
    let mut chars = k.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !matches!(k, "and" | "or" | "not" | "nil" | "true" | "false" | "function" | "end")
}

/// A rule from a file — a theme, or the look block's state line — checked
/// before any of it reaches Lua. Every key has to be a plain name and every
/// value kept as written has to be a literal, which is then written back in
/// the app's own spelling; anything else would let a shared theme run code.
pub fn checked(rule: &Rule) -> Result<Rule> {
    let bad = |what: &str| Error::other(format!("a rule in it has {what}"));
    let mut out = rule.clone();
    for k in rule.matches.keys().chain(rule.effects.keys()) {
        if !is_ident(k) {
            return Err(bad(&format!("the field {k:?}, which is not a name")));
        }
    }
    for (k, v) in &rule.effects {
        if let Some(f) = effect(rule.kind, k) {
            out.effects.insert(k.clone(), parse_value(f, &v.to_json())?);
        }
    }
    out.extra.clear();
    for (k, v) in &rule.extra {
        if !is_ident(k) || matches!(k.as_str(), "match" | "name") {
            return Err(bad(&format!("the field {k:?}, which is not a name")));
        }
        let lit = lua_table(&format!("x = {v}"))
            .and_then(|t| t.into_iter().next())
            .map(|(_, lit)| lit)
            .ok_or_else(|| bad(&format!("`{k} = {v}`, which is not a plain value")))?;
        out.extra.push((k.clone(), lit.lua()));
    }
    Ok(out)
}

/// Apply a change from the page to a copy of `rule`: `label`, `enabled`,
/// and `matches` / `effects` objects whose `null` values take a field away.
pub fn edited(rule: &Rule, payload: &Value) -> Result<Rule> {
    let mut out = rule.clone();
    if let Some(l) = payload.get("label").and_then(Value::as_str) {
        out.label = l.trim().replace(['\n', '\r'], " ");
    }
    if let Some(e) = payload.get("enabled").and_then(Value::as_bool) {
        out.enabled = e;
    }
    if let Some(m) = payload.get("matches").and_then(Value::as_object) {
        for (k, v) in m {
            let f = match_field(rule.kind, k).ok_or_else(|| {
                Error::other(format!("a {:?} rule cannot match on {k}", rule.kind))
            })?;
            let empty_text =
                f.kind == Kind::Text && v.as_str().is_some_and(|s| s.trim().is_empty());
            if v.is_null() || (empty_text && rule.kind == RuleKind::Window) {
                out.matches.remove(k);
            } else if empty_text {
                out.matches.insert(k.clone(), LookValue::Text(String::new()));
            } else {
                out.matches.insert(k.clone(), parse_value(f, v)?);
            }
        }
    }
    if let Some(e) = payload.get("effects").and_then(Value::as_object) {
        for (k, v) in e {
            if v.is_null() {
                out.effects.remove(k);
                out.extra.retain(|(x, _)| x != k);
                continue;
            }
            let f = effect(rule.kind, k).ok_or_else(|| {
                Error::other(format!("{k} is not something this rule can change"))
            })?;
            // `true` alone means "add it with its usual starting value".
            let v =
                if v == &Value::Bool(true) && f.kind != Kind::Bool && !rule.effects.contains_key(k)
                {
                    default_of(f)
                } else {
                    parse_value(f, v)?
                };
            out.effects.insert(k.clone(), v);
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Lua for the file and for the live desktop
// ---------------------------------------------------------------------------

fn comment(label: &str) -> String {
    if label.is_empty() {
        String::new()
    } else {
        format!("-- {}\n", label.replace(['\n', '\r'], " "))
    }
}

/// The rules as they go into the look block: a fresh handle table, then each
/// rule that is on and complete, in order.
pub fn block_lua(rules: &[Rule]) -> String {
    let written: Vec<&Rule> = rules.iter().filter(|r| r.written()).collect();
    if written.is_empty() {
        return String::new();
    }
    let mut out = format!(
        "-- Look rules. Each handle is kept so the app can change the rule live.\n{HANDLES} = {{}}\n"
    );
    for r in written {
        out.push('\n');
        out.push_str(&comment(&r.label));
        out.push_str(&format!("{HANDLES}[{}] = {}\n", lua_quote(&r.id), r.call(true)));
    }
    out
}

fn switch_off(id: &str) -> String {
    let at = format!("{HANDLES}[{}]", lua_quote(id));
    format!("if {at} then {at}:set_enabled(false) end {at} = nil")
}

/// Lua putting one rule on the desktop as it now stands, replacing whatever
/// the app had there under its id. `None` just takes it away. `theirs` is
/// the user's own workspace rules to run again first — see the module notes.
pub fn live_one(id: &str, rule: Option<&Rule>, theirs: &[String]) -> String {
    let mut out = format!("{HANDLES} = {HANDLES} or {{}} {}", switch_off(id));
    for call in theirs {
        out.push_str(&format!(" {call}"));
    }
    if let Some(r) = rule.filter(|r| r.written()) {
        out.push_str(&format!(" {HANDLES}[{}] = {}", lua_quote(&r.id), r.call(false)));
    }
    out
}

/// Lua making the desktop's app rules exactly `rules`, whatever it had.
pub fn live_all(rules: &[Rule], theirs: &[String]) -> String {
    let mut out = format!(
        "{HANDLES} = {HANDLES} or {{}} for _, r in pairs({HANDLES}) do r:set_enabled(false) end \
         {HANDLES} = {{}}"
    );
    for call in theirs {
        out.push_str(&format!(" {call}"));
    }
    for r in rules.iter().filter(|r| r.written()) {
        out.push_str(&format!(" {HANDLES}[{}] = {}", lua_quote(&r.id), r.call(false)));
    }
    out
}

// ---------------------------------------------------------------------------
// The user's own rules
// ---------------------------------------------------------------------------

/// A rule call written by hand in the config, outside the app's blocks.
#[derive(Debug, Clone, PartialEq)]
pub struct HandRule {
    /// Which kind of call.
    pub kind: RuleKind,
    /// The call exactly as it is in the file.
    pub text: String,
    /// The same call with any comments in it blanked, safe to run again as
    /// part of a longer chunk.
    pub code: String,
    /// Its `name`, if it has a plain one.
    pub name: String,
    /// The call read as data, when every value in it is a literal.
    pub rule: Option<Rule>,
    /// Whether it changes anything about the look.
    pub look: bool,
    /// Whether it stands alone on its lines, so it can be commented out.
    pub alone: bool,
}

impl HandRule {
    /// Whether "Move here" can take it.
    pub fn movable(&self) -> bool {
        self.rule.is_some() && self.alone
    }

    /// The rule as the page lists it.
    pub fn view(&self, moving: bool) -> Value {
        let (target, summary) = match &self.rule {
            Some(r) => (r.target(), r.summary()),
            None => (String::new(), String::new()),
        };
        json!({
            "kind": self.kind,
            "name": self.name,
            "text": self.text,
            "target": target,
            "summary": summary,
            "movable": self.movable(),
            "why": if self.rule.is_none() {
                "It is built from variables or expressions, which the app does not read."
            } else if !self.alone {
                "It shares its line with other code, so it cannot be commented out cleanly."
            } else {
                ""
            },
            "moving": moving,
        })
    }
}

/// Every rule call in `document` outside the app's own blocks, comments
/// skipped.
pub fn in_config(document: &str) -> Vec<HandRule> {
    let outside = outside_blocks(document);
    let code = crate::import::strip_comments(&outside);
    let mut out = Vec::new();
    for kind in RuleKind::ALL {
        let name = kind.function();
        for (at, end) in crate::import::call_spans(&code, name) {
            let text = outside[at..end].to_string();
            let call_code = code[at..end].to_string();
            let open = code[at..end].find('{').map(|i| at + i);
            let Some(open) = open else { continue };
            let body = &code[open + 1..code[..end].rfind('}').unwrap_or(end)];
            let table = lua_table(body);
            let rule = table.as_ref().and_then(|t| rule_from(kind, t));
            let name = table
                .as_ref()
                .and_then(|t| t.iter().find(|(k, _)| k.as_deref() == Some("name")))
                .and_then(|(_, v)| match v {
                    Lit::Str(s) => Some(s.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            let look = match &table {
                Some(t) => {
                    t.iter().any(|(k, _)| k.as_deref().is_some_and(|k| effect(kind, k).is_some()))
                }
                // Unreadable: look for an effect's name in the text.
                None => EFFECTS.iter().filter(|f| f.rule == kind).any(|f| {
                    body.contains(&format!("{} ", f.key)) || body.contains(&format!("{}=", f.key))
                }),
            };
            let line_start = code[..at].rfind('\n').map_or(0, |i| i + 1);
            let line_end = code[end..].find('\n').map_or(code.len(), |i| end + i);
            let alone =
                code[line_start..at].trim().is_empty() && code[end..line_end].trim().is_empty();
            out.push(HandRule { kind, text, code: call_code, name, rule, look, alone });
        }
    }
    out
}

/// `document` with the app's two blocks blanked out, byte for byte, so
/// offsets still line up.
fn outside_blocks(document: &str) -> String {
    let mut out = document.to_string();
    for markers in [crate::block::SHADE, look::MARKERS] {
        if let Some(begin) = out.find(markers.begin) {
            if let Some(rel) = out[begin..].find(markers.end) {
                let end = begin + rel + markers.end.len();
                // Byte for byte, so offsets past the block still line up
                // when it holds multi-byte characters.
                let blank: String = out.as_bytes()[begin..end]
                    .iter()
                    .map(|&b| if b == b'\n' { '\n' } else { ' ' })
                    .collect();
                out.replace_range(begin..end, &blank);
            }
        }
    }
    out
}

/// The user's own workspace rules for any of `workspaces`, as their calls
/// with comments blanked — a `--` would otherwise swallow whatever follows
/// it in the chunk they are run in. Line breaks are kept as they are.
pub fn theirs_for<'a>(
    hand: impl IntoIterator<Item = &'a HandRule>,
    workspaces: &[&str],
) -> Vec<String> {
    hand.into_iter()
        .filter(|h| h.kind == RuleKind::Workspace)
        .filter(|h| {
            h.rule.as_ref().and_then(Rule::workspace).is_some_and(|w| workspaces.contains(&w))
        })
        .map(|h| h.code.trim().to_string())
        .collect()
}

/// Comment out the call `text` in `document`, where it stands alone outside
/// the app's blocks, with a line saying where it went. `None` when it is no
/// longer there as it was.
pub fn comment_out(document: &str, text: &str) -> Option<String> {
    let outside = outside_blocks(document);
    let code = crate::import::strip_comments(&outside);
    let mut from = 0;
    while let Some(rel) = document[from..].find(text) {
        let at = from + rel;
        let end = at + text.len();
        from = end;
        // Live code, not a comment or a block.
        if code.get(at..at + 3) != Some("hl.") {
            continue;
        }
        let line_start = document[..at].rfind('\n').map_or(0, |i| i + 1);
        let line_end = document[end..].find('\n').map_or(document.len(), |i| end + i);
        if !code[line_start..at].trim().is_empty() || !code[end..line_end].trim().is_empty() {
            continue;
        }
        let indent = &document[line_start..at];
        let mut out = String::with_capacity(document.len() + 200);
        out.push_str(&document[..line_start]);
        out.push_str(&format!("{indent}-- Moved into the HyprCustard look block:\n"));
        for line in document[line_start..line_end].split('\n') {
            out.push_str(&format!("-- {line}\n"));
        }
        if line_end < document.len() {
            out.push_str(&document[line_end + 1..]);
        }
        return Some(out);
    }
    None
}

// ---------------------------------------------------------------------------
// Reading a Lua table literal
// ---------------------------------------------------------------------------

/// A literal value in a Lua table.
#[derive(Debug, Clone, PartialEq)]
enum Lit {
    Str(String),
    Num(f64, bool),
    Bool(bool),
    Table(Vec<(Option<String>, Lit)>),
}

impl Lit {
    fn lua(&self) -> String {
        match self {
            Lit::Str(s) => lua_quote(s),
            Lit::Num(n, true) => format!("{}", *n as i64),
            Lit::Num(n, false) => lua_number(*n),
            Lit::Bool(b) => b.to_string(),
            Lit::Table(items) => {
                let parts: Vec<String> = items
                    .iter()
                    .map(|(k, v)| match k {
                        Some(k) => format!("{k} = {}", v.lua()),
                        None => v.lua(),
                    })
                    .collect();
                format!("{{ {} }}", parts.join(", "))
            }
        }
    }
}

/// A table's contents, when everything in it is a literal.
fn lua_table(body: &str) -> Option<Vec<(Option<String>, Lit)>> {
    let mut p = Parser { s: body.as_bytes(), i: 0 };
    let items = p.items(None)?;
    p.ws();
    (p.i == p.s.len()).then_some(items)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn items(&mut self, close: Option<u8>) -> Option<Vec<(Option<String>, Lit)>> {
        let mut out = Vec::new();
        loop {
            self.ws();
            match self.s.get(self.i) {
                None if close.is_none() => return Some(out),
                Some(c) if Some(*c) == close => {
                    self.i += 1;
                    return Some(out);
                }
                None => return None,
                _ => {}
            }
            // `key =` or a positional value.
            let save = self.i;
            let mut key = None;
            if let Some(name) = self.ident() {
                self.ws();
                if self.s.get(self.i) == Some(&b'=') && self.s.get(self.i + 1) != Some(&b'=') {
                    self.i += 1;
                    key = Some(name);
                } else {
                    self.i = save;
                }
            }
            let v = self.value()?;
            out.push((key, v));
            self.ws();
            match self.s.get(self.i) {
                Some(b',' | b';') => self.i += 1,
                Some(c) if Some(*c) == close => {}
                None if close.is_none() => {}
                _ => return None,
            }
        }
    }

    fn ident(&mut self) -> Option<String> {
        let start = self.i;
        while self.i < self.s.len()
            && (self.s[self.i].is_ascii_alphanumeric() || self.s[self.i] == b'_')
        {
            self.i += 1;
        }
        let word = std::str::from_utf8(&self.s[start..self.i]).ok()?;
        if word.is_empty() || word.as_bytes()[0].is_ascii_digit() {
            self.i = start;
            return None;
        }
        Some(word.to_string())
    }

    fn value(&mut self) -> Option<Lit> {
        self.ws();
        let c = *self.s.get(self.i)?;
        match c {
            b'"' | b'\'' => {
                self.i += 1;
                let mut bytes = Vec::new();
                while let Some(&b) = self.s.get(self.i) {
                    self.i += 1;
                    match b {
                        b'\\' => {
                            let e = *self.s.get(self.i)?;
                            self.i += 1;
                            bytes.push(match e {
                                b'n' => b'\n',
                                b't' => b'\t',
                                b'r' => b'\r',
                                b'\\' | b'"' | b'\'' => e,
                                _ => return None,
                            });
                        }
                        b'\n' => return None,
                        b if b == c => {
                            let lit = Lit::Str(String::from_utf8(bytes).ok()?);
                            // `"a" .. b` is an expression, not a literal.
                            self.ws();
                            if self.s.get(self.i) == Some(&b'.') {
                                return None;
                            }
                            return Some(lit);
                        }
                        b => bytes.push(b),
                    }
                }
                None
            }
            b'{' => {
                self.i += 1;
                Some(Lit::Table(self.items(Some(b'}'))?))
            }
            b'-' | b'0'..=b'9' | b'.' => {
                let start = self.i;
                self.i += 1;
                while self.i < self.s.len()
                    && (self.s[self.i].is_ascii_alphanumeric() || self.s[self.i] == b'.')
                {
                    self.i += 1;
                }
                let t = std::str::from_utf8(&self.s[start..self.i]).ok()?;
                let n: f64 = t.parse().ok()?;
                let after = self.i;
                self.ws();
                if matches!(self.s.get(self.i), Some(b'+' | b'-' | b'*' | b'/' | b'.')) {
                    return None;
                }
                self.i = after;
                Some(Lit::Num(n, !t.contains(['.', 'e', 'E'])))
            }
            _ => match self.ident()?.as_str() {
                "true" => Some(Lit::Bool(true)),
                "false" => Some(Lit::Bool(false)),
                _ => None,
            },
        }
    }
}

/// A literal table as the JSON a field's parser takes.
fn lit_json(v: &Lit) -> Value {
    match v {
        Lit::Str(s) => json!(s),
        Lit::Num(n, true) => json!(*n as i64),
        Lit::Num(n, false) => json!(n),
        Lit::Bool(b) => json!(b),
        Lit::Table(items) => {
            if items.iter().all(|(k, _)| k.is_none()) {
                Value::Array(items.iter().map(|(_, v)| lit_json(v)).collect())
            } else {
                let order = ["top", "right", "bottom", "left"];
                if items.iter().all(|(k, _)| k.as_deref().is_some_and(|k| order.contains(&k))) {
                    // Named sides, in the order the parser takes them.
                    let get = |side: &str| {
                        items
                            .iter()
                            .find(|(k, _)| k.as_deref() == Some(side))
                            .map(|(_, v)| lit_json(v))
                    };
                    return json!(order
                        .iter()
                        .map(|s| get(s).unwrap_or(json!(0)))
                        .collect::<Vec<_>>());
                }
                Value::Object(
                    items.iter().filter_map(|(k, v)| Some((k.clone()?, lit_json(v)))).collect(),
                )
            }
        }
    }
}

/// An effect read back from how it is written, or `None` when the app would
/// not write it the same way — that one is kept as it was instead.
fn effect_from(f: &Field, v: &Lit) -> Option<LookValue> {
    match (f.key, v) {
        ("opacity", Lit::Str(s)) => {
            // Only the fully overriding form is the app's own.
            let words: Vec<&str> = s.split_whitespace().collect();
            match words[..] {
                [a, "override", i, "override"] => {
                    Some(LookValue::Vec2([a.parse().ok()?, i.parse().ok()?]))
                }
                _ => None,
            }
        }
        // Only the two forms the app writes: one color, or colors and then
        // an angle. Two colors with no angle mean focused and unfocused to
        // Hyprland, which one gradient for both would not be.
        (_, Lit::Str(s)) if f.kind == Kind::Gradient => {
            let words: Vec<&str> = s.split_whitespace().collect();
            let (angle, colors) = match words.split_last() {
                Some((one, [])) => (0.0, vec![*one]),
                Some((last, rest)) => (last.strip_suffix("deg")?.parse().ok()?, rest.to_vec()),
                None => return None,
            };
            if colors.len() > 1 && words.len() == colors.len() {
                return None;
            }
            let colors =
                colors.iter().map(|c| look::parse_color(c).ok()).collect::<Option<Vec<_>>>()?;
            parse_value(f, &json!({ "colors": colors, "angle": angle })).ok()
        }
        _ => {
            // Read once as written and once into the slider's range: a value
            // the range would clamp is not the same rule any more.
            let json = lit_json(v);
            let got = parse_value(f, &json).ok()?;
            let unclamped = look::parse_as(f.key, f.kind, (0.0, 0.0), &json).ok()?;
            got.same_as(&unclamped).then_some(got)
        }
    }
}

/// A hand-written rule's table as a [`Rule`].
fn rule_from(kind: RuleKind, table: &[(Option<String>, Lit)]) -> Option<Rule> {
    let mut rule = Rule::new("", kind);
    for (k, v) in table {
        let k = k.as_deref()?;
        match (k, v) {
            ("name", Lit::Str(s)) => rule.label = s.clone(),
            ("enabled", Lit::Bool(b)) => rule.enabled = *b,
            ("match", Lit::Table(items)) if kind != RuleKind::Workspace => {
                for (mk, mv) in items {
                    let mk = mk.as_deref()?;
                    let value = match mv {
                        Lit::Str(s) => LookValue::Text(s.clone()),
                        Lit::Bool(b) => LookValue::Bool(*b),
                        Lit::Num(n, _) => LookValue::Text(lua_number(*n)),
                        Lit::Table(_) => return None,
                    };
                    rule.matches.insert(mk.to_string(), value);
                }
            }
            ("workspace", Lit::Str(s)) if kind == RuleKind::Workspace => {
                rule.matches.insert("workspace".into(), LookValue::Text(s.clone()));
            }
            ("workspace", Lit::Num(n, _)) if kind == RuleKind::Workspace => {
                rule.matches.insert("workspace".into(), LookValue::Text(lua_number(*n)));
            }
            _ => match effect(kind, k).and_then(|f| effect_from(f, v)) {
                Some(value) => {
                    rule.effects.insert(k.to_string(), value);
                }
                None => rule.extra.push((k.to_string(), v.lua())),
            },
        }
    }
    Some(rule)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(kind: RuleKind, matches: &[(&str, LookValue)], effects: &[(&str, LookValue)]) -> Rule {
        let mut r = Rule::new("r1", kind);
        for (k, v) in matches {
            r.matches.insert(k.to_string(), v.clone());
        }
        for (k, v) in effects {
            r.effects.insert(k.to_string(), v.clone());
        }
        r
    }

    #[test]
    fn every_default_parses() {
        for f in MATCHES.iter().chain(EFFECTS) {
            let _ = default_of(f);
        }
        for f in EFFECTS {
            assert!(parse_value(f, &serde_json::from_str(f.default).unwrap()).is_ok(), "{}", f.key);
        }
    }

    #[test]
    fn fields_are_unique_per_kind() {
        for all in [MATCHES, EFFECTS] {
            let mut keys: Vec<(RuleKind, &str)> = all.iter().map(|f| (f.rule, f.key)).collect();
            let n = keys.len();
            keys.sort_by_key(|(k, s)| (format!("{k:?}"), *s));
            keys.dedup();
            assert_eq!(keys.len(), n);
        }
    }

    #[test]
    fn a_window_rule_is_written_the_way_hyprland_takes_it() {
        let r = rule(
            RuleKind::Window,
            &[("class", LookValue::Text("kitty".into())), ("float", LookValue::Bool(false))],
            &[
                ("opacity", LookValue::Vec2([0.9, 0.8])),
                ("no_shadow", LookValue::Bool(true)),
                (
                    "border_color",
                    LookValue::Gradient {
                        colors: vec!["#ffff0000".into(), "#8000ff00".into()],
                        angle: 45.0,
                    },
                ),
            ],
        );
        assert_eq!(
            r.call(false),
            "hl.window_rule({ match = { class = \"kitty\", float = false }, \
             opacity = \"0.9 override 0.8 override\", \
             border_color = \"rgba(ff0000ff) rgba(00ff0080) 45deg\", no_shadow = true })"
        );
    }

    #[test]
    fn workspace_gaps_are_written_with_named_sides() {
        let r = rule(
            RuleKind::Workspace,
            &[("workspace", LookValue::Text("w[tv1]".into()))],
            &[("gaps_out", LookValue::Sides([0, 0, 0, 0])), ("no_border", LookValue::Bool(true))],
        );
        assert_eq!(
            r.call(false),
            "hl.workspace_rule({ workspace = \"w[tv1]\", \
             gaps_out = { top = 0, right = 0, bottom = 0, left = 0 }, no_border = true })"
        );
    }

    #[test]
    fn incomplete_and_disabled_rules_are_not_written() {
        let mut r = rule(RuleKind::Layer, &[], &[("blur", LookValue::Bool(true))]);
        assert!(r.incomplete().is_some());
        assert_eq!(block_lua(std::slice::from_ref(&r)), "");
        r.matches.insert("namespace".into(), LookValue::Text("waybar".into()));
        assert!(block_lua(std::slice::from_ref(&r)).contains("hl.layer_rule"));
        r.enabled = false;
        assert_eq!(block_lua(std::slice::from_ref(&r)), "");
        let empty = rule(RuleKind::Window, &[("class", LookValue::Text("k".into()))], &[]);
        assert_eq!(empty.incomplete(), Some("It changes nothing yet"));
    }

    #[test]
    fn the_block_keeps_a_handle_for_each_rule() {
        let mut r = rule(
            RuleKind::Window,
            &[("class", LookValue::Text("kitty".into()))],
            &[("rounding", LookValue::Int(4))],
        );
        r.label = "Terminal\nlook".into();
        let lua = block_lua(&[r]);
        assert!(lua.contains("hyprcustard_rules = {}"));
        assert!(lua.contains("-- Terminal look\nhyprcustard_rules[\"r1\"] = hl.window_rule({\n"));
        assert!(lua.contains("    rounding = 4,\n"));
        // No name: same-named rules merge in Hyprland.
        assert!(!lua.contains("name ="));
    }

    #[test]
    fn a_live_edit_switches_the_old_rule_off_first() {
        let r = rule(
            RuleKind::Window,
            &[("class", LookValue::Text("kitty".into()))],
            &[("rounding", LookValue::Int(4))],
        );
        let lua = live_one("r1", Some(&r), &[]);
        let off = lua.find("set_enabled(false)").unwrap();
        let on = lua.find("hl.window_rule").unwrap();
        assert!(off < on);
        assert!(!live_one("r1", None, &[]).contains("hl.window_rule"));
        let theirs = vec!["hl.workspace_rule({ workspace = \"1\" })".to_string()];
        let lua = live_all(&[r], &theirs);
        assert!(lua.find("workspace_rule").unwrap() < lua.find("window_rule").unwrap());
    }

    const CONFIG: &str = r#"
-- hl.workspace_rule({ workspace = "w[tv1]", gaps_out = 0, gaps_in = 0 })
hl.window_rule({
    name  = "suppress-maximize-events",
    match = { class = ".*" },

    suppress_event = "maximize",
})

hl.window_rule({
    name      = "tiled-no-shadow",
    match     = { float = false },
    no_shadow = true,
})

hl.window_rule({
    name  = "octopi-rule",
    match = { class = "octopi", title = "Confirmation" },
    float = true,
    size  = { 200, 300 },
})

hl.layer_rule({
    name       = "waybar layer",
    match      = { namespace = "waybar" },
    blur       = true,
    ignore_alpha = 0.79,
})

hl.workspace_rule({ workspace = "w[tv1]", gaps_out = 0, gaps_in = { top = 1, right = 2, bottom = 3, left = 4 } })
local r = hl.window_rule({ match = { class = mainMod }, rounding = 3 })
hl.window_rule({ match = { class = "x" .. y }, no_blur = true })
"#;

    #[test]
    fn hand_written_rules_are_read() {
        let found = in_config(CONFIG);
        assert_eq!(found.len(), 7, "{found:#?}");
        let by_name = |n: &str| found.iter().find(|h| h.name == n).unwrap();

        let tiled = by_name("tiled-no-shadow");
        assert!(tiled.look && tiled.movable());
        let r = tiled.rule.as_ref().unwrap();
        assert_eq!(r.matches["float"], LookValue::Bool(false));
        assert_eq!(r.effects["no_shadow"], LookValue::Bool(true));
        assert_eq!(r.label, "tiled-no-shadow");

        assert!(!by_name("suppress-maximize-events").look);
        let octopi = by_name("octopi-rule").rule.clone().unwrap();
        assert_eq!(
            octopi.extra,
            vec![
                ("float".to_string(), "true".to_string()),
                ("size".to_string(), "{ 200, 300 }".to_string())
            ]
        );

        let bar = by_name("waybar layer").rule.clone().unwrap();
        assert_eq!(bar.effects["ignore_alpha"], LookValue::Float(0.79));

        let ws = found.iter().find(|h| h.kind == RuleKind::Workspace).unwrap();
        let wr = ws.rule.as_ref().unwrap();
        assert_eq!(wr.workspace(), Some("w[tv1]"));
        assert_eq!(wr.effects["gaps_in"], LookValue::Sides([1, 2, 3, 4]));
        assert_eq!(wr.effects["gaps_out"], LookValue::Sides([0, 0, 0, 0]));

        // `local r = ...` cannot be commented out; a variable cannot be read.
        let assigned = found.iter().find(|h| h.text.contains("mainMod")).unwrap();
        assert!(assigned.rule.is_none() && !assigned.alone && !assigned.movable());
        let concat = found.iter().find(|h| h.text.contains("..")).unwrap();
        assert!(concat.rule.is_none() && concat.look);
    }

    #[test]
    fn a_moved_rule_writes_the_same_fields_back() {
        let found = in_config(CONFIG);
        let octopi = found.iter().find(|h| h.name == "octopi-rule").unwrap();
        let mut r = octopi.rule.clone().unwrap();
        r.label.clear();
        assert_eq!(
            r.call(false),
            "hl.window_rule({ match = { class = \"octopi\", title = \"Confirmation\" }, \
             float = true, size = { 200, 300 } })"
        );
    }

    #[test]
    fn rules_inside_the_apps_blocks_are_not_hand_written() {
        let doc = format!(
            "{}\n{}\nhl.window_rule({{ match = {{ class = \"a\" }}, no_blur = true }})\n{}\n",
            "hl.window_rule({ match = { class = \"b\" }, no_blur = true })",
            look::MARKERS.begin,
            look::MARKERS.end
        );
        let found = in_config(&doc);
        assert_eq!(found.len(), 1);
        assert!(found[0].text.contains("\"b\""));
    }

    #[test]
    fn commenting_out_a_moved_rule() {
        let found = in_config(CONFIG);
        let tiled = found.iter().find(|h| h.name == "tiled-no-shadow").unwrap();
        let out = comment_out(CONFIG, &tiled.text).unwrap();
        assert!(out.contains(
            "-- Moved into the HyprCustard look block:\n-- hl.window_rule({\n--     name      = \"tiled-no-shadow\",\n"
        ));
        assert!(in_config(&out).iter().all(|h| h.name != "tiled-no-shadow"));
        // Everything else is untouched.
        assert_eq!(
            out.len(),
            CONFIG.len() + "-- Moved into the HyprCustard look block:\n".len() + 3 * 5
        );
        // Gone already: nothing to do.
        assert!(comment_out(&out, &tiled.text).is_none());
    }

    #[test]
    fn the_users_workspace_rules_for_a_workspace_are_found() {
        let found = in_config(CONFIG);
        let theirs = theirs_for(&found, &["w[tv1]"]);
        assert_eq!(theirs.len(), 1);
        assert!(theirs[0].starts_with("hl.workspace_rule({ workspace = \"w[tv1]\""));
        assert!(theirs_for(&found, &["f[1]"]).is_empty());
    }

    #[test]
    fn edits_are_checked_and_null_takes_a_field_away() {
        let r = rule(RuleKind::Window, &[("class", LookValue::Text("kitty".into()))], &[]);
        let r2 = edited(&r, &json!({ "effects": { "opacity": true, "rounding": 99 } })).unwrap();
        assert_eq!(r2.effects["opacity"], LookValue::Vec2([0.9, 0.8]));
        assert_eq!(r2.effects["rounding"], LookValue::Int(40));
        let r3 =
            edited(&r2, &json!({ "effects": { "opacity": null }, "matches": { "class": "" } }))
                .unwrap();
        assert!(!r3.effects.contains_key("opacity"));
        assert!(r3.matches.is_empty());
        assert!(edited(&r, &json!({ "effects": { "gaps_out": 1 } })).is_err());
        assert!(edited(&r, &json!({ "matches": { "namespace": "x" } })).is_err());
        assert!(edited(&r, &json!({ "effects": { "no_shadow": "yes" } })).is_err());
    }

    fn moved(src: &str) -> Rule {
        in_config(src).remove(0).rule.unwrap()
    }

    #[test]
    fn a_moved_rule_keeps_values_the_page_would_clamp() {
        let r = moved(r#"hl.workspace_rule({ workspace = "2", gaps_out = 150 })"#);
        assert!(r.effects.is_empty());
        assert_eq!(r.extra, vec![("gaps_out".to_string(), "150".to_string())]);
        let r = moved(r#"hl.window_rule({ match = { class = "k" }, rounding_power = 15 })"#);
        assert_eq!(r.extra[0].0, "rounding_power");
        // In range, and a whole number where a fraction is allowed: kept as an effect.
        let r = moved(r#"hl.layer_rule({ match = { namespace = "w" }, ignore_alpha = 1 })"#);
        assert_eq!(r.effects["ignore_alpha"], LookValue::Float(1.0));
    }

    #[test]
    fn a_moved_border_color_keeps_its_meaning() {
        // Focused and unfocused colors, not a gradient.
        let r = moved(
            r#"hl.window_rule({ match = { class = "k" }, border_color = "rgb(ff0000) rgb(880808)" })"#,
        );
        assert!(r.effects.is_empty() && r.extra[0].0 == "border_color");
        let r = moved(
            r#"hl.window_rule({ match = { class = "k" }, border_color = "rgb(ff0000) rgb(00ff00) 45deg rgb(111111) 0deg" })"#,
        );
        assert!(r.effects.is_empty());
        let r = moved(
            r#"hl.window_rule({ match = { class = "k" }, border_color = "rgb(ff0000) rgb(00ff00) 45deg" })"#,
        );
        assert_eq!(
            effect_lua("border_color", &r.effects["border_color"]),
            "\"rgba(ff0000ff) rgba(00ff00ff) 45deg\""
        );
        let r =
            moved(r#"hl.window_rule({ match = { class = "k" }, border_color = "rgb(ff0000)" })"#);
        assert_eq!(effect_lua("border_color", &r.effects["border_color"]), "\"rgba(ff0000ff)\"");
    }

    #[test]
    fn a_comment_in_the_users_workspace_rule_does_not_reach_the_live_chunk() {
        let src = "hl.workspace_rule({ workspace = \"1\", -- big gaps\n    gaps_out = 20 })\n";
        let theirs = theirs_for(&in_config(src), &["1"]);
        assert_eq!(theirs.len(), 1);
        assert!(!theirs[0].contains("--"), "{}", theirs[0]);
        assert!(theirs[0].contains("gaps_out = 20"));
        // Still the whole call, ready to run.
        assert!(lua_table(
            &theirs[0][theirs[0].find('{').unwrap() + 1..theirs[0].rfind('}').unwrap()]
        )
        .is_some());
    }

    #[test]
    fn multi_byte_text_in_a_block_does_not_shift_the_users_rules() {
        let doc = format!(
            "{}\n-- Café — dim\nhyprcustard_rules[\"lr1\"] = hl.window_rule({{ match = {{ class = \"é\" }}, no_blur = true }})\n{}\nhl.window_rule({{ match = {{ class = \"b\" }}, no_blur = true }})\n",
            look::MARKERS.begin,
            look::MARKERS.end
        );
        let found = in_config(&doc);
        assert_eq!(found.len(), 1);
        assert!(found[0].alone && found[0].text.contains("\"b\""));
        let out = comment_out(&doc, &found[0].text).unwrap();
        assert!(out.ends_with("-- hl.window_rule({ match = { class = \"b\" }, no_blur = true })\n"));
    }

    #[test]
    fn rules_from_a_file_cannot_carry_code() {
        let mut r = Rule::new("lr1", RuleKind::Window);
        r.matches.insert("class".into(), LookValue::Text("k".into()));
        r.extra.push(("float".into(), "os.execute('touch /tmp/x') or true".into()));
        assert!(checked(&r).is_err());

        let mut r2 = r.clone();
        r2.extra = vec![("x = os.execute('y'), z".into(), "true".into())];
        assert!(checked(&r2).is_err());

        let mut r3 = r.clone();
        r3.extra.clear();
        r3.effects.insert("no_blur = os.execute('y') or true, z".into(), LookValue::Bool(true));
        assert!(checked(&r3).is_err());

        let mut ok = r.clone();
        ok.extra = vec![("size".into(), "{200,300}".into()), ("float".into(), "true".into())];
        ok.effects.insert("rounding".into(), LookValue::Int(99));
        let c = checked(&ok).unwrap();
        assert_eq!(c.extra[0], ("size".to_string(), "{ 200, 300 }".to_string()));
        assert_eq!(c.effects["rounding"], LookValue::Int(40));
    }

    #[test]
    fn rules_round_trip_through_json() {
        let r = rule(
            RuleKind::Workspace,
            &[("workspace", LookValue::Text("f[1]".into()))],
            &[("gaps_in", LookValue::Sides([0, 0, 0, 0]))],
        );
        let back: Rule = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(back, r);
    }
}
