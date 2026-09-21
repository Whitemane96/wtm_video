use std::ffi::OsString;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use crate::process::command;
use crate::setup::{bin_dir, install_ytdlp};

/// Locations of the external programs we drive.
#[derive(Debug, Clone)]
pub struct Tools {
    pub ytdlp: PathBuf,
    /// Optional: without it we can't merge separate video/audio streams or
    /// convert to mp3, but single-file downloads still work.
    pub ffmpeg: Option<PathBuf>,
}

impl Tools {
    /// Prefers our own updatable copy of yt-dlp over whatever the system has,
    /// since package managers lag behind YouTube's changes.
    pub fn discover() -> Result<Self, String> {
        let ytdlp = managed_ytdlp_path()
            .filter(|p| p.exists())
            .or_else(|| find("yt-dlp"))
            .ok_or_else(|| {
                format!(
                    "yt-dlp was not found. Run with --update (or open the app, which sets \
                     itself up) to download it.\n{}",
                    install_hint()
                )
            })?;
        Ok(Self {
            ytdlp,
            ffmpeg: find("ffmpeg"),
        })
    }

    pub fn ytdlp_version(&self) -> Option<String> {
        let out = command(&self.ytdlp).arg("--version").output().ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
}

/// Where the app keeps its own copy of yt-dlp.
pub(crate) fn managed_ytdlp_path() -> Option<PathBuf> {
    let name = if cfg!(windows) { "yt-dlp.exe" } else { "yt-dlp" };
    bin_dir().map(|d| d.join(name))
}

/// Installs the standalone yt-dlp build on first call, and updates it in place
/// afterwards. Returns the resulting version.
pub fn update_ytdlp() -> Result<String> {
    let path = managed_ytdlp_path().context("could not determine an app data folder")?;
    if path.exists() {
        // Standalone builds know how to update themselves.
        let out = command(&path).arg("-U").output().context("failed to run yt-dlp -U")?;
        if !out.status.success() {
            bail!("{}", crate::info::last_error_line(&out.stderr));
        }
    } else {
        install_ytdlp(&mut |_| {})?;
    }
    Tools { ytdlp: path, ffmpeg: None }
        .ytdlp_version()
        .context("installed yt-dlp did not report a version")
}

pub fn install_hint() -> &'static str {
    if cfg!(target_os = "macos") {
        "ffmpeg: brew install ffmpeg"
    } else if cfg!(windows) {
        "ffmpeg: winget install Gyan.FFmpeg  (or place ffmpeg.exe next to this program)"
    } else {
        "ffmpeg: sudo apt install ffmpeg (Debian/Ubuntu), sudo dnf install ffmpeg (Fedora), sudo pacman -S ffmpeg (Arch)"
    }
}

/// Looks in the app's own download folder, next to our own executable (for
/// bundled copies), in well-known install locations, then on `PATH`.
///
/// The extra directories matter: an app launched from Finder or the Start menu
/// doesn't inherit the shell `PATH`, so on macOS it would never see Homebrew.
pub(crate) fn find(name: &str) -> Option<PathBuf> {
    // Advanced: WTM_ONLY_MANAGED=1 ignores programs installed elsewhere, which is
    // handy for checking how the app behaves on a computer with nothing installed.
    if std::env::var_os("WTM_ONLY_MANAGED").is_some() {
        let only = std::env::join_paths(bin_dir()).ok()?;
        return search(name, only);
    }
    let extra = extra_dirs();
    if let Ok(joined) = std::env::join_paths(&extra)
        && let Some(p) = search(name, joined)
    {
        return Some(p);
    }
    which::which(name).ok()
}

fn search(name: &str, paths: OsString) -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    which::which_in(name, Some(paths), cwd).ok()
}

fn extra_dirs() -> Vec<PathBuf> {
    // The programs the app downloaded for itself come first.
    let mut dirs: Vec<PathBuf> = bin_dir().into_iter().collect();
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        dirs.push(parent.to_path_buf());
    }
    if cfg!(target_os = "macos") {
        dirs.push("/opt/homebrew/bin".into());
        dirs.push("/usr/local/bin".into());
    }
    if cfg!(windows)
        && let Some(local) = dirs::data_local_dir()
    {
        dirs.push(local.join("Microsoft").join("WinGet").join("Links"));
    }
    dirs
}
