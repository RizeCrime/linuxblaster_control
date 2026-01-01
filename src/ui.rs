use blaster_x_g6_control::{BlasterXG6, EqBand, Equalizer, SoundFeature};
use eframe::egui;

/// Renders a feature row with checkbox and horizontal slider.
fn feature_ui(
    ui: &mut egui::Ui,
    device: &Option<BlasterXG6>,
    state: &mut FeatureState,
    label: &str,
    feature: SoundFeature,
) {
    // Line 1: Feature label (brighter color)
    ui.label(
        egui::RichText::new(label)
            .color(egui::Color32::from_rgb(220, 220, 230)),
    );

    // Line 2: Checkbox + input field + slider
    ui.horizontal(|ui| {
        // Checkbox (no label, just the box)
        if ui.checkbox(&mut state.enabled, "").changed()
            && let Some(device) = device
        {
            if state.enabled {
                let _ = device.enable(feature);
            } else {
                let _ = device.disable(feature);
            }
        }

        ui.add_enabled_ui(state.enabled, |ui| {
            // Input field first (to the left of slider)
            let input = ui.add(
                egui::DragValue::new(&mut state.value)
                    .range(0..=100)
                    .suffix("%")
                    .speed(1.0),
            );

            // Slider (without built-in text input)
            let slider = ui.add(
                egui::Slider::new(&mut state.value, 0..=100)
                    .show_value(false)
                    .clamping(egui::SliderClamping::Always),
            );

            if (input.changed() || slider.changed())
                && let Some(device) = device
            {
                let _ = device.set_slider(feature, state.value);
            }
        });
    });

    ui.add_space(8.0);
}

pub struct FeatureState {
    pub enabled: bool,
    pub value: u8,
}

impl Default for FeatureState {
    fn default() -> Self {
        Self {
            enabled: false,
            value: 50,
        }
    }
}

/// EQ band labels (frequency in Hz)
const EQ_LABELS: [&str; 10] = [
    "31", "62", "125", "250", "500", "1k", "2k", "4k", "8k", "16k",
];

pub struct BlasterApp {
    device: Option<BlasterXG6>,
    surround: FeatureState,
    crystalizer: FeatureState,
    bass: FeatureState,
    smart_volume: FeatureState,
    dialog_plus: FeatureState,
    night_mode: bool,
    eq_enabled: bool,
    eq_bands: [f32; 10], // dB values (-12.0 to +12.0)
    // Debug
    ui_scale: f32,
}

impl BlasterApp {
    pub fn new(device: Option<BlasterXG6>) -> Self {
        Self {
            device,
            surround: FeatureState::default(),
            crystalizer: FeatureState::default(),
            bass: FeatureState::default(),
            smart_volume: FeatureState::default(),
            dialog_plus: FeatureState::default(),
            night_mode: false,
            eq_enabled: false,
            eq_bands: [0.0; 10], // All bands at 0 dB
            ui_scale: 1.5,
        }
    }

    /// Get the EqBand for a given index (0-9)
    fn get_eq_band(&self, index: usize) -> EqBand {
        Equalizer::default().bands()[index]
    }

    /// Reset all UI state to defaults
    fn reset_ui(&mut self) {
        self.surround = FeatureState::default();
        self.crystalizer = FeatureState::default();
        self.bass = FeatureState::default();
        self.smart_volume = FeatureState::default();
        self.dialog_plus = FeatureState::default();
        self.night_mode = false;
        self.eq_enabled = false;
        self.eq_bands = [0.0; 10];
    }
}

impl eframe::App for BlasterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply UI scale
        ctx.set_pixels_per_point(self.ui_scale);

        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_ui(ui, ctx);
        });
    }
}

impl BlasterApp {
    fn render_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // In debug mode, wrap in scroll area for flexibility
        #[cfg(debug_assertions)]
        {
            egui::ScrollArea::vertical().show(ui, |ui| {
                self.render_content(ui, ctx);
            });
        }

