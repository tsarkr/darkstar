use crate::core::plugin::DarkstarPlugin;
use crate::core::manager::DarkstarManager;
use crate::core::history::DarkstarEvent;
use crate::core::l10n::L10n;
use crate::rules::engine::InferenceEngine;
use eframe::egui;
use sophia::api::prelude::*;
use std::collections::HashMap;

pub struct EntityStatsPlugin {
    counts: HashMap<String, usize>,
    total_triples: usize,
}

impl EntityStatsPlugin {
    pub fn new() -> Self {
        Self {
            counts: HashMap::new(),
            total_triples: 0,
        }
    }

    fn refresh(&mut self, manager: &DarkstarManager) {
        self.total_triples = manager.memory.main_graph.triples().count();
        self.counts.clear();

        let rdf_type = InferenceEngine::make_term("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
        let owl_class = InferenceEngine::make_term("http://www.w3.org/2002/07/owl#Class");
        let owl_obj_prop = InferenceEngine::make_term("http://www.w3.org/2002/07/owl#ObjectProperty");
        let owl_data_prop = InferenceEngine::make_term("http://www.w3.org/2002/07/owl#DatatypeProperty");
        let owl_individual = InferenceEngine::make_term("http://www.w3.org/2002/07/owl#NamedIndividual");

        let mut classes = 0;
        let mut obj_props = 0;
        let mut data_props = 0;
        let mut individuals = 0;

        for t in manager.memory.main_graph.triples().flatten() {
            if rdf_type == t.p() {
                if owl_class == t.o() { classes += 1; }
                else if owl_obj_prop == t.o() { obj_props += 1; }
                else if owl_data_prop == t.o() { data_props += 1; }
                else if owl_individual == t.o() { individuals += 1; }
            }
        }

        self.counts.insert("Classes".to_string(), classes);
        self.counts.insert("Object Properties".to_string(), obj_props);
        self.counts.insert("Data Properties".to_string(), data_props);
        self.counts.insert("Individuals".to_string(), individuals);
    }
}

impl DarkstarPlugin for EntityStatsPlugin {
    fn name(&self, l10n: &L10n) -> String { l10n.entity_stats.to_string() }
    fn description(&self, l10n: &L10n) -> String { 
        if l10n.plugins_menu == "플러그인" {
            "온톨로지 엔티티 및 트리플에 대한 상세한 분석을 제공합니다.".to_string()
        } else {
            "Provides a detailed breakdown of ontology entities and triples.".to_string()
        }
    }
    fn version(&self) -> &str { "1.0.0" }

    fn init(&mut self, manager: &mut DarkstarManager) {
        self.refresh(manager);
    }

    fn render_tab(&mut self, ui: &mut egui::Ui, manager: &mut DarkstarManager, l10n: &L10n) {
        ui.heading(format!("📊 {}", l10n.entity_stats));
        ui.add_space(10.0);
        
        ui.group(|ui| {
            ui.label(format!("{}: {}", l10n.total_triples, self.total_triples));
            ui.separator();
            
            let labels = [
                ("Classes", l10n.classes),
                ("Object Properties", l10n.obj_props),
                ("Data Properties", l10n.data_props),
                ("Individuals", l10n.individuals),
            ];

            for (key, label) in labels {
                let count = self.counts.get(key).cloned().unwrap_or(0);
                ui.horizontal(|ui| {
                    ui.label(format!("{}:", label));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(count.to_string()).strong());
                    });
                });
            }
        });

        ui.add_space(20.0);
        if ui.button(format!("🔄 {}", l10n.refresh_data)).clicked() {
            self.refresh(manager);
        }
    }

    fn handle_event(&mut self, _event: &DarkstarEvent) {}
}
