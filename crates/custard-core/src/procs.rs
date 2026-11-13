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
}
