#![allow(unused)]

use blaster_x_g6_control::{BlasterXG6, Feature, FeatureType};
use eframe::egui::{
    self, Button, Color32, Rect, RichText, Stroke, Vec2, accesskit::Size,
};
use eframe::egui::{
    Align, CollapsingHeader, ComboBox, DragValue, Grid, Layout, Margin, Popup,
    PopupCloseBehavior, ScrollArea, Slider, Widget, Window, widgets,
};
use egui_plot::{Line, Plot, PlotPoints, log_grid_spacer};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::error::Error;
use std::sync::{LazyLock, Mutex};
use tracing::debug;

use crate::{AUTOEQ_DB, AutoEqDb, HeadphoneResult};

#[macro_use]
#[path = "macros.rs"]
mod macros;

const ISO_BANDS: [f64; 10] = [31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];

static UI_SELECTED: LazyLock<Mutex<&'static str>> =
    LazyLock::new(|| Mutex::new("Equalizer"));
static AUTOEQ_MODAL: LazyLock<Mutex<bool>> =
    LazyLock::new(|| Mutex::new(false));
static SEARCH_QUERY: LazyLock<Mutex<String>> =
    LazyLock::new(|| Mutex::new(String::new()));
static SEARCH_RESULTS: LazyLock<Mutex<Vec<&'static str>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

static CACHE_DIRTY: LazyLock<Mutex<bool>> = LazyLock::new(|| Mutex::new(false));

/// Cached feature lookup result
struct CachedFeature {
    feature: Box<Feature>,
    dependencies: Option<Box<[&'static str]>>,
}

pub struct BlasterApp(pub BlasterXG6);

impl eframe::App for BlasterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if *CACHE_DIRTY.lock().unwrap() {
            FEATURE_CACHE.lock().unwrap().clear();
            *CACHE_DIRTY.lock().unwrap() = false;
        }

        egui::TopBottomPanel::top("top_panel").resizable(true).show(
            ctx,
            |ui| {
                ui.vertical_centered_justified(|ui| {
                    ui.spacing_mut().window_margin = Margin::symmetric(4, 8);
                    ui.heading(
                        RichText::new("Sound BlasterX G6 Control").strong(),
                    );

                    // Global Buttons
                    ui.horizontal_centered(|ui| {
                        // Reset All Button
                        if ui.button("Reset All").clicked() {
                            let _ = self.0.reset();
                            *CACHE_DIRTY.lock().unwrap() = true;
                        }
                    });
                });
            },
        );
        egui::SidePanel::left("left_panel")
            .resizable(false)
            .show(ctx, |ui| {
                // ui.with_layout(Layout::left_to_right(Align::Center)
                //     .with_cross_align(true), |ui| {
                // });
                ui.vertical_centered(|ui| {
                    ui.horizontal_centered(|ui| {
                        nav_panes!(
                            &mut self.0,
                            ui,
                            ("SBX", "Equalizer", "Scout Mode",)
                        );
                    });
                });
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            let state = *UI_SELECTED.lock().unwrap();
            match *UI_SELECTED.lock().unwrap() {
                "SBX" => {
                    sbx_pane(&mut self.0, ui);
                }
                "Equalizer" => {
                    if *AUTOEQ_MODAL.lock().unwrap() {
                        autoeq_pane(&mut self.0, ui);
                    } else {
                        eq_pane(&mut self.0, ui);
                    }
                }
                "Scout Mode" => {
                    scout_mode_pane(ui);
                }
                _ => {}
            }
        });
    }
}

fn sbx_pane(blaster: &mut BlasterXG6, ui: &mut egui::Ui) {
    Grid::new("sbx_grid").show(ui, |ui| {
        sbx_feature!(blaster, ui, "Surround");
        sbx_feature!(blaster, ui, "Dialog+");
        sbx_feature!(blaster, ui, "Smart Volume");
        sbx_feature!(blaster, ui, "Crystalizer");
        sbx_feature!(blaster, ui, "Bass");
    });
}

