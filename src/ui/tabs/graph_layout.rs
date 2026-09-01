use std::collections::{HashMap, HashSet};
use eframe::egui;
use egui::Vec2;
use sophia::api::prelude::*;
use crate::ui::app::DarkstarApp;
use crate::ui::{GraphNode, GraphEdge, NodeType, LayoutMode};

impl DarkstarApp {
    pub fn sync_graph(&mut self) {
        let mut new_edges = Vec::new();
        let mut seen_nodes = HashSet::new();
        let mut node_types = HashMap::new();

        let owl_class = "i:http://www.w3.org/2002/07/owl#Class";
        let rdfs_class = "i:http://www.w3.org/2000/01/rdf-schema#Class";
        let owl_obj_prop = "i:http://www.w3.org/2002/07/owl#ObjectProperty";
        let owl_data_prop = "i:http://www.w3.org/2002/07/owl#DatatypeProperty";
        let owl_individual = "i:http://www.w3.org/2002/07/owl#NamedIndividual";

        let mut visible_in_focus = HashSet::new();
        if self.filters.focus_mode {
            let mut roots = Vec::new();
            if let Some(selected) = &self.selected_uri { roots.push(selected.clone()); }
            roots.extend(self.expanded_nodes.iter().cloned());

            let mut queue = std::collections::VecDeque::new();
            for r in roots { queue.push_back((r, 0)); }

            while let Some((u, d)) = queue.pop_front() {
                if !visible_in_focus.insert(u.clone()) { continue; }
                if d >= self.filters.expansion_depth { continue; }

                let graphs = [&self.manager.memory.asserted_graph, &self.manager.memory.inferred_graph];
                let u_term = crate::rules::engine::InferenceEngine::make_term(&u);
                for g in graphs {
                    // Outgoing edges
                    for t in g.triples_matching(Some(&u_term), sophia::api::term::matcher::Any, sophia::api::term::matcher::Any).flatten() {
                        let o = crate::rules::extract_str(&t.o());
                        queue.push_back((o, d + 1));
                    }
                    // Incoming edges
                    for t in g.triples_matching(sophia::api::term::matcher::Any, sophia::api::term::matcher::Any, Some(&u_term)).flatten() {
                        let s = crate::rules::extract_str(&t.s());
                        queue.push_back((s, d + 1));
                    }
                }
            }
        }

        // Build node types on-demand for visible nodes (or a capped sample of nodes if focus_mode is disabled)
        let nodes_for_typing: HashSet<String> = if self.filters.focus_mode {
            visible_in_focus.clone()
        } else {
            let mut sample_nodes = HashSet::new();
            let mut count = 0;
            for t in self.manager.memory.asserted_graph.triples().flatten() {
                if count >= 1000 { break; }
                sample_nodes.insert(crate::rules::extract_str(&t.s()));
                sample_nodes.insert(crate::rules::extract_str(&t.o()));
                count += 1;
            }
            sample_nodes
        };

        for uri in &nodes_for_typing {
            if uri.starts_with("_:") {
                node_types.insert(uri.clone(), NodeType::Blank);
                continue;
            }
            if uri.starts_with("l:") {
                node_types.insert(uri.clone(), NodeType::Literal);
                continue;
            }

            let uri_term = crate::rules::engine::InferenceEngine::make_term(uri);
            let rdf_type_term = crate::rules::engine::InferenceEngine::make_term("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
            let type_triples = self.manager.memory.main_graph.triples_matching(Some(&uri_term), Some(&rdf_type_term), sophia::api::term::matcher::Any);
            for t in type_triples.flatten() {
                let o = crate::rules::extract_str(&t.o());
                if o == owl_class || o == rdfs_class {
                    node_types.insert(uri.clone(), NodeType::Class);
                } else if o == owl_obj_prop || o == owl_data_prop {
                    node_types.insert(uri.clone(), NodeType::Property);
                } else if o == owl_individual {
                    node_types.insert(uri.clone(), NodeType::Individual);
                }
            }
        }
        
        let is_system = |p: &str| {
            let p_low = p.to_lowercase();
            p_low.contains("#type") || p_low.contains("#first") || p_low.contains("#rest") || 
            p_low.contains("#onproperty") || p_low.contains("#somevaluesfrom") || p_low.contains("#allvaluesfrom") ||
            p_low.contains("#unionof") || p_low.contains("#intersectionof") || p_low.contains("#oneof") ||
            p_low.contains("#inverseof") || p_low.contains("#equivalentclass") || p_low.contains("#equivalentproperty") ||
            p_low.contains("#namedindividual") || p_low.contains("#ontology") || p_low.contains("#imports") ||
            p_low.contains("/22-rdf-syntax-ns#") || p_low.contains("/owl#") || p_low.contains("/rdf-schema#") ||
            p_low.ends_with("/type") || p_low.ends_with("/first") || p_low.ends_with("/rest")
        };

        let mut seen_triples = HashSet::new();
        
        if self.filters.focus_mode {
            let graphs = [&self.manager.memory.asserted_graph, &self.manager.memory.inferred_graph];
            for &g in &graphs {
                let is_inferred = std::ptr::eq(g, &self.manager.memory.inferred_graph);
                for u in &visible_in_focus {
                    let u_term = crate::rules::engine::InferenceEngine::make_term(u);
                    for t in g.triples_matching(Some(&u_term), sophia::api::term::matcher::Any, sophia::api::term::matcher::Any).flatten() {
                        let s = u.clone();
                        let p = crate::rules::extract_str(&t.p());
                        let o = crate::rules::extract_str(&t.o());
                        let is_o_literal = t.o().lexical_form().is_some() && t.o().iri().is_none();
                        
                        if !visible_in_focus.contains(&o) { continue; }
                        
                        let key = (s.clone(), p.clone(), o.clone());
                        if seen_triples.contains(&key) { continue; }
                        seen_triples.insert(key);

                        let s_type = *node_types.get(&s).unwrap_or(&NodeType::Individual);
                        let o_type = if is_o_literal { NodeType::Literal } else { *node_types.get(&o).unwrap_or(&NodeType::Individual) };

                        if !self.filters.show_literals && o_type == NodeType::Literal { continue; }
                        if !self.filters.show_schema && (s_type == NodeType::Class || o_type == NodeType::Class || s_type == NodeType::Property) { continue; }
                        if !self.filters.show_individuals && (s_type == NodeType::Individual || o_type == NodeType::Individual) { continue; }
                        if !self.filters.show_system && is_system(&p) { continue; }

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
                let s = crate::rules::extract_str(&t.s());
                let p = crate::rules::extract_str(&t.p());
                let o = crate::rules::extract_str(&t.o());
                let is_o_literal = t.o().lexical_form().is_some() && t.o().iri().is_none();
                
                let key = (s.clone(), p.clone(), o.clone());
                if seen_triples.contains(&key) { continue; }
                seen_triples.insert(key);

                let s_type = *node_types.get(&s).unwrap_or(&NodeType::Individual);
                let o_type = if is_o_literal { NodeType::Literal } else { *node_types.get(&o).unwrap_or(&NodeType::Individual) };

                if !self.filters.show_literals && o_type == NodeType::Literal { continue; }
                if !self.filters.show_schema && (s_type == NodeType::Class || o_type == NodeType::Class || s_type == NodeType::Property) { continue; }
                if !self.filters.show_individuals && (s_type == NodeType::Individual || o_type == NodeType::Individual) { continue; }
                if !self.filters.show_system && is_system(&p) { continue; }

                seen_nodes.insert(s.clone());
                seen_nodes.insert(o.clone());
                new_edges.push(GraphEdge { from: s, to: o, label: Self::get_label(&p), is_inferred: false });
                edge_count += 1;
            }

            if self.filters.show_inferred {
                for t in self.manager.memory.inferred_graph.triples().flatten() {
                    if edge_count >= max_edges { break; }
                    if !self.manager.memory.asserted_graph.contains(t.s(), t.p(), t.o()).unwrap() {
                        let s = crate::rules::extract_str(&t.s());
                        let p = crate::rules::extract_str(&t.p());
                        let o = crate::rules::extract_str(&t.o());
                        let is_o_literal = t.o().lexical_form().is_some() && t.o().iri().is_none();

                        let key = (s.clone(), p.clone(), o.clone());
                        if seen_triples.contains(&key) { continue; }
                        seen_triples.insert(key);

                        let s_type = *node_types.get(&s).unwrap_or(&NodeType::Individual);
                        let o_type = if is_o_literal { NodeType::Literal } else { *node_types.get(&o).unwrap_or(&NodeType::Individual) };

                        if !self.filters.show_literals && o_type == NodeType::Literal { continue; }
                        if !self.filters.show_schema && (s_type == NodeType::Class || o_type == NodeType::Class || s_type == NodeType::Property) { continue; }
                        if !self.filters.show_individuals && (s_type == NodeType::Individual || o_type == NodeType::Individual) { continue; }
                        if !self.filters.show_system && is_system(&p) { continue; }

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
            if !self.nodes.contains_key(uri) {
                let angle = new_node_count as f32 * 137.5 * std::f32::consts::PI / 180.0;
                let radius = (new_node_count as f32).sqrt() * 25.0;
                self.nodes.insert(uri.clone(), GraphNode {
                    pos: Vec2::new(400.0 + angle.cos() * radius, 300.0 + angle.sin() * radius),
                    vel: Vec2::ZERO,
                    label: Self::get_label(uri),
                    node_type: *node_types.get(uri).unwrap_or({
                        if uri.starts_with("l:") { &NodeType::Literal }
                        else if uri.starts_with("_:") { &NodeType::Blank }
                        else { &NodeType::Individual }
                    }),
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

