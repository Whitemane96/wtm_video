//! First-run setup: downloads the programs the app drives (yt-dlp and ffmpeg)
//! into the app's own folder, so nobody has to install anything by hand.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::process::command;
use crate::tools::{Tools, install_hint, managed_ytdlp_path};

/// Where a step of the setup has got to, for a progress bar.
#[derive(Debug, Clone)]
pub struct SetupProgress {
    pub label: &'static str,
    pub done: u64,
    /// Unknown for steps like unpacking.
    pub total: Option<u64>,
}

impl SetupProgress {
    pub fn fraction(&self) -> Option<f32> {
        let total = self.total.filter(|t| *t > 0)?;
        Some((self.done as f32 / total as f32).clamp(0.0, 1.0))
    }
}

/// The app's private data folder. Set `WTM_DATA_DIR` to use another location,
/// for example to keep everything on a USB stick.
pub fn data_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("WTM_DATA_DIR") {
        return Some(PathBuf::from(dir));
    }
    dirs::data_local_dir().map(|d| d.join("wtm_video"))
}

/// Where downloaded programs live.
pub fn bin_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join("bin"))
}

/// True when yt-dlp or ffmpeg can't be found anywhere on this computer.
pub fn needs_setup() -> bool {
    Tools::discover().map(|t| t.ffmpeg.is_none()).unwrap_or(true)
}

/// Downloads whichever of yt-dlp and ffmpeg is missing. Blocks, so call it from
/// a worker thread.
pub fn run_setup(mut on_progress: impl FnMut(SetupProgress)) -> Result<()> {
    if Tools::discover().is_err() {
        install_ytdlp(&mut on_progress)?;
    }
    if !Tools::discover().is_ok_and(|t| t.ffmpeg.is_some()) {
        install_ffmpeg(&mut on_progress)?;
    }
    Ok(())
}

/// The standalone yt-dlp build for this machine, if the project publishes one.
fn ytdlp_asset() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some("yt-dlp_macos")
    } else if cfg!(windows) {
        Some("yt-dlp.exe")
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        Some("yt-dlp_linux")
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
        Some("yt-dlp_linux_aarch64")
    } else {
        None
    }
}

pub(crate) fn install_ytdlp(on_progress: &mut impl FnMut(SetupProgress)) -> Result<()> {
    let dest = managed_ytdlp_path().context("could not determine an app data folder")?;
    let asset = ytdlp_asset().context(
        "there is no standalone yt-dlp build for this system; \
         install yt-dlp with your package manager or pip instead",
    )?;
    let url = format!("https://github.com/yt-dlp/yt-dlp/releases/latest/download/{asset}");
    fs::create_dir_all(dest.parent().context("no parent dir")?)?;

    // Download to a temporary name so a failed download never leaves a broken program.
    let tmp = dest.with_extension("download");
    let result = download_file(&url, &tmp, "Downloading yt-dlp", on_progress).and_then(|()| {
        make_executable(&tmp)?;
        fs::rename(&tmp, &dest).context("could not put yt-dlp in place")
    });
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Static ffmpeg builds, chosen for small size. Returns the download URL and
/// the name of the program inside the archive.
fn ffmpeg_source() -> Option<(&'static str, &'static str)> {
    if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        Some(("https://ffmpeg.martin-riedl.de/redirect/latest/macos/arm64/release/ffmpeg.zip", "ffmpeg"))
    } else if cfg!(target_os = "macos") {
        Some(("https://ffmpeg.martin-riedl.de/redirect/latest/macos/amd64/release/ffmpeg.zip", "ffmpeg"))
    } else if cfg!(windows) {
        // The x64 build also runs on Windows on ARM.
        Some(("https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip", "ffmpeg.exe"))
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        Some(("https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz", "ffmpeg"))
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
        Some(("https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-arm64-static.tar.xz", "ffmpeg"))
    } else {
        None
    }
}

