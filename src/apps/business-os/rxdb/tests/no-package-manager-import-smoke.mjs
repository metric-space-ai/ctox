import {
  CTOX_RXDB_PROTOCOL,
  CTOX_BUSINESS_OS_SCHEMA_HASHES,
  CTOX_PROTOCOL_ERROR_CODES,
  CTOX_REQUIRED_PROTOCOL_CAPABILITIES,
  assertCompatibleProtocol,
  buildProtocolPayload,
  canonicalJson,
  createRxDatabase,
  createCtoxWebRtcNativePeer,
  getCtoxIndexedDbStorage,
  addRxPlugin,
  getConnectionHandlerSimplePeer,
  normalizeSignalingControlPlaneError,
  replicateWebRTC,
  rxdbCore,
  schemaHash,
  schemaHashSource,
} from '../dist/ctox-rxdb-js.mjs';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const testDir = dirname(fileURLToPath(import.meta.url));
const frameProtocolFixture = JSON.parse(readFileSync(resolve(testDir, '../../../../core/rxdb/tests/fixtures/webrtc-frame-protocol.json'), 'utf8'));
const rxdbProtocolFixture = JSON.parse(readFileSync(resolve(testDir, '../../../../core/rxdb/tests/fixtures/webrtc-rxdb-protocol.json'), 'utf8'));

if (
  frameProtocolFixture.protocol !== 'ctox-rxdb-frame-v1'
  || frameProtocolFixture.maxInlineFrameBytes !== 14 * 1024
  || frameProtocolFixture.maxChunkBytes !== 10 * 1024
  || frameProtocolFixture.maxTransferBytes !== 8 * 1024 * 1024
  || frameProtocolFixture.ackWindow !== 4
  || frameProtocolFixture.maxFrameRetries !== 2
) {
  throw new Error('WebRTC frame protocol fixture constants drifted from the Browser runtime contract');
}
for (const kind of ['start', 'chunk', 'ack', 'resume']) {
  if (frameProtocolFixture.frames?.[kind]?.ctoxFrame !== frameProtocolFixture.protocol || frameProtocolFixture.frames[kind].kind !== kind) {
    throw new Error(`WebRTC frame protocol fixture ${kind} frame is invalid`);
  }
}
if (
  frameProtocolFixture.frames.start.windowSize !== frameProtocolFixture.ackWindow
  || frameProtocolFixture.frames.ack.receivedFrames !== 2
  || frameProtocolFixture.frames.ack.resume !== false
  || frameProtocolFixture.frames.resume.ackSeq !== frameProtocolFixture.frames.ack.ackSeq
) {
  throw new Error('WebRTC frame protocol fixture ACK/resume metadata is invalid');
}

if (CTOX_RXDB_PROTOCOL !== 'ctox-rxdb-protocol-v1') {
  throw new Error('unexpected protocol constant');
}
if (
  rxdbProtocolFixture.protocol !== CTOX_RXDB_PROTOCOL
  || rxdbProtocolFixture.phase !== 'rxdb-protocol-handshake'
  || JSON.stringify(rxdbProtocolFixture.requiredCapabilities) !== JSON.stringify(CTOX_REQUIRED_PROTOCOL_CAPABILITIES)
  || rxdbProtocolFixture.errorCodes.schemaHashMismatch !== CTOX_PROTOCOL_ERROR_CODES.schemaHashMismatch
  || rxdbProtocolFixture.schemaHashSources.businessOsRegistry !== 'business-os-schema-hash-registry-v1'
  || rxdbProtocolFixture.schemaHashSources.canonicalJson !== 'canonical-json-schema-sha256-v1'
  || rxdbProtocolFixture.schemaHashSources.rxdbRs !== 'rxdb-rs-schema-hash-v1'
) {
  throw new Error('WebRTC RxDB protocol fixture constants drifted from the Browser runtime contract');
}
if (
  CTOX_BUSINESS_OS_SCHEMA_HASHES.business_commands !== '4c273d32175717566fdc42c6f7b5d32e144f9d2ed1c7f5db15d1b9ef04c89d5e'
  || await schemaHash({ version: 99, primaryKey: 'id', properties: { id: { type: 'string', maxLength: 32 } } }, 'business_commands') !== CTOX_BUSINESS_OS_SCHEMA_HASHES.business_commands
  || schemaHashSource('business_commands') !== 'business-os-schema-hash-registry-v1'
) {
  throw new Error('Business OS schema hash registry must match the Rust fixture for known collections');
}

