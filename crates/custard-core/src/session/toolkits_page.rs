//! The GTK and Qt page's half of the session: GTK 3 and Kvantum themes.
//!
//! Finding the themes reads every theme folder, so it happens when the page
//! asks. Setting one writes at once. Previews are drawn by the GUI on a
//! thread, from a [`PreviewJob`] made here.

use serde_json::{json, Value};

use super::Session;
use crate::colors::{self, pins, toolkits};
use crate::error::{Error, Result};
use crate::paths;
use crate::toolkit_themes::{self as tt, PreviewJob};

/// What the page last found.
#[derive(Debug, Clone, Default)]
pub(super) struct ToolkitScan {
    pub gtk: Vec<tt::GtkTheme>,
    pub kvantum: Vec<tt::KvantumTheme>,
    /// Lines in the Hyprland config that set the GTK theme each time it runs.
    pub gtk_pins: Vec<pins::Pin>,
    /// The GTK theme in use.
    pub gtk_now: String,
    /// Whether GTK is told to prefer dark versions.
    pub prefer_dark: bool,
    /// What Qt draws with.
    pub qt_now: tt::QtNow,
}

/// Whether GTK 3 and Kvantum are installed: `(gtk, qt)`. Read with the rest
/// of the desk's facts, not on every state.
pub(super) fn installed() -> (bool, bool) {
    (tt::gtk_installed(), tt::kvantum_installed())
}

/// The page's entry in the state's `toolkitsFound`.
pub(super) fn found_view((gtk, qt): (bool, bool)) -> Value {
    json!({ "gtk": gtk, "qt": qt, "any": gtk || qt })
}

impl Session {
    pub(super) fn toolkits_command(
        &mut self,
        name: &str,
        payload: &Value,
    ) -> Result<Option<String>> {
        let home = paths::apps_config_home();
        let keep = self.settings.backups_to_keep;
        let theme = || {
            payload
                .get("theme")
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
                .map(str::to_string)
                .ok_or_else(|| Error::other(format!("{name} needs a theme")))
        };
        let result = match name {
            "toolkits.refresh" => Ok(None),
            "toolkits.useGtk" => {
                let theme = theme()?;
                let found = self.gtk_theme(&theme);
                tt::use_gtk(&home, &theme, keep)?;
                let mut msg = format!(
                    "GTK 3 apps now use {theme}, set in gsettings and GTK's settings.ini. Most \
                     open ones switch at once; the rest when they next start"
                );
                if let Some(t) = found.filter(|t| !t.follows) {
                    if colors::current().is_some() && !self.color_target_off("gtk3") {
                        msg.push_str(&format!(
                            ". {} has colors of its own, so GTK 3 apps no longer follow your \
                             color scheme; libadwaita apps still do",
                            t.name
                        ));
                    }
                }
                msg.push_str(&self.gtk_pins_note());
                Ok(Some(msg))
            }
            "toolkits.useKvantum" => {
                let theme = theme()?;
                let desk = toolkits::Desktop::read(&home);
                let base = toolkits::kvantum_named(&home, &desk, &theme).ok_or_else(|| {
                    Error::other(format!("there is no Kvantum theme called {theme}"))
                })?;
                let recolor = payload.get("recolor").and_then(Value::as_bool).unwrap_or(false);
                let (changed, mut msg) = if recolor {
                    let scheme = self.kvantum_scheme().ok_or_else(|| {
                        Error::other("there is no color scheme to recolor it with: apply one on the Colors page first")
                    })?;
                    let changed = tt::use_kvantum_recolored(&home, &base, &scheme, keep)?;
                    (changed, format!("Qt apps now use {theme}, recolored with {}", scheme.name))
                } else {
                    (tt::use_kvantum(&home, &theme, keep)?, format!("Qt apps now use {theme}"))
                };
                msg.push_str(", from the next time each one starts");
                if !changed.is_empty() {
                    msg.push_str(&format!(
                        ". Kvantum is now the style in {}",
                        changed.join(" and ")
                    ));
                }
                let now = tt::qt_now(&home, &desk);
                if let Some(o) = now.style_override.as_deref().filter(|o| *o != "kvantum") {
                    msg.push_str(&format!(
                        ". But QT_STYLE_OVERRIDE is set to {o}, which Qt follows over qt6ct: the \
                         Colors page can take it out"
                    ));
                } else if !now.uses_kvantum() {
                    msg.push_str(&format!(
                        ". But Qt apps are not reading qt6ct's settings (QT_QPA_PLATFORMTHEME is \
                         {}), so they will not draw with Kvantum until it is set to qt6ct",
                        now.platform.as_deref().map_or("not set".into(), |p| format!("{p:?}"))
                    ));
                }
                Ok(Some(msg))
            }
            "toolkits.takePins" => {
                let files = self.take_out_pins(&["gtk-theme"])?;
                if files.is_empty() {
                    return Ok(Some("There was nothing to change".into()));
                }
                let mut msg = format!(
                    "Took the line that sets the GTK theme out of {}; it is commented out, \
                     marked as hyprcustard's",
                    files.join(", ")
                );
                // gsettings keeps what such a line last set.
                if let Ok(theme) = theme() {
                    tt::use_gtk(&home, &theme, keep)?;
                    msg.push_str(&format!(", and set {theme} again"));
                }
                Ok(Some(msg))
            }
            other => Err(Error::other(format!("unknown command `{other}`"))),
        };
        self.scan_toolkits();
        result
    }

