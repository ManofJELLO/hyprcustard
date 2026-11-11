//! Putting the user's own desktop behind the preview window.
//!
//! A flat colour is honest but uninformative: whether a dissolve really reaches
//! zero alpha, or a tint lifts blacks, only shows against something with light
//! and dark in it. The most representative something is the desktop the shader
//! is actually going to run on.
//!
//! There are two ways to get it, and the cheap one is tried first.
//!
//! **Ask swaybg.** It is the tool this shows the backdrop with, so it is the
//! one wallpaper daemon the preview can answer for, and a running swaybg was
//! told on its command line which file it is showing. Reading that back costs
//! nothing, touches nothing, and moves nothing — and the nested compositor is
//! then pointed at the user's actual wallpaper file rather than at a
//! photograph of it, which is a sharper picture into the bargain.
//!
//! **Photograph the desktop**, when the wallpaper was set some other way — a
//! different daemon, a video, a shuffling directory, a shader. A screenshot
//! works whatever is behind it, because by then it is just pixels. But
//! photographing the screen itself would catch every window on it, and there
//! is no way to ask for the wallpaper layer alone, so the photograph is taken
//! somewhere nothing has ever been opened: a headless output added to the
//! user's own session for a moment, which arrives with an empty workspace and
//! the wallpaper already drawn on it, and is taken away again immediately.
//!
//! That second route is not free. Removing an output makes Hyprland throw the
//! mouse pointer into the middle of the screen, and while the pointer is put
//! back afterwards (see [`Borrowed`]) the round trip is still visible. So it
//! is taken at most once for the life of the app and remembered; see
//! [`photograph`].

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use crate::error::{Error, Result};
use crate::hyprctl;
use crate::paths;

/// The one tool the backdrop is shown with.
///
/// hyprpaper is not an alternative. Against a headless output it answers
/// `Monitor HEADLESS-1 has no target: no wp will be created` and paints the
/// output white — measured with the wildcard monitor, with the output named
/// explicitly, with and without an instance signature in its environment, and
/// with another encoder's PNG, while swaybg drew the same file on the same
/// instance without complaint. wbg works, but it is not in the Arch
/// repositories, and one name that is beats two where one has to be built.
const SHOW: &str = "swaybg";

/// Held for the whole borrow of a scratch output.
///
/// The output that has just appeared is identified by diffing the monitor list,
/// so two captures overlapping would each see the other's output and one of
/// them would be left behind — which is exactly what running the live tests in
/// parallel did. Two copies of the app could still collide; one copy cannot.
static BORROW: Mutex<()> = Mutex::new(());

/// A headless output borrowed from the user's session, given back on drop.
///
/// A guard rather than a cleanup at the end of the happy path, because every
/// way out of the capture has to return it: an error reading the monitor list,
/// a `grim` that fails, a panic. A stray monitor left on someone's desktop is
/// far worse than a missing backdrop, and the first version of this leaked one
/// whenever the output could not be identified.
struct Borrowed {
    name: String,
    /// The mode it came up in, which is what was photographed.
    size: (u32, u32),
    /// Where the pointer was before any of this, so it can be put back.
    cursor: Option<(i32, i32)>,
}

impl Drop for Borrowed {
    fn drop(&mut self) {
        let _ = hyprctl::run_args(&["output", "remove", &self.name]);
        if let Some(was) = self.cursor {
            restore_cursor(was);
        }
    }
}

/// Undo the jump the compositor makes when an output is taken away.
///
/// Removing a monitor warps the pointer to the middle of one of the others —
/// `CMonitor::onDisconnect` does it with `force`, so `cursor:no_warps` does
/// not stop it and there is nothing to configure around it. A preview is not
/// allowed to move someone's mouse, so the position is noted before the
/// output is borrowed and restored once it has been given back.
///
/// Only the compositor's own warp is undone. If the pointer is anywhere other
/// than the exact middle of a monitor, the person moved it themselves during
/// the second this took, and moving it back would be the same rudeness in the
/// other direction.
fn restore_cursor(was: (i32, i32)) {
    // The warp lands as the output goes away, which is a moment after
    // `output remove` has answered.
    std::thread::sleep(Duration::from_millis(150));

    let Some(now) = cursor_pos() else { return };
    if now == was || !is_a_monitor_middle(now) {
        return;
    }
    warp_to(was);
}

