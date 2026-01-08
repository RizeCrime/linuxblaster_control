use crate::{BlasterXG6, FEATURES, Feature, FeatureType};
use eframe::egui::{
    self, Button, Color32, Rect, RichText, Stroke, Vec2, accesskit::Size,
};
use eframe::egui::{Align, DragValue, Layout, Slider, Widget, widgets};
use std::collections::HashMap;
use std::error::Error;
use std::sync::{LazyLock, Mutex};
use tracing::debug;

static UI_SELECTED: LazyLock<Mutex<&'static str>> =
    LazyLock::new(|| Mutex::new(""));

static CACHE_DIRTY: LazyLock<Mutex<bool>> = LazyLock::new(|| Mutex::new(false));

macro_rules! nav_panes {
    ( $self:ident, $ui:ident, ($($pane_name:expr),* $(,)? ) ) => {
        $ui.vertical(|ui| {
            $(
                ui.scope(|ui| {
                    ui.set_width(160.0);
                    let Ok((feature, _)) = get_feature_cached($self, $pane_name) else { return };

                    ui.vertical_centered_justified(|ui| {
                        ui.label(RichText::new($pane_name).strong());

                        ui.horizontal(|ui| {
                            let toggle_btn = Button::selectable(
                                feature.value.as_bool().expect("Feature must be a Toggle"),
                                RichText::new("Toggle"),
                            )
                            .min_size(Vec2::new(64.0, 24.0))
                            .frame_when_inactive(true);

                            if ui.add(toggle_btn).clicked() {
                                debug!("{} clicked", $pane_name);
                                let _ = $self.set_feature($pane_name, None);
                                *CACHE_DIRTY.lock().unwrap() = true;
                            }

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let current_selection = *UI_SELECTED.lock().unwrap();
                                let is_selected = current_selection == $pane_name;
                                let selector_btn = Button::selectable(
                                    is_selected,
                                    RichText::new("➡"),
                                )
                                .min_size(Vec2::new(32.0, 24.0))
                                .frame_when_inactive(true);

                                if ui.add(selector_btn).clicked() {
                                    debug!("{} selector clicked", $pane_name);
                                    let mut selected = UI_SELECTED.lock().unwrap();
                                    if *selected == $pane_name {
                                        *selected = ""; // Allow deselecting
                                    } else {
                                        *selected = $pane_name; // Select this pane
                                    }
                                }
                            });
                        });
                        ui.separator();
                    });
                });
            )*
        });
    };
}

/// Cached feature lookup result
struct CachedFeature {
    feature: Box<Feature>,
    dependencies: Option<Box<[&'static str]>>,
}

impl eframe::App for BlasterXG6 {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if *CACHE_DIRTY.lock().unwrap() {
            FEATURE_CACHE.lock().unwrap().clear();
            *CACHE_DIRTY.lock().unwrap() = false;
        }

        egui::TopBottomPanel::top("top_panel").resizable(true).show(
            ctx,
            |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading(
                        RichText::new("Sound BlasterX G6 Control").strong(),
                    );
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
                            self,
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
                    sbx_pane(self, ui);
                }
                "Equalizer" => {
                    eq_pane(self, ui);
                }
                "Scout Mode" => {
                    println!("Not implemented");
                }
                _ => {}
            }
        });
    }
}