    fn scan_toolkits(&mut self) {
        let home = paths::apps_config_home();
        let desk = toolkits::Desktop::read(&home);
        let sources = colors::qt_style::hypr_sources(&self.settings.resolved_config_path());
        let prefer_dark = tt::gtk_prefers_dark(&home);
        self.desk.toolkits_found = installed();
        self.desk.toolkit_scan = Some(ToolkitScan {
            gtk: tt::gtk_themes(&desk, prefer_dark),
            kvantum: tt::kvantum_themes(&home, &desk),
            gtk_pins: pins::find(&sources).into_iter().filter(|p| p.key == "gtk-theme").collect(),
            qt_now: tt::qt_now(&home, &desk),
            gtk_now: desk.gtk_theme,
            prefer_dark,
        });
    }

    fn gtk_theme(&self, id: &str) -> Option<tt::GtkTheme> {
        self.desk.toolkit_scan.as_ref()?.gtk.iter().find(|t| t.id == id).cloned()
    }

    fn color_target_off(&self, id: &str) -> bool {
        self.settings.colors.off.iter().any(|o| o == id)
    }

    /// The scheme a Kvantum theme is recolored with: the one applied last,
    /// unless the Colors page has Kvantum turned off.
    fn kvantum_scheme(&self) -> Option<colors::Scheme> {
        if self.color_target_off("kvantum") {
            return None;
        }
        self.colors_applied.clone()
    }

    fn gtk_pins_note(&self) -> String {
        let Some(scan) = &self.desk.toolkit_scan else { return String::new() };
        let live = pins::live(&scan.gtk_pins, "gtk-theme");
        let Some(first) = live.first() else { return String::new() };
        format!(
            ". But {} sets it back to {} each time Hyprland starts or reloads: take that out \
             on this page to keep this one",
            pins::places(&live),
            first.value
        )
    }

    /// The preview a payload of `{kind: "gtk"|"qt", theme, recolor}` asks
    /// for. Drawing it is slow: see [`PreviewJob::run`].
    pub fn toolkit_preview_job(&self, payload: &Value) -> Result<PreviewJob> {
        let home = paths::apps_config_home();
        let theme = payload
            .get("theme")
            .and_then(Value::as_str)
            .filter(|t| !t.is_empty())
            .ok_or_else(|| Error::other("a preview needs a theme"))?;
        let desk = toolkits::Desktop::read(&home);
        match payload.get("kind").and_then(Value::as_str) {
            Some("gtk") => {
                let dir = desk
                    .theme_dirs
                    .iter()
                    .map(|d| d.join(theme))
                    .find(|d| d.join("gtk-3.0/gtk.css").is_file());
                let has_dark = dir
                    .as_ref()
                    .map_or(theme == "Adwaita", |d| d.join("gtk-3.0/gtk-dark.css").is_file());
                let dark = has_dark && tt::gtk_prefers_dark(&home);
                Ok(PreviewJob::gtk(&home, theme, dir.as_deref(), dark))
            }
            Some("qt") => {
                let base = toolkits::kvantum_named(&home, &desk, theme).ok_or_else(|| {
                    Error::other(format!("there is no Kvantum theme called {theme}"))
                })?;
                let recolor = payload.get("recolor").and_then(Value::as_bool).unwrap_or(false);
                Ok(PreviewJob::qt(&home, base, if recolor { self.kvantum_scheme() } else { None }))
            }
            _ => Err(Error::other("a preview is of gtk or qt")),
        }
    }

    /// The page's state: only what the last scan found, so a state sent
    /// for any other page reads no files and starts no processes.
    pub(super) fn toolkits_view(&self) -> Value {
        let Some(scan) = self.desk.toolkit_scan.as_ref() else {
            return json!({ "scanned": false });
        };
        let qt = &scan.qt_now;
        let pins: Vec<Value> = pins::live(&scan.gtk_pins, "gtk-theme")
            .into_iter()
            .map(|p| {
                json!({
                    "place": format!("{}:{}", paths::contract(&p.path), p.line),
                    "value": p.value,
                })
            })
            .collect();
        json!({
            "scanned": true,
            "gtk": {
                "themes": scan.gtk,
                "now": scan.gtk_now,
                "preferDark": scan.prefer_dark,
                "pins": pins,
                "colorsOn": !self.color_target_off("gtk3"),
            },
            "qt": {
                "themes": scan.kvantum,
                "now": qt.theme,
                "base": qt.base,
                "recolored": qt.theme == toolkits::KVANTUM_THEME,
                "usesKvantum": qt.uses_kvantum(),
                "platform": qt.platform,
                "styleOverride": qt.style_override,
                "styles": qt.styles.iter().map(|(ct, s)| json!({ "ct": ct, "style": s })).collect::<Vec<_>>(),
                "scheme": self.kvantum_scheme().map(|s| s.name),
            },
        })
    }
}
