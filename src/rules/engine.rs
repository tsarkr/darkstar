use crate::memory::EngineMemory;
use super::{InferenceRule, Fact};
use rayon::prelude::*;
use std::collections::HashSet;
use sophia::api::prelude::*;
use sophia::inmem::graph::FastGraph;
use sophia::api::term::{SimpleTerm, IriRef, BnodeId};

pub struct InferenceEngine {
    pub rules_core: Vec<Box<dyn InferenceRule>>,
    pub rules_equality: Vec<Box<dyn InferenceRule>>,
    pub rules_chains: Vec<Box<dyn InferenceRule>>,
    pub rules_expressions: Vec<Box<dyn InferenceRule>>,
    pub rules_restrictions: Vec<Box<dyn InferenceRule>>,
    pub rules_datatypes: Vec<Box<dyn InferenceRule>>,
    pub rules_schema: Vec<Box<dyn InferenceRule>>,
}

impl InferenceEngine {
    pub fn new() -> Self {
        Self {
            rules_core: vec![
                Box::new(super::CaxSco),
                Box::new(super::PrpSpo1),
                Box::new(super::PrpTrp),
                Box::new(super::PrpDom),
                Box::new(super::PrpRng),
                Box::new(super::PrpInv),
                Box::new(super::CaxEqc),
            ],
            rules_equality: vec![
                Box::new(super::EqRef),
                Box::new(super::EqSym),
                Box::new(super::EqTrans),
                Box::new(super::EqRepS),
                Box::new(super::EqRepP),
                Box::new(super::EqRepO),
                Box::new(super::EqDiff1),
            ],
            rules_chains: vec![
                Box::new(super::PrpSpo2),
            ],
            rules_expressions: vec![
                Box::new(super::ClsInt1),
                Box::new(super::ClsInt2),
                Box::new(super::ClsUni),
            ],
            rules_restrictions: vec![
                Box::new(super::ClsSvf1),
                Box::new(super::ClsAvf),
                Box::new(super::PrpIrp),
                Box::new(super::PrpAsyp),
                Box::new(super::PrpKey),
            ],
            rules_datatypes: vec![
                Box::new(super::DtDiff),
            ],
            rules_schema: vec![
                Box::new(super::ScmSco),
                Box::new(super::ScmSpo),
                Box::new(super::ClsDj),
                Box::new(super::PrpNpa),
                Box::new(super::ClsNothing),
            ],
        }
    }

