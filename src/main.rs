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
    use std::time::{Duration, Instant};
    let total = Instant::now();
    let mut manager = crate::core::manager::DarkstarManager::new();
    let rules = crate::core::settings::ReasonerRules::default();
    let t = Instant::now();
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
    let t = Instant::now();
    crate::plugins::load_all(&mut pm, &mut manager);
    println!("[bench] plugin init: {:?}", t.elapsed());

    let t = Instant::now();
    let dl_idx = crate::query::dlquery::DlIndex::build(&manager.memory.main_graph);
    println!("[bench] DlIndex::build (individuals={}, labels={}): {:?}", dl_idx.individuals.len(), dl_idx.labels.len(), t.elapsed());

    let t = Instant::now();
    let mut hc = crate::plugins::health::HealthCheckerPlugin::new();
    hc.run_check(&manager);
    println!("[bench] HealthCheckerPlugin::run_check: {:?}", t.elapsed());

    let t = Instant::now();
    let q1 = "SELECT ?class (COUNT(?x) AS ?n) WHERE { ?x a ?class } GROUP BY ?class ORDER BY DESC(?n) LIMIT 50";
    let _ = crate::query::sparql::run_query(&manager.memory.main_graph, q1, 10000, Duration::from_secs(30));
    println!("[bench] SPARQL group-by rdf:type: {:?}", t.elapsed());

    let t = Instant::now();
    let q2 = "SELECT ?p (COUNT(*) AS ?n) WHERE { ?s ?p ?o } GROUP BY ?p ORDER BY DESC(?n)";
    let _ = crate::query::sparql::run_query(&manager.memory.main_graph, q2, 10000, Duration::from_secs(30));
    println!("[bench] SPARQL group-by all predicates: {:?}", t.elapsed());

    let t = Instant::now();
    let q3 = "SELECT ?x ?label WHERE { ?x rdfs:label ?label . FILTER(CONTAINS(STR(?label), \"강명하\")) } LIMIT 100";
    let _ = crate::query::sparql::run_query(&manager.memory.main_graph, q3, 10000, Duration::from_secs(30));
    println!("[bench] SPARQL label filter: {:?}", t.elapsed());

    let t = Instant::now();
    let s = manager.serialize_to_string(crate::core::io::OntologyFormat::Turtle, false).map(|s| s.len()).unwrap_or(0);
    println!("[bench] serialize turtle ({} bytes): {:?}", s, t.elapsed());

    // Benchmark Reasoner when schema axioms (equivalentClass, disjointWith, intersectionOf) exist on large graph
    {
        let mut m2 = crate::core::manager::DarkstarManager::new();
        m2.memory.asserted_graph = manager.memory.asserted_graph.clone();
        // Add 1 equivalentClass, 1 disjointWith, 1 intersectionOf
        m2.add_assertion(
            "i:http://www.cidoc-crm.org/cidoc-crm/E7_Activity".to_string(),
            crate::vocab::OWL_EQUIVALENT_CLASS.to_string(),
            "i:http://example.org/Action".to_string(),
        );
        m2.add_assertion(
            "i:http://www.cidoc-crm.org/cidoc-crm/E39_Actor".to_string(),
            crate::vocab::OWL_DISJOINT_WITH.to_string(),
            "i:http://www.cidoc-crm.org/cidoc-crm/E5_Event".to_string(),
        );
        m2.invalidate_reasoning();
        let t = Instant::now();
        m2.run_reasoning(&rules);
        println!("[bench] full reasoning with eqc + disjoint: {:?}", t.elapsed());
    }

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
