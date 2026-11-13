//! One hyprcustard at a time.
//!
//! The first copy listens on a socket in the user's runtime directory. A
//! second launch — from a launcher while the first sits in the tray —
//! connects, asks it to show its window, and exits, rather than putting a
//! second window on screen.

use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

/// The first copy's socket, from [`claim`] until the backend takes it.
static LISTENER: Mutex<Option<UnixListener>> = Mutex::new(None);

/// What a second copy sends to have the window shown.
const SHOW: &str = "show";

fn socket_path() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir).join("hyprcustard.sock"),
        // No runtime directory: the shared temporary one, so named per user.
        _ => std::env::temp_dir()
            .join(format!("hyprcustard-{}.sock", std::env::var("USER").unwrap_or_default())),
    }
}

/// Whether another copy is running. If one is, it is asked to show its
/// window when `show` is set; if none is, this copy starts listening.
///
/// Anything unexpected (a socket that cannot be made) leaves this copy
/// running on its own, as it would have before there was a socket.
pub fn another_is_running(show: bool) -> bool {
    let path = socket_path();
    match UnixStream::connect(&path) {
        Ok(mut stream) => {
            if show {
                let _ = stream.write_all(format!("{SHOW}\n").as_bytes());
            }
            return true;
        }
        // A socket left by a copy that did not exit cleanly: nobody is
        // listening, so it is replaced.
        Err(e) if e.kind() == ErrorKind::ConnectionRefused => {
            let _ = std::fs::remove_file(&path);
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(_) => return false,
    }
    if let Ok(listener) = UnixListener::bind(&path) {
        *LISTENER.lock().unwrap_or_else(|e| e.into_inner()) = Some(listener);
    }
    false
}

/// Call `show` each time a later launch asks for the window, on a thread
/// of its own. Does nothing when this copy is not listening.
pub fn serve(show: impl Fn() -> bool + Send + 'static) {
    let Some(listener) = LISTENER.lock().unwrap_or_else(|e| e.into_inner()).take() else {
        return;
    };
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            // A client that connects and says nothing does not hold up the
            // next one.
            let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
            let mut line = String::new();
            let _ = BufReader::new(stream).read_line(&mut line);
            // `show` returns false once the window is gone.
            if line.trim() == SHOW && !show() {
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn a_second_launch_shows_the_first_and_a_stale_socket_is_replaced() {
        let dir = std::env::temp_dir().join(format!("hyprcustard-instance-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("XDG_RUNTIME_DIR", &dir);
        let sock = dir.join("hyprcustard.sock");

        // Left behind by a copy that crashed: bound, then dropped.
        drop(UnixListener::bind(&sock).unwrap());
        assert!(sock.exists());
        assert!(!another_is_running(true), "nobody listening, so this is the first");

        let shown = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&shown);
        serve(move || {
            seen.fetch_add(1, Ordering::SeqCst);
            true
        });
        assert!(another_is_running(false), "a --tray launch only leaves");
        assert!(another_is_running(true));
        for _ in 0..50 {
            if shown.load(Ordering::SeqCst) == 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(shown.load(Ordering::SeqCst), 1, "one show, for the one that asked");

        std::fs::remove_file(&sock).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }
}
