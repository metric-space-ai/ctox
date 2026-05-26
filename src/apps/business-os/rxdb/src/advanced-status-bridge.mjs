// V1.5 Advanced-Status bridge: produces a `business-os-advanced-status-v1`
// envelope that the existing Business-OS UI / smoke harness consumes,
// with V1.5 query-demand-loading and file-streaming health folded into
// the well-known shape.

import { CTOX_QUERY_FETCH_CAPABILITY } from './protocol-contract.generated.mjs';
import { snapshotV1_5Status } from './v1_5_status.mjs';

export function buildBusinessOsAdvancedStatus({
  v15Status,
  peerSessions = [],
  remoteProtocol = null,
  feature = {},
} = {}) {
  const snapshot = snapshotV1_5Status(v15Status);
  const remoteCapabilities = Array.isArray(remoteProtocol?.capabilities)
    ? remoteProtocol.capabilities
    : [];
  const v15Negotiated = remoteCapabilities.includes(CTOX_QUERY_FETCH_CAPABILITY)
    && remoteProtocol?.v1_5?.queryDemandLoadingEnabled !== false;
  const ok =
    snapshot.peerConnected === true &&
    snapshot.queryFetchErrorCount < 5 &&
    snapshot.fileStreamErrors < 5;

  return {
    version: 'business-os-advanced-status-v1',
    ok,
    rxdbRuntime: {
      name: 'ctox-rxdb-js',
      source: 'app-local',
      packageManager: 'none',
      protocolVersion: snapshot.rxdbProtocolVersion,
    },
    checks: {
      rxdbRuntimeAppLocal: true,
      queryDemandLoadingEnabled: snapshot.queryDemandLoadingEnabled === true,
      queryDemandLoadingActive: snapshot.queryDemandLoadingActive === true,
      peerCapabilityQueryFetch: snapshot.peerCapabilityQueryFetchV1 === true,
    },
    sync: {
      mode: 'webrtc',
      protocol: 'ctox-rxdb-protocol-v1',
      capabilities: remoteCapabilities,
      peerSessions,
      featureFlag: feature.queryDemandLoadingEnabled ?? null,
      v15Negotiated,
    },
    v1_5: {
      query: {
        inFlight: snapshot.queryFetchInFlight,
        success: snapshot.queryFetchSuccessCount,
        errors: snapshot.queryFetchErrorCount,
        dedupHits: snapshot.queryFetchDedupHitCount,
        lastFetchMs: snapshot.lastQueryFetchMs,
      },
      file: {
        active: snapshot.activeFileStreams,
        bytesReceived: snapshot.fileBytesReceived,
        errors: snapshot.fileStreamErrors,
        dedupHits: snapshot.fileStreamDedupHits,
        lastFetchMs: snapshot.lastFileFetchMs,
      },
      cache: {
        workingSetBytes: snapshot.indexedDbWorkingSetBytes,
        evictionCount: snapshot.indexedDbEvictionCount,
        pinnedDocs: snapshot.pinnedDocCount,
        pinnedBytes: snapshot.pinnedBytes,
      },
      transport: {
        lastBackpressureMs: snapshot.lastTransportBackpressureMs,
        reloadHydrationMs: snapshot.lastReloadHydrationMs,
      },
    },
  };
}
