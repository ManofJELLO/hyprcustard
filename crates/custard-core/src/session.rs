//! The facade the UI drives.
//!
//! Everything the GUI can do is a [`Session::command`] with a JSON payload, and
//! everything the GUI can see is [`Session::state`]. Keeping the surface this
//! narrow means the Qt layer holds no logic of its own and the whole app is
//! testable without a compositor or a display.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::error::{Error, Result};
use crate::look::{self, Look, LookValue};
use crate::model::{
    base_name, Action, Bind, Config, LayerEntry, LoadMode, ShaderRef, StartupAction, Tag, TagGroup,
    TagKind, TagSlot, TagValue, WindowRule, TAGS,
};
use crate::motion;
use crate::preset;
use crate::preview;
use crate::settings::Settings;
use crate::shader::meta::{Param, ParamValue, ShaderInfo, KNOWN_UNIFORMS, MOTION_UNIFORMS};
use crate::shader::{DraftItem, ShaderDraft};
use crate::theme::Theme;
use crate::{block, emit, hyprctl, hyprpm, import, paths, shader, theme};

/// Live information about the running compositor.
#[derive(Debug, Clone, Default)]
pub struct Live {
    /// Whether a Hyprland session is running.
    pub running: bool,
    /// Whether the plugin reports itself loaded.
    pub plugin_loaded: bool,
    /// Window classes currently on screen.
    pub classes: Vec<String>,
    /// Layer namespaces currently mapped.
    pub namespaces: Vec<String>,
    /// The look options as the compositor has them right now, unsaved live
    /// edits included.
    pub look: BTreeMap<String, LookValue>,
    /// The animation leaves and curves the compositor is running with.
    pub motion: motion::LiveMotion,
}

/// Which part of the app noticed a problem.
///
/// Refreshing one source clears only the problems that source reported. The
/// alternative — matching on the text of the message — silently stops working
/// the moment a message is reworded, and quietly accumulates duplicates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Reading `settings.toml`.
    Settings,
    /// Reading the managed block out of the Lua config.
    Config,
    /// Scanning the shader directory.
    Shaders,
    /// Loading theme files.
    Themes,
    /// Reading the Hyprland themes folder.
    Presets,
}

/// A non-fatal problem, and where it came from.
#[derive(Debug, Clone)]
pub struct Problem {
    /// What noticed it.
    pub source: Source,
    /// What to show the user.
    pub text: String,
}

/// The whole application state.
pub struct Session {
    /// App settings.
    pub settings: Settings,
    /// The managed configuration.
    pub config: Config,
    /// The last config read from or written to disk, for the dirty check.
    saved: Config,
    /// Hyprland look options set in the app, saved or not.
    pub look: Look,
    /// The look block as last read from or written to disk.
    saved_look: Look,
    /// The curves the user's own config defines, read from the file. Only the
    /// file can say this: Hyprland keeps every curve until it restarts.
    config_curves: BTreeMap<String, Option<motion::Points>>,
    /// Every theme, built in and from the themes folder.
    presets: Vec<preset::Entry>,
    /// Shaders found in the shader directory.
    pub shaders: Vec<ShaderInfo>,
    /// Shader edits staged but not yet written, by shader path.
    ///
    /// Unlike the config, which is one document saved as a whole, a shader
    /// belongs to its own file and is saved on its own — so these are keyed
    /// per shader rather than held as one "last saved" copy.
    drafts: BTreeMap<String, ShaderDraft>,
    /// The resolved colour theme.
    pub theme: Theme,
    /// Every theme available to choose from.
    pub themes: Vec<Theme>,
    /// Live compositor information.
    pub live: Live,
    /// Non-fatal problems worth showing, tagged with what reported them.
    pub problems: Vec<Problem>,
}

impl Session {
    /// Load settings, the config and the shader list.
    pub fn load() -> Self {
        let (settings, settings_problem) = Settings::load();
        let mut problems: Vec<Problem> = settings_problem
            .into_iter()
            .map(|text| Problem { source: Source::Settings, text })
            .collect();

        let config = match read_config(&settings.resolved_config_path()) {
            Ok(c) => c,
            Err(e) => {
                problems.push(Problem { source: Source::Config, text: e.to_string() });
                Config::default()
            }
        };
        let look = match read_look(&settings.resolved_config_path()) {
            Ok(l) => l,
            Err(e) => {
                problems.push(Problem { source: Source::Config, text: e.to_string() });
                Look::default()
            }
        };

        let (themes, theme_problems) = theme::load_all();
        problems.extend(
            theme_problems.into_iter().map(|text| Problem { source: Source::Themes, text }),
        );
        let theme =
            themes.iter().find(|t| t.id == config.theme).cloned().unwrap_or_else(theme::system);

        let config_curves = read_config_curves(&settings.resolved_config_path());
        let mut session = Session {
            settings,
            saved: config.clone(),
            config,
            saved_look: look.clone(),
            look,
            config_curves,
            presets: Vec::new(),
            shaders: Vec::new(),
            drafts: BTreeMap::new(),
            theme,
            themes,
            live: Live::default(),
            problems,
        };
        session.rescan_shaders();
        session.refresh_live();
        session.rescan_presets();
        session
    }

    /// Re-read the themes folder.
    pub fn rescan_presets(&mut self) {
        let (entries, problems) = preset::list_in(&preset::dir());
        self.forget(Source::Presets);
        self.problems
            .extend(problems.into_iter().map(|text| Problem { source: Source::Presets, text }));
        self.presets = entries;
    }

    /// The theme a payload names: by `file` for one in the folder, by `name`
    /// for one built in.
    fn preset_named(&self, payload: &Value) -> Result<&preset::Entry> {
        let file = payload.get("file").and_then(Value::as_str).filter(|f| !f.is_empty());
        let name = payload.get("name").and_then(Value::as_str).unwrap_or_default();
        self.presets
            .iter()
            .find(|e| match (file, &e.path) {
                (Some(f), Some(p)) => p.to_string_lossy() == f,
                (None, None) => e.preset.name == name,
                _ => false,
            })
            .ok_or_else(|| Error::other(format!("there is no theme {}", file.unwrap_or(name))))
    }

    fn presets_view(&self) -> Value {
        Value::Array(
            self.presets
                .iter()
                .map(|e| {
                    json!({
                        "name": e.preset.name,
                        "description": e.preset.description,
                        "file": e.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
                        "builtin": e.path.is_none(),
                        "active": preset::matches(&e.preset, &self.look, &self.config),
                        "hasShaders": e.preset.shaders.is_some(),
                        "ruleCount": e.preset.shaders.as_ref().map_or(0, |s| s.rules.len()),
                        "layerCount": e.preset.shaders.as_ref().map_or(0, |s| s.layers.len()),
                        "shaderFiles": e.preset.shaders.as_ref().map_or(0, |s| s.files.len()),
                        "options": e.preset.options,
                        "curveCount": e.preset.curves.len(),
                        "animationCount": e.preset.animations.len(),
                    })
                })
                .collect(),
        )
    }

    /// Shaders a theme would have to replace or keep beside the user's own,
    /// for the page to ask about before applying it.
    pub fn preset_conflicts(&self, payload: &Value) -> Result<Vec<String>> {
        let entry = self.preset_named(payload)?;
        Ok(entry.preset.shaders.as_ref().map_or_else(Vec::new, |setup| {
            preset::shader_conflicts(setup, &paths::expand(&self.config.shader_dir))
        }))
    }

    fn apply_preset(&mut self, payload: &Value) -> Result<Option<String>> {
        let entry = self.preset_named(payload)?.clone();
        let look_only = payload.get("lookOnly").and_then(Value::as_bool).unwrap_or(false);

        // The shaders first: they write files, and can refuse, and a refusal
        // should leave the look as it was too.
        let mut shader_notes = Vec::new();
        let mut shaders_applied = false;
        if let (Some(setup), false) = (&entry.preset.shaders, look_only) {
            let dir = paths::expand(&self.config.shader_dir);
            let on_conflict = match payload.get("onConflict").and_then(Value::as_str) {
                Some("replace") => preset::OnConflict::Replace,
                Some("keep_both") => preset::OnConflict::KeepBoth,
                _ => {
                    let conflicts = preset::shader_conflicts(setup, &dir);
                    if !conflicts.is_empty() {
                        return Err(Error::other(format!(
                            "{} would change shaders you already have ({}) — say whether to \
                             replace them or keep both",
                            entry.preset.name,
                            conflicts.join(", ")
                        )));
                    }
                    preset::OnConflict::Replace
                }
            };
            let (rules, layers, notes) = preset::install_shaders(
                setup,
                &dir,
                &entry.preset.name,
                on_conflict,
                self.settings.backups_to_keep,
            )?;
            self.config.rules = rules;
            self.config.layers = layers;
            shader_notes = notes;
            shaders_applied = true;
            self.rescan_shaders();
        }

        let (look, problems) = preset::to_look(&entry.preset, &self.look, &self.live.motion);
        self.look = look;
        if self.look_is_live() {
            self.resync_look()?;
        }
        let mut msg = format!("Applied {} — Save to keep it", entry.preset.name);
        if shaders_applied {
            msg.push_str(". Its shader rules take effect when you save");
            if !shader_notes.is_empty() {
                msg.push_str(&format!(" ({})", shader_notes.join("; ")));
            }
        }
        if !problems.is_empty() {
            msg.push_str(&format!(". Left out: {}", problems.join("; ")));
        }
        Ok(Some(msg))
    }

    /// True when the config has unsaved changes.
    ///
    /// Shader edits are not part of this: they are staged per file and written
    /// by the Shaders page's own save, so a dirty shader must not light up a
    /// Save button that writes the Hyprland config. See [`Session::dirty_shaders`].
    pub fn dirty(&self) -> bool {
        self.config != self.saved || self.look != self.saved_look
    }

    /// How many shaders have edits staged but not written.
    pub fn dirty_shaders(&self) -> usize {
        self.drafts.values().filter(|d| !d.is_empty()).count()
    }

    /// The staged edits for one shader, if it has any.
    fn draft(&self, path: &str) -> Option<&ShaderDraft> {
        self.drafts.get(path).filter(|d| !d.is_empty())
    }

