//! The only bridge between QML and the engine.
//!
//! QML sees one singleton, `Backend`, with:
//!
//! * `stateJson` — the whole application state, re-emitted after every change.
//! * `run(command, payloadJson)` — perform a change.
//! * `query(what, payloadJson)` — read something that is not part of the state,
//!   such as the generated Lua preview.
//! * `notify(message, isError)` — a signal for the toast.
//! * a small group of `hyprpm*` members, which are the one thing here that is
//!   not part of the configuration: a long-running external command, its
//!   output, and the password it asks for.
//!
//! Keeping the surface this small means the UI holds no configuration logic,
//! and the engine stays testable without Qt.

/// The CXX-Qt bridge definition.
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// An alias to the QString type.
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// The singleton QML talks to.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, state_json, cxx_name = "stateJson")]
        #[qproperty(bool, ready)]
        #[qproperty(QString, shot_dir, cxx_name = "shotDir")]
        #[qproperty(bool, hyprpm_busy, cxx_name = "hyprpmBusy")]
        #[qproperty(QString, hyprpm_label, cxx_name = "hyprpmLabel")]
        #[qproperty(bool, preview_busy, cxx_name = "previewBusy")]
        #[qproperty(bool, preview_running, cxx_name = "previewRunning")]
        #[qproperty(QString, preview_shader, cxx_name = "previewShader")]
        type Backend = super::BackendRust;
    }

    extern "RustQt" {
        /// Emitted after a command, for the status toast.
        #[qsignal]
        #[cxx_name = "notify"]
        fn notify(self: Pin<&mut Backend>, message: &QString, is_error: bool);

        /// One line of output from the running hyprpm operation.
        ///
        /// `transient` marks a progress bar redrawn in place: it should
        /// replace the line before it rather than pile up under it.
        #[qsignal]
        #[cxx_name = "hyprpmOutput"]
        fn hyprpm_output(self: Pin<&mut Backend>, line: &QString, transient: bool);

        /// An operation has started; the argument is what to call it.
        #[qsignal]
        #[cxx_name = "hyprpmStarted"]
        fn hyprpm_started(self: Pin<&mut Backend>, label: &QString);

        /// An operation has ended.
        #[qsignal]
        #[cxx_name = "hyprpmFinished"]
        fn hyprpm_finished(self: Pin<&mut Backend>, ok: bool, message: &QString);

        /// sudo is asking for the password. `retry` means the last one was wrong.
        #[qsignal]
        #[cxx_name = "hyprpmPasswordRequested"]
        fn hyprpm_password_requested(self: Pin<&mut Backend>, prompt: &QString, retry: bool);
    }

    extern "RustQt" {
        /// Run a command. `payload` is a JSON object, or empty for none.
        #[qinvokable]
        #[cxx_name = "run"]
        fn run(self: Pin<&mut Backend>, command: &QString, payload: &QString);

        /// Read something that is not part of `stateJson`.
        ///
        /// `what` is one of `preview`, `importPreview`, `shaderSource`,
        /// `state`. Returns JSON, or plain text for `preview` and
        /// `shaderSource`. An error comes back as `{"error": "..."}`.
        #[qinvokable]
        #[cxx_name = "query"]
        fn query(self: &Backend, what: &QString, payload: &QString) -> QString;

        /// Re-read the shader directory and the live compositor state.
        #[qinvokable]
        #[cxx_name = "refresh"]
        fn refresh(self: Pin<&mut Backend>);

        #[qinvokable]
        #[cxx_name = "lookTry"]
        fn look_try(self: &Backend, key: &QString, value: &QString) -> QString;

        /// Ask the compositor what is on screen, on a background thread.
        ///
        /// Returns immediately; `stateJson` updates when the answer arrives.
        #[qinvokable]
        #[cxx_name = "refreshLive"]
        fn refresh_live(self: Pin<&mut Backend>);

        /// Start a hyprpm operation: `add`, `remove`, `enable`, `disable`,
        /// `update` or `reload`. `argument` is the URL or plugin name the
        /// first four need, and is ignored by the others.
        ///
        /// Returns at once. Everything it does arrives as `hyprpmOutput`,
        /// `hyprpmPasswordRequested` and `hyprpmFinished`.
        #[qinvokable]
        #[cxx_name = "hyprpmRun"]
        fn hyprpm_run(self: Pin<&mut Backend>, op: &QString, argument: &QString);

        /// Answer the password prompt.
        #[qinvokable]
        #[cxx_name = "hyprpmAnswerPassword"]
        fn hyprpm_answer_password(self: Pin<&mut Backend>, password: &QString);

        /// Refuse the password prompt, which ends the operation.
        #[qinvokable]
        #[cxx_name = "hyprpmCancelPassword"]
        fn hyprpm_cancel_password(self: Pin<&mut Backend>);

        /// Stop the running operation.
        #[qinvokable]
        #[cxx_name = "hyprpmCancel"]
        fn hyprpm_cancel(self: Pin<&mut Backend>);

        /// What hyprpm has installed, as JSON.
        #[qinvokable]
        #[cxx_name = "hyprpmStatus"]
        fn hyprpm_status(self: &Backend) -> QString;

        /// Run the operation in a terminal emulator instead of in this window.
        #[qinvokable]
        #[cxx_name = "hyprpmOpenTerminal"]
        fn hyprpm_open_terminal(self: Pin<&mut Backend>, op: &QString, argument: &QString);
    }

    extern "RustQt" {
        /// Start previewing one shader at the given pane size.
        ///
        /// Returns at once: a compositor takes a few seconds to come up, so the
        /// work is on a thread and `previewRunning` says when it is ready.
        ///
        /// `background` is the pane's own color, as `#rrggbb`: the preview
        /// clears to it so the window appears to float on the app rather than
        /// on a desktop of its own. It comes from QML because under the system
        /// theme the real palette is Qt's, not the engine's.
        #[qinvokable]
        #[cxx_name = "previewStart"]
        fn preview_start(
            self: Pin<&mut Backend>,
            path: &QString,
            width: i32,
            height: i32,
            background: &QString,
            desktop_backdrop: bool,
        );

        /// Tear the preview down.
        #[qinvokable]
        #[cxx_name = "previewStop"]
        fn preview_stop(self: Pin<&mut Backend>);

        /// Push the shader's staged source into the running preview.
        ///
        /// Cheap — one file write — because the plugin notices the mtime and
        /// reloads by itself. This is what makes a slider live.
        #[qinvokable]
        #[cxx_name = "previewUpdate"]
        fn preview_update(self: Pin<&mut Backend>, path: &QString);

        /// Grab the current frame, as `{"path": ..., "n": ...}`.
        ///
        /// `n` changes every time so QML can defeat its own image cache.
        #[qinvokable]
        #[cxx_name = "previewFrame"]
        fn preview_frame(self: Pin<&mut Backend>) -> QString;
    }

    impl cxx_qt::Initialize for Backend {}

    // Lets `qt_thread()` hand a background thread a way back onto the Qt event
    // loop, which is the only thread allowed to touch the session or the
    // properties QML is bound to.
    impl cxx_qt::Threading for Backend {}
}

