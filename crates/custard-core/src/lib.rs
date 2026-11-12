//! The engine behind hyprcustard.
//!
//! This crate knows nothing about Qt or any other toolkit. It reads and writes
//! the managed block in `hyprland.lua`, reads and edits `.glsl` files, talks to
//! `hyprctl`, and exposes the whole thing as [`session::Session`] — one JSON
//! state document and one command entry point.
//!
//! ```no_run
//! use custard_core::session::Session;
//! use serde_json::json;
//!
//! let mut s = Session::load();
//! s.command("rule.add", &json!({ "class": "kitty" })).unwrap();
//! let id = s.config.rules[0].id.clone();
//! s.command("rule.setTag", &json!({
//!     "id": id, "slot": "shader_inactive", "path": "/path/crt.glsl"
//! })).unwrap();
//! println!("{}", s.save().unwrap());
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod block;
pub mod colors;
pub mod emit;
pub mod error;
pub mod hyprctl;
pub mod hyprpm;
pub mod import;
pub mod look;
pub mod model;
pub mod motion;
pub mod paths;
pub mod preset;
pub mod preview;
pub mod rules;
pub mod session;
pub mod settings;
pub mod shader;
pub mod theme;

pub use error::{Error, Result};
pub use session::Session;

/// The app's version, as the UI should show it.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
