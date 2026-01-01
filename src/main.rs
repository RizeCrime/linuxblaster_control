#![allow(unused)]

mod ui;

use blaster_x_g6_control::BlasterXG6;
use eframe::egui;
use ui::BlasterApp;

fn main() -> eframe::Result<()> {
    let device = BlasterXG6::init().ok();

    // Note: sizes are in physical pixels, UI scale (1.5x) is applied after
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([750.0, 750.0]) // 500*1.5, 500*1.5
            .with_min_inner_size([750.0, 750.0])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        "Sound Blaster X G6 Control",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(BlasterApp::new(device)))
        }),
    )
}
