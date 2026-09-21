use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, anyhow};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use wtm_core::{CancelToken, DownloadError, DownloadOptions, Event, OutputFormat, Tools};

/// Download videos from YouTube and other sites (powered by yt-dlp).
#[derive(Parser)]
#[command(version)]
struct Args {
    /// One or more video URLs.
    #[arg(required_unless_present = "update")]
    urls: Vec<String>,

    /// Where to save files [default: your Downloads folder].
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// File type to save: mp4, mkv, webm, mov, or audio (mp3, m4a, opus, flac, wav).
    #[arg(short, long, value_name = "FORMAT", default_value_t = OutputFormat::Mp4)]
    format: OutputFormat,

    /// Shorthand for --format mp3.
    #[arg(short, long, conflicts_with = "format")]
    audio_only: bool,

    /// Maximum video height, e.g. 1080 or 720 [default: best available]. Ignored for audio.
    #[arg(short, long, value_name = "HEIGHT")]
    quality: Option<u32>,

    /// Print video details instead of downloading.
    #[arg(short, long)]
    info: bool,

    /// Install or update the app's own copy of yt-dlp, then exit unless URLs are given.
    #[arg(short, long)]
    update: bool,
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

/// Returns whether every URL succeeded.
fn run(args: Args) -> Result<bool> {
    if args.update {
        setup_missing_tools()?;
        println!("Updating yt-dlp…");
        println!("yt-dlp is now at {}", wtm_core::update_ytdlp()?);
        if args.urls.is_empty() {
            return Ok(true);
        }
    }
    let tools = Tools::discover().map_err(|e| anyhow!(e))?;
    let out_dir = args.output.clone().unwrap_or_else(wtm_core::default_download_dir);
    let mut all_ok = true;

    // Ctrl+C stops the running download, including everything yt-dlp started,
    // instead of leaving it going in the background.
    let cancel = CancelToken::new();
    {
        let cancel = cancel.clone();
        let _ = ctrlc::set_handler(move || cancel.cancel());
    }

    for url in &args.urls {
        if cancel.is_cancelled() {
            break;
        }
        if args.info {
            match wtm_core::fetch_info(&tools, url) {
                Ok(v) => {
                    println!("{}", v.title);
                    println!(
                        "  by {}  ·  {}  ·  {}",
                        v.uploader.as_deref().unwrap_or("unknown"),
                        v.duration_label().unwrap_or_else(|| "live/unknown length".into()),
                        v.extractor_key.as_deref().unwrap_or("?"),
                    );
                }
                Err(e) => {
                    eprintln!("{url}: {e:#}");
                    all_ok = false;
                }
            }
            continue;
        }

        let opts = DownloadOptions {
            url: url.clone(),
            out_dir: out_dir.clone(),
            format: if args.audio_only { OutputFormat::Mp3 } else { args.format },
            max_height: args.quality,
        };
        match download_one(&tools, &opts, &cancel) {
            Ok(path) => println!("Saved {}", path.display()),
            Err(DownloadError::Cancelled) => {
                eprintln!("Cancelled.");
                all_ok = false;
                break;
            }
            Err(e) => {
                eprintln!("{url}: {e}");
                all_ok = false;
            }
        }
    }
    Ok(all_ok)
}

/// Downloads yt-dlp and ffmpeg if they're missing, so nothing is installed by hand.
fn setup_missing_tools() -> Result<()> {
    if !wtm_core::needs_setup() {
        return Ok(());
    }
    println!("Setting up (first run only)…");
    let bar = ProgressBar::new(1);
    bar.set_style(
        ProgressStyle::with_template("{msg:<22} {bar:30.cyan/blue} {bytes}/{total_bytes}")
            .expect("valid template"),
    );
    let result = wtm_core::run_setup(|p| {
        bar.set_message(p.label);
        match p.total {
            Some(total) => {
                bar.set_length(total);
                bar.set_position(p.done);
            }
            None => {
                bar.set_length(1);
                bar.set_position(0);
            }
        }
    });
    bar.finish_and_clear();
    result
}

fn download_one(
    tools: &Tools,
    opts: &DownloadOptions,
    cancel: &CancelToken,
) -> Result<PathBuf, DownloadError> {
    let bar = ProgressBar::new(100);
    bar.set_style(
        ProgressStyle::with_template("{bar:40.cyan/blue} {percent:>3}%  {msg}")
            .expect("valid template")
            .progress_chars("█▉▊▋▌▍▎▏ "),
    );
    let result = wtm_core::download(tools, opts, cancel, |event| match event {
        Event::Progress(p) => {
            if let Some(f) = p.fraction() {
                bar.set_position((f * 100.0) as u64);
            }
            let speed = p.speed.map(|s| format!("{:.1} MB/s", s / 1e6)).unwrap_or_default();
            let eta = p.eta_secs.map(|e| format!("ETA {e}s")).unwrap_or_default();
            bar.set_message(format!("{speed}  {eta}"));
        }
        Event::Processing => {
            bar.set_position(100);
            bar.set_message("processing…");
        }
    });
    bar.finish_and_clear();
    result
}
