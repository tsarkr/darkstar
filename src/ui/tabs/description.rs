//! Protégé-style "Description" panel for the selected entity: annotations, and per kind
//! subclass / equivalent / disjoint axioms (classes), characteristics / domain / range /
//! inverse / super-properties (properties), types / same / different (individuals), plus
//! IRI renaming. Class expressions are typed in Manchester syntax.

use eframe::egui;
use crate::ui::app::DarkstarApp;
use crate::core::owl_edit::{self as ox, EntityKind, Spo};

/// How the value typed into an axiom section is interpreted.
#[derive(Clone, Copy, PartialEq)]
enum Input {
    /// Any class expression (Manchester) — complex expressions become blank-node structures.
    Expression,
    /// A single named entity (class, property, individual or datatype).
    Entity,
}

struct Section {
    key: &'static str,
    title_ko: &'static str,
    title_en: &'static str,
    predicate: String,
    input: Input,
    /// Also list `(x pred uri)` (symmetric axioms such as equivalentClass, sameAs).
    symmetric: bool,
    hint: &'static str,
}

impl DarkstarApp {
    /// Name index for parsing; rebuilt only when needed for a parse (not every frame).
    fn parse_index(&mut self) -> &crate::query::dlquery::DlIndex {
        self.ensure_dl_index();
        self.dl_index.as_ref().unwrap()
    }

