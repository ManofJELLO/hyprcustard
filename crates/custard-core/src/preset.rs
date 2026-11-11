//! Themes: the whole look — every Look option, the animations and the curves
//! they run on — saved under a name, and put back with one click.
//!
//! A theme is complete rather than a list of changes. Applying one makes the
//! look block set everything it holds, so the result is that theme whatever
//! the user's own config says, and switching from one theme to another never
//! leaves a value behind from the first.
//!
//! Themes are JSON files in `~/.config/hyprcustard/hypr-themes/`, one per
//! theme, readable and shareable as they are. Values take the same shapes the
//! Look page uses, and colours may be written either `#aarrggbb` or
//! Hyprland's `rgba(rrggbbaa)`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{Error, Result};
use crate::look::{self, Look, LookValue};
use crate::model::{Config, LayerEntry, ShaderRef, TagValue, WindowRule};
use crate::motion::{self, AnimSetting, LiveMotion, Points};
use crate::paths;

/// The format version written into each file.
pub const FORMAT: u32 = 1;

/// One theme, as it is written to disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    /// What it is called.
    pub name: String,
    /// A line about it, or empty.
    #[serde(default)]
    pub description: String,
    /// The file format's version.
    #[serde(default = "format")]
    pub format: u32,
    /// Look options by key, as plain JSON values in each option's shape.
    #[serde(default)]
    pub options: BTreeMap<String, Value>,
    /// Curves by name: x0, y0, x1, y1.
    #[serde(default)]
    pub curves: BTreeMap<String, Points>,
    /// Animation leaves the theme sets, by name. Leaves it leaves out inherit,
    /// within the theme, the way Hyprland resolves them.
    #[serde(default)]
    pub animations: BTreeMap<String, AnimSetting>,
    /// The HyprWindowShade setup that goes with the look, if the theme
    /// carries one. A theme without it leaves the shader setup alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shaders: Option<ShaderSetup>,
}

/// What a path to a shader inside a theme starts with: the shader folder,
/// wherever it is on the machine the theme is applied on.
pub const SHADER_PREFIX: &str = "$SHADERS/";

/// The shader side of a theme: the window rules and layer shaders, and the
/// shaders themselves.
///
/// Keybinds and startup actions are left out on purpose — like the cursor,
/// they are behaviour rather than looks — and so is how the plugin is loaded.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ShaderSetup {
    /// Window rules, their shader paths written as `$SHADERS/<file>`.
    #[serde(default)]
    pub rules: Vec<WindowRule>,
    /// Layer shaders, written the same way.
    #[serde(default)]
    pub layers: Vec<LayerEntry>,
    /// Every shader they use, by file name, as source text. Carried in the
    /// theme because a shader's tuned values live inside its file: without
    /// them a theme would look different on another machine, or on this one
    /// after the shader is edited.
    #[serde(default)]
    pub files: BTreeMap<String, String>,
}

/// Every shader reference in a set of rules and layers.
fn for_each_ref(
    rules: &mut [WindowRule],
    layers: &mut [LayerEntry],
    mut f: impl FnMut(&mut ShaderRef),
) {
    for r in rules {
        for t in &mut r.tags {
            if let TagValue::Path(s) = &mut t.value {
                f(s);
            }
        }
    }
    for l in layers {
        for s in [&mut l.shader, &mut l.open_anim, &mut l.close_anim].into_iter().flatten() {
            f(s);
        }
    }
}

