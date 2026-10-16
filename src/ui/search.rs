use std::collections::HashMap;
use sophia::api::prelude::*;
use sophia::api::term::matcher::Any;
use crate::ui::app::DarkstarApp;
use crate::rules::engine::InferenceEngine;

impl DarkstarApp {
    /// Builds (lazily, once per ontology revision) a map of entity -> lowercased rdfs:label text,
    /// so search can match human-readable labels (e.g. Korean names) and not only URIs.
    pub fn ensure_label_index(&mut self) {
        if self.label_index_revision == self.manager.revision { return; }
        let rdfs_label = InferenceEngine::make_term(crate::vocab::RDFS_LABEL);
        let mut index: HashMap<String, String> = HashMap::new();
        for t in self.manager.memory.main_graph.triples_matching(Any, Some(&rdfs_label), Any).flatten() {
            let Some(lex) = t.o().lexical_form() else { continue };
            let entry = index.entry(crate::rules::extract_str(&t.s())).or_default();
            if !entry.is_empty() { entry.push('\n'); }
            entry.push_str(&lex.to_lowercase());
        }
        self.label_index = index;
        self.label_index_revision = self.manager.revision;
    }

    /// Zero-allocation case-insensitive substring check (`needle_lower` must already be lowercased).
    #[inline]
    pub fn contains_case_insensitive(haystack: &str, needle_lower: &str) -> bool {
        if needle_lower.is_empty() {
            return true;
        }
        if needle_lower.len() > haystack.len() {
            return false;
        }
        if haystack.is_ascii() && needle_lower.is_ascii() {
            let n = needle_lower.as_bytes();
            haystack.as_bytes().windows(n.len()).any(|w| w.eq_ignore_ascii_case(n))
        } else if haystack.contains(needle_lower) {
            true
        } else {
            haystack.to_lowercase().contains(needle_lower)
        }
    }

    /// `query` must already be lowercased. Matches the display label, the URI and any rdfs:label.
    /// Call `ensure_label_index` first when the query is non-empty.
    pub fn matches_search(&self, uri: &str, display_label: &str, query: &str) -> bool {
        if query.is_empty() { return true; }
        Self::contains_case_insensitive(display_label, query)
            || Self::contains_case_insensitive(uri, query)
            || self.label_index.get(uri).is_some_and(|l| l.contains(query))
    }

    /// `query` must already be lowercased. Matches the URI (which includes its local name) and any rdfs:label
    /// without allocating a display label `String`.
    #[inline]
    pub fn matches_uri_search(&self, uri: &str, query: &str) -> bool {
        if query.is_empty() { return true; }
        Self::contains_case_insensitive(uri, query)
            || self.label_index.get(uri).is_some_and(|l| l.contains(query))
    }
}
