//! Small lookups over a Darkstar graph using Darkstar term strings (`i:…`, `l:…`, `b:…`).
//! Used by the editors, explanations and queries; the reasoner rules keep their own
//! term-level code for speed.

use sophia::api::prelude::*;
use sophia::api::term::matcher::Any;
use sophia::api::term::SimpleTerm;
use crate::memory::DarkstarGraph;
use crate::rules::engine::InferenceEngine;
use crate::rules::extract_str;

/// Sophia term for a Darkstar term string.
pub fn term(s: &str) -> SimpleTerm<'static> {
    InferenceEngine::make_term(s)
}

/// Objects `o` of `(s, p, o)`.
pub fn objects(g: &DarkstarGraph, s: &str, p: &str) -> Vec<String> {
    g.triples_matching(Some(&term(s)), Some(&term(p)), Any).flatten().map(|t| extract_str(&t.o())).collect()
}

/// Subjects `s` of `(s, p, o)`.
pub fn subjects(g: &DarkstarGraph, p: &str, o: &str) -> Vec<String> {
    g.triples_matching(Any, Some(&term(p)), Some(&term(o))).flatten().map(|t| extract_str(&t.s())).collect()
}

/// Whether `(s, p, o)` is in the graph.
pub fn has(g: &DarkstarGraph, s: &str, p: &str, o: &str) -> bool {
    g.contains(term(s), term(p), term(o)).unwrap_or(false)
}

/// Whether the graph has at least one triple.
pub fn is_empty(g: &DarkstarGraph) -> bool {
    g.triples().next().is_none()
}
