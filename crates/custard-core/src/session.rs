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
use crate::rules::{self, HandRule, Rule, RuleKind};
use crate::settings::Settings;
use crate::shader::meta::{Param, ParamValue, ShaderInfo, KNOWN_UNIFORMS, MOTION_UNIFORMS};
use crate::shader::{DraftItem, ShaderDraft};
use crate::theme::Theme;
use crate::{block, colors, emit, hyprctl, hyprpm, import, paths, shader, theme, walls};

mod walls_page;
pub use walls_page::{WallJob, WallOutcome};

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
    /// Whether custard-watch is running.
    pub watch_running: bool,
    /// What custard-watch last reported.
    pub watch_status: Option<colors::follow::Status>,
}

/// Facts about the desktop the Colors and Wallpaper pages show, read once
/// and kept: which apps and programs are installed, what loads the colors
/// files, the folder list, the login lines.
///
/// Reading them means `which` on a dozen names, several configs and a scan
/// of candidate folders — too much for every state push, which the
/// five-second live probe makes whichever page is open. They change when
/// the user does something, so they are read again after every command and
/// scan, and on Refresh.
#[derive(Debug, Clone, Default)]
struct Desk {
    targets: Vec<colors::targets::Target>,
    ffmpeg: bool,
    watch_installed: bool,
    following: Option<colors::ColorSettings>,
    /// Ids of the installed wallpaper programs.
    programs: Vec<&'static str>,
    folders: Vec<walls::library::Folder>,
    login_managed: bool,
    login_lines: Vec<walls::startup::Line>,
    login_moved: Vec<walls::startup::Line>,
    /// The managed wallpaper block: the line it starts on, and its text.
    login_block: Option<(usize, String)>,
    /// Whether the config starts hyprcustard at login, in the tray.
    autostart: bool,
    /// For waybar and rofi: the colors their config defines itself, and the
    /// ones handed over to the scheme.
    own_colors: Vec<(&'static str, Vec<colors::adopt::Own>, Vec<colors::adopt::Own>)>,
    /// `QT_STYLE_OVERRIDE`: where it is set, and what the Qt rows say.
    qt_style: colors::qt_style::Override,
    qt_style_advice: colors::qt_style::Advice,
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
    /// Reading the color scheme folder.
    Palettes,
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
    /// The rule calls the user wrote themselves, read from the file.
    hand_rules: Vec<HandRule>,
    /// Hand-written rules moved into the look block: their text, and the id
    /// of the rule they became. Each is commented out in the file on the
    /// next save — if that rule is still there to replace it.
    moves: Vec<(String, String)>,
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
    /// The resolved color theme.
    pub theme: Theme,
    /// Every theme available to choose from.
    pub themes: Vec<Theme>,
    /// Live compositor information.
    pub live: Live,
    /// Non-fatal problems worth showing, tagged with what reported them.
    pub problems: Vec<Problem>,
    /// The named color schemes, built in and from the schemes folder.
    palettes: Vec<colors::Named>,
    /// The scheme the Colors page is showing, or why it could not be built.
    /// `None` until one is built, and again whenever the choices change.
    colors_built: Option<std::result::Result<colors::Built, String>>,
    /// True while a build is running elsewhere; the UI sets it.
    pub colors_building: bool,
    /// The scheme last applied, as `current.json` has it.
    colors_applied: Option<colors::Scheme>,
    /// Whether this session runs under systemd's graphical session target,
    /// so the watcher could be a user service. Asked once, at start.
    systemd_session: bool,
    /// What the Wallpaper page last saw on screen; `None` until it looks.
    wall_scan: Option<walls::Scan>,
    /// True while a wallpaper is going up.
    wall_busy: bool,
    /// See [`Desk`].
    desk: Desk,
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

        let (palettes, palette_problems) = colors::named::load_all();
        problems.extend(
            palette_problems.into_iter().map(|text| Problem { source: Source::Palettes, text }),
        );

        let config_curves = read_config_curves(&settings.resolved_config_path());
        let hand_rules = read_hand_rules(&settings.resolved_config_path());
        let mut session = Session {
            settings,
            saved: config.clone(),
            config,
            saved_look: look.clone(),
            look,
            config_curves,
            hand_rules,
            moves: Vec::new(),
            presets: Vec::new(),
            shaders: Vec::new(),
            drafts: BTreeMap::new(),
            theme,
            themes,
            live: Live::default(),
            problems,
            palettes,
            colors_built: None,
            colors_building: false,
            colors_applied: colors::current(),
            systemd_session: colors::follow::systemd_available(),
            wall_scan: None,
            wall_busy: false,
            desk: Desk::default(),
        };
        session.rescan_shaders();
        session.refresh_live();
        session.rescan_presets();
        session.refresh_desk();
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
                        "lookRuleCount": e.preset.rules.len(),
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
        // The theme's rules replace the app's, moved ones included; a move
        // not yet saved is called off, so its original stays where it is.
        self.moves.clear();
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
        // The watcher may have changed the colors since; the app's window
        // follows when it wears the scheme. Read here rather than in the
        // probe, which may have started before an Apply that just happened.
        let current = colors::current();
        if current != self.colors_applied {
            self.colors_applied = current;
            if self.config.theme == "scheme" {
                self.refresh_theme();
            }
        }
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
        // Rules moved into the look block come out of the user's own code.
        let mut lost_moves = Vec::new();
        let mut moved = 0;
        for (text, id) in &self.moves {
            // Moved, then deleted here: theirs is all that is left of it.
            if !self.look.rules.iter().any(|r| &r.id == id) {
                continue;
            }
            match rules::comment_out(&updated, text) {
                Some(next) => {
                    updated = next;
                    moved += 1;
                }
                None => lost_moves.push(text.lines().next().unwrap_or_default().trim().to_string()),
            }
        }
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
        self.hand_rules = rules::in_config(&updated);
        self.moves.clear();

        let mut msg = format!("Saved to {}", paths::contract(&path));
        if let Some(b) = backup {
            msg.push_str(&format!(" (backup: {})", paths::contract(&b)));
        }
        if moved > 0 {
            msg.push_str(&format!(
                ". Commented out {moved} rule{} of yours that now live{} in the look block",
                if moved == 1 { "" } else { "s" },
                if moved == 1 { "s" } else { "" }
            ));
        }
        if !lost_moves.is_empty() {
            msg.push_str(&format!(
                ". Could not find {} in the file any more, so left it alone — the look block \
                 has its own copy, so remove yours by hand",
                lost_moves.join(", ")
            ));
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
        self.hand_rules = read_hand_rules(&path);
        self.moves.clear();
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
            "autostart": self.desk.autostart,
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
            "lookRules": self.rules_view(),
            "colors": self.colors_view(),
            "wallpaper": self.wall_view(),
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
        let result = self.run_command(name, payload);
        // Whatever it did may have changed what the desktop pages show.
        self.refresh_desk();
        result
    }

    /// Read the [`Desk`] facts again.
    pub fn refresh_desk(&mut self) {
        let config = self.settings.resolved_config_path();
        let text = std::fs::read_to_string(&config).unwrap_or_default();
        self.desk = Desk {
            targets: self.color_targets(),
            ffmpeg: colors::extract::have_ffmpeg(),
            watch_installed: colors::follow::binary().is_some(),
            following: colors::follow::following(),
            programs: walls::setter::PROGRAMS
                .iter()
                .filter(|p| cfg!(test) || walls::setter::installed(p))
                .map(|p| p.id)
                .collect(),
            folders: self.wall_folders(),
            login_managed: walls::startup::managed(&text),
            login_lines: walls::startup::user_lines(&text),
            login_moved: walls::startup::moved_lines(&text),
            login_block: walls::startup::block_text(&text),
            autostart: crate::autostart::enabled(&text),
            own_colors: Vec::new(),
            qt_style: Default::default(),
            qt_style_advice: Default::default(),
        };
        // Only the two variables are needed, not all of Desktop::read.
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let uses_ct = matches!(env("QT_QPA_PLATFORMTHEME").as_deref(), Some("qt5ct" | "qt6ct"));
        self.desk.qt_style =
            colors::qt_style::Override::read(env(colors::qt_style::VAR), &self.qt_style_sources());
        let styles: Vec<(&str, Option<String>)> = qt_ct_configs(&paths::apps_config_home())
            .into_iter()
            .map(|(id, _, style)| (id, style))
            .collect();
        self.desk.qt_style_advice = self.desk.qt_style.advice(uses_ct, &styles);
        self.desk.own_colors = self
            .desk
            .targets
            .iter()
            .filter_map(|t| {
                let kind = colors::adopt::Kind::of(t.id)?;
                let text = std::fs::read_to_string(&t.config).unwrap_or_default();
                Some((t.id, colors::adopt::own(kind, &text), colors::adopt::taken(kind, &text)))
            })
            .collect();
    }

    fn run_command(&mut self, name: &str, payload: &Value) -> Result<Option<String>> {
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
            "app.autostart" => {
                let on = payload.get("on").and_then(Value::as_bool).unwrap_or(false);
                let path = self.settings.resolved_config_path();
                let text =
                    if path.exists() { paths::read_to_string(&path)? } else { String::new() };
                let next = crate::autostart::set(&text, on)?;
                self.write_config_text(&path, &text, &next)?;
                self.refresh_desk();
                Ok(Some(if on {
                    format!(
                        "hyprcustard starts in the tray at login, from a block at the end of {}",
                        paths::contract(&path)
                    )
                } else {
                    "hyprcustard no longer starts at login".into()
                }))
            }
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
                self.moves.clear();
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

            // --- look rules ---
            "rule.addLook" => self.add_look_rule(payload),
            "rule.setLook" => self.set_look_rule(payload),
            "rule.removeLook" => {
                let id = id_of(payload)?.to_string();
                let at = self.look_rule_index(&id)?;
                let old = self.look.rules[at].clone();
                if self.look_is_live() {
                    look::eval(&rules::live_one(&id, None, &self.theirs(&[&old])))?;
                }
                self.look.rules.remove(at);
                // A moved rule deleted before saving: the original stays.
                self.moves.retain(|(_, m)| *m != id);
                Ok(None)
            }
            "rule.moveLook" => {
                let at = self.look_rule_index(id_of(payload)?)?;
                let delta = payload.get("delta").and_then(Value::as_i64).unwrap_or(0);
                move_item(&mut self.look.rules, at, delta);
                // Later rules win where two set the same thing.
                if self.look_is_live() {
                    look::eval(&rules::live_all(&self.look.rules, &self.theirs_all()))?;
                }
                Ok(None)
            }
            "rule.adopt" => self.adopt_rule(payload),

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

            // --- desktop colors ---
            "colors.set" => self.set_colors(payload),
            "colors.apply" => self.apply_colors(),
            "colors.wire" => self.wire_colors(payload),
            "colors.fix" => self.fix_colors(payload),
            "colors.handOver" => self.hand_over_colors(payload),
            "colors.giveBack" => self.give_back_colors(payload),
            "colors.qtStyle" => {
                if payload.get("take").and_then(Value::as_bool).unwrap_or(false) {
                    self.take_out_qt_style()
                } else {
                    self.put_back_qt_style()
                }
            }
            "colors.follow" => {
                if payload.get("on").and_then(Value::as_bool).unwrap_or(false) {
                    self.start_following().map(Some)
                } else {
                    self.stop_following()?;
                    Ok(Some("Stopped following the wallpaper".into()))
                }
            }
            "colors.rebuild" => {
                // Look again: the wallpaper may have changed underneath.
                if !self.colors_building {
                    self.colors_built = None;
                }
                Ok(None)
            }
            "colors.reloadPalettes" => {
                let (palettes, problems) = colors::named::load_all();
                self.forget(Source::Palettes);
                self.problems.extend(
                    problems.into_iter().map(|text| Problem { source: Source::Palettes, text }),
                );
                self.palettes = palettes;
                self.colors_built = None;
                Ok(Some(format!("{} color schemes available", self.palettes.len())))
            }

            // --- wallpaper ---
            c if c.starts_with("wallpaper.") => self.wall_command(c, payload),

            other => Err(Error::other(format!("unknown command `{other}`"))),
        }
    }

