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
        let rdf_type = InferenceEngine::make_term(crate::vocab::RDF_TYPE);
        let mut individuals_by_type: HashMap<String, Vec<String>> = HashMap::new();
        // Subjects typed with a schema type (class, property, ontology…) are not individuals.
        let meta_classes: HashSet<String> = crate::vocab::META_TYPES.iter().map(|t| crate::vocab::iri(t)).collect();

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
        // Sort once here instead of cloning + sorting every frame.
        for inds in individuals_by_type.values_mut() {
            inds.sort_unstable();
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
            let matching: Vec<(&String, String)> = inds
                .iter()
                .map(|ind| (ind, Self::get_label(ind)))
                .filter(|(ind, label)| self.matches_search(ind, label, &query))
                .collect();

            if matching.is_empty() && !query.is_empty() { continue; }

            let collapsed = self.collapsed_types.contains(type_uri);
            rows.push(IndividualRow::Header {
                type_uri: type_uri.clone(),
                text: format!("{} {} ({})", i.type_label, Self::get_label(type_uri), matching.len()),
                collapsed,
            });
            if !collapsed {
                rows.extend(matching.into_iter().map(|(uri, label)| IndividualRow::Item { uri: uri.clone(), label }));
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
