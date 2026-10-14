//! Inference explanations (the equivalent of Protégé's "Explain inference").
//!
//! The reasoner records, for every inferred triple, the iteration in which it was first
//! derived (`EngineMemory::inferred_level`). To explain a triple of level L we search, rule by
//! rule, for a set of premises that the rule would combine into that triple, accepting only
//! premises of level < L (asserted and imported triples have level 0). Every explanation
//! therefore terminates in asserted / imported axioms and can never be circular.
//!
//! The backward searches mirror the forward rules in `rules/mod.rs`; if a rule is added
//! there, add its backward counterpart here.

use std::collections::{HashMap, HashSet};
use sophia::api::prelude::*;
use sophia::api::term::matcher::Any;
use crate::memory::{normalize_fact, normalize_term, EngineMemory, Fact};
use crate::rules::engine::InferenceEngine;
use crate::rules::*;
use crate::graph_util;

/// Upper bound on nodes in one explanation (deep chains over hub data could otherwise explode).
const MAX_NODES: usize = 2_000;

#[derive(Debug, Clone, PartialEq)]
pub enum Origin {
    /// Written in the ontology file.
    Asserted,
    /// Comes from an `owl:imports`-ed ontology.
    Imported,
    /// Derived by `rule` from `premises` (indices into `Explanation::nodes`).
    Inferred { rule: &'static str, premises: Vec<usize> },
    /// Inferred, but no derivation could be reconstructed (should not happen; reported honestly).
    Unexplained,
    /// The triple is not in the ontology at all.
    NotEntailed,
    /// Node budget exhausted before this branch was expanded.
    Truncated,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub fact: Fact,
    pub origin: Origin,
}

/// A derivation DAG; `nodes[root]` is the explained triple. Shared sub-derivations appear once.
pub struct Explanation {
    pub nodes: Vec<Node>,
    pub root: usize,
}

impl Explanation {
    /// The asserted/imported axioms the derivation rests on (Protégé's "justification").
    pub fn justification(&self) -> Vec<&Node> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        let mut stack = vec![self.root];
        while let Some(i) = stack.pop() {
            if !seen.insert(i) { continue; }
            match &self.nodes[i].origin {
                Origin::Inferred { premises, .. } => stack.extend(premises.iter().rev()),
                Origin::Asserted | Origin::Imported => out.push(&self.nodes[i]),
                _ => {}
            }
        }
        out.sort_by(|a, b| (&a.fact.s, &a.fact.p, &a.fact.o).cmp(&(&b.fact.s, &b.fact.p, &b.fact.o)));
        out
    }

    /// Number of inference steps (depth of the deepest branch).
    pub fn depth(&self) -> usize {
        fn d(e: &Explanation, i: usize, memo: &mut HashMap<usize, usize>) -> usize {
            if let Some(&v) = memo.get(&i) { return v; }
            let v = match &e.nodes[i].origin {
                Origin::Inferred { premises, .. } => 1 + premises.iter().map(|&p| d(e, p, memo)).max().unwrap_or(0),
                _ => 0,
            };
            memo.insert(i, v);
            v
        }
        d(self, self.root, &mut HashMap::new())
    }
}