    /// Helper to convert a string to a SimpleTerm for safe graph insertion.
    /// Decodes prefixes: i: (IRI), l: (Literal), b: (BlankNode)
    pub fn make_term(s: &str) -> SimpleTerm<'static> {
        let leaked_str: &'static str = Box::leak(s.to_string().into_boxed_str());
        if s.starts_with("i:") {
            SimpleTerm::Iri(IriRef::new_unchecked((&leaked_str[2..]).into()))
        } else if s.starts_with("l:") {
            // Sophia 0.8: Literals must have a datatype or language. Default to xsd:string.
            let xsd_string = IriRef::new_unchecked("http://www.w3.org/2001/XMLSchema#string".into());
            SimpleTerm::LiteralDatatype((&leaked_str[2..]).into(), xsd_string)
        } else if s.starts_with("b:") {
            SimpleTerm::BlankNode(BnodeId::new_unchecked((&leaked_str[2..]).into()))
        } else {
            // Fallback for unprefixed strings (treat as IRI)
            SimpleTerm::Iri(IriRef::new_unchecked(leaked_str.into()))
        }
    }

    /// Executes the forward-chaining semi-naive evaluation algorithm.
    pub fn run_inference(&self, memory: &mut EngineMemory, rules_settings: &crate::core::settings::ReasonerRules) {
        println!("Starting parallel forward-chaining inference...");
        let mut iteration = 1;
        
        // Active rules based on settings
        let mut active_rules: Vec<&Box<dyn InferenceRule>> = Vec::new();
        active_rules.extend(self.rules_core.iter());
        if rules_settings.equality { active_rules.extend(self.rules_equality.iter()); }
        if rules_settings.property_chains { active_rules.extend(self.rules_chains.iter()); }
        if rules_settings.class_expressions { active_rules.extend(self.rules_expressions.iter()); }
        if rules_settings.restrictions { active_rules.extend(self.rules_restrictions.iter()); }
        if rules_settings.datatypes { active_rules.extend(self.rules_datatypes.iter()); }
        if rules_settings.schema { active_rules.extend(self.rules_schema.iter()); }

        loop {
            println!("Iteration {}...", iteration);
            
            let results: Result<Vec<HashSet<Fact>>, super::Inconsistency> = active_rules.par_iter()
                .map(|rule| rule.apply(&memory.main_graph, &memory.delta_graph))
                .collect();
            
            let new_facts: HashSet<Fact> = match results {
                Ok(fact_sets) => {
                    let mut all_facts = HashSet::new();
                    for set in fact_sets {
                        all_facts.extend(set);
                    }
                    all_facts
                },
                Err(inconsistency) => {
                    println!("LOGICAL INCONSISTENCY DETECTED: {:?}", inconsistency);
                    memory.is_consistent = false;
                    memory.inconsistencies.push(inconsistency);
                    return; // Halt inference
                }
            };
            
            // 2. Prepare the next delta graph
            let mut next_delta = FastGraph::new();
            let mut truly_new_count = 0;

            // 3. Materialize inferred facts.
            // Insert into main_graph. If it's truly new, also add to next_delta.
            for fact in new_facts {
                let s_term = Self::make_term(&fact.s);
                let p_term = Self::make_term(&fact.p);
                let o_term = Self::make_term(&fact.o);

                // `FastGraph::insert` returns true if the triple was not already present.
                if memory.main_graph.insert(&s_term, &p_term, &o_term).unwrap_or(false) {
                    next_delta.insert(&s_term, &p_term, &o_term).unwrap();
                    memory.inferred_graph.insert(&s_term, &p_term, &o_term).unwrap();
                    truly_new_count += 1;
                }
            }
            
            // 4. Update the memory's delta graph for the next iteration
            memory.delta_graph = next_delta;

            println!("Iteration {} produced {} new inferred facts.", iteration, truly_new_count);
            
            // 5. Fixpoint Check
            if truly_new_count == 0 {
                println!("Fixpoint reached. Inference complete.");
                break;
            }
            
            iteration += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::rules::{RDF_TYPE, RDFS_SUBCLASS_OF, OWL_TRANSITIVE_PROPERTY};

    fn ensure_prefix(s: &str) -> String {
        if s.len() >= 2 && &s[1..2] == ":" {
            s.to_string()
        } else {
            format!("i:{}", s)
        }
    }

    /// Helper to efficiently insert base facts into the memory model.
    /// Manually injects asserted triples into both the `main_graph` and the `delta_graph`
    /// to bootstrap the 0-th iteration of semi-naive evaluation.
    fn setup_mock_graph(memory: &mut EngineMemory, facts: &[(&str, &str, &str)]) {
        for &(s, p, o) in facts {
            let s_term = InferenceEngine::make_term(&ensure_prefix(s));
            let p_term = InferenceEngine::make_term(&ensure_prefix(p));
            let o_term = InferenceEngine::make_term(&ensure_prefix(o));
            
            memory.asserted_graph.insert(&s_term, &p_term, &o_term).unwrap();
            memory.main_graph.insert(&s_term, &p_term, &o_term).unwrap();
            memory.delta_graph.insert(&s_term, &p_term, &o_term).unwrap();
        }
    }

    /// Helper to assert the presence of a specific fact within the materialized main graph.
    fn assert_fact_exists(memory: &EngineMemory, s: &str, p: &str, o: &str) -> bool {
        let s_term = InferenceEngine::make_term(&ensure_prefix(s));
        let p_term = InferenceEngine::make_term(&ensure_prefix(p));
        let o_term = InferenceEngine::make_term(&ensure_prefix(o));
        memory.main_graph.contains(&s_term, &p_term, &o_term).unwrap()
    }

    #[test]
    fn test_cax_sco() {
        let engine = InferenceEngine::new();
        let mut memory = EngineMemory::new();

        let rickshaw_puller = "http://example.org/RickshawPuller";
        let worker = "http://example.org/Worker";
        let person = "http://example.org/Person";
        let kim = "http://example.org/Kim";

        // Asserted Facts
        setup_mock_graph(&mut memory, &[
            (rickshaw_puller, RDFS_SUBCLASS_OF, worker),
            (worker, RDFS_SUBCLASS_OF, person),
            (kim, RDF_TYPE, rickshaw_puller),
        ]);

        // Action: Run inference
        engine.run_inference(&mut memory);

        // Expected Inferred Facts (Assertions)
        assert!(
            assert_fact_exists(&memory, kim, RDF_TYPE, worker),
            "Failed to infer: Kim is a Worker (cax-sco direct)"
        );
        assert!(
            assert_fact_exists(&memory, kim, RDF_TYPE, person),
            "Failed to infer: Kim is a Person (recursive cax-sco failed)"
        );
    }

    #[test]
    fn test_prp_trp() {
        let engine = InferenceEngine::new();
        let mut memory = EngineMemory::new();

        let located_in = "http://example.org/locatedIn";
        let jongno = "http://example.org/Jongno";
        let seoul = "http://example.org/Seoul";
        let korea = "http://example.org/Korea";

        // Asserted Facts
        setup_mock_graph(&mut memory, &[
            (located_in, RDF_TYPE, OWL_TRANSITIVE_PROPERTY),
            (jongno, located_in, seoul),
            (seoul, located_in, korea),
        ]);

        // Action: Run inference
        engine.run_inference(&mut memory);

        // Expected Inferred Facts (Assertions)
        assert!(
            assert_fact_exists(&memory, jongno, located_in, korea),
            "Failed to infer: Jongno locatedIn Korea (prp-trp failed)"
        );
    }

    #[test]
    fn test_domain_range() {
        let mut memory = EngineMemory::new();
        let engine = InferenceEngine::new();
        
        let participated_in = "http://example.org/participatedIn";
        let person = "http://example.org/Person";
        let event = "http://example.org/Event";
        
        // Assertions mapping
        setup_mock_graph(&mut memory, &[
            (participated_in, crate::rules::RDFS_DOMAIN, person),
            (participated_in, crate::rules::RDFS_RANGE, event),
        ]);
        
        let kim = "http://example.org/Kim_Amugae";
        let march_first = "http://example.org/March_First_Movement";
        
        setup_mock_graph(&mut memory, &[
            (kim, participated_in, march_first),
        ]);
        
        engine.run_inference(&mut memory);
        
        assert!(memory.inferred_graph.contains(
            &InferenceEngine::make_term(kim),
            &InferenceEngine::make_term(crate::rules::RDF_TYPE),
            &InferenceEngine::make_term(person)
        ).unwrap(), "Domain inference failed: Kim is not Person");
        
        assert!(memory.inferred_graph.contains(
            &InferenceEngine::make_term(march_first),
            &InferenceEngine::make_term(crate::rules::RDF_TYPE),
            &InferenceEngine::make_term(event)
        ).unwrap(), "Range inference failed: Movement is not Event");
    }

    #[test]
    fn test_inverse_properties() {
        let mut memory = EngineMemory::new();
        let engine = InferenceEngine::new();
        
        let has_participant = "http://example.org/hasParticipant";
        let participated_in = "http://example.org/participatedIn";
        
        setup_mock_graph(&mut memory, &[
            (has_participant, crate::rules::OWL_INVERSE_OF, participated_in),
        ]);
        
        let kim = "http://example.org/Kim_Amugae";
        let march_first = "http://example.org/March_First_Movement";
        
        setup_mock_graph(&mut memory, &[
            (march_first, has_participant, kim),
        ]);
        
        engine.run_inference(&mut memory);
        
        assert!(memory.inferred_graph.contains(
            &InferenceEngine::make_term(kim),
            &InferenceEngine::make_term(participated_in),
            &InferenceEngine::make_term(march_first)
        ).unwrap(), "Inverse property inference failed");
    }

    #[test]
    fn test_equality_logic() {
        let engine = InferenceEngine::new();
        let mut memory = EngineMemory::new();
        
        let a = "http://example.org/A";
        let b = "http://example.org/B";
        let c = "http://example.org/C";
        let same_as = crate::rules::OWL_SAME_AS;

        setup_mock_graph(&mut memory, &[
            (a, same_as, b),
            (b, same_as, c),
        ]);

        engine.run_inference(&mut memory);

        // eq-trans: A sameAs C
        assert!(assert_fact_exists(&memory, a, same_as, c), "Transitivity failed");
        // eq-sym: B sameAs A
        assert!(assert_fact_exists(&memory, b, same_as, a), "Symmetry failed");
        // eq-sym + eq-trans: C sameAs A
        assert!(assert_fact_exists(&memory, c, same_as, a), "C sameAs A failed");
    }

    #[test]
    fn test_logical_inconsistency() {
        let engine = InferenceEngine::new();
        let mut memory = EngineMemory::new();
        
        let a = "http://example.org/A";
        let b = "http://example.org/B";
        let same_as = crate::rules::OWL_SAME_AS;
        let diff_from = crate::rules::OWL_DIFFERENT_FROM;

        setup_mock_graph(&mut memory, &[
            (a, same_as, b),
            (a, diff_from, b),
        ]);

        engine.run_inference(&mut memory);

        assert!(!memory.is_consistent, "Engine failed to detect inconsistency");
        assert_eq!(memory.inconsistencies[0].rule_name, "eq-diff1");
    }

    #[test]
    fn test_property_chains() {
        let engine = InferenceEngine::new();
        let mut memory = EngineMemory::new();
        
        let has_parent = "http://example.org/hasParent";
        let has_brother = "http://example.org/hasBrother";
        let has_uncle = "http://example.org/hasUncle";
        let kim = "http://example.org/Kim";
        let lee = "http://example.org/Lee";
        let park = "http://example.org/Park";

        // Setup Chain: hasUncle = hasParent -> hasBrother
        let chain_head = "http://example.org/list1";
        let chain_rest = "http://example.org/list2";
        
        setup_mock_graph(&mut memory, &[
            (has_uncle, crate::rules::OWL_PROPERTY_CHAIN_AXIOM, chain_head),
            (chain_head, crate::rules::RDF_FIRST, has_parent),
            (chain_head, crate::rules::RDF_REST, chain_rest),
            (chain_rest, crate::rules::RDF_FIRST, has_brother),
            (chain_rest, crate::rules::RDF_REST, crate::rules::RDF_NIL),
            
            (kim, has_parent, lee),
            (lee, has_brother, park),
        ]);

        engine.run_inference(&mut memory);

        assert!(assert_fact_exists(&memory, kim, has_uncle, park), "Property chain inference failed");
    }

    #[test]
    fn test_intersection_logic() {
        let engine = InferenceEngine::new();
        let mut memory = EngineMemory::new();
        
        let person = "http://example.org/Person";
        let worker = "http://example.org/Worker";
        let working_person = "http://example.org/WorkingPerson";
        let kim = "http://example.org/Kim";

        // working_person owl:intersectionOf (person worker)
        let list1 = "http://example.org/l1";
        let list2 = "http://example.org/l2";

        setup_mock_graph(&mut memory, &[
            (working_person, crate::rules::OWL_INTERSECTION_OF, list1),
            (list1, crate::rules::RDF_FIRST, person),
            (list1, crate::rules::RDF_REST, list2),
            (list2, crate::rules::RDF_FIRST, worker),
            (list2, crate::rules::RDF_REST, crate::rules::RDF_NIL),
            
            (kim, crate::rules::RDF_TYPE, person),
            (kim, crate::rules::RDF_TYPE, worker),
        ]);

        engine.run_inference(&mut memory);

        assert!(assert_fact_exists(&memory, kim, crate::rules::RDF_TYPE, working_person), "Intersection (cls-int1) failed");
    }
}
