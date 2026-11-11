//! What the preview should look like, taken from the running compositor.
//!
//! The preview is only worth looking at if the window in it resembles the
//! user's own windows: a shader that tints a corner reads differently against
//! 0px rounding than against 16px, and an open animation is the shader and the
//! compositor's own motion together.
//!
//! Rather than parse the user's config — which may be Lua, may be `.conf`, and
//! is none of this app's business — the look is asked of the running instance
//! with `hyprctl`. That works whatever the config is written in, and it
//! reflects anything changed at runtime too. With Hyprland not running there is
//! nothing to preview in the first place, so the defaults here are only a
//! fallback for options that cannot be read.

use serde::Deserialize;

use crate::hyprctl;

/// One easing curve, as `hl.curve` takes it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Curve {
    /// The name animations refer to.
    pub name: String,
    /// First control point.
    #[serde(rename = "X0")]
    pub x0: f32,
    /// First control point.
    #[serde(rename = "Y0")]
    pub y0: f32,
    /// Second control point.
    #[serde(rename = "X1")]
    pub x1: f32,
    /// Second control point.
    #[serde(rename = "Y1")]
    pub y1: f32,
}

/// One animation leaf, as `hl.animation` takes it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Animation {
    /// The leaf name, e.g. `windowsIn`.
    pub name: String,
    /// Whether the user set this one themselves.
    #[serde(default)]
    pub overridden: bool,
    /// Whether it runs at all.
    #[serde(default)]
    pub enabled: bool,
    /// Speed in deciseconds.
    #[serde(default)]
    pub speed: f32,
    /// Named curve, or empty.
    #[serde(default)]
    pub bezier: String,
    /// Style string, or empty.
    #[serde(default)]
    pub style: String,
}

/// The compositor settings the preview mirrors.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    /// `general:gaps_in`, as top, right, bottom, left.
    pub gaps_in: [i64; 4],
    /// `general:border_size`.
    pub border_size: i64,
    /// `decoration:rounding`.
    pub rounding: i64,
    /// `decoration:active_opacity`.
    pub active_opacity: f32,
    /// `decoration:inactive_opacity`.
    pub inactive_opacity: f32,
    /// `decoration:blur:enabled`.
    pub blur: bool,
    /// `animations:enabled`.
    pub animations: bool,
    /// Curves worth carrying over, in declaration order.
    pub curves: Vec<Curve>,
    /// Animation leaves the user has overridden.
    pub animation_leaves: Vec<Animation>,
}

impl Default for Look {
    fn default() -> Self {
        // Hyprland's own defaults, so a preview on a machine that cannot be
        // asked still looks like a stock desktop rather than like nothing.
        Self {
            gaps_in: [5; 4],
            border_size: 2,
            rounding: 0,
            active_opacity: 1.0,
            inactive_opacity: 1.0,
            blur: true,
            animations: true,
            curves: Vec::new(),
            animation_leaves: Vec::new(),
        }
    }
}

impl Look {
    /// Ask the running compositor how it draws windows.
    ///
    /// Every option is read on its own and falls back to the default, because
    /// Hyprland renames options between versions and one missing key is no
    /// reason to show the user nothing.
    pub fn from_host() -> Self {
        let d = Look::default();
        let mut look = Look {
            gaps_in: hyprctl::option_sides("general:gaps_in").unwrap_or(d.gaps_in),
            border_size: hyprctl::option_int("general:border_size").unwrap_or(d.border_size),
            rounding: hyprctl::option_int("decoration:rounding").unwrap_or(d.rounding),
            active_opacity: hyprctl::option_float("decoration:active_opacity")
                .unwrap_or(d.active_opacity),
            inactive_opacity: hyprctl::option_float("decoration:inactive_opacity")
                .unwrap_or(d.inactive_opacity),
            blur: hyprctl::option_bool("decoration:blur:enabled").unwrap_or(d.blur),
            animations: hyprctl::option_bool("animations:enabled").unwrap_or(d.animations),
            ..d
        };

        if let Ok((animations, curves)) = hyprctl::animations() {
            // Only the leaves the user actually set: writing all thirty-five
            // back would bake this version's defaults into the preview and
            // drift from the compositor at the next release.
            look.animation_leaves = animations.into_iter().filter(|a| a.overridden).collect();
            look.curves = curves;
        }
        look
    }

    /// Drop curves no carried-over animation refers to.
    ///
    /// `hyprctl` reports every curve the config declared, including ones only
    /// used by leaves the preview does not copy.
    pub fn prune_curves(&mut self) {
        self.curves.retain(|c| self.animation_leaves.iter().any(|a| a.bezier == c.name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unused_curves_are_dropped() {
        let mut look = Look {
            curves: vec![
                Curve { name: "used".into(), x0: 0.1, y0: 0.2, x1: 0.3, y1: 0.4 },
                Curve { name: "spare".into(), x0: 0.0, y0: 0.0, x1: 1.0, y1: 1.0 },
            ],
            animation_leaves: vec![Animation {
                name: "windowsIn".into(),
                overridden: true,
                enabled: true,
                speed: 2.0,
                bezier: "used".into(),
                style: "slidefade".into(),
            }],
            ..Look::default()
        };
        look.prune_curves();

        assert_eq!(look.curves.len(), 1);
        assert_eq!(look.curves[0].name, "used");
    }

    #[test]
    fn the_default_look_is_a_stock_desktop() {
        let d = Look::default();
        assert!(d.animations);
        assert!(d.curves.is_empty(), "no curves means Hyprland's own defaults apply");
    }
}
