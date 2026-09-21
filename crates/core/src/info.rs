use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::process::command;
use crate::tools::Tools;

#[derive(Debug, Clone, Deserialize)]
pub struct VideoInfo {
    pub id: String,
    pub title: String,
    pub uploader: Option<String>,
    pub duration: Option<f64>,
    pub thumbnail: Option<String>,
    pub extractor_key: Option<String>,
}

impl VideoInfo {
    /// `3:07` or `1:02:33`.
    pub fn duration_label(&self) -> Option<String> {
        let secs = self.duration? as u64;
        let (h, m, s) = (secs / 3600, secs % 3600 / 60, secs % 60);
        Some(if h > 0 {
            format!("{h}:{m:02}:{s:02}")
        } else {
            format!("{m}:{s:02}")
        })
    }
}

pub fn fetch_info(tools: &Tools, url: &str) -> Result<VideoInfo> {
    let out = command(&tools.ytdlp)
        .args(["--dump-single-json", "--no-playlist", "--no-warnings", "--"])
        .arg(url)
        .output()
        .context("failed to run yt-dlp")?;
    if !out.status.success() {
        bail!("{}", last_error_line(&out.stderr));
    }
    serde_json::from_slice(&out.stdout).context("could not parse yt-dlp output")
}

pub(crate) fn last_error_line(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    text.lines()
        .rev()
        .find(|l| l.contains("ERROR"))
        .or_else(|| text.lines().rev().find(|l| !l.trim().is_empty()))
        .map(|l| l.trim().trim_start_matches("ERROR: ").to_string())
        .unwrap_or_else(|| "yt-dlp failed with no output".into())
}
