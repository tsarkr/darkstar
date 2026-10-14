use eframe::egui;
use crate::ui::app::DarkstarApp;
use crate::rules::explain::{self, Explanation, Origin, rule_description};

/// Tree levels expanded by default (deeper levels stay collapsed until clicked).
const OPEN_LEVELS: usize = 3;

impl DarkstarApp {
    /// Opens the explanation window for a triple (Darkstar term strings).
    pub fn request_explanation(&mut self, s: &str, p: &str, o: &str) {
        self.explain_target = Some((s.to_string(), p.to_string(), o.to_string()));
        self.explanation = None; // recomputed lazily for the current revision
    }

    fn triple_text(&self, s: &str, p: &str, o: &str) -> String {
        format!("{}  {}  {}", self.display_term(s), Self::get_label(p), self.display_term(o))
    }

    pub fn render_explanation_window(&mut self, ctx: &egui::Context) {
        let Some((s, p, o)) = self.explain_target.clone() else { return };
        let is_korean = self.is_korean();

        // Recompute when the target is new or the ontology changed.
        if self.explanation.is_none() || self.explanation_revision != self.manager.revision {
            self.ensure_dl_index(); // human-readable labels
            self.explanation = Some(explain::explain(&self.manager.memory, &s, &p, &o));
            self.explanation_revision = self.manager.revision;
        }
        let ex = self.explanation.take().unwrap();

        let mut open = true;
        egui::Window::new(if is_korean { "추론 근거 설명 (Explanation)" } else { "Explanation" })
            .open(&mut open)
            .default_width(620.0)
            .default_height(460.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(self.triple_text(&s, &p, &o)).strong().size(13.0));
                ui.add_space(4.0);
                match &ex.nodes[ex.root].origin {
                    Origin::Asserted => { ui.label(if is_korean { "📄 온톨로지에 직접 입력된 사실입니다 (추론 아님)." } else { "📄 Asserted in the ontology (not inferred)." }); }
                    Origin::Imported => { ui.label(if is_korean { "📦 owl:imports로 가져온 온톨로지에 있는 사실입니다." } else { "📦 Stated in an imported ontology." }); }
                    Origin::NotEntailed => { ui.label(if is_korean { "이 트리플은 온톨로지에서 성립하지 않습니다." } else { "This triple does not hold in the ontology." }); }
                    Origin::Unexplained => { ui.label(egui::RichText::new(if is_korean { "⚠ 추론된 사실이지만 유도 과정을 재구성하지 못했습니다. (버그일 수 있으니 알려주세요)" } else { "⚠ Inferred, but no derivation could be reconstructed." }).color(egui::Color32::from_rgb(230, 160, 40))); }
                    Origin::Truncated | Origin::Inferred { .. } => {
                        let just = ex.justification();
                        ui.label(egui::RichText::new(if is_korean {
                            format!("⚙ 추론 단계 {}개 · 근거 공리 {}개", ex.depth(), just.len())
                        } else {
                            format!("⚙ {} inference step(s) · {} supporting axiom(s)", ex.depth(), just.len())
                        }).size(11.0).weak());
                        ui.separator();

                        egui::ScrollArea::vertical().id_salt("explain_scroll").auto_shrink([false, false]).show(ui, |ui| {
                            ui.label(egui::RichText::new(if is_korean { "근거 공리 (Justification)" } else { "Justification" }).strong());
                            ui.label(egui::RichText::new(if is_korean {
                                "아래 공리들만으로 이 결론이 나옵니다. 하나라도 지우면 이 유도 경로는 끊깁니다."
                            } else {
                                "These axioms alone entail the conclusion; removing any one breaks this derivation."
                            }).size(10.0).weak());
                            for n in &just {
                                ui.horizontal(|ui| {
                                    ui.label(if n.origin == Origin::Imported { "📦" } else { "📄" });
                                    self.explain_term_link(ui, &n.fact.s);
                                    ui.label(egui::RichText::new(Self::get_label(&n.fact.p)).color(egui::Color32::LIGHT_BLUE));
                                    self.explain_term_link(ui, &n.fact.o);
                                });
                            }
                            ui.add_space(8.0);
                            ui.separator();
                            ui.label(egui::RichText::new(if is_korean { "유도 과정 (Derivation)" } else { "Derivation" }).strong());
                            let mut counter = 0usize;
                            self.explain_node_ui(ui, &ex, ex.root, 0, is_korean, &mut counter);
                        });
                    }
                }
            });
        self.explanation = Some(ex);
        if !open {
            self.explain_target = None;
            self.explanation = None;
        }
    }

    /// Clickable entity in the explanation view (selects it in the editor / graph).
    fn explain_term_link(&mut self, ui: &mut egui::Ui, term: &str) {
        let text = self.display_term(term);
        if term.starts_with("i:") {
            let r = ui.add(egui::Label::new(egui::RichText::new(text)).sense(egui::Sense::click()));
            if r.on_hover_text(term.trim_start_matches("i:")).clicked() {
                self.selected_uri = Some(term.to_string());
                self.graph_needs_sync = true;
            }
        } else {
            ui.label(egui::RichText::new(text).italics());
        }
    }

    fn explain_node_ui(&mut self, ui: &mut egui::Ui, ex: &Explanation, idx: usize, depth: usize, is_korean: bool, counter: &mut usize) {
        *counter += 1;
        let id = *counter;
        let node = &ex.nodes[idx];
        let text = self.triple_text(&node.fact.s, &node.fact.p, &node.fact.o);
        match &node.origin {
            Origin::Inferred { rule, premises } => {
                let header = egui::RichText::new(format!("⚙ {}   [{}]", text, rule)).color(egui::Color32::from_rgb(100, 200, 100));
                let resp = egui::CollapsingHeader::new(header)
                    .id_salt(("explain_node", id))
                    .default_open(depth < OPEN_LEVELS)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(rule_description(rule, is_korean)).size(10.0).weak());
                        for &p in premises {
                            self.explain_node_ui(ui, ex, p, depth + 1, is_korean, counter);
                        }
                    });
                resp.header_response.on_hover_text(rule_description(rule, is_korean));
            }
            Origin::Asserted => { ui.label(format!("📄 {}", text)); }
            Origin::Imported => { ui.label(format!("📦 {}", text)); }
            Origin::Truncated => { ui.label(egui::RichText::new(format!("… {} ({})", text, if is_korean { "설명이 너무 커서 생략" } else { "omitted: explanation too large" })).weak()); }
            Origin::Unexplained => { ui.label(egui::RichText::new(format!("⚠ {}", text)).color(egui::Color32::from_rgb(230, 160, 40))); }
            Origin::NotEntailed => { ui.label(egui::RichText::new(format!("✖ {}", text)).color(egui::Color32::RED)); }
        }
    }

    /// Lists reasoning inconsistencies with explanations of the conflicting facts.
    pub fn render_inconsistency_window(&mut self, ctx: &egui::Context) {
        if !self.show_inconsistencies { return; }
        let is_korean = self.is_korean();
        let mut open = true;
        let items = self.manager.memory.inconsistencies.clone();
        let mut to_explain = None;
        egui::Window::new(if is_korean { "논리적 모순 (Inconsistency)" } else { "Inconsistency" })
            .open(&mut open)
            .default_width(520.0)
            .show(ctx, |ui| {
                if items.is_empty() {
                    ui.label(if is_korean { "모순이 없습니다." } else { "No inconsistencies." });
                }
                for inc in &items {
                    ui.label(egui::RichText::new(format!("{}: {}", if is_korean { "위반 규칙" } else { "Violated rule" }, inc.rule_name)).strong());
                    ui.label(egui::RichText::new(inconsistency_description(&inc.rule_name, is_korean)).size(10.0).weak());
                    for f in &inc.conflicting_triples {
                        ui.horizontal(|ui| {
                            let (s, p, o) = (crate::memory::normalize_term(&f.s), crate::memory::normalize_term(&f.p), crate::memory::normalize_term(&f.o));
                            ui.label(self.triple_text(&s, &p, &o));
                            if ui.small_button("?").on_hover_text(if is_korean { "이 사실의 근거 보기" } else { "Explain this fact" }).clicked() {
                                to_explain = Some((s, p, o));
                            }
                        });
                    }
                    ui.separator();
                }
                ui.label(egui::RichText::new(if is_korean {
                    "모순이 있으면 추론이 중단됩니다. 근거를 확인해 원인이 되는 공리를 고친 뒤 다시 추론하세요."
                } else {
                    "Reasoning stops at an inconsistency. Check the explanation, fix the offending axiom and reason again."
                }).size(10.0).weak());
            });
        if let Some((s, p, o)) = to_explain { self.request_explanation(&s, &p, &o); }
        self.show_inconsistencies = open;
    }
}

fn inconsistency_description(rule: &str, korean: bool) -> &'static str {
    match (rule, korean) {
        ("cls-dj", true) => "서로소(disjointWith) 클래스 두 곳에 동시에 속한 개체가 있습니다.",
        ("cls-dj", false) => "An individual belongs to two disjoint classes.",
        ("cls-nothing", true) => "owl:Nothing의 개체가 있습니다 (만족 불가능한 클래스).",
        ("cls-nothing", false) => "Something is an instance of owl:Nothing.",
        ("prp-irp", true) => "비반사(irreflexive) 프로퍼티가 자기 자신을 가리킵니다.",
        ("prp-irp", false) => "An irreflexive property relates something to itself.",
        ("prp-asyp", true) => "비대칭(asymmetric) 프로퍼티가 양방향으로 쓰였습니다.",
        ("prp-asyp", false) => "An asymmetric property holds in both directions.",
        ("eq-diff1", true) => "sameAs와 differentFrom이 같은 두 개체에 동시에 성립합니다.",
        ("eq-diff1", false) => "Two individuals are both sameAs and differentFrom each other.",
        (_, true) => "OWL 2 RL 일관성 규칙 위반",
        (_, false) => "OWL 2 RL consistency rule violated",
    }
}
