// Don't open a console window alongside the app on Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod fonts;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use eframe::egui;
use wtm_core::{
    CancelToken, DownloadError, DownloadOptions, Event, OutputFormat, Progress, QUALITY_PRESETS,
    SetupProgress, Tools, VideoInfo,
};

/// Messages sent from worker threads back to the UI thread.
enum Msg {
    Info(Result<VideoInfo, String>),
    Progress(Progress),
    Processing,
    Done(Result<PathBuf, DownloadError>),
    Updated(Result<String, String>),
    Setup(SetupProgress),
    SetupDone(Result<(), String>),
}

#[derive(PartialEq)]
enum Busy {
    Idle,
    Fetching,
    Downloading,
    Updating,
    /// First launch: fetching yt-dlp and ffmpeg.
    SettingUp,
}

/// What the last finished job left for the user to see.
enum Outcome {
    None,
    Saved(PathBuf),
    Failed(String),
    Cancelled,
    Updated(String),
}

struct App {
    ctx: egui::Context,
    tools: Result<Tools, String>,
    /// Cached because reading it spawns a process, which we must not do per frame.
    ytdlp_version: String,
    url: String,
    info: Option<VideoInfo>,
    quality: usize,
    format: OutputFormat,
    out_dir: PathBuf,

    busy: Busy,
    progress: Progress,
    processing: bool,
    setup_progress: Option<SetupProgress>,
    outcome: Outcome,
    cancel: Option<CancelToken>,

    tx: Sender<Msg>,
    rx: Receiver<Msg>,
}

impl App {
    /// `url` is an optional link to look up straight away.
    fn new(ctx: egui::Context, url: Option<String>) -> Self {
        let (tx, rx) = mpsc::channel();
        let mut app = Self {
            ctx,
            tools: Err(String::new()),
            ytdlp_version: String::new(),
            url: url.clone().unwrap_or_default(),
            info: None,
            quality: 0,
            format: OutputFormat::default(),
            out_dir: wtm_core::default_download_dir(),
            busy: Busy::Idle,
            progress: Progress::default(),
            processing: false,
            setup_progress: None,
            outcome: Outcome::None,
            cancel: None,
            tx,
            rx,
        };
        app.refresh_tools();
        if wtm_core::needs_setup() {
            app.start_setup();
        } else if url.is_some() {
            app.fetch_info();
        }
        app
    }

    fn refresh_tools(&mut self) {
        self.tools = Tools::discover();
        self.ytdlp_version = match &self.tools {
            Ok(t) => t.ytdlp_version().unwrap_or_else(|| "unknown".into()),
            Err(_) => String::new(),
        };
    }

    /// Runs `job` on a worker thread; it reports back through `tx` and wakes the UI.
    fn spawn(&self, job: impl FnOnce(Sender<Msg>, egui::Context) + Send + 'static) {
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        thread::spawn(move || job(tx, ctx));
    }

