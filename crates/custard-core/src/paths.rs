//! Where things live, and how to write to them without losing anything.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// `$XDG_CONFIG_HOME` or `~/.config`.
pub fn config_home() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".config"))
}

/// `~/.config/hypr`.
pub fn hypr_dir() -> PathBuf {
    config_home().join("hypr")
}

/// `~/.config/hypr/hyprland.lua`.
pub fn default_lua_config() -> PathBuf {
    hypr_dir().join("hyprland.lua")
}

/// `~/.config/hypr/shaders`.
pub fn default_shader_dir() -> PathBuf {
    hypr_dir().join("shaders")
}

/// `~/.config/hyprcustard`, where this app keeps its own settings.
///
/// `CUSTARD_CONFIG_DIR` overrides it, which is what to set when running the app
/// against a throwaway configuration rather than your real one.
pub fn app_config_dir() -> PathBuf {
    match std::env::var_os("CUSTARD_CONFIG_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => config_home().join("hyprcustard"),
    }
}

/// `~/.config/hyprwindowshade-gui`, where this app's predecessor kept its
/// settings and themes.
fn legacy_app_config_dir() -> PathBuf {
    config_home().join("hyprwindowshade-gui")
}

/// Carry `settings.toml` and `themes/` over from hyprwindowshade-gui, once.
///
/// Runs only while this app has neither settings nor themes of its own, so it
/// never overwrites anything, and does nothing when `CUSTARD_CONFIG_DIR` points
/// the app somewhere else. An empty `backups/` does not count as configuration. Backups stay behind: they belong to the old app's
/// history, and the next save starts a fresh set here. Returns what was copied.
pub fn adopt_legacy_config() -> Vec<PathBuf> {
    if std::env::var_os("CUSTARD_CONFIG_DIR").is_some_and(|d| !d.is_empty()) {
        return Vec::new();
    }
    adopt_from(&legacy_app_config_dir(), &app_config_dir())
}

fn adopt_from(old: &Path, new: &Path) -> Vec<PathBuf> {
    let mut copied = Vec::new();
    if new.join("settings.toml").exists() || new.join("themes").exists() || !old.is_dir() {
        return copied;
    }
    let settings = old.join("settings.toml");
    if settings.is_file() && std::fs::create_dir_all(new).is_ok() {
        let dest = new.join("settings.toml");
        if std::fs::copy(&settings, &dest).is_ok() {
            copied.push(dest);
        }
    }
    if let Ok(entries) = std::fs::read_dir(old.join("themes")) {
        let themes = new.join("themes");
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() || std::fs::create_dir_all(&themes).is_err() {
                continue;
            }
            let dest = themes.join(entry.file_name());
            if std::fs::copy(&path, &dest).is_ok() {
                copied.push(dest);
            }
        }
    }
    copied
}

/// Where backups of edited files go.
pub fn backup_dir() -> PathBuf {
    app_config_dir().join("backups")
}

/// Find an executable on PATH.
///
/// Used to decide whether an optional helper — a terminal emulator, a wallpaper
/// tool — is available before offering something that depends on it.
pub fn which(program: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(&std::env::var_os("PATH")?).find_map(|dir| {
        let candidate = dir.join(program);
        let executable = std::fs::metadata(&candidate)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
        executable.then_some(candidate)
    })
}

/// Expand a leading `~` and return an absolute path.
pub fn expand(path: &str) -> PathBuf {
    let path = path.trim();
    if path == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(rest);
    }
    PathBuf::from(path)
}

/// Replace a `$HOME` prefix with `~` for display.
pub fn contract(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(rest) = path.strip_prefix(&home) {
            return format!("~/{}", rest.display());
        }
    }
    path.display().to_string()
}

/// Copy `path` into the backup directory under a timestamped name, and prune
/// old copies of the same file beyond `keep`.
///
/// Returns the backup path, or `None` if the source does not exist yet.
pub fn backup(path: &Path, keep: usize) -> Result<Option<PathBuf>> {
    backup_into(&backup_dir(), path, keep)
}

