pub mod engine;
pub mod explain;

use crate::memory::{DarkstarGraph, Fact, Inconsistency};
use sophia::api::prelude::*;
use sophia::api::term::matcher::Any;
use std::collections::HashSet;
use std::collections::HashMap;
use sophia::api::term::{SimpleTerm, IriRef};
use crate::rules::engine::InferenceEngine;

// Vocabulary constants live in `crate::vocab`; re-exported so `rules::RDF_TYPE` etc. keep working.
pub use crate::vocab::*;

/// Helper to safely extract an interned string from a `sophia` Term.
/// Blank nodes now preserve their unique ID to prevent massive collisions.
pub fn extract_str<T: Term>(term: &T) -> String {
    if let Some(iri) = term.iri() {
        format!("i:{}", iri)
    } else if let Some(bnode) = term.bnode_id() {
        format!("b:{}", bnode.as_str())
    } else if let Some(lex) = term.lexical_form() {
        if let Some(lang) = term.language_tag() {
            format!("l:{}@{}", lex, lang.as_str())
        } else if let Some(dt) = term.datatype() {
            let dt_str = dt.as_str();
            if dt_str == "http://www.w3.org/2001/XMLSchema#string" {
                format!("l:{}", lex)
            } else {
                format!("l:{}^^{}", lex, dt_str)
            }
        } else {
            format!("l:{}", lex)
        }
    } else {
        "b:anon".to_string()
    }
}

/// Recursively extracts all elements from an RDF list starting at `head`.
pub(crate) fn get_rdf_list<T: Term>(main: &DarkstarGraph, head: &T) -> Vec<String> {
    let mut elements = Vec::new();
    let mut current = extract_str(head);
    let mut visited = HashSet::new(); // Prevent circular lists
    
    let rdf_first = SimpleTerm::Iri(IriRef::new_unchecked(RDF_FIRST.into()));
    let rdf_rest = SimpleTerm::Iri(IriRef::new_unchecked(RDF_REST.into()));
    let rdf_nil = RDF_NIL.to_string();

    while current != rdf_nil && !visited.contains(&current) {
        visited.insert(current.clone());
        let head_term = InferenceEngine::make_term(&current);
        
        if let Some(Ok(t)) = main.triples_matching(Some(&head_term), Some(&rdf_first), Any).next() {
            elements.push(extract_str(&t.o()));
        } else { break; }

        if let Some(Ok(t)) = main.triples_matching(Some(&head_term), Some(&rdf_rest), Any).next() {
            current = extract_str(&t.o());
        } else { break; }
        
        if visited.len() > 1000 { break; } // Safety limit
    }
    elements
}

#[inline]
fn iri(s: &'static str) -> SimpleTerm<'static> {
    SimpleTerm::Iri(IriRef::new_unchecked(s.into()))
}

const I_RDF_TYPE: &str = "i:http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const I_RDFS_SUBCLASS_OF: &str = "i:http://www.w3.org/2000/01/rdf-schema#subClassOf";
const I_RDFS_SUBPROPERTY_OF: &str = "i:http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const I_OWL_SAME_AS: &str = "i:http://www.w3.org/2002/07/owl#sameAs";

/// Builds a `subject -> [objects]` lookup table for all triples with predicate `pred`.
/// Schema-level tables (subClassOf, domain, range, ...) are tiny compared to instance data,
/// so rules use them to query the delta only for relevant predicates/classes
/// instead of scanning every delta triple.
fn pred_map(g: &DarkstarGraph, pred: &SimpleTerm<'_>) -> HashMap<String, Vec<String>> {
    let mut m: HashMap<String, Vec<String>> = HashMap::new();
    for t in g.triples_matching(Any, Some(pred), Any).flatten() {
        m.entry(extract_str(&t.s())).or_default().push(extract_str(&t.o()));
    }
    m
}

/// Builds an `object -> [subjects]` lookup table for all triples with predicate `pred`.
fn pred_map_inv(g: &DarkstarGraph, pred: &SimpleTerm<'_>) -> HashMap<String, Vec<String>> {
    let mut m: HashMap<String, Vec<String>> = HashMap::new();
    for t in g.triples_matching(Any, Some(pred), Any).flatten() {
        m.entry(extract_str(&t.o())).or_default().push(extract_str(&t.s()));
    }
    m
}

#[inline]
fn has_pred(g: &DarkstarGraph, pred: &SimpleTerm<'_>) -> bool {
    g.triples_matching(Any, Some(pred), Any).next().is_some()
}

pub trait InferenceRule: Sync + Send {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency>;
}

/// cax-sco: Class Subsumption
pub struct CaxSco;
impl InferenceRule for CaxSco {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdf_type = iri(RDF_TYPE);
        let rdfs_subclass_of = iri(RDFS_SUBCLASS_OF);
        let full_scan = std::ptr::eq(main, delta);

        // (x type c1) in delta, (c1 subClassOf c2) in main
        let supers = pred_map(main, &rdfs_subclass_of);
        for (c1, sups) in &supers {
            let c1_term = InferenceEngine::make_term(c1);
            let sup_terms: Vec<(SimpleTerm<'static>, &String)> = sups.iter()
                .map(|c2| (InferenceEngine::make_term(c2), c2))
                .collect();
            for t in delta.triples_matching(Any, Some(&rdf_type), Some(&c1_term)).flatten() {
                let mut x_str: Option<String> = None;
                for (c2_term, c2) in &sup_terms {
                    if !main.contains(t.s(), &rdf_type, c2_term).unwrap_or(false) {
                        let x = x_str.get_or_insert_with(|| extract_str(&t.s())).clone();
                        new_facts.insert(Fact { s: x, p: I_RDF_TYPE.to_string(), o: (*c2).clone() });
                    }
                }
            }
        }

        // (c1 subClassOf c2) in delta, (x type c1) in main
        if !full_scan {
            for t in delta.triples_matching(Any, Some(&rdfs_subclass_of), Any).flatten() {
                let c2_term = t.o();
                let mut c2_str: Option<String> = None;
                for t2 in main.triples_matching(Any, Some(&rdf_type), Some(t.s())).flatten() {
                    if !main.contains(t2.s(), &rdf_type, c2_term).unwrap_or(false) {
                        let c2 = c2_str.get_or_insert_with(|| extract_str(c2_term)).clone();
                        new_facts.insert(Fact { s: extract_str(&t2.s()), p: I_RDF_TYPE.to_string(), o: c2 });
                    }
                }
            }
        }
        Ok(new_facts)
    }
}

/// prp-spo1: Property Subsumption
pub struct PrpSpo1;
impl InferenceRule for PrpSpo1 {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdfs_subproperty_of = iri(RDFS_SUBPROPERTY_OF);
        let self_key = I_RDFS_SUBPROPERTY_OF;
        let full_scan = std::ptr::eq(main, delta);

        // (x p1 y) in delta, (p1 subPropertyOf p2) in main
        for (p1, sups) in &pred_map(main, &rdfs_subproperty_of) {
            if p1 == self_key { continue; }
            let p1_term = InferenceEngine::make_term(p1);
            let sup_terms: Vec<(SimpleTerm<'static>, &String)> = sups.iter()
                .map(|p2| (InferenceEngine::make_term(p2), p2))
                .collect();
            for t in delta.triples_matching(Any, Some(&p1_term), Any).flatten() {
                let mut so_str: Option<(String, String)> = None;
                for (p2_term, p2) in &sup_terms {
                    if !main.contains(t.s(), p2_term, t.o()).unwrap_or(false) {
                        let (s, o) = so_str.get_or_insert_with(|| (extract_str(&t.s()), extract_str(&t.o()))).clone();
                        new_facts.insert(Fact { s, p: (*p2).clone(), o });
                    }
                }
            }
        }

        // (p1 subPropertyOf p2) in delta, (x p1 y) in main
        if !full_scan {
            for t in delta.triples_matching(Any, Some(&rdfs_subproperty_of), Any).flatten() {
                let p2_term = t.o();
                let mut p2_str: Option<String> = None;
                for t2 in main.triples_matching(Any, Some(t.s()), Any).flatten() {
                    if !main.contains(t2.s(), p2_term, t2.o()).unwrap_or(false) {
                        let p2 = p2_str.get_or_insert_with(|| extract_str(p2_term)).clone();
                        new_facts.insert(Fact { s: extract_str(&t2.s()), p: p2, o: extract_str(&t2.o()) });
                    }
                }
            }
        }
        Ok(new_facts)
    }
}

/// prp-trp: Transitive Property
pub struct PrpTrp;
impl InferenceRule for PrpTrp {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdf_type = iri(RDF_TYPE);
        let owl_transitive_property = iri(OWL_TRANSITIVE_PROPERTY);
        let full_scan = std::ptr::eq(main, delta);

        let transitive_props: Vec<_> = main.triples_matching(Any, Some(&rdf_type), Some(&owl_transitive_property))
            .flatten()
            .map(|t| (t.s().to_owned(), extract_str(&t.s())))
            .collect();
        
        for (p_term, p_str) in transitive_props {
            for d in delta.triples_matching(Any, Some(&p_term), Any).flatten() {
                let mut ds_str: Option<String> = None;
                for m in main.triples_matching(Some(d.o()), Some(&p_term), Any).flatten() {
                    if !main.contains(d.s(), &p_term, m.o()).unwrap_or(false) {
                        let s = ds_str.get_or_insert_with(|| extract_str(&d.s())).clone();
                        new_facts.insert(Fact { s, p: p_str.clone(), o: extract_str(&m.o()) });
                    }
                }
                if !full_scan {
                    let mut do_str: Option<String> = None;
                    for m in main.triples_matching(Any, Some(&p_term), Some(d.s())).flatten() {
                        if !main.contains(m.s(), &p_term, d.o()).unwrap_or(false) {
                            let o = do_str.get_or_insert_with(|| extract_str(&d.o())).clone();
                            new_facts.insert(Fact { s: extract_str(&m.s()), p: p_str.clone(), o });
                        }
                    }
                }
            }
        }
        Ok(new_facts)
    }
}