/// Where the pointer is.
fn cursor_pos() -> Option<(i32, i32)> {
    parse_pos(&hyprctl::run_args(&["cursorpos"]).ok()?)
}

/// Read what `hyprctl cursorpos` prints: `881, 973`.
fn parse_pos(out: &str) -> Option<(i32, i32)> {
    let (x, y) = out.trim().split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

/// Whether a position is exactly where a removal would have thrown the mouse.
fn is_a_monitor_middle(pos: (i32, i32)) -> bool {
    hyprctl::monitors().is_ok_and(|monitors| is_middle_of(&monitors, pos))
}

/// The same arithmetic Hyprland does — the monitor's origin plus half its
/// size — rather than anything cleverer, so that a layout this does not
/// predict correctly simply leaves the pointer alone.
fn is_middle_of(monitors: &[hyprctl::Monitor], (x, y): (i32, i32)) -> bool {
    monitors.iter().any(|m| {
        let (mx, my) = (m.x + (m.width / 2) as i32, m.y + (m.height / 2) as i32);
        (mx - x).abs() <= 1 && (my - y).abs() <= 1
    })
}

/// Put the pointer at a position, whichever config the session is written in.
///
/// `hyprctl dispatch` is evaluated as Lua when the user's own config is Lua,
/// and taken as a plain dispatcher line when it is not. Nothing from outside
/// says which, and the wrong spelling fails without doing anything, so both
/// are offered and the first one that is accepted wins.
fn warp_to((x, y): (i32, i32)) {
    if hyprctl::run_args(&["dispatch", &format!("movecursor {x} {y}")]).is_ok() {
        return;
    }
    let lua = format!("hl.dsp.cursor.move({{x = {x}, y = {y}}})");
    let _ = hyprctl::run_args(&["dispatch", &lua]);
}

/// Add a headless output and wait for it to arrive.
fn borrow_output() -> Result<Borrowed> {
    // Two captures overlapping would each see the other's output in the diff
    // below, so only one may be in flight; see `BORROW`.
    let before: Vec<String> = names()?;
    // Before the output exists, because giving it back is what moves the
    // pointer; see `Borrowed`.
    let cursor = cursor_pos();
    hyprctl::run_args(&["output", "create", "headless"])?;

    // Give the output time to arrive and the wallpaper time to be drawn on it.
    // A wallpaper daemon told to cover every output will follow; one pinned to
    // a named monitor will not, and then the photograph comes back flat —
    // which is what `looks_blank` is for.
    std::thread::sleep(Duration::from_millis(900));

    let found = hyprctl::monitors()?.into_iter().find(|m| !before.contains(&m.name));
    match found {
        Some(m) => Ok(Borrowed { name: m.name, size: (m.width, m.height), cursor }),
        // Nothing to give back, so no guard is built — but something did
        // probably arrive late, and the next capture's diff would then adopt
        // it. Remove by the name it would have had.
        None => {
            let _ = hyprctl::run_args(&["output", "remove", "HEADLESS-1"]);
            Err(Error::other("the scratch output never arrived"))
        }
    }
}

/// Photograph the user's wallpaper into `dest`, without their windows on it.
///
/// Runs against the session the app is in, not the preview's compositor — this
/// is the desktop worth looking at. Returns the size of what was photographed.
pub fn capture_desktop(dest: &Path) -> Result<(u32, u32)> {
    let _borrow_lock = BORROW.lock().unwrap_or_else(|e| e.into_inner());
    let output = borrow_output()?;
    grim(&output.name, dest)?;
    Ok(output.size)
}

fn grim(output: &str, dest: &Path) -> Result<()> {
    let out = Command::new("grim")
        .args(["-o", output, "-l", "1"])
        .arg(dest)
        .output()
        .map_err(|e| Error::other(format!("grim: {e}")))?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(Error::other(if err.is_empty() {
            "grim could not photograph the desktop".to_string()
        } else {
            err
        }));
    }
    Ok(())
}

