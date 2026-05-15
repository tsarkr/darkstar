use eframe::egui;
use std::collections::{HashMap, HashSet};
use crate::ui::app::DarkstarApp;
use crate::rules::engine::InferenceEngine;
use sophia::api::prelude::*;

impl DarkstarApp {
    pub fn render_individuals_tab(&mut self, ui: &mut egui::Ui) {
        let i = self.i18n();
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(i.individuals).size(11.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("✚ New Individual").clicked() {
                        let new_uri = self.generate_unique_uri("NewIndividual");
                        self.manager.add_individual(new_uri.clone());
                        self.selected_uri = Some(new_uri);
                    }
                });
            });
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                self.render_individuals_list(ui);
            });
        });
    }

    pub fn render_individuals_list(&mut self, ui: &mut egui::Ui) {
        use sophia::api::term::matcher::Any;
        let rdf_type = InferenceEngine::make_term("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
        let mut individuals_by_type: HashMap<String, Vec<String>> = HashMap::new();
        let meta_classes: HashSet<_> = [
            "i:http://www.w3.org/2002/07/owl#Class",
            "i:http://www.w3.org/2000/01/rdf-schema#Class",
            "i:http://www.w3.org/2002/07/owl#ObjectProperty",
            "i:http://www.w3.org/2002/07/owl#DatatypeProperty",
            "i:http://www.w3.org/1999/02/22-rdf-syntax-ns#Property",
            "i:http://www.w3.org/2002/07/owl#Ontology",
        ].into_iter().collect();

        let mut known_meta = HashSet::new();
        let triples = self.manager.memory.main_graph.triples_matching(Any, Some(&rdf_type), Any);
        
        let mut candidates = Vec::new();
        for t in triples.flatten() {
            let s = crate::rules::extract_str(&t.s());
            let o = crate::rules::extract_str(&t.o());
            if meta_classes.contains(o.as_str()) { known_meta.insert(s); }
            else { candidates.push((s, o)); }
        }

        for (s, o) in candidates {
            if !known_meta.contains(&s) { individuals_by_type.entry(o).or_default().push(s); }
        }

        let mut types: Vec<_> = individuals_by_type.keys().cloned().collect();
        types.sort();

        let i = self.i18n();
        let query = self.search_query.to_lowercase();
        for type_uri in types {
            let mut inds = individuals_by_type.get(&type_uri).unwrap().clone();
            inds.sort();
            
            // Filter individuals
            let filtered_inds: Vec<_> = if query.is_empty() {
                inds
            } else {
                inds.into_iter().filter(|ind| {
                    ind.to_lowercase().contains(&query) || Self::get_label(ind).to_lowercase().contains(&query)
                }).collect()
            };

            if filtered_inds.is_empty() && !query.is_empty() { continue; }

            egui::CollapsingHeader::new(format!("{} {}", i.type_label, Self::get_label(&type_uri)))
                .default_open(true)
                .show(ui, |ui| {
                    for ind in filtered_inds {
                        let is_selected = self.selected_uri.as_deref() == Some(&ind);
                        ui.horizontal(|ui| {
                            let mut is_checked = self.selected_individuals.contains(&ind);
                            if ui.checkbox(&mut is_checked, "").changed() {
                                if is_checked { self.selected_individuals.insert(ind.clone()); }
                                else { self.selected_individuals.remove(&ind); }
                            }
                            if ui.selectable_label(is_selected, Self::get_label(&ind)).clicked() {
                                self.selected_uri = Some(ind.clone());
                                self.graph_needs_sync = true;
                            }
                        });
                    }
                });
        }
    }
}