pub struct PrpDom;
impl InferenceRule for PrpDom {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdfs_domain = iri(RDFS_DOMAIN);
        let rdf_type = iri(RDF_TYPE);
        let self_key = "i:http://www.w3.org/2000/01/rdf-schema#domain";
        let full_scan = std::ptr::eq(main, delta);

        // (x p y) in delta, (p domain c) in main
        for (p, classes) in &pred_map(main, &rdfs_domain) {
            if p == self_key { continue; }
            let p_term = InferenceEngine::make_term(p);
            let class_terms: Vec<(SimpleTerm<'static>, &String)> = classes.iter()
                .map(|c| (InferenceEngine::make_term(c), c))
                .collect();
            let mut prev_s: Option<*const SimpleTerm<'static>> = None;
            for t in delta.triples_matching(Any, Some(&p_term), Any).flatten() {
                let s_ptr = t[0] as *const SimpleTerm<'static>;
                if prev_s == Some(s_ptr) { continue; }
                prev_s = Some(s_ptr);
                let mut s_str: Option<String> = None;
                for (c_term, c) in &class_terms {
                    if !main.contains(t[0], &rdf_type, c_term).unwrap_or(false) {
                        let s = s_str.get_or_insert_with(|| extract_str(t[0])).clone();
                        new_facts.insert(Fact { s, p: I_RDF_TYPE.to_string(), o: (*c).clone() });
                    }
                }
            }
        }

        // (p domain c) in delta, (x p y) in main
        if !full_scan {
            for t in delta.triples_matching(Any, Some(&rdfs_domain), Any).flatten() {
                let c_term = t.o();
                let mut c_str: Option<String> = None;
                for t2 in main.triples_matching(Any, Some(t.s()), Any).flatten() {
                    if !main.contains(t2.s(), &rdf_type, c_term).unwrap_or(false) {
                        let c = c_str.get_or_insert_with(|| extract_str(c_term)).clone();
                        new_facts.insert(Fact { s: extract_str(&t2.s()), p: I_RDF_TYPE.to_string(), o: c });
                    }
                }
            }
        }
        Ok(new_facts)
    }
}

pub struct PrpRng;
impl InferenceRule for PrpRng {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdfs_range = iri(RDFS_RANGE);
        let rdf_type = iri(RDF_TYPE);
        let self_key = "i:http://www.w3.org/2000/01/rdf-schema#range";
        let full_scan = std::ptr::eq(main, delta);

