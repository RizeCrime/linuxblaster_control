// #![allow(unused)]

use linuxblaster_control::{
    BlasterXG6, ConnectError, DEFAULT_BASE_PATH, FeatureId, LinkState,
    MIC_EQ_PRESETS, SINAD_VOLUME_CAP, UDEV_RULE, UDEV_RULE_PATH, ValueKind,
    system,
};
use eframe::egui::{
    self, Button, Color32, RichText, Vec2, Vec2b,
};
use eframe::egui::{
    Align, DragValue, Grid, Layout, ScrollArea, Slider, Widget,
};
use egui_plot::{CoordinatesFormatter, Corner, GridInput, GridMark, Line, Plot, PlotPoints, log_grid_spacer};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use std::cmp::Reverse;
use std::sync::{LazyLock, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tracing::{debug, error, warn};

use crate::{AUTOEQ_DB, AutoEqDb, HeadphoneResult};

#[macro_use]
#[path = "macros.rs"]
mod macros;

const ISO_BANDS: [f64; 10] = [31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];

static UI_SELECTED: LazyLock<Mutex<&'static str>> =
    LazyLock::new(|| Mutex::new("SBX"));
static AUTOEQ_MODAL: LazyLock<Mutex<bool>> =
    LazyLock::new(|| Mutex::new(false));
static SEARCH_QUERY: LazyLock<Mutex<String>> =
    LazyLock::new(|| Mutex::new(String::new()));
static SEARCH_RESULTS: LazyLock<Mutex<Vec<&'static str>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static PROFILE_NAME: LazyLock<Mutex<String>> =
    LazyLock::new(|| Mutex::new(String::new()));
static LAST_MIXER_REFRESH: LazyLock<Mutex<Instant>> =
    LazyLock::new(|| Mutex::new(Instant::now()));
static LAST_DEVICE_MONITOR: LazyLock<Mutex<Instant>> =
    LazyLock::new(|| Mutex::new(Instant::now()));
static DEVICE_LINK: LazyLock<Mutex<LinkState>> =
    LazyLock::new(|| Mutex::new(LinkState::Connected));
static DEVICE_RECONNECTS: LazyLock<Mutex<u64>> =
    LazyLock::new(|| Mutex::new(0));
/// Write count that has already been persisted, if any.
static LAST_SAVED_WRITES: LazyLock<Mutex<Option<u64>>> =
    LazyLock::new(|| Mutex::new(None));
static LAST_STATE_SAVE: LazyLock<Mutex<Instant>> =
    LazyLock::new(|| Mutex::new(Instant::now()));

/// Persisted System-pane settings. Loaded once; saved on every change.
static SYSTEM_SETTINGS: LazyLock<Mutex<system::Settings>> =
    LazyLock::new(|| Mutex::new(system::Settings::load()));
/// Cached `pactl` state, refreshed every couple of seconds.
static SYSTEM_STATUS: LazyLock<Mutex<system::Status>> =
    LazyLock::new(|| Mutex::new(system::Status::default()));
static LAST_SYSTEM_REFRESH: LazyLock<Mutex<Option<Instant>>> =
    LazyLock::new(|| Mutex::new(None));
/// Slider value while the user drags it; avoids a stale poll fighting it.
static SYSTEM_VOLUME_UI: LazyLock<Mutex<f32>> =
    LazyLock::new(|| Mutex::new(100.0));
static SYSTEM_VOLUME_EDITING: LazyLock<Mutex<bool>> =
    LazyLock::new(|| Mutex::new(false));
static LAST_SYSTEM_VOLUME_WRITE: LazyLock<Mutex<Instant>> =
    LazyLock::new(|| Mutex::new(Instant::now()));
/// Background installer job. PipeWire is restarted by it, which should
/// not freeze the UI thread.
type InstallJob = JoinHandle<Result<String, String>>;
static INSTALL_JOB: LazyLock<Mutex<Option<InstallJob>>> =
    LazyLock::new(|| Mutex::new(None));
static INSTALL_MESSAGE: LazyLock<Mutex<Option<Result<String, String>>>> =
    LazyLock::new(|| Mutex::new(None));
/// The installer restarts PipeWire; the G6 sink disappears for a moment
/// and `Status::refresh` would report it as missing. Hide errors for a
/// short settle window after the job ends.
static STATUS_ERROR_SETTLE_UNTIL: LazyLock<Mutex<Option<Instant>>> =
    LazyLock::new(|| Mutex::new(None));
/// One-time startup gain staging, once the first system poll is in.
static STARTUP_POLICY_APPLIED: LazyLock<Mutex<bool>> =
    LazyLock::new(|| Mutex::new(false));

/// How often the UI checks for state the device pushed on its own
/// (hardware buttons, cascading changes after writes).
const DEVICE_POLL_INTERVAL: Duration = Duration::from_millis(250);
/// The hardware mixer can also change outside of this app
/// (volume knob, pavucontrol, ...), so it's re-read periodically.
const MIXER_REFRESH_INTERVAL: Duration = Duration::from_secs(1);
/// The G6 re-enumerates after Direct Mode and can be unplugged at any time.
/// A cheap ping every two seconds keeps the link (and the state display)
/// honest without flooding the control endpoint.
const DEVICE_MONITOR_INTERVAL: Duration = Duration::from_secs(2);
/// Writes the last-state snapshot at most this often while a setting is
/// being dragged. Slider drags emit a write on every frame.
const LAST_STATE_SAVE_INTERVAL: Duration = Duration::from_secs(2);
/// `pactl` spawns a process per call, so the system snapshot is cached.
const SYSTEM_REFRESH_INTERVAL: Duration = Duration::from_secs(2);
/// Slider drags emit changes on every frame; throttle `pactl` writes.
const SYSTEM_VOLUME_WRITE_INTERVAL: Duration = Duration::from_millis(50);


pub struct BlasterApp(pub BlasterXG6);

impl eframe::App for BlasterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(debug_assertions)]
        if ctx.input(|i| i.key_pressed(egui::Key::D)) {
            ctx.set_debug_on_hover(!ctx.debug_on_hover());
        }

        self.0.process_device_events();
        poll_device(&self.0);
        persist_last_state(&self.0);
        poll_install_job(ctx);
        poll_system(ctx);
        apply_startup_gain_policy(&self.0);
        {
            let mut last_refresh = LAST_MIXER_REFRESH.lock().unwrap();
            if last_refresh.elapsed() >= MIXER_REFRESH_INTERVAL {
                self.0.read_mixer_state();
                enforce_sync_policy(&self.0);
                *last_refresh = Instant::now();
            }
        }
        ctx.request_repaint_after(DEVICE_POLL_INTERVAL);

        egui::TopBottomPanel::top("top_panel")
            .resizable(false)
            .exact_height(56.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    // Reset All Button
                    if ui.button("Reset All").clicked() {
                        let _ = self.0.reset();
                    }

                    // Profile Management 
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Load Profile").clicked() {
                            let Some(path) = rfd::FileDialog::new() 
                                .add_filter("Profile", &["json"])
                                .set_directory(DEFAULT_BASE_PATH.join("profiles"))
                                .pick_file()
                            else {
                                debug!("No path selected");
                                return;
                            };

                            if let Err(error) = self.0.apply_profile(path.clone()) {
                                error!("Failed to apply profile from file");
                                error!("Path: {}", path.display());
                                error!("Error: {}", error);
                            }
                        }
                        
                        if ui.button("Save Profile").clicked() {
                            let Some(path) = rfd::FileDialog::new()
                                .set_file_name("profile.json")
                                .add_filter("Profile", &["json"])
                                .set_directory(DEFAULT_BASE_PATH.join("profiles"))
                                .save_file()
                            else {
                                debug!("No path selected");
                                return;
                            };

                            if let Err(error) = self.0.save_profile(path.clone()) {
                                error!("Failed to save profile to file");
                                error!("Path: {}", path.display());
                                error!("Error: {}", error);
                            }
                        }
                    });
                });
            },
        );
        egui::TopBottomPanel::bottom("g6_status")
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let link = *DEVICE_LINK.lock().unwrap();
                    let (color, text) = match link {
                        LinkState::Connected => (
                            Color32::from_rgb(80, 200, 120),
                            "G6 connected",
                        ),
                        LinkState::Reconnected => (
                            Color32::from_rgb(230, 190, 80),
                            "G6 reconnected",
                        ),
                        LinkState::Disconnected => (
                            Color32::from_rgb(230, 100, 100),
                            "G6 disconnected — retrying",
                        ),
                    };
                    ui.colored_label(color, "●");
                    ui.label(text);

                    ui.separator();

                    let direct = direct_mode_on(&self.0);
                    let sbx =
                        self.0.feature(FeatureId::SbxMaster).value() > 0.5;
                    let scout =
                        self.0.feature(FeatureId::ScoutMode).value() > 0.5;
                    ui.label(format!(
                        "Direct Mode: {}",
                        if direct { "on" } else { "off" }
                    ));
                    ui.separator();
                    ui.label(format!(
                        "SBX: {}",
                        if sbx { "on" } else { "off" }
                    ));
                    ui.separator();
                    ui.label(format!(
                        "Scout: {}",
                        if scout { "on" } else { "off" }
                    ));

                    ui.separator();
                    let output = self.0.feature(FeatureId::Output).value();
                    let output = if output > 0.5 {
                        "Headphones"
                    } else if output == 0.0 {
                        "Speakers"
                    } else {
                        "unknown"
                    };
                    ui.label(format!("Output: {}", output));

                    ui.separator();
                    let sink = {
                        let status = SYSTEM_STATUS.lock().unwrap();
                        match status.default_sink.as_deref() {
                            Some(system::VIRTUAL_SINK_NAME) => {
                                "7.1".to_owned()
                            }
                            Some(name)
                                if Some(name)
                                    == status.g6_sink.as_deref() =>
                            {
                                "G6".to_owned()
                            }
                            Some(_) => "other".to_owned(),
                            None => "?".to_owned(),
                        }
                    };
                    ui.label(format!("Sink: {}", sink));

                    ui.with_layout(
                        Layout::right_to_left(Align::Center),
                        |ui| {
                            ui.label(format!(
                                "Reconnects: {}",
                                *DEVICE_RECONNECTS.lock().unwrap()
                            ));
                        },
                    );
                });
            });
        let direct = direct_mode_on(&self.0);
        egui::SidePanel::left("left_panel")
            .resizable(false)
            .show(ctx, |ui| {
                nav_pane(&self.0, ui, "SBX", Some(FeatureId::SbxMaster), true);
                nav_pane(&self.0, ui, "Playback", Some(FeatureId::Output), true);
                nav_pane(&self.0, ui, "Recording", None, true);
                nav_pane(&self.0, ui, "Decoder", None, true);
                nav_pane(&self.0, ui, "Mixer", None, true);
                nav_pane(&self.0, ui, "System", None, true);
                nav_pane(&self.0, ui, "Lighting", Some(FeatureId::LightingToggle), true);
                ui.add_enabled_ui(!direct, |ui| {
                    nav_pane(
                        &self.0,
                        ui,
                        "Scout Mode",
                        Some(FeatureId::ScoutMode),
                        false,
                    );
                });
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            let state = *UI_SELECTED.lock().unwrap();
            match *UI_SELECTED.lock().unwrap() {
                "SBX" => {
                    if direct {
                        // The AutoEQ wizard edits the EQ, which Direct Mode
                        // bypasses. Close it instead of leaving a disabled
                        // modal with no way out.
                        *AUTOEQ_MODAL.lock().unwrap() = false;
                        direct_mode_notice(
                            ui,
                            "Direct Mode bypasses SBX and EQ.",
                        );
                    }
                    ui.add_enabled_ui(!direct, |ui| {
                        if *AUTOEQ_MODAL.lock().unwrap() {
                            autoeq_pane(&self.0, ui);
                        } else {
                            sbx_pane(&self.0, ui);
                        }
                    });
                }
                "Scout Mode" => {
                    if direct {
                        direct_mode_notice(
                            ui,
                            "Direct Mode disables Scout Mode.",
                        );
                    }
                }
                "Decoder" => {
                    if direct {
                        direct_mode_notice(
                            ui,
                            "Direct Mode bypasses the decoder.",
                        );
                    }
                    ui.add_enabled_ui(!direct, |ui| {
                        decoder_pane(&self.0, ui);
                    });
                }
                "Mixer" => {
                    mixer_pane(&self.0, ui);
                }
                "System" => {
                    system_pane(&self.0, ui);
                }
                "Lighting" => {
                    lighting_pane(&self.0, ui);
                }
                "Playback" => {
                    playback_pane(&self.0, ui);
                }
                "Recording" => {
                    if direct {
                        direct_mode_notice(
                            ui,
                            "Direct Mode disables the DSP voice path \
                             (Voice Clarity, Mic EQ, AEC).",
                        );
                    }
                    ui.add_enabled_ui(!direct, |ui| {
                        recording_pane(&self.0, ui);
                    });
                }
                _ => {
                    warn!("Unknown UI selected: {}", state);
                }
            }
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        persist_on_exit(&self.0);
    }
}

