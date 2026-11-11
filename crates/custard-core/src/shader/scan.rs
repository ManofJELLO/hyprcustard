//! Finding the shaders on disk.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::error::{Error, Result};
use crate::shader::meta::{self, ShaderInfo};

/// Extensions treated as shader source.
const EXTENSIONS: &[&str] = &["glsl", "frag", "fs"];

/// Scan a directory for shaders and parse each one.
///
/// Sub-directories are walked one level deep, which covers the common habit of
/// grouping shaders into folders without turning a mistyped path into a walk of
/// the whole home directory. Unreadable files become entries with a note rather
/// than failing the whole scan.
pub fn scan_dir(dir: &Path) -> Result<Vec<ShaderInfo>> {
    let mut out = Vec::new();
    if !dir.exists() {
        return Err(Error::Shader(format!(
            "{} does not exist — point the shader folder at your shaders in Settings",
            crate::paths::contract(dir)
        )));
    }
    collect(dir, 1, &mut out)?;
    out.sort_by_key(|a| a.name.to_lowercase());
    Ok(out)
}

fn collect(dir: &Path, depth: usize, out: &mut Vec<ShaderInfo>) -> Result<()> {
    let entries = std::fs::read_dir(dir).map_err(|e| Error::io(dir, e))?;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        if meta.is_dir() {
            if depth > 0 {
                collect(&path, depth - 1, out)?;
            }
            continue;
        }

        let is_shader = path
            .extension()
            .map(|e| EXTENSIONS.contains(&e.to_string_lossy().to_lowercase().as_str()))
            .unwrap_or(false);
        if !is_shader {
            continue;
        }

        out.push(read(&path));
    }
    Ok(())
}

/// Read and parse one shader. Unreadable files come back as a stub carrying a
/// note, so the UI can show the problem instead of dropping the file.
pub fn read(path: &Path) -> ShaderInfo {
    let mtime = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let path_str = path.to_string_lossy().into_owned();
    match std::fs::read_to_string(path) {
        Ok(src) => meta::parse(&path_str, &src, mtime),
        Err(e) => {
            let mut info = meta::parse(&path_str, "", mtime);
            info.notes.push(format!("could not be read: {e}"));
            info
        }
    }
}

/// Shader paths a config refers to that are not on disk.
pub fn missing(paths: &[String]) -> Vec<String> {
    paths.iter().filter(|p| !PathBuf::from(p).exists()).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, body: &str) {
        std::fs::write(dir.join(name), body).unwrap();
    }

    #[test]
    fn finds_shaders_and_ignores_other_files() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "a.glsl", "const float A = 1.0;\n");
        write(d.path(), "b.frag", "const float B = 1.0;\n");
        write(d.path(), "notes.txt", "hello");

        let found = scan_dir(d.path()).unwrap();
        let names: Vec<_> = found.iter().map(|s| s.stem.clone()).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"a".to_string()));
        assert!(names.contains(&"b".to_string()));
    }

    #[test]
    fn walks_one_level_of_subdirectories() {
        let d = tempfile::tempdir().unwrap();
        let sub = d.path().join("animations");
        std::fs::create_dir(&sub).unwrap();
        write(&sub, "open.glsl", "// @duration 0.5\n");

        let found = scan_dir(d.path()).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].duration, Some(0.5));
    }

    #[test]
    fn a_missing_directory_says_so() {
        let err = scan_dir(Path::new("/nonexistent/hws/shaders")).unwrap_err();
        assert!(err.to_string().contains("does not exist"));
    }

    #[test]
    fn results_are_sorted_by_name() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "zebra.glsl", "");
        write(d.path(), "apple.glsl", "");
        let found = scan_dir(d.path()).unwrap();
        assert_eq!(found[0].stem, "apple");
    }

    #[test]
    fn missing_paths_are_reported() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "a.glsl", "");
        let here = d.path().join("a.glsl").to_string_lossy().into_owned();
        let gone = d.path().join("gone.glsl").to_string_lossy().into_owned();
        assert_eq!(missing(&[here, gone.clone()]), vec![gone]);
    }
}
