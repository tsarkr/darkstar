//! SPARQL 1.1 query support (SELECT / ASK / CONSTRUCT / DESCRIBE) evaluated directly
//! over Darkstar's in-memory graphs, using Oxigraph's `spargebra` parser and `spareval`
//! evaluator. Nothing is copied: `spareval` pulls triples through `QueryableDataset`.

use std::convert::Infallible;
use std::time::{Duration, Instant};
use oxrdf::{BlankNode, Literal, NamedNode, Term};
use sophia::api::prelude::*;
use sophia::api::term::{BnodeId, IriRef, LanguageTag, SimpleTerm};
use sophia::inmem::graph::FastGraph;
use spareval::{CancellationToken, InternalQuad, QueryEvaluator, QueryResults, QueryableDataset};
use spargebra::SparqlParser;

/// Prefixes available in every query without declaring them (a query's own PREFIX wins).
pub const DEFAULT_PREFIXES: &[(&str, &str)] = crate::vocab::STANDARD_PREFIXES;

pub enum SparqlResult {
    /// SELECT: variable names and rows of cells (Darkstar term strings: `i:`, `l:`, `b:`).
    Select { vars: Vec<String>, rows: Vec<Vec<Option<String>>> },
    Ask(bool),
    /// CONSTRUCT / DESCRIBE
    Graph(Vec<[String; 3]>),
}

pub struct SparqlOutput {
    pub result: SparqlResult,
    /// More results existed than `max_rows`.
    pub truncated: bool,
    pub elapsed: Duration,
}

// ---------------------------------------------------------------------------
// Term conversion (sophia <-> oxrdf)
// ---------------------------------------------------------------------------

fn to_ox<T: sophia::api::term::Term>(t: T) -> Term {
    if let Some(iri) = t.iri() {
        NamedNode::new_unchecked(iri.as_str()).into()
    } else if let Some(b) = t.bnode_id() {
        BlankNode::new_unchecked(b.as_str()).into()
    } else if let Some(lex) = t.lexical_form() {
        if let Some(lang) = t.language_tag() {
            Literal::new_language_tagged_literal_unchecked(lex.as_ref(), lang.as_str()).into()
        } else {
            let dt = t.datatype().map(|d| d.as_str().to_string()).unwrap_or_else(|| crate::vocab::XSD_STRING.into());
            Literal::new_typed_literal(lex.as_ref(), NamedNode::new_unchecked(dt)).into()
        }
    } else {
        BlankNode::new_unchecked("anon").into()
    }
}

fn to_sophia(t: &Term) -> Option<SimpleTerm<'static>> {
    Some(match t {
        Term::NamedNode(n) => SimpleTerm::Iri(IriRef::new_unchecked(n.as_str().to_string().into())),
        Term::BlankNode(b) => SimpleTerm::BlankNode(BnodeId::new_unchecked(b.as_str().to_string().into())),
        Term::Literal(l) => match l.language() {
            Some(lang) => SimpleTerm::LiteralLanguage(l.value().to_string().into(), LanguageTag::new_unchecked(lang.to_string().into())),
            None => SimpleTerm::LiteralDatatype(l.value().to_string().into(), IriRef::new_unchecked(l.datatype().as_str().to_string().into())),
        },
        #[allow(unreachable_patterns)]
        _ => return None,
    })
}

/// Same string encoding as `rules::extract_str`, so results can be selected in the UI.
pub fn term_to_darkstar(t: &Term) -> String {
    match t {
        Term::NamedNode(n) => format!("i:{}", n.as_str()),
        Term::BlankNode(b) => format!("b:{}", b.as_str()),
        Term::Literal(l) => {
            if let Some(lang) = l.language() {
                format!("l:{}@{}", l.value(), lang)
            } else if l.datatype().as_str() == crate::vocab::XSD_STRING {
                format!("l:{}", l.value())
            } else {
                format!("l:{}^^{}", l.value(), l.datatype().as_str())
            }
        }
        #[allow(unreachable_patterns)]
        other => format!("l:{}", other),
    }
}

// ---------------------------------------------------------------------------
// Dataset adapter
// ---------------------------------------------------------------------------

