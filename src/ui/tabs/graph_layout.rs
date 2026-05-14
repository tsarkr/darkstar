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

        let rdf_type = "i:http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        let owl_class = "i:http://www.w3.org/2002/07/owl#Class";
        let rdfs_class = "i:http://www.w3.org/2000/01/rdf-schema#Class";
        let owl_obj_prop = "i:http://www.w3.org/2002/07/owl#ObjectProperty";
        let owl_data_prop = "i:http://www.w3.org/2002/07/owl#DatatypeProperty";
        let owl_individual = "i:http://www.w3.org/2002/07/owl#NamedIndividual";

        for t in self.manager.memory.main_graph.triples().flatten() {
            let s = crate::rules::extract_str(&t.s());
            let p = crate::rules::extract_str(&t.p());
            let o = crate::rules::extract_str(&t.o());

            if s.starts_with("_:") { node_types.entry(s.clone()).or_insert(NodeType::Blank); }
            if o.starts_with("_:") { node_types.entry(o.clone()).or_insert(NodeType::Blank); }

            if p == rdf_type {
                if o == owl_class || o == rdfs_class { node_types.insert(s, NodeType::Class); }
                else if o == owl_obj_prop || o == owl_data_prop { node_types.insert(s, NodeType::Property); }
                else if o == owl_individual { node_types.insert(s, NodeType::Individual); }
            }
        }

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
                for g in graphs {
                    for t in g.triples().flatten() {
                        let s = crate::rules::extract_str(&t.s());
                        let o = crate::rules::extract_str(&t.o());
                        if s == u { queue.push_back((o, d + 1)); }
                        else if o == u { queue.push_back((s, d + 1)); }
                    }
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
        
        for t in self.manager.memory.asserted_graph.triples().flatten() {
            let s = crate::rules::extract_str(&t.s());
            let p = crate::rules::extract_str(&t.p());
            let o = crate::rules::extract_str(&t.o());
            let is_o_literal = t.o().lexical_form().is_some() && t.o().iri().is_none();
            
            let key = (s.clone(), p.clone(), o.clone());
            if seen_triples.contains(&key) { continue; }
            seen_triples.insert(key);

            if self.filters.focus_mode && (!visible_in_focus.contains(&s) || !visible_in_focus.contains(&o)) { continue; }

            let s_type = *node_types.get(&s).unwrap_or(&NodeType::Individual);
            let o_type = if is_o_literal { NodeType::Literal } else { *node_types.get(&o).unwrap_or(&NodeType::Individual) };

            if !self.filters.show_literals && o_type == NodeType::Literal { continue; }
            if !self.filters.show_schema && (s_type == NodeType::Class || o_type == NodeType::Class || s_type == NodeType::Property) { continue; }
            if !self.filters.show_individuals && (s_type == NodeType::Individual || o_type == NodeType::Individual) { continue; }
            if !self.filters.show_system && is_system(&p) { continue; }

            seen_nodes.insert(s.clone());
            seen_nodes.insert(o.clone());
            new_edges.push(GraphEdge { from: s, to: o, label: Self::get_label(&p), is_inferred: false });
        }

        if self.filters.show_inferred {
            for t in self.manager.memory.inferred_graph.triples().flatten() {
                if !self.manager.memory.asserted_graph.contains(t.s(), t.p(), t.o()).unwrap() {
                    let s = crate::rules::extract_str(&t.s());
                    let p = crate::rules::extract_str(&t.p());
                    let o = crate::rules::extract_str(&t.o());
                    let is_o_literal = t.o().lexical_form().is_some() && t.o().iri().is_none();

                    let key = (s.clone(), p.clone(), o.clone());
                    if seen_triples.contains(&key) { continue; }
                    seen_triples.insert(key);

                    if self.filters.focus_mode && (!visible_in_focus.contains(&s) || !visible_in_focus.contains(&o)) { continue; }

                    let s_type = *node_types.get(&s).unwrap_or(&NodeType::Individual);
                    let o_type = if is_o_literal { NodeType::Literal } else { *node_types.get(&o).unwrap_or(&NodeType::Individual) };

                    if !self.filters.show_literals && o_type == NodeType::Literal { continue; }
                    if !self.filters.show_schema && (s_type == NodeType::Class || o_type == NodeType::Class || s_type == NodeType::Property) { continue; }
                    if !self.filters.show_individuals && (s_type == NodeType::Individual || o_type == NodeType::Individual) { continue; }
                    if !self.filters.show_system && is_system(&p) { continue; }

                    seen_nodes.insert(s.clone());
                    seen_nodes.insert(o.clone());
                    new_edges.push(GraphEdge { from: s, to: o, label: Self::get_label(&p), is_inferred: true });
                }
            }
        }

        let mut new_node_count = 0;
        for uri in &seen_nodes {
            if !self.nodes.contains_key(uri) {
                let angle = new_node_count as f32 * 137.5 * std::f32::consts::PI / 180.0;
                let radius = (new_node_count as f32).sqrt() * 50.0;
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
            LayoutMode::Hierarchical => self.apply_hierarchical_layout(),
            LayoutMode::Radial => self.apply_radial_layout(),
            LayoutMode::Grid => self.apply_grid_layout(),
            LayoutMode::Circular => self.apply_circular_layout(),
            LayoutMode::Concentric => self.apply_concentric_layout(),
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
            if let Some(node) = self.nodes.get_mut(&uri) { node.pos = Vec2::new(*count as f32 * 180.0 + 50.0, layer as f32 * 150.0 + 50.0); }
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
                    let r = d as f32 * 220.0;
                    node.pos = Vec2::new(r * angle.cos() + 500.0, r * angle.sin() + 500.0);
                }
            }
        }
    }

    pub fn apply_grid_layout(&mut self) {
        if self.nodes.is_empty() { return; }
        let count = self.nodes.len();
        let cols = (count as f32).sqrt().ceil() as usize;
        let spacing = 180.0;
        let mut i = 0;
        let mut sorted_uris: Vec<_> = self.nodes.keys().cloned().collect();
        sorted_uris.sort();
        for uri in sorted_uris {
            if let Some(node) = self.nodes.get_mut(&uri) {
                let row = i / cols;
                let col = i % cols;
                node.pos = Vec2::new(col as f32 * spacing + 100.0, row as f32 * spacing + 100.0);
                i += 1;
            }
        }
    }

    pub fn apply_circular_layout(&mut self) {
        if self.nodes.is_empty() { return; }
        let count = self.nodes.len();
        let r = (count as f32 * 20.0).max(300.0);
        let mut sorted_uris: Vec<_> = self.nodes.keys().cloned().collect();
        sorted_uris.sort();
        for (i, uri) in sorted_uris.into_iter().enumerate() {
            if let Some(node) = self.nodes.get_mut(&uri) {
                let angle = (i as f32 / count as f32) * std::f32::consts::TAU;
                node.pos = Vec2::new(r * angle.cos() + 500.0, r * angle.sin() + 500.0);
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
        let groups = vec![(classes, 150.0), (properties, 350.0), (individuals, 600.0), (literals, 800.0)];
        for (nodes, r) in groups {
            let count = nodes.len();
            if count == 0 { continue; }
            for (i, uri) in nodes.into_iter().enumerate() {
                if let Some(node) = self.nodes.get_mut(&uri) {
                    let angle = (i as f32 / count as f32) * std::f32::consts::TAU;
                    node.pos = Vec2::new(r * angle.cos() + 500.0, r * angle.sin() + 500.0);
                }
            }
        }
    }
}
