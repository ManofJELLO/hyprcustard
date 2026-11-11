//! Previewing a shader by running it, in a Hyprland of its own.
//!
//! The alternative was to reimplement the plugin inside the GUI — translate the
//! GLSL to Qt's dialect, bake it, and invent values for the twenty-seven
//! uniforms the plugin fills in. That is an approximation, and an approximation
//! that disagrees with the real thing is worse than no preview at all. So the
//! preview *is* the real thing: a second Hyprland, the plugin loaded into it,
//! and one window with the shader tagged onto it. What that window shows is a
//! test card rather than a black terminal — see [`card`] for why.
//!
//! Three things make it unobtrusive:
//!
//! * **It is headless.** A nested instance would otherwise come up with one
//!   output — a window on the user's screen. Its config disables that output
//!   before it is ever opened, and the instance is given a headless one to draw
//!   on instead, so the only thing it renders to is the framebuffer we capture.
//!   Nothing appears on their desktop, not even for a moment, and no rule has
//!   to be pushed into their session to hide it.
//! * **It never touches their files.** The shader it renders is a copy in a
//!   runtime directory, written from whatever is staged in the app — which is
//!   how a preview can show an edit that has not been saved.
//! * **It stands on the user's own wallpaper**, photographed without any of
//!   their windows in the way — see [`wallpaper`] for how. Whether a dissolve
//!   really reaches zero alpha only shows against something with light and dark
//!   in it, and a flat colour shows nothing. Failing that — no screenshot tool,
//!   nothing to show it with, a wallpaper that could not be photographed — the
//!   compositor clears to the colour of the pane instead, and the window
//!   appears to float on the app. True transparency would be better than either
//!   and is not available: on a headless output an opaque window is captured
//!   with zero alpha, with or without the plugin, so the frame would come back
//!   empty.
//! * **It looks like their desktop.** Rounding, gaps, borders, opacity, blur
//!   and their own animation curves are read from the running compositor with
//!   `hyprctl`, so the window in the pane is shaped like the windows around it.

pub mod card;
pub mod config;
pub mod look;
pub mod wallpaper;

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::error::{Error, Result};
use crate::hyprctl;
use crate::model::TagSlot;
use crate::paths;
use crate::preview::config::PreviewConfig;
use crate::preview::look::Look;

/// The headless output the preview draws on.
const OUTPUT: &str = "HEADLESS-1";

/// How long to wait for the nested instance to register itself.
const START_TIMEOUT: Duration = Duration::from_secs(15);

/// What to preview, and how.
#[derive(Debug, Clone)]
pub struct Request {
    /// The shader file being edited, for naming the copy.
    pub original: PathBuf,
    /// Its source, with staged edits already applied.
    pub source: String,
    /// True when the shader drives itself from `progress`.
    pub is_animation: bool,
    /// True when it reads velocity or a move delta, and so needs the window to
    /// actually go somewhere before it shows anything.
    pub is_motion_driven: bool,
    /// The plugin `.so` to load.
    pub plugin_so: PathBuf,
    /// Pane size in pixels.
    pub size: (u32, u32),
    /// Seconds the demo window stays open before closing again.
    pub hold_secs: f32,
    /// Whether to stand the preview on a photograph of the user's desktop.
    ///
    /// Off means the flat `background` colour instead. It is a choice worth
    /// leaving open: the photograph is the whole screen, windows and all,
    /// because there is no way to capture the wallpaper layer on its own, and
    /// whether that reads as useful context or as clutter depends on the
    /// desktop and on the shader.
    pub desktop_backdrop: bool,
    /// The colour the compositor clears to, as `0xRRGGBB`.
    ///
    /// The pane's own background, so the preview reads as part of the window
    /// rather than as a picture of somewhere else. It comes from the UI and not
    /// from the theme in the session, because under the `system` theme the
    /// engine's palette is only a stand-in for the one Qt actually draws with.
    pub background: u32,
}

impl Request {
    /// The tag slots this shader should be previewed through.
    pub fn slots(&self) -> Vec<TagSlot> {
        config::slots_for(self.is_animation, self.is_motion_driven)
    }
}

/// A running preview, or the absence of one.
#[derive(Default)]
pub struct Preview {
    running: Option<Running>,
}

