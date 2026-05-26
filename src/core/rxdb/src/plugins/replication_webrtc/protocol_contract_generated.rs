// Generated from src/core/rxdb/tests/fixtures/webrtc-rxdb-protocol.json.
// Run: node src/core/rxdb/tools/build_webrtc_rxdb_protocol_contract.mjs

pub(super) const CTOX_RXDB_PROTOCOL: &str = "ctox-rxdb-protocol-v1";
pub(super) const CTOX_REQUIRED_PROTOCOL_CAPABILITIES: &[&str] = &[
    "ctox-schema-hash-v1",
    "ctox-peer-session-v1",
    "ctox-checkpoint-epoch-v1",
];
pub(super) const CTOX_PROTOCOL_ERROR_MISSING: &str = "ctox_rxdb_protocol_missing";
pub(super) const CTOX_PROTOCOL_ERROR_MISMATCH: &str = "ctox_rxdb_protocol_mismatch";
pub(super) const CTOX_PROTOCOL_ERROR_CAPABILITY_MISSING: &str = "ctox_rxdb_capability_missing";
pub(super) const CTOX_PROTOCOL_ERROR_COLLECTION_MISMATCH: &str = "ctox_rxdb_collection_mismatch";
pub(super) const CTOX_PROTOCOL_ERROR_SCHEMA_VERSION_MISMATCH: &str =
    "ctox_rxdb_schema_version_mismatch";
pub(super) const CTOX_PROTOCOL_ERROR_SCHEMA_HASH_MISMATCH: &str = "ctox_rxdb_schema_hash_mismatch";
pub(super) const CTOX_RXDB_RS_SCHEMA_HASH_SOURCE: &str = "rxdb-rs-schema-hash-v1";
