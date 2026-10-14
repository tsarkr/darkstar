//! `owl:imports` resolution, modelled on Protégé's behaviour:
//! imported ontologies are loaded into a separate, read-only graph that takes part in
//! reasoning and browsing but is never written back when the ontology is saved.
//!
//! Lookup order for each imported IRI:
//! 1. `catalog-v001.xml` next to the importing file (the catalog Protégé writes),
//! 2. `file:` IRIs,
//! 3. a file in the importing file's folder named after the IRI's last segment
//!    (with `.owl` / `.ttl` / `.rdf` / `.nt` tried when it has no extension),
//! 4. `http(s):` IRIs fetched with the system `curl` (content negotiation for Turtle / RDF/XML).

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use sophia::api::prelude::*;
use sophia::api::term::matcher::Any;
use sophia::inmem::graph::FastGraph;
use crate::core::io::{self, OntologyFormat};
use crate::rules::engine::InferenceEngine;

use crate::vocab::{OWL_IMPORTS, OWL_ONTOLOGY, RDF_TYPE};
const FETCH_TIMEOUT_SECS: u32 = 20;

#[derive(Debug, Clone)]
pub struct ImportStatus {
    /// The imported ontology IRI as written in `owl:imports`.
    pub iri: String,
    /// Where it was loaded from (file path or URL), if found.
    pub source: Option<String>,
    /// Number of triples loaded from it.
    pub triples: usize,
    /// Why it could not be loaded, if it failed.
    pub error: Option<String>,
    /// Import depth (1 = imported directly by the opened ontology).
    pub depth: usize,
}

impl ImportStatus {
    pub fn ok(&self) -> bool { self.error.is_none() }
}

/// IRIs imported by any ontology header in `g`.
pub fn imported_iris(g: &FastGraph) -> Vec<String> {
    let imports = InferenceEngine::make_term(OWL_IMPORTS);
    let mut v: Vec<String> = g.triples_matching(Any, Some(&imports), Any)
        .flatten()
        .filter_map(|t| t.o().iri().map(|i| i.as_str().to_string()))
        .collect();
    v.sort();
    v.dedup();
    v
}

/// IRIs declared as `owl:Ontology` in `g` (to avoid re-importing the ontology itself).
fn ontology_iris(g: &FastGraph) -> Vec<String> {
    let rdf_type = InferenceEngine::make_term(RDF_TYPE);
    let ont = InferenceEngine::make_term(OWL_ONTOLOGY);
    g.triples_matching(Any, Some(&rdf_type), Some(&ont))
        .flatten()
        .filter_map(|t| t.s().iri().map(|i| i.as_str().to_string()))
        .collect()
}

/// Parses `catalog-v001.xml` (`<uri name="IRI" uri="relative/or/absolute"/>` entries).
fn read_catalog(dir: &Path) -> HashMap<String, PathBuf> {
    let mut map = HashMap::new();
    let Ok(xml) = std::fs::read_to_string(dir.join("catalog-v001.xml")) else { return map };
    for chunk in xml.split("<uri").skip(1) {
        let end = chunk.find('>').unwrap_or(chunk.len());
        let tag = &chunk[..end];
        let attr = |name: &str| -> Option<String> {
            let key = format!("{}=\"", name);
            let start = tag.find(&key)? + key.len();
            let len = tag[start..].find('"')?;
            Some(tag[start..start + len].replace("&amp;", "&"))
        };
        if let (Some(name), Some(uri)) = (attr("name"), attr("uri")) {
            let target = uri.strip_prefix("file:").map(|p| p.trim_start_matches("//").to_string()).unwrap_or(uri);
            let p = PathBuf::from(&target);
            map.insert(name, if p.is_absolute() { p } else { dir.join(p) });
        }
    }
    map
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) { out.push(b); i += 3; continue; }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Finds a local file for an imported IRI. Returns None if it has to be fetched (or is missing).
fn locate_local(iri: &str, dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(dir) = dir {
        if let Some(p) = read_catalog(dir).get(iri) {
            if p.is_file() { return Some(p.clone()); }
        }
    }
    if let Some(rest) = iri.strip_prefix("file:") {
        let p = PathBuf::from(percent_decode(rest.trim_start_matches("//")));
        return p.is_file().then_some(p);
    }
    let dir = dir?;
    let last = iri.trim_end_matches(['/', '#']).rsplit(['/', '#']).next()?;
    if last.is_empty() { return None; }
    let candidate = dir.join(last);
    if candidate.is_file() { return Some(candidate); }
    if Path::new(last).extension().is_none() {
        for ext in ["owl", "ttl", "rdf", "nt"] {
            let c = dir.join(format!("{}.{}", last, ext));
            if c.is_file() { return Some(c); }
        }
    }
    None
}

