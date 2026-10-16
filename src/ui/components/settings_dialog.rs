//! Preferences and help windows.

use eframe::egui;
use crate::ui::app::DarkstarApp;

impl DarkstarApp {
    pub(crate) fn render_settings(&mut self, ctx: &egui::Context) {
        if !self.show_settings { return; }
        let i = self.i18n();
        let mut show = self.show_settings;
        let mut ui_changed = false;
        let mut rules_changed = false;
        let mut close_clicked = false;
        
        egui::Window::new(i.settings_title)
            .open(&mut show)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.group(|ui| {
                        ui.label(egui::RichText::new(i.appearance).strong());
                        if ui.checkbox(&mut self.settings.theme_dark, i.theme_dark).changed() { ui_changed = true; }
                        ui.horizontal(|ui| {
                            ui.label(i.ui_scale);
                            if ui.add(egui::Slider::new(&mut self.settings.ui_scale, 0.5..=2.0)).changed() {
                                ui_changed = true;
                            }
                            if ui.button(i.apply).clicked() {
                                let native_pp = ctx.native_pixels_per_point().unwrap_or(1.0);
                                ctx.set_pixels_per_point(native_pp * self.settings.ui_scale);
                            }
                            if ui.button("1.0").on_hover_text("기본 크기(1.0)로 초기화").clicked() {
                                self.settings.ui_scale = 1.0;
                                let native_pp = ctx.native_pixels_per_point().unwrap_or(1.0);
                                ctx.set_pixels_per_point(native_pp);
                                ui_changed = true;
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
                            if self.settings.language != old_lang { ui_changed = true; }
                        });
                    });
                    
                    ui.add_space(10.0);
                    ui.group(|ui| {
                        ui.label(egui::RichText::new(i.rules_title).strong());
                        if ui.checkbox(&mut self.settings.enabled_rules.equality, i.eq_rules).changed() { rules_changed = true; }
                        if ui.checkbox(&mut self.settings.enabled_rules.property_chains, i.chain_rules).changed() { rules_changed = true; }
                        if ui.checkbox(&mut self.settings.enabled_rules.class_expressions, i.exp_rules).changed() { rules_changed = true; }
                        if ui.checkbox(&mut self.settings.enabled_rules.restrictions, i.restr_rules).changed() { rules_changed = true; }
                        if ui.checkbox(&mut self.settings.enabled_rules.datatypes, i.dt_rules).changed() { rules_changed = true; }
                        if ui.checkbox(&mut self.settings.enabled_rules.schema, i.scm_rules).changed() { rules_changed = true; }
                    });

                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button(i.close).clicked() { close_clicked = true; }
                    });
                });
            });
        
        if close_clicked { show = false; }
        if ui_changed || rules_changed {
            self.settings.save();
        }
        if rules_changed {
            self.manager.invalidate_reasoning();
            if self.auto_reasoning {
                let _ = self.manager.run_reasoning(&self.settings.enabled_rules);
            }
            self.classes_cache_dirty = true;
            self.properties_cache_dirty = true;
            self.individuals_cache_dirty = true;
            self.graph_needs_sync = true;
        }
        self.show_settings = show;
    }

    pub(crate) fn render_help(&mut self, ctx: &egui::Context) {
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
