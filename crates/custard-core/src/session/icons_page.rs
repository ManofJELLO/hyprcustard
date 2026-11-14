//! The Icons page's half of the session: icon and cursor themes.
//!
//! Finding the themes reads every theme folder and writes cursor previews,
//! so it happens when the page asks rather than on every refresh. Setting
//! one writes at once; nothing waits for Save.

use serde_json::{json, Value};

use super::Session;
use crate::error::{Error, Result};
use crate::{icons, paths};

/// What the page last found.
#[derive(Debug, Clone, Default)]
pub(super) struct IconScan {
    pub icons: Vec<icons::IconTheme>,
    pub cursors: Vec<icons::CursorTheme>,
    pub current: icons::Current,
}

impl Session {
    pub(super) fn icons_command(&mut self, name: &str, payload: &Value) -> Result<Option<String>> {
        let places = icons::Places::session();
        let keep = self.settings.backups_to_keep;
        let theme = || {
            payload
                .get("theme")
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| Error::other(format!("{name} needs a theme")))
        };
        let result = match name {
            "icons.refresh" => {
                self.desk.icon_scan = Some(IconScan {
                    icons: icons::icon_themes(&places),
                    cursors: icons::cursor_themes(&places),
                    current: icons::current(&places),
                });
                return Ok(None);
            }
            "icons.setIcons" => {
                let theme = theme()?;
                let changed = icons::set_icon_theme(&places, &theme, keep)?;
                Ok(Some(format!(
                    "Icons are now {theme} in {}. Apps already open keep theirs until they restart",
                    changed.join(", ")
                )))
            }
            "icons.setCursor" => {
                let theme = theme()?;
                let size = self.cursor_size(payload);
                let changed = icons::set_cursor_theme(&places, &theme, size, keep)?;
                let mut msg =
                    format!("The cursor is now {theme} at {size} in {}", changed.join(", "));
                msg.push_str(&self.cursor_block(&theme, size, false)?);
                Ok(Some(msg))
            }
            "icons.takeOver" => {
                let (theme, size) = self.cursor_now(payload)?;
                let msg = self.cursor_block(&theme, size, true)?;
                Ok(Some(msg.trim_start_matches(". ").to_string()))
            }
            "icons.giveBack" => {
                let path = self.settings.resolved_config_path();
                let text = paths::read_to_string(&path)?;
                let written = icons::give_back(&text)?;
                self.write_config_text(&path, &text, &written)?;
                Ok(Some("Your own cursor lines are back as they were".into()))
            }
            other => Err(Error::other(format!("unknown command `{other}`"))),
        };
        if let Some(scan) = self.desk.icon_scan.as_mut() {
            scan.current = icons::current(&places);
        }
        result
    }

    fn cursor_size(&self, payload: &Value) -> u32 {
        payload
            .get("size")
            .and_then(Value::as_u64)
            .map(|s| s as u32)
            .or_else(|| self.desk.icon_scan.as_ref().and_then(|s| s.current.cursor_size))
            .unwrap_or(24)
    }

    /// The cursor to write when taking over: the one asked for, else the one
    /// gsettings or GTK has now.
    fn cursor_now(&self, payload: &Value) -> Result<(String, u32)> {
        let size = self.cursor_size(payload);
        if let Some(t) = payload.get("theme").and_then(Value::as_str) {
            return Ok((t.to_string(), size));
        }
        let current = icons::current(&icons::Places::session());
        current
            .cursors
            .first()
            .map(|s| (s.value.clone(), size))
            .ok_or_else(|| Error::other("no cursor theme is set anywhere yet; pick one first"))
    }

    /// Write the cursor block, taking over the user's lines when `take_over`.
    /// The returned text continues a sentence about the cursor.
    fn cursor_block(&mut self, theme: &str, size: u32, take_over: bool) -> Result<String> {
        let path = self.settings.resolved_config_path();
        let text = if path.exists() { paths::read_to_string(&path)? } else { String::new() };
        let hyprcursor = self
            .desk
            .icon_scan
            .as_ref()
            .and_then(|s| s.cursors.iter().find(|c| c.id == theme))
            .is_some_and(|c| c.hyprcursor);
        let body = icons::body(theme, size, hyprcursor);
        let mine = icons::user_lines(&text);
        let written = if take_over {
            icons::hand_over(&text, &body)?
        } else if icons::managed(&text) || mine.is_empty() {
            icons::apply_block(&text, &body)?
        } else {
            let numbers: Vec<String> = mine.iter().map(|l| l.number.to_string()).collect();
            return Ok(format!(
                ". Lines {} of {} set the cursor at login too, so it goes back to theirs \
                 then: take them over to keep this one",
                numbers.join(", "),
                paths::contract(&path)
            ));
        };
        let moved = icons::moved_lines(&written).len() - icons::moved_lines(&text).len();
        self.write_config_text(&path, &text, &written)?;
        Ok(if moved > 0 {
            format!(
                ". {} {} of yours in {} {} commented out; Give them back restores them",
                moved,
                if moved == 1 { "line" } else { "lines" },
                paths::contract(&path),
                if moved == 1 { "is" } else { "are" }
            )
        } else {
            format!(", and in {} for the next login", paths::contract(&path))
        })
    }

    pub(super) fn icons_view(&self) -> Value {
        let scan = self.desk.icon_scan.as_ref();
        json!({
            "scanned": scan.is_some(),
            "icons": scan.map(|s| s.icons.clone()).unwrap_or_default(),
            "cursors": scan.map(|s| s.cursors.clone()).unwrap_or_default(),
            "current": scan.map(|s| s.current.clone()).unwrap_or_default(),
            "cursorLines": self.desk.cursor_lines,
            "cursorMoved": self.desk.cursor_moved,
            "cursorManaged": self.desk.cursor_managed,
        })
    }
}