/// Human-readable description of each rule (shown in the explanation view).
pub fn rule_description(rule: &str, korean: bool) -> &'static str {
    match (rule, korean) {
        ("cax-sco", true) => "클래스 상속: x가 C₁의 개체이고 C₁ ⊑ C₂이면 x는 C₂의 개체",
        ("cax-sco", false) => "Subclass: x is a C₁ and C₁ ⊑ C₂, so x is a C₂",
        ("cax-eqc", true) => "동등 클래스: x가 C₁의 개체이고 C₁ ≡ C₂이면 x는 C₂의 개체",
        ("cax-eqc", false) => "Equivalent class: x is a C₁ and C₁ ≡ C₂, so x is a C₂",
        ("prp-dom", true) => "정의역(domain): x p y이고 p의 domain이 C이면 x는 C의 개체",
        ("prp-dom", false) => "Domain: x p y and domain(p) = C, so x is a C",
        ("prp-rng", true) => "치역(range): x p y이고 p의 range가 C이면 y는 C의 개체",
        ("prp-rng", false) => "Range: x p y and range(p) = C, so y is a C",
        ("prp-spo1", true) => "하위 프로퍼티: x p₁ y이고 p₁ ⊑ p₂이면 x p₂ y",
        ("prp-spo1", false) => "Subproperty: x p₁ y and p₁ ⊑ p₂, so x p₂ y",
        ("prp-inv", true) => "역관계(inverseOf): x p y이고 p ≡ q⁻이면 y q x",
        ("prp-inv", false) => "Inverse: x p y and p = inverse(q), so y q x",
        ("prp-trp", true) => "이행적 프로퍼티: x p z, z p y이고 p가 transitive이면 x p y",
        ("prp-trp", false) => "Transitive: x p z and z p y with p transitive, so x p y",
        ("prp-spo2", true) => "프로퍼티 체인: p₁∘…∘pₙ ⊑ p를 따라 이어진 경로",
        ("prp-spo2", false) => "Property chain: a path along p₁∘…∘pₙ ⊑ p",
        ("prp-key", true) => "키(hasKey): 같은 클래스의 두 개체가 키 값이 모두 같으면 동일 개체",
        ("prp-key", false) => "Key: two members of the class share all key values, so they are the same",
        ("cls-int1", true) => "교집합: x가 C₁…Cₙ 모두의 개체이면 x는 그 교집합 클래스의 개체",
        ("cls-int1", false) => "Intersection: x is in every Cᵢ, so x is in their intersection",
        ("cls-int2", true) => "교집합 분해: x가 교집합 클래스의 개체이면 각 구성 클래스의 개체",
        ("cls-int2", false) => "Intersection: x is in the intersection, so in each Cᵢ",
        ("cls-uni", true) => "합집합: x가 Cᵢ의 개체이면 Cᵢ를 포함한 합집합 클래스의 개체",
        ("cls-uni", false) => "Union: x is a Cᵢ, so x is in the union containing Cᵢ",
        ("cls-svf1", true) => "존재 제약(someValuesFrom): x p y이고 y가 C의 개체이면 x는 (p some C)의 개체",
        ("cls-svf1", false) => "someValuesFrom: x p y and y is a C, so x is in (p some C)",
        ("cls-avf", true) => "전칭 제약(allValuesFrom): x가 (p only C)의 개체이고 x p y이면 y는 C의 개체",
        ("cls-avf", false) => "allValuesFrom: x is in (p only C) and x p y, so y is a C",
        ("scm-sco", true) => "하위 클래스 이행성: C₁ ⊑ C₂, C₂ ⊑ C₃이면 C₁ ⊑ C₃",
        ("scm-sco", false) => "Subclass transitivity: C₁ ⊑ C₂ and C₂ ⊑ C₃, so C₁ ⊑ C₃",
        ("scm-spo", true) => "하위 프로퍼티 이행성: p₁ ⊑ p₂, p₂ ⊑ p₃이면 p₁ ⊑ p₃",
        ("scm-spo", false) => "Subproperty transitivity: p₁ ⊑ p₂ and p₂ ⊑ p₃, so p₁ ⊑ p₃",
        ("eq-sym", true) => "sameAs 대칭: x = y이면 y = x",
        ("eq-sym", false) => "sameAs symmetry: x = y, so y = x",
        ("eq-trans", true) => "sameAs 이행: x = y, y = z이면 x = z",
        ("eq-trans", false) => "sameAs transitivity: x = y and y = z, so x = z",
        ("eq-rep-s", true) => "동일 개체 치환(주어): x = x′이고 x p o이면 x′ p o",
        ("eq-rep-s", false) => "sameAs substitution (subject): x = x′ and x p o, so x′ p o",
        ("eq-rep-p", true) => "동일 개체 치환(술어): p = p′이고 s p o이면 s p′ o",
        ("eq-rep-p", false) => "sameAs substitution (predicate): p = p′ and s p o, so s p′ o",
        ("eq-rep-o", true) => "동일 개체 치환(목적어): o = o′이고 s p o이면 s p o′",
        ("eq-rep-o", false) => "sameAs substitution (object): o = o′ and s p o, so s p o′",
        (_, true) => "규칙",
        (_, false) => "rule",
    }
}