/// Saves the current state after a change, at most once every two seconds.
/// This keeps slider drags from writing the snapshot on every frame.
fn persist_last_state(blaster: &BlasterXG6) {
    let writes = blaster.write_count();

    let mut saved_writes = LAST_SAVED_WRITES.lock().unwrap();
    if *saved_writes == Some(writes) {
        return;
    }

    let mut last_save = LAST_STATE_SAVE.lock().unwrap();
    if last_save.elapsed() < LAST_STATE_SAVE_INTERVAL {
        return;
    }
    *last_save = Instant::now();
    drop(last_save);

    match blaster.save_last_state() {
        Ok(()) => *saved_writes = Some(writes),
        Err(error) => error!("Couldn't save the last state: {}", error),
    }
}

/// The normal exit path. A periodic save may be up to two seconds old.
fn persist_on_exit(blaster: &BlasterXG6) {
    if let Err(error) = blaster.save_last_state() {
        error!("Couldn't save the last state on exit: {}", error);
    }
}

fn sync_volume_enabled() -> bool {
    SYSTEM_SETTINGS.lock().unwrap().sync_volume
}

fn sinad_fix_enabled() -> bool {
    SYSTEM_SETTINGS.lock().unwrap().sinad_fix
}

fn save_system_settings(settings: &system::Settings) {
    if let Err(error) = settings.save() {
        error!("Couldn't save the System settings: {}", error);
    }
}

/// Refreshes the cached `pactl` state at most every
/// [`SYSTEM_REFRESH_INTERVAL`]. The UI reads the cache, never `pactl`.
fn poll_system(ctx: &egui::Context) {
    let due = {
        let mut last = LAST_SYSTEM_REFRESH.lock().unwrap();
        match *last {
            Some(instant)
                if instant.elapsed() < SYSTEM_REFRESH_INTERVAL =>
            {
                false
            }
            _ => {
                *last = Some(Instant::now());
                true
            }
        }
    };
    if !due {
        return;
    }

    let status = system::Status::refresh();
    if !*SYSTEM_VOLUME_EDITING.lock().unwrap()
        && let Some(volume) = status.active_volume
    {
        *SYSTEM_VOLUME_UI.lock().unwrap() = volume;
    }
    *SYSTEM_STATUS.lock().unwrap() = status;
    ctx.request_repaint();
}

/// A blocking refresh for actions that just changed the system state
/// (sink switch, checkbox toggles). These are user-initiated, so the
/// short `pactl` round trip is preferable to showing stale state.
fn refresh_system_now() {
    let status = system::Status::refresh();
    if !*SYSTEM_VOLUME_EDITING.lock().unwrap()
        && let Some(volume) = status.active_volume
    {
        *SYSTEM_VOLUME_UI.lock().unwrap() = volume;
    }
    *SYSTEM_STATUS.lock().unwrap() = status;
    *LAST_SYSTEM_REFRESH.lock().unwrap() = Some(Instant::now());
}

fn poll_install_job(ctx: &egui::Context) {
    let running = {
        let job = INSTALL_JOB.lock().unwrap();
        match job.as_ref() {
            Some(handle) => !handle.is_finished(),
            None => return,
        }
    };
    if running {
        ctx.request_repaint_after(Duration::from_millis(100));
        return;
    }

    let handle = INSTALL_JOB.lock().unwrap().take();
    if let Some(handle) = handle {
        let result = handle.join().unwrap_or_else(|_| {
            Err("The installer thread panicked.".to_owned())
        });
        *INSTALL_MESSAGE.lock().unwrap() = Some(result);
        *STATUS_ERROR_SETTLE_UNTIL.lock().unwrap() =
            Some(Instant::now() + Duration::from_secs(3));
        *LAST_SYSTEM_REFRESH.lock().unwrap() = None;
        ctx.request_repaint();
    }
}

/// Applies the persisted volume policy once per connection. The cap is
/// applied immediately; sync mode then converts the current hardware
/// volume into the active sink volume without a loudness jump.
fn apply_startup_gain_policy(blaster: &BlasterXG6) {
    let mut applied = STARTUP_POLICY_APPLIED.lock().unwrap();
    if *applied {
        return;
    }
    *applied = true;
    drop(applied);

    let settings = *SYSTEM_SETTINGS.lock().unwrap();
    blaster.set_volume_cap(if settings.sinad_fix {
        Some(SINAD_VOLUME_CAP)
    } else {
        None
    });
    if settings.sinad_fix {
        let _ = blaster.enforce_volume_cap();
    }
    if settings.sync_volume {
        apply_single_gain_stage(blaster);
        enforce_sync_policy(blaster);
    }
}

