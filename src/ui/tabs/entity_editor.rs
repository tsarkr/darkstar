use eframe::egui;
use crate::ui::app::DarkstarApp;
use crate::rules::engine::InferenceEngine;
use sophia::api::graph::Graph;
use sophia::api::prelude::*;
use sophia::api::term::matcher::Any;

/// Rows rendered per relation section (and added per "show more" click). Hub entities
/// (e.g. a class with 100k+ instances) would otherwise lay out every edge on every frame.
pub const RELATION_ROWS_STEP: usize = 200;

/// "Show more" row; returns true when clicked.
fn show_more_button(ui: &mut egui::Ui, shown: usize) -> bool {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("… {}개 표시 중", shown)).size(10.0).weak());
        ui.button(egui::RichText::new(format!("➕ {}개 더 보기", RELATION_ROWS_STEP)).size(10.0)).clicked()
    }).inner
}

impl DarkstarApp {
    pub fn render_entity_editor_tab(&mut self, ui: &mut egui::Ui) {
        let i = self.i18n();
        // Reset "show more" limits when a different entity is selected.
        if self.relation_limits_uri != self.selected_uri {
            self.relation_limits_uri = self.selected_uri.clone();
            self.relation_limits = [RELATION_ROWS_STEP; 3];
        }
        let limits = self.relation_limits;
        let mut more_section: Option<usize> = None;
        let mut explain_request: Option<(String, String, String)> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(i.property_editor).size(11.0).strong());
                ui.separator();
                
                if let Some(uri_owned) = self.selected_uri.clone() {
                    let uri = &uri_owned;
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

                    // Protégé-style description: annotations, axioms by entity kind, IRI rename.
                    self.render_description(ui, uri);

                    ui.add_space(5.0);
                    ui.separator();

                    // Add New Relation
                    ui.label(egui::RichText::new(i.add_relation).size(11.0).strong());
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.new_prop_predicate).hint_text(i.predicate).font(egui::FontId::proportional(11.0)).desired_width(120.0));
                    });
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.new_prop_value).hint_text(i.object_label).font(egui::FontId::proportional(11.0)).desired_width(120.0));
                        if ui.button(egui::RichText::new(format!("✚ {}", i.add)).size(11.0)).clicked()
                            && !self.new_prop_predicate.is_empty() && !self.new_prop_value.is_empty() {
                                self.manager.add_assertion(uri.clone(), self.new_prop_predicate.clone(), self.new_prop_value.clone());
                                self.new_prop_predicate.clear();
                                self.new_prop_value.clear();
                                self.graph_needs_sync = true;
                                if self.auto_reasoning { self.manager.run_reasoning(&self.settings.enabled_rules); }
                            }
                    });
                    ui.add_space(10.0);
                    ui.separator();
                    
                    // Existing Assertions (QUERY DIRECTLY FROM MANAGER)
                    ui.label(egui::RichText::new(i.property_assertions).size(11.0).strong());
                    let mut to_delete = None;
                    let mut to_modify = None;
                    
                    let s_term = InferenceEngine::make_term(uri);
                    
                    // Show Asserted (indexed by subject instead of scanning the whole graph)
                    let mut shown = 0usize;
                    for t in self.manager.memory.asserted_graph.triples_matching(Some(&s_term), Any, Any).flatten() {
                        shown += 1;
                        if shown > limits[0] {
                            if show_more_button(ui, limits[0]) { more_section = Some(0); }
                            break;
                        }
                        {
                            let p = crate::rules::extract_str(&t.p());
                            let o = crate::rules::extract_str(&t.o());
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(Self::get_label(&p)).size(11.0).color(egui::Color32::LIGHT_BLUE));
                                ui.label("➔");
                                
                                let edit_id = ui.make_persistent_id(format!("edit_{}_{}_{}", uri, p, o));
                                let mut is_editing = ui.data_mut(|d| d.get_temp::<bool>(edit_id).unwrap_or(false));
                                
                                if is_editing {
                                    let mut edit_val = ui.data_mut(|d| d.get_temp::<String>(edit_id.with("val")).unwrap_or_else(|| {
                                        o.strip_prefix("l:").unwrap_or(&o).to_string()
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
                                            d.insert_temp(edit_id.with("val"), o.strip_prefix("l:").unwrap_or(&o).to_string());
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
                    let mut shown = 0usize;
                    for t in self.manager.memory.inferred_graph.triples_matching(Some(&s_term), Any, Any).flatten() {
                        shown += 1;
                        if shown > limits[1] {
                            if show_more_button(ui, limits[1]) { more_section = Some(1); }
                            break;
                        }
                        {
                            if !self.manager.memory.asserted_graph.contains(t.s(), t.p(), t.o()).unwrap() {
                                let p = crate::rules::extract_str(&t.p());
                                let o = crate::rules::extract_str(&t.o());
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(Self::get_label(&p)).size(11.0).color(egui::Color32::from_rgb(100, 200, 100)));
                                    ui.label("➔");
                                    ui.label(egui::RichText::new(Self::get_label(&o)).size(11.0).italics());
                                    // Protégé-style "?" : why was this inferred?
                                    if ui.small_button("?").on_hover_text("추론 근거 보기 (Explain)").clicked() {
                                        explain_request = Some((uri.clone(), p.clone(), o.clone()));
                                    }
                                });
                            }
                        }
                    }

                    // Imported (owl:imports) axioms about this entity: read-only, like Protégé.
                    if self.manager.memory.imported_graph.triples_matching(Some(&s_term), Any, Any).flatten().next().is_some() {
                        ui.add_space(5.0);
                        ui.label(egui::RichText::new("가져온 공리 (Imported, read-only)").size(10.0).weak());
                        for t in self.manager.memory.imported_graph.triples_matching(Some(&s_term), Any, Any).flatten().take(RELATION_ROWS_STEP) {
                            let p = crate::rules::extract_str(&t.p());
                            let o = crate::rules::extract_str(&t.o());
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(Self::get_label(&p)).size(11.0).color(egui::Color32::from_rgb(180, 150, 230)));
                                ui.label("➔");
                                ui.label(egui::RichText::new(Self::get_label(&o)).size(11.0));
                            });
                        }
                    }

                    ui.add_space(10.0);
                    ui.separator();
                    ui.label(egui::RichText::new("Incoming Relations (as Object)").size(11.0).strong());
                    let mut shown = 0usize;
                    for t in self.manager.memory.asserted_graph.triples_matching(Any, Any, Some(&s_term)).flatten()
                        .chain(self.manager.memory.inferred_graph.triples_matching(Any, Any, Some(&s_term)).flatten())
                        .chain(self.manager.memory.imported_graph.triples_matching(Any, Any, Some(&s_term)).flatten())
                    {
                        shown += 1;
                        if shown > limits[2] {
                            if show_more_button(ui, limits[2]) { more_section = Some(2); }
                            break;
                        }
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
        if let Some(k) = more_section {
            self.relation_limits[k] += RELATION_ROWS_STEP;
        }
        if let Some((s, p, o)) = explain_request {
            self.request_explanation(&s, &p, &o);
        }
    }
}
