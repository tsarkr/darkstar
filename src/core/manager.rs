use std::path::PathBuf;
use sophia::api::prelude::*;
use super::history::{ChangeHistory, DarkstarEvent};
use crate::rules::engine::InferenceEngine;
use crate::memory::EngineMemory;

pub struct DarkstarManager {
    pub memory: EngineMemory,
    pub active_file_path: Option<PathBuf>,
    pub file_modified_time: Option<std::time::SystemTime>,
    pub history: ChangeHistory,
    pub is_dirty: bool,
    pub needs_save: bool,
    pub event_queue: Vec<DarkstarEvent>,
    /// Monotonic counter bumped on every change to the asserted/inferred graphs.
    /// UI layers compare it against their last-seen value to invalidate caches cheaply.
    pub revision: u64,
    /// Serialization of the opened file; "Save" writes back in the same format.
    pub active_format: crate::core::io::OntologyFormat,
    /// Result of the last `owl:imports` resolution.
    pub import_statuses: Vec<crate::core::imports::ImportStatus>,
    /// Set when a change cannot be handled incrementally (removals, reloads, rule changes).
    needs_full_reasoning: bool,
    engine: InferenceEngine,
}

impl Default for DarkstarManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DarkstarManager {
    pub fn new() -> Self {
        Self {
            memory: EngineMemory::new(),
            active_file_path: None,
            file_modified_time: None,
            history: ChangeHistory::new(),
            is_dirty: false,
            needs_save: false,
            event_queue: Vec::new(),
            revision: 0,
            active_format: crate::core::io::OntologyFormat::Turtle,
            import_statuses: Vec::new(),
            needs_full_reasoning: false,
            engine: InferenceEngine::new(),
        }
    }

    pub fn new_ontology(&mut self) {
        self.memory.clear();
        self.import_statuses.clear();
        self.active_format = crate::core::io::OntologyFormat::Turtle;
        self.active_file_path = None;
        self.file_modified_time = None;
        self.history.clear();
        self.is_dirty = false;
        self.needs_save = false;
        self.needs_full_reasoning = false;
        self.event_queue.clear();
        self.revision += 1;
    }

    /// Forces the next `run_reasoning` call to fully re-materialize (e.g. after rule settings change).
    pub fn invalidate_reasoning(&mut self) {
        self.is_dirty = true;
        self.needs_full_reasoning = true;
    }

    pub fn add_assertion(&mut self, s: String, p: String, o: String) {
        let event = DarkstarEvent::AxiomAdded { s: s.clone(), p: p.clone(), o: o.clone() };
        self.history.push_change(event.clone());
        self.event_queue.push(event);
        self.add_assertion_internal(s, p, o);
    }

    fn add_assertion_internal(&mut self, s: String, p: String, o: String) {
        let s_term = InferenceEngine::make_term(&s);
        let p_term = InferenceEngine::make_term(&p);
        let o_term = InferenceEngine::make_term(&o);
        
        self.memory.asserted_graph.insert(&s_term, &p_term, &o_term).unwrap();
        self.memory.main_graph.insert(&s_term, &p_term, &o_term).unwrap();
        self.memory.delta_graph.insert(&s_term, &p_term, &o_term).unwrap();
        self.is_dirty = true;
        self.needs_save = true;
        self.revision += 1;
    }

    pub fn remove_assertion(&mut self, s: String, p: String, o: String) {
        let event = DarkstarEvent::AxiomRemoved { s: s.clone(), p: p.clone(), o: o.clone() };
        self.history.push_change(event.clone());
        self.event_queue.push(event);
        self.remove_assertion_internal(s, p, o);
    }

    fn remove_assertion_internal(&mut self, s: String, p: String, o: String) {
        let s_term = InferenceEngine::make_term(&s);
        let p_term = InferenceEngine::make_term(&p);
        let o_term = InferenceEngine::make_term(&o);
        
        self.memory.asserted_graph.remove(&s_term, &p_term, &o_term).unwrap();
        self.memory.main_graph.remove(&s_term, &p_term, &o_term).unwrap();
        self.is_dirty = true;
        self.needs_save = true;
        // Inferences derived from the removed triple may now be unsupported.
        self.needs_full_reasoning = true;
        self.revision += 1;
    }

