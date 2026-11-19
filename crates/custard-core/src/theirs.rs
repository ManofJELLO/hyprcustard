//! The settings the user wrote into their own `hl.config({...})` tables,
//! found by key, so that a setting the look block takes over can be
//! commented out where they wrote it rather than set twice.
//!
//! Only literal tables passed straight to `hl.config` are read. A table built
//! in a variable, or a key written as `["x"] = ...`, is left alone: the app
//! cannot be sure what it sets without running the file.

use std::collections::BTreeSet;

use serde_json::{json, Value};

/// One `key = value` in the user's own `hl.config` tables.
#[derive(Debug, Clone, PartialEq)]
pub struct HandValue {
    /// The option's full path, as the look block names it:
    /// `decoration.shadow.range`.
    pub key: String,
    /// The entry as written, value and all.
    pub text: String,
    /// Its first line, counting from 1.
    pub line: usize,
    /// Whether it has its lines to itself and can be commented out without
    /// leaving the table broken.
    pub alone: bool,
    /// Byte offsets of the lines it stands on, newline excluded.
    lines: (usize, usize),
}

impl HandValue {
    /// The entry as the page lists it.
    pub fn view(&self, comment_out: bool) -> Value {
        json!({
            "line": self.line,
            "text": self.text,
            "alone": self.alone,
            "commentOut": comment_out,
        })
    }
}

/// Every `key = value` in the user's own `hl.config` tables, nested tables
/// included, in file order. Comments and the app's blocks do not count.
pub fn hand_values(document: &str) -> Vec<HandValue> {
    let outside = crate::rules::outside_blocks(document);
    let code = crate::import::strip_comments(&outside);
    let mut out = Vec::new();
    for (at, end) in crate::import::call_spans(&code, "hl.config") {
        let Some(open) = code[at..end].find('{').map(|i| at + i) else { continue };
        entries(document, &code, open, "", &mut out);
    }
    out
}

/// Comment out the user's entries for `keys`, where each stands alone on its
/// lines, with a line saying where it went. Returns the new document and the
/// keys done, in file order.
pub fn comment_out(document: &str, keys: &BTreeSet<&str>) -> (String, Vec<String>) {
    let mut found: Vec<HandValue> = hand_values(document)
        .into_iter()
        .filter(|h| h.alone && keys.contains(h.key.as_str()))
        .collect();
    let done = found.iter().map(|h| h.key.clone()).collect();
    // From the end, so the offsets of the ones still to do stay right.
    found.sort_by_key(|h| std::cmp::Reverse(h.lines.0));
    let mut out = document.to_string();
    for h in found {
        let (start, end) = h.lines;
        let first = &document[start..end];
        let indent: String = first.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
        let mut block = format!("{indent}-- Moved into the HyprCustard look block:\n");
        for (i, line) in document[start..end].split('\n').enumerate() {
            if i > 0 {
                block.push('\n');
            }
            match line.strip_prefix(indent.as_str()) {
                Some(rest) => block.push_str(&format!("{indent}-- {rest}")),
                None => block.push_str(&format!("-- {line}")),
            }
        }
        out.replace_range(start..end, &block);
    }
    (out, done)
}

/// Read the entries of the table whose `{` is at `open`, recursing into
/// nested tables. `code` is `document` with comments and the app's blocks
/// blanked, byte for byte.
fn entries(document: &str, code: &str, open: usize, prefix: &str, out: &mut Vec<HandValue>) {
    let b = code.as_bytes();
    let Some(close) = match_close(b, open) else { return };
    let mut i = open + 1;
    while i < close {
        // Between entries: blanks and separators.
        if b[i].is_ascii_whitespace() || b[i] == b',' || b[i] == b';' {
            i += 1;
            continue;
        }
        let key_start = i;
        if b[i].is_ascii_alphabetic() || b[i] == b'_' {
            let mut j = i;
            while j < close && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                j += 1;
            }
            let key = &code[key_start..j];
            let mut k = j;
            while k < close && b[k].is_ascii_whitespace() {
                k += 1;
            }
            if k < close && b[k] == b'=' && b.get(k + 1) != Some(&b'=') {
                let mut v = k + 1;
                while v < close && b[v].is_ascii_whitespace() {
                    v += 1;
                }
                let path = format!("{prefix}{key}");
                let value_end = if b.get(v) == Some(&b'{') {
                    let Some(c) = match_close(b, v) else { return };
                    entries(document, code, v, &format!("{path}."), out);
                    c + 1
                } else {
                    value_end(b, v, close)
                };
                out.push(entry(document, code, path, key_start, value_end));
                i = value_end;
                continue;
            }
        }
        // Not `name = value`: a positional value or `[expr] = value`. Skip it.
        i = value_end(b, i, close).max(i + 1);
    }
}

