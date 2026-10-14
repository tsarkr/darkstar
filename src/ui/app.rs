use eframe::egui;
use egui_extras::install_image_loaders;
use egui_dock::DockState;
use egui::Vec2;
use std::collections::{HashMap, HashSet};

use crate::core::manager::DarkstarManager;
use crate::core::plugin::PluginManager;
use crate::core::settings::{AppSettings, Language};
use crate::core::l10n::L10n;
use crate::ui::{DarkstarTab, GraphNode, GraphEdge, GraphFilters, AppState};
use crate::rules::engine::InferenceEngine;
use sophia::api::prelude::*;
use sophia::api::term::matcher::Any;

pub struct DarkstarApp {
    pub manager: DarkstarManager,
    pub selected_uri: Option<String>,
    
    // UI State
    pub search_query: String,
    pub auto_reasoning: bool,
    pub status_message: Option<(String, std::time::Instant)>,
    pub show_settings: bool,
    pub show_metrics_dialog: bool,
    pub show_help: bool,

    // Persistence
    pub settings: AppSettings,

    // Docking
    pub dock_state: DockState<DarkstarTab>,
    
    // Add Property Form State
    pub new_prop_predicate: String,
    pub new_prop_value: String,
    
    // Custom Graph State
    pub nodes: HashMap<String, GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub graph_needs_sync: bool,
    pub dragging_node: Option<String>,
    pub filters: GraphFilters,
    pub expanded_nodes: HashSet<String>,
    pub trigger_fit: bool,
    pub graph_offset: Vec2,
    pub graph_scale: f32,
    pub last_graph_rect: Option<egui::Rect>,
    pub screenshot_pending: bool,

    // Refactor & Bulk State
    pub renaming_uri: Option<(String, String)>,
    pub clipboard: Vec<String>,
    pub selected_individuals: HashSet<String>,
    pub confirm_delete_uri: Option<String>,
    pub confirm_revert_dialog: bool,
    pub merging_uri: Option<(String, String)>,
    pub annotation_buffer: Option<(String, String)>,
    pub simulation_alpha: f32,
    
    // Source Editor State
    pub source_code_buffer: String,
    pub source_code_error: Option<String>,
    pub source_needs_sync: bool,
    
    // Exit Dialog
    pub show_exit_dialog: bool,
    pub allowed_to_close: bool,
    
    // Plugins
    pub plugin_manager: PluginManager,
    
    // Onboarding/Splash state
    pub state: AppState,
    pub splash_start_time: std::time::Instant,
    pub onboarding_step: usize,
    
    pub show_plugin_manual: bool,
    pub plugin_manual_lang: Language,

    pub external_file_changed: bool,
    pub last_disk_check: std::time::Instant,

    // Caching for hierarchies & lists
    pub classes_cache_dirty: bool,
    pub class_roots: Vec<String>,
    pub class_children: HashMap<String, Vec<String>>,
    
    pub properties_cache_dirty: bool,
    pub object_prop_roots: Vec<String>,
    pub object_prop_children: HashMap<String, Vec<String>>,
    pub datatype_prop_roots: Vec<String>,
    pub datatype_prop_children: HashMap<String, Vec<String>>,
    
    pub individuals_cache_dirty: bool,
    pub individuals_by_type: HashMap<String, Vec<String>>,

    // Virtualized individuals list (flattened rows, rebuilt only when data/query/collapse state changes)
    pub individual_rows: Vec<crate::ui::IndividualRow>,
    pub individual_rows_dirty: bool,
    pub individual_rows_query: String,
    pub collapsed_types: HashSet<String>,

    /// Last `manager.revision` the UI caches were built against.
    pub cache_revision: u64,
    /// Cached (asserted, inferred) triple counts for the status bar.
    pub triple_counts: (usize, usize),
    /// True when focus-mode graph exploration hit the node cap.
    pub graph_truncated: bool,
    /// Source editor: user explicitly asked to load a very large ontology into the text editor.
    pub source_force_full: bool,
    /// Source editor: read-only preview shown for very large ontologies.
    pub source_preview: Option<String>,

