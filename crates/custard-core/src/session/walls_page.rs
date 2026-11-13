//! The Wallpaper page's half of the session.
//!
//! Putting a wallpaper up starts and stops processes and can take most of a
//! second, so it is split the same way a colors build is: the session makes
//! a [`WallJob`], the UI runs it on a thread, and [`Session::wall_done`]
//! takes the result back.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::Session;
use crate::colors::wallpaper::session_processes;
use crate::error::{Error, Result};
use crate::walls::{self, library, setter, startup, thumbs};
use crate::{colors, hyprctl, paths};

/// One wallpaper to put up.
#[derive(Debug, Clone)]
pub struct WallJob {
    /// What to do, in order.
    pub steps: Vec<setter::Step>,
    /// The program showing it.
    pub program: &'static str,
    /// The file or folder.
    pub path: PathBuf,
    /// One monitor, or `None` for all of them.
    pub monitor: Option<String>,
    /// The color choices to rebuild the desktop's colors with, afterwards.
    pub recolor: Option<colors::ColorSettings>,
}

impl WallJob {
    /// Do it, look at the result, and read colors from it if asked: all of
    /// it slow, none of it touching the session.
    pub fn run(&self) -> WallOutcome {
        let result = setter::run(&self.steps).map_err(|e| e.to_string());
        if result.is_ok() && !cfg!(test) {
            // A program just started needs a moment before it can be asked
            // what it shows.
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
        let scan = if cfg!(test) { walls::Scan::default() } else { walls::scan() };
        let colors = match (&self.recolor, &result) {
            (Some(settings), Ok(())) => {
                // Nothing put up: the colors come from what is on screen.
                let found = if self.steps.is_empty() {
                    scan.found.clone()
                } else {
                    walls::with_set(&scan.found, self.program, self.monitor.as_deref(), &self.path)
                };
                Some(colors::build_from(settings, &found).map_err(|e| e.to_string()))
            }
            _ => None,
        };
        WallOutcome { result, scan, colors }
    }
}

/// What came of a [`WallJob`].
#[derive(Debug, Clone)]
pub struct WallOutcome {
    /// Whether it went up.
    pub result: std::result::Result<(), String>,
    /// The session afterwards.
    pub scan: walls::Scan,
    /// The colors read from it, when asked for.
    pub colors: Option<std::result::Result<colors::Built, String>>,
}

impl Session {
    /// Run a `wallpaper.*` command.
    pub(super) fn wall_command(
        &mut self,
        command: &str,
        payload: &Value,
    ) -> Result<Option<String>> {
        match command {
            "wallpaper.set" => self.set_wall_choices(payload),
            "wallpaper.addFolder" => {
                let path = folder_arg(payload)?;
                if !path.is_dir() {
                    return Err(Error::other(format!(
                        "{} is not a folder",
                        paths::contract(&path)
                    )));
                }
                let shown = paths::contract(&path);
                // A folder on the list already lists everything inside it.
                if let Some(outer) =
                    self.wall_folders().into_iter().find(|f| path.starts_with(&f.path))
                {
                    let outer = paths::contract(&outer.path);
                    return Ok(Some(if outer == shown {
                        format!("{shown} is already on the list")
                    } else {
                        format!(
                            "{shown} is inside {outer}, which is already on the list, \
                             so its wallpapers show there"
                        )
                    }));
                }
                let w = &mut self.settings.wallpaper;
                let before = w.hidden.len();
                w.hidden.retain(|h| paths::expand(h) != path);
                if w.hidden.len() == before && !w.folders.contains(&shown) {
                    w.folders.push(shown.clone());
                }
                self.settings.save()?;
                if library::scan(std::slice::from_ref(&path)).is_empty() {
                    return Ok(Some(format!(
                        "Added {shown}. It has no images or videos yet; Look again once it does"
                    )));
                }
                Ok(None)
            }
            "wallpaper.removeFolder" => {
                let path = folder_arg(payload)?;
                let w = &mut self.settings.wallpaper;
                let before = w.folders.len();
                w.folders.retain(|f| paths::expand(f) != path);
                if w.folders.len() == before {
                    w.hidden.push(paths::contract(&path));
                }
                self.settings.save()?;
                Ok(Some(format!("Took {} off the list", paths::contract(&path))))
            }
            "wallpaper.keepAtLogin" => self.keep_wall_at_login(),
            "wallpaper.giveBack" => {
                let path = self.settings.resolved_config_path();
                let text = paths::read_to_string(&path)?;
                let next = startup::give_back(&text)?;
                self.write_config_text(&path, &text, &next)?;
                Ok(Some(
                    "Your own lines start the wallpaper at login again, as they did before".into(),
                ))
            }
            other => Err(Error::other(format!("unknown command `{other}`"))),
        }
    }

