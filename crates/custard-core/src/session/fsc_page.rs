//! The Fishsticks & Custard page's half of the session.
//!
//! Its start and key choices live in the app's settings and are written to
//! a managed block of their own in the Hyprland config, at once rather than
//! on Save, like the autostart block. Its config is FSC's own file, edited in
//! place on the page's own Save; changes to it are staged here until then. Installing is a job the UI runs ([`crate::fsc::job`]), so it is not
//! here at all.

use serde_json::{json, Map, Value};

use super::Session;
use crate::error::{Error, Result};
use crate::{fsc, paths};

impl Session {
    pub(super) fn fsc_command(&mut self, name: &str, payload: &Value) -> Result<Option<String>> {
        match name {
            // What is installed: processes to start, so only when asked.
            "fsc.refresh" => {
                self.desk.fsc_facts = Some(fsc::facts(&self.desk.fsc_values));
                Ok(None)
            }
            "fsc.set" => {
                let next: fsc::FscSettings =
                    serde_json::from_value(payload.get("settings").cloned().unwrap_or(Value::Null))
                        .map_err(|e| Error::other(format!("fsc.set: {e}")))?;
                let next = next.sanitized();
                let path = self.settings.resolved_config_path();
                let text =
                    if path.exists() { paths::read_to_string(&path)? } else { String::new() };
                if !fsc::managed(&text)
                    && fsc::user_lines(&text).iter().any(|l| l.does != fsc::Does::Other)
                {
                    return Err(Error::other(
                        "Your config already starts Fishsticks & Custard or binds its keys. \
                         Take those lines over first, so nothing runs twice",
                    ));
                }
                let written = fsc::apply(&text, &next)?;
                self.write_config_text(&path, &text, &written)?;
                self.settings.fsc = next;
                self.settings.save()?;
                Ok(Some(if fsc::managed(&written) {
                    format!(
                        "Fishsticks & Custard's start and keys are in {}",
                        paths::contract(&path)
                    )
                } else {
                    "Fishsticks & Custard's block is gone from your config".into()
                }))
            }
            "fsc.handOver" => {
                let path = self.settings.resolved_config_path();
                let text = paths::read_to_string(&path)?;
                let (written, next) = fsc::hand_over(&text, &self.settings.fsc)?;
                let moved = fsc::moved_lines(&written).len() - fsc::moved_lines(&text).len();
                self.write_config_text(&path, &text, &written)?;
                self.settings.fsc = next;
                self.settings.save()?;
                Ok(Some(format!(
                    "{} {} in {} {} commented out, and the Fishsticks & Custard block does \
                     what {} did. Give them back restores them",
                    moved,
                    if moved == 1 { "line" } else { "lines" },
                    paths::contract(&path),
                    if moved == 1 { "is" } else { "are" },
                    if moved == 1 { "it" } else { "they" },
                )))
            }
            "fsc.giveBack" => {
                let path = self.settings.resolved_config_path();
                let text = paths::read_to_string(&path)?;
                let written = fsc::give_back(&text)?;
                self.write_config_text(&path, &text, &written)?;
                Ok(Some("Your own Fishsticks & Custard lines are back as they were".into()))
            }
            "fsc.stage" => {
                let field = payload
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| Error::other("fsc.stage needs a name"))?;
                let value = payload.get("value").cloned().unwrap_or(Value::Null);
                self.fsc_pending.insert(field.to_string(), value);
                Ok(None)
            }
            "fsc.revert" => {
                self.fsc_pending.clear();
                Ok(None)
            }
            // Write what is staged, plus any `changes` passed along. What is
            // staged stays staged when the write fails, so nothing is lost.
            "fsc.config" => {
                let mut changes = self.fsc_pending.clone();
                if let Some(more) = payload.get("changes") {
                    let more = more
                        .as_object()
                        .ok_or_else(|| Error::other("fsc.config takes an object of changes"))?;
                    changes.extend(more.clone());
                }
                let path = fsc::config_path();
                let text = fsc::read_config(&path)?;
                let written = fsc::edit_config(&text, &changes)?;
                if written == text {
                    self.fsc_pending.clear();
                    return Ok(Some("Nothing to change in FSC's config".into()));
                }
                let backup = if path.exists() {
                    paths::backup(&path, self.settings.backups_to_keep)?
                } else {
                    None
                };
                paths::write_atomic(&path, &written)?;
                self.fsc_pending.clear();
                let mut msg = format!("Saved {}", paths::contract(&path));
                if let Some(b) = backup {
                    msg.push_str(&format!(" (backup: {})", paths::contract(&b)));
                }
                msg.push_str(". fscd reads it when it starts: restart it to use the new settings");
                Ok(Some(msg))
            }
            "fsc.restart" => {
                let msg = fsc::restart()?;
                self.fsc_recheck_running();
                Ok(Some(msg))
            }
            other => Err(Error::other(format!("unknown command `{other}`"))),
        }
    }

    fn fsc_recheck_running(&mut self) {
        if let Some(facts) = self.desk.fsc_facts.as_mut() {
            facts.running = !crate::procs::named("fscd").is_empty();
        }
    }

    /// Read the Hyprland config's lines and FSC's config again.
    pub(super) fn refresh_fsc(&mut self, hyprland: &str) {
        self.desk.fsc_lines = fsc::user_lines(hyprland);
        self.desk.fsc_moved = fsc::moved_lines(hyprland);
        self.desk.fsc_block = fsc::block_text(hyprland);
        self.desk.fsc_clashes = fsc::clashes(&self.settings.fsc, hyprland);
        let path = fsc::config_path();
        match fsc::read_config(&path).and_then(|t| fsc::config_values(&t)) {
            Ok(values) => {
                self.desk.fsc_values = values;
                self.desk.fsc_config_error = None;
            }
            Err(e) => {
                self.desk.fsc_values = Map::new();
                self.desk.fsc_config_error = Some(e.to_string());
            }
        }
    }

    pub(super) fn fsc_view(&self) -> Value {
        let path = fsc::config_path();
        json!({
            "settings": self.settings.fsc,
            "actions": fsc::Action::ALL.iter().map(|a| json!({
                "action": a, "label": a.label(),
            })).collect::<Vec<_>>(),
            "managed": self.desk.fsc_block.is_some(),
            "block": self.desk.fsc_block.as_ref().map(|(line, text)| json!({
                "line": line, "text": text,
            })),
            "userLines": self.desk.fsc_lines,
            "moved": self.desk.fsc_moved,
            "clashes": self.desk.fsc_clashes,
            "facts": self.desk.fsc_facts,
            "configPath": paths::contract(&path),
            "configExists": path.exists(),
            "configError": self.desk.fsc_config_error,
            "pending": self.fsc_pending,
            "fields": fsc::fields_view(&self.desk.fsc_values),
            "homePage": fsc::HOME_PAGE,
        })
    }
}
