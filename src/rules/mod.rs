pub mod engine;

use crate::memory::{DarkstarGraph, Fact, Inconsistency};
use sophia::api::prelude::*;
use sophia::api::term::matcher::Any;
use std::collections::HashSet;

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
pub const OWL_NEG_OBJ_PROP_ASSERT: &str = "http://www.w3.org/2002/07/owl#NegativeObjectPropertyAssertion";
pub const OWL_SOURCE_INDIVIDUAL: &str = "http://www.w3.org/2002/07/owl#sourceIndividual";
pub const OWL_ASSERTED_PROPERTY: &str = "http://www.w3.org/2002/07/owl#assertionProperty";
pub const OWL_TARGET_INDIVIDUAL: &str = "http://www.w3.org/2002/07/owl#targetIndividual";

/// Helper to safely extract a string from a `sophia` Term.
pub fn extract_str<T: Term>(term: &T) -> String {
    if let Some(iri) = term.iri() {
        format!("i:{}", iri)
    } else if let Some(lex) = term.lexical_form() {
        format!("l:{}", lex)
    } else {
        format!("b:bnode")
    }
}

/// Recursively extracts all elements from an RDF list starting at `head`.
fn get_rdf_list<T: Term>(main: &DarkstarGraph, head: &T) -> Vec<String> {
    let mut elements = Vec::new();
    let mut current = extract_str(head);
    let rdf_first = SimpleTerm::Iri(IriRef::new_unchecked(RDF_FIRST.into()));
    let rdf_rest = SimpleTerm::Iri(IriRef::new_unchecked(RDF_REST.into()));
    let rdf_nil = RDF_NIL.to_string();

    while current != rdf_nil {
        let head_term = engine::InferenceEngine::make_term(&current);
        
        // Find rdf:first
        let t_first = main.triples_matching(Some(&head_term), Some(&rdf_first), Any).next();
        if let Some(Ok(t)) = t_first {
            elements.push(extract_str(&t.o()));
        } else {
            break; // Malformed list
        }

        // Find rdf:rest
        let t_rest = main.triples_matching(Some(&head_term), Some(&rdf_rest), Any).next();
        if let Some(Ok(t)) = t_rest {
            current = extract_str(&t.o());
        } else {
            break; // Malformed or end of list without nil
        }
    }
    elements
}

/// Represents an OWL 2 RL inference rule.
pub trait InferenceRule: Sync + Send {
    /// Executes the rule logic. Takes the main graph and the delta graph from the last iteration.
    /// Returns a set of newly inferred Facts, or an Inconsistency if a contradiction is detected.
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency>;
}

use sophia::api::term::{SimpleTerm, IriRef};

/// cax-sco: Class Subsumption
/// If `?x rdf:type ?c1` and `?c1 rdfs:subClassOf ?c2` -> `?x rdf:type ?c2`
pub struct CaxSco;
impl InferenceRule for CaxSco {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        let rdfs_subclass_of = SimpleTerm::Iri(IriRef::new_unchecked(RDFS_SUBCLASS_OF.into()));

        // Match 1: Delta(?x, type, ?c1) JOIN Main(?c1, subClassOf, ?c2)
        let delta_types = delta.triples_matching(Any, Some(&rdf_type), Any);
        for t1 in delta_types.flatten() {
            let x = extract_str(&t1.s());
            let c1_term = t1.o();

            let sub_classes = main.triples_matching(Some(c1_term), Some(&rdfs_subclass_of), Any);
            for t2 in sub_classes.flatten() {
                let c2 = extract_str(&t2.o());
                new_facts.insert(Fact { s: x.clone(), p: RDF_TYPE.to_string(), o: c2 });
            }
        }

        // Match 2: Delta(?c1, subClassOf, ?c2) JOIN Main(?x, type, ?c1)
        let delta_subclasses = delta.triples_matching(Any, Some(&rdfs_subclass_of), Any);
        for t1 in delta_subclasses.flatten() {
            let c1_term = t1.s();
            let c2 = extract_str(&t1.o());

            let types = main.triples_matching(Any, Some(&rdf_type), Some(c1_term));
            for t2 in types.flatten() {
                let x = extract_str(&t2.s());
                new_facts.insert(Fact { s: x, p: RDF_TYPE.to_string(), o: c2.clone() });
            }
        }

        Ok(new_facts)
    }
}

/// prp-spo1: Property Subsumption
/// If `?x ?p1 ?y` and `?p1 rdfs:subPropertyOf ?p2` -> `?x ?p2 ?y`
pub struct PrpSpo1;
impl InferenceRule for PrpSpo1 {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdfs_subproperty_of = SimpleTerm::Iri(IriRef::new_unchecked(RDFS_SUBPROPERTY_OF.into()));