/// The shader setup in `config`, with the files it uses read in, or `None`
/// when there is nothing to carry.
///
/// A reference that is not a readable file — a layer's `clear`, a shader
/// since deleted — is kept as it is written. Two different files with the
/// same name are told apart with a number.
pub fn capture_shaders(config: &Config) -> Option<ShaderSetup> {
    if config.rules.is_empty() && config.layers.is_empty() {
        return None;
    }
    let mut rules = config.rules.clone();
    let mut layers = config.layers.clone();
    let mut files: BTreeMap<String, String> = BTreeMap::new();
    let mut named: BTreeMap<String, String> = BTreeMap::new();
    for_each_ref(&mut rules, &mut layers, |s| {
        if let Some(name) = named.get(&s.path) {
            s.path = format!("{SHADER_PREFIX}{name}");
            return;
        }
        let path = paths::expand(&s.path);
        let Ok(source) = std::fs::read_to_string(&path) else { return };
        let base = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mut name = base.clone();
        let mut n = 2;
        while files.get(&name).is_some_and(|other| *other != source) {
            name = numbered(&base, n);
            n += 1;
        }
        files.insert(name.clone(), source);
        named.insert(s.path.clone(), name.clone());
        s.path = format!("{SHADER_PREFIX}{name}");
    });
    Some(ShaderSetup { rules, layers, files })
}

/// `crt.glsl` as `crt-2.glsl`.
fn numbered(name: &str, n: usize) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) => format!("{stem}-{n}.{ext}"),
        None => format!("{name}-{n}"),
    }
}

/// A shader file name a theme may write: no directories, nothing hidden.
fn safe_file_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && !name.starts_with('.')
        && !name.contains('/')
        && !name.contains('\\')
        && name != "..";
    if ok {
        Ok(())
    } else {
        Err(Error::other(format!("{name:?} cannot be a shader file name")))
    }
}

/// The shaders in `setup` that exist in `dir` with different contents: what
/// applying it would have to either replace or keep beside.
pub fn shader_conflicts(setup: &ShaderSetup, dir: &Path) -> Vec<String> {
    setup
        .files
        .iter()
        .filter(|(name, source)| {
            std::fs::read_to_string(dir.join(name)).is_ok_and(|have| have != **source)
        })
        .map(|(name, _)| name.clone())
        .collect()
}

/// What to do with a shader the user already has a different version of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnConflict {
    /// Back the user's up, then write the theme's over it.
    Replace,
    /// Write the theme's beside it under a new name, and use that.
    KeepBoth,
}

/// Write the theme's shaders into `dir` and return its rules and layers
/// pointing at them, plus a note per file that needed handling.
pub fn install_shaders(
    setup: &ShaderSetup,
    dir: &Path,
    theme: &str,
    on_conflict: OnConflict,
    backups_to_keep: usize,
) -> Result<(Vec<WindowRule>, Vec<LayerEntry>, Vec<String>)> {
    let mut notes = Vec::new();
    let mut final_name: BTreeMap<&str, String> = BTreeMap::new();
    for (name, source) in &setup.files {
        safe_file_name(name)?;
        let target = dir.join(name);
        let chosen = match std::fs::read_to_string(&target) {
            Err(_) => {
                paths::write_atomic(&target, source)?;
                notes.push(format!("added {name}"));
                name.clone()
            }
            Ok(have) if have == *source => name.clone(),
            Ok(_) if on_conflict == OnConflict::Replace => {
                paths::backup(&target, backups_to_keep)?;
                paths::write_atomic(&target, source)?;
                notes.push(format!("replaced {name}"));
                name.clone()
            }
            Ok(_) => {
                let slug = file_name(theme);
                let slug = slug.trim_end_matches(".json");
                let (stem, ext) = name.rsplit_once('.').unwrap_or((name.as_str(), "glsl"));
                let mut candidate = format!("{stem}-{slug}.{ext}");
                let mut n = 2;
                loop {
                    match std::fs::read_to_string(dir.join(&candidate)) {
                        Err(_) => {
                            paths::write_atomic(&dir.join(&candidate), source)?;
                            break;
                        }
                        Ok(have) if have == *source => break,
                        Ok(_) => {
                            candidate = format!("{stem}-{slug}-{n}.{ext}");
                            n += 1;
                        }
                    }
                }
                notes.push(format!("kept your {name} and added the theme's as {candidate}"));
                candidate
            }
        };
        final_name.insert(name.as_str(), chosen);
    }
    let (rules, layers) = resolve(setup, dir, &final_name);
    Ok((rules, layers, notes))
}

