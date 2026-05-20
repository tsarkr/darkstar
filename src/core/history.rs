#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DarkstarEvent {
    AxiomAdded { s: String, p: String, o: String },
    AxiomRemoved { s: String, p: String, o: String },
    Batch(Vec<DarkstarEvent>),
}

impl DarkstarEvent {
    pub fn reverse(&self) -> Self {
        match self {
            DarkstarEvent::AxiomAdded { s, p, o } => DarkstarEvent::AxiomRemoved { s: s.clone(), p: p.clone(), o: o.clone() },
            DarkstarEvent::AxiomRemoved { s, p, o } => DarkstarEvent::AxiomAdded { s: s.clone(), p: p.clone(), o: o.clone() },
            DarkstarEvent::Batch(events) => {
                let mut rev_events = events.iter().map(|e| e.reverse()).collect::<Vec<_>>();
                rev_events.reverse();
                DarkstarEvent::Batch(rev_events)
            }
        }
    }
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

    pub fn get_redo_stack(&self) -> &[DarkstarEvent] {
        &self.redo_stack
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}
