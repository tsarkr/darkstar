use eframe::egui;
use crate::ui::app::DarkstarApp;
use crate::core::io::OntologyFormat;

impl DarkstarApp {
    pub fn render_source_editor_tab(&mut self, ui: &mut egui::Ui) {
        let i = self.i18n();
        
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(i.source_editor).size(11.0).strong());
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Apply Changes button
                    if ui.button(egui::RichText::new(i.apply_changes).color(egui::Color32::from_rgb(100, 200, 100))).clicked() {
                        match self.manager.replace_from_string(&self.source_code_buffer, OntologyFormat::Turtle) {
                            Ok(_) => {
                                self.source_code_error = None;
                                self.graph_needs_sync = true;
                                self.source_needs_sync = false;
                            }
                            Err(e) => {
                                self.source_code_error = Some(format!("Parse Error: {}", e));
                            }
                        }
                    }
                    
                    // Sync from Graph button
                    if ui.button(i.sync_from_graph).clicked() || self.source_needs_sync {
                        if let Ok(turtle) = self.manager.serialize_to_string(OntologyFormat::Turtle, false) {
                            self.source_code_buffer = turtle;
                            self.source_code_error = None;
                            self.source_needs_sync = false;
                        }
                    }
                });
            });
            ui.separator();
            
            // Show errors if any
            if let Some(err) = &self.source_code_error {
                ui.label(egui::RichText::new(err).color(egui::Color32::RED).monospace().size(11.0));
                ui.add_space(5.0);
            }
            
            // The text editor
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.source_code_buffer)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .desired_rows(40)
                        .code_editor()
                        .lock_focus(true)
                );
            });
        });
    }
}
