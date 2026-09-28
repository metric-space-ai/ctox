import {
  compareHybridLogicalClocks,
  clearHybridLogicalClockTimeAnchor,
  correctedHybridLogicalClockNowMs,
  ctoxIndexedDbStorageTestInternals,
  formatHybridLogicalClock,
  nextHybridLogicalClock,
  parseHybridLogicalClock,
  hybridLogicalClockStatus,
  isFutureHybridLogicalClock,
  setHybridLogicalClockTimeAnchor,
  setHybridLogicalClockTimeAnchorFromRoundTrip,
} from '../dist/ctox-rxdb-js.mjs';

const assert = (condition, message) => {
  if (!condition) throw new Error(message);
};

const first = nextHybridLogicalClock(null, { nowMs: 1_000, nodeId: 'tab-a' });
const second = nextHybridLogicalClock(first, { nowMs: 900, nodeId: 'tab-a' });
assert(compareHybridLogicalClocks(second, first) > 0,
  'logical component advances when the wall clock moves backwards');
assert(parseHybridLogicalClock(second)?.logical === 1,
  'backward wall-clock write increments the logical counter');

const deviceA = formatHybridLogicalClock({ physicalMs: 2_000, logical: 0, nodeId: 'tab-a' });
const deviceB = formatHybridLogicalClock({ physicalMs: 2_000, logical: 0, nodeId: 'tab-b' });
assert(compareHybridLogicalClocks(deviceA, deviceB) < 0,
  'node id deterministically resolves otherwise-equal cross-device clocks');
assert(compareHybridLogicalClocks(deviceB, deviceA) > 0,
  'HLC ordering is antisymmetric');
assert(compareHybridLogicalClocks('', deviceA) < 0,
  'mixed-version state with a valid HLC outranks missing clock metadata');

// SYNC-11: the browser PULL gate orders the local-veto decision by the same
// HLC comparison the push conflict path uses — one ordering for both
// directions, so relay-vs-push interleaving cannot flip an LWW winner.
// (Kept before the skew-anchor mutation below: the gate consults the global
// corrected clock for skew classification.)
{
  const { shouldAcceptDocumentWrite } = ctoxIndexedDbStorageTestInternals;
  const origin = { role: 'ctox_instance', peerId: 'peer-native' };
  const localRow = (lwt, ctoxHlc) => ({ lwt, doc: { id: 'doc-1', _meta: { lwt, ctoxHlc } } });
  const masterRow = (ctoxHlc) => ({ id: 'doc-1', _meta: { ctoxHlc } });
  const older = formatHybridLogicalClock({ physicalMs: 3_000, logical: 0, nodeId: 'tab-a' });
  const newer = formatHybridLogicalClock({ physicalMs: 4_000, logical: 0, nodeId: 'native' });
  assert(shouldAcceptDocumentWrite(localRow(100, newer), 9_000, origin, masterRow(older)) === false,
    'pull gate: an HLC-newer local unsynced edit vetoes an HLC-older master row despite newer wall-clock lwt');
  assert(shouldAcceptDocumentWrite(localRow(9_000, older), 100, origin, masterRow(newer)) === true,
    'pull gate: an HLC-newer master row wins over an HLC-older local edit despite older wall-clock lwt');
  assert(shouldAcceptDocumentWrite(localRow(9_000, older), 100, origin, masterRow(older)) === true,
    'pull gate: an equal HLC (own push echo) is accepted');
}

setHybridLogicalClockTimeAnchor(601_000, 1_000);
assert(correctedHybridLogicalClockNowMs(2_000) === 602_000,
  'browser HLC time must apply the measured native offset');
assert(hybridLogicalClockStatus().code === 'clock_skew_detected',
  'offsets above five minutes must surface a typed skew status');
assert(isFutureHybridLogicalClock(formatHybridLogicalClock({ physicalMs: 400_001, nodeId: 'future' }), 10_000),
  'a strongly future clock must be classified for durable conflict handling');

const sampleStart = 1_722_000_000_000;
const fresh = setHybridLogicalClockTimeAnchorFromRoundTrip(sampleStart + 100, sampleStart, sampleStart + 200, 200);
assert(fresh.nativeClockOffsetMs === 0 && fresh.code === null,
  'a fresh native round trip must replace an earlier skewed time anchor');
const delayed = setHybridLogicalClockTimeAnchorFromRoundTrip(
  sampleStart + 100, sampleStart, sampleStart + 16 * 60 * 1000, 16 * 60 * 1000,
);
assert(delayed.nativeClockOffsetMs === 0 && delayed.code === null,
  'a delayed protocol answer must not turn transport latency into clock skew');
const changedWallClock = setHybridLogicalClockTimeAnchorFromRoundTrip(
  sampleStart + 100, sampleStart, sampleStart + 60_200, 200,
);
assert(changedWallClock.nativeClockOffsetMs === 0,
  'a wall-clock adjustment during the round trip must not become a time anchor');
const actualSkew = setHybridLogicalClockTimeAnchorFromRoundTrip(
  sampleStart - 6 * 60 * 1000 + 100, sampleStart, sampleStart + 200, 200,
);
assert(actualSkew.code === 'clock_skew_detected',
  'a fresh round trip must still detect real clock skew');

const peerA = 'room-a:connection-a';
const peerB = 'room-a:connection-b';
setHybridLogicalClockTimeAnchorFromRoundTrip(
  sampleStart + 60_100, sampleStart, sampleStart + 200, 200, peerA,
);
assert(hybridLogicalClockStatus().nativeClockOffsetMs === 60_000,
  'the authenticated first peer provides its own clock offset');
const rejectedPeerB = setHybridLogicalClockTimeAnchorFromRoundTrip(
  sampleStart + 60_100, sampleStart, sampleStart + 20_000, 20_000, peerB,
);
assert(rejectedPeerB.nativeClockOffsetMs === 0 && rejectedPeerB.nativeClockSource === null,
  'a rejected new-peer sample must not reuse the previous peer offset');
setHybridLogicalClockTimeAnchorFromRoundTrip(
  sampleStart + 100, sampleStart, sampleStart + 200, 200, peerB,
);
clearHybridLogicalClockTimeAnchor(peerA);
assert(hybridLogicalClockStatus().nativeClockSource === peerB,
  'closing an old peer must not erase the replacement peer anchor');
clearHybridLogicalClockTimeAnchor(peerB);

console.log('ctox-rxdb HLC conflict smoke OK');
