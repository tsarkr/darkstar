use eframe::egui;
use std::collections::{HashMap, HashSet};
use crate::ui::app::DarkstarApp;
use crate::rules::engine::InferenceEngine;
use sophia::api::prelude::*;

impl DarkstarApp {
    pub fn render_properties_tab(&mut self, ui: &mut egui::Ui, filter_object: bool) {
        let i = self.i18n();
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(if filter_object { i.obj_props } else { i.data_props }).size(11.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(if filter_object { i.add_subproperty } else { i.add_sibling_property }).clicked() {
                        let new_uri = self.generate_unique_uri(if filter_object { "NewObjectProperty" } else { "NewDataProperty" });
                        if filter_object { self.manager.add_object_property(new_uri.clone()); }
                        else { self.manager.add_data_property(new_uri.clone()); }
                        
                        if let Some(parent) = &self.selected_uri {
                            self.manager.add_subproperty(new_uri.clone(), parent.clone());
                        }
                        self.selected_uri = Some(new_uri);
                    }
                });
            });
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                self.render_property_hierarchy(ui, filter_object);
            });
        });
    }

    pub fn rebuild_properties_cache(&mut self) {
        if !self.properties_cache_dirty { return; }
        use sophia::api::term::matcher::Any;
        let rdfs_subprop_of = InferenceEngine::make_term("http://www.w3.org/2000/01/rdf-schema#subPropertyOf");
        let rdf_type = InferenceEngine::make_term("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");

        // Object Properties
        {
            let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
            let mut all_props = HashSet::new();
            let mut has_parent = HashSet::new();
            let target_type = InferenceEngine::make_term("http://www.w3.org/2002/07/owl#ObjectProperty");
            let type_triples = self.manager.memory.main_graph.triples_matching(Any, Some(&rdf_type), Some(&target_type));
            for t in type_triples.flatten() { all_props.insert(crate::rules::extract_str(&t.s())); }

            let triples = self.manager.memory.main_graph.triples_matching(Any, Some(&rdfs_subprop_of), Any);
            for t in triples.flatten() {
                let child = crate::rules::extract_str(&t.s());
                let parent = crate::rules::extract_str(&t.o());
                if all_props.contains(&child) || all_props.contains(&parent) {
                    children_map.entry(parent.clone()).or_default().push(child.clone());
                    all_props.insert(child.clone());
                    all_props.insert(parent.clone());
                    has_parent.insert(child);
                }
            }
            let mut roots: Vec<_> = all_props.into_iter().filter(|p| !has_parent.contains(p)).collect();
            roots.sort();
            self.object_prop_roots = roots;
            self.object_prop_children = children_map;
        }

        // Datatype Properties
        {
            let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
            let mut all_props = HashSet::new();
            let mut has_parent = HashSet::new();
            let target_type = InferenceEngine::make_term("http://www.w3.org/2002/07/owl#DatatypeProperty");
            let type_triples = self.manager.memory.main_graph.triples_matching(Any, Some(&rdf_type), Some(&target_type));
            for t in type_triples.flatten() { all_props.insert(crate::rules::extract_str(&t.s())); }

            let triples = self.manager.memory.main_graph.triples_matching(Any, Some(&rdfs_subprop_of), Any);
            for t in triples.flatten() {
                let child = crate::rules::extract_str(&t.s());
                let parent = crate::rules::extract_str(&t.o());
                if all_props.contains(&child) || all_props.contains(&parent) {
                    children_map.entry(parent.clone()).or_default().push(child.clone());
                    all_props.insert(child.clone());
                    all_props.insert(parent.clone());
                    has_parent.insert(child);
                }
            }
            let mut roots: Vec<_> = all_props.into_iter().filter(|p| !has_parent.contains(p)).collect();
            roots.sort();
            self.datatype_prop_roots = roots;
            self.datatype_prop_children = children_map;
        }

        self.properties_cache_dirty = false;
    }

    pub fn render_property_hierarchy(&mut self, ui: &mut egui::Ui, filter_object: bool) {
        self.rebuild_properties_cache();

        if filter_object {
            let roots = self.object_prop_roots.clone();
            let children_map = std::mem::take(&mut self.object_prop_children);
            for root in roots {
                self.render_hierarchy_node(ui, &root, &children_map);
            }
            self.object_prop_children = children_map;
        } else {
            let roots = self.datatype_prop_roots.clone();
            let children_map = std::mem::take(&mut self.datatype_prop_children);
            for root in roots {
                self.render_hierarchy_node(ui, &root, &children_map);
            }
            self.datatype_prop_children = children_map;
        }
    }
}