        // (x p y) in delta, (p range c) in main
        for (p, classes) in &pred_map(main, &rdfs_range) {
            if p == self_key { continue; }
            let p_term = InferenceEngine::make_term(p);
            let class_terms: Vec<(SimpleTerm<'static>, &String)> = classes.iter()
                .map(|c| (InferenceEngine::make_term(c), c))
                .collect();
            let mut seen_o: HashSet<*const SimpleTerm<'static>> = HashSet::new();
            for t in delta.triples_matching(Any, Some(&p_term), Any).flatten() {
                let o_ptr = t[2] as *const SimpleTerm<'static>;
                if !seen_o.insert(o_ptr) { continue; }
                let mut o_str: Option<String> = None;
                for (c_term, c) in &class_terms {
                    if !main.contains(t[2], &rdf_type, c_term).unwrap_or(false) {
                        let o = o_str.get_or_insert_with(|| extract_str(t[2])).clone();
                        new_facts.insert(Fact { s: o, p: I_RDF_TYPE.to_string(), o: (*c).clone() });
                    }
                }
            }
        }

        // (p range c) in delta, (x p y) in main
        if !full_scan {
            for t in delta.triples_matching(Any, Some(&rdfs_range), Any).flatten() {
                let c_term = t.o();
                let mut c_str: Option<String> = None;
                for t2 in main.triples_matching(Any, Some(t.s()), Any).flatten() {
                    if !main.contains(t2.o(), &rdf_type, c_term).unwrap_or(false) {
                        let c = c_str.get_or_insert_with(|| extract_str(c_term)).clone();
                        new_facts.insert(Fact { s: extract_str(&t2.o()), p: I_RDF_TYPE.to_string(), o: c });
                    }
                }
            }
        }
        Ok(new_facts)
    }
}

pub struct PrpInv;
impl InferenceRule for PrpInv {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let inverse_of = iri(OWL_INVERSE_OF);
        let self_key = "i:http://www.w3.org/2002/07/owl#inverseOf";
        let full_scan = std::ptr::eq(main, delta);

        // (x p y) in delta, (p inverseOf q) in main  =>  (y q x)
        for (p, qs) in &pred_map(main, &inverse_of) {
            if p == self_key { continue; }
            let p_term = InferenceEngine::make_term(p);
            let q_terms: Vec<(SimpleTerm<'static>, &String)> = qs.iter()
                .map(|q| (InferenceEngine::make_term(q), q))
                .collect();
            for t in delta.triples_matching(Any, Some(&p_term), Any).flatten() {
                let mut so_str: Option<(String, String)> = None;
                for (q_term, q) in &q_terms {
                    if !main.contains(t.o(), q_term, t.s()).unwrap_or(false) {
                        let (s, o) = so_str.get_or_insert_with(|| (extract_str(&t.s()), extract_str(&t.o()))).clone();
                        new_facts.insert(Fact { s: o, p: (*q).clone(), o: s });
                    }
                }
            }
        }
        // (x q y) in delta, (p inverseOf q) in main  =>  (y p x)
        for (q, ps) in &pred_map_inv(main, &inverse_of) {
            if q == self_key { continue; }
            let q_term = InferenceEngine::make_term(q);
            let p_terms: Vec<(SimpleTerm<'static>, &String)> = ps.iter()
                .map(|p| (InferenceEngine::make_term(p), p))
                .collect();
            for t in delta.triples_matching(Any, Some(&q_term), Any).flatten() {
                let mut so_str: Option<(String, String)> = None;
                for (p_term, p) in &p_terms {
                    if !main.contains(t.o(), p_term, t.s()).unwrap_or(false) {
                        let (s, o) = so_str.get_or_insert_with(|| (extract_str(&t.s()), extract_str(&t.o()))).clone();
                        new_facts.insert(Fact { s: o, p: (*p).clone(), o: s });
                    }
                }
            }
        }

        // (p1 inverseOf p2) in delta
        if !full_scan {
            for t in delta.triples_matching(Any, Some(&inverse_of), Any).flatten() {
                let p1 = extract_str(&t.s());
                let p2 = extract_str(&t.o());
                for t2 in main.triples_matching(Any, Some(t.s()), Any).flatten() {
                    if !main.contains(t2.o(), t.o(), t2.s()).unwrap_or(false) {
                        new_facts.insert(Fact { s: extract_str(&t2.o()), p: p2.clone(), o: extract_str(&t2.s()) });
                    }
                }
                for t2 in main.triples_matching(Any, Some(t.o()), Any).flatten() {
                    if !main.contains(t2.o(), t.s(), t2.s()).unwrap_or(false) {
                        new_facts.insert(Fact { s: extract_str(&t2.o()), p: p1.clone(), o: extract_str(&t2.s()) });
                    }
                }
            }
        }
        Ok(new_facts)
    }
}

pub struct EqRepS;
impl InferenceRule for EqRepS {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let same_as = iri(OWL_SAME_AS);
        let full_scan = std::ptr::eq(main, delta);

        // (s sameAs s2) in delta, (s p o) in main
        for t in delta.triples_matching(Any, Some(&same_as), Any).flatten() {
            let s2_term = t.o();
            let s2 = extract_str(s2_term);
            for t2 in main.triples_matching(Some(t.s()), Any, Any).flatten() {
                if !main.contains(s2_term, t2.p(), t2.o()).unwrap_or(false) {
                    new_facts.insert(Fact { s: s2.clone(), p: extract_str(&t2.p()), o: extract_str(&t2.o()) });
                }
            }
        }

        // (s p o) in delta, (s sameAs s2) in main
        if !full_scan {
            for (x, ys) in &pred_map(main, &same_as) {
                let x_term = InferenceEngine::make_term(x);
                let y_terms: Vec<(SimpleTerm<'static>, &String)> = ys.iter()
                    .map(|y| (InferenceEngine::make_term(y), y))
                    .collect();
                for t in delta.triples_matching(Some(&x_term), Any, Any).flatten() {
                    if *t.p() == same_as { continue; }
                    let mut po_str: Option<(String, String)> = None;
                    for (y_term, y) in &y_terms {
                        if !main.contains(y_term, t.p(), t.o()).unwrap_or(false) {
                            let (p, o) = po_str.get_or_insert_with(|| (extract_str(&t.p()), extract_str(&t.o()))).clone();
                            new_facts.insert(Fact { s: (*y).clone(), p, o });
                        }
                    }
                }
            }
        }
        Ok(new_facts)
    }
}

pub struct CaxEqc;
impl InferenceRule for CaxEqc {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdf_type = iri(RDF_TYPE);
        let owl_equiv = iri(OWL_EQUIVALENT_CLASS);
        if !has_pred(main, &owl_equiv) { return Ok(new_facts); }
        let full_scan = std::ptr::eq(main, delta);