fn fact(s: &str, p: &str, o: &str) -> Fact {
    Fact { s: normalize_term(s), p: normalize_term(p), o: normalize_term(o) }
}

struct Explainer<'a> {
    mem: &'a EngineMemory,
    nodes: Vec<Node>,
    memo: HashMap<Fact, usize>,
}

impl<'a> Explainer<'a> {
    fn contains(&self, g: &crate::memory::DarkstarGraph, f: &Fact) -> bool {
        graph_util::has(g, &f.s, &f.p, &f.o)
    }

    /// 0 for asserted / imported, the derivation iteration for inferred, MAX if absent.
    fn level(&self, f: &Fact) -> u32 {
        if self.contains(&self.mem.asserted_graph, f) || self.contains(&self.mem.imported_graph, f) { return 0; }
        self.mem.inferred_level.get(f).copied().unwrap_or(u32::MAX)
    }

    // --- small graph helpers over the full (materialized) graph ---
    fn objects(&self, s: &str, p: &str) -> Vec<String> { graph_util::objects(&self.mem.main_graph, s, p) }
    fn subjects(&self, p: &str, o: &str) -> Vec<String> { graph_util::subjects(&self.mem.main_graph, p, o) }
    fn list(&self, head: &str) -> Vec<String> {
        get_rdf_list(&self.mem.main_graph, &InferenceEngine::make_term(head))
    }

