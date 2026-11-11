//! Driving `hyprpm`, Hyprland's plugin manager, from a window.
//!
//! `hyprpm` refuses to run as root — it keeps its state per user and would
//! leave root-owned files behind — and escalates by itself, with `sudo`, for
//! the steps that write outside `$HOME`: its state store and the Hyprland
//! headers. So it is run as the user here, and the only real problem is
//! answering the password prompt from a GUI instead of a terminal.
//!
//! sudo asks `$SUDO_ASKPASS` for the password whenever it has no terminal of
//! its own to read from. So the operation is started in a session of its own
//! with `setsid`, and `SUDO_ASKPASS` points at a helper that re-runs this same
//! application with `--askpass`. The helper connects back over a unix socket
//! in the runtime directory, the window puts the prompt in front of the user,
//! and the answer goes back down the socket. The password is never written to
//! disk, never appears in a command line, and is not put in the environment.
//!
//! `SUDO_ASKPASS` is a program, not a command line: sudo runs it with the
//! prompt as its one argument and nothing else. Pointing it straight at this
//! executable would therefore start the interface again rather than answer
//! anything, so what it points at is a two-line shell script, written beside
//! the socket, that adds the flag.
//!
//! The rest is plumbing: output arrives as [`Event::Line`]s with terminal
//! escapes taken out, and the whole thing runs under one process group so that
//! [`Job::cancel`] can stop a half-finished build.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use serde::Serialize;
use serde_json::{json, Value};

use crate::error::{Error, Result};
use crate::paths::which;

// ---------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------

/// One thing the plugin page can ask `hyprpm` to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// Install a plugin repository from a git URL.
    Add(String),
    /// Remove an installed repository.
    Remove(String),
    /// Enable an installed plugin.
    Enable(String),
    /// Disable an installed plugin.
    Disable(String),
    /// Rebuild everything against the current Hyprland.
    Update,
    /// Load enabled plugins into the running compositor.
    Reload,
}

impl Op {
    /// Build an operation from what the UI sent.
    ///
    /// The argument is typed by the user, so it is checked here rather than
    /// trusted: an empty one is a mistake, and one starting with `-` would
    /// turn into a flag for `hyprpm` instead of a name.
    pub fn parse(name: &str, argument: &str) -> Result<Op> {
        let arg = argument.trim();
        let needs = |what: &str| -> Result<String> {
            if arg.is_empty() {
                return Err(Error::other(format!("`hyprpm {name}` needs {what}")));
            }
            if arg.starts_with('-') {
                return Err(Error::other(format!("`{arg}` is not {what}")));
            }
            Ok(arg.to_string())
        };

        Ok(match name {
            "add" => Op::Add(needs("a repository URL")?),
            "remove" => Op::Remove(needs("a plugin name")?),
            "enable" => Op::Enable(needs("a plugin name")?),
            "disable" => Op::Disable(needs("a plugin name")?),
            "update" => Op::Update,
            "reload" => Op::Reload,
            other => return Err(Error::other(format!("unknown hyprpm operation `{other}`"))),
        })
    }

    /// The arguments to hand `hyprpm`.
    pub fn args(&self) -> Vec<String> {
        match self {
            Op::Add(url) => vec!["add".into(), url.clone()],
            Op::Remove(name) => vec!["remove".into(), name.clone()],
            Op::Enable(name) => vec!["enable".into(), name.clone()],
            Op::Disable(name) => vec!["disable".into(), name.clone()],
            Op::Update => vec!["update".into()],
            Op::Reload => vec!["reload".into()],
        }
    }

    /// What the window says while this is running.
    pub fn label(&self) -> String {
        match self {
            Op::Add(url) => format!("Installing {}", repo_name(url)),
            Op::Remove(name) => format!("Removing {name}"),
            Op::Enable(name) => format!("Enabling {name}"),
            Op::Disable(name) => format!("Disabling {name}"),
            Op::Update => "Updating plugins".to_string(),
            Op::Reload => "Reloading plugins".to_string(),
        }
    }
}

/// The last path segment of a git URL, without `.git`.
fn repo_name(url: &str) -> String {
    let trimmed = url.trim_end_matches('/');
    let last = trimmed.rsplit('/').next().unwrap_or(trimmed);
    last.trim_end_matches(".git").to_string()
}