    // -----------------------------------------------------------------------
    // Desktop colors
    // -----------------------------------------------------------------------

    /// What a colors build needs, to run away from the session: building
    /// from a wallpaper runs ffmpeg, which is too slow for the UI thread.
    pub fn colors_job(&self) -> (colors::ColorSettings, Vec<colors::Named>) {
        (self.settings.colors.clone(), self.palettes.clone())
    }

    /// Whether the page is waiting on a build.
    pub fn colors_need_build(&self) -> bool {
        self.colors_built.is_none() && !self.colors_building
    }

    /// Store a finished build — unless the choices changed while it ran, in
    /// which case it describes something no longer asked for.
    pub fn set_colors_built(
        &mut self,
        asked: &colors::ColorSettings,
        result: std::result::Result<colors::Built, String>,
    ) {
        self.colors_building = false;
        if colors_shape(asked) == colors_shape(&self.settings.colors) {
            self.colors_built = Some(result);
        }
    }

    /// Change some of the Colors page's choices. Only the keys given change.
    fn set_colors(&mut self, payload: &Value) -> Result<Option<String>> {
        let mut merged = serde_json::to_value(&self.settings.colors)?;
        let (Some(into), Some(from)) = (merged.as_object_mut(), payload.as_object()) else {
            return Err(Error::other("colors.set takes an object of choices"));
        };
        for (k, v) in from {
            if !into.contains_key(k) {
                return Err(Error::other(format!("there is no color choice called `{k}`")));
            }
            into.insert(k.clone(), v.clone());
        }
        let next: colors::ColorSettings = serde_json::from_value(merged)
            .map_err(|e| Error::other(format!("not a valid color choice: {e}")))?;
        let next = next.sanitized();
        if next == self.settings.colors {
            return Ok(None);
        }
        if colors_shape(&next) != colors_shape(&self.settings.colors) {
            self.colors_built = None;
        }
        let gap_changed = next.min_minutes != self.settings.colors.min_minutes;
        let how_changed = next.start_with != self.settings.colors.start_with;
        self.settings.colors = next;
        self.settings.save()?;
        // How the watcher runs takes effect at once; what it follows only
        // changes on Apply.
        if let Some(mut following) = colors::follow::following() {
            if gap_changed || how_changed {
                following.min_minutes = self.settings.colors.min_minutes;
                following.start_with = self.settings.colors.start_with.clone();
                colors::follow::write_snapshot(&following)?;
            }
            if how_changed {
                self.install_watcher(&following.start_with)?;
                return Ok(Some(match following.start_with.as_str() {
                    "systemd" => "The watcher now runs as a systemd user service".into(),
                    _ => "The watcher now starts with Hyprland".into(),
                }));
            }
        }
        Ok(None)
    }

    /// Follow the wallpaper from now on, with the page's choices as they are.
    fn start_following(&mut self) -> Result<String> {
        if self.settings.colors.source != "wallpaper" {
            return Err(Error::other(
                "following is for the wallpaper — choose The wallpaper first",
            ));
        }
        if colors::follow::binary().is_none() {
            return Err(Error::other(format!(
                "{} is not installed beside hyprcustard or on your PATH",
                colors::follow::PROGRAM
            )));
        }
        colors::follow::write_snapshot(&self.settings.colors)?;
        let how = self.settings.colors.start_with.clone();
        self.install_watcher(&how)?;
        Ok("Following the wallpaper: the colors change with it from now on, and from each login"
            .into())
    }

    /// Stop following: the snapshot goes (which a running watcher takes as
    /// its cue to exit), and so does whatever starts it at login.
    fn stop_following(&mut self) -> Result<()> {
        colors::follow::clear_snapshot()?;
        colors::follow::disable_systemd()?;
        self.set_watch_block(None)?;
        colors::follow::stop();
        Ok(())
    }

    /// Start the watcher now and at each login, the way `how` says, and
    /// undo the other way.
    fn install_watcher(&mut self, how: &str) -> Result<()> {
        let bin = colors::follow::binary()
            .ok_or_else(|| Error::other(format!("{} is not installed", colors::follow::PROGRAM)))?;
        if how == "systemd" {
            self.set_watch_block(None)?;
            colors::follow::stop();
            colors::follow::enable_systemd(&bin)
        } else {
            colors::follow::disable_systemd()?;
            self.set_watch_block(Some(&bin))?;
            colors::follow::start(&bin)
        }
    }

    /// Put the block that starts the watcher into the Hyprland config, or
    /// take it out. Nothing is written when nothing changes.
    fn set_watch_block(&mut self, binary: Option<&Path>) -> Result<()> {
        let path = self.settings.resolved_config_path();
        let text = if path.exists() { paths::read_to_string(&path)? } else { String::new() };
        let next = match binary {
            Some(b) => block::splice_with(
                &text,
                &colors::follow::startup_block(b),
                &colors::follow::MARKERS,
            )?,
            None => block::remove_with(&text, &colors::follow::MARKERS)?,
        };
        if next != text {
            paths::backup(&path, self.settings.backups_to_keep)?;
            paths::write_atomic(&path, &next)?;
            self.hand_rules = read_hand_rules(&path);
        }
        Ok(())
    }

    /// The apps a scheme can be written for, as the disk has them now.
    fn color_targets(&self) -> Vec<colors::targets::Target> {
        colors::targets::all(&paths::apps_config_home(), &self.settings.resolved_config_path())
    }

    /// Write the scheme on show for every app turned on.
    fn apply_colors(&mut self) -> Result<Option<String>> {
        let built = match &self.colors_built {
            Some(Ok(b)) => b.scheme.clone(),
            Some(Err(e)) => return Err(Error::other(e.clone())),
            None => return Err(Error::other("the scheme is still being built")),
        };
        let mut msg = self.apply_scheme(&built, &self.settings.colors.clone())?;
        // The watcher follows what was last applied: the new choices, or —
        // after applying something that is not the wallpaper — nothing.
        if let Some(following) = colors::follow::following() {
            if self.settings.colors.source == "wallpaper" {
                colors::follow::write_snapshot(&colors::ColorSettings {
                    start_with: following.start_with,
                    ..self.settings.colors.clone()
                })?;
            } else {
                self.stop_following()?;
                msg.push_str(". Stopped following the wallpaper");
            }
        }
        Ok(Some(msg))
    }

    /// Write `scheme` for the apps `choices` turn on, and reload them. The
    /// message says what happened.
    fn apply_scheme(
        &mut self,
        scheme: &colors::Scheme,
        choices: &colors::ColorSettings,
    ) -> Result<String> {
        let done = colors::apply(scheme, choices, &self.color_targets())?;
        self.colors_applied = Some(scheme.clone());
        if self.config.theme == "scheme" {
            self.refresh_theme();
        }
        Ok(done.summary(&scheme.name))
    }