struct Running {
    compositor: Child,
    backdrop: Option<Child>,
    signature: String,
    socket: String,
    shader: PathBuf,
    /// What was last written to `shader`, so an unchanged source is not
    /// written again.
    source: String,
    stop: Arc<AtomicBool>,
    demo: Option<JoinHandle<()>>,
    /// Two frame files, written alternately.
    ///
    /// The UI loads these asynchronously, so writing one file over and over
    /// means the loader can be half-way through the frame that the next
    /// capture is truncating. A torn PNG fails to load and the pane blinks.
    /// Alternating gives every reader a whole file to itself.
    frames: [PathBuf; 2],
    next_frame: usize,
}

/// What the UI needs to know about the preview.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    /// Whether an instance is up.
    pub running: bool,
    /// The shader being previewed, if any.
    pub shader: Option<String>,
}

impl Preview {
    /// Nothing running.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a preview instance is up.
    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// What the UI should show about it.
    pub fn status(&self) -> Status {
        match &self.running {
            Some(r) => {
                Status { running: true, shader: Some(r.shader.to_string_lossy().into_owned()) }
            }
            None => Status::default(),
        }
    }

    /// Start a preview, replacing any that is already up.
    ///
    /// Takes a few seconds: a compositor has to come up and report itself.
    /// Callers with a UI thread should run this off it.
    pub fn start(&mut self, request: &Request) -> Result<()> {
        self.stop();

        if !hyprctl::is_running() {
            return Err(Error::other(
                "the preview runs a second Hyprland inside this session, so Hyprland has to be \
                 running first",
            ));
        }
        if !request.plugin_so.exists() {
            return Err(Error::other(format!(
                "{} is not there — install the plugin from the Plugin page first",
                paths::contract(&request.plugin_so)
            )));
        }

        let dir = config::dir();
        std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;

        let shader = config::shader_path(&request.original);
        paths::write_atomic(&shader, &request.source)?;

        // What the demo window will have in it. Written before the compositor
        // is up, so the first window already has something to show.
        let card = card::path(&dir);
        let (cols, rows) = card::geometry(request.size);
        paths::write_atomic(&card, &card::render(cols, rows))?;

        // Before anything is started. If this has to fall back to
        // photographing the desktop, the preview's own compositor never puts
        // a window on the screen any more, but the app's toast and its own
        // window are on it, and a photograph taken while a start is in flight
        // catches whatever the click brought up.
        let backdrop = request.desktop_backdrop.then(|| wallpaper::find_image(&dir)).flatten();

        let mut look = Look::from_host();
        look.prune_curves();

        let cfg = PreviewConfig {
            shader: shader.to_string_lossy().into_owned(),
            slots: request.slots(),
            demo_class: ".*".into(),
            plugin_so: request.plugin_so.to_string_lossy().into_owned(),
            background: request.background,
            size: (request.size.0.max(160), request.size.1.max(120)),
            app_pid: std::process::id(),
            look,
        };

        let config_path = dir.join("preview.lua");
        paths::write_atomic(&config_path, &config::render(&cfg))?;

        // Not `unwrap_or_default()`: an empty list here would make the first
        // instance found afterwards look new, and the first instance is the
        // user's own session. Everything downstream is then aimed at their
        // real desktop — `output create headless` would add a phantom monitor
        // to it. A preview that refuses to start is cheaper by a mile.
        let before = hyprctl::instances()?;
        let compositor = spawn_compositor(&config_path, &dir)?;
        let (signature, socket) = match wait_for_instance(&before) {
            Ok(found) => found,
            Err(e) => {
                let mut compositor = compositor;
                let _ = compositor.kill();
                return Err(e);
            }
        };

        let stop = Arc::new(AtomicBool::new(false));
        let mut running = Running {
            compositor,
            backdrop: None,
            signature,
            socket,
            shader,
            source: request.source.clone(),
            stop: Arc::clone(&stop),
            demo: None,
            frames: [dir.join("frame-a.png"), dir.join("frame-b.png")],
            next_frame: 0,
        };

        if let Err(e) = go_headless(&running.signature) {
            running.shut_down();
            return Err(e);
        }

        // Only now: a layer surface needs an output to bind to, and until this
        // point the only one was about to be taken away.
        running.backdrop = backdrop
            .and_then(|image| wallpaper::prepare(&image))
            .and_then(|(program, args)| spawn_on_socket(&running.socket, program, args));

        running.demo = Some(spawn_demo_loop(running.socket.clone(), card, request.hold_secs, stop));
        self.running = Some(running);
        Ok(())
    }