// ---------------------------------------------------------------------------
// What is installed
// ---------------------------------------------------------------------------

/// One plugin, as `hyprpm list` reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Plugin {
    /// The repository it came from.
    pub repository: String,
    /// Who wrote it, when hyprpm says.
    pub author: String,
    /// The plugin's own name, which is what `enable` and `disable` take.
    pub name: String,
    /// Whether hyprpm will load it.
    pub enabled: bool,
}

/// True when `hyprpm` is installed.
pub fn available() -> bool {
    which("hyprpm").is_some()
}

/// Ask `hyprpm` what is installed.
///
/// This is the one hyprpm call that needs neither root nor a terminal, and it
/// returns in a few milliseconds, so the UI can make it synchronously.
pub fn list() -> Result<Vec<Plugin>> {
    let out = Command::new("hyprpm").arg("list").output().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => Error::other("hyprpm is not on PATH"),
        _ => Error::other(format!("hyprpm list: {e}")),
    })?;

    let text = String::from_utf8_lossy(&out.stdout);
    if !out.status.success() && text.trim().is_empty() {
        let err = strip_ansi(&String::from_utf8_lossy(&out.stderr)).trim().to_string();
        return Err(Error::other(if err.is_empty() {
            "`hyprpm list` failed".to_string()
        } else {
            err
        }));
    }
    Ok(parse_list(&strip_ansi(&text)))
}