        // (x type c1) in delta, (c1 equivalentClass c2) in main => (x type c2)
        for (c1, c2s) in &pred_map(main, &owl_equiv) {
            let c1_term = InferenceEngine::make_term(c1);
            let c2_terms: Vec<(SimpleTerm<'static>, &String)> = c2s.iter()
                .map(|c2| (InferenceEngine::make_term(c2), c2))
                .collect();
            for t in delta.triples_matching(Any, Some(&rdf_type), Some(&c1_term)).flatten() {
                let mut x_str: Option<String> = None;
                for (c2_term, c2) in &c2_terms {
                    if !main.contains(t.s(), &rdf_type, c2_term).unwrap_or(false) {
                        let x = x_str.get_or_insert_with(|| extract_str(&t.s())).clone();
                        new_facts.insert(Fact { s: x, p: I_RDF_TYPE.to_string(), o: (*c2).clone() });
                    }
                }
            }
        }

        // (x type c2) in delta, (c1 equivalentClass c2) in main => (x type c1)
        for (c2, c1s) in &pred_map_inv(main, &owl_equiv) {
            let c2_term = InferenceEngine::make_term(c2);
            let c1_terms: Vec<(SimpleTerm<'static>, &String)> = c1s.iter()
                .map(|c1| (InferenceEngine::make_term(c1), c1))
                .collect();
            for t in delta.triples_matching(Any, Some(&rdf_type), Some(&c2_term)).flatten() {
                let mut x_str: Option<String> = None;
                for (c1_term, c1) in &c1_terms {
                    if !main.contains(t.s(), &rdf_type, c1_term).unwrap_or(false) {
                        let x = x_str.get_or_insert_with(|| extract_str(&t.s())).clone();
                        new_facts.insert(Fact { s: x, p: I_RDF_TYPE.to_string(), o: (*c1).clone() });
                    }
                }
            }
        }

        // (c1 equivalentClass c2) in delta
        if !full_scan {
            for t in delta.triples_matching(Any, Some(&owl_equiv), Any).flatten() {
                let c1 = extract_str(&t.s());
                let c2 = extract_str(&t.o());
                for t2 in main.triples_matching(Any, Some(&rdf_type), Some(t.s())).flatten() {
                    if !main.contains(t2.s(), &rdf_type, t.o()).unwrap_or(false) {
                        new_facts.insert(Fact { s: extract_str(&t2.s()), p: I_RDF_TYPE.to_string(), o: c2.clone() });
                    }
                }
                for t2 in main.triples_matching(Any, Some(&rdf_type), Some(t.o())).flatten() {
                    if !main.contains(t2.s(), &rdf_type, t.s()).unwrap_or(false) {
                        new_facts.insert(Fact { s: extract_str(&t2.s()), p: I_RDF_TYPE.to_string(), o: c1.clone() });
                    }
                }
            }
        }
        Ok(new_facts)
    }
}

/// eq-ref: `x owl:sameAs x` for every term. Intentionally NOT registered in the engine:
/// on large data it adds one triple per distinct term (~290k on CIDOC_Timeline_Mappings.ttl)
/// and makes every eq-rep rule rescan the graph, while deriving nothing other rules need.
/// Kept so it can be re-enabled explicitly.
#[allow(dead_code)]
pub struct EqRef;
impl InferenceRule for EqRef {
    fn apply(&self, _main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        for t in delta.triples().flatten() {
            let s = extract_str(&t.s());
            let p = extract_str(&t.p());
            let o = extract_str(&t.o());
            new_facts.insert(Fact { s: s.clone(), p: I_OWL_SAME_AS.to_string(), o: s });
            new_facts.insert(Fact { s: p.clone(), p: I_OWL_SAME_AS.to_string(), o: p });
            new_facts.insert(Fact { s: o.clone(), p: I_OWL_SAME_AS.to_string(), o });
        }
        Ok(new_facts)
    }
}

pub struct EqSym;
impl InferenceRule for EqSym {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let same_as = iri(OWL_SAME_AS);
        for t in delta.triples_matching(Any, Some(&same_as), Any).flatten() {
            if !main.contains(t.o(), &same_as, t.s()).unwrap_or(false) {
                new_facts.insert(Fact { s: extract_str(&t.o()), p: I_OWL_SAME_AS.to_string(), o: extract_str(&t.s()) });
            }
        }
        Ok(new_facts)
    }
}

pub struct EqTrans;
impl InferenceRule for EqTrans {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let same_as = iri(OWL_SAME_AS);
        let full_scan = std::ptr::eq(main, delta);
        for t in delta.triples_matching(Any, Some(&same_as), Any).flatten() {
            let mut x_str: Option<String> = None;
            for t2 in main.triples_matching(Some(t.o()), Some(&same_as), Any).flatten() {
                if !main.contains(t.s(), &same_as, t2.o()).unwrap_or(false) {
                    let x = x_str.get_or_insert_with(|| extract_str(&t.s())).clone();
                    new_facts.insert(Fact { s: x, p: I_OWL_SAME_AS.to_string(), o: extract_str(&t2.o()) });
                }
            }
            if !full_scan {
                let mut z_str: Option<String> = None;
                for t2 in main.triples_matching(Any, Some(&same_as), Some(t.s())).flatten() {
                    if !main.contains(t2.s(), &same_as, t.o()).unwrap_or(false) {
                        let z = z_str.get_or_insert_with(|| extract_str(&t.o())).clone();
                        new_facts.insert(Fact { s: extract_str(&t2.s()), p: I_OWL_SAME_AS.to_string(), o: z });
                    }
                }
            }
        }
        Ok(new_facts)
    }
}

pub struct EqRepP;
impl InferenceRule for EqRepP {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let same_as = iri(OWL_SAME_AS);
        let same_as_key = I_OWL_SAME_AS;
        let full_scan = std::ptr::eq(main, delta);

        // (p sameAs q) in delta, (s p o) in main
        for t in delta.triples_matching(Any, Some(&same_as), Any).flatten() {
            let q_term = t.o();
            let q = extract_str(q_term);
            for t2 in main.triples_matching(Any, Some(t.s()), Any).flatten() {
                if !main.contains(t2.s(), q_term, t2.o()).unwrap_or(false) {
                    new_facts.insert(Fact { s: extract_str(&t2.s()), p: q.clone(), o: extract_str(&t2.o()) });
                }
            }
        }