        // Match 1: Delta(?p1, subPropertyOf, ?p2) JOIN Main(?x, ?p1, ?y)
        let delta_subprops = delta.triples_matching(Any, Some(&rdfs_subproperty_of), Any);
        for t1 in delta_subprops.flatten() {
            let p1_term = t1.s();
            let p2 = extract_str(&t1.o());

            let triples = main.triples_matching(Any, Some(p1_term), Any);
            for t2 in triples.flatten() {
                let x = extract_str(&t2.s());
                let y = extract_str(&t2.o());
                new_facts.insert(Fact { s: x, p: p2.clone(), o: y });
            }
        }

        // Match 2: Delta(?x, ?p1, ?y) JOIN Main(?p1, subPropertyOf, ?p2)
        // Note: For delta triples, we iterate all and check if the predicate is a subproperty.
        for t1 in delta.triples().flatten() {
            let p1_term = t1.p();
            let x = extract_str(&t1.s());
            let y = extract_str(&t1.o());

            let subprops = main.triples_matching(Some(p1_term), Some(&rdfs_subproperty_of), Any);
            for t2 in subprops.flatten() {
                let p2 = extract_str(&t2.o());
                new_facts.insert(Fact { s: x.clone(), p: p2, o: y.clone() });
            }
        }

        Ok(new_facts)
    }
}

/// prp-trp: Transitive Property
/// If `?p rdf:type owl:TransitiveProperty` and `?x ?p ?y` and `?y ?p ?z` -> `?x ?p ?z`
pub struct PrpTrp;
impl InferenceRule for PrpTrp {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        let owl_transitive_property = SimpleTerm::Iri(IriRef::new_unchecked(OWL_TRANSITIVE_PROPERTY.into()));

        // We assume owl:TransitiveProperty declarations rarely change, but we query them dynamically.
        let transitive_props = main.triples_matching(Any, Some(&rdf_type), Some(&owl_transitive_property));
        
        for t in transitive_props.flatten() {
            let p_term = t.s();
            let p_str = extract_str(&p_term);

            // Delta(?x, ?p, ?y) JOIN Main(?y, ?p, ?z)
            let delta_left = delta.triples_matching(Any, Some(p_term), Any);
            for d in delta_left.flatten() {
                let x = extract_str(&d.s());
                let y_term = d.o();
                
                let main_right = main.triples_matching(Some(y_term), Some(p_term), Any);
                for m in main_right.flatten() {
                    let z = extract_str(&m.o());
                    new_facts.insert(Fact { s: x.clone(), p: p_str.clone(), o: z });
                }
            }

            // Main(?x, ?p, ?y) JOIN Delta(?y, ?p, ?z)
            let delta_right = delta.triples_matching(Any, Some(p_term), Any);
            for d in delta_right.flatten() {
                let y_term = d.s();
                let z = extract_str(&d.o());

                let main_left = main.triples_matching(Any, Some(p_term), Some(y_term));
                for m in main_left.flatten() {
                    let x = extract_str(&m.s());
                    new_facts.insert(Fact { s: x, p: p_str.clone(), o: z.clone() });
                }
            }
        }

        Ok(new_facts)
    }
}

/// prp-dom: Property Domain
/// If `?p rdfs:domain ?c` and `?x ?p ?y` -> `?x rdf:type ?c`
pub struct PrpDom;
impl InferenceRule for PrpDom {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdfs_domain = SimpleTerm::Iri(IriRef::new_unchecked(RDFS_DOMAIN.into()));

        // Match 1: Delta(?p, domain, ?c) JOIN Main(?x, ?p, ?y)
        let delta_domains = delta.triples_matching(Any, Some(&rdfs_domain), Any);
        for t1 in delta_domains.flatten() {
            let p_term = t1.s();
            let c = extract_str(&t1.o());
            for t2 in main.triples_matching(Any, Some(p_term), Any).flatten() {
                let x = extract_str(&t2.s());
                new_facts.insert(Fact { s: x, p: RDF_TYPE.to_string(), o: c.clone() });
            }
        }

        // Match 2: Delta(?x, ?p, ?y) JOIN Main(?p, domain, ?c)
        for t1 in delta.triples().flatten() {
            let p_term = t1.p();
            let x = extract_str(&t1.s());
            for t2 in main.triples_matching(Some(p_term), Some(&rdfs_domain), Any).flatten() {
                let c = extract_str(&t2.o());
                new_facts.insert(Fact { s: x.clone(), p: RDF_TYPE.to_string(), o: c });
            }
        }
        Ok(new_facts)
    }
}

/// prp-rng: Property Range
/// If `?p rdfs:range ?c` and `?x ?p ?y` -> `?y rdf:type ?c`
pub struct PrpRng;
impl InferenceRule for PrpRng {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdfs_range = SimpleTerm::Iri(IriRef::new_unchecked(RDFS_RANGE.into()));