/// [`backup`], into a directory of the caller's choosing.
///
/// Separate so the naming and pruning can be exercised without writing into
/// the real configuration directory.
fn backup_into(dir: &Path, path: &Path, keep: usize) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;

    let stem =
        path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let mut dest = dir.join(format!("{stem}.{stamp}.bak"));
    // Two saves inside one second must not clobber each other.
    let mut n = 1;
    while dest.exists() {
        dest = dir.join(format!("{stem}.{stamp}-{n}.bak"));
        n += 1;
    }
    std::fs::copy(path, &dest).map_err(|e| Error::io(&dest, e))?;

    prune_backups(dir, &stem, keep)?;
    Ok(Some(dest))
}

fn prune_backups(dir: &Path, stem: &str, keep: usize) -> Result<()> {
    let prefix = format!("{stem}.");
    let mut mine: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| Error::io(dir, e))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .map(|n| {
                    let n = n.to_string_lossy();
                    n.starts_with(&prefix) && n.ends_with(".bak")
                })
                .unwrap_or(false)
        })
        .collect();
    if mine.len() <= keep {
        return Ok(());
    }
    mine.sort_by_key(|p| {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        backup_order(&name, &prefix)
    });
    let excess = mine.len() - keep;
    for p in mine.into_iter().take(excess) {
        let _ = std::fs::remove_file(p);
    }
    Ok(())
}

/// Sort key for a backup's file name: when it was taken, then which one it was
/// within that second.
///
/// The counter cannot be compared as text. A name is `stem.20260918-071638.bak`
/// or, for the second save inside one second, `stem.20260918-071638-1.bak` —
/// and `-` sorts before `.`, so as plain strings the newer copy looks like the
/// older one and pruning would throw away the wrong file.
fn backup_order(name: &str, prefix: &str) -> (String, u32) {
    let rest = name.strip_prefix(prefix).unwrap_or(name);
    let rest = rest.strip_suffix(".bak").unwrap_or(rest);
    // The timestamp itself contains one `-`, between the date and the time.
    let mut bits = rest.splitn(3, '-');
    let date = bits.next().unwrap_or_default();
    let time = bits.next().unwrap_or_default();
    // No counter means the first copy taken that second.
    let seq = bits.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (format!("{date}-{time}"), seq)
}

/// Write `contents` to `path` atomically: write a sibling temp file, fsync it,
/// then rename over the target.
///
/// A half-written `hyprland.lua` is a broken session, so this never truncates
/// the real file.
pub fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let tmp = path.with_extension(format!(
        "{}.custard-tmp",
        path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default()
    ));

    {
        let mut f = std::fs::File::create(&tmp).map_err(|e| Error::io(&tmp, e))?;
        f.write_all(contents.as_bytes()).map_err(|e| Error::io(&tmp, e))?;
        f.sync_all().map_err(|e| Error::io(&tmp, e))?;
    }

    // Keep the original file mode if there was one.
    if let Ok(meta) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }

    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        Error::io(path, e)
    })?;

    // Make the rename itself durable.
    //
    // The contents were fsynced before the rename and the rename is atomic, so
    // no crash can leave a half-written config either way. What the directory
    // fsync buys is the rename surviving a power cut: without it the new file
    // is on disk but the directory entry pointing at it may not be, and the
    // save silently rolls back to the previous version.
    //
    // Best-effort on purpose. If the directory cannot be opened or synced the
    // file is already written and renamed, and failing the save over it would
    // trade a small durability gap for a loud error about nothing.
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        if let Ok(dir) = std::fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }
    Ok(())
}

