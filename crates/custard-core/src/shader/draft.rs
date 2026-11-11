//! Shader edits held in memory until the user writes them out.
//!
//! A `const` in a `.glsl` is the plugin's only tuning channel, so editing one
//! means rewriting the user's file. Doing that on every slider release — which
//! is what this app used to do — makes a tuning session a stream of writes and
//! backups, and leaves nothing to undo with: the file *is* the state.
//!
//! So an edit is staged here instead. A draft holds the pending value of each
//! parameter, and of `// @duration` and `// @overlay`, against the file as it
//! was last read. Nothing reaches the disk until [`ShaderDraft::apply`] is
//! handed the source and the result is written.
//!
//! Staging the value a file already has is not an edit, so [`ShaderDraft::set_param`]
//! and its neighbours drop the entry instead of recording it. Dragging a slider
//! away and back leaves the shader clean, which is what makes "unsaved" mean
//! something.

use std::collections::BTreeMap;

use crate::error::Result;
use crate::shader::meta::{ParamValue, ShaderInfo};
use crate::shader::patch;

/// Floats here have been through a slider, a JSON round trip and GLSL's own
/// decimal formatting, so "the same value" cannot mean bit equality.
const EPSILON: f32 = 1e-6;

fn same_scalar(a: f32, b: f32) -> bool {
    (a - b).abs() <= EPSILON
}

fn same_value(a: &ParamValue, b: &ParamValue) -> bool {
    let (a, b) = (a.components(), b.components());
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| same_scalar(*x, *y))
}

fn same_duration(a: Option<f32>, b: Option<f32>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => same_scalar(x, y),
        _ => false,
    }
}

/// Which part of a shader a revert applies to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftItem {
    /// One `const`, by name.
    Param(String),
    /// The `// @duration` directive.
    Duration,
    /// The `// @overlay` directive.
    Overlay,
    /// Everything staged for this shader.
    All,
}

/// Edits staged against one shader file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShaderDraft {
    /// Pending parameter values, by `const` name.
    params: BTreeMap<String, ParamValue>,
    /// Pending `// @duration`. `Some(None)` removes the directive.
    duration: Option<Option<f32>>,
    /// Pending `// @overlay`.
    overlay: Option<bool>,
}

impl ShaderDraft {
    /// True when nothing is staged.
    pub fn is_empty(&self) -> bool {
        self.params.is_empty() && self.duration.is_none() && self.overlay.is_none()
    }

    /// How many separate edits are staged, for a count in the UI.
    pub fn len(&self) -> usize {
        self.params.len()
            + usize::from(self.duration.is_some())
            + usize::from(self.overlay.is_some())
    }

    /// Stage a parameter value, or drop the entry when it matches the file.
    pub fn set_param(&mut self, info: &ShaderInfo, name: &str, value: ParamValue) {
        match info.params.iter().find(|p| p.name == name) {
            Some(p) if same_value(&p.value, &value) => {
                self.params.remove(name);
            }
            _ => {
                self.params.insert(name.to_string(), value);
            }
        }
    }

    /// Stage a `// @duration`, or drop the entry when it matches the file.
    pub fn set_duration(&mut self, info: &ShaderInfo, seconds: Option<f32>) {
        if same_duration(info.duration, seconds) {
            self.duration = None;
        } else {
            self.duration = Some(seconds);
        }
    }

    /// Stage a `// @overlay`, or drop the entry when it matches the file.
    pub fn set_overlay(&mut self, info: &ShaderInfo, overlay: bool) {
        if info.overlay == overlay {
            self.overlay = None;
        } else {
            self.overlay = Some(overlay);
        }
    }

    /// Throw away one staged edit, or all of them.
    pub fn revert(&mut self, item: &DraftItem) {
        match item {
            DraftItem::Param(name) => {
                self.params.remove(name);
            }
            DraftItem::Duration => self.duration = None,
            DraftItem::Overlay => self.overlay = None,
            DraftItem::All => *self = Self::default(),
        }
    }

    /// The staged value of one parameter, if there is one.
    pub fn param(&self, name: &str) -> Option<&ParamValue> {
        self.params.get(name)
    }

    /// The `// @duration` this draft would write, given the file's own.
    pub fn duration_over(&self, saved: Option<f32>) -> Option<f32> {
        self.duration.unwrap_or(saved)
    }

    /// The `// @overlay` this draft would write, given the file's own.
    pub fn overlay_over(&self, saved: bool) -> bool {
        self.overlay.unwrap_or(saved)
    }

    /// True when `// @duration` is staged.
    pub fn duration_staged(&self) -> bool {
        self.duration.is_some()
    }

    /// True when `// @overlay` is staged.
    pub fn overlay_staged(&self) -> bool {
        self.overlay.is_some()
    }