        // (s p o) in delta, (p sameAs q) in main
        if !full_scan {
            for (x, ys) in &pred_map(main, &same_as) {
                if x == same_as_key { continue; }
                let x_term = InferenceEngine::make_term(x);
                let y_terms: Vec<(SimpleTerm<'static>, &String)> = ys.iter()
                    .map(|y| (InferenceEngine::make_term(y), y))
                    .collect();
                for t in delta.triples_matching(Any, Some(&x_term), Any).flatten() {
                    let mut so_str: Option<(String, String)> = None;
                    for (y_term, y) in &y_terms {
                        if !main.contains(t.s(), y_term, t.o()).unwrap_or(false) {
                            let (s, o) = so_str.get_or_insert_with(|| (extract_str(&t.s()), extract_str(&t.o()))).clone();
                            new_facts.insert(Fact { s, p: (*y).clone(), o });
                        }
                    }
                }
            }
        }
        Ok(new_facts)
    }
}

pub struct EqRepO;
impl InferenceRule for EqRepO {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let same_as = iri(OWL_SAME_AS);
        let full_scan = std::ptr::eq(main, delta);

        // (o sameAs o2) in delta, (s p o) in main
        for t in delta.triples_matching(Any, Some(&same_as), Any).flatten() {
            let o_prime_term = t.o();
            let o_prime = extract_str(o_prime_term);
            for t2 in main.triples_matching(Any, Any, Some(t.s())).flatten() {
                if !main.contains(t2.s(), t2.p(), o_prime_term).unwrap_or(false) {
                    new_facts.insert(Fact { s: extract_str(&t2.s()), p: extract_str(&t2.p()), o: o_prime.clone() });
                }
            }
        }

        // (s p o) in delta, (o sameAs o2) in main
        if !full_scan {
            for (x, ys) in &pred_map(main, &same_as) {
                let x_term = InferenceEngine::make_term(x);
                let y_terms: Vec<(SimpleTerm<'static>, &String)> = ys.iter()
                    .map(|y| (InferenceEngine::make_term(y), y))
                    .collect();
                for t in delta.triples_matching(Any, Any, Some(&x_term)).flatten() {
                    if *t.p() == same_as { continue; }
                    let mut sp_str: Option<(String, String)> = None;
                    for (y_term, y) in &y_terms {
                        if !main.contains(t.s(), t.p(), y_term).unwrap_or(false) {
                            let (s, p) = sp_str.get_or_insert_with(|| (extract_str(&t.s()), extract_str(&t.p()))).clone();
                            new_facts.insert(Fact { s, p, o: (*y).clone() });
                        }
                    }
                }
            }
        }
        Ok(new_facts)
    }
}

pub struct PrpIrp;
impl InferenceRule for PrpIrp {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let rdf_type = iri(RDF_TYPE);
        let irreflexive = iri(OWL_IRREFLEXIVE_PROPERTY);
        for tp in main.triples_matching(Any, Some(&rdf_type), Some(&irreflexive)).flatten() {
            for t in delta.triples_matching(Any, Some(tp.s()), Any).flatten() {
                if t.s() == t.o() {
                    return Err(Inconsistency {
                        rule_name: "prp-irp".to_string(),
                        conflicting_triples: vec![
                            Fact { s: extract_str(&t.s()), p: extract_str(&t.p()), o: extract_str(&t.o()) },
                            Fact { s: extract_str(&t.p()), p: RDF_TYPE.to_string(), o: OWL_IRREFLEXIVE_PROPERTY.to_string() },
                        ],
                    });
                }
            }
        }
        Ok(HashSet::new())
    }
}

pub struct PrpAsyp;
impl InferenceRule for PrpAsyp {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let rdf_type = iri(RDF_TYPE);
        let asymmetric = iri(OWL_ASYMMETRIC_PROPERTY);
        for tp in main.triples_matching(Any, Some(&rdf_type), Some(&asymmetric)).flatten() {
            for t in delta.triples_matching(Any, Some(tp.s()), Any).flatten() {
                if main.contains(t.o(), t.p(), t.s()).unwrap_or(false) {
                    return Err(Inconsistency {
                        rule_name: "prp-asyp".to_string(),
                        conflicting_triples: vec![
                            Fact { s: extract_str(&t.s()), p: extract_str(&t.p()), o: extract_str(&t.o()) },
                            Fact { s: extract_str(&t.o()), p: extract_str(&t.p()), o: extract_str(&t.s()) },
                            Fact { s: extract_str(&t.p()), p: RDF_TYPE.to_string(), o: OWL_ASYMMETRIC_PROPERTY.to_string() },
                        ],
                    });
                }
            }
        }
        Ok(HashSet::new())
    }
}

pub struct EqDiff1;
impl InferenceRule for EqDiff1 {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let same_as = iri(OWL_SAME_AS);
        let diff_from = iri(OWL_DIFFERENT_FROM);
        for t in delta.triples_matching(Any, Some(&same_as), Any).flatten() {
            if main.contains(t.s(), &diff_from, t.o()).unwrap_or(false) {
                return Err(Inconsistency { rule_name: "eq-diff1".to_string(), conflicting_triples: vec![
                    Fact { s: extract_str(&t.s()), p: extract_str(&t.p()), o: extract_str(&t.o()) },
                    Fact { s: extract_str(&t.s()), p: OWL_DIFFERENT_FROM.to_string(), o: extract_str(&t.o()) },
                ] });
            }
        }
        Ok(HashSet::new())
    }
}

pub struct PrpSpo2;
impl InferenceRule for PrpSpo2 {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let axiom = iri(OWL_PROPERTY_CHAIN_AXIOM);
        if !has_pred(main, &axiom) { return Ok(new_facts); }
        let full_scan = std::ptr::eq(main, delta);

