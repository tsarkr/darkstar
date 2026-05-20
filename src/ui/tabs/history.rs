use eframe::egui;
use crate::ui::app::DarkstarApp;
use crate::core::history::DarkstarEvent;

impl DarkstarApp {
    pub fn render_history_tab(&mut self, ui: &mut egui::Ui) {
        let i = self.i18n();
        let is_korean = self.settings.language == crate::core::settings::Language::Korean;

        ui.vertical(|ui| {
            // Header actions
            ui.horizontal(|ui| {
                let undo_empty = self.manager.history.is_undo_empty();
                let redo_empty = self.manager.history.is_redo_empty();

                if ui.add_enabled(!undo_empty, egui::Button::new(format!("⟲ {}", i.undo))).clicked() {
                    self.manager.undo();
                    self.graph_needs_sync = true;
                }

                if ui.add_enabled(!redo_empty, egui::Button::new(format!("⟳ {}", i.redo))).clicked() {
                    self.manager.redo();
                    self.graph_needs_sync = true;
                }

                ui.separator();

                if ui.add_enabled(!undo_empty || !redo_empty, egui::Button::new(format!("🗑 {}", i.delete_selected))).clicked() {
                    self.manager.history.clear();
                }
            });

            ui.separator();
            ui.add_space(5.0);

            egui::ScrollArea::vertical().show(ui, |ui| {
                let undo_stack = self.manager.history.get_undo_stack();
                let redo_stack = self.manager.history.get_redo_stack();

                if undo_stack.is_empty() && redo_stack.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label(egui::RichText::new(if is_korean { "변경 이력이 없습니다." } else { "No history available." }).weak());
                    });
                    return;
                }

                ui.vertical(|ui| {
                    // Active changes
                    if !undo_stack.is_empty() {
                        ui.label(egui::RichText::new(if is_korean { "적용된 변경 사항" } else { "Applied Changes" }).strong());
                        ui.add_space(3.0);
                        
                        for (idx, event) in undo_stack.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(format!("{}.", idx + 1)).weak());
                                ui.label(egui::RichText::new("✓").color(egui::Color32::GREEN));
                                self.render_history_event(ui, event, is_korean, true);
                            });
                        }
                        ui.add_space(10.0);
                    }

                    // Undone changes (can be redone)
                    if !redo_stack.is_empty() {
                        ui.label(egui::RichText::new(if is_korean { "취소된 변경 사항 (다시 실행 가능)" } else { "Undone Changes (Redoable)" }).strong().color(egui::Color32::GRAY));
                        ui.add_space(3.0);

                        for (idx, event) in redo_stack.iter().rev().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(format!("{}.", idx + 1)).weak());
                                ui.label(egui::RichText::new("⟳").color(egui::Color32::GRAY));
                                self.render_history_event(ui, event, is_korean, false);
                            });
                        }
                    }
                });
            });
        });
    }

    fn render_history_event(&self, ui: &mut egui::Ui, event: &DarkstarEvent, is_korean: bool, active: bool) {
        let label_color = if active {
            if self.settings.theme_dark { egui::Color32::WHITE } else { egui::Color32::BLACK }
        } else {
            egui::Color32::GRAY
        };

        match event {
            DarkstarEvent::AxiomAdded { s, p, o } => {
                let s_lbl = Self::get_label(s);
                let p_lbl = Self::get_label(p);
                let o_lbl = Self::get_label(o);
                let text = if is_korean {
                    format!("추가: {} ➔ {} ➔ {}", s_lbl, p_lbl, o_lbl)
                } else {
                    format!("Add: {} ➔ {} ➔ {}", s_lbl, p_lbl, o_lbl)
                };
                ui.label(egui::RichText::new(text).color(label_color));
            }
            DarkstarEvent::AxiomRemoved { s, p, o } => {
                let s_lbl = Self::get_label(s);
                let p_lbl = Self::get_label(p);
                let o_lbl = Self::get_label(o);
                let text = if is_korean {
                    format!("제거: {} ➔ {} ➔ {}", s_lbl, p_lbl, o_lbl)
                } else {
                    format!("Remove: {} ➔ {} ➔ {}", s_lbl, p_lbl, o_lbl)
                };
                ui.label(egui::RichText::new(text).color(label_color));
            }
            DarkstarEvent::Batch(events) => {
                let text = if is_korean {
                    format!("일괄 작업 ({}개 항목)", events.len())
                } else {
                    format!("Batch Action ({} items)", events.len())
                };
                ui.collapsing(egui::RichText::new(text).color(label_color), |ui| {
                    for e in events {
                        ui.horizontal(|ui| {
                            ui.add_space(10.0);
                            self.render_history_event(ui, e, is_korean, active);
                        });
                    }
                });
            }
        }
    }
}