/// Read a file to a string, with the path in the error.
pub fn read_to_string(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| Error::io(path, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_and_contract_are_inverse_for_home_paths() {
        let Some(home) = dirs::home_dir() else { return };
        let p = expand("~/x/y");
        assert_eq!(p, home.join("x/y"));
        assert_eq!(contract(&p), "~/x/y");
    }

    #[test]
    fn legacy_settings_and_themes_are_adopted_once() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("old");
        let new = root.path().join("new");
        std::fs::create_dir_all(old.join("themes")).unwrap();
        std::fs::create_dir_all(old.join("backups")).unwrap();
        std::fs::write(old.join("settings.toml"), "backups_to_keep = 3\n").unwrap();
        std::fs::write(old.join("themes/mine.toml"), "name = \"Mine\"\n").unwrap();
        std::fs::write(old.join("backups/hyprland.lua.1.bak"), "x").unwrap();

        // A backups directory alone, as a save or a test run leaves, is not
        // configuration and must not stop the adoption.
        std::fs::create_dir_all(new.join("backups")).unwrap();
        let copied = adopt_from(&old, &new);
        assert_eq!(copied.len(), 2, "{copied:?}");
        assert_eq!(
            std::fs::read_to_string(new.join("settings.toml")).unwrap(),
            "backups_to_keep = 3\n"
        );
        assert!(new.join("themes/mine.toml").is_file());
        assert!(!new.join("backups/hyprland.lua.1.bak").exists());

        // Once the new directory exists it is never touched again.
        std::fs::write(new.join("settings.toml"), "backups_to_keep = 7\n").unwrap();
        assert!(adopt_from(&old, &new).is_empty());
        assert_eq!(
            std::fs::read_to_string(new.join("settings.toml")).unwrap(),
            "backups_to_keep = 7\n"
        );
    }

    #[test]
    fn nothing_to_adopt_creates_nothing() {
        let root = tempfile::tempdir().unwrap();
        let new = root.path().join("new");
        assert!(adopt_from(&root.path().join("missing"), &new).is_empty());
        assert!(!new.exists());
    }

    #[test]
    fn expand_leaves_absolute_paths_alone() {
        assert_eq!(expand("/etc/hosts"), PathBuf::from("/etc/hosts"));
    }

    #[test]
    fn backups_within_one_second_order_by_their_counter() {
        let p = "hyprland.lua.";
        let mut names = vec![
            "hyprland.lua.20260918-071638-2.bak".to_string(),
            "hyprland.lua.20260918-071638.bak".to_string(),
            "hyprland.lua.20260918-071638-1.bak".to_string(),
            "hyprland.lua.20260918-071637.bak".to_string(),
        ];
        names.sort_by_key(|n| backup_order(n, p));
        assert_eq!(
            names,
            vec![
                "hyprland.lua.20260918-071637.bak",
                "hyprland.lua.20260918-071638.bak",
                "hyprland.lua.20260918-071638-1.bak",
                "hyprland.lua.20260918-071638-2.bak",
            ]
        );
    }

    #[test]
    fn pruning_keeps_the_newest() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f.lua");
        std::fs::write(&file, "x").unwrap();

        // Three copies in quick succession, which normally collide on the
        // one-second stamp and so exercise the counter.
        let taken: Vec<PathBuf> =
            (0..3).map(|_| backup_into(dir.path(), &file, 2).unwrap().unwrap()).collect();

        let left: Vec<&PathBuf> = taken.iter().filter(|p| p.exists()).collect();
        assert_eq!(left.len(), 2, "kept {left:?} of {taken:?}");
        // The oldest is the one that went, whichever naming the clock produced.
        assert!(!taken[0].exists(), "dropped the wrong one: {taken:?}");
        assert!(taken[1].exists() && taken[2].exists(), "{taken:?}");
    }

    #[test]
    fn atomic_write_replaces_contents() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("f.lua");
        write_atomic(&p, "one").unwrap();
        write_atomic(&p, "two").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "two");
        // No temp files left behind.
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("custard-tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }
}