use core::pin::Pin;
use std::sync::{Arc, Mutex};

use custard_core::hyprpm;
use custard_core::preview::Preview;
use custard_core::Session;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use serde_json::{json, Value};

/// A `#rrggbb` from QML as a plain 24-bit number.
///
/// Anything unreadable falls back to a dark gray, which is wrong but visible —
/// the alternative is refusing to preview over a color.
fn parse_hex_rgb(text: &str) -> u32 {
    u32::from_str_radix(text.trim().trim_start_matches('#'), 16)
        .map(|v| v & 0x00ff_ffff)
        .unwrap_or(0x1e_1e1e)
}

/// How long the demo window stays open before closing again.
///
/// Long enough to look at the steady state, short enough that the open and
/// close — which is all an animation shader ever is — come round again soon.
const HOLD_SECS: f32 = 10.0;

/// The Rust side of the singleton.
#[derive(Default)]
pub struct BackendRust {
    state_json: QString,
    /// Set from CUSTARD_SHOT_DIR. When non-empty the UI renders each page to a PNG
    /// there and quits — a development aid for checking layout without a
    /// compositor, and the only thing in this file that is not about the config.
    shot_dir: QString,
    ready: bool,
    session: Option<Session>,
    /// True while a background probe is out.
    ///
    /// The timer fires every five seconds; if `hyprctl` ever takes longer than
    /// that, threads would pile up behind it.
    probing: bool,
    /// True while a hyprpm operation is running.
    hyprpm_busy: bool,
    /// What that operation is called, for the header.
    hyprpm_label: QString,
    /// The running operation, kept so it can be answered and canceled.
    hyprpm_job: Option<hyprpm::Job>,

