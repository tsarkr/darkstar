//! Bottom status bar: messages, file name, external-change prompt, counts, imports, format.

use eframe::egui;
use crate::ui::app::DarkstarApp;

impl DarkstarApp {
    pub(crate) fn render_status_bar(&mut self, ctx: &egui::Context) {
        let i = self.i18n();
        let is_korean = self.is_korean();

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
                        let badge = ui.add(egui::Label::new(egui::RichText::new(format!("● {}", i.inconsistent)).color(egui::Color32::RED).size(11.0)).sense(egui::Sense::click()));
                        if badge.on_hover_text(if is_korean { "클릭해서 모순 원인 보기" } else { "Click to see why" }).clicked() {
                            self.show_inconsistencies = true;
                        }
                    }
                    ui.separator();
                    
                    let (assert_count, inf_count) = self.triple_counts;
                    ui.label(format!("Triples: {} (+{} inferred)", assert_count, inf_count));

                    // owl:imports indicator
                    let statuses = &self.manager.import_statuses;
                    if !statuses.is_empty() {
                        ui.separator();
                        let failed = statuses.iter().filter(|s| !s.ok()).count();
                        let text = if failed == 0 {
                            egui::RichText::new(format!("📦 imports {} (+{} triples)", statuses.len(), self.imported_count))
                        } else {
                            egui::RichText::new(format!("⚠ imports {}/{} failed", failed, statuses.len())).color(egui::Color32::from_rgb(230, 160, 40))
                        };
                        if ui.add(egui::Label::new(text).sense(egui::Sense::click())).on_hover_text(if is_korean { "클릭해서 imports 상세 보기" } else { "Click for import details" }).clicked() {
                            self.show_imports_dialog = true;
                        }
                    }
                    ui.separator();
                    ui.label(self.manager.active_format.label());
                });
            });
        });
    }
}