    /// Change some of the page's choices. Only the keys given change.
    fn set_wall_choices(&mut self, payload: &Value) -> Result<Option<String>> {
        let mut merged = serde_json::to_value(&self.settings.wallpaper)?;
        let (Some(into), Some(from)) = (merged.as_object_mut(), payload.as_object()) else {
            return Err(Error::other("wallpaper.set takes an object of choices"));
        };
        for (k, v) in from {
            match (k.as_str(), v.as_object(), into.get_mut("mpv").and_then(Value::as_object_mut)) {
                // mpvpaper's choices merge too: `{"mpv": {"shuffle": true}}`
                // keeps the timer.
                ("mpv", Some(changes), Some(mpv)) => {
                    for (mk, mv) in changes {
                        mpv.insert(mk.clone(), mv.clone());
                    }
                }
                ("program" | "fit" | "recolor", _, _) => {
                    into.insert(k.clone(), v.clone());
                }
                _ => {
                    return Err(Error::other(format!("there is no wallpaper choice called `{k}`")))
                }
            }
        }
        let next: walls::WallSettings = serde_json::from_value(merged)
            .map_err(|e| Error::other(format!("not a valid wallpaper choice: {e}")))?;
        let next = next.sanitized();
        if next != self.settings.wallpaper {
            self.settings.wallpaper = next;
            self.settings.save()?;
        }
        Ok(None)
    }

    /// Take a scan made away from the UI thread.
    pub fn set_wall_scan(&mut self, scan: walls::Scan) {
        self.wall_scan = Some(scan);
        self.remember_mpvpaper();
        // The found folders come from it.
        self.refresh_desk();
    }

    /// Keep the user's own mpvpaper command when the scan shows one running,
    /// for a pick made after another program has taken over.
    fn remember_mpvpaper(&mut self) {
        let Some(seen) = self.wall_scan.as_ref().and_then(|s| setter::users_mpvpaper(&s.procs))
        else {
            return;
        };
        if seen != self.settings.wallpaper.mpvpaper {
            self.settings.wallpaper.mpvpaper = seen;
            // Losing it only loses a convenience; the pick still works.
            let _ = self.settings.save();
        }
    }