        // Match 1: Delta(?p, range, ?c) JOIN Main(?x, ?p, ?y)
        let delta_ranges = delta.triples_matching(Any, Some(&rdfs_range), Any);
        for t1 in delta_ranges.flatten() {
            let p_term = t1.s();
            let c = extract_str(&t1.o());
            for t2 in main.triples_matching(Any, Some(p_term), Any).flatten() {
                let y = extract_str(&t2.o());
                new_facts.insert(Fact { s: y, p: RDF_TYPE.to_string(), o: c.clone() });
            }
        }

        // Match 2: Delta(?x, ?p, ?y) JOIN Main(?p, range, ?c)
        for t1 in delta.triples().flatten() {
            let p_term = t1.p();
            let y = extract_str(&t1.o());
            for t2 in main.triples_matching(Some(p_term), Some(&rdfs_range), Any).flatten() {
                let c = extract_str(&t2.o());
                new_facts.insert(Fact { s: y.clone(), p: RDF_TYPE.to_string(), o: c });
            }
        }
        Ok(new_facts)
    }
}

/// prp-inv: Inverse Properties
/// If `?p1 owl:inverseOf ?p2` and `?x ?p1 ?y` -> `?y ?p2 ?x` (and vice-versa)
pub struct PrpInv;
impl InferenceRule for PrpInv {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let owl_inverse_of = SimpleTerm::Iri(IriRef::new_unchecked(OWL_INVERSE_OF.into()));

        // Case 1: Delta inverse property definition JOIN main facts
        for t1 in delta.triples_matching(Any, Some(&owl_inverse_of), Any).flatten() {
            let p1_term = t1.s();
            let p2_term = t1.o();
            let p1_str = extract_str(&p1_term);
            let p2_str = extract_str(&p2_term);
            
            // ?x ?p1 ?y -> ?y ?p2 ?x
            for t2 in main.triples_matching(Any, Some(p1_term), Any).flatten() {
                let x = extract_str(&t2.s());
                let y = extract_str(&t2.o());
                new_facts.insert(Fact { s: y.clone(), p: p2_str.clone(), o: x.clone() });
            }
            // ?x ?p2 ?y -> ?y ?p1 ?x
            for t2 in main.triples_matching(Any, Some(p2_term), Any).flatten() {
                let x = extract_str(&t2.s());
                let y = extract_str(&t2.o());
                new_facts.insert(Fact { s: y.clone(), p: p1_str.clone(), o: x.clone() });
            }
        }

        // Case 2: Delta facts JOIN main inverse property definitions
        for t1 in delta.triples().flatten() {
            let p_term = t1.p();
            let x = extract_str(&t1.s());
            let y = extract_str(&t1.o());
            
            // p_term owl:inverseOf ?p2
            for t2 in main.triples_matching(Some(p_term), Some(&owl_inverse_of), Any).flatten() {
                let p2_str = extract_str(&t2.o());
                new_facts.insert(Fact { s: y.clone(), p: p2_str, o: x.clone() });
            }
            // ?p1 owl:inverseOf p_term
            for t2 in main.triples_matching(Any, Some(&owl_inverse_of), Some(p_term)).flatten() {
                let p1_str = extract_str(&t2.s());
                new_facts.insert(Fact { s: y.clone(), p: p1_str, o: x.clone() });
            }
        }
        Ok(new_facts)
    }
}

/// eq-rep-s: Equality/SameAs Substitution (Subject)
/// If `?s1 owl:sameAs ?s2` and `?s1 ?p ?o` -> `?s2 ?p ?o`
pub struct EqRepS;
impl InferenceRule for EqRepS {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let owl_same_as = SimpleTerm::Iri(IriRef::new_unchecked(OWL_SAME_AS.into()));

        for t1 in delta.triples_matching(Any, Some(&owl_same_as), Any).flatten() {
            let s1_term = t1.s();
            let s2 = extract_str(&t1.o());
            for t2 in main.triples_matching(Some(s1_term), Any, Any).flatten() {
                let p = extract_str(&t2.p());
                let o = extract_str(&t2.o());
                new_facts.insert(Fact { s: s2.clone(), p, o });
            }
        }

        for t1 in delta.triples().flatten() {
            let s1_term = t1.s();
            let p = extract_str(&t1.p());
            let o = extract_str(&t1.o());
            for t2 in main.triples_matching(Some(s1_term), Some(&owl_same_as), Any).flatten() {
                let s2 = extract_str(&t2.o());
                new_facts.insert(Fact { s: s2, p: p.clone(), o: o.clone() });
            }
        }
        Ok(new_facts)
    }
}

