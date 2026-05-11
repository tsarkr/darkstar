#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DarkstarEvent {
    AxiomAdded { s: String, p: String, o: String },
    AxiomRemoved { s: String, p: String, o: String },
    Batch(Vec<DarkstarEvent>),
}

pub struct ChangeHistory {
    undo_stack: Vec<DarkstarEvent>,
    redo_stack: Vec<DarkstarEvent>,
}

impl ChangeHistory {
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn push_change(&mut self, change: DarkstarEvent) {
        self.undo_stack.push(change);
        // Any new action clears the redo stack
        self.redo_stack.clear();
    }

    pub fn pop_undo(&mut self) -> Option<DarkstarEvent> {
        if let Some(change) = self.undo_stack.pop() {
            self.redo_stack.push(change.clone());
            Some(change)
        } else {
            None
        }
    }

    pub fn pop_redo(&mut self) -> Option<DarkstarEvent> {
        if let Some(change) = self.redo_stack.pop() {
            self.undo_stack.push(change.clone());
            Some(change)
        } else {
            None
        }
    }

    pub fn is_undo_empty(&self) -> bool {
        self.undo_stack.is_empty()
    }

    pub fn is_redo_empty(&self) -> bool {
        self.redo_stack.is_empty()
    }

    pub fn get_undo_stack(&self) -> &[DarkstarEvent] {
        &self.undo_stack
    }
}