    pub fn render_description(&mut self, ui: &mut egui::Ui, uri: &str) {
        let is_korean = self.is_korean();
        let g = &self.manager.memory.main_graph;
        let kind = ox::entity_kind(g, uri);
        let mut changes: Vec<(Vec<Spo>, Vec<Spo>)> = Vec::new();

        let kind_label = match (kind, is_korean) {
            (EntityKind::Class, true) => "클래스", (EntityKind::Class, false) => "Class",
            (EntityKind::ObjectProperty, true) => "객체 프로퍼티", (EntityKind::ObjectProperty, false) => "Object property",
            (EntityKind::DataProperty, true) => "데이터 프로퍼티", (EntityKind::DataProperty, false) => "Data property",
            (EntityKind::AnnotationProperty, true) => "주석 프로퍼티", (EntityKind::AnnotationProperty, false) => "Annotation property",
            (EntityKind::Datatype, true) => "데이터 타입", (EntityKind::Datatype, false) => "Datatype",
            (EntityKind::Individual, true) => "개체", (EntityKind::Individual, false) => "Individual",
            (EntityKind::Unknown, true) => "엔티티", (EntityKind::Unknown, false) => "Entity",
        };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(kind_label).size(10.0).color(egui::Color32::from_rgb(160, 140, 230)));
            if ui.small_button(if is_korean { "✏ IRI 변경" } else { "✏ Rename IRI" }).clicked() {
                self.rename_buffer = Some(uri.trim_start_matches("i:").to_string());
            }
        });
        self.render_rename(ui, uri, is_korean, &mut changes);

        // --- Annotations -------------------------------------------------
        self.render_annotations(ui, uri, is_korean, &mut changes);

        // --- Kind-specific sections -------------------------------------
        let sections: Vec<Section> = match kind {
            EntityKind::Class => vec![
                Section { key: "sco", title_ko: "상위 클래스 (SubClass Of)", title_en: "SubClass Of", predicate: ox::rdfs("subClassOf"), input: Input::Expression, symmetric: false, hint: "Person and P14_carried_out_by some E39_Actor" },
                Section { key: "eqc", title_ko: "동등 클래스 (Equivalent To)", title_en: "Equivalent To", predicate: ox::owl("equivalentClass"), input: Input::Expression, symmetric: true, hint: "E7_Activity and P14_carried_out_by value '손병희'" },
                Section { key: "dj", title_ko: "서로소 클래스 (Disjoint With)", title_en: "Disjoint With", predicate: ox::owl("disjointWith"), input: Input::Expression, symmetric: true, hint: "E39_Actor" },
            ],
            EntityKind::ObjectProperty => vec![
                Section { key: "dom", title_ko: "정의역 (Domain)", title_en: "Domains", predicate: ox::rdfs("domain"), input: Input::Expression, symmetric: false, hint: "E7_Activity" },
                Section { key: "rng", title_ko: "치역 (Range)", title_en: "Ranges", predicate: ox::rdfs("range"), input: Input::Expression, symmetric: false, hint: "E39_Actor" },
                Section { key: "inv", title_ko: "역관계 (Inverse Of)", title_en: "Inverse Of", predicate: ox::owl("inverseOf"), input: Input::Entity, symmetric: true, hint: "P14i_performed" },
                Section { key: "spo", title_ko: "상위 프로퍼티 (SubProperty Of)", title_en: "SubProperty Of", predicate: ox::rdfs("subPropertyOf"), input: Input::Entity, symmetric: false, hint: "P11_had_participant" },
                Section { key: "eqp", title_ko: "동등 프로퍼티 (Equivalent To)", title_en: "Equivalent To", predicate: ox::owl("equivalentProperty"), input: Input::Entity, symmetric: true, hint: "" },
            ],
            EntityKind::DataProperty => vec![
                Section { key: "dom", title_ko: "정의역 (Domain)", title_en: "Domains", predicate: ox::rdfs("domain"), input: Input::Expression, symmetric: false, hint: "E5_Event" },
                Section { key: "rng", title_ko: "치역 (Range)", title_en: "Ranges", predicate: ox::rdfs("range"), input: Input::Entity, symmetric: false, hint: "xsd:integer" },
                Section { key: "spo", title_ko: "상위 프로퍼티 (SubProperty Of)", title_en: "SubProperty Of", predicate: ox::rdfs("subPropertyOf"), input: Input::Entity, symmetric: false, hint: "" },
            ],
            EntityKind::Individual | EntityKind::Unknown => vec![
                Section { key: "type", title_ko: "타입 (Types)", title_en: "Types", predicate: ox::rdf("type"), input: Input::Expression, symmetric: false, hint: "E39_Actor" },
                Section { key: "same", title_ko: "같은 개체 (Same As)", title_en: "Same Individual As", predicate: ox::owl("sameAs"), input: Input::Entity, symmetric: true, hint: "'강명하'" },
                Section { key: "diff", title_ko: "다른 개체 (Different From)", title_en: "Different Individuals", predicate: ox::owl("differentFrom"), input: Input::Entity, symmetric: true, hint: "" },
            ],
            _ => vec![],
        };

        if matches!(kind, EntityKind::ObjectProperty | EntityKind::DataProperty) {
            self.render_characteristics(ui, uri, kind, is_korean, &mut changes);
        }
        for sec in &sections {
            self.render_axiom_section(ui, uri, sec, is_korean, &mut changes);
        }
        if kind == EntityKind::Class {
            self.render_inferred_supers(ui, uri, &ox::rdfs("subClassOf"), if is_korean { "추론된 상위 클래스" } else { "Inferred superclasses" });
        }
        if matches!(kind, EntityKind::Individual | EntityKind::Unknown) {
            self.render_inferred_supers(ui, uri, &ox::rdf("type"), if is_korean { "추론된 타입" } else { "Inferred types" });
        }

        for (removes, adds) in changes {
            self.manager.apply_changes(removes, adds);
            self.graph_needs_sync = true;
        }
    }

    fn section_header(ui: &mut egui::Ui, text: &str) {
        ui.add_space(6.0);
        ui.label(egui::RichText::new(text).size(11.0).strong());
    }

    fn render_axiom_section(&mut self, ui: &mut egui::Ui, uri: &str, sec: &Section, is_korean: bool, changes: &mut Vec<(Vec<Spo>, Vec<Spo>)>) {
        Self::section_header(ui, if is_korean { sec.title_ko } else { sec.title_en });
        let g = &self.manager.memory.asserted_graph;
        let idx = self.dl_index.as_ref();
        // (value, triple that states it)
        let mut rows: Vec<(String, Spo)> = ox::objects(g, uri, &sec.predicate).into_iter()
            .map(|o| (o.clone(), (uri.to_string(), sec.predicate.clone(), o))).collect();
        if sec.symmetric {
            rows.extend(ox::subjects(g, &sec.predicate, uri).into_iter()
                .filter(|s| s != uri)
                .map(|s| (s.clone(), (s, sec.predicate.clone(), uri.to_string()))));
        }
        // rdf:type owl:Class / NamedIndividual declarations are not "types" worth editing here
        if sec.key == "type" {
            rows.retain(|(v, _)| !v.starts_with(&format!("i:{}", ox::OWL)));
        }
        if rows.is_empty() {
            ui.label(egui::RichText::new("—").weak());
        }
        for (value, triple) in rows.iter().take(200) {
            let text = ox::render(g, value, idx);
            ui.horizontal(|ui| {
                let resp = ui.add(egui::Label::new(egui::RichText::new(&text).monospace().size(11.0)).sense(egui::Sense::click()).truncate());
                if value.starts_with("i:") && resp.on_hover_text(value.trim_start_matches("i:")).clicked() {
                    self.selected_uri = Some(value.clone());
                    self.graph_needs_sync = true;
                }
                if ui.small_button("✖").on_hover_text(if is_korean { "이 공리 삭제" } else { "Remove axiom" }).clicked() {
                    let (s, p, o) = triple;
                    changes.push((ox::axiom_removal(g, s, p, o), vec![]));
                }
            });
        }
        if rows.len() > 200 {
            ui.label(egui::RichText::new(format!("… +{}", rows.len() - 200)).weak());
        }

        // add row
        let buf_key = sec.key.to_string();
        let mut buf = self.desc_inputs.remove(&buf_key).unwrap_or_default();
        let mut submit = false;
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut buf).hint_text(sec.hint).desired_width(ui.available_width() - 40.0).font(egui::TextStyle::Monospace));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) { submit = true; }
            if ui.small_button("➕").clicked() { submit = true; }
        });
        if submit && !buf.trim().is_empty() {
            let input = sec.input;
            let parsed = ox::manchester_to_rdf(buf.trim(), self.parse_index());
            match parsed {
                Ok((t, _)) if input == Input::Entity && t.starts_with("b:") => {
                    self.desc_error = Some((buf_key.clone(), if is_korean { "이름 하나만 입력하세요 (복합 식은 쓸 수 없는 항목입니다)".into() } else { "Enter a single name here".into() }));
                }
                Ok((t, mut adds)) => {
                    adds.push((uri.to_string(), sec.predicate.clone(), t));
                    changes.push((vec![], adds));
                    buf.clear();
                    self.desc_error = None;
                }
                Err(e) => self.desc_error = Some((buf_key.clone(), e)),
            }
        }
        if let Some((k, e)) = &self.desc_error {
            if *k == buf_key { ui.label(egui::RichText::new(e).size(10.0).color(egui::Color32::from_rgb(230, 100, 90))); }
        }
        self.desc_inputs.insert(buf_key, buf);
    }

    fn render_characteristics(&mut self, ui: &mut egui::Ui, uri: &str, kind: EntityKind, is_korean: bool, changes: &mut Vec<(Vec<Spo>, Vec<Spo>)>) {
        Self::section_header(ui, if is_korean { "특성 (Characteristics)" } else { "Characteristics" });
        let all: &[(&str, &str)] = &[
            ("FunctionalProperty", "Functional"), ("InverseFunctionalProperty", "Inverse functional"),
            ("TransitiveProperty", "Transitive"), ("SymmetricProperty", "Symmetric"), ("AsymmetricProperty", "Asymmetric"),
            ("ReflexiveProperty", "Reflexive"), ("IrreflexiveProperty", "Irreflexive"),
        ];
        let shown: &[(&str, &str)] = if kind == EntityKind::DataProperty { &all[..1] } else { all };
        let ty = ox::rdf("type");
        ui.horizontal_wrapped(|ui| {
            for (local, label) in shown {
                let c = ox::owl(local);
                let on = ox::has(&self.manager.memory.asserted_graph, uri, &ty, &c);
                let mut v = on;
                if ui.checkbox(&mut v, *label).changed() {
                    let t = vec![(uri.to_string(), ty.clone(), c.clone())];
                    changes.push(if on { (t, vec![]) } else { (vec![], t) });
                }
            }
        });
        ui.label(egui::RichText::new(if is_korean {
            "추론기가 쓰는 특성: Transitive, Symmetric(역관계 경유), Irreflexive·Asymmetric(모순 검사). Functional 계열은 저장되지만 아직 추론에 쓰이지 않습니다."
        } else {
            "Used by the reasoner: Transitive, Irreflexive/Asymmetric (consistency). Functional-type characteristics are saved but not reasoned with yet."
        }).size(9.0).weak());
    }

    fn render_inferred_supers(&mut self, ui: &mut egui::Ui, uri: &str, pred: &str, title: &str) {
        let inferred = ox::objects(&self.manager.memory.inferred_graph, uri, pred);
        let inferred: Vec<String> = inferred.into_iter().filter(|o| o != uri && !o.starts_with("b:")).collect();
        if inferred.is_empty() { return; }
        Self::section_header(ui, title);
        let mut explain = None;
        for o in inferred.iter().take(100) {
            let text = ox::render(&self.manager.memory.main_graph, o, self.dl_index.as_ref());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&text).monospace().size(11.0).color(egui::Color32::from_rgb(100, 200, 100)).italics());
                if ui.small_button("?").on_hover_text("추론 근거 보기 (Explain)").clicked() { explain = Some(o.clone()); }
            });
        }
        if let Some(o) = explain { self.request_explanation(uri, pred, &o); }
    }

    fn render_annotations(&mut self, ui: &mut egui::Ui, uri: &str, is_korean: bool, changes: &mut Vec<(Vec<Spo>, Vec<Spo>)>) {
        Self::section_header(ui, if is_korean { "주석 (Annotations)" } else { "Annotations" });
        let g = &self.manager.memory.asserted_graph;
        let annotation_props: Vec<String> = ox::COMMON_ANNOTATIONS.iter().map(|(_, iri)| format!("i:{}", iri))
            .chain(ox::subjects(g, &ox::rdf("type"), &ox::owl("AnnotationProperty")))
            .collect();
        let mut any = false;
        for p in &annotation_props {
            for o in ox::objects(g, uri, p) {
                any = true;
                let pname = ox::COMMON_ANNOTATIONS.iter().find(|(_, iri)| p == &format!("i:{}", iri)).map(|(n, _)| n.to_string())
                    .unwrap_or_else(|| ox::short_name(p, None));
                let shown = ox::short_name(&o, None);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&pname).size(11.0).color(egui::Color32::LIGHT_BLUE));
                    ui.add(egui::Label::new(egui::RichText::new(&shown).size(11.0)).truncate());
                    if ui.small_button("✖").clicked() {
                        changes.push((vec![(uri.to_string(), p.clone(), o.clone())], vec![]));
                    }
                });
            }
        }
        if !any { ui.label(egui::RichText::new("—").weak()); }

        let mut add = false;
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("annotation_prop")
                .width(110.0)
                .selected_text(ox::COMMON_ANNOTATIONS[self.ann_prop_idx.min(ox::COMMON_ANNOTATIONS.len() - 1)].0)
                .show_ui(ui, |ui| {
                    for (i, (name, _)) in ox::COMMON_ANNOTATIONS.iter().enumerate() {
                        ui.selectable_value(&mut self.ann_prop_idx, i, *name);
                    }
                });
            let r = ui.add(egui::TextEdit::singleline(&mut self.ann_value).hint_text(if is_korean { "값" } else { "value" }).desired_width((ui.available_width() - 80.0).max(60.0)));
            ui.add(egui::TextEdit::singleline(&mut self.ann_lang).hint_text("lang").desired_width(30.0))
                .on_hover_text(if is_korean { "언어 태그 (예: ko, en). 비우면 언어 없음" } else { "Language tag (ko, en…); empty = none" });
            if (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) || ui.small_button("➕").clicked() { add = true; }
        });
        if add && !self.ann_value.trim().is_empty() {
            let (_, iri) = ox::COMMON_ANNOTATIONS[self.ann_prop_idx.min(ox::COMMON_ANNOTATIONS.len() - 1)];
            let v = self.ann_value.trim();
            // seeAlso / isDefinedBy / source with a URL value point at a resource, not a string
            let is_ref = matches!(iri.rsplit(['#', '/']).next(), Some("seeAlso" | "isDefinedBy" | "source"))
                && (v.starts_with("http://") || v.starts_with("https://"));
            let o = if is_ref { format!("i:{}", v) }
                else if self.ann_lang.trim().is_empty() { format!("l:{}", v) }
                else { format!("l:{}@{}", v, self.ann_lang.trim()) };
            changes.push((vec![], vec![(uri.to_string(), format!("i:{}", iri), o)]));
            self.ann_value.clear();
        }
    }

    fn render_rename(&mut self, ui: &mut egui::Ui, uri: &str, is_korean: bool, changes: &mut Vec<(Vec<Spo>, Vec<Spo>)>) {
        let Some(mut buf) = self.rename_buffer.take() else { return };
        let mut keep = true;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(egui::RichText::new(if is_korean { "새 IRI (파일 안의 모든 사용처가 함께 바뀝니다)" } else { "New IRI (every use in the file is updated)" }).size(10.0));
            ui.add(egui::TextEdit::singleline(&mut buf).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace));
            if let Some((k, e)) = &self.desc_error {
                if k == "rename" { ui.label(egui::RichText::new(e).size(10.0).color(egui::Color32::from_rgb(230, 100, 90))); }
            }
            ui.horizontal(|ui| {
                if ui.button(if is_korean { "적용" } else { "Apply" }).clicked() {
                    match ox::validate_iri(&buf) {
                        Ok(new) if new == uri => keep = false,
                        Ok(new) => {
                            let g = &self.manager.memory.asserted_graph;
                            let taken = !ox::objects(g, &new, &ox::rdf("type")).is_empty();
                            if taken {
                                self.desc_error = Some(("rename".into(), if is_korean { "이미 쓰이고 있는 IRI입니다 (합치려면 sameAs를 쓰세요)".into() } else { "IRI already in use".into() }));
                            } else {
                                let (removes, adds) = ox::rename_changes(g, uri, &new);
                                let n = removes.len();
                                changes.push((removes, adds));
                                self.selected_uri = Some(new);
                                self.desc_error = None;
                                self.status_message = Some((
                                    if is_korean { format!("IRI를 변경했습니다 ({}개 트리플 수정, 실행 취소 가능)", n) } else { format!("Renamed ({} triples updated; undoable)", n) },
                                    std::time::Instant::now(),
                                ));
                                keep = false;
                            }
                        }
                        Err(e) => self.desc_error = Some(("rename".into(), e)),
                    }
                }
                if ui.button(if is_korean { "취소" } else { "Cancel" }).clicked() { keep = false; }
            });
            if !ox::objects(&self.manager.memory.imported_graph, uri, &ox::rdf("type")).is_empty() {
                ui.label(egui::RichText::new(if is_korean { "⚠ 이 엔티티는 가져온(import) 온톨로지에도 정의되어 있습니다. 그쪽 파일은 바뀌지 않습니다." } else { "⚠ Also defined in an imported ontology, which is not changed." }).size(10.0).color(egui::Color32::from_rgb(230, 160, 40)));
            }
        });
        if keep { self.rename_buffer = Some(buf); }
    }
}
