use sophia::inmem::graph::FastGraph;

/// Type alias for the memory-efficient graph structure.
pub type DarkstarGraph = FastGraph;

/// Inferred triple structure that uniquely owns its string data.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Fact {
    pub s: String,
    pub p: String,
    pub o: String,
}

/// Represents a logical contradiction detected during reasoning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inconsistency {
    pub rule_name: String,
    pub conflicting_triples: Vec<Fact>,
}

/// The central memory model for the inference engine.
pub struct EngineMemory {
    pub main_graph: DarkstarGraph,
    pub delta_graph: DarkstarGraph,
    pub asserted_graph: DarkstarGraph,
    pub inferred_graph: DarkstarGraph,
    /// Triples from `owl:imports` (read-only; reasoned over but never saved with the ontology).
    pub imported_graph: DarkstarGraph,
    pub is_consistent: bool,
    pub inconsistencies: Vec<Inconsistency>,
    /// Iteration in which each inferred triple was first derived (keys normalized with
    /// `normalize_fact`). Explanations only accept premises from strictly earlier
    /// iterations, which guarantees they bottom out in asserted/imported triples.
    pub inferred_level: std::collections::HashMap<Fact, u32>,
    /// Highest level used so far (incremental runs continue from here).
    pub max_level: u32,
}

/// Canonical string form of a term (`Fact` fields may omit the `i:` prefix for IRIs).
pub fn normalize_term(t: &str) -> String {
    if t.starts_with("i:") || t.starts_with("l:") || t.starts_with("b:") { t.to_string() } else { format!("i:{}", t) }
}

pub fn normalize_term_owned(t: String) -> String {
    if t.starts_with("i:") || t.starts_with("l:") || t.starts_with("b:") { t } else { format!("i:{}", t) }
}

pub fn normalize_fact(f: &Fact) -> Fact {
    Fact { s: normalize_term(&f.s), p: normalize_term(&f.p), o: normalize_term(&f.o) }
}

pub fn normalize_fact_owned(f: Fact) -> Fact {
    Fact { s: normalize_term_owned(f.s), p: normalize_term_owned(f.p), o: normalize_term_owned(f.o) }
}

impl EngineMemory {
    pub fn new() -> Self {
        Self {
            main_graph: FastGraph::new(),
            delta_graph: FastGraph::new(),
            asserted_graph: FastGraph::new(),
            inferred_graph: FastGraph::new(),
            imported_graph: FastGraph::new(),
            is_consistent: true,
            inconsistencies: Vec::new(),
            inferred_level: std::collections::HashMap::new(),
            max_level: 0,
        }
    }

    pub fn has_imports(&self) -> bool {
        use sophia::api::graph::Graph;
        self.imported_graph.triples().next().is_some()
    }

    /// asserted ∪ imported (i.e. everything except reasoner output)
    pub fn asserted_plus_imported(&self) -> DarkstarGraph {
        use sophia::api::graph::{Graph, MutableGraph};
        use sophia::api::triple::Triple;
        let mut g = self.asserted_graph.clone();
        for t in self.imported_graph.triples().flatten() {
            let _ = g.insert(t.s(), t.p(), t.o());
        }
        g
    }

    pub fn clear(&mut self) {
        self.main_graph = FastGraph::new();
        self.delta_graph = FastGraph::new();
        self.asserted_graph = FastGraph::new();
        self.inferred_graph = FastGraph::new();
        self.imported_graph = FastGraph::new();
        self.is_consistent = true;
        self.inconsistencies.clear();
        self.inferred_level.clear();
        self.max_level = 0;
    }
}