/// Term matcher where `None` is a wildcard (sophia's `Option` matcher treats `None` as "nothing").
struct Slot(Option<SimpleTerm<'static>>);

impl sophia::api::term::matcher::TermMatcher for Slot {
    type Term = SimpleTerm<'static>;
    fn matches<T2: sophia::api::term::Term + ?Sized>(&self, term: &T2) -> bool {
        match &self.0 {
            Some(mine) => sophia::api::term::Term::eq(mine, term.borrow_term()),
            None => true,
        }
    }
    fn constant(&self) -> Option<&Self::Term> {
        self.0.as_ref()
    }
}

/// Exposes one Darkstar graph as the SPARQL default graph (no named graphs).
#[derive(Clone, Copy)]
pub struct GraphDataset<'a> {
    graph: &'a FastGraph,
}

impl<'a> QueryableDataset<'a> for GraphDataset<'a> {
    type InternalTerm = Term;
    type Error = Infallible;

    fn internal_quads_for_pattern(
        &self,
        subject: Option<&Term>,
        predicate: Option<&Term>,
        object: Option<&Term>,
        graph_name: Option<Option<&Term>>,
    ) -> impl Iterator<Item = Result<InternalQuad<Term>, Infallible>> + use<'a> {
        // Only the default graph exists. A bound term that can't exist in the graph
        // (e.g. a literal in subject position) yields nothing.
        // `Some(None)` = the default graph. spareval also asks with `None` when the default graph
        // is "the union of all graphs" (no FROM clause); here that union is just our one graph.
        let in_default = matches!(graph_name, Some(None) | None);
        let conv = |t: Option<&Term>| -> Result<Option<SimpleTerm<'static>>, ()> {
            match t { None => Ok(None), Some(t) => to_sophia(t).map(Some).ok_or(()) }
        };
        let pattern = match (conv(subject), conv(predicate), conv(object)) {
            (Ok(s), Ok(p), Ok(o)) if in_default => Some((s, p, o)),
            _ => None,
        };
        let graph = self.graph;
        pattern.into_iter().flat_map(move |(s, p, o)| {
            graph.triples_matching(Slot(s), Slot(p), Slot(o)).flatten().map(|t| {
                Ok(InternalQuad {
                    subject: to_ox(t.s()),
                    predicate: to_ox(t.p()),
                    object: to_ox(t.o()),
                    graph_name: None,
                })
            })
        })
    }

    fn internalize_term(&self, term: Term) -> Result<Term, Infallible> { Ok(term) }
    fn externalize_term(&self, term: Term) -> Result<Term, Infallible> { Ok(term) }
}