pub(crate) fn install_ffmpeg(on_progress: &mut impl FnMut(SetupProgress)) -> Result<()> {
    let Some((url, binary)) = ffmpeg_source() else {
        bail!("there is no automatic ffmpeg download for this system.\n{}", install_hint());
    };
    let bin = bin_dir().context("could not determine an app data folder")?;
    fs::create_dir_all(&bin)?;
    let archive = bin.join("ffmpeg-download.tmp");
    let unpack = bin.join("ffmpeg-unpack");

    let result = (|| -> Result<()> {
        download_file(url, &archive, "Downloading ffmpeg", on_progress)?;
        on_progress(SetupProgress { label: "Unpacking ffmpeg", done: 0, total: None });

        let member = pick_member(&list_archive(&archive)?, binary)
            .with_context(|| format!("{binary} was not found inside the download"))?;
        let _ = fs::remove_dir_all(&unpack);
        fs::create_dir_all(&unpack)?;
        extract_member(&archive, &unpack, &member)?;

        let dest = bin.join(binary);
        let _ = fs::remove_file(&dest);
        fs::rename(unpack.join(&member), &dest).context("could not put ffmpeg in place")?;
        make_executable(&dest)?;

        // Make sure it really runs here before we rely on it.
        let runs = command(&dest).arg("-version").output().is_ok_and(|o| o.status.success());
        if !runs {
            let _ = fs::remove_file(&dest);
            bail!("the downloaded ffmpeg does not run on this computer");
        }
        Ok(())
    })();

    let _ = fs::remove_file(&archive);
    let _ = fs::remove_dir_all(&unpack);
    result
}

/// The archive entry for `binary`, wherever it sits. Release archives put it in
/// a folder named after the version, which changes with every release.
fn pick_member(names: &[String], binary: &str) -> Option<String> {
    names
        .iter()
        .map(|n| n.trim())
        .filter(|n| !n.ends_with('/') && !n.ends_with('\\'))
        .filter(|n| n.rsplit(['/', '\\']).next() == Some(binary))
        .min_by_key(|n| n.len())
        .map(str::to_string)
}

/// The system's own `tar` can read both .zip and .tar.xz, so no extra
/// libraries are needed. On Windows use the built-in one by full path, since a
/// GNU tar earlier on the PATH (from Git, say) can't read zip files.
fn tar_program() -> PathBuf {
    if cfg!(windows) {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        PathBuf::from(root).join("System32").join("tar.exe")
    } else {
        PathBuf::from("tar")
    }
}