fn names() -> Result<Vec<String>> {
    Ok(hyprctl::monitors()?.into_iter().map(|m| m.name).collect())
}

/// The program and arguments that show `image`, or `None` when swaybg is not
/// installed.
pub fn show_command(image: &Path) -> Option<(PathBuf, Vec<String>)> {
    Some((paths::which(SHOW)?, show_args(image)))
}

/// `fill` crops to cover, which is the cropping-to-aspect the pane wants: a
/// wallpaper is a whole monitor and the pane is a small rectangle of a
/// different shape, so anything else would squash it.
fn show_args(image: &Path) -> Vec<String> {
    vec!["-i".into(), image.to_string_lossy().into_owned(), "-m".into(), "fill".into()]
}

/// Where the photograph of the desktop is kept.
pub fn image_path(dir: &Path) -> PathBuf {
    dir.join("desktop.png")
}

/// Whether a capture looks like it caught nothing.
///
/// A wallpaper daemon pinned to a named monitor will not follow onto the
/// scratch output, and what comes back is then a single flat colour. Decoding
/// the image to find out would mean carrying a PNG decoder for one check, so
/// this goes by size instead: a flat colour compresses to almost nothing,
/// while any real picture does not come close to this floor. Being wrong in
/// the cautious direction costs the backdrop, not the preview.
fn looks_blank(image: &Path, pixels: u64) -> bool {
    let bytes = std::fs::metadata(image).map(|m| m.len()).unwrap_or(0);
    bytes < pixels / 100
}

/// The image to stand the preview on, or `None` for the flat colour.
///
/// Called **before** the preview's compositor is started, and that ordering is
/// not incidental: a nested compositor puts a window on the user's screen for
/// the moment before it is made headless, and photographing the desktop after
/// that catches the preview's own scaffolding in the picture.
///
/// Best-effort throughout: without it the preview still runs, on the flat
/// colour of the pane, and only the backdrop is lost.
pub fn find_image(dir: &Path) -> Option<PathBuf> {
    // Nothing to show it with is reason enough not to go looking.
    paths::which(SHOW)?;
    swaybg_image(dir).or_else(|| photograph(dir))
}

/// The wallpaper the user's own swaybg was told to show.
///
/// This is why the pointer usually stays where it is. swaybg is given the
/// file on its command line and the kernel keeps that command line readable,
/// so the whole question is answered by reading `/proc`: nothing is added to
/// the session, nothing is removed from it, and nothing moves.
///
/// `ours` is the preview's own runtime directory. The preview runs a swaybg
/// of its own, and on the fallback route that one is showing the photograph
/// in there — which would make the next preview a photograph of a
/// photograph. A swaybg showing a file from that directory is therefore not
/// the user's.
fn swaybg_image(ours: &Path) -> Option<PathBuf> {
    swaybg_image_under(Path::new("/proc"), ours)
}

/// The same, told where the process table is, so it can be tested.
fn swaybg_image_under(proc_root: &Path, ours: &Path) -> Option<PathBuf> {
    let procs = std::fs::read_dir(proc_root).ok()?;
    procs
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().chars().all(|c| c.is_ascii_digit()))
        .find_map(|e| {
            let argv = argv_of(&e.path())?;
            (argv.first()?.rsplit('/').next()? == SHOW).then_some(())?;
            let image = resolve(image_in(&argv)?, &e.path());
            (image.is_file() && !image.starts_with(ours)).then_some(image)
        })
}

/// One process's command line, as the arguments it was started with.
fn argv_of(proc: &Path) -> Option<Vec<String>> {
    let raw = std::fs::read(proc.join("cmdline")).ok()?;
    let argv: Vec<String> = raw
        .split(|b| *b == 0)
        .filter(|a| !a.is_empty())
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect();
    (!argv.is_empty()).then_some(argv)
}

/// The image a swaybg command line was told to show.
///
/// swaybg takes one `-i` per output, so a desktop with a different wallpaper
/// on each monitor has several. The first is as good a choice as any: the
/// preview is one small pane and cannot show them all.
fn image_in(argv: &[String]) -> Option<&str> {
    let mut args = argv.iter().skip(1);
    while let Some(arg) = args.next() {
        if let Some(path) = arg.strip_prefix("--image=") {
            return Some(path);
        }
        if arg == "-i" || arg == "--image" {
            return args.next().map(String::as_str);
        }
    }
    None
}