    /// Search: entity -> lowercased rdfs:label text (built lazily per revision)
    pub label_index: HashMap<String, String>,
    pub label_index_revision: u64,
    /// Focus-mode node cap; raised by the graph's "show more" button, reset on new selection.
    pub focus_node_limit: usize,
    pub focus_limit_uri: Option<String>,
    /// Search query the graph was last synced with (resync when it changes while truncated).
    pub graph_sync_query: String,
    /// Entity editor: rows shown per section [asserted, inferred, incoming], reset on new selection.
    pub relation_limits: [usize; 3],
    pub relation_limits_uri: Option<String>,
}

impl DarkstarApp {
    pub fn i18n(&self) -> L10n {
        L10n::get(self.settings.language)
    }

    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_image_loaders(&cc.egui_ctx);
        let settings = AppSettings::load();
        
        let mut fonts = egui::FontDefinitions::default();
        let font_paths = [
            "/System/Library/Fonts/AppleSDGothicNeo.ttc",
            "/System/Library/Fonts/Supplemental/AppleGothic.ttf",
            "/Library/Fonts/NanumGothic.ttf",
            "C:\\Windows\\Fonts\\malgun.ttf",
            "C:\\Windows\\Fonts\\malgunsl.ttf",
            "C:\\Windows\\Fonts\\gulim.ttc",
        ];