fn list_archive(archive: &Path) -> Result<Vec<String>> {
    let out = command(&tar_program())
        .arg("-tf")
        .arg(archive)
        .output()
        .context("could not run tar to open the download")?;
    if !out.status.success() {
        bail!("could not open the download: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect())
}

fn extract_member(archive: &Path, into: &Path, member: &str) -> Result<()> {
    let out = command(&tar_program())
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(into)
        .arg(member)
        .output()
        .context("could not run tar to unpack the download")?;
    if !out.status.success() {
        bail!("could not unpack the download: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

/// Downloads `url` to `dest`, reporting progress as it goes.
fn download_file(
    url: &str,
    dest: &Path,
    label: &'static str,
    on_progress: &mut impl FnMut(SetupProgress),
) -> Result<()> {
    let response = ureq::get(url).call().with_context(|| format!("downloading {url}"))?;
    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    let mut body = response.into_body().into_reader();
    let mut file = File::create(dest).context("could not create a temporary file")?;

    let mut buffer = vec![0u8; 64 * 1024];
    let (mut done, mut reported) = (0u64, 0u64);
    on_progress(SetupProgress { label, done, total });
    loop {
        let n = body.read(&mut buffer).context("the download was interrupted")?;
        if n == 0 {
            break;
        }
        file.write_all(&buffer[..n])?;
        done += n as u64;
        if done - reported >= 256 * 1024 {
            reported = done;
            on_progress(SetupProgress { label, done, total });
        }
    }
    file.flush()?;
    if let Some(total) = total
        && done != total
    {
        bail!("the download was cut short ({done} of {total} bytes)");
    }
    on_progress(SetupProgress { label, done, total });
    Ok(())
}

fn make_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn finds_the_program_inside_a_versioned_folder() {
        // Layouts of the real archives, whose folder names change every release.
        let windows = names(&["ffmpeg-9.0.2-essentials_build/", "ffmpeg-9.0.2-essentials_build/bin/", "ffmpeg-9.0.2-essentials_build/bin/ffmpeg.exe", "ffmpeg-9.0.2-essentials_build/bin/ffprobe.exe"]);
        assert_eq!(
            pick_member(&windows, "ffmpeg.exe").as_deref(),
            Some("ffmpeg-9.0.2-essentials_build/bin/ffmpeg.exe")
        );
        let linux = names(&["ffmpeg-7.0.2-amd64-static/ffprobe", "ffmpeg-7.0.2-amd64-static/ffmpeg", "ffmpeg-7.0.2-amd64-static/manpages/ffmpeg.txt"]);
        assert_eq!(pick_member(&linux, "ffmpeg").as_deref(), Some("ffmpeg-7.0.2-amd64-static/ffmpeg"));
        assert_eq!(pick_member(&names(&["ffmpeg"]), "ffmpeg").as_deref(), Some("ffmpeg"));
    }

    #[test]
    fn ignores_folders_lookalikes_and_mac_metadata() {
        let list = names(&["ffmpeg/", "__MACOSX/._ffmpeg", "docs/ffmpeg-notes.txt", "bin/ffmpeg"]);
        assert_eq!(pick_member(&list, "ffmpeg").as_deref(), Some("bin/ffmpeg"));
        assert_eq!(pick_member(&names(&["a/ffprobe"]), "ffmpeg"), None);
    }

    #[test]
    fn prefers_the_shallowest_match() {
        let list = names(&["x/y/z/ffmpeg", "x/ffmpeg"]);
        assert_eq!(pick_member(&list, "ffmpeg").as_deref(), Some("x/ffmpeg"));
    }

    #[test]
    fn this_platform_has_a_download_source() {
        // Fails loudly if someone builds for a target we have no ffmpeg build for.
        if cfg!(any(target_os = "macos", windows, all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")))) {
            assert!(ffmpeg_source().is_some());
            assert!(ytdlp_asset().is_some());
        }
    }

    /// Downloads the real programs (about 70 MB), so it only runs on request:
    ///   WTM_DATA_DIR=/some/scratch/folder cargo test -p wtm_core -- --ignored real_install
    /// It refuses to run without WTM_DATA_DIR so it can't touch your real setup.
    #[test]
    #[ignore = "downloads about 70 MB"]
    fn real_install_puts_working_programs_in_the_data_dir() {
        let dir = std::env::var_os("WTM_DATA_DIR").expect("set WTM_DATA_DIR to a scratch folder");
        let bin = bin_dir().expect("bin dir");
        assert!(bin.starts_with(&dir));

        let mut steps = Vec::new();
        let mut record = |p: SetupProgress| {
            if steps.last() != Some(&p.label) {
                steps.push(p.label);
            }
        };
        install_ytdlp(&mut record).expect("yt-dlp installs");
        install_ffmpeg(&mut record).expect("ffmpeg installs");
        println!("steps reported: {steps:?}");

        let exe = if cfg!(windows) { ".exe" } else { "" };
        for name in ["yt-dlp", "ffmpeg"] {
            let path = bin.join(format!("{name}{exe}"));
            assert!(path.exists(), "{name} missing");
            let ok = command(&path).arg(if name == "ffmpeg" { "-version" } else { "--version" }).output().is_ok_and(|o| o.status.success());
            assert!(ok, "{name} does not run");
        }
        let leftovers: Vec<_> = fs::read_dir(&bin).unwrap().flatten().map(|e| e.file_name()).collect();
        println!("files in bin: {leftovers:?}");
        assert!(!bin.join("ffmpeg-download.tmp").exists() && !bin.join("ffmpeg-unpack").exists(), "temporary files were not cleaned up");
    }

    #[test]
    fn progress_fraction_handles_unknown_totals() {
        let known = SetupProgress { label: "x", done: 25, total: Some(100) };
        assert_eq!(known.fraction(), Some(0.25));
        assert_eq!(SetupProgress { label: "x", done: 5, total: None }.fraction(), None);
        assert_eq!(SetupProgress { label: "x", done: 5, total: Some(0) }.fraction(), None);
    }
}
