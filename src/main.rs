mod memory;
mod rules;
pub mod core;

use eframe::egui;
use egui_extras::install_image_loaders;
use crate::core::manager::DarkstarManager;
use crate::core::settings::{AppSettings, Language};
use crate::core::l10n::L10n;
use crate::rules::engine::InferenceEngine;
use sophia::api::prelude::*;
use std::collections::{HashMap, HashSet};
use egui::Vec2;
use egui_dock::{DockState, TabViewer};

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum DarkstarTab {
    Classes,
    ObjectProperties,
    DataProperties,
    Individuals,
    Graph,
    History,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum NodeType {
    Class,
    Property,
    Individual,
    Literal,
    Blank,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum AppState {
    Splash,
    Onboarding,
    Main,
}

struct GraphNode {
    pos: Vec2,
    vel: Vec2,
    label: String,
    node_type: NodeType,
}

struct GraphEdge {
    from: String,
    to: String,
    label: String,
    is_inferred: bool,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum LayoutMode {
    ForceDirected,
    Hierarchical,
    Radial,
    Grid,
    Circular,
    Concentric,
}

struct GraphFilters {
    show_individuals: bool,
    show_schema: bool,
    show_inferred: bool,
    show_literals: bool,
    layout_mode: LayoutMode,
    focus_mode: bool,
    expansion_depth: usize,
    show_system: bool,
}

impl Default for GraphFilters {
    fn default() -> Self {
        Self {
            show_individuals: true,
            show_schema: false,
            show_inferred: true,
            show_literals: false,
            layout_mode: LayoutMode::ForceDirected,
            focus_mode: true,
            expansion_depth: 1,
            show_system: false,
        }
    }
}

struct DarkstarApp {
    manager: DarkstarManager,
    selected_uri: Option<String>,
    
    // UI State
    search_query: String,
    auto_reasoning: bool,
    status_message: Option<(String, std::time::Instant)>,
    show_settings: bool,
    show_metrics_dialog: bool,

    // Persistence
    settings: AppSettings,

    // Docking
    dock_state: DockState<DarkstarTab>,
    
    // Add Property Form State
    new_prop_predicate: String,
    new_prop_value: String,
    
    // Custom Graph State
    nodes: HashMap<String, GraphNode>,
    edges: Vec<GraphEdge>,
    graph_needs_sync: bool,
    dragging_node: Option<String>,
    filters: GraphFilters,
    expanded_nodes: HashSet<String>,
    trigger_fit: bool,
    graph_offset: Vec2,
    graph_scale: f32,

    // Refactor & Bulk State
    renaming_uri: Option<(String, String)>, // (old_uri, new_name_buffer)
    clipboard: Vec<String>,
    selected_individuals: HashSet<String>,
    confirm_delete_uri: Option<String>,
    confirm_revert_dialog: bool,
    merging_uri: Option<(String, String)>, // (Target buffer, source_uri)
    annotation_buffer: Option<(String, String)>, // (uri, buffer)
    simulation_alpha: f32,
    
    // New Onboarding/Splash state
    state: AppState,
    splash_start_time: std::time::Instant,
    onboarding_step: usize,
}

impl DarkstarApp {
    fn i18n(&self) -> L10n {
        L10n::get(self.settings.language)
    }

    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_image_loaders(&cc.egui_ctx);
        let settings = AppSettings::load();
        
        // --- Korean Font Support (Fix for Tofu boxes) ---
        let mut fonts = egui::FontDefinitions::default();
        
        // Common paths for Korean fonts on macOS and Windows
        let font_paths = [
            // Mac
            "/System/Library/Fonts/AppleSDGothicNeo.ttc",
            "/System/Library/Fonts/Supplemental/AppleGothic.ttf",
            "/Library/Fonts/NanumGothic.ttf",
            // Windows
            "C:\\Windows\\Fonts\\malgun.ttf",
            "C:\\Windows\\Fonts\\malgunsl.ttf",
            "C:\\Windows\\Fonts\\gulim.ttc",
        ];

        let mut font_loaded = false;
        for path in font_paths {
            if let Ok(font_data) = std::fs::read(path) {
                fonts.font_data.insert(
                    "korean_font".to_owned(),
                    egui::FontData::from_owned(font_data),
                );
                
                // Add to proportional and monospace families
                fonts.families.get_mut(&egui::FontFamily::Proportional)
                    .unwrap()
                    .insert(0, "korean_font".to_owned());
                
                fonts.families.get_mut(&egui::FontFamily::Monospace)
                    .unwrap()
                    .push("korean_font".to_owned());
                
                font_loaded = true;
                break;
            }
        }
        
        if font_loaded {
            cc.egui_ctx.set_fonts(fonts);
        }

        let mut dock_state = DockState::new(vec![DarkstarTab::Graph]);
        let [_left, main] = dock_state.main_surface_mut().split_left(
            egui_dock::NodeIndex::root(),
            0.2,
            vec![DarkstarTab::Classes],
        );
        let [_main, _bottom] = dock_state.main_surface_mut().split_below(
            main,
            0.75,
            vec![DarkstarTab::Individuals, DarkstarTab::ObjectProperties, DarkstarTab::DataProperties, DarkstarTab::History],
        );

        Self {
            manager: DarkstarManager::new(),
            selected_uri: None,
            settings,
            dock_state,
            search_query: String::new(),
            auto_reasoning: true,
            status_message: None,
            show_settings: false,
            show_metrics_dialog: false,
            new_prop_predicate: String::new(),
            new_prop_value: String::new(),
            nodes: HashMap::new(),
            edges: Vec::new(),
            graph_needs_sync: true,
            dragging_node: None,
            filters: GraphFilters::default(),
            expanded_nodes: HashSet::new(),
            trigger_fit: true, // Fit on first load
            graph_offset: Vec2::ZERO,
            graph_scale: 1.0,
            renaming_uri: None,
            clipboard: Vec::new(),
            selected_individuals: HashSet::new(),
            confirm_delete_uri: None,
            confirm_revert_dialog: false,
            merging_uri: None,
            annotation_buffer: None,
            simulation_alpha: 1.0,
            state: AppState::Splash,
            splash_start_time: std::time::Instant::now(),
            onboarding_step: 0,
        }
    }

    fn get_label(prefixed_uri: &str) -> String {
        let uri = if prefixed_uri.len() >= 2 && &prefixed_uri[1..2] == ":" {
            &prefixed_uri[2..]
        } else {
            prefixed_uri
        };
        uri.split('/').last().unwrap_or(uri).split('#').last().unwrap_or(uri).to_string()
    }

    fn is_object_property(&self, uri: &str) -> bool {
        let rdf_type = InferenceEngine::make_term("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
        let obj_prop = InferenceEngine::make_term("http://www.w3.org/2002/07/owl#ObjectProperty");
        let term = InferenceEngine::make_term(uri);
        self.manager.memory.main_graph.contains(&term, &rdf_type, &obj_prop).unwrap_or(false)
    }

    fn sync_graph(&mut self) {
        use sophia::api::graph::Graph as _;
        
        let mut new_edges = Vec::new();
        let mut seen_nodes = HashSet::new();
        let mut node_types = HashMap::new();

        let rdf_type = "i:http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        let owl_class = "i:http://www.w3.org/2002/07/owl#Class";
        let rdfs_class = "i:http://www.w3.org/2000/01/rdf-schema#Class";
        let owl_obj_prop = "i:http://www.w3.org/2002/07/owl#ObjectProperty";
        let owl_data_prop = "i:http://www.w3.org/2002/07/owl#DatatypeProperty";
        let owl_individual = "i:http://www.w3.org/2002/07/owl#NamedIndividual";

        // 1. Pre-identify node types
        for t in self.manager.memory.main_graph.triples().flatten() {
            let s = crate::rules::extract_str(&t.s());
            let p = crate::rules::extract_str(&t.p());
            let o = crate::rules::extract_str(&t.o());

            // Identify Blank Nodes
            if s.starts_with("_:") { node_types.entry(s.clone()).or_insert(NodeType::Blank); }
            if o.starts_with("_:") { node_types.entry(o.clone()).or_insert(NodeType::Blank); }

            if p == rdf_type {
                if o == owl_class || o == rdfs_class {
                    node_types.insert(s, NodeType::Class);
                } else if o == owl_obj_prop || o == owl_data_prop {
                    node_types.insert(s, NodeType::Property);
                } else if o == owl_individual {
                    node_types.insert(s, NodeType::Individual);
                }
            }
        }

        // 2. Identify "Visible" nodes based on Focus Mode
        let mut visible_in_focus = HashSet::new();
        if self.filters.focus_mode {
            let mut roots = Vec::new();
            if let Some(selected) = &self.selected_uri {
                roots.push(selected.clone());
            }
            roots.extend(self.expanded_nodes.iter().cloned());

            // BFS for Depth
            let mut queue = std::collections::VecDeque::new();
            for r in roots {
                queue.push_back((r, 0));
            }

            while let Some((u, d)) = queue.pop_front() {
                if !visible_in_focus.insert(u.clone()) { continue; }
                if d >= self.filters.expansion_depth { continue; }

                // Find neighbors in both asserted and inferred
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

        // 3. Build edges with focus and schema filtering
        let mut seen_triples = std::collections::HashSet::new();
        let mut add_triple = |s: String, p: String, o: String, is_o_literal: bool, is_inferred: bool| {
            let key = (s.clone(), p.clone(), o.clone());
            if seen_triples.contains(&key) {
                return;
            }
            seen_triples.insert(key);

            if self.filters.focus_mode && (!visible_in_focus.contains(&s) || !visible_in_focus.contains(&o)) {
                return;
            }

            let s_type = *node_types.get(&s).unwrap_or(&NodeType::Individual);
            let o_type = if is_o_literal { NodeType::Literal } else { *node_types.get(&o).unwrap_or(&NodeType::Individual) };

            if !self.filters.show_literals && o_type == NodeType::Literal { return; }
            if !self.filters.show_schema && (s_type == NodeType::Class || o_type == NodeType::Class || s_type == NodeType::Property) { return; }
            if !self.filters.show_individuals && (s_type == NodeType::Individual || o_type == NodeType::Individual) { return; }

            // Filter internal bnodes if not show_system
            seen_nodes.insert(s.clone());
            seen_nodes.insert(o.clone());
            new_edges.push(GraphEdge { from: s, to: o, label: Self::get_label(&p), is_inferred });
        };

        for t in self.manager.memory.asserted_graph.triples().flatten() {
            let is_o_literal = t.o().lexical_form().is_some() && t.o().iri().is_none();
            add_triple(crate::rules::extract_str(&t.s()), crate::rules::extract_str(&t.p()), crate::rules::extract_str(&t.o()), is_o_literal, false);
        }

        if self.filters.show_inferred {
            for t in self.manager.memory.inferred_graph.triples().flatten() {
                if !self.manager.memory.asserted_graph.contains(t.s(), t.p(), t.o()).unwrap() {
                    let is_o_literal = t.o().lexical_form().is_some() && t.o().iri().is_none();
                    add_triple(crate::rules::extract_str(&t.s()), crate::rules::extract_str(&t.p()), crate::rules::extract_str(&t.o()), is_o_literal, true);
                }
            }
        }

        // 4. Update nodes
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

        if new_node_count > 0 {
            self.simulation_alpha = 10.0; // Boost physics on new nodes
        }

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

    fn apply_hierarchical_layout(&mut self) {
        let mut layers: HashMap<String, usize> = HashMap::new();
        let mut children: HashMap<String, Vec<String>> = HashMap::new();
        
        for edge in &self.edges {
            if edge.label == "subClassOf" {
                children.entry(edge.to.clone()).or_default().push(edge.from.clone());
            }
        }

        let roots: Vec<String> = self.nodes.keys()
            .filter(|k| !self.edges.iter().any(|e| &e.from == *k && e.label == "subClassOf"))
            .cloned().collect();

        let mut queue = std::collections::VecDeque::new();
        for r in roots { queue.push_back((r, 0)); }

        while let Some((node, layer)) = queue.pop_front() {
            layers.insert(node.clone(), layer);
            if let Some(subs) = children.get(&node) {
                for sub in subs {
                    queue.push_back((sub.clone(), layer + 1));
                }
            }
        }

        let mut layer_counts = HashMap::new();
        for (uri, layer) in layers {
            let count = layer_counts.entry(layer).or_insert(0);
            if let Some(node) = self.nodes.get_mut(&uri) {
                node.pos = Vec2::new(*count as f32 * 180.0 + 50.0, layer as f32 * 150.0 + 50.0);
            }
            *count += 1;
        }
    }

    fn apply_radial_layout(&mut self) {
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
                if !dists.contains_key(v) {
                    dists.insert(v.clone(), d + 1);
                    queue.push_back(v.clone());
                }
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

    fn apply_grid_layout(&mut self) {
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

    fn apply_circular_layout(&mut self) {
        if self.nodes.is_empty() { return; }
        let count = self.nodes.len();
        let r = (count as f32 * 20.0).max(300.0); // Dynamic radius, min 300
        let mut sorted_uris: Vec<_> = self.nodes.keys().cloned().collect();
        sorted_uris.sort();
        for (i, uri) in sorted_uris.into_iter().enumerate() {
            if let Some(node) = self.nodes.get_mut(&uri) {
                let angle = (i as f32 / count as f32) * std::f32::consts::TAU;
                node.pos = Vec2::new(r * angle.cos() + 500.0, r * angle.sin() + 500.0);
            }
        }
    }

    fn apply_concentric_layout(&mut self) {
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
        
        let groups = vec![
            (classes, 150.0),       // Innermost
            (properties, 350.0),
            (individuals, 600.0),
            (literals, 800.0)       // Outermost
        ];
        
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

impl eframe::App for DarkstarApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        match self.state {
            AppState::Splash => self.render_splash(ctx),
            AppState::Onboarding => self.render_onboarding(ctx),
            AppState::Main => self.render_main(ctx),
        }
    }
}

impl DarkstarApp {
    fn render_main(&mut self, ctx: &egui::Context) {
        // Apply theme from settings
        ctx.set_visuals(if self.settings.theme_dark { egui::Visuals::dark() } else { egui::Visuals::light() });

        // Handle auto-reasoning
        if self.auto_reasoning && self.manager.is_dirty {
            self.manager.run_reasoning(&self.settings.enabled_rules);
            self.graph_needs_sync = true;
        }

        // Handle Screenshots
        ctx.input(|i| {
            for event in &i.raw.events {
                if let egui::Event::Screenshot { image, .. } = event {
                    let pixels = &image.pixels;
                    let width = image.width() as u32;
                    let height = image.height() as u32;
                    
                    let mut rgba = Vec::with_capacity(pixels.len() * 4);
                    for p in pixels {
                        rgba.extend_from_slice(&p.to_array());
                    }
                    
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name("darkstar_graph.png")
                        .add_filter("PNG", &["png"])
                        .save_file() {
                        if let Err(e) = image::save_buffer(
                            path,
                            &rgba,
                            width,
                            height,
                            image::ExtendedColorType::Rgba8,
                        ) {
                            eprintln!("Failed to save screenshot: {}", e);
                        } else {
                            self.status_message = Some(("Screenshot saved".to_string(), std::time::Instant::now()));
                        }
                    }
                }
            }
        });

        // Handle Keyboard Shortcuts
        self.handle_shortcuts(ctx);

        // Top Menu Bar
        self.render_menu_bar(ctx);

        // Bottom Status Bar
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            let i = self.i18n();
            ui.horizontal(|ui| {
                ui.label(format!("{}: {}", i.individuals, self.manager.memory.main_graph.triples().count()));
                ui.separator();
                if let Some((msg, time)) = &self.status_message {
                    if time.elapsed().as_secs() < 3 {
                        ui.colored_label(egui::Color32::YELLOW, format!("✨ {}", msg));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !self.manager.memory.is_consistent {
                        ui.colored_label(egui::Color32::RED, format!("⚠ {}", i.inconsistent));
                    } else {
                        ui.colored_label(egui::Color32::GREEN, format!("✔ {}", i.consistent));
                    }
                });
            });
        });

        // Right Panel: Entity Editor
        egui::SidePanel::right("editor_panel").resizable(true).default_width(400.0).show(ctx, |ui| {
            let i = self.i18n();
            ui.heading(i.editor_title);
            ui.separator();
            if let Some(uri) = &self.selected_uri {
                self.render_entity_editor(ui, uri.clone());
            } else {
                ui.vertical_centered(|ui| {
                    ui.label(i.select_entity);
                });
            }
        });

        // Settings Modal
        if self.show_settings {
            let mut open = true;
            let mut close_clicked = false;
            let i = self.i18n();
            egui::Window::new(i.settings_title).open(&mut open).show(ctx, |ui| {
                ui.heading(i.appearance);
                if ui.checkbox(&mut self.settings.theme_dark, i.theme_dark).changed() {
                    self.settings.save();
                }
                ui.horizontal(|ui| {
                    ui.label(format!("{}:", i.ui_scale));
                    if ui.add(egui::Slider::new(&mut self.settings.ui_scale, 0.5..=2.0)).changed() {
                        self.settings.save();
                    }
                });
                
                ui.horizontal(|ui| {
                    ui.label(format!("{}:", i.language));
                    egui::ComboBox::from_id_salt("lang_selector")
                        .selected_text(match self.settings.language {
                            Language::English => "English",
                            Language::Korean => "한국어",
                        })
                        .show_ui(ui, |ui| {
                            if ui.selectable_value(&mut self.settings.language, Language::English, "English").clicked() {
                                self.settings.save();
                            }
                            if ui.selectable_value(&mut self.settings.language, Language::Korean, "한국어").clicked() {
                                self.settings.save();
                            }
                        });
                });

                ui.separator();
                ui.heading(i.rules_title);
                ui.checkbox(&mut self.settings.enabled_rules.equality, i.eq_rules);
                ui.checkbox(&mut self.settings.enabled_rules.property_chains, i.chain_rules);
                ui.checkbox(&mut self.settings.enabled_rules.class_expressions, i.exp_rules);
                ui.checkbox(&mut self.settings.enabled_rules.restrictions, i.restr_rules);
                ui.checkbox(&mut self.settings.enabled_rules.datatypes, i.dt_rules);
                ui.checkbox(&mut self.settings.enabled_rules.schema, i.scm_rules);
                
                if ui.button(i.close).clicked() {
                    close_clicked = true;
                }
            });
            if !open || close_clicked {
                self.show_settings = false;
                self.settings.save();
            }
        }

        // Confirmation Modal for Deletion
        if let Some(uri) = self.confirm_delete_uri.clone() {
            let mut open = true;
            let i = self.i18n();
            egui::Window::new(i.delete).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.label(i.confirm_delete_msg);
                ui.label(egui::RichText::new(&uri).strong());
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(egui::RichText::new(i.confirm_delete_btn).color(egui::Color32::WHITE)).fill(egui::Color32::from_rgb(150, 50, 50))).clicked() {
                        self.manager.delete_entity(&uri);
                        self.selected_uri = None;
                        self.confirm_delete_uri = None;
                        self.graph_needs_sync = true;
                        self.status_message = Some((if self.settings.language == Language::Korean { "엔티티가 삭제되었습니다" } else { "Entity deleted" }.to_string(), std::time::Instant::now()));
                    }
                    if ui.button(i.cancel).clicked() {
                        self.confirm_delete_uri = None;
                    }
                });
            });
            if !open {
                self.confirm_delete_uri = None;
            }
        }

        // Confirmation Modal for Revert
        if self.confirm_revert_dialog {
            let mut open = true;
            let i = self.i18n();
            egui::Window::new(i.revert).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.label(i.confirm_revert_msg);
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(egui::RichText::new(i.confirm_revert_btn).color(egui::Color32::WHITE)).fill(egui::Color32::from_rgb(150, 50, 50))).clicked() {
                        if let Some(path) = self.manager.active_file_path.clone() {
                            if let Err(e) = self.manager.load_from_file(&path) {
                                eprintln!("Failed to revert: {}", e);
                            } else {
                                self.graph_needs_sync = true;
                                self.status_message = Some(("Reverted to saved state".to_string(), std::time::Instant::now()));
                            }
                        }
                        self.confirm_revert_dialog = false;
                    }
                    if ui.button(i.cancel).clicked() { self.confirm_revert_dialog = false; }
                });
            });
            if !open { self.confirm_revert_dialog = false; }
        }

        // Confirmation Modal for Renaming
        let mut rename_to_apply = None;
        let mut closed_by_button = false;
        let i = self.i18n();
        if let Some((old_uri, buffer)) = &mut self.renaming_uri {
            let mut open = true;
            egui::Window::new(i.rename_entity).collapsible(false).resizable(false).open(&mut open).show(ctx, |ui| {
                ui.label(format!("Old URI: {}", old_uri));
                ui.horizontal(|ui| {
                    ui.label("New URI:");
                    ui.text_edit_singleline(buffer);
                });
                ui.horizontal(|ui| {
                    if ui.add_enabled(!buffer.trim().is_empty(), egui::Button::new(i.apply)).clicked() {
                        rename_to_apply = Some((old_uri.clone(), buffer.clone()));
                    }
                    if ui.button(i.cancel).clicked() {
                        closed_by_button = true;
                    }
                });
            });
            if !open || closed_by_button {
                self.renaming_uri = None;
            }
        }

        if let Some((old, new)) = rename_to_apply {
            self.manager.rename_entity(&old, &new);
            self.selected_uri = Some(new);
            self.renaming_uri = None;
            self.graph_needs_sync = true;
            self.status_message = Some((if self.settings.language == Language::Korean { "이름이 변경되었습니다" } else { "Entity renamed" }.to_string(), std::time::Instant::now()));
        }

        // Modal for Merging Entities
        let mut merge_to_apply = None;
        if let Some((buffer, source)) = &mut self.merging_uri {
            let mut open = true;
            let mut closed_by_btn = false;
            egui::Window::new(i.merge_entities).collapsible(false).resizable(false).open(&mut open).show(ctx, |ui| {
                ui.label(format!("Source: {}", source));
                ui.horizontal(|ui| {
                    ui.label(i.merge_entities_prompt);
                    ui.text_edit_singleline(buffer);
                });
                ui.horizontal(|ui| {
                    if ui.add_enabled(!buffer.trim().is_empty() && buffer.trim() != source, egui::Button::new(i.apply)).clicked() {
                        merge_to_apply = Some((source.clone(), buffer.clone()));
                    }
                    if ui.button(i.cancel).clicked() {
                        closed_by_btn = true;
                    }
                });
            });
            if !open || closed_by_btn {
                self.merging_uri = None;
            }
        }

        if let Some((src, target)) = merge_to_apply {
            self.manager.merge_nodes(&src, &target);
            self.selected_uri = Some(target);
            self.merging_uri = None;
            self.graph_needs_sync = true;
            self.status_message = Some((if self.settings.language == Language::Korean { "엔티티가 병합되었습니다" } else { "Entities merged" }.to_string(), std::time::Instant::now()));
        }

        // Ontology Metrics Modal
        if self.show_metrics_dialog {
            let mut open = true;
            let mut close_clicked = false;
            let i = self.i18n();
            egui::Window::new(i.ontology_metrics).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                use sophia::api::graph::Graph;
                let graph = &self.manager.memory.main_graph;
                
                // Simplified metric counts
                let mut class_count = 0;
                let mut obj_prop_count = 0;
                let mut data_prop_count = 0;
                let mut ind_count = 0;
                
                let rdf_type = "i:http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
                let owl_class = "i:http://www.w3.org/2002/07/owl#Class";
                let owl_obj_prop = "i:http://www.w3.org/2002/07/owl#ObjectProperty";
                let owl_data_prop = "i:http://www.w3.org/2002/07/owl#DatatypeProperty";
                let owl_individual = "i:http://www.w3.org/2002/07/owl#NamedIndividual";
                
                for t in graph.triples().flatten() {
                    let p = crate::rules::extract_str(&t.p());
                    let o = crate::rules::extract_str(&t.o());
                    if p == rdf_type {
                        if o == owl_class { class_count += 1; }
                        else if o == owl_obj_prop { obj_prop_count += 1; }
                        else if o == owl_data_prop { data_prop_count += 1; }
                        else if o == owl_individual { ind_count += 1; }
                    }
                }
                
                egui::Grid::new("metrics_grid").striped(true).show(ui, |ui| {
                    ui.label(i.classes); ui.label(class_count.to_string()); ui.end_row();
                    ui.label(i.obj_props); ui.label(obj_prop_count.to_string()); ui.end_row();
                    ui.label(i.data_props); ui.label(data_prop_count.to_string()); ui.end_row();
                    ui.label(i.individuals); ui.label(ind_count.to_string()); ui.end_row();
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Total Triples:");
                    ui.label(graph.triples().count().to_string());
                });
                
                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    if ui.button(i.close).clicked() {
                        close_clicked = true;
                    }
                });
            });
            if !open || close_clicked {
                self.show_metrics_dialog = false;
            }
        }

        // Central Panel: DockArea
        let mut dock_state = std::mem::replace(&mut self.dock_state, DockState::new(vec![]));
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut tab_viewer = DarkstarTabViewer {
                app: self,
            };
            egui_dock::DockArea::new(&mut dock_state)
                .style(egui_dock::Style::from_egui(ui.style()))
                .show_inside(ui, &mut tab_viewer);
        });
        self.dock_state = dock_state;
    }

    fn render_splash(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().frame(egui::Frame::none().fill(egui::Color32::from_rgb(10, 10, 15))).show(ctx, |ui| {
            let elapsed = self.splash_start_time.elapsed().as_secs_f32();
            let i = self.i18n();

            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() * 0.2);
                
                // Logo
                ui.add(egui::Image::new(egui::include_image!("../assets/logo.png")).max_width(300.0).rounding(10.0));
                
                ui.add_space(40.0);
                
                // Title
                ui.label(egui::RichText::new("DARKSTAR").size(48.0).strong().color(egui::Color32::WHITE).extra_letter_spacing(4.0));
                ui.label(egui::RichText::new("Professional Ontology Editor").size(16.0).color(egui::Color32::GRAY));
                
                ui.add_space(60.0);
                
                // Loading Messages
                let msg = if elapsed < 1.0 {
                    i.splash_loading
                } else if elapsed < 2.5 {
                    i.splash_checking
                } else {
                    "Ready"
                };
                
                ui.label(egui::RichText::new(msg).size(14.0).color(egui::Color32::from_rgb(100, 150, 255)));
                
                ui.add_space(20.0);
                
                // Simple Progress Bar
                let progress = (elapsed / 3.0).min(1.0);
                let bar_width = 200.0;
                let (rect, _) = ui.allocate_at_least(Vec2::new(bar_width, 4.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 20));
                let mut progress_rect = rect;
                progress_rect.set_width(bar_width * progress);
                ui.painter().rect_filled(progress_rect, 2.0, egui::Color32::from_rgb(0, 150, 255));
            });
            
            if elapsed > 3.0 {
                if self.settings.is_first_run {
                    self.state = AppState::Onboarding;
                } else {
                    self.state = AppState::Main;
                }
            }
            
            ctx.request_repaint();
        });
    }

    fn render_onboarding(&mut self, ctx: &egui::Context) {
        let i = self.i18n();
        
        egui::CentralPanel::default().frame(egui::Frame::none().fill(egui::Color32::from_rgb(10, 10, 20))).show(ctx, |ui| {
            let card_width = 450.0;
            let card_height = 380.0;
            
            let card_rect = egui::Rect::from_center_size(
                ui.max_rect().center(),
                Vec2::new(card_width, card_height)
            );
            
            ui.allocate_new_ui(egui::UiBuilder { max_rect: Some(card_rect), ..Default::default() }, |ui| {
                egui::Frame::window(&ui.style())
                    .fill(egui::Color32::from_rgb(25, 25, 35))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(50, 50, 70)))
                    .rounding(15.0)
                    .shadow(egui::Shadow { offset: [0.0, 10.0].into(), blur: 20.0, spread: 0.0, color: egui::Color32::from_black_alpha(100) })
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(20.0);
                            
                            // Step Indicator (Progress dots)
                            ui.horizontal(|ui| {
                                let total_steps = 3;
                                let dot_spacing = 30.0;
                                let start_x = (card_width - (total_steps as f32 - 1.0) * dot_spacing) / 2.0 - 15.0;
                                for s in 0..total_steps {
                                    let dot_pos = ui.cursor().min + Vec2::new(start_x + s as f32 * dot_spacing, 10.0);
                                    let active = self.onboarding_step == s;
                                    let color = if active { egui::Color32::from_rgb(0, 150, 255) } else { egui::Color32::from_gray(60) };
                                    ui.painter().circle_filled(dot_pos, 5.0, color);
                                }
                            });
                            ui.add_space(40.0);
                            
                            ui.heading(egui::RichText::new(i.onboarding_welcome).size(26.0).strong().color(egui::Color32::WHITE));
                            ui.add_space(20.0);
                            ui.separator();
                            ui.add_space(25.0);
                            
                            match self.onboarding_step {
                                0 => { // Language Selection
                                    ui.label(egui::RichText::new(i.onboarding_lang_desc).size(16.0).color(egui::Color32::GRAY));
                                    ui.add_space(30.0);
                                    ui.horizontal(|ui| {
                                        ui.add_space(30.0);
                                        if ui.add_sized([160.0, 45.0], egui::SelectableLabel::new(self.settings.language == Language::English, "🇺🇸 English")).clicked() {
                                            self.settings.language = Language::English;
                                        }
                                        ui.add_space(20.0);
                                        if ui.add_sized([160.0, 45.0], egui::SelectableLabel::new(self.settings.language == Language::Korean, "🇰🇷 한국어")).clicked() {
                                            self.settings.language = Language::Korean;
                                        }
                                    });
                                }
                                1 => { // UI Preferences
                                    ui.label(egui::RichText::new(if self.settings.language == Language::Korean { "✨ 인터페이스 환경을 설정하세요" } else { "✨ Setup your interface" }).size(16.0).color(egui::Color32::GRAY));
                                    ui.add_space(25.0);
                                    
                                    ui.scope(|ui| {
                                        ui.spacing_mut().item_spacing.y = 15.0;
                                        ui.horizontal(|ui| {
                                            ui.label(if self.settings.language == Language::Korean { "화면 테마:" } else { "Theme:" });
                                            ui.selectable_value(&mut self.settings.theme_dark, true, "🌙 Dark");
                                            ui.selectable_value(&mut self.settings.theme_dark, false, "☀️ Light");
                                        });
                                        ui.horizontal(|ui| {
                                            ui.label(if self.settings.language == Language::Korean { "UI 크기 조절:" } else { "UI Scale:" });
                                            ui.add(egui::Slider::new(&mut self.settings.ui_scale, 0.8..=1.5).smart_aim(false));
                                        });
                                    });
                                }
                                _ => { // Completion
                                    ui.label(egui::RichText::new(i.onboarding_finish).size(18.0).color(egui::Color32::from_rgb(100, 255, 150)));
                                    ui.add_space(40.0);
                                    if ui.add_sized([220.0, 50.0], egui::Button::new(egui::RichText::new(i.onboarding_start).size(18.0).strong())).clicked() {
                                        self.settings.is_first_run = false;
                                        self.settings.save();
                                        self.state = AppState::Main;
                                    }
                                }
                            }
                            
                            // Bottom Navigation
                            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                                ui.add_space(20.0);
                                if self.onboarding_step < 2 {
                                    ui.horizontal(|ui| {
                                        ui.add_space(card_width - 130.0);
                                        if ui.add_sized([100.0, 35.0], egui::Button::new(if self.settings.language == Language::Korean { "다음 ➜" } else { "Next ➜" })).clicked() {
                                            self.onboarding_step += 1;
                                        }
                                    });
                                }
                            });
                        });
                    });
            });
        });
    }
}

