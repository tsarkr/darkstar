//! Protégé-style "DL Query": a Manchester-syntax class expression is parsed and evaluated
//! against the materialized (OWL 2 RL) graph.
//!
//! Supported syntax:
//!   `A and B`, `A that B`, `A or B`, `not A`, `( … )`, `{a, b}`, `owl:Thing` / `Thing`,
//!   `p some C`, `p only C`, `p value a`, `p value "text"@ko`, `p value 1919`,
//!   `p min|max|exactly n [C]`, `p Self`, `inverse p some C`,
//!   data ranges: `p some xsd:integer`, `p some xsd:integer[>= 1900, < 1946]`.
//! Names: `<full IRI>`, `prefix:local` (rdf, rdfs, owl, xsd), local names (`E5_Event`),
//! or labels (`'강명하'`) — the same short forms Protégé accepts.
//!
//! Semantics: named classes, `and`, `or`, `some`, `value`, `min`, `{…}` use the facts the RL
//! reasoner derived, so their answers are sound. `not`, `only`, `max`, `exactly` need negation;
//! they are answered under a closed-world reading (what is *not stated or derived*), which can
//! differ from an OWL DL reasoner such as HermiT. Results flag this.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use sophia::api::prelude::*;
use sophia::api::term::matcher::Any;
use sophia::inmem::graph::FastGraph;
use crate::rules::engine::InferenceEngine;
use crate::rules::extract_str;

use crate::vocab::{
    OWL_EQUIVALENT_CLASS as OWL_EQUIV, OWL_NOTHING, OWL_THING, RDFS_LABEL, RDFS_LITERAL,
    RDFS_SUBCLASS_OF as RDFS_SUBCLASS, RDF_LANG_STRING, RDF_TYPE, META_TYPES, STANDARD_PREFIXES as PREFIXES, XSD, XSD_STRING,
};

// ---------------------------------------------------------------------------
// AST
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Prop { pub iri: String, pub inverse: bool }

#[derive(Debug, Clone, PartialEq)]
pub enum Facet { Ge(f64), Gt(f64), Le(f64), Lt(f64) }

#[derive(Debug, Clone, PartialEq)]
pub enum Ce {
    Named(String),
    And(Vec<Ce>),
    Or(Vec<Ce>),
    Not(Box<Ce>),
    OneOf(Vec<String>),
    /// `p some C` (filler None = `owl:Thing`)
    Some(Prop, Option<Box<Ce>>),
    Only(Prop, Box<Ce>),
    /// `p value v` — `v` is a Darkstar term string (`i:…` or `l:…`)
    Value(Prop, String),
    Min(u32, Prop, Option<Box<Ce>>),
    Max(u32, Prop, Option<Box<Ce>>),
    Exactly(u32, Prop, Option<Box<Ce>>),
    SelfRef(Prop),
    /// Data range used as a filler: datatype IRI + facets
    Datatype(String, Vec<Facet>),
}

