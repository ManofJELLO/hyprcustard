//! Driving a `hyprpm` job, with a stand-in for hyprpm itself.
//!
//! The real thing needs a compositor, a network and several minutes, so what
//! is checked here is the machinery around it: that output arrives a line at a
//! time with the color taken out, that sudo's askpass helper is reachable and
//! is handed the flag that makes this app answer rather than start, that the
//! exit status is reported, and that canceling reaches the process the shell
//! is waiting on rather than only the shell.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::Duration;

use custard_core::hyprpm::{Event, Job, Op};

/// Write an executable shell script.
fn script(path: &Path, body: &str) {
    let mut file = std::fs::File::create(path).expect("write script");
    write!(file, "#!/bin/sh\n{body}\n").expect("write script");
    drop(file);
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
}

/// Put a fake `hyprpm` first on PATH.
fn shim(body: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    script(&dir.path().join("hyprpm"), body);
    let existing = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{existing}", dir.path().display()));
    dir
}

/// A stand-in for this app's `--askpass` mode: connect to the socket the job
/// set up, send the prompt, print what comes back.
///
/// The real one is in `hyprpm::askpass`, and the app it lives in is not
/// available from here, so the protocol is spoken by a few lines of Python.
/// Returns `None` where there is no Python to speak it with.
fn pretend_gui(dir: &Path) -> Option<PathBuf> {
    which("python3")?;
    let path = dir.join("pretend-gui");
    script(
        &path,
        r#"exec python3 -c '
import os, socket, sys
args = [a for a in sys.argv[1:] if a != "--askpass"]
s = socket.socket(socket.AF_UNIX)
s.connect(os.environ["CUSTARD_ASKPASS_SOCKET"])
s.sendall((" ".join(args) + "\n").encode())
line = s.makefile("r").readline()
if not line:
    sys.exit(1)
print(line.rstrip("\n"))
' "$@""#,
    );
    Some(path)
}

/// Whether a program is on PATH.
fn which(program: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?).find_map(|dir| {
        let candidate = dir.join(program);
        candidate.is_file().then_some(candidate)
    })
}

/// Wait for the next event, rather than hanging if one never comes.
fn next(rx: &Receiver<Event>) -> Event {
    match rx.recv_timeout(Duration::from_secs(30)) {
        Ok(event) => event,
        Err(RecvTimeoutError::Timeout) => panic!("the job went quiet"),
        Err(RecvTimeoutError::Disconnected) => panic!("the job hung up"),
    }
}

/// What a finished job produced.
struct Ran {
    lines: Vec<(String, bool)>,
    /// Every password the job asked for, and whether it said the last one was
    /// refused.
    prompts: Vec<bool>,
    ok: bool,
    message: String,
}

impl Ran {
    fn texts(&self) -> Vec<&str> {
        self.lines.iter().map(|(t, _)| t.as_str()).collect()
    }
}

/// Run a job to its end, answering each password prompt from `answers`.
fn run_answering(op: Op, askpass: &Path, answers: &[&str]) -> Ran {
    let (tx, rx) = channel();
    let job = Job::start(op, askpass, move |event| {
        let _ = tx.send(event);
    })
    .expect("start the job");

    let mut ran = Ran { lines: Vec::new(), prompts: Vec::new(), ok: false, message: String::new() };
    loop {
        match next(&rx) {
            Event::Line { text, transient } => ran.lines.push((text, transient)),
            Event::Password { retry, .. } => {
                ran.prompts.push(retry);
                match answers.get(ran.prompts.len() - 1) {
                    Some(answer) => job.answer_password(answer),
                    None => job.deny_password(),
                }
            }
            Event::Finished { ok, message } => {
                ran.ok = ok;
                ran.message = message;
                return ran;
            }
        }
    }
}

/// Run a job that is not expected to ask for anything.
fn run(op: Op, askpass: &Path) -> Ran {
    let ran = run_answering(op, askpass, &[]);
    assert!(ran.prompts.is_empty(), "nothing here should ask for a password");
    ran
}

/// PATH belongs to the whole process, and each phase puts its own fake hyprpm
/// on it, so they are one test rather than three racing ones.
#[test]
fn a_job_is_driven_from_start_to_finish() {
    output_is_cleaned_up_and_the_exit_status_is_reported();
    sudos_askpass_program_is_this_app_with_the_flag_that_answers();
    a_second_escalation_reuses_the_password();
    a_refused_password_is_asked_for_again();
    canceling_stops_what_hyprpm_is_waiting_on();
}