fn eq_pane(blaster: &mut BlasterXG6, ui: &mut egui::Ui) {
    let mut binding = blaster.features.clone();
    let eq_bands: Vec<&mut Feature> = binding
        .iter_mut()
        .filter(|f| f.name.starts_with("EQ"))
        .collect();

    ui.vertical_centered_justified(|ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("AutoEq: ").color(Color32::GRAY));
            if ui.button(RichText::new("Select Profile")).clicked() {
                *AUTOEQ_MODAL.lock().unwrap() = true;
            }
        });

        ui.separator();

        let names = eq_bands
            .iter()
            .map(|f| {
                let name = f.name.strip_prefix("EQ ").unwrap_or(f.name);
                match name.find('-') {
                    Some(idx) => &name[..idx],
                    None => name,
                }
            })
            .collect::<Vec<&str>>();

        Grid::new("eq_grid").show(ui, |ui| {
            for band in eq_bands {
                if let Some(value) = band.value.as_f32_mut() {
                    let clean_name =
                        band.name.strip_prefix("EQ ").unwrap_or(band.name);
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
                        value,
                        suffix = " dB",
                        decimals = 1,
                        step = 0.1,
                        range = -12.0..=12.0
                    ));
                    let slider = ui.add(slider!(
                        value,
                        vertical = false,
                        decimals = 1,
                        step = 0.1,
                        range = -12.0..=12.0
                    ));

                    if drag_value.changed() || slider.changed() {
                        let _ = blaster.set_slider(band.name, *value);
                        *CACHE_DIRTY.lock().unwrap() = true;
                    }

                    ui.end_row();
                }
            }
        });
    });
}

fn scout_mode_pane(ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        ui.heading("Scout Mode & AutoEq Debug");

        let mut search = SEARCH_QUERY.lock().unwrap();
        ui.horizontal(|ui| {
            ui.label("Search Headphones:");
            ui.text_edit_singleline(&mut *search);
        });

        ui.separator();

        if search.is_empty() {
            ui.label("Enter search term to see results");
        } else {
            let search_term = search.to_lowercase();
            // Access the public static DB
            let db = &crate::AUTOEQ_DB;

            egui::ScrollArea::vertical().show(ui, |ui| {
                for (name, results) in db.entries() {
                    if name.to_lowercase().contains(&search_term) {
                        ui.push_id(name, |ui| {
                            ui.group(|ui| {
                                ui.label(RichText::new(*name).strong());
                                for (i, res) in results.iter().enumerate() {
                                    ui.label(format!(
                                        "Variant: {}",
                                        res.variant.unwrap_or("")
                                    ));
                                    ui.label(format!(
                                        "Preamp: {} dB",
                                        res.preamp
                                    ));
                                    ui.label(format!(
                                        "Test Device: {}",
                                        res.test_device.unwrap_or("Unknown")
                                    ));
                                    ui.label(format!("Tester: {}", res.tester));
                                    ui.push_id(i, |ui| {
                                        ui.collapsing("EQ Bands", |ui| {
                                            for (i, gain) in res
                                                .ten_band_eq
                                                .iter()
                                                .enumerate()
                                            {
                                                ui.label(format!(
                                                    "Band {}: {} dB",
                                                    i + 1,
                                                    gain
                                                ));
                                            }
                                        });
                                    });
                                }
                            });
                        });
                    }
                }
            });
        }
    });
}