impl Ce {
    fn uses_negation(&self) -> bool {
        match self {
            Ce::Not(_) | Ce::Only(..) | Ce::Max(..) | Ce::Exactly(..) => true,
            Ce::And(v) | Ce::Or(v) => v.iter().any(Ce::uses_negation),
            Ce::Some(_, Some(f)) | Ce::Min(_, _, Some(f)) => f.uses_negation(),
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// Name index (built once per ontology revision)
// ---------------------------------------------------------------------------

/// Lookup tables for resolving short names / labels and for evaluation.
pub struct DlIndex {
    /// schema local name (classes, properties, predicates) -> IRIs
    by_local: HashMap<String, Vec<String>>,
    /// extra `(iri, label)` pairs only for entities with >1 `rdfs:label`
    extra_labels: Vec<(String, String)>,
    /// individuals (Darkstar term strings) = the universe for `not`, `only`, `max`
    pub individuals: HashSet<String>,
    /// IRI -> first rdfs:label (for display and on-demand label resolution)
    pub labels: HashMap<String, String>,
}

fn local_name(iri: &str) -> &str {
    let t = iri.trim_end_matches(['/', '#']);
    t.rsplit(['#', '/']).next().unwrap_or(t)
}

impl DlIndex {
    pub fn build(g: &FastGraph) -> Self {
        use sophia::api::term::SimpleTerm;
        let rdf_type = InferenceEngine::make_term(RDF_TYPE);
        let meta: HashSet<&str> = META_TYPES.iter().copied().collect();
        let mut schema_iris: HashSet<&str> = HashSet::new();
        let mut schema_bnodes: HashSet<&str> = HashSet::new();
        let mut typed_iris: HashSet<&str> = HashSet::with_capacity(64_000);
        let mut typed_bnodes: HashSet<&str> = HashSet::new();

        for t in g.triples_matching(Any, Some(&rdf_type), Any).flatten() {
            let is_meta = matches!(t[2], SimpleTerm::Iri(o) if meta.contains(o.as_str()));
            if let SimpleTerm::Iri(o_iri) = t[2] {
                if !is_meta {
                    schema_iris.insert(o_iri.as_str());
                }
            }
            match t[0] {
                SimpleTerm::Iri(iri) => {
                    if is_meta { schema_iris.insert(iri.as_str()); } else { typed_iris.insert(iri.as_str()); }
                }
                SimpleTerm::BlankNode(b) => {
                    if is_meta { schema_bnodes.insert(b.as_str()); } else { typed_bnodes.insert(b.as_str()); }
                }
                _ => {}
            }
        }

        // Include all predicates and subClassOf/subPropertyOf participants in schema_iris
        for p_term in g.predicates().flatten() {
            if let SimpleTerm::Iri(iri) = p_term {
                schema_iris.insert(iri.as_str());
            }
        }
        let sco = InferenceEngine::make_term(RDFS_SUBCLASS);
        for t in g.triples_matching(Any, Some(&sco), Any).flatten() {
            if let SimpleTerm::Iri(s) = t[0] { schema_iris.insert(s.as_str()); }
            if let SimpleTerm::Iri(o) = t[2] { schema_iris.insert(o.as_str()); }
        }

        let mut by_local: HashMap<String, Vec<String>> = HashMap::with_capacity(schema_iris.len());
        for &iri in &schema_iris {
            by_local.entry(local_name(iri).to_string()).or_default().push(iri.to_string());
        }
        for v in by_local.values_mut() {
            if v.len() > 1 { v.sort_unstable(); v.dedup(); }
        }

        let mut individuals = HashSet::with_capacity(typed_iris.len() + typed_bnodes.len());
        for iri in typed_iris {
            if !schema_iris.contains(iri) {
                let mut s = String::with_capacity(2 + iri.len());
                s.push_str("i:");
                s.push_str(iri);
                individuals.insert(s);
            }
        }
        for b in typed_bnodes {
            if !schema_bnodes.contains(b) {
                let mut s = String::with_capacity(2 + b.len());
                s.push_str("b:");
                s.push_str(b);
                individuals.insert(s);
            }
        }

        let label_p = InferenceEngine::make_term(RDFS_LABEL);
        let mut labels: HashMap<String, String> = HashMap::with_capacity(individuals.len());
        let mut extra_labels: Vec<(String, String)> = Vec::new();
        for t in g.triples_matching(Any, Some(&label_p), Any).flatten() {
            if let (SimpleTerm::Iri(s), Some(lex)) = (t[0], t[2].lexical_form()) {
                let s_str = s.as_str();
                if !labels.contains_key(s_str) {
                    labels.insert(s_str.to_string(), lex.to_string());
                } else {
                    extra_labels.push((s_str.to_string(), lex.to_string()));
                }
            }
        }

        DlIndex { by_local, extra_labels, individuals, labels }
    }

    fn find_by_local(&self, target: &str) -> Vec<String> {
        if let Some(c) = self.by_local.get(target) {
            return c.clone();
        }
        let mut matches: Vec<String> = self.individuals.iter()
            .filter_map(|ind| ind.strip_prefix("i:"))
            .filter(|&iri| local_name(iri) == target)
            .map(|iri| iri.to_string())
            .collect();
        for iri in self.labels.keys() {
            if local_name(iri) == target {
                matches.push(iri.clone());
            }
        }
        matches.sort_unstable();
        matches.dedup();
        matches
    }

    fn find_by_label(&self, target: &str) -> Vec<String> {
        let target_lower = target.to_lowercase();
        let eq_ci = |lbl: &str| -> bool {
            if lbl.is_ascii() && target_lower.is_ascii() {
                lbl.eq_ignore_ascii_case(&target_lower)
            } else {
                lbl == target_lower || lbl.to_lowercase() == target_lower
            }
        };
        let mut matches: Vec<String> = self.labels.iter()
            .filter(|(_, lbl)| eq_ci(lbl))
            .map(|(iri, _)| iri.clone())
            .collect();
        for (iri, lbl) in &self.extra_labels {
            if eq_ci(lbl) {
                matches.push(iri.clone());
            }
        }
        matches.sort_unstable();
        matches.dedup();
        matches
    }

    /// Resolves a name token to an IRI.
    fn resolve(&self, name: &str) -> Result<String, String> {
        if let Some(iri) = name.strip_prefix('<').and_then(|n| n.strip_suffix('>')) {
            return Ok(iri.to_string());
        }
        if let Some(label) = name.strip_prefix('\'').and_then(|n| n.strip_suffix('\'')) {
            let by_lbl = self.find_by_label(label);
            if !by_lbl.is_empty() {
                return self.pick(name, Some(&by_lbl));
            }
            let by_loc = self.find_by_local(label);
            return self.pick(name, Some(&by_loc));
        }
        if name == "Thing" { return Ok(OWL_THING.into()); }
        if name == "Nothing" { return Ok(OWL_NOTHING.into()); }
        if let Some((pfx, local)) = name.split_once(':') {
            if let Some((_, ns)) = PREFIXES.iter().find(|(p, _)| *p == pfx) {
                return Ok(format!("{}{}", ns, local));
            }
            // Unknown prefix (Turtle prefixes are not kept after parsing): fall back to the local name.
            let by_loc = self.find_by_local(local);
            if !by_loc.is_empty() {
                return self.pick(name, Some(&by_loc));
            }
        }
        let by_loc = self.find_by_local(name);
        if !by_loc.is_empty() {
            return self.pick(name, Some(&by_loc));
        }
        let by_lbl = self.find_by_label(name);
        self.pick(name, Some(&by_lbl))
    }

    fn pick(&self, name: &str, cands: Option<&Vec<String>>) -> Result<String, String> {
        match cands.map(|v| v.as_slice()) {
            Some([one]) => Ok(one.clone()),
            Some(many) if !many.is_empty() => Err(format!(
                "'{}'에 해당하는 개체가 여러 개입니다 (ambiguous name). <전체 IRI>로 지정하세요: {}",
                name,
                many.iter().take(5).map(|i| format!("<{}>", i)).collect::<Vec<_>>().join(", ")
            )),
            _ => Err(format!("'{}'을(를) 찾을 수 없습니다 (unknown name)", name)),
        }
    }

    pub fn label_of(&self, term: &str) -> Option<&String> {
        term.strip_prefix("i:").and_then(|iri| self.labels.get(iri))
    }
}

// ---------------------------------------------------------------------------
// Tokenizer + parser
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Name(String),
    Str(String),      // complete literal in Darkstar form (`l:…`)
    Num(String),
    LParen, RParen, LBrace, RBrace, LBracket, RBracket, Comma,
    Op(String),       // >= > <= <
}

fn tokenize(src: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() { i += 1; continue; }
        match c {
            '(' => { out.push(Tok::LParen); i += 1; }
            ')' => { out.push(Tok::RParen); i += 1; }
            '{' => { out.push(Tok::LBrace); i += 1; }
            '}' => { out.push(Tok::RBrace); i += 1; }
            '[' => { out.push(Tok::LBracket); i += 1; }
            ']' => { out.push(Tok::RBracket); i += 1; }
            ',' => { out.push(Tok::Comma); i += 1; }
            '>' | '<' if chars.get(i + 1) == Some(&'=') => { out.push(Tok::Op(format!("{}=", c))); i += 2; }
            '>' => { out.push(Tok::Op(">".into())); i += 1; }
            '<' => {
                // `<IRI>` vs `<` facet: an IRI has no whitespace before the closing `>`
                let end = chars[i..].iter().position(|&x| x == '>');
                match end {
                    Some(e) if !chars[i..i + e].iter().any(|x| x.is_whitespace()) && e > 1 => {
                        out.push(Tok::Name(chars[i..=i + e].iter().collect()));
                        i += e + 1;
                    }
                    _ => { out.push(Tok::Op("<".into())); i += 1; }
                }
            }
            '\'' => {
                let end = chars[i + 1..].iter().position(|&x| x == '\'').ok_or("닫히지 않은 ' 따옴표 (unclosed quote)")?;
                out.push(Tok::Name(chars[i..=i + 1 + end].iter().collect()));
                i += end + 2;
            }
            '"' => {
                let end = chars[i + 1..].iter().position(|&x| x == '"').ok_or("닫히지 않은 \" 따옴표 (unclosed string)")?;
                let lex: String = chars[i + 1..i + 1 + end].iter().collect();
                i += end + 2;
                if chars.get(i) == Some(&'@') {
                    let start = i + 1;
                    let mut j = start;
                    while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '-') { j += 1; }
                    out.push(Tok::Str(format!("l:{}@{}", lex, chars[start..j].iter().collect::<String>())));
                    i = j;
                } else if chars.get(i) == Some(&'^') && chars.get(i + 1) == Some(&'^') {
                    let start = i + 2;
                    let mut j = start;
                    while j < chars.len() && !chars[j].is_whitespace() && !"(){}[],".contains(chars[j]) { j += 1; }
                    let dt: String = chars[start..j].iter().collect();
                    let dt = match dt.split_once(':') {
                        Some(("xsd", l)) => format!("{}{}", XSD, l),
                        _ => dt.trim_start_matches('<').trim_end_matches('>').to_string(),
                    };
                    out.push(Tok::Str(format!("l:{}^^{}", lex, dt)));
                    i = j;
                } else {
                    out.push(Tok::Str(format!("l:{}", lex)));
                }
            }
            c if c.is_ascii_digit() || (c == '-' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())) => {
                let start = i;
                i += 1;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') { i += 1; }
                out.push(Tok::Num(chars[start..i].iter().collect()));
            }
            _ => {
                let start = i;
                while i < chars.len() && !chars[i].is_whitespace() && !"(){}[],'\"".contains(chars[i]) { i += 1; }
                out.push(Tok::Name(chars[start..i].iter().collect()));
            }
        }
    }
    Ok(out)
}