macro_rules! sbx_feature {
    ($blaster:ident, $ui:ident, $name:expr) => {
        $ui.allocate_ui_with_layout(
            Vec2::new(64.0, $ui.available_height()),
            Layout::top_down(Align::Center),
            |ui| {
                ui.label(RichText::new($name));

                if let Ok((feature, _)) = get_feature_cached($blaster, $name) {
                    let is_enabled = feature.value.as_bool().unwrap_or(false);

                    let slider_name = format!("{} Slider", $name);
                    let slider_data = $blaster
                        .features
                        .iter()
                        .find(|f| f.name == slider_name.as_str())
                        .and_then(|f| f.value.as_f32().map(|v| (f.name, v)));

                    if let Some((s_name, mut value)) = slider_data {
                        // Drag Value
                        let input = ui.add_enabled(
                            is_enabled,
                            DragValue::new(&mut value)
                                .range(0.0..=100.0)
                                .speed(1.0)
                                .suffix("%")
                                .fixed_decimals(0),
                        );

                        if input.changed() {
                            let _ = $blaster.set_slider(s_name, value);
                            *CACHE_DIRTY.lock().unwrap() = true;
                        }

                        // Slider
                        let slider = Slider::new(&mut value, 0.0..=100.0)
                            .clamping(egui::SliderClamping::Always)
                            .fixed_decimals(0)
                            .step_by(1.0)
                            .show_value(false)
                            .vertical();

                        let response = ui.add_enabled(is_enabled, slider);
                        if response.changed() {
                            if let Some(f) = $blaster
                                .features
                                .iter_mut()
                                .find(|f| f.name == s_name)
                            {
                                f.value = FeatureType::Slider(value);
                            }
                            *CACHE_DIRTY.lock().unwrap() = true;
                        }

                        if response.drag_stopped() {
                            let _ = $blaster.set_slider(s_name, value);
                            *CACHE_DIRTY.lock().unwrap() = true;
                        }
                    }

                    // Toggle Button (at the bottom)
                    let toggle_btn =
                        Button::selectable(is_enabled, RichText::new("Toggle"))
                            .min_size(Vec2::new(64.0, 24.0))
                            .frame_when_inactive(true);

                    if ui.add(toggle_btn).clicked() {
                        let _ = $blaster.set_feature($name, None);
                        *CACHE_DIRTY.lock().unwrap() = true;
                    }
                }
            },
        );
    };
}

fn sbx_pane(blaster: &mut BlasterXG6, ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        sbx_feature!(blaster, ui, "Surround");
        sbx_feature!(blaster, ui, "Dialog+");
        sbx_feature!(blaster, ui, "Smart Volume");
        sbx_feature!(blaster, ui, "Crystalizer");
        sbx_feature!(blaster, ui, "Bass");
    });
}

fn eq_pane(blaster: &mut BlasterXG6, ui: &mut egui::Ui) {
    let eq_bands: Vec<&mut Feature> = blaster
        .features
        .iter_mut()
        .filter(|f| f.name.starts_with("EQ"))
        .collect();

    ui.vertical_centered_justified(|ui| {
        ui.heading(RichText::new("AutoEq; TODO"));

        ui.separator();

        ui.horizontal_centered(|ui| {
            for band in eq_bands {
                ui.allocate_ui_with_layout(
                    Vec2::new(50.0, ui.available_height()),
                    Layout::top_down(Align::Center),
                    |ui| {
                        // Strip prefix and anything after dash for clean label
                        let name =
                            band.name.strip_prefix("EQ ").unwrap_or(band.name);
                        let name = match name.find('-') {
                            Some(idx) => &name[..idx],
                            None => name,
                        };

                        let input = ui.add(
                            DragValue::new(
                                band.value
                                    .as_f32_mut()
                                    .expect("Feature must be a Slider"),
                            )
                            .range(-12.0..=12.0)
                            .speed(0.1)
                            .suffix(" dB")
                            .fixed_decimals(1),
                        );

                        let slider = ui.add(
                            Slider::new(
                                band.value
                                    .as_f32_mut()
                                    .expect("Feature must be a Slider"),
                                -12.0..=12.0,
                            )
                            .clamping(egui::SliderClamping::Always)
                            .text(name)
                            .fixed_decimals(1)
                            .step_by(0.1)
                            .show_value(false)
                            .vertical(),
                        );
                        // ui.label(RichText::new(name).small().color(Color32::GRAY));
                    },
                );
            }
        });
    });
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