    /// The shader at `path`, or an error naming it.
    fn shader_at(&self, path: &str) -> Result<&ShaderInfo> {
        self.shaders
            .iter()
            .find(|s| s.path == path)
            .ok_or_else(|| Error::other(format!("{} is not in the shader list", base_name(path))))
    }

    /// Drop every problem one source reported, before it reports again.
    fn forget(&mut self, source: Source) {
        self.problems.retain(|p| p.source != source);
    }

    /// The problems as the UI shows them.
    pub fn messages(&self) -> Vec<String> {
        self.problems.iter().map(|p| p.text.clone()).collect()
    }

    /// Re-read the shader directory.
    pub fn rescan_shaders(&mut self) {
        let dir = paths::expand(&self.config.shader_dir);
        self.forget(Source::Shaders);
        match shader::scan_dir(&dir) {
            Ok(list) => {
                self.shaders = list;
                self.prune_drafts();
            }
            Err(e) => {
                self.shaders.clear();
                self.problems.push(Problem { source: Source::Shaders, text: e.to_string() });
            }
        }
    }

    /// Ask the compositor what is on screen. Never fails; absence is just
    /// reported as "not running".
    ///
    /// This runs three `hyprctl` processes and takes tens of milliseconds, so
    /// a UI should prefer [`probe_live`] on a background thread and hand the
    /// answer back through [`Session::set_live`].
    pub fn refresh_live(&mut self) {
        self.live = probe_live();
    }

    /// Store the result of a [`probe_live`] run made elsewhere.
    pub fn set_live(&mut self, live: Live) {
        self.live = live;
    }

    /// Re-read the theme list and resolve the configured one.
    fn refresh_theme(&mut self) {
        let (themes, problems) = theme::load_all();
        self.forget(Source::Themes);
        self.problems
            .extend(problems.into_iter().map(|text| Problem { source: Source::Themes, text }));
        self.theme = themes
            .iter()
            .find(|t| t.id == self.config.theme)
            .cloned()
            .unwrap_or_else(theme::system);
        self.themes = themes;
    }

    // -----------------------------------------------------------------------
    // Saving
    // -----------------------------------------------------------------------

    /// Write the managed block back into the Lua config.
    ///
    /// The rest of the file is preserved byte for byte, a timestamped backup is
    /// taken first, and the write is atomic.
    pub fn save(&mut self) -> Result<String> {
        let path = self.settings.resolved_config_path();
        let existing = if path.exists() { paths::read_to_string(&path)? } else { String::new() };

        // The shader block is rewritten only when something in it changed. A
        // save from the Look page must leave it byte for byte as it was:
        // regenerating it would restamp it and could quietly change lines a
        // different version of the emitter wrote. And someone who only uses
        // the Look page should not find an empty shader block appear at all.
        let mut updated = existing.clone();
        let shade_changed = self.config != self.saved;
        let shade_exists = block::split(&existing)?.is_some();
        if shade_changed && (shade_exists || self.config != Config::default()) {
            updated = block::splice(&updated, &emit::emit(&self.config)?)?;
        }
        updated = if self.look.is_empty() {
            block::remove_with(&updated, &look::MARKERS)?
        } else {
            block::splice_with(&updated, &look::emit(&self.look)?, &look::MARKERS)?
        };

        let look_changed = self.look != self.saved_look;
        let backup = paths::backup(&path, self.settings.backups_to_keep)?;
        paths::write_atomic(&path, &updated)?;
        self.saved = self.config.clone();
        self.saved_look = self.look.clone();
        self.config_curves = motion::curves_in_config(&updated);

        let mut msg = format!("Saved to {}", paths::contract(&path));
        if let Some(b) = backup {
            msg.push_str(&format!(" (backup: {})", paths::contract(&b)));
        }
        let orphans = self.orphaned_curves();
        if !orphans.is_empty() {
            msg.push_str(&format!(
                ". Note: {} — fine if another file your config loads defines it, but otherwise \
                 that animation will not load after Hyprland restarts",
                orphans.join("; ")
            ));
        }

        if self.live.running && self.settings.reload_after_save {
            match hyprctl::reload_config() {
                Ok(()) => msg.push_str(" and reloaded Hyprland"),
                Err(e) => msg.push_str(&format!(", but the reload failed: {e}")),
            }
        } else if self.live.running && look_changed {
            // The desktop already shows the new look, edited live, but an
            // override taken away is still showing until the file is re-read.
            // A config-only reload also proves the saved block takes effect.
            if let Err(e) = hyprctl::reload_config_only() {
                msg.push_str(&format!(", but the reload failed: {e}"));
            }
        }
        if self.live.running && look_changed {
            self.refresh_live_look();
            let lost = self.overridden_elsewhere();
            if !lost.is_empty() {
                msg.push_str(&format!(
                    ". Something later in your config sets {} again, so the app's value does not \
                     stick — move the HyprCustard look block below it",
                    lost.join(", ")
                ));
            }
        }
        Ok(msg)
    }

    /// Throw away unsaved changes and re-read the file.
    pub fn reload_from_disk(&mut self) -> Result<String> {
        let path = self.settings.resolved_config_path();
        let config = read_config(&path)?;
        let look = read_look(&path)?;
        let look_was_dirty = self.look != look;
        self.config = config.clone();
        self.saved = config;
        self.look = look.clone();
        self.saved_look = look;
        self.config_curves = read_config_curves(&path);
        // Unsaved look edits are on the desktop as well as in the app.
        if look_was_dirty && self.live.running {
            self.resync_look()?;
        }
        self.rescan_shaders();
        self.refresh_theme();
        Ok(format!("Reloaded {}", paths::contract(&path)))
    }

    /// A preview of the Lua that would be written.
    pub fn preview(&self) -> Result<String> {
        let body = emit::emit(&self.config)?;
        Ok(format!("{}\n{}\n{}\n", block::BEGIN, body.trim_end_matches('\n'), block::END))
    }

    /// Remove the managed block from the config file entirely.
    pub fn remove_block(&mut self) -> Result<String> {
        let path = self.settings.resolved_config_path();
        if !path.exists() {
            return Err(Error::other(format!("{} does not exist", paths::contract(&path))));
        }
        let existing = paths::read_to_string(&path)?;
        let updated = block::remove(&existing)?;
        let backup = paths::backup(&path, self.settings.backups_to_keep)?;
        paths::write_atomic(&path, &updated)?;

        let mut msg = format!("Removed the managed block from {}", paths::contract(&path));
        if let Some(b) = backup {
            msg.push_str(&format!(" (backup: {})", paths::contract(&b)));
        }
        Ok(msg)
    }

    // -----------------------------------------------------------------------
    // The view the UI renders
    // -----------------------------------------------------------------------

    /// Everything the UI needs, as one JSON document.
    pub fn state(&self) -> Value {
        let config_path = self.settings.resolved_config_path();
        let shader_dir = paths::expand(&self.config.shader_dir);
        let referenced = self.config.referenced_shaders();

        json!({
            "dirty": self.dirty(),
            "configPath": config_path.to_string_lossy(),
            "configPathDisplay": paths::contract(&config_path),
            "configExists": config_path.exists(),
            "shaderDir": self.config.shader_dir,
            "shaderDirDisplay": paths::contract(&shader_dir),
            "useShaderDirVariable": self.config.use_shader_dir_variable,
            "loadMode": match self.config.load_mode {
                LoadMode::None => "none",
                LoadMode::Hyprpm => "hyprpm",
                LoadMode::Manual => "manual",
            },
            "pluginSoPath": self.config.plugin_so_path,
            "startupDelaySecs": self.config.startup_delay_secs,
            "live": {
                "running": self.live.running,
                "pluginLoaded": self.live.plugin_loaded,
                "classes": self.live.classes,
                "namespaces": self.live.namespaces,
            },
            "theme": self.theme,
            "themes": self.themes.iter().map(|t| json!({
                "id": t.id, "name": t.name, "dark": t.dark,
            })).collect::<Vec<_>>(),
            "themeDir": paths::contract(&theme::theme_dir()),
            "settings": self.settings,
            "rules": self.config.rules.iter().map(|r| self.rule_view(r)).collect::<Vec<_>>(),
            "layers": self.config.layers,
            "binds": self.config.binds.iter().map(|b| json!({
                "id": b.id,
                "enabled": b.enabled,
                "key": b.key,
                "action": b.action,
                "summary": b.action.summary(),
            })).collect::<Vec<_>>(),
            "startup": self.config.startup.iter().map(|s| json!({
                "id": s.id,
                "enabled": s.enabled,
                "action": s.action,
                "summary": s.action.summary(),
            })).collect::<Vec<_>>(),
            "shaders": self.shaders
                .iter()
                .map(|s| self.shader_view(s, &referenced))
                .collect::<Vec<_>>(),
            "dirtyShaders": self.dirty_shaders(),
            "missingShaders": shader::missing(&referenced)
                .iter()
                .map(|p| json!({ "path": p, "name": base_name(p) }))
                .collect::<Vec<_>>(),
            "look": look::view(&self.look, &self.saved_look, &self.live.look, self.live.running),
            "presets": self.presets_view(),
            "presetDir": paths::contract(&preset::dir()),
            "motion": motion::view(
                &self.look.curves,
                &self.look.animations,
                &self.saved_look.curves,
                &self.saved_look.animations,
                &self.live.motion,
                &self.config_curves,
            ),
            "tagCatalog": tag_catalog(),
            "uniformCatalog": uniform_catalog(),
            "problems": self.messages(),
            "appVersion": env!("CARGO_PKG_VERSION"),
        })
    }

    /// One shader as the UI sees it: what its file says, with any staged edits
    /// laid over the top and marked as staged.
    ///
    /// `value`, `duration` and `overlay` are always what the user is looking at
    /// and what the plugin will see once it is saved; the `saved*` fields are
    /// what the file still says. A revert button needs both, and so does a
    /// preview that wants to show the edit before it exists on disk.
    fn shader_view(&self, s: &ShaderInfo, referenced: &[String]) -> Value {
        let draft = self.draft(&s.path);
        let duration = draft.map_or(s.duration, |d| d.duration_over(s.duration));
        let overlay = draft.map_or(s.overlay, |d| d.overlay_over(s.overlay));

        json!({
            "path": s.path,
            "name": s.name,
            "stem": s.stem,
            "display": paths::contract(std::path::Path::new(&s.path)),
            "description": s.description,
            "duration": duration,
            "savedDuration": s.duration,
            "durationStaged": draft.is_some_and(|d| d.duration_staged()),
            // The plugin's own fallback when a shader declares no duration.
            "effectiveDuration": duration.unwrap_or(0.3),
            "overlay": overlay,
            "savedOverlay": s.overlay,
            "overlayStaged": draft.is_some_and(|d| d.overlay_staged()),
            "uniforms": s.uniforms,
            "params": s.params.iter().map(|p| param_view(p, draft)).collect::<Vec<_>>(),
            "notes": s.notes,
            "isAnimation": s.is_animation(),
            "isMotionDriven": s.is_motion_driven(),
            "inUse": referenced.contains(&s.path),
            "staged": draft.map_or(0, ShaderDraft::len),
            "dirty": draft.is_some(),
        })
    }

