//! An end-to-end run of the preview, against a real compositor.
//!
//! Ignored by default: it starts a second Hyprland, so it needs a Wayland
//! session, the plugin built and installed, and `grim`. Run it deliberately:
//!
//! ```sh
//! cargo test -p custard-core --test preview_live -- --ignored --nocapture
//! ```
//!
//! Each test starts a compositor of its own, so they are cheaper and calmer
//! one at a time: add `--test-threads=1` when watching what they do.
//!
//! `CUSTARD_PLUGIN_SO` overrides where the plugin is looked for, and
//! `CUSTARD_PREVIEW_OUT` says where to leave the captured frames for inspection.

use std::path::PathBuf;
use std::time::Duration;

use custard_core::preview::{Preview, Request};

fn plugin_so() -> PathBuf {
    if let Some(p) = std::env::var_os("CUSTARD_PLUGIN_SO") {
        return PathBuf::from(p);
    }
    let user = std::env::var("USER").unwrap_or_default();
    PathBuf::from(format!("/var/cache/hyprpm/{user}/HyprWindowShade/HyprWindowShade.so"))
}

fn out_dir() -> PathBuf {
    std::env::var_os("CUSTARD_PREVIEW_OUT").map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
}

/// The colour at `(x, y)` of a captured frame.
fn pixel(png_bytes: &[u8], (x, y): (usize, usize)) -> [u8; 3] {
    let mut reader = png::Decoder::new(std::io::Cursor::new(png_bytes)).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    let channels = info.color_type.samples();
    let at = y * info.line_size + x * channels;
    [buf[at], buf[at + 1], buf[at + 2]]
}

/// Plain terminal background, low in the left half of the pane: below the
/// test card however far it has scrolled, and inside the first window even
/// while the neighbour has it tiled to half width.
const BACKGROUND: (usize, usize) = (100, 400);

const SHADER: &str = r#"#version 320 es
precision highp float;
// @label Live preview test
in  vec2 v_texcoord;
out vec4 fragColor;
uniform sampler2D tex;
uniform float is_active;

// @param 0.0 1.0 0.01 "Tint strength"
const float STRENGTH = 0.85;

// @color "Tint"
const vec3 TINT = vec3(1.0, 0.35, 0.1);

void main() {
    vec4 col = texture(tex, v_texcoord);
    col.rgb = mix(col.rgb, col.rgb * TINT + TINT * 0.35, STRENGTH);
    fragColor = col;
}
"#;

#[test]
#[ignore = "starts a nested Hyprland; needs a session, the plugin and grim"]
fn a_preview_runs_and_answers_to_an_edit() {
    let mut preview = Preview::new();
    let request = Request {
        original: PathBuf::from("/tmp/preview_live.glsl"),
        source: SHADER.to_string(),
        is_animation: false,
        is_motion_driven: false,
        plugin_so: plugin_so(),
        size: (760, 480),
        hold_secs: 10.0,
        background: 0x1e1e2e,
        desktop_backdrop: true,
    };

    preview.start(&request).expect("the preview should start");
    assert!(preview.is_running());
    assert_eq!(preview.status().shader.as_deref().map(|s| s.ends_with(".glsl")), Some(true));

    // The demo window has to open and be drawn before there is anything to see.
    std::thread::sleep(Duration::from_secs(4));

    let first = preview.capture().expect("a frame");
    let before = std::fs::read(&first).expect("the frame file");
    std::fs::write(out_dir().join("preview-before.png"), &before).unwrap();

    // The whole point: rewrite the shader and the running compositor shows it,
    // with nothing restarted and nothing told to reload.
    let edited = SHADER.replace(
        "const vec3 TINT = vec3(1.0, 0.35, 0.1);",
        "const vec3 TINT = vec3(0.1, 0.5, 1.0);",
    );
    preview.set_source(&edited).expect("the staged source should be written");
    std::thread::sleep(Duration::from_secs(2));

    let after = std::fs::read(preview.capture().expect("a second frame")).expect("the frame file");
    std::fs::write(out_dir().join("preview-after.png"), &after).unwrap();

    // By colour, not by bytes: the terminal scrolls, so two frames differ
    // whether or not a shader is on the window at all — which is how a preview
    // drawing no shader whatsoever once passed this test.
    let [r, _, b] = pixel(&before, BACKGROUND);
    assert!(r > b + 40, "the window should be tinted orange, got {:?}", pixel(&before, BACKGROUND));
    let [r, _, b] = pixel(&after, BACKGROUND);
    assert!(b > r + 40, "the edit should have tinted it blue, got {:?}", pixel(&after, BACKGROUND));

    // The UI pushes the staged source on every state change, and the state
    // changes by itself every few seconds when the compositor is probed. If an
    // unchanged source still reached the file, its mtime would move and the
    // plugin would recompile the shader on a timer, forever.
    let copy = preview.status().shader.expect("a shader path");
    let touched_at = std::fs::metadata(&copy).unwrap().modified().unwrap();
    std::thread::sleep(Duration::from_millis(1100));
    preview.set_source(&edited).expect("writing the same source again");
    assert_eq!(
        std::fs::metadata(&copy).unwrap().modified().unwrap(),
        touched_at,
        "an unchanged source must not touch the file"
    );

    preview.set_source(SHADER).expect("writing a different source");
    assert_ne!(
        std::fs::metadata(&copy).unwrap().modified().unwrap(),
        touched_at,
        "a changed source must reach the file"
    );

    preview.stop();
    assert!(!preview.is_running());
}

#[test]
#[ignore = "starts a nested Hyprland; needs a session, the plugin and grim"]
fn the_demo_window_opens_holds_and_closes_again() {
    let mut preview = Preview::new();
    let request = Request {
        original: PathBuf::from("/tmp/preview_cycle.glsl"),
        source: SHADER.to_string(),
        is_animation: false,
        is_motion_driven: false,
        plugin_so: plugin_so(),
        size: (400, 260),
        // Short, so a whole cycle — open, hold, nudge, hold, close — fits in
        // the frames captured below.
        hold_secs: 4.0,
        background: 0x1e1e2e,
        desktop_backdrop: true,
    };

    preview.start(&request).expect("the preview should start");

    // A window that opens, holds and closes cannot produce the same frame
    // throughout; a loop that has stalled with the window up will.
    let mut frames: Vec<Vec<u8>> = Vec::new();
    for _ in 0..16 {
        if let Ok(path) = preview.capture() {
            if let Ok(bytes) = std::fs::read(path) {
                frames.push(bytes);
            }
        }
        std::thread::sleep(Duration::from_millis(400));
    }

    assert!(frames.len() > 8, "expected to capture most frames, got {}", frames.len());
    let distinct = frames.iter().filter(|f| **f != frames[0]).count();
    assert!(distinct > 0, "every frame was identical, so the demo loop never cycled");

    // A filmstrip to look through when something about the cycle is wrong.
    for (i, frame) in frames.iter().enumerate() {
        std::fs::write(out_dir().join(format!("cycle-{i:02}.png")), frame).unwrap();
    }

    preview.stop();
}