    /// First premise set (all strictly below `lvl`) from which some rule derives `t`.
    fn find(&self, t: &Fact, lvl: u32) -> Option<(&'static str, Vec<Fact>)> {
        let below = |fs: &[Fact]| fs.iter().all(|f| self.level(f) < lvl);
        let ty = normalize_term(RDF_TYPE);
        let (s, p, o) = (t.s.as_str(), t.p.as_str(), t.o.as_str());

        if p == ty {
            // cax-sco
            for c1 in self.subjects(RDFS_SUBCLASS_OF, o) {
                let ps = vec![fact(s, RDF_TYPE, &c1), fact(&c1, RDFS_SUBCLASS_OF, o)];
                if c1 != o && below(&ps) { return Some(("cax-sco", ps)); }
            }
            // cax-eqc
            for c1 in self.subjects(OWL_EQUIVALENT_CLASS, o) {
                let ps = vec![fact(s, RDF_TYPE, &c1), fact(&c1, OWL_EQUIVALENT_CLASS, o)];
                if below(&ps) { return Some(("cax-eqc", ps)); }
            }
            for c1 in self.objects(o, OWL_EQUIVALENT_CLASS) {
                let ps = vec![fact(s, RDF_TYPE, &c1), fact(o, OWL_EQUIVALENT_CLASS, &c1)];
                if below(&ps) { return Some(("cax-eqc", ps)); }
            }
            // prp-dom
            for q in self.subjects(RDFS_DOMAIN, o) {
                for y in self.objects(s, &q) {
                    let ps = vec![fact(s, &q, &y), fact(&q, RDFS_DOMAIN, o)];
                    if below(&ps) { return Some(("prp-dom", ps)); }
                }
            }
            // prp-rng
            for q in self.subjects(RDFS_RANGE, o) {
                for x in self.subjects(&q, s) {
                    let ps = vec![fact(&x, &q, s), fact(&q, RDFS_RANGE, o)];
                    if below(&ps) { return Some(("prp-rng", ps)); }
                }
            }
            // cls-uni
            for l in self.objects(o, OWL_UNION_OF) {
                for m in self.list(&l) {
                    let ps = vec![fact(s, RDF_TYPE, &m), fact(o, OWL_UNION_OF, &l)];
                    if below(&ps) { return Some(("cls-uni", ps)); }
                }
            }
            // cls-int1
            for l in self.objects(o, OWL_INTERSECTION_OF) {
                let members = self.list(&l);
                if members.is_empty() { continue; }
                let mut ps: Vec<Fact> = members.iter().map(|m| fact(s, RDF_TYPE, m)).collect();
                ps.push(fact(o, OWL_INTERSECTION_OF, &l));
                if below(&ps) { return Some(("cls-int1", ps)); }
            }
            // cls-int2
            for t_int in self.mem.main_graph.triples_matching(Any, Some(&InferenceEngine::make_term(OWL_INTERSECTION_OF)), Any).flatten() {
                let (c, l) = (extract_str(&t_int.s()), extract_str(&t_int.o()));
                if self.list(&l).iter().any(|m| m == o) {
                    let ps = vec![fact(s, RDF_TYPE, &c), fact(&c, OWL_INTERSECTION_OF, &l)];
                    if below(&ps) { return Some(("cls-int2", ps)); }
                }
            }
            // cls-svf1: o is a restriction (onProperty q, someValuesFrom y)
            for y in self.objects(o, OWL_SOME_VALUES_FROM) {
                for q in self.objects(o, OWL_ON_PROPERTY) {
                    for v in self.objects(s, &q) {
                        let ps = vec![fact(s, &q, &v), fact(&v, RDF_TYPE, &y), fact(o, OWL_SOME_VALUES_FROM, &y), fact(o, OWL_ON_PROPERTY, &q)];
                        if below(&ps) { return Some(("cls-svf1", ps)); }
                    }
                }
            }
            // cls-avf: restriction r (onProperty q, allValuesFrom o), u type r, u q s
            for r in self.subjects(OWL_ALL_VALUES_FROM, o) {
                for q in self.objects(&r, OWL_ON_PROPERTY) {
                    for u in self.subjects(&q, s) {
                        let ps = vec![fact(&u, RDF_TYPE, &r), fact(&u, &q, s), fact(&r, OWL_ALL_VALUES_FROM, o), fact(&r, OWL_ON_PROPERTY, &q)];
                        if below(&ps) { return Some(("cls-avf", ps)); }
                    }
                }
            }
        }

        if p == normalize_term(RDFS_SUBCLASS_OF) {
            for z in self.objects(s, RDFS_SUBCLASS_OF) {
                if z == s || z == o { continue; }
                let ps = vec![fact(s, RDFS_SUBCLASS_OF, &z), fact(&z, RDFS_SUBCLASS_OF, o)];
                if below(&ps) { return Some(("scm-sco", ps)); }
            }
        }
        if p == normalize_term(RDFS_SUBPROPERTY_OF) {
            for z in self.objects(s, RDFS_SUBPROPERTY_OF) {
                if z == s || z == o { continue; }
                let ps = vec![fact(s, RDFS_SUBPROPERTY_OF, &z), fact(&z, RDFS_SUBPROPERTY_OF, o)];
                if below(&ps) { return Some(("scm-spo", ps)); }
            }
        }

        if p == normalize_term(OWL_SAME_AS) {
            let ps = vec![fact(o, OWL_SAME_AS, s)];
            if below(&ps) { return Some(("eq-sym", ps)); }
            for z in self.objects(s, OWL_SAME_AS) {
                if z == o { continue; }
                let ps = vec![fact(s, OWL_SAME_AS, &z), fact(&z, OWL_SAME_AS, o)];
                if below(&ps) { return Some(("eq-trans", ps)); }
            }
            // prp-key
            for tk in self.mem.main_graph.triples_matching(Any, Some(&InferenceEngine::make_term(OWL_HAS_KEY)), Any).flatten() {
                let (c, l) = (extract_str(&tk.s()), extract_str(&tk.o()));
                let mut ps = vec![fact(&c, OWL_HAS_KEY, &l), fact(s, RDF_TYPE, &c), fact(o, RDF_TYPE, &c)];
                let mut ok = true;
                for k in self.list(&l) {
                    let sv: HashSet<String> = self.objects(s, &k).into_iter().collect();
                    let ov: HashSet<String> = self.objects(o, &k).into_iter().collect();
                    if sv.is_empty() || sv != ov { ok = false; break; }
                    for v in &sv { ps.push(fact(s, &k, v)); ps.push(fact(o, &k, v)); }
                }
                if ok && below(&ps) { return Some(("prp-key", ps)); }
            }
        }

        // prp-spo1: q ⊑ p and s q o
        for q in self.subjects(RDFS_SUBPROPERTY_OF, p) {
            if q == p { continue; }
            let ps = vec![fact(s, &q, o), fact(&q, RDFS_SUBPROPERTY_OF, p)];
            if below(&ps) { return Some(("prp-spo1", ps)); }
        }
        // prp-inv: p inverseOf q / q inverseOf p, and o q s
        for q in self.objects(p, OWL_INVERSE_OF) {
            let ps = vec![fact(o, &q, s), fact(p, OWL_INVERSE_OF, &q)];
            if below(&ps) { return Some(("prp-inv", ps)); }
        }
        for q in self.subjects(OWL_INVERSE_OF, p) {
            let ps = vec![fact(o, &q, s), fact(&q, OWL_INVERSE_OF, p)];
            if below(&ps) { return Some(("prp-inv", ps)); }
        }
        // prp-trp
        let trans = fact(p, RDF_TYPE, OWL_TRANSITIVE_PROPERTY);
        if self.level(&trans) < lvl {
            for z in self.objects(s, p) {
                if z == o { continue; }
                let ps = vec![fact(s, p, &z), fact(&z, p, o), trans.clone()];
                if below(&ps) { return Some(("prp-trp", ps)); }
            }
        }
        // prp-spo2: property chain
        for l in self.objects(p, OWL_PROPERTY_CHAIN_AXIOM) {
            let chain = self.list(&l);
            if chain.is_empty() { continue; }
            if let Some(path) = self.chain_path(s, o, &chain, lvl) {
                let mut ps = path;
                ps.push(fact(p, OWL_PROPERTY_CHAIN_AXIOM, &l));
                if below(&ps) { return Some(("prp-spo2", ps)); }
            }
        }
        // eq-rep-s / eq-rep-p / eq-rep-o
        for s0 in self.subjects(OWL_SAME_AS, s) {
            if s0 == s { continue; }
            let ps = vec![fact(&s0, p, o), fact(&s0, OWL_SAME_AS, s)];
            if below(&ps) { return Some(("eq-rep-s", ps)); }
        }
        for p0 in self.subjects(OWL_SAME_AS, p) {
            if p0 == p { continue; }
            let ps = vec![fact(s, &p0, o), fact(&p0, OWL_SAME_AS, p)];
            if below(&ps) { return Some(("eq-rep-p", ps)); }
        }
        for o0 in self.subjects(OWL_SAME_AS, o) {
            if o0 == o { continue; }
            let ps = vec![fact(s, p, &o0), fact(&o0, OWL_SAME_AS, o)];
            if below(&ps) { return Some(("eq-rep-o", ps)); }
        }
        None
    }

