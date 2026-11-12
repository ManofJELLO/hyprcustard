//! The Lua config the preview instance runs on.
//!
//! Lua, not `.conf`, and not by preference: in Hyprland 0.56 a `.conf` window
//! rule rejects `class:` matchers outright — every spelling comes back as
//! "invalid field type class" — while `hl.window_rule` applies the plugin's tag
//! first time. That is the same call [`crate::emit`] writes into the user's own
//! config, so the preview and the real thing agree by construction.
//!
//! Nothing here touches the user's files. The config, the shader it points at
//! and the backdrop all live in a private runtime directory, which is what lets
//! the preview show staged edits that have never been saved.

use std::path::Path;

use crate::model::{trim_float, TagSlot};
use crate::preview::look::Look;

/// Everything the generated config needs to know.
#[derive(Debug, Clone)]
pub struct PreviewConfig {
    /// The temporary `.glsl` the rule points at.
    pub shader: String,
    /// Which of the plugin's tags to apply it through.
    ///
    /// More than one, because a shader is only visible in the phases its tags
    /// cover and the preview should not make the user guess which phase they
    /// are meant to be watching.
    pub slots: Vec<TagSlot>,
    /// Class pattern the rule matches.
    ///
    /// `.*` by default, and that is not laziness: a preview instance holds
    /// exactly one window, so matching everything works whichever terminal is
    /// installed and whatever class it reports — kitty says `kitty`, ghostty
    /// says `com.mitchellh.ghostty`, and the backdrop is a layer surface, not
    /// a window, so it is not affected either way.
    pub demo_class: String,
    /// The plugin `.so` to load once the session is up.
    pub plugin_so: String,
    /// Background color behind everything, as `0xRRGGBB`.
    pub background: u32,
    /// Pane size in pixels, which the preview's output is fixed to.
    pub size: (u32, u32),
    /// The app's own process id, for the watchdog.
    pub app_pid: u32,
    /// How the compositor should draw.
    pub look: Look,
}

/// Quote a string for Lua, the way [`crate::emit`] does.
fn lua_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', r"\\").replace('"', "\\\""))
}

/// Render the whole config.
pub fn render(cfg: &PreviewConfig) -> String {
    let mut out = String::new();

    out.push_str("-- hyprcustard preview instance.\n");
    out.push_str("--\n");
    out.push_str("-- Generated for one preview and deleted with it. This is not your config,\n");
    out.push_str("-- and nothing here is written back to it: the shader below is a temporary\n");
    out.push_str("-- copy carrying whatever values are staged in the app right now.\n\n");

    emit_monitor(&mut out, cfg);
    emit_curves(&mut out, cfg);
    emit_animations(&mut out, cfg);
    emit_look(&mut out, cfg);
    emit_rule(&mut out, cfg);
    emit_startup(&mut out, cfg);

    out
}

/// Fix every output to the pane's size, and never open a window.
///
/// A catch-all rule rather than one naming the headless output, because it has
/// to be in force *before* that output exists: the preview creates it after the
/// compositor is already up, and a `hyprctl keyword monitor` aimed at it then
/// is accepted and quietly ignored. Declared here it simply applies on arrival.
/// `auto` placement rather than a fixed origin, because for the moment both
/// outputs exist a shared origin is an overlapping layout — and Hyprland says
/// so in a banner across the preview.
///
/// `WAYLAND-1` is the nested instance's own backend output, which is a window
/// on the user's screen — `aquamarine - WAYLAND-1`. Disabling it here means
/// that window is never opened at all. Removing the output afterwards, which is
/// what the preview used to rely on, could only ever take the window away a
/// second after it had already appeared. An instance whose only output is
/// disabled still comes up and still registers itself, and the headless output
/// it draws on is created moments later.
fn emit_monitor(out: &mut String, cfg: &PreviewConfig) {
    let (w, h) = cfg.size;
    out.push_str(&format!(
        "hl.monitor({{ output = \"\", mode = \"{w}x{h}@60\", position = \"auto\", scale = 1 }})\n"
    ));
    out.push_str("hl.monitor({ output = \"WAYLAND-1\", disabled = true })\n\n");
}