impl DarkstarApp {
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::N))) {
            self.manager = DarkstarManager::new();
            self.selected_uri = None;
            self.graph_needs_sync = true;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::O))) {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Ontology", &["ttl", "nt", "owl"])
                .pick_file() {
                if let Err(e) = self.manager.load_from_file(&path) {
                    eprintln!("Failed to load: {}", e);
                } else {
                    self.settings.add_recent(path);
                    self.graph_needs_sync = true;
                }
            }
        }
        if ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S))) {
            if let Some(path) = &self.manager.active_file_path {
                if let Ok(_) = self.manager.save_to_file(path, crate::core::io::OntologyFormat::Turtle, false) {
                    self.status_message = Some((format!("Saved to {}", path.display()), std::time::Instant::now()));
                }
            }
        }
        if ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::S))) {
            if let Some(path) = rfd::FileDialog::new().add_filter("Turtle", &["ttl"]).save_file() {
                if let Ok(_) = self.manager.save_to_file(&path, crate::core::io::OntologyFormat::Turtle, false) {
                    self.settings.add_recent(path.clone());
                    self.status_message = Some((format!("Exported to {}", path.display()), std::time::Instant::now()));
                }
            }
        }
        if ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Z))) {
            if !self.manager.history.is_undo_empty() {
                self.manager.undo();
                self.graph_needs_sync = true;
            }
        }
        if ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::Z))) {
            if !self.manager.history.is_redo_empty() {
                self.manager.redo();
                self.graph_needs_sync = true;
            }
        }
        if ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::C))) {
            if let Some(uri) = &self.selected_uri {
                self.clipboard = vec![uri.clone()];
                ctx.output_mut(|o| o.copied_text = uri.clone());
            }
        }
        if ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::R))) {
            self.manager.run_reasoning(&self.settings.enabled_rules);
            self.graph_needs_sync = true;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Plus))) || 
           ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Equals))) {
            self.graph_scale = (self.graph_scale * 1.2).clamp(0.05, 10.0);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Minus))) {
            self.graph_scale = (self.graph_scale / 1.2).clamp(0.05, 10.0);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Num0))) {
            self.graph_scale = 1.0;
        }
    }

    fn render_menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                let i = self.i18n();
                
                // File Menu
                ui.menu_button(i.file, |ui| {
                    if ui.add(egui::Button::new(i.new).shortcut_text("Cmd+N")).clicked() {
                        self.manager = DarkstarManager::new();
                        self.selected_uri = None;
                        self.graph_needs_sync = true;
                        ui.close_menu();
                    }
                    if ui.add(egui::Button::new(i.open).shortcut_text("Cmd+O")).clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Ontology", &["ttl", "nt", "owl"])
                            .pick_file() {
                            if let Err(e) = self.manager.load_from_file(&path) {
                                eprintln!("Failed to load: {}", e);
                            } else {
                                self.settings.add_recent(path);
                                self.graph_needs_sync = true;
                            }
                        }
                        ui.close_menu();
                    }
                    
                    ui.menu_button(i.open_recent, |ui| {
                        let recent = self.settings.recent_files.clone();
                        for path in recent {
                            if ui.button(path.display().to_string()).clicked() {
                                if let Err(e) = self.manager.load_from_file(&path) {
                                    eprintln!("Failed to load recent: {}", e);
                                } else {
                                    self.settings.add_recent(path);
                                    self.graph_needs_sync = true;
                                }
                                ui.close_menu();
                            }
                        }
                    });

                    if ui.button(i.merge).clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Ontology", &["ttl", "nt", "owl"])
                            .pick_file() {
                            if let Err(e) = self.manager.merge_ontology(&path, &self.settings.enabled_rules) {
                                self.status_message = Some((format!("Merge failed: {}", e), std::time::Instant::now()));
                            } else {
                                self.status_message = Some(("Ontologies merged".to_string(), std::time::Instant::now()));
                                self.graph_needs_sync = true;
                            }
                        }
                        ui.close_menu();
                    }
                    
                    ui.separator();
                    if ui.add_enabled(self.manager.active_file_path.is_some(), egui::Button::new(i.revert)).clicked() {
                        self.confirm_revert_dialog = true;
                        ui.close_menu();
                    }
                    if ui.add(egui::Button::new(i.save).shortcut_text("Cmd+S")).clicked() {
                        if let Some(path) = &self.manager.active_file_path {
                            if let Ok(_) = self.manager.save_to_file(path, crate::core::io::OntologyFormat::Turtle, false) {
                                self.status_message = Some((format!("Saved to {}", path.display()), std::time::Instant::now()));
                            }
                        }
                        ui.close_menu();
                    }
                    if ui.add(egui::Button::new(i.save_as).shortcut_text("Cmd+Shift+S")).clicked() {
                        if let Some(path) = rfd::FileDialog::new().add_filter("Turtle", &["ttl"]).save_file() {
                            if let Ok(_) = self.manager.save_to_file(&path, crate::core::io::OntologyFormat::Turtle, false) {
                                self.settings.add_recent(path.clone());
                                self.status_message = Some((format!("Exported to {}", path.display()), std::time::Instant::now()));
                            }
                        }
                        ui.close_menu();
                    }

                    if ui.button(i.export_inferred).clicked() {
                        if let Some(path) = rfd::FileDialog::new().add_filter("Turtle", &["ttl"]).save_file() {
                            if let Ok(_) = self.manager.save_to_file(&path, crate::core::io::OntologyFormat::Turtle, true) {
                                self.status_message = Some((format!("Inferred graph exported to {}", path.display()), std::time::Instant::now()));
                            }
                        }
                        ui.close_menu();
                    }

                    ui.separator();
                    if ui.button(i.preferences).clicked() {
                        self.show_settings = true;
                        ui.close_menu();
                    }
                    if ui.button(i.exit).clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                // Edit Menu
                ui.menu_button(i.edit, |ui| {
                    if ui.add_enabled(!self.manager.history.is_undo_empty(), egui::Button::new(i.undo).shortcut_text("Cmd+Z")).clicked() {
                        self.manager.undo();
                        self.graph_needs_sync = true;
                        ui.close_menu();
                    }
                    if ui.add_enabled(!self.manager.history.is_redo_empty(), egui::Button::new(i.redo).shortcut_text("Cmd+Shift+Z")).clicked() {
                        self.manager.redo();
                        self.graph_needs_sync = true;
                        ui.close_menu();
                    }
                    if ui.add_enabled(self.selected_uri.is_some(), egui::Button::new(i.delete_entity).shortcut_text("Cmd+Backspace")).clicked() {
                        self.confirm_delete_uri = self.selected_uri.clone();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.add(egui::Button::new(i.copy_entity).shortcut_text("Cmd+C")).clicked() {
                        if let Some(uri) = &self.selected_uri {
                            self.clipboard = vec![uri.clone()];
                            ctx.output_mut(|o| o.copied_text = uri.clone());
                        }
                        ui.close_menu();
                    }
                });

                // Refactor Menu
                ui.menu_button(i.refactor, |ui| {
                    if ui.add_enabled(self.selected_uri.is_some(), egui::Button::new(i.rename_entity).shortcut_text("Cmd+E")).clicked() {
                        if let Some(uri) = &self.selected_uri {
                            self.renaming_uri = Some((uri.clone(), uri.clone()));
                        }
                        ui.close_menu();
                    }
                    if ui.add_enabled(self.selected_uri.is_some(), egui::Button::new(i.merge_entities)).clicked() {
                        if let Some(uri) = &self.selected_uri {
                            self.merging_uri = Some((String::new(), uri.clone()));
                        }
                        ui.close_menu();
                    }
                });

                // View Menu
                ui.menu_button(i.view, |ui| {
                    if ui.add(egui::Button::new(i.zoom_in).shortcut_text("Cmd++")).clicked() {
                        self.graph_scale = (self.graph_scale * 1.2).clamp(0.05, 10.0);
                        ui.close_menu();
                    }
                    if ui.add(egui::Button::new(i.zoom_out).shortcut_text("Cmd+-")).clicked() {
                        self.graph_scale = (self.graph_scale / 1.2).clamp(0.05, 10.0);
                        ui.close_menu();
                    }
                    if ui.add(egui::Button::new(i.reset_zoom).shortcut_text("Cmd+0")).clicked() {
                        self.graph_scale = 1.0;
                        ui.close_menu();
                    }
                    if ui.button(i.fit_screen).clicked() {
                        self.trigger_fit = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(if self.settings.theme_dark { "☀ Light Mode" } else { "🌙 Dark Mode" }).clicked() {
                        self.settings.theme_dark = !self.settings.theme_dark;
                        self.settings.save();
                        ui.close_menu();
                    }
                });

                // Reasoning Menu
                ui.menu_button(i.reasoning, |ui| {
                    if ui.add(egui::Button::new(i.run_reasoning).shortcut_text("Cmd+R")).clicked() {
                        self.manager.run_reasoning(&self.settings.enabled_rules);
                        self.graph_needs_sync = true;
                        ui.close_menu();
                    }
                    ui.checkbox(&mut self.auto_reasoning, i.auto_reasoning);
                });


                // Tools Menu
                ui.menu_button(i.tools, |ui| {
                    if ui.button(i.ontology_metrics).clicked() {
                        self.show_metrics_dialog = true;
                        ui.close_menu();
                    }
                });

                // Window Menu
                ui.menu_button(i.window, |ui| {
                    if ui.button(i.reset_layout).clicked() {
                        let mut new_dock = egui_dock::DockState::new(vec![DarkstarTab::Graph]);
                        let [_left, main] = new_dock.main_surface_mut().split_left(egui_dock::NodeIndex::root(), 0.2, vec![DarkstarTab::Classes]);
                        let [_main, _bottom] = new_dock.main_surface_mut().split_below(main, 0.75, vec![DarkstarTab::Individuals, DarkstarTab::ObjectProperties, DarkstarTab::DataProperties, DarkstarTab::History]);
                        self.dock_state = new_dock;
                        ui.close_menu();
                    }
                });

                // Help Menu
                ui.menu_button(i.help, |ui| {
                    if ui.button(i.about).clicked() {
                        self.status_message = Some(("Darkstar Ontology Editor v1.0".to_string(), std::time::Instant::now()));
                        ui.close_menu();
                    }
                });

                // Global Search on the right side
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    let response = ui.add(egui::TextEdit::singleline(&mut self.search_query).desired_width(150.0).hint_text(i.search));
                    if response.changed() {
                        // Optional real-time search logic could go here
                    }
                });
            });
        });
    }

    fn render_custom_graph(&mut self, ui: &mut egui::Ui) {
        let (response, painter) = ui.allocate_painter(ui.available_size(), egui::Sense::drag().union(egui::Sense::click()));
        let rect = response.rect;

        // 1. Zoom and Pan Handling
        if response.hovered() {
            let zoom_delta = ui.input(|i| i.smooth_scroll_delta.y);
            if zoom_delta != 0.0 {
                let zoom_factor = (zoom_delta * 0.002).exp();
                let old_scale = self.graph_scale;
                self.graph_scale = (self.graph_scale * zoom_factor).clamp(0.05, 10.0);
                
                // Zoom towards mouse pointer
                if let Some(mouse_pos) = ui.input(|i| i.pointer.hover_pos()) {
                    let local_mouse = mouse_pos - rect.min;
                    let graph_mouse = (local_mouse - self.graph_offset) / old_scale;
                    self.graph_offset = local_mouse - graph_mouse * self.graph_scale;
                }
            }
        }

        if response.dragged() && self.dragging_node.is_none() {
            self.graph_offset += response.drag_delta();
        }

        // Helper for coordinate conversion
        let to_screen_pos = |p: Vec2, offset: Vec2, scale: f32| -> egui::Pos2 {
            rect.min + offset + p * scale
        };

        // 2. Auto-Fit Logic
        if !self.nodes.is_empty() && (self.trigger_fit || self.graph_needs_sync) {
            let mut min = Vec2::new(f32::MAX, f32::MAX);
            let mut max = Vec2::new(f32::MIN, f32::MIN);
            for node in self.nodes.values() {
                min.x = min.x.min(node.pos.x);
                min.y = min.y.min(node.pos.y);
                max.x = max.x.max(node.pos.x);
                max.y = max.y.max(node.pos.y);
            }
            
            let size = max - min;
            let center = (min + max) * 0.5;
            
            if size.x > 0.0 && size.y > 0.0 {
                let padding = 100.0;
                let scale_x = (rect.width() - padding) / size.x;
                let scale_y = (rect.height() - padding) / size.y;
                self.graph_scale = scale_x.min(scale_y).clamp(0.1, 2.0);
                self.graph_offset = (rect.size() * 0.5) - (center * self.graph_scale);
            }
            self.trigger_fit = false;
            // Note: graph_needs_sync will be set to false at the end of sync_graph, 
            // but we use it here to trigger fit on every fresh sync.
        }

        // 3. Physics Update (Force-Directed)
        if self.filters.layout_mode == LayoutMode::ForceDirected {
            let mut forces: HashMap<String, Vec2> = HashMap::new();
            let keys: Vec<String> = self.nodes.keys().cloned().collect();

            // Repulsion
            for i in 0..keys.len() {
                for j in i+1..keys.len() {
                    let u = &keys[i];
                    let v = &keys[j];
                    let diff = self.nodes[u].pos - self.nodes[v].pos;
                    let dist_sq = diff.length_sq().max(10.0);
                    let force = diff.normalized() * (25000.0 / dist_sq) * self.simulation_alpha.max(1.0);
                    *forces.entry(u.clone()).or_default() += force;
                    *forces.entry(v.clone()).or_default() -= force;
                }
            }

            // Springs
            for edge in &self.edges {
                if let (Some(n1), Some(n2)) = (self.nodes.get(&edge.from), self.nodes.get(&edge.to)) {
                    let diff = n1.pos - n2.pos;
                    let dist = diff.length().max(1.0);
                    let force = diff.normalized() * (dist - 150.0) * -0.15 * self.simulation_alpha.max(1.0).sqrt();
                    *forces.entry(edge.from.clone()).or_default() += force;
                    *forces.entry(edge.to.clone()).or_default() -= force;
                }
            }

            // Apply forces
            for (uri, node) in &mut self.nodes {
                node.vel += *forces.get(uri).unwrap_or(&Vec2::ZERO);
                node.vel *= 0.7; // Damping
                if self.dragging_node.as_ref() != Some(uri) {
                    node.pos += node.vel * 0.1;
                }
            }

            // Decay alpha
            if self.simulation_alpha > 1.0 {
                self.simulation_alpha *= 0.95;
                ui.ctx().request_repaint();
            } else {
                self.simulation_alpha = 1.0;
            }
            
            // Continuous repaint if still moving significantly
            let total_vel: f32 = self.nodes.values().map(|n| n.vel.length()).sum();
            if total_vel > 0.1 {
                ui.ctx().request_repaint();
            }
        }

        // 4. Interaction (Node Drag/Select)
        if let Some(pointer_pos) = response.interact_pointer_pos() {
            let graph_pointer = (pointer_pos - rect.min - self.graph_offset) / self.graph_scale;
            
            if response.drag_started() {
                for (uri, node) in &self.nodes {
                    if (node.pos - graph_pointer).length() < (30.0 / self.graph_scale) {
                        self.dragging_node = Some(uri.clone());
                        break;
                    }
                }
            }

            if response.clicked() {
                for (uri, node) in &self.nodes {
                    if (node.pos - graph_pointer).length() < (30.0 / self.graph_scale) {
                        self.selected_uri = Some(uri.clone());
                        self.graph_needs_sync = true;
                        break;
                    }
                }
            }

            if response.double_clicked() {
                for (uri, node) in &self.nodes {
                    if (node.pos - graph_pointer).length() < (30.0 / self.graph_scale) {
                        if self.expanded_nodes.insert(uri.clone()) {
                            self.graph_needs_sync = true;
                        }
                        break;
                    }
                }
            }
        }

        if response.drag_stopped() {
            self.dragging_node = None;
        }

        if let Some(uri) = &self.dragging_node {
            if let Some(pointer_pos) = response.interact_pointer_pos() {
                let graph_pointer = (pointer_pos - rect.min - self.graph_offset) / self.graph_scale;
                if let Some(node) = self.nodes.get_mut(uri) {
                    node.pos = graph_pointer;
                    node.vel = Vec2::ZERO;
                }
            }
        }

        // 5. Draw Edges
        let mut edge_groups: HashMap<(String, String), Vec<&GraphEdge>> = HashMap::new();
        for edge in &self.edges {
            let key = if edge.from < edge.to { (edge.from.clone(), edge.to.clone()) } else { (edge.to.clone(), edge.from.clone()) };
            edge_groups.entry(key).or_default().push(edge);
        }

        for group in edge_groups.values() {
            for (i, edge) in group.iter().enumerate() {
                if let (Some(n1), Some(n2)) = (self.nodes.get(&edge.from), self.nodes.get(&edge.to)) {
                    let p1 = to_screen_pos(n1.pos, self.graph_offset, self.graph_scale);
                    let p2 = to_screen_pos(n2.pos, self.graph_offset, self.graph_scale);
                    
                    let is_system = edge.label == "first" || edge.label == "rest" || edge.label == "type" || edge.label == "nil";
                    let alpha = if is_system && !self.filters.show_system { 60 } else { 255 };
                    
                    let base_color = if edge.is_inferred { egui::Color32::from_rgb(100, 180, 255) } else { egui::Color32::GRAY };
                    let color = base_color.linear_multiply(alpha as f32 / 255.0);
                    
                    let stroke_width = if is_system { 1.0 } else { 1.5 };
                    let stroke = egui::Stroke::new(stroke_width * self.graph_scale.sqrt(), color);
                    
                    let dir = (p2 - p1).normalized();
                    
                    // Offset p2 to node boundary
                    let is_bnode = edge.to.starts_with("_:");
                    let target_radius = if is_bnode && !self.filters.show_system {
                        4.0
                    } else {
                        match n2.node_type {
                            NodeType::Class | NodeType::Individual => 20.0,
                            NodeType::Property => 30.0,
                            NodeType::Literal => 25.0,
                            NodeType::Blank => 4.0,
                        }
                    } * self.graph_scale;
                    
                    let p2_boundary = p2 - dir * target_radius;
                    painter.line_segment([p1, p2_boundary], stroke);
                    
                    // Arrow at boundary
                    let head_size = if is_system { 6.0 } else { 10.0 } * self.graph_scale;
                    let head = p2_boundary - dir * head_size;
                    painter.line_segment([p2_boundary, head + dir.rot90() * (head_size * 0.6)], stroke);
                    painter.line_segment([p2_boundary, head - dir.rot90() * (head_size * 0.6)], stroke);
                    
                    if self.graph_scale > 0.4 && alpha > 100 {
                        let mid = p1 + (p2 - p1) * 0.5;
                        let offset_dist = (i as f32 - (group.len() as f32 - 1.0) / 2.0) * 16.0 * self.graph_scale;
                        let label_pos = mid + dir.rot90() * offset_dist;
                        
                        let font_id = egui::FontId::proportional(10.0 * self.graph_scale);
                        let galley = ui.painter().layout_no_wrap(edge.label.clone(), font_id, color);
                        let rect = galley.rect.expand(2.0).translate(label_pos - galley.rect.center());
                        
                        let bg_color = if self.settings.theme_dark { 
                            egui::Color32::from_black_alpha(alpha.min(160)) 
                        } else { 
                            egui::Color32::from_white_alpha(alpha.min(180)) 
                        };
                        let text_color = if self.settings.theme_dark { egui::Color32::LIGHT_GRAY } else { egui::Color32::DARK_GRAY };
                        
                        painter.rect_filled(rect, 2.0, bg_color);
                        painter.galley(rect.min, galley, text_color);
                    }
                }
            }
        }

        // 6. Draw Nodes
        for (uri, node) in &self.nodes {
            let pos = to_screen_pos(node.pos, self.graph_offset, self.graph_scale);
            let is_selected = self.selected_uri.as_deref() == Some(uri);
            let is_bnode = uri.starts_with("_:");
            let is_internal = is_bnode && !self.filters.show_system;
            
            let stroke_width = if is_selected { 3.0 } else { 1.0 };
            let stroke_color = if is_selected { egui::Color32::WHITE } else if is_internal { egui::Color32::from_gray(80) } else { egui::Color32::BLACK };
            let stroke = egui::Stroke::new(stroke_width * self.graph_scale, stroke_color);
            
            let radius = if is_internal { 4.0 } else { 20.0 } * self.graph_scale;
            let alpha = if is_internal { 100 } else { 255 };

            match node.node_type {
                NodeType::Class => {
                    let color = egui::Color32::from_rgb(249, 232, 88).linear_multiply(alpha as f32 / 255.0);
                    painter.circle(pos, radius, color, stroke);
                }
                NodeType::Property => {
                    let color = egui::Color32::from_rgb(170, 204, 255).linear_multiply(alpha as f32 / 255.0);
                    let rect_node = egui::Rect::from_center_size(pos, Vec2::new(radius * 3.0, radius * 1.5));
                    painter.rect(rect_node, 5.0 * self.graph_scale, color, stroke);
                }
                NodeType::Individual => {
                    let color = egui::Color32::from_rgb(194, 174, 255).linear_multiply(alpha as f32 / 255.0);
                    let p1 = pos + Vec2::new(0.0, -radius);
                    let p2 = pos + Vec2::new(radius, 0.0);
                    let p3 = pos + Vec2::new(0.0, radius);
                    let p4 = pos + Vec2::new(-radius, 0.0);
                    painter.add(egui::Shape::convex_polygon(vec![p1, p2, p3, p4], color, stroke));
                }
                NodeType::Literal => {
                    let color = if self.settings.theme_dark { egui::Color32::WHITE } else { egui::Color32::from_rgb(240, 240, 240) }.linear_multiply(alpha as f32 / 255.0);
                    let rect_node = egui::Rect::from_center_size(pos, Vec2::new(radius * 2.5, radius * 1.2));
                    painter.rect(rect_node, 0.0, color, stroke);
                }
                NodeType::Blank => {
                    painter.circle(pos, radius, egui::Color32::GRAY, stroke);
                }
            }

            if self.graph_scale > 0.5 && !is_internal {
                let label_color = if self.settings.theme_dark { egui::Color32::WHITE } else { egui::Color32::BLACK };
                painter.text(pos + Vec2::new(0.0, radius + 8.0 * self.graph_scale), egui::Align2::CENTER_TOP, &node.label, egui::FontId::proportional(12.0 * self.graph_scale), label_color);
            }
        }
    }

    fn render_class_hierarchy(&mut self, ui: &mut egui::Ui) {
        use sophia::api::term::matcher::Any;
        let rdfs_subclass_of = InferenceEngine::make_term("http://www.w3.org/2000/01/rdf-schema#subClassOf");
        let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut all_classes = HashSet::new();
        let mut has_parent = HashSet::new();

        let triples = self.manager.memory.main_graph.triples_matching(
            Any,
            Some(&rdfs_subclass_of),
            Any
        );

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

    fn render_property_hierarchy(&mut self, ui: &mut egui::Ui, filter_object: bool) {
        use sophia::api::term::matcher::Any;
        let rdfs_subprop_of = InferenceEngine::make_term("http://www.w3.org/2000/01/rdf-schema#subPropertyOf");
        let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut all_props = HashSet::new();
        let mut has_parent = HashSet::new();

        let type_to_match = if filter_object {
            "http://www.w3.org/2002/07/owl#ObjectProperty"
        } else {
            "http://www.w3.org/2002/07/owl#DatatypeProperty"
        };
        let target_type = InferenceEngine::make_term(type_to_match);
        let rdf_type = InferenceEngine::make_term("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");

        let type_triples = self.manager.memory.main_graph.triples_matching(Any, Some(&rdf_type), Some(&target_type));
        for t in type_triples.flatten() {
            all_props.insert(crate::rules::extract_str(&t.s()));
        }

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

        for root in roots {
            self.render_hierarchy_node(ui, &root, &children_map);
        }
    }

    fn render_hierarchy_node(&mut self, ui: &mut egui::Ui, uri: &str, children_map: &HashMap<String, Vec<String>>) {
        let label = Self::get_label(uri);
        let children = children_map.get(uri);
        
        let is_selected = self.selected_uri.as_deref() == Some(uri);
        
        if let Some(children) = children {
            let mut collapsing = egui::CollapsingHeader::new(label);
            if is_selected {
                collapsing = collapsing.default_open(true);
            }
            
            collapsing.show(ui, |ui| {
                if ui.selectable_label(is_selected, " (self)").clicked() {
                    self.selected_uri = Some(uri.to_string());
                    self.graph_needs_sync = true;
                }
                let mut sorted_children = children.clone();
                sorted_children.sort();
                for child in sorted_children {
                    self.render_hierarchy_node(ui, &child, children_map);
                }
            });
        } else {
            if ui.selectable_label(is_selected, label).clicked() {
                self.selected_uri = Some(uri.to_string());
                self.graph_needs_sync = true;
            }
        }
    }

    fn render_individuals_list(&mut self, ui: &mut egui::Ui) {
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
            if meta_classes.contains(o.as_str()) {
                known_meta.insert(s);
            } else {
                candidates.push((s, o));
            }
        }

        for (s, o) in candidates {
            if !known_meta.contains(&s) {
                individuals_by_type.entry(o).or_default().push(s);
            }
        }

        let mut types: Vec<_> = individuals_by_type.keys().cloned().collect();
        types.sort();

        let i = self.i18n();
        for type_uri in types {
            egui::CollapsingHeader::new(format!("{} {}", i.type_label, Self::get_label(&type_uri)))
                .default_open(true)
                .show(ui, |ui| {
                    let mut inds = individuals_by_type.get(&type_uri).unwrap().clone();
                    inds.sort();
                    for ind in inds {
                        let is_selected = self.selected_uri.as_deref() == Some(&ind);
                        ui.horizontal(|ui| {
                            let mut is_checked = self.selected_individuals.contains(&ind);
                            if ui.checkbox(&mut is_checked, "").changed() {
                                if is_checked {
                                    self.selected_individuals.insert(ind.clone());
                                } else {
                                    self.selected_individuals.remove(&ind);
                                }
                            }
                            if ui.selectable_label(is_selected, Self::get_label(&ind)).clicked() {
                                self.selected_uri = Some(ind);
                                self.graph_needs_sync = true;
                            }
                        });
                    }
                });
        }
    }

    fn render_classes_tab(&mut self, ui: &mut egui::Ui) {
        let i = self.i18n();
        ui.horizontal(|ui| {
            ui.heading(i.class_hierarchy);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(i.add_sibling).clicked() {
                    let new_uri = format!("http://example.org/NewClass_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
                    self.manager.add_assertion(new_uri.clone(), "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(), "http://www.w3.org/2002/07/owl#Class".to_string());
                    
                    if let Some(sibling) = self.selected_uri.clone() {
                        use sophia::api::term::matcher::Any;
                        let sub_class_of = InferenceEngine::make_term("http://www.w3.org/2000/01/rdf-schema#subClassOf");
                        let sibling_term = InferenceEngine::make_term(&sibling);
                        let mut parents = Vec::new();
                        for t in self.manager.memory.main_graph.triples_matching(Some(&sibling_term), Some(&sub_class_of), Any) {
                            if let Ok(t) = t {
                                parents.push(crate::rules::extract_str(&t.o()));
                            }
                        }
                        for parent in parents {
                            self.manager.add_assertion(new_uri.clone(), "http://www.w3.org/2000/01/rdf-schema#subClassOf".to_string(), parent);
                        }
                    }
                    self.selected_uri = Some(new_uri.clone());
                    self.renaming_uri = Some((new_uri.clone(), new_uri));
                    self.graph_needs_sync = true;
                }
                if ui.button(i.add_subclass).clicked() {
                    let new_uri = format!("http://example.org/NewClass_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
                    self.manager.add_assertion(new_uri.clone(), "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(), "http://www.w3.org/2002/07/owl#Class".to_string());
                    if let Some(parent) = self.selected_uri.clone() {
                        self.manager.add_assertion(new_uri.clone(), "http://www.w3.org/2000/01/rdf-schema#subClassOf".to_string(), parent);
                    }
                    self.selected_uri = Some(new_uri.clone());
                    self.renaming_uri = Some((new_uri.clone(), new_uri));
                    self.graph_needs_sync = true;
                }
            });
        });
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            self.render_class_hierarchy(ui);
        });
    }

    fn render_properties_tab(&mut self, ui: &mut egui::Ui, is_object_property: bool) {
        let i = self.i18n();
        ui.horizontal(|ui| {
            ui.heading(if is_object_property { i.obj_props } else { i.data_props });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(if is_object_property { i.add_sibling_property } else { i.add_sibling_property }).clicked() {
                     let new_uri = format!("http://example.org/NewProperty_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
                     self.manager.add_assertion(new_uri.clone(), "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(), (if is_object_property { "http://www.w3.org/2002/07/owl#ObjectProperty" } else { "http://www.w3.org/2002/07/owl#DatatypeProperty" }).to_string());
                     
                     if let Some(sibling) = self.selected_uri.clone() {
                         use sophia::api::term::matcher::Any;
                         let sub_prop_of = InferenceEngine::make_term("http://www.w3.org/2000/01/rdf-schema#subPropertyOf");
                         let sibling_term = InferenceEngine::make_term(&sibling);
                         let mut parents = Vec::new();
                         for t in self.manager.memory.main_graph.triples_matching(Some(&sibling_term), Some(&sub_prop_of), Any) {
                             if let Ok(t) = t {
                                 parents.push(crate::rules::extract_str(&t.o()));
                             }
                         }
                         for parent in parents {
                             self.manager.add_assertion(new_uri.clone(), "http://www.w3.org/2000/01/rdf-schema#subPropertyOf".to_string(), parent);
                         }
                     }
                     self.selected_uri = Some(new_uri.clone());
                     self.renaming_uri = Some((new_uri.clone(), new_uri));
                     self.graph_needs_sync = true;
                }
                if ui.button(if is_object_property { i.add_subproperty } else { i.add_subproperty }).clicked() {
                    let new_uri = format!("http://example.org/NewProperty_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
                    self.manager.add_assertion(new_uri.clone(), "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(), (if is_object_property { "http://www.w3.org/2002/07/owl#ObjectProperty" } else { "http://www.w3.org/2002/07/owl#DatatypeProperty" }).to_string());
                    if let Some(parent) = self.selected_uri.clone() {
                        self.manager.add_assertion(new_uri.clone(), "http://www.w3.org/2000/01/rdf-schema#subPropertyOf".to_string(), parent);
                    }
                    self.selected_uri = Some(new_uri.clone());
                    self.renaming_uri = Some((new_uri.clone(), new_uri));
                    self.graph_needs_sync = true;
                }
            });
        });
        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            self.render_property_hierarchy(ui, is_object_property);
        });
    }

    fn render_individuals_tab(&mut self, ui: &mut egui::Ui) {
        let i = self.i18n();
        ui.horizontal(|ui| {
            ui.heading(i.individuals);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_enabled(!self.selected_individuals.is_empty(), egui::Button::new(i.delete_selected)).clicked() {
                    // Bulk delete logic
                    let mut changes = Vec::new();
                    for uri in &self.selected_individuals {
                        let subject_term = InferenceEngine::make_term(uri);
                        let triples: Vec<_> = self.manager.memory.asserted_graph.triples_matching(Some(&subject_term), sophia::api::term::matcher::Any, sophia::api::term::matcher::Any).flatten().collect();
                        for t in triples {
                            changes.push(crate::core::history::DarkstarEvent::AxiomRemoved {
                                s: crate::rules::extract_str(&t.s()),
                                p: crate::rules::extract_str(&t.p()),
                                o: crate::rules::extract_str(&t.o()),
                            });
                        }
                    }
                    if !changes.is_empty() {
                        self.manager.history.push_change(crate::core::history::DarkstarEvent::Batch(changes.clone()));
                        for event in changes {
                            match event {
                                crate::core::history::DarkstarEvent::AxiomRemoved { s, p, o } => {
                                    let s_term = InferenceEngine::make_term(&s);
                                    let p_term = InferenceEngine::make_term(&p);
                                    let o_term = InferenceEngine::make_term(&o);
                                    self.manager.memory.asserted_graph.remove(&s_term, &p_term, &o_term).unwrap();
                                    self.manager.memory.main_graph.remove(&s_term, &p_term, &o_term).unwrap();
                                }
                                _ => {}
                            }
                        }
                        self.manager.is_dirty = true;
                        self.selected_individuals.clear();
                    }
                }
            });
        });
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            self.render_individuals_list(ui);
        });
    }

    fn render_entity_editor(&mut self, ui: &mut egui::Ui, uri: String) {
        let i = self.i18n();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(Self::get_label(&uri)).heading());
            if ui.button(format!("✎ {}", i.rename)).clicked() {
                self.renaming_uri = Some((uri.clone(), uri.clone()));
            }
        });
        ui.label(egui::RichText::new(&uri).small().weak());
        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            let i = self.i18n();
            // General Description / Annotations
            ui.collapsing(i.annotations, |ui| {
                ui.label("rdfs:comment");
                
                // Initialize buffer if needed
                if self.annotation_buffer.is_none() || self.annotation_buffer.as_ref().unwrap().0 != uri {
                    let comment = self.manager.get_comment(&uri);
                    self.annotation_buffer = Some((uri.clone(), comment));
                }

                if let Some((_, buffer)) = &mut self.annotation_buffer {
                    let response = ui.add(egui::TextEdit::multiline(buffer).desired_width(f32::INFINITY));
                    if response.changed() {
                        // We could save on every keystroke but that's what caused the focus loss.
                        // Instead, let's just keep the buffer and save on lost_focus or manually.
                    }
                    if response.lost_focus() {
                        self.manager.set_comment(&uri, buffer);
                        self.graph_needs_sync = true;
                    }
                }
            });

            // Property Assertions
            ui.collapsing(i.property_assertions, |ui| {
                self.render_property_assertions_editor(ui, &uri);
            });

            // Class-specific (SubClassOf / EquivalentClass)
            ui.collapsing(i.class_relations, |ui| {
                ui.label(i.subclass_of);
                let subject_term = InferenceEngine::make_term(&uri);
                let subclass_p = InferenceEngine::make_term("http://www.w3.org/2000/01/rdf-schema#subClassOf");
                
                let mut to_remove = None;
                for t in self.manager.memory.asserted_graph.triples_matching(Some(&subject_term), Some(&subclass_p), sophia::api::term::matcher::Any).flatten() {
                    let o_str = crate::rules::extract_str(&t.o());
                    ui.horizontal(|ui| {
                        ui.label(Self::get_label(&o_str));
                        if ui.button("🗑").clicked() {
                            to_remove = Some(o_str);
                        }
                    });
                }
                
                if let Some(o) = to_remove {
                    self.manager.remove_assertion(uri.clone(), "http://www.w3.org/2000/01/rdf-schema#subClassOf".to_string(), o);
                    self.graph_needs_sync = true;
                }

                if ui.button(i.add_superclass).clicked() {
                    let placeholder = "http://www.w3.org/2002/07/owl#Thing";
                    self.manager.add_assertion(uri.clone(), "http://www.w3.org/2000/01/rdf-schema#subClassOf".to_string(), placeholder.to_string());
                    self.graph_needs_sync = true;
                }
            });

            // Property Characteristics (for Object Properties)
            if self.is_object_property(&uri) {
                ui.collapsing(i.characteristics, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let chars = [
                            ("http://www.w3.org/2002/07/owl#TransitiveProperty", if self.settings.language == Language::Korean { "전이적(Transitive)" } else { "Transitive" }),
                            ("http://www.w3.org/2002/07/owl#SymmetricProperty", if self.settings.language == Language::Korean { "대칭적(Symmetric)" } else { "Symmetric" }),
                            ("http://www.w3.org/2002/07/owl#AsymmetricProperty", if self.settings.language == Language::Korean { "비대칭적(Asymmetric)" } else { "Asymmetric" }),
                            ("http://www.w3.org/2002/07/owl#ReflexiveProperty", if self.settings.language == Language::Korean { "반사적(Reflexive)" } else { "Reflexive" }),
                            ("http://www.w3.org/2002/07/owl#FunctionalProperty", if self.settings.language == Language::Korean { "기능적(Functional)" } else { "Functional" }),
                            ("http://www.w3.org/2002/07/owl#InverseFunctionalProperty", if self.settings.language == Language::Korean { "역기능적(Inverse Functional)" } else { "Inverse Functional" }),
                        ];
                        for (char_uri, label) in chars {
                            let mut val = self.manager.has_characteristic(&uri, char_uri);
                            if ui.checkbox(&mut val, label).changed() {
                                self.manager.toggle_characteristic(&uri, char_uri);
                            }
                        }
                    });
                });
            }

            ui.separator();
            if ui.add(egui::Button::new(i.delete_entity).fill(egui::Color32::from_rgb(150, 50, 50))).clicked() {
                self.confirm_delete_uri = Some(uri);
            }
        });
    }

    fn render_property_assertions_editor(&mut self, ui: &mut egui::Ui, uri: &str) {
        let subject_term = InferenceEngine::make_term(uri);
        let mut to_remove = None;
        let i = self.i18n();

        ui.label(egui::RichText::new(i.existing_assertions).strong());
        egui::Grid::new(format!("assertion_grid_{}", uri)).striped(true).num_columns(3).show(ui, |ui| {
            let triples = self.manager.memory.asserted_graph.triples_matching(Some(&subject_term), sophia::api::term::matcher::Any, sophia::api::term::matcher::Any);
            for t in triples.flatten() {
                let p_str = crate::rules::extract_str(&t.p());
                let o_str = crate::rules::extract_str(&t.o());
                
                ui.label(Self::get_label(&p_str));
                ui.label(Self::get_label(&o_str));
                if ui.button("🗑").on_hover_text(i.delete).clicked() {
                    to_remove = Some((p_str.clone(), o_str.clone()));
                }
                ui.end_row();
            }
        });

        if let Some((p, o)) = to_remove {
            self.manager.remove_assertion(uri.to_string(), p, o);
            self.status_message = Some((if self.settings.language == Language::Korean { "어설션이 제거되었습니다" } else { "Assertion removed" }.to_string(), std::time::Instant::now()));
        }

        ui.separator();
        ui.group(|ui| {
            let i = self.i18n();
            ui.label(egui::RichText::new(i.add_property).strong());
            egui::Grid::new(format!("add_prop_grid_{}", uri)).num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                ui.label(format!("{}:", i.predicate));
                ui.add(egui::TextEdit::singleline(&mut self.new_prop_predicate).desired_width(f32::INFINITY).hint_text("e.g. i:hasName"));
                ui.end_row();
                
                ui.label(format!("{}:", i.value));
                ui.add(egui::TextEdit::singleline(&mut self.new_prop_value).desired_width(f32::INFINITY).hint_text("e.g. l:John or i:Person"));
                ui.end_row();
            });
            ui.add_space(5.0);
            if ui.button(format!("✚ {}", i.add)).clicked() {
                if !self.new_prop_predicate.is_empty() && !self.new_prop_value.is_empty() {
                    self.manager.add_assertion(
                        uri.to_string(),
                        self.new_prop_predicate.clone(),
                        self.new_prop_value.clone(),
                    );
                    self.status_message = Some((if self.settings.language == Language::Korean { "어설션이 추가되었습니다" } else { "Assertion added" }.to_string(), std::time::Instant::now()));
                    self.new_prop_value.clear();
                }
            }
        });
    }
}