const customSchema = { version: 1, primaryKey: 'id', properties: { id: { type: 'string', maxLength: 64 } } };
if (
  schemaHashSource('custom_local_collection') !== 'canonical-json-schema-sha256-v1'
  || await schemaHash(customSchema, 'custom_local_collection') === CTOX_BUSINESS_OS_SCHEMA_HASHES.business_commands
) {
  throw new Error('Unknown/custom collections must use canonical JSON schema hashing, not the Business OS registry');
}

const canonical = canonicalJson({ b: 1, a: { d: 2, c: 3 } });
if (canonical !== '{"a":{"c":3,"d":2},"b":1}') {
  throw new Error(`canonical JSON mismatch: ${canonical}`);
}

const payload = buildProtocolPayload({
  collectionName: 'business_commands',
  schemaVersion: 1,
  schemaHash: 'abc',
  checkpoint: { state: 'advertised', epoch: 'epoch-1' },
  peerSessionId: 'session-1',
  peerGeneration: 1,
});
if (payload.collection?.name !== 'business_commands' || payload.collection?.schemaHash !== 'abc' || payload.peerSession?.role !== 'browser') {
  throw new Error('protocol payload mismatch');
}
if (payload.collection?.schemaHashSource !== 'business-os-schema-hash-registry-v1') {
  throw new Error('protocol payload must advertise known Business OS schema hash source');
}
if (payload.checkpoint?.epoch !== 'epoch-1' || payload.collection?.checkpoint?.epoch !== 'epoch-1') {
  throw new Error('checkpoint evidence must be exposed both top-level and under collection');
}
const customPayload = buildProtocolPayload({
  collectionName: 'custom_local_collection',
  schemaVersion: 1,
  schemaHash: await schemaHash(customSchema, 'custom_local_collection'),
});
if (customPayload.collection?.schemaHashSource !== 'canonical-json-schema-sha256-v1') {
  throw new Error('custom protocol payload must advertise canonical schema hash source');
}
assertCompatibleProtocol(rxdbProtocolFixture.compatible.browser, rxdbProtocolFixture.compatible.native);
for (const [mutator, expectedCode] of [
  [(remote) => { delete remote.protocol; }, CTOX_PROTOCOL_ERROR_CODES.protocolMissing],
  [(remote) => { remote.protocol = 'rxdb-upstream'; }, CTOX_PROTOCOL_ERROR_CODES.protocolMismatch],
  [(remote) => { remote.capabilities = remote.capabilities.filter((capability) => capability !== 'ctox-peer-session-v1'); }, CTOX_PROTOCOL_ERROR_CODES.capabilityMissing],
  [(remote) => { remote.collection.name = 'desktop_file_chunks'; }, CTOX_PROTOCOL_ERROR_CODES.collectionMismatch],
  [(remote) => { remote.collection.schemaVersion = 2; }, CTOX_PROTOCOL_ERROR_CODES.schemaVersionMismatch],
  [(remote) => { remote.collection.schemaHash = 'different-fixture-hash'; }, CTOX_PROTOCOL_ERROR_CODES.schemaHashMismatch],
]) {
  const remote = structuredClone(rxdbProtocolFixture.compatible.native);
  mutator(remote);
  try {
    assertCompatibleProtocol(rxdbProtocolFixture.compatible.browser, remote);
    throw new Error(`protocol mismatch ${expectedCode} was not rejected`);
  } catch (error) {
    if (error.code !== expectedCode || error.name !== 'CtoxRxdbProtocolError' || error.phase !== 'rxdb-protocol-handshake' || error.retryable !== false) {
      throw new Error(`unexpected protocol compatibility error for ${expectedCode}: ${error.name}/${error.code}/${error.phase}`);
    }
  }
}

if (typeof createRxDatabase !== 'function' || getCtoxIndexedDbStorage().name !== 'ctox-indexeddb-native') {
  throw new Error('CTOX DB database exports are missing');
}

if (addRxPlugin({ name: 'ignored-transition-plugin' }) !== undefined) {
  throw new Error('addRxPlugin must remain a non-unlocking transition shim');
}

const handler = getConnectionHandlerSimplePeer({
  signalingServerUrl: 'ws://127.0.0.1:19998',
  config: { iceServers: [] },
});
if (handler.kind !== 'ctox-native-webrtc' || typeof replicateWebRTC !== 'function') {
  throw new Error('WebRTC compatibility exports are missing');
}