/// Moves the variable gain into the active PipeWire sink and pins the
/// G6 hardware Speaker control at the SINAD cap (or 0 dB if the fix is
/// off). The sink volume is adjusted so the product of both stages stays
/// the same.
fn apply_single_gain_stage(blaster: &BlasterXG6) {
    let settings = *SYSTEM_SETTINGS.lock().unwrap();
    if !settings.sync_volume {
        return;
    }

    let target_hw = if settings.sinad_fix {
        SINAD_VOLUME_CAP
    } else {
        1.0
    };
    let status = SYSTEM_STATUS.lock().unwrap().clone();
    let Some(active) = status.active_sink().map(str::to_owned) else {
        return;
    };
    let hardware = blaster.feature(FeatureId::SpeakerVolume).value();

    if !hardware.is_nan()
        && let Ok(sink) = system::sink_volume(&active)
    {
        let target_sink =
            (sink * hardware / target_hw).clamp(0.0, 100.0);
        if (target_sink - sink).abs() > 0.05
            && let Err(error) =
                system::set_sink_volume(&active, target_sink)
        {
            warn!(
                "Couldn't preserve the system volume while syncing: {}",
                error
            );
        }
    }

    blaster.set_volume_cap(if settings.sinad_fix {
        Some(SINAD_VOLUME_CAP)
    } else {
        None
    });
    if !hardware.is_nan()
        && let Err(error) =
            blaster.set_feature(FeatureId::SpeakerVolume, Some(target_hw))
    {
        warn!("Couldn't set the fixed hardware volume: {}", error);
    }
}

/// Keeps sync mode coherent while it is active: the inactive G6 gain
/// stage stays at 100% and the hardware Speaker stays at the cap (or
/// 0 dB). A hardware knob move is corrected on the next mixer refresh.
fn enforce_sync_policy(blaster: &BlasterXG6) {
    let settings = *SYSTEM_SETTINGS.lock().unwrap();
    if !settings.sync_volume {
        return;
    }
    if !linuxblaster_control::mixer::is_available() {
        return;
    }

    let status = SYSTEM_STATUS.lock().unwrap().clone();
    if let Some(active) = status.active_sink() {
        if active == system::VIRTUAL_SINK_NAME
            && let Some(g6) = status.g6_sink.as_deref()
            && status
                .g6_volume
                .is_some_and(|volume| (volume - 100.0).abs() > 0.5)
        {
            let _ = system::set_sink_volume(g6, 100.0);
        } else if active != system::VIRTUAL_SINK_NAME
            && status.virtual_installed
            && status
                .virtual_volume
                .is_some_and(|volume| (volume - 100.0).abs() > 0.5)
        {
            let _ = system::set_sink_volume(
                system::VIRTUAL_SINK_NAME,
                100.0,
            );
        }
    }

    let target = if settings.sinad_fix {
        SINAD_VOLUME_CAP
    } else {
        1.0
    };
    let current = blaster.feature(FeatureId::SpeakerVolume).value();
    if !current.is_nan() && (current - target).abs() > 1e-4 {
        let _ = blaster.set_feature(FeatureId::SpeakerVolume, Some(target));
    }
}

fn system_pane(blaster: &BlasterXG6, ui: &mut egui::Ui) {
    let mut status = SYSTEM_STATUS.lock().unwrap().clone();
    let mut settings = *SYSTEM_SETTINGS.lock().unwrap();
    let install_busy = INSTALL_JOB.lock().unwrap().is_some();
    let install_message = INSTALL_MESSAGE.lock().unwrap().clone();
    let settling = STATUS_ERROR_SETTLE_UNTIL
        .lock()
        .unwrap()
        .is_some_and(|until| Instant::now() < until);
    if install_busy || settling {
        status.error = None;
    }

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.vertical_centered_justified(|ui| {
                section_header(ui, "Virtual 7.1 sink (headphones)");
                ui.label(
                    RichText::new(
                        "The virtual sink is a PipeWire filter-chain, not the \
                         G6's DSP: 7.1 PCM arrives at the sink and is \
                         downmixed by a 1:1 code port of the Sound BlasterX \
                         G6 downmixer APO (CMatrixEncoderEfxMod from \
                         KSUSBAPO64.dll). The port runs in f64 instead of the \
                         original f32 — the only difference.",
                    )
                    .color(Color32::GRAY),
                );
                ui.add_space(6.0);

                Grid::new("system_sink_status")
                    .num_columns(2)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("Sink");
                        if status.virtual_installed {
                            ui.colored_label(
                                Color32::from_rgb(80, 200, 120),
                                "installed",
                            );
                        } else if status.config_present {
                            ui.colored_label(
                                Color32::from_rgb(230, 190, 80),
                                "config present, not loaded",
                            );
                        } else {
                            ui.colored_label(
                                Color32::from_rgb(230, 100, 100),
                                "not installed",
                            );
                        }
                        ui.end_row();

                        ui.label("Default output");
                        let default = match status.default_sink.as_deref()
                        {
                            Some(system::VIRTUAL_SINK_NAME) => {
                                "Virtual 7.1".to_owned()
                            }
                            Some(name)
                                if Some(name)
                                    == status.g6_sink.as_deref() =>
                            {
                                "G6 Pro (direct)".to_owned()
                            }
                            Some(name) => name.to_owned(),
                            None => "unknown".to_owned(),
                        };
                        ui.label(default);
                        ui.end_row();

                        ui.label("Plugin");
                        let plugin_label = status
                            .plugin
                            .as_ref()
                            .and_then(|path| path.file_name())
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "not found".to_owned());
                        ui.label(plugin_label).on_hover_text(
                            status
                                .plugin
                                .as_ref()
                                .map(|path| path.display().to_string())
                                .unwrap_or_else(|| {
                                    "Bundled with the app; install copies it \
                                     to ~/.local/lib/linuxblaster"
                                        .to_owned()
                                }),
                        );
                        ui.end_row();

                        ui.label("Config");
                        let config_label = if status.config_present {
                            "written"
                        } else {
                            "not written"
                        };
                        ui.label(config_label).on_hover_text(
                            system::config_path().display().to_string(),
                        );
                        ui.end_row();

                        if let Some(error) = &status.error {
                            ui.label("Problem");
                            ui.colored_label(
                                Color32::from_rgb(230, 100, 100),
                                error,
                            );
                            ui.end_row();
                        }
                    });

                ui.add_space(6.0);
                if install_busy {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            RichText::new(
                                "Installing and reloading PipeWire... \
                                 audio drops for a moment.",
                            )
                            .color(Color32::GRAY),
                        );
                    });
                } else {
                    let can_install = status.g6_sink.is_some();
                    let label = if status.virtual_installed {
                        "Reinstall / repair virtual 7.1 sink"
                    } else {
                        "Install virtual 7.1 sink"
                    };
                    let button = ui.add_enabled(
                        can_install,
                        Button::new(label),
                    );
                    if button.clicked() {
                        *INSTALL_MESSAGE.lock().unwrap() = None;
                        let handle = std::thread::spawn(
                            system::install_virtual_sink,
                        );
                        *INSTALL_JOB.lock().unwrap() = Some(handle);
                        *LAST_SYSTEM_REFRESH.lock().unwrap() = None;
                    }
                    if !can_install {
                        ui.label(
                            RichText::new(
                                "Connect the G6 first: the installer needs \
                                 the sink name of the attached device.",
                            )
                            .color(Color32::from_rgb(230, 190, 80)),
                        );
                    }
                }

                if let Some(result) = &install_message {
                    match result {
                        Ok(message) => {
                            ui.colored_label(
                                Color32::from_rgb(80, 200, 120),
                                message,
                            );
                        }
                        Err(message) => {
                            ui.colored_label(
                                Color32::from_rgb(230, 100, 100),
                                message,
                            );
                        }
                    }
                }

                ui.add_space(8.0);
                let mut use_virtual = status.default_sink.as_deref()
                    == Some(system::VIRTUAL_SINK_NAME);
                let can_switch = status.virtual_installed
                    && status.g6_sink.is_some()
                    && !install_busy;
                let switch = ui.add_enabled(
                    can_switch,
                    egui::Checkbox::new(
                        &mut use_virtual,
                        "Use virtual 7.1 sink (headphones)",
                    ),
                );
                if switch.changed() {
                    match system::switch_output(
                        use_virtual,
                        settings.sync_volume,
                    ) {
                        Ok(()) => {
                            refresh_system_now();
                            status =
                                SYSTEM_STATUS.lock().unwrap().clone();
                            if settings.sync_volume {
                                enforce_sync_policy(blaster);
                            }
                        }
                        Err(error) => {
                            SYSTEM_STATUS.lock().unwrap().error =
                                Some(error);
                            status =
                                SYSTEM_STATUS.lock().unwrap().clone();
                        }
                    }
                }
                ui.label(
                    RichText::new(
                        "Off routes the default sink back to the G6 Pro \
                         device. Current application streams are moved with it.",
                    )
                    .color(Color32::GRAY),
                );

                // If another sink owns the default, offer a direct route
                // so the checkbox is not a two-click puzzle.
                let g6_is_default = status
                    .g6_sink
                    .as_deref()
                    .is_some_and(|g6| {
                        status.default_sink.as_deref() == Some(g6)
                    });
                if status.virtual_installed
                    && !g6_is_default
                    && status.default_sink.as_deref()
                        != Some(system::VIRTUAL_SINK_NAME)
                    && ui.button("Use G6 Pro (direct)").clicked()
                {
                    match system::switch_output(
                        false,
                        settings.sync_volume,
                    ) {
                        Ok(()) => {
                            refresh_system_now();
                            status =
                                SYSTEM_STATUS.lock().unwrap().clone();
                        }
                        Err(error) => {
                            SYSTEM_STATUS.lock().unwrap().error =
                                Some(error);
                            status =
                                SYSTEM_STATUS.lock().unwrap().clone();
                        }
                    }
                }

                ui.add_space(10.0);
                section_header(ui, "Volume");
                if ui
                    .checkbox(
                        &mut settings.sync_volume,
                        "Sync volume with the active system sink",
                    )
                    .changed()
                {
                    save_system_settings(&settings);
                    *SYSTEM_SETTINGS.lock().unwrap() = settings;
                    if settings.sync_volume {
                        refresh_system_now();
                        status =
                            SYSTEM_STATUS.lock().unwrap().clone();
                        apply_single_gain_stage(blaster);
                        enforce_sync_policy(blaster);
                        refresh_system_now();
                        status =
                            SYSTEM_STATUS.lock().unwrap().clone();
                    }
                }
                ui.label(
                    RichText::new(
                        "The active PipeWire sink is the single volume \
                         control. The G6 hardware Speaker stays at the \
                         SINAD cap (or 0 dB if the fix is off), so there is \
                         only one variable gain stage before the DAC.",
                    )
                    .color(Color32::GRAY),
                );

                if settings.sync_volume {
                    match status.active_sink().map(str::to_owned) {
                        Some(active) => {
                            let mut volume =
                                *SYSTEM_VOLUME_UI.lock().unwrap();
                            let active_label = if active
                                == system::VIRTUAL_SINK_NAME
                            {
                                "System volume (virtual 7.1)"
                            } else {
                                "System volume (G6 Pro)"
                            };

                            Grid::new("system_volume_grid")
                                .num_columns(2)
                                .spacing([12.0, 4.0])
                                .show(ui, |ui| {
                                    ui.label(active_label);
                                    let drag = ui.add(drag_value!(
                                        &mut volume,
                                        suffix = "%",
                                        range = 0.0..=100.0
                                    ));
                                    ui.end_row();

                                    ui.label("");
                                    fit_slider_to_column(ui);
                                    let slider = ui.add(slider!(
                                        &mut volume,
                                        vertical = false
                                    ));
                                    ui.end_row();

                                    let stopped = drag.drag_stopped()
                                        || slider.drag_stopped()
                                        || drag.lost_focus();
                                    let interacting = drag.dragged()
                                        || slider.dragged();
                                    *SYSTEM_VOLUME_EDITING
                                        .lock()
                                        .unwrap() = interacting;

                                    if (drag.changed() || slider.changed())
                                        && (drag.dragged()
                                            || drag.drag_stopped()
                                            || drag.lost_focus()
                                            || slider.dragged()
                                            || slider.drag_stopped())
                                    {
                                        *SYSTEM_VOLUME_UI
                                            .lock()
                                            .unwrap() = volume;
                                        let should_write = {
                                            let mut last =
                                                LAST_SYSTEM_VOLUME_WRITE
                                                    .lock()
                                                    .unwrap();
                                            if stopped
                                                || last.elapsed()
                                                    >= SYSTEM_VOLUME_WRITE_INTERVAL
                                            {
                                                *last = Instant::now();
                                                true
                                            } else {
                                                false
                                            }
                                        };
                                        if should_write
                                            && let Err(error) =
                                                system::set_sink_volume(
                                                    &active,
                                                    volume,
                                                )
                                        {
                                            warn!(
                                                "Couldn't set the system volume: {}",
                                                error
                                            );
                                        }
                                    }
                                    if stopped {
                                        *SYSTEM_VOLUME_EDITING
                                            .lock()
                                            .unwrap() = false;
                                    }
                                });

                            let hardware = blaster
                                .feature(FeatureId::SpeakerVolume)
                                .value();
                            let hardware_db = FeatureId::SpeakerVolume
                                .alsa_element()
                                .filter(|_| !hardware.is_nan())
                                .and_then(|(element, direction)| {
                                    linuxblaster_control::mixer::volume_db(
                                        element,
                                        direction,
                                        hardware,
                                    )
                                    .ok()
                                });
                            let hardware_text = if hardware.is_nan() {
                                "unknown".to_owned()
                            } else if let Some(db) = hardware_db {
                                format!(
                                    "{:.0}% ({:+.2} dB)",
                                    hardware * 100.0,
                                    db
                                )
                            } else {
                                format!("{:.0}%", hardware * 100.0)
                            };
                            ui.label(format!(
                                "G6 hardware Speaker: {}",
                                hardware_text
                            ));
                        }
                        None => {
                            ui.colored_label(
                                Color32::from_rgb(230, 190, 80),
                                "The system default sink is not the G6 Pro \
                                 or the virtual sink. Choose one above to \
                                 use volume sync.",
                            );
                        }
                    }
                } else {
                    ui.label(
                        RichText::new(
                            "Enable sync to control the active system sink \
                             from here. The Mixer pane still exposes the G6 \
                             hardware controls.",
                        )
                        .color(Color32::GRAY),
                    );
                }

                ui.add_space(10.0);
                section_header(ui, "SINAD bug fix");
                if ui
                    .checkbox(
                        &mut settings.sinad_fix,
                        "Limit the G6 hardware volume to -2.00 dB (97%)",
                    )
                    .changed()
                {
                    save_system_settings(&settings);
                    *SYSTEM_SETTINGS.lock().unwrap() = settings;
                    blaster.set_volume_cap(if settings.sinad_fix {
                        Some(SINAD_VOLUME_CAP)
                    } else {
                        None
                    });
                    if settings.sync_volume {
                        apply_single_gain_stage(blaster);
                        enforce_sync_policy(blaster);
                        refresh_system_now();
                        status =
                            SYSTEM_STATUS.lock().unwrap().clone();
                    } else if settings.sinad_fix {
                        let _ = blaster.enforce_volume_cap();
                    }
                }
                ui.label(
                    RichText::new(
                        "Raw 124/128 on the G6's Speaker control is exactly \
                         -2.00 dB. The cap is enforced after the knob or \
                         another client changes it, and the Mixer slider \
                         stops at 97% while this is on.",
                    )
                    .color(Color32::GRAY),
                );

                ui.add_space(8.0);
                ui.label(
                    RichText::new(
                        "The virtual sink uses the g6ct LADSPA plugin, \
                         vendored under vendor/g6ct and licensed MIT like \
                         the app. It is compiled into this binary, so the \
                         installer needs no source tree and no download.",
                    )
                    .color(Color32::GRAY),
                );
            });
        });
}