    /// The preview compositor, if one is up.
    ///
    /// Behind a mutex because starting one takes seconds and so happens on a
    /// thread, while frames are grabbed from the Qt thread in between.
    preview: Arc<Mutex<Preview>>,
    /// True while a preview is starting or stopping.
    preview_busy: bool,
    /// True once it is up.
    preview_running: bool,
    /// Which shader it is showing.
    preview_shader: QString,
    /// Bumped per frame, so QML sees a new URL each time.
    preview_frames: u64,
}

impl cxx_qt::Initialize for qobject::Backend {
    /// Load everything once the QObject exists.
    fn initialize(mut self: Pin<&mut Self>) {
        let shot_dir = std::env::var("CUSTARD_SHOT_DIR").unwrap_or_default();
        self.as_mut().set_shot_dir(QString::from(&shot_dir));

        let session = Session::load();
        let problems = session.messages();
        self.as_mut().rust_mut().session = Some(session);
        self.as_mut().push_state();
        self.as_mut().set_ready(true);

        for problem in problems {
            let msg = QString::from(&problem);
            self.as_mut().notify(&msg, true);
        }
    }
}

impl qobject::Backend {
    /// Run a command and push the new state.
    pub fn run(mut self: Pin<&mut Self>, command: &QString, payload: &QString) {
        let command = command.to_string();
        let payload = parse_payload(&payload.to_string());

        let outcome = match self.as_mut().rust_mut().session.as_mut() {
            Some(session) => session.command(&command, &payload),
            None => Err(custard_core::Error::other("the backend is still starting up")),
        };

        self.as_mut().push_state();

        match outcome {
            Ok(Some(message)) => {
                let m = QString::from(&message);
                self.as_mut().notify(&m, false);
            }
            Ok(None) => {}
            Err(e) => {
                let m = QString::from(&e.to_string());
                self.as_mut().notify(&m, true);
            }
        }
    }

    /// Answer a read-only question.
    pub fn query(&self, what: &QString, payload: &QString) -> QString {
        let Some(session) = self.rust().session.as_ref() else {
            return QString::from("{}");
        };
        let what = what.to_string();
        let payload = parse_payload(&payload.to_string());

        let answer: Result<String, String> = match what.as_str() {
            "state" => Ok(session.state().to_string()),
            "preview" => session.preview().map_err(|e| e.to_string()),
            "presetConflicts" => session
                .preset_conflicts(&payload)
                .map(|v| json!(v).to_string())
                .map_err(|e| e.to_string()),
            "importPreview" => {
                session.import_preview().map(|v| v.to_string()).map_err(|e| e.to_string())
            }
            "shaderSource" => payload
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "no shader path given".to_string())
                .and_then(|p| {
                    custard_core::paths::read_to_string(&custard_core::paths::expand(p))
                        .map_err(|e| e.to_string())
                }),
            other => Err(format!("unknown query `{other}`")),
        };