const controlPlaneError = normalizeSignalingControlPlaneError({
  type: 'ctoxError',
  scope: 'control-plane',
  code: 'instance_mismatch',
  reason: 'Browser joined the wrong signaling instance room.',
});
if (
  controlPlaneError.name !== 'CtoxSignalingControlPlaneError'
  || controlPlaneError.code !== 'instance_mismatch'
  || controlPlaneError.phase !== 'signaling-control-plane'
  || controlPlaneError.retryable !== false
) {
  throw new Error('signaling control-plane errors must stay typed through the app-local fork');
}

const originalWebSocket = globalThis.WebSocket;
const originalRtcPeerConnection = globalThis.RTCPeerConnection;
const closedConnections = [];
try {
  globalThis.WebSocket = class FakeWebSocket {
    static OPEN = 1;
    constructor() {
      this.readyState = FakeWebSocket.OPEN;
    }
    send() {}
    close() {
      this.readyState = 3;
    }
  };
  globalThis.RTCPeerConnection = class FakeRtcPeerConnection {
    constructor() {
      this.connectionState = 'new';
      this.localDescription = null;
    }
    createDataChannel() {
      return { readyState: 'connecting', close() {} };
    }
    async createOffer() {
      return { type: 'offer', sdp: 'fake-sdp' };
    }
    async setLocalDescription(description) {
      this.localDescription = description;
    }
    close() {
      closedConnections.push(this);
      this.connectionState = 'closed';
    }
  };
  const peer = createCtoxWebRtcNativePeer({
    signalingUrl: 'ws://127.0.0.1:19998',
    room: 'ctox-business-os:test:desktop_file_chunks',
    clientId: 'browser-peer',
  });
  const peers = [{ peerId: 'browser-peer', role: 'browser' }, { peerId: 'ctox-peer', role: 'ctox_instance' }];
  peer.handleSignalingMessage(JSON.stringify({ type: 'joined', peers }));
  peer.handleSignalingMessage(JSON.stringify({ type: 'joined', otherPeerIds: ['stale-browser-peer'] }));
  if (peer.connections.has('stale-browser-peer')) {
    throw new Error('browser peer must ignore signaling peers without native role metadata');
  }
  const firstConnection = peer.connections.get('ctox-peer');
  peer.handleSignalingMessage(JSON.stringify({ type: 'joined', peers }));
  const secondConnection = peer.connections.get('ctox-peer');
  if (!firstConnection || !secondConnection || firstConnection === secondConnection || closedConnections.length !== 1) {
    throw new Error('rejoined native peer must replace stale RTC connections');
  }
  const prefixedPeer = createCtoxWebRtcNativePeer({
    signalingUrl: 'ws://127.0.0.1:19998',
    room: 'ctox-business-os:test:ctox_runtime_settings',
    clientId: 'browser-prefixed',
  });
  prefixedPeer.handleSignalingMessage(JSON.stringify({ type: 'joined', peers: [{ peerId: 'ctox-core-live', role: 'ctox_instance_webserver' }] }));
  if (!prefixedPeer.connections.has('ctox-core-live')) {
    throw new Error('ctox-core native peer ids must be accepted even when signaling role metadata is not exact');
  }
  const targetedPeer = createCtoxWebRtcNativePeer({
    signalingUrl: 'ws://127.0.0.1:19998',
    room: 'ctox-business-os:test:business_commands',
    clientId: 'browser-targeted',
    expectedNativePeerId: 'ctox-core-current',
  });
  targetedPeer.handleSignalingMessage(JSON.stringify({
    type: 'joined',
    peers: [
      { peerId: 'ctox-core-stale', role: 'ctox_instance' },
      { peerId: 'ctox-core-current', role: 'ctox_instance_webserver' },
    ],
  }));
  if (targetedPeer.connections.has('ctox-core-stale') || !targetedPeer.connections.has('ctox-core-current')) {
    throw new Error('expected native peer id must prevent stale native peer fan-out');
  }
  const serverAssignedTargetPeer = createCtoxWebRtcNativePeer({
    signalingUrl: 'ws://127.0.0.1:19998',
    room: 'ctox-business-os:test:business_commands',
    clientId: 'browser-server-assigned-target',
    expectedNativePeerId: 'ctox-core-current',
  });
  serverAssignedTargetPeer.handleSignalingMessage(JSON.stringify({
    type: 'joined',
    peers: [
      { peerId: 'native-server-id', role: 'ctox_instance', client: 'ctox-business-os-native' },
      { peerId: 'browser-server-id', role: 'browser', client: 'business-os' },
    ],
  }));
  if (!serverAssignedTargetPeer.connections.has('native-server-id') || serverAssignedTargetPeer.connections.has('browser-server-id')) {
    throw new Error('expected native peer id must fall back to one native signaling peer when the server assigns opaque peer ids');
  }
  const presencePeer = createCtoxWebRtcNativePeer({
    signalingUrl: 'ws://127.0.0.1:19998',
    room: 'ctox-business-os:test:business_commands',
    clientId: 'browser-presence',
  });
  presencePeer.handleSignalingMessage(JSON.stringify({
    type: 'ctoxPresence',
    peerId: 'browser-presence',
    peers: [
      { peerId: 'presence-native-id', role: 'ctox_instance', client: 'ctox-business-os-native' },
    ],
  }));
  if (!presencePeer.connections.has('presence-native-id')) {
    throw new Error('ctoxPresence metadata must be accepted as native peer evidence');
  }
} finally {
  globalThis.WebSocket = originalWebSocket;
  globalThis.RTCPeerConnection = originalRtcPeerConnection;
}

