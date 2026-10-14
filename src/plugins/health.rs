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
    /// Set by events; the full scan runs lazily when the tab is shown.
    dirty: bool,
}

/// Max entries rendered per section (lists can contain 100k+ entities).
const MAX_LIST_ROWS: usize = 500;

fn render_capped(ui: &mut egui::Ui, items: &[String]) {
    for node in items.iter().take(MAX_LIST_ROWS) {
        ui.label(node);
    }
    if items.len() > MAX_LIST_ROWS {
        ui.label(egui::RichText::new(format!("… (+{})", items.len() - MAX_LIST_ROWS)).weak());
    }
}

impl Default for HealthCheckerPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthCheckerPlugin {
    pub fn new() -> Self {
        Self {
            isolated_nodes: Vec::new(),
            missing_labels: Vec::new(),
            empty_classes: Vec::new(),
            dirty: true,
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

        let rdf_type = InferenceEngine::make_term(crate::vocab::RDF_TYPE);
        let rdfs_label = InferenceEngine::make_term(crate::vocab::RDFS_LABEL);
        let owl_class = InferenceEngine::make_term(crate::vocab::OWL_CLASS);

        for t in manager.memory.main_graph.triples().flatten() {
            let s = crate::rules::extract_str(&t.s());
            let o_is_literal = t.o().kind() == sophia::api::term::TermKind::Literal;
            let is_type = rdf_type == t.p();
            // Literal objects never become entities, so skip allocating a string for them
            // unless it is needed (it never is for literals).
            let o = if o_is_literal { String::new() } else { crate::rules::extract_str(&t.o()) };

            if !s.starts_with("l:") && !all_entities.contains(&s) { all_entities.insert(s.clone()); }
            if !o_is_literal && !all_entities.contains(&o) { all_entities.insert(o.clone()); }

            // Check connections (excluding rdf:type)
            if !is_type {
                if !connected_entities.contains(&s) { connected_entities.insert(s.clone()); }
                if !o_is_literal && !connected_entities.contains(&o) { connected_entities.insert(o.clone()); }
            }

            // Check labels
            if is_type && owl_class == t.o() {
                classes.push(s.clone());
            }

            if rdfs_label == t.p() {
                has_label.insert(s);
            } else if is_type && !o_is_literal {
                class_with_instances.insert(o);
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

    fn init(&mut self, _manager: &mut DarkstarManager) {
        self.dirty = true;
    }

    fn render_tab(&mut self, ui: &mut egui::Ui, manager: &mut DarkstarManager, l10n: &L10n) {
        if self.dirty {
            self.run_check(manager);
            self.dirty = false;
        }
        ui.heading(format!("🩺 {}", l10n.health_report));
        ui.add_space(10.0);

        if ui.button(format!("🚀 {}", l10n.run_scan)).clicked() {
            self.run_check(manager);
        }

        ui.add_space(15.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.collapsing(format!("⚠️ {} ({})", l10n.isolated_entities, self.isolated_nodes.len()), |ui| {
                render_capped(ui, &self.isolated_nodes);
            });

            ui.collapsing(format!("📝 {} ({})", l10n.missing_labels, self.missing_labels.len()), |ui| {
                render_capped(ui, &self.missing_labels);
            });

            ui.collapsing(format!("📁 {} ({})", l10n.empty_classes, self.empty_classes.len()), |ui| {
                render_capped(ui, &self.empty_classes);
            });
        });
    }

    fn handle_event(&mut self, _event: &DarkstarEvent, _manager: &mut DarkstarManager) {
        // Defer the full scan until the tab is shown.
        self.dirty = true;
    }
}