        // In release mode, render directly without scroll
        #[cfg(not(debug_assertions))]
        {
            self.render_content(ui, ctx);
        }
    }

    fn render_content(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // ===== TITLE =====
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new("Sound Blaster X G6").heading().strong(),
            );
        });
        ui.add_space(8.0);

        if self.device.is_none() {
            ui.vertical_centered(|ui| {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    "⚠ Device not connected",
                );
            });
            ui.add_space(8.0);
        }

        ui.separator();
        ui.add_space(8.0);

        // ===== ROW: Features | Toggles =====
        ui.horizontal(|ui| {
            // Column: Sound Features (Sliders)
            ui.vertical(|ui| {
                feature_ui(
                    ui,
                    &self.device,
                    &mut self.surround,
                    "Surround Sound",
                    SoundFeature::SurroundSound,
                );
                feature_ui(
                    ui,
                    &self.device,
                    &mut self.crystalizer,
                    "Crystalizer",
                    SoundFeature::Crystalizer,
                );
                feature_ui(
                    ui,
                    &self.device,
                    &mut self.bass,
                    "Bass",
                    SoundFeature::Bass,
                );
                feature_ui(
                    ui,
                    &self.device,
                    &mut self.smart_volume,
                    "Smart Volume",
                    SoundFeature::SmartVolume,
                );
                feature_ui(
                    ui,
                    &self.device,
                    &mut self.dialog_plus,
                    "Dialog Plus",
                    SoundFeature::DialogPlus,
                );
            });

            ui.add_space(16.0);

            // Column: Toggles (Night Mode + Equalizer)
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("Toggles")
                        .color(egui::Color32::from_rgb(220, 220, 230)),
                );
                ui.add_space(4.0);

                // Night Mode toggle
                if ui.checkbox(&mut self.night_mode, "🌙 Night Mode").changed()
                    && let Some(device) = &self.device
                {
                    if self.night_mode {
                        let _ = device.enable(SoundFeature::NightMode);
                    } else {
                        let _ = device.disable(SoundFeature::NightMode);
                    }
                }

                // Equalizer toggle
                if ui.checkbox(&mut self.eq_enabled, "🎚 Equalizer").changed()
                    && let Some(device) = &self.device
                {
                    if self.eq_enabled {
                        let _ = device.enable(SoundFeature::Equalizer);
                    } else {
                        let _ = device.disable(SoundFeature::Equalizer);
                    }
                }
            });

            ui.add_space(16.0);

            // Column: Actions
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("Actions")
                        .color(egui::Color32::from_rgb(220, 220, 230)),
                );
                ui.add_space(4.0);

                if ui.button("🔄 Reset All").clicked() {
                    if let Some(device) = &self.device {
                        let _ = device.reset();
                    }
                    self.reset_ui();
                }
            });
        });

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);

        // ===== Equalizer Bands (full width below) =====
        ui.label(
            egui::RichText::new("Equalizer")
                .color(egui::Color32::from_rgb(220, 220, 230)),
        );
        ui.add_space(4.0);

        ui.add_enabled_ui(self.eq_enabled, |ui| {
            ui.horizontal(|ui| {
                for i in 0..10 {
                    ui.vertical(|ui| {
                        // Compact dB value input (no suffix, integer display)
                        let input = ui.add(
                            egui::DragValue::new(&mut self.eq_bands[i])
                                .range(-12.0..=12.0)
                                .speed(0.1)
                                .fixed_decimals(0)
                                .custom_formatter(|v, _| format!("{:+.0}", v)),
                        );

                        // Vertical slider
                        let slider = ui.add(
                            egui::Slider::new(
                                &mut self.eq_bands[i],
                                -12.0..=12.0,
                            )
                            .vertical()
                            .show_value(false)
                            .clamping(egui::SliderClamping::Always),
                        );

                        if input.changed() || slider.changed() {
                            if let Some(device) = &self.device {
                                let band = self.get_eq_band(i);
                                let _ = device
                                    .set_eq_band_db(band, self.eq_bands[i]);
                            }
                        }

                        // Frequency label
                        ui.label(
                            egui::RichText::new(EQ_LABELS[i])
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                    });
                }
            });
        });

        // ===== DEBUG SECTION (only in debug builds) =====
        #[cfg(debug_assertions)]
        {
            ui.add_space(16.0);
            ui.separator();
            ui.collapsing("🔧 Debug", |ui| {
                ui.horizontal(|ui| {
                    ui.label("UI Scale:");
                    if ui
                        .add(
                            egui::Slider::new(&mut self.ui_scale, 0.5..=3.0)
                                .step_by(0.1),
                        )
                        .changed()
                    {}
                    if ui.button("Reset").clicked() {
                        self.ui_scale = 1.5;
                    }
                });
                ui.label(format!("Current scale: {:.1}x", self.ui_scale));

                ui.add_space(8.0);
                ui.separator();

                // Window size info (logical pixels)
                let viewport = ctx.input(|i| i.viewport_rect());
                let logical_w = viewport.width();
                let logical_h = viewport.height();

                // Physical pixels = logical * scale
                let physical_w = logical_w * self.ui_scale;
                let physical_h = logical_h * self.ui_scale;

                ui.label(format!(
                    "Window: {:.0} × {:.0} logical px",
                    logical_w, logical_h
                ));
                ui.label(format!(
                    "Window: {:.0} × {:.0} physical px",
                    physical_w, physical_h
                ));

                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format!(
                        ".with_inner_size([{:.0}, {:.0}])",
                        physical_w, physical_h
                    ))
                    .monospace()
                    .small(),
                );
                ui.label(
                    egui::RichText::new(format!(
                        ".with_min_inner_size([{:.0}, {:.0}])",
                        physical_w, physical_h
                    ))
                    .monospace()
                    .small(),
                );
            });
        }
    }
}

