//! Foundation utilities — port of `rxdb/src/plugins/utils/`.
//!
//! Mirrors the upstream barrel `src/plugins/utils/index.ts`. The skipped utils
//! (`utils-blob`, `utils-base64`, `utils-premium`, `utils-rxdb-version.template`)
//! are omitted from this re-export; see `PORTING.md` for the rationale.

// ref: rxdb/src/plugins/utils/index.ts (re-exports below mirror upstream order,
//      minus the four omitted modules)
pub mod utils_array;
pub mod utils_document;
pub mod utils_error;
pub mod utils_global;
pub mod utils_hash;
pub mod utils_map;
pub mod utils_number;
pub mod utils_object;
pub mod utils_object_deep_equal;
pub mod utils_object_dot_prop;
pub mod utils_other;
pub mod utils_promise;
pub mod utils_regex;
pub mod utils_revision;
pub mod utils_rxdb_version;
pub mod utils_string;
pub mod utils_time;