    fn rule_view(&self, rule: &WindowRule) -> Value {
        json!({
            "id": rule.id,
            "name": rule.name,
            "enabled": rule.enabled,
            "match": rule.match_,
            "matchSummary": rule.match_.summary(),
            "matchesEverything": rule.match_.is_empty(),
            "tags": rule.tags.iter().map(|t| {
                let info = t.slot.info();
                let (path, duration, flag) = match &t.value {
                    TagValue::Path(r) => (
                        Value::String(r.path.clone()),
                        r.duration.map(|d| json!(d)).unwrap_or(Value::Null),
                        Value::Null,
                    ),
                    TagValue::Flag { on } => (Value::Null, Value::Null, json!(on)),
                };
                json!({
                    "slot": t.slot,
                    "key": info.key,
                    "label": info.label,
                    "isDefault": t.is_default,
                    "path": path,
                    "pathName": match &t.value {
                        TagValue::Path(r) => json!(base_name(&r.path)),
                        _ => Value::Null,
                    },
                    "duration": duration,
                    "flag": flag,
                })
            }).collect::<Vec<_>>(),
        })
    }

    // -----------------------------------------------------------------------
    // Commands
    // -----------------------------------------------------------------------

    /// Run one named command.
    ///
    /// Returns a message for the status bar, or `None` when the change speaks
    /// for itself.
    pub fn command(&mut self, name: &str, payload: &Value) -> Result<Option<String>> {
        match name {
            // --- rules ---
            "rule.add" => {
                let id = self.config.next_id("rule");
                let mut rule = WindowRule::new(&id);
                if let Some(class) = payload.get("class").and_then(Value::as_str) {
                    rule.match_.class = Some(class.to_string());
                    rule.name = format!("{}-shade", slugify(class));
                }
                self.config.rules.push(rule);
                Ok(Some(format!("Added rule {id}")))
            }
            "rule.update" => {
                let rule: WindowRule = from_field(payload, "rule")?;
                let idx = self.rule_index(&rule.id)?;
                self.config.rules[idx] = rule;
                Ok(None)
            }
            "rule.remove" => {
                let idx = self.rule_index(id_of(payload)?)?;
                let removed = self.config.rules.remove(idx);
                Ok(Some(format!("Removed rule for {}", removed.match_.summary())))
            }
            "rule.duplicate" => {
                let idx = self.rule_index(id_of(payload)?)?;
                let mut copy = self.config.rules[idx].clone();
                copy.id = self.config.next_id("rule");
                if !copy.name.is_empty() {
                    copy.name = format!("{}-copy", copy.name);
                }
                self.config.rules.insert(idx + 1, copy);
                Ok(Some("Duplicated rule".into()))
            }
            "rule.move" => {
                let idx = self.rule_index(id_of(payload)?)?;
                let delta = payload.get("delta").and_then(Value::as_i64).unwrap_or(0);
                move_item(&mut self.config.rules, idx, delta);
                Ok(None)
            }
            "rule.setEnabled" => {
                let idx = self.rule_index(id_of(payload)?)?;
                self.config.rules[idx].enabled =
                    payload.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                Ok(None)
            }
            "rule.setName" => {
                let idx = self.rule_index(id_of(payload)?)?;
                self.config.rules[idx].name = payload
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                Ok(None)
            }
            "rule.setMatch" => {
                let idx = self.rule_index(id_of(payload)?)?;
                let field = |key: &str| -> Option<String> {
                    payload
                        .get(key)
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                };
                let m = &mut self.config.rules[idx].match_;
                if payload.get("class").is_some() {
                    m.class = field("class");
                }
                if payload.get("title").is_some() {
                    m.title = field("title");
                }
                if payload.get("initialClass").is_some() {
                    m.initial_class = field("initialClass");
                }
                if payload.get("initialTitle").is_some() {
                    m.initial_title = field("initialTitle");
                }
                Ok(None)
            }
            "rule.setTag" => self.set_tag(payload),
            "rule.clearTag" => {
                let idx = self.rule_index(id_of(payload)?)?;
                let slot = slot_of(payload)?;
                self.config.rules[idx].set_tag(None, slot);
                Ok(None)
            }

            // --- layers ---
            "layer.add" => {
                let ns =
                    payload.get("namespace").and_then(Value::as_str).unwrap_or("*").to_string();
                if self.config.layers.iter().any(|l| l.namespace == ns) {
                    return Err(Error::other(format!("{ns} is already in the list")));
                }
                let id = self.config.next_id("layer");
                self.config.layers.push(LayerEntry::new(id, &ns));
                Ok(Some(format!("Added layer {ns}")))
            }
            "layer.update" => {
                let entry: LayerEntry = from_field(payload, "layer")?;
                let idx = self.layer_index(&entry.id)?;
                self.config.layers[idx] = entry;
                Ok(None)
            }
            "layer.remove" => {
                let idx = self.layer_index(id_of(payload)?)?;
                let removed = self.config.layers.remove(idx);
                Ok(Some(format!("Removed layer {}", removed.namespace)))
            }

            // --- binds and startup ---
            "bind.add" => {
                let id = self.config.next_id("bind");
                self.config.binds.push(Bind {
                    id: id.clone(),
                    enabled: true,
                    key: payload
                        .get("key")
                        .and_then(Value::as_str)
                        .unwrap_or("SUPER + W")
                        .to_string(),
                    action: Action::ReloadShaders,
                });
                Ok(Some(format!("Added keybind {id}")))
            }
            "bind.update" => {
                let bind: Bind = from_field(payload, "bind")?;
                let idx = self
                    .config
                    .binds
                    .iter()
                    .position(|b| b.id == bind.id)
                    .ok_or_else(|| Error::other(format!("no keybind {}", bind.id)))?;
                self.config.binds[idx] = bind;
                Ok(None)
            }
            "bind.remove" => {
                let id = id_of(payload)?;
                let before = self.config.binds.len();
                self.config.binds.retain(|b| b.id != id);
                if self.config.binds.len() == before {
                    return Err(Error::other(format!("no keybind {id}")));
                }
                Ok(Some("Removed keybind".into()))
            }
            "startup.add" => {
                let id = self.config.next_id("start");
                self.config.startup.push(StartupAction {
                    id: id.clone(),
                    enabled: true,
                    action: Action::ReloadShaders,
                });
                Ok(Some(format!("Added startup action {id}")))
            }
            "startup.update" => {
                let action: StartupAction = from_field(payload, "startup")?;
                let idx = self
                    .config
                    .startup
                    .iter()
                    .position(|s| s.id == action.id)
                    .ok_or_else(|| Error::other(format!("no startup action {}", action.id)))?;
                self.config.startup[idx] = action;
                Ok(None)
            }
            "startup.remove" => {
                let id = id_of(payload)?;
                self.config.startup.retain(|s| s.id != id);
                Ok(Some("Removed startup action".into()))
            }

            // --- config-level settings ---
            "config.update" => self.update_config(payload),
            "config.save" => self.save().map(Some),
            "config.reload" => self.reload_from_disk().map(Some),
            "config.removeBlock" => self.remove_block().map(Some),

            // --- app settings ---
            "settings.update" => {
                let settings: Settings = from_field(payload, "settings")?;
                let path_changed = settings.config_path != self.settings.config_path;
                self.settings = settings;
                self.settings.save()?;
                if path_changed {
                    return self.reload_from_disk().map(Some);
                }
                Ok(Some("Settings saved".into()))
            }

            // --- shaders ---
            "shader.rescan" => {
                self.rescan_shaders();
                Ok(Some(format!("Found {} shaders", self.shaders.len())))
            }
            "shader.setParam" => self.stage_shader_param(payload),
            "shader.setDuration" => {
                let seconds = payload.get("seconds").and_then(Value::as_f64).map(|v| v as f32);
                self.stage(payload, |draft, info| draft.set_duration(info, seconds))?;
                Ok(None)
            }
            "shader.setOverlay" => {
                let on = payload.get("overlay").and_then(Value::as_bool).unwrap_or(false);
                self.stage(payload, |draft, info| draft.set_overlay(info, on))?;
                Ok(None)
            }
            "shader.revert" => self.revert_shader(payload),
            "shader.save" => self.save_shader(payload),
            "shader.reload" => {
                hyprctl::reload_shaders()?;
                Ok(Some("Asked the plugin to reload its shaders".into()))
            }
            "shader.read" => {
                let path = path_of(payload)?;
                let text = paths::read_to_string(&path)?;
                Ok(Some(text))
            }

            // --- live preview ---
            "live.apply" => {
                let action: Action = from_field(payload, "action")?;
                hyprctl::dispatch(&action)?;
                Ok(Some(format!("Applied: {}", action.summary())))
            }
            "live.refresh" => {
                self.refresh_live();
                Ok(None)
            }

            // --- Hyprland look ---
            "look.set" => {
                let key = payload.get("key").and_then(Value::as_str).unwrap_or_default();
                let opt = look::require(key)?;
                let value = look::parse(opt, payload.get("value").unwrap_or(&Value::Null))?;
                // Apply before storing, so a value Hyprland refuses is never
                // saved into the block.
                if self.look_is_live() {
                    look::apply_live(&[(key, &value)])?;
                    self.live.look.insert(key.to_string(), value.clone());
                }
                self.look.values.insert(key.to_string(), value);
                Ok(None)
            }
            "look.reset" => {
                let key = payload.get("key").and_then(Value::as_str).unwrap_or_default();
                if self.look.values.remove(key).is_some() && self.look_is_live() {
                    self.resync_look()?;
                }
                Ok(None)
            }
            "look.resetAll" => {
                let n = self.look.values.len();
                self.look.values.clear();
                if n > 0 && self.look_is_live() {
                    self.resync_look()?;
                }
                Ok(Some(format!(
                    "Cleared {n} look setting{} — save to take them out of hyprland.lua",
                    if n == 1 { "" } else { "s" }
                )))
            }
            "motion.set" => self.set_animation(payload),
            "motion.reset" => {
                let name = payload.get("leaf").and_then(Value::as_str).unwrap_or_default();
                if self.look.animations.remove(name).is_some() && self.look_is_live() {
                    self.resync_look()?;
                }
                Ok(None)
            }
            "motion.resetAll" => {
                let n = self.look.animations.len() + self.look.curves.len();
                self.look.animations.clear();
                self.look.curves.clear();
                if n > 0 && self.look_is_live() {
                    self.resync_look()?;
                }
                Ok(Some("Cleared every animation and curve set here".into()))
            }
            "preset.save" => {
                let name = payload.get("name").and_then(Value::as_str).unwrap_or_default();
                let description =
                    payload.get("description").and_then(Value::as_str).unwrap_or_default();
                let replace = payload.get("replace").and_then(Value::as_bool).unwrap_or(false);
                let with_shaders =
                    payload.get("includeShaders").and_then(Value::as_bool).unwrap_or(true);
                if self.presets.iter().any(|e| e.path.is_none() && e.preset.name == name.trim()) {
                    return Err(Error::other(format!("{name} is a built-in theme's name")));
                }
                let mut p = preset::snapshot(
                    name,
                    description,
                    &self.look,
                    &self.live.look,
                    &self.live.motion,
                    &self.config_curves,
                );
                if with_shaders {
                    p.shaders = preset::capture_shaders(&self.config);
                }
                let path = preset::save_into(&preset::dir(), &p, replace)?;
                self.rescan_presets();
                Ok(Some(format!("Saved the theme {} to {}", p.name, paths::contract(&path))))
            }
            "preset.apply" => self.apply_preset(payload),
            "preset.delete" => {
                let entry = self.preset_named(payload)?.clone();
                let path =
                    entry.path.ok_or_else(|| Error::other("built-in themes cannot be deleted"))?;
                std::fs::remove_file(&path).map_err(|e| Error::io(&path, e))?;
                self.rescan_presets();
                Ok(Some(format!("Deleted the theme {}", entry.preset.name)))
            }
            "preset.import" => {
                let from =
                    paths::expand(payload.get("path").and_then(Value::as_str).unwrap_or_default());
                let p = preset::load(&from)?;
                let replace = payload.get("replace").and_then(Value::as_bool).unwrap_or(false);
                let path = preset::save_into(&preset::dir(), &p, replace)?;
                self.rescan_presets();
                Ok(Some(format!("Imported {} into {}", p.name, paths::contract(&path))))
            }
            "preset.export" => {
                let entry = self.preset_named(payload)?.clone();
                let to =
                    paths::expand(payload.get("to").and_then(Value::as_str).unwrap_or_default());
                if to.as_os_str().is_empty() {
                    return Err(Error::other("no file to export to"));
                }
                let text = serde_json::to_string_pretty(&entry.preset)?;
                paths::write_atomic(&to, &format!("{text}\n"))?;
                Ok(Some(format!("Exported {} to {}", entry.preset.name, paths::contract(&to))))
            }
            "preset.rescan" => {
                self.rescan_presets();
                Ok(None)
            }
            "curve.set" => self.set_curve(payload),
            "curve.remove" => self.remove_curve(payload),
            "look.discard" => {
                self.look = self.saved_look.clone();
                if self.live.running {
                    self.resync_look()?;
                }
                Ok(Some("Put the look back to what hyprland.lua says".into()))
            }
            "look.resync" => {
                if self.live.running {
                    self.resync_look()?;
                }
                Ok(None)
            }

            // --- import ---
            "import.apply" => self.apply_import(),

            // --- themes ---
            "theme.writeExample" => {
                let p = theme::write_example()?;
                self.refresh_theme();
                Ok(Some(format!("Wrote {}", paths::contract(&p))))
            }
            "theme.reload" => {
                self.refresh_theme();
                Ok(Some(format!("{} themes available", self.themes.len())))
            }

            other => Err(Error::other(format!("unknown command `{other}`"))),
        }
    }

