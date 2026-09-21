# WTM Video Downloaded

A video downloader for YouTube and other sites, with both a CLI and a GUI. Written in Rust, for macOS, Windows and Linux.

It's a friendly front-end over [yt-dlp](https://github.com/yt-dlp/yt-dlp) and [ffmpeg](https://ffmpeg.org): yt-dlp does the site-specific work, and this project adds the interface, progress reporting, cancelling, and keeping yt-dlp up to date.

> **Use it responsibly.** Only download content you have the right to: your own videos, Creative Commons or public-domain material, or anything the owner has given permission for. Downloading from YouTube and many other sites can go against their terms of service.

## Features

- Video download via URL.
- File types: MP4, MKV, WebM or MOV video, or MP3, M4A, Opus, FLAC or WAV audio.
- Up to 4K quality.
- Looks up the title, uploader and length before downloading.
- Manages its own copy of yt-dlp and updates it with one click or one flag.
- Sets itself up: downloads yt-dlp and ffmpeg on first launch, so there's nothing to install by hand.

## Download

If you just want to use the app, you don't need Rust or a terminal. Open the **Releases** page of this repository and download the file for your computer:

| You use | Download |
|---|---|
| Mac (Apple Silicon or Intel) | `WTM-Video-macOS.zip` |
| Windows | `WTM-Video-Windows.zip` |
| Linux | `WTM-Video-linux-x86_64.tar.gz` |

Unzip it and open the app. Two things to expect:

- **A one-time security warning.** These downloads aren't code-signed (signing costs money every year), so macOS and Windows warn you the first time. It's safe to continue. Each download has a **READ ME FIRST** note with the exact clicks.
- **A short first-launch setup.** The app downloads yt-dlp and ffmpeg (around 100 MB in total) the first time it starts, so it needs an internet connection. After that it opens straight away.

Files named `wtm-video-cli-…` are the command-line version.

## Requirements

This section is for building from source or using the command-line tool without the app's first-launch setup.

