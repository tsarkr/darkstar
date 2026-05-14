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
    pub is_consistent: bool,
    pub inconsistencies: Vec<Inconsistency>,
}

impl EngineMemory {
    pub fn new() -> Self {
        Self {
            main_graph: FastGraph::new(),
            delta_graph: FastGraph::new(),
            asserted_graph: FastGraph::new(),
            inferred_graph: FastGraph::new(),
            is_consistent: true,
            inconsistencies: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.main_graph = FastGraph::new();
        self.delta_graph = FastGraph::new();
        self.asserted_graph = FastGraph::new();
        self.inferred_graph = FastGraph::new();
        self.is_consistent = true;
        self.inconsistencies.clear();
    }
}