fn emit_curves(out: &mut String, cfg: &PreviewConfig) {
    if cfg.look.curves.is_empty() {
        return;
    }
    out.push_str("-- Easing curves, copied from the running compositor.\n");
    for c in &cfg.look.curves {
        out.push_str(&format!(
            "hl.curve({}, {{ type = \"bezier\", points = {{ {{{}, {}}}, {{{}, {}}} }} }})\n",
            lua_quote(&c.name),
            trim_float(c.x0),
            trim_float(c.y0),
            trim_float(c.x1),
            trim_float(c.y1),
        ));
    }
    out.push('\n');
}

fn emit_animations(out: &mut String, cfg: &PreviewConfig) {
    if cfg.look.animation_leaves.is_empty() {
        return;
    }
    out.push_str("-- The animation leaves you have overridden. An open or close shader runs\n");
    out.push_str("-- alongside these, so a preview without them is not the same effect.\n");
    for a in &cfg.look.animation_leaves {
        out.push_str(&format!(
            "hl.animation({{ leaf = {}, enabled = {}, speed = {}",
            lua_quote(&a.name),
            a.enabled,
            trim_float(a.speed),
        ));
        if !a.bezier.is_empty() {
            out.push_str(&format!(", bezier = {}", lua_quote(&a.bezier)));
        }
        if !a.style.is_empty() {
            out.push_str(&format!(", style = {}", lua_quote(&a.style)));
        }
        out.push_str(" })\n");
    }
    out.push('\n');
}

/// A gap the way the user's own config would spell it: one number when every
/// side agrees, the per-side table when they do not.
fn gaps_lua([top, right, bottom, left]: [i64; 4]) -> String {
    if top == right && right == bottom && bottom == left {
        top.to_string()
    } else {
        format!("{{ top = {top}, right = {right}, bottom = {bottom}, left = {left} }}")
    }
}

fn emit_look(out: &mut String, cfg: &PreviewConfig) {
    let l = &cfg.look;
    out.push_str("hl.config({\n");
    out.push_str("    general = {\n");
    out.push_str(&format!("        gaps_in     = {},\n", gaps_lua(l.gaps_in)));
    // Not the user's gaps_out: this is a small pane, not a monitor, and their
    // desktop margins would push the window off it.
    out.push_str("        gaps_out    = 14,\n");
    out.push_str(&format!("        border_size = {},\n", l.border_size));
    out.push_str("    },\n");
    out.push_str("    decoration = {\n");
    out.push_str(&format!("        rounding         = {},\n", l.rounding));
    out.push_str(&format!("        active_opacity   = {},\n", trim_float(l.active_opacity)));
    out.push_str(&format!("        inactive_opacity = {},\n", trim_float(l.inactive_opacity)));
    out.push_str(&format!("        blur = {{ enabled = {} }},\n", l.blur));
    out.push_str("    },\n");
    out.push_str(&format!("    animations = {{ enabled = {} }},\n", l.animations));
    out.push_str("    misc = {\n");
    out.push_str("        disable_hyprland_logo    = true,\n");
    out.push_str("        disable_splash_rendering = true,\n");
    out.push_str("        force_default_wallpaper  = 0,\n");
    // Every one of these is a banner Hyprland would otherwise draw across the
    // top of the preview — over the very window the user is trying to look at.
    // The watchdog one fires because this instance is started directly rather
    // than through start-hyprland, which is exactly what a preview should do.
    out.push_str("        disable_watchdog_warning       = true,\n");
    out.push_str("        disable_xdg_env_checks         = true,\n");
    out.push_str("        disable_hyprland_guiutils_check = true,\n");
    out.push_str("        disable_scale_notification     = true,\n");
    out.push_str("        disable_autoreload             = true,\n");
    out.push_str(&format!("        background_color         = 0x{:06x},\n", cfg.background));
    out.push_str("    },\n");
    out.push_str("    input = {\n");
    // The preview has no one typing into it, and a follow-mouse instance that
    // reacts to the host's pointer would flicker focus mid-capture.
    out.push_str("        follow_mouse = 0,\n");
    out.push_str("    },\n");
    out.push_str("})\n\n");
}

