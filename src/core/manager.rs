use std::path::PathBuf;
use sophia::api::prelude::*;
use super::history::{ChangeHistory, DarkstarEvent};
use crate::rules::engine::InferenceEngine;
use crate::memory::EngineMemory;

pub struct DarkstarManager {
    pub memory: EngineMemory,
    pub active_file_path: Option<PathBuf>,
    pub history: ChangeHistory,
    pub is_dirty: bool,
    engine: InferenceEngine,
}

impl DarkstarManager {
    pub fn new() -> Self {
        Self {
            memory: EngineMemory::new(),
            active_file_path: None,
            history: ChangeHistory::new(),
            is_dirty: false,
            engine: InferenceEngine::new(),
        }
    }

    pub fn add_assertion(&mut self, s: String, p: String, o: String) {
        let event = DarkstarEvent::AxiomAdded { s: s.clone(), p: p.clone(), o: o.clone() };
        self.history.push_change(event);
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
    }

    pub fn remove_assertion(&mut self, s: String, p: String, o: String) {
        let event = DarkstarEvent::AxiomRemoved { s: s.clone(), p: p.clone(), o: o.clone() };
        self.history.push_change(event);
        self.remove_assertion_internal(s, p, o);
    }

    fn remove_assertion_internal(&mut self, s: String, p: String, o: String) {
        let s_term = InferenceEngine::make_term(&s);
        let p_term = InferenceEngine::make_term(&p);
        let o_term = InferenceEngine::make_term(&o);
        
        self.memory.asserted_graph.remove(&s_term, &p_term, &o_term).unwrap();
        self.memory.main_graph.remove(&s_term, &p_term, &o_term).unwrap();
        // Note: Removing doesn't easily work with incremental reasoning in our simple engine
        // without a full re-run, so we mark it dirty.
        self.is_dirty = true;
    }

    pub fn undo(&mut self) {
        if let Some(event) = self.history.pop_undo() {
            self.apply_event_reverse(event);
        }
    }

    pub fn redo(&mut self) {
        if let Some(event) = self.history.pop_redo() {
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
            println!("Darkstar: Triggering incremental reasoning...");
            self.engine.run_inference(&mut self.memory, rules_settings);
            self.is_dirty = false;
        }
    }

    pub fn load_from_string(&mut self, data: &str, format: crate::core::io::OntologyFormat) -> Result<(), Box<dyn std::error::Error>> {
        use crate::core::io::OntologyFormat;
        use sophia::api::parser::TripleParser;
        
        match format {
            OntologyFormat::Turtle => {
                let parser = sophia::turtle::parser::turtle::TurtleParser { base: None }.parse_str(data);
                self.memory.asserted_graph.insert_all(parser)?;
            },
            OntologyFormat::NTriples => {
                let parser = sophia::turtle::parser::nt::NTriplesParser {}.parse_str(data);
                self.memory.asserted_graph.insert_all(parser)?;
            }
        }
        
        for t in self.memory.asserted_graph.triples() {
            let t = t?;
            self.memory.main_graph.insert(t.s(), t.p(), t.o()).unwrap();
            self.memory.delta_graph.insert(t.s(), t.p(), t.o()).unwrap();
        }
        
        self.is_dirty = true;
        Ok(())
    }

