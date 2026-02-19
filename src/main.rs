#![allow(unused)]

use std::sync::LazyLock;

use eframe::egui::{self, Vec2, ahash::HashMap, mutex::Mutex};
use linuxblaster_control::BlasterXG6;
use tracing::Level;

mod app;
use app::BlasterApp;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    let device = BlasterXG6::init();
    let app = BlasterApp(device);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_resizable(true)
            .with_inner_size(Vec2::new(1050.0, 600.0)),
        // .with_inner_size()
        ..Default::default()
    };

    eframe::run_native(
        "Sound Blaster X G6 Control",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            cc.egui_ctx.set_pixels_per_point(1.5);

            #[cfg(debug_assertions)]
            {
                cc.egui_ctx.debug_painter();
                cc.egui_ctx.set_debug_on_hover(false);
            }

            Ok(Box::new(app))
        }),
    )
}

/// Results from "<headphone_name> FixedBandEQ.txt"
/// mapped to ten bands (31Hz, 62Hz, 125Hz, 250Hz, 500Hz, 1kHz, 2kHz, 4kHz, 8kHz, 16kHz)
#[derive(Debug, Clone)]
pub struct HeadphoneResult {
    pub tester: &'static str,
    pub variant: Option<&'static str>,
    pub test_device: Option<&'static str>,
    pub preamp: f32,
    pub ten_band_eq: [f32; 10],
}

struct AutoEqDb {
    // HashMap
    // Key: Name (DT 990 Pro (250 Ohm))
    // Value: Vec<HeadphoneResult>
    results:
        Option<&'static phf::Map<&'static str, &'static [HeadphoneResult]>>,
}

include!(concat!(env!("OUT_DIR"), "/autoeq_db.rs"));
