//! External source resolvers and adapters around the existing
//! `tools/web-stack` deep_research engine.

pub mod scholarly;
pub mod web;

pub use scholarly::CanonicalCitation;
pub use scholarly::extract_dois_from_text;
pub use scholarly::resolve_arxiv;
pub use scholarly::resolve_doi_via_crossref;
pub use scholarly::resolve_doi_via_openalex;
