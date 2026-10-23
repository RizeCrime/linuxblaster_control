#![allow(unused)]

use std::sync::LazyLock;

use eframe::egui::{self, Vec2, ahash::HashMap, mutex::Mutex};
use linuxblaster_control::BlasterXG6;
use tracing::Level;

mod app;
use app::Launcher;

fn main() -> eframe::Result<()> {
    // RUST_LOG overrides this, e.g. RUST_LOG=debug linuxblaster_control
    let default_level = if cfg!(debug_assertions) {
        Level::DEBUG
    } else {
        Level::INFO
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(default_level.into())
                .from_env_lossy(),
        )
        .init();

    let app = Launcher::new();

    let icon = eframe::icon_data::from_png_bytes(include_bytes!(
        "../packaging/linuxblaster-control.png"
    ))
    .expect("bundled icon is a valid PNG");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Linuxblaster Control")
            // matches the .desktop file, so the taskbar shows the right icon
            .with_app_id("linuxblaster-control")
            .with_icon(icon)
            .with_resizable(true)
            .with_inner_size(Vec2::new(1050.0, 600.0))
            // the two-column panes need at least this much room
            .with_min_inner_size(Vec2::new(1050.0, 600.0)),
        // .with_inner_size()
        ..Default::default()
    };

    eframe::run_native(
        "Linuxblaster Control",
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
