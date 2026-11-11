//! Hyprland's animations: the tree of animation leaves, and the easing curves
//! they run on.
//!
//! Like the rest of the look, only what is changed in the app is written —
//! `hl.curve(...)` and `hl.animation({...})` calls in the look block, after its
//! `hl.config`. A leaf not set anywhere inherits from its parent, which is how
//! Hyprland itself resolves them, so setting `windows` reaches `windowsIn`,
//! `windowsOut` and `windowsMove` unless one of those is set too.
//!
//! What is running is read with `hyprctl animations -j`, which reports each
//! leaf the config set ("overridden") and every curve defined.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{Error, Result};
use crate::model::lua_quote;

/// Which style vocabulary a leaf takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    /// Windows opening, closing and moving.
    Windows,
    /// Layer surfaces: bars, launchers, notifications.
    Layers,
    /// Workspace switches.
    Workspaces,
    /// A gradient turning.
    Angle,
    /// Anything that only fades or blends: no style.
    Plain,
}

/// What a style's optional second word is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StyleParam {
    /// Nothing follows.
    None,
    /// `top`, `bottom`, `left` or `right`.
    Side,
    /// A percentage, such as `80%`.
    Percent,
}

impl Family {
    /// The styles a leaf of this family accepts, and what may follow each.
    pub fn styles(self) -> &'static [(&'static str, StyleParam)] {
        match self {
            Family::Windows => &[
                ("slide", StyleParam::Side),
                ("popin", StyleParam::Percent),
                ("gnomed", StyleParam::None),
            ],
            Family::Layers => &[
                ("slide", StyleParam::Side),
                ("popin", StyleParam::Percent),
                ("fade", StyleParam::None),
            ],
            Family::Workspaces => &[
                ("slide", StyleParam::None),
                ("slidevert", StyleParam::None),
                ("fade", StyleParam::None),
                ("slidefade", StyleParam::Percent),
                ("slidefadevert", StyleParam::Percent),
            ],
            Family::Angle => &[("once", StyleParam::None), ("loop", StyleParam::None)],
            Family::Plain => &[],
        }
    }
}

/// One animation leaf.
#[derive(Debug, Clone, Copy)]
pub struct Leaf {
    /// Hyprland's name for it.
    pub name: &'static str,
    /// The leaf it inherits from, or `None` for `global`.
    pub parent: Option<&'static str>,
    /// What the page calls it.
    pub label: &'static str,
    /// Its style vocabulary.
    pub family: Family,
}

const fn leaf(
    name: &'static str,
    parent: &'static str,
    label: &'static str,
    family: Family,
) -> Leaf {
    Leaf { name, parent: Some(parent), label, family }
}

/// Every leaf, parents before children, in the order the page lists them.
///
/// Hyprland has no call that reports this tree, so it is written out here
/// from its documentation, with the 0.57 additions (`fadeGlow`, `glowangle`,
/// `shadowangle`) placed beside their siblings.
pub static TREE: &[Leaf] = &[
    Leaf { name: "global", parent: None, label: "Everything", family: Family::Plain },
    leaf("windows", "global", "Windows", Family::Windows),
    leaf("windowsIn", "windows", "Window opening", Family::Windows),
    leaf("windowsOut", "windows", "Window closing", Family::Windows),
    leaf("windowsMove", "windows", "Window moving", Family::Windows),
    leaf("layers", "global", "Layers — bars, launchers, notifications", Family::Layers),
    leaf("layersIn", "layers", "Layer appearing", Family::Layers),
    leaf("layersOut", "layers", "Layer disappearing", Family::Layers),
    leaf("fade", "global", "Fades", Family::Plain),
    leaf("fadeIn", "fade", "Window fading in", Family::Plain),
    leaf("fadeOut", "fade", "Window fading out", Family::Plain),
    leaf("fadeSwitch", "fade", "Focus changing", Family::Plain),
    leaf("fadeShadow", "fade", "Shadow on focus change", Family::Plain),
    leaf("fadeDim", "fade", "Dimming unfocused windows", Family::Plain),
    leaf("fadeLayers", "fade", "Layer fades", Family::Plain),
    leaf("fadeLayersIn", "fadeLayers", "Layer fading in", Family::Plain),
    leaf("fadeLayersOut", "fadeLayers", "Layer fading out", Family::Plain),
    leaf("fadePopups", "fade", "Popup fades", Family::Plain),
    leaf("fadePopupsIn", "fadePopups", "Popup fading in", Family::Plain),
    leaf("fadePopupsOut", "fadePopups", "Popup fading out", Family::Plain),
    leaf("fadeDpms", "fade", "Screen turning on and off", Family::Plain),
    leaf("fadeGlow", "fade", "Glow", Family::Plain),
    leaf("border", "global", "Border colour changing", Family::Plain),
    leaf("borderangle", "global", "Border gradient turning", Family::Angle),
    leaf("glowangle", "global", "Glow gradient turning", Family::Angle),
    leaf("shadowangle", "global", "Shadow gradient turning", Family::Angle),
    leaf("workspaces", "global", "Workspaces", Family::Workspaces),
    leaf("workspacesIn", "workspaces", "Workspace arriving", Family::Workspaces),
    leaf("workspacesOut", "workspaces", "Workspace leaving", Family::Workspaces),
    leaf("specialWorkspace", "workspaces", "Special workspace", Family::Workspaces),
    leaf(
        "specialWorkspaceIn",
        "specialWorkspace",
        "Special workspace arriving",
        Family::Workspaces,
    ),
    leaf(
        "specialWorkspaceOut",
        "specialWorkspace",
        "Special workspace leaving",
        Family::Workspaces,
    ),
    leaf("zoomFactor", "global", "Zoom", Family::Plain),
    leaf("monitorAdded", "global", "A monitor being added", Family::Plain),
];