/// The entry from `start` to `end`, and whether it can be commented out.
fn entry(document: &str, code: &str, key: String, start: usize, end: usize) -> HandValue {
    let line_start = code[..start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = code[end..].find('\n').map_or(code.len(), |i| end + i);
    // What follows on its last line: nothing but its own separator.
    let tail = code[end..line_end].trim();
    let own_separator = tail == "," || tail == ";";
    // The entry before it ends in a separator, or this is the first one; and
    // without a separator of its own, it is the last one — otherwise taking
    // it out would join its neighbors.
    let before = code[..start].trim_end().as_bytes().last().copied();
    let after = code[line_end..].trim_start().as_bytes().first().copied();
    let alone = code[line_start..start].trim().is_empty()
        && (tail.is_empty() || own_separator)
        && matches!(before, Some(b'{' | b',' | b';'))
        && (own_separator || after == Some(b'}'));
    HandValue {
        key,
        text: document[start..end].to_string(),
        line: document[..start].matches('\n').count() + 1,
        alone,
        lines: (line_start, line_end),
    }
}

/// Where a plain value starting at `from` ends: the first `,`, `;` or the
/// table's `}` outside any brackets or strings, with trailing blanks dropped.
fn value_end(b: &[u8], from: usize, close: usize) -> usize {
    let mut depth = 0i32;
    let mut i = from;
    while i < close {
        match b[i] {
            b'"' | b'\'' => i = skip_string(b, i),
            b'(' | b'[' | b'{' => {
                depth += 1;
                i += 1;
            }
            b')' | b']' | b'}' => {
                depth -= 1;
                i += 1;
            }
            b',' | b';' if depth <= 0 => break,
            _ => i += 1,
        }
    }
    let mut end = i.min(close);
    while end > from && b[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    end
}

/// Past the end of the string literal whose quote is at `at`.
fn skip_string(b: &[u8], at: usize) -> usize {
    let q = b[at];
    let mut i = at + 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'\n' => return i,
            c if c == q => return i + 1,
            _ => i += 1,
        }
    }
    b.len()
}

