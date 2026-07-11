#!/usr/bin/env node
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const toolDir = dirname(fileURLToPath(import.meta.url));
const rxdbRoot = resolve(toolDir, '..');
const repoRoot = resolve(rxdbRoot, '..', '..');
const fixturePath = resolve(rxdbRoot, 'tests/fixtures/webrtc-rxdb-protocol.json');
const jsPath = resolve(repoRoot, 'apps/business-os/rxdb/src/protocol-contract.generated.mjs');
const rustPath = resolve(rxdbRoot, 'src/plugins/replication_webrtc/protocol_contract_generated.rs');
const fixture = JSON.parse(readFileSync(fixturePath, 'utf8'));

const queryRpc = fixture.queryRpc || {};
const fileRpc = fixture.fileRpc || {};
const presenceRpc = fixture.presenceRpc || {};
const queryFetchCapability = fixture.optionalCapabilities?.queryFetch || 'ctox-rxdb-query-fetch-v1';
const presenceCapability = fixture.optionalCapabilities?.presence || 'ctox-presence-v1';
const commandLifecycleCapability = fixture.optionalCapabilities?.commandLifecycle
  || 'ctox-command-lifecycle-v2';
const checkpointGenerationCapability = fixture.optionalCapabilities?.checkpointGeneration
  || 'ctox-checkpoint-generation-v2';

const js = `// Generated from src/core/rxdb/tests/fixtures/webrtc-rxdb-protocol.json.
// Run: node src/core/rxdb/tools/build_webrtc_rxdb_protocol_contract.mjs

export const CTOX_RXDB_PROTOCOL = ${json(fixture.protocol)};
export const CTOX_PROTOCOL_PHASE = ${json(fixture.phase)};
export const CTOX_REQUIRED_PROTOCOL_CAPABILITIES = Object.freeze(${json(fixture.requiredCapabilities)});
export const CTOX_PROTOCOL_ERROR_CODES = Object.freeze(${json(fixture.errorCodes)});
export const CTOX_SCHEMA_HASH_SOURCES = Object.freeze(${json(fixture.schemaHashSources)});
export const CTOX_QUERY_FETCH_CAPABILITY = ${json(queryFetchCapability)};
export const CTOX_QUERY_RPC = Object.freeze(${json(queryRpc)});
export const CTOX_FILE_RPC = Object.freeze(${json(fileRpc)});
export const CTOX_PRESENCE_CAPABILITY = ${json(presenceCapability)};
export const CTOX_PRESENCE_RPC = Object.freeze(${json(presenceRpc)});
export const CTOX_COMMAND_LIFECYCLE_CAPABILITY = ${json(commandLifecycleCapability)};
export const CTOX_CHECKPOINT_GENERATION_CAPABILITY = ${json(checkpointGenerationCapability)};
`;