    /// Whether look edits should reach the compositor as they are made.
    fn look_is_live(&self) -> bool {
        self.live.running && self.settings.live_look_edits
    }

    /// Put one look value on the desktop without storing it — for following
    /// a slider while it is dragged. [`Session::command`] `look.set` stores it
    /// once the slider is let go.
    pub fn try_look(&self, key: &str, value: &Value) -> Result<()> {
        if !self.look_is_live() {
            return Ok(());
        }
        let opt = look::require(key)?;
        let value = look::parse(opt, value)?;
        look::apply_live(&[(key, &value)])
    }

    /// Make the desktop match the app: re-read the file, then lay the
    /// unsaved overrides back over it.
    ///
    /// Hyprland cannot unset an option, so taking an override away live means
    /// reloading the config and re-applying everything else.
    fn resync_look(&mut self) -> Result<()> {
        hyprctl::reload_config_only()?;
        if self.settings.live_look_edits {
            look::apply_all_live(&self.look)?;
        }
        self.refresh_live_look();
        Ok(())
    }

    /// Re-read the look options from the compositor.
    fn refresh_live_look(&mut self) {
        self.live.look = look::read_live().unwrap_or_default();
        self.live.motion = motion::read_live().unwrap_or_default();
    }

    /// Labels of the saved overrides the compositor does not show, after a
    /// reload — meaning something later in the file sets them again.
    fn overridden_elsewhere(&self) -> Vec<String> {
        let mut lost: Vec<String> = self
            .saved_look
            .values
            .iter()
            .filter(|(k, v)| self.live.look.get(*k).is_some_and(|live| !live.same_as(v)))
            .map(|(k, _)| look::find(k).map_or_else(|| k.clone(), |o| o.label.to_lowercase()))
            .collect();
        lost.extend(motion::not_sticking(&self.saved_look.animations, &self.live.motion));
        lost
    }

    /// Animations set here whose curve nothing in hyprland.lua defines.
    ///
    /// Hyprland keeps curves until it restarts, so one left over from an
    /// earlier edit works now and is gone after the next login.
    fn orphaned_curves(&self) -> Vec<String> {
        self.look
            .animations
            .iter()
            .filter(|(_, a)| {
                a.enabled
                    && a.bezier != "default"
                    && !self.look.curves.contains_key(&a.bezier)
                    && !self.config_curves.contains_key(&a.bezier)
            })
            .map(|(leaf, a)| {
                format!("{leaf} uses the curve {}, which hyprland.lua does not define", a.bezier)
            })
            .collect()
    }

    /// Set one animation leaf.
    fn set_animation(&mut self, payload: &Value) -> Result<Option<String>> {
        let name = payload.get("leaf").and_then(Value::as_str).unwrap_or_default();
        let leaf = motion::find(name)
            .ok_or_else(|| Error::other(format!("no animation called {name}")))?;
        let setting = motion::parse_setting(leaf, payload.get("setting").unwrap_or(&Value::Null))?;
        if setting.enabled && self.live.running {
            let known = self.look.curves.contains_key(&setting.bezier)
                || self.live.motion.curves.contains_key(&setting.bezier)
                || self.config_curves.contains_key(&setting.bezier);
            if !known {
                return Err(Error::other(format!("there is no curve called {}", setting.bezier)));
            }
        }
        if self.look_is_live() {
            look::eval(motion::animation_lua(name, &setting).trim_end())?;
            self.refresh_live_look();
        }
        self.look.animations.insert(name.to_string(), setting);
        Ok(None)
    }

    /// Define or redefine one curve.
    fn set_curve(&mut self, payload: &Value) -> Result<Option<String>> {
        let name = payload.get("name").and_then(Value::as_str).unwrap_or_default().trim();
        motion::check_curve_name(name)?;
        let points = motion::parse_points(payload.get("points").unwrap_or(&Value::Null))?;
        let fresh =
            !self.look.curves.contains_key(name) && !self.live.motion.curves.contains_key(name);
        if self.look_is_live() {
            look::eval(motion::curve_lua(name, &points).trim_end())?;
            self.live.motion.curves.insert(name.to_string(), points);
        }
        self.look.curves.insert(name.to_string(), points);
        Ok(fresh.then(|| format!("Added the curve {name}")))
    }

    /// Stop defining a curve here.
    fn remove_curve(&mut self, payload: &Value) -> Result<Option<String>> {
        let name = payload.get("name").and_then(Value::as_str).unwrap_or_default();
        if !self.look.curves.contains_key(name) {
            return Err(Error::other(format!("{name} is not defined here")));
        }
        // Not while something set here uses it: if the curve exists only in
        // this block, Hyprland would refuse those animations on the next read.
        let users: Vec<&str> = self
            .look
            .animations
            .iter()
            .filter(|(_, a)| a.enabled && a.bezier == name)
            .map(|(n, _)| n.as_str())
            .collect();
        if !users.is_empty() {
            return Err(Error::other(format!(
                "{name} is still used by {} — give those another curve first",
                users.join(", ")
            )));
        }
        self.look.curves.remove(name);
        if self.look_is_live() {
            self.resync_look()?;
        }
        Ok(Some(format!("Removed the curve {name}")))
    }

