// #![allow(unused)]

use linuxblaster_control::{BlasterXG6, DEFAULT_BASE_PATH, FeatureId, ValueKind};
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


pub struct BlasterApp(pub BlasterXG6);

impl eframe::App for BlasterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(debug_assertions)]
        if ctx.input(|i| i.key_pressed(egui::Key::D)) {
            ctx.set_debug_on_hover(!ctx.debug_on_hover());
        }

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
        egui::SidePanel::left("left_panel")
            .resizable(false)
            .show(ctx, |ui| {
                nav_pane(&self.0, ui, "SBX", Some(FeatureId::SbxMaster), true);
                nav_pane(&self.0, ui, "Playback", Some(FeatureId::Output), true);
                nav_pane(&self.0, ui, "Recording", None, true);
                nav_pane(&self.0, ui, "Scout Mode", Some(FeatureId::ScoutMode), false);
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            let state = *UI_SELECTED.lock().unwrap();
            match *UI_SELECTED.lock().unwrap() {
                "SBX" => {
                    if *AUTOEQ_MODAL.lock().unwrap() {
                        autoeq_pane(&self.0, ui);
                    }
                    else {
                        sbx_pane(&self.0, ui);
                    }
                }
                "Scout Mode" => {
                    let _two = 1 + 1;
                }
                "Playback" => {
                    todo!();
                }
                "Recording" => {
                    todo!();
                }
                _ => {
                    warn!("Unknown UI selected: {}", state);
                }
            }
        });
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
    pane_name: &str,
    feature_id: Option<FeatureId>, 
    with_selector: bool,
) {
    let feature = feature_id.map(|id| blaster.feature(id));

    ui.vertical_centered_justified(|ui| {
        ui.set_width(160.0);

        ui.label(RichText::new(pane_name).strong());

        ui.horizontal(|ui| {
            #[allow(clippy::collapsible_if)]
            if let Some(feature) = feature {
                let feature_value = feature.as_bool();
                if toggle_button!(ui, feature_value).clicked() {
                    if blaster.set_feature(feature.id, None).is_err() {
                        error!("Failed to set feature: {:?}", feature.id);
                    }
                }
            }

            if with_selector {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let current_selection = *UI_SELECTED.lock().unwrap();
                    let is_selected = current_selection == pane_name;
                    let selector_button = Button::selectable(
                        is_selected,
                        RichText::new("➡"),
                    )
                    .min_size(Vec2::new(32.0, 24.0))
                    .frame_when_inactive(true);

                    if ui.add(selector_button).clicked() {
                        let mut selected = UI_SELECTED.lock().unwrap();
                        if *selected == pane_name {
                            *selected = "";
                        }
                    }
                });
            }
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
            ui.heading(
                RichText::new("Select AutoEq Profile").color(Color32::GRAY),
            );
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
                        if let Some(score) = matcher.fuzzy_match(key, &search) {
                            results.push((score, *key));
                        }
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
                if let Some(results) = db.results.and_then(|map| map.get(name))
                {
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
                                                if let Some(&center_freq) = ISO_BANDS.get(index)
                                                    && gain.abs() > 0.01 {
                                                    total_y += calculate_peaking_eq_response(f, center_freq, *gain as f64, 1.41);
                                                }
                                            }

                                            [f.log10(), total_y]
                                        }).collect();

                                        plot_ui.line(Line::new(format!("eq_curve_{}", name), points).width(2.0));
                                    });
                                    let apply_button = ui.button(RichText::new("Apply Profile"));
                                    if apply_button.clicked() {
                                        for (index, gain) in result.ten_band_eq.iter().enumerate() {
                                            if gain.abs() > 0.01 {
                                                let _ = blaster.set_feature(FeatureId::EQ_BANDS[index], Some(*gain));
                                            }
                                        }
                                        let _ = blaster.set_feature(FeatureId::EqPreAmp, Some(result.preamp));
                                    }

                                });
                            });
                        }
                    });

                    ui.separator();
                }
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