    /// Applies removals then additions as ONE undoable change (one history entry, one event).
    /// Used by the description editor and refactorings (e.g. IRI rename, expression edits).
    pub fn apply_changes(&mut self, removes: Vec<(String, String, String)>, adds: Vec<(String, String, String)>) {
        let mut events: Vec<DarkstarEvent> = removes.into_iter().map(|(s, p, o)| DarkstarEvent::AxiomRemoved { s, p, o }).collect();
        events.extend(adds.into_iter().map(|(s, p, o)| DarkstarEvent::AxiomAdded { s, p, o }));
        if events.is_empty() { return; }
        let batch = if events.len() == 1 { events.pop().unwrap() } else { DarkstarEvent::Batch(events) };
        self.apply_event(batch.clone());
        self.history.push_change(batch.clone());
        self.event_queue.push(batch);
    }

    pub fn undo(&mut self) {
        if let Some(event) = self.history.pop_undo() {
            let rev = event.reverse();
            self.event_queue.push(rev);
            self.apply_event_reverse(event);
        }
    }

    pub fn redo(&mut self) {
        if let Some(event) = self.history.pop_redo() {
            self.event_queue.push(event.clone());
            self.apply_event(event);
        }
    }

    fn apply_event(&mut self, event: DarkstarEvent) {
        match event {
            DarkstarEvent::AxiomAdded { s, p, o } => self.add_assertion_internal(s, p, o),
            DarkstarEvent::AxiomRemoved { s, p, o } => self.remove_assertion_internal(s, p, o),
            DarkstarEvent::Batch(events) => {
                for e in events {
                    self.apply_event(e);
                }
            }
        }
    }

    fn apply_event_reverse(&mut self, event: DarkstarEvent) {
        match event {
            DarkstarEvent::AxiomAdded { s, p, o } => self.remove_assertion_internal(s, p, o),
            DarkstarEvent::AxiomRemoved { s, p, o } => self.add_assertion_internal(s, p, o),
            DarkstarEvent::Batch(mut events) => {
                events.reverse();
                for e in events {
                    self.apply_event_reverse(e);
                }
            }
        }
    }

    pub fn run_reasoning(&mut self, rules_settings: &crate::core::settings::ReasonerRules) {
        if self.is_dirty {
            if self.needs_full_reasoning || !self.memory.is_consistent {
                self.engine.run_inference(&mut self.memory, rules_settings);
            } else {
                // Addition-only changes: only propagate the newly added triples.
                self.engine.run_incremental(&mut self.memory, rules_settings);
            }
            self.needs_full_reasoning = false;
            self.is_dirty = false;
            self.revision += 1;
        }
    }

    pub fn add_individual(&mut self, uri: String) {
        self.add_assertion(uri, crate::vocab::RDF_TYPE.to_string(), crate::vocab::OWL_NAMED_INDIVIDUAL.to_string());
    }

    pub fn add_class(&mut self, uri: String) {
        self.add_assertion(uri, crate::vocab::RDF_TYPE.to_string(), crate::vocab::OWL_CLASS.to_string());
    }

    pub fn add_subclass(&mut self, sub: String, sup: String) {
        self.add_assertion(sub, crate::vocab::RDFS_SUBCLASS_OF.to_string(), sup);
    }

    pub fn add_object_property(&mut self, uri: String) {
        self.add_assertion(uri, crate::vocab::RDF_TYPE.to_string(), crate::vocab::OWL_OBJECT_PROPERTY.to_string());
    }

    pub fn add_data_property(&mut self, uri: String) {
        self.add_assertion(uri, crate::vocab::RDF_TYPE.to_string(), crate::vocab::OWL_DATATYPE_PROPERTY.to_string());
    }

    pub fn add_subproperty(&mut self, sub: String, sup: String) {
        self.add_assertion(sub, crate::vocab::RDFS_SUBPROPERTY_OF.to_string(), sup);
    }