/// Downloads an http(s) ontology with the system `curl` (present by default on macOS,
/// Windows 10+ and most Linux distributions), asking for Turtle or RDF/XML.
fn fetch_http(url: &str) -> Result<String, String> {
    let out = std::process::Command::new("curl")
        .args([
            "-sSL", "--fail",
            "--max-time", &FETCH_TIMEOUT_SECS.to_string(),
            "-H", "Accept: text/turtle, application/rdf+xml;q=0.9, application/n-triples;q=0.8, */*;q=0.1",
            url,
        ])
        .output()
        .map_err(|e| format!("curl 실행 실패 (curl not available): {}", e))?;
    if !out.status.success() {
        return Err(format!("다운로드 실패 (download failed): {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    String::from_utf8(out.stdout).map_err(|_| "응답이 UTF-8 텍스트가 아닙니다 (response is not UTF-8)".to_string())
}

/// Loads the transitive `owl:imports` closure of `root` into `out`.
/// `root_path` is the opened file (used for relative lookups and the catalog).
pub fn resolve_imports(root: &FastGraph, root_path: Option<&Path>, out: &mut FastGraph) -> Vec<ImportStatus> {
    let mut statuses = Vec::new();
    let mut seen: HashSet<String> = ontology_iris(root).into_iter().collect();
    let root_dir = root_path.and_then(|p| p.parent()).map(|p| p.to_path_buf());

    let mut queue: VecDeque<(String, Option<PathBuf>, usize)> =
        imported_iris(root).into_iter().map(|i| (i, root_dir.clone(), 1)).collect();

    while let Some((iri, dir, depth)) = queue.pop_front() {
        if !seen.insert(iri.clone()) { continue; }
        let mut status = ImportStatus { iri: iri.clone(), source: None, triples: 0, error: None, depth };

        let (data, source, next_dir, path_for_detect) = if let Some(p) = locate_local(&iri, dir.as_deref()) {
            match std::fs::read_to_string(&p) {
                Ok(d) => (d, p.display().to_string(), p.parent().map(|x| x.to_path_buf()), Some(p)),
                Err(e) => { status.error = Some(format!("{}: {}", p.display(), e)); statuses.push(status); continue; }
            }
        } else if iri.starts_with("http://") || iri.starts_with("https://") {
            match fetch_http(&iri) {
                Ok(d) => (d, iri.clone(), None, None),
                Err(e) => { status.error = Some(e); statuses.push(status); continue; }
            }
        } else {
            status.error = Some("파일을 찾을 수 없습니다 (not found locally; add it to catalog-v001.xml or the same folder)".to_string());
            statuses.push(status);
            continue;
        };
        status.source = Some(source);

        let format = match OntologyFormat::detect(path_for_detect.as_deref(), &data) {
            Ok(f) => f,
            Err(e) => { status.error = Some(e); statuses.push(status); continue; }
        };
        let base = path_for_detect.as_deref().and_then(io::file_iri)
            .or_else(|| sophia::iri::Iri::new(iri.clone()).ok());
        let mut g = FastGraph::new();
        match io::parse_into(&mut g, &data, format, base) {
            Ok(_) => {
                for t in g.triples().flatten() {
                    if out.insert(t.s(), t.p(), t.o()).unwrap_or(false) { status.triples += 1; }
                }
                seen.extend(ontology_iris(&g));
                for next in imported_iris(&g) { queue.push_back((next, next_dir.clone(), depth + 1)); }
            }
            Err(e) => status.error = Some(format!("구문 오류 (parse error): {}", e)),
        }
        statuses.push(status);
    }
    statuses
}
