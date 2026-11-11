//! Asking the running compositor what is on screen.
//!
//! Everything here is best-effort: the app has to work with Hyprland not
//! running, so a failure is a message, never a crash.

use std::collections::BTreeSet;
use std::process::Command;

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::model::Action;

/// True when a Hyprland instance appears to be running for this user.
pub fn is_running() -> bool {
    std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok()
}

fn run(args: &[&str]) -> Result<String> {
    let out = Command::new("hyprctl").args(args).output().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => {
            Error::Hyprctl("hyprctl is not on PATH — is Hyprland installed?".into())
        }
        _ => Error::Hyprctl(e.to_string()),
    })?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(Error::Hyprctl(if err.is_empty() {
            format!("`hyprctl {}` failed", args.join(" "))
        } else {
            err
        }));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

// ---------------------------------------------------------------------------
// Clients
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawClient {
    #[serde(default)]
    class: String,
    #[serde(default)]
    title: String,
    #[serde(default, rename = "initialClass")]
    initial_class: String,
}

/// An open window, reduced to what the rule editor needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    /// Window class.
    pub class: String,
    /// Window title.
    pub title: String,
    /// Initial class, which is what a rule usually wants to match.
    pub initial_class: String,
}

/// List open windows.
pub fn clients() -> Result<Vec<Client>> {
    let json = run(&["-j", "clients"])?;
    let raw: Vec<RawClient> =
        serde_json::from_str(&json).map_err(|e| Error::Hyprctl(format!("clients: {e}")))?;
    Ok(raw
        .into_iter()
        .map(|c| Client { class: c.class, title: c.title, initial_class: c.initial_class })
        .collect())
}

/// Distinct window classes currently on screen, sorted.
pub fn classes() -> Result<Vec<String>> {
    let set: BTreeSet<String> = clients()?
        .into_iter()
        .flat_map(|c| [c.class, c.initial_class])
        .filter(|s| !s.is_empty())
        .collect();
    Ok(set.into_iter().collect())
}

// ---------------------------------------------------------------------------
// Layers
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawMonitorLayers {
    #[serde(default)]
    levels: std::collections::HashMap<String, Vec<RawLayer>>,
}

#[derive(Debug, Deserialize)]
struct RawLayer {
    #[serde(default)]
    namespace: String,
}

/// Distinct layer namespaces currently mapped, sorted.
///
/// These are what `layershader` and the layer animations take.
pub fn layer_namespaces() -> Result<Vec<String>> {
    let json = run(&["-j", "layers"])?;
    let raw: std::collections::HashMap<String, RawMonitorLayers> =
        serde_json::from_str(&json).map_err(|e| Error::Hyprctl(format!("layers: {e}")))?;

    let set: BTreeSet<String> = raw
        .into_values()
        .flat_map(|m| m.levels.into_values())
        .flatten()
        .map(|l| l.namespace)
        .filter(|s| !s.is_empty())
        .collect();
    Ok(set.into_iter().collect())
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

/// The complaint `hl.dispatch` makes about a call that returned nothing.
///
/// The plugin's functions all return nothing, so every one of them trips this
/// *after* having already done its work. It is the expected outcome here, not a
/// failure.
const NOT_A_DISPATCHER: &str = "expected a dispatcher";

/// Run a plugin action against the live session.
///
/// `hyprctl dispatch` evaluates its argument as Lua and reports the result on
/// **stdout**, not stderr; its exit status is 7 for every plugin call whether
/// or not the call worked, because `hl.dispatch` rejects the nil return. So
/// stdout is the only thing that distinguishes a real failure — a bad path or
/// malformed Lua — from the harmless complaint about the return value.
pub fn dispatch(action: &Action) -> Result<()> {
    let expr = action.to_lua_call("hl.plugin.HyprWindowShade");
    let out =
        Command::new("hyprctl").args(["dispatch", &expr]).output().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                Error::Hyprctl("hyprctl is not on PATH — is Hyprland installed?".into())
            }
            _ => Error::Hyprctl(e.to_string()),
        })?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    check_dispatch_output(&stdout, &stderr)
}

