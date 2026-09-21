use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::format::OutputFormat;
use crate::info::last_error_line;
use crate::process::command;
use crate::runner::run_streaming;
use crate::tools::Tools;

/// Label and max-height pairs shared by the CLI and the GUI dropdown.
pub const QUALITY_PRESETS: &[(&str, Option<u32>)] = &[
    ("Best available", None),
    ("2160p (4K)", Some(2160)),
    ("1440p", Some(1440)),
    ("1080p", Some(1080)),
    ("720p", Some(720)),
    ("480p", Some(480)),
    ("360p", Some(360)),
];

#[derive(Debug, Clone)]
pub struct DownloadOptions {
    pub url: String,
    pub out_dir: PathBuf,
    pub format: OutputFormat,
    /// Prefer the best video at or below this height (ignored for audio formats).
    pub max_height: Option<u32>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Progress {
    pub downloaded: u64,
    pub total: Option<u64>,
    /// Bytes per second.
    pub speed: Option<f64>,
    pub eta_secs: Option<u64>,
}

impl Progress {
    pub fn fraction(&self) -> Option<f32> {
        let total = self.total.filter(|t| *t > 0)?;
        Some((self.downloaded as f32 / total as f32).clamp(0.0, 1.0))
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    Progress(Progress),
    /// Merging streams / converting audio.
    Processing,
}

#[derive(Debug)]
pub enum DownloadError {
    Cancelled,
    Failed(String),
}

impl fmt::Display for DownloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("cancelled"),
            Self::Failed(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for DownloadError {}

#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

const PROGRESS_TAG: &str = "WTM_PROGRESS|";
const POST_TAG: &str = "WTM_POST|";
const FILE_TAG: &str = "WTM_FILE|";

/// Runs a download to completion, reporting progress through `on_event`.
/// Blocks, so front-ends should call it from a worker thread.
///
/// YouTube occasionally answers a valid request with a one-off `403`, which
/// goes away on retry, so that specific failure is retried once.
pub fn download(
    tools: &Tools,
    opts: &DownloadOptions,
    cancel: &CancelToken,
    mut on_event: impl FnMut(Event),
) -> Result<PathBuf, DownloadError> {
    match download_once(tools, opts, cancel, &mut on_event) {
        Err(DownloadError::Failed(msg)) if msg.contains("HTTP Error 403") => {
            download_once(tools, opts, cancel, &mut on_event)
        }
        other => other,
    }
}

fn download_once(
    tools: &Tools,
    opts: &DownloadOptions,
    cancel: &CancelToken,
    on_event: &mut impl FnMut(Event),
) -> Result<PathBuf, DownloadError> {
    if opts.format.needs_ffmpeg() && tools.ffmpeg.is_none() {
        return Err(DownloadError::Failed(format!(
            "ffmpeg is required to save as {}.\n{}",
            opts.format.extension().to_uppercase(),
            crate::tools::install_hint()
        )));
    }

    let mut cmd = command(&tools.ytdlp);
    cmd.args(["--no-playlist", "--no-warnings", "--newline", "--progress"])
        .args([
            "--progress-template",
            &format!(
                "download:{PROGRESS_TAG}%(progress.downloaded_bytes)s|%(progress.total_bytes)s|\
                 %(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s"
            ),
            "--progress-template",
            &format!("postprocess:{POST_TAG}%(progress.status)s"),
            "--print",
            &format!("after_move:{FILE_TAG}%(filepath)s"),
            "--no-simulate",
            "-P",
        ])
        .arg(&opts.out_dir)
        .args(["-o", "%(title).150B [%(id)s].%(ext)s"]);

    if let Some(ffmpeg) = &tools.ffmpeg {
        cmd.arg("--ffmpeg-location").arg(ffmpeg);
    }

    let ext = opts.format.extension();
    if opts.format.is_audio() {
        cmd.args(["-f", "ba/b", "-x", "--audio-format", ext, "--audio-quality", "0"]);
    } else if tools.ffmpeg.is_some() {
        cmd.args(["-f", "bv*+ba/b"]);
        let sort = video_sort(opts);
        if !sort.is_empty() {
            cmd.args(["-S", &sort]);
        }
        cmd.args(["--merge-output-format", ext, "--remux-video", ext]);
    } else {
        // No ffmpeg: only pre-merged single-file formats are possible.
        cmd.args(["-f", "b"]);
    }

    cmd.arg("--").arg(&opts.url);

    let mut final_path = None;
    let (status, stderr) = run_streaming(cmd, "yt-dlp", cancel, |line| {
        if let Some(rest) = line.strip_prefix(PROGRESS_TAG) {
            if let Some(p) = parse_progress(rest) {
                on_event(Event::Progress(p));
            }
        } else if line.starts_with(POST_TAG) {
            on_event(Event::Processing);
        } else if let Some(path) = line.strip_prefix(FILE_TAG) {
            final_path = Some(PathBuf::from(path.trim()));
        }
    })?;

    if !status.success() {
        return Err(DownloadError::Failed(last_error_line(&stderr)));
    }
    final_path.ok_or_else(|| DownloadError::Failed("download finished but no file was reported".into()))
}

/// Builds yt-dlp's `-S` preference list. Sorting rather than filtering keeps
/// downloads working when a site has nothing at the requested height, and
/// preferring codecs that fit the container avoids failed remuxes.
fn video_sort(opts: &DownloadOptions) -> String {
    let res = opts.max_height.map(|h| format!("res:{h}"));
    let parts: Vec<Option<String>> = match opts.format {
        // H.264 first (ahead of resolution): QuickTime can't play VP9/AV1 in MOV.
        OutputFormat::Mov => vec![
            Some("vcodec:h264".into()),
            res,
            Some("acodec:aac".into()),
            Some("ext:mp4:m4a".into()),
        ],
        OutputFormat::Webm => vec![res, Some("ext:webm:webm".into())],
        OutputFormat::Mkv => vec![res],
        // MP4: resolution still wins, but at equal resolution prefer H.264/AAC,
        // which plays everywhere. Above 1080p YouTube only offers VP9/AV1, so
        // those sizes fall back to VP9/AV1 in mp4.
        _ => vec![
            res,
            Some("vcodec:h264".into()),
            Some("acodec:aac".into()),
            Some("ext:mp4:m4a".into()),
        ],
    };
    parts.into_iter().flatten().collect::<Vec<_>>().join(",")
}

/// Parses `downloaded|total|total_estimate|speed|eta`, where yt-dlp prints
/// `NA` for anything it doesn't know.
fn parse_progress(s: &str) -> Option<Progress> {
    let mut f = s.split('|');
    let num = |v: Option<&str>| v.and_then(|v| v.trim().parse::<f64>().ok());
    let downloaded = num(f.next())? as u64;
    let total = num(f.next());
    let estimate = num(f.next());
    let speed = num(f.next());
    let eta = num(f.next());
    Some(Progress {
        downloaded,
        total: total.or(estimate).map(|t| t as u64),
        speed,
        eta_secs: eta.map(|e| e as u64),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_progress_line() {
        let p = parse_progress("500|1000|NA|2048.5|12").unwrap();
        assert_eq!(p.downloaded, 500);
        assert_eq!(p.total, Some(1000));
        assert_eq!(p.speed, Some(2048.5));
        assert_eq!(p.eta_secs, Some(12));
        assert_eq!(p.fraction(), Some(0.5));
    }

    #[test]
    fn falls_back_to_estimated_total_and_tolerates_na() {
        let p = parse_progress("100|NA|400|NA|NA").unwrap();
        assert_eq!(p.total, Some(400));
        assert_eq!(p.speed, None);
        assert_eq!(p.eta_secs, None);
    }

    fn opts(format: OutputFormat, max_height: Option<u32>) -> DownloadOptions {
        DownloadOptions { url: String::new(), out_dir: PathBuf::new(), format, max_height }
    }

    #[test]
    fn sort_prefers_h264_before_resolution_for_mov() {
        assert_eq!(
            video_sort(&opts(OutputFormat::Mov, Some(1080))),
            "vcodec:h264,res:1080,acodec:aac,ext:mp4:m4a"
        );
    }

    #[test]
    fn sort_omits_resolution_when_unlimited() {
        assert_eq!(
            video_sort(&opts(OutputFormat::Mp4, None)),
            "vcodec:h264,acodec:aac,ext:mp4:m4a"
        );
        assert_eq!(video_sort(&opts(OutputFormat::Mkv, None)), "");
        assert_eq!(video_sort(&opts(OutputFormat::Webm, Some(720))), "res:720,ext:webm:webm");
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_progress("NA|NA|NA|NA|NA").is_none());
    }
}