        let mut font_loaded = false;
        for path in font_paths {
            if let Ok(font_data) = std::fs::read(path) {
                fonts.font_data.insert("korean_font".to_owned(), egui::FontData::from_owned(font_data));
                fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "korean_font".to_owned());
                fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().push("korean_font".to_owned());
                font_loaded = true;
                break;
            }
        }
        if font_loaded { cc.egui_ctx.set_fonts(fonts); }

        // Set global font size to 11
        let mut style = (*cc.egui_ctx.style()).clone();
        for id in style.text_styles.values_mut() {
            id.size = 11.0;
        }
        cc.egui_ctx.set_style(style);
        let native_pp = cc.egui_ctx.native_pixels_per_point().unwrap_or(1.0);
        cc.egui_ctx.set_pixels_per_point(native_pp * settings.ui_scale);

        let mut dock_state = DockState::new(vec![DarkstarTab::Graph, DarkstarTab::SourceEditor]);
        // 1. Split far right for EntityEditor
        let [left_and_center, _right] = dock_state.main_surface_mut().split_right(egui_dock::NodeIndex::root(), 0.8, vec![DarkstarTab::EntityEditor]);
        // 2. Split far left for Classes
        let [_left, center] = dock_state.main_surface_mut().split_left(left_and_center, 0.2, vec![DarkstarTab::Classes]);
        // 3. Split center for bottom tabs
        dock_state.main_surface_mut().split_below(center, 0.75, vec![DarkstarTab::Individuals, DarkstarTab::ObjectProperties, DarkstarTab::DataProperties, DarkstarTab::History]);

        let mut app = Self {
            manager: DarkstarManager::new(),
            selected_uri: None,
            settings: settings.clone(),
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
            trigger_fit: true,
            graph_offset: Vec2::ZERO,
            graph_scale: 1.0,
            last_graph_rect: None,
            screenshot_pending: false,
            renaming_uri: None,
            clipboard: Vec::new(),
            selected_individuals: HashSet::new(),
            confirm_delete_uri: None,
            confirm_revert_dialog: false,
            merging_uri: None,
            annotation_buffer: None,
            simulation_alpha: 1.0,
            source_code_buffer: String::new(),
            source_code_error: None,
            source_needs_sync: true,
            show_exit_dialog: false,
            allowed_to_close: false,
            plugin_manager: PluginManager::new(),
            state: AppState::Splash,
            splash_start_time: std::time::Instant::now(),
            onboarding_step: 0,
            show_plugin_manual: false,
            plugin_manual_lang: settings.language,
            show_help: false,
            external_file_changed: false,
            last_disk_check: std::time::Instant::now(),

            classes_cache_dirty: true,
            class_roots: Vec::new(),
            class_children: HashMap::new(),
            properties_cache_dirty: true,
            object_prop_roots: Vec::new(),
            object_prop_children: HashMap::new(),
            datatype_prop_roots: Vec::new(),
            datatype_prop_children: HashMap::new(),
            individuals_cache_dirty: true,
            individuals_by_type: HashMap::new(),
            individual_rows: Vec::new(),
            individual_rows_dirty: true,
            individual_rows_query: String::new(),
            collapsed_types: HashSet::new(),
            cache_revision: u64::MAX,
            triple_counts: (0, 0),
            graph_truncated: false,
            source_force_full: false,
            source_preview: None,
            label_index: HashMap::new(),
            label_index_revision: u64::MAX,
            focus_node_limit: crate::ui::tabs::graph_layout::FOCUS_NODES_STEP,
            focus_limit_uri: None,
            graph_sync_query: String::new(),
            relation_limits: [crate::ui::tabs::entity_editor::RELATION_ROWS_STEP; 3],
            relation_limits_uri: None,
        };
        
        crate::plugins::load_all(&mut app.plugin_manager, &mut app.manager);
        app
    }

    pub fn open_ontology_file(&mut self, path: &std::path::Path) {
        let prev_path = self.manager.active_file_path.clone();
        let prev_name = prev_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string());
        let new_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());

        match self.manager.load_from_file(path, &self.settings.enabled_rules) {
            Ok(_) => {
                self.settings.add_recent(path.to_path_buf());
                self.on_ontology_loaded();

                let is_korean = self.settings.language == crate::core::settings::Language::Korean;
                let msg = if let Some(prev) = prev_name {
                    if is_korean {
                        format!("기존 파일 '{}'을(를) 닫고 '{}'을(를) 새로 불러왔습니다.", prev, new_name)
                    } else {
                        format!("Closed previous file '{}' and successfully loaded '{}'.", prev, new_name)
                    }
                } else {
                    if is_korean {
                        format!("'{}' 파일을 성공적으로 불러왔습니다.", new_name)
                    } else {
                        format!("Successfully loaded '{}'.", new_name)
                    }
                };
                self.status_message = Some((msg, std::time::Instant::now()));
            }
            Err(e) => {
                let err_msg = format!("Failed to load {}: {}", new_name, e);
                eprintln!("{}", err_msg);
                self.status_message = Some((err_msg, std::time::Instant::now()));
            }
        }
    }

    pub fn new_ontology_file(&mut self) {
        let prev_path = self.manager.active_file_path.clone();
        let prev_name = prev_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string());

        self.manager.new_ontology();
        self.on_ontology_cleared();

        let is_korean = self.settings.language == crate::core::settings::Language::Korean;
        let msg = if let Some(prev) = prev_name {
            if is_korean {
                format!("기존 파일 '{}'을(를) 닫고 새 온톨로지를 생성했습니다.", prev)
            } else {
                format!("Closed '{}' and created a new empty ontology.", prev)
            }
        } else {
            if is_korean {
                "새 온톨로지를 생성했습니다.".to_string()
            } else {
                "Created new empty ontology.".to_string()
            }
        };
        self.status_message = Some((msg, std::time::Instant::now()));
    }

    pub fn on_ontology_loaded(&mut self) {
        // 1. Clear all graph nodes and edges completely
        self.nodes.clear();
        self.edges.clear();
        self.selected_uri = None;
        self.expanded_nodes.clear();
        self.selected_individuals.clear();
        self.dragging_node = None;
        self.last_graph_rect = None;
        self.graph_offset = Vec2::ZERO;
        self.graph_scale = 1.0;
        self.trigger_fit = true;
        self.graph_needs_sync = true;

        // 2. Invalidate entity trees and caches
        self.classes_cache_dirty = true;
        self.properties_cache_dirty = true;
        self.individuals_cache_dirty = true;
        self.class_roots.clear();
        self.class_children.clear();
        self.object_prop_roots.clear();
        self.object_prop_children.clear();
        self.datatype_prop_roots.clear();
        self.datatype_prop_children.clear();
        self.individuals_by_type.clear();

        // 3. Source editor is synchronized lazily (only when its tab is actually visible).
        //    Serializing a large ontology eagerly costs hundreds of ms and tens of MB.
        self.source_code_error = None;
        self.source_code_buffer.clear();
        self.source_preview = None;
        self.source_force_full = false;
        self.source_needs_sync = true;
        self.collapsed_types.clear();
        self.individual_rows_dirty = true;

        // 4. Run reasoning with active user settings
        self.manager.run_reasoning(&self.settings.enabled_rules);

        // 5. Notify plugins
        let event = crate::core::history::DarkstarEvent::Batch(vec![]);
        for plugin in &mut self.plugin_manager.plugins {
            plugin.handle_event(&event, &mut self.manager);
        }
    }

    pub fn on_ontology_cleared(&mut self) {
        self.nodes.clear();
        self.edges.clear();
        self.selected_uri = None;
        self.expanded_nodes.clear();
        self.selected_individuals.clear();
        self.dragging_node = None;
        self.last_graph_rect = None;
        self.graph_offset = Vec2::ZERO;
        self.graph_scale = 1.0;
        self.trigger_fit = true;
        self.graph_needs_sync = true;

        self.classes_cache_dirty = true;
        self.properties_cache_dirty = true;
        self.individuals_cache_dirty = true;
        self.class_roots.clear();
        self.class_children.clear();
        self.object_prop_roots.clear();
        self.object_prop_children.clear();
        self.datatype_prop_roots.clear();
        self.datatype_prop_children.clear();
        self.individuals_by_type.clear();

        self.source_code_error = None;
        self.source_needs_sync = false;
        self.source_code_buffer.clear();
        self.source_preview = None;
        self.source_force_full = false;
        self.collapsed_types.clear();
        self.individual_rows_dirty = true;

        let event = crate::core::history::DarkstarEvent::Batch(vec![]);
        for plugin in &mut self.plugin_manager.plugins {
            plugin.handle_event(&event, &mut self.manager);
        }
    }

    pub fn get_label(prefixed_uri: &str) -> String {
        if prefixed_uri.starts_with("l:") {
            let raw = &prefixed_uri[2..];
            if let Some(idx) = raw.rfind("^^") {
                let val = &raw[..idx];
                let dt = &raw[idx + 2..];
                let dt_name = dt.split('/').last().unwrap_or(dt).split('#').last().unwrap_or(dt);
                format!("\"{}\"^^{}", val, dt_name)
            } else if let Some(idx) = raw.rfind('@') {
                format!("\"{}\"@{}", &raw[..idx], &raw[idx + 1..])
            } else {
                format!("\"{}\"", raw)
            }
        } else if prefixed_uri.starts_with("b:") {
            let bnode = &prefixed_uri[2..];
            format!("_:{}", bnode)
        } else {
            let uri = if prefixed_uri.len() >= 2 && &prefixed_uri[1..2] == ":" {
                &prefixed_uri[2..]
            } else {
                prefixed_uri
            };
            uri.split('/').last().unwrap_or(uri).split('#').last().unwrap_or(uri).to_string()
        }
    }

    pub fn render_main(&mut self, ctx: &egui::Context) {
        ctx.set_visuals(if self.settings.theme_dark { egui::Visuals::dark() } else { egui::Visuals::light() });

        // Check external file modification periodically
        if self.last_disk_check.elapsed() >= std::time::Duration::from_millis(1000) {
            self.last_disk_check = std::time::Instant::now();
            if let Some(path) = &self.manager.active_file_path {
                if let Ok(meta) = std::fs::metadata(path) {
                    if let Ok(disk_mtime) = meta.modified() {
                        if let Some(last_known) = self.manager.file_modified_time {
                            if disk_mtime > last_known && !self.external_file_changed {
                                self.external_file_changed = true;
                            }
                        }
                    }
                }
            }
        }

        // Drain events and notify plugins
        let events: Vec<_> = self.manager.event_queue.drain(..).collect();
        for event in events {
            for plugin in &mut self.plugin_manager.plugins {
                plugin.handle_event(&event, &mut self.manager);
            }
        }

        if self.manager.is_dirty {
            self.source_needs_sync = true;
            if self.auto_reasoning {
                self.manager.run_reasoning(&self.settings.enabled_rules);
                self.graph_needs_sync = true;
            }
        }

        // Invalidate data-derived caches only when the ontology actually changed.
        // (Previously every selection click set graph_needs_sync and forced all
        // class/property/individual caches to be rebuilt from the full graph.)
        if self.manager.revision != self.cache_revision {
            self.cache_revision = self.manager.revision;
            self.classes_cache_dirty = true;
            self.properties_cache_dirty = true;
            self.individuals_cache_dirty = true;
            self.graph_needs_sync = true;
            self.triple_counts = (
                self.manager.memory.asserted_graph.triples().count(),
                self.manager.memory.inferred_graph.triples().count(),
            );
        }

        self.render_plugin_guide(ctx);

        self.render_settings(ctx);
        self.render_help(ctx);
        self.render_top_bar(ctx);
        self.render_status_bar(ctx);
        self.render_dialogs(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut dock_state = std::mem::replace(&mut self.dock_state, egui_dock::DockState::new(vec![]));
            {
                let mut viewer = crate::ui::dock::DarkstarTabViewer { app: self };
                egui_dock::DockArea::new(&mut dock_state).show_inside(ui, &mut viewer);
            }
            self.dock_state = dock_state;
        });
    }

    pub fn render_status_bar(&mut self, ctx: &egui::Context) {
        let i = self.i18n();
        let is_korean = self.settings.language == Language::Korean;

        egui::TopBottomPanel::bottom("bottom_status_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Status message or active file info
                let mut has_message = false;
                if let Some((msg, time)) = &self.status_message {
                    if time.elapsed().as_secs() < 8 {
                        has_message = true;
                        ui.label(
                            egui::RichText::new(format!("ℹ {}", msg))
                                .color(if self.settings.theme_dark { egui::Color32::from_rgb(120, 220, 160) } else { egui::Color32::from_rgb(20, 140, 60) })
                                .strong()
                        );
                    }
                }

                if !has_message {
                    if let Some(path) = &self.manager.active_file_path {
                        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("ontology");
                        let dirty_flag = if self.manager.needs_save { "*" } else { "" };
                        ui.label(format!("📄 {}{}", filename, dirty_flag));
                    } else {
                        ui.label(format!("✨ {}", i.status_ready));
                    }
                }

                // If external file change was detected
                if self.external_file_changed {
                    ui.separator();
                    ui.label(
                        egui::RichText::new(if is_korean { "⚠️ 파일이 외부에서 변경되었습니다!" } else { "⚠️ File changed externally!" })
                            .color(egui::Color32::YELLOW)
                            .strong()
                    );
                    if ui.button(if is_korean { "새로고침 (다시 로드)" } else { "Reload" }).clicked() {
                        if let Some(path) = self.manager.active_file_path.clone() {
                            self.open_ontology_file(&path);
                        }
                        self.external_file_changed = false;
                    }
                    if ui.button(if is_korean { "무시" } else { "Ignore" }).clicked() {
                        if let Some(path) = &self.manager.active_file_path {
                            self.manager.file_modified_time = std::fs::metadata(path).and_then(|m| m.modified()).ok();
                        }
                        self.external_file_changed = false;
                    }
                }

                // Right aligned stats
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Consistency badge
                    if self.manager.memory.is_consistent {
                        ui.label(egui::RichText::new(format!("● {}", i.consistent)).color(egui::Color32::GREEN).size(11.0));
                    } else {
                        ui.label(egui::RichText::new(format!("● {}", i.inconsistent)).color(egui::Color32::RED).size(11.0));
                    }
                    ui.separator();
                    
                    let (assert_count, inf_count) = self.triple_counts;
                    ui.label(format!("Triples: {} (+{} inferred)", assert_count, inf_count));
                });
            });
        });
    }

    fn render_settings(&mut self, ctx: &egui::Context) {
        if !self.show_settings { return; }
        let i = self.i18n();
        let mut show = self.show_settings;
        let mut changed = false;
        let mut close_clicked = false;
        
        egui::Window::new(i.settings_title)
            .open(&mut show)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.group(|ui| {
                        ui.label(egui::RichText::new(i.appearance).strong());
                        if ui.checkbox(&mut self.settings.theme_dark, i.theme_dark).changed() { changed = true; }
                        ui.horizontal(|ui| {
                            ui.label(i.ui_scale);
                            if ui.add(egui::Slider::new(&mut self.settings.ui_scale, 0.5..=2.0)).changed() {
                                changed = true;
                            }
                            if ui.button(i.apply).clicked() {
                                let native_pp = ctx.native_pixels_per_point().unwrap_or(1.0);
                                ctx.set_pixels_per_point(native_pp * self.settings.ui_scale);
                            }
                            if ui.button("1.0").on_hover_text("기본 크기(1.0)로 초기화").clicked() {
                                self.settings.ui_scale = 1.0;
                                let native_pp = ctx.native_pixels_per_point().unwrap_or(1.0);
                                ctx.set_pixels_per_point(native_pp);
                                changed = true;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(i.language);
                            let old_lang = self.settings.language;
                            egui::ComboBox::from_id_salt("lang_combo")
                                .selected_text(format!("{:?}", self.settings.language))
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.settings.language, crate::core::settings::Language::English, "English");
                                    ui.selectable_value(&mut self.settings.language, crate::core::settings::Language::Korean, "한국어");
                                });
                            if self.settings.language != old_lang { changed = true; }
                        });
                    });
                    
                    ui.add_space(10.0);
                    ui.group(|ui| {
                        ui.label(egui::RichText::new(i.rules_title).strong());
                        if ui.checkbox(&mut self.settings.enabled_rules.equality, i.eq_rules).changed() { changed = true; }
                        if ui.checkbox(&mut self.settings.enabled_rules.property_chains, i.chain_rules).changed() { changed = true; }
                        if ui.checkbox(&mut self.settings.enabled_rules.class_expressions, i.exp_rules).changed() { changed = true; }
                        if ui.checkbox(&mut self.settings.enabled_rules.restrictions, i.restr_rules).changed() { changed = true; }
                        if ui.checkbox(&mut self.settings.enabled_rules.datatypes, i.dt_rules).changed() { changed = true; }
                        if ui.checkbox(&mut self.settings.enabled_rules.schema, i.scm_rules).changed() { changed = true; }
                    });

                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button(i.close).clicked() { close_clicked = true; }
                    });
                });
            });
        
        if close_clicked { show = false; }
        if changed {
            self.settings.save();
            self.manager.is_dirty = true;
            self.graph_needs_sync = true;
        }
        self.show_settings = show;
    }

    fn render_help(&mut self, ctx: &egui::Context) {
        if !self.show_help { return; }
        let i = self.i18n();
        let mut show = self.show_help;
        let mut close_clicked = false;
        egui::Window::new(i.help)
            .open(&mut show)
            .default_width(500.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.heading("Darkstar Ontology Editor Help");
                        ui.separator();
                        
                        ui.group(|ui| {
                            ui.label(egui::RichText::new(i.file).strong());
                            ui.label("• New: Create a fresh ontology.");
                            ui.label("• Open: Load RDF/OWL files from your disk.");
                            ui.label("• Open Recent: Quick access to your last 5 files.");
                            ui.label("• Save/Save As: Persist your changes to Turtle (.ttl) format.");
                        });

                        ui.add_space(5.0);
                        ui.group(|ui| {
                            ui.label(egui::RichText::new(i.reasoning).strong());
                            ui.label("• Start Reasoner: Manually trigger the parallel forward-chaining engine.");
                            ui.label("• Auto-Reasoning: Automatically re-run inference when any triple is added or removed.");
                        });

                        ui.add_space(5.0);
                        ui.group(|ui| {
                            ui.label(egui::RichText::new(i.graph_view).strong());
                            ui.label("• Focus Mode: (Default) Only show nodes near the selected entity.");
                            ui.label("• Expansion Depth: Control how many hops are visible around the focused node.");
                        });

                        ui.add_space(5.0);
                        ui.group(|ui| {
                            ui.label(egui::RichText::new(i.window).strong());
                            ui.label("• Show/Hide Tabs: Toggle visibility of Classes, Graph, Editor, etc.");
                            ui.label("• Reset Layout: Restore the default professional workspace arrangement.");
                        });

                        ui.add_space(10.0);
                        if ui.button(i.close).clicked() { close_clicked = true; }
                    });
                });
            });
        if close_clicked { show = false; }
        self.show_help = show;
    }

    fn render_dialogs(&mut self, ctx: &egui::Context) {
        let i = self.i18n();
        // use removed here as we added them to top level

        // 1. Delete Confirmation
        if let Some(uri) = self.confirm_delete_uri.clone() {
            let mut open = true;
            egui::Window::new(i.delete)
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(i.confirm_delete_msg);
                        ui.add_space(5.0);
                        ui.label(egui::RichText::new(&uri).monospace().color(egui::Color32::GRAY));
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if ui.button("Confirm").clicked() {
                                let s_term = InferenceEngine::make_term(&uri);
                                // Indexed lookups instead of a full scan of the asserted graph.
                                let mut seen = HashSet::new();
                                let mut to_delete = Vec::new();
                                let g = &self.manager.memory.asserted_graph;
                                for t in g.triples_matching(Some(&s_term), Any, Any).flatten()
                                    .chain(g.triples_matching(Any, Any, Some(&s_term)).flatten())
                                {
                                    let key = (crate::rules::extract_str(&t.s()), crate::rules::extract_str(&t.p()), crate::rules::extract_str(&t.o()));
                                    if seen.insert(key.clone()) { to_delete.push(key); }
                                }
                                self.manager.remove_assertions_batch(to_delete);
                                self.selected_uri = None;
                                self.confirm_delete_uri = None;
                                self.graph_needs_sync = true;
                            }
                            if ui.button(i.close).clicked() {
                                self.confirm_delete_uri = None;
                            }
                        });
                    });
                });
            if !open { self.confirm_delete_uri = None; }
        }

        // 2. Rename Dialog
        if let Some((old_uri, mut new_label)) = self.renaming_uri.clone() {
            let mut open = true;
            egui::Window::new(i.rename_entity)
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.vertical(|ui| {
                        ui.label(format!("Old URI: {}", old_uri));
                        ui.horizontal(|ui| {
                            ui.label("New Label:");
                            ui.text_edit_singleline(&mut new_label);
                        });
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if ui.button("Rename (Apply Label)").clicked() {
                                let rdfs_label = "http://www.w3.org/2000/01/rdf-schema#label".to_string();
                                let s_term = InferenceEngine::make_term(&old_uri);
                                let p_term = InferenceEngine::make_term(&rdfs_label);
                                let mut to_remove = Vec::new();
                                for t in self.manager.memory.asserted_graph.triples_matching(Some(&s_term), Some(&p_term), Any).flatten() {
                                    to_remove.push(crate::rules::extract_str(&t.o()));
                                }
                                for old_val in to_remove {
                                    self.manager.remove_assertion(old_uri.clone(), rdfs_label.clone(), old_val);
                                }
                                self.manager.add_assertion(old_uri.clone(), rdfs_label, format!("l:{}", new_label));
                                self.renaming_uri = None;
                                self.graph_needs_sync = true;
                            }
                            if ui.button(i.close).clicked() {
                                self.renaming_uri = None;
                            }
                        });
                    });
                });
            if !open { self.renaming_uri = None; }
            else if self.renaming_uri.is_some() { self.renaming_uri = Some((old_uri, new_label)); }
        }
    }

    pub fn generate_unique_uri(&self, prefix: &str) -> String {
        let mut i = 1;
        loop {
            let uri = format!("i:{}_{}", prefix, i);
            let s_term = InferenceEngine::make_term(&uri);
            if !self.manager.memory.main_graph.triples_matching(Some(&s_term), Any, Any).flatten().next().is_some() {
                return uri;
            }
            i += 1;
        }
    }

    pub fn render_exit_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_exit_dialog { return; }
        
        let i = self.i18n();
        let mut open = self.show_exit_dialog;
        
        egui::Window::new(i.unsaved_changes_title)
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.label(i.unsaved_changes_msg);
                    ui.add_space(10.0);
                    
                    ui.horizontal(|ui| {
                        if ui.button(i.save_and_quit).clicked() {
                            // If we have a file, save and close
                            if let Some(path) = self.manager.active_file_path.clone() {
                                if let Err(e) = self.manager.save_to_file(&path, crate::core::io::OntologyFormat::Turtle, false) {
                                    eprintln!("Save error: {}", e);
                                } else {
                                    self.allowed_to_close = true;
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                }
                            } else {
                                // Save As dialog
                                if let Some(path) = rfd::FileDialog::new().set_file_name("ontology.owl").save_file() {
                                    if let Err(e) = self.manager.save_to_file(&path, crate::core::io::OntologyFormat::Turtle, false) {
                                        eprintln!("Save error: {}", e);
                                    } else {
                                        self.settings.add_recent(path);
                                        self.allowed_to_close = true;
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                    }
                                }
                            }
                            self.show_exit_dialog = false;
                        }
                        
                        if ui.button(i.quit_without_saving).clicked() {
                            self.allowed_to_close = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            self.show_exit_dialog = false;
                        }
                        
                        if ui.button(i.close).clicked() {
                            self.show_exit_dialog = false;
                        }
                    });
                });
            });
            
        if !open { self.show_exit_dialog = false; }
    }
}