/// Where hyprpm keeps the built plugin, if it is there.
///
/// hyprpm builds into a store of its own under `/var/cache/hyprpm/<user>/`,
/// one directory per repository, and the `.so` inside is named after the
/// plugin. The repository directory is not always named the same as the
/// plugin, so the directories are searched rather than guessed at. A manual
/// install under the Hyprland data directory is the fallback.
pub fn plugin_so(name: &str) -> Option<PathBuf> {
    let file = format!("{name}.so");

    let user = std::env::var("USER").unwrap_or_default();
    let store = PathBuf::from("/var/cache/hyprpm").join(&user);
    if let Ok(entries) = std::fs::read_dir(&store) {
        for entry in entries.filter_map(|e| e.ok()) {
            let candidate = entry.path().join(&file);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    let manual = dirs::data_dir()?.join("hyprland").join("plugins").join(&file);
    manual.is_file().then_some(manual)
}

/// Everything the plugin page needs to describe the installation, as JSON.
///
/// Best-effort like the rest of the live state: a machine without hyprpm is a
/// thing to say, not an error to raise.
pub fn status() -> Value {
    if !available() {
        return json!({ "installed": false, "plugins": [] });
    }
    match list() {
        Ok(plugins) => json!({ "installed": true, "plugins": plugins }),
        Err(e) => json!({ "installed": true, "plugins": [], "error": e.to_string() }),
    }
}

/// Pull plugins out of `hyprpm list`'s box-drawn output.
fn parse_list(text: &str) -> Vec<Plugin> {
    let repo_re = Regex::new(r"^Repository (.+?)(?: \(by (.+?)\))?:?$").unwrap();
    let plugin_re = Regex::new(r"^Plugin (.+?)$").unwrap();
    let enabled_re = Regex::new(r"^enabled:\s*(\S+)$").unwrap();

    let mut plugins = Vec::new();
    let mut repository = String::new();
    let mut author = String::new();
    let mut name = String::new();

    for raw in text.lines() {
        let line = raw.trim_matches(|c: char| c.is_whitespace() || BOX_CHARS.contains(c));
        if let Some(caps) = repo_re.captures(line) {
            repository = caps[1].trim().to_string();
            author = caps.get(2).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
            name.clear();
        } else if let Some(caps) = plugin_re.captures(line) {
            name = caps[1].trim().to_string();
        } else if let Some(caps) = enabled_re.captures(line) {
            if !name.is_empty() {
                plugins.push(Plugin {
                    repository: repository.clone(),
                    author: author.clone(),
                    name: std::mem::take(&mut name),
                    enabled: caps[1].eq_ignore_ascii_case("true"),
                });
            }
        }
    }
    plugins
}

/// The frame and bullet characters hyprpm draws its list with.
const BOX_CHARS: &str = "→·│└├┌┐─┃┏┗┣┫";

// ---------------------------------------------------------------------------
// Running an operation
// ---------------------------------------------------------------------------

/// Something that happened while an operation ran.
#[derive(Debug, Clone)]
pub enum Event {
    /// A line of output, with terminal escapes removed.
    Line {
        /// The text of the line.
        text: String,
        /// True when the line was redrawn in place with a carriage return
        /// rather than ended with a newline — a progress bar. The next line
        /// should take its place instead of piling up underneath it.
        transient: bool,
    },
    /// sudo wants the user's password.
    Password {
        /// What sudo asked, verbatim.
        prompt: String,
        /// True when the password given last time was rejected.
        retry: bool,
    },
    /// The operation ended, one way or another.
    Finished {
        /// Whether it succeeded.
        ok: bool,
        /// A sentence for the toast.
        message: String,
    },
}

/// A running `hyprpm` operation.
///
/// Dropping this does not stop anything; the operation is stopped with
/// [`Job::cancel`] and reports its own end through the sink.
pub struct Job {
    shared: Arc<Shared>,
}

/// What the UI thread and the job's own thread both touch.
struct Shared {
    socket: PathBuf,
    pidfile: PathBuf,
    /// The script `SUDO_ASKPASS` points at.
    helper: PathBuf,
    /// The askpass connection waiting for an answer, if one is waiting.
    pending: Mutex<Option<UnixStream>>,
    /// The password already accepted for this job.
    ///
    /// One operation can escalate several times — the state store, then the
    /// headers — and being asked three times for the same password in one
    /// click is not something a window should do. It is dropped when the job
    /// ends, and as soon as sudo says it was wrong.
    remembered: Mutex<Option<String>>,
    /// The pid of the direct child, in case the process group is not known yet.
    child_pid: Mutex<Option<u32>>,
    /// Whether hyprpm has printed anything since the last password was given.
    ///
    /// sudo asks for a password up to three times, and hyprpm swallows the
    /// "Sorry, try again" it prints in between — so a second prompt with no
    /// output between it and the last answer is the only sign there is that
    /// the answer was refused.
    output_since_answer: AtomicBool,
    cancelled: AtomicBool,
}

/// What the reader, socket and waiter threads send to the job's own thread.
enum Msg {
    Out(String, bool),
    Ask(UnixStream, String),
    Exited(Option<i32>),
    Eof,
}

impl Job {
    /// Start an operation. `askpass` is the program sudo should ask, which is
    /// this application's own executable.
    ///
    /// Returns as soon as the process is running; everything else arrives
    /// through `sink`, on a thread of the job's own.
    pub fn start(op: Op, askpass: &Path, sink: impl Fn(Event) + Send + 'static) -> Result<Job> {
        if !available() {
            return Err(Error::other(
                "hyprpm is not on PATH — it comes with Hyprland, as the hyprpm package on Arch",
            ));
        }

        let dir = runtime_dir()?;
        let stamp = format!("{}-{}", std::process::id(), nanos());
        let socket = dir.join(format!("askpass-{stamp}.sock"));
        let pidfile = dir.join(format!("job-{stamp}.pid"));

        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket).map_err(|e| Error::io(&socket, e))?;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| Error::io(&socket, e))?;

        // sudo runs the askpass program with the prompt and nothing else, so
        // the flag that turns this executable into the helper has to come from
        // somewhere. This is that somewhere.
        let helper = dir.join(format!("askpass-{stamp}.sh"));
        let script = format!(
            "#!/bin/sh\nexec {} --askpass \"$@\"\n",
            shell_quote(&askpass.display().to_string())
        );
        std::fs::write(&helper, script).map_err(|e| Error::io(&helper, e))?;
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| Error::io(&helper, e))?;

        let (mut command, detached) = build_command(&op, &pidfile);
        command
            .env("SUDO_ASKPASS", &helper)
            .env("CUSTARD_ASKPASS_SOCKET", &socket)
            // git would otherwise sit forever waiting for credentials that
            // nobody can type, on a private or mistyped repository URL.
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|e| {
            let _ = std::fs::remove_file(&socket);
            let _ = std::fs::remove_file(&helper);
            Error::other(format!("could not start hyprpm: {e}"))
        })?;

        let shared = Arc::new(Shared {
            socket,
            pidfile,
            helper,
            pending: Mutex::new(None),
            remembered: Mutex::new(None),
            child_pid: Mutex::new(Some(child.id())),
            output_since_answer: AtomicBool::new(true),
            cancelled: AtomicBool::new(false),
        });

        let (tx, rx) = channel::<Msg>();
        if let Some(stdout) = child.stdout.take() {
            pump(stdout, tx.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            pump(stderr, tx.clone());
        }
        {
            let tx = tx.clone();
            std::thread::spawn(move || accept_loop(listener, tx));
        }
        {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let code = child.wait().ok().and_then(|status| status.code());
                let _ = tx.send(Msg::Exited(code));
            });
        }
        drop(tx);

        if !detached {
            sink(Event::Line {
                text: "warning: setsid is not installed, so sudo may ask for the password on \
                       the terminal this app was started from rather than in this window."
                    .to_string(),
                transient: false,
            });
        }

        let job_shared = Arc::clone(&shared);
        std::thread::spawn(move || run_loop(op, job_shared, rx, sink));

        Ok(Job { shared })
    }

    /// Give sudo the password it asked for.
    ///
    /// It is kept for the rest of this operation so that a second escalation
    /// does not mean a second dialog. Rust strings cannot be wiped reliably
    /// without leaving copies behind, so this is memory-only and short-lived
    /// rather than a guarantee.
    pub fn answer_password(&self, password: &str) {
        let password = password.trim_end_matches(['\r', '\n']).to_string();
        self.shared.output_since_answer.store(false, Ordering::SeqCst);
        if let Some(stream) = self.shared.pending.lock().unwrap().take() {
            let _ = answer(&stream, &password);
        }
        *self.shared.remembered.lock().unwrap() = Some(password);
    }

    /// Refuse to give a password.
    ///
    /// Closing the connection with nothing on it is how the askpass contract
    /// says no; sudo then gives up and the operation ends with an error of its
    /// own, which is exactly what should be shown.
    pub fn deny_password(&self) {
        self.shared.pending.lock().unwrap().take();
    }

    /// Stop the operation.
    ///
    /// The whole thing runs in a session of its own, so the signal goes to the
    /// process group: `hyprpm` itself, and whichever compiler it is waiting on.
    pub fn cancel(&self) {
        self.shared.cancelled.store(true, Ordering::SeqCst);
        self.shared.pending.lock().unwrap().take();

        let target = match read_pid(&self.shared.pidfile) {
            // A negative pid means "the group with that id", which is what the
            // session leader's pid is.
            Some(group) => format!("-{group}"),
            None => match *self.shared.child_pid.lock().unwrap() {
                Some(pid) => pid.to_string(),
                None => return,
            },
        };
        let _ = Command::new("kill")
            .args(["-TERM", &target])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// The command that runs one operation.
///
/// `setsid` puts it in a session with no controlling terminal, which is what
/// makes sudo ask the askpass helper instead of trying to read from a terminal
/// that either does not exist or belongs to whoever started the app. The shell
/// in the middle is there to record the session leader's pid, which is the
/// process group to signal on cancel.
///
/// The second return value is false when `setsid` is missing.
fn build_command(op: &Op, pidfile: &Path) -> (Command, bool) {
    const SCRIPT: &str = r#"echo $$ > "$1"; shift; exec hyprpm "$@""#;

    let mut sh_args: Vec<String> =
        vec!["-c".into(), SCRIPT.into(), "hyprpm-job".into(), pidfile.display().to_string()];
    sh_args.extend(op.args());

    match which("setsid") {
        Some(setsid) => {
            let mut command = Command::new(setsid);
            command.arg("-w").arg("/bin/sh").args(&sh_args);
            (command, true)
        }
        None => {
            let mut command = Command::new("/bin/sh");
            command.args(&sh_args);
            (command, false)
        }
    }
}

/// The job's own thread: everything that happens while it runs, in order.
fn run_loop(op: Op, shared: Arc<Shared>, rx: Receiver<Msg>, sink: impl Fn(Event)) {
    let mut code: Option<Option<i32>> = None;
    let mut open_pipes = 2;
    let mut retry = false;
    let mut previous: Option<(String, bool)> = None;

    while open_pipes > 0 || code.is_none() {
        let Ok(msg) = rx.recv() else { break };
        match msg {
            Msg::Eof => open_pipes -= 1,
            Msg::Exited(status) => code = Some(status),
            Msg::Out(line, transient) => {
                // Anything at all means hyprpm moved on rather than sudo
                // asking the same thing twice.
                shared.output_since_answer.store(true, Ordering::SeqCst);
                if rejected(&line) {
                    *shared.remembered.lock().unwrap() = None;
                    retry = true;
                }
                // An erase-line escape with nothing after it is one frame of a
                // progress bar, and has nothing to show.
                if transient && line.is_empty() {
                    continue;
                }
                let repeat =
                    previous.as_ref().is_some_and(|(text, was)| *text == line && *was == transient);
                if !repeat {
                    sink(Event::Line { text: line.clone(), transient });
                    previous = Some((line, transient));
                }
            }
            Msg::Ask(stream, prompt) => {
                let mut remembered = shared.remembered.lock().unwrap();
                // Asked again, with nothing printed in between: the password
                // was refused, so it is worth nothing and the user should be
                // told rather than have their remaining attempts spent on it.
                if remembered.is_some() && !shared.output_since_answer.load(Ordering::SeqCst) {
                    *remembered = None;
                    retry = true;
                }
                let known = remembered.clone();
                drop(remembered);

                match known {
                    Some(password) => {
                        shared.output_since_answer.store(false, Ordering::SeqCst);
                        let _ = answer(&stream, &password);
                    }
                    None => {
                        *shared.pending.lock().unwrap() = Some(stream);
                        sink(Event::Password { prompt, retry });
                        retry = false;
                    }
                }
            }
        }
    }

    let cancelled = shared.cancelled.load(Ordering::SeqCst);
    let ok = !cancelled && matches!(code, Some(Some(0)));
    let message = if cancelled {
        format!("{} — cancelled", op.label())
    } else if ok {
        format!("{} — done", op.label())
    } else {
        match code {
            Some(Some(status)) => format!("{} — failed (exit code {status})", op.label()),
            _ => format!("{} — failed", op.label()),
        }
    };

    *shared.remembered.lock().unwrap() = None;
    shared.pending.lock().unwrap().take();
    let _ = std::fs::remove_file(&shared.pidfile);
    let _ = std::fs::remove_file(&shared.helper);
    // Unblocks the accept loop, which is sitting in accept() on a socket
    // nothing will connect to again.
    let _ = UnixStream::connect(&shared.socket);
    let _ = std::fs::remove_file(&shared.socket);

    sink(Event::Finished { ok, message });
}

/// Whether a line is sudo saying the password was wrong.
///
/// Only some of the time does this reach the log — hyprpm captures the output
/// of the command it escalates — which is why it is not the only check.
fn rejected(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("sorry, try again")
        || lower.contains("incorrect password")
        || lower.contains("authentication failure")
}

/// Read one pipe into lines, on a thread.
///
/// Bytes are split before they are decoded: hyprpm draws boxes, and a 4 KiB
/// read can land in the middle of one of those characters. Splitting on the
/// two ASCII line endings first means every line decodes whole.
fn pump(mut reader: impl Read + Send + 'static, tx: Sender<Msg>) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut line: Vec<u8> = Vec::new();
        // A line ended with a carriage return is held back one byte: followed
        // by a newline it is an ordinary CRLF line, and on its own it is a
        // progress bar being redrawn.
        let mut pending_return = false;

        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    for &byte in &buf[..n] {
                        if pending_return {
                            pending_return = false;
                            if byte == b'\n' {
                                let _ = tx.send(Msg::Out(finish_line(&mut line), false));
                                continue;
                            }
                            let _ = tx.send(Msg::Out(finish_line(&mut line), true));
                        }
                        match byte {
                            b'\r' => pending_return = true,
                            b'\n' => {
                                let _ = tx.send(Msg::Out(finish_line(&mut line), false));
                            }
                            _ => line.push(byte),
                        }
                    }
                }
            }
        }

        if pending_return {
            let _ = tx.send(Msg::Out(finish_line(&mut line), true));
        } else if !line.is_empty() {
            let _ = tx.send(Msg::Out(finish_line(&mut line), false));
        }
        let _ = tx.send(Msg::Eof);
    });
}

