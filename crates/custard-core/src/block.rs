//! The managed block inside `hyprland.lua`.
//!
//! Everything this app generates lives between two marker comments. The rest of
//! the file is never touched. A single comment line inside the block carries the
//! whole [`Config`] as JSON, so the next run reads back exactly what it wrote
//! rather than trying to parse Lua.

use crate::error::{Error, Result};
use crate::model::Config;

/// Opening marker.
pub const BEGIN: &str = "-- >>> HyprWindowShade (managed by hyprwindowshade-gui) >>>";
/// Closing marker.
pub const END: &str = "-- <<< HyprWindowShade (managed by hyprwindowshade-gui) <<<";
/// Prefix of the line carrying the serialised state.
pub const STATE_PREFIX: &str = "-- hws-state-v1 ";

/// A file split around its managed block.
#[derive(Debug, Clone, PartialEq)]
pub struct Split {
    /// Everything before the opening marker.
    pub before: String,
    /// Everything between the markers, markers excluded.
    pub inner: String,
    /// Everything after the closing marker.
    pub after: String,
}

/// Find the managed block in a document.
///
/// Returns `Ok(None)` when the document has no block yet.
pub fn split(document: &str) -> Result<Option<Split>> {
    let begins: Vec<usize> = line_positions(document, BEGIN);
    let ends: Vec<usize> = line_positions(document, END);

    match (begins.len(), ends.len()) {
        (0, 0) => Ok(None),
        (1, 1) => {
            let b = begins[0];
            let e = ends[0];
            if e < b {
                return Err(Error::Block(
                    "the managed block's closing marker appears before its opening marker in \
                     hyprland.lua — fix or delete the markers and try again"
                        .into(),
                ));
            }
            let begin_line_end = line_end(document, b);
            let before = document[..b].to_string();
            let inner = document[begin_line_end..e].to_string();
            let after = document[line_end(document, e)..].to_string();
            Ok(Some(Split { before, inner, after }))
        }
        (0, _) | (_, 0) => Err(Error::Block(
            "hyprland.lua has one half of the managed block's markers but not the other — \
             remove the stray marker and try again"
                .into(),
        )),
        _ => Err(Error::Block(
            "hyprland.lua contains more than one HyprWindowShade managed block — remove the \
             extra one and try again"
                .into(),
        )),
    }
}

/// Byte offsets of every line that, trimmed, equals `needle`.
fn line_positions(document: &str, needle: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    for line in document.split_inclusive('\n') {
        if line.trim() == needle {
            out.push(offset);
        }
        offset += line.len();
    }
    out
}

/// Offset just past the end of the line starting at `start`.
fn line_end(document: &str, start: usize) -> usize {
    match document[start..].find('\n') {
        Some(i) => start + i + 1,
        None => document.len(),
    }
}

/// Replace (or append) the managed block in `document` with `block_body`.
///
/// `block_body` is the text that goes between the markers; the markers
/// themselves are added here.
pub fn splice(document: &str, block_body: &str) -> Result<String> {
    let body = block_body.trim_end_matches('\n');
    let block = format!("{BEGIN}\n{body}\n{END}\n");

    match split(document)? {
        Some(s) => Ok(format!("{}{}{}", s.before, block, s.after)),
        None => {
            let mut out = document.to_string();
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&block);
            Ok(out)
        }
    }
}

/// Remove the managed block entirely, leaving the rest of the file alone.
pub fn remove(document: &str) -> Result<String> {
    match split(document)? {
        Some(s) => {
            let mut before = s.before.to_string();
            // Take back exactly the one blank line `splice` inserted when it
            // appended the block. Anything beyond that is the user's own
            // spacing, and removing the block is not licence to reflow it.
            if before.ends_with("\n\n") {
                before.pop();
            }
            Ok(format!("{}{}", before, s.after))
        }
        None => Ok(document.to_string()),
    }
}

/// Pull the serialised [`Config`] out of a block body.
pub fn extract_state(inner: &str) -> Result<Option<Config>> {
    for line in inner.lines() {
        let line = line.trim();
        if let Some(json) = line.strip_prefix(STATE_PREFIX) {
            let cfg: Config = serde_json::from_str(json.trim())?;
            return Ok(Some(cfg));
        }
    }
    Ok(None)
}

/// Render the state line that [`extract_state`] reads back.
pub fn state_line(config: &Config) -> Result<String> {
    // Compact, single line: a Lua comment cannot span lines.
    Ok(format!("{STATE_PREFIX}{}", serde_json::to_string(config)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_when_absent() {
        let doc = "hl.bind(\"SUPER + Q\", ...)\n";
        let out = splice(doc, "-- body").unwrap();
        assert!(out.starts_with(doc));
        assert!(out.contains(BEGIN));
        assert!(out.contains("-- body"));
        assert!(out.trim_end().ends_with(END));
    }

    #[test]
    fn replace_when_present_leaves_surroundings_untouched() {
        let doc = format!("before\n{BEGIN}\nold\n{END}\nafter\n");
        let out = splice(&doc, "new").unwrap();
        assert_eq!(out, format!("before\n{BEGIN}\nnew\n{END}\nafter\n"));
    }

    #[test]
    fn splice_is_idempotent() {
        let doc = "x\n";
        let once = splice(doc, "body").unwrap();
        let twice = splice(&once, "body").unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn remove_restores_the_original() {
        let doc = "before\nstuff\n";
        let with = splice(doc, "body").unwrap();
        assert_eq!(remove(&with).unwrap(), doc);
    }

    #[test]
    fn remove_keeps_blank_lines_the_user_wrote() {
        // Deliberate spacing above the block is the user's, not ours.
        let doc = "before\n\n\n";
        let with = splice(doc, "body").unwrap();
        assert_eq!(remove(&with).unwrap(), doc);
    }

    #[test]
    fn remove_restores_a_hand_placed_block_without_eating_the_file() {
        let doc = format!("a\n\n\n\n{BEGIN}\nbody\n{END}\nb\n");
        // One blank line goes, because that is the one splice would have added;
        // the rest of the run stays.
        assert_eq!(remove(&doc).unwrap(), "a\n\n\nb\n");
    }

    #[test]
    fn duplicate_blocks_are_refused() {
        let doc = format!("{BEGIN}\na\n{END}\n{BEGIN}\nb\n{END}\n");
        assert!(splice(&doc, "x").is_err());
    }

    #[test]
    fn half_a_block_is_refused() {
        let doc = format!("{BEGIN}\na\n");
        assert!(split(&doc).is_err());
    }

    #[test]
    fn inverted_markers_are_refused() {
        let doc = format!("{END}\na\n{BEGIN}\n");
        assert!(split(&doc).is_err());
    }

    #[test]
    fn state_round_trips() {
        let mut cfg = Config::default();
        cfg.theme = "gruvbox-light".into();
        let line = state_line(&cfg).unwrap();
        let back = extract_state(&format!("stuff\n{line}\nmore\n")).unwrap().unwrap();
        assert_eq!(back.theme, "gruvbox-light");
    }

    #[test]
    fn missing_state_is_not_an_error() {
        assert!(extract_state("-- nothing here\n").unwrap().is_none());
    }
}
