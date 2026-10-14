use eframe::egui;
use crate::ui::app::DarkstarApp;
use crate::core::io::OntologyFormat;

/// Above this many asserted triples the full Turtle source is not loaded into the
/// interactive text editor: egui's TextEdit re-lays out the entire buffer, so tens of MB
/// of text would freeze the UI. A read-only preview is shown instead.
const SOURCE_EDITOR_MAX_TRIPLES: usize = 50_000;
const SOURCE_PREVIEW_LINES: usize = 500;

impl DarkstarApp {
    fn source_is_large(&self) -> bool {
        self.triple_counts.0 > SOURCE_EDITOR_MAX_TRIPLES && !self.source_force_full
    }

    pub fn render_source_editor_tab(&mut self, ui: &mut egui::Ui) {
        let i = self.i18n();
        let is_korean = self.is_korean();
        let large = self.source_is_large();

        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(i.source_editor).size(11.0).strong());

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Apply Changes button (disabled for preview mode: the preview is partial,
                    // applying it would wipe most of the ontology).
                    let apply = ui.add_enabled(
                        !large,
                        egui::Button::new(egui::RichText::new(i.apply_changes).color(egui::Color32::from_rgb(100, 200, 100))),
                    );
                    if apply.clicked() {
                        match self.manager.replace_from_string(&self.source_code_buffer, OntologyFormat::Turtle, &self.settings.enabled_rules) {
                            Ok(_) => {
                                self.source_code_error = None;
                                self.graph_needs_sync = true;
                                self.classes_cache_dirty = true;
                                self.properties_cache_dirty = true;
                                self.individuals_cache_dirty = true;
                                self.source_needs_sync = false;
                                self.status_message = Some((
                                    if is_korean { "소스 코드 변경 사항이 온톨로지에 적용되었습니다." } else { "Source code changes applied to ontology." }.to_string(),
                                    std::time::Instant::now()
                                ));
                            }
                            Err(e) => {
                                self.source_code_error = Some(format!("Parse Error: {}", e));
                            }
                        }
                    }

                    // Sync from Graph button (serialization only happens while this tab is visible)
                    if ui.button(i.sync_from_graph).clicked() || self.source_needs_sync {
                        self.source_needs_sync = false;
                        self.source_code_error = None;
                        if large {
                            self.source_code_buffer.clear();
                            self.source_preview = None; // regenerated on demand
                        } else if let Ok(turtle) = self.manager.serialize_to_string(OntologyFormat::Turtle, false) {
                            self.source_code_buffer = turtle;
                            self.source_preview = None;
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

            if large {
                ui.label(egui::RichText::new(if is_korean {
                    format!("⚠️ 온톨로지가 커서(트리플 {}개) 전체 소스를 편집기에 띄우지 않았습니다. 처음 {}줄만 읽기 전용으로 표시합니다.", self.triple_counts.0, SOURCE_PREVIEW_LINES)
                } else {
                    format!("⚠️ Ontology is large ({} triples); showing the first {} lines read-only instead of the full source.", self.triple_counts.0, SOURCE_PREVIEW_LINES)
                }).size(11.0).color(egui::Color32::from_rgb(230, 160, 40)));
                if ui.button(if is_korean { "전체 소스 편집 열기 (느릴 수 있음)" } else { "Open full source for editing (may be slow)" }).clicked() {
                    self.source_force_full = true;
                    self.source_needs_sync = true;
                }
                ui.add_space(4.0);

                if self.source_preview.is_none() {
                    let preview = self.manager.serialize_to_string(OntologyFormat::Turtle, false)
                        .map(|full| {
                            let mut out = String::new();
                            for (n, line) in full.lines().enumerate() {
                                if n >= SOURCE_PREVIEW_LINES { out.push_str("\n…"); break; }
                                out.push_str(line);
                                out.push('\n');
                            }
                            out
                        })
                        .unwrap_or_default();
                    self.source_preview = Some(preview);
                }
                let mut preview: &str = self.source_preview.as_deref().unwrap_or("");
                egui::ScrollArea::vertical().id_salt("source_preview").show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut preview)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .desired_rows(40)
                            .code_editor()
                    );
                });
                return;
            }

            // The text editor
            egui::ScrollArea::vertical().id_salt("source_editor").show(ui, |ui| {
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