fn autoeq_pane(blaster: &mut BlasterXG6, ui: &mut egui::Ui) {
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
                                    // ui.separator();
                                });
                                // eq curve
                                let plot =
                                    Plot::new(format!("eq_curve_{}_{}_{}", name, result.tester, result.variant.unwrap_or("")))
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

                                plot.show(ui, |plot_ui| {
                                    let points: PlotPoints = (0..=500).map(|i| {
                                        let t = i as f64 / 500.0; 
                                        let f = 20.0 * (20000.0 / 20.0_f64).powf(t);
                                        
                                        let mut total_y = 0.0; 
                                        for (idx, gain) in result.ten_band_eq.iter().enumerate() {
                                            if let Some(&center_freq) = ISO_BANDS.get(idx) {
                                                if gain.abs() > 0.01 {
                                                    total_y += calculate_peaking_eq_response(f, center_freq as f64, *gain as f64, 1.41);
                                                }
                                            }
                                        }

                                        [f.log10(), total_y]
                                    }).collect();

                                    plot_ui.line(Line::new(format!("eq_curve_{}", name), points).width(2.0));
                                });

                                let apply_button = ui.button(RichText::new("Apply Profile"));
                                if apply_button.clicked() {
                                    for (idx, gain) in result.ten_band_eq.iter().enumerate() {
                                        if let Some(&center_freq) = ISO_BANDS.get(idx) {
                                        if gain.abs() > 0.01 {
                                            let mut feature_name;
                                            if center_freq < 1000.0 {
                                                feature_name = format!("EQ {}Hz", center_freq);
                                            } else {
                                                feature_name = format!("EQ {}kHz", center_freq / 1000.0);
                                            }
                                            let _ = blaster.set_slider(&feature_name, *gain as f32);
                                        }
                                        }
                                    }
                                    let _ = blaster.set_slider("EQ Pre-Amp", result.preamp as f32);
                                    *CACHE_DIRTY.lock().unwrap() = true;
                                }
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

/// Cache for get_feature() results
static FEATURE_CACHE: LazyLock<Mutex<HashMap<&'static str, CachedFeature>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

// the type really isn't all that complex
// it's a tuple of a Feature and an Option of a slice of strings:
// Result<(Feature, Option<[&str]>), Error>
// but all ampercented to make them stack allocated,
// so it might look a little weird at first ...
#[allow(clippy::type_complexity)]
/// Wrapper function that caches the results of get_feature()
/// Returns cached data without calling get_feature() every time.
/// On cache miss, uses the provided BlasterXG6 instance to populate the cache.
fn get_feature_cached(
    blaster: &BlasterXG6,
    feature: &'static str,
) -> Result<(&'static Feature, Option<&'static [&'static str]>), Box<dyn Error>>
{
    // Check cache first
    {
        let cache = FEATURE_CACHE.lock().unwrap();
        if let Some(cached) = cache.get(feature) {
            // Cache hit: return references to boxed data
            // Safe because Box provides stable addresses in static storage
            let feature_ref = cached.feature.as_ref() as *const Feature;
            let deps_ref = cached
                .dependencies
                .as_ref()
                .map(|deps| deps.as_ref() as *const [&'static str]);

            unsafe {
                return Ok((&*feature_ref, deps_ref.map(|d| &*d)));
            }
        }
    }

    // Cache miss: use existing BlasterXG6 instance to populate cache
    let (feature_ref, dependencies) = blaster.get_feature(feature)?;

    // Store in cache with Box for stable addresses
    let cached = CachedFeature {
        feature: Box::new(feature_ref.clone()),
        dependencies: dependencies.map(|deps| deps.to_vec().into_boxed_slice()),
    };

    {
        let mut cache = FEATURE_CACHE.lock().unwrap();
        cache.insert(feature, cached);
    }

    // Retrieve from cache to return references
    let cache = FEATURE_CACHE.lock().unwrap();
    let cached = cache.get(feature).unwrap();

    let feature_ref = cached.feature.as_ref() as *const Feature;
    let deps_ref = cached
        .dependencies
        .as_ref()
        .map(|deps| deps.as_ref() as *const [&'static str]);

    unsafe { Ok((&*feature_ref, deps_ref.map(|d| &*d))) }
}
