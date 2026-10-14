use std::time::Duration;
use eframe::egui;
use egui_extras::{Column, TableBuilder};
use crate::ui::app::DarkstarApp;
use crate::query::sparql::{self, SparqlResult};
use crate::query::dlquery;

const SPARQL_MAX_ROWS: usize = 10_000;
const SPARQL_TIMEOUT: Duration = Duration::from_secs(30);

pub const SPARQL_DEFAULT: &str = "# rdf:, rdfs:, owl:, xsd: 접두어는 선언 없이 쓸 수 있습니다.\nSELECT ?class (COUNT(?x) AS ?n)\nWHERE { ?x a ?class }\nGROUP BY ?class\nORDER BY DESC(?n)\nLIMIT 50";

const SPARQL_EXAMPLES: &[(&str, &str)] = &[
    ("클래스별 개체 수 (instances per class)", SPARQL_DEFAULT),
    ("라벨로 찾기 (find by label)", "SELECT ?x ?label\nWHERE {\n  ?x rdfs:label ?label .\n  FILTER(CONTAINS(STR(?label), \"강명하\"))\n}\nLIMIT 100"),
    ("클래스 계층 (class hierarchy)", "SELECT ?sub ?super\nWHERE { ?sub rdfs:subClassOf ?super . FILTER(?sub != ?super) }\nORDER BY ?super"),
    ("프로퍼티 사용 빈도 (property usage)", "SELECT ?p (COUNT(*) AS ?n)\nWHERE { ?s ?p ?o }\nGROUP BY ?p\nORDER BY DESC(?n)"),
    ("인물별 활동 수 (activities per actor)", "PREFIX crm: <http://www.cidoc-crm.org/cidoc-crm/>\nSELECT ?actor ?name (COUNT(?act) AS ?n)\nWHERE {\n  ?act crm:P14_carried_out_by ?actor .\n  OPTIONAL { ?actor rdfs:label ?name }\n}\nGROUP BY ?actor ?name\nORDER BY DESC(?n)\nLIMIT 50"),
    ("라벨 없는 개체 (entities without label)", "SELECT DISTINCT ?x\nWHERE {\n  ?x a ?c .\n  FILTER NOT EXISTS { ?x rdfs:label ?l }\n}\nLIMIT 200"),
];

const DL_HELP: &str = "문법 (Manchester syntax):\n\
  A and B · A that B · A or B · not A · ( … ) · { a, b }\n\
  p some C · p only C · p value a · p value \"텍스트\"@ko · p value 1919\n\
  p min 2 C · p max 1 · p exactly 1 C · inverse p some C · p Self\n\
  p some xsd:integer[>= 1900, < 1946]\n\
이름: E5_Event · crm:E5_Event · <전체 IRI> · '라벨' (공백이 있으면 작은따옴표)\n\
예: E7_Activity and P14_carried_out_by value '손병희'";

impl DarkstarApp {
    /// (Re)builds the DL-query name index when the ontology changed.
    pub fn ensure_dl_index(&mut self) {
        if self.dl_index.is_some() && self.dl_index_revision == self.manager.revision { return; }
        self.dl_index = Some(dlquery::DlIndex::build(&self.manager.memory.main_graph));
        self.dl_index_revision = self.manager.revision;
    }

    /// Human-readable text for a result cell: rdfs:label when known, else the short form.
    pub(crate) fn display_term(&self, term: &str) -> String {
        if let Some(l) = self.dl_index.as_ref().and_then(|i| i.label_of(term)) {
            return l.clone();
        }
        Self::get_label(term)
    }

    /// A clickable result cell: clicking an IRI selects it in the editor / graph.
    fn term_cell(&mut self, ui: &mut egui::Ui, term: &str) {
        let text = self.display_term(term);
        let full = term.strip_prefix("i:").unwrap_or(term);
        if term.starts_with("i:") {
            let r = ui.add(egui::Label::new(egui::RichText::new(text).color(egui::Color32::from_rgb(110, 170, 240))).sense(egui::Sense::click()).truncate());
            if r.on_hover_text(full).clicked() {
                self.selected_uri = Some(term.to_string());
                self.graph_needs_sync = true;
            }
        } else {
            ui.add(egui::Label::new(text).truncate()).on_hover_text(full);
        }
    }