    /// Work out how to put up the wallpaper `payload` names
    /// (`{"path": ..., "monitor": ...}`, no monitor meaning all of them).
    pub fn wall_job(&mut self, payload: &Value) -> Result<WallJob> {
        if self.wall_busy {
            return Err(Error::other("still putting up the last one"));
        }
        // Only the colors, read from what is on screen now: for the
        // recolor switch turned on, with no new pick to wait for.
        if payload.get("colorsOnly").and_then(Value::as_bool) == Some(true) {
            let running = self.wall_scan.as_ref().map(|s| s.running.clone()).unwrap_or_default();
            let program = walls::pick_program(&self.settings.wallpaper.program, &running)
                .map(|p| p.id)
                .unwrap_or_default();
            let recolor = colors::follow::following().unwrap_or_else(|| colors::ColorSettings {
                source: "wallpaper".into(),
                ..self.settings.colors.clone()
            });
            self.wall_busy = true;
            return Ok(WallJob {
                steps: Vec::new(),
                program,
                path: PathBuf::new(),
                monitor: None,
                recolor: Some(recolor),
            });
        }
        let path = payload
            .get("path")
            .and_then(Value::as_str)
            .map(paths::expand)
            .ok_or_else(|| Error::other("no wallpaper given"))?;
        let monitor = payload
            .get("monitor")
            .and_then(Value::as_str)
            .filter(|m| !m.is_empty())
            .map(String::from);
        let procs = session_processes(true);
        let running = setter::running(&procs);
        let program =
            walls::pick_program(&self.settings.wallpaper.program, &running).ok_or_else(|| {
                Error::other(
                    "No wallpaper program is installed. Install one of awww, swww, hyprpaper, \
                     swaybg or mpvpaper (the only one of them that plays videos).",
                )
            })?;
        if !setter::installed(&program) && !cfg!(test) {
            return Err(Error::other(format!("{} is not installed", program.id)));
        }
        setter::check_version(&program)?;
        let monitors = match &self.wall_scan {
            Some(s) if !s.found.monitors.is_empty() => s.found.monitors.clone(),
            _ => hyprctl::monitors()
                .map(|ms| ms.into_iter().map(|m| m.name).collect())
                .unwrap_or_default(),
        };
        // The user's options when no mpvpaper is running to copy them from:
        // as last seen running, else from the line that starts it at login.
        let mpvpaper = Some(self.settings.wallpaper.mpvpaper.clone())
            .filter(|a| !a.is_empty())
            .or_else(|| {
                let text = std::fs::read_to_string(self.settings.resolved_config_path()).ok()?;
                startup::users_mpvpaper(&text)
            })
            .unwrap_or_default();
        let request = setter::Request {
            program,
            monitor: monitor.clone(),
            path: path.clone(),
            fit: self.settings.wallpaper.fit.clone(),
            monitors,
            hyprpaper_conf: walls::hyprpaper_conf(),
            mpvpaper,
            mpv: self.settings.wallpaper.mpv.clone(),
        };
        let steps = setter::plan(&request, &procs)?;

        // Colors follow it when following is on — with the choices being
        // followed, not whatever the Colors page is trying out, and at once
        // rather than after the watcher's minimum gap — or when the user
        // asked for that here, with the Colors page's choices read from this
        // wallpaper. Neither changes the page or what is followed.
        let recolor = colors::follow::following().or_else(|| {
            self.settings.wallpaper.recolor.then(|| colors::ColorSettings {
                source: "wallpaper".into(),
                ..self.settings.colors.clone()
            })
        });
        self.wall_busy = true;
        Ok(WallJob { steps, program: program.id, path, monitor, recolor })
    }

    /// Take back a finished [`WallJob`]: keep the login block in step with
    /// what is on screen, and apply the colors read from it.
    pub fn wall_done(&mut self, job: &WallJob, outcome: WallOutcome) -> Result<Option<String>> {
        let result = self.finish_wall(job, outcome);
        self.refresh_desk();
        result
    }

    fn finish_wall(&mut self, job: &WallJob, outcome: WallOutcome) -> Result<Option<String>> {
        self.wall_busy = false;
        self.wall_scan = Some(outcome.scan);
        self.remember_mpvpaper();
        if let Err(e) = outcome.result {
            return Err(Error::other(e));
        }
        if job.steps.is_empty() {
            let (Some(asked), Some(built)) = (&job.recolor, outcome.colors) else {
                return Ok(None);
            };
            let applied =
                built.map_err(Error::other).and_then(|b| self.apply_scheme(&b.scheme, asked));
            if self.settings.colors.source == "wallpaper" && !self.colors_building {
                self.colors_built = None;
            }
            return applied
                .map(|a| Some(format!("Colors read from the wallpaper on screen. {a}")))
                .map_err(|e| Error::other(format!("The colors were not changed: {e}")));
        }
        let what =
            job.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let on = job.monitor.as_deref().map(|m| format!(" on {m}")).unwrap_or_default();
        let mut msg = format!("{what} is up{on}, shown by {}", job.program);

        let path = self.settings.resolved_config_path();
        let text = if path.exists() { paths::read_to_string(&path)? } else { String::new() };
        if startup::managed(&text) {
            let next = startup::update(&text, &startup::body(&self.wall_commands(job.program)?))?;
            self.write_config_text(&path, &text, &next)?;
        } else if !startup::user_lines(&text).is_empty() {
            msg.push_str(". Your hyprland.lua still starts the old one at login");
        }

        if let (Some(asked), Some(built)) = (&job.recolor, outcome.colors) {
            match built.map_err(Error::other).and_then(|b| self.apply_scheme(&b.scheme, asked)) {
                Ok(applied) => msg.push_str(&format!(". {applied}")),
                Err(e) => msg.push_str(&format!(". The colors were not changed: {e}")),
            }
            // A Colors page reading the wallpaper is showing the old one.
            if self.settings.colors.source == "wallpaper" && !self.colors_building {
                self.colors_built = None;
            }
        }
        Ok(Some(msg))
    }