        for t in main.triples_matching(Any, Some(&axiom), Any).flatten() {
            let p_axiom_term = t.s();
            let p_axiom = extract_str(p_axiom_term);
            let chain = get_rdf_list(main, &t.o());
            if chain.is_empty() { continue; }
            let chain_terms: Vec<SimpleTerm<'static>> = chain.iter()
                .map(|p| InferenceEngine::make_term(p))
                .collect();

            if !full_scan {
                let any_in_delta = has_pred(delta, &axiom)
                    || chain_terms.iter().any(|pt| has_pred(delta, pt));
                if !any_in_delta { continue; }
            }

            let first_p = &chain_terms[0];
            for t1 in main.triples_matching(Any, Some(first_p), Any).flatten() {
                let mut current_nodes = vec![extract_str(&t1.o())];
                for p_term in &chain_terms[1..] {
                    let mut next_nodes = Vec::new();
                    for node in current_nodes {
                        let node_term = InferenceEngine::make_term(&node);
                        for t_next in main.triples_matching(Some(&node_term), Some(p_term), Any).flatten() {
                            next_nodes.push(extract_str(&t_next.o()));
                        }
                    }
                    current_nodes = next_nodes;
                    if current_nodes.is_empty() { break; }
                }
                let mut x1_str: Option<String> = None;
                for x_last in current_nodes {
                    let x_last_term = InferenceEngine::make_term(&x_last);
                    if !main.contains(t1.s(), p_axiom_term, &x_last_term).unwrap_or(false) {
                        let x1 = x1_str.get_or_insert_with(|| extract_str(&t1.s())).clone();
                        new_facts.insert(Fact { s: x1, p: p_axiom.clone(), o: x_last });
                    }
                }
            }
        }
        Ok(new_facts)
    }
}

pub struct ClsInt1;
impl InferenceRule for ClsInt1 {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdf_type = iri(RDF_TYPE);
        let intersection_of = iri(OWL_INTERSECTION_OF);
        if !has_pred(main, &intersection_of) { return Ok(new_facts); }
        let full_scan = std::ptr::eq(main, delta);

        for t in main.triples_matching(Any, Some(&intersection_of), Any).flatten() {
            let c_term = t.s();
            let c = extract_str(c_term);
            let list = get_rdf_list(main, &t.o());
            if list.is_empty() { continue; }
            let member_terms: Vec<SimpleTerm<'static>> = list.iter()
                .map(|m| InferenceEngine::make_term(m))
                .collect();

            if !full_scan {
                let any_in_delta = delta.triples_matching(Some(c_term), Some(&intersection_of), Any).next().is_some()
                    || member_terms.iter().any(|mt| delta.triples_matching(Any, Some(&rdf_type), Some(mt)).next().is_some());
                if !any_in_delta { continue; }
            }

            let first_class = &member_terms[0];
            for t_y in main.triples_matching(Any, Some(&rdf_type), Some(first_class)).flatten() {
                let y_term = t_y.s();
                if main.contains(y_term, &rdf_type, c_term).unwrap_or(false) { continue; }
                let mut has_all = true;
                for other_term in &member_terms[1..] {
                    if !main.contains(y_term, &rdf_type, other_term).unwrap_or(false) {
                        has_all = false;
                        break;
                    }
                }
                if has_all {
                    new_facts.insert(Fact { s: extract_str(y_term), p: I_RDF_TYPE.to_string(), o: c.clone() });
                }
            }
        }
        Ok(new_facts)
    }
}

pub struct ClsInt2;
impl InferenceRule for ClsInt2 {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdf_type = iri(RDF_TYPE);
        let intersection_of = iri(OWL_INTERSECTION_OF);
        if !has_pred(main, &intersection_of) { return Ok(new_facts); }
        let full_scan = std::ptr::eq(main, delta);

        // Schema-driven: only check delta for instances of classes that actually have an intersectionOf axiom
        for t_int in main.triples_matching(Any, Some(&intersection_of), Any).flatten() {
            let c_term = t_int.s();
            let list = get_rdf_list(main, &t_int.o());
            if list.is_empty() { continue; }
            let ci_terms: Vec<(SimpleTerm<'static>, String)> = list.into_iter()
                .map(|ci| (InferenceEngine::make_term(&ci), ci))
                .collect();

            for t in delta.triples_matching(Any, Some(&rdf_type), Some(c_term)).flatten() {
                let mut y_str: Option<String> = None;
                for (ci_term, ci) in &ci_terms {
                    if !main.contains(t.s(), &rdf_type, ci_term).unwrap_or(false) {
                        let y = y_str.get_or_insert_with(|| extract_str(&t.s())).clone();
                        new_facts.insert(Fact { s: y, p: I_RDF_TYPE.to_string(), o: ci.clone() });
                    }
                }
            }
        }

        if !full_scan {
            for t_int in delta.triples_matching(Any, Some(&intersection_of), Any).flatten() {
                let list = get_rdf_list(main, &t_int.o());
                if list.is_empty() { continue; }
                let ci_terms: Vec<(SimpleTerm<'static>, String)> = list.into_iter()
                    .map(|ci| (InferenceEngine::make_term(&ci), ci))
                    .collect();
                for t in main.triples_matching(Any, Some(&rdf_type), Some(t_int.s())).flatten() {
                    let mut y_str: Option<String> = None;
                    for (ci_term, ci) in &ci_terms {
                        if !main.contains(t.s(), &rdf_type, ci_term).unwrap_or(false) {
                            let y = y_str.get_or_insert_with(|| extract_str(&t.s())).clone();
                            new_facts.insert(Fact { s: y, p: I_RDF_TYPE.to_string(), o: ci.clone() });
                        }
                    }
                }
            }
        }
        Ok(new_facts)
    }
}

pub struct ClsUni;
impl InferenceRule for ClsUni {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdf_type = iri(RDF_TYPE);
        let union_of = iri(OWL_UNION_OF);
        if !has_pred(main, &union_of) { return Ok(new_facts); }

        // member class -> [union classes containing it] (resolved once per iteration)
        let mut member_of: HashMap<String, Vec<String>> = HashMap::new();
        for t_uni in main.triples_matching(Any, Some(&union_of), Any).flatten() {
            let union_class = extract_str(&t_uni.s());
            for member in get_rdf_list(main, &t_uni.o()) {
                member_of.entry(member).or_default().push(union_class.clone());
            }
        }

        for (member, unions) in &member_of {
            let member_term = InferenceEngine::make_term(member);
            let union_terms: Vec<(SimpleTerm<'static>, &String)> = unions.iter()
                .map(|u| (InferenceEngine::make_term(u), u))
                .collect();
            for t in delta.triples_matching(Any, Some(&rdf_type), Some(&member_term)).flatten() {
                let mut x_str: Option<String> = None;
                for (u_term, u) in &union_terms {
                    if !main.contains(t.s(), &rdf_type, u_term).unwrap_or(false) {
                        let x = x_str.get_or_insert_with(|| extract_str(&t.s())).clone();
                        new_facts.insert(Fact { s: x, p: I_RDF_TYPE.to_string(), o: (*u).clone() });
                    }
                }
            }
        }
        Ok(new_facts)
    }
}

pub struct ClsSvf1;
impl InferenceRule for ClsSvf1 {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdf_type = iri(RDF_TYPE);
        let on_property = iri(OWL_ON_PROPERTY);
        let some_values = iri(OWL_SOME_VALUES_FROM);
        if !has_pred(main, &some_values) { return Ok(new_facts); }
        let full_scan = std::ptr::eq(main, delta);