/// The leaf with this name.
pub fn find(name: &str) -> Option<&'static Leaf> {
    TREE.iter().find(|l| l.name == name)
}

fn depth(leaf: &Leaf) -> usize {
    let mut d = 0;
    let mut at = leaf.parent;
    while let Some(p) = at {
        d += 1;
        at = find(p).and_then(|l| l.parent);
    }
    d
}

/// One leaf's settings, as `hl.animation` takes them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimSetting {
    /// Whether it animates at all.
    pub enabled: bool,
    /// In tenths of a second: 1 is 100 ms.
    pub speed: f64,
    /// The curve's name.
    pub bezier: String,
    /// The style, with its parameter if it has one, or empty for the default.
    #[serde(default)]
    pub style: String,
}

impl AnimSetting {
    /// What Hyprland does when nothing sets `global`.
    pub fn hyprland_default() -> Self {
        AnimSetting { enabled: true, speed: 8.0, bezier: "default".into(), style: String::new() }
    }

    fn same_as(&self, other: &AnimSetting) -> bool {
        if !self.enabled && !other.enabled {
            return true;
        }
        self.enabled == other.enabled
            && (self.speed - other.speed).abs() < 1e-3
            && self.bezier == other.bezier
            && self.style.trim() == other.style.trim()
    }
}

/// An easing curve's two control points: x0, y0, x1, y1.
pub type Points = [f64; 4];

/// What the compositor reports.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LiveMotion {
    /// Leaves the config sets, by name. Leaves it leaves alone are absent.
    pub set: BTreeMap<String, AnimSetting>,
    /// Every curve defined, by name.
    pub curves: BTreeMap<String, Points>,
}

/// Read the running animation setup.
pub fn read_live() -> Result<LiveMotion> {
    let (animations, curves) = crate::hyprctl::animations()?;
    Ok(LiveMotion {
        set: animations
            .into_iter()
            .filter(|a| a.overridden && find(&a.name).is_some())
            .map(|a| {
                let setting = AnimSetting {
                    enabled: a.enabled,
                    // Hyprland holds speeds as f32; 3.03 comes back as 3.0299999.
                    speed: (f64::from(a.speed) * 1000.0).round() / 1000.0,
                    bezier: a.bezier,
                    style: a.style,
                };
                (a.name, setting)
            })
            .collect(),
        curves: curves
            .into_iter()
            .map(|c| {
                // Held as f32, so 0.15 comes back as 0.15000000596.
                let r = |v: f32| (f64::from(v) * 1000.0).round() / 1000.0;
                let points = [r(c.x0), r(c.y0), r(c.x1), r(c.y1)];
                (c.name, points)
            })
            .collect(),
    })
}

// ---------------------------------------------------------------------------
// Checking what the UI sends
// ---------------------------------------------------------------------------

/// A curve name Hyprland and Lua will both take without quoting surprises.
pub fn check_curve_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 40
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok {
        Ok(())
    } else {
        Err(Error::other(format!("{name:?} cannot name a curve — use letters, digits, - and _")))
    }
}

