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

    // Refactor & Bulk State
    pub renaming_uri: Option<(String, String)>,
    pub clipboard: Vec<String>,
    pub selected_individuals: HashSet<String>,
    pub confirm_delete_uri: Option<String>,
    pub confirm_revert_dialog: bool,
    pub merging_uri: Option<(String, String)>,
    pub annotation_buffer: Option<(String, String)>,
    pub simulation_alpha: f32,
    
    // Plugins
    pub plugin_manager: PluginManager,
    
    // Onboarding/Splash state
    pub state: AppState,
    pub splash_start_time: std::time::Instant,
    pub onboarding_step: usize,
    
    pub show_plugin_manual: bool,
    pub plugin_manual_lang: Language,
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
        cc.egui_ctx.set_pixels_per_point(settings.ui_scale);

        let mut dock_state = DockState::new(vec![DarkstarTab::Graph]);
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
            renaming_uri: None,
            clipboard: Vec::new(),
            selected_individuals: HashSet::new(),
            confirm_delete_uri: None,
            confirm_revert_dialog: false,
            merging_uri: None,
            annotation_buffer: None,
            simulation_alpha: 1.0,
            plugin_manager: PluginManager::new(),
            state: AppState::Splash,
            splash_start_time: std::time::Instant::now(),
            onboarding_step: 0,
            show_plugin_manual: false,
            plugin_manual_lang: settings.language,
            show_help: false,
        };
        
        crate::plugins::load_all(&mut app.plugin_manager, &mut app.manager);
        app
    }

    pub fn get_label(prefixed_uri: &str) -> String {
        let uri = if prefixed_uri.len() >= 2 && &prefixed_uri[1..2] == ":" {
            &prefixed_uri[2..]
        } else {
            prefixed_uri
        };
        uri.split('/').last().unwrap_or(uri).split('#').last().unwrap_or(uri).to_string()
    }

    pub fn render_main(&mut self, ctx: &egui::Context) {
        ctx.set_visuals(if self.settings.theme_dark { egui::Visuals::dark() } else { egui::Visuals::light() });

        if self.auto_reasoning && self.manager.is_dirty {
            self.manager.run_reasoning(&self.settings.enabled_rules);
            self.graph_needs_sync = true;
        }

        self.render_plugin_guide(ctx);

        self.render_settings(ctx);
        self.render_help(ctx);
        self.render_top_bar(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut dock_state = std::mem::replace(&mut self.dock_state, egui_dock::DockState::new(vec![]));
            {
                let mut viewer = crate::ui::dock::DarkstarTabViewer { app: self };
                egui_dock::DockArea::new(&mut dock_state).show_inside(ui, &mut viewer);
            }
            self.dock_state = dock_state;
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
                                ctx.set_pixels_per_point(self.settings.ui_scale);
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
        if changed { self.settings.save(); }
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
