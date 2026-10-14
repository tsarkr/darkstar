use std::path::Path;
use sophia::api::prelude::*;
use sophia::inmem::graph::FastGraph;
use sophia::iri::Iri;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OntologyFormat {
    Turtle,
    NTriples,
    /// RDF/XML — the default serialization of `.owl` / `.rdf` files produced by Protégé.
    RdfXml,
}

impl OntologyFormat {
    /// Format implied by a file extension (used when saving).
    pub fn from_path(path: &Path) -> Self {
        match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
            Some("nt") => OntologyFormat::NTriples,
            Some("owl") | Some("rdf") | Some("xml") => OntologyFormat::RdfXml,
            _ => OntologyFormat::Turtle,
        }
    }

    /// Format of existing content: sniffs the data first (an `.owl` file may contain Turtle),
    /// then falls back to the extension. Returns an error for OWL syntaxes we can't read yet.
    pub fn detect(path: Option<&Path>, data: &str) -> Result<Self, String> {
        let head: String = data.trim_start_matches('\u{feff}').trim_start().chars().take(4096).collect();
        if head.starts_with('<') && (head.starts_with("<?xml") || head.contains("<rdf:RDF") || head.starts_with("<!DOCTYPE")) {
            // OWL/XML uses <Ontology> as its root element instead of <rdf:RDF>.
            if !head.contains("<rdf:RDF") && head.contains("<Ontology") {
                return Err("OWL/XML 형식은 아직 지원하지 않습니다. Protégé에서 RDF/XML 또는 Turtle로 다시 저장해 주세요. (OWL/XML syntax is not supported yet — re-save as RDF/XML or Turtle.)".to_string());
            }
            return Ok(OntologyFormat::RdfXml);
        }
        let first_word = head.split_whitespace().next().unwrap_or("");
        if matches!(first_word, "Prefix:" | "Ontology:") {
            return Err("Manchester 구문 파일은 아직 지원하지 않습니다. RDF/XML 또는 Turtle로 저장해 주세요. (Manchester syntax files are not supported yet.)".to_string());
        }
        if head.starts_with("Prefix(") || head.starts_with("Ontology(") {
            return Err("OWL 함수형 구문(Functional syntax)은 아직 지원하지 않습니다. RDF/XML 또는 Turtle로 저장해 주세요. (OWL Functional syntax is not supported yet.)".to_string());
        }
        Ok(match path.map(Self::from_path) {
            Some(OntologyFormat::NTriples) => OntologyFormat::NTriples,
            // N-Triples is a subset of Turtle, so Turtle is the safe default for anything else.
            _ => OntologyFormat::Turtle,
        })
    }

    pub fn label(&self) -> &'static str {
        match self {
            OntologyFormat::Turtle => "Turtle",
            OntologyFormat::NTriples => "N-Triples",
            OntologyFormat::RdfXml => "RDF/XML",
        }
    }
}

/// `file://` IRI for a local path, used as the base IRI so that relative references
/// (e.g. `rdf:about=""` in RDF/XML) resolve the same way Protégé resolves them.
pub fn file_iri(path: &Path) -> Option<Iri<String>> {
    let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut s = String::from("file://");
    let raw = abs.to_string_lossy().replace('\\', "/");
    if !raw.starts_with('/') { s.push('/'); } // Windows drive letters
    for b in raw.bytes() {
        let c = b as char;
        if c.is_ascii_alphanumeric() || "-._~/:".contains(c) { s.push(c); } else { s.push_str(&format!("%{:02X}", b)); }
    }
    Iri::new(s).ok()
}

/// Parses `data` in `format` and inserts every triple into `graph`.
pub fn parse_into(graph: &mut FastGraph, data: &str, format: OntologyFormat, base: Option<Iri<String>>) -> Result<usize, Box<dyn std::error::Error>> {
    let n = match format {
        OntologyFormat::Turtle => graph.insert_all(sophia::turtle::parser::turtle::TurtleParser { base }.parse_str(data))?,
        OntologyFormat::NTriples => graph.insert_all(sophia::turtle::parser::nt::NTriplesParser {}.parse_str(data))?,
        OntologyFormat::RdfXml => graph.insert_all(sophia::xml::parser::RdfXmlParser { base }.parse_str(data))?,
    };
    Ok(n)
}

/// Serializes a graph in the given format.
pub fn serialize_graph(graph: &FastGraph, format: OntologyFormat) -> Result<String, Box<dyn std::error::Error>> {
    use sophia::api::serializer::*;
    Ok(match format {
        OntologyFormat::Turtle => {
            let mut s = sophia::turtle::serializer::turtle::TurtleSerializer::new_stringifier();
            s.serialize_graph(graph)?;
            s.as_str().to_string()
        }
        OntologyFormat::NTriples => {
            let mut s = sophia::turtle::serializer::nt::NtSerializer::new_stringifier();
            s.serialize_graph(graph)?;
            s.as_str().to_string()
        }
        OntologyFormat::RdfXml => {
            let mut s = sophia::xml::serializer::RdfXmlSerializer::new_stringifier();
            s.serialize_graph(graph)?;
            s.as_str().to_string()
        }
    })
}
