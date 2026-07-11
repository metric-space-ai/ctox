// V1.5 status surface. Filled progressively by later waves.
// Values reflect the V1-only baseline until Wave 1 lights up the capability
// and Wave 3 wires demand-loading into the query path. Do NOT widen the
// schema without bumping the field list documented in docs/rxdb_on-demand-load.md.

import { CTOX_QUERY_FETCH_CAPABILITY, CTOX_QUERY_RPC } from './protocol-contract.generated.mjs';

export const V1_5_QUERY_FETCH_CAPABILITY = CTOX_QUERY_FETCH_CAPABILITY;
export const V1_5_QUERY_RPC = CTOX_QUERY_RPC;

export const V1_5_STATUS_FIELDS = Object.freeze([
  'rxdbRuntime',
  'rxdbProtocolVersion',
  'transport',
  'peerConnected',
  'peerCapabilityQueryFetchV1',
  'queryDemandLoadingEnabled',
  'queryDemandLoadingActive',
  'queryFetchInFlight',
  'pendingQueryFetchCollectors',
  'queuedQueryFetchRequests',
  'maxPendingQueryFetchCollectors',
  'queryFetchSuccessCount',
  'queryFetchErrorCount',
  'queryFetchDedupHitCount',
  'indexedDbWorkingSetBytes',
  'indexedDbEvictionCount',
  'pinnedDocCount',
  'pinnedBytes',
  'lastQueryFetchMs',
  'lastTransportBackpressureMs',
  'lastReloadHydrationMs',
  'activeFileStreams',
  'pendingFileFetchCollectors',
  'maxPendingFileFetchCollectors',
  'fileBytesReceived',
  'fileStreamErrors',
  'fileStreamDedupHits',
  'lastFileFetchMs',
  'localPushChangedSinceCalls',
  'localPushChangedSinceScannedRows',
  'localPushChangedSinceScanLimitHits',
  'localPushChangedSinceMaxScannedRows',
  'clockSkewDetected',
  'nativeClockOffsetMs',
  'nativeClockObservedAtMs',
  'code',
]);

export function createV1_5StatusState() {
  return {
    rxdbRuntime: 'ctox-rxdb-js',
    rxdbProtocolVersion: '1',
    transport: 'webrtc',
    peerConnected: false,
    peerCapabilityQueryFetchV1: false,
    queryDemandLoadingEnabled: false,
    queryDemandLoadingActive: false,
    queryFetchInFlight: 0,
    pendingQueryFetchCollectors: 0,
    queuedQueryFetchRequests: 0,
    maxPendingQueryFetchCollectors: 0,
    queryFetchSuccessCount: 0,
    queryFetchErrorCount: 0,
    queryFetchDedupHitCount: 0,
    indexedDbWorkingSetBytes: 0,
    indexedDbEvictionCount: 0,
    pinnedDocCount: 0,
    pinnedBytes: 0,
    lastQueryFetchMs: null,
    lastTransportBackpressureMs: null,
    lastReloadHydrationMs: null,
    activeFileStreams: 0,
    pendingFileFetchCollectors: 0,
    maxPendingFileFetchCollectors: 0,
    fileBytesReceived: 0,
    fileStreamErrors: 0,
    fileStreamDedupHits: 0,
    lastFileFetchMs: null,
    localPushChangedSinceCalls: 0,
    localPushChangedSinceScannedRows: 0,
    localPushChangedSinceScanLimitHits: 0,
    localPushChangedSinceMaxScannedRows: 0,
    clockSkewDetected: false,
    nativeClockOffsetMs: 0,
    nativeClockObservedAtMs: null,
    code: null,
  };
}

export function projectStatusFromSidecar(state, sidecarStats, registry = null) {
  const next = { ...state };
  if (sidecarStats) {
    next.indexedDbWorkingSetBytes = sidecarStats.estimatedBytes || 0;
  }
  if (registry?.pinnedDocCount !== undefined) next.pinnedDocCount = registry.pinnedDocCount;
  if (registry?.pinnedBytes !== undefined) next.pinnedBytes = registry.pinnedBytes;
  return next;
}

export function snapshotV1_5Status(state) {
  const snapshot = {};
  for (const field of V1_5_STATUS_FIELDS) {
    snapshot[field] = state?.[field] ?? null;
  }
  return snapshot;
}
