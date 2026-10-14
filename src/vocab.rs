//! Shared RDF / RDFS / OWL / XSD vocabulary.
//!
//! Two forms are used across the code base:
//! - bare IRIs (`RDF_TYPE`, …) for building sophia terms with `InferenceEngine::make_term`,
//! - Darkstar term strings (`"i:<iri>"`, built with [`rdf`], [`rdfs`], [`owl`], [`xsd`]) for
//!   comparing against `rules::extract_str` output and for edit change sets.

pub const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
pub const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
pub const OWL: &str = "http://www.w3.org/2002/07/owl#";
pub const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

/// Prefixes understood everywhere a short name can be typed (SPARQL, DL Query, editors).
pub const STANDARD_PREFIXES: &[(&str, &str)] = &[("rdf", RDF), ("rdfs", RDFS), ("owl", OWL), ("xsd", XSD)];

// rdf:
pub const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
pub const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
pub const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
pub const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
pub const RDF_PROPERTY: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property";
pub const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";

// rdfs:
pub const RDFS_CLASS: &str = "http://www.w3.org/2000/01/rdf-schema#Class";
pub const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
pub const RDFS_SUBPROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
pub const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
pub const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
pub const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
pub const RDFS_LITERAL: &str = "http://www.w3.org/2000/01/rdf-schema#Literal";
pub const RDFS_DATATYPE: &str = "http://www.w3.org/2000/01/rdf-schema#Datatype";

// owl:
pub const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
pub const OWL_THING: &str = "http://www.w3.org/2002/07/owl#Thing";
pub const OWL_NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";
pub const OWL_ONTOLOGY: &str = "http://www.w3.org/2002/07/owl#Ontology";
pub const OWL_IMPORTS: &str = "http://www.w3.org/2002/07/owl#imports";
pub const OWL_NAMED_INDIVIDUAL: &str = "http://www.w3.org/2002/07/owl#NamedIndividual";
pub const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
pub const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
pub const OWL_ANNOTATION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";
pub const OWL_RESTRICTION: &str = "http://www.w3.org/2002/07/owl#Restriction";
pub const OWL_TRANSITIVE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";
pub const OWL_SYMMETRIC_PROPERTY: &str = "http://www.w3.org/2002/07/owl#SymmetricProperty";
pub const OWL_FUNCTIONAL_PROPERTY: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";
pub const OWL_INVERSE_FUNCTIONAL_PROPERTY: &str = "http://www.w3.org/2002/07/owl#InverseFunctionalProperty";
pub const OWL_IRREFLEXIVE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#IrreflexiveProperty";
pub const OWL_ASYMMETRIC_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AsymmetricProperty";
pub const OWL_INVERSE_OF: &str = "http://www.w3.org/2002/07/owl#inverseOf";
pub const OWL_SAME_AS: &str = "http://www.w3.org/2002/07/owl#sameAs";
pub const OWL_DIFFERENT_FROM: &str = "http://www.w3.org/2002/07/owl#differentFrom";
pub const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
pub const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";
pub const OWL_PROPERTY_CHAIN_AXIOM: &str = "http://www.w3.org/2002/07/owl#propertyChainAxiom";
pub const OWL_INTERSECTION_OF: &str = "http://www.w3.org/2002/07/owl#intersectionOf";
pub const OWL_UNION_OF: &str = "http://www.w3.org/2002/07/owl#unionOf";
pub const OWL_ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
pub const OWL_SOME_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
pub const OWL_ALL_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#allValuesFrom";
pub const OWL_HAS_KEY: &str = "http://www.w3.org/2002/07/owl#hasKey";

// xsd:
pub const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

/// `i:` term string for an rdf: local name.
pub fn rdf(local: &str) -> String { format!("i:{}{}", RDF, local) }
/// `i:` term string for an rdfs: local name.
pub fn rdfs(local: &str) -> String { format!("i:{}{}", RDFS, local) }
/// `i:` term string for an owl: local name.
pub fn owl(local: &str) -> String { format!("i:{}{}", OWL, local) }
/// `i:` term string for an xsd: local name.
pub fn xsd(local: &str) -> String { format!("i:{}{}", XSD, local) }
/// `i:` term string for a full IRI.
pub fn iri(full: &str) -> String { format!("i:{}", full) }

/// Types that mark schema entities (classes, properties, ontology headers) rather than
/// individuals. Shared by the individuals list and the DL-query universe.
pub const META_TYPES: &[&str] = &[
    OWL_CLASS,
    RDFS_CLASS,
    OWL_OBJECT_PROPERTY,
    OWL_DATATYPE_PROPERTY,
    OWL_ANNOTATION_PROPERTY,
    OWL_TRANSITIVE_PROPERTY,
    OWL_SYMMETRIC_PROPERTY,
    OWL_FUNCTIONAL_PROPERTY,
    OWL_INVERSE_FUNCTIONAL_PROPERTY,
    RDF_PROPERTY,
    OWL_ONTOLOGY,
    OWL_RESTRICTION,
    RDFS_DATATYPE,
];