    fn drain_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Info(Ok(info)) => {
                    self.info = Some(info);
                    self.busy = Busy::Idle;
                }
                Msg::Info(Err(e)) => {
                    self.info = None;
                    self.outcome = Outcome::Failed(e);
                    self.busy = Busy::Idle;
                }
                Msg::Progress(p) => {
                    self.progress = p;
                    self.processing = false;
                }
                Msg::Processing => self.processing = true,
                Msg::Done(result) => {
                    self.outcome = match result {
                        Ok(path) => Outcome::Saved(path),
                        Err(DownloadError::Cancelled) => Outcome::Cancelled,
                        Err(DownloadError::Failed(e)) => Outcome::Failed(e),
                    };
                    self.cancel = None;
                    self.busy = Busy::Idle;
                }
                Msg::Setup(p) => self.setup_progress = Some(p),
                Msg::SetupDone(result) => {
                    self.busy = Busy::Idle;
                    self.setup_progress = None;
                    match result {
                        Ok(()) => {
                            self.refresh_tools();
                            if !self.url.trim().is_empty() {
                                self.fetch_info();
                            }
                        }
                        Err(e) => self.outcome = Outcome::Failed(e),
                    }
                }
                Msg::Updated(result) => {
                    self.busy = Busy::Idle;
                    match result {
                        Ok(version) => {
                            self.refresh_tools();
                            self.outcome = Outcome::Updated(version);
                        }
                        Err(e) => self.outcome = Outcome::Failed(e),
                    }
                }
            }
        }
    }

    fn fetch_info(&mut self) {
        let Ok(tools) = self.tools.clone() else { return };
        let url = self.url.trim().to_string();
        if url.is_empty() {
            return;
        }
        self.busy = Busy::Fetching;
        self.outcome = Outcome::None;
        self.info = None;
        self.spawn(move |tx, ctx| {
            let result = wtm_core::fetch_info(&tools, &url).map_err(|e| format!("{e:#}"));
            let _ = tx.send(Msg::Info(result));
            ctx.request_repaint();
        });
    }

    fn start_download(&mut self) {
        let Ok(tools) = self.tools.clone() else { return };
        let opts = DownloadOptions {
            url: self.url.trim().to_string(),
            out_dir: self.out_dir.clone(),
            format: self.format,
            max_height: QUALITY_PRESETS[self.quality].1,
        };
        if opts.url.is_empty() {
            return;
        }
        let cancel = CancelToken::new();
        self.cancel = Some(cancel.clone());
        self.busy = Busy::Downloading;
        self.progress = Progress::default();
        self.processing = false;
        self.outcome = Outcome::None;
        self.spawn(move |tx, ctx| {
            let events = (tx.clone(), ctx.clone());
            let result = wtm_core::download(&tools, &opts, &cancel, |event| {
                let _ = events.0.send(match event {
                    Event::Progress(p) => Msg::Progress(p),
                    Event::Processing => Msg::Processing,
                });
                events.1.request_repaint();
            });
            let _ = tx.send(Msg::Done(result));
            ctx.request_repaint();
        });
    }

    /// Downloads whatever is missing (yt-dlp, ffmpeg) so nothing needs installing by hand.
    fn start_setup(&mut self) {
        self.busy = Busy::SettingUp;
        self.setup_progress = None;
        self.outcome = Outcome::None;
        self.spawn(|tx, ctx| {
            let events = (tx.clone(), ctx.clone());
            let result = wtm_core::run_setup(|p| {
                let _ = events.0.send(Msg::Setup(p));
                events.1.request_repaint();
            })
            .map_err(|e| format!("{e:#}"));
            let _ = tx.send(Msg::SetupDone(result));
            ctx.request_repaint();
        });
    }

    fn start_update(&mut self) {
        self.busy = Busy::Updating;
        self.outcome = Outcome::None;
        self.spawn(|tx, ctx| {
            let result = wtm_core::update_ytdlp().map_err(|e| format!("{e:#}"));
            let _ = tx.send(Msg::Updated(result));
            ctx.request_repaint();
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_messages();
        let idle = self.busy == Busy::Idle;

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(4.0);
            ui.heading("WTM Video Downloader");
            ui.label("Download videos from YouTube and other sites.");
            ui.add_space(10.0);

            if self.busy == Busy::SettingUp || self.tools.is_err() {
                self.setup_screen(ui);
                return;
            }

            // URL row
            let mut submit = false;
            ui.horizontal(|ui| {
                let edit = ui.add_enabled(
                    idle,
                    egui::TextEdit::singleline(&mut self.url)
                        .hint_text("Paste a video link")
                        .desired_width(ui.available_width() - 90.0),
                );
                submit = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let has_url = !self.url.trim().is_empty();
                if ui.add_enabled(idle && has_url, egui::Button::new("Look up")).clicked() {
                    submit = true;
                }
            });
            if submit && idle {
                self.fetch_info();
            }

            if self.busy == Busy::Fetching {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Looking up video…");
                });
            }
            if let Some(info) = &self.info {
                ui.add_space(6.0);
                ui.group(|ui| {
                    ui.set_width(ui.available_width());
                    ui.strong(&info.title);
                    ui.label(format!(
                        "{}  ·  {}",
                        info.uploader.as_deref().unwrap_or("unknown uploader"),
                        info.duration_label().unwrap_or_else(|| "unknown length".into()),
                    ));
                });
            }

            ui.add_space(10.0);
            ui.add_enabled_ui(idle, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Format");
                    egui::ComboBox::from_id_salt("format")
                        .selected_text(self.format.label())
                        .show_ui(ui, |ui| {
                            for f in OutputFormat::ALL {
                                ui.selectable_value(&mut self.format, f, f.label());
                            }
                        });
                    ui.add_enabled_ui(!self.format.is_audio(), |ui| {
                        ui.label("Quality");
                        egui::ComboBox::from_id_salt("quality")
                            .selected_text(QUALITY_PRESETS[self.quality].0)
                            .show_ui(ui, |ui| {
                                for (i, (label, _)) in QUALITY_PRESETS.iter().enumerate() {
                                    ui.selectable_value(&mut self.quality, i, *label);
                                }
                            });
                    });
                });
                if let Some(note) = self.format.note() {
                    ui.small(note);
                }
                ui.horizontal(|ui| {
                    ui.label("Save to");
                    ui.label(self.out_dir.display().to_string());
                    if ui.button("Change…").clicked()
                        && let Some(dir) = rfd::FileDialog::new().set_directory(&self.out_dir).pick_folder()
                    {
                        self.out_dir = dir;
                    }
                });
            });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let can_download = idle && !self.url.trim().is_empty();
                if ui.add_enabled(can_download, egui::Button::new("Download")).clicked() {
                    self.start_download();
                }
                if self.busy == Busy::Downloading
                    && ui.button("Cancel").clicked()
                    && let Some(c) = &self.cancel
                {
                    c.cancel();
                }
            });

            if self.busy == Busy::Downloading {
                ui.add_space(6.0);
                self.progress_bar(ui);
            }
            self.status_area(ui);

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                self.footer(ui, idle);
            });
        });
    }
}