    /// Apply every staged edit to a shader's source.
    ///
    /// Parameters are replaced by name, so the order edits are applied in does
    /// not matter even though the directives can insert a line above them.
    pub fn apply(&self, source: &str) -> Result<String> {
        let mut out = source.to_string();
        for (name, value) in &self.params {
            out = patch::set_param(&out, name, value)?;
        }
        if let Some(seconds) = self.duration {
            out = patch::set_duration(&out, seconds);
        }
        if let Some(overlay) = self.overlay {
            out = patch::set_overlay(&out, overlay);
        }
        Ok(out)
    }

    /// Drop edits the file has caught up with.
    ///
    /// A shader can change under the app — the user has an editor open, or the
    /// draft has just been written out — and an edit that matches what is on
    /// disk is no longer an edit. A parameter that has gone from the file
    /// altogether goes with it, because there is no longer a line to write it
    /// to.
    pub fn prune(&mut self, info: &ShaderInfo) {
        self.params.retain(|name, value| match info.params.iter().find(|p| p.name == *name) {
            Some(p) => !same_value(&p.value, value),
            None => false,
        });
        if self.duration.is_some_and(|d| same_duration(info.duration, d)) {
            self.duration = None;
        }
        if self.overlay.is_some_and(|o| info.overlay == o) {
            self.overlay = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shader::meta;

    const SRC: &str = "#version 320 es\n\
                       // @duration 0.4\n\
                       // @param 0 1 0.01 \"Dim\"\n\
                       const float DIM = 0.6;\n\
                       const vec3 TINT = vec3(1.0, 0.9, 0.8);\n\
                       void main() {}\n";

    fn info() -> ShaderInfo {
        meta::parse("/tmp/x.glsl", SRC, 0)
    }

    #[test]
    fn staging_a_value_leaves_the_source_alone_until_applied() {
        let mut d = ShaderDraft::default();
        d.set_param(&info(), "DIM", ParamValue::Scalar(0.25));
        assert!(!d.is_empty());

        let out = d.apply(SRC).unwrap();
        assert!(out.contains("const float DIM = 0.25;"));
        // The draft is the only thing that changed; SRC is untouched.
        assert!(SRC.contains("const float DIM = 0.6;"));
    }

    #[test]
    fn staging_the_value_the_file_already_has_is_not_an_edit() {
        let mut d = ShaderDraft::default();
        d.set_param(&info(), "DIM", ParamValue::Scalar(0.25));
        d.set_param(&info(), "DIM", ParamValue::Scalar(0.6));
        assert!(d.is_empty(), "dragging back to the saved value should leave nothing staged");
    }

    #[test]
    fn reverting_one_parameter_keeps_the_others() {
        let i = info();
        let mut d = ShaderDraft::default();
        d.set_param(&i, "DIM", ParamValue::Scalar(0.25));
        d.set_param(&i, "TINT", ParamValue::Vector(vec![0.1, 0.2, 0.3]));
        d.revert(&DraftItem::Param("DIM".into()));

        assert_eq!(d.len(), 1);
        let out = d.apply(SRC).unwrap();
        assert!(out.contains("const float DIM = 0.6;"));
        assert!(out.contains("const vec3 TINT = vec3(0.1, 0.2, 0.3);"));
    }

    #[test]
    fn several_edits_apply_together() {
        let i = info();
        let mut d = ShaderDraft::default();
        d.set_param(&i, "DIM", ParamValue::Scalar(0.25));
        d.set_duration(&i, Some(1.5));
        d.set_overlay(&i, true);
        assert_eq!(d.len(), 3);

        let out = d.apply(SRC).unwrap();
        assert!(out.contains("const float DIM = 0.25;"));
        assert!(out.contains("// @duration 1.5"));
        assert!(out.contains("// @overlay"));
    }

    #[test]
    fn a_missing_const_fails_on_apply_rather_than_writing_half_of_it() {
        let mut d = ShaderDraft::default();
        d.params.insert("NOPE".into(), ParamValue::Scalar(1.0));
        d.params.insert("DIM".into(), ParamValue::Scalar(0.25));
        assert!(d.apply(SRC).is_err());
    }

    #[test]
    fn toggling_a_directive_back_clears_the_entry() {
        let i = info();
        let mut d = ShaderDraft::default();
        d.set_overlay(&i, true);
        assert!(d.overlay_staged());
        d.set_overlay(&i, false);
        assert!(!d.overlay_staged());

        d.set_duration(&i, None);
        assert!(d.duration_staged());
        d.set_duration(&i, Some(0.4));
        assert!(!d.duration_staged());
    }

    #[test]
    fn pruning_drops_what_the_file_has_caught_up_with() {
        let i = info();
        let mut d = ShaderDraft::default();
        d.set_param(&i, "DIM", ParamValue::Scalar(0.25));
        d.set_param(&i, "TINT", ParamValue::Vector(vec![0.1, 0.2, 0.3]));

        // The file now says what the DIM edit was going to say, and TINT is gone.
        let after = meta::parse("/tmp/x.glsl", "const float DIM = 0.25;\nvoid main() {}\n", 0);
        d.prune(&after);
        assert!(d.is_empty());
    }
}