/// Decode a line's bytes, drop the escapes, and empty the buffer.
fn finish_line(bytes: &mut Vec<u8>) -> String {
    let text = String::from_utf8_lossy(bytes).into_owned();
    bytes.clear();
    strip_ansi(&text).trim_end().to_string()
}

/// Take askpass connections and hand them to the job's thread.
fn accept_loop(listener: UnixListener, tx: Sender<Msg>) {
    for connection in listener.incoming() {
        let Ok(stream) = connection else { break };
        let mut prompt = String::new();
        if BufReader::new(&stream).read_line(&mut prompt).is_err() {
            continue;
        }
        let prompt = prompt.trim().to_string();
        // A connection with nothing on it is the job saying it has ended.
        if prompt.is_empty() {
            break;
        }
        if tx.send(Msg::Ask(stream, prompt)).is_err() {
            break;
        }
    }
}

/// Send a password down an askpass connection.
fn answer(mut stream: &UnixStream, password: &str) -> std::io::Result<()> {
    writeln!(stream, "{password}")?;
    stream.flush()
}

// ---------------------------------------------------------------------------
// The askpass helper
// ---------------------------------------------------------------------------

/// The `--askpass` side of the app: ask the window for the password sudo wants.
///
/// sudo runs this with the prompt as its only argument and reads the password
/// from its standard output. Returns the exit code to leave with; anything but
/// zero tells sudo there is no password, which it treats as a refusal.
pub fn askpass(prompt: &str) -> i32 {
    let Some(socket) = std::env::var_os("CUSTARD_ASKPASS_SOCKET") else {
        eprintln!("hyprcustard: --askpass is for sudo to call, not to be run by hand");
        return 1;
    };

    let Ok(stream) = UnixStream::connect(&socket) else {
        eprintln!("hyprcustard: the window that asked for this is no longer listening");
        return 1;
    };

    // The prompt is one line on the wire, so a stray newline in it would be
    // read as the end of the message.
    let one_line = prompt.replace(['\r', '\n'], " ");
    if writeln!(&stream, "{one_line}").is_err() {
        return 1;
    }

    let mut reply = String::new();
    if BufReader::new(&stream).read_line(&mut reply).is_err() {
        return 1;
    }
    if reply.is_empty() {
        // The window closed the connection: the user pressed Cancel.
        return 1;
    }

    println!("{}", reply.trim_end_matches(['\r', '\n']));
    let _ = std::io::stdout().flush();
    0
}