/// Direct Mode's cached state. Unknown (NaN) counts as off; the monitor
/// refreshes the cache every couple of seconds.
fn direct_mode_on(blaster: &BlasterXG6) -> bool {
    let value = blaster.feature(FeatureId::DirectMode).value();
    !value.is_nan() && value > 0.5
}

/// Explains why a DSP pane is disabled while Direct Mode is on.
fn direct_mode_notice(ui: &mut egui::Ui, what: &str) {
    ui.label(
        RichText::new(format!(
            "{} Turn SBX on to leave Direct Mode.",
            what
        ))
        .color(Color32::from_rgb(230, 190, 80)),
    );
    ui.separator();
}

/// Periodic G6 state monitor.
///
/// * pings the control endpoint and reopens it after a re-enumeration
/// * refreshes the states the hardware buttons can change
/// * never lets SBX and Direct Mode stay on together
fn poll_device(blaster: &BlasterXG6) {
    let due = {
        let mut last = LAST_DEVICE_MONITOR.lock().unwrap();
        if last.elapsed() < DEVICE_MONITOR_INTERVAL {
            false
        } else {
            *last = Instant::now();
            true
        }
    };
    if !due {
        return;
    }

    let link = blaster.ensure_connected();
    if link == LinkState::Reconnected {
        *DEVICE_RECONNECTS.lock().unwrap() += 1;
    }
    *DEVICE_LINK.lock().unwrap() = link;

    if link == LinkState::Connected {
        // Hardware buttons can change these without a packet the GUI
        // understands. Re-read them so the gray-out and status stay true.
        blaster.feature(FeatureId::DirectMode).read_from_device();
        blaster.feature(FeatureId::SbxMaster).read_from_device();
        blaster.feature(FeatureId::ScoutMode).read_from_device();
    }

    if link != LinkState::Disconnected
        && direct_mode_on(blaster)
        && blaster.feature(FeatureId::SbxMaster).value() > 0.5
    {
        warn!(
            "G6 reports SBX and Direct Mode on together; leaving Direct Mode"
        );
        let _ = blaster.set_feature(FeatureId::DirectMode, Some(0.0));
    }
}

