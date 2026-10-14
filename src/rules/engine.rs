use crate::memory::EngineMemory;
use super::{InferenceRule, Fact};
use rayon::prelude::*;
use std::collections::HashSet;
use sophia::api::prelude::*;
use sophia::inmem::graph::FastGraph;
use sophia::api::term::{SimpleTerm, IriRef, BnodeId, LanguageTag};
use std::sync::Mutex;
use once_cell::sync::Lazy;

/// A global string interner to prevent massive memory leaks from Box::leak
static STR_INTERNER: Lazy<Mutex<HashSet<&'static str>>> = Lazy::new(|| Mutex::new(HashSet::new()));

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

    /// Optimized string interning to avoid memory explosion
    pub fn intern(s: &str) -> &'static str {
        let mut cache = STR_INTERNER.lock().unwrap();
        if let Some(existing) = cache.get(s) {
            return existing;
        }
        let leaked: &'static str = Box::leak(s.to_string().into_boxed_str());
        cache.insert(leaked);
        leaked
    }

    pub fn make_term(s: &str) -> SimpleTerm<'static> {
        if s.starts_with("i:") {
            let val = Self::intern(&s[2..]);
            SimpleTerm::Iri(IriRef::new_unchecked(val.into()))
        } else if s.starts_with("l:") {
            let raw = &s[2..];
            if let Some(idx) = raw.rfind("^^") {
                let lex = Self::intern(&raw[..idx]);
                let dt = Self::intern(&raw[idx + 2..]);
                SimpleTerm::LiteralDatatype(lex.into(), IriRef::new_unchecked(dt.into()))
            } else if let Some(idx) = raw.rfind('@') {
                let lex = Self::intern(&raw[..idx]);
                let lang = Self::intern(&raw[idx + 1..]);
                SimpleTerm::LiteralLanguage(lex.into(), LanguageTag::new_unchecked(lang.into()))
            } else {
                let val = Self::intern(raw);
                let xsd_string = IriRef::new_unchecked("http://www.w3.org/2001/XMLSchema#string".into());
                SimpleTerm::LiteralDatatype(val.into(), xsd_string)
            }
        } else if s.starts_with("b:") {
            let val = Self::intern(&s[2..]);
            SimpleTerm::BlankNode(BnodeId::new_unchecked(val.into()))
        } else {
            let val = Self::intern(s);
            SimpleTerm::Iri(IriRef::new_unchecked(val.into()))
        }
    }

    /// Full re-materialization: discards previous inferences and saturates from the asserted graph.
    pub fn run_inference(&self, memory: &mut EngineMemory, rules_settings: &crate::core::settings::ReasonerRules) {
        // Reset inferred graph and inconsistencies. `main` needs its own copy because it is
        // mutated during saturation; the first delta is the asserted graph itself (borrowed,
        // not cloned), which avoids copying hundreds of thousands of triples.
        memory.inferred_graph = FastGraph::new();
        memory.main_graph = memory.asserted_graph.clone();
        memory.delta_graph = FastGraph::new();
        memory.is_consistent = true;
        memory.inconsistencies.clear();
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
        let mut active_rules: Vec<&Box<dyn InferenceRule>> = Vec::new();
        
        // Populate active rules based on settings
        active_rules.extend(self.rules_core.iter());
        if rules_settings.equality { active_rules.extend(self.rules_equality.iter()); }
        if rules_settings.property_chains { active_rules.extend(self.rules_chains.iter()); }
        if rules_settings.class_expressions { active_rules.extend(self.rules_expressions.iter()); }
        if rules_settings.restrictions { active_rules.extend(self.rules_restrictions.iter()); }
        if rules_settings.datatypes { active_rules.extend(self.rules_datatypes.iter()); }
        if rules_settings.schema { active_rules.extend(self.rules_schema.iter()); }

        // `None` => the first delta is the asserted graph (full run).
        let mut current_delta: Option<FastGraph> = initial_delta;

        loop {
            let iter_start = std::time::Instant::now();
            
            let results: Result<Vec<HashSet<Fact>>, super::Inconsistency> = {
                let main = &memory.main_graph;
                let delta = match &current_delta {
                    Some(d) => d,
                    None => &memory.asserted_graph,
                };
                active_rules.par_iter()
                    .map(|rule| rule.apply(main, delta))
                    .collect()
            };
            
            let new_facts: HashSet<Fact> = match results {
                Ok(fact_sets) => {
                    let mut all_facts = HashSet::new();
                    for set in fact_sets { all_facts.extend(set); }
                    all_facts
                },
                Err(inconsistency) => {
                    println!("!!! LOGICAL INCONSISTENCY DETECTED [Rule: {}]: {:?}", inconsistency.rule_name, inconsistency);
                    memory.is_consistent = false;
                    memory.inconsistencies.push(inconsistency);
                    return;
                }
            };
            
            let mut next_delta = FastGraph::new();
            let mut truly_new_count = 0;

            for fact in new_facts {
                let s_term = Self::make_term(&fact.s);
                let p_term = Self::make_term(&fact.p);
                let o_term = Self::make_term(&fact.o);

                if memory.main_graph.insert(&s_term, &p_term, &o_term).unwrap_or(false) {
                    next_delta.insert(&s_term, &p_term, &o_term).unwrap();
                    memory.inferred_graph.insert(&s_term, &p_term, &o_term).unwrap();
                    truly_new_count += 1;
                }
            }
            
            current_delta = Some(next_delta);
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
