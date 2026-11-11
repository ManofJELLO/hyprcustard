//! Writing values back into a `.glsl` file.
//!
//! Every edit is a surgical replacement on one line. Indentation, trailing
//! comments, line endings and the rest of the file are preserved, because the
//! file belongs to the user and the app is only adjusting numbers in it.
//!
//! The plugin re-reads a shader when its mtime changes, so saving here is what
//! makes a slider feel live.

use std::sync::OnceLock;

use regex::Regex;

use crate::error::{Error, Result};
use crate::shader::meta::{render_literal, ParamValue};

fn re_const_named(name: &str) -> Regex {
    Regex::new(&format!(
        r"(?x)
        ^(?P<head>\s*const\s+
          (?:highp\s+|mediump\s+|lowp\s+)?
          (?P<ty>float|int|bool|vec2|vec3|vec4|ivec2|ivec3|ivec4)\s+
          {}\s*=\s*)
        (?P<lit>.+?)
        (?P<tail>\s*;.*)$",
        regex::escape(name)
    ))
    .expect("const regex")
}

fn re_duration() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"^(?P<head>\s*//\s*@duration\s+)(?P<val>\S+)(?P<tail>.*)$")
            .expect("duration regex")
    })
}

fn re_overlay() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^\s*//\s*@overlay\s*$").expect("overlay regex"))
}

/// Split a source file into lines that keep their terminators.
fn lines(source: &str) -> Vec<String> {
    source.split_inclusive('\n').map(String::from).collect()
}

fn terminator(line: &str) -> &str {
    if line.ends_with("\r\n") {
        "\r\n"
    } else if line.ends_with('\n') {
        "\n"
    } else {
        ""
    }
}

fn body(line: &str) -> &str {
    let t = terminator(line);
    &line[..line.len() - t.len()]
}

/// Replace the value of `const <ty> <name> = ...;`.
///
/// Returns the new source. Fails if the constant is missing or its declaration
/// does not have a plain literal on the right-hand side.
pub fn set_param(source: &str, name: &str, value: &ParamValue) -> Result<String> {
    let re = re_const_named(name);
    let mut out = lines(source);
    let mut hits = 0usize;

    for line in out.iter_mut() {
        let term = terminator(line).to_string();
        let text = body(line).to_string();
        if let Some(caps) = re.captures(&text) {
            let ty = caps.name("ty").expect("ty group").as_str();
            let replacement = render_literal(ty, value);
            let head = caps.name("head").expect("head group").as_str();
            let tail = caps.name("tail").expect("tail group").as_str();
            *line = format!("{head}{replacement}{tail}{term}");
            hits += 1;
        }
    }

    match hits {
        0 => Err(Error::Shader(format!(
            "no editable `const … {name} = …;` line found in this shader"
        ))),
        1 => Ok(out.concat()),
        n => Err(Error::Shader(format!(
            "`{name}` is declared {n} times in this shader; edit it by hand so there is only one"
        ))),
    }
}

/// Set, change or remove the `// @duration` directive.
///
/// `None` removes it. A new directive is inserted after the last leading
/// preprocessor or `precision` line, which is where shaders in the wild put it.
pub fn set_duration(source: &str, seconds: Option<f32>) -> String {
    let mut out = lines(source);
    let existing = out.iter().position(|l| re_duration().is_match(body(l)));

    match (existing, seconds) {
        (Some(i), Some(v)) => {
            let term = terminator(&out[i]).to_string();
            let text = body(&out[i]).to_string();
            let caps = re_duration().captures(&text).expect("matched above");
            let head = caps.name("head").expect("head group").as_str();
            let tail = caps.name("tail").expect("tail group").as_str();
            out[i] = format!("{head}{}{tail}{term}", crate::model::trim_float(v.clamp(0.0, 5.0)));
        }
        (Some(i), None) => {
            out.remove(i);
        }
        (None, Some(v)) => {
            let at = insertion_point(&out);
            let term =
                if out.is_empty() { "\n".to_string() } else { terminator(&out[0]).to_string() };
            let term = if term.is_empty() { "\n".to_string() } else { term };
            out.insert(
                at,
                format!("// @duration {}{term}", crate::model::trim_float(v.clamp(0.0, 5.0))),
            );
        }
        (None, None) => {}
    }

    out.concat()
}

/// Add or remove the `// @overlay` directive.
pub fn set_overlay(source: &str, overlay: bool) -> String {
    let mut out = lines(source);
    let existing = out.iter().position(|l| re_overlay().is_match(body(l)));

    match (existing, overlay) {
        (Some(_), true) | (None, false) => {}
        (Some(i), false) => {
            out.remove(i);
        }
        (None, true) => {
            let at = insertion_point(&out);
            let term = out.first().map(|l| terminator(l)).filter(|t| !t.is_empty()).unwrap_or("\n");
            out.insert(at, format!("// @overlay{term}"));
        }
    }

    out.concat()
}