/// ## Navigation Pane 
/// **Args**:
/// - `blaster`: the BlasterXG6 instance
/// - `ui`: the egui::Ui instance
/// - `pane_name`: Display Name
/// - `feature_id`: the FeatureId of the feature to be toggled 
/// - `with_selector`: for when the Feature requires its own pane
fn nav_pane(
    blaster: &BlasterXG6, 
    ui: &mut egui::Ui, 
    pane_name: &'static str,
    feature_id: Option<FeatureId>, 
    with_selector: bool,
) {
    let feature = feature_id.map(|id| blaster.feature(id));

    ui.vertical_centered_justified(|ui| {
        ui.set_width(180.0);

        // one compact row per pane, so all panes fit without scrolling
        ui.horizontal(|ui| {
            ui.label(RichText::new(pane_name).strong());

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if with_selector {
                    let current_selection = *UI_SELECTED.lock().unwrap();
                    let is_selected = current_selection == pane_name;
                    let selector_button = Button::selectable(
                        is_selected,
                        RichText::new("➡"),
                    )
                    .min_size(Vec2::new(32.0, 24.0))
                    .frame_when_inactive(true);

                    if ui.add(selector_button).clicked() {
                        *UI_SELECTED.lock().unwrap() = pane_name;
                    }
                }

                #[allow(clippy::collapsible_if)]
                if let Some(feature) = feature {
                    let feature_value = feature.as_bool();
                    if toggle_button!(ui, feature_value).clicked() {
                        if blaster.set_feature(feature.id, None).is_err() {
                            error!("Failed to set feature: {:?}", feature.id);
                        }
                    }
                }
            });
        });

        ui.separator();

    });
}

fn sbx_pane(blaster: &BlasterXG6, ui: &mut egui::Ui) {
    ui.columns(2, |columns| {
        // SBX Features
        columns[0].with_layout(Layout::top_down_justified(Align::TOP), |ui| {
            sbx_features(blaster, ui);
        });
        // Equalizer Sliders
        columns[1].with_layout(Layout::top_down_justified(Align::TOP), |ui| {
            eq_features(blaster, ui);
        });
    });
}

fn eq_features(blaster: &BlasterXG6, ui: &mut egui::Ui) {
    let eq_bands: Vec<_> = FeatureId::EQ_ALL
        .iter()
        .map(|&id| blaster.feature(id))
        .collect();

    ui.vertical_centered_justified(|ui| {
        ui.horizontal(|ui| {
            if ui.button(RichText::new("Select AutoEq Profile").color(Color32::GRAY)).clicked() {
                *AUTOEQ_MODAL.lock().unwrap() = true;
            }
        });

        ui.separator();

        Grid::new("eq_grid").show(ui, |ui| {
            for band in &eq_bands {
                let mut value = band.value();
                let clean_name =
                    band.id.display_name().strip_prefix("EQ ").unwrap_or(band.id.display_name());
                let clean_name =
                    clean_name.split('-').next().unwrap_or(clean_name);

                ui.vertical_centered_justified(|ui| {
                    ui.add_sized(
                        [ui.available_width(), 24.0],
                        egui::Label::new(
                            RichText::new(clean_name).color(Color32::GRAY),
                        ),
                    );
                });

                let drag_value = ui.add(drag_value!(
                    &mut value,
                    suffix = " dB",
                    decimals = 1,
                    step = 0.1,
                    range = -12.0..=12.0
                ));
                let slider = ui.add(slider!(
                    &mut value,
                    vertical = false,
                    decimals = 1,
                    step = 0.1,
                    range = -12.0..=12.0
                ));

                // do not simplify this. 
                // only `.changed()` will trigger even when the slider modifies itself
                // for example when it rounds the number (which is necessary for the GUI)
                if (drag_value.changed() || slider.changed())
                    // to get the behavior I want, this is sadly necessary
                    && (drag_value.dragged()
                        || drag_value.drag_stopped()
                        || drag_value.lost_focus()
                        || slider.dragged()
                        || slider.drag_stopped())
                {
                    let _ = blaster.set_feature(
                        band.id, 
                        Some(value)
                    );
                }

                ui.end_row();
            }
        });
    });
}

fn sbx_features(blaster: &BlasterXG6, ui: &mut egui::Ui) {
    let eq_enabled = blaster.feature(FeatureId::EqToggle).as_bool();

    ui.vertical_centered_justified(|ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("SBX Features").color(Color32::GRAY));
        });
        ui.separator();

        Grid::new("sbx_grid").show(ui, |ui| {
            for &toggle_id in FeatureId::SBX_TOGGLES {
                let toggle_feature = blaster.feature(toggle_id);
                let slider_id = toggle_id
                    .paired_slider()
                    .expect("SBX toggle must have a paired slider");
                let slider_feature = blaster.feature(slider_id);
                let is_percentage = matches!(slider_id.value_kind(), ValueKind::Percentage);
                let mut slider_value = if is_percentage {
                    slider_feature.value() * 100.0
                } else {
                    slider_feature.value()
                };

                let toggle = toggle_button!(
                    ui, 
                    toggle_feature.as_bool(), 
                    toggle_feature.id.display_name(), 
                    width = full
                );

                let drag_value = ui.add(drag_value!(
                    &mut slider_value
                ));
                let slider = ui.add(slider!(
                    &mut slider_value,
                    vertical = false,
                ));
                if toggle.clicked() {
                    let _ = blaster.set_feature(toggle_id, None);
                }
                // do not simplify this. 
                // only `.changed()` will trigger even when the slider modifies itself
                // for example when it rounds the number (which is necessary for the GUI)
                if (drag_value.changed() || slider.changed())
                    // to get the behavior I want, this is sadly necessary
                    && (drag_value.dragged()
                        || drag_value.drag_stopped()
                        || drag_value.lost_focus()
                        || slider.dragged()
                        || slider.drag_stopped())
                {
                    let write_value = if is_percentage {
                        slider_value / 100.0
                    } else {
                        slider_value
                    };
                    let _ = blaster.set_feature(slider_id, Some(write_value));
                }
                ui.end_row();
            }
            preset_row(blaster, ui, FeatureId::SmartVolMode, "Smart Vol. Mode");
            ui.end_row();

            let eq_toggle = toggle_button!(ui, eq_enabled, "Equalizer", width = full);
            if eq_toggle.clicked() {
                let _ = blaster.set_feature(FeatureId::EqToggle, None);
            }
            ui.end_row();
        });

        // Ten Band EQ Plot
        ui.separator();
        let gains: [f32; 11] = std::array::from_fn(|index| {
            blaster.feature(FeatureId::EQ_ALL[index]).value()
        });
        ui.add(|ui: &mut egui::Ui| {
            let old_override = ui.visuals().override_text_color;
            ui.visuals_mut().override_text_color = Some(
                ui.visuals().text_color().gamma_multiply(0.45),
            );
            let resp = eq_plot!(ui, Some(gains), width = full, height = full);
            ui.visuals_mut().override_text_color = old_override;
            resp
        });
    });
}

fn autoeq_pane(blaster: &BlasterXG6, ui: &mut egui::Ui) {
    let mut search = SEARCH_QUERY.lock().unwrap();
    let db: AutoEqDb = AutoEqDb {
        results: Some(&AUTOEQ_DB),
    };

    ui.vertical_centered_justified(|ui| {
        // Header
        ui.horizontal(|ui| {
            if ui.button(RichText::new("Back")).clicked() {
                *AUTOEQ_MODAL.lock().unwrap() = false;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.heading(
                    RichText::new("Select AutoEq Profile").color(Color32::GRAY),
                );
            });
        });
        ui.separator();

        // Search Bar
        ui.horizontal(|ui| {
            ui.label("Search Headphones:");

            let response = ui.text_edit_singleline(&mut *search);
            if response.changed() {
                let matcher = SkimMatcherV2::default();
                let mut results: Vec<(i64, &'static str)> = Vec::new();

                if let Some(map) = db.results {
                    for key in map.keys() {
                        let Some(score) = matcher.fuzzy_match(key, &search) else { continue };
                        results.push((score, *key));
                    }
                }

                results.sort_unstable_by_key(|k| Reverse(k.0));

                *SEARCH_RESULTS.lock().unwrap() =
                    results.into_iter().take(50).map(|(_, key)| key).collect();
            }
        });
        ui.separator();

        // Search Results

        if search.is_empty() {
            ui.label("Enter search term to see results");
            return;
        }

        let results_cache = SEARCH_RESULTS.lock().unwrap();

        ScrollArea::vertical().show(ui, |ui| {
            if results_cache.is_empty() {
                ui.label("No results found");
                return;
            }

            for name in results_cache.iter() {
                let Some(results) = db.results.and_then(|map| map.get(name)) else { continue; };

                ui.collapsing(RichText::new(*name).strong(), |ui| {
                    for result in results.iter() {
                        ui.horizontal(|ui| {
                            // metadata
                            ui.vertical(|ui| {
                                ui.set_width(180.0);
                                ui.label(
                                    RichText::new(format!(
                                        "By: {}",
                                        result.tester
                                    ))
                                    .color(Color32::GRAY),
                                );
                                ui.label(
                                    RichText::new(format!(
                                        "Variant: {}",
                                        result.variant.unwrap_or("")
                                    ))
                                    .color(Color32::GRAY),
                                );
                                ui.label(
                                    RichText::new(format!(
                                        "Test Device: {}",
                                        result.test_device.unwrap_or("")
                                    ))
                                    .color(Color32::GRAY),
                                );
                                let apply_button = ui.button(RichText::new("Apply Profile"));
                                if apply_button.clicked() {
                                    // every band is written, otherwise 0 dB bands would keep their old value
                                    for (index, gain) in result.ten_band_eq.iter().enumerate() {
                                        let _ = blaster.set_feature(FeatureId::EQ_BANDS[index], Some(*gain));
                                    }
                                    let _ = blaster.set_feature(FeatureId::EqPreAmp, Some(result.preamp));
                                    *AUTOEQ_MODAL.lock().unwrap() = false;
                                }
                            });
                            ui.separator();

                            // eq curve
                            let plot =
                                Plot::new(format!("eq_curve_{}_{}_{}_{}", 
                                        name, 
                                        result.tester, 
                                        result.variant.unwrap_or(""), 
                                        result.test_device.unwrap_or("")
                                    ))
                                    .x_grid_spacer(log_grid_spacer(10))
                                    .x_axis_formatter(|x, _range| {
                                        let freq = 10.0_f64.powf(x.value);
                                        if freq >= 1000.0 {
                                            format!("{} kHz", freq / 1000.0)
                                        } else {
                                            format!("{} Hz", freq)
                                        }
                                    })
                                    .y_axis_min_width(40.0)
                                    .show_grid(true)
                                    .include_y(-12.0)
                                    .include_y(12.0)
                                    .include_x(20.0_f64.log10())
                                    .include_x(16000.0_f64.log10())
                                    .allow_scroll(false)
                                    .allow_zoom(false)
                                    .allow_drag(false)
                                    .allow_axis_zoom_drag(false)
                                    .allow_boxed_zoom(false)
                                    .height(80.0)
                                    .view_aspect(3.0);

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                plot.show(ui, |plot_ui| {
                                    let points: PlotPoints = (0..=500).map(|i| {
                                        let t = i as f64 / 500.0; 
                                        let f = 20.0 * (20000.0 / 20.0_f64).powf(t);
                                        
                                        let mut total_y = 0.0; 
                                        for (index, gain) in result.ten_band_eq.iter().enumerate() {
                                            let Some(&center_freq) = ISO_BANDS.get(index) else { continue };
                                            if gain.abs() <= 0.01 {
                                                continue;
                                            }
                                            total_y += calculate_peaking_eq_response(f, center_freq, *gain as f64, 1.41);
                                        }

                                        [f.log10(), total_y]
                                    }).collect();

                                    plot_ui.line(Line::new(format!("eq_curve_{}", name), points).width(2.0_f32));
                                });

                            });
                        });
                    }
                });

                ui.separator();
            }
        });
    });
}

