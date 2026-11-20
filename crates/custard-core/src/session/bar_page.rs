//! The Bar page's half of the session: hyprcustard's own Quickshell bar.
//!
//! Its choices save at once: into the app's settings, and into the bar's
//! `bar.json`, which a running bar follows without restarting. What depends
//! on the machine (fonts, a mixer, monitors, whether Quickshell is there)
//! starts processes to find out, so it is read when the page asks.

use serde_json::{json, Value};

use super::Session;
use crate::error::{Error, Result};
use crate::{bar, colors, hyprctl, paths};

/// What the page last found about the machine.
#[derive(Debug, Clone, Default)]
pub(super) struct BarScan {
    pub machine: bar::Machine,
    pub monitors: Vec<String>,
}

impl Session {
    pub(super) fn bar_command(&mut self, name: &str, payload: &Value) -> Result<Option<String>> {
        let home = paths::apps_config_home();
        match name {
            "bar.refresh" => {
                self.desk.bar_scan = Some(BarScan {
                    machine: bar::Machine::detect(),
                    monitors: if cfg!(test) {
                        Vec::new()
                    } else {
                        hyprctl::monitors()
                            .map(|ms| ms.into_iter().map(|m| m.name).collect())
                            .unwrap_or_default()
                    },
                });
                Ok(None)
            }
            "bar.set" => self.set_bar_choices(payload),
            "bar.install" => {
                let scheme = colors::current().map(Ok).unwrap_or_else(|| {
                    colors::build(&colors::ColorSettings::default(), &colors::named::builtins())
                        .map(|b| b.scheme)
                })?;
                bar::install(&home, &self.settings.bar, &self.bar_machine(), &scheme)?;
                Ok(Some(format!(
                    "The bar is set up in {}. Start it now, or have it start at login",
                    paths::contract(&bar::dir(&home))
                )))
            }
            "bar.start" | "bar.stop" | "bar.restart" | "bar.remove" => {
                // The window runs these on a thread (bar_control, then
                // bar_control_done); here they run in one go.
                let job = self.bar_control(name)?;
                let outcome = job.run();
                self.bar_control_done(&job, outcome)
            }
            "bar.login" => {
                let on = payload.get("on").and_then(Value::as_bool).unwrap_or(true);
                let path = self.settings.resolved_config_path();
                let text =
                    if path.exists() { paths::read_to_string(&path)? } else { String::new() };
                if on {
                    let program = self.bar_program()?;
                    self.bar_ready()?;
                    let next = bar::add_block(&text, &bar::start_command(&program, &home))?;
                    self.write_config_text(&path, &text, &next)?;
                    let others = bar::user_lines(&next).len();
                    Ok(Some(if others > 0 {
                        format!(
                            "The bar starts at login, from {}. Your config still starts \
                             another bar too",
                            paths::contract(&path)
                        )
                    } else {
                        format!("The bar starts at login, from {}", paths::contract(&path))
                    }))
                } else {
                    let moved = bar::moved_lines(&text).len();
                    let next = bar::give_back(&text)?;
                    self.write_config_text(&path, &text, &next)?;
                    Ok(Some(match moved {
                        0 => "The bar no longer starts at login".into(),
                        1 => "The bar no longer starts at login, and your own line is back".into(),
                        n => format!(
                            "The bar no longer starts at login, and your {n} lines are back"
                        ),
                    }))
                }
            }
            "bar.handOver" => {
                let program = self.bar_program()?;
                self.bar_ready()?;
                let path = self.settings.resolved_config_path();
                let text = paths::read_to_string(&path)?;
                let next = bar::hand_over(&text, &bar::start_command(&program, &home))?;
                let moved = bar::moved_lines(&next).len() - bar::moved_lines(&text).len();
                self.write_config_text(&path, &text, &next)?;
                Ok(Some(format!(
                    "{moved} {} in {} {} commented out, and the bar starts at login in {}. \
                     Give {} back restores {}. The other bar is still running until you log \
                     in again or stop it",
                    if moved == 1 { "line" } else { "lines" },
                    paths::contract(&path),
                    if moved == 1 { "is" } else { "are" },
                    if moved == 1 { "its place" } else { "their place" },
                    if moved == 1 { "it" } else { "them" },
                    if moved == 1 { "it" } else { "them" },
                )))
            }
            other => Err(Error::other(format!("unknown command `{other}`"))),
        }
    }

    /// Start, stop, restart or remove the bar: `bar.start` and so on. What
    /// waits on Quickshell is in the returned [`bar::Control`], to run off
    /// the window's thread; [`Session::bar_control_done`] finishes.
    pub fn bar_control(&mut self, name: &str) -> Result<bar::Control> {
        let op = name.trim_start_matches("bar.");
        if let Some(busy) = &self.bar_busy {
            return Err(Error::other(format!(
                "Still waiting for the bar to {busy}; try again in a moment"
            )));
        }
        let program = match op {
            "start" | "restart" => {
                let p = self.bar_program()?;
                self.bar_ready()?;
                p
            }
            "stop" | "remove" => bar::program().unwrap_or_default(),
            other => return Err(Error::other(format!("unknown command `bar.{other}`"))),
        };
        self.bar_busy = Some(op.to_string());
        Ok(bar::Control { op: op.to_string(), program, config_home: paths::apps_config_home() })
    }