        for t_restr in main.triples_matching(Any, Some(&some_values), Any).flatten() {
            if let Some(Ok(t_prop)) = main.triples_matching(Some(t_restr.s()), Some(&on_property), Any).next() {
                let x_term = t_restr.s();
                let x = extract_str(x_term);
                let p_term = t_prop.o();
                let y_term = t_restr.o();

                if full_scan
                    || delta.triples_matching(Some(x_term), Any, Any).next().is_some()
                {
                    for t_v in main.triples_matching(Any, Some(&rdf_type), Some(y_term)).flatten() {
                        for t_u in main.triples_matching(Any, Some(p_term), Some(t_v.s())).flatten() {
                            if !main.contains(t_u.s(), &rdf_type, x_term).unwrap_or(false) {
                                new_facts.insert(Fact { s: extract_str(&t_u.s()), p: I_RDF_TYPE.to_string(), o: x.clone() });
                            }
                        }
                    }
                } else {
                    for t_v in delta.triples_matching(Any, Some(&rdf_type), Some(y_term)).flatten() {
                        for t_u in main.triples_matching(Any, Some(p_term), Some(t_v.s())).flatten() {
                            if !main.contains(t_u.s(), &rdf_type, x_term).unwrap_or(false) {
                                new_facts.insert(Fact { s: extract_str(&t_u.s()), p: I_RDF_TYPE.to_string(), o: x.clone() });
                            }
                        }
                    }
                    for t_u in delta.triples_matching(Any, Some(p_term), Any).flatten() {
                        if main.contains(t_u.o(), &rdf_type, y_term).unwrap_or(false)
                            && !main.contains(t_u.s(), &rdf_type, x_term).unwrap_or(false)
                        {
                            new_facts.insert(Fact { s: extract_str(&t_u.s()), p: I_RDF_TYPE.to_string(), o: x.clone() });
                        }
                    }
                }
            }
        }
        Ok(new_facts)
    }
}

pub struct ClsAvf;
impl InferenceRule for ClsAvf {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdf_type = iri(RDF_TYPE);
        let on_property = iri(OWL_ON_PROPERTY);
        let all_values = iri(OWL_ALL_VALUES_FROM);
        if !has_pred(main, &all_values) { return Ok(new_facts); }
        let full_scan = std::ptr::eq(main, delta);

        for t_restr in main.triples_matching(Any, Some(&all_values), Any).flatten() {
            if let Some(Ok(t_prop)) = main.triples_matching(Some(t_restr.s()), Some(&on_property), Any).next() {
                let x_term = t_restr.s();
                let y_term = t_restr.o();
                let y = extract_str(y_term);
                let p_term = t_prop.o();

                if full_scan
                    || delta.triples_matching(Some(x_term), Any, Any).next().is_some()
                {
                    for t_u in main.triples_matching(Any, Some(&rdf_type), Some(x_term)).flatten() {
                        for t_v in main.triples_matching(Some(t_u.s()), Some(p_term), Any).flatten() {
                            if !main.contains(t_v.o(), &rdf_type, y_term).unwrap_or(false) {
                                new_facts.insert(Fact { s: extract_str(&t_v.o()), p: I_RDF_TYPE.to_string(), o: y.clone() });
                            }
                        }
                    }
                } else {
                    for t_u in delta.triples_matching(Any, Some(&rdf_type), Some(x_term)).flatten() {
                        for t_v in main.triples_matching(Some(t_u.s()), Some(p_term), Any).flatten() {
                            if !main.contains(t_v.o(), &rdf_type, y_term).unwrap_or(false) {
                                new_facts.insert(Fact { s: extract_str(&t_v.o()), p: I_RDF_TYPE.to_string(), o: y.clone() });
                            }
                        }
                    }
                    for t_v in delta.triples_matching(Any, Some(p_term), Any).flatten() {
                        if main.contains(t_v.s(), &rdf_type, x_term).unwrap_or(false)
                            && !main.contains(t_v.o(), &rdf_type, y_term).unwrap_or(false)
                        {
                            new_facts.insert(Fact { s: extract_str(&t_v.o()), p: I_RDF_TYPE.to_string(), o: y.clone() });
                        }
                    }
                }
            }
        }
        Ok(new_facts)
    }
}

pub struct PrpKey;
impl InferenceRule for PrpKey {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let owl_has_key = iri(OWL_HAS_KEY);
        let rdf_type = iri(RDF_TYPE);
        let same_as = iri(OWL_SAME_AS);
        if !has_pred(main, &owl_has_key) { return Ok(new_facts); }
        let full_scan = std::ptr::eq(main, delta);

        for t_key in main.triples_matching(Any, Some(&owl_has_key), Any).flatten() {
            let class_term = t_key.s();
            let key_properties = get_rdf_list(main, &t_key.o());
            if key_properties.is_empty() { continue; }
            let prop_terms: Vec<SimpleTerm<'static>> = key_properties.iter()
                .map(|p| InferenceEngine::make_term(p))
                .collect();

            if !full_scan {
                let any_in_delta = delta.triples_matching(Some(class_term), Some(&owl_has_key), Any).next().is_some()
                    || delta.triples_matching(Any, Some(&rdf_type), Some(class_term)).next().is_some()
                    || prop_terms.iter().any(|pt| has_pred(delta, pt));
                if !any_in_delta { continue; }
            }

            let mut key_map: HashMap<Vec<String>, Vec<String>> = HashMap::new();
            for t_ind in main.triples_matching(Any, Some(&rdf_type), Some(class_term)).flatten() {
                let ind_term = t_ind.s();
                let mut values = Vec::with_capacity(prop_terms.len());
                for prop_term in &prop_terms {
                    let vals: Vec<String> = main.triples_matching(Some(ind_term), Some(prop_term), Any).flatten().map(|t| extract_str(&t.o())).collect();
                    if vals.is_empty() { break; }
                    values.push(vals.join("|"));
                }
                if values.len() == prop_terms.len() {
                    key_map.entry(values).or_default().push(extract_str(ind_term));
                }
            }
            for inds in key_map.values() {
                if inds.len() > 1 {
                    for i in 0..inds.len() {
                        let term_i = InferenceEngine::make_term(&inds[i]);
                        for j in i+1..inds.len() {
                            let term_j = InferenceEngine::make_term(&inds[j]);
                            if !main.contains(&term_i, &same_as, &term_j).unwrap_or(false) {
                                new_facts.insert(Fact { s: inds[i].clone(), p: I_OWL_SAME_AS.to_string(), o: inds[j].clone() });
                            }
                        }
                    }
                }
            }
        }
        Ok(new_facts)
    }
}

