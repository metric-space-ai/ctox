//! Minimal Rust port of the mingo MongoDB-style query library (v6.5.6).
//!
//! The port is intentionally restricted to the subset that
//! `vendor/rxdb-16.20.0/src/rx-query-mingo.ts` actually wires up. Anything else
//! that mingo upstream supports ($where, geospatial, aggregation beyond
//! $sort/$project, etc.) is out of scope.
//!
//! Upstream source: `vendor/mingo-6.5.6/`. Anchors use the prefix
//! `mingo/<upstream-path>` as required by `PORT_STYLE.md` §7.
//!
//! ## Differences from upstream that are intentional and documented
//!
//! - mingo operates on opaque `Any` values (`unknown` in TS). The Rust port
//!   operates on `serde_json::Value` end-to-end, because that is what
//!   `ctox-rxdb` uses for document data and `rx-query-mingo` is itself a JSON
//!   query layer. Date / RegExp / typed-array branches of upstream that cannot
//!   appear inside a JSON document are pruned. The `$type` operator therefore
//!   only recognises BSON types that map to JSON shapes (`number`, `string`,
//!   `bool`, `null`, `array`, `object`); `date`, `regexp`, etc. always return
//!   `false`.
//! - mingo's global mutable operator registry (`useOperators` + module-level
//!   `mingoInitDone` flag in rx-query-mingo.ts) is replaced by a per-`Query`
//!   `Context` populated once via `Query::new`. The public `get_mingo_query`
//!   constructor wires up the same operator set that rx-query-mingo.ts pins.
//! - The `Cursor` lazy-iterator type from upstream is collapsed to a strict
//!   `Vec` in `Query::find` because the only call site in CTOX
//!   (`rx-query-mingo.ts → Query::find` consumers) materialises results
//!   immediately.

pub mod core;
pub mod operators;
pub mod query;

// ref: rxdb/src/rx-query-mingo.ts:47-78
pub use self::query::{get_mingo_query, sort_documents, MangoSelector, Query};