fn calculate_peaking_eq_response(freq: f64, center_freq: f64, gain: f64, q: f64) -> f64 {
    let bandwidth = center_freq / q;
    let diff = (freq - center_freq).abs();
    let falloff = 1.0 / (1.0 + (diff / (bandwidth * 0.5)).powf(2.0));
    gain * falloff
}

fn playback_pane(blaster: &BlasterXG6, ui: &mut egui::Ui) {
    let output_toggle = blaster.feature(FeatureId::Output);

    ui.columns(2, |columns| {
        columns[0].with_layout(Layout::top_down_justified(Align::TOP), |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Output").color(Color32::GRAY));
            });
            ui.separator(); 

            ui.horizontal(|ui| {
                // Output value: 0.0 = Speakers, 1.0 = Headphones
                let headphones_active = output_toggle.as_bool();

                // Speaker Toggle Button
                let speaker_toggle = toggle_button!(
                    ui,
                    !headphones_active,
                    "Speakers",
                    width = 100.0,
                );
                // Headphone Toggle Button 
                let headphone_toggle = toggle_button!(
                    ui,
                    headphones_active,
                    "Headphones",
                    width = 100.0,
                );

                // yeah yeah code duplication, but it's easier to read this way
                if speaker_toggle.clicked() && headphones_active {
                    let _ = blaster.set_feature(output_toggle.id, Some(0.0));
                }
                // and quite maintainable at this scale 
                if headphone_toggle.clicked() && !headphones_active {
                    let _ = blaster.set_feature(output_toggle.id, Some(1.0));
                }
            });
        });

        columns[1].with_layout(Layout::top_down_justified(Align::TOP), |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("DAC Filter").color(Color32::GRAY));
            });
            ui.separator();
            
            preset_radio(blaster, ui, FeatureId::DacFilter);

        });
    });

}

/// Grid row: toggle button, percentage drag value, slider.
/// Shared by the Recording and Mixer panes.
fn toggle_slider_row(
    blaster: &BlasterXG6,
    ui: &mut egui::Ui,
    toggle_id: FeatureId,
    slider_id: FeatureId,
    label: &str,
) -> (egui::Response, egui::Response) {
    let toggle_feature = blaster.feature(toggle_id);
    let slider_feature = blaster.feature(slider_id);
    // NaN = couldn't be read (e.g. ALSA card not found)
    let toggle_available = !toggle_feature.value().is_nan();
    let slider_available = !slider_feature.value().is_nan();
    let is_speaker = slider_id == FeatureId::SpeakerVolume;
    // The SINAD fix makes 97% the highest selectable hardware volume.
    let slider_max = if is_speaker && sinad_fix_enabled() {
        SINAD_VOLUME_CAP * 100.0
    } else {
        100.0
    };
    let mut slider_value = if slider_available {
        (slider_feature.value() * 100.0).clamp(0.0, slider_max)
    } else {
        0.0
    };
    // In sync mode the active sink owns the volume; the hardware slider
    // is informational and must not introduce a second attenuation stage.
    let volume_available =
        slider_available && !(is_speaker && sync_volume_enabled());

    let toggle = ui
        .add_enabled_ui(toggle_available, |ui| {
            toggle_button!(ui, toggle_feature.as_bool(), label, width = full)
        })
        .inner;
    let drag_value = ui.add_enabled(
        volume_available,
        drag_value!(
            &mut slider_value,
            range = 0.0..=slider_max
        ),
    );
    fit_slider_to_column(ui);
    let slider = ui.add_enabled(
        volume_available,
        slider!(
            &mut slider_value,
            vertical = false,
            range = 0.0..=slider_max
        ),
    );
    if is_speaker && sync_volume_enabled() {
        slider.clone().on_hover_text(
            "Volume sync is on. Use System -> System volume; \
             the hardware stage is held at the safe level.",
        );
    }

    if toggle.clicked() {
        let _ = blaster.set_feature(toggle_id, None);
    }
    // do not simplify this. 
    // only `.changed()` will trigger even when the slider modifies itself
    // for example when it rounds the number (which is necessary for the GUI)
    if (drag_value.changed() || slider.changed())
        // to get the behavior I want, this is sadly necessary
        && (drag_value.dragged()
            || drag_value.drag_stopped()
            || drag_value.lost_focus()
            || slider.dragged()
            || slider.drag_stopped())
    {
        let _ = blaster.set_feature(slider_id, Some(slider_value / 100.0));
    }

    (drag_value, slider)
}

/// Sliders have a fixed default width, which overflows into the neighboring
/// column when the labels in front of them are wide. Call this right before
/// adding a slider as the last cell of a grid row, so it takes exactly the
/// width that's left in its column.
fn fit_slider_to_column(ui: &mut egui::Ui) {
    // inside a grid cell, `available_width()` is the remembered column width,
    // so measure up to the right edge of the surrounding column instead
    let remaining = ui.max_rect().right() - ui.cursor().min.x;
    let default_width = egui::Spacing::default().slider_width;
    ui.spacing_mut().slider_width = remaining.clamp(40.0, default_width);
}

/// Grid row: a single toggle button.
fn toggle_row(blaster: &BlasterXG6, ui: &mut egui::Ui, id: FeatureId, label: &str) {
    let feature = blaster.feature(id);
    let toggle = ui
        .add_enabled_ui(!feature.value().is_nan(), |ui| {
            toggle_button!(ui, feature.as_bool(), label, width = full)
        })
        .inner;
    if toggle.clicked() {
        let _ = blaster.set_feature(id, None);
    }
}

/// Grid row: label, then a dropdown with the preset entries.
fn preset_row(blaster: &BlasterXG6, ui: &mut egui::Ui, id: FeatureId, label: &str) {
    let ValueKind::Preset(labels) = id.value_kind() else {
        unreachable!("{:?} must be a Preset", id)
    };
    let current = blaster.feature(id).value();
    let current_label = labels.get(current as usize).filter(|_| !current.is_nan());

    // not justified: a wrapped label would get stretched letter spacing
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(label).color(Color32::GRAY));
    });
    ui.label("");
    // like fit_slider_to_column(): stay inside the surrounding column
    let remaining = ui.max_rect().right() - ui.cursor().min.x;
    egui::ComboBox::from_id_salt(id)
        .width(remaining.clamp(60.0, 150.0))
        .selected_text(current_label.copied().unwrap_or("Unknown"))
        .show_ui(ui, |ui| {
            for (index, &name) in labels.iter().enumerate() {
                let is_selected = current == index as f32;
                if ui.selectable_label(is_selected, name).clicked() && !is_selected {
                    let _ = blaster.set_feature(id, Some(index as f32));
                }
            }
        });
}

