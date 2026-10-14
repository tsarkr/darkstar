//! OWL editing helpers behind the Protégé-style entity description panel:
//! - Manchester class expressions → standard OWL/RDF structures (blank-node restrictions),
//! - RDF class-expression structures → Manchester text for display,
//! - collecting a blank-node structure (for removing an axiom cleanly),
//! - IRI renaming (refactoring) as one change set,
//! - entity kind detection.
//!
//! All terms use Darkstar's string form (`i:…`, `l:…`, `b:…`).

use std::collections::HashSet;
use sophia::api::prelude::*;
use sophia::api::term::matcher::Any;
use sophia::inmem::graph::FastGraph;
use crate::query::dlquery::{Ce, Facet, Prop, DlIndex};
use crate::rules::extract_str;

pub use crate::vocab::{owl, rdf, rdfs, OWL, RDF, RDFS, XSD};
pub use crate::graph_util::{has, objects, subjects};
use crate::graph_util::term;

/// A triple as three Darkstar term strings.
pub type Spo = (String, String, String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityKind { Class, ObjectProperty, DataProperty, AnnotationProperty, Individual, Datatype, Unknown }

/// Property characteristics shown as checkboxes (OWL names, without namespace).
pub const OBJECT_CHARACTERISTICS: &[&str] = &[
    "FunctionalProperty", "InverseFunctionalProperty", "TransitiveProperty", "SymmetricProperty",
    "AsymmetricProperty", "ReflexiveProperty", "IrreflexiveProperty",
];

/// Annotation properties offered by default in the annotation editor.
pub const COMMON_ANNOTATIONS: &[(&str, &str)] = &[
    ("rdfs:label", "http://www.w3.org/2000/01/rdf-schema#label"),
    ("rdfs:comment", "http://www.w3.org/2000/01/rdf-schema#comment"),
    ("rdfs:seeAlso", "http://www.w3.org/2000/01/rdf-schema#seeAlso"),
    ("rdfs:isDefinedBy", "http://www.w3.org/2000/01/rdf-schema#isDefinedBy"),
    ("owl:versionInfo", "http://www.w3.org/2002/07/owl#versionInfo"),
    ("skos:prefLabel", "http://www.w3.org/2004/02/skos/core#prefLabel"),
    ("skos:altLabel", "http://www.w3.org/2004/02/skos/core#altLabel"),
    ("skos:definition", "http://www.w3.org/2004/02/skos/core#definition"),
    ("dcterms:source", "http://purl.org/dc/terms/source"),
    ("dcterms:date", "http://purl.org/dc/terms/date"),
];

/// What kind of entity `uri` is, judged from declarations first, then from usage.
pub fn entity_kind(g: &FastGraph, uri: &str) -> EntityKind {
    let types: HashSet<String> = objects(g, uri, &rdf("type")).into_iter().collect();
    let t = |l: &str| types.contains(&owl(l));
    if t("Class") || types.contains(&rdfs("Class")) || t("Restriction") { return EntityKind::Class; }
    if t("ObjectProperty") || OBJECT_CHARACTERISTICS.iter().any(|c| t(c)) && !t("DatatypeProperty") { return EntityKind::ObjectProperty; }
    if t("DatatypeProperty") { return EntityKind::DataProperty; }
    if t("AnnotationProperty") { return EntityKind::AnnotationProperty; }
    if types.contains(&rdfs("Datatype")) { return EntityKind::Datatype; }
    if types.contains(&rdf("Property")) { return EntityKind::ObjectProperty; }
    // usage-based
    let sco = term(&rdfs("subClassOf"));
    let ty = term(&rdf("type"));
    let u = term(uri);
    if g.triples_matching(Some(&u), Some(&sco), Any).next().is_some()
        || g.triples_matching(Any, Some(&sco), Some(&u)).next().is_some()
        || g.triples_matching(Any, Some(&ty), Some(&u)).next().is_some()
    {
        return EntityKind::Class;
    }
    if g.triples_matching(Any, Some(&u), Any).next().is_some() { return EntityKind::ObjectProperty; }
    if !types.is_empty() { return EntityKind::Individual; }
    EntityKind::Unknown
}

// ---------------------------------------------------------------------------
// Manchester AST → RDF
// ---------------------------------------------------------------------------

/// Generates fresh blank node ids.
pub struct BnodeGen { prefix: String, n: usize }
impl Default for BnodeGen {
    fn default() -> Self {
        Self::new()
    }
}

impl BnodeGen {
    pub fn new() -> Self {
        Self { prefix: format!("ds{}", uuid::Uuid::new_v4().simple().to_string().chars().take(8).collect::<String>()), n: 0 }
    }
    pub fn fresh(&mut self) -> String { self.n += 1; format!("b:{}_{}", self.prefix, self.n) }
}

fn rdf_list(items: Vec<String>, gen: &mut BnodeGen, out: &mut Vec<Spo>) -> String {
    let mut head = rdf("nil");
    for item in items.into_iter().rev() {
        let node = gen.fresh();
        out.push((node.clone(), rdf("first"), item));
        out.push((node.clone(), rdf("rest"), head));
        head = node;
    }
    head
}

fn prop_term(p: &Prop, gen: &mut BnodeGen, out: &mut Vec<Spo>) -> String {
    let iri = format!("i:{}", p.iri);
    if !p.inverse { return iri; }
    let b = gen.fresh();
    out.push((b.clone(), owl("inverseOf"), iri));
    b
}

fn facet_literal(v: f64) -> String {
    if v.fract() == 0.0 { format!("l:{}^^{}integer", v as i64, XSD) } else { format!("l:{}^^{}decimal", v, XSD) }
}

/// Converts a class expression into RDF. Returns the term standing for it (an IRI for a
/// named class, otherwise a fresh blank node) and the triples to add.
pub fn ce_to_rdf(ce: &Ce, gen: &mut BnodeGen, out: &mut Vec<Spo>) -> String {
    let restriction = |gen: &mut BnodeGen, out: &mut Vec<Spo>, p: &Prop| -> String {
        let r = gen.fresh();
        out.push((r.clone(), rdf("type"), owl("Restriction")));
        let pt = prop_term(p, gen, out);
        out.push((r.clone(), owl("onProperty"), pt));
        r
    };
    match ce {
        Ce::Named(iri) => format!("i:{}", iri),
        Ce::And(items) | Ce::Or(items) => {
            let members: Vec<String> = items.iter().map(|i| ce_to_rdf(i, gen, out)).collect();
            let b = gen.fresh();
            out.push((b.clone(), rdf("type"), owl("Class")));
            let list = rdf_list(members, gen, out);
            let p = if matches!(ce, Ce::And(_)) { "intersectionOf" } else { "unionOf" };
            out.push((b.clone(), owl(p), list));
            b
        }
        Ce::Not(inner) => {
            let c = ce_to_rdf(inner, gen, out);
            let b = gen.fresh();
            out.push((b.clone(), rdf("type"), owl("Class")));
            out.push((b.clone(), owl("complementOf"), c));
            b
        }
        Ce::OneOf(items) => {
            let b = gen.fresh();
            out.push((b.clone(), rdf("type"), owl("Class")));
            let list = rdf_list(items.iter().map(|i| format!("i:{}", i)).collect(), gen, out);
            out.push((b.clone(), owl("oneOf"), list));
            b
        }
        Ce::Datatype(dt, facets) => {
            if facets.is_empty() { return format!("i:{}", dt); }
            let b = gen.fresh();
            out.push((b.clone(), rdf("type"), rdfs("Datatype")));
            out.push((b.clone(), owl("onDatatype"), format!("i:{}", dt)));
            let fs: Vec<String> = facets.iter().map(|f| {
                let (name, v) = match *f { Facet::Ge(v) => ("minInclusive", v), Facet::Gt(v) => ("minExclusive", v), Facet::Le(v) => ("maxInclusive", v), Facet::Lt(v) => ("maxExclusive", v) };
                let fb = gen.fresh();
                out.push((fb.clone(), format!("i:{}{}", XSD, name), facet_literal(v)));
                fb
            }).collect();
            let list = rdf_list(fs, gen, out);
            out.push((b.clone(), owl("withRestrictions"), list));
            b
        }
        Ce::Some(p, f) => {
            let r = restriction(gen, out, p);
            let filler = match f { Some(f) => ce_to_rdf(f, gen, out), None => owl("Thing") };
            out.push((r.clone(), owl("someValuesFrom"), filler));
            r
        }
        Ce::Only(p, f) => {
            let r = restriction(gen, out, p);
            let filler = ce_to_rdf(f, gen, out);
            out.push((r.clone(), owl("allValuesFrom"), filler));
            r
        }
        Ce::Value(p, v) => {
            let r = restriction(gen, out, p);
            out.push((r.clone(), owl("hasValue"), v.clone()));
            r
        }
        Ce::SelfRef(p) => {
            let r = restriction(gen, out, p);
            out.push((r.clone(), owl("hasSelf"), format!("l:true^^{}boolean", XSD)));
            r
        }
        Ce::Min(n, p, f) | Ce::Max(n, p, f) | Ce::Exactly(n, p, f) => {
            let r = restriction(gen, out, p);
            let base = match ce { Ce::Min(..) => "min", Ce::Max(..) => "max", _ => "" };
            let nlit = format!("l:{}^^{}nonNegativeInteger", n, XSD);
            match f {
                None => {
                    let pn = if base.is_empty() { "cardinality".to_string() } else { format!("{}Cardinality", base) };
                    out.push((r.clone(), owl(&pn), nlit));
                }
                Some(f) => {
                    let pn = if base.is_empty() { "qualifiedCardinality".to_string() } else { format!("{}QualifiedCardinality", base) };
                    out.push((r.clone(), owl(&pn), nlit));
                    let is_data = matches!(f.as_ref(), Ce::Datatype(..));
                    let filler = ce_to_rdf(f, gen, out);
                    out.push((r.clone(), owl(if is_data { "onDataRange" } else { "onClass" }), filler));
                }
            }
            r
        }
    }
}

/// Parses a Manchester expression and returns (term, triples to add).
pub fn manchester_to_rdf(src: &str, idx: &DlIndex) -> Result<(String, Vec<Spo>), String> {
    let ce = crate::query::dlquery::parse(src, idx)?;
    let mut gen = BnodeGen::new();
    let mut out = Vec::new();
    let t = ce_to_rdf(&ce, &mut gen, &mut out);
    Ok((t, out))
}

// ---------------------------------------------------------------------------
// RDF → Manchester (display)
// ---------------------------------------------------------------------------

/// Short, re-parseable name for an IRI: local name, or `'label'` when the local name is
/// unreadable (e.g. percent-encoded), or `<IRI>` as a last resort.
pub fn short_name(iri_term: &str, idx: Option<&DlIndex>) -> String {
    let Some(iri) = iri_term.strip_prefix("i:") else { return literal_text(iri_term) };
    for (pfx, ns) in [("owl", OWL), ("rdfs", RDFS), ("xsd", XSD), ("rdf", RDF)] {
        if let Some(l) = iri.strip_prefix(ns) { return if pfx == "owl" && (l == "Thing" || l == "Nothing") { l.to_string() } else { format!("{}:{}", pfx, l) }; }
    }
    let local = iri.trim_end_matches(['/', '#']).rsplit(['#', '/']).next().unwrap_or(iri);
    let readable = !local.is_empty() && !local.contains(['%', '?', '=', '&', ' ']);
    if readable { return local.to_string(); }
    if let Some(l) = idx.and_then(|i| i.label_of(iri_term)) { return format!("'{}'", l); }
    format!("<{}>", iri)
}

fn literal_text(t: &str) -> String {
    let Some(raw) = t.strip_prefix("l:") else { return t.to_string() };
    if let Some(i) = raw.rfind("^^") {
        let (lex, dt) = (&raw[..i], &raw[i + 2..]);
        let numeric = ["integer", "decimal", "double", "float", "int", "long", "nonNegativeInteger"].iter().any(|n| dt == format!("{}{}", XSD, n));
        if numeric { return lex.to_string(); }
        return format!("\"{}\"^^{}", lex, short_name(&format!("i:{}", dt), None));
    }
    if let Some(i) = raw.rfind('@') { return format!("\"{}\"@{}", &raw[..i], &raw[i + 1..]); }
    format!("\"{}\"", raw)
}

fn list_items(g: &FastGraph, head: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = head.to_string();
    let mut seen = HashSet::new();
    while cur != rdf("nil") && seen.insert(cur.clone()) && out.len() < 1000 {
        match objects(g, &cur, &rdf("first")).into_iter().next() { Some(f) => out.push(f), None => break }
        match objects(g, &cur, &rdf("rest")).into_iter().next() { Some(r) => cur = r, None => break }
    }
    out
}

/// Renders a class expression term (IRI or blank-node structure) in Manchester syntax.
pub fn render(g: &FastGraph, t: &str, idx: Option<&DlIndex>) -> String {
    render_depth(g, t, idx, 0)
}

fn render_depth(g: &FastGraph, t: &str, idx: Option<&DlIndex>, depth: usize) -> String {
    if !t.starts_with("b:") || depth > 12 { return short_name(t, idx); }
    let one = |p: &str| objects(g, t, &owl(p)).into_iter().next();
    // Restrictions and `not` bind tighter than `and`/`or`, so only nested and/or need parentheses.
    let sub = |x: &str| {
        let s = render_depth(g, x, idx, depth + 1);
        let compound = x.starts_with("b:")
            && (!objects(g, x, &owl("intersectionOf")).is_empty() || !objects(g, x, &owl("unionOf")).is_empty());
        if compound { format!("({})", s) } else { s }
    };
    if let Some(l) = one("intersectionOf") { return list_items(g, &l).iter().map(|x| sub(x)).collect::<Vec<_>>().join(" and "); }
    if let Some(l) = one("unionOf") { return list_items(g, &l).iter().map(|x| sub(x)).collect::<Vec<_>>().join(" or "); }
    if let Some(c) = one("complementOf") { return format!("not {}", sub(&c)); }
    if let Some(l) = one("oneOf") { return format!("{{{}}}", list_items(g, &l).iter().map(|x| short_name(x, idx)).collect::<Vec<_>>().join(", ")); }
    if let Some(p) = one("inverseOf") { return format!("inverse {}", short_name(&p, idx)); }
    if let Some(dt) = one("onDatatype") {
        let facets: Vec<String> = one("withRestrictions").map(|l| list_items(g, &l)).unwrap_or_default().iter().flat_map(|f| {
            [("minInclusive", ">="), ("minExclusive", ">"), ("maxInclusive", "<="), ("maxExclusive", "<")].iter()
                .filter_map(|(n, op)| objects(g, f, &format!("i:{}{}", XSD, n)).into_iter().next().map(|v| format!("{} {}", op, literal_text(&v))))
                .collect::<Vec<_>>()
        }).collect();
        return format!("{}[{}]", short_name(&dt, idx), facets.join(", "));
    }
    if let Some(p) = one("onProperty") {
        let p = if p.starts_with("b:") { render_depth(g, &p, idx, depth + 1) } else { short_name(&p, idx) };
        if let Some(c) = one("someValuesFrom") { return format!("{} some {}", p, sub(&c)); }
        if let Some(c) = one("allValuesFrom") { return format!("{} only {}", p, sub(&c)); }
        if let Some(v) = one("hasValue") { return format!("{} value {}", p, short_name(&v, idx)); }
        if one("hasSelf").is_some() { return format!("{} Self", p); }
        let filler = one("onClass").or_else(|| one("onDataRange")).map(|c| format!(" {}", sub(&c))).unwrap_or_default();
        for (pred, kw) in [("minQualifiedCardinality", "min"), ("maxQualifiedCardinality", "max"), ("qualifiedCardinality", "exactly"), ("minCardinality", "min"), ("maxCardinality", "max"), ("cardinality", "exactly")] {
            if let Some(n) = one(pred) { return format!("{} {} {}{}", p, kw, literal_text(&n), filler); }
        }
        return format!("{} (?)", p);
    }
    short_name(t, idx)
}

/// All triples that make up the blank-node structure rooted at `t` (for clean removal).
/// Stops at IRIs and literals; never follows a blank node that is also used elsewhere
/// as the object of more than one triple (shared structures are left alone).
pub fn structure_triples(g: &FastGraph, t: &str) -> Vec<Spo> {
    let mut out = Vec::new();
    let mut stack = vec![t.to_string()];
    let mut seen = HashSet::new();
    while let Some(n) = stack.pop() {
        if !n.starts_with("b:") || !seen.insert(n.clone()) { continue; }
        for tr in g.triples_matching(Some(&term(&n)), Any, Any).flatten() {
            let (p, o) = (extract_str(&tr.p()), extract_str(&tr.o()));
            if o.starts_with("b:") && g.triples_matching(Any, Any, Some(&term(&o))).count() <= 1 { stack.push(o.clone()); }
            out.push((n.clone(), p, o));
        }
    }
    out
}

/// Removing axiom `(s p o)`: the axiom itself plus `o`'s blank-node structure, if any.
pub fn axiom_removal(g: &FastGraph, s: &str, p: &str, o: &str) -> Vec<Spo> {
    let mut v = vec![(s.to_string(), p.to_string(), o.to_string())];
    if o.starts_with("b:") && g.triples_matching(Any, Any, Some(&term(o))).count() <= 1 {
        v.extend(structure_triples(g, o));
    }
    v
}

// ---------------------------------------------------------------------------
// Refactoring
// ---------------------------------------------------------------------------

/// Change set that renames IRI `old` to `new` everywhere in `g` (subject, predicate, object).
pub fn rename_changes(g: &FastGraph, old: &str, new: &str) -> (Vec<Spo>, Vec<Spo>) {
    let ot = term(old);
    let mut removes = Vec::new();
    let mut seen = HashSet::new();
    for tr in g.triples_matching(Some(&ot), Any, Any).flatten()
        .chain(g.triples_matching(Any, Some(&ot), Any).flatten())
        .chain(g.triples_matching(Any, Any, Some(&ot)).flatten())
    {
        let k = (extract_str(&tr.s()), extract_str(&tr.p()), extract_str(&tr.o()));
        if seen.insert(k.clone()) { removes.push(k); }
    }
    let swap = |x: &String| if x == old { new.to_string() } else { x.clone() };
    let adds = removes.iter().map(|(s, p, o)| (swap(s), swap(p), swap(o))).collect();
    (removes, adds)
}

/// Validates a user-entered IRI for renaming / creating entities.
pub fn validate_iri(s: &str) -> Result<String, String> {
    let s = s.trim().trim_start_matches('<').trim_end_matches('>');
    if s.is_empty() { return Err("IRI가 비어 있습니다 (empty IRI)".into()); }
    if s.chars().any(|c| c.is_whitespace() || "<>\"{}|\\^`".contains(c)) {
        return Err("IRI에 공백이나 < > \" { } | \\ ^ ` 문자를 쓸 수 없습니다".into());
    }
    if !s.contains(':') { return Err("전체 IRI를 입력하세요 (예: http://example.org/onto#Name)".into()); }
    Ok(format!("i:{}", s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::io::{parse_into, OntologyFormat};

    fn setup() -> (FastGraph, DlIndex) {
        let ttl = r#"
            @prefix ex: <http://example.org/> .
            @prefix owl: <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            ex:Person a owl:Class . ex:Activist a owl:Class . ex:Org a owl:Class .
            ex:memberOf a owl:ObjectProperty . ex:year a owl:DatatypeProperty .
            ex:kim a ex:Person ; rdfs:label "김아무개"@ko .
        "#;
        let mut g = FastGraph::new();
        parse_into(&mut g, ttl, OntologyFormat::Turtle, None).unwrap();
        let idx = DlIndex::build(&g);
        (g, idx)
    }

    fn apply(g: &mut FastGraph, ts: &[Spo]) {
        for (s, p, o) in ts { g.insert(&term(s), &term(p), &term(o)).unwrap(); }
    }

    #[test]
    fn roundtrip_manchester_rdf_manchester() {
        let (mut g, idx) = setup();
        for src in [
            "Person and memberOf some Org",
            "Person or Org",
            "not Org",
            "memberOf only (Org or Person)",
            "memberOf value kim",
            "memberOf min 2 Org",
            "memberOf exactly 1",
            "year some xsd:integer[>= 1900, < 1946]",
            "inverse memberOf some Person",
            "{kim}",
        ] {
            let (t, ts) = manchester_to_rdf(src, &idx).unwrap_or_else(|e| panic!("{}: {}", src, e));
            apply(&mut g, &ts);
            let rendered = render(&g, &t, Some(&idx));
            assert_eq!(rendered, src, "round trip of {:?}", src);
            // what we render must parse back to the same structure
            let (_, ts2) = manchester_to_rdf(&rendered, &idx).unwrap();
            assert_eq!(ts.len(), ts2.len(), "{}", src);
        }
    }

    #[test]
    fn removal_takes_whole_structure_and_reasoner_understands_it() {
        let (mut g, idx) = setup();
        let (t, ts) = manchester_to_rdf("Person and memberOf some Org", &idx).unwrap();
        let ax = ("i:http://example.org/Activist".to_string(), rdfs("subClassOf"), t.clone());
        apply(&mut g, &ts);
        apply(&mut g, &[ax.clone()]);
        let rm = axiom_removal(&g, &ax.0, &ax.1, &ax.2);
        assert_eq!(rm.len(), ts.len() + 1, "{:?}", rm);
        for (s, p, o) in &rm { g.remove(&term(s), &term(p), &term(o)).unwrap(); }
        assert!(g.triples().flatten().all(|t| !extract_str(&t.s()).starts_with("b:")), "left-over blank nodes");
    }

    #[test]
    fn rename_and_kinds() {
        let (g, _) = setup();
        let (rm, add) = rename_changes(&g, "i:http://example.org/Person", "i:http://example.org/Human");
        assert_eq!(rm.len(), 2); // Person a owl:Class ; kim a Person
        assert!(add.iter().all(|(s, _, o)| !s.ends_with("Person") && !o.ends_with("Person")));
        assert_eq!(entity_kind(&g, "i:http://example.org/Person"), EntityKind::Class);
        assert_eq!(entity_kind(&g, "i:http://example.org/memberOf"), EntityKind::ObjectProperty);
        assert_eq!(entity_kind(&g, "i:http://example.org/year"), EntityKind::DataProperty);
        assert_eq!(entity_kind(&g, "i:http://example.org/kim"), EntityKind::Individual);
        assert!(validate_iri("http://example.org/A B").is_err());
        assert_eq!(validate_iri("<http://example.org/A>").unwrap(), "i:http://example.org/A");
    }

    #[test]
    fn end_to_end_edit_reason_save_undo() {
        use crate::core::manager::DarkstarManager;
        use crate::core::settings::ReasonerRules;
        let rules = ReasonerRules::default();
        let mut m = DarkstarManager::new();
        m.replace_from_string(r#"
            @prefix ex: <http://example.org/> .
            @prefix owl: <http://www.w3.org/2002/07/owl#> .
            ex:Person a owl:Class . ex:Org a owl:Class . ex:Activist a owl:Class .
            ex:memberOf a owl:ObjectProperty .
            ex:kim a ex:Person ; ex:memberOf ex:club . ex:club a ex:Org .
            ex:lee a ex:Person .
        "#, OntologyFormat::Turtle, &rules).unwrap();
        let kim = "i:http://example.org/kim";
        let activist = "i:http://example.org/Activist";
        let is_activist = |m: &DarkstarManager, x: &str| has(&m.memory.main_graph, x, &rdf("type"), activist);
        assert!(!is_activist(&m, kim));

        // Activist ≡ Person and memberOf some Org   (a defined class, typed in Manchester)
        let idx = DlIndex::build(&m.memory.main_graph);
        let (t, mut adds) = manchester_to_rdf("Person and memberOf some Org", &idx).unwrap();
        adds.push((activist.to_string(), owl("equivalentClass"), t.clone()));
        let added = adds.len();
        m.apply_changes(vec![], adds);
        m.run_reasoning(&rules);
        assert!(is_activist(&m, kim), "reasoner should classify kim as Activist");
        assert!(!is_activist(&m, "i:http://example.org/lee"));
        // the explanation bottoms out in the new axiom
        let e = crate::rules::explain::explain(&m.memory, kim, &rdf("type"), activist);
        assert!(matches!(e.nodes[e.root].origin, crate::rules::explain::Origin::Inferred { .. }));

        // save + reload in both formats: the expression survives and renders the same
        let dir = std::env::temp_dir().join(format!("darkstar_owl_edit_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["edited.owl", "edited.ttl"] {
            let path = dir.join(name);
            m.save_to_file(&path, OntologyFormat::from_path(&path), false).unwrap();
            let mut m2 = DarkstarManager::new();
            m2.load_from_file(&path, &rules).unwrap();
            let eq = objects(&m2.memory.asserted_graph, activist, &owl("equivalentClass"));
            assert_eq!(eq.len(), 1, "{}", name);
            assert_eq!(render(&m2.memory.asserted_graph, &eq[0], None), "Person and memberOf some Org", "{}", name);
            assert!(is_activist(&m2, kim), "{}", name);
        }
        let _ = std::fs::remove_dir_all(&dir);

        // removing the axiom takes its whole blank-node structure; reasoning retracts
        let rm = axiom_removal(&m.memory.asserted_graph, activist, &owl("equivalentClass"), &t);
        assert_eq!(rm.len(), added);
        let before = m.memory.asserted_graph.triples().count();
        m.apply_changes(rm, vec![]);
        m.run_reasoning(&rules);
        assert_eq!(m.memory.asserted_graph.triples().count(), before - added);
        assert!(!is_activist(&m, kim));
        m.undo();
        m.run_reasoning(&rules);
        assert!(is_activist(&m, kim), "undo restores the definition");

        // rename is one undoable step
        let (r, a) = rename_changes(&m.memory.asserted_graph, "i:http://example.org/Person", "i:http://example.org/Human");
        m.apply_changes(r, a);
        m.run_reasoning(&rules);
        assert!(has(&m.memory.asserted_graph, kim, &rdf("type"), "i:http://example.org/Human"));
        assert!(is_activist(&m, kim), "definition follows the renamed class");
        m.undo();
        m.run_reasoning(&rules);
        assert!(has(&m.memory.asserted_graph, kim, &rdf("type"), "i:http://example.org/Person"));
        assert!(!has(&m.memory.asserted_graph, kim, &rdf("type"), "i:http://example.org/Human"));
    }
}