const core = rxdbCore();
if (core.CTOX_RXDB_PROTOCOL !== CTOX_RXDB_PROTOCOL || typeof core.replicateWebRTC !== 'function' || typeof core.getConnectionHandlerSimplePeer !== 'function') {
  throw new Error('rxdbCore must expose the full Business OS runtime surface');
}

{
  const peer = createCtoxWebRtcNativePeer({
    signalingUrl: 'ws://127.0.0.1:19998',
    room: 'ctox-business-os:test:desktop_file_chunks',
    clientId: 'browser-peer',
  });
  const acks = [];
  const payloads = [];
  peer.send = (_peerId, payload) => {
    acks.push(payload);
    return true;
  };
  peer.handleDataChannelFrame = async (_peerId, payload) => {
    payloads.push(payload);
  };
  const original = JSON.stringify({
    id: 'large-request',
    method: 'masterChangesSince',
    params: [{ checkpoint: null, batchSize: 8, payload: 'x'.repeat(64 * 1024) }],
  });
  const chunkSize = 16 * 1024;
  const chunks = [];
  for (let offset = 0; offset < original.length; offset += chunkSize) {
    chunks.push(original.slice(offset, offset + chunkSize));
  }
  await peer.handleTransportFrame('ctox-peer', {
    ctoxFrame: 'ctox-rxdb-frame-v1',
    kind: 'start',
    transferId: 'transfer-1',
    totalFrames: chunks.length,
    totalBytes: new TextEncoder().encode(original).length,
  });
  await peer.handleTransportFrame('ctox-peer', {
    ctoxFrame: 'ctox-rxdb-frame-v1',
    kind: 'chunk',
    transferId: 'transfer-1',
    seq: 0,
    data: chunks[0],
  });
  await peer.handleTransportFrame('ctox-peer', {
    ctoxFrame: 'ctox-rxdb-frame-v1',
    kind: 'resume',
    transferId: 'transfer-1',
  });
  const partialResumeAck = acks.at(-1);
  if (partialResumeAck?.kind !== 'ack' || partialResumeAck?.resume !== true || partialResumeAck?.ackSeq !== 0 || partialResumeAck?.final !== false) {
    throw new Error('WebRTC frame resume must expose the highest contiguous received frame');
  }
  for (const seq of [1, 2, 3, ...Array.from({ length: chunks.length - 4 }, (_, index) => index + 4)]) {
    await peer.handleTransportFrame('ctox-peer', {
      ctoxFrame: 'ctox-rxdb-frame-v1',
      kind: 'chunk',
      transferId: 'transfer-1',
      seq,
      data: chunks[seq],
    });
  }
  const finalAck = acks.at(-1);
  if (acks.length < 2 || finalAck?.kind !== 'ack' || finalAck?.transferId !== 'transfer-1' || finalAck?.final !== true) {
    throw new Error('WebRTC frame reassembly must ACK windows and completed transfers');
  }
  if (payloads.length !== 1 || payloads[0]?.id !== 'large-request' || payloads[0]?.method !== 'masterChangesSince') {
    throw new Error('WebRTC frame reassembly must emit the original RxDB request');
  }
  await peer.handleTransportFrame('ctox-peer', {
    ctoxFrame: 'ctox-rxdb-frame-v1',
    kind: 'resume',
    transferId: 'transfer-1',
  });
  const completedResumeAck = acks.at(-1);
  if (completedResumeAck?.kind !== 'ack' || completedResumeAck?.resume !== true || completedResumeAck?.final !== true || completedResumeAck?.ackSeq !== chunks.length - 1) {
    throw new Error('WebRTC frame resume must replay completed transfer ACKs');
  }
  const transportStatus = peer.getTransportStatus();
  if (
    transportStatus.protocol !== 'ctox-rxdb-frame-v1'
    || transportStatus.receivedFrames < chunks.length + 3
    || transportStatus.resumeAckCount < 0
    || transportStatus.completedAckCacheSize < 1
  ) {
    throw new Error('WebRTC transport status must expose frame stream counters');
  }
}