    /// Rewrite the shader the preview is rendering.
    ///
    /// This is the whole live-update mechanism: the plugin reloads a shader when
    /// its mtime changes, so writing the file is all it takes for the next frame
    /// to show the new values. No restart, and nothing to tell the compositor.
    ///
    /// Which is also why an unchanged source must not be written. The UI calls
    /// this whenever anything at all changes in the application state — and
    /// that includes the compositor probe that runs every five seconds, on its
    /// own, forever. Writing every time would touch the mtime every time, and
    /// the plugin would dutifully recompile a shader that had not changed,
    /// for as long as the preview was open.
    pub fn set_source(&mut self, source: &str) -> Result<()> {
        let Some(running) = &mut self.running else {
            return Ok(());
        };
        if running.source == source {
            return Ok(());
        }
        paths::write_atomic(&running.shader, source)?;
        running.source = source.to_string();
        Ok(())
    }

    /// Grab the current frame, returning the file it was written to.
    pub fn capture(&mut self) -> Result<PathBuf> {
        let Some(running) = &mut self.running else {
            return Err(Error::other("no preview is running"));
        };

        let frame = running.frames[running.next_frame].clone();
        running.next_frame = 1 - running.next_frame;

        let out = Command::new("grim")
            .args(["-o", OUTPUT, "-l", "0"])
            .arg(&frame)
            .env("WAYLAND_DISPLAY", &running.socket)
            .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
            .output()
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => Error::other(
                    "grim is not installed, and the preview needs it to read frames out of the \
                     nested compositor",
                ),
                _ => Error::other(e.to_string()),
            })?;

        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(Error::other(if err.is_empty() {
                "grim could not capture the preview".into()
            } else {
                err
            }));
        }
        Ok(frame)
    }

    /// Stop the preview and clean up after it.
    pub fn stop(&mut self) {
        if let Some(mut running) = self.running.take() {
            running.shut_down();
        }
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        // A preview that outlives the app would be a compositor nobody can see
        // and nobody can stop.
        self.stop();
    }
}

impl Running {
    fn shut_down(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.demo.take() {
            let _ = handle.join();
        }
        if let Some(mut backdrop) = self.backdrop.take() {
            let _ = backdrop.kill();
            let _ = backdrop.wait();
        }
        let _ = self.compositor.kill();
        let _ = self.compositor.wait();

        // Hyprland does not tidy its own runtime directory, gracefully or
        // otherwise — an `hl.dsp.exit()` leaves it behind exactly as a kill
        // does — so a preview that has been opened and closed a few dozen
        // times leaves a few dozen directories in $XDG_RUNTIME_DIR. This one
        // was made by an instance we started and have just reaped, so it is
        // ours to remove.
        if !self.signature.is_empty() {
            let dir = std::env::var_os("XDG_RUNTIME_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join("hypr")
                .join(&self.signature);
            let _ = std::fs::remove_dir_all(dir);
        }
        // The shader copy, the generated config and the compositor's log are
        // left in the runtime directory until the next preview overwrites
        // them: when one fails to start, they are the evidence of why.
    }
}

/// Start the nested compositor.
fn spawn_compositor(config: &Path, dir: &Path) -> Result<Child> {
    let log = std::fs::File::create(dir.join("hyprland.log")).ok();

    Command::new("Hyprland")
        .arg("-c")
        .arg(config)
        // Without this the child reads the host's signature and talks to the
        // wrong instance the moment anything in it shells out to hyprctl.
        .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
        .env_remove("HYPRLAND_CMD")
        .stdin(Stdio::null())
        .stdout(log.as_ref().and_then(|f| f.try_clone().ok()).map_or(Stdio::null(), Stdio::from))
        .stderr(log.map_or(Stdio::null(), Stdio::from))
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                Error::other("Hyprland is not on PATH, so there is nothing to run the preview in")
            }
            _ => Error::other(format!("could not start the preview compositor: {e}")),
        })
}