/// The `}` matching the `{` at `open`, skipping strings.
fn match_close(b: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut i = open;
    while i < b.len() {
        match b[i] {
            b'"' | b'\'' => {
                i = skip_string(b, i);
                continue;
            }
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINE: &str = "\
hl.config({
    general = {
        gaps_in = 4,
        col = {
            active_border   = \"rgba(EBDBB2FF)\",
            inactive_border = \"rgba(595959aa)\", -- dim
        },
    },
    decoration = {
        rounding     = 8,
        dim_strength = 0.5,
        shadow = {
            enabled = true,
            render_power = 4
        },
        -- dim_inactive = true,
        blur = { enabled = true, size = 6 },
    },
})
";

    fn keys(doc: &str) -> Vec<String> {
        hand_values(doc).into_iter().map(|h| h.key).collect()
    }

    #[test]
    fn every_entry_is_found_by_its_path() {
        let k = keys(MINE);
        for want in [
            "general.gaps_in",
            "general.col.active_border",
            "general.col.inactive_border",
            "decoration.dim_strength",
            "decoration.shadow.render_power",
            "decoration.blur.size",
        ] {
            assert!(k.iter().any(|x| x == want), "{want} not in {k:?}");
        }
        // Commented out is not set.
        assert!(!k.iter().any(|x| x == "decoration.dim_inactive"));
        let dim = hand_values(MINE).into_iter().find(|h| h.key == "decoration.dim_strength");
        assert_eq!(dim.unwrap().line, 11);
    }

    #[test]
    fn only_entries_with_their_own_lines_can_go() {
        let all = hand_values(MINE);
        let alone = |k: &str| all.iter().find(|h| h.key == k).unwrap().alone;
        assert!(alone("decoration.dim_strength"));
        // A trailing comment is not other code.
        assert!(alone("general.col.inactive_border"));
        // The last entry, without a comma.
        assert!(alone("decoration.shadow.render_power"));
        // Sharing a line with the rest of its table.
        assert!(!alone("decoration.blur.size"));
        assert!(!alone("decoration.blur.enabled"));
    }

    #[test]
    fn commenting_out_keeps_the_table_whole() {
        let taken: BTreeSet<&str> =
            ["decoration.dim_strength", "decoration.shadow.render_power", "decoration.blur.size"]
                .into();
        let (out, done) = comment_out(MINE, &taken);
        // blur.size shares its line, so it stays.
        assert_eq!(done, ["decoration.dim_strength", "decoration.shadow.render_power"]);
        assert!(out.contains(
            "        -- Moved into the HyprCustard look block:\n        -- dim_strength = 0.5,\n"
        ));
        assert!(out.contains("            -- render_power = 4\n        },"));
        assert!(out.contains("blur = { enabled = true, size = 6 },"));
        // Nothing left of them to read, and everything else still there.
        let after = keys(&out);
        assert!(!after.contains(&"decoration.dim_strength".to_string()));
        assert!(!after.contains(&"decoration.shadow.render_power".to_string()));
        assert!(after.contains(&"decoration.shadow.enabled".to_string()));
        assert_eq!(after.len(), keys(MINE).len() - 2);
    }

    #[test]
    fn a_table_entry_can_be_taken_over_whole() {
        let doc = "hl.config({\n    general = {\n        col = {\n            active_border = {\n                colors = { \"rgba(ffffffff)\" },\n                angle = 45,\n            },\n        },\n    },\n})\n";
        let (out, done) = comment_out(doc, &["general.col.active_border"].into());
        assert_eq!(done, ["general.col.active_border"]);
        assert!(out.contains("            -- active_border = {\n            --     colors"));
        assert!(out.contains("            -- },\n        },"));
        assert!(keys(&out).iter().all(|k| !k.starts_with("general.col.active_border")));
    }

    #[test]
    fn neighbors_joined_by_a_leading_comma_are_left_alone() {
        let doc = "hl.config({ general = {\n    gaps_in = 4\n    , gaps_out = 8\n} })\n";
        let all = hand_values(doc);
        assert!(!all.iter().find(|h| h.key == "general.gaps_in").unwrap().alone);
        assert!(!all.iter().find(|h| h.key == "general.gaps_out").unwrap().alone);
    }

    #[test]
    fn the_look_block_and_other_calls_do_not_count() {
        let doc = format!(
            "hl.bind(\"SUPER + Q\", hl.dsp.exec_cmd(\"x\"))\n{}\nhl.config({{\n    general = {{ gaps_in = 2 }},\n}})\n{}\n",
            crate::look::MARKERS.begin,
            crate::look::MARKERS.end
        );
        assert!(hand_values(&doc).is_empty());
    }

    /// Read-only: what the reader finds in a real config, given by path.
    #[test]
    #[ignore = "reads CUSTARD_THEIRS_FILE"]
    fn dump_a_real_config() {
        let doc = std::fs::read_to_string(std::env::var("CUSTARD_THEIRS_FILE").unwrap()).unwrap();
        for h in hand_values(&doc) {
            println!("{:4} {:5} {}", h.line, h.alone, h.key);
        }
    }

    #[test]
    fn a_string_holding_braces_does_not_confuse_the_reader() {
        let doc = "hl.config({\n    misc = {\n        font_family = \"a}b{,c\",\n        vrr = 1,\n    },\n})\n";
        let k = keys(doc);
        assert_eq!(k, ["misc.font_family", "misc.vrr", "misc"]);
    }
}