struct DarkstarTabViewer<'a> {
    app: &'a mut DarkstarApp,
}

impl<'a> TabViewer for DarkstarTabViewer<'a> {
    type Tab = DarkstarTab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        let i = self.app.i18n();
        match tab {
            DarkstarTab::Classes => i.classes.into(),
            DarkstarTab::ObjectProperties => i.obj_props.into(),
            DarkstarTab::DataProperties => i.data_props.into(),
            DarkstarTab::Individuals => i.individuals.into(),
            DarkstarTab::Graph => i.graph_view.into(),
            DarkstarTab::History => i.history.into(),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            DarkstarTab::Classes => self.app.render_classes_tab(ui),
            DarkstarTab::ObjectProperties => self.app.render_properties_tab(ui, true),
            DarkstarTab::DataProperties => self.app.render_properties_tab(ui, false),
            DarkstarTab::Individuals => self.app.render_individuals_tab(ui),
            DarkstarTab::Graph => {
                let i = self.app.i18n();
                if self.app.filters.focus_mode && self.app.selected_uri.is_none() {
                    ui.centered_and_justified(|ui| {
                        ui.label(egui::RichText::new(if self.app.settings.language == Language::Korean { 
                            "그래프를 탐색하려면 왼쪽 패널에서\n클래스, 속성 또는 개체를 선택하세요." 
                        } else { 
                            "Please select a Class, Property, or Individual\nfrom the left panel to explore its graph." 
                        })
                        .heading()
                        .weak());
                    });
                } else {
                    if self.app.graph_needs_sync {
                        self.app.sync_graph();
                    }
                    
                    ui.group(|ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                if egui::ComboBox::new("layout_selector", i.layout)
                                    .selected_text(format!("{:?}", self.app.filters.layout_mode))
                                    .show_ui(ui, |ui| {
                                        let mut changed = false;
                                        changed |= ui.selectable_value(&mut self.app.filters.layout_mode, LayoutMode::ForceDirected, "Force-Directed").clicked();
                                        changed |= ui.selectable_value(&mut self.app.filters.layout_mode, LayoutMode::Hierarchical, "Hierarchical").clicked();
                                        changed |= ui.selectable_value(&mut self.app.filters.layout_mode, LayoutMode::Radial, "Radial").clicked();
                                        changed |= ui.selectable_value(&mut self.app.filters.layout_mode, LayoutMode::Grid, i.layout_grid).clicked();
                                        changed |= ui.selectable_value(&mut self.app.filters.layout_mode, LayoutMode::Circular, i.layout_circular).clicked();
                                        changed |= ui.selectable_value(&mut self.app.filters.layout_mode, LayoutMode::Concentric, i.layout_concentric).clicked();
                                        changed
                                    }).inner.unwrap_or(false) {
                                    self.app.graph_needs_sync = true;
                                }
                                
                                ui.separator();
                                if ui.checkbox(&mut self.app.filters.focus_mode, i.focus_mode).changed() {
                                    self.app.graph_needs_sync = true;
                                }
                                if self.app.filters.focus_mode {
                                    ui.label(i.expansion);
                                    if ui.add(egui::Slider::new(&mut self.app.filters.expansion_depth, 1..=3)).changed() {
                                        self.app.graph_needs_sync = true;
                                    }
                                    if ui.button(i.cancel).clicked() { // Clear
                                        self.app.expanded_nodes.clear();
                                        self.app.graph_needs_sync = true;
                                    }
                                }
                                if ui.button(i.fit_screen).clicked() {
                                    self.app.trigger_fit = true;
                                }
                                if ui.button(i.screenshot).clicked() {
                                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot);
                                }
                            });
                            ui.horizontal(|ui| {
                                if ui.checkbox(&mut self.app.filters.show_individuals, i.show_individuals).changed() { self.app.graph_needs_sync = true; }
                                if ui.checkbox(&mut self.app.filters.show_schema, i.show_schema).changed() { self.app.graph_needs_sync = true; }
                                if ui.checkbox(&mut self.app.filters.show_inferred, i.show_inferred).changed() { self.app.graph_needs_sync = true; }
                                if ui.checkbox(&mut self.app.filters.show_literals, i.show_literals).changed() { self.app.graph_needs_sync = true; }
                                if ui.checkbox(&mut self.app.filters.show_system, i.show_system).changed() { self.app.graph_needs_sync = true; }
                            });
                        });
                    });
                    if self.app.graph_needs_sync {
                        ui.ctx().request_repaint();
                    }
                    self.app.render_custom_graph(ui);
                }
            }
            DarkstarTab::History => {
                let i = self.app.i18n();
                ui.heading(i.history);
                egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                    for event in self.app.manager.history.get_undo_stack() {
                        ui.label(format!("✓ {:?}", event));
                    }
                });
            }
        }
    }
}

fn load_icon() -> Option<egui::IconData> {
    let icon_bytes = include_bytes!("../assets/icon.png");
    let image = image::load_from_memory(icon_bytes).ok()?;
    let image = image.to_rgba8();
    let (width, height) = image.dimensions();
    Some(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_icon(load_icon().unwrap_or_default()),
        ..Default::default()
    };
    eframe::run_native(
        "Darkstar - Native Ontology Editor & Reasoner",
        options,
        Box::new(|cc| Ok(Box::new(DarkstarApp::new(cc)))),
    )
}