/// cax-eqc: Equivalent Classes
/// If `?c1 owl:equivalentClass ?c2` and `?x rdf:type ?c1` -> `?x rdf:type ?c2`
pub struct CaxEqc;
impl InferenceRule for CaxEqc {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        let owl_equiv_class = SimpleTerm::Iri(IriRef::new_unchecked(OWL_EQUIVALENT_CLASS.into()));

        // Case 1: Delta equiv class definitions
        for t1 in delta.triples_matching(Any, Some(&owl_equiv_class), Any).flatten() {
            let c1_term = t1.s();
            let c2 = extract_str(&t1.o());
            for t2 in main.triples_matching(Any, Some(&rdf_type), Some(c1_term)).flatten() {
                let x = extract_str(&t2.s());
                new_facts.insert(Fact { s: x, p: RDF_TYPE.to_string(), o: c2.clone() });
            }
            
            // Also symmetric
            let c1 = extract_str(&c1_term);
            let c2_term = t1.o();
            for t2 in main.triples_matching(Any, Some(&rdf_type), Some(c2_term)).flatten() {
                let x = extract_str(&t2.s());
                new_facts.insert(Fact { s: x, p: RDF_TYPE.to_string(), o: c1.clone() });
            }
        }

        Ok(new_facts)
    }
}

/// eq-ref: Reflexivity
/// If `?s ?p ?o` -> `?s sameAs ?s`, `?p sameAs ?p`, `?o sameAs ?o`
pub struct EqRef;
impl InferenceRule for EqRef {
    fn apply(&self, _main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        // Since semi-naive delta might only have some triples, we check all components in delta
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

/// eq-sym: Symmetry
/// If `?x owl:sameAs ?y` -> `?y owl:sameAs ?x`
pub struct EqSym;
impl InferenceRule for EqSym {
    fn apply(&self, _main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let same_as = SimpleTerm::Iri(IriRef::new_unchecked(OWL_SAME_AS.into()));
        for t in delta.triples_matching(Any, Some(&same_as), Any).flatten() {
            let x = extract_str(&t.s());
            let y = extract_str(&t.o());
            new_facts.insert(Fact { s: y, p: OWL_SAME_AS.to_string(), o: x });
        }
        Ok(new_facts)
    }
}

/// eq-trans: Transitivity
/// If `?x owl:sameAs ?y` and `?y owl:sameAs ?z` -> `?x owl:sameAs ?z`
pub struct EqTrans;
impl InferenceRule for EqTrans {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let same_as = SimpleTerm::Iri(IriRef::new_unchecked(OWL_SAME_AS.into()));
        
        // Delta(?x, sameAs, ?y) JOIN Main(?y, sameAs, ?z)
        for t1 in delta.triples_matching(Any, Some(&same_as), Any).flatten() {
            let x = extract_str(&t1.s());
            let y_term = t1.o();
            for t2 in main.triples_matching(Some(y_term), Some(&same_as), Any).flatten() {
                let z = extract_str(&t2.o());
                new_facts.insert(Fact { s: x.clone(), p: OWL_SAME_AS.to_string(), o: z });
            }
        }
        Ok(new_facts)
    }
}

/// eq-rep-p: Property Substitution
/// If `?p owl:sameAs ?q` and `?s ?p ?o` -> `?s ?q ?o`
pub struct EqRepP;
impl InferenceRule for EqRepP {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let same_as = SimpleTerm::Iri(IriRef::new_unchecked(OWL_SAME_AS.into()));
        
        // Delta(?p, sameAs, ?q) JOIN Main(?s, ?p, ?o)
        for t1 in delta.triples_matching(Any, Some(&same_as), Any).flatten() {
            let p_term = t1.s();
            let q = extract_str(&t1.o());
            for t2 in main.triples_matching(Any, Some(p_term), Any).flatten() {
                let s = extract_str(&t2.s());
                let o = extract_str(&t2.o());
                new_facts.insert(Fact { s, p: q.clone(), o });
            }
        }
        Ok(new_facts)
    }
}

/// eq-rep-o: Object Substitution
/// If `?o owl:sameAs ?o'` and `?s ?p ?o` -> `?s ?p ?o'`
pub struct EqRepO;
impl InferenceRule for EqRepO {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let same_as = SimpleTerm::Iri(IriRef::new_unchecked(OWL_SAME_AS.into()));
        
        // Delta(?o, sameAs, ?o') JOIN Main(?s, ?p, ?o)
        for t1 in delta.triples_matching(Any, Some(&same_as), Any).flatten() {
            let o_term = t1.s();
            let o_prime = extract_str(&t1.o());
            for t2 in main.triples_matching(Any, Any, Some(o_term)).flatten() {
                let s = extract_str(&t2.s());
                let p = extract_str(&t2.p());
                new_facts.insert(Fact { s, p, o: o_prime.clone() });
            }
        }
        Ok(new_facts)
    }
}

