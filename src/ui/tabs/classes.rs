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

    pub fn rebuild_classes_cache(&mut self) {
        if !self.classes_cache_dirty { return; }
        use sophia::api::term::matcher::Any;
        let g = &self.manager.memory.main_graph;
        let rdfs_subclass_of = InferenceEngine::make_term(crate::vocab::RDFS_SUBCLASS_OF);
        let mut all_classes = HashSet::new();
        let mut raw_edges = Vec::new();

        for t in g.triples_matching(Any, Some(&rdfs_subclass_of), Any).flatten() {
            let child = crate::rules::extract_str(&t.s());
            let parent = crate::rules::extract_str(&t.o());
            all_classes.insert(child.clone());
            all_classes.insert(parent.clone());
            if child != parent {
                raw_edges.push((child, parent));
            }
        }

        let rdf_type = InferenceEngine::make_term(crate::vocab::RDF_TYPE);
        for class_meta in [crate::vocab::OWL_CLASS, crate::vocab::RDFS_CLASS] {
            let meta_term = InferenceEngine::make_term(class_meta);
            for t in g.triples_matching(Any, Some(&rdf_type), Some(&meta_term)).flatten() {
                all_classes.insert(crate::rules::extract_str(&t.s()));
            }
        }

        let (roots, children_map) = build_reduced_hierarchy(all_classes, raw_edges);
        self.class_roots = roots;
        self.class_children = children_map;
        self.classes_cache_dirty = false;
    }

    pub fn render_class_hierarchy(&mut self, ui: &mut egui::Ui) {
        self.rebuild_classes_cache();
        let query = self.search_query.to_lowercase();
        if !query.is_empty() { self.ensure_label_index(); }

        let roots = self.class_roots.clone();
        let children_map = std::mem::take(&mut self.class_children);
        let visible = if query.is_empty() {
            None
        } else {
            Some(self.compute_visible_hierarchy_nodes(&roots, &children_map, &query))
        };

        let mut path_visited = HashSet::new();
        for root in roots {
            self.render_hierarchy_node_inner(ui, &root, &children_map, &query, visible.as_ref(), &mut path_visited);
        }

        self.class_children = children_map;
    }

    pub fn render_hierarchy_node(&mut self, ui: &mut egui::Ui, uri: &str, children_map: &HashMap<String, Vec<String>>) {
        let query = self.search_query.to_lowercase();
        let roots = vec![uri.to_string()];
        let visible = if query.is_empty() {
            None
        } else {
            Some(self.compute_visible_hierarchy_nodes(&roots, children_map, &query))
        };
        let mut path_visited = HashSet::new();
        self.render_hierarchy_node_inner(ui, uri, children_map, &query, visible.as_ref(), &mut path_visited);
    }

    pub(crate) fn render_hierarchy_node_inner(
        &mut self,
        ui: &mut egui::Ui,
        uri: &str,
        children_map: &HashMap<String, Vec<String>>,
        query: &str,
        visible: Option<&HashSet<String>>,
        path_visited: &mut HashSet<String>,
    ) {
        if let Some(vis) = visible {
            if !vis.contains(uri) {
                return;
            }
        }
        if !path_visited.insert(uri.to_string()) {
            return;
        }

        let label = Self::get_label(uri);
        let children = children_map.get(uri);
        let is_selected = self.selected_uri.as_deref() == Some(uri);
        let is_match = !query.is_empty() && self.matches_search(uri, &label, query);

        let text = if is_match {
            egui::RichText::new(label).color(egui::Color32::from_rgb(255, 165, 0)).strong()
        } else {
            egui::RichText::new(label)
        };

        if let Some(children) = children {
            let mut collapsing = egui::CollapsingHeader::new(text).id_salt(uri);
            if is_selected || !query.is_empty() { collapsing = collapsing.default_open(true); }
            collapsing.show(ui, |ui| {
                if ui.selectable_label(is_selected, " (self)").clicked() {
                    self.selected_uri = Some(uri.to_string());
                    self.graph_needs_sync = true;
                }
                for child in children {
                    self.render_hierarchy_node_inner(ui, child, children_map, query, visible, path_visited);
                }
            });
        } else if ui.selectable_label(is_selected, text).clicked() {
            self.selected_uri = Some(uri.to_string());
            self.graph_needs_sync = true;
        }

        path_visited.remove(uri);
    }

    pub(crate) fn compute_visible_hierarchy_nodes(
        &self,
        roots: &[String],
        children_map: &HashMap<String, Vec<String>>,
        query: &str,
    ) -> HashSet<String> {
        let mut memo: HashMap<String, bool> = HashMap::new();
        let mut visiting: HashSet<String> = HashSet::new();
        for root in roots {
            self.eval_visible_dfs(root, children_map, query, &mut memo, &mut visiting);
        }
        memo.into_iter().filter_map(|(k, v)| if v { Some(k) } else { None }).collect()
    }

    fn eval_visible_dfs(
        &self,
        uri: &str,
        children_map: &HashMap<String, Vec<String>>,
        query: &str,
        memo: &mut HashMap<String, bool>,
        visiting: &mut HashSet<String>,
    ) -> bool {
        if let Some(&res) = memo.get(uri) {
            return res;
        }
        if !visiting.insert(uri.to_string()) {
            return false;
        }
        let mut vis = self.matches_uri_search(uri, query);
        if let Some(children) = children_map.get(uri) {
            for child in children {
                if self.eval_visible_dfs(child, children_map, query, memo, visiting) {
                    vis = true;
                }
            }
        }
        visiting.remove(uri);
        memo.insert(uri.to_string(), vis);
        vis
    }
}

