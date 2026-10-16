use crate::memory::EngineMemory;
use super::{InferenceRule, Fact};
use rayon::prelude::*;
use std::collections::HashSet;
use sophia::api::prelude::*;
use sophia::inmem::graph::FastGraph;
use sophia::api::term::{SimpleTerm, IriRef, BnodeId, LanguageTag};
use std::sync::RwLock;
use std::hash::Hasher;
use once_cell::sync::Lazy;

const INTERN_SHARDS: usize = 64;

/// Sharded RwLock string interner to eliminate lock contention across Rayon threads
static STR_INTERNER: Lazy<[RwLock<HashSet<&'static str>>; INTERN_SHARDS]> =
    Lazy::new(|| std::array::from_fn(|_| RwLock::new(HashSet::new())));

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
                // NOTE: eq-ref (x owl:sameAs x for every term) is intentionally not materialized.
                // It adds one triple per distinct term (hundreds of thousands on large datasets)
                // and forces every eq-rep rule to rescan the whole graph, while deriving nothing
                // that other rules need. `super::EqRef` is kept available for explicit use.
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

    /// Optimized lock-free-fast-path + sharded RwLock string interning
    pub fn intern(s: &str) -> &'static str {
        use crate::vocab::*;
        match s {
            RDF_TYPE => return RDF_TYPE,
            RDFS_SUBCLASS_OF => return RDFS_SUBCLASS_OF,
            RDFS_SUBPROPERTY_OF => return RDFS_SUBPROPERTY_OF,
            RDFS_DOMAIN => return RDFS_DOMAIN,
            RDFS_RANGE => return RDFS_RANGE,
            RDFS_LABEL => return RDFS_LABEL,
            RDFS_CLASS => return RDFS_CLASS,
            OWL_CLASS => return OWL_CLASS,
            OWL_THING => return OWL_THING,
            OWL_NOTHING => return OWL_NOTHING,
            OWL_SAME_AS => return OWL_SAME_AS,
            OWL_DIFFERENT_FROM => return OWL_DIFFERENT_FROM,
            OWL_EQUIVALENT_CLASS => return OWL_EQUIVALENT_CLASS,
            OWL_DISJOINT_WITH => return OWL_DISJOINT_WITH,
            OWL_INVERSE_OF => return OWL_INVERSE_OF,
            OWL_NAMED_INDIVIDUAL => return OWL_NAMED_INDIVIDUAL,
            OWL_OBJECT_PROPERTY => return OWL_OBJECT_PROPERTY,
            OWL_DATATYPE_PROPERTY => return OWL_DATATYPE_PROPERTY,
            OWL_ANNOTATION_PROPERTY => return OWL_ANNOTATION_PROPERTY,
            OWL_TRANSITIVE_PROPERTY => return OWL_TRANSITIVE_PROPERTY,
            OWL_SYMMETRIC_PROPERTY => return OWL_SYMMETRIC_PROPERTY,
            OWL_FUNCTIONAL_PROPERTY => return OWL_FUNCTIONAL_PROPERTY,
            OWL_INVERSE_FUNCTIONAL_PROPERTY => return OWL_INVERSE_FUNCTIONAL_PROPERTY,
            OWL_RESTRICTION => return OWL_RESTRICTION,
            OWL_ON_PROPERTY => return OWL_ON_PROPERTY,
            OWL_SOME_VALUES_FROM => return OWL_SOME_VALUES_FROM,
            OWL_ALL_VALUES_FROM => return OWL_ALL_VALUES_FROM,
            OWL_INTERSECTION_OF => return OWL_INTERSECTION_OF,
            OWL_UNION_OF => return OWL_UNION_OF,
            OWL_PROPERTY_CHAIN_AXIOM => return OWL_PROPERTY_CHAIN_AXIOM,
            OWL_HAS_KEY => return OWL_HAS_KEY,
            RDF_FIRST => return RDF_FIRST,
            RDF_REST => return RDF_REST,
            RDF_NIL => return RDF_NIL,
            XSD_STRING => return XSD_STRING,
            "ko" => return "ko",
            "en" => return "en",
            _ => {}
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(s, &mut hasher);
        let shard = &STR_INTERNER[(hasher.finish() as usize) & (INTERN_SHARDS - 1)];
        {
            let read_guard = shard.read().unwrap();
            if let Some(&existing) = read_guard.get(s) {
                return existing;
            }
        }
        let mut write_guard = shard.write().unwrap();
        if let Some(&existing) = write_guard.get(s) {
            return existing;
        }
        let leaked: &'static str = Box::leak(s.to_string().into_boxed_str());
        write_guard.insert(leaked);
        leaked
    }

    /// Builds a sophia term from a Darkstar term string (`i:`/`l:`/`b:` prefix; bare = IRI).
    pub fn make_term(s: &str) -> SimpleTerm<'static> {
        if let Some(iri) = s.strip_prefix("i:") {
            SimpleTerm::Iri(IriRef::new_unchecked(Self::intern(iri).into()))
        } else if let Some(raw) = s.strip_prefix("l:") {
            if let Some(idx) = raw.rfind("^^") {
                let lex = Self::intern(&raw[..idx]);
                let dt = Self::intern(&raw[idx + 2..]);
                SimpleTerm::LiteralDatatype(lex.into(), IriRef::new_unchecked(dt.into()))
            } else if let Some(idx) = raw.rfind('@') {
                let lex = Self::intern(&raw[..idx]);
                let lang = Self::intern(&raw[idx + 1..]);
                SimpleTerm::LiteralLanguage(lex.into(), LanguageTag::new_unchecked(lang.into()))
            } else {
                let xsd_string = IriRef::new_unchecked(crate::vocab::XSD_STRING.into());
                SimpleTerm::LiteralDatatype(Self::intern(raw).into(), xsd_string)
            }
        } else if let Some(id) = s.strip_prefix("b:") {
            SimpleTerm::BlankNode(BnodeId::new_unchecked(Self::intern(id).into()))
        } else {
            SimpleTerm::Iri(IriRef::new_unchecked(Self::intern(s).into()))
        }
    }

    /// Full re-materialization: discards previous inferences and saturates from the asserted graph.
    pub fn run_inference(&self, memory: &mut EngineMemory, rules_settings: &crate::core::settings::ReasonerRules) {
        // Reset inferred graph and inconsistencies. `main` is initialized to `asserted ∪ imported`;
        // on the first iteration `current_delta` is `None`, borrowing `main_graph` directly without cloning.
        memory.inferred_graph = FastGraph::new();
        memory.main_graph = memory.asserted_graph.clone();
        memory.delta_graph = FastGraph::new();
        memory.is_consistent = true;
        memory.inconsistencies.clear();
        memory.inferred_level.clear();
        memory.max_level = 0;
        for t in memory.imported_graph.triples().flatten() {
            let _ = memory.main_graph.insert(t.s(), t.p(), t.o());
        }
        self.saturate(memory, rules_settings, None);
    }

    /// Incremental saturation for addition-only changes (semi-naive evaluation).
    /// Uses `memory.delta_graph` (triples added since the last reasoning run) as the initial delta.
    /// Only valid when the previous run reached a consistent fixpoint and nothing was removed.
    pub fn run_incremental(&self, memory: &mut EngineMemory, rules_settings: &crate::core::settings::ReasonerRules) {
        let delta = std::mem::take(&mut memory.delta_graph);
        if delta.triples().next().is_none() {
            return;
        }
        self.saturate(memory, rules_settings, Some(delta));
    }

    fn saturate(&self, memory: &mut EngineMemory, rules_settings: &crate::core::settings::ReasonerRules, initial_delta: Option<FastGraph>) {
        let start_time = std::time::Instant::now();
        println!("Darkstar Reasoner: Starting inference ({})...", if initial_delta.is_some() { "incremental" } else { "full" });
        
        let mut iteration = 1;
        // Levels continue after earlier runs so incremental derivations stay well-founded.
        let level_base = memory.max_level;
        let mut active_rules: Vec<&Box<dyn InferenceRule>> = Vec::new();
        
        // Populate active rules based on settings
        active_rules.extend(self.rules_core.iter());
        if rules_settings.equality { active_rules.extend(self.rules_equality.iter()); }
        if rules_settings.property_chains { active_rules.extend(self.rules_chains.iter()); }
        if rules_settings.class_expressions { active_rules.extend(self.rules_expressions.iter()); }
        if rules_settings.restrictions { active_rules.extend(self.rules_restrictions.iter()); }
        if rules_settings.datatypes { active_rules.extend(self.rules_datatypes.iter()); }
        if rules_settings.schema { active_rules.extend(self.rules_schema.iter()); }

        let is_full_run = initial_delta.is_none();
        let mut current_delta: Option<FastGraph> = initial_delta;

        loop {
            let iter_start = std::time::Instant::now();
            
            let results: Result<Vec<HashSet<Fact>>, super::Inconsistency> = {
                let main = &memory.main_graph;
                let delta = match &current_delta {
                    Some(d) => d,
                    None if iteration == 1 => main,
                    None => &memory.inferred_graph,
                };
                active_rules.par_iter()
                    .map(|rule| rule.apply(main, delta))
                    .collect()
            };
            
            let fact_sets = match results {
                Ok(sets) => sets,
                Err(inconsistency) => {
                    println!("!!! LOGICAL INCONSISTENCY DETECTED [Rule: {}]: {:?}", inconsistency.rule_name, inconsistency);
                    memory.is_consistent = false;
                    memory.inconsistencies.push(inconsistency);
                    return;
                }
            };
            
            let mut all_facts = HashSet::new();
            for set in fact_sets {
                if set.is_empty() { continue; }
                if all_facts.is_empty() {
                    all_facts = set;
                } else {
                    all_facts.extend(set);
                }
            }

            let target_level = level_base + iteration;
            let need_level_entry = target_level > 1;
            let need_next_delta = !(is_full_run && iteration == 1);

            let new_triples: Vec<([SimpleTerm<'static>; 3], Option<Fact>)> = {
                let main = &memory.main_graph;
                all_facts
                    .into_par_iter()
                    .filter_map(|fact| {
                        let s_term = Self::make_term(&fact.s);
                        let p_term = Self::make_term(&fact.p);
                        let o_term = Self::make_term(&fact.o);
                        if !main.contains(&s_term, &p_term, &o_term).unwrap_or(false) {
                            let kept_fact = if need_level_entry {
                                Some(crate::memory::normalize_fact_owned(fact))
                            } else {
                                None
                            };
                            Some(([s_term, p_term, o_term], kept_fact))
                        } else {
                            None
                        }
                    })
                    .collect()
            };

            let truly_new_count = new_triples.len();
            if truly_new_count > 0 {
                memory.max_level = target_level;
                let main_graph = &mut memory.main_graph;
                let inferred_graph = &mut memory.inferred_graph;
                let inferred_level = &mut memory.inferred_level;

                let mut next_delta = FastGraph::new();
                rayon::join(
                    || {
                        for ([s, p, o], _) in &new_triples {
                            let _ = main_graph.insert(s, p, o);
                        }
                    },
                    || {
                        for ([s, p, o], _) in &new_triples {
                            let _ = inferred_graph.insert(s, p, o);
                            if need_next_delta {
                                let _ = next_delta.insert(s, p, o);
                            }
                        }
                    },
                );

                if need_level_entry {
                    for (_, fact_opt) in new_triples {
                        if let Some(fact) = fact_opt {
                            inferred_level.insert(fact, target_level);
                        }
                    }
                }

                current_delta = if need_next_delta { Some(next_delta) } else { None };
            } else {
                current_delta = Some(FastGraph::new());
            }
            
            println!("Iteration {} took {:?}, produced {} new facts.", iteration, iter_start.elapsed(), truly_new_count);
            
            if truly_new_count == 0 {
                println!("Fixpoint reached in {} iterations. Total time: {:?}", iteration, start_time.elapsed());
                break;
            }
            
            iteration += 1;
            if iteration > 50 {
                println!("WARNING: Maximum iterations reached. Breaking to prevent infinite loop.");
                break;
            }
        }
        memory.delta_graph = FastGraph::new();
    }
}