const rust = `// Generated from src/core/rxdb/tests/fixtures/webrtc-rxdb-protocol.json.
// Run: node src/core/rxdb/tools/build_webrtc_rxdb_protocol_contract.mjs

pub(super) const CTOX_RXDB_PROTOCOL: &str = ${rustString(fixture.protocol)};
pub(super) const CTOX_REQUIRED_PROTOCOL_CAPABILITIES: &[&str] = &[
${fixture.requiredCapabilities.map((value) => `    ${rustString(value)},`).join('\n')}
];
pub(super) const CTOX_PROTOCOL_ERROR_MISSING: &str = ${rustString(fixture.errorCodes.protocolMissing)};
pub(super) const CTOX_PROTOCOL_ERROR_MISMATCH: &str = ${rustString(fixture.errorCodes.protocolMismatch)};
pub(super) const CTOX_PROTOCOL_ERROR_CAPABILITY_MISSING: &str = ${rustString(fixture.errorCodes.capabilityMissing)};
pub(super) const CTOX_PROTOCOL_ERROR_COLLECTION_MISMATCH: &str = ${rustString(fixture.errorCodes.collectionMismatch)};
pub(super) const CTOX_PROTOCOL_ERROR_SCHEMA_VERSION_MISMATCH: &str =
    ${rustString(fixture.errorCodes.schemaVersionMismatch)};
pub(super) const CTOX_PROTOCOL_ERROR_SCHEMA_HASH_MISMATCH: &str = ${rustString(fixture.errorCodes.schemaHashMismatch)};
pub(super) const CTOX_RXDB_RS_SCHEMA_HASH_SOURCE: &str = ${rustString(fixture.schemaHashSources.rxdbRs)};
pub(super) const CTOX_QUERY_FETCH_CAPABILITY: &str = ${rustString(queryFetchCapability)};
pub(super) const CTOX_QUERY_RPC_FETCH: &str = ${rustString(queryRpc.fetch || 'rxdb.query.fetch')};
pub(super) const CTOX_QUERY_RPC_CHUNK: &str = ${rustString(queryRpc.chunk || 'rxdb.query.chunk')};
pub(super) const CTOX_QUERY_RPC_ERROR: &str = ${rustString(queryRpc.error || 'rxdb.query.error')};
pub(super) const CTOX_QUERY_RPC_CANCEL: &str = ${rustString(queryRpc.cancel || 'rxdb.query.cancel')};
pub(super) const CTOX_QUERY_MAX_DOCUMENTS_PER_CHUNK: u32 = ${queryRpc.maxDocumentsPerChunk ?? 200};
pub(super) const CTOX_QUERY_MAX_BYTES_PER_CHUNK: u32 = ${queryRpc.maxBytesPerChunk ?? 262144};
pub(super) const CTOX_QUERY_MAX_IN_FLIGHT_STREAMS: u32 = ${queryRpc.maxInFlightStreams ?? 4};
pub(super) const CTOX_QUERY_MAX_RUNTIME_MS: u32 = ${queryRpc.maxQueryRuntimeMs ?? 30000};
pub(super) const CTOX_QUERY_DEFAULT_WINDOW_LIMIT: u32 = ${queryRpc.defaultWindowLimit ?? 200};
pub(super) const CTOX_FILE_RPC_FETCH: &str = ${rustString(fileRpc.fetch || 'rxdb.file.fetch')};
pub(super) const CTOX_FILE_RPC_CHUNK: &str = ${rustString(fileRpc.chunk || 'rxdb.file.chunk')};
pub(super) const CTOX_FILE_RPC_ERROR: &str = ${rustString(fileRpc.error || 'rxdb.file.error')};
pub(super) const CTOX_FILE_RPC_CANCEL: &str = ${rustString(fileRpc.cancel || 'rxdb.file.cancel')};
pub(super) const CTOX_FILE_MAX_BYTES_PER_CHUNK: u32 = ${fileRpc.maxBytesPerChunk ?? 262144};
pub(super) const CTOX_PRESENCE_CAPABILITY: &str = ${rustString(presenceCapability)};
pub(super) const CTOX_PRESENCE_RPC_UPDATE: &str = ${rustString(presenceRpc.update || 'rxdb.presence.update')};
pub(super) const CTOX_PRESENCE_STREAM_ID: &str = ${rustString(presenceRpc.streamId || 'presence$')};
pub(super) const CTOX_PRESENCE_TTL_MS: u64 = ${presenceRpc.ttlMs ?? 45000};
pub(super) const CTOX_PRESENCE_MAX_ENTRIES_PER_PEER: usize = ${presenceRpc.maxEntriesPerPeer ?? 32};
#[allow(dead_code)]
pub(super) const CTOX_COMMAND_LIFECYCLE_CAPABILITY: &str = ${rustString(commandLifecycleCapability)};
pub(super) const CTOX_CHECKPOINT_GENERATION_CAPABILITY: &str = ${rustString(checkpointGenerationCapability)};
`;

writeFileSync(jsPath, js);
writeFileSync(rustPath, rust);
console.log(`wrote ${jsPath}`);
console.log(`wrote ${rustPath}`);

function json(value) {
  return JSON.stringify(value, null, 2)
    .replace(/\n/g, '\n')
    .replace(/"([^"]+)":/g, '$1:');
}

function rustString(value) {
  return JSON.stringify(String(value));
}