        match answer {
            Ok(text) => QString::from(&text),
            Err(e) => QString::from(&json!({ "error": e }).to_string()),
        }
    }

    /// Show a look value on the desktop while a control is still moving.
    ///
    /// Stores nothing and pushes no state — it runs for every step of a drag,
    /// and the whole state document is far too much to rebuild that often.
    /// Returns an empty string, or the reason it did not take.
    pub fn look_try(&self, key: &QString, value: &QString) -> QString {
        let Some(session) = self.rust().session.as_ref() else {
            return QString::from("");
        };
        let value: Value = serde_json::from_str(&value.to_string()).unwrap_or(Value::Null);
        match session.try_look(&key.to_string(), &value) {
            Ok(()) => QString::from(""),
            Err(e) => QString::from(&e.to_string()),
        }
    }

    /// Re-read the shader directory and the compositor state.
    ///
    /// The directory scan is local and quick, so it happens here; the
    /// compositor probe is not, so it goes to a thread.
    pub fn refresh(mut self: Pin<&mut Self>) {
        if let Some(session) = self.as_mut().rust_mut().session.as_mut() {
            session.rescan_shaders();
        }
        self.as_mut().push_state();
        self.refresh_live();
    }

    /// Probe the compositor on a background thread.
    pub fn refresh_live(mut self: Pin<&mut Self>) {
        if self.as_ref().rust().probing {
            return;
        }
        self.as_mut().rust_mut().probing = true;

        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let live = custard_core::session::probe_live();
            // The queue fails only if the QObject is already gone, which on
            // the way out is exactly the right time to drop the answer.
            let _ = thread.queue(move |mut backend| {
                backend.as_mut().rust_mut().probing = false;
                if let Some(session) = backend.as_mut().rust_mut().session.as_mut() {
                    session.set_live(live);
                }
                backend.push_state();
            });
        });
    }

    // -----------------------------------------------------------------------
    // Preview
    //
    // A second Hyprland with the plugin loaded in it. Like hyprpm this is a
    // child process rather than a Session command, but unlike hyprpm it is
    // long-lived and answers questions while it runs, so the handle lives here
    // behind a mutex.
    // -----------------------------------------------------------------------

    /// Start previewing a shader.
    pub fn preview_start(
        mut self: Pin<&mut Self>,
        path: &QString,
        width: i32,
        height: i32,
        background: &QString,
        desktop_backdrop: bool,
    ) {
        if self.as_ref().rust().preview_busy {
            return;
        }

        let path = path.to_string();
        let size = (width.max(1) as u32, height.max(1) as u32);
        let background = parse_hex_rgb(&background.to_string());

        // Built here, on the Qt thread, because only the session knows what is
        // staged — and the session must not be touched from the worker.
        let request = match self.as_mut().rust_mut().session.as_ref() {
            Some(session) => {
                session.preview_request(&path, size, HOLD_SECS, background, desktop_backdrop)
            }
            None => Err(custard_core::Error::other("the backend is still starting up")),
        };
        let request = match request {
            Ok(request) => request,
            Err(e) => {
                let m = QString::from(&e.to_string());
                self.as_mut().notify(&m, true);
                return;
            }
        };

        self.as_mut().set_preview_busy(true);
        let preview = Arc::clone(&self.as_ref().rust().preview);
        let thread = self.qt_thread();

        std::thread::spawn(move || {
            let outcome = preview
                .lock()
                .map_err(|_| "the preview is wedged".to_string())
                .and_then(|mut preview| preview.start(&request).map_err(|e| e.to_string()));

            let _ = thread.queue(move |mut backend| {
                backend.as_mut().set_preview_busy(false);
                match outcome {
                    Ok(()) => {
                        backend.as_mut().set_preview_running(true);
                        backend.as_mut().set_preview_shader(QString::from(&path));
                    }
                    Err(e) => {
                        backend.as_mut().set_preview_running(false);
                        backend.as_mut().set_preview_shader(QString::from(""));
                        let m = QString::from(&e);
                        backend.as_mut().notify(&m, true);
                    }
                }
            });
        });
    }

    /// Stop the preview.
    pub fn preview_stop(mut self: Pin<&mut Self>) {
        self.as_mut().set_preview_running(false);
        self.as_mut().set_preview_shader(QString::from(""));

        let preview = Arc::clone(&self.as_ref().rust().preview);
        // Killing a compositor and joining the demo loop is quick but not
        // instant, and the window must not freeze on the way out.
        std::thread::spawn(move || {
            if let Ok(mut preview) = preview.lock() {
                preview.stop();
            }
        });
    }

    /// Write the shader's staged source into the running preview.
    pub fn preview_update(mut self: Pin<&mut Self>, path: &QString) {
        if !self.as_ref().rust().preview_running {
            return;
        }
        let path = path.to_string();
        let source = match self.as_mut().rust_mut().session.as_ref() {
            Some(session) => session.shader_preview_source(&path),
            None => return,
        };
        let Ok(source) = source else {
            return;
        };

        let preview = Arc::clone(&self.as_ref().rust().preview);
        // try_lock, not lock: this runs on every slider move, and a start still
        // in progress holds the mutex for seconds. A dropped update is nothing
        // — the next one carries the same value.
        let guard = preview.try_lock();
        if let Ok(mut preview) = guard {
            let _ = preview.set_source(&source);
        }
    }

    /// Grab a frame for the pane.
    pub fn preview_frame(mut self: Pin<&mut Self>) -> QString {
        if !self.as_ref().rust().preview_running {
            return QString::from(&json!({ "error": "not running" }).to_string());
        }

        let preview = Arc::clone(&self.as_ref().rust().preview);
        let captured = match preview.try_lock() {
            Ok(mut preview) => preview.capture().map_err(|e| e.to_string()),
            // Busy starting or stopping; the timer will ask again.
            Err(_) => return QString::from(&json!({ "error": "busy" }).to_string()),
        };

        match captured {
            Ok(path) => {
                let n = self.as_ref().rust().preview_frames.wrapping_add(1);
                self.as_mut().rust_mut().preview_frames = n;
                QString::from(&json!({ "path": path.to_string_lossy(), "n": n }).to_string())
            }
            Err(e) => QString::from(&json!({ "error": e }).to_string()),
        }
    }

    // -----------------------------------------------------------------------
    // hyprpm
    //
    // The only part of the app that drives something long-running and outside
    // itself. It is deliberately not a `Session` command: it takes minutes,
    // produces output as it goes, and stops halfway to ask for a password.
    // -----------------------------------------------------------------------

    /// Start a hyprpm operation, and report it back through the signals.
    pub fn hyprpm_run(mut self: Pin<&mut Self>, op: &QString, argument: &QString) {
        if self.as_ref().rust().hyprpm_busy {
            let m = QString::from("hyprpm is already busy — wait for it, or cancel it");
            self.as_mut().notify(&m, true);
            return;
        }

        let op = match hyprpm::Op::parse(&op.to_string(), &argument.to_string()) {
            Ok(op) => op,
            Err(e) => {
                let m = QString::from(&e.to_string());
                self.as_mut().notify(&m, true);
                return;
            }
        };

        // sudo runs this executable again, with --askpass, to ask the window
        // for the password.
        let askpass = match std::env::current_exe() {
            Ok(path) => path,
            Err(e) => {
                let m = QString::from(&format!("could not find this program on disk: {e}"));
                self.as_mut().notify(&m, true);
                return;
            }
        };

        let label = op.label();
        let thread = self.as_mut().qt_thread();
        let started = hyprpm::Job::start(op, &askpass, move |event| {
            // Fails only once the QObject is gone, and an operation whose
            // window has closed has nowhere to report to anyway.
            let _ = thread.queue(move |backend| deliver(backend, event));
        });

        match started {
            Ok(job) => {
                self.as_mut().rust_mut().hyprpm_job = Some(job);
                self.as_mut().set_hyprpm_busy(true);
                self.as_mut().set_hyprpm_label(QString::from(&label));
                let l = QString::from(&label);
                self.as_mut().hyprpm_started(&l);
            }
            Err(e) => {
                let m = QString::from(&e.to_string());
                self.as_mut().notify(&m, true);
            }
        }
    }

    /// Hand the running operation the password sudo asked for.
    pub fn hyprpm_answer_password(self: Pin<&mut Self>, password: &QString) {
        if let Some(job) = self.rust().hyprpm_job.as_ref() {
            job.answer_password(&password.to_string());
        }
    }

    /// Tell sudo there is no password, which ends the operation.
    pub fn hyprpm_cancel_password(self: Pin<&mut Self>) {
        if let Some(job) = self.rust().hyprpm_job.as_ref() {
            job.deny_password();
        }
    }

    /// Stop the running operation.
    pub fn hyprpm_cancel(self: Pin<&mut Self>) {
        if let Some(job) = self.rust().hyprpm_job.as_ref() {
            job.cancel();
        }
    }

    /// What hyprpm has installed.
    pub fn hyprpm_status(&self) -> QString {
        QString::from(&hyprpm::status().to_string())
    }

    /// Run the operation in a terminal emulator instead.
    pub fn hyprpm_open_terminal(mut self: Pin<&mut Self>, op: &QString, argument: &QString) {
        let op = match hyprpm::Op::parse(&op.to_string(), &argument.to_string()) {
            Ok(op) => op,
            Err(e) => {
                let m = QString::from(&e.to_string());
                self.as_mut().notify(&m, true);
                return;
            }
        };
        let message = match hyprpm::open_in_terminal(&op) {
            Ok(()) => (format!("Running `hyprpm {}` in a terminal", op.args().join(" ")), false),
            Err(e) => (e.to_string(), true),
        };
        let m = QString::from(&message.0);
        self.as_mut().notify(&m, message.1);
    }

    /// Serialize the session and hand it to QML.
    fn push_state(mut self: Pin<&mut Self>) {
        let json = match self.as_ref().rust().session.as_ref() {
            Some(session) => session.state().to_string(),
            None => "{}".to_string(),
        };
        self.as_mut().set_state_json(QString::from(&json));
    }
}