/// Make a path from a command line absolute, the way that process would.
fn resolve(path: &str, proc: &Path) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    // Relative to wherever it was started, which is not where this app was.
    match std::fs::read_link(proc.join("cwd")) {
        Ok(cwd) => cwd.join(path),
        Err(_) => path.to_path_buf(),
    }
}

/// The photograph, taken at most once for the life of the app.
///
/// Every photograph costs a monitor borrowed from the user's session and a
/// pointer thrown across the screen and put back, and that is too much to pay
/// on every press of Run. A wallpaper that changes while the app is open is
/// the smaller wrong, and the honest fix for it is to use swaybg, which is
/// read afresh every time and costs nothing.
fn photograph(dir: &Path) -> Option<PathBuf> {
    static PHOTO: OnceLock<Option<PathBuf>> = OnceLock::new();
    PHOTO.get_or_init(|| take_photograph(dir)).clone()
}

/// How to show an image that [`find_image`] found.
pub fn prepare(image: &Path) -> Option<(PathBuf, Vec<String>)> {
    image.is_file().then(|| show_command(image))?
}

fn take_photograph(dir: &Path) -> Option<PathBuf> {
    let image = image_path(dir);
    let _ = std::fs::remove_file(&image);

    // The size comes back from the output that was actually photographed. The
    // user's own monitors are the wrong yardstick: the scratch output comes up
    // in whatever mode the headless backend chooses, and on a large desktop
    // that would set the floor far above what the capture could ever weigh.
    let (width, height) = capture_desktop(&image).ok()?;
    if looks_blank(&image, u64::from(width) * u64::from(height)) {
        let _ = std::fs::remove_file(&image);
        return None;
    }
    Some(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swaybg_is_told_what_to_show_and_how() {
        let args = show_args(Path::new("/tmp/desktop.png"));
        assert!(args.contains(&"/tmp/desktop.png".to_string()), "{args:?}");
        // A whole wallpaper shown in a small pane of another shape has to be
        // cropped; stretching it would misrepresent every proportion in it.
        assert!(args.contains(&"fill".to_string()), "{args:?}");
    }

    #[test]
    fn the_wallpaper_is_read_off_swaybgs_command_line() {
        let argv = |s: &str| s.split(' ').map(String::from).collect::<Vec<_>>();

        assert_eq!(
            image_in(&argv("swaybg -i /home/me/wall.png -m fill")),
            Some("/home/me/wall.png")
        );
        assert_eq!(image_in(&argv("swaybg --image /home/me/wall.png")), Some("/home/me/wall.png"));
        assert_eq!(image_in(&argv("swaybg --image=/home/me/wall.png")), Some("/home/me/wall.png"));
        // One wallpaper per output: the first will do for a pane this size.
        assert_eq!(image_in(&argv("swaybg -o DP-1 -i /a.png -o DP-2 -i /b.png")), Some("/a.png"));
        // A flat colour is not an image, and there is nothing to show.
        assert_eq!(image_in(&argv("swaybg -c 1e1e2e")), None);
        // `-i` is the last word, so there is nothing after it to take.
        assert_eq!(image_in(&argv("swaybg -i")), None);
    }

    #[test]
    fn a_relative_wallpaper_is_resolved_where_swaybg_was_started() {
        assert_eq!(
            resolve("/home/me/wall.png", Path::new("/proc/1")),
            PathBuf::from("/home/me/wall.png")
        );
        // No such process, so no cwd to read: left as it was rather than
        // silently resolved against this app's own directory.
        assert_eq!(resolve("wall.png", Path::new("/proc/nonexistent")), PathBuf::from("wall.png"));
    }

    #[test]
    fn a_flat_capture_is_treated_as_no_capture() {
        let d = tempfile::tempdir().unwrap();
        let blank = d.path().join("blank.png");
        // Two and a half million pixels would never compress to this.
        std::fs::write(&blank, vec![0u8; 4_000]).unwrap();
        assert!(looks_blank(&blank, 2_560 * 1_080));

        let photo = d.path().join("photo.png");
        std::fs::write(&photo, vec![0u8; 900_000]).unwrap();
        assert!(!looks_blank(&photo, 2_560 * 1_080));
    }

    #[test]
    fn a_capture_that_never_happened_is_blank() {
        assert!(looks_blank(Path::new("/nonexistent/none.png"), 1_000_000));
    }

    fn monitor(name: &str, x: i32, y: i32, width: u32, height: u32) -> hyprctl::Monitor {
        hyprctl::Monitor { name: name.into(), focused: false, x, y, width, height }
    }

    #[test]
    fn the_pointer_is_read_back_the_way_hyprctl_prints_it() {
        assert_eq!(parse_pos("881, 973\n"), Some((881, 973)));
        assert_eq!(parse_pos("0, 0"), Some((0, 0)));
        assert_eq!(parse_pos("no such thing"), None);
    }

    #[test]
    fn only_the_compositors_own_warp_is_undone() {
        // Where a removal throws the pointer on a two-monitor desktop.
        let monitors =
            [monitor("DP-3", 0, 0, 2560, 1080), monitor("HDMI-A-1", 2560, 0, 1920, 1080)];
        assert!(is_middle_of(&monitors, (1280, 540)));
        assert!(is_middle_of(&monitors, (3520, 540)));
        // Anywhere else is someone using their mouse, and is left alone.
        assert!(!is_middle_of(&monitors, (1280, 600)));
        assert!(!is_middle_of(&monitors, (337, 911)));
    }

    /// A process table with one process in it, spelled the way /proc is.
    fn fake_proc(pid: &str, argv: &[&str]) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(pid);
        std::fs::create_dir(&dir).unwrap();
        let mut cmdline = Vec::new();
        for arg in argv {
            cmdline.extend_from_slice(arg.as_bytes());
            cmdline.push(0);
        }
        std::fs::write(dir.join("cmdline"), cmdline).unwrap();
        root
    }

    #[test]
    fn a_running_swaybg_is_where_the_wallpaper_comes_from() {
        let wall = tempfile::tempdir().unwrap();
        let image = wall.path().join("wall.png");
        std::fs::write(&image, b"pretend png").unwrap();

        let proc_root =
            fake_proc("1234", &["swaybg", "-o", "*", "-i", &image.to_string_lossy(), "-m", "fill"]);
        assert_eq!(swaybg_image_under(proc_root.path(), Path::new("/run/preview")), Some(image));
    }

    #[test]
    fn the_previews_own_swaybg_is_not_mistaken_for_the_users() {
        // Otherwise the fallback route feeds on itself: the preview's swaybg
        // shows the photograph, and the next preview takes that for the
        // wallpaper — a photograph of a photograph, forever.
        let ours = tempfile::tempdir().unwrap();
        let photo = ours.path().join("desktop.png");
        std::fs::write(&photo, b"pretend png").unwrap();

        let proc_root =
            fake_proc("1234", &["swaybg", "-i", &photo.to_string_lossy(), "-m", "fill"]);
        assert_eq!(swaybg_image_under(proc_root.path(), ours.path()), None);
    }

    #[test]
    fn nothing_else_in_the_process_table_is_taken_for_a_wallpaper() {
        let wall = tempfile::tempdir().unwrap();
        let image = wall.path().join("wall.png");
        std::fs::write(&image, b"pretend png").unwrap();

        // Another tool showing an image is not swaybg, and this app cannot
        // answer for what it would do with the file.
        let proc_root = fake_proc("1234", &["mpvpaper", "-i", &image.to_string_lossy()]);
        assert_eq!(swaybg_image_under(proc_root.path(), Path::new("/run/preview")), None);

        // A swaybg whose image has been deleted since it started.
        let gone = fake_proc("1234", &["swaybg", "-i", "/nowhere/wall.png"]);
        assert_eq!(swaybg_image_under(gone.path(), Path::new("/run/preview")), None);

        // A swaybg showing a flat colour has no image to lend.
        let colour = fake_proc("1234", &["swaybg", "-c", "1e1e2e"]);
        assert_eq!(swaybg_image_under(colour.path(), Path::new("/run/preview")), None);
    }
}