    /// What starts `program` the way it shows things now. For hyprpaper,
    /// also writes the config that command reads.
    fn wall_commands(&self, program: &str) -> Result<Vec<String>> {
        let conf = walls::hyprpaper_conf();
        if program == "hyprpaper" && !cfg!(test) {
            let shown: Vec<(Option<String>, PathBuf)> = self
                .wall_scan
                .iter()
                .flat_map(|s| &s.found.wallpapers)
                .filter(|w| w.program == "hyprpaper")
                .map(|w| (w.monitor.clone(), w.path.clone()))
                .collect();
            if let Some(dir) = conf.parent() {
                std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
            }
            paths::write_atomic(
                &conf,
                &setter::hyprpaper_conf_text(&shown, &self.settings.wallpaper.fit),
            )?;
        }
        let procs = self.wall_scan.as_ref().map(|s| s.procs.as_slice()).unwrap_or_default();
        Ok(startup::commands(program, procs, &conf))
    }

    /// Start what is on screen now at each login, in place of the user's
    /// own lines.
    fn keep_wall_at_login(&mut self) -> Result<Option<String>> {
        let running = self.wall_scan.as_ref().map(|s| s.running.clone()).unwrap_or_default();
        let Some(program) = running.first().copied() else {
            return Err(Error::other(
                "No wallpaper program hyprcustard can start is running. Pick a wallpaper first",
            ));
        };
        let commands = self.wall_commands(program)?;
        if commands.is_empty() {
            return Err(Error::other(format!("could not read how {program} was started")));
        }
        let path = self.settings.resolved_config_path();
        let text = if path.exists() { paths::read_to_string(&path)? } else { String::new() };
        let moved = startup::user_lines(&text).len();
        let next = startup::hand_over(&text, &startup::body(&commands))?;
        self.write_config_text(&path, &text, &next)?;
        Ok(Some(match moved {
            0 => format!("{program} now starts at login with what is on screen"),
            1 => format!(
                "{program} now starts at login with what is on screen. Your own line is \
                 commented out, and Give it back restores it"
            ),
            n => format!(
                "{program} now starts at login with what is on screen. Your {n} lines are \
                 commented out, and Give it back restores them"
            ),
        }))
    }

    /// Write the config if it changed, after a backup.
    fn write_config_text(&mut self, path: &Path, old: &str, new: &str) -> Result<()> {
        if new != old {
            if path.exists() {
                paths::backup(path, self.settings.backups_to_keep)?;
            }
            paths::write_atomic(path, new)?;
            self.hand_rules = super::read_hand_rules(path);
        }
        Ok(())
    }

    /// The folders to list, as paths.
    pub fn wall_folders(&self) -> Vec<library::Folder> {
        let found = self.wall_scan.as_ref().map(|s| s.found.clone()).unwrap_or_default();
        library::folders(&found, &self.settings.wallpaper.folders, &self.settings.wallpaper.hidden)
    }