/// Wait for an instance that was not there before, and return it.
fn wait_for_instance(before: &[hyprctl::Instance]) -> Result<(String, String)> {
    let deadline = Instant::now() + START_TIMEOUT;
    while Instant::now() < deadline {
        if let Ok(now) = hyprctl::instances() {
            if let Some(new) = now.iter().find(|i| !before.iter().any(|b| b.instance == i.instance))
            {
                return Ok((new.instance.clone(), new.wl_socket.clone()));
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err(Error::other(
        "the preview compositor did not come up — see preview/hyprland.log in the runtime \
         directory",
    ))
}

/// Give the instance the headless output it draws on.
///
/// It is sized by the monitor rule in the generated config, because a
/// `hyprctl keyword monitor` aimed at the headless output after the fact is
/// accepted and then ignored — which leaves the preview rendering at the
/// default 1920x1080 with the window still laid out for something else.
///
/// The instance's own backend output is disabled by that config, so there is
/// no window on the user's screen to take away. Removing it anyway is a
/// leftover with one job: if a future Hyprland were to open the window despite
/// the rule, this still closes it a moment later rather than leaving it up for
/// the length of the preview. It is allowed to fail — an output that was never
/// enabled is one Hyprland may refuse to remove, and that refusal means the
/// rule did its work.
fn go_headless(signature: &str) -> Result<()> {
    hyprctl::on_instance(signature, &["output", "create", "headless"])?;
    std::thread::sleep(Duration::from_millis(300));
    let _ = hyprctl::on_instance(signature, &["output", "remove", "WAYLAND-1"]);
    std::thread::sleep(Duration::from_millis(400));
    Ok(())
}

/// Open a window, hold it, close it, repeat.
///
/// The loop is the point, not decoration: an open or close shader exists only
/// during the transition, so a preview that shows a window already open shows
/// nothing at all of it. A steady shader is served just as well.
///
/// Every window in the loop shows the same test card; see [`card`].
fn spawn_demo_loop(
    socket: String,
    card: PathBuf,
    hold_secs: f32,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    let hold = Duration::from_secs_f32(hold_secs.clamp(1.0, 60.0));
    // The still half of the cycle is split around the nudge, so the steady
    // state is seen before the window is disturbed and again after it settles.
    let still = hold.mul_f32(0.4);
    let moving = hold.mul_f32(0.2);

    std::thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            // Each client is given a lifetime and closes itself when it runs
            // out. That is not a detail: a window the user closes is how a
            // close animation — and any close shader, which is the whole
            // reason the cycle exists — actually happens. Signalling the
            // terminal instead would be a kill, because that is all
            // `Child::kill` can send, and a killed client is a surface that
            // vanishes rather than a window that closes.
            let lifetime = still + moving + still;
            let Some(mut client) = spawn_demo_client(&socket, &card, lifetime) else {
                // No terminal to demo with. Sleeping rather than spinning keeps
                // this from becoming a fork bomb on a bare system.
                sleep_unless_stopped(Duration::from_secs(5), &stop);
                continue;
            };

            // Open, then hold: the open animation, then the steady state.
            sleep_unless_stopped(still, &stop);

            // Then make it move. A second window is the whole trick: tiling it
            // shoves the first one aside, which is a real move and a real
            // resize with real velocity behind them — the only thing that makes
            // a wobble shader show anything at all.
            let mut neighbour = (!stop.load(Ordering::SeqCst))
                .then(|| spawn_demo_client(&socket, &card, moving))
                .flatten();
            sleep_unless_stopped(moving, &stop);
            reap(neighbour.take());

            // The first window is still up, settling back.
            sleep_unless_stopped(still, &stop);

            if stop.load(Ordering::SeqCst) {
                // Going away: no time left for a graceful close.
                reap(Some(client));
                return;
            }

            // Otherwise it closes by itself, on time. Wait for that rather
            // than pre-empting it, so the close is the client's own.
            let _ = client.wait();

            // Long enough for the close animation to finish before the next
            // window opens over the top of it.
            sleep_unless_stopped(Duration::from_millis(900), &stop);
        }
    })
}

