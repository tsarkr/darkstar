pub mod engine;

use crate::memory::{DarkstarGraph, Fact, Inconsistency};
use sophia::api::prelude::*;
use sophia::api::term::matcher::Any;
use std::collections::HashSet;
use std::collections::HashMap;
use sophia::api::term::{SimpleTerm, IriRef};
use crate::rules::engine::InferenceEngine;

// Standard ontology URIs
pub const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
pub const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
pub const RDFS_SUBPROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
pub const OWL_TRANSITIVE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";
pub const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
pub const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
pub const OWL_INVERSE_OF: &str = "http://www.w3.org/2002/07/owl#inverseOf";
pub const OWL_SAME_AS: &str = "http://www.w3.org/2002/07/owl#sameAs";
pub const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
pub const OWL_IRREFLEXIVE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#IrreflexiveProperty";
pub const OWL_ASYMMETRIC_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AsymmetricProperty";
pub const OWL_DIFFERENT_FROM: &str = "http://www.w3.org/2002/07/owl#differentFrom";
pub const OWL_PROPERTY_CHAIN_AXIOM: &str = "http://www.w3.org/2002/07/owl#propertyChainAxiom";
pub const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
pub const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
pub const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
pub const OWL_INTERSECTION_OF: &str = "http://www.w3.org/2002/07/owl#intersectionOf";
pub const OWL_UNION_OF: &str = "http://www.w3.org/2002/07/owl#unionOf";
pub const OWL_ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
pub const OWL_SOME_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
pub const OWL_ALL_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#allValuesFrom";
pub const OWL_HAS_KEY: &str = "http://www.w3.org/2002/07/owl#hasKey";
pub const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";
pub const OWL_NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";

/// Helper to safely extract an interned string from a `sophia` Term.
/// Blank nodes now preserve their unique ID to prevent massive collisions.
pub fn extract_str<T: Term>(term: &T) -> String {
    if let Some(iri) = term.iri() {
        format!("i:{}", iri)
    } else if let Some(lex) = term.lexical_form() {
        format!("l:{}", lex)
    } else if let Some(bnode) = term.bnode_id() {
        format!("b:{}", bnode.as_str())
    } else {
        format!("b:anon")
    }
}