/// Read a curve's points, keeping x within 0–1 (Hyprland accepts more, but
/// time cannot run backwards) and y within a range a curve can sensibly use.
pub fn parse_points(value: &Value) -> Result<Points> {
    let wrong = || Error::other("a curve takes four numbers: x0, y0, x1, y1");
    let a = value.as_array().filter(|a| a.len() == 4).ok_or_else(wrong)?;
    let mut p = [0.0; 4];
    for (i, v) in a.iter().enumerate() {
        let n = v.as_f64().ok_or_else(wrong)?;
        p[i] = if i % 2 == 0 { n.clamp(0.0, 1.0) } else { n.clamp(-2.0, 3.0) };
        p[i] = (p[i] * 1000.0).round() / 1000.0;
    }
    Ok(p)
}

/// Read one leaf's settings from the UI.
///
/// The style is not checked against [`Family::styles`]: those lists are what
/// the page offers, not the whole of what Hyprland takes (0.57 accepts
/// `slidefade` on windows, which its documentation does not list). Hyprland
/// checks a style when it is applied live and says what is wrong.
pub fn parse_setting(leaf: &Leaf, value: &Value) -> Result<AnimSetting> {
    let enabled = value.get("enabled").and_then(Value::as_bool).unwrap_or(true);
    let speed = value.get("speed").and_then(Value::as_f64).unwrap_or(1.0).clamp(0.1, 100.0);
    let speed = (speed * 100.0).round() / 100.0;
    let bezier = value.get("bezier").and_then(Value::as_str).unwrap_or("default").trim();
    if bezier.is_empty() {
        return Err(Error::other("an animation needs a curve"));
    }
    let style = value
        .get("style")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if style.split(' ').count() > 2 {
        return Err(Error::other(format!(
            "{style:?} is not a style for {} — a style is one word, and at most one after it",
            leaf.name
        )));
    }
    Ok(AnimSetting { enabled, speed, bezier: bezier.to_string(), style })
}

// ---------------------------------------------------------------------------
// Lua
// ---------------------------------------------------------------------------