/// The setup's rules and layers with `$SHADERS/` paths pointing into `dir`,
/// under the names in `renamed` where a file was renamed.
fn resolve(
    setup: &ShaderSetup,
    dir: &Path,
    renamed: &BTreeMap<&str, String>,
) -> (Vec<WindowRule>, Vec<LayerEntry>) {
    let mut rules = setup.rules.clone();
    let mut layers = setup.layers.clone();
    for_each_ref(&mut rules, &mut layers, |s| {
        if let Some(name) = s.path.strip_prefix(SHADER_PREFIX) {
            let name = renamed.get(name).cloned().unwrap_or_else(|| name.to_string());
            s.path = dir.join(name).to_string_lossy().into_owned();
        }
    });
    (rules, layers)
}

fn format() -> u32 {
    FORMAT
}

/// Whether an option belongs in a theme. The cursor options are behaviour,
/// not looks: a theme should not decide when someone's cursor hides.
pub fn themed(key: &str) -> bool {
    !key.starts_with("cursor.")
}

/// Where themes are kept.
pub fn dir() -> PathBuf {
    paths::app_config_dir().join("hypr-themes")
}

// ---------------------------------------------------------------------------
// Making one from what is on screen
// ---------------------------------------------------------------------------

/// The look as it stands — unsaved edits included — as a theme.
///
/// Options are what the Look page shows for each, leaving out one that only
/// follows another and is unset. Animations are every leaf that is set,
/// here or in the user's config; inherited leaves stay inherited. Curves are
/// those animations' curves, from wherever their points can be found.
pub fn snapshot(
    name: &str,
    description: &str,
    current: &Look,
    live: &BTreeMap<String, LookValue>,
    motion_live: &LiveMotion,
    config_curves: &BTreeMap<String, Option<Points>>,
) -> Preset {
    let mut options = BTreeMap::new();
    for o in look::OPTIONS.iter().filter(|o| themed(o.key)) {
        let value = current.values.get(o.key).or_else(|| live.get(o.key)).cloned();
        let value = match value {
            Some(v) => v,
            // Unset and following another: let it keep following.
            None if !o.follows.is_empty() => continue,
            None => look::default_of(o),
        };
        options.insert(o.key.to_string(), value.to_json());
    }

    let mut animations = BTreeMap::new();
    for leaf in motion::TREE {
        let (setting, source) = motion::effective(leaf.name, &current.animations, motion_live);
        if matches!(source, motion::Source::Here | motion::Source::Config) {
            animations.insert(leaf.name.to_string(), setting);
        }
    }

    let mut curves = BTreeMap::new();
    for a in animations.values().filter(|a| a.enabled && a.bezier != "default") {
        let points = current
            .curves
            .get(&a.bezier)
            .or_else(|| motion_live.curves.get(&a.bezier))
            .copied()
            .or_else(|| config_curves.get(&a.bezier).copied().flatten());
        if let Some(p) = points {
            curves.insert(a.bezier.clone(), p);
        }
    }
    for (name, p) in &current.curves {
        curves.entry(name.clone()).or_insert(*p);
    }

    Preset {
        name: name.trim().to_string(),
        description: description.trim().to_string(),
        format: FORMAT,
        options,
        curves,
        animations,
        shaders: None,
    }
}

// ---------------------------------------------------------------------------
// Putting one back
// ---------------------------------------------------------------------------