    /// An app's own colors for the page: the ones it still defines itself,
    /// or the ones handed over.
    fn own_colors_view(&self, id: &str, taken: bool) -> Vec<Value> {
        let Some((_, own, gone)) = self.desk.own_colors.iter().find(|(i, ..)| *i == id) else {
            return Vec::new();
        };
        let scheme = self.colors_applied.as_ref();
        (if taken { gone } else { own })
            .iter()
            .map(|o| {
                json!({
                    "line": o.line,
                    "name": o.name,
                    "value": o.value,
                    "swatch": colors::scheme::hex(o.rgb),
                    "role": o.role,
                    "becomes": scheme.map(|s| colors::targets::color_of(s, &o.role)),
                })
            })
            .collect()
    }

    /// Hand some of an app's own colors over to the scheme: their lines are
    /// commented out, marked with the scheme color each one gets, and the
    /// colors file defines them from then on.
    fn hand_over_colors(&mut self, payload: &Value) -> Result<Option<String>> {
        let id = id_of(payload)?;
        let pairs: Vec<(String, String)> = payload
            .get("pairs")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|p| {
                        Some((
                            p.get("name")?.as_str()?.to_string(),
                            p.get("role")?.as_str()?.to_string(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let (t, kind) = self.own_colors_target(id)?;
        let text = paths::read_to_string(&t.config)?;
        let next = colors::adopt::hand_over(kind, &text, &pairs);
        let moved =
            colors::adopt::taken(kind, &next).len() - colors::adopt::taken(kind, &text).len();
        if moved == 0 {
            return Ok(Some(format!("None of {}'s colors were handed over", t.name)));
        }
        let backup = paths::backup(&t.config, self.settings.backups_to_keep)?;
        paths::write_atomic(&t.config, &next)?;
        let reloaded = self.rewrite_colors_file(&t)?;
        self.refresh_desk();
        let mut msg = format!(
            "{} of {}'s own colors now come from the scheme. Their lines in {} are commented \
             out, and Give them back restores them",
            moved,
            t.name,
            paths::contract(&t.config)
        );
        if let Some(b) = backup {
            msg.push_str(&format!(" (backup: {})", paths::contract(&b)));
        }
        if let Some(r) = reloaded {
            msg.push_str(&format!(". {r}"));
        }
        Ok(Some(msg))
    }

    /// Put an app's handed-over colors back as they were.
    fn give_back_colors(&mut self, payload: &Value) -> Result<Option<String>> {
        let id = id_of(payload)?;
        let (t, _) = self.own_colors_target(id)?;
        let text = paths::read_to_string(&t.config)?;
        let next = colors::adopt::give_back(&text);
        if next == text {
            return Ok(Some(format!("{} has no colors handed over", t.name)));
        }
        let backup = paths::backup(&t.config, self.settings.backups_to_keep)?;
        paths::write_atomic(&t.config, &next)?;
        let reloaded = self.rewrite_colors_file(&t)?;
        self.refresh_desk();
        let mut msg = format!("{}'s own colors are back as they were", t.name);
        if let Some(b) = backup {
            msg.push_str(&format!(" (backup: {})", paths::contract(&b)));
        }
        if let Some(r) = reloaded {
            msg.push_str(&format!(". {r}"));
        }
        Ok(Some(msg))
    }

    /// The files `QT_STYLE_OVERRIDE` is looked for in. A unit test leaves
    /// the shell profiles out, since they are in the real home.
    fn qt_style_sources(&self) -> Vec<colors::qt_style::Source> {
        let home = if cfg!(test) { None } else { dirs::home_dir() };
        colors::qt_style::sources(
            &paths::apps_config_home(),
            home.as_deref(),
            &self.settings.resolved_config_path(),
        )
    }

    /// Comment out every line that sets `QT_STYLE_OVERRIDE`, and have qt5ct
    /// and qt6ct choose the style it chose, so apps look the same.
    fn take_out_qt_style(&mut self) -> Result<Option<String>> {
        use colors::qt_style;
        let sources = self.qt_style_sources();
        let set: Vec<qt_style::Setting> =
            qt_style::find(&sources).into_iter().filter(|s| !s.taken).collect();
        let Some(value) = set.first().map(|s| s.value.clone()) else {
            return Ok(Some(format!("{} is not set in any file hyprcustard reads", qt_style::VAR)));
        };
        let keep = self.settings.backups_to_keep;
        let mut files = Vec::new();
        for src in sources.iter().filter(|src| set.iter().any(|s| s.path == src.path)) {
            let text = paths::read_to_string(&src.path)?;
            let next = qt_style::take_out(src.kind, &text);
            if next != text {
                guard_test_write(&src.path);
                paths::backup(&src.path, keep)?;
                paths::write_atomic(&src.path, &next)?;
                files.push(paths::contract(&src.path));
            }
        }
        let home = paths::apps_config_home();
        let mut styled = Vec::new();
        for (_, conf, style) in qt_ct_configs(&home) {
            if style.as_deref() == Some(value.as_str()) {
                continue;
            }
            let text = std::fs::read_to_string(&conf).unwrap_or_default();
            let next = colors::toolkits::ini_set(&text, "Appearance", &[("style", &value)]);
            guard_test_write(&conf);
            paths::backup(&conf, keep)?;
            paths::write_atomic(&conf, &next)?;
            styled.push(paths::contract(&conf));
        }
        self.refresh_desk();
        let mut msg = format!("Took {} out of {}", qt_style::VAR, files.join(", "));
        if !styled.is_empty() {
            msg.push_str(&format!(" and set the style to {value} in {}", styled.join(", ")));
        }
        msg.push_str(". Log out and back in for it to take effect");
        Ok(Some(msg))
    }

    /// Put back the `QT_STYLE_OVERRIDE` lines [`Self::take_out_qt_style`]
    /// took out. qt5ct's and qt6ct's style can stay: the variable wins
    /// over it again.
    fn put_back_qt_style(&mut self) -> Result<Option<String>> {
        use colors::qt_style;
        let sources = self.qt_style_sources();
        let taken: Vec<qt_style::Setting> =
            qt_style::find(&sources).into_iter().filter(|s| s.taken).collect();
        let mut files = Vec::new();
        for src in sources.iter().filter(|src| taken.iter().any(|s| s.path == src.path)) {
            let text = paths::read_to_string(&src.path)?;
            let next = qt_style::put_back(src.kind, &text);
            if next != text {
                guard_test_write(&src.path);
                paths::backup(&src.path, self.settings.backups_to_keep)?;
                paths::write_atomic(&src.path, &next)?;
                files.push(paths::contract(&src.path));
            }
        }
        self.refresh_desk();
        if files.is_empty() {
            return Ok(Some(format!(
                "hyprcustard has not taken {} out of anything",
                qt_style::VAR
            )));
        }
        Ok(Some(format!(
            "Put {} back in {}. Log out and back in for it to take effect",
            qt_style::VAR,
            files.join(", ")
        )))
    }

    fn own_colors_target(
        &self,
        id: &str,
    ) -> Result<(colors::targets::Target, colors::adopt::Kind)> {
        let t = self
            .color_targets()
            .into_iter()
            .find(|t| t.id == id)
            .ok_or_else(|| Error::other(format!("there is no app called `{id}`")))?;
        let kind = colors::adopt::Kind::of(t.id).ok_or_else(|| {
            Error::other(format!("{}'s own colors cannot be handed over", t.name))
        })?;
        Ok((t, kind))
    }

    /// Write a target's colors file again with the scheme applied last, so a
    /// hand-over shows at once, and reload the app. Nothing when no scheme
    /// has been applied, or the app is turned off.
    fn rewrite_colors_file(&self, t: &colors::targets::Target) -> Result<Option<String>> {
        let Some(s) = &self.colors_applied else { return Ok(None) };
        if self.settings.colors.off.iter().any(|o| o == t.id) {
            return Ok(None);
        }
        colors::targets::write(t, s, self.settings.colors.opacity)?;
        Ok(if t.wired { colors::targets::reload(t, s) } else { None })
    }

    /// Make one app's config load its colors file.
    fn wire_colors(&mut self, payload: &Value) -> Result<Option<String>> {
        let id = id_of(payload)?;
        let list = self.color_targets();
        let t = list.iter().find(|t| t.id == id).ok_or_else(|| {
            Error::other(format!("there is no app called `{id}` to write colors for"))
        })?;
        if t.wired {
            return Ok(Some(format!("{} already loads its colors", t.name)));
        }
        // A scheme already applied is written first, so the line has
        // something to load rather than waiting for the next Apply.
        if let Some(s) = &self.colors_applied {
            if !self.settings.colors.off.iter().any(|o| o == t.id) {
                colors::targets::write(t, s, self.settings.colors.opacity)?;
            }
        }
        let backup = colors::targets::wire(t, self.settings.backups_to_keep)?;
        if t.id == "hyprland" {
            self.hand_rules = read_hand_rules(&self.settings.resolved_config_path());
        }
        let config = paths::contract(&t.config);
        let mut msg = if t.line.is_empty() {
            format!("Made {config} your own, so {} reads the colors", t.name)
        } else if t.replaces {
            format!("Set your theme in {config} aside and started a new one that loads the colors")
        } else if t.id == "kvantum" {
            format!("Set Kvantum to the {} theme in {config}", colors::toolkits::KVANTUM_THEME)
        } else if matches!(t.id, "qt6ct" | "qt5ct") {
            format!("Set {} to the hyprcustard colors in {config}", t.id)
        } else {
            format!("Added the colors line to {config}")
        };
        if let Some(b) = backup {
            msg.push_str(&format!(" (backup: {})", paths::contract(&b)));
        }
        Ok(Some(msg))
    }

    /// Make the change a target's row offers to let the scheme show: for
    /// GTK 3, switching to adw-gtk3.
    fn fix_colors(&mut self, payload: &Value) -> Result<Option<String>> {
        let id = id_of(payload)?;
        if id != "gtk3" {
            return Err(Error::other(format!("there is nothing to fix for `{id}`")));
        }
        let home = paths::apps_config_home();
        let desk = colors::toolkits::Desktop::read(&home);
        let dark = match (&self.colors_applied, &self.colors_built) {
            (Some(s), _) => s.dark,
            (None, Some(Ok(b))) => b.scheme.dark,
            _ => self.settings.colors.dark,
        };
        let was =
            colors::toolkits::use_adw_gtk3(&home, &desk, dark, self.settings.backups_to_keep)?;
        let now = desk.adw_gtk3(dark).unwrap_or("adw-gtk3");
        Ok(Some(format!(
            "Switched the GTK theme to {now} (it was {was}). Apps opened from now on use it"
        )))
    }

    fn colors_view(&self) -> Value {
        let c = &self.settings.colors;
        let built = match &self.colors_built {
            None => Value::Null,
            Some(Err(e)) => json!({ "ok": false, "error": e }),
            Some(Ok(b)) => json!({
                "ok": true,
                "scheme": scheme_view(&b.scheme),
                "candidates": b.candidates,
                "from": b.from,
                "monitors": b.monitors,
                "wallpaper": b.wallpaper.as_ref().map(|w| json!({
                    "program": w.program,
                    "monitor": w.monitor,
                    "path": paths::contract(&w.path),
                    "folder": w.folder().is_some(),
                    "showing": w.showing.as_ref().map(|p| paths::contract(p)),
                })),
            }),
        };
        let conflicts: Vec<&str> =
            ["general.col.active_border", "general.col.inactive_border", "decoration.shadow.color"]
                .into_iter()
                .filter(|k| self.look.values.contains_key(*k))
                .filter_map(|k| look::find(k).map(|o| o.label))
                .collect();
        json!({
            "settings": c,
            "building": self.colors_building,
            "built": built,
            "applied": self.colors_applied.as_ref().map(scheme_view),
            "palettes": self.palettes.iter().map(|n| json!({
                "id": n.id,
                "name": n.name,
                "dark": n.dark,
                "file": n.file,
                "swatch": SWATCH.iter().map(|i| n.base16[*i].clone()).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "styles": colors::scheme::STYLES.iter().map(|(id, label, hint)| json!({
                "id": id, "label": label, "hint": hint,
            })).collect::<Vec<_>>(),
            "targets": self.desk.targets.iter().map(|t| json!({
                "id": t.id,
                "name": t.name,
                "installed": t.installed,
                "on": !c.off.iter().any(|o| o == t.id),
                "wired": t.wired,
                "config": paths::contract(&t.config),
                "file": paths::contract(&t.file),
                "line": t.line,
                "place": t.place,
                "usage": t.usage,
                "warning": t.warning,
                "fix": t.fix,
                "replaces": t.replaces,
                "own": self.own_colors_view(t.id, false),
                "taken": self.own_colors_view(t.id, true),
            })).collect::<Vec<_>>(),
            "colorChoices": colors::adopt::CHOICES,
            "qtStyle": {
                "note": self.desk.qt_style_advice.note,
                "warn": self.desk.qt_style_advice.warn,
                "action": self.desk.qt_style_advice.action,
            },
            // What each choice is in the scheme applied last, for swatches.
            "choiceColors": self.colors_applied.as_ref().map(|s| {
                colors::adopt::CHOICES
                    .iter()
                    .map(|c| (c.to_string(), Value::from(colors::targets::color_of(s, c))))
                    .collect::<serde_json::Map<_, _>>()
            }),
            "hyprlandConflicts": conflicts,
            "ffmpeg": self.desk.ffmpeg,
            "paletteDir": paths::contract(&colors::named::dir()),
            "appUsesScheme": self.config.theme == "scheme",
            "follow": {
                "on": self.desk.following.is_some(),
                "running": self.live.watch_running,
                "status": self.live.watch_status,
                "installed": self.desk.watch_installed,
                "systemd": self.systemd_session,
            },
        })
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
        // `rule:<id>:<effect>`: one effect of a look rule, mid-drag.
        if let Some(rest) = key.strip_prefix("rule:") {
            let (id, effect) = rest.split_once(':').unwrap_or((rest, ""));
            let old = &self.look.rules[self.look_rule_index(id)?];
            let new = rules::edited(old, &json!({ "effects": { effect: value } }))?;
            return look::eval(&rules::live_one(id, Some(&new), &self.theirs(&[old, &new])));
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
            look::apply_all_live(&self.look, &self.theirs_all())?;
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

    // -----------------------------------------------------------------------
    // Look rules
    // -----------------------------------------------------------------------

    fn look_rule_index(&self, id: &str) -> Result<usize> {
        self.look
            .rules
            .iter()
            .position(|r| r.id == id)
            .ok_or_else(|| Error::other(format!("no look rule {id}")))
    }

    /// The user's own workspace rules for the workspaces these rules are
    /// for — run again around a live change, see [`crate::rules`].
    fn theirs(&self, touched: &[&Rule]) -> Vec<String> {
        let workspaces: Vec<&str> = touched.iter().filter_map(|r| r.workspace()).collect();
        let live_hand = self.hand_rules.iter().filter(|h| !self.is_moving(&h.text));
        rules::theirs_for(live_hand, &workspaces)
    }

    /// [`Session::theirs`] for every rule the app has or had on the desktop.
    fn theirs_all(&self) -> Vec<String> {
        let all: Vec<&Rule> = self.look.rules.iter().chain(&self.saved_look.rules).collect();
        self.theirs(&all)
    }

    fn is_moving(&self, text: &str) -> bool {
        self.moves.iter().any(|(t, _)| t == text)
    }

    fn next_look_rule_id(&self) -> String {
        let taken =
            |id: &str| self.look.rules.iter().chain(&self.saved_look.rules).any(|r| r.id == id);
        (1..).map(|n| format!("lr{n}")).find(|id| !taken(id)).expect("an unused id")
    }

    /// Refuse a second rule of the app's for one workspace: Hyprland merges
    /// them into one, so switching either off would switch off both.
    fn check_workspace_free(&self, rule: &Rule) -> Result<()> {
        if let Some(w) = rule.workspace().filter(|w| !w.is_empty()) {
            if self.look.rules.iter().any(|r| r.id != rule.id && r.workspace() == Some(w)) {
                return Err(Error::other(format!(
                    "there is already a rule here for workspace {w} — add to that one"
                )));
            }
        }
        Ok(())
    }

    /// Put a changed rule on the desktop and keep it, or refuse it with
    /// Hyprland's reason and keep the old one.
    fn put_look_rule(&mut self, at: Option<usize>, rule: Rule) -> Result<()> {
        self.check_workspace_free(&rule)?;
        if self.look_is_live() {
            let old = at.map(|i| self.look.rules[i].clone());
            let touched: Vec<&Rule> = old.iter().chain([&rule]).collect();
            let theirs = self.theirs(&touched);
            look::eval(&rules::live_one(&rule.id, Some(&rule), &theirs))?;
        }
        match at {
            Some(i) => self.look.rules[i] = rule,
            None => self.look.rules.push(rule),
        }
        Ok(())
    }

    fn add_look_rule(&mut self, payload: &Value) -> Result<Option<String>> {
        let kind: RuleKind = from_field(payload, "kind")?;
        let rule = rules::edited(&Rule::new(self.next_look_rule_id(), kind), payload)?;
        self.put_look_rule(None, rule)?;
        Ok(None)
    }

    fn set_look_rule(&mut self, payload: &Value) -> Result<Option<String>> {
        let at = self.look_rule_index(id_of(payload)?)?;
        let rule = rules::edited(&self.look.rules[at], payload)?;
        if rule != self.look.rules[at] {
            self.put_look_rule(Some(at), rule)?;
        }
        Ok(None)
    }

    /// Move one of the user's own rules into the look block, to be edited
    /// here. The original is commented out on the next save.
    fn adopt_rule(&mut self, payload: &Value) -> Result<Option<String>> {
        let text = payload.get("text").and_then(Value::as_str).unwrap_or_default();
        let hand = self
            .hand_rules
            .iter()
            .find(|h| h.text == text)
            .ok_or_else(|| Error::other("that rule is no longer in the file — reload"))?
            .clone();
        if self.is_moving(&hand.text) {
            return Ok(None);
        }
        let mut rule = match (&hand.rule, hand.alone) {
            (Some(r), true) => r.clone(),
            _ => return Err(Error::other("that rule cannot be moved")),
        };
        rule.id = self.next_look_rule_id();
        // Theirs and the moved copy are one rule; they merge as they should.
        if let Some(w) = rule.workspace().map(str::to_string) {
            if let Some(i) = self.look.rules.iter().position(|r| r.workspace() == Some(w.as_str()))
            {
                let mut merged = self.look.rules[i].clone();
                for (k, v) in rule.effects {
                    merged.effects.entry(k).or_insert(v);
                }
                merged.extra.extend(rule.extra);
                let id = merged.id.clone();
                // Queued only once Hyprland has taken the merged rule.
                self.put_look_rule(Some(i), merged)?;
                self.moves.push((hand.text, id));
                return Ok(Some(format!(
                    "Added it to the rule for workspace {w} — Save to move it"
                )));
            }
        }
        let id = rule.id.clone();
        self.put_look_rule(None, rule)?;
        self.moves.push((hand.text, id));
        Ok(Some("Moved here — Save comments out your original in hyprland.lua".into()))
    }

    fn rules_view(&self) -> Value {
        json!({
            "rules": self.look.rules.iter().map(Rule::view).collect::<Vec<_>>(),
            "changed": self.look.rules != self.saved_look.rules || !self.moves.is_empty(),
            "hand": self.hand_rules
                .iter()
                .filter(|h| h.look)
                .map(|h| h.view(self.is_moving(&h.text)))
                .collect::<Vec<_>>(),
            "catalog": rules::catalog(),
        })
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
        watch_running: !colors::follow::running_pids().is_empty(),
        watch_status: colors::follow::read_status(),
    }
}

/// The curves the file at `path` defines outside the look block. A file that
/// cannot be read defines none; reading the config proper reports that.
fn read_config_curves(path: &Path) -> BTreeMap<String, Option<motion::Points>> {
    paths::read_to_string(path).map(|text| motion::curves_in_config(&text)).unwrap_or_default()
}

/// The rule calls in the file at `path`, outside the app's blocks.
fn read_hand_rules(path: &Path) -> Vec<HandRule> {
    paths::read_to_string(path).map(|text| rules::in_config(&text)).unwrap_or_default()
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

/// The base16 slots a palette's swatch shows: background, two grays, then
/// the accents.
const SWATCH: [usize; 10] = [0x0, 0x2, 0x5, 0x8, 0x9, 0xA, 0xB, 0xC, 0xD, 0xE];

/// The choices that decide what a build produces. Opacity, which apps are
/// on and how the watcher runs only matter later, so changing them keeps
/// the build.
/// qt6ct's and qt5ct's configs, where they are installed or set up, with
/// the style each chooses.
fn qt_ct_configs(config_home: &Path) -> Vec<(&'static str, PathBuf, Option<String>)> {
    ["qt6ct", "qt5ct"]
        .iter()
        .filter_map(|id| {
            let conf = config_home.join(id).join(format!("{id}.conf"));
            let text = std::fs::read_to_string(&conf).ok();
            if text.is_none() && (cfg!(test) || paths::which(id).is_none()) {
                return None;
            }
            let style = text.and_then(|t| colors::toolkits::ini_get(&t, "Appearance", "style"));
            Some((*id, conf, style))
        })
        .collect()
}

/// A unit test writes only under the temp folder: the Hyprland config it
/// is pointed at, or the throwaway config home.
fn guard_test_write(path: &Path) {
    if cfg!(test) {
        assert!(path.starts_with(std::env::temp_dir()), "a test would write {}", path.display());
    }
}

fn colors_shape(c: &colors::ColorSettings) -> colors::ColorSettings {
    colors::ColorSettings {
        opacity: 1.0,
        off: Vec::new(),
        min_minutes: 0,
        start_with: String::new(),
        ..c.clone()
    }
}

fn scheme_view(s: &colors::Scheme) -> Value {
    json!({
        "name": s.name,
        "dark": s.dark,
        "source": s.source,
        "roles": s.roles,
        "base16": s.base16,
    })
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
        let mut s = Session {
            settings: Settings::default(),
            config: Config::default(),
            saved: Config::default(),
            look: Look::default(),
            saved_look: Look::default(),
            config_curves: BTreeMap::new(),
            hand_rules: Vec::new(),
            moves: Vec::new(),
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
            palettes: colors::named::builtins(),
            colors_built: None,
            colors_building: false,
            colors_applied: None,
            systemd_session: false,
            wall_scan: None,
            wall_busy: false,
            desk: Desk::default(),
        };
        s.refresh_desk();
        s
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

    fn rule_session(config: &str) -> (tempfile::TempDir, PathBuf, Session) {
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("hyprland.lua");
        std::fs::write(&cfg_path, config).unwrap();
        let mut s = session();
        s.settings.config_path = cfg_path.to_string_lossy().into_owned();
        s.settings.backups_to_keep = 0;
        s.hand_rules = read_hand_rules(&cfg_path);
        (dir, cfg_path, s)
    }

    #[test]
    fn starting_at_login_adds_a_block_and_takes_it_out_again() {
        let mine = "hl.config({ general = { gaps_in = 4 } })\n";
        let (_dir, cfg, mut s) = rule_session(mine);
        s.refresh_desk();
        assert_eq!(s.state()["autostart"], false);
        let msg = s.command("app.autostart", &json!({ "on": true })).unwrap().unwrap();
        assert!(msg.contains("starts in the tray at login"), "{msg}");
        assert!(std::fs::read_to_string(&cfg).unwrap().contains("hyprcustard --tray"));
        assert_eq!(s.state()["autostart"], true);
        s.command("app.autostart", &json!({ "on": false })).unwrap();
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), mine);
        assert_eq!(s.state()["autostart"], false);
    }

    #[test]
    fn qt_style_override_is_taken_out_and_put_back() {
        let mine = "hl.env(\"QT_QPA_PLATFORMTHEME\", \"qt6ct\")\n\
                    hl.env(\"QT_STYLE_OVERRIDE\", \"kvantum-dark\")\n";
        let (_dir, cfg, mut s) = rule_session(mine);
        s.refresh_desk();
        let msg = s.command("colors.qtStyle", &json!({ "take": true })).unwrap().unwrap();
        assert!(msg.starts_with("Took QT_STYLE_OVERRIDE out of"), "{msg}");
        let text = std::fs::read_to_string(&cfg).unwrap();
        assert!(text.contains("-- [hyprcustard took this out"), "{text}");
        assert!(text.starts_with("hl.env(\"QT_QPA_PLATFORMTHEME\""), "{text}");
        assert_eq!(s.state()["colors"]["qtStyle"]["action"], "put");

        let msg = s.command("colors.qtStyle", &json!({ "take": false })).unwrap().unwrap();
        assert!(msg.starts_with("Put QT_STYLE_OVERRIDE back"), "{msg}");
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), mine);
        assert_ne!(s.state()["colors"]["qtStyle"]["action"], "put");
        let msg = s.command("colors.qtStyle", &json!({ "take": false })).unwrap().unwrap();
        assert!(msg.contains("has not taken"), "{msg}");
    }

    /// Flipping a toggle on the Settings page writes that toggle alone:
    /// theming edits waiting for Save stay waiting, and the Save that comes
    /// later keeps what the toggle wrote.
    #[test]
    fn settings_toggles_leave_unsaved_edits_unsaved() {
        let mine = "hl.config({ general = { gaps_in = 4 } })\n";
        let (_dir, cfg, mut s) = rule_session(mine);
        s.command("look.set", &json!({ "key": "decoration.rounding", "value": 12 })).unwrap();
        assert!(s.dirty());

        s.command("app.autostart", &json!({ "on": true })).unwrap();
        let mut settings = serde_json::to_value(&s.settings).unwrap();
        settings["tray"] = json!(true);
        s.command("settings.update", &json!({ "settings": settings })).unwrap();

        let text = std::fs::read_to_string(&cfg).unwrap();
        assert!(text.contains("hyprcustard --tray"));
        assert!(!text.contains("rounding"), "the Look edit is not written by a toggle");
        assert!(s.dirty(), "and is still waiting for Save");
        assert!(s.settings.tray);

        s.save().unwrap();
        let text = std::fs::read_to_string(&cfg).unwrap();
        assert!(text.contains("rounding") && text.contains("hyprcustard --tray"));
        assert!(!s.dirty());
    }

    /// rofi's own colors handed over: the line is marked, the colors file
    /// defines the name in the scheme's color, and giving it back restores
    /// the config byte for byte.
    #[test]
    fn an_apps_own_colors_are_handed_over_and_given_back() {
        let rofi = paths::apps_config_home().join("rofi");
        std::fs::create_dir_all(&rofi).unwrap();
        let config = rofi.join("config.rasi");
        let mine = "@import \"hyprcustard-colors.rasi\"\n* {\n    foreground: #EBDBB2;\n    \
                    bar-background: rgba(40,40,40,0.8);\n    text-color: @foreground;\n}\n";
        std::fs::write(&config, mine).unwrap();
        let base16: [String; 16] =
            std::array::from_fn(|i| format!("#{:02x}{:02x}{:02x}", i * 16, i * 8, 0));
        let mut s = session();
        s.colors_applied = Some(colors::scheme::from_base16("Test", true, &base16));
        s.refresh_desk();
        let rofi_row = |s: &Session| {
            s.state()["colors"]["targets"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["id"] == "rofi")
                .cloned()
                .unwrap()
        };
        let row = rofi_row(&s);
        assert_eq!(row["own"].as_array().unwrap().len(), 2);
        assert_eq!(row["own"][0]["role"], "on_surface");

        let msg = s
            .command(
                "colors.handOver",
                &json!({ "id": "rofi", "pairs": [{ "name": "foreground", "role": "on_surface" }] }),
            )
            .unwrap()
            .unwrap();
        assert!(msg.starts_with("1 of Rofi's own colors"), "{msg}");
        let text = std::fs::read_to_string(&config).unwrap();
        assert!(text.contains("/* [hyprcustard sets this: on_surface] foreground: #EBDBB2; */"));
        // The colors file defines the name in the scheme's color. Written to a
        // file of this test's own: other tests applying a scheme rewrite the
        // shared one at the same time.
        let mut target = s.color_targets().into_iter().find(|t| t.id == "rofi").unwrap();
        let out = tempfile::tempdir().unwrap();
        target.file = out.path().join("hyprcustard-colors.rasi");
        let scheme = s.colors_applied.clone().unwrap();
        colors::targets::write(&target, &scheme, 1.0).unwrap();
        let file = std::fs::read_to_string(&target.file).unwrap();
        assert!(
            file.contains(&format!("    foreground: {};", scheme.role("on_surface"))),
            "{file}"
        );
        let row = rofi_row(&s);
        assert_eq!(
            (row["own"].as_array().unwrap().len(), row["taken"].as_array().unwrap().len()),
            (1, 1)
        );

        s.command("colors.giveBack", &json!({ "id": "rofi" })).unwrap();
        assert_eq!(std::fs::read_to_string(&config).unwrap(), mine);
        assert!(s.command("colors.handOver", &json!({ "id": "kitty", "pairs": [] })).is_err());
    }

    #[test]
    fn look_rules_are_added_edited_and_saved_into_the_look_block() {
        let (_dir, cfg_path, mut s) = rule_session("-- mine\n");
        s.command("rule.addLook", &json!({ "kind": "window", "matches": { "class": "kitty" } }))
            .unwrap();
        let id = s.look.rules[0].id.clone();
        assert_eq!(s.look.rules[0].incomplete(), Some("It changes nothing yet"));
        s.command(
            "rule.setLook",
            &json!({ "id": id, "label": "Terminal", "effects": { "opacity": [0.95, 0.85] } }),
        )
        .unwrap();
        assert!(s.state()["lookRules"]["changed"].as_bool().unwrap());
        s.save().unwrap();

        let written = std::fs::read_to_string(&cfg_path).unwrap();
        assert!(written.contains("-- Terminal\nhyprcustard_rules[\"lr1\"] = hl.window_rule({"));
        assert!(written.contains("opacity = \"0.95 override 0.85 override\""));
        assert_eq!(read_look(&cfg_path).unwrap().rules, s.look.rules);
        assert!(!s.state()["lookRules"]["changed"].as_bool().unwrap());

        s.command("rule.removeLook", &json!({ "id": id })).unwrap();
        s.save().unwrap();
        let written = std::fs::read_to_string(&cfg_path).unwrap();
        assert!(!written.contains("hl.window_rule"));
    }

    #[test]
    fn one_rule_per_workspace() {
        let (_dir, _p, mut s) = rule_session("");
        let ws = |w: &str| json!({ "kind": "workspace", "matches": { "workspace": w } });
        s.command("rule.addLook", &ws("w[tv1]")).unwrap();
        assert!(s.command("rule.addLook", &ws("w[tv1]")).is_err());
        s.command("rule.addLook", &ws("f[1]")).unwrap();
        let second = s.look.rules[1].id.clone();
        assert!(s
            .command("rule.setLook", &json!({ "id": second, "matches": { "workspace": "w[tv1]" } }))
            .is_err());
        assert_eq!(s.look.rules[1].workspace(), Some("f[1]"));
    }

    #[test]
    fn moving_a_hand_written_rule_comments_it_out_on_save() {
        let config = "hl.window_rule({\n    name      = \"tiled-no-shadow\",\n    \
                      match     = { float = false },\n    no_shadow = true,\n})\n";
        let (_dir, cfg_path, mut s) = rule_session(config);
        let hand = s.state()["lookRules"]["hand"].clone();
        assert_eq!(hand.as_array().unwrap().len(), 1);
        assert_eq!(hand[0]["movable"], json!(true));
        let text = hand[0]["text"].as_str().unwrap().to_string();

        s.command("rule.adopt", &json!({ "text": text })).unwrap();
        assert_eq!(s.look.rules[0].label, "tiled-no-shadow");
        assert_eq!(s.state()["lookRules"]["hand"][0]["moving"], json!(true));
        // Nothing is written until Save.
        assert_eq!(std::fs::read_to_string(&cfg_path).unwrap(), config);

        let msg = s.save().unwrap();
        assert!(msg.contains("Commented out 1 rule"), "{msg}");
        let written = std::fs::read_to_string(&cfg_path).unwrap();
        assert!(
            written.starts_with("-- Moved into the HyprCustard look block:\n-- hl.window_rule({")
        );
        assert!(written.contains("hyprcustard_rules[\"lr1\"] = hl.window_rule({"));
        assert!(written.contains("    match = { float = false },\n    no_shadow = true,\n"));
        assert!(s.state()["lookRules"]["hand"].as_array().unwrap().is_empty());
    }

    #[test]
    fn discarding_forgets_a_pending_move() {
        let config = "hl.layer_rule({ match = { namespace = \"waybar\" }, blur = true })\n";
        let (_dir, cfg_path, mut s) = rule_session(config);
        let text = s.hand_rules[0].text.clone();
        s.command("rule.adopt", &json!({ "text": text })).unwrap();
        s.command("look.discard", &json!({})).unwrap();
        assert!(s.look.rules.is_empty());
        s.save().unwrap();
        assert!(std::fs::read_to_string(&cfg_path).unwrap().starts_with(config));
    }

    #[test]
    fn a_moved_rule_deleted_before_saving_leaves_the_original_alone() {
        let config = "hl.layer_rule({ match = { namespace = \"waybar\" }, blur = true })\n";
        let (_dir, cfg_path, mut s) = rule_session(config);
        let text = s.hand_rules[0].text.clone();
        s.command("rule.adopt", &json!({ "text": text })).unwrap();
        let id = s.look.rules[0].id.clone();
        s.command("rule.removeLook", &json!({ "id": id })).unwrap();
        let msg = s.save().unwrap();
        assert!(!msg.contains("Commented out"), "{msg}");
        assert!(std::fs::read_to_string(&cfg_path).unwrap().starts_with(config));
    }

    #[test]
    fn a_theme_with_rules_cancels_a_pending_move() {
        let config = "hl.layer_rule({ match = { namespace = \"waybar\" }, blur = true })\n";
        let (_dir, cfg_path, mut s) = rule_session(config);
        let text = s.hand_rules[0].text.clone();
        s.command("rule.adopt", &json!({ "text": text })).unwrap();
        let mut p = preset::builtin().remove(1);
        let mut r = Rule::new("lr1", RuleKind::Layer);
        r.matches.insert("namespace".into(), LookValue::Text("rofi".into()));
        r.effects.insert("blur".into(), LookValue::Bool(true));
        p.rules = vec![r];
        p.name = "Ruled".into();
        s.presets.push(preset::Entry { preset: p.clone(), path: None });
        s.command("preset.apply", &json!({ "name": p.name })).unwrap();
        s.save().unwrap();
        assert!(std::fs::read_to_string(&cfg_path).unwrap().starts_with(config));
    }

    #[test]
    fn a_theme_rule_carrying_code_is_left_out() {
        let (_dir, _p, mut s) = rule_session("");
        let mut p = preset::builtin().remove(1);
        let mut r = Rule::new("lr1", RuleKind::Window);
        r.matches.insert("class".into(), LookValue::Text("k".into()));
        r.extra.push(("float".into(), "os.execute('x') or true".into()));
        p.rules = vec![r];
        p.name = "Sneaky".into();
        s.presets.push(preset::Entry { preset: p, path: None });
        let msg = s.command("preset.apply", &json!({ "name": "Sneaky" })).unwrap().unwrap();
        assert!(msg.contains("Left out"), "{msg}");
        assert!(s.look.rules.is_empty());
    }

    #[test]
    fn themes_carry_look_rules_and_one_without_takes_them_away() {
        let (_dir, _p, mut s) = rule_session("");
        s.command(
            "rule.addLook",
            &json!({ "kind": "layer", "matches": { "namespace": "waybar" }, "effects": { "blur": true } }),
        )
        .unwrap();
        let p = preset::snapshot(
            "mine",
            "",
            &s.look,
            &BTreeMap::new(),
            &Default::default(),
            &BTreeMap::new(),
        );
        assert_eq!(p.rules.len(), 1);
        s.command("preset.apply", &json!({ "name": "Soft glass" })).unwrap();
        assert!(s.look.rules.is_empty(), "a theme without rules takes the app's away");
        let back: preset::Preset =
            serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(back.rules, p.rules);

        s.look.rules.clear();
        let (applied, problems) = preset::to_look(&p, &s.look, &Default::default());
        assert!(problems.is_empty());
        assert_eq!(applied.rules.len(), 1);
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

    #[test]
    fn color_choices_merge_and_only_shape_changes_drop_the_build() {
        let mut s = session();
        let job = s.colors_job().0;
        s.set_colors_built(&job, Ok(colors::build(&job, &s.palettes).unwrap()));
        assert!(!s.colors_need_build());

        // Opacity is only used when applying: the build stays.
        s.command("colors.set", &json!({ "opacity": 0.8 })).unwrap();
        assert!(!s.colors_need_build());
        assert_eq!(s.settings.colors.opacity, 0.8);
        assert_eq!(s.settings.colors.source, "scheme:gruvbox-dark", "untouched keys stay");

        s.command("colors.set", &json!({ "source": "scheme:nord" })).unwrap();
        assert!(s.colors_need_build());

        let e = s.command("colors.set", &json!({ "sorce": "x" })).unwrap_err();
        assert!(e.to_string().contains("sorce"));
        let e = s.command("colors.set", &json!({ "contrast": "high" })).unwrap_err();
        assert!(e.to_string().contains("not a valid"), "{e}");
    }

    #[test]
    fn a_build_that_finishes_after_the_choices_changed_is_dropped() {
        let mut s = session();
        let asked = s.colors_job().0;
        s.colors_building = true;
        s.command("colors.set", &json!({ "source": "scheme:nord" })).unwrap();
        s.set_colors_built(&asked, Ok(colors::build(&asked, &s.palettes).unwrap()));
        assert!(!s.colors_building);
        assert!(s.colors_need_build(), "the Gruvbox build must not stand in for Nord");
    }

    #[test]
    fn applying_needs_a_build_and_then_writes_and_remembers_it() {
        let (_dir, _cfg, mut s) = rule_session("-- mine\n");
        // Apply reads the follow snapshot, which other tests write.
        let _lock = colors::follow::SNAPSHOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let e = s.command("colors.apply", &json!({})).unwrap_err();
        assert!(e.to_string().contains("still being built"));
        let asked = s.colors_job().0;
        s.set_colors_built(&asked, Err("no wallpaper".into()));
        assert!(s
            .command("colors.apply", &json!({}))
            .unwrap_err()
            .to_string()
            .contains("no wallpaper"));

        s.set_colors_built(&asked, Ok(colors::build(&asked, &s.palettes).unwrap()));
        let msg = s.command("colors.apply", &json!({})).unwrap().unwrap();
        assert!(msg.contains("Gruvbox Dark"), "{msg}");
        assert_eq!(s.colors_applied.as_ref().unwrap().name, "Gruvbox Dark");
        let view = &s.state()["colors"];
        assert_eq!(view["applied"]["name"], "Gruvbox Dark");
        assert_eq!(view["built"]["ok"], true);
        assert_eq!(view["palettes"][0]["swatch"].as_array().unwrap().len(), 10);
        assert!(view["targets"].as_array().unwrap().iter().any(|t| t["id"] == "hyprland"));
    }

    #[test]
    fn wiring_hyprland_appends_its_line_and_keeps_the_file() {
        let (_dir, cfg, mut s) =
            rule_session("-- mine\nhl.config({ general = { gaps_in = 3 } })\n");
        s.command("colors.wire", &json!({ "id": "hyprland" })).unwrap();
        let text = std::fs::read_to_string(&cfg).unwrap();
        assert!(text.starts_with("-- mine\nhl.config("), "{text}");
        assert!(text.trim_end().ends_with("-- hyprcustard colors"), "{text}");
        let again = s.command("colors.wire", &json!({ "id": "hyprland" })).unwrap().unwrap();
        assert!(again.contains("already"));
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), text);
        assert!(s.command("colors.wire", &json!({ "id": "emacs" })).is_err());
    }

    #[test]
    fn border_colors_set_on_the_look_page_are_flagged() {
        let mut s = session();
        assert!(s.state()["colors"]["hyprlandConflicts"].as_array().unwrap().is_empty());
        s.look.values.insert(
            "general.col.inactive_border".into(),
            LookValue::Gradient { colors: vec!["#ff000000".into()], angle: 0.0 },
        );
        assert_eq!(s.state()["colors"]["hyprlandConflicts"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn following_is_only_for_the_wallpaper_and_needs_the_watcher() {
        let mut s = session();
        let e = s.command("colors.follow", &json!({ "on": true })).unwrap_err();
        assert!(e.to_string().contains("choose The wallpaper"), "{e}");
        s.settings.colors.source = "wallpaper".into();
        // Tests have no custard-watch beside them or on PATH.
        if colors::follow::binary().is_none() {
            let e = s.command("colors.follow", &json!({ "on": true })).unwrap_err();
            assert!(e.to_string().contains("not installed"), "{e}");
        }
    }

    #[test]
    fn the_startup_block_goes_in_and_comes_out_cleanly() {
        let mine = "-- mine\nhl.config({ general = { gaps_in = 3 } })\n";
        let (_dir, cfg, mut s) = rule_session(mine);
        s.set_watch_block(Some(Path::new("/opt/bin/custard-watch"))).unwrap();
        let text = std::fs::read_to_string(&cfg).unwrap();
        assert!(text.starts_with(mine));
        assert!(text.contains("hl.exec_cmd(\"/opt/bin/custard-watch\")"), "{text}");
        // Again: replaced in place, not added twice.
        s.set_watch_block(Some(Path::new("/usr/bin/custard-watch"))).unwrap();
        let text = std::fs::read_to_string(&cfg).unwrap();
        assert_eq!(text.matches("hyprland.start").count(), 1);
        assert!(text.contains("/usr/bin/custard-watch"));

        let _lock = colors::follow::SNAPSHOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        s.command("colors.follow", &json!({ "on": false })).unwrap();
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), mine);
        assert!(colors::follow::following().is_none());
    }

    #[test]
    fn applying_something_else_stops_following_and_applying_the_wallpaper_updates_it() {
        let (_dir, cfg, mut s) = rule_session("-- mine\n");
        let _lock = colors::follow::SNAPSHOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let follow = colors::ColorSettings { source: "wallpaper".into(), ..Default::default() };
        colors::follow::write_snapshot(&follow).unwrap();
        s.set_watch_block(Some(Path::new("/x/custard-watch"))).unwrap();

        // Changing the gap reaches the watcher at once; the page's other
        // choices do not.
        s.command("colors.set", &json!({ "min_minutes": 20, "style": "vibrant" })).unwrap();
        let now = colors::follow::following().unwrap();
        assert_eq!(now.min_minutes, 20);
        assert_eq!(now.style, "tonal-spot");

        let asked = s.colors_job().0;
        s.set_colors_built(&asked, Ok(colors::build(&asked, &s.palettes).unwrap()));
        let msg = s.command("colors.apply", &json!({})).unwrap().unwrap();
        assert!(msg.ends_with("Stopped following the wallpaper"), "{msg}");
        assert!(colors::follow::following().is_none());
        assert!(!std::fs::read_to_string(&cfg).unwrap().contains("custard-watch"));
    }

    fn argv(s: &str) -> Vec<String> {
        s.split(' ').map(String::from).collect()
    }

    fn swaybg_scan(image: &str) -> walls::Scan {
        let procs = vec![(1, argv(&format!("swaybg -o * -i {image} -m fill")))];
        walls::Scan {
            found: colors::wallpaper::Found {
                wallpapers: colors::wallpaper::swaybg_args(&procs[0].1),
                monitors: vec!["DP-1".into()],
                ..Default::default()
            },
            running: vec!["swaybg"],
            services: Vec::new(),
            procs,
        }
    }

    /// The page shows the on-screen preview straight from disk when it is
    /// there, rather than waiting for one to be made again.
    #[test]
    fn the_wallpaper_on_screen_says_whether_its_preview_is_made() {
        let mut s = session();
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("on-screen.png");
        std::fs::write(&png, b"x").unwrap();
        s.set_wall_scan(swaybg_scan(&png.to_string_lossy()));
        let shown = |s: &Session| s.state()["wallpaper"]["monitors"][0]["wallpaper"].clone();
        assert_eq!(shown(&s)["file"], json!(png));
        assert_eq!(shown(&s)["ready"], false);

        let thumb = walls::thumbs::path_for(&png).unwrap();
        assert_eq!(shown(&s)["thumb"], json!(thumb));
        std::fs::create_dir_all(thumb.parent().unwrap()).unwrap();
        std::fs::write(&thumb, b"jpg").unwrap();
        assert_eq!(shown(&s)["ready"], true);
        std::fs::remove_file(&thumb).unwrap();
    }

    /// Seen once, the user's mpvpaper options outlast another program
    /// taking over; one hyprcustard started with its own does not replace
    /// them.
    #[test]
    fn the_users_mpvpaper_is_remembered() {
        let mut s = session();
        let theirs = argv("mpvpaper -p -a FULL -o no-audio ALL /w/clips/");
        let scan_of = |argv: Vec<String>| walls::Scan {
            procs: vec![(4, argv)],
            running: vec!["mpvpaper"],
            ..swaybg_scan("/cur.png")
        };
        s.set_wall_scan(scan_of(theirs.clone()));
        assert_eq!(s.settings.wallpaper.mpvpaper, theirs);

        let ours: Vec<String> =
            ["mpvpaper", "-p", "-o", "no-audio loop panscan=1.0", "ALL", "/a.png"]
                .map(String::from)
                .to_vec();
        s.set_wall_scan(scan_of(ours));
        assert_eq!(s.settings.wallpaper.mpvpaper, theirs, "not replaced by hyprcustard's own");
        s.set_wall_scan(swaybg_scan("/cur.png"));
        assert_eq!(s.settings.wallpaper.mpvpaper, theirs, "kept while swaybg runs");
    }

    /// Turning the recolor switch on reads the wallpaper already up, with
    /// nothing put up again and the login line left alone.
    #[test]
    fn colors_alone_are_read_from_the_wallpaper_on_screen() {
        let _lock = colors::follow::SNAPSHOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut s = session();
        s.set_wall_scan(swaybg_scan("/cur.png"));
        let job = s.wall_job(&json!({ "colorsOnly": true })).unwrap();
        assert!(job.steps.is_empty());
        assert_eq!(job.program, "swaybg");
        assert_eq!(job.recolor.as_ref().unwrap().source, "wallpaper");
        assert!(s.wall_job(&json!({ "colorsOnly": true })).is_err(), "one at a time");
        let out = WallOutcome {
            result: Ok(()),
            scan: swaybg_scan("/cur.png"),
            colors: Some(Err("no frame".into())),
        };
        let e = s.wall_done(&job, out).unwrap_err().to_string();
        assert!(e.contains("not changed") && e.contains("no frame"), "{e}");
        assert!(!s.state()["wallpaper"]["busy"].as_bool().unwrap());
    }

    #[test]
    fn wallpaper_choices_merge_and_folders_are_added_and_hidden() {
        let mut s = session();
        s.command("wallpaper.set", &json!({ "fit": "fit", "program": "swaybg" })).unwrap();
        assert_eq!(s.settings.wallpaper.fit, "fit");
        assert_eq!(s.state()["wallpaper"]["using"]["id"], "swaybg");
        assert!(s.command("wallpaper.set", &json!({ "folders": [] })).is_err());

        // mpvpaper's timer and shuffle merge, and the page sees them.
        assert_eq!(s.state()["wallpaper"]["mpv"]["changeEvery"], 0);
        s.command("wallpaper.set", &json!({ "mpv": { "change_every": 90 } })).unwrap();
        s.command("wallpaper.set", &json!({ "mpv": { "shuffle": true } })).unwrap();
        assert_eq!(s.settings.wallpaper.mpv.change_every, Some(90));
        assert_eq!(s.state()["wallpaper"]["mpv"], json!({ "changeEvery": 90, "shuffle": true }));
        assert!(s.command("wallpaper.set", &json!({ "mpv": { "change_every": -1 } })).is_err());

        let dir = tempfile::tempdir().unwrap();
        let found = dir.path().join("found");
        let added = dir.path().join("added");
        std::fs::create_dir_all(&found).unwrap();
        std::fs::create_dir_all(&added).unwrap();
        let mut scan = swaybg_scan(&found.join("a.png").to_string_lossy());
        scan.found.wallpapers[0].path = found.join("a.png");
        s.set_wall_scan(scan);
        let listed = |s: &Session| -> Vec<(String, bool)> {
            s.state()["wallpaper"]["folders"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|f| f["path"].as_str().unwrap().starts_with(&*dir.path().to_string_lossy()))
                .map(|f| (f["path"].as_str().unwrap().to_string(), f["found"].as_bool().unwrap()))
                .collect()
        };
        let url = format!("file://{}", added.display());
        let msg = s.command("wallpaper.addFolder", &json!({ "path": url })).unwrap().unwrap();
        assert!(msg.contains("no images or videos yet"), "{msg}");
        assert_eq!(listed(&s).len(), 2);
        assert!(listed(&s)[0].1 && !listed(&s)[1].1);

        // One already on the list, or inside one, says so and adds nothing.
        let msg = s.command("wallpaper.addFolder", &json!({ "path": added })).unwrap().unwrap();
        assert!(msg.contains("already on the list"), "{msg}");
        let inner = found.join("large");
        std::fs::create_dir_all(&inner).unwrap();
        let msg = s.command("wallpaper.addFolder", &json!({ "path": inner })).unwrap().unwrap();
        assert!(msg.contains("is inside") && msg.contains("so its wallpapers show there"), "{msg}");
        assert_eq!(s.settings.wallpaper.folders.len(), 1);

        s.command("wallpaper.removeFolder", &json!({ "path": added })).unwrap();
        s.command("wallpaper.removeFolder", &json!({ "path": found })).unwrap();
        assert!(listed(&s).is_empty());
        assert_eq!(s.settings.wallpaper.hidden.len(), 1, "the found one is hidden");
        s.command("wallpaper.addFolder", &json!({ "path": found })).unwrap();
        assert!(s.settings.wallpaper.hidden.is_empty() && s.settings.wallpaper.folders.is_empty());
        assert!(s.command("wallpaper.addFolder", &json!({ "path": "/no/such" })).is_err());
    }

    #[test]
    fn keeping_a_wallpaper_at_login_follows_each_pick_and_gives_back() {
        let _lock = colors::follow::SNAPSHOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = colors::follow::clear_snapshot();
        let mine =
            "hl.on(\"hyprland.start\", function()\n    hl.exec_cmd(\"swaybg -i /old.png\")\nend)\n";
        let (dir, cfg, mut s) = rule_session(mine);
        let png = dir.path().join("new.png");
        std::fs::write(&png, b"x").unwrap();

        // Not handed over: a pick says the old one comes back at login.
        s.set_wall_scan(swaybg_scan("/cur.png"));
        s.settings.wallpaper.program = "swaybg".into();
        let job = s.wall_job(&json!({ "path": png, "monitor": "" })).unwrap();
        assert_eq!(job.program, "swaybg");
        assert!(job.recolor.is_none());
        assert!(s.state()["wallpaper"]["busy"].as_bool().unwrap());
        assert!(s.wall_job(&json!({ "path": png })).is_err(), "one at a time");
        let out = WallOutcome { result: Ok(()), scan: swaybg_scan("/cur.png"), colors: None };
        let msg = s.wall_done(&job, out).unwrap().unwrap();
        assert!(msg.contains("still starts the old one"), "{msg}");
        let lines = s.state()["wallpaper"]["startup"]["lines"].clone();
        assert_eq!(lines[0]["number"], 2);

        s.command("wallpaper.keepAtLogin", &json!({})).unwrap();
        let text = std::fs::read_to_string(&cfg).unwrap();
        assert!(text.contains("hl.exec_cmd(\"swaybg -o '*' -i /cur.png -m fill\")"), "{text}");
        assert!(
            text.contains(&format!("{}hl.exec_cmd(\"swaybg -i /old.png", walls::startup::MOVED))
        );
        assert!(s.state()["wallpaper"]["startup"]["managed"].as_bool().unwrap());

        // Handed over: each pick rewrites the block.
        let job = s.wall_job(&json!({ "path": png })).unwrap();
        let shown = png.to_string_lossy().into_owned();
        let out = WallOutcome { result: Ok(()), scan: swaybg_scan(&shown), colors: None };
        let msg = s.wall_done(&job, out).unwrap().unwrap();
        assert!(!msg.contains("old one"), "{msg}");
        let text = std::fs::read_to_string(&cfg).unwrap();
        assert!(text.contains(&format!("-i {shown} -m fill")) && !text.contains("/cur.png"));

        // A failure leaves the block alone and says why.
        let job = s.wall_job(&json!({ "path": png })).unwrap();
        let out = WallOutcome {
            result: Err("swaybg stopped as soon as it started".into()),
            scan: swaybg_scan("/other.png"),
            colors: None,
        };
        assert!(s.wall_done(&job, out).is_err());
        assert!(!s.wall_busy);
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), text);

        s.command("wallpaper.giveBack", &json!({})).unwrap();
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), mine);
    }

    #[test]
    fn a_pick_recolors_without_touching_the_colors_page_or_what_is_followed() {
        let _lock = colors::follow::SNAPSHOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = colors::follow::clear_snapshot();
        let (dir, _cfg, mut s) = rule_session("-- mine\n");
        let png = dir.path().join("new.png");
        std::fs::write(&png, b"x").unwrap();
        s.set_wall_scan(swaybg_scan("/cur.png"));
        s.settings.wallpaper.program = "swaybg".into();
        s.settings.colors.source = "scheme:nord".into();
        s.settings.colors.style = "vibrant".into();

        // Not asked for: nothing to read.
        let job = s.wall_job(&json!({ "path": png })).unwrap();
        assert!(job.recolor.is_none());
        s.wall_done(&job, WallOutcome { result: Ok(()), scan: swaybg_scan("/x"), colors: None })
            .unwrap();

        // Asked for here: the page's choices, read from the wallpaper.
        s.settings.wallpaper.recolor = true;
        let job = s.wall_job(&json!({ "path": png })).unwrap();
        let asked = job.recolor.clone().unwrap();
        assert_eq!((asked.source.as_str(), asked.style.as_str()), ("wallpaper", "vibrant"));
        let seed = colors::ColorSettings { source: "color:#88d0ec".into(), ..asked.clone() };
        let built = colors::build(&seed, &s.palettes).unwrap();
        let out =
            WallOutcome { result: Ok(()), scan: swaybg_scan("/cur.png"), colors: Some(Ok(built)) };
        let msg = s.wall_done(&job, out).unwrap().unwrap();
        assert!(msg.contains("is up") && msg.contains("#88d0ec"), "{msg}");
        assert_eq!(colors::current().unwrap().name, "#88d0ec");
        assert_eq!(s.settings.colors.source, "scheme:nord", "the page is left alone");

        // Following: the followed choices win over the page's untried ones,
        // and the snapshot is not rewritten.
        let followed = colors::ColorSettings {
            source: "wallpaper".into(),
            style: "tonal-spot".into(),
            ..Default::default()
        };
        colors::follow::write_snapshot(&followed).unwrap();
        let job = s.wall_job(&json!({ "path": png })).unwrap();
        assert_eq!(job.recolor.as_ref().unwrap().style, "tonal-spot");
        let built = colors::build(
            &colors::ColorSettings { source: "color:#c02020".into(), ..followed.clone() },
            &s.palettes,
        )
        .unwrap();
        let out =
            WallOutcome { result: Ok(()), scan: swaybg_scan("/cur.png"), colors: Some(Ok(built)) };
        s.wall_done(&job, out).unwrap();
        assert_eq!(colors::current().unwrap().name, "#c02020");
        assert_eq!(colors::follow::following().unwrap(), followed);
        assert_eq!(s.settings.colors.style, "vibrant");
        colors::follow::clear_snapshot().unwrap();
    }
}