/// Decide whether a `hyprctl dispatch` run actually failed.
///
/// Split out from [`dispatch`] so it can be tested against real hyprctl output
/// without a compositor.
fn check_dispatch_output(stdout: &str, stderr: &str) -> Result<()> {
    // stderr first: if hyprctl itself could not run the request (no instance,
    // a socket problem) that is where it says so.
    let complaint = [stderr.trim(), stdout.trim()].into_iter().find(|s| !s.is_empty());

    match complaint {
        None => Ok(()),
        // The known false alarm: the call ran, it just returned nothing.
        Some(s) if s.contains(NOT_A_DISPATCHER) => Ok(()),
        // A successful dispatcher answers "ok".
        Some("ok") => Ok(()),
        Some(s) => Err(Error::Hyprctl(s.to_string())),
    }
}

/// Ask the plugin to drop its compiled shader cache.
pub fn reload_shaders() -> Result<()> {
    dispatch(&Action::ReloadShaders)
}

/// Ask Hyprland to re-read its config.
pub fn reload_config() -> Result<()> {
    run(&["reload"]).map(|_| ())
}

/// Whether the plugin reports itself as loaded.
pub fn plugin_loaded() -> Result<bool> {
    let json = run(&["-j", "plugin", "list"])?;
    Ok(json.contains("HyprWindowShade"))
}

// ---------------------------------------------------------------------------
// Monitors
// ---------------------------------------------------------------------------

/// One output, reduced to what the preview needs to photograph the right one.
#[derive(Debug, Clone, Deserialize)]
pub struct Monitor {
    /// Connector name, e.g. `DP-1`.
    pub name: String,
    /// Whether this is the one the user is looking at.
    #[serde(default)]
    pub focused: bool,
    /// Where its top-left corner sits in the layout.
    #[serde(default)]
    pub x: i32,
    /// Where its top-left corner sits in the layout.
    #[serde(default)]
    pub y: i32,
    /// Pixel width.
    #[serde(default)]
    pub width: u32,
    /// Pixel height.
    #[serde(default)]
    pub height: u32,
}

/// Run a `hyprctl` command against the user's own session.
///
/// The preview reaches for this to borrow a scratch output for a moment; most
/// things here have a typed wrapper instead.
pub fn run_args(args: &[&str]) -> Result<String> {
    run(args)
}

/// Every output the compositor is driving.
pub fn monitors() -> Result<Vec<Monitor>> {
    let out = run(&["monitors", "-j"])?;
    serde_json::from_str(&out)
        .map_err(|e| Error::Hyprctl(format!("could not read the monitor list: {e}")))
}

// ---------------------------------------------------------------------------
// Instances, for the preview
// ---------------------------------------------------------------------------

/// One running Hyprland, as `hyprctl instances -j` reports it.
#[derive(Debug, Clone, Deserialize)]
pub struct Instance {
    /// The instance signature, which `-i` takes.
    pub instance: String,
    /// The Wayland socket clients connect to.
    pub wl_socket: String,
    /// The compositor's process id.
    #[serde(default)]
    pub pid: i64,
}

/// Every Hyprland instance running for this user.
pub fn instances() -> Result<Vec<Instance>> {
    let out = run(&["instances", "-j"])?;
    serde_json::from_str(&out)
        .map_err(|e| Error::Hyprctl(format!("could not read the instance list: {e}")))
}

/// Run a command against one instance rather than the session's own.
///
/// The preview drives a second compositor, and every call meant for it has to
/// say so — otherwise it lands on the user's real desktop.
pub fn on_instance(signature: &str, args: &[&str]) -> Result<String> {
    let mut full = vec!["-i", signature];
    full.extend_from_slice(args);
    run(&full)
}

// ---------------------------------------------------------------------------
// Options, for the preview
// ---------------------------------------------------------------------------

