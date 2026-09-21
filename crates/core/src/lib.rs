//! Shared download logic for the CLI and GUI front-ends.
//!
//! Everything platform-specific (locating binaries, hiding console windows,
//! killing process trees) lives here so both front-ends behave identically on
//! macOS and Windows.

mod download;
mod format;
mod info;
mod process;
mod runner;
mod setup;
mod tools;

pub use download::{
    CancelToken, DownloadError, DownloadOptions, Event, Progress, QUALITY_PRESETS, download,
};
pub use format::OutputFormat;
pub use info::{VideoInfo, fetch_info};
pub use setup::{SetupProgress, bin_dir, data_dir, needs_setup, run_setup};
pub use tools::{Tools, install_hint, update_ytdlp};

use std::path::PathBuf;

/// The user's Downloads folder, falling back to the current directory.
pub fn default_download_dir() -> PathBuf {
    dirs::download_dir().unwrap_or_else(|| PathBuf::from("."))
}