/// Stop a demo client that has not run out of time, and collect it.
fn reap(child: Option<Child>) {
    if let Some(mut child) = child {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn sleep_unless_stopped(total: Duration, stop: &AtomicBool) {
    let step = Duration::from_millis(100);
    let mut left = total;
    while left > Duration::ZERO && !stop.load(Ordering::SeqCst) {
        let this = step.min(left);
        std::thread::sleep(this);
        left -= this;
    }
}

/// Open the demo window on the nested compositor.
///
/// The client is pointed straight at the nested socket rather than dispatched
/// through `hyprctl`, which under a Lua config evaluates its argument as Lua
/// and so cannot simply be handed a command line.
fn spawn_demo_client(socket: &str, card: &Path, lifetime: Duration) -> Option<Child> {
    let (program, args) = demo_terminal(card, lifetime)?;
    spawn_on_socket(socket, program, args)
}

/// Start a client on the preview's compositor rather than the user's own.
///
/// `DISPLAY` goes too, or a toolkit that prefers X11 connects to the session's
/// Xwayland and the window opens on the user's actual screen.
fn spawn_on_socket(socket: &str, program: PathBuf, args: Vec<String>) -> Option<Child> {
    Command::new(program)
        .args(args)
        .env("WAYLAND_DISPLAY", socket)
        .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
        .env_remove("DISPLAY")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()
}

/// A terminal to use as the demo window, with the arguments that fill it.
fn demo_terminal(card: &Path, lifetime: Duration) -> Option<(PathBuf, Vec<String>)> {
    const KNOWN: &[(&str, &[&str])] = &[
        ("kitty", &["-o", "font_size=12", "-o", "cursor_blink_interval=0", "-e"]),
        ("foot", &["-e"]),
        ("alacritty", &["-e"]),
        ("ghostty", &["-e"]),
        ("wezterm", &["start", "--"]),
        ("xterm", &["-e"]),
    ];

    // The test card, and then a `sleep` that runs out — which is how the
    // window comes to close itself. See [`card`] for what is in it and why a
    // file is `cat`ed rather than the whole thing being spelled out here.
    let content = format!("cat {}; exec sleep {:.2}", sh_quote(card), lifetime.as_secs_f32());

    KNOWN.iter().find_map(|(name, prefix)| {
        paths::which(name).map(|path| {
            let mut args: Vec<String> = prefix.iter().map(|s| s.to_string()).collect();
            args.push("/bin/sh".to_string());
            args.push("-c".to_string());
            args.push(content.clone());
            (path, args)
        })
    })
}

/// Quote a path for `/bin/sh`.
fn sh_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> Request {
        Request {
            original: PathBuf::from("/home/me/.config/hypr/shaders/dim.glsl"),
            source: "const float DIM = 0.5;\n".into(),
            is_animation: false,
            is_motion_driven: false,
            plugin_so: PathBuf::from("/nowhere/HyprWindowShade.so"),
            size: (640, 400),
            hold_secs: 10.0,
            desktop_backdrop: false,
            background: 0x1e1e2e,
        }
    }

    #[test]
    fn a_steady_shader_previews_on_the_plain_tag() {
        assert_eq!(request().slots(), vec![TagSlot::Shader]);
    }

    #[test]
    fn an_animation_previews_on_both_ends_of_the_cycle() {
        let mut r = request();
        r.is_animation = true;
        assert_eq!(r.slots(), vec![TagSlot::Open, TagSlot::Close]);
    }

    #[test]
    fn a_motion_shader_previews_on_move_and_resize() {
        let mut r = request();
        r.is_motion_driven = true;
        assert_eq!(r.slots(), vec![TagSlot::Move, TagSlot::Resize]);
    }

    #[test]
    fn a_missing_plugin_is_refused_before_anything_is_started() {
        let mut p = Preview::new();
        let err = p.start(&request()).unwrap_err().to_string();
        // Either complaint is correct, and which one depends on whether the
        // machine running the tests has a Hyprland session at all.
        assert!(
            err.contains("HyprWindowShade.so") || err.contains("Hyprland has to be running"),
            "{err}"
        );
        assert!(!p.is_running());
    }

    #[test]
    fn setting_the_source_without_a_preview_is_a_no_op() {
        let mut p = Preview::new();
        assert!(p.set_source("const float DIM = 1.0;\n").is_ok());
    }

    #[test]
    fn capturing_without_a_preview_says_so() {
        let mut p = Preview::new();
        assert!(p.capture().is_err());
    }

    #[test]
    fn the_demo_window_shows_the_card_and_then_runs_out() {
        let Some((_, args)) =
            demo_terminal(Path::new("/run/preview/testcard.ans"), Duration::from_secs(6))
        else {
            // No terminal installed, so there is nothing to assert about.
            return;
        };
        let command = args.last().expect("a shell command");
        assert!(command.contains("cat '/run/preview/testcard.ans'"), "{command}");
        // `exec`, so the shell is replaced and the window closes when the
        // sleep runs out rather than a moment after it.
        assert!(command.ends_with("exec sleep 6.00"), "{command}");
    }

    #[test]
    fn an_awkward_path_survives_the_shell() {
        assert_eq!(sh_quote(Path::new("/tmp/it's here.ans")), r"'/tmp/it'\''s here.ans'");
    }

    #[test]
    fn an_idle_preview_reports_nothing_running() {
        assert_eq!(Preview::new().status(), Status { running: false, shader: None });
    }
}
