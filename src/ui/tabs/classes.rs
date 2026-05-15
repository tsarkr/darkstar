use eframe::egui;
use std::collections::{HashMap, HashSet};
use crate::ui::app::DarkstarApp;
use crate::rules::engine::InferenceEngine;
use sophia::api::prelude::*;

impl DarkstarApp {
    pub fn render_classes_tab(&mut self, ui: &mut egui::Ui) {
        let i = self.i18n();
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(i.class_hierarchy).size(11.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(i.add_subclass).clicked() {
                        let new_uri = self.generate_unique_uri("NewClass");
                        self.manager.add_class(new_uri.clone());
                        if let Some(parent) = &self.selected_uri {
                            self.manager.add_subclass(new_uri.clone(), parent.clone());
                        }
                        self.selected_uri = Some(new_uri);
                    }
                });
            });
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                self.render_class_hierarchy(ui);
            });
        });
    }

    pub fn render_class_hierarchy(&mut self, ui: &mut egui::Ui) {
        use sophia::api::term::matcher::Any;
        let rdfs_subclass_of = InferenceEngine::make_term("http://www.w3.org/2000/01/rdf-schema#subClassOf");
        let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut all_classes = HashSet::new();
        let mut has_parent = HashSet::new();

        let triples = self.manager.memory.main_graph.triples_matching(Any, Some(&rdfs_subclass_of), Any);
        for t in triples.flatten() {
            let child = crate::rules::extract_str(&t.s());
            let parent = crate::rules::extract_str(&t.o());
            children_map.entry(parent.clone()).or_default().push(child.clone());
            all_classes.insert(child.clone());
            all_classes.insert(parent.clone());
            has_parent.insert(child);
        }

        let rdf_type = InferenceEngine::make_term("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
        let owl_class = InferenceEngine::make_term("http://www.w3.org/2002/07/owl#Class");
        let type_triples = self.manager.memory.main_graph.triples_matching(Any, Some(&rdf_type), Some(&owl_class));
        for t in type_triples.flatten() {
            all_classes.insert(crate::rules::extract_str(&t.s()));
        }

        let mut roots: Vec<_> = all_classes.into_iter().filter(|c| !has_parent.contains(c)).collect();
        roots.sort();
        for root in roots {
            self.render_hierarchy_node(ui, &root, &children_map);
        }
    }

    pub fn render_hierarchy_node(&mut self, ui: &mut egui::Ui, uri: &str, children_map: &HashMap<String, Vec<String>>) {
        let query = self.search_query.to_lowercase();
        if !query.is_empty() && !self.matches_search_recursive(uri, children_map, &query) {
            return;
        }

        let label = Self::get_label(uri);
        let children = children_map.get(uri);
        let is_selected = self.selected_uri.as_deref() == Some(uri);
        let is_match = !query.is_empty() && (uri.to_lowercase().contains(&query) || label.to_lowercase().contains(&query));
        
        let text = if is_match {
            egui::RichText::new(label).color(egui::Color32::from_rgb(255, 165, 0)).strong()
        } else {
            egui::RichText::new(label)
        };

        if let Some(children) = children {
            let mut collapsing = egui::CollapsingHeader::new(text);
            if is_selected || !query.is_empty() { collapsing = collapsing.default_open(true); }
            collapsing.show(ui, |ui| {
                if ui.selectable_label(is_selected, " (self)").clicked() {
                    self.selected_uri = Some(uri.to_string());
                    self.graph_needs_sync = true;
                }
                let mut sorted_children = children.clone();
                sorted_children.sort();
                for child in sorted_children { self.render_hierarchy_node(ui, &child, children_map); }
            });
        } else {
            if ui.selectable_label(is_selected, text).clicked() {
                self.selected_uri = Some(uri.to_string());
                self.graph_needs_sync = true;
            }
        }
    }

    fn matches_search_recursive(&self, uri: &str, children_map: &HashMap<String, Vec<String>>, query: &str) -> bool {
        if uri.to_lowercase().contains(query) || Self::get_label(uri).to_lowercase().contains(query) {
            return true;
        }
        if let Some(children) = children_map.get(uri) {
            for child in children {
                if self.matches_search_recursive(child, children_map, query) {
                    return true;
                }
            }
        }
        false
    }
}
