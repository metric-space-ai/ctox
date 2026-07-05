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
pub(super) const CTOX_QUERY_FETCH_CAPABILITY: &str = "ctox-rxdb-query-fetch-v1";
pub(super) const CTOX_QUERY_RPC_FETCH: &str = "rxdb.query.fetch";
pub(super) const CTOX_QUERY_RPC_CHUNK: &str = "rxdb.query.chunk";
pub(super) const CTOX_QUERY_RPC_ERROR: &str = "rxdb.query.error";
pub(super) const CTOX_QUERY_RPC_CANCEL: &str = "rxdb.query.cancel";
pub(super) const CTOX_QUERY_MAX_DOCUMENTS_PER_CHUNK: u32 = 200;
pub(super) const CTOX_QUERY_MAX_BYTES_PER_CHUNK: u32 = 262144;
pub(super) const CTOX_QUERY_MAX_IN_FLIGHT_STREAMS: u32 = 8;
pub(super) const CTOX_QUERY_MAX_RUNTIME_MS: u32 = 30000;
pub(super) const CTOX_QUERY_DEFAULT_WINDOW_LIMIT: u32 = 200;
pub(super) const CTOX_FILE_RPC_FETCH: &str = "rxdb.file.fetch";
pub(super) const CTOX_FILE_RPC_CHUNK: &str = "rxdb.file.chunk";
pub(super) const CTOX_FILE_RPC_ERROR: &str = "rxdb.file.error";
pub(super) const CTOX_FILE_RPC_CANCEL: &str = "rxdb.file.cancel";
pub(super) const CTOX_FILE_MAX_BYTES_PER_CHUNK: u32 = 262144;
pub(super) const CTOX_PRESENCE_CAPABILITY: &str = "ctox-presence-v1";
pub(super) const CTOX_PRESENCE_RPC_UPDATE: &str = "rxdb.presence.update";
pub(super) const CTOX_PRESENCE_STREAM_ID: &str = "presence$";
pub(super) const CTOX_PRESENCE_TTL_MS: u64 = 45000;
pub(super) const CTOX_PRESENCE_MAX_ENTRIES_PER_PEER: usize = 32;