/// Parses and evaluates a SPARQL query over `graph`.
/// Stops after `max_rows` results and cancels evaluation after `timeout`.
pub fn run_query(graph: &FastGraph, query: &str, max_rows: usize, timeout: Duration) -> Result<SparqlOutput, String> {
    let mut parser = SparqlParser::new();
    for (p, iri) in DEFAULT_PREFIXES {
        parser = parser.with_prefix(*p, *iri).map_err(|e| e.to_string())?;
    }
    let parsed = parser.parse_query(query).map_err(|e| format!("구문 오류 (syntax error): {}", e))?;

    let start = Instant::now();
    let token = CancellationToken::new();
    // Watchdog: cancels the query if it is still running after `timeout`.
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    {
        let token = token.clone();
        std::thread::spawn(move || {
            if done_rx.recv_timeout(timeout).is_err() { token.cancel(); }
        });
    }

    let evaluator = QueryEvaluator::new().with_cancellation_token(token);
    let dataset = GraphDataset { graph };
    let results = evaluator.prepare(&parsed).execute(dataset).map_err(|e| e.to_string());
    let timeout_msg = || format!("{}초 안에 끝나지 않아 중단했습니다. LIMIT을 추가하거나 조건을 좁혀 보세요. (Query cancelled after {}s.)", timeout.as_secs(), timeout.as_secs());

    let mut truncated = false;
    let out = match results? {
        QueryResults::Boolean(b) => SparqlResult::Ask(b),
        QueryResults::Solutions(solutions) => {
            let vars: Vec<String> = solutions.variables().iter().map(|v| v.as_str().to_string()).collect();
            let mut rows = Vec::new();
            for sol in solutions {
                if rows.len() >= max_rows { truncated = true; break; }
                let sol = sol.map_err(|e| if start.elapsed() >= timeout { timeout_msg() } else { e.to_string() })?;
                rows.push(sol.values().iter().map(|v| v.as_ref().map(term_to_darkstar)).collect());
            }
            SparqlResult::Select { vars, rows }
        }
        QueryResults::Graph(triples) => {
            let mut rows = Vec::new();
            for t in triples {
                if rows.len() >= max_rows { truncated = true; break; }
                let t = t.map_err(|e| if start.elapsed() >= timeout { timeout_msg() } else { e.to_string() })?;
                rows.push([
                    term_to_darkstar(&Term::from(t.subject)),
                    term_to_darkstar(&Term::from(t.predicate)),
                    term_to_darkstar(&t.object),
                ]);
            }
            SparqlResult::Graph(rows)
        }
    };
    let _ = done_tx.send(());
    Ok(SparqlOutput { result: out, truncated, elapsed: start.elapsed() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::io::{parse_into, OntologyFormat};

    fn graph(ttl: &str) -> FastGraph {
        let mut g = FastGraph::new();
        parse_into(&mut g, ttl, OntologyFormat::Turtle, None).unwrap();
        g
    }

    const DATA: &str = r#"
        @prefix ex: <http://example.org/> .
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
        ex:kim a ex:Activist ; rdfs:label "김아무개"@ko ; ex:age 25 ; ex:knows ex:lee .
        ex:lee a ex:Activist ; rdfs:label "이아무개"@ko ; ex:age 31 .
        ex:park a ex:Merchant ; rdfs:label "박아무개"@ko .
    "#;

    #[test]
    fn select_filter_optional_order_count() {
        let g = graph(DATA);
        let q = r#"PREFIX ex: <http://example.org/>
            SELECT ?x ?label ?friend WHERE {
              ?x a ex:Activist ; rdfs:label ?label ; ex:age ?age .
              OPTIONAL { ?x ex:knows ?friend }
              FILTER(?age > 20 && LANG(?label) = "ko")
            } ORDER BY DESC(?age)"#;
        let out = run_query(&g, q, 100, Duration::from_secs(10)).unwrap();
        let SparqlResult::Select { vars, rows } = out.result else { panic!() };
        assert_eq!(vars, vec!["x", "label", "friend"]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0].as_deref(), Some("i:http://example.org/lee"));
        assert_eq!(rows[0][1].as_deref(), Some("l:이아무개@ko"));
        assert_eq!(rows[0][2], None);
        assert_eq!(rows[1][2].as_deref(), Some("i:http://example.org/lee"));

        let q = "PREFIX ex: <http://example.org/> SELECT ?t (COUNT(?x) AS ?n) WHERE { ?x a ?t } GROUP BY ?t ORDER BY ?t";
        let SparqlResult::Select { rows, .. } = run_query(&g, q, 100, Duration::from_secs(10)).unwrap().result else { panic!() };
        assert_eq!(rows[0][1].as_deref(), Some("l:2^^http://www.w3.org/2001/XMLSchema#integer"));
    }

    #[test]
    fn ask_construct_limit_and_errors() {
        let g = graph(DATA);
        let SparqlResult::Ask(b) = run_query(&g, "ASK { ?x rdfs:label \"박아무개\"@ko }", 10, Duration::from_secs(5)).unwrap().result else { panic!() };
        assert!(b);
        let out = run_query(&g, "CONSTRUCT { ?x rdfs:seeAlso ?y } WHERE { ?x <http://example.org/knows> ?y }", 10, Duration::from_secs(5)).unwrap();
        let SparqlResult::Graph(t) = out.result else { panic!() };
        assert_eq!(t.len(), 1);
        let out = run_query(&g, "SELECT * WHERE { ?s ?p ?o }", 3, Duration::from_secs(5)).unwrap();
        assert!(out.truncated);
        assert!(run_query(&g, "SELEC * WHERE { ?s ?p ?o }", 3, Duration::from_secs(5)).is_err());
    }
}