/// Vertical list of radio buttons for a Preset feature.
fn preset_radio(blaster: &BlasterXG6, ui: &mut egui::Ui, id: FeatureId) {
    let ValueKind::Preset(labels) = id.value_kind() else {
        unreachable!("{:?} must be a Preset", id)
    };
    let value = blaster.feature(id).value();
    // NaN (unknown) selects nothing
    let original = if value.is_nan() { usize::MAX } else { value as usize };

    let mut selected = original;
    for (index, &label) in labels.iter().enumerate() {
        ui.radio_value(&mut selected, index, label);
    }
    if selected != original {
        let _ = blaster.set_feature(id, Some(selected as f32));
    }
}

fn section_header(ui: &mut egui::Ui, title: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).color(Color32::GRAY));
    });
    ui.separator();
}

fn recording_pane(blaster: &BlasterXG6, ui: &mut egui::Ui) {
    ui.columns(2, |columns| {
        columns[0].with_layout(Layout::top_down_justified(Align::TOP), |ui| {
            section_header(ui, "Microphone");

            Grid::new("recording_mic_grid").show(ui, |ui| {
                toggle_slider_row(
                    blaster,
                    ui,
                    FeatureId::RecordMicSwitch,
                    FeatureId::RecordMicVolume,
                    "Recording",
                );
                ui.end_row();
                toggle_slider_row(
                    blaster,
                    ui,
                    FeatureId::MonitorMicSwitch,
                    FeatureId::MonitorMicVolume,
                    "Monitoring",
                );
                ui.end_row();
                preset_row(blaster, ui, FeatureId::MicBoost, "Mic Boost");
                ui.end_row();
            });

            ui.add_space(12.0);
            section_header(ui, "Voice Clarity");

            Grid::new("recording_voice_clarity_grid").show(ui, |ui| {
                toggle_slider_row(
                    blaster,
                    ui,
                    FeatureId::VoiceClarityToggle,
                    FeatureId::VoiceClarityNoiseReduction,
                    "Noise Reduction",
                );
                ui.end_row();
                toggle_row(
                    blaster,
                    ui,
                    FeatureId::AcousticEchoCancellation,
                    "Echo Cancellation",
                );
                ui.end_row();
                toggle_row(
                    blaster,
                    ui,
                    FeatureId::RecordingSmartVolToggle,
                    "Smart Volume",
                );
                ui.end_row();
            });
        });

        columns[1].with_layout(Layout::top_down_justified(Align::TOP), |ui| {
            section_header(ui, "Mic Equalizer");
            mic_eq(blaster, ui);
        });
    });
}

fn mic_eq(blaster: &BlasterXG6, ui: &mut egui::Ui) {
    let bands: Vec<f32> = FeatureId::MIC_EQ_BANDS
        .iter()
        .map(|&id| blaster.feature(id).value())
        .collect();
    let current_preset = MIC_EQ_PRESETS.iter().position(|(_, gains)| {
        gains
            .iter()
            .zip(&bands)
            .all(|(gain, band)| (gain - band).abs() < 0.01)
    });

    ui.horizontal(|ui| {
        let mic_eq = blaster.feature(FeatureId::MicEqToggle);
        if toggle_button!(ui, mic_eq.as_bool(), "Mic EQ").clicked() {
            let _ = blaster.set_feature(FeatureId::MicEqToggle, None);
        }

        egui::ComboBox::from_id_salt("mic_eq_preset")
            .selected_text(
                current_preset
                    .map(|index| MIC_EQ_PRESETS[index].0)
                    .unwrap_or("Custom"),
            )
            .show_ui(ui, |ui| {
                for (index, (name, gains)) in MIC_EQ_PRESETS.iter().enumerate() {
                    let is_selected = current_preset == Some(index);
                    if ui.selectable_label(is_selected, *name).clicked() {
                        for (&band, &gain) in
                            FeatureId::MIC_EQ_BANDS.iter().zip(gains)
                        {
                            let _ = blaster.set_feature(band, Some(gain));
                        }
                    }
                }
            });
    });

    Grid::new("mic_eq_grid").show(ui, |ui| {
        // The band center frequencies are unknown (not in any capture),
        // so they're simply numbered.
        for (index, &band) in FeatureId::MIC_EQ_BANDS.iter().enumerate() {
            let mut value = blaster.feature(band).value();

            ui.vertical_centered_justified(|ui| {
                ui.label(
                    RichText::new(format!("Band {}", index + 1))
                        .color(Color32::GRAY),
                );
            });

            let drag_value = ui.add(drag_value!(
                &mut value,
                suffix = " dB",
                decimals = 1,
                step = 0.5,
                range = -12.0..=12.0
            ));
            fit_slider_to_column(ui);
            let slider = ui.add(slider!(
                &mut value,
                vertical = false,
                decimals = 1,
                step = 0.5,
                range = -12.0..=12.0
            ));

            // do not simplify this, see eq_features()
            if (drag_value.changed() || slider.changed())
                && (drag_value.dragged()
                    || drag_value.drag_stopped()
                    || drag_value.lost_focus()
                    || slider.dragged()
                    || slider.drag_stopped())
            {
                let _ = blaster.set_feature(band, Some(value));
            }
            ui.end_row();
        }
    });
}

fn decoder_pane(blaster: &BlasterXG6, ui: &mut egui::Ui) {
    ui.vertical(|ui| {
        section_header(ui, "Decoder");

        preset_radio(blaster, ui, FeatureId::Decoder);

        ui.add_space(8.0);
        ui.label(
            RichText::new(
                "Dynamic range of decoded Dolby Digital audio (e.g. from S/PDIF in).\n\
                 Normal: standard range · Full: no compression · Night: compressed for quiet listening",
            )
            .color(Color32::GRAY),
        );
    });
}

fn mixer_pane(blaster: &BlasterXG6, ui: &mut egui::Ui) {
    let mixer_rows = |ui: &mut egui::Ui,
                      grid_id: &str,
                      rows: &[(&str, FeatureId, FeatureId)]| {
        Grid::new(grid_id).show(ui, |ui| {
            for &(label, switch_id, volume_id) in rows {
                let (drag_value, slider) =
                    toggle_slider_row(blaster, ui, switch_id, volume_id, label);

                // show the hardware gain the percentage corresponds to
                let volume = blaster.feature(volume_id).value();
                let db = volume_id
                    .alsa_element()
                    .filter(|_| !volume.is_nan())
                    .and_then(|(element, direction)| {
                        linuxblaster_control::mixer::volume_db(element, direction, volume).ok()
                    });
                if let Some(db) = db {
                    let gain = format!("{:+.1} dB", db);
                    drag_value.on_hover_text(&gain);
                    slider.on_hover_text(&gain);
                }
                ui.end_row();
            }
        });
    };

    ui.columns(2, |columns| {
        columns[0].with_layout(Layout::top_down_justified(Align::TOP), |ui| {
            section_header(ui, "Playback");
            mixer_rows(ui, "mixer_playback_grid", FeatureId::MIXER_PLAYBACK);

            ui.add_space(12.0);
            section_header(ui, "Monitoring");
            mixer_rows(ui, "mixer_monitoring_grid", FeatureId::MIXER_MONITORING);
        });

        columns[1].with_layout(Layout::top_down_justified(Align::TOP), |ui| {
            section_header(ui, "Recording");
            mixer_rows(ui, "mixer_recording_grid", FeatureId::MIXER_RECORDING);

            ui.add_space(12.0);
            section_header(ui, "Recording Source");
            preset_radio(blaster, ui, FeatureId::CaptureSource);
        });
    });

    ui.add_space(12.0);
    ui.label(
        RichText::new(
            "These are the G6's own hardware mixer controls (USB Audio Class), \
             changed through ALSA. No audio is processed in software.",
        )
        .color(Color32::GRAY),
    );
    if sync_volume_enabled() {
        ui.label(
            RichText::new(
                "Volume sync is on: the Speaker volume slider is locked \
                 while the System pane's system-sink slider owns the \
                 volume. The Speaker mute still works.",
            )
            .color(Color32::from_rgb(230, 190, 80)),
        );
    } else if sinad_fix_enabled() {
        ui.label(
            RichText::new(
                "SINAD fix is on: the Speaker volume slider stops at 97% \
                 (-2.00 dB).",
            )
            .color(Color32::GRAY),
        );
    }
}

fn lighting_pane(blaster: &BlasterXG6, ui: &mut egui::Ui) {
    const SWATCHES: &[(&str, [u8; 3])] = &[
        ("Red", [0xff, 0x00, 0x00]),
        ("Orange", [0xff, 0x40, 0x00]),
        ("Yellow", [0xff, 0xff, 0x00]),
        ("Green", [0x00, 0xff, 0x00]),
        ("Cyan", [0x00, 0xff, 0xff]),
        ("Blue", [0x00, 0x00, 0xff]),
        ("Purple", [0x80, 0x00, 0xff]),
        ("White", [0xff, 0xff, 0xff]),
    ];

    let color_feature = blaster.feature(FeatureId::LightingColor);
    let [_, red, green, blue] = if color_feature.value().is_nan() {
        [0; 4]
    } else {
        (color_feature.value() as u32).to_be_bytes()
    };
    let mut color = [red, green, blue];

    ui.vertical(|ui| {
        section_header(ui, "Lighting");

        Grid::new("lighting_grid").show(ui, |ui| {
            toggle_row(blaster, ui, FeatureId::LightingToggle, "Lighting");
            ui.end_row();

            ui.label(RichText::new("Color").color(Color32::GRAY));
            let mut new_color = None;
            if ui.color_edit_button_srgb(&mut color).changed() {
                new_color = Some(color);
            }
            ui.end_row();

            ui.label("");
            ui.horizontal(|ui| {
                for &(name, swatch) in SWATCHES {
                    let [r, g, b] = swatch;
                    let button = Button::new("")
                        .fill(Color32::from_rgb(r, g, b))
                        .min_size(Vec2::new(24.0, 24.0));
                    if ui.add(button).on_hover_text(name).clicked() {
                        new_color = Some(swatch);
                    }
                }
            });
            ui.end_row();

            if let Some([r, g, b]) = new_color {
                let rgb = u32::from_be_bytes([0, r, g, b]);
                let _ = blaster.set_feature(FeatureId::LightingColor, Some(rgb as f32));
            }
        });
    });
}