impl App {
    fn setup_screen(&mut self, ui: &mut egui::Ui) {
        if self.busy == Busy::SettingUp {
            ui.label("Getting ready. This only happens the first time, and it needs an internet connection.");
            ui.add_space(10.0);
            match &self.setup_progress {
                Some(p) => {
                    ui.label(p.label);
                    match p.fraction() {
                        Some(f) => {
                            let text = match p.total {
                                Some(total) => format!("{} / {}", format_bytes(p.done), format_bytes(total)),
                                None => String::new(),
                            };
                            ui.add(egui::ProgressBar::new(f).text(text));
                        }
                        None => {
                            ui.spinner();
                        }
                    }
                }
                None => {
                    ui.spinner();
                }
            }
            return;
        }

        // Setup didn't finish, or nothing could be set up automatically.
        let problem = match (&self.outcome, &self.tools) {
            (Outcome::Failed(msg), _) => Some(msg.as_str()),
            (_, Err(msg)) => Some(msg.as_str()),
            _ => None,
        };
        if let Some(msg) = problem {
            ui.colored_label(egui::Color32::LIGHT_RED, msg);
        }
        ui.add_space(8.0);
        if ui.button("Try again").clicked() {
            self.start_setup();
        }
    }

    fn progress_bar(&self, ui: &mut egui::Ui) {
        if self.processing {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Processing (merging / converting)…");
            });
            return;
        }
        let p = &self.progress;
        let mut detail = format_bytes(p.downloaded);
        if let Some(total) = p.total {
            detail = format!("{detail} / {}", format_bytes(total));
        }
        if let Some(speed) = p.speed {
            detail = format!("{detail}  ·  {}/s", format_bytes(speed as u64));
        }
        if let Some(eta) = p.eta_secs {
            detail = format!("{detail}  ·  {eta}s left");
        }
        match p.fraction() {
            Some(f) => ui.add(egui::ProgressBar::new(f).text(detail).show_percentage()),
            None => ui.add(egui::ProgressBar::new(0.0).text(detail).animate(true)),
        };
    }

    fn status_area(&mut self, ui: &mut egui::Ui) {
        if self.busy == Busy::Updating {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Updating yt-dlp…");
            });
        }
        ui.add_space(6.0);
        match &self.outcome {
            Outcome::None => {}
            Outcome::Cancelled => {
                ui.label("Cancelled.");
            }
            Outcome::Updated(version) => {
                ui.label(format!("yt-dlp is now at {version}."));
            }
            Outcome::Failed(msg) => {
                ui.colored_label(egui::Color32::LIGHT_RED, msg);
                ui.label("If downloads keep failing, YouTube has probably changed — try Update yt-dlp below.");
            }
            Outcome::Saved(path) => {
                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::LIGHT_GREEN, "Saved.");
                    if ui.button("Show in folder").clicked() {
                        reveal(path);
                    }
                });
                ui.small(path.display().to_string());
            }
        }
    }

    fn footer(&mut self, ui: &mut egui::Ui, idle: bool) {
        let Ok(tools) = &self.tools else { return };
        let version = self.ytdlp_version.clone();
        let has_ffmpeg = tools.ffmpeg.is_some();
        ui.horizontal(|ui| {
            ui.small(format!("yt-dlp {version}"));
            if ui.add_enabled(idle, egui::Button::new("Update yt-dlp").small()).clicked() {
                self.start_update();
            }
            if !has_ffmpeg {
                ui.colored_label(egui::Color32::YELLOW, "ffmpeg not found — limited quality, no mp3");
                if ui.add_enabled(idle, egui::Button::new("Install ffmpeg").small()).clicked() {
                    self.start_setup();
                }
            }
        });
    }
}

fn format_bytes(n: u64) -> String {
    const MB: f64 = 1_000_000.0;
    let n = n as f64;
    if n >= 1000.0 * MB {
        format!("{:.2} GB", n / (1000.0 * MB))
    } else if n >= MB {
        format!("{:.1} MB", n / MB)
    } else {
        format!("{:.0} KB", n / 1000.0)
    }
}

/// Opens the file manager with the saved file selected.
fn reveal(path: &Path) {
    use std::process::Command;
    let _ = if cfg!(target_os = "macos") {
        Command::new("open").arg("-R").arg(path).spawn()
    } else if cfg!(windows) {
        Command::new("explorer").arg("/select,").arg(path).spawn()
    } else {
        Command::new("xdg-open").arg(path.parent().unwrap_or(path)).spawn()
    };
}

fn main() -> eframe::Result {
    // Optional: `wtm-video-gui <url>` opens with that link already looked up.
    let start_url = std::env::args().skip(1).find(|a| !a.starts_with('-'));

    // Window, taskbar and Dock icon. Swap assets/icon.png to change it.
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/icon.png"))
        .expect("assets/icon.png is a valid PNG");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_icon(icon)
            // Lets Linux desktops match the window to its launcher entry and icon.
            .with_app_id("wtm-video")
            .with_inner_size([560.0, 460.0])
            .with_min_inner_size([420.0, 380.0]),
        ..Default::default()
    };
    eframe::run_native(
        "WTM Video Downloader",
        options,
        Box::new(|cc| {
            fonts::install(&cc.egui_ctx);
            Ok(Box::new(App::new(cc.egui_ctx.clone(), start_url)))
        }),
    )
}
