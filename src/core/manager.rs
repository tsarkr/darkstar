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

    pub fn new_ontology(&mut self) {
        self.memory.clear();
        self.active_file_path = None;
        self.history.clear();
        self.is_dirty = false;
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
            self.engine.run_inference(&mut self.memory, rules_settings);
            self.is_dirty = false;
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
            let s = crate::rules::extract_str(&t.s());
            let p = crate::rules::extract_str(&t.p());
            let o = crate::rules::extract_str(&t.o());
            
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
            self.run_reasoning(&rules_settings);
        }
        
        Ok(())
    }

    pub fn load_from_file(&mut self, path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
        self.memory.clear();
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