// ─── Launcher / Setup Screen ─────────────────────────────────────────────────

/// How often the setup screen tries to connect again on its own.
const RECONNECT_INTERVAL: Duration = Duration::from_secs(2);

/// Wraps the main UI with a setup screen that's shown while the G6
/// can't be used yet: not plugged in, or no permission to access it.
/// Most users start the app from their menu, where a panic would just look
/// like "nothing happens", so every problem gets explained here instead.
pub struct Launcher {
    app: Option<BlasterApp>,
    error: Option<ConnectError>,
    last_attempt: Instant,
    /// Running "Allow access" helper (pkexec), if any.
    setup: Option<std::thread::JoinHandle<Result<(), String>>>,
    setup_result: Option<Result<(), String>>,
}

impl Launcher {
    pub fn new() -> Self {
        let mut launcher = Self {
            app: None,
            error: None,
            last_attempt: Instant::now(),
            setup: None,
            setup_result: None,
        };
        launcher.try_connect();
        launcher
    }

    fn try_connect(&mut self) {
        self.last_attempt = Instant::now();
        match BlasterXG6::try_init() {
            Ok(device) => {
                match device.restore_last_state() {
                    Ok(0) => debug!("No last-state settings to restore"),
                    Ok(count) => {
                        debug!("Asserted {} last-state settings", count);
                    }
                    Err(error) => {
                        warn!("Couldn't restore the last state: {}", error);
                    }
                }
                // Apply the persisted SINAD cap before the first frame.
                // The first frame handles the optional sync gain staging.
                let settings = *SYSTEM_SETTINGS.lock().unwrap();
                device.set_volume_cap(if settings.sinad_fix {
                    Some(SINAD_VOLUME_CAP)
                } else {
                    None
                });
                if settings.sinad_fix {
                    let _ = device.enforce_volume_cap();
                }
                *STARTUP_POLICY_APPLIED.lock().unwrap() = false;
                self.app = Some(BlasterApp(device));
                self.error = None;
            }
            Err(error) => {
                // only log changes, the screen retries every few seconds
                if self.error.as_ref() != Some(&error) {
                    warn!("Can't connect to the G6: {}", error);
                }
                self.error = Some(error);
            }
        }
    }

    /// Installs the udev rule with a graphical password prompt (pkexec)
    /// and re-applies the rules to the already connected device.
    fn install_udev_rule() -> Result<(), String> {
        let temp_rule = std::env::temp_dir().join("70-linuxblaster-control.rules");
        std::fs::write(&temp_rule, UDEV_RULE)
            .map_err(|error| format!("Couldn't write {}: {}", temp_rule.display(), error))?;

        let status = std::process::Command::new("pkexec")
            .args([
                "sh",
                "-c",
                "install -m 0644 \"$1\" \"$2\" \
                 && udevadm control --reload-rules \
                 && udevadm trigger --subsystem-match=hidraw --action=change \
                 && udevadm settle",
                "sh",
            ])
            .arg(&temp_rule)
            .arg(UDEV_RULE_PATH)
            .status();
        let _ = std::fs::remove_file(&temp_rule);

        match status {
            Ok(status) if status.success() => Ok(()),
            // pkexec: 126 = dialog dismissed / not authorized
            Ok(status) if status.code() == Some(126) => {
                Err("The password prompt was cancelled.".to_owned())
            }
            Ok(status) => Err(format!("Setup failed ({}).", status)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(
                "pkexec isn't installed, so the password prompt can't be shown. \
                 Please use the terminal commands below."
                    .to_owned(),
            ),
            Err(error) => Err(format!("Couldn't start pkexec: {}", error)),
        }
    }

    fn setup_screen(&mut self, ui: &mut egui::Ui) {
        let is_nixos = std::path::Path::new("/etc/NIXOS").exists();

        ui.vertical_centered(|ui| {
            ui.add_space(24.0);
            ui.heading("Linuxblaster Control");
            ui.add_space(16.0);

            match &self.error {
                Some(ConnectError::NotFound) | None => {
                    ui.label(RichText::new("No Sound BlasterX G6 found").strong().size(18.0));
                    ui.add_space(8.0);
                    ui.label("Plug your G6 into a USB port and make sure it's switched on.");
                    ui.label(
                        RichText::new("This window will continue automatically once it's connected.")
                            .color(Color32::GRAY),
                    );
                }
                Some(ConnectError::PermissionDenied { .. }) => {
                    ui.label(RichText::new("Your G6 was found, but access is not allowed yet").strong().size(18.0));
                    ui.add_space(8.0);
                    ui.label(
                        "Linux only lets apps control USB devices like the G6 after a one-time setup.\n\
                         This installs a small system rule that lets your user account control the G6.",
                    );
                    ui.add_space(12.0);

                    if is_nixos {
                        ui.label("On NixOS, add this to your configuration and rebuild:");
                        ui.add_space(4.0);
                        ui.code(
                            "services.udev.extraRules = ''\n  \
                             SUBSYSTEM==\"hidraw\",\n  \
                             ATTRS{idVendor}==\"041e\", ATTRS{idProduct}==\"3256\",\n  \
                             MODE=\"0666\", TAG+=\"uaccess\"\n\
                             '';",
                        );
                        ui.label(
                            RichText::new("(or use the NixOS module from the README)")
                                .color(Color32::GRAY),
                        );
                    } else if self.setup.is_some() {
                        ui.add(egui::Spinner::new());
                        ui.label("Waiting for your password…");
                    } else {
                        let button = Button::new(RichText::new("Allow access").size(16.0))
                            .min_size(Vec2::new(160.0, 32.0));
                        if ui.add(button).clicked() {
                            self.setup_result = None;
                            self.setup = Some(std::thread::spawn(Self::install_udev_rule));
                        }
                        ui.label(
                            RichText::new("You'll be asked for your password once.")
                                .color(Color32::GRAY),
                        );
                    }

                    match &self.setup_result {
                        Some(Err(message)) => {
                            ui.add_space(8.0);
                            ui.colored_label(Color32::from_rgb(0xe0, 0x60, 0x60), message);
                        }
                        Some(Ok(())) => {
                            ui.add_space(8.0);
                            ui.label("Done. If the G6 still isn't accessible, unplug it and plug it back in.");
                        }
                        None => {}
                    }

                    if !is_nixos {
                        ui.add_space(16.0);
                        ui.collapsing("Prefer the terminal?", |ui| {
                            ui.label("Run these commands, then unplug and replug the G6:");
                            let commands = format!(
                                "echo '{}' | sudo tee {}\n\
                                 sudo udevadm control --reload-rules\n\
                                 sudo udevadm trigger",
                                UDEV_RULE
                                    .lines()
                                    .filter(|line| !line.starts_with('#'))
                                    .collect::<String>(),
                                UDEV_RULE_PATH
                            );
                            ui.code(&commands);
                            if ui.button("Copy commands").clicked() {
                                ui.ctx().copy_text(commands);
                            }
                        });
                    }
                }
                Some(ConnectError::Other(message)) => {
                    ui.label(RichText::new("Couldn't connect to the G6").strong().size(18.0));
                    ui.add_space(8.0);
                    ui.label(message);
                    ui.label(
                        RichText::new("Unplugging and replugging the G6 often helps.")
                            .color(Color32::GRAY),
                    );
                }
            }

            ui.add_space(24.0);
            if ui.button("Try again").clicked() {
                self.try_connect();
            }
        });
    }
}

impl eframe::App for Launcher {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if let Some(app) = &mut self.app {
            app.update(ctx, frame);
            return;
        }

        if self.setup.as_ref().is_some_and(|setup| setup.is_finished()) {
            let setup = self.setup.take().expect("checked above");
            self.setup_result = Some(
                setup
                    .join()
                    .unwrap_or_else(|_| Err("The setup helper crashed.".to_owned())),
            );
            self.try_connect();
        }

        if self.setup.is_none() && self.last_attempt.elapsed() >= RECONNECT_INTERVAL {
            self.try_connect();
        }

        if self.app.is_none() {
            egui::CentralPanel::default().show(ctx, |ui| self.setup_screen(ui));
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // eframe calls the Launcher's hook, not the nested BlasterApp's.
        if let Some(app) = &self.app {
            persist_on_exit(&app.0);
        }
    }
}
