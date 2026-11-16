//! Finding this user's running processes by name.
//!
//! One reader for everything that signals or counts processes — the colors
//! targets reloading an app, and custard-watch making sure it runs once —
//! so all of them agree on what counts: processes of this user only (a
//! signal to anyone else's fails, and is not ours to send), and only ones
//! still alive. A process that has exited but not yet been reaped (a
//! zombie) stays in `/proc` with its name, and is not running.

use std::os::unix::fs::MetadataExt;

/// The ids of this user's live processes whose name (`comm`, which the
/// kernel cuts at 15 characters) is `name`, leaving out this one.
pub fn named(name: &str) -> Vec<u32> {
    let me = std::process::id();
    let Ok(uid) = std::fs::metadata("/proc/self").map(|m| m.uid()) else { return Vec::new() };
    let Ok(rd) = std::fs::read_dir("/proc") else { return Vec::new() };
    let mut out: Vec<u32> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let pid: u32 = e.file_name().to_str()?.parse().ok()?;
            (pid != me && e.metadata().ok()?.uid() == uid).then_some(pid)
        })
        .filter(|pid| {
            std::fs::read_to_string(format!("/proc/{pid}/comm")).is_ok_and(|c| c.trim() == name)
        })
        .filter(|pid| alive(*pid))
        .collect();
    out.sort_unstable();
    out
}

/// Whether a process exists and has not exited.
pub fn alive(pid: u32) -> bool {
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else { return false };
    // The state letter follows the name, which is in parentheses and may
    // itself hold spaces or parentheses.
    let state = stat.rsplit_once(')').and_then(|(_, rest)| rest.trim_start().chars().next());
    !matches!(state, None | Some('Z') | Some('X'))
}

/// Whether a process has a handler of its own for signal `signo` — and so
/// will not be ended by it, as a program still starting would be by any
/// signal whose default action is to terminate.
pub fn catches(pid: u32, signo: u32) -> bool {
    let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) else { return false };
    status
        .lines()
        .find_map(|l| l.strip_prefix("SigCgt:"))
        .and_then(|mask| u64::from_str_radix(mask.trim(), 16).ok())
        .is_some_and(|mask| (1..=64).contains(&signo) && mask & (1 << (signo - 1)) != 0)
}

/// How long ago a process started.
pub fn age(pid: u32) -> Option<std::time::Duration> {
    // Start time is in clock ticks since boot; Linux fixes the tick that
    // /proc reports in (USER_HZ) at 100 on every architecture it runs on.
    const USER_HZ: f64 = 100.0;
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (_, rest) = stat.rsplit_once(')')?;
    // Fields after the name start at the 3rd; start time is the 22nd.
    let start: f64 = rest.split_whitespace().nth(19)?.parse().ok()?;
    let uptime: f64 =
        std::fs::read_to_string("/proc/uptime").ok()?.split_whitespace().next()?.parse().ok()?;
    Some(std::time::Duration::from_secs_f64((uptime - start / USER_HZ).max(0.0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn finds_a_live_child_and_not_a_dead_unreaped_one() {
        let mut child = Command::new("sleep").arg("30").stdin(Stdio::null()).spawn().unwrap();
        let pid = child.id();
        assert!(named("sleep").contains(&pid));
        assert!(alive(pid));
        child.kill().unwrap();
        // Killed but not waited for: a zombie, still listed in /proc.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while alive(pid) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(std::path::Path::new(&format!("/proc/{pid}")).exists(), "still a zombie");
        assert!(!named("sleep").contains(&pid));
        child.wait().unwrap();
        assert!(!alive(pid));
    }

    #[test]
    fn tells_a_caught_signal_from_one_that_would_end_it() {
        let me = std::process::id();
        // Rust's runtime ignores SIGPIPE rather than catching it, and nothing
        // here handles SIGUSR2.
        assert!(!catches(me, 12));
        let mut child = Command::new("bash")
            .args(["-c", "trap 'echo' USR2; sleep 30"])
            .stdin(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !catches(pid, 12) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(catches(pid, 12), "bash traps USR2");
        assert!(!catches(pid, 10), "but not USR1");
        assert!(age(pid).is_some_and(|a| a < std::time::Duration::from_secs(10)));
        child.kill().unwrap();
        child.wait().unwrap();
    }
}