fn emit_rule(out: &mut String, cfg: &PreviewConfig) {
    out.push_str("-- The shader under test. The plugin reloads a shader when its mtime\n");
    out.push_str("-- changes, so rewriting the file below is what makes a slider live.\n");
    out.push_str("--\n");
    out.push_str("-- One rule per tag, the way the app writes them into your own config:\n");
    out.push_str("-- Hyprland's rule takes a single tag and the plugin stacks the ones that\n");
    out.push_str("-- match the same window.\n");

    for slot in &cfg.slots {
        out.push_str("\nhl.window_rule({\n");
        out.push_str(&format!("    name  = \"hws-preview-{}\",\n", slot.info().key));
        out.push_str(&format!("    match = {{ class = {} }},\n", lua_quote(&cfg.demo_class)));
        out.push_str(&format!(
            "    tag   = {},\n",
            lua_quote(&format!("+{}:{}", slot.info().key, cfg.shader))
        ));
        out.push_str("})\n");
    }

    // The plugin draws nothing over a fullscreen or maximized window unless it
    // opted in, and on 0.57 the demo terminal asks to be maximized as it maps.
    // Granted, every window in the cycle came up unshaded — the neighbor too,
    // since a window opened over a maximized one inherits its mode — and only
    // took its shader once focus moved and the request was forgotten.
    out.push_str("\nhl.window_rule({\n");
    out.push_str("    name           = \"hws-preview-stay-tiled\",\n");
    out.push_str(&format!("    match          = {{ class = {} }},\n", lua_quote(&cfg.demo_class)));
    out.push_str("    suppress_event = \"maximize fullscreen\",\n");
    out.push_str("})\n");
    out.push('\n');
}

fn emit_startup(out: &mut String, cfg: &PreviewConfig) {
    out.push_str("hl.on(\"hyprland.start\", function()\n");

    // Loaded here rather than at parse time for the same reason the app's own
    // generated block does it: a plugin that fails to load costs one preview,
    // not the session.
    out.push_str(&format!(
        "    hl.exec_cmd({})\n",
        lua_quote(&format!("hyprctl plugin load {}", cfg.plugin_so))
    ));

    // A compositor nobody can see and nobody remembers starting is the worst
    // thing this feature could leave behind, and the app cannot promise to
    // clean up after itself — it may be killed, or crash. So the preview
    // watches the app instead, and shows itself out.
    out.push_str("\n    -- Outlive nothing: when the app that started this preview is gone,\n");
    out.push_str("    -- so is the preview.\n");
    out.push_str(&format!(
        "    hl.exec_cmd({})\n",
        lua_quote(&format!(
            "sh -c 'while kill -0 {} 2>/dev/null; do sleep 2; done; \
             hyprctl dispatch \"hl.dsp.exit()\"'",
            cfg.app_pid
        ))
    ));
    out.push_str("end)\n");
}

/// Which tags a shader should be previewed through.
///
/// A shader is only visible during the phases its tags cover, and the phase it
/// was written for is written into the shader itself — so the preview reads it
/// off rather than asking:
///
/// * one that drives itself from `progress` exists only during a transition, so
///   it goes on **open and close**, and the demo loop provides both;
/// * one driven by velocity or a move delta exists only while the window is
///   going somewhere, so it goes on **move and resize**;
/// * anything else is continuous and goes on the plain **shader** tag, where it
///   is visible for the whole hold.
///
/// A shader can be more than one of these — a wobble that also fades in reads
/// both `progress` and `velocity` — and then it gets all of the tags it earns.
pub fn slots_for(is_animation: bool, is_motion_driven: bool) -> Vec<TagSlot> {
    let mut slots = Vec::new();
    if is_animation {
        slots.push(TagSlot::Open);
        slots.push(TagSlot::Close);
    }
    if is_motion_driven {
        slots.push(TagSlot::Move);
        slots.push(TagSlot::Resize);
    }
    if slots.is_empty() {
        slots.push(TagSlot::Shader);
    }
    slots
}