/// One option, as `hyprctl getoption -j` reports it.
///
/// Which field is populated depends on the option's type, so all of them are
/// optional and the caller asks for the one it wants.
#[derive(Debug, Default, Deserialize)]
struct RawOption {
    #[serde(default)]
    int: Option<i64>,
    #[serde(default)]
    float: Option<f64>,
    #[serde(default)]
    bool: Option<bool>,
    /// A gap-style option, as `"top right bottom left"`.
    #[serde(default)]
    css: Option<String>,
    /// False when the option name was not recognised at all.
    #[serde(default)]
    set: bool,
}

fn option(name: &str) -> Option<RawOption> {
    let out = run(&["getoption", name, "-j"]).ok()?;
    let parsed: RawOption = serde_json::from_str(&out).ok()?;
    parsed.set.then_some(parsed)
}

/// An integer option, or `None` when it is missing or another type.
///
/// Hyprland reports a `bool` option as an int in some versions, so a boolean
/// read through here still answers sensibly.
pub fn option_int(name: &str) -> Option<i64> {
    let o = option(name)?;
    o.int.or_else(|| o.bool.map(i64::from))
}

/// A gap-style option as its four sides — top, right, bottom, left — or
/// `None`.
///
/// Hyprland reports `general:gaps_in` and its kind as a CSS-style string, not
/// an int, so [`option_int`] never sees them. A plain int is still accepted,
/// for a version that reports one.
pub fn option_sides(name: &str) -> Option<[i64; 4]> {
    let o = option(name)?;
    match o.css {
        Some(css) => parse_sides(&css),
        None => o.int.map(|v| [v; 4]),
    }
}

/// Read a CSS shorthand of one to four values into top, right, bottom, left.
fn parse_sides(css: &str) -> Option<[i64; 4]> {
    let v: Vec<i64> = css.split_whitespace().map(|s| s.parse().ok()).collect::<Option<_>>()?;
    match v[..] {
        [a] => Some([a, a, a, a]),
        [tb, rl] => Some([tb, rl, tb, rl]),
        [t, rl, b] => Some([t, rl, b, rl]),
        [t, r, b, l] => Some([t, r, b, l]),
        _ => None,
    }
}

/// A float option, or `None`.
pub fn option_float(name: &str) -> Option<f32> {
    let o = option(name)?;
    o.float.map(|v| v as f32).or_else(|| o.int.map(|v| v as f32))
}

/// A boolean option, or `None`.
pub fn option_bool(name: &str) -> Option<bool> {
    let o = option(name)?;
    o.bool.or_else(|| o.int.map(|v| v != 0))
}