/// The look block that applies `preset`, and anything in it that could not
/// be used.
///
/// Every option the theme holds is set. An animation leaf the user's config
/// sets but the theme does not is set too, to what it would inherit within the
/// theme — otherwise the config's setting would show through and the theme
/// would not look the way it was saved. Options outside themes (the cursor)
/// keep whatever the app had set for them.
pub fn to_look(preset: &Preset, current: &Look, motion_live: &LiveMotion) -> (Look, Vec<String>) {
    let mut problems = Vec::new();
    let mut out = Look::default();

    for (key, value) in &preset.options {
        match look::find(key) {
            Some(o) if themed(key) => match look::parse(o, value) {
                Ok(v) => {
                    out.values.insert(key.clone(), v);
                }
                Err(e) => problems.push(e.to_string()),
            },
            Some(_) => {}
            None => problems.push(format!("{key} is not an option this version knows")),
        }
    }
    for (key, v) in current.values.iter().filter(|(k, _)| !themed(k)) {
        out.values.insert(key.clone(), v.clone());
    }

    for (name, points) in &preset.curves {
        match motion::check_curve_name(name)
            .and_then(|()| motion::parse_points(&json!(points.to_vec())))
        {
            Ok(p) => {
                out.curves.insert(name.clone(), p);
            }
            Err(e) => problems.push(e.to_string()),
        }
    }

    for (name, a) in &preset.animations {
        match motion::find(name) {
            Some(leaf) => {
                match motion::parse_setting(leaf, &serde_json::to_value(a).unwrap_or_default()) {
                    Ok(s) => {
                        out.animations.insert(name.clone(), s);
                    }
                    Err(e) => problems.push(e.to_string()),
                }
            }
            None => problems.push(format!("{name} is not an animation this version knows")),
        }
    }
    // Leaves the config sets and the theme does not: pin them to what the
    // theme would give them, so the config's own values do not show through.
    let theme_only = LiveMotion { set: BTreeMap::new(), curves: BTreeMap::new() };
    let mut pinned = BTreeMap::new();
    for name in motion_live.set.keys() {
        if !out.animations.contains_key(name) && motion::find(name).is_some() {
            let (setting, _) = motion::effective(name, &out.animations, &theme_only);
            pinned.insert(name.clone(), setting);
        }
    }
    out.animations.extend(pinned);

    (out, problems)
}

/// Whether `current` is what applying `preset` would give, as far as the
/// theme itself goes — so the page can mark the theme in use.
pub fn matches(preset: &Preset, current: &Look, config: &Config) -> bool {
    let options_match = preset.options.iter().all(|(key, value)| {
        match (look::find(key), current.values.get(key)) {
            (Some(o), Some(have)) => look::parse(o, value).is_ok_and(|want| want.same_as(have)),
            _ => false,
        }
    });
    let animations_match =
        preset.animations.iter().all(|(name, a)| current.animations.get(name) == Some(a));
    let shaders_match = preset.shaders.as_ref().is_none_or(|setup| {
        let (rules, layers) = resolve(setup, &paths::expand(&config.shader_dir), &BTreeMap::new());
        rules == config.rules && layers == config.layers
    });
    options_match && animations_match && shaders_match && !preset.options.is_empty()
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

/// A theme's file name: its name, lower-cased, with anything awkward in a
/// path turned into `-`.
pub fn file_name(name: &str) -> String {
    let mut slug: String = name
        .trim()
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    let slug = slug.trim_matches('-');
    format!("{}.json", if slug.is_empty() { "theme" } else { slug })
}

/// Read one theme file.
pub fn load(path: &Path) -> Result<Preset> {
    let text = paths::read_to_string(path)?;
    let mut preset: Preset = serde_json::from_str(&text)
        .map_err(|e| Error::other(format!("{} is not a theme file: {e}", paths::contract(path))))?;
    if preset.format > FORMAT {
        return Err(Error::other(format!(
            "{} was written by a newer hyprcustard (theme format {})",
            paths::contract(path),
            preset.format
        )));
    }
    if preset.name.trim().is_empty() {
        preset.name =
            path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    }
    Ok(preset)
}

/// Write a theme into `dir`, refusing to replace another unless asked.
pub fn save_into(dir: &Path, preset: &Preset, replace: bool) -> Result<PathBuf> {
    if preset.name.trim().is_empty() {
        return Err(Error::other("a theme needs a name"));
    }
    let path = dir.join(file_name(&preset.name));
    if path.exists() && !replace {
        return Err(Error::other(format!("there is already a theme called {}", preset.name)));
    }
    let text = serde_json::to_string_pretty(preset)?;
    paths::write_atomic(&path, &format!("{text}\n"))?;
    Ok(path)
}

/// One theme in the list.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The theme.
    pub preset: Preset,
    /// Its file, or `None` for one built into the app.
    pub path: Option<PathBuf>,
}