    pub fn rename_entity(&mut self, old_uri: &str, new_uri: &str) {
        let old_prefixed = self.ensure_iri_prefix(old_uri);
        let new_prefixed = self.ensure_iri_prefix(new_uri);
        
        if old_prefixed == new_prefixed { return; }
        
        let mut changes = Vec::new();
        let mut triples_to_process = Vec::new();

        // Collect all triples involving old_uri in asserted_graph
        for t in self.memory.asserted_graph.triples() {
            let t = t.unwrap();
            let s_str = crate::rules::extract_str(&t.s());
            let p_str = crate::rules::extract_str(&t.p());
            let o_str = crate::rules::extract_str(&t.o());

            if s_str == old_prefixed || p_str == old_prefixed || o_str == old_prefixed {
                triples_to_process.push((s_str, p_str, o_str));
            }
        }

        for (s, p, o) in triples_to_process {
            // Remove old
            changes.push(DarkstarEvent::AxiomRemoved { s: s.clone(), p: p.clone(), o: o.clone() });
            
            // Add new
            let new_s = if s == old_prefixed { new_prefixed.clone() } else { s };
            let new_p = if p == old_prefixed { new_prefixed.clone() } else { p };
            let new_o = if o == old_prefixed { new_prefixed.clone() } else { o };
            changes.push(DarkstarEvent::AxiomAdded { s: new_s, p: new_p, o: new_o });
        }

        if !changes.is_empty() {
            let batch = DarkstarEvent::Batch(changes);
            self.apply_event(batch.clone());
            self.history.push_change(batch);
        }
    }

    pub fn merge_nodes(&mut self, source_uri: &str, target_uri: &str) {
        let source_prefixed = self.ensure_iri_prefix(source_uri);
        let target_prefixed = self.ensure_iri_prefix(target_uri);
        
        if source_prefixed == target_prefixed { return; }
        
        let mut changes = Vec::new();
        let mut triples_to_process = Vec::new();

        for t in self.memory.asserted_graph.triples() {
            let t = t.unwrap();
            let s_str = crate::rules::extract_str(&t.s());
            let p_str = crate::rules::extract_str(&t.p());
            let o_str = crate::rules::extract_str(&t.o());

            if s_str == source_prefixed || p_str == source_prefixed || o_str == source_prefixed {
                triples_to_process.push((s_str, p_str, o_str));
            }
        }

        for (s, p, o) in triples_to_process {
            changes.push(DarkstarEvent::AxiomRemoved { s: s.clone(), p: p.clone(), o: o.clone() });
            
            let new_s = if s == source_prefixed { target_prefixed.clone() } else { s };
            let new_p = if p == source_prefixed { target_prefixed.clone() } else { p };
            let new_o = if o == source_prefixed { target_prefixed.clone() } else { o };
            
            let ns_term = InferenceEngine::make_term(&new_s);
            let np_term = InferenceEngine::make_term(&new_p);
            let no_term = InferenceEngine::make_term(&new_o);
            
            if !self.memory.asserted_graph.contains(&ns_term, &np_term, &no_term).unwrap() {
                changes.push(DarkstarEvent::AxiomAdded { s: new_s, p: new_p, o: new_o });
            }
        }

        if !changes.is_empty() {
            let batch = DarkstarEvent::Batch(changes);
            self.apply_event(batch.clone());
            self.history.push_change(batch);
        }
    }

    pub fn delete_entity(&mut self, uri: &str) {
        let prefixed = self.ensure_iri_prefix(uri);
        let mut changes = Vec::new();
        let mut triples_to_process = Vec::new();

        for t in self.memory.asserted_graph.triples() {
            let t = t.unwrap();
            let s_str = crate::rules::extract_str(&t.s());
            let p_str = crate::rules::extract_str(&t.p());
            let o_str = crate::rules::extract_str(&t.o());

            if s_str == prefixed || p_str == prefixed || o_str == prefixed {
                triples_to_process.push((s_str, p_str, o_str));
            }
        }

        for (s, p, o) in triples_to_process {
            changes.push(DarkstarEvent::AxiomRemoved { s, p, o });
        }

        if !changes.is_empty() {
            let batch = DarkstarEvent::Batch(changes);
            self.apply_event(batch.clone());
            self.history.push_change(batch);
        }
    }

    pub fn get_comment(&self, uri: &str) -> String {
        let prefixed = self.ensure_iri_prefix(uri);
        let rdfs_comment = "http://www.w3.org/2000/01/rdf-schema#comment";
        let s_term = InferenceEngine::make_term(&prefixed);
        let p_term = InferenceEngine::make_term(rdfs_comment);
        
        for t in self.memory.main_graph.triples_matching(Some(&s_term), Some(&p_term), sophia::api::term::matcher::Any).flatten() {
            return crate::rules::extract_str(&t.o());
        }
        String::new()
    }