impl eframe::App for DarkstarApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle Screenshot
        for event in ctx.input(|i| i.raw.events.clone()) {
            if let egui::Event::Screenshot { image, .. } = event {
                let ppp = ctx.pixels_per_point();
                if let (Some(path), Some(rect)) = (
                    rfd::FileDialog::new()
                        .set_file_name("graph_capture.png")
                        .add_filter("PNG Image", &["png"])
                        .save_file(),
                    self.last_graph_rect
                ) {
                    let left = (rect.min.x * ppp).round() as u32;
                    let top = (rect.min.y * ppp).round() as u32;
                    let width = (rect.width() * ppp).round() as u32;
                    let height = (rect.height() * ppp).round() as u32;

                    let full_width = image.width() as u32;
                    let full_height = image.height() as u32;
                    let pixels = image.as_raw();

                    let mut img_buf = image::ImageBuffer::new(width, height);
                    for (x, y, pixel) in img_buf.enumerate_pixels_mut() {
                        let px = left + x;
                        let py = top + y;
                        if px < full_width && py < full_height {
                            let idx = (py as usize * full_width as usize + px as usize) * 4;
                            if idx + 3 < pixels.len() {
                                *pixel = image::Rgba([pixels[idx], pixels[idx+1], pixels[idx+2], pixels[idx+3]]);
                            }
                        }
                    }
                    if let Err(e) = img_buf.save(path) {
                        eprintln!("Failed to save screenshot: {}", e);
                    }
                }
            }
        }

        if ctx.input(|i| i.viewport().close_requested()) {
            if self.manager.needs_save && !self.allowed_to_close {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.show_exit_dialog = true;
            }
        }

        match self.state {
            AppState::Splash => self.render_splash(ctx),
            AppState::Onboarding => self.render_onboarding(ctx),
            AppState::Main => {
                self.render_main(ctx);
                self.render_exit_dialog(ctx);
            }
        }
    }
}