    /// A preview of what an import would pull in.
    pub fn import_preview(&self) -> Result<Value> {
        let path = self.settings.resolved_config_path();
        if !path.exists() {
            return Err(Error::other(format!("{} does not exist", paths::contract(&path))));
        }
        let text = paths::read_to_string(&path)?;
        let found = import::from_document(&text);
        Ok(json!({
            "summary": found.summary(),
            "empty": found.is_empty(),
            "rules": found.rules.iter().map(|r| json!({
                "name": r.name,
                "match": r.match_.summary(),
                "tags": r.tags.iter().map(|t| t.to_tag_string()).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "layers": found.layers.iter().map(|l| json!({
                "namespace": l.namespace,
                "shader": l.shader.as_ref().map(|s| base_name(&s.path)),
                "openAnim": l.open_anim.as_ref().map(|s| base_name(&s.path)),
                "closeAnim": l.close_anim.as_ref().map(|s| base_name(&s.path)),
            })).collect::<Vec<_>>(),
            "binds": found.binds.iter().map(|b| json!({
                "key": b.key, "summary": b.action.summary(),
            })).collect::<Vec<_>>(),
            "startup": found.startup.iter().map(|s| json!({
                "summary": s.action.summary(),
            })).collect::<Vec<_>>(),
            "skipped": found.skipped,
        }))
    }

    fn apply_import(&mut self) -> Result<Option<String>> {
        let path = self.settings.resolved_config_path();
        let text = paths::read_to_string(&path)?;
        let found = import::from_document(&text);
        if found.is_empty() {
            return Ok(Some("Nothing to import".into()));
        }
        let summary = found.summary();

        for mut rule in found.rules {
            rule.id = self.config.next_id("rule");
            self.config.rules.push(rule);
        }
        for mut layer in found.layers {
            if let Some(existing) =
                self.config.layers.iter_mut().find(|l| l.namespace == layer.namespace)
            {
                existing.shader = layer.shader.or(existing.shader.clone());
                existing.open_anim = layer.open_anim.or(existing.open_anim.clone());
                existing.close_anim = layer.close_anim.or(existing.close_anim.clone());
            } else {
                layer.id = self.config.next_id("layer");
                self.config.layers.push(layer);
            }
        }
        for mut bind in found.binds {
            bind.id = self.config.next_id("bind");
            self.config.binds.push(bind);
        }
        for mut start in found.startup {
            start.id = self.config.next_id("start");
            self.config.startup.push(start);
        }

        Ok(Some(format!(
            "Imported {summary}. Nothing has been written yet — review, then Save. \
             The originals are still in your config outside the managed block; \
             remove them yourself once you are happy."
        )))
    }

    // -----------------------------------------------------------------------
    // Command helpers
    // -----------------------------------------------------------------------

    fn update_config(&mut self, payload: &Value) -> Result<Option<String>> {
        let mut rescan = false;
        let mut retheme = false;

        if let Some(dir) = payload.get("shaderDir").and_then(Value::as_str) {
            if dir.trim() != self.config.shader_dir {
                self.config.shader_dir = dir.trim().to_string();
                rescan = true;
            }
        }
        if let Some(v) = payload.get("useShaderDirVariable").and_then(Value::as_bool) {
            self.config.use_shader_dir_variable = v;
        }
        if let Some(v) = payload.get("loadMode").and_then(Value::as_str) {
            self.config.load_mode = match v {
                "hyprpm" => LoadMode::Hyprpm,
                "manual" => LoadMode::Manual,
                _ => LoadMode::None,
            };
        }
        if let Some(v) = payload.get("pluginSoPath").and_then(Value::as_str) {
            self.config.plugin_so_path = v.trim().to_string();
        }
        if let Some(v) = payload.get("startupDelaySecs").and_then(Value::as_f64) {
            self.config.startup_delay_secs = (v as f32).clamp(0.0, 30.0);
        }
        if let Some(v) = payload.get("theme").and_then(Value::as_str) {
            if v != self.config.theme {
                self.config.theme = v.to_string();
                retheme = true;
            }
        }

        if rescan {
            self.rescan_shaders();
        }
        if retheme {
            self.refresh_theme();
        }
        Ok(None)
    }

    fn set_tag(&mut self, payload: &Value) -> Result<Option<String>> {
        let idx = self.rule_index(id_of(payload)?)?;
        let slot = slot_of(payload)?;
        let info = slot.info();

        let tag = match info.kind {
            TagKind::Flag => {
                let on = payload.get("flag").and_then(Value::as_bool).unwrap_or(true);
                if on {
                    Some(Tag { slot, is_default: false, value: TagValue::Flag { on: true } })
                } else {
                    None
                }
            }
            TagKind::Path => {
                let path = payload
                    .get("path")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                match path {
                    None => None,
                    Some(p) => {
                        let duration = payload
                            .get("duration")
                            .and_then(Value::as_f64)
                            .map(|d| (d as f32).clamp(0.0, 5.0))
                            .filter(|_| info.takes_duration);
                        let is_default =
                            payload.get("isDefault").and_then(Value::as_bool).unwrap_or(false)
                                && info.supports_default;
                        Some(Tag {
                            slot,
                            is_default,
                            value: TagValue::Path(ShaderRef { path: p.to_string(), duration }),
                        })
                    }
                }
            }
        };

        self.config.rules[idx].set_tag(tag, slot);
        Ok(None)
    }

    /// Stage one parameter value against the shader's file.
    fn stage_shader_param(&mut self, payload: &Value) -> Result<Option<String>> {
        let name = payload
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::other("no parameter name given"))?
            .to_string();

        let value = match payload.get("value") {
            Some(Value::Number(n)) => ParamValue::Scalar(n.as_f64().unwrap_or(0.0) as f32),
            Some(Value::Bool(b)) => ParamValue::Scalar(if *b { 1.0 } else { 0.0 }),
            Some(Value::Array(items)) => {
                ParamValue::Vector(items.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect())
            }
            _ => return Err(Error::other("no value given")),
        };

        self.stage(payload, |draft, info| draft.set_param(info, &name, value))?;
        Ok(None)
    }

    /// Apply an edit to a shader's draft, and prove the whole draft still fits
    /// the file before keeping it.
    ///
    /// The dry run is what keeps a mistake — a `const` that is not there, or is
    /// declared twice — an error at the moment the slider moves rather than a
    /// surprise at save time, which is where it landed when every edit went
    /// straight to disk. It reads the file and throws the result away; a shader
    /// is a few kilobytes, and being told immediately is worth it.
    fn stage<F>(&mut self, payload: &Value, edit: F) -> Result<()>
    where
        F: FnOnce(&mut ShaderDraft, &ShaderInfo),
    {
        let path = path_of(payload)?;
        let key = path.to_string_lossy().into_owned();
        let info = self.shader_at(&key)?.clone();

        let mut draft = self.drafts.get(&key).cloned().unwrap_or_default();
        edit(&mut draft, &info);

        if !draft.is_empty() {
            let src = paths::read_to_string(&path)?;
            draft.apply(&src)?;
        }

        if draft.is_empty() {
            self.drafts.remove(&key);
        } else {
            self.drafts.insert(key, draft);
        }
        Ok(())
    }

    /// A shader's source with its staged edits applied, without writing them.
    ///
    /// This is what the preview renders: the file as it *would* be saved, so a
    /// slider that has not been committed anywhere is still what you see.
    pub fn shader_preview_source(&self, path: &str) -> Result<String> {
        let expanded = paths::expand(path);
        let src = paths::read_to_string(&expanded)?;
        match self.draft(&expanded.to_string_lossy()) {
            Some(draft) => draft.apply(&src),
            None => Ok(src),
        }
    }

    /// Describe a preview of one shader.
    ///
    /// The session does not run the preview — that owns child processes and
    /// outlives any one command — but it is the only thing that knows what
    /// should be in it.
    pub fn preview_request(
        &self,
        path: &str,
        size: (u32, u32),
        hold_secs: f32,
        background: u32,
        desktop_backdrop: bool,
    ) -> Result<preview::Request> {
        let expanded = paths::expand(path);
        let key = expanded.to_string_lossy().into_owned();
        let info = self.shader_at(&key)?;

        let plugin_so = hyprpm::plugin_so(&self.settings.plugin_name).ok_or_else(|| {
            Error::other(format!(
                "could not find {}.so — install the plugin from the Plugin page first",
                self.settings.plugin_name
            ))
        })?;

        Ok(preview::Request {
            original: expanded.clone(),
            source: self.shader_preview_source(path)?,
            is_animation: info.is_animation(),
            is_motion_driven: info.is_motion_driven(),
            plugin_so,
            size,
            hold_secs,
            background,
            desktop_backdrop,
        })
    }

    /// Throw away one staged edit, or every edit staged for a shader.
    fn revert_shader(&mut self, payload: &Value) -> Result<Option<String>> {
        let path = path_of(payload)?;
        let key = path.to_string_lossy().into_owned();

        let item = match payload.get("item").and_then(Value::as_str) {
            None | Some("all") => DraftItem::All,
            Some("duration") => DraftItem::Duration,
            Some("overlay") => DraftItem::Overlay,
            Some(name) => DraftItem::Param(name.to_string()),
        };

        let Some(draft) = self.drafts.get_mut(&key) else {
            return Ok(None);
        };
        draft.revert(&item);
        let emptied = draft.is_empty();
        if emptied {
            self.drafts.remove(&key);
        }

        Ok(Some(match item {
            DraftItem::All => {
                format!("{} is back to what its file says", base_name(&key))
            }
            DraftItem::Duration => "Duration is back to what the file says".into(),
            DraftItem::Overlay => "Overlay is back to what the file says".into(),
            DraftItem::Param(name) => format!("{name} is back to what the file says"),
        }))
    }

    /// Write one shader's staged edits to its file.
    fn save_shader(&mut self, payload: &Value) -> Result<Option<String>> {
        let path = path_of(payload)?;
        let key = path.to_string_lossy().into_owned();

        let Some(draft) = self.draft(&key).cloned() else {
            return Ok(Some(format!("{} has no unsaved changes", base_name(&key))));
        };
        let staged = draft.len();

        let (info, backup) = shader::save_draft(&path, &draft, self.settings.backups_to_keep)?;
        self.drafts.remove(&key);
        self.replace_shader(info);

        let mut msg = format!(
            "Wrote {staged} change{} to {}",
            if staged == 1 { "" } else { "s" },
            paths::contract(&path)
        );
        if let Some(b) = backup {
            msg.push_str(&format!(" (backup: {})", paths::contract(&b)));
        }

        // The plugin reloads a shader when its mtime changes, so this is only
        // for the case where that is not enough — and only now that something
        // has actually been written.
        if self.settings.reload_shaders_after_edit && self.live.running {
            let _ = hyprctl::reload_shaders();
        }
        Ok(Some(msg))
    }

    fn replace_shader(&mut self, info: ShaderInfo) {
        if let Some(draft) = self.drafts.get_mut(&info.path) {
            draft.prune(&info);
        }
        self.drafts.retain(|_, d| !d.is_empty());
        match self.shaders.iter_mut().find(|s| s.path == info.path) {
            Some(slot) => *slot = info,
            None => self.shaders.push(info),
        }
    }

    /// Drop staged edits the files have caught up with, and any for a shader
    /// that is no longer there.
    fn prune_drafts(&mut self) {
        let shaders = &self.shaders;
        self.drafts.retain(|path, draft| match shaders.iter().find(|s| s.path == *path) {
            Some(info) => {
                draft.prune(info);
                !draft.is_empty()
            }
            None => false,
        });
    }

    fn rule_index(&self, id: &str) -> Result<usize> {
        self.config
            .rules
            .iter()
            .position(|r| r.id == id)
            .ok_or_else(|| Error::other(format!("no rule {id}")))
    }

    fn layer_index(&self, id: &str) -> Result<usize> {
        self.config
            .layers
            .iter()
            .position(|l| l.id == id)
            .ok_or_else(|| Error::other(format!("no layer {id}")))
    }
}

// ---------------------------------------------------------------------------
// Free helpers
// ---------------------------------------------------------------------------

/// Ask the compositor what is on screen.
///
/// Free of `Session`, and so free of anything the UI thread owns: three
/// `hyprctl` processes are roughly a dropped frame's worth of work, which is
/// no business of a thread that is meant to be drawing.
pub fn probe_live() -> Live {
    let running = hyprctl::is_running();
    Live {
        running,
        plugin_loaded: running && hyprctl::plugin_loaded().unwrap_or(false),
        classes: if running { hyprctl::classes().unwrap_or_default() } else { Vec::new() },
        namespaces: if running {
            hyprctl::layer_namespaces().unwrap_or_default()
        } else {
            Vec::new()
        },
        look: if running { look::read_live().unwrap_or_default() } else { BTreeMap::new() },
        motion: if running { motion::read_live().unwrap_or_default() } else { Default::default() },
    }
}

/// The curves the file at `path` defines outside the look block. A file that
/// cannot be read defines none; reading the config proper reports that.
fn read_config_curves(path: &Path) -> BTreeMap<String, Option<motion::Points>> {
    paths::read_to_string(path).map(|text| motion::curves_in_config(&text)).unwrap_or_default()
}

fn read_look(path: &Path) -> Result<Look> {
    if !path.exists() {
        return Ok(Look::default());
    }
    look::read(&paths::read_to_string(path)?)
}

fn read_config(path: &Path) -> Result<Config> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = paths::read_to_string(path)?;
    match block::split(&text)? {
        Some(s) => Ok(block::extract_state(&s.inner)?.unwrap_or_default()),
        None => Ok(Config::default()),
    }
}

fn id_of(payload: &Value) -> Result<&str> {
    payload.get("id").and_then(Value::as_str).ok_or_else(|| Error::other("no id given"))
}

fn path_of(payload: &Value) -> Result<PathBuf> {
    payload
        .get("path")
        .and_then(Value::as_str)
        .map(paths::expand)
        .ok_or_else(|| Error::other("no shader path given"))
}

/// One parameter as the UI sees it.
///
/// `value` is what the user is editing — the staged value when there is one —
/// and `savedValue` is what the file still says, so a per-parameter revert can
/// show what it would go back to.
fn param_view(p: &Param, draft: Option<&ShaderDraft>) -> Value {
    let staged = draft.and_then(|d| d.param(&p.name));
    let mut view = serde_json::to_value(p).unwrap_or_else(|_| json!({}));
    if let Some(obj) = view.as_object_mut() {
        obj.insert("savedValue".into(), json!(p.value));
        obj.insert("staged".into(), json!(staged.is_some()));
        if let Some(value) = staged {
            obj.insert("value".into(), json!(value));
        }
    }
    view
}

fn slot_of(payload: &Value) -> Result<TagSlot> {
    let key =
        payload.get("slot").and_then(Value::as_str).ok_or_else(|| Error::other("no tag given"))?;
    // Accept either the serde name (`fullscreen_stack`) or the config key
    // (`shader_fullscreen_stack`).
    if let Ok(slot) = serde_json::from_value::<TagSlot>(Value::String(key.to_string())) {
        return Ok(slot);
    }
    TagSlot::parse_key(key)
        .map(|(s, _)| s)
        .ok_or_else(|| Error::other(format!("`{key}` is not a shader tag")))
}

fn from_field<T: serde::de::DeserializeOwned>(payload: &Value, field: &str) -> Result<T> {
    let v =
        payload.get(field).ok_or_else(|| Error::other(format!("no `{field}` in the request")))?;
    serde_json::from_value(v.clone()).map_err(Error::State)
}

fn move_item<T>(items: &mut Vec<T>, idx: usize, delta: i64) {
    if items.is_empty() {
        return;
    }
    let target = (idx as i64 + delta).clamp(0, items.len() as i64 - 1) as usize;
    if target == idx {
        return;
    }
    let item = items.remove(idx);
    items.insert(target, item);
}

fn slugify(s: &str) -> String {
    let out: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    out.trim_matches('-').to_string()
}

/// The static tag table, as JSON for the UI.
pub fn tag_catalog() -> Value {
    Value::Array(
        TAGS.iter()
            .map(|(slot, info)| {
                json!({
                    "slot": slot,
                    "key": info.key,
                    "label": info.label,
                    "help": info.help,
                    "kind": match info.kind { TagKind::Path => "path", TagKind::Flag => "flag" },
                    "takesDuration": info.takes_duration,
                    "supportsDefault": info.supports_default,
                    "defaultDocumented": info.default_documented,
                    "group": info.group,
                    "groupLabel": info.group.label(),
                })
            })
            .collect(),
    )
}

/// The uniform reference, as JSON for the UI.
pub fn uniform_catalog() -> Value {
    Value::Array(
        KNOWN_UNIFORMS
            .iter()
            .map(|(name, help)| {
                json!({
                    "name": name,
                    "help": help,
                    "motion": MOTION_UNIFORMS.contains(name),
                })
            })
            .collect(),
    )
}

/// The order tag groups appear in.
pub fn tag_groups() -> Value {
    let groups = [TagGroup::Persistent, TagGroup::State, TagGroup::Animation, TagGroup::Behavior];
    Value::Array(groups.iter().map(|g| json!({ "group": g, "label": g.label() })).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        Session {
            settings: Settings::default(),
            config: Config::default(),
            saved: Config::default(),
            look: Look::default(),
            saved_look: Look::default(),
            config_curves: BTreeMap::new(),
            presets: preset::builtin()
                .into_iter()
                .map(|preset| preset::Entry { preset, path: None })
                .collect(),
            drafts: BTreeMap::new(),
            shaders: Vec::new(),
            theme: theme::gruvbox_dark(),
            themes: theme::builtin(),
            live: Live::default(),
            problems: Vec::new(),
        }
    }

    #[test]
    fn adding_a_rule_names_it_after_the_class() {
        let mut s = session();
        s.command("rule.add", &json!({ "class": "google-chrome" })).unwrap();
        assert_eq!(s.config.rules.len(), 1);
        assert_eq!(s.config.rules[0].name, "google-chrome-shade");
        assert!(s.dirty());
    }

    #[test]
    fn setting_and_clearing_a_tag() {
        let mut s = session();
        s.command("rule.add", &json!({})).unwrap();
        let id = s.config.rules[0].id.clone();

        s.command(
            "rule.setTag",
            &json!({ "id": id, "slot": "shader_open", "path": "/p/x.glsl", "duration": 0.6 }),
        )
        .unwrap();
        let tag = &s.config.rules[0].tags[0];
        assert_eq!(tag.slot, TagSlot::Open);
        match &tag.value {
            TagValue::Path(r) => assert_eq!(r.duration, Some(0.6)),
            _ => panic!("expected a path"),
        }

        s.command("rule.clearTag", &json!({ "id": id, "slot": "shader_open" })).unwrap();
        assert!(s.config.rules[0].tags.is_empty());
    }

    #[test]
    fn a_duration_on_a_slot_that_ignores_it_is_dropped() {
        let mut s = session();
        s.command("rule.add", &json!({})).unwrap();
        let id = s.config.rules[0].id.clone();
        s.command(
            "rule.setTag",
            &json!({ "id": id, "slot": "shader_move", "path": "/p/x.glsl", "duration": 2.0 }),
        )
        .unwrap();
        match &s.config.rules[0].tags[0].value {
            TagValue::Path(r) => assert_eq!(r.duration, None),
            _ => panic!("expected a path"),
        }
    }

    #[test]
    fn an_empty_path_clears_the_slot() {
        let mut s = session();
        s.command("rule.add", &json!({})).unwrap();
        let id = s.config.rules[0].id.clone();
        s.command("rule.setTag", &json!({ "id": id, "slot": "shader", "path": "/p/x.glsl" }))
            .unwrap();
        s.command("rule.setTag", &json!({ "id": id, "slot": "shader", "path": "  " })).unwrap();
        assert!(s.config.rules[0].tags.is_empty());
    }

    #[test]
    fn ids_stay_unique_across_kinds() {
        let mut s = session();
        s.command("rule.add", &json!({})).unwrap();
        s.command("rule.add", &json!({})).unwrap();
        s.command("rule.duplicate", &json!({ "id": "rule1" })).unwrap();
        let ids: Vec<&str> = s.config.rules.iter().map(|r| r.id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(ids.len(), sorted.len());
    }

    #[test]
    fn duplicate_layer_namespaces_are_refused() {
        let mut s = session();
        s.command("layer.add", &json!({ "namespace": "rofi" })).unwrap();
        assert!(s.command("layer.add", &json!({ "namespace": "rofi" })).is_err());
    }

    #[test]
    fn moving_a_rule_clamps_at_the_ends() {
        let mut s = session();
        for _ in 0..3 {
            s.command("rule.add", &json!({})).unwrap();
        }
        let first = s.config.rules[0].id.clone();
        s.command("rule.move", &json!({ "id": first, "delta": -5 })).unwrap();
        assert_eq!(s.config.rules[0].id, first);
        s.command("rule.move", &json!({ "id": first, "delta": 99 })).unwrap();
        assert_eq!(s.config.rules[2].id, first);
    }

    #[test]
    fn rescanning_replaces_its_own_problem_rather_than_stacking_them() {
        let mut s = session();
        s.problems
            .push(Problem { source: Source::Settings, text: "settings.toml is unreadable".into() });
        s.config.shader_dir = "/nonexistent/hws/shaders".into();

        for _ in 0..3 {
            s.rescan_shaders();
        }

        let shader_problems = s.problems.iter().filter(|p| p.source == Source::Shaders).count();
        assert_eq!(shader_problems, 1, "{:?}", s.messages());
        // And the unrelated one is still there.
        assert!(s.messages().iter().any(|m| m.contains("settings.toml")));
    }

    #[test]
    fn a_shader_directory_that_starts_working_clears_its_problem() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session();
        s.config.shader_dir = "/nonexistent/hws/shaders".into();
        s.rescan_shaders();
        assert_eq!(s.problems.iter().filter(|p| p.source == Source::Shaders).count(), 1);

        s.config.shader_dir = dir.path().to_string_lossy().into_owned();
        s.rescan_shaders();
        assert!(s.messages().is_empty(), "{:?}", s.messages());
    }

    #[test]
    fn an_unknown_command_is_an_error_not_a_panic() {
        let mut s = session();
        assert!(s.command("nope.nope", &json!({})).is_err());
    }

    #[test]
    fn an_unknown_tag_slot_is_rejected() {
        let mut s = session();
        s.command("rule.add", &json!({})).unwrap();
        let id = s.config.rules[0].id.clone();
        assert!(s
            .command("rule.setTag", &json!({ "id": id, "slot": "shader_sparkle", "path": "/x" }))
            .is_err());
    }

    #[test]
    fn state_is_valid_json_with_the_keys_the_ui_expects() {
        let s = session();
        let v = s.state();
        for key in [
            "rules",
            "layers",
            "binds",
            "startup",
            "shaders",
            "theme",
            "themes",
            "settings",
            "tagCatalog",
            "uniformCatalog",
            "live",
            "dirty",
        ] {
            assert!(v.get(key).is_some(), "missing {key}");
        }
        assert_eq!(v["tagCatalog"].as_array().unwrap().len(), 20);
    }

    #[test]
    fn save_writes_a_block_and_reading_it_back_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("hyprland.lua");
        std::fs::write(&cfg_path, "-- my config\nhl.bind(\"SUPER + Q\", function() end)\n")
            .unwrap();

        let mut s = session();
        s.settings.config_path = cfg_path.to_string_lossy().into_owned();
        s.settings.backups_to_keep = 0;
        s.command("rule.add", &json!({ "class": "kitty" })).unwrap();
        let id = s.config.rules[0].id.clone();
        s.command(
            "rule.setTag",
            &json!({ "id": id, "slot": "shader_inactive", "path": "/p/crt.glsl" }),
        )
        .unwrap();

        s.save().unwrap();
        assert!(!s.dirty());

        let written = std::fs::read_to_string(&cfg_path).unwrap();
        assert!(written.starts_with("-- my config\n"));
        assert!(written.contains("hl.bind(\"SUPER + Q\""));
        assert!(written.contains("+shader_inactive:"));

        let back = read_config(&cfg_path).unwrap();
        assert_eq!(back.rules.len(), 1);
        assert_eq!(back.rules[0].match_.class.as_deref(), Some("kitty"));
        assert_eq!(back.rules[0].tags[0].slot, TagSlot::Inactive);
    }

