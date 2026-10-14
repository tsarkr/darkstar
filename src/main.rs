mod memory;
pub mod vocab;
pub mod graph_util;
mod rules;
pub mod query;
pub mod core;
pub mod plugins;
pub mod ui;

use eframe::egui;
use crate::ui::app::DarkstarApp;

/// Headless benchmark: `darkstar --bench <file.ttl>`
/// Measures parse + reasoning + UI cache-equivalent work without opening a window.
fn run_bench(path: &std::path::Path) {
    use sophia::api::prelude::*;
    let total = std::time::Instant::now();
    let mut manager = crate::core::manager::DarkstarManager::new();
    let rules = crate::core::settings::ReasonerRules::default();
    let t = std::time::Instant::now();
    if let Err(e) = manager.load_from_file(path, &rules) {
        eprintln!("load failed: {}", e);
        return;
    }
    println!("[bench] load_from_file (parse + reasoning): {:?}", t.elapsed());
    println!(
        "[bench] asserted={} inferred={} main={}",
        manager.memory.asserted_graph.triples().count(),
        manager.memory.inferred_graph.triples().count(),
        manager.memory.main_graph.triples().count()
    );

    let mut pm = crate::core::plugin::PluginManager::new();
    let t = std::time::Instant::now();
    crate::plugins::load_all(&mut pm, &mut manager);
    println!("[bench] plugin init: {:?}", t.elapsed());

    let t = std::time::Instant::now();
    let s = manager.serialize_to_string(crate::core::io::OntologyFormat::Turtle, false).map(|s| s.len()).unwrap_or(0);
    println!("[bench] serialize turtle ({} bytes): {:?}", s, t.elapsed());
    println!("[bench] TOTAL: {:?}", total.elapsed());
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--bench") {
        if let Some(p) = args.get(pos + 1) {
            run_bench(std::path::Path::new(p));
            return Ok(());
        }
    }

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
