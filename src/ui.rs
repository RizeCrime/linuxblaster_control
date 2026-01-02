use crate::{BlasterXG6, EqBand, Equalizer, Preset, SoundFeature};
use eframe::egui;

/// Renders a feature row with checkbox and horizontal slider.
fn feature_ui(
    ui: &mut egui::Ui,
    device: &mut Option<BlasterXG6>,
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
            && let Some(device) = device.as_mut()
        {
            if state.enabled {
                let _ = device.enable(feature).ok();
            } else {
                let _ = device.disable(feature).ok();
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
                && let Some(device) = device.as_mut()
            {
                let _ = device.set_slider(feature, state.value).ok();
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
    pub(crate) device: Option<BlasterXG6>,
    pub(crate) surround: FeatureState,
    pub(crate) crystalizer: FeatureState,
    pub(crate) bass: FeatureState,
    pub(crate) smart_volume: FeatureState,
    pub(crate) dialog_plus: FeatureState,
    pub(crate) night_mode: bool,
    pub(crate) loud_mode: bool,
    pub(crate) eq_enabled: bool,
    pub(crate) eq_bands: [f32; 10], // dB values (-12.0 to +12.0)
    // Presets
    pub(crate) presets: Vec<Preset>,
    pub(crate) selected_preset: Option<usize>,
    pub(crate) save_preset_name: String,
    pub(crate) preset_error: Option<String>,
    pub(crate) show_file_picker: bool,
    // Debug
    pub(crate) ui_scale: f32,
}

impl BlasterApp {
    pub fn new(device: Option<BlasterXG6>) -> Self {
        let mut app = Self {
            device,
            surround: FeatureState::default(),
            crystalizer: FeatureState::default(),
            bass: FeatureState::default(),
            smart_volume: FeatureState::default(),
            dialog_plus: FeatureState::default(),
            night_mode: false,
            loud_mode: false,
            eq_enabled: false,
            eq_bands: [0.0; 10], // All bands at 0 dB
            presets: Vec::new(),
            selected_preset: None,
            save_preset_name: String::new(),
            preset_error: None,
            show_file_picker: false,
            ui_scale: 1.5,
        };
        app.refresh_presets();
        app
    }

    fn refresh_presets(&mut self) {
        self.presets = crate::list_presets().unwrap_or_default();
        self.preset_error = None;
    }

    /// Get the EqBand for a given index (0-9)
    fn get_eq_band(&self, index: usize) -> EqBand {
        Equalizer::default().bands()[index]
    }

    /// Reset all UI state to defaults
    pub(crate) fn reset_ui(&mut self) {
        self.surround = FeatureState::default();
        self.crystalizer = FeatureState::default();
        self.bass = FeatureState::default();
        self.smart_volume = FeatureState::default();
        self.dialog_plus = FeatureState::default();
        self.night_mode = false;
        self.loud_mode = false;
        self.eq_enabled = false;
        self.eq_bands = [0.0; 10];
    }

    /// Sync UI state from device state
    fn sync_ui_from_device(&mut self) {
        if let Some(device) = &self.device {
            self.surround.enabled = device.surround_sound_enabled;
            self.surround.value = device.surround_sound_value;
            self.crystalizer.enabled = device.crystalizer_enabled;
            self.crystalizer.value = device.crystalizer_value;
            self.bass.enabled = device.bass_enabled;
            self.bass.value = device.bass_value;
            self.smart_volume.enabled = device.smart_volume_enabled;
            self.smart_volume.value = device.smart_volume_value;
            self.dialog_plus.enabled = device.dialog_plus_enabled;
            self.dialog_plus.value = device.dialog_plus_value;
            self.night_mode = device.night_mode_enabled;
            self.loud_mode = device.loud_mode_enabled;
            self.eq_enabled = device.equalizer_enabled;
            self.eq_bands = device.eq_bands;
        }
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
                    &mut self.device,
                    &mut self.surround,
                    "Surround Sound",
                    SoundFeature::SurroundSound,
                );
                feature_ui(
                    ui,
                    &mut self.device,
                    &mut self.crystalizer,
                    "Crystalizer",
                    SoundFeature::Crystalizer,
                );
                feature_ui(
                    ui,
                    &mut self.device,
                    &mut self.bass,
                    "Bass",
                    SoundFeature::Bass,
                );
                feature_ui(
                    ui,
                    &mut self.device,
                    &mut self.smart_volume,
                    "Smart Volume",
                    SoundFeature::SmartVolume,
                );
                feature_ui(
                    ui,
                    &mut self.device,
                    &mut self.dialog_plus,
                    "Dialog Plus",
                    SoundFeature::DialogPlus,
                );
            });

            ui.add_space(16.0);

            // Column: Toggles (Night Mode + Loud Mode + Equalizer)
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("Toggles")
                        .color(egui::Color32::from_rgb(220, 220, 230)),
                );
                ui.add_space(4.0);

                // Night Mode toggle
                if ui.checkbox(&mut self.night_mode, "🌙 Night Mode").changed()
                    && let Some(device) = self.device.as_mut()
                {
                    if self.night_mode {
                        // Disable loud mode when enabling night mode
                        if self.loud_mode {
                            self.loud_mode = false;
                            let _ = device.disable(SoundFeature::LoudMode).ok();
                        }
                        let _ = device.enable(SoundFeature::NightMode).ok();
                    } else {
                        let _ = device.disable(SoundFeature::NightMode).ok();
                    }
                }

                // Loud Mode toggle
                if ui.checkbox(&mut self.loud_mode, "🔊 Loud Mode").changed()
                    && let Some(device) = self.device.as_mut()
                {
                    if self.loud_mode {
                        // Disable night mode when enabling loud mode
                        if self.night_mode {
                            self.night_mode = false;
                            let _ =
                                device.disable(SoundFeature::NightMode).ok();
                        }
                        let _ = device.enable(SoundFeature::LoudMode).ok();
                    } else {
                        let _ = device.disable(SoundFeature::LoudMode).ok();
                    }
                }

                // Equalizer toggle
                if ui.checkbox(&mut self.eq_enabled, "🎚 Equalizer").changed()
                    && let Some(device) = self.device.as_mut()
                {
                    if self.eq_enabled {
                        let _ = device.enable(SoundFeature::Equalizer).ok();
                    } else {
                        let _ = device.disable(SoundFeature::Equalizer).ok();
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
                    if let Some(device) = self.device.as_mut() {
                        let _ = device.reset().ok();
                    }
                    self.reset_ui();
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // Presets section
                ui.label(
                    egui::RichText::new("Presets")
                        .color(egui::Color32::from_rgb(220, 220, 230)),
                );
                ui.add_space(4.0);

                // Save preset
                ui.label("Save Current Settings:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.save_preset_name)
                        .desired_width(150.0),
                );
                if ui.button("💾 Save Preset").clicked() {
                    if let Some(device) = &self.device {
                        if !self.save_preset_name.trim().is_empty() {
                            match crate::save_preset(
                                device,
                                self.save_preset_name.clone(),
                            ) {
                                Ok(_) => {
                                    self.save_preset_name.clear();
                                    self.refresh_presets();
                                    self.preset_error = None;
                                }
                                Err(e) => {
                                    self.preset_error =
                                        Some(format!("Failed to save: {}", e));
                                }
                            }
                        } else {
                            self.preset_error =
                                Some("Preset name cannot be empty".to_string());
                        }
                    } else {
                        self.preset_error =
                            Some("Device not connected".to_string());
                    }
                }

                // Error message
                if let Some(error) = &self.preset_error {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }

                ui.add_space(4.0);

                // Load preset dropdown
                if !self.presets.is_empty() {
                    let preset_names: Vec<String> =
                        self.presets.iter().map(|p| p.name.clone()).collect();

                    let mut selected_idx = self.selected_preset.unwrap_or(0);
                    if selected_idx >= preset_names.len() {
                        selected_idx = 0;
                    }

                    egui::ComboBox::from_id_salt("preset_selector")
                        .selected_text(if preset_names.is_empty() {
                            "No presets"
                        } else {
                            &preset_names[selected_idx]
                        })
                        .show_ui(ui, |ui| {
                            for (idx, name) in preset_names.iter().enumerate() {
                                if ui
                                    .selectable_value(
                                        &mut selected_idx,
                                        idx,
                                        name,
                                    )
                                    .clicked()
                                {
                                    self.selected_preset = Some(idx);
                                }
                            }
                        });

                    self.selected_preset = Some(selected_idx);

                    ui.add_space(4.0);

                    // Load and Delete buttons
                    ui.horizontal(|ui| {
                        if ui.button("📂 Load").clicked()
                            && let Some(idx) = self.selected_preset
                        {
                            if let Some(device) = self.device.as_mut() {
                                if let Some(preset) = self.presets.get(idx) {
                                    match crate::load_preset(device, preset) {
                                        Ok(_) => {
                                            // Sync UI state with device state
                                            self.sync_ui_from_device();
                                            self.preset_error = None;
                                        }
                                        Err(e) => {
                                            self.preset_error = Some(format!(
                                                "Failed to load: {}",
                                                e
                                            ));
                                        }
                                    }
                                }
                            } else {
                                self.preset_error =
                                    Some("Device not connected".to_string());
                            }
                        }

                        if ui.button("🗑 Delete").clicked()
                            && let Some(idx) = self.selected_preset
                            && let Some(preset) = self.presets.get(idx)
                        {
                            match crate::delete_preset(preset) {
                                Ok(_) => {
                                    self.refresh_presets();
                                    self.selected_preset = None;
                                    self.preset_error = None;
                                }
                                Err(e) => {
                                    self.preset_error = Some(format!(
                                        "Failed to delete: {}",
                                        e
                                    ));
                                }
                            }
                        }
                    });
                } else {
                    ui.label(
                        egui::RichText::new("No presets saved")
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                }

                // Refresh button
                if ui.button("🔄 Refresh").clicked() {
                    self.refresh_presets();
                }

                ui.add_space(4.0);

                // File picker button
                if ui.button("📁 Load from File...").clicked() {
                    self.show_file_picker = true;
                }
            });

            // File picker dialog (using native file dialog)
            if self.show_file_picker {
                self.show_file_picker = false;
                if let Some(device) = self.device.as_mut() {
                    // Use native file dialog
                    let file_path = rfd::FileDialog::new()
                        .add_filter("JSON Presets", &["json"])
                        .set_title("Load Preset")
                        .pick_file();

                    if let Some(path) = file_path {
                        match std::fs::read_to_string(&path) {
                            Ok(json) => {
                                match serde_json::from_str::<Preset>(&json) {
                                    Ok(preset) => {
                                        match crate::load_preset(
                                            device, &preset,
                                        ) {
                                            Ok(_) => {
                                                self.sync_ui_from_device();
                                                self.preset_error = None;
                                            }
                                            Err(e) => {
                                                self.preset_error =
                                                    Some(format!(
                                                        "Failed to load: {}",
                                                        e
                                                    ));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        self.preset_error = Some(format!(
                                            "Invalid preset file: {}",
                                            e
                                        ));
                                    }
                                }
                            }
                            Err(e) => {
                                self.preset_error =
                                    Some(format!("Failed to read file: {}", e));
                            }
                        }
                    }
                } else {
                    self.preset_error =
                        Some("Device not connected".to_string());
                }
            }
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
                for (i, label) in EQ_LABELS.iter().enumerate() {
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
                            let band = self.get_eq_band(i);
                            let db_value = self.eq_bands[i];
                            if let Some(device) = self.device.as_mut() {
                                let _ =
                                    device.set_eq_band_db(band, db_value).ok();
                            }
                        }

                        // Frequency label
                        ui.label(
                            egui::RichText::new(*label)
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
                    ui.add(
                        egui::Slider::new(&mut self.ui_scale, 0.5..=3.0)
                            .step_by(0.1),
                    )
                    .changed();
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
