use eframe::egui;
use crate::ui::app::DarkstarApp;
use crate::rules::engine::InferenceEngine;
use sophia::api::graph::Graph;
use sophia::api::prelude::*;

impl DarkstarApp {
    pub fn render_entity_editor_tab(&mut self, ui: &mut egui::Ui) {
        let i = self.i18n();
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(i.property_editor).size(11.0).strong());
                ui.separator();
                
                if let Some(uri) = &self.selected_uri {
                    ui.label(egui::RichText::new(uri).monospace().size(11.0).color(egui::Color32::GRAY));
                    ui.add_space(5.0);
                    
                    if let Some(node) = self.nodes.get(uri) {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&node.label).size(11.0).strong());
                            if ui.button(egui::RichText::new("🗑").size(11.0)).on_hover_text(i.delete_entity).clicked() {
                                self.confirm_delete_uri = Some(uri.clone());
                            }
                        });
                        ui.label(egui::RichText::new(format!("{}: {:?}", i.type_label, node.node_type)).size(11.0));
                    }
                    
                    ui.add_space(10.0);
                    ui.separator();
                    
                    // Add New Relation
                    ui.label(egui::RichText::new(i.add_relation).size(11.0).strong());
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.new_prop_predicate).hint_text(i.predicate).font(egui::FontId::proportional(11.0)).desired_width(120.0));
                    });
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.new_prop_value).hint_text(i.object_label).font(egui::FontId::proportional(11.0)).desired_width(120.0));
                        if ui.button(egui::RichText::new(format!("✚ {}", i.add)).size(11.0)).clicked() {
                            if !self.new_prop_predicate.is_empty() && !self.new_prop_value.is_empty() {
                                self.manager.add_assertion(uri.clone(), self.new_prop_predicate.clone(), self.new_prop_value.clone());
                                self.new_prop_predicate.clear();
                                self.new_prop_value.clear();
                                self.graph_needs_sync = true;
                                if self.auto_reasoning { self.manager.run_reasoning(&self.settings.enabled_rules); }
                            }
                        }
                    });
                    ui.add_space(10.0);
                    ui.separator();
                    
                    // Existing Assertions (QUERY DIRECTLY FROM MANAGER)
                    ui.label(egui::RichText::new(i.property_assertions).size(11.0).strong());
                    let mut to_delete = None;
                    
                    let s_term = InferenceEngine::make_term(uri);
                    
                    // Show Asserted
                    for t in self.manager.memory.asserted_graph.triples().flatten() {
                        if s_term == t.s() {
                            let p = crate::rules::extract_str(&t.p());
                            let o = crate::rules::extract_str(&t.o());
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(Self::get_label(&p)).size(11.0).color(egui::Color32::LIGHT_BLUE));
                                ui.label("➔");
                                ui.label(egui::RichText::new(Self::get_label(&o)).size(11.0));
                                if ui.add(egui::Button::new(egui::RichText::new("x").size(11.0))).clicked() {
                                    to_delete = Some((uri.clone(), p.clone(), o.clone()));
                                }
                            });
                        }
                    }
                    
                    // Show Inferred
                    ui.add_space(5.0);
                    for t in self.manager.memory.inferred_graph.triples().flatten() {
                        if s_term == t.s() {
                            if !self.manager.memory.asserted_graph.contains(t.s(), t.p(), t.o()).unwrap() {
                                let p = crate::rules::extract_str(&t.p());
                                let o = crate::rules::extract_str(&t.o());
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(Self::get_label(&p)).size(11.0).color(egui::Color32::from_rgb(100, 200, 100)));
                                    ui.label("➔");
                                    ui.label(egui::RichText::new(Self::get_label(&o)).size(11.0).italics());
                                });
                            }
                        }
                    }

                    ui.add_space(10.0);
                    ui.separator();
                    ui.label(egui::RichText::new("Incoming Relations (as Object)").size(11.0).strong());
                    for t in self.manager.memory.asserted_graph.triples().flatten().chain(self.manager.memory.inferred_graph.triples().flatten()) {
                        if s_term == t.o() {
                            let s = crate::rules::extract_str(&t.s());
                            let p = crate::rules::extract_str(&t.p());
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(Self::get_label(&s)).size(11.0));
                                ui.label(egui::RichText::new(format!("({})", Self::get_label(&p))).size(10.0).color(egui::Color32::GRAY));
                                ui.label("➔ [Self]");
                            });
                        }
                    }

                    if let Some((s, p, o)) = to_delete {
                        self.manager.remove_assertion(s, p, o);
                        self.graph_needs_sync = true;
                        if self.auto_reasoning { self.manager.run_reasoning(&self.settings.enabled_rules); }
                    }
                } else {
                    ui.vertical_centered(|ui| {
                        ui.add_space(50.0);
                        ui.label(egui::RichText::new(i.select_entity).size(11.0).weak());
                    });
                }
            });
        });
    }
}
