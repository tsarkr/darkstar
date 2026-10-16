use eframe::egui;
use std::collections::{HashMap, HashSet};
use crate::ui::app::DarkstarApp;
use crate::ui::IndividualRow;
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
            self.render_individuals_list(ui);
        });
    }

    pub fn rebuild_individuals_cache(&mut self) {
        if !self.individuals_cache_dirty { return; }
        use sophia::api::term::matcher::Any;
        let g = &self.manager.memory.main_graph;
        let rdf_type = InferenceEngine::make_term(crate::vocab::RDF_TYPE);
        let mut individuals_by_type: HashMap<String, Vec<String>> = HashMap::new();

        // 1. Collect meta-entities (classes, properties, ontologies) via indexed object lookups
        let mut known_meta: HashSet<String> = HashSet::new();
        for &meta_iri in crate::vocab::META_TYPES {
            let meta_term = InferenceEngine::make_term(meta_iri);
            for t in g.triples_matching(Any, Some(&rdf_type), Some(&meta_term)).flatten() {
                known_meta.insert(crate::rules::extract_str(&t.s()));
            }
        }
        let meta_classes: HashSet<&'static str> = crate::vocab::META_TYPES.iter().copied().collect();

        // 2. Group non-meta typed subjects by class
        for t in g.triples_matching(Any, Some(&rdf_type), Any).flatten() {
            if let Some(o_iri) = t.o().iri() {
                if meta_classes.contains(o_iri.as_str()) { continue; }
            }
            let s = crate::rules::extract_str(&t.s());
            if known_meta.contains(&s) { continue; }
            let o = crate::rules::extract_str(&t.o());
            individuals_by_type.entry(o).or_default().push(s);
        }

        // Sort and deduplicate once here instead of cloning + sorting every frame.
        for inds in individuals_by_type.values_mut() {
            inds.sort_unstable();
            inds.dedup();
        }

        self.individuals_by_type = individuals_by_type;
        self.individuals_cache_dirty = false;
        self.individual_rows_dirty = true;
    }

    /// Flattens types + individuals into display rows, applying the search filter and collapse state.
    fn rebuild_individual_rows(&mut self) {
        let query = self.search_query.to_lowercase();
        if !self.individual_rows_dirty && query == self.individual_rows_query { return; }

        if !query.is_empty() { self.ensure_label_index(); }
        let i = self.i18n();
        let mut types: Vec<&String> = self.individuals_by_type.keys().collect();
        types.sort_unstable();

        let mut rows = Vec::new();
        for type_uri in types {
            let inds = &self.individuals_by_type[type_uri];
            let collapsed = self.collapsed_types.contains(type_uri);

            if query.is_empty() {
                rows.push(IndividualRow::Header {
                    type_uri: type_uri.clone(),
                    text: format!("{} {} ({})", i.type_label, Self::get_label(type_uri), inds.len()),
                    collapsed,
                });
                if !collapsed {
                    rows.extend(inds.iter().map(|uri| IndividualRow::Item {
                        uri: uri.clone(),
                        label: Self::get_label(uri),
                    }));
                }
            } else if collapsed {
                let match_count = inds.iter().filter(|ind| self.matches_uri_search(ind, &query)).count();
                if match_count == 0 { continue; }
                rows.push(IndividualRow::Header {
                    type_uri: type_uri.clone(),
                    text: format!("{} {} ({})", i.type_label, Self::get_label(type_uri), match_count),
                    collapsed: true,
                });
            } else {
                let matching: Vec<&String> = inds
                    .iter()
                    .filter(|ind| self.matches_uri_search(ind, &query))
                    .collect();
                if matching.is_empty() { continue; }
                rows.push(IndividualRow::Header {
                    type_uri: type_uri.clone(),
                    text: format!("{} {} ({})", i.type_label, Self::get_label(type_uri), matching.len()),
                    collapsed: false,
                });
                rows.extend(matching.into_iter().map(|uri| IndividualRow::Item {
                    uri: uri.clone(),
                    label: Self::get_label(uri),
                }));
            }
        }

        self.individual_rows = rows;
        self.individual_rows_query = query;
        self.individual_rows_dirty = false;
    }

    pub fn render_individuals_list(&mut self, ui: &mut egui::Ui) {
        self.rebuild_individuals_cache();
        self.rebuild_individual_rows();

        // Only the rows inside the viewport are laid out, so the list stays fast
        // even with hundreds of thousands of individuals.
        let rows = std::mem::take(&mut self.individual_rows);
        let row_height = ui.spacing().interact_size.y;
        let mut toggle_type: Option<String> = None;

        egui::ScrollArea::vertical()
            .id_salt("individuals_list")
            .auto_shrink([false, false])
            .show_rows(ui, row_height, rows.len(), |ui, range| {
                for row in &rows[range] {
                    match row {
                        IndividualRow::Header { type_uri, text, collapsed } => {
                            let arrow = if *collapsed { "▶" } else { "▼" };
                            let resp = ui.add(
                                egui::Label::new(egui::RichText::new(format!("{} {}", arrow, text)).strong())
                                    .sense(egui::Sense::click())
                                    .truncate(),
                            );
                            if resp.clicked() { toggle_type = Some(type_uri.clone()); }
                        }
                        IndividualRow::Item { uri, label } => {
                            let is_selected = self.selected_uri.as_deref() == Some(uri.as_str());
                            ui.horizontal(|ui| {
                                ui.add_space(12.0);
                                let mut is_checked = self.selected_individuals.contains(uri);
                                if ui.checkbox(&mut is_checked, "").changed() {
                                    if is_checked { self.selected_individuals.insert(uri.clone()); }
                                    else { self.selected_individuals.remove(uri); }
                                }
                                if ui.selectable_label(is_selected, label).clicked() {
                                    self.selected_uri = Some(uri.clone());
                                    self.graph_needs_sync = true;
                                }
                            });
                        }
                    }
                }
            });

        self.individual_rows = rows;
        if let Some(t) = toggle_type {
            if !self.collapsed_types.remove(&t) { self.collapsed_types.insert(t); }
            self.individual_rows_dirty = true;
        }
    }
}
