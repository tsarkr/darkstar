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
        let g = &self.manager.memory.main_graph;
        let (roots, children) = property_hierarchy(g, crate::vocab::OWL_OBJECT_PROPERTY);
        self.object_prop_roots = roots;
        self.object_prop_children = children;
        let (roots, children) = property_hierarchy(g, crate::vocab::OWL_DATATYPE_PROPERTY);
        self.datatype_prop_roots = roots;
        self.datatype_prop_children = children;
        self.properties_cache_dirty = false;
    }

    pub fn render_property_hierarchy(&mut self, ui: &mut egui::Ui, filter_object: bool) {
        self.rebuild_properties_cache();
        let query = self.search_query.to_lowercase();
        if !query.is_empty() { self.ensure_label_index(); }

        if filter_object {
            let roots = self.object_prop_roots.clone();
            let children_map = std::mem::take(&mut self.object_prop_children);
            let visible = if query.is_empty() {
                None
            } else {
                Some(self.compute_visible_hierarchy_nodes(&roots, &children_map, &query))
            };
            let mut path_visited = HashSet::new();
            for root in roots {
                self.render_hierarchy_node_inner(ui, &root, &children_map, &query, visible.as_ref(), &mut path_visited);
            }
            self.object_prop_children = children_map;
        } else {
            let roots = self.datatype_prop_roots.clone();
            let children_map = std::mem::take(&mut self.datatype_prop_children);
            let visible = if query.is_empty() {
                None
            } else {
                Some(self.compute_visible_hierarchy_nodes(&roots, &children_map, &query))
            };
            let mut path_visited = HashSet::new();
            for root in roots {
                self.render_hierarchy_node_inner(ui, &root, &children_map, &query, visible.as_ref(), &mut path_visited);
            }
            self.datatype_prop_children = children_map;
        }
    }
}

/// Root properties and `parent -> children` map for properties declared with `prop_type`
/// (plus anything linked to them by rdfs:subPropertyOf). Lists are transitively reduced and sorted.
fn property_hierarchy(g: &crate::memory::DarkstarGraph, prop_type: &str) -> (Vec<String>, HashMap<String, Vec<String>>) {
    use sophia::api::term::matcher::Any;
    let rdfs_subprop_of = InferenceEngine::make_term(crate::vocab::RDFS_SUBPROPERTY_OF);
    let rdf_type = InferenceEngine::make_term(crate::vocab::RDF_TYPE);
    let target_type = InferenceEngine::make_term(prop_type);

    let mut all_props = HashSet::new();
    let mut raw_edges = Vec::new();
    for t in g.triples_matching(Any, Some(&rdf_type), Some(&target_type)).flatten() {
        all_props.insert(crate::rules::extract_str(&t.s()));
    }
    for t in g.triples_matching(Any, Some(&rdfs_subprop_of), Any).flatten() {
        let child = crate::rules::extract_str(&t.s());
        let parent = crate::rules::extract_str(&t.o());
        if all_props.contains(&child) || all_props.contains(&parent) {
            all_props.insert(child.clone());
            all_props.insert(parent.clone());
            if child != parent {
                raw_edges.push((child, parent));
            }
        }
    }
    crate::ui::tabs::classes::build_reduced_hierarchy(all_props, raw_edges)
}