    /// Take a [`bar::Control`]'s outcome: the page shows what is true now,
    /// and Remove, once the bar has stopped, takes out its start and files.
    pub fn bar_control_done(
        &mut self,
        job: &bar::Control,
        outcome: Result<String>,
    ) -> Result<Option<String>> {
        self.bar_busy = None;
        let done = outcome.and_then(|msg| {
            if job.op != "remove" {
                return Ok(msg);
            }
            let home = paths::apps_config_home();
            let path = self.settings.resolved_config_path();
            if path.exists() {
                let text = paths::read_to_string(&path)?;
                let next = bar::give_back(&text)?;
                self.write_config_text(&path, &text, &next)?;
            }
            bar::uninstall(&home)?;
            Ok(format!(
                "Removed the bar from {}, and its start from your config. Its settings \
                 are kept, should you set it up again",
                paths::contract(&bar::dir(&home))
            ))
        });
        self.refresh_desk();
        done.map(Some)
    }

    /// Quickshell's program, or why the bar cannot run.
    fn bar_program(&self) -> Result<std::path::PathBuf> {
        if cfg!(test) {
            return Ok(std::path::PathBuf::from("/usr/bin/qs"));
        }
        bar::program().ok_or_else(|| {
            Error::other(
                "Quickshell is not installed. Install the quickshell package with your \
                 package manager, then come back to this page",
            )
        })
    }

    /// Refused until the bar's files are there.
    fn bar_ready(&self) -> Result<()> {
        if bar::installed(&paths::apps_config_home()) {
            Ok(())
        } else {
            Err(Error::other("Set the bar up first"))
        }
    }

    fn bar_machine(&self) -> bar::Machine {
        match &self.desk.bar_scan {
            Some(s) => s.machine.clone(),
            None => bar::Machine::detect(),
        }
    }

    /// Change some of the page's choices. Only the keys given change.
    fn set_bar_choices(&mut self, payload: &Value) -> Result<Option<String>> {
        let mut merged = serde_json::to_value(&self.settings.bar)?;
        let (Some(into), Some(from)) = (merged.as_object_mut(), payload.as_object()) else {
            return Err(Error::other("bar.set takes an object of choices"));
        };
        for (k, v) in from {
            if !into.contains_key(k) {
                return Err(Error::other(format!("there is no bar choice called `{k}`")));
            }
            into.insert(k.clone(), v.clone());
        }
        let next: bar::BarSettings = serde_json::from_value(merged)
            .map_err(|e| Error::other(format!("not a valid bar choice: {e}")))?;
        let next = next.sanitized();
        if next != self.settings.bar {
            self.settings.bar = next;
            self.settings.save()?;
        }
        let home = paths::apps_config_home();
        if bar::installed(&home) {
            bar::write_settings(&home, &self.settings.bar, &self.bar_machine())?;
        }
        Ok(None)
    }

    pub(super) fn bar_view(&self) -> Value {
        let home = paths::apps_config_home();
        let scan = self.desk.bar_scan.as_ref();
        let machine = scan.map(|s| s.machine.clone()).unwrap_or_default();
        let config = self.settings.resolved_config_path();
        let line =
            |l: &bar::Line| json!({ "number": l.number, "text": l.text, "movable": l.movable });
        let program = if cfg!(test) { None } else { bar::program() };
        json!({
            "settings": self.settings.bar,
            "scanned": scan.is_some(),
            "quickshell": program.as_ref().map(|p| p.to_string_lossy().into_owned()),
            "installed": self.desk.bar_installed,
            "version": self.desk.bar_version,
            "running": self.desk.bar_running,
            "busy": self.bar_busy,
            "dir": paths::contract(&bar::dir(&home)),
            "command": bar::start_command(
                program.as_deref().unwrap_or(std::path::Path::new("qs")),
                &home
            ),
            "modules": bar::MODULES.iter().map(|(id, label)| json!({
                "id": id,
                "label": label,
            })).collect::<Vec<_>>(),
            "monitors": scan.map(|s| s.monitors.clone()).unwrap_or_default(),
            "nerdFont": machine.nerd_font,
            "mixerFound": machine.mixer,
            "bluetoothFound": machine.bluetooth,
            "startup": {
                "config": paths::contract(&config),
                "managed": self.desk.bar_login_block.is_some(),
                "lines": self.desk.bar_login_lines.iter().map(line).collect::<Vec<_>>(),
                "moved": self.desk.bar_login_moved.iter().map(line).collect::<Vec<_>>(),
                "block": self.desk.bar_login_block.as_ref().map(|(number, text)| json!({
                    "number": number,
                    "text": text,
                })),
            },
        })
    }
}
