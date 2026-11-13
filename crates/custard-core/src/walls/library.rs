//! The wallpapers a user has: which folders to look in, and what is in them.
//!
//! Nobody's folders are assumed. The list starts from where the running
//! wallpaper program is reading and the handful of places wallpapers are
//! commonly kept, and the user adds or hides folders from there.

use std::path::{Path, PathBuf};

use crate::colors::extract::{is_media, is_video};
use crate::colors::wallpaper::Found;

/// How deep below a folder to look.
const DEPTH: usize = 3;
/// The most files listed, across every folder.
pub const MAX_FILES: usize = 4000;

/// Places wallpapers are often kept, relative to the home directory.
const COMMON: [&str; 9] = [
    "Pictures/Wallpapers",
    "Pictures/wallpapers",
    "Pictures/Wallpaper",
    "Pictures/wallpaper",
    "Wallpapers",
    "wallpapers",
    "Wallpaper",
    "wallpaper",
    ".local/share/backgrounds",
];

/// A folder on the page.
#[derive(Debug, Clone, PartialEq)]
pub struct Folder {
    /// The folder.
    pub path: PathBuf,
    /// Found rather than added: removing it hides it instead.
    pub found: bool,
}

/// The folders to show: what was found, less what the user hid, then what
/// the user added. A folder inside another one on the list is left out,
/// because the outer one already lists it.
pub fn folders(found: &Found, added: &[String], hidden: &[String]) -> Vec<Folder> {
    let home = dirs::home_dir().unwrap_or_default();
    let mut found_dirs: Vec<PathBuf> = Vec::new();
    for w in &found.wallpapers {
        let dir = match w.folder() {
            Some(d) => Some(d.to_path_buf()),
            None => w.path.parent().map(Path::to_path_buf),
        };
        found_dirs.extend(dir);
    }
    found_dirs.extend(COMMON.iter().map(|c| home.join(c)));
    found_dirs.push(PathBuf::from("/usr/share/backgrounds"));
    let hidden: Vec<PathBuf> = hidden.iter().map(|h| crate::paths::expand(h)).collect();

    let mut out: Vec<Folder> = Vec::new();
    let push = |path: PathBuf, found: bool, out: &mut Vec<Folder>| {
        let path = clean(&path);
        if !path.is_dir() || out.iter().any(|f| f.path == path) {
            return;
        }
        out.push(Folder { path, found });
    };
    for d in found_dirs {
        if !hidden.iter().any(|h| clean(h) == clean(&d)) {
            push(d, true, &mut out);
        }
    }
    for a in added {
        push(crate::paths::expand(a), false, &mut out);
    }
    // Drop a folder another one on the list already covers.
    let all = out.clone();
    out.retain(|f| !all.iter().any(|g| g.path != f.path && f.path.starts_with(&g.path)));
    out
}

/// A path without a trailing slash, symlinks resolved where possible.
fn clean(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.components().collect())
}

/// One folder's files.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// The folder the files are in.
    pub dir: PathBuf,
    /// Which listed folder it is in.
    pub root: PathBuf,
    /// Its images and videos, by name.
    pub files: Vec<PathBuf>,
}

/// Every image and video under `roots`, grouped by the folder they are in:
/// each root first, then its subfolders by name. Hidden folders are skipped,
/// and listing stops at [`MAX_FILES`].
pub fn scan(roots: &[PathBuf]) -> Vec<Group> {
    let mut groups = Vec::new();
    let mut count = 0;
    for root in roots {
        walk(root, root, 0, &mut groups, &mut count);
    }
    groups
}

fn walk(root: &Path, dir: &Path, depth: usize, groups: &mut Vec<Group>, count: &mut usize) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    for e in rd.filter_map(|e| e.ok()) {
        let name = e.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let path = e.path();
        // Follows symlinks, so a linked folder or file counts.
        if path.is_dir() {
            dirs.push(path);
        } else if is_media(&path) {
            files.push(path);
        }
    }
    files.sort_by_key(|p| p.file_name().map(|n| n.to_ascii_lowercase()));
    dirs.sort_by_key(|p| p.file_name().map(|n| n.to_ascii_lowercase()));
    let room = MAX_FILES.saturating_sub(*count);
    files.truncate(room);
    *count += files.len();
    if !files.is_empty() {
        groups.push(Group { dir: dir.to_path_buf(), root: root.to_path_buf(), files });
    }
    if depth < DEPTH {
        for d in dirs {
            if *count >= MAX_FILES {
                break;
            }
            walk(root, &d, depth + 1, groups, count);
        }
    }
}

/// Whether a file is a video (as opposed to a still image or a GIF).
pub fn video(path: &Path) -> bool {
    is_video(path)
}

/// Whether a file is a GIF, which not every program can read.
pub fn gif(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("gif"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colors::wallpaper::Wallpaper;

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"x").unwrap();
    }

    #[test]
    fn scans_groups_by_folder_and_skips_hidden_and_other_files() {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        touch(&r.join("b.png"));
        touch(&r.join("A.jpg"));
        touch(&r.join("notes.txt"));
        touch(&r.join("clips/z.mp4"));
        touch(&r.join(".thumbs/hidden.png"));
        touch(&r.join("empty/readme.md"));
        let groups = scan(&[r.to_path_buf()]);
        assert_eq!(groups.len(), 2);
        let names: Vec<_> =
            groups[0].files.iter().map(|f| f.file_name().unwrap().to_owned()).collect();
        assert_eq!(names, ["A.jpg", "b.png"]);
        assert_eq!(groups[1].dir, r.join("clips"));
        assert_eq!(groups[1].root, r);
    }

    #[test]
    fn folders_start_from_the_running_program_and_drop_nested_ones() {
        let dir = tempfile::tempdir().unwrap();
        let walls = dir.path().join("walls");
        let clips = walls.join("clips");
        let other = dir.path().join("other");
        std::fs::create_dir_all(&clips).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        let found = Found {
            wallpapers: vec![Wallpaper {
                program: "mpvpaper".into(),
                monitor: None,
                path: clips.clone(),
                showing: None,
            }],
            ..Found::default()
        };
        let list = folders(&found, &[other.to_string_lossy().into()], &[]);
        let mine: Vec<_> = list.iter().filter(|f| f.path.starts_with(dir.path())).collect();
        assert_eq!(mine.len(), 2);
        assert!(mine[0].found && mine[0].path.ends_with("walls/clips"));
        assert!(!mine[1].found);

        // Adding the outer folder covers the found one.
        let list = folders(&found, &[walls.to_string_lossy().into()], &[]);
        let mine: Vec<_> = list.iter().filter(|f| f.path.starts_with(dir.path())).collect();
        assert_eq!(mine.len(), 1);
        assert!(mine[0].path.ends_with("walls"));

        // Hiding a found folder takes it off.
        let list = folders(&found, &[], &[clips.to_string_lossy().into()]);
        assert!(!list.iter().any(|f| f.path.starts_with(dir.path())));
    }
}