    #[test]
    fn look_settings_get_their_own_block_and_leave_out_an_empty_shade_block() {
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("hyprland.lua");
        std::fs::write(&cfg_path, "hl.config({ decoration = { rounding = 6 } })\n").unwrap();

        let mut s = session();
        s.settings.config_path = cfg_path.to_string_lossy().into_owned();
        s.settings.backups_to_keep = 0;
        s.command("look.set", &json!({ "key": "decoration.rounding", "value": 12 })).unwrap();
        assert!(s.dirty());
        s.save().unwrap();
        assert!(!s.dirty());

        let written = std::fs::read_to_string(&cfg_path).unwrap();
        assert!(written.starts_with("hl.config({ decoration = { rounding = 6 } })\n"));
        assert!(written.contains(look::MARKERS.begin));
        assert!(written.contains("        rounding = 12,\n"));
        assert!(!written.contains(block::BEGIN), "an empty shade block was written");
        assert_eq!(read_look(&cfg_path).unwrap(), s.look);

        // Resetting the last override and saving takes the block out again.
        s.command("look.reset", &json!({ "key": "decoration.rounding" })).unwrap();
        assert!(s.dirty());
        s.save().unwrap();
        let written = std::fs::read_to_string(&cfg_path).unwrap();
        assert_eq!(written, "hl.config({ decoration = { rounding = 6 } })\n");
    }