{
  const peer = createCtoxWebRtcNativePeer({
    signalingUrl: 'ws://127.0.0.1:19998',
    room: 'ctox-business-os:test:fixture-exchange',
    clientId: 'browser-peer',
  });
  const sentFrames = [];
  let totalFrames = 0;
  const connection = {
    remotePeerId: 'ctox-peer',
    channel: {
      readyState: 'open',
      bufferedAmount: 0,
      send: (text) => {
        const frame = JSON.parse(text);
        sentFrames.push(frame);
        if (frame.kind === 'start') {
          totalFrames = frame.totalFrames;
        }
        if (frame.kind === 'chunk') {
          const isWindowBoundary = (frame.seq + 1) % frameProtocolFixture.ackWindow === 0;
          const isFinal = frame.seq === totalFrames - 1;
          if (isWindowBoundary || isFinal) {
            queueMicrotask(() => {
              peer.handleTransportFrame('ctox-peer', {
                ctoxFrame: frameProtocolFixture.protocol,
                kind: 'ack',
                transferId: frame.transferId,
                ackSeq: frame.seq,
                receivedFrames: frame.seq + 1,
                resume: false,
                final: isFinal,
              });
            });
          }
        }
      },
    },
  };
  await peer.sendFramed(connection, JSON.stringify({
    id: 'fixture-large-write',
    method: 'masterWrite',
    params: [{ payload: 'x'.repeat(25 * 1024) }],
  }));
  const startFrame = sentFrames.find((frame) => frame.kind === 'start');
  const chunkFrame = sentFrames.find((frame) => frame.kind === 'chunk');
  if (
    startFrame?.ctoxFrame !== frameProtocolFixture.protocol
    || startFrame.windowSize !== frameProtocolFixture.ackWindow
    || startFrame.attempt !== 0
    || startFrame.totalFrames < 3
    || startFrame.totalBytes <= frameProtocolFixture.maxChunkBytes
  ) {
    throw new Error('WebRTC executable fixture exchange must emit fixture-compatible start frames');
  }
  if (
    chunkFrame?.ctoxFrame !== frameProtocolFixture.protocol
    || chunkFrame.attempt !== 0
    || !Number.isInteger(chunkFrame.seq)
    || typeof chunkFrame.data !== 'string'
    || chunkFrame.data.length > frameProtocolFixture.maxChunkBytes
  ) {
    throw new Error('WebRTC executable fixture exchange must emit fixture-compatible chunk frames');
  }
}

{
  const peer = createCtoxWebRtcNativePeer({
    signalingUrl: 'ws://127.0.0.1:19998',
    room: 'ctox-business-os:test:scheduler',
    clientId: 'browser-peer',
  });
  const sent = [];
  const connection = {
    remotePeerId: 'ctox-peer',
    channel: {
      readyState: 'open',
      bufferedAmount: 0,
      send: (text) => sent.push(JSON.parse(text)),
    },
  };
  peer.enqueueSendFrame(connection, {
    payload: { id: 'large-write', method: 'masterWrite' },
    text: JSON.stringify({ id: 'large-write', method: 'masterWrite' }),
    inline: true,
    priority: 'low',
  });
  peer.enqueueSendFrame(connection, {
    payload: { id: 'token-request', method: 'token' },
    text: JSON.stringify({ id: 'token-request', method: 'token' }),
    inline: true,
    priority: 'high',
  });
  await new Promise((resolve) => setTimeout(resolve, 0));
  if (sent[0]?.method !== 'token' || sent[1]?.method !== 'masterWrite') {
    throw new Error('WebRTC send scheduler must prioritize control traffic ahead of low-priority writes');
  }
  const status = peer.getTransportStatus();
  if (status.sentScheduledFrames < 2 || status.lastSendPriority !== 'low' || status.priorityQueueDepth !== 0) {
    throw new Error('WebRTC send scheduler must expose queue status counters');
  }
}

console.log('ctox-rxdb-js import smoke OK');