struct Parser<'a> { toks: Vec<Tok>, pos: usize, idx: &'a DlIndex }

fn is_kw(t: Option<&Tok>, kw: &str) -> bool {
    matches!(t, Some(Tok::Name(n)) if n.eq_ignore_ascii_case(kw))
}
const RESTRICTION_KWS: &[&str] = &["some", "only", "value", "min", "max", "exactly", "self"];

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Tok> { self.toks.get(self.pos) }
    fn peek_at(&self, k: usize) -> Option<&Tok> { self.toks.get(self.pos + k) }
    fn next(&mut self) -> Option<Tok> { let t = self.toks.get(self.pos).cloned(); self.pos += 1; t }
    fn expect(&mut self, t: Tok, what: &str) -> Result<(), String> {
        if self.peek() == Some(&t) { self.pos += 1; Ok(()) } else { Err(format!("{}이(가) 필요합니다 (expected {})", what, what)) }
    }

    fn expr(&mut self) -> Result<Ce, String> {
        let mut items = vec![self.conj()?];
        while is_kw(self.peek(), "or") { self.pos += 1; items.push(self.conj()?); }
        Ok(if items.len() == 1 { items.pop().unwrap() } else { Ce::Or(items) })
    }

    fn conj(&mut self) -> Result<Ce, String> {
        let mut items = vec![self.unary()?];
        while is_kw(self.peek(), "and") || is_kw(self.peek(), "that") { self.pos += 1; items.push(self.unary()?); }
        Ok(if items.len() == 1 { items.pop().unwrap() } else { Ce::And(items) })
    }

    fn unary(&mut self) -> Result<Ce, String> {
        if is_kw(self.peek(), "not") { self.pos += 1; return Ok(Ce::Not(Box::new(self.unary()?))); }
        // property restriction?  [inverse] p <kw> …
        let (offset, inverse) = if is_kw(self.peek(), "inverse") { (1, true) } else { (0, false) };
        if matches!(self.peek_at(offset), Some(Tok::Name(_)))
            && RESTRICTION_KWS.iter().any(|k| is_kw(self.peek_at(offset + 1), k))
        {
            self.pos += offset;
            let Some(Tok::Name(pname)) = self.next() else { return Err("프로퍼티 이름이 필요합니다 (expected property)".into()) };
            let prop = Prop { iri: self.idx.resolve(&pname)?, inverse };
            return self.restriction(prop);
        }
        self.atomic()
    }

    fn restriction(&mut self, prop: Prop) -> Result<Ce, String> {
        let Some(Tok::Name(kw)) = self.next() else { unreachable!() };
        match kw.to_ascii_lowercase().as_str() {
            "some" => Ok(Ce::Some(prop, Some(Box::new(self.filler()?)))),
            "only" => Ok(Ce::Only(prop, Box::new(self.filler()?))),
            "self" => Ok(Ce::SelfRef(prop)),
            "value" => match self.next() {
                Some(Tok::Str(l)) => Ok(Ce::Value(prop, l)),
                Some(Tok::Num(n)) => {
                    let dt = if n.contains('.') { "decimal" } else { "integer" };
                    Ok(Ce::Value(prop, format!("l:{}^^{}{}", n, XSD, dt)))
                }
                Some(Tok::Name(n)) => Ok(Ce::Value(prop, format!("i:{}", self.idx.resolve(&n)?))),
                _ => Err("value 다음에 개체나 값이 필요합니다 (expected individual or literal after 'value')".into()),
            },
            k @ ("min" | "max" | "exactly") => {
                let Some(Tok::Num(n)) = self.next() else { return Err(format!("{} 다음에 숫자가 필요합니다 (expected a number)", k)) };
                let n: u32 = n.parse().map_err(|_| "정수가 필요합니다 (expected an integer)")?;
                let filler = if self.starts_ce() { Some(Box::new(self.filler()?)) } else { None };
                Ok(match k { "min" => Ce::Min(n, prop, filler), "max" => Ce::Max(n, prop, filler), _ => Ce::Exactly(n, prop, filler) })
            }
            _ => unreachable!(),
        }
    }

    fn starts_ce(&self) -> bool {
        match self.peek() {
            Some(Tok::LParen) | Some(Tok::LBrace) => true,
            Some(Tok::Name(n)) => !["and", "or", "that"].iter().any(|k| n.eq_ignore_ascii_case(k)),
            _ => false,
        }
    }

    /// Filler of some/only/min…: a class expression, or a datatype with optional facets.
    fn filler(&mut self) -> Result<Ce, String> {
        if let Some(Tok::Name(n)) = self.peek().cloned() {
            if let Ok(iri) = self.idx.resolve(&n) {
                if iri.starts_with(XSD) || iri == RDFS_LITERAL {
                    self.pos += 1;
                    let mut facets = Vec::new();
                    if self.peek() == Some(&Tok::LBracket) {
                        self.pos += 1;
                        loop {
                            let Some(Tok::Op(op)) = self.next() else { return Err("패싯 연산자(>=, >, <=, <)가 필요합니다 (expected facet operator)".into()) };
                            let Some(Tok::Num(v)) = self.next() else { return Err("패싯 값은 숫자여야 합니다 (facet value must be a number)".into()) };
                            let v: f64 = v.parse().map_err(|_| "숫자 형식 오류 (bad number)")?;
                            facets.push(match op.as_str() { ">=" => Facet::Ge(v), ">" => Facet::Gt(v), "<=" => Facet::Le(v), _ => Facet::Lt(v) });
                            if self.peek() == Some(&Tok::Comma) { self.pos += 1; continue; }
                            self.expect(Tok::RBracket, "]")?;
                            break;
                        }
                    }
                    return Ok(Ce::Datatype(iri, facets));
                }
            }
        }
        self.unary()
    }

    fn atomic(&mut self) -> Result<Ce, String> {
        match self.next() {
            Some(Tok::LParen) => { let e = self.expr()?; self.expect(Tok::RParen, ")")?; Ok(e) }
            Some(Tok::LBrace) => {
                let mut items = Vec::new();
                loop {
                    match self.next() {
                        Some(Tok::Name(n)) => items.push(self.idx.resolve(&n)?),
                        _ => return Err("{ } 안에는 개체 이름이 와야 합니다 (expected individual names in { })".into()),
                    }
                    match self.next() {
                        Some(Tok::Comma) => continue,
                        Some(Tok::RBrace) => break,
                        _ => return Err("',' 또는 '}'가 필요합니다".into()),
                    }
                }
                Ok(Ce::OneOf(items))
            }
            Some(Tok::Name(n)) => Ok(Ce::Named(self.idx.resolve(&n)?)),
            Some(t) => Err(format!("예상치 못한 기호 {:?} (unexpected token)", t)),
            None => Err("식이 비어 있거나 중간에 끝났습니다 (unexpected end of expression)".into()),
        }
    }
}

