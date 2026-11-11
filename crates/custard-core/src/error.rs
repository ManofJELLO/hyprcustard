//! Error type shared by the whole core.

use std::path::{Path, PathBuf};

/// Anything that can go wrong in the core.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A filesystem operation failed, with the path that failed.
    #[error("{path}: {source}")]
    Io {
        /// The path involved.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },

    /// The embedded state blob could not be decoded.
    #[error("could not read the managed block's saved state: {0}")]
    State(#[from] serde_json::Error),

    /// A theme file could not be decoded.
    #[error("theme file is not valid TOML: {0}")]
    Theme(#[from] toml::de::Error),

    /// The managed block markers are damaged.
    #[error("{0}")]
    Block(String),

    /// A shader file could not be edited as expected.
    #[error("{0}")]
    Shader(String),

    /// Running `hyprctl` failed.
    #[error("hyprctl: {0}")]
    Hyprctl(String),

    /// Anything else worth surfacing to the user.
    #[error("{0}")]
    Other(String),
}

impl Error {
    /// Build an [`Error::Io`] for a path.
    pub fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        Error::Io { path: path.as_ref().to_path_buf(), source }
    }

    /// Build an [`Error::Other`].
    pub fn other(msg: impl Into<String>) -> Self {
        Error::Other(msg.into())
    }
}

/// Convenience alias.
pub type Result<T> = std::result::Result<T, Error>;