    #[test]
    fn a_look_save_leaves_an_unchanged_shade_block_byte_for_byte() {
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("hyprland.lua");
        let mut s = session();
        s.settings.config_path = cfg_path.to_string_lossy().into_owned();
        s.settings.backups_to_keep = 0;
        // A block an older emitter wrote, which this one would word differently.
        let old =
            block::splice("-- mine\n", "-- old body\nhl.exec_cmd(\"hyprpm reload\")\n").unwrap();
        std::fs::write(&cfg_path, &old).unwrap();

        s.command("look.set", &json!({ "key": "decoration.rounding", "value": 12 })).unwrap();
        s.save().unwrap();
        let written = std::fs::read_to_string(&cfg_path).unwrap();
        assert!(written.starts_with(&old), "the shade block was touched:\n{written}");
        assert!(written.contains("rounding = 12"));

        // Changing the shade config does rewrite it.
        s.command("rule.add", &json!({ "class": "kitty" })).unwrap();
        s.save().unwrap();
        let written = std::fs::read_to_string(&cfg_path).unwrap();
        assert!(!written.contains("-- old body"));
        assert!(written.contains("rounding = 12"));
    }

    #[test]
    fn look_values_are_checked_against_their_option() {
        let mut s = session();
        assert!(s.command("look.set", &json!({ "key": "decoration.nope", "value": 1 })).is_err());
        assert!(s
            .command("look.set", &json!({ "key": "decoration.blur.enabled", "value": "yes" }))
            .is_err());
        assert!(s.look.is_empty());
        s.command("look.set", &json!({ "key": "general.gaps_out", "value": [1, 4] })).unwrap();
        let view = s.state();
        let gaps = view["look"]["options"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["key"] == "general.gaps_out")
            .unwrap()
            .clone();
        assert_eq!(gaps["value"], json!([1, 4, 1, 4]));
        assert_eq!(gaps["overridden"], json!(true));
        assert_eq!(gaps["changed"], json!(true));
        assert_eq!(view["look"]["dirty"], json!(true));
    }

    #[test]
    fn animations_and_curves_save_into_the_look_block() {
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("hyprland.lua");
        std::fs::write(&cfg_path, "-- mine\n").unwrap();
        let mut s = session();
        s.settings.config_path = cfg_path.to_string_lossy().into_owned();
        s.settings.backups_to_keep = 0;

        s.command("curve.set", &json!({ "name": "snappy", "points": [0.2, 1, 0.3, 1] })).unwrap();
        s.command(
            "motion.set",
            &json!({ "leaf": "windowsIn", "setting": {
                "enabled": true, "speed": 3, "bezier": "snappy", "style": "popin 80%" } }),
        )
        .unwrap();
        assert!(s.dirty());
        s.save().unwrap();

        let written = std::fs::read_to_string(&cfg_path).unwrap();
        assert!(written.contains(look::MARKERS.begin));
        assert!(!written.contains("hl.config({"), "an empty hl.config was written");
        let curve = written.find("hl.curve(\"snappy\"").unwrap();
        let anim = written.find("hl.animation({ leaf = \"windowsIn\"").unwrap();
        assert!(curve < anim, "the curve must be defined before it is used");
        assert_eq!(read_look(&cfg_path).unwrap(), s.look);

        // The curve cannot go while windowsIn uses it.
        assert!(s.command("curve.remove", &json!({ "name": "snappy" })).is_err());
        s.command("motion.reset", &json!({ "leaf": "windowsIn" })).unwrap();
        s.command("curve.remove", &json!({ "name": "snappy" })).unwrap();
        assert!(s.look.is_empty());
    }

    #[test]
    fn saving_an_animation_on_a_curve_no_file_defines_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("hyprland.lua");
        std::fs::write(
            &cfg_path,
            "hl.curve(\"quick\", { type = \"bezier\", points = { {0.15, 0}, {0.1, 1} } })\n",
        )
        .unwrap();
        let mut s = session();
        s.settings.config_path = cfg_path.to_string_lossy().into_owned();
        s.settings.backups_to_keep = 0;
        s.config_curves = read_config_curves(&cfg_path);
        assert!(s.config_curves.contains_key("quick"));

        let set =
            |bezier: &str| json!({ "leaf": "fade", "setting": { "speed": 2, "bezier": bezier } });
        s.command("motion.set", &set("quick")).unwrap();
        assert!(!s.save().unwrap().contains("does not define"));