// ---------------------------------------------------------------------------
// Running it in a terminal instead
// ---------------------------------------------------------------------------

/// Run an operation in a terminal emulator rather than in the app.
///
/// The escape hatch for a build that needs looking at properly, and the way
/// out on a system where the askpass route does not work — in a real terminal
/// sudo prompts the way it always has.
pub fn open_in_terminal(op: &Op) -> Result<()> {
    let mut line = String::from("hyprpm");
    for arg in op.args() {
        line.push(' ');
        line.push_str(&shell_quote(&arg));
    }
    line.push_str("; printf '\\n[finished — press enter to close]'; read _");

    let (program, prefix) = terminal()
        .ok_or_else(|| Error::other("no terminal emulator found — set $TERMINAL to one"))?;

    Command::new(&program)
        .args(prefix)
        .args(["/bin/sh", "-c", &line])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Error::other(format!("could not start {}: {e}", program.display())))?;
    Ok(())
}

/// The terminal emulator to use, and the arguments that mean "run this".
fn terminal() -> Option<(PathBuf, Vec<String>)> {
    const KNOWN: &[(&str, &[&str])] = &[
        ("kitty", &["-e"]),
        ("foot", &["-e"]),
        ("alacritty", &["-e"]),
        ("ghostty", &["-e"]),
        ("wezterm", &["start", "--"]),
        ("konsole", &["-e"]),
        ("gnome-terminal", &["--"]),
        ("xterm", &["-e"]),
    ];

    if let Some(configured) = std::env::var_os("TERMINAL") {
        let configured = PathBuf::from(configured);
        let name = configured.file_name().map(|n| n.to_string_lossy().into_owned());
        let found = if configured.is_absolute() {
            configured.is_file().then_some(configured)
        } else {
            which(&configured.to_string_lossy())
        };
        if let Some(path) = found {
            let args = name
                .and_then(|n| KNOWN.iter().find(|(k, _)| *k == n).map(|(_, a)| *a))
                .unwrap_or(&["-e"]);
            return Some((path, args.iter().map(|s| s.to_string()).collect()));
        }
    }

    KNOWN.iter().find_map(|(name, args)| {
        which(name).map(|path| (path, args.iter().map(|s| s.to_string()).collect()))
    })
}