    pub fn set_comment(&mut self, uri: &str, comment: &str) {
        let prefixed = self.ensure_iri_prefix(uri);
        let rdfs_comment = "http://www.w3.org/2000/01/rdf-schema#comment";
        
        // Remove existing
        let current = self.get_comment(uri);
        if !current.is_empty() {
            self.remove_assertion(prefixed.clone(), rdfs_comment.to_string(), format!("l:{}", current));
        }
        
        // Add new
        if !comment.is_empty() {
            self.add_assertion(prefixed, rdfs_comment.to_string(), format!("l:{}", comment));
        }
    }

    pub fn has_characteristic(&self, uri: &str, char_uri: &str) -> bool {
        let prefixed = self.ensure_iri_prefix(uri);
        let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        let s_term = InferenceEngine::make_term(&prefixed);
        let p_term = InferenceEngine::make_term(rdf_type);
        let o_term = InferenceEngine::make_term(char_uri);
        
        self.memory.main_graph.contains(&s_term, &p_term, &o_term).unwrap_or(false)
    }

    pub fn toggle_characteristic(&mut self, uri: &str, char_uri: &str) {
        let prefixed = self.ensure_iri_prefix(uri);
        if self.has_characteristic(uri, char_uri) {
            self.remove_assertion(prefixed, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(), format!("i:{}", char_uri));
        } else {
            self.add_assertion(prefixed, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(), format!("i:{}", char_uri));
        }
    }

    fn ensure_iri_prefix(&self, uri: &str) -> String {
        if uri.starts_with("i:") || uri.starts_with("l:") || uri.starts_with("b:") {
            uri.to_string()
        } else {
            format!("i:{}", uri)
        }
    }

    pub fn merge_ontology(&mut self, path: &std::path::Path, rules_settings: &crate::core::settings::ReasonerRules) -> Result<(), Box<dyn std::error::Error>> {
        let data = std::fs::read_to_string(path)?;
        let format = if path.extension().and_then(|e| e.to_str()) == Some("nt") {
            crate::core::io::OntologyFormat::NTriples
        } else {
            crate::core::io::OntologyFormat::Turtle
        };

        // Temporary graph to parse the new data
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
            let s = crate::rules::extract_str(&t.s());
            let p = crate::rules::extract_str(&t.p());
            let o = crate::rules::extract_str(&t.o());
            
            // Only add if not already in asserted_graph
            let s_term = InferenceEngine::make_term(&s);
            let p_term = InferenceEngine::make_term(&p);
            let o_term = InferenceEngine::make_term(&o);
            
            if !self.memory.asserted_graph.contains(&s_term, &p_term, &o_term).unwrap() {
                changes.push(DarkstarEvent::AxiomAdded { s, p, o });
            }
        }

        if !changes.is_empty() {
            let batch = DarkstarEvent::Batch(changes);
            self.apply_event(batch.clone());
            self.history.push_change(batch);
            self.run_reasoning(rules_settings);
        }
        
        Ok(())
    }

    pub fn load_from_file(&mut self, path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
        // Clear current state for a fresh load
        self.memory = EngineMemory::new();
        let data = std::fs::read_to_string(path)?;
        let format = if path.extension().and_then(|e| e.to_str()) == Some("nt") {
            crate::core::io::OntologyFormat::NTriples
        } else {
            crate::core::io::OntologyFormat::Turtle
        };
        self.load_from_string(&data, format)?;
        self.active_file_path = Some(path.to_path_buf());
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

    pub fn save_to_file(&self, path: &std::path::Path, format: crate::core::io::OntologyFormat, include_inferred: bool) -> Result<(), Box<dyn std::error::Error>> {
        let data = self.serialize_to_string(format, include_inferred)?;
        std::fs::write(path, data)?;
        Ok(())
    }
}
