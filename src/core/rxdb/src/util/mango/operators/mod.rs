//! Mingo operators — port of the subset used by
//! `vendor/rxdb-16.20.0/src/rx-query-mingo.ts`.
//!
//! The module layout mirrors upstream:
//! - `operators/_predicates.ts` → `predicates.rs` (shared predicate impls and
//!   the `create_query_operator` factory)
//! - `operators/query/{comparison,logical,array,element,evaluation}` →
//!   `{comparison,logical,array,element,evaluation}.rs`
//! - `operators/pipeline/{sort,project}` → `pipeline.rs`

pub mod array;
pub mod comparison;
pub mod element;
pub mod evaluation;
pub mod logical;
pub mod pipeline;
pub mod predicates;