/// prp-irp: Irreflexive Property
/// If `?p rdf:type owl:IrreflexiveProperty` and `?x ?p ?x` -> Inconsistency
pub struct PrpIrp;
impl InferenceRule for PrpIrp {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        let irreflexive = SimpleTerm::Iri(IriRef::new_unchecked(OWL_IRREFLEXIVE_PROPERTY.into()));
        
        for t1 in main.triples_matching(Any, Some(&rdf_type), Some(&irreflexive)).flatten() {
            let p_term = t1.s();
            for t2 in delta.triples_matching(Any, Some(p_term), Any).flatten() {
                if t2.s() == t2.o() {
                    return Err(Inconsistency {
                        rule_name: "prp-irp".to_string(),
                        conflicting_triples: vec![
                            Fact { s: extract_str(&t1.s()), p: extract_str(&t1.p()), o: extract_str(&t1.o()) },
                            Fact { s: extract_str(&t2.s()), p: extract_str(&t2.p()), o: extract_str(&t2.o()) },
                        ],
                    });
                }
            }
        }
        Ok(HashSet::new())
    }
}

/// prp-asyp: Asymmetric Property
/// If `?p rdf:type owl:AsymmetricProperty` and `?x ?p ?y` and `?y ?p ?x` -> Inconsistency
pub struct PrpAsyp;
impl InferenceRule for PrpAsyp {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        let asymmetric = SimpleTerm::Iri(IriRef::new_unchecked(OWL_ASYMMETRIC_PROPERTY.into()));
        
        for t1 in main.triples_matching(Any, Some(&rdf_type), Some(&asymmetric)).flatten() {
            let p_term = t1.s();
            // Delta(?x, ?p, ?y) JOIN Main(?y, ?p, ?x)
            for t2 in delta.triples_matching(Any, Some(p_term), Any).flatten() {
                let x_term = t2.s();
                let y_term = t2.o();
                if main.contains(y_term, p_term, x_term).unwrap_or(false) {
                    return Err(Inconsistency {
                        rule_name: "prp-asyp".to_string(),
                        conflicting_triples: vec![
                            Fact { s: extract_str(&t1.s()), p: extract_str(&t1.p()), o: extract_str(&t1.o()) },
                            Fact { s: extract_str(&t2.s()), p: extract_str(&t2.p()), o: extract_str(&t2.o()) },
                            Fact { s: extract_str(y_term), p: extract_str(p_term), o: extract_str(x_term) },
                        ],
                    });
                }
            }
        }
        Ok(HashSet::new())
    }
}

/// eq-diff1: Equality/Inequality contradiction
/// If `?x owl:sameAs ?y` and `?x owl:differentFrom ?y` -> Inconsistency
pub struct EqDiff1;
impl InferenceRule for EqDiff1 {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let same_as = SimpleTerm::Iri(IriRef::new_unchecked(OWL_SAME_AS.into()));
        let different_from = SimpleTerm::Iri(IriRef::new_unchecked(OWL_DIFFERENT_FROM.into()));
        
        // Delta(?x, sameAs, ?y) JOIN Main(?x, differentFrom, ?y)
        for t1 in delta.triples_matching(Any, Some(&same_as), Any).flatten() {
            let x_term = t1.s();
            let y_term = t1.o();
            if main.contains(x_term, &different_from, y_term).unwrap_or(false) {
                return Err(Inconsistency {
                    rule_name: "eq-diff1".to_string(),
                    conflicting_triples: vec![
                        Fact { s: extract_str(&t1.s()), p: extract_str(&t1.p()), o: extract_str(&t1.o()) },
                        Fact { s: extract_str(x_term), p: OWL_DIFFERENT_FROM.to_string(), o: extract_str(y_term) },
                    ],
                });
            }
        }
        Ok(HashSet::new())
    }
}