/// Recursively extracts all elements from an RDF list starting at `head`.
fn get_rdf_list<T: Term>(main: &DarkstarGraph, head: &T) -> Vec<String> {
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

pub trait InferenceRule: Sync + Send {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency>;
}

/// cax-sco: Class Subsumption
pub struct CaxSco;
impl InferenceRule for CaxSco {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        let rdfs_subclass_of = SimpleTerm::Iri(IriRef::new_unchecked(RDFS_SUBCLASS_OF.into()));

        for t in delta.triples().flatten() {
            let p = t.p();
            if *p == rdf_type {
                let x = extract_str(&t.s());
                for t2 in main.triples_matching(Some(t.o()), Some(&rdfs_subclass_of), Any).flatten() {
                    new_facts.insert(Fact { s: x.clone(), p: RDF_TYPE.to_string(), o: extract_str(&t2.o()) });
                }
            } else if *p == rdfs_subclass_of {
                let c2 = extract_str(&t.o());
                for t2 in main.triples_matching(Any, Some(&rdf_type), Some(t.s())).flatten() {
                    new_facts.insert(Fact { s: extract_str(&t2.s()), p: RDF_TYPE.to_string(), o: c2.clone() });
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
        let rdfs_subproperty_of = SimpleTerm::Iri(IriRef::new_unchecked(RDFS_SUBPROPERTY_OF.into()));

        for t in delta.triples().flatten() {
            let p = t.p();
            if *p == rdfs_subproperty_of {
                let p2 = extract_str(&t.o());
                for t2 in main.triples_matching(Any, Some(t.s()), Any).flatten() {
                    new_facts.insert(Fact { s: extract_str(&t2.s()), p: p2.clone(), o: extract_str(&t2.o()) });
                }
            } else {
                for t2 in main.triples_matching(Some(p), Some(&rdfs_subproperty_of), Any).flatten() {
                    new_facts.insert(Fact { s: extract_str(&t.s()), p: extract_str(&t2.o()), o: extract_str(&t.o()) });
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
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        let owl_transitive_property = SimpleTerm::Iri(IriRef::new_unchecked(OWL_TRANSITIVE_PROPERTY.into()));

        let transitive_props: Vec<_> = main.triples_matching(Any, Some(&rdf_type), Some(&owl_transitive_property))
            .flatten()
            .map(|t| (t.s().to_owned(), extract_str(&t.s())))
            .collect();
        
        for (p_term, p_str) in transitive_props {
            for d in delta.triples_matching(Any, Some(&p_term), Any).flatten() {
                for m in main.triples_matching(Some(d.o()), Some(&p_term), Any).flatten() {
                    new_facts.insert(Fact { s: extract_str(&d.s()), p: p_str.clone(), o: extract_str(&m.o()) });
                }
                for m in main.triples_matching(Any, Some(&p_term), Some(d.s())).flatten() {
                    new_facts.insert(Fact { s: extract_str(&m.s()), p: p_str.clone(), o: extract_str(&d.o()) });
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
        let rdfs_domain = SimpleTerm::Iri(IriRef::new_unchecked(RDFS_DOMAIN.into()));
        for t in delta.triples().flatten() {
            if *t.p() == rdfs_domain {
                let c = extract_str(&t.o());
                for t2 in main.triples_matching(Any, Some(t.s()), Any).flatten() {
                    new_facts.insert(Fact { s: extract_str(&t2.s()), p: RDF_TYPE.to_string(), o: c.clone() });
                }
            } else {
                for t2 in main.triples_matching(Some(t.p()), Some(&rdfs_domain), Any).flatten() {
                    new_facts.insert(Fact { s: extract_str(&t.s()), p: RDF_TYPE.to_string(), o: extract_str(&t2.o()) });
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
        let rdfs_range = SimpleTerm::Iri(IriRef::new_unchecked(RDFS_RANGE.into()));
        for t in delta.triples().flatten() {
            if *t.p() == rdfs_range {
                let c = extract_str(&t.o());
                for t2 in main.triples_matching(Any, Some(t.s()), Any).flatten() {
                    new_facts.insert(Fact { s: extract_str(&t2.o()), p: RDF_TYPE.to_string(), o: c.clone() });
                }
            } else {
                for t2 in main.triples_matching(Some(t.p()), Some(&rdfs_range), Any).flatten() {
                    new_facts.insert(Fact { s: extract_str(&t.o()), p: RDF_TYPE.to_string(), o: extract_str(&t2.o()) });
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
        let inverse_of = SimpleTerm::Iri(IriRef::new_unchecked(OWL_INVERSE_OF.into()));
        for t in delta.triples().flatten() {
            if *t.p() == inverse_of {
                let p1 = extract_str(&t.s());
                let p2 = extract_str(&t.o());
                for t2 in main.triples_matching(Any, Some(t.s()), Any).flatten() {
                    new_facts.insert(Fact { s: extract_str(&t2.o()), p: p2.clone(), o: extract_str(&t2.s()) });
                }
                for t2 in main.triples_matching(Any, Some(t.o()), Any).flatten() {
                    new_facts.insert(Fact { s: extract_str(&t2.o()), p: p1.clone(), o: extract_str(&t2.s()) });
                }
            } else {
                for t2 in main.triples_matching(Some(t.p()), Some(&inverse_of), Any).flatten() {
                    new_facts.insert(Fact { s: extract_str(&t.o()), p: extract_str(&t2.o()), o: extract_str(&t.s()) });
                }
                for t2 in main.triples_matching(Any, Some(&inverse_of), Some(t.p())).flatten() {
                    new_facts.insert(Fact { s: extract_str(&t.o()), p: extract_str(&t2.s()), o: extract_str(&t.s()) });
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
        let same_as = SimpleTerm::Iri(IriRef::new_unchecked(OWL_SAME_AS.into()));
        for t in delta.triples().flatten() {
            if *t.p() == same_as {
                let s2 = extract_str(&t.o());
                for t2 in main.triples_matching(Some(t.s()), Any, Any).flatten() {
                    new_facts.insert(Fact { s: s2.clone(), p: extract_str(&t2.p()), o: extract_str(&t2.o()) });
                }
            } else {
                for t2 in main.triples_matching(Some(t.s()), Some(&same_as), Any).flatten() {
                    new_facts.insert(Fact { s: extract_str(&t2.o()), p: extract_str(&t.p()), o: extract_str(&t.o()) });
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
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        let owl_equiv = SimpleTerm::Iri(IriRef::new_unchecked(OWL_EQUIVALENT_CLASS.into()));
        for t in delta.triples().flatten() {
            if *t.p() == owl_equiv {
                let c1 = extract_str(&t.s());
                let c2 = extract_str(&t.o());
                for t2 in main.triples_matching(Any, Some(&rdf_type), Some(t.s())).flatten() {
                    new_facts.insert(Fact { s: extract_str(&t2.s()), p: RDF_TYPE.to_string(), o: c2.clone() });
                }
                for t2 in main.triples_matching(Any, Some(&rdf_type), Some(t.o())).flatten() {
                    new_facts.insert(Fact { s: extract_str(&t2.s()), p: RDF_TYPE.to_string(), o: c1.clone() });
                }
            } else if *t.p() == rdf_type {
                let x = extract_str(&t.s());
                for t2 in main.triples_matching(Some(t.o()), Some(&owl_equiv), Any).flatten() {
                    new_facts.insert(Fact { s: x.clone(), p: RDF_TYPE.to_string(), o: extract_str(&t2.o()) });
                }
                for t2 in main.triples_matching(Any, Some(&owl_equiv), Some(t.o())).flatten() {
                    new_facts.insert(Fact { s: x.clone(), p: RDF_TYPE.to_string(), o: extract_str(&t2.s()) });
                }
            }
        }
        Ok(new_facts)
    }
}

pub struct EqRef;
impl InferenceRule for EqRef {
    fn apply(&self, _main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        for t in delta.triples().flatten() {
            let s = extract_str(&t.s());
            let p = extract_str(&t.p());
            let o = extract_str(&t.o());
            new_facts.insert(Fact { s: s.clone(), p: OWL_SAME_AS.to_string(), o: s });
            new_facts.insert(Fact { s: p.clone(), p: OWL_SAME_AS.to_string(), o: p });
            new_facts.insert(Fact { s: o.clone(), p: OWL_SAME_AS.to_string(), o: o });
        }
        Ok(new_facts)
    }
}

pub struct EqSym;
impl InferenceRule for EqSym {
    fn apply(&self, _main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let same_as = SimpleTerm::Iri(IriRef::new_unchecked(OWL_SAME_AS.into()));
        for t in delta.triples_matching(Any, Some(&same_as), Any).flatten() {
            new_facts.insert(Fact { s: extract_str(&t.o()), p: OWL_SAME_AS.to_string(), o: extract_str(&t.s()) });
        }
        Ok(new_facts)
    }
}

pub struct EqTrans;
impl InferenceRule for EqTrans {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let same_as = SimpleTerm::Iri(IriRef::new_unchecked(OWL_SAME_AS.into()));
        for t in delta.triples_matching(Any, Some(&same_as), Any).flatten() {
            let x = extract_str(&t.s());
            for t2 in main.triples_matching(Some(t.o()), Some(&same_as), Any).flatten() {
                new_facts.insert(Fact { s: x.clone(), p: OWL_SAME_AS.to_string(), o: extract_str(&t2.o()) });
            }
            let z = extract_str(&t.o());
            for t2 in main.triples_matching(Any, Some(&same_as), Some(t.s())).flatten() {
                new_facts.insert(Fact { s: extract_str(&t2.s()), p: OWL_SAME_AS.to_string(), o: z.clone() });
            }
        }
        Ok(new_facts)
    }
}

pub struct EqRepP;
impl InferenceRule for EqRepP {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let same_as = SimpleTerm::Iri(IriRef::new_unchecked(OWL_SAME_AS.into()));
        for t in delta.triples().flatten() {
            if *t.p() == same_as {
                let q = extract_str(&t.o());
                for t2 in main.triples_matching(Any, Some(t.s()), Any).flatten() {
                    new_facts.insert(Fact { s: extract_str(&t2.s()), p: q.clone(), o: extract_str(&t2.o()) });
                }
            } else {
                for t2 in main.triples_matching(Some(t.p()), Some(&same_as), Any).flatten() {
                    new_facts.insert(Fact { s: extract_str(&t.s()), p: extract_str(&t2.o()), o: extract_str(&t.o()) });
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
        let same_as = SimpleTerm::Iri(IriRef::new_unchecked(OWL_SAME_AS.into()));
        for t in delta.triples().flatten() {
            if *t.p() == same_as {
                let o_prime = extract_str(&t.o());
                for t2 in main.triples_matching(Any, Any, Some(t.s())).flatten() {
                    new_facts.insert(Fact { s: extract_str(&t2.s()), p: extract_str(&t2.p()), o: o_prime.clone() });
                }
            } else {
                for t2 in main.triples_matching(Some(t.o()), Some(&same_as), Any).flatten() {
                    new_facts.insert(Fact { s: extract_str(&t.s()), p: extract_str(&t.p()), o: extract_str(&t2.o()) });
                }
            }
        }
        Ok(new_facts)
    }
}

pub struct PrpIrp;
impl InferenceRule for PrpIrp {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        let irreflexive = SimpleTerm::Iri(IriRef::new_unchecked(OWL_IRREFLEXIVE_PROPERTY.into()));
        for t in delta.triples().flatten() {
            let p = t.p();
            if main.contains(p, &rdf_type, &irreflexive).unwrap_or(false) && t.s() == t.o() {
                return Err(Inconsistency {
                    rule_name: "prp-irp".to_string(),
                    conflicting_triples: vec![Fact { s: extract_str(&t.s()), p: extract_str(&t.p()), o: extract_str(&t.o()) }],
                });
            }
        }
        Ok(HashSet::new())
    }
}

pub struct PrpAsyp;
impl InferenceRule for PrpAsyp {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        let asymmetric = SimpleTerm::Iri(IriRef::new_unchecked(OWL_ASYMMETRIC_PROPERTY.into()));
        for t in delta.triples().flatten() {
            if main.contains(t.p(), &rdf_type, &asymmetric).unwrap_or(false) && main.contains(t.o(), t.p(), t.s()).unwrap_or(false) {
                return Err(Inconsistency {
                    rule_name: "prp-asyp".to_string(),
                    conflicting_triples: vec![Fact { s: extract_str(&t.s()), p: extract_str(&t.p()), o: extract_str(&t.o()) }],
                });
            }
        }
        Ok(HashSet::new())
    }
}

pub struct EqDiff1;
impl InferenceRule for EqDiff1 {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let same_as = SimpleTerm::Iri(IriRef::new_unchecked(OWL_SAME_AS.into()));
        let diff_from = SimpleTerm::Iri(IriRef::new_unchecked(OWL_DIFFERENT_FROM.into()));
        for t in delta.triples_matching(Any, Some(&same_as), Any).flatten() {
            if main.contains(t.s(), &diff_from, t.o()).unwrap_or(false) {
                return Err(Inconsistency { rule_name: "eq-diff1".to_string(), conflicting_triples: vec![Fact { s: extract_str(&t.s()), p: extract_str(&t.p()), o: extract_str(&t.o()) }] });
            }
        }
        Ok(HashSet::new())
    }
}

pub struct PrpSpo2;
impl InferenceRule for PrpSpo2 {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let axiom = SimpleTerm::Iri(IriRef::new_unchecked(OWL_PROPERTY_CHAIN_AXIOM.into()));
        if delta.triples().next().is_none() { return Ok(new_facts); }

        for t in main.triples_matching(Any, Some(&axiom), Any).flatten() {
            let p_axiom = extract_str(&t.s());
            let chain = get_rdf_list(main, &t.o());
            if chain.is_empty() { continue; }
            let first_p = InferenceEngine::make_term(&chain[0]);
            for t1 in main.triples_matching(Any, Some(&first_p), Any).flatten() {
                let mut current_nodes = vec![extract_str(&t1.o())];
                for p_str in &chain[1..] {
                    let mut next_nodes = Vec::new();
                    let p_term = InferenceEngine::make_term(p_str);
                    for node in current_nodes {
                        let node_term = InferenceEngine::make_term(&node);
                        for t_next in main.triples_matching(Some(&node_term), Some(&p_term), Any).flatten() {
                            next_nodes.push(extract_str(&t_next.o()));
                        }
                    }
                    current_nodes = next_nodes;
                    if current_nodes.is_empty() { break; }
                }
                let x1 = extract_str(&t1.s());
                for x_last in current_nodes {
                    new_facts.insert(Fact { s: x1.clone(), p: p_axiom.clone(), o: x_last });
                }
            }
        }
        Ok(new_facts)
    }
}

pub struct ClsInt1;
impl InferenceRule for ClsInt1 {
    fn apply(&self, main: &DarkstarGraph, _delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        let intersection_of = SimpleTerm::Iri(IriRef::new_unchecked(OWL_INTERSECTION_OF.into()));
        for t in main.triples_matching(Any, Some(&intersection_of), Any).flatten() {
            let c = extract_str(&t.s());
            let list = get_rdf_list(main, &t.o());
            if list.is_empty() { continue; }
            let first_class = InferenceEngine::make_term(&list[0]);
            for t_y in main.triples_matching(Any, Some(&rdf_type), Some(&first_class)).flatten() {
                let y = extract_str(&t_y.s());
                let y_term = InferenceEngine::make_term(&y);
                let mut has_all = true;
                for other in &list[1..] {
                    let other_term = InferenceEngine::make_term(other);
                    if main.triples_matching(Some(&y_term), Some(&rdf_type), Some(&other_term)).next().is_none() { has_all = false; break; }
                }
                if has_all { new_facts.insert(Fact { s: y, p: RDF_TYPE.to_string(), o: c.clone() }); }
            }
        }
        Ok(new_facts)
    }
}

pub struct ClsInt2;
impl InferenceRule for ClsInt2 {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        let intersection_of = SimpleTerm::Iri(IriRef::new_unchecked(OWL_INTERSECTION_OF.into()));
        for t in delta.triples_matching(Any, Some(&rdf_type), Any).flatten() {
            for t_int in main.triples_matching(Some(t.o()), Some(&intersection_of), Any).flatten() {
                let list = get_rdf_list(main, &t_int.o());
                let y = extract_str(&t.s());
                for ci in list { new_facts.insert(Fact { s: y.clone(), p: RDF_TYPE.to_string(), o: ci }); }
            }
        }
        Ok(new_facts)
    }
}

pub struct ClsUni;
impl InferenceRule for ClsUni {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        let union_of = SimpleTerm::Iri(IriRef::new_unchecked(OWL_UNION_OF.into()));
        for t in delta.triples_matching(Any, Some(&rdf_type), Any).flatten() {
            let ci_str = extract_str(&t.o());
            for t_uni in main.triples_matching(Any, Some(&union_of), Any).flatten() {
                let list = get_rdf_list(main, &t_uni.o());
                if list.contains(&ci_str) {
                    new_facts.insert(Fact { s: extract_str(&t.s()), p: RDF_TYPE.to_string(), o: extract_str(&t_uni.s()) });
                }
            }
        }
        Ok(new_facts)
    }
}

pub struct ClsSvf1;
impl InferenceRule for ClsSvf1 {
    fn apply(&self, main: &DarkstarGraph, _delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        let on_property = SimpleTerm::Iri(IriRef::new_unchecked(OWL_ON_PROPERTY.into()));
        let some_values = SimpleTerm::Iri(IriRef::new_unchecked(OWL_SOME_VALUES_FROM.into()));
        for t_restr in main.triples_matching(Any, Some(&some_values), Any).flatten() {
            if let Some(Ok(t_prop)) = main.triples_matching(Some(t_restr.s()), Some(&on_property), Any).next() {
                let x = extract_str(&t_restr.s());
                let p_term = t_prop.o();
                let y_term = t_restr.o();
                for t_v in main.triples_matching(Any, Some(&rdf_type), Some(y_term)).flatten() {
                    for t_u in main.triples_matching(Any, Some(p_term), Some(t_v.s())).flatten() {
                        new_facts.insert(Fact { s: extract_str(&t_u.s()), p: RDF_TYPE.to_string(), o: x.clone() });
                    }
                }
            }
        }
        Ok(new_facts)
    }
}

pub struct ClsAvf;
impl InferenceRule for ClsAvf {
    fn apply(&self, main: &DarkstarGraph, _delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        let on_property = SimpleTerm::Iri(IriRef::new_unchecked(OWL_ON_PROPERTY.into()));
        let all_values = SimpleTerm::Iri(IriRef::new_unchecked(OWL_ALL_VALUES_FROM.into()));
        for t_restr in main.triples_matching(Any, Some(&all_values), Any).flatten() {
            if let Some(Ok(t_prop)) = main.triples_matching(Some(t_restr.s()), Some(&on_property), Any).next() {
                let y = extract_str(&t_restr.o());
                let p_term = t_prop.o();
                for t_u in main.triples_matching(Any, Some(&rdf_type), Some(t_restr.s())).flatten() {
                    for t_v in main.triples_matching(Some(t_u.s()), Some(p_term), Any).flatten() {
                        new_facts.insert(Fact { s: extract_str(&t_v.o()), p: RDF_TYPE.to_string(), o: y.clone() });
                    }
                }
            }
        }
        Ok(new_facts)
    }
}

pub struct PrpKey;
impl InferenceRule for PrpKey {
    fn apply(&self, main: &DarkstarGraph, _delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let owl_has_key = SimpleTerm::Iri(IriRef::new_unchecked(OWL_HAS_KEY.into()));
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        for t_key in main.triples_matching(Any, Some(&owl_has_key), Any).flatten() {
            let class_uri = extract_str(&t_key.s());
            let key_properties = get_rdf_list(main, &t_key.o());
            if key_properties.is_empty() { continue; }
            let mut key_map: HashMap<Vec<String>, Vec<String>> = HashMap::new();
            let class_term = InferenceEngine::make_term(&class_uri);
            for t_ind in main.triples_matching(Any, Some(&rdf_type), Some(&class_term)).flatten() {
                let ind = extract_str(&t_ind.s());
                let ind_term = t_ind.s();
                let mut values = Vec::new();
                for prop in &key_properties {
                    let prop_term = InferenceEngine::make_term(prop);
                    let vals: Vec<String> = main.triples_matching(Some(ind_term), Some(&prop_term), Any).flatten().map(|t| extract_str(&t.o())).collect();
                    if vals.is_empty() { break; }
                    values.push(vals.join("|"));
                }
                if values.len() == key_properties.len() { key_map.entry(values).or_default().push(ind); }
            }
            for inds in key_map.values() {
                if inds.len() > 1 {
                    for i in 0..inds.len() {
                        for j in i+1..inds.len() {
                            new_facts.insert(Fact { s: inds[i].clone(), p: OWL_SAME_AS.to_string(), o: inds[j].clone() });
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
        let sub_class = SimpleTerm::Iri(IriRef::new_unchecked(RDFS_SUBCLASS_OF.into()));
        for t in delta.triples_matching(Any, Some(&sub_class), Any).flatten() {
            let c1 = t.s();
            let c2 = t.o();
            for t2 in main.triples_matching(Some(c2), Some(&sub_class), Any).flatten() {
                new_facts.insert(Fact { s: extract_str(&c1), p: RDFS_SUBCLASS_OF.to_string(), o: extract_str(&t2.o()) });
            }
            for t2 in main.triples_matching(Any, Some(&sub_class), Some(c1)).flatten() {
                new_facts.insert(Fact { s: extract_str(&t2.s()), p: RDFS_SUBCLASS_OF.to_string(), o: extract_str(&c2) });
            }
        }
        Ok(new_facts)
    }
}

pub struct ScmSpo;
impl InferenceRule for ScmSpo {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let sub_prop = SimpleTerm::Iri(IriRef::new_unchecked(RDFS_SUBPROPERTY_OF.into()));
        for t in delta.triples_matching(Any, Some(&sub_prop), Any).flatten() {
            let p1 = t.s();
            let p2 = t.o();
            for t2 in main.triples_matching(Some(p2), Some(&sub_prop), Any).flatten() {
                new_facts.insert(Fact { s: extract_str(&p1), p: RDFS_SUBPROPERTY_OF.to_string(), o: extract_str(&t2.o()) });
            }
            for t2 in main.triples_matching(Any, Some(&sub_prop), Some(p1)).flatten() {
                new_facts.insert(Fact { s: extract_str(&t2.s()), p: RDFS_SUBPROPERTY_OF.to_string(), o: extract_str(&p2) });
            }
        }
        Ok(new_facts)
    }
}

pub struct ClsDj;
impl InferenceRule for ClsDj {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let disjoint = SimpleTerm::Iri(IriRef::new_unchecked(OWL_DISJOINT_WITH.into()));
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        for t in delta.triples_matching(Any, Some(&rdf_type), Any).flatten() {
            let x = t.s();
            let c = t.o();
            for t2 in main.triples_matching(Some(c), Some(&disjoint), Any).flatten() {
                if main.contains(x, &rdf_type, t2.o()).unwrap_or(false) {
                    return Err(Inconsistency { rule_name: "cls-dj".to_string(), conflicting_triples: vec![Fact { s: extract_str(&x), p: RDF_TYPE.to_string(), o: extract_str(&c) }] });
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
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        let nothing = SimpleTerm::Iri(IriRef::new_unchecked(OWL_NOTHING.into()));
        for t in delta.triples_matching(Any, Some(&rdf_type), Some(&nothing)).flatten() {
            return Err(Inconsistency { rule_name: "cls-nothing".to_string(), conflicting_triples: vec![Fact { s: extract_str(&t.s()), p: RDF_TYPE.to_string(), o: OWL_NOTHING.to_string() }] });
        }
        Ok(HashSet::new())
    }
}
