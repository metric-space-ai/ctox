// Verifies the V1.5 → business-os-advanced-status-v1 mapping. Specifically:
//   - envelope shape matches what browser_rust_smoke.js expects
//   - V1.5-specific fields are correctly aggregated
//   - status.ok reflects healthy + unhealthy peer states

import {
  V1_5_QUERY_FETCH_CAPABILITY,
  buildBusinessOsAdvancedStatus,
  createV1_5StatusState,
} from '../dist/ctox-rxdb-js.mjs';

// === Healthy state ===
const healthy = createV1_5StatusState();
healthy.peerConnected = true;
healthy.peerCapabilityQueryFetchV1 = true;
healthy.queryDemandLoadingEnabled = true;
healthy.queryDemandLoadingActive = true;
healthy.queryFetchSuccessCount = 17;
healthy.localPushChangedSinceCalls = 3;
healthy.localPushChangedSinceScannedRows = 301;
healthy.localPushChangedSinceScanLimitHits = 1;
healthy.localPushChangedSinceMaxScannedRows = 300;

const env = buildBusinessOsAdvancedStatus({
  v15Status: healthy,
  peerSessions: [{ role: 'ctox_instance', sessionId: 'native:test' }],
  remoteProtocol: {
    capabilities: ['ctox-peer-session-v1', V1_5_QUERY_FETCH_CAPABILITY],
    v1_5: { queryDemandLoadingEnabled: true },
  },
  feature: { queryDemandLoadingEnabled: true },
});

// Shape match against browser_rust_smoke.js expectations.
assert(env.version === 'business-os-advanced-status-v1', 'version match');
assert(env.ok === true, 'healthy peer is ok');
assert(env.sync.mode === 'webrtc', 'webrtc mode');
assert(env.sync.protocol === 'ctox-rxdb-protocol-v1', 'protocol id');
assert(env.sync.capabilities.includes('ctox-peer-session-v1'), 'peer-session cap');
assert(Array.isArray(env.sync.peerSessions), 'peerSessions is array');
assert(env.checks.rxdbRuntimeAppLocal === true, 'app-local check on');
assert(env.checks.queryDemandLoadingActive === true, 'V1.5 active');
assert(env.rxdbRuntime.name === 'ctox-rxdb-js', 'runtime name');
assert(env.rxdbRuntime.publicName === 'CTOX Sync Engine', 'public runtime name');
assert(env.rxdbRuntime.source === 'app-local', 'app-local source');
assert(env.rxdbRuntime.packageManager === 'none', 'no package manager');
assert(env.rxdbRuntime.apiContract === 'ctox-db-business-os-v1', 'api contract');
assert(env.rxdbRuntime.upstreamCompatible === false, 'not upstream compatible');
assert(env.rxdbRuntime.upstreamCompatibility === 'not-upstream-rxdb', 'upstream marker');
assert(env.v1_5.query.success === 17, 'success count flows through');
assert(env.v1_5.localPush.changedSinceCalls === 3, 'local push changed-since calls flow through');
assert(env.v1_5.localPush.scannedRows === 301, 'local push scanned rows flow through');
assert(env.v1_5.localPush.scanLimitHits === 1, 'local push scan-limit hits flow through');
assert(env.v1_5.localPush.maxScannedRows === 300, 'local push max scanned rows flow through');
assert(env.sync.v15Negotiated === true, 'V1.5 negotiation reflected in sync block');

// === Degraded state: too many errors → not ok ===
const degraded = createV1_5StatusState();
degraded.peerConnected = true;
degraded.queryFetchErrorCount = 10;
const env2 = buildBusinessOsAdvancedStatus({ v15Status: degraded });
assert(env2.ok === false, 'high error count must flip ok to false');

// === V1-only peer (no V1.5 capability) ===
const v1Only = createV1_5StatusState();
v1Only.peerConnected = true;
const env3 = buildBusinessOsAdvancedStatus({
  v15Status: v1Only,
  remoteProtocol: { capabilities: ['ctox-peer-session-v1'] },
});
assert(env3.sync.v15Negotiated === false, 'no V1.5 capability → not negotiated');
assert(env3.checks.queryDemandLoadingActive === false, 'V1.5 inactive when peer is V1');

console.log('ctox-rxdb-js advanced-status bridge smoke OK', {
  ok: env.ok,
  v15Negotiated: env.sync.v15Negotiated,
  v15: env.v1_5.query.success,
});

function assert(c, m) { if (!c) throw new Error(m); }
