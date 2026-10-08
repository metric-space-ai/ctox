import test from 'node:test';
import assert from 'node:assert/strict';
import { runInNewContext } from 'node:vm';
import { browserTransportSnapshot } from './installed-sync-browser-transport.mjs';

const snapshot = peer => runInNewContext('(' + browserTransportSnapshot.toString() + ')()',
  { __installedAcceptance: { state: { peer } } });
const plain = value => JSON.parse(JSON.stringify(value));

test('installed browser snapshot preserves handshake evidence without authority or addresses', () => {
  const secret = 'NEVER_EXPORT';
  const peerId = 'private-peer-id';
  const descriptor = { role: 'ctox_instance', capability_token: secret };
  const peer = {
    options: { expectedNativePeerId: peerId, signalingUrl: 'wss://secret.invalid/?token=' + secret },
    peerMetadata: new Map([[peerId, descriptor]]),
    peerMatchesExpectedNativePeerId: id => id === peerId,
    getTransportStatus(options) {
      assert.equal(options.includeDiagnostics, true);
      return { activePeerCount: 1, pendingRequests: 0, pendingAcks: 0,
        recentMessages: [{ token: secret, candidate: '192.0.2.1' }],
        rtcConnections: [{ peerId, ageMs: 15000, connectionState: 'connecting',
          iceConnectionState: 'checking', iceGatheringState: 'complete', signalingState: 'have-local-offer',
          channelReadyState: 'connecting', pendingCandidates: 0,
          hasLocalDescription: true, hasRemoteDescription: false,
          localCandidateTypes: { host: 2, address: '192.0.2.1' },
          remoteCandidateTypes: { relay: 1 },
          signal: { offerSent: 1, answerReceived: 0, candidateSent: 2, selectedLocalCandidateType: 'host',
            selectedRemoteCandidateType: 'relay', selectedCandidateProtocol: 'udp', token: secret },
          lastError: { message: secret }, sdp: secret }],
        recentRtcEvents: [{ event: 'handshake-timeout', atMs: 123, peerId,
          iceConnectionState: 'checking', message: secret }] };
    },
  };
  const result = plain(snapshot(peer));
  assert.equal(result.available, true);
  assert.equal(result.matchedNativeDescriptors, 1);
  assert.equal(result.connections[0].expectedNative, true);
  assert.equal(result.connections[0].iceConnectionState, 'checking');
  assert.equal(result.connections[0].signal.offerSent, 1);
  assert.equal(result.connections[0].signal.answerReceived, 0);
  assert.equal(result.connections[0].hasRemoteDescription, false);
  assert.equal(result.recentRtcEvents[0].event, 'handshake-timeout');
  for (const forbidden of [secret, peerId, 'secret.invalid', '192.0.2.1', 'capability_token', 'sdp'])
    assert.equal(JSON.stringify(result).includes(forbidden), false);
});

test('transport export bounds collections and rejects unexpected state strings and malformed counters', () => {
  const peer = {
    options: {}, peerMetadata: new Map(Array.from({ length: 100 }, (_, i) => [i, {}])),
    getTransportStatus: () => ({
      activePeerCount: Infinity, pendingRequests: '42',
      rtcConnections: Array.from({ length: 100 }, () => ({
        connectionState: 'NEVER_EXPORT', iceConnectionState: 'checking',
        ageMs: -1, signal: { offerSent: 'NEVER_EXPORT' },
      })),
      recentRtcEvents: Array.from({ length: 100 }, () => ({
        event: 'created', state: 'NEVER_EXPORT', atMs: 1, peerId: 'NEVER_EXPORT',
      })),
    }),
  };
  const result = plain(snapshot(peer));
  assert.equal(result.metadataScanned, 64);
  assert.equal(result.connections.length, 8);
  assert.equal(result.connectionsTruncated, true);
  assert.equal(result.recentRtcEvents.length, 24);
  assert.equal(result.activePeerCount, null);
  assert.equal(result.pendingRequests, null);
  assert.equal(result.connections[0].ageMs, null);
  assert.equal(result.connections[0].expectedNative, null);
  assert.equal(JSON.stringify(result).includes('NEVER_EXPORT'), false);
});

test('missing and throwing browser transports stay explicitly unavailable without exporting errors', () => {
  assert.deepEqual(plain(snapshot(null)), { available: false, reason: 'transport-unavailable' });
  const result = snapshot({ getTransportStatus() { throw new Error('Bearer NEVER_EXPORT'); } });
  assert.deepEqual(plain(result), { available: false, reason: 'transport-snapshot-failed' });
});
