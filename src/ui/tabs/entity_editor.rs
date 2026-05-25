use eframe::egui;
use crate::ui::app::DarkstarApp;
use crate::rules::engine::InferenceEngine;
use sophia::api::graph::Graph;
use sophia::api::prelude::*;
use sophia::api::term::matcher::Any;

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
                    
                    let label = if let Some(node) = self.nodes.get(uri) {
                        node.label.clone()
                    } else {
                        Self::get_label(uri)
                    };

                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&label).size(14.0).strong());
                        if ui.button(egui::RichText::new("🗑").size(12.0)).on_hover_text(i.delete_entity).clicked() {
                            self.confirm_delete_uri = Some(uri.clone());
                        }
                    });

                    // Quick Edit: Label
                    let mut current_label = label.clone();
                    ui.horizontal(|ui| {
                        ui.label(format!("{}:", i.object_label));
                        if ui.add(egui::TextEdit::singleline(&mut current_label).desired_width(200.0)).lost_focus() && current_label != label {
                            // Update label (rdfs:label)
                            let rdfs_label = "http://www.w3.org/2000/01/rdf-schema#label".to_string();
                            // Remove old labels
                            let s_term = InferenceEngine::make_term(uri);
                            let p_term = InferenceEngine::make_term(&rdfs_label);
                            let mut to_remove = Vec::new();
                            for t in self.manager.memory.asserted_graph.triples_matching(Some(&s_term), Some(&p_term), Any).flatten() {
                                to_remove.push(crate::rules::extract_str(&t.o()));
                            }
                            for old_val in to_remove {
                                self.manager.remove_assertion(uri.clone(), rdfs_label.clone(), old_val);
                            }
                            // Add new label
                            self.manager.add_assertion(uri.clone(), rdfs_label, format!("l:{}", current_label));
                            self.graph_needs_sync = true;
                        }
                    });

                    ui.add_space(5.0);
                    
                    // Property Characteristics
                    let owl_functional = "http://www.w3.org/2002/07/owl#FunctionalProperty".to_string();
                    let owl_transitive = "http://www.w3.org/2002/07/owl#TransitiveProperty".to_string();
                    let owl_symmetric = "http://www.w3.org/2002/07/owl#SymmetricProperty".to_string();
                    let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string();
                    
                    let s_term = InferenceEngine::make_term(uri);
                    let rdf_type_term = InferenceEngine::make_term(&rdf_type);
                    
                    let is_functional = self.manager.memory.asserted_graph.contains(&s_term, &rdf_type_term, &InferenceEngine::make_term(&owl_functional)).unwrap();
                    let is_transitive = self.manager.memory.asserted_graph.contains(&s_term, &rdf_type_term, &InferenceEngine::make_term(&owl_transitive)).unwrap();
                    let is_symmetric = self.manager.memory.asserted_graph.contains(&s_term, &rdf_type_term, &InferenceEngine::make_term(&owl_symmetric)).unwrap();

                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut is_functional.clone(), "Functional").changed() {
                            if !is_functional { self.manager.add_assertion(uri.clone(), rdf_type.clone(), owl_functional); }
                            else { self.manager.remove_assertion(uri.clone(), rdf_type.clone(), owl_functional); }
                        }
                        if ui.checkbox(&mut is_transitive.clone(), "Transitive").changed() {
                            if !is_transitive { self.manager.add_assertion(uri.clone(), rdf_type.clone(), owl_transitive); }
                            else { self.manager.remove_assertion(uri.clone(), rdf_type.clone(), owl_transitive); }
                        }
                        if ui.checkbox(&mut is_symmetric.clone(), "Symmetric").changed() {
                            if !is_symmetric { self.manager.add_assertion(uri.clone(), rdf_type.clone(), owl_symmetric); }
                            else { self.manager.remove_assertion(uri.clone(), rdf_type.clone(), owl_symmetric); }
                        }
                    });

                    ui.add_space(5.0);
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
                    let mut to_modify = None;
                    
                    let s_term = InferenceEngine::make_term(uri);
                    
                    // Show Asserted
                    for t in self.manager.memory.asserted_graph.triples().flatten() {
                        if s_term == t.s() {
                            let p = crate::rules::extract_str(&t.p());
                            let o = crate::rules::extract_str(&t.o());
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(Self::get_label(&p)).size(11.0).color(egui::Color32::LIGHT_BLUE));
                                ui.label("➔");
                                
                                let edit_id = ui.make_persistent_id(format!("edit_{}_{}_{}", uri, p, o));
                                let mut is_editing = ui.data_mut(|d| d.get_temp::<bool>(edit_id).unwrap_or(false));
                                
                                if is_editing {
                                    let mut edit_val = ui.data_mut(|d| d.get_temp::<String>(edit_id.with("val")).unwrap_or_else(|| {
                                        if o.starts_with("l:") { o[2..].to_string() } else { o.clone() }
                                    }));
                                    
                                    let response = ui.add(egui::TextEdit::singleline(&mut edit_val).desired_width(120.0));
                                    
                                    if ui.button("💾").on_hover_text("Save").clicked() || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                                        let final_val = if o.starts_with("l:") { format!("l:{}", edit_val) } else { edit_val.clone() };
                                        if final_val != o {
                                            to_modify = Some((uri.clone(), p.clone(), o.clone(), final_val));
                                        }
                                        is_editing = false;
                                    }
                                    if ui.button("❌").on_hover_text("Cancel").clicked() || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape))) {
                                        is_editing = false;
                                    }
                                    
                                    ui.data_mut(|d| {
                                        d.insert_temp(edit_id, is_editing);
                                        d.insert_temp(edit_id.with("val"), edit_val);
                                    });
                                } else {
                                    ui.label(egui::RichText::new(Self::get_label(&o)).size(11.0));
                                    if ui.button("✏").on_hover_text("Edit").clicked() {
                                        ui.data_mut(|d| {
                                            d.insert_temp(edit_id, true);
                                            d.insert_temp(edit_id.with("val"), if o.starts_with("l:") { o[2..].to_string() } else { o.clone() });
                                        });
                                    }
                                    if ui.add(egui::Button::new(egui::RichText::new("x").size(11.0))).on_hover_text("Delete").clicked() {
                                        to_delete = Some((uri.clone(), p.clone(), o.clone()));
                                    }
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
                    for t in self.manager.memory.asserted_graph.triples_matching(Any, Any, Some(&s_term)).flatten().chain(
                        self.manager.memory.inferred_graph.triples_matching(Any, Any, Some(&s_term)).flatten()
                    ) {
                        let s = crate::rules::extract_str(&t.s());
                        let p = crate::rules::extract_str(&t.p());
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(Self::get_label(&s)).size(11.0));
                            ui.label(egui::RichText::new(format!("({})", Self::get_label(&p))).size(10.0).color(egui::Color32::GRAY));
                            ui.label("➔ [Self]");
                        });
                    }

                    if let Some((s, p, o)) = to_delete {
                        self.manager.remove_assertion(s, p, o);
                        self.graph_needs_sync = true;
                        if self.auto_reasoning { self.manager.run_reasoning(&self.settings.enabled_rules); }
                    }
                    
                    if let Some((s, p, old_o, new_o)) = to_modify {
                        self.manager.remove_assertion(s.clone(), p.clone(), old_o);
                        self.manager.add_assertion(s, p, new_o);
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