| | macOS | Windows | Linux |
|---|---|---|---|
| Rust (to build) | [rustup.rs](https://rustup.rs) | [rustup.rs](https://rustup.rs) plus the MSVC build tools | [rustup.rs](https://rustup.rs) plus a C compiler and some libraries (see Build) |
| ffmpeg | `brew install ffmpeg` | `winget install Gyan.FFmpeg`, or put `ffmpeg.exe` next to the program | `sudo apt install ffmpeg` (Debian/Ubuntu), `sudo dnf install ffmpeg` (Fedora), `sudo pacman -S ffmpeg` (Arch) |
| yt-dlp | downloaded by the app on first use | downloaded by the app on first use | downloaded by the app on first use (x86-64 and ARM64) |

ffmpeg is needed for everything except plain MP4, and for high-quality MP4 too. Without it, MP4 falls back to single-file formats (lower quality), and every other format is unavailable. The app tells you when it's missing. You don't need to install ffmpeg or yt-dlp yourself: the app downloads them on first launch, and `wtm-video --update` does the same for the command line.

## Build

On Linux, install the build libraries first. On Debian or Ubuntu:

```sh
sudo apt install build-essential pkg-config libxkbcommon-dev libwayland-dev libx11-dev \
  libxcursor-dev libxrandr-dev libxi-dev libgl1-mesa-dev \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
```

Other distributions have equivalent packages. If the build stops on a missing library, install its `-dev` (or `-devel`) package.

```sh
cargo build --release
```

The programs land in `target/release/`:

- `wtm-video` (`wtm-video.exe` on Windows), the CLI
- `wtm-video-gui` (`wtm-video-gui.exe` on Windows), the desktop app

### Installing on Linux

```sh
./scripts/install-linux.sh              # installs under ~/.local and adds a menu entry
./scripts/install-linux.sh --uninstall  # removes it again
```

The app then appears in your application menu with its icon, and `wtm-video` and `wtm-video-gui` work from a terminal (make sure `~/.local/bin` is on your `PATH`).

## Usage

### Desktop app

```sh
cargo run --release -p wtm_gui
```

Paste a link, optionally press **Look up** to preview it, choose the format, quality and folder, then press **Download**.

You can also start the app with a link ready: `wtm-video-gui <url>`. **Update yt-dlp** at the bottom refreshes the downloader.

If yt-dlp isn't installed yet, the app shows a **Download yt-dlp** button on first launch.

### Command line

```sh
wtm-video [OPTIONS] <URLS>...
```

| Option | Meaning |
|---|---|
| `-o, --output <DIR>` | Where to save files (default: your Downloads folder) |
| `-q, --quality <HEIGHT>` | Maximum video height, e.g. `1080` or `720` (default: best available) |
| `-f, --format <FORMAT>` | File type: `mp4` (default), `mkv`, `webm`, `mov`, `mp3`, `m4a`, `opus`, `flac`, `wav` |
| `-a, --audio-only` | Shorthand for `--format mp3` |
| `-i, --info` | Print title, uploader and length instead of downloading |
| `-u, --update` | Install or update the app's own yt-dlp, then exit unless URLs are given |

Examples:

```sh
wtm-video "https://www.youtube.com/watch?v=..."
wtm-video -q 720 -o ~/Videos "<url1>" "<url2>"
wtm-video -f mov -q 1080 "<url>"
wtm-video -f flac "<url>"
wtm-video -a "<url>"          # same as -f mp3
wtm-video --update
```

Files are named `Title [video-id].ext`. The exit code is non-zero if any URL failed.

## Choosing a format

| Format | Best for | Notes |
|---|---|---|
| MP4 | Playing anywhere | Prefers H.264/AAC, so 1080p and below play on nearly everything. 1440p and 4K are only offered by YouTube as VP9 or AV1, which some players can't open. |
| MOV | QuickTime, Final Cut, iMovie | Always H.264/AAC, so it's usually limited to 1080p. |
| MKV | Best quality, no re-encoding | Keeps the best streams available (often VP9/AV1 and Opus). Some players can't open it. |
| WebM | Web use | Keeps YouTube's original VP9 and Opus streams. |
| MP3, M4A, Opus | Music and podcasts | M4A and Opus are usually saved without re-encoding. |
| FLAC, WAV | Editing or archiving | Lossless, so files are much larger. They can't add quality that the source didn't have. |

## How yt-dlp is found

In order:

1. The app's own copy, in the per-user app-data folder: `~/Library/Application Support/wtm_video/bin/` on macOS, `%LOCALAPPDATA%\wtm_video\bin\` on Windows, `~/.local/share/wtm_video/bin/` on Linux.
2. Next to the program's executable, then common install locations (Homebrew on macOS, WinGet on Windows).
3. Your `PATH`.

The app's own copy comes first on purpose: package managers often lag behind YouTube's changes, and an old yt-dlp is the most common reason downloads fail. ffmpeg is looked up in the same places.

Two advanced switches, mostly for testing:

- `WTM_DATA_DIR=/some/folder` keeps the app's downloaded programs somewhere else, for example on a USB stick.
- `WTM_ONLY_MANAGED=1` ignores programs installed elsewhere and uses only the app's own copies, which shows how the app behaves on a computer with nothing installed.

## Troubleshooting

**`HTTP Error 403: Forbidden`, or "Requested format is not available".** YouTube has changed something and your yt-dlp is out of date. Run `wtm-video --update` (or press **Update yt-dlp**) and try again.

**"yt-dlp was not found" or the setup screen shows an error.** The first-launch download failed, usually because there was no internet connection. Press **Try again** in the app, or run `wtm-video --update`.

**"ffmpeg is required to save as …".** Press **Install ffmpeg** at the bottom of the app, run `wtm-video --update`, or install ffmpeg yourself (see Requirements).

**Sign-in, age-restricted or members-only videos.**

## Project layout

```
crates/core   download logic shared by both front-ends
crates/cli    the wtm-video command-line tool
crates/gui    the wtm-video-gui desktop app (egui)
assets/       app icon (icon.png, plus icon.ico for Windows and icon-512.png for Linux)
scripts/      make-icons.py, bundle-macos.sh, install-linux.sh, package-macos.sh, package-linux.sh
packaging/    the READ ME FIRST notes that go inside each download
```

Platform-specific handling lives in `crates/core`:

- **Finding programs:** covers the case where a Finder-launched Mac app doesn't inherit the shell `PATH`.
- **Windows console windows:** they're hidden when the app starts yt-dlp or ffmpeg.
- **Cancelling:** it ends the whole process tree (`taskkill /T` on Windows, a process group on macOS and Linux), because the standalone yt-dlp starts a second process. Ctrl+C in the CLI does the same.

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

GitHub Actions (`.github/workflows/ci.yml`) runs clippy, tests and a release build on macOS, Windows and Linux for every push, and uploads the built programs as artifacts.

### Making a release

Pushing a version tag builds the downloads for every platform and attaches them to a GitHub release:

```sh
git tag v0.1.0
git push origin v0.1.0
```

`.github/workflows/release.yml` does the work (macOS as one universal app for Apple Silicon and Intel, a Windows zip, and a Linux archive). You can also build a package on your own machine with `./scripts/package-macos.sh` or `./scripts/package-linux.sh`; the results land in `dist/`.

## Linux notes

- **Wayland and X11** are both supported by the window toolkit.
- **The "Change…" folder button** uses your desktop's file chooser through `xdg-desktop-portal`. GNOME, KDE and most other desktops have this by default; a minimal window manager may need `xdg-desktop-portal` and a backend installed.
- **Japanese, Chinese and Korean titles** show as squares if no East Asian font is installed, which is common on minimal installs. Add one, for example `sudo apt install fonts-noto-cjk`.
- **Unusual CPUs.** yt-dlp publishes a standalone build for x86-64 and ARM64 Linux only. On anything else, install yt-dlp with your package manager and the app will find it on your `PATH`.
