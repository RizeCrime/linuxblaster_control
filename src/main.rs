use blaster_x_g6_control::BlasterXG6;
use eframe::egui;
use tracing::Level;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    let device = BlasterXG6::init().expect("Failed to initialize device");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_resizable(true),
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
                cc.egui_ctx.set_debug_on_hover(true);
            }

            Ok(Box::new(device))
        }),
    )
}