/// Every animation leaf and easing curve the compositor is running with.
///
/// `hyprctl animations -j` answers with a two-element array: the leaves, then
/// the curves.
pub fn animations(
) -> Result<(Vec<crate::preview::look::Animation>, Vec<crate::preview::look::Curve>)> {
    let out = run(&["animations", "-j"])?;
    let (animations, curves) = serde_json::from_str(&out)
        .map_err(|e| Error::Hyprctl(format!("could not read the animation list: {e}")))?;
    Ok((animations, curves))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ShaderRef;

    #[test]
    fn clients_json_is_reduced_to_what_rules_need() {
        let json = r#"[{"class":"kitty","title":"zsh","initialClass":"kitty"},
                       {"class":"google-chrome","title":"News","initialClass":"chrome"}]"#;
        let raw: Vec<RawClient> = serde_json::from_str(json).unwrap();
        assert_eq!(raw.len(), 2);
        assert_eq!(raw[1].initial_class, "chrome");
    }

    #[test]
    fn missing_fields_do_not_break_parsing() {
        let raw: Vec<RawClient> = serde_json::from_str(r#"[{"class":"kitty"}]"#).unwrap();
        assert_eq!(raw[0].title, "");
    }

    #[test]
    fn layer_json_is_flattened_across_monitors_and_levels() {
        let json = r#"{
            "DP-1": {"levels": {"0": [{"namespace":"mpvpaper"}], "2": [{"namespace":"waybar"}]}},
            "HDMI-A-1": {"levels": {"2": [{"namespace":"waybar"}, {"namespace":"rofi"}]}}
        }"#;
        let raw: std::collections::HashMap<String, RawMonitorLayers> =
            serde_json::from_str(json).unwrap();
        let set: BTreeSet<String> = raw
            .into_values()
            .flat_map(|m| m.levels.into_values())
            .flatten()
            .map(|l| l.namespace)
            .filter(|s| !s.is_empty())
            .collect();
        let got: Vec<String> = set.into_iter().collect();
        assert_eq!(got, vec!["mpvpaper", "rofi", "waybar"]);
    }

    // The three strings below are verbatim hyprctl 0.56.2 output, captured by
    // running the commands against a live session. hyprctl writes all of them
    // to stdout and leaves stderr empty, which is why `dispatch` reads stdout.

    #[test]
    fn the_nil_return_complaint_is_not_a_failure() {
        let out = "error: return hl.dispatch(hl.plugin.HyprWindowShade.reloadshaders()):1: \
                   hl.dispatch: expected a dispatcher (e.g. hl.dsp.window.close())\n";
        assert!(check_dispatch_output(out, "").is_ok());
    }

    #[test]
    fn a_real_lua_error_on_stdout_is_surfaced() {
        // What a broken generated call looks like: the expression referred to a
        // local that does not exist in the dispatch context.
        let out = "error: [string \"return hl.dispatch(hl.plugin.HyprWindowShade....\"]:1: \
                   attempt to concatenate a nil value (global 'hws_shaders')\n";
        let err = check_dispatch_output(out, "").unwrap_err().to_string();
        assert!(err.contains("concatenate a nil value"), "{err}");
    }

    #[test]
    fn a_successful_dispatcher_is_not_a_failure() {
        assert!(check_dispatch_output("ok\n", "").is_ok());
    }

    #[test]
    fn stderr_still_wins_when_hyprctl_itself_cannot_run() {
        let err = check_dispatch_output("", "Couldn't connect to the Hyprland socket")
            .unwrap_err()
            .to_string();
        assert!(err.contains("socket"), "{err}");
    }

    #[test]
    fn silence_is_success() {
        assert!(check_dispatch_output("", "").is_ok());
    }

    #[test]
    fn gaps_are_read_from_the_css_string_hyprland_reports() {
        // Verbatim `hyprctl getoption general:gaps_in -j` on 0.57.
        let json = r#"{"option": "general:gaps_in", "css": "2 2 2 2", "set": true }"#;
        let raw: RawOption = serde_json::from_str(json).unwrap();
        assert_eq!(raw.int, None, "which is why option_int could not read it");
        assert_eq!(parse_sides(&raw.css.unwrap()), Some([2, 2, 2, 2]));
    }

    #[test]
    fn short_css_expands_the_way_css_does() {
        assert_eq!(parse_sides("1 4 4 4"), Some([1, 4, 4, 4]));
        assert_eq!(parse_sides("3"), Some([3, 3, 3, 3]));
        assert_eq!(parse_sides("1 2"), Some([1, 2, 1, 2]));
        assert_eq!(parse_sides("1 2 3"), Some([1, 2, 3, 2]));
        assert_eq!(parse_sides(""), None);
        assert_eq!(parse_sides("1 x"), None);
    }

    #[test]
    fn dispatch_builds_a_lua_expression() {
        let a = Action::ToggleLayerShader {
            namespace: "rofi".into(),
            shader: ShaderRef::new("/p/blur.glsl"),
        };
        assert_eq!(
            a.to_lua_call("hl.plugin.HyprWindowShade"),
            r#"hl.plugin.HyprWindowShade.togglelayershader("rofi", "/p/blur.glsl")"#
        );
    }
}