    pub fn render_sparql_tab(&mut self, ui: &mut egui::Ui) {
        let is_korean = self.is_korean();
        let mut run = false;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("SPARQL").size(11.0).strong());
            egui::ComboBox::from_id_salt("sparql_examples")
                .selected_text(if is_korean { "예제 쿼리" } else { "Examples" })
                .show_ui(ui, |ui| {
                    for (name, q) in SPARQL_EXAMPLES {
                        if ui.selectable_label(false, *name).clicked() { self.sparql_query = q.to_string(); }
                    }
                });
            ui.checkbox(&mut self.sparql_include_inferred, if is_korean { "추론 결과 포함" } else { "Include inferences" })
                .on_hover_text(if is_korean { "끄면 직접 입력한 트리플(+imports)만 조회합니다" } else { "Off: only asserted (+imported) triples" });
            if ui.button(egui::RichText::new(if is_korean { "▶ 실행 (⌘/Ctrl+Enter)" } else { "▶ Run (⌘/Ctrl+Enter)" }).color(egui::Color32::from_rgb(100, 200, 100))).clicked() { run = true; }
        });

        let editor = egui::ScrollArea::vertical().id_salt("sparql_editor").max_height(170.0).show(ui, |ui| {
            ui.add(egui::TextEdit::multiline(&mut self.sparql_query).code_editor().desired_rows(8).desired_width(f32::INFINITY))
        }).inner;
        if editor.has_focus() && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter)) { run = true; }

        if run {
            self.ensure_dl_index(); // labels for display
            let union;
            let graph = if self.sparql_include_inferred {
                &self.manager.memory.main_graph
            } else if !self.manager.memory.has_imports() {
                &self.manager.memory.asserted_graph
            } else {
                union = self.manager.memory.asserted_plus_imported(); // no inferences
                &union
            };
            self.sparql_result = Some(sparql::run_query(graph, &self.sparql_query, SPARQL_MAX_ROWS, SPARQL_TIMEOUT));
        }
        ui.separator();

        let Some(result) = self.sparql_result.take() else {
            ui.label(egui::RichText::new(if is_korean { "쿼리를 입력하고 실행하세요." } else { "Write a query and run it." }).weak());
            return;
        };
        match &result {
            Err(e) => { ui.label(egui::RichText::new(e).color(egui::Color32::from_rgb(230, 100, 90)).monospace()); }
            Ok(out) => {
                let count = match &out.result { SparqlResult::Select { rows, .. } => rows.len(), SparqlResult::Graph(t) => t.len(), SparqlResult::Ask(_) => 1 };
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("{} {} · {:.0?}", count, if is_korean { "행" } else { "rows" }, out.elapsed)).size(11.0).weak());
                    if out.truncated {
                        ui.label(egui::RichText::new(format!("⚠ {}행까지만 표시 (LIMIT으로 줄이세요)", SPARQL_MAX_ROWS)).size(11.0).color(egui::Color32::from_rgb(230, 160, 40)));
                    }
                    if ui.small_button(if is_korean { "📋 TSV 복사" } else { "📋 Copy TSV" }).clicked() {
                        ui.ctx().copy_text(sparql_tsv(&out.result));
                    }
                });
                match &out.result {
                    SparqlResult::Ask(b) => { ui.heading(if *b { "true" } else { "false" }); }
                    SparqlResult::Select { vars, rows } => {
                        let headers: Vec<String> = vars.iter().map(|v| format!("?{}", v)).collect();
                        self.result_table(ui, "sparql_table", &headers, rows.len(), |app, ui, r, c| {
                            if let Some(t) = &rows[r][c] { app.term_cell(ui, t); }
                        });
                    }
                    SparqlResult::Graph(triples) => {
                        let headers = vec!["subject".to_string(), "predicate".to_string(), "object".to_string()];
                        self.result_table(ui, "sparql_graph_table", &headers, triples.len(), |app, ui, r, c| app.term_cell(ui, &triples[r][c]));
                    }
                }
            }
        }
        self.sparql_result = Some(result);
    }

    /// Virtualized result table (only visible rows are laid out).
    fn result_table(&mut self, ui: &mut egui::Ui, id: &str, headers: &[String], n_rows: usize, mut cell: impl FnMut(&mut Self, &mut egui::Ui, usize, usize)) {
        if headers.is_empty() { return; }
        let row_h = ui.spacing().interact_size.y;
        TableBuilder::new(ui)
            .id_salt(id)
            .striped(true)
            .resizable(true)
            .auto_shrink([false, false])
            .columns(Column::initial(180.0).at_least(40.0).clip(true), headers.len().saturating_sub(1))
            .column(Column::remainder().at_least(40.0).clip(true))
            .header(row_h, |mut h| {
                for name in headers { h.col(|ui| { ui.strong(name); }); }
            })
            .body(|body| {
                body.rows(row_h, n_rows, |mut row| {
                    let r = row.index();
                    for c in 0..headers.len() { row.col(|ui| cell(self, ui, r, c)); }
                });
            });
    }

    pub fn render_dl_query_tab(&mut self, ui: &mut egui::Ui) {
        let is_korean = self.is_korean();
        let mut run = false;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("DL Query").size(11.0).strong());
            ui.label(egui::RichText::new("ⓘ").size(11.0)).on_hover_text(DL_HELP);
        });
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut self.dl_query)
                .hint_text("E7_Activity and P14_carried_out_by value '손병희'")
                .desired_width(ui.available_width() - 90.0)
                .font(egui::TextStyle::Monospace));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) { run = true; }
            if ui.button(egui::RichText::new(if is_korean { "▶ 실행" } else { "▶ Execute" }).color(egui::Color32::from_rgb(100, 200, 100))).clicked() { run = true; }
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.dl_show[0], if is_korean { "개체" } else { "Instances" });
            ui.checkbox(&mut self.dl_show[1], if is_korean { "상위 클래스" } else { "Superclasses" });
            ui.checkbox(&mut self.dl_show[2], if is_korean { "동등 클래스" } else { "Equivalent" });
            ui.checkbox(&mut self.dl_show[3], if is_korean { "하위 클래스" } else { "Subclasses" });
        });

        if run {
            self.ensure_dl_index();
            let idx = self.dl_index.as_ref().unwrap();
            self.dl_result = Some(dlquery::run(&self.manager.memory.main_graph, idx, &self.dl_query));
        }
        ui.separator();

        let Some(result) = self.dl_result.take() else {
            ui.label(egui::RichText::new(DL_HELP).size(11.0).weak());
            return;
        };
        match &result {
            Err(e) => { ui.label(egui::RichText::new(e).color(egui::Color32::from_rgb(230, 100, 90))); }
            Ok(res) => {
                if res.closed_world {
                    ui.label(egui::RichText::new(if is_korean {
                        "ⓘ not / only / max / exactly는 '명시되거나 추론되지 않은 것은 거짓'으로 보는 닫힌 세계 방식으로 계산했습니다. HermiT 같은 OWL DL 추론기 결과와 다를 수 있습니다."
                    } else {
                        "ⓘ not / only / max / exactly were answered under a closed-world reading; an OWL DL reasoner (e.g. HermiT) may answer differently."
                    }).size(10.0).color(egui::Color32::from_rgb(230, 160, 40)));
                }
                ui.label(egui::RichText::new(format!("{:.0?}", res.elapsed)).size(10.0).weak());
                egui::ScrollArea::vertical().id_salt("dl_results").auto_shrink([false, false]).show(ui, |ui| {
                    let sections: [(usize, &str, &str, Option<&Vec<String>>); 4] = [
                        (1, "상위 클래스", "Superclasses", res.superclasses.as_ref()),
                        (2, "동등 클래스", "Equivalent classes", res.equivalent.as_ref()),
                        (3, "하위 클래스", "Subclasses", res.subclasses.as_ref()),
                        (0, "개체", "Instances", Some(&res.instances)),
                    ];
                    for (k, ko, en, items) in sections {
                        if !self.dl_show[k] { continue; }
                        let title = if is_korean { ko } else { en };
                        match items {
                            None => {
                                ui.label(egui::RichText::new(format!("{}: {}", title, if is_korean {
                                    "복합 클래스 식의 상·하위 클래스 계산은 OWL DL 추론기가 필요해 지원하지 않습니다"
                                } else {
                                    "needs an OWL DL reasoner for complex expressions (not supported)"
                                })).size(10.0).weak());
                            }
                            Some(items) => {
                                egui::CollapsingHeader::new(egui::RichText::new(format!("{} ({})", title, items.len())).strong())
                                    .id_salt(("dl_section", k))
                                    .default_open(true)
                                    .show(ui, |ui| {
                                        let row_h = ui.spacing().interact_size.y;
                                        let height = (items.len() as f32 * (row_h + ui.spacing().item_spacing.y)).min(320.0);
                                        egui::ScrollArea::vertical().id_salt(("dl_list", k)).max_height(height).auto_shrink([false, true])
                                            .show_rows(ui, row_h, items.len(), |ui, range| {
                                                for item in &items[range] { self.term_cell(ui, item); }
                                            });
                                    });
                            }
                        }
                    }
                });
            }
        }
        self.dl_result = Some(result);
    }
}

fn sparql_tsv(r: &SparqlResult) -> String {
    let clean = |s: &str| s.replace(['\t', '\n'], " ");
    let term = |t: &str| clean(t.strip_prefix("i:").or_else(|| t.strip_prefix("l:")).unwrap_or(t));
    match r {
        SparqlResult::Ask(b) => b.to_string(),
        SparqlResult::Select { vars, rows } => {
            let mut out = vars.join("\t");
            for row in rows {
                out.push('\n');
                out.push_str(&row.iter().map(|c| c.as_deref().map(term).unwrap_or_default()).collect::<Vec<_>>().join("\t"));
            }
            out
        }
        SparqlResult::Graph(ts) => ts.iter().map(|t| t.iter().map(|x| term(x)).collect::<Vec<_>>().join("\t")).collect::<Vec<_>>().join("\n"),
    }
}
