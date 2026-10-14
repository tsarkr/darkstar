//! Modal dialogs: delete / rename confirmation, owl:imports details, unsaved-changes on exit.

use eframe::egui;
use std::collections::HashSet;
use sophia::api::prelude::*;
use sophia::api::term::matcher::Any;
use crate::rules::engine::InferenceEngine;
use crate::ui::app::DarkstarApp;

impl DarkstarApp {
    pub(crate) fn render_dialogs(&mut self, ctx: &egui::Context) {
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
                                self.manager.apply_changes(to_delete, vec![]);
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
                                let rdfs_label = crate::vocab::RDFS_LABEL.to_string();
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

    pub(crate) fn render_imports_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_imports_dialog { return; }
        let is_korean = self.is_korean();
        let mut open = true;
        let mut reload = false;
        egui::Window::new(if is_korean { "가져온 온톨로지 (owl:imports)" } else { "Imported ontologies (owl:imports)" })
            .open(&mut open)
            .default_width(560.0)
            .show(ctx, |ui| {
                if self.manager.import_statuses.is_empty() {
                    ui.label(if is_korean { "이 온톨로지에는 owl:imports가 없습니다." } else { "This ontology has no owl:imports." });
                }
                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                    for st in &self.manager.import_statuses {
                        ui.horizontal(|ui| {
                            ui.add_space(12.0 * (st.depth.saturating_sub(1)) as f32);
                            if st.ok() {
                                ui.label(egui::RichText::new("✔").color(egui::Color32::GREEN));
                            } else {
                                ui.label(egui::RichText::new("✖").color(egui::Color32::RED));
                            }
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new(&st.iri).monospace().size(11.0));
                                match &st.error {
                                    None => { ui.label(egui::RichText::new(format!("{}  ·  {} triples", st.source.as_deref().unwrap_or(""), st.triples)).size(10.0).weak()); }
                                    Some(e) => { ui.label(egui::RichText::new(e).size(10.0).color(egui::Color32::from_rgb(230, 120, 80))); }
                                }
                            });
                        });
                    }
                });
                ui.separator();
                ui.label(egui::RichText::new(if is_korean {
                    "찾는 순서: 같은 폴더의 catalog-v001.xml → file: IRI → 같은 폴더에서 IRI 마지막 이름과 같은 파일 → 웹(http/https)에서 다운로드. 가져온 공리는 추론과 탐색에만 쓰이고 저장할 때 파일에 포함되지 않습니다."
                } else {
                    "Lookup order: catalog-v001.xml in the same folder → file: IRIs → a file named after the IRI in the same folder → download over http(s). Imported axioms are used for reasoning and browsing but are not written when saving."
                }).size(10.0).weak());
                if ui.button(if is_korean { "🔄 다시 불러오기" } else { "🔄 Reload imports" }).clicked() { reload = true; }
            });
        if reload {
            self.manager.reload_imports();
            self.manager.invalidate_reasoning();
            self.manager.run_reasoning(&self.settings.enabled_rules);
        }
        self.show_imports_dialog = open;
    }

    pub(crate) fn render_exit_dialog(&mut self, ctx: &egui::Context) {
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
                            if self.save_current() {
                                self.allowed_to_close = true;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
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
