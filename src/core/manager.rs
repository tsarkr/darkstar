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
    /// Set when a change cannot be handled incrementally (removals, reloads, rule changes).
    needs_full_reasoning: bool,
    engine: InferenceEngine,
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
            needs_full_reasoning: false,
            engine: InferenceEngine::new(),
        }
    }

    pub fn new_ontology(&mut self) {
        self.memory.clear();
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

    /// Removes many assertions as a single undoable change (one history entry, one event).
    pub fn remove_assertions_batch(&mut self, triples: Vec<(String, String, String)>) {
        if triples.is_empty() { return; }
        let events: Vec<DarkstarEvent> = triples
            .into_iter()
            .map(|(s, p, o)| DarkstarEvent::AxiomRemoved { s, p, o })
            .collect();
        let batch = DarkstarEvent::Batch(events);
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
        self.add_assertion(uri, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(), "http://www.w3.org/2002/07/owl#NamedIndividual".to_string());
    }

    pub fn add_class(&mut self, uri: String) {
        self.add_assertion(uri, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(), "http://www.w3.org/2002/07/owl#Class".to_string());
    }

    pub fn add_subclass(&mut self, sub: String, sup: String) {
        self.add_assertion(sub, "http://www.w3.org/2000/01/rdf-schema#subClassOf".to_string(), sup);
    }

    pub fn add_object_property(&mut self, uri: String) {
        self.add_assertion(uri, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(), "http://www.w3.org/2002/07/owl#ObjectProperty".to_string());
    }

    pub fn add_data_property(&mut self, uri: String) {
        self.add_assertion(uri, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(), "http://www.w3.org/2002/07/owl#DatatypeProperty".to_string());
    }

    pub fn add_subproperty(&mut self, sub: String, sup: String) {
        self.add_assertion(sub, "http://www.w3.org/2000/01/rdf-schema#subPropertyOf".to_string(), sup);
    }

    pub fn load_from_string(&mut self, data: &str, format: crate::core::io::OntologyFormat) -> Result<(), Box<dyn std::error::Error>> {
        let rules_settings = crate::core::settings::ReasonerRules::default();
        let mut temp_graph = sophia::inmem::graph::FastGraph::new();
        match format {
            crate::core::io::OntologyFormat::Turtle => {
                let parser = sophia::turtle::parser::turtle::TurtleParser { base: None }.parse_str(&data);
                temp_graph.insert_all(parser)?;
            },
            crate::core::io::OntologyFormat::NTriples => {
                let parser = sophia::turtle::parser::nt::NTriplesParser {}.parse_str(&data);
                temp_graph.insert_all(parser)?;
            }
        }

        let mut changes = Vec::new();
        for t in temp_graph.triples() {
            let t = t?;
            if !self.memory.asserted_graph.contains(t.s(), t.p(), t.o()).unwrap() {
                let s = crate::rules::extract_str(&t.s());
                let p = crate::rules::extract_str(&t.p());
                let o = crate::rules::extract_str(&t.o());
                changes.push(DarkstarEvent::AxiomAdded { s, p, o });
            }
        }

        if !changes.is_empty() {
            let batch = DarkstarEvent::Batch(changes);
            self.apply_event(batch.clone());
            self.history.push_change(batch.clone());
            self.event_queue.push(batch);
            self.run_reasoning(&rules_settings);
        }
        
        Ok(())
    }

    pub fn replace_from_string(&mut self, data: &str, format: crate::core::io::OntologyFormat, rules_settings: &crate::core::settings::ReasonerRules) -> Result<(), Box<dyn std::error::Error>> {
        let mut temp_graph = sophia::inmem::graph::FastGraph::new();
        match format {
            crate::core::io::OntologyFormat::Turtle => {
                let parser = sophia::turtle::parser::turtle::TurtleParser { base: None }.parse_str(&data);
                temp_graph.insert_all(parser)?;
            },
            crate::core::io::OntologyFormat::NTriples => {
                let parser = sophia::turtle::parser::nt::NTriplesParser {}.parse_str(&data);
                temp_graph.insert_all(parser)?;
            }
        }

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
        self.memory.clear();
        let data = std::fs::read_to_string(path)?;
        let format = if path.extension().and_then(|e| e.to_str()) == Some("nt") {
            crate::core::io::OntologyFormat::NTriples
        } else {
            crate::core::io::OntologyFormat::Turtle
        };
        
        let parse_start = std::time::Instant::now();
        match format {
            crate::core::io::OntologyFormat::Turtle => {
                let parser = sophia::turtle::parser::turtle::TurtleParser { base: None }.parse_str(&data);
                self.memory.asserted_graph.insert_all(parser)?;
            },
            crate::core::io::OntologyFormat::NTriples => {
                let parser = sophia::turtle::parser::nt::NTriplesParser {}.parse_str(&data);
                self.memory.asserted_graph.insert_all(parser)?;
            }
        }
        drop(data);
        println!("Darkstar: parsed {:?} in {:?}", path.file_name().unwrap_or_default(), parse_start.elapsed());

        self.is_dirty = false;
        
        self.history.clear();
        self.event_queue.clear();
        
        // run_inference builds main_graph from asserted_graph (single clone).
        self.engine.run_inference(&mut self.memory, rules_settings);
        self.needs_full_reasoning = false;
        self.revision += 1;
        
        self.active_file_path = Some(path.to_path_buf());
        self.file_modified_time = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        self.needs_save = false;
        Ok(())
    }

    pub fn serialize_to_string(&self, format: crate::core::io::OntologyFormat, include_inferred: bool) -> Result<String, Box<dyn std::error::Error>> {
        use crate::core::io::OntologyFormat;
        use sophia::api::serializer::*;
        
        let graph_to_serialize = if include_inferred {
            &self.memory.main_graph
        } else {
            &self.memory.asserted_graph
        };

        match format {
            OntologyFormat::Turtle => {
                let mut serializer = sophia::turtle::serializer::turtle::TurtleSerializer::new_stringifier();
                serializer.serialize_graph(graph_to_serialize)?;
                Ok(serializer.as_str().to_string())
            },
            OntologyFormat::NTriples => {
                let mut serializer = sophia::turtle::serializer::nt::NtSerializer::new_stringifier();
                serializer.serialize_graph(graph_to_serialize)?;
                Ok(serializer.as_str().to_string())
            }
        }
    }

    pub fn save_to_file(&mut self, path: &std::path::Path, format: crate::core::io::OntologyFormat, include_inferred: bool) -> Result<(), Box<dyn std::error::Error>> {
        let data = self.serialize_to_string(format, include_inferred)?;
        std::fs::write(path, data)?;
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
}