fn num(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    if r == r.trunc() {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

/// `hl.curve(...)` for one curve.
pub fn curve_lua(name: &str, p: &Points) -> String {
    format!(
        "hl.curve({}, {{ type = \"bezier\", points = {{ {{{}, {}}}, {{{}, {}}} }} }})\n",
        lua_quote(name),
        num(p[0]),
        num(p[1]),
        num(p[2]),
        num(p[3])
    )
}

/// `hl.animation({...})` for one leaf.
pub fn animation_lua(name: &str, a: &AnimSetting) -> String {
    if !a.enabled {
        return format!("hl.animation({{ leaf = {}, enabled = false }})\n", lua_quote(name));
    }
    let mut out = format!(
        "hl.animation({{ leaf = {}, enabled = true, speed = {}, bezier = {}",
        lua_quote(name),
        num(a.speed),
        lua_quote(&a.bezier)
    );
    if !a.style.is_empty() {
        out.push_str(&format!(", style = {}", lua_quote(&a.style)));
    }
    out.push_str(" })\n");
    out
}

/// Every curve, then every animation — curves first, because an animation
/// naming a curve that is not defined yet is an error.
pub fn lua(
    curves: &BTreeMap<String, Points>,
    animations: &BTreeMap<String, AnimSetting>,
) -> String {
    let mut out = String::new();
    for (name, p) in curves {
        out.push_str(&curve_lua(name, p));
    }
    if !curves.is_empty() && !animations.is_empty() {
        out.push('\n');
    }
    // Tree order, so a parent reads before its children.
    for leaf in TREE {
        if let Some(a) = animations.get(leaf.name) {
            out.push_str(&animation_lua(leaf.name, a));
        }
    }
    for (name, a) in animations.iter().filter(|(n, _)| find(n).is_none()) {
        out.push_str(&animation_lua(name, a));
    }
    out
}

// ---------------------------------------------------------------------------
// Resolving, and the view
// ---------------------------------------------------------------------------

/// Where a leaf's effective settings come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Set on this leaf in the app.
    Here,
    /// Set on this leaf by the user's config.
    Config,
    /// Taken from an ancestor, named.
    Inherited(String),
    /// Nothing sets it or any ancestor: Hyprland's built-in default.
    Default,
}

/// What a leaf actually runs with, and why.
pub fn effective(
    name: &str,
    here: &BTreeMap<String, AnimSetting>,
    live: &LiveMotion,
) -> (AnimSetting, Source) {
    let mut at = Some(name);
    while let Some(n) = at {
        let found = here
            .get(n)
            .map(|s| (s, Source::Here))
            .or_else(|| live.set.get(n).map(|s| (s, Source::Config)));
        if let Some((s, src)) = found {
            let src = if n == name { src } else { Source::Inherited(n.to_string()) };
            return (s.clone(), src);
        }
        at = find(n).and_then(|l| l.parent);
    }
    (AnimSetting::hyprland_default(), Source::Default)
}

/// Leaves whose saved setting the compositor does not show after a reload —
/// something later in the file sets them again.
pub fn not_sticking(saved: &BTreeMap<String, AnimSetting>, live: &LiveMotion) -> Vec<String> {
    saved
        .iter()
        .filter(|(name, s)| live.set.get(*name).is_some_and(|l| !l.same_as(s)))
        .map(|(name, _)| format!("the {name} animation"))
        .collect()
}

/// The Animation page's state.
pub fn view(
    curves: &BTreeMap<String, Points>,
    animations: &BTreeMap<String, AnimSetting>,
    saved_curves: &BTreeMap<String, Points>,
    saved_animations: &BTreeMap<String, AnimSetting>,
    live: &LiveMotion,
) -> Value {
    let leaves: Vec<Value> = TREE
        .iter()
        .map(|l| {
            let (setting, source) = effective(l.name, animations, live);
            let (source, from) = match source {
                Source::Here => ("here", Value::Null),
                Source::Config => ("config", Value::Null),
                Source::Inherited(from) => ("inherited", json!(from)),
                Source::Default => ("default", Value::Null),
            };
            json!({
                "name": l.name,
                "label": l.label,
                "parent": l.parent,
                "depth": depth(l),
                "family": l.family,
                "styles": l.family.styles().iter().map(|(s, p)| json!({ "name": s, "param": p })).collect::<Vec<_>>(),
                "setting": setting,
                "source": source,
                "from": from,
                "overridden": animations.contains_key(l.name),
                "changed": animations.get(l.name) != saved_animations.get(l.name),
            })
        })
        .collect();

    // Every curve a leaf can name: the compositor's, with the app's own laid
    // over them.
    let mut all: BTreeMap<&str, (&Points, &str)> = BTreeMap::new();
    for (name, p) in &live.curves {
        all.insert(name, (p, if name == "default" { "builtin" } else { "config" }));
    }
    for (name, p) in curves {
        all.insert(name, (p, "here"));
    }
    let curves_view: Vec<Value> = all
        .iter()
        .map(|(name, (p, source))| {
            let used_by: Vec<&str> = TREE
                .iter()
                .filter(|l| animations.get(l.name).is_some_and(|a| a.bezier == *name))
                .map(|l| l.name)
                .collect();
            json!({
                "name": name,
                "points": p,
                "source": source,
                "changed": curves.get(*name) != saved_curves.get(*name),
                "usedBy": used_by,
            })
        })
        .collect();

    json!({
        "leaves": leaves,
        "curves": curves_view,
        "count": animations.len() + curves.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(speed: f64, bezier: &str) -> AnimSetting {
        AnimSetting { enabled: true, speed, bezier: bezier.into(), style: String::new() }
    }

    #[test]
    fn every_parent_exists_and_comes_first() {
        for (i, l) in TREE.iter().enumerate() {
            if let Some(p) = l.parent {
                let at = TREE.iter().position(|x| x.name == p).expect(p);
                assert!(at < i, "{} is listed before its parent {p}", l.name);
            }
        }
    }

    #[test]
    fn a_leaf_inherits_from_the_nearest_set_ancestor() {
        let mut live = LiveMotion::default();
        live.set.insert("windows".into(), s(2.0, "overshoot"));
        let mut here = BTreeMap::new();

        let (a, src) = effective("windowsIn", &here, &live);
        assert_eq!(a.bezier, "overshoot");
        assert_eq!(src, Source::Inherited("windows".into()));

        here.insert("windowsIn".into(), s(4.0, "quick"));
        assert_eq!(effective("windowsIn", &here, &live), (s(4.0, "quick"), Source::Here));
        assert_eq!(effective("windows", &here, &live).1, Source::Config);
        assert_eq!(effective("fadeIn", &here, &live).1, Source::Default);

        // Set here on a parent beats the config's setting on a grandparent.
        here.insert("fade".into(), s(3.0, "linear"));
        live.set.insert("global".into(), s(1.0, "default"));
        assert_eq!(effective("fadeLayersIn", &here, &live).1, Source::Inherited("fade".into()));
    }

    #[test]
    fn styles_are_tidied_and_only_their_shape_checked() {
        let w = find("windowsIn").unwrap();
        let ok =
            parse_setting(w, &json!({ "speed": 3, "bezier": "quick", "style": " popin   80% " }))
                .unwrap();
        assert_eq!(ok.style, "popin 80%");
        // Off the page's list, but Hyprland is the judge of that.
        assert!(parse_setting(w, &json!({ "bezier": "quick", "style": "slidefade 20%" })).is_ok());
        assert!(parse_setting(w, &json!({ "bezier": "quick", "style": "slide left now" })).is_err());
        assert!(parse_setting(w, &json!({ "bezier": "  " })).is_err());
        let fast = parse_setting(w, &json!({ "speed": 0, "bezier": "q" })).unwrap();
        assert_eq!(fast.speed, 0.1);
    }

    #[test]
    fn curve_points_are_kept_in_range() {
        assert_eq!(parse_points(&json!([1.5, -5, 0.25, 1.1])).unwrap(), [1.0, -2.0, 0.25, 1.1]);
        assert!(parse_points(&json!([1, 2, 3])).is_err());
        assert!(check_curve_name("snappy-2_x").is_ok());
        assert!(check_curve_name("no spaces").is_err());
        assert!(check_curve_name("").is_err());
    }

    #[test]
    fn lua_puts_curves_first_and_leaves_in_tree_order() {
        let mut curves = BTreeMap::new();
        curves.insert("snappy".to_string(), [0.2, 1.0, 0.3, 1.0]);
        let mut anims = BTreeMap::new();
        anims.insert(
            "windowsIn".to_string(),
            AnimSetting { style: "popin 80%".into(), ..s(3.5, "snappy") },
        );
        anims.insert("global".to_string(), s(8.0, "default"));
        anims.insert("fadeDpms".to_string(), AnimSetting { enabled: false, ..s(1.0, "x") });
        assert_eq!(
            lua(&curves, &anims),
            "hl.curve(\"snappy\", { type = \"bezier\", points = { {0.2, 1}, {0.3, 1} } })\n\
             \n\
             hl.animation({ leaf = \"global\", enabled = true, speed = 8, bezier = \"default\" })\n\
             hl.animation({ leaf = \"windowsIn\", enabled = true, speed = 3.5, bezier = \"snappy\", style = \"popin 80%\" })\n\
             hl.animation({ leaf = \"fadeDpms\", enabled = false })\n"
        );
    }

    #[test]
    fn a_setting_that_is_not_running_is_named() {
        let mut saved = BTreeMap::new();
        saved.insert("windows".to_string(), s(3.0, "quick"));
        saved.insert("fade".to_string(), s(2.0, "linear"));
        let mut live = LiveMotion::default();
        live.set.insert("windows".into(), s(3.0004, "quick"));
        live.set.insert("fade".into(), s(5.0, "linear"));
        assert_eq!(not_sticking(&saved, &live), vec!["the fade animation"]);
    }

    #[test]
    fn the_view_lists_every_curve_with_its_source() {
        let mut live = LiveMotion::default();
        live.curves.insert("default".into(), [0.0, 0.75, 0.15, 1.0]);
        live.curves.insert("quick".into(), [0.15, 0.0, 0.1, 1.0]);
        let mut curves = BTreeMap::new();
        curves.insert("quick".to_string(), [0.2, 0.0, 0.1, 1.0]);
        let mut anims = BTreeMap::new();
        anims.insert("fade".to_string(), s(2.0, "quick"));
        let v = view(&curves, &anims, &BTreeMap::new(), &BTreeMap::new(), &live);
        let quick = v["curves"].as_array().unwrap().iter().find(|c| c["name"] == "quick").unwrap();
        assert_eq!(quick["source"], "here");
        assert_eq!(quick["points"], json!([0.2, 0.0, 0.1, 1.0]));
        assert_eq!(quick["usedBy"], json!(["fade"]));
        assert_eq!(quick["changed"], json!(true));
        let fade_in =
            v["leaves"].as_array().unwrap().iter().find(|l| l["name"] == "fadeIn").unwrap();
        assert_eq!(fade_in["source"], "inherited");
        assert_eq!(fade_in["from"], "fade");
        assert_eq!(fade_in["depth"], 2);
    }
}
