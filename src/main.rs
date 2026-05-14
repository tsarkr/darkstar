mod memory;
mod rules;
pub mod core;
pub mod plugins;
pub mod ui;

use eframe::egui;
use crate::ui::app::DarkstarApp;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_icon(
                eframe::icon_data::from_png_bytes(&include_bytes!("../assets/icon.png")[..])
                    .unwrap_or_default(),
            ),
        ..Default::default()
    };
    
    eframe::run_native(
        "Darkstar Ontology Editor",
        native_options,
        Box::new(|cc| Ok(Box::new(DarkstarApp::new(cc)))),
    )
}
