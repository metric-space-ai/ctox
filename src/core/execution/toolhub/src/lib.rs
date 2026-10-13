//! CTOX Toolhub: service adapters with managed authentication.
//!
//! DE: Grundlage für Dienst-Adapter (Toolkits) mit verwalteter Anmeldung.
//! Tokens bleiben im CTOX-Host (verschlüsselter Secret-Store); Workjet und
//! Modelle sehen nur Tool-Schemas und Ergebnisse.
//!
//! EN: Foundation for service adapters (toolkits) with managed auth. Tokens
//! stay inside the CTOX host (encrypted secret store); Workjet and models only
//! see tool schemas and results.
//!
//! Concept vocabulary follows Composio (MIT, see LICENSE.upstream).

pub mod http;
pub mod hub;
pub mod model;
pub mod oauth2;
pub mod pkce;
pub mod refresh;
pub mod remote_mcp;
pub mod store;
pub mod toolkits;

pub use hub::{ToolHub, ToolOutcome};
pub use model::*;
pub use store::{AccountStore, MemorySecretBackend, SecretBackend, SecretEntry};