pub fn parse(src: &str, idx: &DlIndex) -> Result<Ce, String> {
    let toks = tokenize(src)?;
    if toks.is_empty() { return Err("클래스 식을 입력하세요 (enter a class expression)".into()); }
    let mut p = Parser { toks, pos: 0, idx };
    let ce = p.expr()?;
    if p.pos < p.toks.len() {
        return Err(format!("식 뒤에 남는 부분이 있습니다: {:?} (unexpected trailing input)", p.toks[p.pos]));
    }
    Ok(ce)
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

struct Eval<'a> { g: &'a FastGraph, idx: &'a DlIndex }

impl<'a> Eval<'a> {
    /// (x, y) pairs of a property (respecting `inverse`).
    fn pairs(&self, p: &Prop) -> Vec<(String, String)> {
        let pt = InferenceEngine::make_term(&p.iri);
        self.g.triples_matching(Any, Some(&pt), Any).flatten()
            .map(|t| {
                let (s, o) = (extract_str(&t.s()), extract_str(&t.o()));
                if p.inverse { (o, s) } else { (s, o) }
            })
            .collect()
    }

    fn literal_matches(&self, term: &str, dt: &str, facets: &[Facet]) -> bool {
        let Some(raw) = term.strip_prefix("l:") else { return false };
        let (lex, this_dt) = match raw.rfind("^^") {
            Some(i) => (&raw[..i], &raw[i + 2..]),
            None => (raw, if raw.rfind('@').is_some() { RDF_LANG_STRING } else { XSD_STRING }),
        };
        if dt != RDFS_LITERAL {
            let numeric = |d: &str| ["integer", "int", "long", "short", "decimal", "double", "float", "nonNegativeInteger", "positiveInteger", "gYear"].iter().any(|n| d == format!("{}{}", XSD, n));
            let same = this_dt == dt || (numeric(dt) && numeric(this_dt) && dt.ends_with("decimal"));
            if !same { return false; }
        }
        if facets.is_empty() { return true; }
        let Ok(v) = lex.trim().parse::<f64>() else { return false };
        facets.iter().all(|f| match *f { Facet::Ge(x) => v >= x, Facet::Gt(x) => v > x, Facet::Le(x) => v <= x, Facet::Lt(x) => v < x })
    }

    /// Returns `None` for a pure data range (only meaningful as a filler).
    fn filler_test<'b>(&'b self, f: &'b Option<Box<Ce>>) -> Box<dyn Fn(&str) -> bool + 'b> {
        match f.as_deref() {
            None => Box::new(|_| true),
            Some(Ce::Named(n)) if n == OWL_THING => Box::new(|_| true),
            Some(Ce::Datatype(dt, facets)) => Box::new(move |y: &str| self.literal_matches(y, dt, facets)),
            Some(ce) => {
                let set = self.eval(ce);
                Box::new(move |y: &str| set.contains(y))
            }
        }
    }

    fn eval(&self, ce: &Ce) -> HashSet<String> {
        match ce {
            Ce::Named(c) if c == OWL_THING => self.idx.individuals.clone(),
            Ce::Named(c) if c == OWL_NOTHING => HashSet::new(),
            Ce::Named(c) => {
                let ty = InferenceEngine::make_term(RDF_TYPE);
                let ct = InferenceEngine::make_term(c);
                self.g.triples_matching(Any, Some(&ty), Some(&ct)).flatten().map(|t| extract_str(&t.s())).collect()
            }
            Ce::And(items) => {
                let mut sets: Vec<HashSet<String>> = items.iter().map(|i| self.eval(i)).collect();
                sets.sort_by_key(|s| s.len());
                let mut it = sets.into_iter();
                let first = it.next().unwrap_or_default();
                it.fold(first, |acc, s| acc.into_iter().filter(|x| s.contains(x)).collect())
            }
            Ce::Or(items) => items.iter().flat_map(|i| self.eval(i)).collect(),
            Ce::Not(inner) => {
                let ex = self.eval(inner);
                self.idx.individuals.iter().filter(|x| !ex.contains(*x)).cloned().collect()
            }
            Ce::OneOf(items) => items.iter().map(|i| format!("i:{}", i)).collect(),
            Ce::Datatype(..) => HashSet::new(),
            Ce::SelfRef(p) => self.pairs(p).into_iter().filter(|(x, y)| x == y).map(|(x, _)| x).collect(),
            Ce::Value(p, v) => {
                let pt = InferenceEngine::make_term(&p.iri);
                let vt = InferenceEngine::make_term(v);
                if p.inverse {
                    self.g.triples_matching(Some(&vt), Some(&pt), Any).flatten().map(|t| extract_str(&t.o())).collect()
                } else {
                    self.g.triples_matching(Any, Some(&pt), Some(&vt)).flatten().map(|t| extract_str(&t.s())).collect()
                }
            }
            Ce::Some(p, f) => {
                let test = self.filler_test(f);
                self.pairs(p).into_iter().filter(|(_, y)| test(y)).map(|(x, _)| x).collect()
            }
            Ce::Only(p, f) => {
                let f = Some(f.clone());
                let test = self.filler_test(&f);
                let mut bad = HashSet::new();
                for (x, y) in self.pairs(p) { if !test(&y) { bad.insert(x); } }
                self.idx.individuals.iter().filter(|x| !bad.contains(*x)).cloned().collect()
            }
            Ce::Min(n, p, f) | Ce::Max(n, p, f) | Ce::Exactly(n, p, f) => {
                let test = self.filler_test(f);
                let mut counts: HashMap<String, HashSet<String>> = HashMap::new();
                for (x, y) in self.pairs(p) { if test(&y) { counts.entry(x).or_default().insert(y); } }
                let n = *n as usize;
                match ce {
                    Ce::Min(..) => counts.into_iter().filter(|(_, ys)| ys.len() >= n).map(|(x, _)| x).collect(),
                    _ => {
                        let exact = matches!(ce, Ce::Exactly(..));
                        self.idx.individuals.iter()
                            .filter(|x| {
                                let c = counts.get(*x).map_or(0, |ys| ys.len());
                                if exact { c == n } else { c <= n }
                            })
                            .cloned().collect()
                    }
                }
            }
        }
    }
}

pub struct DlResult {
    /// Darkstar term strings, sorted
    pub instances: Vec<String>,
    /// Only for named classes (or intersections of them): `None` otherwise.
    pub superclasses: Option<Vec<String>>,
    pub subclasses: Option<Vec<String>>,
    pub equivalent: Option<Vec<String>>,
    /// `not` / `only` / `max` / `exactly` were answered under the closed-world reading.
    pub closed_world: bool,
    pub elapsed: Duration,
}

fn named_classes(ce: &Ce) -> Option<Vec<&str>> {
    match ce {
        Ce::Named(c) => Some(vec![c.as_str()]),
        Ce::And(items) => items.iter().map(|i| match i { Ce::Named(c) => Some(c.as_str()), _ => None }).collect(),
        _ => None,
    }
}

/// Parses and evaluates a DL query over the materialized graph.
pub fn run(g: &FastGraph, idx: &DlIndex, src: &str) -> Result<DlResult, String> {
    let start = Instant::now();
    let ce = parse(src, idx)?;
    let ev = Eval { g, idx };
    let mut instances: Vec<String> = ev.eval(&ce).into_iter().collect();
    instances.sort();

    let (mut superclasses, mut subclasses, mut equivalent) = (None, None, None);
    if let Some(classes) = named_classes(&ce) {
        let sco = InferenceEngine::make_term(RDFS_SUBCLASS);
        let eq = InferenceEngine::make_term(OWL_EQUIV);
        let iri_set = |it: &mut dyn Iterator<Item = String>| -> HashSet<String> { it.collect() };
        // superclasses of an intersection = union of the parts' superclasses
        let mut sup = HashSet::new();
        // subclasses of an intersection = classes below every part
        let mut sub: Option<HashSet<String>> = None;
        let mut eqv = HashSet::new();
        for c in &classes {
            let ct = InferenceEngine::make_term(c);
            sup.extend(g.triples_matching(Some(&ct), Some(&sco), Any).flatten().map(|t| extract_str(&t.o())));
            let below = iri_set(&mut g.triples_matching(Any, Some(&sco), Some(&ct)).flatten().map(|t| extract_str(&t.s())));
            sub = Some(match sub { None => below, Some(acc) => acc.intersection(&below).cloned().collect() });
            if classes.len() == 1 {
                eqv.extend(g.triples_matching(Some(&ct), Some(&eq), Any).flatten().map(|t| extract_str(&t.o())));
                eqv.extend(g.triples_matching(Any, Some(&eq), Some(&ct)).flatten().map(|t| extract_str(&t.s())));
            }
        }
        let own: HashSet<String> = classes.iter().map(|c| format!("i:{}", c)).collect();
        let clean = |s: HashSet<String>| -> Vec<String> {
            let mut v: Vec<String> = s.into_iter().filter(|x| !own.contains(x) && !x.starts_with("b:")).collect();
            v.sort();
            v
        };
        superclasses = Some(clean(sup));
        subclasses = Some(clean(sub.unwrap_or_default()));
        equivalent = Some(clean(eqv));
    }

    Ok(DlResult { instances, superclasses, subclasses, equivalent, closed_world: ce.uses_negation(), elapsed: start.elapsed() })
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
            @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
            ex:Person a owl:Class . ex:Activist a owl:Class ; rdfs:subClassOf ex:Person .
            ex:Event a owl:Class . ex:Activity a owl:Class ; rdfs:subClassOf ex:Event .
            ex:carriedOutBy a owl:ObjectProperty . ex:year a owl:DatatypeProperty .
            ex:kim a ex:Activist , ex:Person ; rdfs:label "김아무개"@ko .
            ex:lee a ex:Person ; rdfs:label "이아무개"@ko .
            ex:a1 a ex:Activity , ex:Event ; ex:carriedOutBy ex:kim ; ex:year "1919"^^xsd:integer .
            ex:a2 a ex:Activity , ex:Event ; ex:carriedOutBy ex:kim , ex:lee ; ex:year "1932"^^xsd:integer .
            ex:e1 a ex:Event ; ex:year "1950"^^xsd:integer .
        "#;
        let mut g = FastGraph::new();
        parse_into(&mut g, ttl, OntologyFormat::Turtle, None).unwrap();
        let idx = DlIndex::build(&g);
        (g, idx)
    }

    fn inst(g: &FastGraph, idx: &DlIndex, q: &str) -> Vec<String> {
        run(g, idx, q).unwrap_or_else(|e| panic!("{}: {}", q, e)).instances
            .into_iter().map(|s| s.rsplit('/').next().unwrap().to_string()).collect()
    }

    #[test]
    fn named_and_or_not() {
        let (g, idx) = setup();
        assert_eq!(inst(&g, &idx, "Person"), vec!["kim", "lee"]);
        assert_eq!(inst(&g, &idx, "Person and not Activist"), vec!["lee"]);
        assert_eq!(inst(&g, &idx, "Activist or Activity"), vec!["a1", "a2", "kim"]);
        assert_eq!(inst(&g, &idx, "'김아무개'"), Vec::<String>::new()); // individual name is not a class
        assert_eq!(inst(&g, &idx, "{ '김아무개', lee }"), vec!["kim", "lee"]);
        let r = run(&g, &idx, "Activist").unwrap();
        assert_eq!(r.superclasses.unwrap(), vec!["i:http://example.org/Person"]);
        assert!(!r.closed_world);
        assert!(run(&g, &idx, "not Activist").unwrap().closed_world);
    }

    #[test]
    fn restrictions() {
        let (g, idx) = setup();
        assert_eq!(inst(&g, &idx, "carriedOutBy some Activist"), vec!["a1", "a2"]);
        assert_eq!(inst(&g, &idx, "Event that carriedOutBy value '이아무개'"), vec!["a2"]);
        assert_eq!(inst(&g, &idx, "carriedOutBy min 2 Person"), vec!["a2"]);
        assert_eq!(inst(&g, &idx, "Activity and carriedOutBy only Activist"), vec!["a1"]);
        assert_eq!(inst(&g, &idx, "Activity and carriedOutBy exactly 1"), vec!["a1"]);
        assert_eq!(inst(&g, &idx, "inverse carriedOutBy some Activity"), vec!["kim", "lee"]);
        assert_eq!(inst(&g, &idx, "year some xsd:integer[>= 1910, < 1946]"), vec!["a1", "a2"]);
        assert_eq!(inst(&g, &idx, "Event and (year value 1950)"), vec!["e1"]);
        assert_eq!(inst(&g, &idx, "<http://example.org/Activity> and ex:year some xsd:integer[> 1920]"), vec!["a2"]);
    }

    #[test]
    fn errors() {
        let (g, idx) = setup();
        assert!(run(&g, &idx, "Persn").is_err());
        assert!(run(&g, &idx, "Person and").is_err());
        assert!(run(&g, &idx, "(Person").is_err());
        assert!(run(&g, &idx, "").is_err());
    }
}
