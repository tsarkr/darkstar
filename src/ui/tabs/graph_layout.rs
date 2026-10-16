use std::collections::{HashMap, HashSet};
use eframe::egui;
use egui::Vec2;
use sophia::api::prelude::*;
use crate::ui::app::DarkstarApp;
use crate::ui::{GraphNode, GraphEdge, NodeType, LayoutMode};

/// Focus-mode node cap per step; the graph's "show more" button adds this many again.
pub const FOCUS_NODES_STEP: usize = 300;

impl DarkstarApp {
    pub fn sync_graph(&mut self) {
        let mut new_edges = Vec::new();
        let mut seen_nodes = HashSet::new();
        let mut node_types = HashMap::new();

        use crate::vocab as v;
        use sophia::api::term::matcher::Any;
        let g_main = &self.manager.memory.main_graph;
        let rdf_type_term = crate::rules::engine::InferenceEngine::make_term(v::RDF_TYPE);
        let rdfs_subclass_term = crate::rules::engine::InferenceEngine::make_term(v::RDFS_SUBCLASS_OF);
        let rdfs_subprop_term = crate::rules::engine::InferenceEngine::make_term(v::RDFS_SUBPROPERTY_OF);

        // 1. Classify all schema entities (classes & properties) via indexed lookups in O(|schema|)
        for class_iri in [v::OWL_CLASS, v::RDFS_CLASS] {
            let cls_term = crate::rules::engine::InferenceEngine::make_term(class_iri);
            for t in g_main.triples_matching(Any, Some(&rdf_type_term), Some(&cls_term)).flatten() {
                node_types.insert(crate::rules::extract_str(&t.s()), NodeType::Class);
            }
        }
        for t in g_main.triples_matching(Any, Some(&rdfs_subclass_term), Any).flatten() {
            node_types.insert(crate::rules::extract_str(&t.s()), NodeType::Class);
            node_types.insert(crate::rules::extract_str(&t.o()), NodeType::Class);
        }
        for prop_iri in [v::OWL_OBJECT_PROPERTY, v::OWL_DATATYPE_PROPERTY] {
            let prop_term = crate::rules::engine::InferenceEngine::make_term(prop_iri);
            for t in g_main.triples_matching(Any, Some(&rdf_type_term), Some(&prop_term)).flatten() {
                node_types.insert(crate::rules::extract_str(&t.s()), NodeType::Property);
            }
        }
        for t in g_main.triples_matching(Any, Some(&rdfs_subprop_term), Any).flatten() {
            node_types.insert(crate::rules::extract_str(&t.s()), NodeType::Property);
            node_types.insert(crate::rules::extract_str(&t.o()), NodeType::Property);
        }

        let classify_node = |uri: &str, node_types: &HashMap<String, NodeType>| -> NodeType {
            if uri.starts_with("l:") {
                NodeType::Literal
            } else if uri.starts_with("b:") || uri.starts_with("_:") {
                NodeType::Blank
            } else {
                *node_types.get(uri).unwrap_or(&NodeType::Individual)
            }
        };

        let is_system = |p: &str| -> bool {
            p.contains("/22-rdf-syntax-ns#")
                || p.contains("/owl#")
                || p.contains("/rdf-schema#")
                || Self::contains_case_insensitive(p, "#type")
                || Self::contains_case_insensitive(p, "#first")
                || Self::contains_case_insensitive(p, "#rest")
                || Self::contains_case_insensitive(p, "#onproperty")
                || Self::contains_case_insensitive(p, "#somevaluesfrom")
                || Self::contains_case_insensitive(p, "#allvaluesfrom")
                || Self::contains_case_insensitive(p, "#unionof")
                || Self::contains_case_insensitive(p, "#intersectionof")
                || Self::contains_case_insensitive(p, "#oneof")
                || Self::contains_case_insensitive(p, "#inverseof")
                || Self::contains_case_insensitive(p, "#equivalentclass")
                || Self::contains_case_insensitive(p, "#equivalentproperty")
                || Self::contains_case_insensitive(p, "#namedindividual")
                || Self::contains_case_insensitive(p, "#ontology")
                || Self::contains_case_insensitive(p, "#imports")
                || p.ends_with("/type")
                || p.ends_with("/first")
                || p.ends_with("/rest")
        };

        // Reset the "show more" limit whenever the focused entity changes.
        if self.focus_limit_uri != self.selected_uri {
            self.focus_limit_uri = self.selected_uri.clone();
            self.focus_node_limit = FOCUS_NODES_STEP;
        }
        let limit = self.focus_node_limit;
        let query = self.search_query.trim().to_lowercase();
        if !query.is_empty() { self.ensure_label_index(); }
        self.graph_sync_query = self.search_query.clone();

        let mut visible_in_focus = HashSet::new();
        let mut truncated = false;
        if self.filters.focus_mode {
            let mut roots = Vec::new();
            if let Some(selected) = &self.selected_uri { roots.push(selected.clone()); }
            roots.extend(self.expanded_nodes.iter().cloned());

            let mut queue = std::collections::VecDeque::new();
            for r in roots { queue.push_back((r, 0)); }

            while let Some((u, d)) = queue.pop_front() {
                if visible_in_focus.len() >= limit {
                    truncated = true;
                    break;
                }
                if !visible_in_focus.insert(u.clone()) { continue; }
                if d >= self.filters.expansion_depth { continue; }

                let budget = (limit * 2).saturating_sub(queue.len() + visible_in_focus.len());
                let mut matched: Vec<String> = Vec::new();
                let mut others: Vec<String> = Vec::new();
                let mut scanned = 0usize;
                let max_hub_scan = 10_000usize;

                let u_term = crate::rules::engine::InferenceEngine::make_term(&u);
                let graphs: [&crate::memory::DarkstarGraph; 3] = [
                    &self.manager.memory.asserted_graph,
                    &self.manager.memory.inferred_graph,
                    &self.manager.memory.imported_graph,
                ];
                'neigh: for (g_idx, g) in graphs.iter().enumerate() {
                    if g_idx == 1 && !self.filters.show_inferred { continue; }
                    let outgoing = g.triples_matching(Some(&u_term), Any, Any)
                        .flatten().map(|t| (crate::rules::extract_str(&t.p()), crate::rules::extract_str(&t.o())));
                    let incoming = g.triples_matching(Any, Any, Some(&u_term))
                        .flatten().map(|t| (crate::rules::extract_str(&t.p()), crate::rules::extract_str(&t.s())));
                    for (p, n) in outgoing.chain(incoming) {
                        if !self.filters.show_system && is_system(&p) { continue; }
                        let n_type = classify_node(&n, &node_types);
                        if !self.filters.show_literals && n_type == NodeType::Literal { continue; }
                        if !self.filters.show_schema && (n_type == NodeType::Class || n_type == NodeType::Property) { continue; }
                        if !self.filters.show_individuals && n_type == NodeType::Individual { continue; }

                        scanned += 1;
                        if query.is_empty() {
                            if others.len() >= budget { truncated = true; break 'neigh; }
                            others.push(n);
                        } else if self.matches_uri_search(&n, &query) {
                            if matched.len() < budget { matched.push(n); } else { truncated = true; }
                        } else if others.len() < budget {
                            others.push(n);
                        } else {
                            truncated = true;
                            if matched.len() >= budget || scanned >= max_hub_scan { break 'neigh; }
                        }
                        if scanned >= max_hub_scan {
                            truncated = true;
                            break 'neigh;
                        }
                    }
                }

                let remaining = budget.saturating_sub(matched.len());
                for n in matched.into_iter().rev() { queue.push_front((n, d + 1)); }
                for n in others.into_iter().take(remaining) { queue.push_back((n, d + 1)); }
            }
        }
        self.graph_truncated = truncated;

        let mut seen_triples = HashSet::new();
        
        if self.filters.focus_mode {
            let graphs = [&self.manager.memory.asserted_graph, &self.manager.memory.inferred_graph, &self.manager.memory.imported_graph];
            for (g_idx, &g) in graphs.iter().enumerate() {
                let is_inferred = g_idx == 1;
                if is_inferred && !self.filters.show_inferred { continue; }
                for u in &visible_in_focus {
                    let u_term = crate::rules::engine::InferenceEngine::make_term(u);
                    for t in g.triples_matching(Some(&u_term), Any, Any).flatten() {
                        let o = crate::rules::extract_str(&t.o());
                        if !visible_in_focus.contains(&o) { continue; }
                        let p = crate::rules::extract_str(&t.p());
                        if !self.filters.show_system && is_system(&p) { continue; }

                        let s_type = classify_node(u, &node_types);
                        let o_type = classify_node(&o, &node_types);

                        if !self.filters.show_literals && o_type == NodeType::Literal { continue; }
                        if !self.filters.show_schema && (s_type == NodeType::Class || o_type == NodeType::Class || s_type == NodeType::Property) { continue; }
                        if !self.filters.show_individuals && (s_type == NodeType::Individual || o_type == NodeType::Individual) { continue; }

                        let s = u.clone();
                        let key = (s.clone(), p.clone(), o.clone());
                        if !seen_triples.insert(key) { continue; }

                        seen_nodes.insert(s.clone());
                        seen_nodes.insert(o.clone());
                        new_edges.push(GraphEdge { from: s, to: o, label: Self::get_label(&p), is_inferred });
                    }
                }
            }
        } else {
            let max_edges = 500;
            let mut edge_count = 0;

            for t in self.manager.memory.asserted_graph.triples().flatten() {
                if edge_count >= max_edges { break; }
                let is_o_literal = t.o().lexical_form().is_some() && t.o().iri().is_none();
                if !self.filters.show_literals && is_o_literal { continue; }

                let p = crate::rules::extract_str(&t.p());
                if !self.filters.show_system && is_system(&p) { continue; }

                let s = crate::rules::extract_str(&t.s());
                let s_type = classify_node(&s, &node_types);
                if !self.filters.show_individuals && s_type == NodeType::Individual { continue; }
                if !self.filters.show_schema && (s_type == NodeType::Class || s_type == NodeType::Property) { continue; }

                let o = crate::rules::extract_str(&t.o());
                let o_type = if is_o_literal { NodeType::Literal } else { classify_node(&o, &node_types) };
                if !self.filters.show_individuals && o_type == NodeType::Individual { continue; }
                if !self.filters.show_schema && o_type == NodeType::Class { continue; }

                let key = (s.clone(), p.clone(), o.clone());
                if !seen_triples.insert(key) { continue; }

                seen_nodes.insert(s.clone());
                seen_nodes.insert(o.clone());
                new_edges.push(GraphEdge { from: s, to: o, label: Self::get_label(&p), is_inferred: false });
                edge_count += 1;
            }

            if self.filters.show_inferred {
                for t in self.manager.memory.inferred_graph.triples().flatten() {
                    if edge_count >= max_edges { break; }
                    let is_o_literal = t.o().lexical_form().is_some() && t.o().iri().is_none();
                    if !self.filters.show_literals && is_o_literal { continue; }

                    let p = crate::rules::extract_str(&t.p());
                    if !self.filters.show_system && is_system(&p) { continue; }

                    let s = crate::rules::extract_str(&t.s());
                    let s_type = classify_node(&s, &node_types);
                    if !self.filters.show_individuals && s_type == NodeType::Individual { continue; }
                    if !self.filters.show_schema && (s_type == NodeType::Class || s_type == NodeType::Property) { continue; }

                    let o = crate::rules::extract_str(&t.o());
                    let o_type = if is_o_literal { NodeType::Literal } else { classify_node(&o, &node_types) };
                    if !self.filters.show_individuals && o_type == NodeType::Individual { continue; }
                    if !self.filters.show_schema && o_type == NodeType::Class { continue; }

                    if !self.manager.memory.asserted_graph.contains(t.s(), t.p(), t.o()).unwrap_or(false) {
                        let key = (s.clone(), p.clone(), o.clone());
                        if !seen_triples.insert(key) { continue; }

                        seen_nodes.insert(s.clone());
                        seen_nodes.insert(o.clone());
                        new_edges.push(GraphEdge { from: s, to: o, label: Self::get_label(&p), is_inferred: true });
                        edge_count += 1;
                    }
                }
            }
        }

        let mut new_node_count = 0;
        for uri in &seen_nodes {
            let ntype = classify_node(uri, &node_types);
            if let Some(node) = self.nodes.get_mut(uri) {
                node.label = Self::get_label(uri);
                node.node_type = ntype;
            } else {
                let angle = new_node_count as f32 * 137.5 * std::f32::consts::PI / 180.0;
                let radius = (new_node_count as f32).sqrt() * 25.0;
                self.nodes.insert(uri.clone(), GraphNode {
                    pos: Vec2::new(400.0 + angle.cos() * radius, 300.0 + angle.sin() * radius),
                    vel: Vec2::ZERO,
                    label: Self::get_label(uri),
                    node_type: ntype,
                });
                new_node_count += 1;
            }
        }

        if new_node_count > 0 { self.simulation_alpha = 25.0; }

        self.nodes.retain(|k, _| seen_nodes.contains(k));
        self.edges = new_edges;

        match self.filters.layout_mode {
            LayoutMode::Hierarchical => {
                self.apply_hierarchical_layout();
                self.resolve_collisions(50);
            }
            LayoutMode::Radial => {
                self.apply_radial_layout();
                self.resolve_collisions(50);
            }
            LayoutMode::Grid => {
                self.apply_grid_layout();
                self.resolve_collisions(50);
            }
            LayoutMode::Circular => {
                self.apply_circular_layout();
                self.resolve_collisions(50);
            }
            LayoutMode::Concentric => {
                self.apply_concentric_layout();
                self.resolve_collisions(50);
            }
            LayoutMode::ForceDirected => {},
        }
        self.graph_needs_sync = false;
    }

    pub fn apply_hierarchical_layout(&mut self) {
        let mut layers: HashMap<String, usize> = HashMap::new();
        let mut children: HashMap<String, Vec<String>> = HashMap::new();
        for edge in &self.edges {
            if edge.label == "subClassOf" { children.entry(edge.to.clone()).or_default().push(edge.from.clone()); }
        }
        let roots: Vec<String> = self.nodes.keys().filter(|k| !self.edges.iter().any(|e| &e.from == *k && e.label == "subClassOf")).cloned().collect();
        let mut queue = std::collections::VecDeque::new();
        for r in roots { queue.push_back((r, 0)); }
        while let Some((node, layer)) = queue.pop_front() {
            layers.insert(node.clone(), layer);
            if let Some(subs) = children.get(&node) { for sub in subs { queue.push_back((sub.clone(), layer + 1)); } }
        }
        let mut layer_counts = HashMap::new();
        for (uri, layer) in layers {
            let count = layer_counts.entry(layer).or_insert(0);
            if let Some(node) = self.nodes.get_mut(&uri) { node.pos = Vec2::new(*count as f32 * 85.0 + 40.0, layer as f32 * 75.0 + 40.0); }
            *count += 1;
        }
    }

    pub fn apply_radial_layout(&mut self) {
        if self.nodes.is_empty() { return; }
        let center = self.selected_uri.clone().unwrap_or_else(|| self.nodes.keys().next().unwrap().clone());
        let mut dists = HashMap::new();
        let mut queue = std::collections::VecDeque::new();
        dists.insert(center.clone(), 0);
        queue.push_back(center);
        while let Some(u) = queue.pop_front() {
            let d = dists[&u];
            for edge in &self.edges {
                let v = if edge.from == u { &edge.to } else if edge.to == u { &edge.from } else { continue };
                if !dists.contains_key(v) { dists.insert(v.clone(), d + 1); queue.push_back(v.clone()); }
            }
        }
        let mut layer_nodes: HashMap<usize, Vec<String>> = HashMap::new();
        for (u, d) in dists { layer_nodes.entry(d).or_default().push(u); }
        for (d, nodes) in layer_nodes {
            let count = nodes.len();
            for (i, uri) in nodes.into_iter().enumerate() {
                if let Some(node) = self.nodes.get_mut(&uri) {
                    let angle = (i as f32 / count as f32) * std::f32::consts::TAU;
                    let r = d as f32 * 95.0;
                    node.pos = Vec2::new(r * angle.cos() + 400.0, r * angle.sin() + 300.0);
                }
            }
        }
    }

    pub fn apply_grid_layout(&mut self) {
        if self.nodes.is_empty() { return; }
        let count = self.nodes.len();
        let cols = (count as f32).sqrt().ceil() as usize;
        let spacing = 75.0;
        let mut i = 0;
        let mut sorted_uris: Vec<_> = self.nodes.keys().cloned().collect();
        sorted_uris.sort();
        for uri in sorted_uris {
            if let Some(node) = self.nodes.get_mut(&uri) {
                let row = i / cols;
                let col = i % cols;
                node.pos = Vec2::new(col as f32 * spacing + 50.0, row as f32 * spacing + 50.0);
                i += 1;
            }
        }
    }

    pub fn apply_circular_layout(&mut self) {
        if self.nodes.is_empty() { return; }
        let count = self.nodes.len();
        let r = (count as f32 * 10.0).max(140.0);
        let mut sorted_uris: Vec<_> = self.nodes.keys().cloned().collect();
        sorted_uris.sort();
        for (i, uri) in sorted_uris.into_iter().enumerate() {
            if let Some(node) = self.nodes.get_mut(&uri) {
                let angle = (i as f32 / count as f32) * std::f32::consts::TAU;
                node.pos = Vec2::new(r * angle.cos() + 400.0, r * angle.sin() + 300.0);
            }
        }
    }

    pub fn apply_concentric_layout(&mut self) {
        if self.nodes.is_empty() { return; }
        let mut classes = Vec::new();
        let mut properties = Vec::new();
        let mut individuals = Vec::new();
        let mut literals = Vec::new();
        for (uri, node) in &self.nodes {
            match node.node_type {
                NodeType::Class => classes.push(uri.clone()),
                NodeType::Property => properties.push(uri.clone()),
                NodeType::Individual => individuals.push(uri.clone()),
                NodeType::Literal => literals.push(uri.clone()),
                NodeType::Blank => {}
            }
        }
        let groups = vec![(classes, 65.0), (properties, 150.0), (individuals, 250.0), (literals, 350.0)];
        for (nodes, r) in groups {
            let count = nodes.len();
            if count == 0 { continue; }
            for (i, uri) in nodes.into_iter().enumerate() {
                if let Some(node) = self.nodes.get_mut(&uri) {
                    let angle = (i as f32 / count as f32) * std::f32::consts::TAU;
                    node.pos = Vec2::new(r * angle.cos() + 400.0, r * angle.sin() + 300.0);
                }
            }
        }
    }

    pub fn resolve_collisions(&mut self, iterations: usize) {
        if self.nodes.is_empty() { return; }
        
        let mut keys: Vec<String> = Vec::with_capacity(self.nodes.len());
        let mut widths: Vec<f32> = Vec::with_capacity(self.nodes.len());
        let mut heights: Vec<f32> = Vec::with_capacity(self.nodes.len());
        
        for (key, node) in &self.nodes {
            keys.push(key.clone());
            
            // Base radius/bounds for the node shape (compact)
            let (base_w, base_h): (f32, f32) = match node.node_type {
                NodeType::Class => (13.0, 13.0),
                NodeType::Property => (16.0, 10.0),
                NodeType::Individual => (13.0, 13.0),
                NodeType::Literal => (14.0, 9.0),
                NodeType::Blank => (11.0, 11.0),
            };
            
            // Dynamic width: max of base width and label width with compact padding
            let label_len = node.label.chars().count() as f32;
            let half_w = base_w.max(label_len * 2.2) + 5.0;
            let half_h = base_h + 7.0;
            
            widths.push(half_w);
            heights.push(half_h);
        }
        
        let len = keys.len();
        let mut positions: Vec<Vec2> = keys.iter().map(|k| self.nodes[k].pos).collect();
        
        for _ in 0..iterations {
            let mut moved = false;
            
            for i in 0..len {
                for j in i+1..len {
                    let diff = positions[i] - positions[j];
                    let combined_w = widths[i] + widths[j];
                    let combined_h = heights[i] + heights[j];
                    
                    let dx = diff.x;
                    let dy = diff.y;
                    
                    // Elliptic distance ratio: if < 1.0, they overlap
                    let dx_ratio = dx / combined_w;
                    let dy_ratio = dy / combined_h;
                    let d_sq = dx_ratio * dx_ratio + dy_ratio * dy_ratio;
                    
                    if d_sq < 0.9999 {
                        moved = true;
                        let d = d_sq.sqrt();
                        
                        let push = if d > 0.0001 {
                            let push_factor = (1.0 - d) / d;
                            diff * push_factor * 0.85
                        } else {
                            // If exactly on top of each other, nudge them slightly in a deterministic direction
                            Vec2::new(1.0, 0.1) * (combined_w * 0.5)
                        };
                        
                        let is_i_dragged = self.dragging_node.as_ref() == Some(&keys[i]);
                        let is_j_dragged = self.dragging_node.as_ref() == Some(&keys[j]);
                        
                        match (is_i_dragged, is_j_dragged) {
                            (true, true) => {}
                            (true, false) => {
                                positions[j] -= push;
                            }
                            (false, true) => {
                                positions[i] += push;
                            }
                            (false, false) => {
                                positions[i] += push * 0.5;
                                positions[j] -= push * 0.5;
                            }
                        }
                    }
                }
            }
            
            if !moved {
                break;
            }
        }

        // Write back positions
        for i in 0..len {
            if let Some(node) = self.nodes.get_mut(&keys[i]) {
                node.pos = positions[i];
            }
        }
    }
}

