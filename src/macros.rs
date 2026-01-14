#![allow(unused)]

macro_rules! toggle_button {
    // Entry point: set defaults
    ($ui:expr, $enabled:expr, $label:expr $(, $($rest:tt)*)?) => {
        toggle_button!(@parse ($ui) ($enabled) ($label) (None) (None) $($($rest)*)?)
    };

    ($ui:expr, $enabled:expr) => {
        toggle_button!($ui, $enabled, "Toggle")
    };

    // Parse 'width = full'
    (@parse ($ui:expr) ($e:expr) ($l:expr) ($w:expr) ($h:expr) width = full $(, $($rest:tt)*)?) => {
        toggle_button!(@parse ($ui) ($e) ($l) (Some($ui.available_width())) ($h) $($($rest)*)?)
    };

    // Parse 'width = val'
    (@parse ($ui:expr) ($e:expr) ($l:expr) ($w:expr) ($h:expr) width = $val:expr $(, $($rest:tt)*)?) => {
        toggle_button!(@parse ($ui) ($e) ($l) (Some($val)) ($h) $($($rest)*)?)
    };

    // Final expansion
    (@parse ($ui:expr) ($e:expr) ($l:expr) ($w:expr) ($h:expr)) => {{
        let btn = Button::selectable($e, RichText::new($l))
            .min_size(Vec2::new(64.0, 24.0))
            .frame_when_inactive(true);
        if let Some(w) = $w {
             $ui.add_sized(Vec2::new(w, 24.0), btn)
        } else {
             $ui.add(btn)
        }
    }};
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
                    let is_enabled = {
                        let Ok((feature, _)) = $self.get_feature($pane_name) else { return };
                        feature.value.as_bool().expect("Feature must be a Toggle")
                    };

                    ui.vertical_centered_justified(|ui| {
                        ui.label(RichText::new($pane_name).strong());

                        ui.horizontal(|ui| {
                            if toggle_button!(
                                ui,
                                is_enabled
                            ).clicked() {
                                debug!("{} clicked", $pane_name);
                                let _ = $self.set_feature($pane_name, None);
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
                if toggle_button!(ui, is_enabled).clicked() {
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
    };
}

macro_rules! eq_plot {
    // Entry point: default values (width=None, height=Some(80.0), aspect=true)
    ($ui:expr, $gains_opt:expr $(, $($rest:tt)*)?) => {
        eq_plot!(@parse ($ui) ($gains_opt) (None) (Some(80.0)) (true) $($($rest)*)?)
    };

    // Parse 'width = full'
    (@parse ($ui:expr) ($gains:expr) ($w:expr) ($h:expr) ($a:expr) width = full $(, $($rest:tt)*)?) => {
        eq_plot!(@parse ($ui) ($gains) (Some($ui.available_width())) ($h) (false) $($($rest)*)?)
    };

    // Parse 'width = val'
    (@parse ($ui:expr) ($gains:expr) ($w:expr) ($h:expr) ($a:expr) width = $val:expr $(, $($rest:tt)*)?) => {
        eq_plot!(@parse ($ui) ($gains) (Some($val)) ($h) (false) $($($rest)*)?)
    };

    // Parse 'height = full'
    (@parse ($ui:expr) ($gains:expr) ($w:expr) ($h:expr) ($a:expr) height = full $(, $($rest:tt)*)?) => {
        eq_plot!(@parse ($ui) ($gains) ($w) (Some($ui.available_height())) (false) $($($rest)*)?)
    };

    // Parse 'height = val'
    (@parse ($ui:expr) ($gains:expr) ($w:expr) ($h:expr) ($a:expr) height = $val:expr $(, $($rest:tt)*)?) => {
        eq_plot!(@parse ($ui) ($gains) ($w) (Some($val)) (false) $($($rest)*)?)
    };

    // Final expansion
    (@parse ($ui:expr) ($gains_opt:expr) ($w:expr) ($h:expr) ($a:expr)) => {{
        if let Some(gains) = $gains_opt {
            let mut plot = Plot::new("current_eq_curve")
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
                // .y_axis_formatter(|_mark, _range| String::from(" "))
                .show_axes(Vec2b::new(true, false))
                .show_grid(true)
                .include_y(-12.0)
                .include_y(12.0)
                .include_x(20.0_f64.log10())
                .include_x(16000.0_f64.log10())
                .allow_scroll(false)
                .allow_zoom(false)
                .allow_drag(false)
                .allow_axis_zoom_drag(false)
                .allow_boxed_zoom(false);

            if $a {
                plot = plot.view_aspect(3.0);
            }

            if let Some(w) = $w {
                plot = plot.width(w);
            }
            if let Some(h) = $h {
                plot = plot.height(h);
            }

            plot.show($ui, |plot_ui| {
                let points: PlotPoints = (0..=500)
                    .map(|i| {
                        let t = i as f64 / 500.0;
                        let f = 20.0 * (20000.0 / 20.0_f64).powf(t);

                        let mut total_y = 0.0;
                        // Skip index 0 (Pre-Amp)
                        for (idx, &center_freq) in ISO_BANDS.iter().enumerate()
                        {
                            let gain = gains[idx + 1];
                            if gain.abs() > 0.01 {
                                total_y += calculate_peaking_eq_response(
                                    f,
                                    center_freq as f64,
                                    gain as f64,
                                    1.41,
                                );
                            }
                        }

                        [f.log10(), total_y]
                    })
                    .collect();

                plot_ui.line(Line::new("current_eq_line", points).width(2.0));
            })
            .response
        } else {
            $ui.label("EQ Data Unavailable")
        }
    }};
}
