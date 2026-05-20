//! Port of `src/rx-document-prototype-merge.ts`.
//!
//! **T1 deviation: skipped.**
//!
//! Upstream merges three JS prototype objects (schemaProto + ormProto +
//! basePrototype) via `Object.defineProperty` with custom getters that do
//! `.bind(this)`. This pattern has no Rust equivalent — Rust has no
//! prototype chain, and ORM methods become `impl` blocks on a type rather
//! than runtime-attached property descriptors.
//!
//! The CTOX port handles the equivalent surface differently:
//! - ORM methods on documents: user code adds an `impl` block on their
//!   document type. CTOX storage is `serde_json::Value`-based, so user types
//!   are constructed via deserialization on top of raw doc data.
//! - Document construction: lives inline in the future `rx_collection.rs`
//!   (phase-6), not in a separate prototype-merge module.
//!
//! Functions that conceptually live here (`get_document_prototype`,
//! `get_rx_document_constructor`, `create_new_rx_document`,
//! `get_document_orm_prototype`) are all subsumed by direct Rust impls and
//! free functions in `rx_document.rs` / `rx_collection.rs` when those land.

// Intentionally empty.
