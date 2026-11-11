//! Reading, describing and editing the `.glsl` files the plugin uses.

pub mod draft;
pub mod meta;
pub mod patch;
pub mod scan;

pub use draft::{DraftItem, ShaderDraft};
pub use meta::{Param, ParamKind, ParamValue, ShaderInfo, KNOWN_UNIFORMS, MOTION_UNIFORMS};
pub use scan::{missing, read, scan_dir};

use std::path::Path;

use crate::error::Result;

/// Write a shader's staged edits to its file: read, patch, back up, write.
///
/// One draft is one write and one backup, however many values it carries —
/// which is the point of staging them. A draft that comes out identical to the
/// file on disk writes nothing at all.
///
/// Returns the re-parsed shader, and the backup if one was taken.
pub fn save_draft(
    path: &Path,
    draft: &ShaderDraft,
    backups_to_keep: usize,
) -> Result<(ShaderInfo, Option<std::path::PathBuf>)> {
    let src = crate::paths::read_to_string(path)?;
    let patched = draft.apply(&src)?;
    let backup = if patched == src {
        None
    } else {
        let backup = crate::paths::backup(path, backups_to_keep)?;
        crate::paths::write_atomic(path, &patched)?;
        backup
    };
    Ok((read(path), backup))
}

#[cfg(test)]
mod tests {
    use super::*;

    // `keep = 0` so the backups these take are pruned again immediately: a
    // test has no business leaving files in the real configuration directory.
    #[test]
    fn a_saved_draft_round_trips_through_the_parser() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("dim.glsl");
        std::fs::write(&p, "// @param 0 1 0.01 \"Dim\"\nconst float DIM = 0.6;\nvoid main() {}\n")
            .unwrap();

        let before = read(&p);
        let mut draft = ShaderDraft::default();
        draft.set_param(&before, "DIM", ParamValue::Scalar(0.25));
        draft.set_duration(&before, Some(0.8));
        draft.set_overlay(&before, true);

        let (info, _) = save_draft(&p, &draft, 0).unwrap();
        let dim = info.params.iter().find(|x| x.name == "DIM").unwrap();
        assert_eq!(dim.value, ParamValue::Scalar(0.25));
        assert!(dim.annotated);
        assert_eq!(info.duration, Some(0.8));
        assert!(info.overlay);
    }

    #[test]
    fn an_empty_draft_does_not_touch_the_file() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("dim.glsl");
        let src = "const float DIM = 0.6;\nvoid main() {}\n";
        std::fs::write(&p, src).unwrap();

        let (_, backup) = save_draft(&p, &ShaderDraft::default(), 0).unwrap();
        assert!(backup.is_none(), "nothing changed, so nothing should have been backed up");
        assert_eq!(std::fs::read_to_string(&p).unwrap(), src);
    }
}
