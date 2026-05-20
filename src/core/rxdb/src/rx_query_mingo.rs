//! Port of `src/rx-query-mingo.ts`.
//!
//! Upstream lazily initialises a global mingo operator registry (`useOperators`
//! + `mingoInitDone` flag) on first `getMingoQuery` call. The Rust port (see
//! [`crate::util::mango`]) replaces the global mutable registry with a
//! per-`Query` `Context` that is populated by `util::mango::get_mingo_query`
//! itself, so this module is just a re-export.
//!
//! The exact operator set wired up matches upstream verbatim:
//! - logical: $and, $or, $nor, $not
//! - comparison: $eq, $ne, $gt, $gte, $lt, $lte, $in, $nin
//! - evaluation: $regex, $mod
//! - array: $elemMatch, $size
//! - element: $exists, $type
//! - pipeline: $sort, $project

// ref: rxdb/src/rx-query-mingo.ts:47-78
pub use crate::util::mango::{get_mingo_query, Query};
