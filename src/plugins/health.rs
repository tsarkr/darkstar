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

    pub fn run_check(&mut self, manager: &DarkstarManager) {
        use sophia::api::term::matcher::Any;
        self.isolated_nodes.clear();
        self.missing_labels.clear();
        self.empty_classes.clear();

        let g = &manager.memory.main_graph;
        let rdf_type = InferenceEngine::make_term(crate::vocab::RDF_TYPE);
        let rdfs_label = InferenceEngine::make_term(crate::vocab::RDFS_LABEL);
        let owl_class = InferenceEngine::make_term(crate::vocab::OWL_CLASS);

        let mut connected_entities: HashSet<&str> = HashSet::with_capacity(64_000);
        let mut type_only_entities: HashSet<&str> = HashSet::new();

        use sophia::api::term::SimpleTerm;
        for t in g.triples().flatten() {
            let is_type = &rdf_type == t[1];
            if !is_type {
                if let SimpleTerm::Iri(s_iri) = t[0] {
                    connected_entities.insert(s_iri.as_str());
                }
                if let SimpleTerm::Iri(o_iri) = t[2] {
                    connected_entities.insert(o_iri.as_str());
                }
            } else {
                if let SimpleTerm::Iri(s_iri) = t[0] {
                    let s = s_iri.as_str();
                    if !connected_entities.contains(s) {
                        type_only_entities.insert(s);
                    }
                }
                if let SimpleTerm::Iri(o_iri) = t[2] {
                    let o = o_iri.as_str();
                    if !connected_entities.contains(o) {
                        type_only_entities.insert(o);
                    }
                }
            }
        }

        let mut has_label: HashSet<&str> = HashSet::with_capacity(connected_entities.len());
        for t in g.triples_matching(Any, Some(&rdfs_label), Any).flatten() {
            if let SimpleTerm::Iri(s_iri) = t[0] {
                has_label.insert(s_iri.as_str());
            }
        }

        // 1. Isolated Nodes & 2. Missing Labels
        for &entity in &connected_entities {
            if !has_label.contains(entity) {
                let mut s = String::with_capacity(2 + entity.len());
                s.push_str("i:");
                s.push_str(entity);
                self.missing_labels.push(s);
            }
        }
        for &entity in &type_only_entities {
            if !connected_entities.contains(entity) {
                let mut s = String::with_capacity(2 + entity.len());
                s.push_str("i:");
                s.push_str(entity);
                if !has_label.contains(entity) {
                    self.missing_labels.push(s.clone());
                }
                self.isolated_nodes.push(s);
            }
        }
        self.isolated_nodes.sort_unstable();
        self.missing_labels.sort_unstable();

        // 3. Empty Classes (indexed check per class)
        for t in g.triples_matching(Any, Some(&rdf_type), Some(&owl_class)).flatten() {
            if t.s().iri().is_some() {
                let c_term = t.s();
                if g.triples_matching(Any, Some(&rdf_type), Some(c_term)).next().is_none() {
                    self.empty_classes.push(crate::rules::extract_str(&c_term));
                }
            }
        }
        self.empty_classes.sort_unstable();
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
