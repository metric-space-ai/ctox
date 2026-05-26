// Generated from src/core/rxdb/tests/fixtures/webrtc-rxdb-protocol.json.
// Run: node src/core/rxdb/tools/build_webrtc_rxdb_protocol_contract.mjs

export const CTOX_RXDB_PROTOCOL = "ctox-rxdb-protocol-v1";
export const CTOX_PROTOCOL_PHASE = "rxdb-protocol-handshake";
export const CTOX_REQUIRED_PROTOCOL_CAPABILITIES = Object.freeze([
  "ctox-schema-hash-v1",
  "ctox-peer-session-v1",
  "ctox-checkpoint-epoch-v1"
]);
export const CTOX_PROTOCOL_ERROR_CODES = Object.freeze({
  protocolMissing: "ctox_rxdb_protocol_missing",
  protocolMismatch: "ctox_rxdb_protocol_mismatch",
  capabilityMissing: "ctox_rxdb_capability_missing",
  collectionMismatch: "ctox_rxdb_collection_mismatch",
  schemaVersionMismatch: "ctox_rxdb_schema_version_mismatch",
  schemaHashMismatch: "ctox_rxdb_schema_hash_mismatch"
});
export const CTOX_SCHEMA_HASH_SOURCES = Object.freeze({
  businessOsRegistry: "business-os-schema-hash-registry-v1",
  canonicalJson: "canonical-json-schema-sha256-v1",
  rxdbRs: "rxdb-rs-schema-hash-v1"
});