    pub fn replace_from_string(&mut self, data: &str, format: crate::core::io::OntologyFormat, rules_settings: &crate::core::settings::ReasonerRules) -> Result<(), Box<dyn std::error::Error>> {
        let mut temp_graph = sophia::inmem::graph::FastGraph::new();
        let base = self.active_file_path.as_deref().and_then(crate::core::io::file_iri);
        crate::core::io::parse_into(&mut temp_graph, data, format, base)?;

        let mut changes = Vec::new();
        
        // Find newly added triples
        for t in temp_graph.triples() {
            let t = t?;
            if !self.memory.asserted_graph.contains(t.s(), t.p(), t.o()).unwrap() {
                let s = crate::rules::extract_str(&t.s());
                let p = crate::rules::extract_str(&t.p());
                let o = crate::rules::extract_str(&t.o());
                changes.push(DarkstarEvent::AxiomAdded { s, p, o });
            }
        }
        
        // Find removed triples
        let mut removed = Vec::new();
        for t in self.memory.asserted_graph.triples().flatten() {
            if !temp_graph.contains(t.s(), t.p(), t.o()).unwrap() {
                let s = crate::rules::extract_str(&t.s());
                let p = crate::rules::extract_str(&t.p());
                let o = crate::rules::extract_str(&t.o());
                removed.push(DarkstarEvent::AxiomRemoved { s, p, o });
            }
        }
        changes.extend(removed);

        // Directly adopt the freshly parsed graph to preserve exact typed literals, language tags, and blank nodes.
        // (main/delta graphs are rebuilt by run_inference, so no extra clones are needed here.)
        self.memory.asserted_graph = temp_graph;
        self.is_dirty = true;
        self.needs_save = true;
        // owl:imports may have been added or removed in the edited source.
        self.reload_imports();

        if !changes.is_empty() {
            let batch = DarkstarEvent::Batch(changes);
            self.history.push_change(batch.clone());
            self.event_queue.push(batch);
        } else {
            self.event_queue.push(DarkstarEvent::Batch(vec![]));
        }

        self.engine.run_inference(&mut self.memory, rules_settings);
        self.is_dirty = false;
        self.needs_full_reasoning = false;
        self.revision += 1;
        
        Ok(())
    }

    pub fn load_from_file(&mut self, path: &std::path::Path, rules_settings: &crate::core::settings::ReasonerRules) -> Result<(), Box<dyn std::error::Error>> {
        let data = std::fs::read_to_string(path)?;
        let format = crate::core::io::OntologyFormat::detect(Some(path), &data)?;
        self.memory.clear();

        let parse_start = std::time::Instant::now();
        crate::core::io::parse_into(&mut self.memory.asserted_graph, &data, format, crate::core::io::file_iri(path))?;
        self.active_format = format;
        drop(data);
        println!("Darkstar: parsed {:?} in {:?}", path.file_name().unwrap_or_default(), parse_start.elapsed());

        self.is_dirty = false;
        
        self.history.clear();
        self.event_queue.clear();

        self.active_file_path = Some(path.to_path_buf());
        self.reload_imports();

        // run_inference builds main_graph from asserted (+ imported) graphs.
        self.engine.run_inference(&mut self.memory, rules_settings);
        self.needs_full_reasoning = false;
        self.revision += 1;
        
        self.active_file_path = Some(path.to_path_buf());
        self.file_modified_time = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        self.needs_save = false;
        Ok(())
    }

    pub fn serialize_to_string(&self, format: crate::core::io::OntologyFormat, include_inferred: bool) -> Result<String, Box<dyn std::error::Error>> {
        if !include_inferred {
            return crate::core::io::serialize_graph(&self.memory.asserted_graph, format);
        }
        if self.memory.imported_graph.triples().next().is_none() {
            return crate::core::io::serialize_graph(&self.memory.main_graph, format);
        }
        // asserted ∪ inferred, without the (possibly huge) imported ontologies
        let mut g = self.memory.asserted_graph.clone();
        for t in self.memory.inferred_graph.triples().flatten() {
            let _ = g.insert(t.s(), t.p(), t.o());
        }
        crate::core::io::serialize_graph(&g, format)
    }

    /// (Re)loads the `owl:imports` closure of the asserted graph into `memory.imported_graph`.
    /// Call before full reasoning.
    pub fn reload_imports(&mut self) {
        let mut imported = sophia::inmem::graph::FastGraph::new();
        self.import_statuses = crate::core::imports::resolve_imports(
            &self.memory.asserted_graph,
            self.active_file_path.as_deref(),
            &mut imported,
        );
        for st in &self.import_statuses {
            match &st.error {
                None => println!("Darkstar: imported <{}> from {} ({} triples)", st.iri, st.source.as_deref().unwrap_or("?"), st.triples),
                Some(e) => println!("Darkstar: import <{}> failed: {}", st.iri, e),
            }
        }
        self.memory.imported_graph = imported;
        self.needs_full_reasoning = true;
    }

