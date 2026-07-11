// =============================================================================
// AGENT GUARDRAILS — ctox-rxdb data plane (read docs/ctox-rxdb.md first)
// =============================================================================
// This file is part of CTOX Sync Engine, the WebRTC-ONLY data plane between Business OS
// and the CTOX daemon. Hard rules (each one has caused real regressions):
//   1. NO HTTP fallback/bridge for collection data — ever. WebRTC only.
//   2. NO npm/bare/node: imports — this runtime is package-manager-free.
//   3. After ANY src edit: rebuild dist with the pinned esbuild command and
//      bump the ?v= cache-buster (see docs/ctox-rxdb.md "Build & release").
//      Never patch dist/ctox-rxdb-js.mjs directly.
//   4. Wire-contract constants are GENERATED from fixtures — never hand-edit
//      *-contract.generated.mjs or the Rust twins.
//   5. Run `node src/apps/business-os/rxdb/tests/run-all.mjs` and keep it
//      green. Never delete or weaken a failing test to make it pass.
// =============================================================================

// esbuild entry point: every module reachable from here lands in the dist
// bundle the browser actually runs.
export {
  CTOX_CHECKPOINT_EPOCH_CAPABILITY,
  CTOX_BUSINESS_OS_SCHEMA_HASHES,
  CTOX_PEER_SESSION_CAPABILITY,
  CTOX_PROTOCOL_ERROR_CODES,
  CTOX_REQUIRED_PROTOCOL_CAPABILITIES,
  CTOX_RXDB_PROTOCOL,
  CTOX_SCHEMA_HASH_SOURCES,
  CTOX_SCHEMA_HASH_CAPABILITY,
  assertCompatibleProtocol,
  buildProtocolPayload,
  canonicalJson,
  schemaHash,
  schemaHashSource,
  sha256Hex,
} from './schema.mjs';

export {
  CtoxIndexedDbCollection,
  CtoxIndexedDbStorage,
  ctoxIndexedDbStorageTestInternals,
  openCtoxIndexedDbStorage,
} from './storage-indexeddb.mjs';

export {
  SHELL_CRITICAL_COLLECTIONS,
  CtoxWebRtcNativePeer,
  createCtoxWebRtcNativePeer,
  normalizeSignalingControlPlaneError,
} from './webrtc-native.mjs';

export {
  ACTIVE_COLLECTIONS_METHOD,
  DEFAULT_QUERY_META_BUDGET_BYTES,
  getConnectionHandlerSimplePeer,
  remoteSupportsQueryFetch,
  replicateWebRTC,
  replicationWebRtcTestInternals,
} from './replication-webrtc.mjs';

export {
  ACTIVE_NOTIFY_DEBOUNCE_MS,
  RECENT_EXEC_ACTIVE_MS,
  createActiveCollectionRegistry,
  getActiveCollectionRegistry,
} from './active-collections.mjs';

export {
  createMultiTabSyncCoordinator,
  getMultiTabSyncCoordinator,
  multiTabSyncCoordinatorTestInternals,
} from './multi-tab-sync-coordinator.mjs';

export {
  PRESENCE_NOTIFY_DEBOUNCE_MS,
  createPresenceRegistry,
  getPresenceRegistry,
} from './presence.mjs';

export {
  deepEqualJson,
  normalizeConflictStrategy,
  threeWayMergeDocuments,
} from './conflict-merge.mjs';

export {
  compareHybridLogicalClocks,
  correctedHybridLogicalClockNowMs,
  formatHybridLogicalClock,
  hybridLogicalClockNodeId,
  hybridLogicalClockStatus,
  isFutureHybridLogicalClock,
  nextHybridLogicalClock,
  parseHybridLogicalClock,
  setHybridLogicalClockTimeAnchor,
} from './hybrid-logical-clock.mjs';

export { CtoxEventEmitter, waitForEvent } from './event-target.mjs';
export { CtoxSubject } from './observable.mjs';

export {
  RxDBMigrationSchemaPlugin,
  addRxPlugin,
  createRxDatabase,
  ctoxRxdbTestInternals,
  getCtoxIndexedDbStorage,
  removeRxDatabase,
  rxdbCore,
} from './rx-database.mjs';

export {
  V1_5_QUERY_FETCH_CAPABILITY,
  V1_5_QUERY_RPC,
  V1_5_STATUS_FIELDS,
  createV1_5StatusState,
  projectStatusFromSidecar,
  snapshotV1_5Status,
} from './v1_5_status.mjs';

export {
  canonicalQueryJson,
  canonicalizeQueryInput,
  queryFingerprint,
} from './query-fingerprint.mjs';

export {
  SIDECAR_DATABASE_NAME,
  SIDECAR_PIN_RECENT_READ_TTL_MS,
  QueryMetaStorage,
  createSidecarWithMemoryBackend,
  recoverQueryMetaQuota,
} from './query-meta-storage.mjs';

export {
  CtoxRecoveryJournal,
  openRecoveryJournal,
  recoveryJournalTestInternals,
} from './recovery-journal.mjs';

export {
  decryptRecoveryArtifact,
  encryptRecoveryArtifact,
  recoveryCryptoTestInternals,
  sha256Json,
} from './recovery-crypto.mjs';

export { createMemoryMetaBackend } from './query-meta-backend-memory.mjs';
export { createIndexedDbMetaBackend } from './query-meta-backend-indexeddb.mjs';

export {
  DEFAULT_WINDOW_LIMIT,
  createQueryDemandLoader,
  setV15LogSink,
} from './query-demand-loader.mjs';

export {
  createBroadcastChannelBroker,
  createMemoryBroker,
} from './multi-tab-broker.mjs';

export { OBSERVABLE_DEBOUNCE_MS } from './rx-database.mjs';

export {
  FILE_CHUNK_PRESENCE_KEY,
  createFileDemandLoader,
} from './file-demand-loader.mjs';

export { decodeChunk } from './chunk-decoder.mjs';
export { buildBusinessOsAdvancedStatus } from './advanced-status-bridge.mjs';
export { createDemandLoadingTransport } from './demand-loading-transport.mjs';