/// Builds a transitively-reduced `parent -> children` hierarchy and sorted root list.
/// Removes self-loops (`C subClassOf C`), redundant transitive edges (`A -> C` when `A -> B -> C`),
/// and ensures cyclic components still have a deterministic root.
pub(crate) fn build_reduced_hierarchy(
    all_nodes: HashSet<String>,
    raw_edges: Vec<(String, String)>,
) -> (Vec<String>, HashMap<String, Vec<String>>) {
    let mut parents_of: HashMap<String, HashSet<String>> = HashMap::new();
    for (child, parent) in raw_edges {
        if child != parent {
            parents_of.entry(child).or_default().insert(parent);
        }
    }

    // Compute transitive reduction: keep edge (child -> parent) only if no intermediate `mid`
    // exists with (child -> mid -> parent), ignoring mutual equivalence cycles (parent <-> mid).
    let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut has_parent: HashSet<String> = HashSet::new();

    for (child, parents) in &parents_of {
        for parent in parents {
            let is_redundant = parents.iter().any(|mid| {
                if mid == parent {
                    return false;
                }
                let mid_has_parent = parents_of.get(mid).is_some_and(|mp| mp.contains(parent));
                let parent_has_mid = parents_of.get(parent).is_some_and(|pp| pp.contains(mid));
                mid_has_parent && !parent_has_mid
            });
            if !is_redundant {
                children_map.entry(parent.clone()).or_default().push(child.clone());
                has_parent.insert(child.clone());
            }
        }
    }

    for children in children_map.values_mut() {
        children.sort_unstable();
        children.dedup();
    }

    let mut roots: Vec<String> = all_nodes.iter().filter(|c| !has_parent.contains(*c)).cloned().collect();
    roots.sort_unstable();

    // Ensure nodes in isolated equivalence cycles are still reachable from at least one root
    let mut reachable: HashSet<String> = HashSet::with_capacity(all_nodes.len());
    let mut stack: Vec<String> = roots.clone();
    while let Some(node) = stack.pop() {
        if reachable.insert(node.clone()) {
            if let Some(children) = children_map.get(&node) {
                for ch in children {
                    if !reachable.contains(ch) {
                        stack.push(ch.clone());
                    }
                }
            }
        }
    }
    if reachable.len() < all_nodes.len() {
        let mut remaining: Vec<String> = all_nodes.into_iter().filter(|n| !reachable.contains(n)).collect();
        remaining.sort_unstable();
        for node in remaining {
            if !reachable.contains(&node) {
                roots.push(node.clone());
                stack.push(node);
                while let Some(cur) = stack.pop() {
                    if reachable.insert(cur.clone()) {
                        if let Some(children) = children_map.get(&cur) {
                            for ch in children {
                                if !reachable.contains(ch) {
                                    stack.push(ch.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
        roots.sort_unstable();
    }

    (roots, children_map)
}