    pub fn save_to_file(&mut self, path: &std::path::Path, format: crate::core::io::OntologyFormat, include_inferred: bool) -> Result<(), Box<dyn std::error::Error>> {
        let data = self.serialize_to_string(format, include_inferred)?;
        std::fs::write(path, data)?;
        if include_inferred {
            // Exporting inferences is a side output; the working file stays the same.
            return Ok(());
        }
        self.active_format = format;
        self.active_file_path = Some(path.to_path_buf());
        self.file_modified_time = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        self.needs_save = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::io::OntologyFormat;
    use crate::core::settings::ReasonerRules;

    #[test]
    fn test_replace_from_string_preserves_literals_and_replaces_cleanly() {
        let mut manager = DarkstarManager::new();
        let rules = ReasonerRules::default();

        let initial_ttl = r#"
            @prefix ex: <http://example.org/> .
            @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

            ex:john a ex:Person ;
                ex:age "25"^^xsd:integer ;
                ex:name "John Doe"@en .
        "#;

        manager.replace_from_string(initial_ttl, OntologyFormat::Turtle, &rules).unwrap();
        assert_eq!(manager.memory.asserted_graph.triples().count(), 3);

        // Update the TTL: change age from 25 to 26 and add nationality
        let updated_ttl = r#"
            @prefix ex: <http://example.org/> .
            @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

            ex:john a ex:Person ;
                ex:age "26"^^xsd:integer ;
                ex:name "John Doe"@en ;
                ex:nationality "Korean" .
        "#;

        manager.replace_from_string(updated_ttl, OntologyFormat::Turtle, &rules).unwrap();
        assert_eq!(manager.memory.asserted_graph.triples().count(), 4);

        // Serialize and verify datatypes are preserved
        let serialized = manager.serialize_to_string(OntologyFormat::Turtle, false).unwrap();
        assert!(serialized.contains("26"));
        assert!(!serialized.contains("25"));
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("darkstar_test_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    const MAIN_RDFXML: &str = r#"<?xml version="1.0"?>
<rdf:RDF xmlns="http://example.org/main#"
     xml:base="http://example.org/main"
     xmlns:owl="http://www.w3.org/2002/07/owl#"
     xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
     xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#">
    <owl:Ontology rdf:about="http://example.org/main">
        <owl:imports rdf:resource="http://example.org/base"/>
        <owl:imports rdf:resource="http://example.org/other"/>
    </owl:Ontology>
    <owl:Class rdf:about="http://example.org/main#Activist">
        <rdfs:subClassOf rdf:resource="http://example.org/base#Person"/>
    </owl:Class>
    <owl:NamedIndividual rdf:about="http://example.org/main#kim">
        <rdf:type rdf:resource="http://example.org/main#Activist"/>
        <rdfs:label xml:lang="ko">김아무개</rdfs:label>
    </owl:NamedIndividual>
</rdf:RDF>
"#;

    const BASE_TTL: &str = r#"
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix b: <http://example.org/base#> .
<http://example.org/base> a owl:Ontology .
b:Person a owl:Class ; rdfs:subClassOf b:Agent .
b:Agent a owl:Class .
"#;

    const OTHER_TTL: &str = r#"
@prefix owl: <http://www.w3.org/2002/07/owl#> .
<http://example.org/other> a owl:Ontology ; owl:imports <http://example.org/main> .
<http://example.org/other#Thing2> a owl:Class .
"#;

    #[test]
    fn test_rdfxml_with_local_and_catalog_imports() {
        let dir = temp_dir("imports");
        std::fs::write(dir.join("main.owl"), MAIN_RDFXML).unwrap();
        // found by IRI last segment + extension guess
        std::fs::write(dir.join("base.ttl"), BASE_TTL).unwrap();
        // found only through the catalog (file name doesn't match the IRI); imports main back (cycle)
        std::fs::create_dir_all(dir.join("deps")).unwrap();
        std::fs::write(dir.join("deps").join("other-v2.ttl"), OTHER_TTL).unwrap();
        std::fs::write(dir.join("catalog-v001.xml"), r#"<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<catalog prefer="public" xmlns="urn:oasis:names:tc:entity:xmlns:xml:catalog">
    <uri id="Imports Wizard Entry" name="http://example.org/other" uri="deps/other-v2.ttl"/>
</catalog>"#).unwrap();

        let mut m = DarkstarManager::new();
        m.load_from_file(&dir.join("main.owl"), &ReasonerRules::default()).unwrap();
        assert_eq!(m.active_format, OntologyFormat::RdfXml);
        assert_eq!(m.import_statuses.len(), 2, "{:?}", m.import_statuses);
        assert!(m.import_statuses.iter().all(|s| s.ok()), "{:?}", m.import_statuses);

        // Reasoning crosses the import boundary: kim → Activist → base:Person → base:Agent
        let kim = InferenceEngine::make_term("i:http://example.org/main#kim");
        let ty = InferenceEngine::make_term(crate::vocab::RDF_TYPE);
        let agent = InferenceEngine::make_term("i:http://example.org/base#Agent");
        assert!(m.memory.main_graph.contains(&kim, &ty, &agent).unwrap());
        assert!(m.memory.inferred_graph.contains(&kim, &ty, &agent).unwrap());

        // Imported axioms are not part of the asserted ontology and are not saved.
        let person = InferenceEngine::make_term("i:http://example.org/base#Person");
        let sco = InferenceEngine::make_term(crate::vocab::RDFS_SUBCLASS_OF);
        assert!(!m.memory.asserted_graph.contains(&person, &sco, &agent).unwrap());
        assert!(m.memory.imported_graph.contains(&person, &sco, &agent).unwrap());

        // Save (keeps RDF/XML) and reload: identical asserted graph, Korean literal intact.
        let asserted = m.memory.asserted_graph.triples().count();
        let out = dir.join("main.owl");
        let fmt = m.active_format;
        m.save_to_file(&out, fmt, false).unwrap();
        let saved = std::fs::read_to_string(&out).unwrap();
        assert!(saved.contains("rdf:RDF") || saved.contains("RDF"), "not RDF/XML: {}", &saved[..200.min(saved.len())]);
        assert!(saved.contains("김아무개"));
        assert!(!saved.contains("base#Agent"));
        let mut m2 = DarkstarManager::new();
        m2.load_from_file(&out, &ReasonerRules::default()).unwrap();
        assert_eq!(m2.memory.asserted_graph.triples().count(), asserted);
        assert!(m2.memory.inferred_graph.contains(&kim, &ty, &agent).unwrap());

        // Save As Turtle switches the working format.
        let ttl = dir.join("main_copy.ttl");
        m2.save_to_file(&ttl, OntologyFormat::from_path(&ttl), false).unwrap();
        assert_eq!(m2.active_format, OntologyFormat::Turtle);
        let mut m3 = DarkstarManager::new();
        m3.load_from_file(&ttl, &ReasonerRules::default()).unwrap();
        assert_eq!(m3.memory.asserted_graph.triples().count(), asserted);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_missing_import_is_reported_not_fatal() {
        let dir = temp_dir("missing");
        let ttl = "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n<http://example.org/x> a owl:Ontology ; owl:imports <urn:nowhere:onto> .\n<http://example.org/x#A> a owl:Class .\n";
        std::fs::write(dir.join("x.ttl"), ttl).unwrap();
        let mut m = DarkstarManager::new();
        m.load_from_file(&dir.join("x.ttl"), &ReasonerRules::default()).unwrap();
        assert_eq!(m.import_statuses.len(), 1);
        assert!(!m.import_statuses[0].ok());
        assert_eq!(m.memory.asserted_graph.triples().count(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_format_detection() {
        use std::path::Path;
        assert_eq!(OntologyFormat::detect(Some(Path::new("a.owl")), "@prefix x: <http://x/> .").unwrap(), OntologyFormat::Turtle);
        assert_eq!(OntologyFormat::detect(Some(Path::new("a.ttl")), "<?xml version=\"1.0\"?><rdf:RDF/>").unwrap(), OntologyFormat::RdfXml);
        assert_eq!(OntologyFormat::detect(Some(Path::new("a.nt")), "<http://a> <http://b> <http://c> .").unwrap(), OntologyFormat::NTriples);
        assert!(OntologyFormat::detect(Some(Path::new("a.owl")), "<?xml version=\"1.0\"?>\n<Ontology xmlns=\"http://www.w3.org/2002/07/owl#\">").is_err());
        assert!(OntologyFormat::detect(Some(Path::new("a.omn")), "Prefix: : <http://x#>\nOntology: <http://x>").is_err());
        assert_eq!(OntologyFormat::from_path(Path::new("a.OWL")), OntologyFormat::RdfXml);
    }
}