    /// The `wallpaperLibrary` query: every file in the folders, grouped,
    /// with where each one's preview is (or will be).
    pub fn wall_library(&self) -> Value {
        let folders = self.wall_folders();
        let roots: Vec<PathBuf> = folders.iter().map(|f| f.path.clone()).collect();
        let groups = library::scan(&roots);
        let total: usize = groups.iter().map(|g| g.files.len()).sum();
        json!({
            "total": total,
            "capped": total >= library::MAX_FILES,
            "groups": groups.iter().map(|g| json!({
                "dir": g.dir,
                "display": paths::contract(&g.dir),
                "name": g.dir.strip_prefix(&g.root).ok()
                    .filter(|r| !r.as_os_str().is_empty())
                    .map(|r| r.display().to_string())
                    .unwrap_or_else(|| paths::contract(&g.dir)),
                "files": g.files.iter().map(|f| file_view(f)).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })
    }

    /// The page's state.
    pub(super) fn wall_view(&self) -> Value {
        let w = &self.settings.wallpaper;
        let scan = self.wall_scan.as_ref();
        let running: Vec<&str> = scan.map(|s| s.running.clone()).unwrap_or_default();
        let using = walls::pick_program(&w.program, &running);
        let monitors: Vec<Value> = scan
            .map(|s| {
                s.found
                    .monitors
                    .iter()
                    .map(|m| {
                        let shown = s
                            .found
                            .wallpapers
                            .iter()
                            .find(|x| x.monitor.as_deref() == Some(m))
                            .or_else(|| s.found.wallpapers.iter().find(|x| x.monitor.is_none()));
                        json!({
                            "name": m,
                            "wallpaper": shown.map(|x| {
                                let file = x.file().map(Path::to_path_buf);
                                let thumb = file.as_deref().and_then(thumbs::path_for);
                                json!({
                                    "program": x.program,
                                    // As the library lists folders: no
                                    // trailing slash.
                                    "path": x.path.components().collect::<PathBuf>(),
                                    "display": paths::contract(&x.path),
                                    "folder": x.folder().is_some(),
                                    "file": file,
                                    "fileName": file.as_ref()
                                        .and_then(|f| f.file_name())
                                        .map(|n| n.to_string_lossy().into_owned()),
                                    "ready": thumb.as_ref().is_some_and(|t| t.exists()),
                                    "thumb": thumb,
                                })
                            }),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let config = self.settings.resolved_config_path();
        let desk = &self.desk;
        let line =
            |l: &startup::Line| json!({ "number": l.number, "text": l.text, "movable": l.movable });
        // What mpvpaper would play a folder with now, for the page's
        // timer and shuffle switches.
        let mpv_argv = setter::mpvpaper_for_folder(
            scan.map(|s| s.procs.as_slice()).unwrap_or_default(),
            &w.mpvpaper,
            &w.fit,
            &w.mpv,
        );
        let mpv = json!({
            "changeEvery": setter::slideshow(&mpv_argv).unwrap_or(0),
            "shuffle": setter::shuffled(&mpv_argv),
        });
        json!({
            "settings": w,
            "busy": self.wall_busy,
            "scanned": scan.is_some(),
            "programs": setter::PROGRAMS.iter().map(|p| json!({
                "id": p.id,
                "installed": desk.programs.contains(&p.id),
                "running": running.contains(&p.id),
                "videos": p.videos,
                "folders": p.folders,
                "gifs": p.gifs,
            })).collect::<Vec<_>>(),
            "mpv": mpv,
            "using": using.map(|p| {
                json!({ "id": p.id, "videos": p.videos, "folders": p.folders, "gifs": p.gifs })
            }),
            "monitors": monitors,
            "unreadable": scan.map(|s| s.found.unreadable.clone()).unwrap_or_default(),
            "folders": desk.folders.iter().map(|f| json!({
                "path": f.path,
                "display": paths::contract(&f.path),
                "found": f.found,
            })).collect::<Vec<_>>(),
            "startup": {
                "config": paths::contract(&config),
                "managed": desk.login_managed,
                "lines": desk.login_lines.iter().map(line).collect::<Vec<_>>(),
                "moved": desk.login_moved.iter().map(line).collect::<Vec<_>>(),
                "block": desk.login_block.as_ref().map(|(number, text)| json!({
                    "number": number,
                    "text": text,
                })),
                "services": scan.map(|s| s.services.clone()).unwrap_or_default(),
            },
            "colorsFollow": desk.following.is_some(),
            "colorsSource": self.settings.colors.source,
            "ffmpeg": desk.ffmpeg,
        })
    }
}

/// One file in the library.
fn file_view(f: &Path) -> Value {
    let thumb = thumbs::path_for(f);
    json!({
        "path": f,
        "name": f.file_name().map(|n| n.to_string_lossy().into_owned()),
        "video": library::video(f),
        "gif": library::gif(f),
        "thumb": thumb,
        "ready": thumb.as_ref().is_some_and(|t| t.exists()),
    })
}

/// A folder from a payload: a path, `~/...`, or a `file://` URL as a folder
/// dialog gives it.
fn folder_arg(payload: &Value) -> Result<PathBuf> {
    let raw = payload
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::other("no folder given"))?;
    let raw = raw.strip_prefix("file://").map(percent_decode).unwrap_or_else(|| raw.to_string());
    let p = paths::expand(&raw);
    Ok(std::fs::canonicalize(&p).unwrap_or(p))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_urls_are_decoded() {
        let p = folder_arg(&json!({ "path": "file:///no/such/My%20Walls" })).unwrap();
        assert_eq!(p, PathBuf::from("/no/such/My Walls"));
        assert_eq!(percent_decode("a%2"), "a%2");
    }
}
