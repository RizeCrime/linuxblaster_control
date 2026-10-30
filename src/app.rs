// #![allow(unused)]

use linuxblaster_control::{
    BlasterXG6, ConnectError, DEFAULT_BASE_PATH, FeatureId, LinkState,
    MIC_EQ_PRESETS, UDEV_RULE, UDEV_RULE_PATH, ValueKind,
    remember_last_profile,
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
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
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
        {
            let mut last_refresh = LAST_MIXER_REFRESH.lock().unwrap();
            if last_refresh.elapsed() >= MIXER_REFRESH_INTERVAL {
                self.0.read_mixer_state();
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

                            match self.0.apply_profile(path.clone()) {
                                Ok(()) => remember_last_profile(&path),
                                Err(error) => {
                                    error!("Failed to apply profile from file");
                                    error!("Path: {}", path.display());
                                    error!("Error: {}", error);
                                }
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

                            match self.0.save_profile(path.clone()) {
                                Ok(()) => remember_last_profile(&path),
                                Err(error) => {
                                    error!("Failed to save profile to file");
                                    error!("Path: {}", path.display());
                                    error!("Error: {}", error);
                                }
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

                    ui.with_layout(
                        Layout::right_to_left(Align::Center),
                        |ui| {
                            ui.label(format!(
                                "Link reconnects: {}",
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
    let mut slider_value = if slider_available {
        slider_feature.value() * 100.0
    } else {
        0.0
    };

    let toggle = ui
        .add_enabled_ui(toggle_available, |ui| {
            toggle_button!(ui, toggle_feature.as_bool(), label, width = full)
        })
        .inner;
    let drag_value =
        ui.add_enabled(slider_available, drag_value!(&mut slider_value));
    fit_slider_to_column(ui);
    let slider = ui.add_enabled(
        slider_available,
        slider!(&mut slider_value, vertical = false),
    );

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
    /// Profile to apply once the G6 is connected (`--apply-last`, `--apply-profile`).
    startup_profile: Option<PathBuf>,
}

impl Launcher {
    pub fn new(startup_profile: Option<PathBuf>) -> Self {
        let mut launcher = Self {
            app: None,
            error: None,
            last_attempt: Instant::now(),
            setup: None,
            setup_result: None,
            startup_profile,
        };
        launcher.try_connect();
        launcher
    }

    /// The connected G6, if any (used by the tray).
    /// While the window is hidden the setup screen doesn't run,
    /// so this keeps retrying the connection like it does.
    pub fn device(&mut self) -> Option<&BlasterXG6> {
        if self.app.is_none()
            && self.setup.is_none()
            && self.last_attempt.elapsed() >= RECONNECT_INTERVAL
        {
            self.try_connect();
        }
        self.app.as_ref().map(|app| &app.0)
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

                // on the first connection, which may be a few retries later
                // (e.g. while the G6 is still coming back from a USB reset)
                if let Some(path) = self.startup_profile.take() {
                    match device.sync_profile(&path) {
                        Ok(_) => remember_last_profile(&path),
                        Err(error) => error!(
                            "Failed to apply profile {} on startup: {}",
                            path.display(),
                            error
                        ),
                    }
                }
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
