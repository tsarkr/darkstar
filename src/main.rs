mod memory;
mod rules;
pub mod core;

use eframe::egui;
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
}

struct GraphFilters {
    show_individuals: bool,
    show_schema: bool,
    show_inferred: bool,
    show_literals: bool,
    layout_mode: LayoutMode,
    focus_mode: bool,
    expansion_depth: usize,
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
}

impl DarkstarApp {
    fn i18n(&self) -> L10n {
        L10n::get(self.settings.language)
    }

    fn new(cc: &eframe::CreationContext<'_>) -> Self {
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

    fn sync_graph(&mut self) {
        use sophia::api::graph::Graph as _;
        
        let mut new_edges = Vec::new();
        let mut seen_nodes = HashSet::new();
        let mut node_types = HashMap::new();

        let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        let owl_class = "http://www.w3.org/2002/07/owl#Class";
        let rdfs_class = "http://www.w3.org/2000/01/rdf-schema#Class";
        let owl_obj_prop = "http://www.w3.org/2002/07/owl#ObjectProperty";
        let owl_data_prop = "http://www.w3.org/2002/07/owl#DatatypeProperty";
        let owl_individual = "http://www.w3.org/2002/07/owl#NamedIndividual";

        // 1. Pre-identify node types
        for t in self.manager.memory.main_graph.triples().flatten() {
            let s = crate::rules::extract_str(&t.s());
            let p = crate::rules::extract_str(&t.p());
            let o = crate::rules::extract_str(&t.o());

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
        let mut add_triple = |s: String, p: String, o: String, is_o_literal: bool, is_inferred: bool| {
            if self.filters.focus_mode && (!visible_in_focus.contains(&s) || !visible_in_focus.contains(&o)) {
                return;
            }

            let s_type = *node_types.get(&s).unwrap_or(&NodeType::Individual);
            let o_type = if is_o_literal { NodeType::Literal } else { *node_types.get(&o).unwrap_or(&NodeType::Individual) };

            if !self.filters.show_literals && o_type == NodeType::Literal { return; }
            if !self.filters.show_schema && (s_type == NodeType::Class || o_type == NodeType::Class || s_type == NodeType::Property) { return; }
            if !self.filters.show_individuals && (s_type == NodeType::Individual || o_type == NodeType::Individual) { return; }

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
        for uri in &seen_nodes {
            if !self.nodes.contains_key(uri) {
                let n_len = uri.len() as f32;
                self.nodes.insert(uri.clone(), GraphNode {
                    pos: Vec2::new(n_len * 20.0 % 800.0, n_len * 30.0 % 600.0),
                    vel: Vec2::ZERO,
                    label: Self::get_label(uri),
                    node_type: *node_types.get(uri).unwrap_or(if uri.starts_with("l:") { &NodeType::Literal } else { &NodeType::Individual }),
                });
            }
        }

        self.nodes.retain(|k, _| seen_nodes.contains(k));
        self.edges = new_edges;

        match self.filters.layout_mode {
            LayoutMode::Hierarchical => self.apply_hierarchical_layout(),
            LayoutMode::Radial => self.apply_radial_layout(),
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
}

impl eframe::App for DarkstarApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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

        // Top Menu Bar
        egui::TopBottomPanel::top("top_menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                let i = self.i18n();
                ui.menu_button(i.file, |ui| {
                    if ui.button(i.new).clicked() {
                        self.manager = DarkstarManager::new();
                        self.selected_uri = None;
                        self.graph_needs_sync = true;
                        ui.close_menu();
                    }
                    if ui.button(i.open).clicked() {
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
                    if ui.button(i.save).clicked() {
                        if let Some(path) = &self.manager.active_file_path {
                            if let Ok(_) = self.manager.save_to_file(path, crate::core::io::OntologyFormat::Turtle, false) {
                                self.status_message = Some((format!("Saved to {}", path.display()), std::time::Instant::now()));
                            }
                        }
                        ui.close_menu();
                    }
                    if ui.button(i.save_as).clicked() {
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

                ui.menu_button(i.edit, |ui| {
                    if ui.add_enabled(!self.manager.history.is_undo_empty(), egui::Button::new(i.undo)).clicked() {
                        self.manager.undo();
                        self.graph_needs_sync = true;
                        ui.close_menu();
                    }
                    if ui.add_enabled(!self.manager.history.is_redo_empty(), egui::Button::new(i.redo)).clicked() {
                        self.manager.redo();
                        self.graph_needs_sync = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(i.copy_entity).clicked() {
                        if let Some(uri) = &self.selected_uri {
                            self.clipboard = vec![uri.clone()];
                        }
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(i.search);
                        ui.text_edit_singleline(&mut self.search_query);
                    });
                });

                ui.menu_button(i.reasoning, |ui| {
                    if ui.button(i.run_reasoning).clicked() {
                        self.manager.run_reasoning(&self.settings.enabled_rules);
                        self.graph_needs_sync = true;
                        ui.close_menu();
                    }
                    ui.checkbox(&mut self.auto_reasoning, i.auto_reasoning);
                });
            });
        });

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
        egui::SidePanel::right("editor_panel").resizable(true).default_width(350.0).show(ctx, |ui| {
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
}

impl DarkstarApp {
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

            for i in 0..keys.len() {
                for j in i+1..keys.len() {
                    let u = &keys[i];
                    let v = &keys[j];
                    let diff = self.nodes[u].pos - self.nodes[v].pos;
                    let dist_sq = diff.length_sq().max(1.0);
                    let force = diff.normalized() * (20000.0 / dist_sq);
                    *forces.entry(u.clone()).or_default() += force;
                    *forces.entry(v.clone()).or_default() -= force;
                }
            }

            for edge in &self.edges {
                if let (Some(n1), Some(n2)) = (self.nodes.get(&edge.from), self.nodes.get(&edge.to)) {
                    let diff = n1.pos - n2.pos;
                    let dist = diff.length();
                    let force = diff.normalized() * (dist - 150.0) * -0.1;
                    *forces.entry(edge.from.clone()).or_default() += force;
                    *forces.entry(edge.to.clone()).or_default() -= force;
                }
            }

            for (uri, node) in &mut self.nodes {
                node.vel += *forces.get(uri).unwrap_or(&Vec2::ZERO);
                node.vel *= 0.8; // Friction
                if self.dragging_node.as_ref() != Some(uri) {
                    node.pos += node.vel * 0.1;
                }
            }
            ui.ctx().request_repaint();
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
        for edge in &self.edges {
            if let (Some(n1), Some(n2)) = (self.nodes.get(&edge.from), self.nodes.get(&edge.to)) {
                let p1 = to_screen_pos(n1.pos, self.graph_offset, self.graph_scale);
                let p2 = to_screen_pos(n2.pos, self.graph_offset, self.graph_scale);
                let color = if edge.is_inferred { egui::Color32::from_rgb(100, 180, 255) } else { egui::Color32::GRAY };
                let stroke = egui::Stroke::new(1.5 * self.graph_scale.sqrt(), color);
                
                painter.line_segment([p1, p2], stroke);
                
                // Arrow
                let dir = (p2 - p1).normalized();
                let head = p2 - dir * (18.0 * self.graph_scale);
                painter.line_segment([p2, head + dir.rot90() * 5.0 * self.graph_scale], stroke);
                painter.line_segment([p2, head - dir.rot90() * 5.0 * self.graph_scale], stroke);
                
                if self.graph_scale > 0.4 {
                    let mid = p1 + (p2 - p1) * 0.5;
                    painter.text(mid, egui::Align2::CENTER_CENTER, &edge.label, egui::FontId::proportional(11.0 * self.graph_scale), color);
                }
            }
        }

        // 6. Draw Nodes
        for (uri, node) in &self.nodes {
            let pos = to_screen_pos(node.pos, self.graph_offset, self.graph_scale);
            let is_selected = self.selected_uri.as_deref() == Some(uri);
            let stroke = if is_selected { egui::Stroke::new(3.0 * self.graph_scale, egui::Color32::WHITE) } else { egui::Stroke::new(1.0 * self.graph_scale, egui::Color32::BLACK) };
            let radius = 20.0 * self.graph_scale;

            match node.node_type {
                NodeType::Class => {
                    painter.circle(pos, radius, egui::Color32::from_rgb(249, 232, 88), stroke);
                }
                NodeType::Property => {
                    let rect_node = egui::Rect::from_center_size(pos, Vec2::new(60.0 * self.graph_scale, 30.0 * self.graph_scale));
                    painter.rect(rect_node, 5.0 * self.graph_scale, egui::Color32::from_rgb(170, 204, 255), stroke);
                }
                NodeType::Individual => {
                    let size = 18.0 * self.graph_scale;
                    let p1 = pos + Vec2::new(0.0, -size);
                    let p2 = pos + Vec2::new(size, 0.0);
                    let p3 = pos + Vec2::new(0.0, size);
                    let p4 = pos + Vec2::new(-size, 0.0);
                    painter.add(egui::Shape::convex_polygon(vec![p1, p2, p3, p4], egui::Color32::from_rgb(194, 174, 255), stroke));
                }
                NodeType::Literal => {
                    let rect_node = egui::Rect::from_center_size(pos, Vec2::new(50.0 * self.graph_scale, 25.0 * self.graph_scale));
                    painter.rect(rect_node, 0.0, egui::Color32::WHITE, stroke);
                }
            }

            if self.graph_scale > 0.3 {
                painter.text(pos + Vec2::new(0.0, radius + 8.0 * self.graph_scale), egui::Align2::CENTER_TOP, &node.label, egui::FontId::proportional(12.0 * self.graph_scale), egui::Color32::WHITE);
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
            "http://www.w3.org/2002/07/owl#Class",
            "http://www.w3.org/2000/01/rdf-schema#Class",
            "http://www.w3.org/2002/07/owl#ObjectProperty",
            "http://www.w3.org/2002/07/owl#DatatypeProperty",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property",
            "http://www.w3.org/2002/07/owl#Ontology",
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
            ui.add_space(ui.available_width() - 250.0);
            if ui.button(i.add_subclass).clicked() {
                // Placeholder for logic
            }
            if ui.button(i.add_sibling).clicked() {
                // Placeholder for logic
            }
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
        });
        ui.separator();
        
        // Characteristics (only for object properties)
        if is_object_property {
            if self.selected_uri.is_some() {
                ui.group(|ui| {
                    ui.label(i.characteristics);
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut false, if self.settings.language == Language::Korean { "전이적(Transitive)" } else { "Transitive" });
                        ui.checkbox(&mut false, if self.settings.language == Language::Korean { "대칭적(Symmetric)" } else { "Symmetric" });
                        ui.checkbox(&mut false, if self.settings.language == Language::Korean { "비대칭적(Asymmetric)" } else { "Asymmetric" });
                        ui.checkbox(&mut false, if self.settings.language == Language::Korean { "반사적(Reflexive)" } else { "Reflexive" });
                    });
                });
            }
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            self.render_property_hierarchy(ui, is_object_property);
        });
    }

    fn render_individuals_tab(&mut self, ui: &mut egui::Ui) {
        let i = self.i18n();
        ui.horizontal(|ui| {
            ui.heading(i.individuals);
            ui.add_space(ui.available_width() - 400.0);
            if ui.add_enabled(!self.selected_individuals.is_empty(), egui::Button::new(i.delete_selected)).clicked() {
                // Bulk delete logic
                let mut changes = Vec::new();
                for uri in &self.selected_individuals {
                    // This is complex as an individual has many triples. 
                    // For now, let's just clear their types.
                    // A better way is to find all triples where they are Subject.
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

        // Modal for renaming
        if let Some((old_uri, mut buffer)) = self.renaming_uri.take() {
            egui::Window::new(i.rename_entity).collapsible(false).resizable(false).show(ui.ctx(), |ui| {
                ui.label(format!("Old URI: {}", old_uri));
                ui.horizontal(|ui| {
                    ui.label("New URI:");
                    ui.text_edit_singleline(&mut buffer);
                });
                ui.horizontal(|ui| {
                    if ui.button(i.apply).clicked() {
                        self.manager.rename_entity(&old_uri, &buffer);
                        self.selected_uri = Some(buffer.clone());
                        self.graph_needs_sync = true;
                        self.status_message = Some((if self.settings.language == Language::Korean { "이름이 변경되었습니다" } else { "Entity renamed" }.to_string(), std::time::Instant::now()));
                        self.renaming_uri = None;
                    }
                    if ui.button(i.cancel).clicked() {
                        self.renaming_uri = None;
                    }
                });
                self.renaming_uri = self.renaming_uri.as_ref().map(|_| (old_uri, buffer));
            });
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            let i = self.i18n();
            // General Description / Annotations
            ui.collapsing(i.annotations, |ui| {
                ui.label("rdfs:comment");
                ui.text_edit_multiline(&mut String::new());
            });

            // Property Assertions
            ui.collapsing(i.property_assertions, |ui| {
                self.render_property_assertions_editor(ui, &uri);
            });

            // Class-specific (SubClassOf / EquivalentClass)
            ui.collapsing(i.class_relations, |ui| {
                ui.label(i.subclass_of);
                // List existing superclasses with remove buttons
                if ui.button(i.add_superclass).clicked() {}
            });

            ui.separator();
            if ui.add(egui::Button::new(i.delete_entity).fill(egui::Color32::from_rgb(150, 50, 50))).clicked() {
                // Delete logic
            }
        });
    }

    fn render_property_assertions_editor(&mut self, ui: &mut egui::Ui, uri: &str) {
        let subject_term = InferenceEngine::make_term(uri);
        let mut to_remove = None;
        let i = self.i18n();

        ui.label(egui::RichText::new(i.existing_assertions).strong());
        egui::Grid::new("assertion_editor_grid").striped(true).num_columns(3).show(ui, |ui| {
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
            ui.horizontal(|ui| {
                ui.label(format!("{}:", i.predicate));
                ui.text_edit_singleline(&mut self.new_prop_predicate).on_hover_text("e.g., i:http://example.org/hasName");
                ui.label(format!("{}:", i.value));
                ui.text_edit_singleline(&mut self.new_prop_value).on_hover_text("Value (use i: for IRI, l: for Literal)");
            });
            ui.horizontal(|ui| {
                if ui.button(format!("✚ {}", i.add)).clicked() {
                    if !self.new_prop_predicate.is_empty() && !self.new_prop_value.is_empty() {
                        self.manager.add_assertion(
                            uri.to_string(),
                            self.new_prop_predicate.clone(),
                            self.new_prop_value.clone(),
                        );
                        self.status_message = Some((if self.settings.language == Language::Korean { "어설션이 추가되었습니다" } else { "Assertion added" }.to_string(), std::time::Instant::now()));
                        self.new_prop_value.clear(); // Clear value after add
                    }
                }
            });
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

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Darkstar - Native Ontology Editor & Reasoner",
        options,
        Box::new(|cc| Ok(Box::new(DarkstarApp::new(cc)))),
    )
}