/// Where the preview's private files live.
pub fn dir() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("hyprcustard").join("preview")
}

/// The path the temporary copy of a shader is written to.
///
/// Named after the original so the plugin's own log lines, and any error it
/// reports, still say which shader the user is looking at.
pub fn shader_path(original: &Path) -> std::path::PathBuf {
    let name = original.file_name().map(|n| n.to_os_string()).unwrap_or_else(|| "preview".into());
    dir().join(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preview::look::{Animation, Curve};

    fn config() -> PreviewConfig {
        PreviewConfig {
            shader: "/run/user/1000/hyprcustard/preview/dim.glsl".into(),
            slots: vec![TagSlot::Shader],
            demo_class: "kitty".into(),
            plugin_so: "/var/cache/hyprpm/me/HyprWindowShade/HyprWindowShade.so".into(),
            background: 0x1e1e2e,
            size: (760, 480),
            app_pid: 4321,
            look: Look::default(),
        }
    }

    #[test]
    fn the_rule_carries_the_tag_the_plugin_expects() {
        let out = render(&config());
        assert!(
            out.contains(r#"tag   = "+shader:/run/user/1000/hyprcustard/preview/dim.glsl","#),
            "{out}"
        );
        assert!(out.contains(r#"match = { class = "kitty" },"#));
    }

    #[test]
    fn a_shader_is_previewed_through_the_tags_it_earns() {
        assert_eq!(slots_for(false, false), vec![TagSlot::Shader]);
        assert_eq!(slots_for(true, false), vec![TagSlot::Open, TagSlot::Close]);
        assert_eq!(slots_for(false, true), vec![TagSlot::Move, TagSlot::Resize]);
        assert_eq!(
            slots_for(true, true),
            vec![TagSlot::Open, TagSlot::Close, TagSlot::Move, TagSlot::Resize]
        );
    }

    #[test]
    fn every_tag_gets_a_rule_of_its_own() {
        let mut cfg = config();
        cfg.slots = slots_for(true, false);

        let out = render(&cfg);
        assert!(out.contains("+shader_open:"), "{out}");
        assert!(out.contains("+shader_close:"), "{out}");
        assert_eq!(out.matches("    tag   = ").count(), 2);
        // A plain `shader` tag would leave a progress-driven effect frozen
        // on screen for the whole hold.
        assert!(!out.contains(r#"tag   = "+shader:"#), "{out}");
    }

    #[test]
    fn the_look_is_carried_over() {
        let mut cfg = config();
        cfg.look.rounding = 12;
        cfg.look.border_size = 3;
        cfg.look.inactive_opacity = 0.85;
        cfg.look.blur = false;

        let out = render(&cfg);
        assert!(out.contains("rounding         = 12,"));
        assert!(out.contains("border_size = 3,"));
        assert!(out.contains("inactive_opacity = 0.85,"));
        assert!(out.contains("blur = { enabled = false },"));
    }

    #[test]
    fn curves_and_animations_are_written_in_the_lua_api_shape() {
        let mut cfg = config();
        cfg.look.curves =
            vec![Curve { name: "overshoot".into(), x0: 0.15, y0: 0.67, x1: 0.25, y1: 1.19 }];
        cfg.look.animation_leaves = vec![Animation {
            name: "windowsIn".into(),
            overridden: true,
            enabled: true,
            speed: 2.0,
            bezier: "overshoot".into(),
            style: "slidefade".into(),
        }];

        let out = render(&cfg);
        assert!(
            out.contains(
                r#"hl.curve("overshoot", { type = "bezier", points = { {0.15, 0.67}, {0.25, 1.19} } })"#
            ),
            "{out}"
        );
        assert!(out.contains(
            r#"hl.animation({ leaf = "windowsIn", enabled = true, speed = 2, bezier = "overshoot", style = "slidefade" })"#
        ));
    }

    #[test]
    fn an_animation_without_a_style_leaves_the_key_out() {
        let mut cfg = config();
        cfg.look.animation_leaves = vec![Animation {
            name: "fadeIn".into(),
            overridden: true,
            enabled: true,
            speed: 1.7,
            bezier: "linear".into(),
            style: String::new(),
        }];
        let out = render(&cfg);
        assert!(out.contains(r#"speed = 1.7, bezier = "linear" })"#), "{out}");
        assert!(!out.contains("style ="));
    }

    #[test]
    fn the_output_is_pinned_to_the_pane_size() {
        let mut cfg = config();
        cfg.size = (640, 400);
        assert!(
            render(&cfg).contains(
                r#"hl.monitor({ output = "", mode = "640x400@60", position = "auto", scale = 1 })"#
            ),
            "the mode has to be declared, not set at runtime"
        );
    }

    #[test]
    fn the_instances_own_window_is_disabled_before_it_opens() {
        assert!(
            render(&config()).contains(r#"hl.monitor({ output = "WAYLAND-1", disabled = true })"#),
            "without this the nested compositor opens a window on the user's screen, and \
             removing the output afterwards can only take it away again a second later"
        );
    }

    #[test]
    fn the_warning_banners_are_turned_off() {
        // A banner is drawn over the window being previewed, so each of these
        // is the difference between seeing the shader and seeing a warning.
        let out = render(&config());
        for option in [
            "disable_watchdog_warning",
            "disable_xdg_env_checks",
            "disable_hyprland_logo",
            "disable_splash_rendering",
        ] {
            assert!(out.contains(option), "{option} is not disabled");
        }
    }

    #[test]
    fn the_demo_window_is_never_maximized() {
        let out = render(&config());
        assert!(out.contains(r#"suppress_event = "maximize fullscreen","#), "{out}");
    }

    #[test]
    fn gaps_are_one_number_until_the_sides_differ() {
        let mut cfg = config();
        cfg.look.gaps_in = [2; 4];
        assert!(render(&cfg).contains("gaps_in     = 2,\n"));

        cfg.look.gaps_in = [1, 4, 4, 4];
        let out = render(&cfg);
        assert!(
            out.contains("gaps_in     = { top = 1, right = 4, bottom = 4, left = 4 },"),
            "{out}"
        );
    }

    #[test]
    fn the_plugin_loads_at_session_start_not_at_parse_time() {
        let out = render(&config());
        let start = out.find("hl.on(\"hyprland.start\"").expect("a start handler");
        let load = out.find("hyprctl plugin load").expect("a load line");
        assert!(start < load, "the loader must be inside the handler");
    }

    #[test]
    fn the_preview_stops_itself_when_the_app_is_gone() {
        let out = render(&config());
        assert!(out.contains("kill -0 4321"), "{out}");
        assert!(out.contains(r#"hyprctl dispatch \"hl.dsp.exit()\""#), "{out}");
    }

    #[test]
    fn the_backdrop_is_not_started_from_the_config() {
        // It is launched against the nested socket once the instance is
        // headless; see `backdrop::prepare` for why that has to wait.
        let out = render(&config());
        assert!(!out.contains("swaybg"), "{out}");
    }

    #[test]
    fn a_shader_copy_keeps_the_name_of_the_original() {
        let p = shader_path(Path::new("/home/me/.config/hypr/shaders/crt_mode.glsl"));
        assert_eq!(p.file_name().unwrap(), "crt_mode.glsl");
        assert!(p.starts_with(dir()));
    }
}
