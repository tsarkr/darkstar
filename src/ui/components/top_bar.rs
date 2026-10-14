use eframe::egui;
use crate::ui::app::DarkstarApp;
use crate::ui::DarkstarTab;
use crate::core::l10n::L10n;
use crate::core::settings::Language;

impl DarkstarApp {
    pub fn render_top_bar(&mut self, ctx: &egui::Context) {
        let i = self.i18n();
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                // 1. File Menu
                ui.menu_button(i.file, |ui| {
                    if ui.button(i.new).clicked() { self.new_ontology_file(); ui.close_menu(); }
                    if ui.button(i.open).clicked() {
                        if let Some(path) = rfd::FileDialog::new().add_filter("OWL/RDF", &["owl", "rdf", "xml", "ttl", "n3", "nt"]).pick_file() {
                            self.open_ontology_file(&path);
                        }
                        ui.close_menu();
                    }
                    
                    ui.menu_button(i.open_recent, |ui| {
                        let recent = self.settings.recent_files.clone();
                        if recent.is_empty() {
                            ui.label("No recent files");
                        } else {
                            for path in recent {
                                if ui.button(path.to_string_lossy()).clicked() {
                                    self.open_ontology_file(&path);
                                    ui.close_menu();
                                }
                            }
                        }
                    });

                    if let Some(path) = self.manager.active_file_path.clone() {
                        if ui.button(i.revert).clicked() {
                            self.open_ontology_file(&path);
                            ui.close_menu();
                        }
                    }

                    ui.separator();
                    if ui.button(i.save).clicked() { self.save_current(); ui.close_menu(); }
                    if ui.button(i.save_as).clicked() { self.save_as(); ui.close_menu(); }
                    ui.separator();
                    if ui.button(i.export_inferred).clicked() { self.export_inferred(); ui.close_menu(); }
                    ui.separator();
                    if ui.button(i.exit).clicked() { ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
                    ui.separator();
                    if ui.button(i.preferences).clicked() { self.show_settings = true; ui.close_menu(); }
                });

                // 2. Edit Menu
                ui.menu_button(i.edit, |ui| {
                    if ui.button(i.undo).clicked() { self.manager.undo(); ui.close_menu(); }
                    if ui.button(i.redo).clicked() { self.manager.redo(); ui.close_menu(); }
                    ui.separator();
                    ui.add_enabled(false, egui::Button::new(i.cut)); // not implemented yet
                    ui.add_enabled(false, egui::Button::new(i.copy)); // not implemented yet
                    ui.add_enabled(false, egui::Button::new(i.paste)); // not implemented yet
                    ui.separator();
                    // Deletes the selected entity (same confirmation dialog as the editor's 🗑 button).
                    if ui.add_enabled(self.selected_uri.is_some(), egui::Button::new(i.delete)).clicked() {
                        self.confirm_delete_uri = self.selected_uri.clone();
                        ui.close_menu();
                    }
                });

                // 3. View Menu
                ui.menu_button(i.view, |ui| {
                    if ui.button(i.zoom_in).clicked() { self.graph_scale *= 1.2; ui.close_menu(); }
                    if ui.button(i.zoom_out).clicked() { self.graph_scale /= 1.2; ui.close_menu(); }
                    if ui.button(i.reset_zoom).clicked() { self.graph_scale = 1.0; ui.close_menu(); }
                    ui.separator();
                    if ui.button(i.fit_screen).clicked() { self.trigger_fit = true; ui.close_menu(); }
                });

                // 4. Reasoning Menu
                ui.menu_button(i.reasoning, |ui| {
                    if ui.button(i.start_reasoner).clicked() { self.manager.run_reasoning(&self.settings.enabled_rules); ui.close_menu(); }
                    if ui.button(i.sync_reasoner).clicked() { self.manager.run_reasoning(&self.settings.enabled_rules); ui.close_menu(); }
                    ui.separator();
                    ui.checkbox(&mut self.auto_reasoning, i.auto_reasoning);
                });

                // 5. Refactor Menu
                ui.menu_button(i.refactor, |ui| {
                    if ui.button(i.rename_entity).clicked() { 
                        if let Some(uri) = &self.selected_uri { self.renaming_uri = Some((uri.clone(), Self::get_label(uri))); }
                        ui.close_menu();
                    }
                });

                // 6. Tools Menu
                ui.menu_button(i.tools, |ui| {
                    if ui.button(i.ontology_metrics).clicked() { self.show_metrics_dialog = true; ui.close_menu(); }
                });

                // 7. Window Menu
                ui.menu_button(i.window, |ui| {
                    ui.label("Show/Hide Tabs");
                    let tabs = [
                        (DarkstarTab::Classes, i.classes),
                        (DarkstarTab::ObjectProperties, i.obj_props),
                        (DarkstarTab::DataProperties, i.data_props),
                        (DarkstarTab::Individuals, i.individuals),
                        (DarkstarTab::Graph, i.graph_view),
                        (DarkstarTab::EntityEditor, i.property_editor),
                        (DarkstarTab::History, i.history),
                        (DarkstarTab::Sparql, "SPARQL"),
                        (DarkstarTab::DlQuery, "DL Query"),
                    ];

                    for (tab, label) in tabs {
                        if ui.button(format!("👁 {}", label)).clicked() {
                            if let Some(path) = self.dock_state.find_tab(&tab) {
                                self.dock_state.set_active_tab(path);
                            } else {
                                self.dock_state.main_surface_mut().push_to_focused_leaf(tab);
                            }
                            ui.close_menu();
                        }
                    }

                    ui.separator();
                    if ui.button(i.reset_layout).clicked() {
                        let mut new_dock = egui_dock::DockState::new(vec![DarkstarTab::Graph, DarkstarTab::SourceEditor, DarkstarTab::Sparql, DarkstarTab::DlQuery]);
                        let [left_and_center, _right] = new_dock.main_surface_mut().split_right(egui_dock::NodeIndex::root(), 0.8, vec![DarkstarTab::EntityEditor]);
                        let [_left, center] = new_dock.main_surface_mut().split_left(left_and_center, 0.2, vec![DarkstarTab::Classes]);
                        new_dock.main_surface_mut().split_below(center, 0.75, vec![DarkstarTab::Individuals, DarkstarTab::ObjectProperties, DarkstarTab::DataProperties, DarkstarTab::History]);
                        self.dock_state = new_dock;
                        ui.close_menu();
                    }
                });
                
                // 8. Plugins Menu
                ui.menu_button(i.plugins_menu, |ui| {
                    for plugin in &self.plugin_manager.plugins {
                        let p_name = plugin.name(&i);
                        if ui.button(&p_name).clicked() {
                            self.dock_state.main_surface_mut().push_to_focused_leaf(DarkstarTab::Plugin(plugin.name(&L10n::get(Language::English))));
                            ui.close_menu();
                        }
                    }
                });

                // 9. Help Menu
                ui.menu_button(i.help, |ui| {
                    if ui.button(i.help).clicked() { self.show_help = true; ui.close_menu(); }
                    if ui.button(i.plugin_manual).clicked() { self.show_plugin_manual = true; ui.close_menu(); }
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    ui.add(egui::TextEdit::singleline(&mut self.search_query).desired_width(150.0).hint_text(i.search));
                });
            });
        });
    }
}