/// Quote one argument for `/bin/sh`.
fn shell_quote(arg: &str) -> String {
    format!("'{}'", arg.replace('\'', r"'\''"))
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// A private directory for the sockets and pid files of running jobs.
fn runtime_dir() -> Result<PathBuf> {
    let base =
        std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let dir = base.join("hyprcustard");
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)
        .map_err(|e| Error::io(&dir, e))?;
    // `recursive` leaves an existing directory's mode alone, and the socket in
    // it should not be reachable by anyone else.
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| Error::io(&dir, e))?;
    Ok(dir)
}

/// Enough of a clock to tell two jobs apart.
fn nanos() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
}

/// The pid a job's shell wrote for itself, if it got that far.
fn read_pid(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Remove terminal escape sequences from text.
///
/// hyprpm colours its output whether or not it is talking to a terminal, so
/// without this the log pane shows the escapes as mojibake.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            // CSI: parameters, then one final byte in @ to ~.
            Some('[') => {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            // OSC: runs until a bell or a string terminator.
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\u{7}' {
                        break;
                    }
                    if c == '\u{1b}' {
                        chars.next();
                        break;
                    }
                }
            }
            // Anything else is a two-character sequence, already consumed.
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_are_removed() {
        assert_eq!(strip_ansi("\u{1b}[0m\u{1b}[32mgreen\u{1b}[0m"), "green");
        assert_eq!(strip_ansi("\u{1b}]0;title\u{7}plain"), "plain");
        assert_eq!(strip_ansi("no escapes here"), "no escapes here");
    }

    #[test]
    fn a_plugin_list_is_understood() {
        let text = "\
→ Repository HyprWindowShade (by ManofJELLO):
  │ Plugin HyprWindowShade
  └─ enabled: true
→ Repository other (by someone):
  │ Plugin borderspp
  └─ enabled: false
";
        let plugins = parse_list(text);
        assert_eq!(plugins.len(), 2);
        assert_eq!(plugins[0].name, "HyprWindowShade");
        assert_eq!(plugins[0].author, "ManofJELLO");
        assert!(plugins[0].enabled);
        assert_eq!(plugins[1].name, "borderspp");
        assert!(!plugins[1].enabled);
    }

    #[test]
    fn a_repository_without_an_author_still_parses() {
        let plugins = parse_list("→ Repository solo:\n  │ Plugin solo\n  └─ enabled: true\n");
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].repository, "solo");
        assert_eq!(plugins[0].author, "");
    }

    #[test]
    fn arguments_are_checked_before_they_reach_hyprpm() {
        assert!(Op::parse("enable", "").is_err());
        assert!(Op::parse("add", "--force").is_err());
        assert!(Op::parse("nonsense", "").is_err());
        assert_eq!(Op::parse("update", "").unwrap(), Op::Update);
        assert_eq!(
            Op::parse("add", " https://example.com/x ").unwrap(),
            Op::Add("https://example.com/x".into())
        );
    }

    #[test]
    fn a_label_names_the_repository() {
        assert_eq!(
            Op::Add("https://github.com/a/HyprWindowShade.git".into()).label(),
            "Installing HyprWindowShade"
        );
    }

    #[test]
    fn arguments_are_quoted_for_the_terminal() {
        assert_eq!(shell_quote("plain"), "'plain'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }
}