/// Where a new directive goes: after `#version`, `precision` and any directives
/// already there, before the first real declaration.
fn insertion_point(lines: &[String]) -> usize {
    let mut at = 0usize;
    for (i, line) in lines.iter().enumerate() {
        let t = body(line).trim();
        if t.is_empty() || t.starts_with('#') || t.starts_with("precision") || t.starts_with("//") {
            at = i + 1;
            continue;
        }
        break;
    }
    at
}

/// Back up a shader, then write the new source atomically.
pub fn save(path: &std::path::Path, source: &str, backups_to_keep: usize) -> Result<()> {
    crate::paths::backup(path, backups_to_keep)?;
    crate::paths::write_atomic(path, source)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "#version 320 es\nprecision highp float;\n\n\
                       // @duration 0.4\nconst float DIM = 0.6;  // keep me\n\
                       const vec3 TINT = vec3(1.0, 0.9, 0.8);\n\
                       void main() {}\n";

    #[test]
    fn scalar_edit_preserves_the_rest_of_the_line() {
        let out = set_param(SRC, "DIM", &ParamValue::Scalar(0.25)).unwrap();
        assert!(out.contains("const float DIM = 0.25;  // keep me\n"));
        assert!(out.contains("void main() {}"));
    }

    #[test]
    fn vector_edit_rewrites_all_components() {
        let out = set_param(SRC, "TINT", &ParamValue::Vector(vec![0.1, 0.2, 0.3])).unwrap();
        assert!(out.contains("const vec3 TINT = vec3(0.1, 0.2, 0.3);"));
    }

    #[test]
    fn a_missing_const_is_an_error_not_a_silent_no_op() {
        assert!(set_param(SRC, "NOPE", &ParamValue::Scalar(1.0)).is_err());
    }

    #[test]
    fn nothing_else_in_the_file_moves() {
        let out = set_param(SRC, "DIM", &ParamValue::Scalar(0.25)).unwrap();
        assert_eq!(out.lines().count(), SRC.lines().count());
    }

    #[test]
    fn duration_is_updated_in_place() {
        let out = set_duration(SRC, Some(1.25));
        assert!(out.contains("// @duration 1.25\n"));
        assert_eq!(out.matches("@duration").count(), 1);
    }

    #[test]
    fn duration_is_clamped_to_the_plugin_cap() {
        let out = set_duration(SRC, Some(30.0));
        assert!(out.contains("// @duration 5\n"));
    }

    #[test]
    fn duration_can_be_removed_and_added_back() {
        let without = set_duration(SRC, None);
        assert!(!without.contains("@duration"));
        let back = set_duration(&without, Some(0.5));
        assert!(back.contains("// @duration 0.5"));
        // It lands in the header, not in the middle of the code.
        let idx = back.lines().position(|l| l.contains("@duration")).unwrap();
        let main = back.lines().position(|l| l.contains("void main")).unwrap();
        assert!(idx < main);
    }

    #[test]
    fn overlay_toggles() {
        let on = set_overlay(SRC, true);
        assert!(on.contains("// @overlay"));
        let off = set_overlay(&on, false);
        assert_eq!(off, SRC);
    }

    #[test]
    fn crlf_files_keep_their_line_endings() {
        let src = "#version 320 es\r\nconst float A = 1.0;\r\n";
        let out = set_param(src, "A", &ParamValue::Scalar(2.0)).unwrap();
        assert_eq!(out, "#version 320 es\r\nconst float A = 2.0;\r\n");
    }

    #[test]
    fn a_file_without_a_trailing_newline_stays_that_way() {
        let src = "const float A = 1.0;";
        let out = set_param(src, "A", &ParamValue::Scalar(2.0)).unwrap();
        assert_eq!(out, "const float A = 2.0;");
    }

    #[test]
    fn a_duplicate_declaration_is_refused() {
        let src = "const float A = 1.0;\nconst float A = 2.0;\n";
        assert!(set_param(src, "A", &ParamValue::Scalar(3.0)).is_err());
    }

    #[test]
    fn a_similar_name_is_not_matched() {
        let src = "const float A = 1.0;\nconst float AB = 2.0;\n";
        let out = set_param(src, "A", &ParamValue::Scalar(9.0)).unwrap();
        assert!(out.contains("const float A = 9.0;"));
        assert!(out.contains("const float AB = 2.0;"));
    }
}
