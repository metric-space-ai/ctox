//! Core type foundation (gap-item **N17**).
//!
//! Upstream `src/types/*.d.ts` is type-only TypeScript that does not compile to
//! a runtime artifact. Rust needs concrete `struct`s / `enum`s / `trait`s for
//! the same semantic surface, so this module defines them incrementally as
//! consuming modules need them.
//!
//! T1 decisions captured here:
//! - Documents are represented as `serde_json::Value` end-to-end (untyped at the
//!   storage layer; user code does its own deserialization on top). Upstream
//!   `RxDocumentData<RxDocType>` therefore becomes a plain type alias for
//!   [`Value`], not a generic struct.
//! - `DeepReadonly<T>`/`MaybeReadonly<T>` are no-ops in Rust because values
//!   are immutable-by-default.
//! - `RxJsonSchema` is a concrete struct with explicit fields for everything
//!   `rx-schema-helper.ts` reads/writes, plus a `#[serde(flatten)] extra` map
//!   for forward-compatibility with fields we haven't modelled yet.

pub mod checkpoint;
pub mod document;
pub mod hash;
pub mod internal_store;
pub mod query;
pub mod replication;
pub mod schema;
pub mod storage;
pub mod util;

pub use checkpoint::*;
pub use document::*;
pub use hash::*;
pub use internal_store::*;
pub use query::*;
pub use replication::*;
pub use schema::*;
pub use storage::*;
pub use util::*;