/// Turn a payload string into a JSON object, tolerating an empty string.
fn parse_payload(text: &str) -> Value {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return json!({});
    }
    serde_json::from_str(trimmed).unwrap_or_else(|_| json!({}))
}

/// Turn one thing that happened in a hyprpm job into signals, on the Qt thread.
fn deliver(mut backend: Pin<&mut qobject::Backend>, event: hyprpm::Event) {
    match event {
        hyprpm::Event::Line { text, transient } => {
            let line = QString::from(&text);
            backend.as_mut().hyprpm_output(&line, transient);
        }
        hyprpm::Event::Password { prompt, retry } => {
            let prompt = QString::from(&prompt);
            backend.as_mut().hyprpm_password_requested(&prompt, retry);
        }
        hyprpm::Event::Finished { ok, message } => {
            backend.as_mut().rust_mut().hyprpm_job = None;
            backend.as_mut().set_hyprpm_busy(false);
            backend.as_mut().set_hyprpm_label(QString::from(""));

            let text = QString::from(&message);
            backend.as_mut().hyprpm_finished(ok, &text);
            backend.as_mut().notify(&text, !ok);

            // Installing, enabling or reloading changes what the compositor
            // has loaded, which the header badge is showing.
            backend.refresh_live();
        }
    }
}