fn output_is_cleaned_up_and_the_exit_status_is_reported() {
    let _guard = shim(
        r#"printf '\033[32mbuilding\033[0m\n'
printf 'args:%s\n' "$*"
printf '\033[2K\r'
printf '\033[2K 1 / 2  working\r'
printf '\033[2K 2 / 2  working\r'
printf 'done\r\n'
echo 'to stderr' >&2
exit 3"#,
    );

    let ran = run(Op::Update, Path::new("/bin/true"));
    let (lines, ok, message) = (&ran.lines, ran.ok, &ran.message);
    let texts = ran.texts();

    assert!(texts.contains(&"building"), "color is stripped: {texts:?}");
    assert!(texts.contains(&"args:update"), "arguments reach hyprpm: {texts:?}");
    assert!(texts.contains(&"to stderr"), "stderr is part of the log: {texts:?}");

    assert!(
        lines.iter().any(|(t, transient)| t == " 1 / 2  working" && *transient),
        "a bar redrawn with a carriage return is marked as replacing the last line: {lines:?}"
    );
    assert!(
        !lines.iter().any(|(t, transient)| t.is_empty() && *transient),
        "an erase-line frame with nothing in it is dropped: {lines:?}"
    );
    assert!(
        lines.iter().any(|(t, transient)| t == "done" && !*transient),
        "a line ended with CRLF is one ordinary line: {lines:?}"
    );
    assert_eq!(
        lines.iter().filter(|(t, _)| t.is_empty()).count(),
        0,
        "CRLF does not leave a blank line behind: {lines:?}"
    );

    assert!(!ok, "exit code 3 is a failure");
    assert!(message.contains("exit code 3"), "the message says why: {message}");
}

fn sudos_askpass_program_is_this_app_with_the_flag_that_answers() {
    // sudo runs $SUDO_ASKPASS with the prompt as its only argument, so what it
    // points at has to be something that adds `--askpass` itself. Without
    // that, this executable would start its interface instead of answering.
    let dir = tempfile::tempdir().expect("temp dir");
    let seen = dir.path().join("argv");
    let askpass = dir.path().join("pretend-gui");
    script(&askpass, &format!("echo \"$@\" > '{}'\necho 'the-password'", seen.display()));

    let _guard = shim(
        r#"printf 'askpass is %s\n' "$SUDO_ASKPASS"
answer=$("$SUDO_ASKPASS" '[sudo] password for tester: ')
printf 'got:%s\n' "$answer""#,
    );

    let ran = run(Op::Reload, &askpass);
    let texts = ran.texts();

    assert!(ran.ok, "the job succeeded: {texts:?}");
    assert!(texts.contains(&"got:the-password"), "the answer reached hyprpm: {texts:?}");

    let argv = std::fs::read_to_string(&seen).expect("the askpass program ran");
    assert!(argv.contains("--askpass"), "the flag is added: {argv:?}");
    assert!(argv.contains("[sudo] password for tester:"), "the prompt is passed on: {argv:?}");
}

/// One click can escalate more than once. As long as hyprpm is getting on
/// with things in between, the password already given is used again rather
/// than asked for again.
fn a_second_escalation_reuses_the_password() {
    let dir = tempfile::tempdir().expect("temp dir");
    let Some(askpass) = pretend_gui(dir.path()) else {
        eprintln!("no python3: skipping the password-reuse phase");
        return;
    };

    let _guard = shim(
        r#"a=$("$SUDO_ASKPASS" 'password:')
printf 'first:%s\n' "$a"
b=$("$SUDO_ASKPASS" 'password:')
printf 'second:%s\n' "$b""#,
    );

    let ran = run_answering(Op::Update, &askpass, &["secret"]);
    let texts = ran.texts();

    assert_eq!(ran.prompts.len(), 1, "asked once, not twice: {texts:?}");
    assert!(texts.contains(&"first:secret"), "{texts:?}");
    assert!(texts.contains(&"second:secret"), "the second one is answered too: {texts:?}");
}

/// sudo asks up to three times and hyprpm hides its complaint, so a prompt
/// that arrives with nothing printed since the last answer means the answer
/// was wrong. Spending the remaining attempts on a password already refused
/// helps nobody.
fn a_refused_password_is_asked_for_again() {
    let dir = tempfile::tempdir().expect("temp dir");
    let Some(askpass) = pretend_gui(dir.path()) else {
        eprintln!("no python3: skipping the refused-password phase");
        return;
    };

    let _guard = shim(
        r#"a=$("$SUDO_ASKPASS" 'password:')
b=$("$SUDO_ASKPASS" 'password:')
printf 'tried:%s,%s\n' "$a" "$b""#,
    );

    let ran = run_answering(Op::Update, &askpass, &["wrong", "right"]);
    let texts = ran.texts();

    assert_eq!(ran.prompts.len(), 2, "asked a second time: {texts:?}");
    assert!(!ran.prompts[0], "the first time is not a retry");
    assert!(ran.prompts[1], "the second time says the last one was refused");
    assert!(texts.contains(&"tried:wrong,right"), "both answers got through: {texts:?}");
}

fn canceling_stops_what_hyprpm_is_waiting_on() {
    let _guard = shim("echo started\nsleep 60\necho 'should never get here'");

    let (tx, rx) = channel();
    let job = Job::start(Op::Reload, Path::new("/bin/true"), move |event| {
        let _ = tx.send(event);
    })
    .expect("start the job");

    loop {
        match next(&rx) {
            Event::Line { text, .. } if text == "started" => break,
            Event::Line { .. } => {}
            other => panic!("expected output first, got {other:?}"),
        }
    }

    job.cancel();

    let finished = loop {
        if let Event::Finished { ok, message } = next(&rx) {
            break (ok, message);
        }
    };
    assert!(!finished.0);
    assert!(finished.1.contains("canceled"), "the message says so: {}", finished.1);
}