        s.command("motion.set", &set("leftover")).unwrap();
        let msg = s.save().unwrap();
        assert!(msg.contains("fade uses the curve leftover"), "{msg}");
    }

    #[test]
    fn applying_a_theme_sets_the_whole_look_and_marks_it_in_use() {
        let mut s = session();
        s.command("look.set", &json!({ "key": "decoration.rounding", "value": 3 })).unwrap();
        s.command("preset.apply", &json!({ "name": "Soft glass" })).unwrap();
        assert_eq!(s.look.values["decoration.rounding"], LookValue::Int(14));
        assert!(s.look.animations.contains_key("windows"));
        assert!(s.look.curves.contains_key("glide"));
        assert!(s.dirty());
        let v = s.state();
        let glass = v["presets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == "Soft glass")
            .unwrap()
            .clone();
        assert_eq!(glass["active"], json!(true));
        assert!(s.command("preset.apply", &json!({ "name": "No such" })).is_err());
        assert!(s.command("preset.delete", &json!({ "name": "Soft glass" })).is_err());
    }

    #[test]
    fn a_theme_with_shaders_asks_before_changing_the_users_and_then_applies() {
        use crate::model::{Tag, TagSlot};
        let dir = tempfile::tempdir().unwrap();
        let shaders = dir.path().join("shaders");
        std::fs::create_dir_all(&shaders).unwrap();
        std::fs::write(shaders.join("crt.glsl"), "// theme crt\n").unwrap();

        let mut s = session();
        s.config.shader_dir = shaders.to_string_lossy().into_owned();
        let mut rule = WindowRule::new("rule1");
        rule.match_.class = Some("kitty".into());
        rule.tags.push(Tag {
            slot: TagSlot::Inactive,
            is_default: false,
            value: TagValue::Path(ShaderRef::new(shaders.join("crt.glsl").to_string_lossy())),
        });
        s.config.rules.push(rule);
        let mut p = preset::builtin().remove(1);
        p.name = "With shaders".into();
        p.shaders = preset::capture_shaders(&s.config);
        s.presets.push(preset::Entry { preset: p, path: Some(dir.path().join("t.json")) });
        let pick =
            json!({ "name": "With shaders", "file": dir.path().join("t.json").to_string_lossy() });

        // The user edits their crt, and clears their rules.
        std::fs::write(shaders.join("crt.glsl"), "// my crt\n").unwrap();
        s.config.rules.clear();
        assert_eq!(s.preset_conflicts(&pick).unwrap(), vec!["crt.glsl"]);
        assert!(s.command("preset.apply", &pick).is_err(), "applied without asking");
        assert!(s.config.rules.is_empty());

        let mut keep = pick.clone();
        keep["onConflict"] = json!("keep_both");
        s.command("preset.apply", &keep).unwrap();
        assert_eq!(s.config.rules.len(), 1);
        assert_eq!(std::fs::read_to_string(shaders.join("crt.glsl")).unwrap(), "// my crt\n");
        assert!(shaders.join("crt-with-shaders.glsl").exists());

        // Look only leaves the rules alone.
        s.config.rules.clear();
        let mut look_only = pick.clone();
        look_only["lookOnly"] = json!(true);
        s.command("preset.apply", &look_only).unwrap();
        assert!(s.config.rules.is_empty());
    }

    #[test]
    fn animation_settings_are_checked() {
        let mut s = session();
        let set = |leaf: &str, style: &str| json!({ "leaf": leaf, "setting": { "speed": 2, "bezier": "default", "style": style } });
        assert!(s.command("motion.set", &set("nope", "")).is_err());
        assert!(s.command("motion.set", &set("fadeIn", "slide in from far")).is_err());
        assert!(s
            .command("curve.set", &json!({ "name": "bad name", "points": [0, 0, 1, 1] }))
            .is_err());
        assert!(s.look.is_empty());
        s.command("motion.set", &set("workspaces", "slidefade 20%")).unwrap();
        let v = s.state();
        let ws = v["motion"]["leaves"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["name"] == "workspacesIn")
            .unwrap()
            .clone();
        assert_eq!(ws["source"], "inherited");
        assert_eq!(ws["from"], "workspaces");
        assert_eq!(ws["setting"]["style"], "slidefade 20%");
    }

    #[test]
    fn saving_twice_does_not_duplicate_the_block() {
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("hyprland.lua");
        std::fs::write(&cfg_path, "-- config\n").unwrap();

        let mut s = session();
        s.settings.config_path = cfg_path.to_string_lossy().into_owned();
        s.settings.backups_to_keep = 0;
        s.command("rule.add", &json!({ "class": "kitty" })).unwrap();
        s.save().unwrap();
        s.save().unwrap();

        let written = std::fs::read_to_string(&cfg_path).unwrap();
        assert_eq!(written.matches(block::BEGIN).count(), 1);
    }

    #[test]
    fn removing_the_block_restores_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("hyprland.lua");
        let original = "-- config\nhl.bind(\"SUPER + Q\", function() end)\n";
        std::fs::write(&cfg_path, original).unwrap();

        let mut s = session();
        s.settings.config_path = cfg_path.to_string_lossy().into_owned();
        s.settings.backups_to_keep = 0;
        s.command("rule.add", &json!({ "class": "kitty" })).unwrap();
        s.save().unwrap();
        s.remove_block().unwrap();

        assert_eq!(std::fs::read_to_string(&cfg_path).unwrap(), original);
    }

    /// A session with one shader in a directory of its own.
    fn session_with_shader(src: &str) -> (tempfile::TempDir, PathBuf, Session) {
        let dir = tempfile::tempdir().unwrap();
        let shader_path = dir.path().join("dim.glsl");
        std::fs::write(&shader_path, src).unwrap();

        let mut s = session();
        s.settings.backups_to_keep = 0;
        s.config.shader_dir = dir.path().to_string_lossy().into_owned();
        s.rescan_shaders();
        assert_eq!(s.shaders.len(), 1);
        // Pointing the session at the temp directory is a config change; take
        // it as the baseline so `dirty()` still means what the test means.
        s.saved = s.config.clone();
        (dir, shader_path, s)
    }

    fn set_param(s: &mut Session, path: &Path, name: &str, value: f64) -> Result<Option<String>> {
        s.command(
            "shader.setParam",
            &json!({ "path": path.to_string_lossy(), "name": name, "value": value }),
        )
    }

    #[test]
    fn editing_a_shader_param_stages_it_without_touching_the_file() {
        let (_d, path, mut s) = session_with_shader("const float DIM = 0.6;\n");

        set_param(&mut s, &path, "DIM", 0.2).unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "const float DIM = 0.6;\n");
        assert_eq!(s.dirty_shaders(), 1);
        // The config's own dirty flag is a separate thing and must not move.
        assert!(!s.dirty());

        // The view shows the staged value, and what the file still says.
        let view = s.state();
        let param = &view["shaders"][0]["params"][0];
        assert_eq!(param["value"], json!(0.2_f32));
        assert_eq!(param["savedValue"], json!(0.6_f32));
        assert_eq!(param["staged"], json!(true));
        assert_eq!(view["shaders"][0]["dirty"], json!(true));
    }

    #[test]
    fn saving_a_shader_writes_every_staged_edit_at_once() {
        let (_d, path, mut s) = session_with_shader(
            "const float DIM = 0.6;\nconst float GAIN = 1.0;\nvoid main() {}\n",
        );

        set_param(&mut s, &path, "DIM", 0.2).unwrap();
        set_param(&mut s, &path, "GAIN", 2.0).unwrap();
        s.command("shader.setOverlay", &json!({ "path": path.to_string_lossy(), "overlay": true }))
            .unwrap();

        let msg =
            s.command("shader.save", &json!({ "path": path.to_string_lossy() })).unwrap().unwrap();
        assert!(msg.contains("3 changes"), "{msg}");

        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("const float DIM = 0.2;"));
        assert!(written.contains("const float GAIN = 2.0;"));
        assert!(written.contains("// @overlay"));

        assert_eq!(s.dirty_shaders(), 0);
        assert_eq!(s.shaders[0].params[0].value, ParamValue::Scalar(0.2));
    }

    #[test]
    fn reverting_one_parameter_leaves_the_others_staged() {
        let (_d, path, mut s) = session_with_shader(
            "const float DIM = 0.6;\nconst float GAIN = 1.0;\nvoid main() {}\n",
        );

        set_param(&mut s, &path, "DIM", 0.2).unwrap();
        set_param(&mut s, &path, "GAIN", 2.0).unwrap();
        s.command("shader.revert", &json!({ "path": path.to_string_lossy(), "item": "DIM" }))
            .unwrap();

        assert_eq!(s.state()["shaders"][0]["staged"], json!(1));
        s.command("shader.save", &json!({ "path": path.to_string_lossy() })).unwrap();

        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("const float DIM = 0.6;"), "the reverted edit should be gone");
        assert!(written.contains("const float GAIN = 2.0;"));
    }

    #[test]
    fn reverting_a_whole_shader_stages_nothing() {
        let (_d, path, mut s) = session_with_shader("const float DIM = 0.6;\n");

        set_param(&mut s, &path, "DIM", 0.2).unwrap();
        s.command("shader.revert", &json!({ "path": path.to_string_lossy() })).unwrap();

        assert_eq!(s.dirty_shaders(), 0);
        assert_eq!(s.state()["shaders"][0]["params"][0]["value"], json!(0.6_f32));
    }

    #[test]
    fn dragging_back_to_the_saved_value_is_not_an_unsaved_change() {
        let (_d, path, mut s) = session_with_shader("const float DIM = 0.6;\n");

        set_param(&mut s, &path, "DIM", 0.2).unwrap();
        set_param(&mut s, &path, "DIM", 0.6).unwrap();

        assert_eq!(s.dirty_shaders(), 0);
    }

    #[test]
    fn saving_a_shader_with_nothing_staged_writes_nothing() {
        let (_d, path, mut s) = session_with_shader("const float DIM = 0.6;\n");

        let before = std::fs::metadata(&path).unwrap().modified().unwrap();
        s.command("shader.save", &json!({ "path": path.to_string_lossy() })).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), before);
    }

    #[test]
    fn a_shader_edited_underneath_us_drops_the_staged_edit_it_matches() {
        let (_d, path, mut s) = session_with_shader(
            "const float DIM = 0.6;\nconst float GAIN = 1.0;\nvoid main() {}\n",
        );

        set_param(&mut s, &path, "DIM", 0.2).unwrap();
        set_param(&mut s, &path, "GAIN", 2.0).unwrap();

        // Someone saves the same DIM from an editor, and deletes GAIN entirely.
        std::fs::write(&path, "const float DIM = 0.2;\nvoid main() {}\n").unwrap();
        s.command("shader.rescan", &json!({})).unwrap();

        assert_eq!(s.dirty_shaders(), 0, "neither edit is an edit any more");
    }

    #[test]
    fn editing_a_missing_param_reports_instead_of_staging_it() {
        let (_d, path, mut s) = session_with_shader("const float DIM = 0.6;\n");

        let err = set_param(&mut s, &path, "GONE", 1.0).unwrap_err();
        assert!(err.to_string().contains("GONE"), "{err}");
        assert_eq!(s.dirty_shaders(), 0, "a refused edit must not stay staged");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "const float DIM = 0.6;\n");
    }

    #[test]
    fn preview_is_a_complete_block() {
        let mut s = session();
        s.command("rule.add", &json!({ "class": "kitty" })).unwrap();
        let text = s.preview().unwrap();
        assert!(text.starts_with(block::BEGIN));
        assert!(text.trim_end().ends_with(block::END));
    }

    #[test]
    fn a_config_with_no_block_loads_as_empty_rather_than_failing() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("hyprland.lua");
        std::fs::write(&p, "monitor = ,preferred,auto,1\n").unwrap();
        assert_eq!(read_config(&p).unwrap().rules.len(), 0);
    }
}