/// Every theme: the built-in ones first, then the folder's, by name. Files
/// that cannot be read are reported rather than dropped silently.
pub fn list_in(dir: &Path) -> (Vec<Entry>, Vec<String>) {
    let mut entries: Vec<Entry> =
        builtin().into_iter().map(|preset| Entry { preset, path: None }).collect();
    let mut problems = Vec::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).collect())
        .unwrap_or_default();
    files.retain(|p| p.extension().is_some_and(|x| x == "json"));
    files.sort();
    for path in files {
        match load(&path) {
            Ok(preset) => entries.push(Entry { preset, path: Some(path) }),
            Err(e) => problems.push(e.to_string()),
        }
    }
    (entries, problems)
}

// ---------------------------------------------------------------------------
// Built in
// ---------------------------------------------------------------------------

/// Themes that ship with the app.
///
/// "Hyprland defaults" is every option at Hyprland's own default and nothing
/// animated beyond Hyprland's built-in curve — a way back to a plain desktop
/// without touching a file.
pub fn builtin() -> Vec<Preset> {
    let mut defaults = Preset {
        name: "Hyprland defaults".into(),
        description: "Everything as Hyprland ships it.".into(),
        format: FORMAT,
        options: BTreeMap::new(),
        curves: BTreeMap::new(),
        animations: BTreeMap::new(),
        shaders: None,
    };
    for o in look::OPTIONS.iter().filter(|o| themed(o.key) && o.follows.is_empty()) {
        defaults.options.insert(o.key.to_string(), look::default_of(o).to_json());
    }
    defaults.animations.insert("global".into(), AnimSetting::hyprland_default());
    // borderangle and its siblings are off unless asked for.
    for leaf in ["borderangle", "glowangle", "shadowangle"] {
        defaults
            .animations
            .insert(leaf.into(), AnimSetting { enabled: false, ..AnimSetting::hyprland_default() });
    }

    let mut glass = defaults.clone();
    glass.name = "Soft glass".into();
    glass.description = "Round, airy and translucent, with a gentle glow.".into();
    for (k, v) in [
        ("general.gaps_in", json!([6, 6, 6, 6])),
        ("general.gaps_out", json!([14, 14, 14, 14])),
        ("general.border_size", json!(2)),
        (
            "general.col.active_border",
            json!({ "colors": ["rgba(89b4faee)", "rgba(cba6f7ee)"], "angle": 45 }),
        ),
        ("general.col.inactive_border", json!({ "colors": ["rgba(45475a99)"], "angle": 0 })),
        ("decoration.rounding", json!(14)),
        ("decoration.rounding_power", json!(2.5)),
        ("decoration.inactive_opacity", json!(0.88)),
        ("decoration.shadow.range", json!(24)),
        ("decoration.shadow.render_power", json!(3)),
        ("decoration.shadow.color", json!("rgba(11111b88)")),
        ("decoration.glow.enabled", json!(true)),
        ("decoration.glow.range", json!(12)),
        ("decoration.glow.color", json!("rgba(89b4fa44)")),
        ("decoration.blur.size", json!(8)),
        ("decoration.blur.passes", json!(3)),
        ("decoration.blur.vibrancy", json!(0.25)),
    ] {
        glass.options.insert(k.into(), v);
    }
    glass.curves.insert("glide".into(), [0.05, 0.9, 0.1, 1.05]);
    glass.animations.insert(
        "global".into(),
        AnimSetting { enabled: true, speed: 6.0, bezier: "glide".into(), style: String::new() },
    );
    glass.animations.insert(
        "windows".into(),
        AnimSetting {
            enabled: true,
            speed: 5.0,
            bezier: "glide".into(),
            style: "popin 85%".into(),
        },
    );
    glass.animations.insert(
        "workspaces".into(),
        AnimSetting {
            enabled: true,
            speed: 5.0,
            bezier: "glide".into(),
            style: "slidefade 20%".into(),
        },
    );

    let mut sharp = defaults.clone();
    sharp.name = "Sharp and quick".into();
    sharp.description = "Square corners, thin borders, no blur, and short snappy motion.".into();
    for (k, v) in [
        ("general.gaps_in", json!([2, 2, 2, 2])),
        ("general.gaps_out", json!([4, 4, 4, 4])),
        ("general.border_size", json!(1)),
        ("general.col.active_border", json!({ "colors": ["rgba(ebdbb2ff)"], "angle": 0 })),
        ("general.col.inactive_border", json!({ "colors": ["rgba(3c3836ff)"], "angle": 0 })),
        ("decoration.rounding", json!(0)),
        ("decoration.shadow.enabled", json!(false)),
        ("decoration.blur.enabled", json!(false)),
    ] {
        sharp.options.insert(k.into(), v);
    }
    sharp.curves.insert("snap".into(), [0.2, 1.0, 0.3, 1.0]);
    sharp.animations.insert(
        "global".into(),
        AnimSetting { enabled: true, speed: 2.5, bezier: "snap".into(), style: String::new() },
    );
    sharp.animations.insert(
        "windows".into(),
        AnimSetting { enabled: true, speed: 2.0, bezier: "snap".into(), style: "slide".into() },
    );

    vec![defaults, glass, sharp]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live_with(set: &[(&str, AnimSetting)]) -> LiveMotion {
        LiveMotion {
            set: set.iter().map(|(n, s)| (n.to_string(), s.clone())).collect(),
            curves: BTreeMap::new(),
        }
    }

    fn anim(speed: f64, bezier: &str) -> AnimSetting {
        AnimSetting { enabled: true, speed, bezier: bezier.into(), style: String::new() }
    }

    #[test]
    fn every_builtin_applies_cleanly() {
        for p in builtin() {
            let (look, problems) = to_look(&p, &Look::default(), &LiveMotion::default());
            assert!(problems.is_empty(), "{}: {problems:?}", p.name);
            assert!(
                matches(&p, &look, &Config::default()),
                "{} does not match its own result",
                p.name
            );
        }
    }

    #[test]
    fn a_snapshot_round_trips_through_applying_it() {
        let mut current = Look::default();
        current.values.insert("decoration.rounding".into(), LookValue::Int(9));
        current.curves.insert("snappy".into(), [0.2, 1.0, 0.3, 1.0]);
        current.animations.insert("windows".into(), anim(3.0, "snappy"));
        current.values.insert("cursor.inactive_timeout".into(), LookValue::Float(3.0));
        let mut live = BTreeMap::new();
        live.insert("decoration.blur.size".to_string(), LookValue::Int(6));
        let motion_live = live_with(&[("fade", anim(3.0, "quick"))]);
        let mut config_curves = BTreeMap::new();
        config_curves.insert("quick".to_string(), Some([0.15, 0.0, 0.1, 1.0]));

        let p = snapshot("Mine", "", &current, &live, &motion_live, &config_curves);
        assert_eq!(p.options["decoration.rounding"], json!(9));
        assert_eq!(p.options["decoration.blur.size"], json!(6));
        assert!(!p.options.contains_key("cursor.inactive_timeout"), "the cursor is not a look");
        // Unset followers keep following.
        assert!(!p.options.contains_key("decoration.shadow.color_inactive"));
        assert_eq!(p.animations.keys().collect::<Vec<_>>(), vec!["fade", "windows"]);
        assert_eq!(p.curves["quick"], [0.15, 0.0, 0.1, 1.0]);
        assert_eq!(p.curves["snappy"], [0.2, 1.0, 0.3, 1.0]);

        let (applied, problems) = to_look(&p, &current, &motion_live);
        assert!(problems.is_empty(), "{problems:?}");
        assert!(matches(&p, &applied, &Config::default()));
        // The app's cursor setting survives applying a theme.
        assert_eq!(applied.values["cursor.inactive_timeout"], LookValue::Float(3.0));
    }

    #[test]
    fn a_leaf_the_config_sets_and_the_theme_does_not_is_pinned_to_the_theme() {
        let mut p = builtin().remove(0);
        p.animations.insert("windows".into(), anim(4.0, "default"));
        // The user's config sets windowsIn and fade; the theme sets neither.
        let motion_live =
            live_with(&[("windowsIn", anim(2.0, "overshoot")), ("fade", anim(3.0, "quick"))]);
        let (look, _) = to_look(&p, &Look::default(), &motion_live);
        assert_eq!(look.animations["windowsIn"], anim(4.0, "default"), "should follow windows");
        assert_eq!(
            look.animations["fade"],
            AnimSetting::hyprland_default(),
            "should follow global"
        );
    }

    #[test]
    fn bad_values_are_reported_and_the_rest_still_applies() {
        let mut p = builtin().remove(0);
        p.options.insert("decoration.rounding".into(), json!("round"));
        p.options.insert("decoration.wobble".into(), json!(1));
        p.animations.insert("nope".into(), anim(1.0, "default"));
        p.curves.insert("bad name".into(), [0.0, 0.0, 1.0, 1.0]);
        let (look, problems) = to_look(&p, &Look::default(), &LiveMotion::default());
        assert_eq!(problems.len(), 4, "{problems:?}");
        assert!(look.values.contains_key("decoration.blur.size"));
        assert!(!look.values.contains_key("decoration.rounding"));
    }

    #[test]
    fn files_are_named_after_the_theme_and_not_overwritten_by_accident() {
        assert_eq!(file_name("Soft glass"), "soft-glass.json");
        assert_eq!(file_name("  ../Ugly // name!! "), "ugly-name.json");
        assert_eq!(file_name("***"), "theme.json");

        let dir = tempfile::tempdir().unwrap();
        let p = builtin().remove(1);
        let path = save_into(dir.path(), &p, false).unwrap();
        assert!(save_into(dir.path(), &p, false).is_err());
        assert!(save_into(dir.path(), &p, true).is_ok());
        assert_eq!(load(&path).unwrap(), p);

        std::fs::write(dir.path().join("broken.json"), "{ nope").unwrap();
        let (entries, problems) = list_in(dir.path());
        assert_eq!(entries.len(), builtin().len() + 1);
        assert_eq!(problems.len(), 1);
    }

    fn shader_config(dir: &Path) -> Config {
        use crate::model::{Match, Tag, TagSlot};
        std::fs::write(dir.join("crt.glsl"), "// crt\n").unwrap();
        std::fs::write(dir.join("blur.glsl"), "// blur\n").unwrap();
        let mut config = Config::default();
        config.shader_dir = dir.to_string_lossy().into_owned();
        let mut rule = WindowRule::new("rule1");
        rule.match_ = Match { class: Some("kitty".into()), ..Default::default() };
        rule.tags.push(Tag {
            slot: TagSlot::Inactive,
            is_default: false,
            value: TagValue::Path(ShaderRef {
                path: dir.join("crt.glsl").to_string_lossy().into_owned(),
                duration: Some(0.4),
            }),
        });
        config.rules.push(rule);
        let mut layer = LayerEntry::new("layer1", "rofi");
        layer.shader = Some(ShaderRef::new(dir.join("blur.glsl").to_string_lossy()));
        layer.close_anim = Some(ShaderRef::new("clear"));
        config.layers.push(layer);
        config
    }

    #[test]
    fn a_shader_setup_travels_with_its_files() {
        let from = tempfile::tempdir().unwrap();
        let config = shader_config(from.path());
        let setup = capture_shaders(&config).unwrap();
        assert_eq!(setup.files.len(), 2);
        assert_eq!(setup.files["crt.glsl"], "// crt\n");
        let TagValue::Path(r) = &setup.rules[0].tags[0].value else { panic!() };
        assert_eq!(r.path, "$SHADERS/crt.glsl");
        assert_eq!(r.duration, Some(0.4));
        assert_eq!(setup.layers[0].close_anim.as_ref().unwrap().path, "clear");
        assert!(capture_shaders(&Config::default()).is_none());

        // On another machine, with an empty shader folder.
        let to = tempfile::tempdir().unwrap();
        assert!(shader_conflicts(&setup, to.path()).is_empty());
        let (rules, layers, notes) =
            install_shaders(&setup, to.path(), "Mine", OnConflict::Replace, 0).unwrap();
        assert_eq!(notes.len(), 2);
        assert_eq!(std::fs::read_to_string(to.path().join("crt.glsl")).unwrap(), "// crt\n");
        let TagValue::Path(r) = &rules[0].tags[0].value else { panic!() };
        assert_eq!(std::path::Path::new(&r.path), to.path().join("crt.glsl"));
        assert_eq!(layers[0].close_anim.as_ref().unwrap().path, "clear");

        // Applying it over the same folder again needs nothing.
        let (_, _, notes) =
            install_shaders(&setup, to.path(), "Mine", OnConflict::Replace, 0).unwrap();
        assert!(notes.is_empty(), "{notes:?}");
    }

    #[test]
    fn a_shader_the_user_changed_is_replaced_or_kept_beside() {
        let from = tempfile::tempdir().unwrap();
        let setup = capture_shaders(&shader_config(from.path())).unwrap();
        let to = tempfile::tempdir().unwrap();
        std::fs::write(to.path().join("crt.glsl"), "// my own crt\n").unwrap();
        assert_eq!(shader_conflicts(&setup, to.path()), vec!["crt.glsl"]);

        let (rules, _, notes) =
            install_shaders(&setup, to.path(), "Soft glass", OnConflict::KeepBoth, 0).unwrap();
        assert_eq!(std::fs::read_to_string(to.path().join("crt.glsl")).unwrap(), "// my own crt\n");
        let kept = to.path().join("crt-soft-glass.glsl");
        assert_eq!(std::fs::read_to_string(&kept).unwrap(), "// crt\n");
        let TagValue::Path(r) = &rules[0].tags[0].value else { panic!() };
        assert_eq!(std::path::Path::new(&r.path), kept);
        assert!(notes.iter().any(|n| n.contains("crt-soft-glass.glsl")), "{notes:?}");

        install_shaders(&setup, to.path(), "Soft glass", OnConflict::Replace, 0).unwrap();
        assert_eq!(std::fs::read_to_string(to.path().join("crt.glsl")).unwrap(), "// crt\n");
    }

    #[test]
    fn a_theme_cannot_write_outside_the_shader_folder() {
        let mut setup = ShaderSetup::default();
        setup.files.insert("../escape.glsl".into(), "x".into());
        let to = tempfile::tempdir().unwrap();
        assert!(install_shaders(&setup, to.path(), "Bad", OnConflict::Replace, 0).is_err());
        assert!(!to.path().parent().unwrap().join("escape.glsl").exists());
    }

    #[test]
    fn a_theme_with_shaders_is_in_use_only_with_those_rules() {
        let dir = tempfile::tempdir().unwrap();
        let config = shader_config(dir.path());
        let mut p = builtin().remove(1);
        p.shaders = capture_shaders(&config);
        let (look, _) = to_look(&p, &Look::default(), &LiveMotion::default());
        assert!(matches(&p, &look, &config));
        assert!(!matches(&p, &look, &Config { rules: Vec::new(), ..config.clone() }));
    }

    #[test]
    fn colours_may_be_written_either_way_round() {
        let mut p = builtin().remove(0);
        p.options.insert("decoration.shadow.color".into(), json!("rgba(1d1d1d66)"));
        let (look, problems) = to_look(&p, &Look::default(), &LiveMotion::default());
        assert!(problems.is_empty());
        assert_eq!(look.values["decoration.shadow.color"], LookValue::Color("#661d1d1d".into()));
    }
}
