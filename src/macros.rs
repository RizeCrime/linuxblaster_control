#![allow(unused)]

macro_rules! toggle_button {
    ($enabled:expr, $label:expr) => {
        Button::selectable($enabled, RichText::new($label))
            .min_size(Vec2::new(64.0, 24.0))
            .frame_when_inactive(true)
    };
    ($enabled:expr) => {
        toggle_button!($enabled, "Toggle")
    };
}

macro_rules! slider {
    // Entry point: set defaults and start parsing
    ($value:expr $(, $($rest:tt)*)?) => {
        slider!(@parse ($value) (0.0..=100.0) (1.0) (0) (true) $($($rest)*)?)
    };

    // Parse 'range = val'
    (@parse ($val_expr:expr) ($range:expr) ($step:expr) ($dec:expr) ($vert:expr) range = $v:expr $(, $($rest:tt)*)?) => {
        slider!(@parse ($val_expr) ($v) ($step) ($dec) ($vert) $($($rest)*)?)
    };

    // Parse 'step = val'
    (@parse ($val_expr:expr) ($range:expr) ($step:expr) ($dec:expr) ($vert:expr) step = $v:expr $(, $($rest:tt)*)?) => {
        slider!(@parse ($val_expr) ($range) ($v) ($dec) ($vert) $($($rest)*)?)
    };

    // Parse 'decimals = val'
    (@parse ($val_expr:expr) ($range:expr) ($step:expr) ($dec:expr) ($vert:expr) decimals = $v:expr $(, $($rest:tt)*)?) => {
        slider!(@parse ($val_expr) ($range) ($step) ($v) ($vert) $($($rest)*)?)
    };

    // Parse 'vertical = val'
    (@parse ($val_expr:expr) ($range:expr) ($step:expr) ($dec:expr) ($vert:expr) vertical = $v:expr $(, $($rest:tt)*)?) => {
        slider!(@parse ($val_expr) ($range) ($step) ($dec) ($v) $($($rest)*)?)
    };

    // Final expansion
    (@parse ($val_expr:expr) ($range:expr) ($step:expr) ($dec:expr) ($vert:expr)) => {{
        let s = Slider::new($val_expr, $range)
            .clamping(egui::SliderClamping::Always)
            .fixed_decimals($dec)
            .step_by($step)
            .show_value(false);
        if $vert { s.vertical() } else { s }
    }};
}

macro_rules! drag_value {
    // Entry point
    ($value:expr $(, $($rest:tt)*)?) => {
        drag_value!(@parse ($value) (0.0..=100.0) (1.0) ("%") (0) $($($rest)*)?)
    };

    // Parse keys
    (@parse ($val_expr:expr) ($range:expr) ($step:expr) ($suffix:expr) ($dec:expr) range = $v:expr $(, $($rest:tt)*)?) => {
        drag_value!(@parse ($val_expr) ($v) ($step) ($suffix) ($dec) $($($rest)*)?)
    };
    (@parse ($val_expr:expr) ($range:expr) ($step:expr) ($suffix:expr) ($dec:expr) step = $v:expr $(, $($rest:tt)*)?) => {
        drag_value!(@parse ($val_expr) ($range) ($v) ($suffix) ($dec) $($($rest)*)?)
    };
    (@parse ($val_expr:expr) ($range:expr) ($step:expr) ($suffix:expr) ($dec:expr) suffix = $v:expr $(, $($rest:tt)*)?) => {
        drag_value!(@parse ($val_expr) ($range) ($step) ($v) ($dec) $($($rest)*)?)
    };
    (@parse ($val_expr:expr) ($range:expr) ($step:expr) ($suffix:expr) ($dec:expr) decimals = $v:expr $(, $($rest:tt)*)?) => {
        drag_value!(@parse ($val_expr) ($range) ($step) ($suffix) ($v) $($($rest)*)?)
    };

    // Final expansion
    (@parse ($val_expr:expr) ($range:expr) ($step:expr) ($suffix:expr) ($dec:expr)) => {
        DragValue::new($val_expr)
            .range($range)
            .speed($step)
            .suffix($suffix)
            .fixed_decimals($dec)
    };
}

macro_rules! nav_panes {
    ( $self:expr, $ui:ident, ($($pane_name:expr),* $(,)? ) ) => {
        $ui.vertical(|ui| {
            $(
                ui.scope(|ui| {
                    ui.set_width(160.0);
                    let Ok((feature, _)) = get_feature_cached($self, $pane_name) else { return };

                    ui.vertical_centered_justified(|ui| {
                        ui.label(RichText::new($pane_name).strong());

                        ui.horizontal(|ui| {
                            let toggle_btn = toggle_button!(
                                feature.value.as_bool().expect("Feature must be a Toggle")
                            );

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

macro_rules! sbx_feature {
    ($blaster:expr, $ui:ident, $name:expr) => {
        $ui.vertical_centered_justified(|ui| {
            ui.label(RichText::new($name));
            if let Ok((feature, _)) = get_feature_cached($blaster, $name) {
                let is_enabled = feature.value.as_bool().unwrap_or(false);
                if ui.add(toggle_button!(is_enabled)).clicked() {
                    let _ = $blaster.set_feature($name, None);
                    *CACHE_DIRTY.lock().unwrap() = true;
                }
            }
        });
        $ui.vertical(|ui| {
            if let Ok((feature, _)) = get_feature_cached($blaster, $name) {
                let is_enabled = feature.value.as_bool().unwrap_or(false);
                let slider_name = format!("{} Slider", $name);
                let slider_data = $blaster
                    .features
                    .iter()
                    .find(|f| f.name == slider_name.as_str())
                    .and_then(|f| f.value.as_f32().map(|v| (f.name, v)));

                if let Some((s_name, mut value)) = slider_data {
                    let input =
                        ui.add_enabled(is_enabled, drag_value!(&mut value));

                    if input.changed() {
                        let _ = $blaster.set_slider(s_name, value);
                        *CACHE_DIRTY.lock().unwrap() = true;
                    }

                    let slider = slider!(&mut value, vertical = false);

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
            }
        });
        $ui.end_row();
        $ui.separator();
        $ui.separator();
        $ui.end_row();
    };
}
