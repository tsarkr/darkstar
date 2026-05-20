use crate::core::plugin::DarkstarPlugin;
use crate::core::manager::DarkstarManager;
use crate::core::history::DarkstarEvent;
use crate::core::l10n::L10n;
use crate::rules::engine::InferenceEngine;
use eframe::egui;
use sophia::api::prelude::*;
use std::collections::HashSet;

pub struct HealthCheckerPlugin {
    isolated_nodes: Vec<String>,
    missing_labels: Vec<String>,
    empty_classes: Vec<String>,
}

impl HealthCheckerPlugin {
    pub fn new() -> Self {
        Self {
            isolated_nodes: Vec::new(),
            missing_labels: Vec::new(),
            empty_classes: Vec::new(),
        }
    }

    fn run_check(&mut self, manager: &DarkstarManager) {
        self.isolated_nodes.clear();
        self.missing_labels.clear();
        self.empty_classes.clear();

        let mut all_entities = HashSet::new();
        let mut connected_entities = HashSet::new();
        let mut has_label = HashSet::new();
        let mut class_with_instances = HashSet::new();
        let mut classes = Vec::new();

        let rdf_type = InferenceEngine::make_term("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
        let rdfs_label = InferenceEngine::make_term("http://www.w3.org/2000/01/rdf-schema#label");
        let owl_class = InferenceEngine::make_term("http://www.w3.org/2002/07/owl#Class");

        for t in manager.memory.main_graph.triples().flatten() {
            let s = crate::rules::extract_str(&t.s());
            let p = crate::rules::extract_str(&t.p());
            let o = crate::rules::extract_str(&t.o());

            if !s.starts_with("l:") { all_entities.insert(s.clone()); }
            if !o.starts_with("l:") { all_entities.insert(o.clone()); }

            // Check connections (excluding rdf:type)
            if p != "i:http://www.w3.org/1999/02/22-rdf-syntax-ns#type" {
                connected_entities.insert(s.clone());
                if !o.starts_with("l:") { connected_entities.insert(o.clone()); }
            }

            // Check labels
            if rdf_type == t.p() && owl_class == t.o() {
                classes.push(s.clone());
            }

            if rdfs_label == t.p() {
                has_label.insert(s.clone());
            }

            if rdf_type == t.p() {
                class_with_instances.insert(o.clone());
            }
        }

        // 1. Isolated Nodes
        for entity in &all_entities {
            if !connected_entities.contains(entity) && !entity.starts_with("_:") {
                self.isolated_nodes.push(entity.clone());
            }
        }

        // 2. Missing Labels
        for entity in &all_entities {
            if !has_label.contains(entity) && !entity.starts_with("_:") {
                self.missing_labels.push(entity.clone());
            }
        }

        // 3. Empty Classes
        for class_uri in classes {
            if !class_with_instances.contains(&class_uri) {
                self.empty_classes.push(class_uri);
            }
        }
    }
}

impl DarkstarPlugin for HealthCheckerPlugin {
    fn name(&self, l10n: &L10n) -> String { l10n.health_checker.to_string() }
    fn description(&self, l10n: &L10n) -> String { 
        if l10n.plugins_menu == "플러그인" {
            "온톨로지에서 고립된 노드, 누락된 라벨, 비어 있는 클래스를 스캔합니다.".to_string()
        } else {
            "Scans ontology for isolated nodes, missing labels, and empty classes.".to_string()
        }
    }
    fn version(&self) -> &str { "1.0.0" }

    fn init(&mut self, manager: &mut DarkstarManager) {
        self.run_check(manager);
    }

    fn render_tab(&mut self, ui: &mut egui::Ui, manager: &mut DarkstarManager, l10n: &L10n) {
        ui.heading(format!("🩺 {}", l10n.health_report));
        ui.add_space(10.0);

        if ui.button(format!("🚀 {}", l10n.run_scan)).clicked() {
            self.run_check(manager);
        }

        ui.add_space(15.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.collapsing(format!("⚠️ {} ({})", l10n.isolated_entities, self.isolated_nodes.len()), |ui| {
                for node in &self.isolated_nodes {
                    ui.label(node);
                }
            });

            ui.collapsing(format!("📝 {} ({})", l10n.missing_labels, self.missing_labels.len()), |ui| {
                for node in &self.missing_labels {
                    ui.label(node);
                }
            });

            ui.collapsing(format!("📁 {} ({})", l10n.empty_classes, self.empty_classes.len()), |ui| {
                for class_uri in &self.empty_classes {
                    ui.label(class_uri);
                }
            });
        });
    }

    fn handle_event(&mut self, _event: &DarkstarEvent, manager: &mut DarkstarManager) {
        self.run_check(manager);
    }
}