pub struct DtDiff;
impl InferenceRule for DtDiff {
    fn apply(&self, _main: &DarkstarGraph, _delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> { Ok(HashSet::new()) }
}

pub struct ScmSco;
impl InferenceRule for ScmSco {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let sub_class = iri(RDFS_SUBCLASS_OF);
        let full_scan = std::ptr::eq(main, delta);
        for t in delta.triples_matching(Any, Some(&sub_class), Any).flatten() {
            let c1 = t.s();
            let c2 = t.o();
            let mut c1_str: Option<String> = None;
            for t2 in main.triples_matching(Some(c2), Some(&sub_class), Any).flatten() {
                if !main.contains(c1, &sub_class, t2.o()).unwrap_or(false) {
                    let s = c1_str.get_or_insert_with(|| extract_str(c1)).clone();
                    new_facts.insert(Fact { s, p: I_RDFS_SUBCLASS_OF.to_string(), o: extract_str(&t2.o()) });
                }
            }
            if !full_scan {
                let mut c2_str: Option<String> = None;
                for t2 in main.triples_matching(Any, Some(&sub_class), Some(c1)).flatten() {
                    if !main.contains(t2.s(), &sub_class, c2).unwrap_or(false) {
                        let o = c2_str.get_or_insert_with(|| extract_str(c2)).clone();
                        new_facts.insert(Fact { s: extract_str(&t2.s()), p: I_RDFS_SUBCLASS_OF.to_string(), o });
                    }
                }
            }
        }
        Ok(new_facts)
    }
}

pub struct ScmSpo;
impl InferenceRule for ScmSpo {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let sub_prop = iri(RDFS_SUBPROPERTY_OF);
        let full_scan = std::ptr::eq(main, delta);
        for t in delta.triples_matching(Any, Some(&sub_prop), Any).flatten() {
            let p1 = t.s();
            let p2 = t.o();
            let mut p1_str: Option<String> = None;
            for t2 in main.triples_matching(Some(p2), Some(&sub_prop), Any).flatten() {
                if !main.contains(p1, &sub_prop, t2.o()).unwrap_or(false) {
                    let s = p1_str.get_or_insert_with(|| extract_str(p1)).clone();
                    new_facts.insert(Fact { s, p: I_RDFS_SUBPROPERTY_OF.to_string(), o: extract_str(&t2.o()) });
                }
            }
            if !full_scan {
                let mut p2_str: Option<String> = None;
                for t2 in main.triples_matching(Any, Some(&sub_prop), Some(p1)).flatten() {
                    if !main.contains(t2.s(), &sub_prop, p2).unwrap_or(false) {
                        let o = p2_str.get_or_insert_with(|| extract_str(p2)).clone();
                        new_facts.insert(Fact { s: extract_str(&t2.s()), p: I_RDFS_SUBPROPERTY_OF.to_string(), o });
                    }
                }
            }
        }
        Ok(new_facts)
    }
}

pub struct ClsDj;
impl InferenceRule for ClsDj {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let disjoint = iri(OWL_DISJOINT_WITH);
        let rdf_type = iri(RDF_TYPE);
        if !has_pred(main, &disjoint) { return Ok(HashSet::new()); }
        let full_scan = std::ptr::eq(main, delta);

        // Schema-driven: only inspect instances of classes participating in owl:disjointWith
        for td in main.triples_matching(Any, Some(&disjoint), Any).flatten() {
            let c1 = td.s();
            let c2 = td.o();
            for t in delta.triples_matching(Any, Some(&rdf_type), Some(c1)).flatten() {
                let x = t.s();
                if main.contains(x, &rdf_type, c2).unwrap_or(false) {
                    return Err(Inconsistency { rule_name: "cls-dj".to_string(), conflicting_triples: vec![
                        Fact { s: extract_str(&x), p: RDF_TYPE.to_string(), o: extract_str(&c1) },
                        Fact { s: extract_str(&x), p: RDF_TYPE.to_string(), o: extract_str(&c2) },
                        Fact { s: extract_str(&c1), p: OWL_DISJOINT_WITH.to_string(), o: extract_str(&c2) },
                    ] });
                }
            }
            if !full_scan {
                for t in delta.triples_matching(Any, Some(&rdf_type), Some(c2)).flatten() {
                    let x = t.s();
                    if main.contains(x, &rdf_type, c1).unwrap_or(false) {
                        return Err(Inconsistency { rule_name: "cls-dj".to_string(), conflicting_triples: vec![
                            Fact { s: extract_str(&x), p: RDF_TYPE.to_string(), o: extract_str(&c1) },
                            Fact { s: extract_str(&x), p: RDF_TYPE.to_string(), o: extract_str(&c2) },
                            Fact { s: extract_str(&c1), p: OWL_DISJOINT_WITH.to_string(), o: extract_str(&c2) },
                        ] });
                    }
                }
            }
        }
        Ok(HashSet::new())
    }
}

pub struct PrpNpa;
impl InferenceRule for PrpNpa {
    fn apply(&self, _main: &DarkstarGraph, _delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> { Ok(HashSet::new()) }
}

pub struct ClsNothing;
impl InferenceRule for ClsNothing {
    fn apply(&self, _main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let rdf_type = iri(RDF_TYPE);
        let nothing = iri(OWL_NOTHING);
        if let Some(t) = delta.triples_matching(Any, Some(&rdf_type), Some(&nothing)).flatten().next() {
            return Err(Inconsistency { rule_name: "cls-nothing".to_string(), conflicting_triples: vec![Fact { s: extract_str(&t.s()), p: RDF_TYPE.to_string(), o: OWL_NOTHING.to_string() }] });
        }
        Ok(HashSet::new())
    }
}