    /// A path s -p1-> … -pn-> o whose edges are all below `lvl` (BFS, bounded).
    fn chain_path(&self, s: &str, o: &str, chain: &[String], lvl: u32) -> Option<Vec<Fact>> {
        let mut frontier: Vec<(String, Vec<Fact>)> = vec![(s.to_string(), Vec::new())];
        for (i, pi) in chain.iter().enumerate() {
            let mut next = Vec::new();
            for (node, path) in &frontier {
                for y in self.objects(node, pi) {
                    let edge = fact(node, pi, &y);
                    if self.level(&edge) >= lvl { continue; }
                    if i == chain.len() - 1 && y != o { continue; }
                    let mut p2 = path.clone();
                    p2.push(edge);
                    next.push((y, p2));
                    if next.len() > 5_000 { break; }
                }
            }
            frontier = next;
            if frontier.is_empty() { return None; }
        }
        frontier.into_iter().find(|(n, _)| n == o).map(|(_, p)| p)
    }

    fn explain(&mut self, f: Fact) -> usize {
        if let Some(&i) = self.memo.get(&f) { return i; }
        if self.nodes.len() >= MAX_NODES {
            self.nodes.push(Node { fact: f, origin: Origin::Truncated });
            return self.nodes.len() - 1;
        }
        let level = self.mem.inferred_level.get(&f).copied();
        let origin = if self.contains(&self.mem.asserted_graph, &f) {
            Origin::Asserted
        } else if self.contains(&self.mem.imported_graph, &f) {
            Origin::Imported
        } else if level.is_some() {
            Origin::Unexplained // replaced below once a derivation is found
        } else {
            Origin::NotEntailed
        };
        let idx = self.nodes.len();
        self.nodes.push(Node { fact: f.clone(), origin: origin.clone() });
        // Memoize before recursing: shared sub-derivations are explained once.
        self.memo.insert(f.clone(), idx);

        if let (Origin::Unexplained, Some(lvl)) = (origin, level) {
            if let Some((rule, premises)) = self.find(&f, lvl) {
                let ids = premises.into_iter().map(|p| self.explain(p)).collect();
                self.nodes[idx].origin = Origin::Inferred { rule, premises: ids };
            }
        }
        idx
    }
}