/// prp-spo2: Property Chains
/// If `?p owl:propertyChainAxiom (?p1 ?p2 ... ?pn)` and `?x1 ?p1 ?x2`, `?x2 ?p2 ?x3` ... -> `?x1 ?p ?xn+1`
pub struct PrpSpo2;
impl InferenceRule for PrpSpo2 {
    fn apply(&self, main: &DarkstarGraph, _delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let axiom = SimpleTerm::Iri(IriRef::new_unchecked(OWL_PROPERTY_CHAIN_AXIOM.into()));
        
        for t in main.triples_matching(Any, Some(&axiom), Any).flatten() {
            let p_axiom = extract_str(&t.s());
            let chain = get_rdf_list(main, &t.o());
            if chain.is_empty() { continue; }

            // Find all starts ?x1 where ?x1 ?p1 ?x2
            let first_p = engine::InferenceEngine::make_term(&chain[0]);
            for t1 in main.triples_matching(Any, Some(&first_p), Any).flatten() {
                let mut current_nodes = vec![extract_str(&t1.o())];
                
                for p_str in &chain[1..] {
                    let mut next_nodes = Vec::new();
                    let p_term = engine::InferenceEngine::make_term(p_str);
                    for node in current_nodes {
                        let node_term = engine::InferenceEngine::make_term(&node);
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

/// cls-int1: Intersection (1)
/// If `?c owl:intersectionOf ?list` and `?y` has type of all classes in `?list` -> `?y rdf:type ?c`
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

            // Find candidates: individuals that have type of the first class in the list
            let first_class = engine::InferenceEngine::make_term(&list[0]);
            for t_y in main.triples_matching(Any, Some(&rdf_type), Some(&first_class)).flatten() {
                let y = extract_str(&t_y.s());
                let y_term = engine::InferenceEngine::make_term(&y);
                
                let mut has_all = true;
                for other_class_str in &list[1..] {
                    let other_class = engine::InferenceEngine::make_term(other_class_str);
                    if main.triples_matching(Some(&y_term), Some(&rdf_type), Some(&other_class)).next().is_none() {
                        has_all = false;
                        break;
                    }
                }
                
                if has_all {
                    new_facts.insert(Fact { s: y, p: RDF_TYPE.to_string(), o: c.clone() });
                }
            }
        }
        Ok(new_facts)
    }
}

/// cls-int2: Intersection (2)
/// If `?c owl:intersectionOf ?list` and `?y rdf:type ?c` -> `?y rdf:type ?ci` for all `?ci` in `?list`
pub struct ClsInt2;
impl InferenceRule for ClsInt2 {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        let intersection_of = SimpleTerm::Iri(IriRef::new_unchecked(OWL_INTERSECTION_OF.into()));

        // Delta(?y, type, ?c) JOIN Main(?c, intersectionOf, ?list)
        for t in delta.triples_matching(Any, Some(&rdf_type), Any).flatten() {
            let y = extract_str(&t.s());
            let c_term = t.o();
            
            for t_int in main.triples_matching(Some(c_term), Some(&intersection_of), Any).flatten() {
                let list = get_rdf_list(main, &t_int.o());
                for ci in list {
                    new_facts.insert(Fact { s: y.clone(), p: RDF_TYPE.to_string(), o: ci });
                }
            }
        }
        Ok(new_facts)
    }
}

/// cls-uni: Union
/// If `?c owl:unionOf ?list` and `?y rdf:type ?ci` where `?ci` in `?list` -> `?y rdf:type ?c`
pub struct ClsUni;
impl InferenceRule for ClsUni {
    fn apply(&self, main: &DarkstarGraph, delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        let union_of = SimpleTerm::Iri(IriRef::new_unchecked(OWL_UNION_OF.into()));

        for t in delta.triples_matching(Any, Some(&rdf_type), Any).flatten() {
            let y = extract_str(&t.s());
            let ci_str = extract_str(&t.o());
            
            for t_uni in main.triples_matching(Any, Some(&union_of), Any).flatten() {
                let list = get_rdf_list(main, &t_uni.o());
                if list.contains(&ci_str) {
                    let c = extract_str(&t_uni.s());
                    new_facts.insert(Fact { s: y.clone(), p: RDF_TYPE.to_string(), o: c });
                }
            }
        }
        Ok(new_facts)
    }
}

/// cls-svf1: SomeValuesFrom
/// If `?x owl:someValuesFrom ?y` on `?p` and `?u ?p ?v` and `?v rdf:type ?y` -> `?u rdf:type ?x`
pub struct ClsSvf1;
impl InferenceRule for ClsSvf1 {
    fn apply(&self, main: &DarkstarGraph, _delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        let on_property = SimpleTerm::Iri(IriRef::new_unchecked(OWL_ON_PROPERTY.into()));
        let some_values = SimpleTerm::Iri(IriRef::new_unchecked(OWL_SOME_VALUES_FROM.into()));

        for t_restr in main.triples_matching(Any, Some(&some_values), Any).flatten() {
            let x = extract_str(&t_restr.s());
            let y_term = t_restr.o();
            
            if let Some(t_prop) = main.triples_matching(Some(t_restr.s()), Some(&on_property), Any).next() {
                let p_term = t_prop.unwrap().o();
                
                for t_v in main.triples_matching(Any, Some(&rdf_type), Some(y_term)).flatten() {
                    let v_term = t_v.s();
                    for t_u in main.triples_matching(Any, Some(p_term), Some(v_term)).flatten() {
                        let u = extract_str(&t_u.s());
                        new_facts.insert(Fact { s: u, p: RDF_TYPE.to_string(), o: x.clone() });
                    }
                }
            }
        }
        Ok(new_facts)
    }
}

/// cls-avf: AllValuesFrom
/// If `?x owl:allValuesFrom ?y` on `?p` and `?u rdf:type ?x` and `?u ?p ?v` -> `?v rdf:type ?y`
pub struct ClsAvf;
impl InferenceRule for ClsAvf {
    fn apply(&self, main: &DarkstarGraph, _delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        let on_property = SimpleTerm::Iri(IriRef::new_unchecked(OWL_ON_PROPERTY.into()));
        let all_values = SimpleTerm::Iri(IriRef::new_unchecked(OWL_ALL_VALUES_FROM.into()));

        for t_restr in main.triples_matching(Any, Some(&all_values), Any).flatten() {
            let y = extract_str(&t_restr.o());
            if let Some(t_prop) = main.triples_matching(Some(t_restr.s()), Some(&on_property), Any).next() {
                let p_term = t_prop.unwrap().o();
                let x_term = t_restr.s();
                
                for t_u in main.triples_matching(Any, Some(&rdf_type), Some(x_term)).flatten() {
                    let u_term = t_u.s();
                    for t_v in main.triples_matching(Some(u_term), Some(p_term), Any).flatten() {
                        let v = extract_str(&t_v.o());
                        new_facts.insert(Fact { s: v, p: RDF_TYPE.to_string(), o: y.clone() });
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
            let key_list_node = t_key.o();
            let key_properties = get_rdf_list(main, &key_list_node);
            if key_properties.is_empty() { continue; }
            let class_term = SimpleTerm::Iri(IriRef::new_unchecked(class_uri.strip_prefix("i:").unwrap_or(&class_uri).into()));
            let individuals: Vec<String> = main.triples_matching(Any, Some(&rdf_type), Some(&class_term)).flatten().map(|t| extract_str(&t.s())).collect();
            for i in 0..individuals.len() {
                for j in i+1..individuals.len() {
                    let x = &individuals[i];
                    let y = &individuals[j];
                    let mut all_match = true;
                    for prop in &key_properties {
                        let prop_iri = prop.strip_prefix("i:").unwrap_or(prop);
                        let prop_term = SimpleTerm::Iri(IriRef::new_unchecked(prop_iri.into()));
                        let x_iri = x.strip_prefix("i:").unwrap_or(x);
                        let y_iri = y.strip_prefix("i:").unwrap_or(y);
                        let x_term = SimpleTerm::Iri(IriRef::new_unchecked(x_iri.into()));
                        let y_term = SimpleTerm::Iri(IriRef::new_unchecked(y_iri.into()));
                        let x_vals: HashSet<String> = main.triples_matching(Some(&x_term), Some(&prop_term), Any).flatten().map(|t| extract_str(&t.o())).collect();
                        let y_vals: HashSet<String> = main.triples_matching(Some(&y_term), Some(&prop_term), Any).flatten().map(|t| extract_str(&t.o())).collect();
                        if x_vals.is_empty() || y_vals.is_empty() || x_vals != y_vals { all_match = false; break; }
                    }
                    if all_match { new_facts.insert(Fact { s: x.clone(), p: OWL_SAME_AS.to_string(), o: y.clone() }); }
                }
            }
        }
        Ok(new_facts)
    }
}

pub struct DtDiff;
impl InferenceRule for DtDiff {
    fn apply(&self, main: &DarkstarGraph, _delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let owl_same_as = SimpleTerm::Iri(IriRef::new_unchecked(OWL_SAME_AS.into()));
        for t in main.triples_matching(Any, Some(&owl_same_as), Any).flatten() {
            let s = t.s();
            let o = t.o();
            if let (Some(_l1), Some(_l2)) = (s.lexical_form(), o.lexical_form()) {
                let dt1 = s.datatype().map(|d| d.to_string()).unwrap_or_default();
                let dt2 = o.datatype().map(|d| d.to_string()).unwrap_or_default();
                if dt1 != dt2 && !dt1.is_empty() && !dt2.is_empty() {
                    if (dt1.contains("integer") && dt2.contains("string")) || (dt1.contains("string") && dt2.contains("integer")) {
                        return Err(Inconsistency { 
                            rule_name: "dt-diff".into(), 
                            conflicting_triples: vec![
                                Fact { s: extract_str(&s), p: OWL_SAME_AS.to_string(), o: extract_str(&o) }
                            ]
                        });
                    }
                }
            }
        }
        Ok(HashSet::new())
    }
}

pub struct ScmSco;
impl InferenceRule for ScmSco {
    fn apply(&self, main: &DarkstarGraph, _delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let sco = SimpleTerm::Iri(IriRef::new_unchecked(RDFS_SUBCLASS_OF.into()));
        for t1 in main.triples_matching(Any, Some(&sco), Any).flatten() {
            let c1 = t1.s();
            let c2 = t1.o();
            for t2 in main.triples_matching(Some(c2), Some(&sco), Any).flatten() {
                let c3 = extract_str(&t2.o());
                new_facts.insert(Fact { s: extract_str(&c1), p: RDFS_SUBCLASS_OF.to_string(), o: c3 });
            }
        }
        Ok(new_facts)
    }
}

pub struct ScmSpo;
impl InferenceRule for ScmSpo {
    fn apply(&self, main: &DarkstarGraph, _delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let mut new_facts = HashSet::new();
        let spo = SimpleTerm::Iri(IriRef::new_unchecked(RDFS_SUBPROPERTY_OF.into()));
        for t1 in main.triples_matching(Any, Some(&spo), Any).flatten() {
            let p1 = t1.s();
            let p2 = t1.o();
            for t2 in main.triples_matching(Some(p2), Some(&spo), Any).flatten() {
                let p3 = extract_str(&t2.o());
                new_facts.insert(Fact { s: extract_str(&p1), p: RDFS_SUBPROPERTY_OF.to_string(), o: p3 });
            }
        }
        Ok(new_facts)
    }
}

pub struct ClsDj;
impl InferenceRule for ClsDj {
    fn apply(&self, main: &DarkstarGraph, _delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let disjoint_with = SimpleTerm::Iri(IriRef::new_unchecked(OWL_DISJOINT_WITH.into()));
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        for t_dj in main.triples_matching(Any, Some(&disjoint_with), Any).flatten() {
            let c1 = t_dj.s();
            let c2 = t_dj.o();
            for t1 in main.triples_matching(Any, Some(&rdf_type), Some(c1)).flatten() {
                let x = t1.s();
                if main.contains(x, &rdf_type, c2).unwrap_or(false) {
                    return Err(Inconsistency { rule_name: "cls-dj".into(), conflicting_triples: vec![Fact { s: extract_str(&x), p: RDF_TYPE.to_string(), o: extract_str(&c1) }, Fact { s: extract_str(&x), p: RDF_TYPE.to_string(), o: extract_str(&c2) }] });
                }
            }
        }
        Ok(HashSet::new())
    }
}

pub struct PrpNpa;
impl InferenceRule for PrpNpa {
    fn apply(&self, main: &DarkstarGraph, _delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let npa_type = SimpleTerm::Iri(IriRef::new_unchecked(OWL_NEG_OBJ_PROP_ASSERT.into()));
        let source_ind = SimpleTerm::Iri(IriRef::new_unchecked(OWL_SOURCE_INDIVIDUAL.into()));
        let assert_prop = SimpleTerm::Iri(IriRef::new_unchecked(OWL_ASSERTED_PROPERTY.into()));
        let target_ind = SimpleTerm::Iri(IriRef::new_unchecked(OWL_TARGET_INDIVIDUAL.into()));
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        for t_npa in main.triples_matching(Any, Some(&rdf_type), Some(&npa_type)).flatten() {
            let npa_node = t_npa.s();
            if let (Some(Ok(t_s)), Some(Ok(t_p)), Some(Ok(t_t))) = (main.triples_matching(Some(npa_node), Some(&source_ind), Any).next(), main.triples_matching(Some(npa_node), Some(&assert_prop), Any).next(), main.triples_matching(Some(npa_node), Some(&target_ind), Any).next()) {
                let s = t_s.o();
                let p = t_p.o();
                let t = t_t.o();
                if main.contains(s, p, t).unwrap_or(false) {
                    return Err(Inconsistency { rule_name: "prp-npa".into(), conflicting_triples: vec![Fact { s: extract_str(&s), p: extract_str(&p), o: extract_str(&t) }] });
                }
            }
        }
        Ok(HashSet::new())
    }
}

pub struct ClsNothing;
impl InferenceRule for ClsNothing {
    fn apply(&self, main: &DarkstarGraph, _delta: &DarkstarGraph) -> Result<HashSet<Fact>, Inconsistency> {
        let rdf_type = SimpleTerm::Iri(IriRef::new_unchecked(RDF_TYPE.into()));
        let owl_nothing = SimpleTerm::Iri(IriRef::new_unchecked(OWL_NOTHING.into()));
        
        if let Some(Ok(t)) = main.triples_matching(Any, Some(&rdf_type), Some(&owl_nothing)).next() {
            return Err(Inconsistency {
                rule_name: "cls-nothing".into(),
                conflicting_triples: vec![Fact { s: extract_str(&t.s()), p: RDF_TYPE.to_string(), o: OWL_NOTHING.to_string() }]
            });
        }
        Ok(HashSet::new())
    }
}
