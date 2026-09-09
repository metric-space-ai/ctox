// Executes the shell's actual readiness mapping, without importing its DOM bootstrap.
// The old HTTP-bridge diagnostics must never satisfy the WebRTC data contract.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

const source = fs.readFileSync(new URL('../../app.js', import.meta.url), 'utf8');
const start = source.indexOf('function sanitizeAdvancedStatusRemoteCheckpoint(');
const end = source.indexOf('async function collectAdvancedStatusCounts(', start);
assert(start >= 0 && end > start, 'shell status implementation boundary missing');
const context = vm.createContext({});
vm.runInContext(source.slice(start, end), context);
const collection = 'business_commands';
const evidence = { hasCollection: true, hasData: true };
const legacy = {
  httpBridgeStatus: 'ready', httpBridgePulledAt: '2026-09-08T00:00:00.000Z',
  initialReplicationState: 'pending', connectionStatus: 'error',
  frameTransport: { activePeerCount: 0 },
};
assert.equal(context.hasAdvertisedCheckpointEpoch(legacy), false,
  'obsolete HTTP receipt cannot substitute for a negotiated checkpoint epoch');
assert.equal(context.isRequiredCollectionStreamingReady(legacy), false,
  'obsolete HTTP receipt cannot make a disconnected stream live');
assert.equal(context.isRequiredCollectionReady({ collection, diagnostics: legacy, evidence }), false,
  'cached documents plus obsolete HTTP receipt cannot make an errored collection ready');
const initial = context.buildAdvancedStatusInitialSync([collection], { [collection]: legacy });
assert.equal(initial.completedTotal, 0);
assert.equal(initial.entries[0].initialReplicationAt, null);
assert.equal(initial.entries[0].checkpointEpochAdvertised, false);
assert.equal(initial.entries[0].streamingReady, false);
assert.equal(initial.entries[0].source, null);

const native = {
  initialReplicationAt: '2026-09-08T01:00:00.000Z',
  initialReplicationState: 'complete', connectionStatus: 'connected',
  initialReplicationSource: 'webrtc', frameTransport: { activePeerCount: 1 },
  remoteCapabilities: ['ctox-checkpoint-epoch-v1'],
  remoteCheckpoint: { state: 'advertised', epoch: 'native-fixture-epoch' },
};
assert.equal(context.hasAdvertisedCheckpointEpoch(native), true);
assert.equal(context.isRequiredCollectionStreamingReady(native), true);
assert.equal(context.isRequiredCollectionReady({ collection, diagnostics: native, evidence }), true);
const complete = context.buildAdvancedStatusInitialSync([collection], { [collection]: native });
assert.equal(complete.completedTotal, 1);
assert.equal(complete.entries[0].source, 'webrtc');
assert.equal(complete.entries[0].checkpointEpoch, 'native-fixture-epoch');
assert.equal(complete.entries[0].streamingReady, true);
assert.equal(context.isRequiredCollectionStreamingReady({
  ...native, frameTransport: { activePeerCount: 0 }, ...legacy,
}), false);
assert.equal(context.hasAdvertisedCheckpointEpoch({
  ...native, ...legacy, remoteCheckpoint: { state: 'missing', epoch: null },
}), false);

console.log('shell WebRTC-only readiness regression OK');