/// Explains why `(s, p, o)` holds. Terms use Darkstar's string form (`i:…`, `l:…`, `b:…`).
pub fn explain(mem: &EngineMemory, s: &str, p: &str, o: &str) -> Explanation {
    let mut ex = Explainer { mem, nodes: Vec::new(), memo: HashMap::new() };
    let root = ex.explain(normalize_fact(&Fact { s: s.into(), p: p.into(), o: o.into() }));
    Explanation { nodes: ex.nodes, root }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::manager::DarkstarManager;
    use crate::core::io::OntologyFormat;
    use crate::core::settings::ReasonerRules;

    fn load(ttl: &str) -> DarkstarManager {
        let mut m = DarkstarManager::new();
        m.replace_from_string(ttl, OntologyFormat::Turtle, &ReasonerRules::default()).unwrap();
        m
    }

    const EX: &str = "http://example.org/";
    fn i(x: &str) -> String { format!("i:{}{}", EX, x) }

    #[test]
    fn subclass_chain_and_domain() {
        let m = load(r#"
            @prefix ex: <http://example.org/> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            ex:Activity rdfs:subClassOf ex:Event . ex:Event rdfs:subClassOf ex:Temporal .
            ex:carriedOutBy rdfs:domain ex:Activity .
            ex:a1 ex:carriedOutBy ex:kim .
        "#);
        let ty = format!("i:{}", RDF_TYPE);
        // a1 : Temporal  ⇐ a1 : Event ⇐ a1 : Activity ⇐ domain(carriedOutBy)
        let e = explain(&m.memory, &i("a1"), &ty, &i("Temporal"));
        let Origin::Inferred { rule, .. } = &e.nodes[e.root].origin else { panic!("{:?}", e.nodes[e.root]) };
        assert!(*rule == "cax-sco");
        let just: Vec<String> = e.justification().iter().map(|n| format!("{} {} {}", n.fact.s, n.fact.p, n.fact.o)).collect();
        assert!(just.iter().all(|j| !j.contains("Temporal") || j.contains("subClassOf")), "{:?}", just);
        assert!(e.nodes.iter().all(|n| !matches!(n.origin, Origin::Unexplained | Origin::NotEntailed)));
        assert!(e.justification().iter().all(|n| n.origin == Origin::Asserted));
        // depth: domain (1) -> sco (2) -> sco or scm-sco (3)
        assert!(e.depth() >= 2);
        // asserted triple explains as asserted
        let e = explain(&m.memory, &i("a1"), &i("carriedOutBy"), &i("kim"));
        assert_eq!(e.nodes[e.root].origin, Origin::Asserted);
        // non-entailed
        let e = explain(&m.memory, &i("kim"), &ty, &i("Event"));
        assert_eq!(e.nodes[e.root].origin, Origin::NotEntailed);
    }

    #[test]
    fn inverse_transitive_sameas() {
        let m = load(r#"
            @prefix ex: <http://example.org/> .
            @prefix owl: <http://www.w3.org/2002/07/owl#> .
            ex:partOf a owl:TransitiveProperty . ex:hasPart owl:inverseOf ex:partOf .
            ex:a ex:partOf ex:b . ex:b ex:partOf ex:c .
            ex:x owl:sameAs ex:y . ex:x ex:name "X" .
        "#);
        let e = explain(&m.memory, &i("a"), &i("partOf"), &i("c"));
        assert!(matches!(&e.nodes[e.root].origin, Origin::Inferred { rule: "prp-trp", .. }));
        let e = explain(&m.memory, &i("c"), &i("hasPart"), &i("a"));
        assert!(matches!(&e.nodes[e.root].origin, Origin::Inferred { rule: "prp-inv", .. }));
        assert_eq!(e.depth(), 2); // inverse of the transitive inference
        let e = explain(&m.memory, &i("y"), &i("name"), "l:X");
        assert!(matches!(&e.nodes[e.root].origin, Origin::Inferred { rule: "eq-rep-s", .. }), "{:?}", e.nodes[e.root]);
        assert!(e.nodes.iter().all(|n| !matches!(n.origin, Origin::Unexplained)));
    }

    #[test]
    fn every_inference_is_explainable() {
        // Exhaustive check on a mixed ontology: every inferred triple gets a full derivation.
        let m = load(r#"
            @prefix ex: <http://example.org/> .
            @prefix owl: <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            ex:A rdfs:subClassOf ex:B . ex:B rdfs:subClassOf ex:C . ex:C owl:equivalentClass ex:D .
            ex:p rdfs:subPropertyOf ex:q . ex:q rdfs:range ex:C . ex:q owl:inverseOf ex:qi .
            ex:R owl:onProperty ex:p ; owl:someValuesFrom ex:A .
            ex:U owl:unionOf ( ex:A ex:Z ) .
            ex:I owl:intersectionOf ( ex:B ex:D ) .
            ex:gp owl:propertyChainAxiom ( ex:p ex:p ) .
            ex:x a ex:A ; ex:p ex:y . ex:y a ex:A ; ex:p ex:z .
            ex:x owl:sameAs ex:x2 .
        "#);
        let mut checked = 0;
        for t in m.memory.inferred_graph.triples().flatten() {
            let (s, p, o) = (extract_str(&t.s()), extract_str(&t.p()), extract_str(&t.o()));
            let e = explain(&m.memory, &s, &p, &o);
            assert!(matches!(e.nodes[e.root].origin, Origin::Inferred { .. }), "unexplained: {} {} {}", s, p, o);
            assert!(e.nodes.iter().all(|n| !matches!(n.origin, Origin::Unexplained | Origin::NotEntailed)), "{} {} {}: {:?}", s, p, o, e.nodes);
            checked += 1;
        }
        assert!(checked > 20, "only {} inferences", checked);
    }

    #[test]
    fn inconsistency_is_explained_from_both_sides() {
        let m = load(r#"
            @prefix ex: <http://example.org/> .
            @prefix owl: <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            ex:Activist rdfs:subClassOf ex:Person .
            ex:Person owl:disjointWith ex:Organization .
            ex:memberOf rdfs:range ex:Organization .
            ex:kim a ex:Activist .
            ex:lee ex:memberOf ex:kim .
        "#);
        assert!(!m.memory.is_consistent);
        let inc = &m.memory.inconsistencies[0];
        assert_eq!(inc.rule_name, "cls-dj");
        assert_eq!(inc.conflicting_triples.len(), 3);
        let mut all_axioms = HashSet::new();
        for f in &inc.conflicting_triples {
            let n = normalize_fact(f);
            let e = explain(&m.memory, &n.s, &n.p, &n.o);
            assert!(!matches!(e.nodes[e.root].origin, Origin::Unexplained | Origin::NotEntailed), "{:?}", n);
            for j in e.justification() { all_axioms.insert(format!("{} {}", j.fact.p, j.fact.o)); }
        }
        // the 5 asserted axioms involved: subClassOf, disjointWith, range, kim a Activist, lee memberOf kim
        assert_eq!(all_axioms.len(), 5, "{:?}", all_axioms);
    }
}
