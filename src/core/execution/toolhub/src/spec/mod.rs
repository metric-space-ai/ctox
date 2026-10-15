//! Hot-pluggable adapters: versioned JSON specs interpreted at runtime.
//!
//! DE: Ein Adapter-Spec beschreibt einen Dienst deklarativ (Basis-URL, erlaubte
//! Hosts, Anmeldung, Aktionen mit Methode/Pfad/Parametern/Antwort-Abbildung).
//! `SpecAdapter` führt ihn aus, ohne CTOX neu zu kompilieren. Specs werden pro
//! Instanz gespeichert, versioniert und erst nach Freigabe aktiv geschaltet.
//!
//! EN: An adapter spec describes a service declaratively. `SpecAdapter`
//! interprets it through the same `ToolkitAdapter` seam as the compiled
//! adapters, so hub, credential store and transport stay unchanged. Specs are
//! stored per instance, versioned, and activated only after approval.
//!
//! Format: `schema/adapter-spec.v1.schema.json` (`"schema": "ctox.adapter-spec/v1"`).
pub mod adapter;
pub mod egress;
pub mod registry;
pub mod validate;

pub use adapter::SpecAdapter;
pub use egress::EgressPolicy;
pub use registry::{ActivePointer, SpecStatus, SpecStore};

/// Schema identifier every spec must carry.
pub const SCHEMA_ID: &str = "ctox.adapter-spec/v1";
