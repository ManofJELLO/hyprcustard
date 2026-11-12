//! Small previews of wallpapers, made once with ffmpeg and kept in the
//! cache.
//!
//! A preview is named after the file's path, size and modification time, so
//! a changed file gets a new one and nothing needs invalidating. Videos are
//! previewed from a frame a couple of seconds in, which is usually past a
//! fade from black.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::error::{Error, Result};

/// Preview width and height, in pixels: 16:9, sharp at twice the size the
/// page shows them.
pub const SIZE: (u32, u32) = (384, 216);

/// `$XDG_CACHE_HOME/hyprcustard/thumbs`. A test gets its own.
pub fn dir() -> PathBuf {
    if cfg!(test) {
        return crate::paths::app_config_dir().join("thumbs");
    }
    dirs::cache_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".cache"))
        .join("hyprcustard")
        .join("thumbs")
}

/// Where `file`'s preview goes, or `None` if the file cannot be read.
pub fn path_for(file: &Path) -> Option<PathBuf> {
    let meta = std::fs::metadata(file).ok()?;
    let mut h = Fnv::default();
    h.write(file.as_os_str().as_encoded_bytes());
    h.write(&meta.len().to_le_bytes());
    h.write(&meta.mtime().to_le_bytes());
    h.write(&meta.mtime_nsec().to_le_bytes());
    Some(dir().join(format!("{:016x}.jpg", h.0)))
}

/// FNV-1a: small, and the same from one build to the next, unlike std's
/// hasher.
struct Fnv(u64);
impl Default for Fnv {
    fn default() -> Self {
        Fnv(0xcbf29ce484222325)
    }
}
impl Fnv {
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }
}

/// Make `file`'s preview at `out`, unless it is there already.
pub fn make(file: &Path, out: &Path) -> Result<()> {
    if out.exists() {
        return Ok(());
    }
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let (w, h) = SIZE;
    let filter =
        format!("scale={w}:{h}:force_original_aspect_ratio=increase:flags=bilinear,crop={w}:{h}");
    let tmp = out.with_extension("part.jpg");
    let attempt = |seek: Option<&str>| {
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-nostdin", "-v", "error", "-y"]);
        if let Some(s) = seek {
            cmd.args(["-ss", s]);
        }
        cmd.arg("-i")
            .arg(file)
            .args(["-vf", &filter, "-frames:v", "1", "-q:v", "4", "-f", "image2"])
            .arg(&tmp)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success() && tmp.metadata().map(|m| m.len() > 0).unwrap_or(false))
            .unwrap_or(false)
    };
    // Two seconds in, for a video; a clip shorter than that, or a still,
    // from the start.
    let ok = (super::library::video(file) && attempt(Some("2"))) || attempt(None);
    if !ok {
        let _ = std::fs::remove_file(&tmp);
        return Err(Error::other(format!(
            "ffmpeg could not read {}",
            crate::paths::contract(file)
        )));
    }
    std::fs::rename(&tmp, out).map_err(|e| Error::io(out, e))?;
    Ok(())
}

/// Make the previews for `files` that are missing, a few at a time, in
/// order, calling `done(file, preview)` as each one is ready. Stops early
/// when `stop` is set.
pub fn make_all(files: &[PathBuf], stop: &AtomicBool, done: impl Fn(&Path, &Path) + Sync) {
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).clamp(1, 4);
    let next = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(file) = files.get(i) else { break };
                let Some(out) = path_for(file) else { continue };
                if make(file, &out).is_ok() {
                    done(file, &out);
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ffmpeg() -> bool {
        crate::paths::which("ffmpeg").is_some()
    }

    #[test]
    fn the_name_changes_with_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.png");
        std::fs::write(&f, b"one").unwrap();
        let first = path_for(&f).unwrap();
        assert_eq!(path_for(&f).unwrap(), first, "stable");
        std::fs::write(&f, b"three").unwrap();
        assert_ne!(path_for(&f).unwrap(), first, "a new size is a new preview");
        assert!(path_for(&dir.path().join("gone.png")).is_none());
    }

    #[test]
    fn previews_a_still_and_a_short_video() {
        if !ffmpeg() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let still = dir.path().join("s.png");
        let clip = dir.path().join("c.mp4");
        for (out, src) in [(&still, "color=c=red:s=64x48"), (&clip, "testsrc=s=64x48:d=1")] {
            let ok = Command::new("ffmpeg")
                .args(["-nostdin", "-v", "error", "-y", "-f", "lavfi", "-i", src])
                .args(if out == &still {
                    vec!["-frames:v", "1"]
                } else {
                    vec!["-pix_fmt", "yuv420p"]
                })
                .arg(out)
                .status()
                .unwrap()
                .success();
            assert!(ok);
        }
        let seen = std::sync::Mutex::new(Vec::new());
        let stop = AtomicBool::new(false);
        let files = vec![still.clone(), clip.clone(), dir.path().join("broken.png")];
        std::fs::write(&files[2], b"not an image").unwrap();
        make_all(&files, &stop, |f, p| {
            assert!(p.exists());
            seen.lock().unwrap().push(f.to_path_buf());
        });
        let mut seen = seen.into_inner().unwrap();
        seen.sort();
        assert_eq!(seen, vec![clip.clone(), still.clone()], "the one-second clip falls back");
        // Made once: a second pass finds them there.
        let out = path_for(&still).unwrap();
        let before = std::fs::metadata(&out).unwrap().modified().unwrap();
        make(&still, &out).unwrap();
        assert_eq!(std::fs::metadata(&out).unwrap().modified().unwrap(), before);
    }
}
