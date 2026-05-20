//! RxDB version constant. Upstream replaces this file in the
//! `npm run build:version` script; in Rust we keep the version as a `&'static str`.

// ref: rxdb/src/plugins/utils/utils-rxdb-version.ts:4
pub const RXDB_VERSION: &str = "16.20.0";
