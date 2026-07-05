// =============================================================================
// AGENT GUARDRAILS — ctox-rxdb data plane (read docs/ctox-rxdb.md first)
// =============================================================================
// This file is part of CTOX DB, the WebRTC-ONLY data plane between Business OS
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

// Connection lifecycle invariants pinned by tests: token freshness re-stamp
// per connect, yourPeerId-only identity adoption, joined-based backoff reset
// (signaling-freshness-smoke), byte-budgeted frame chunking mirroring the
// Rust splitter (frame-chunking-smoke), multiplex admission + the
// SHELL_CRITICAL_COLLECTIONS pin (rtc-critical-pool-smoke).
import { CtoxEventEmitter } from './event-target.mjs';
import { buildProtocolPayload, CTOX_RXDB_PROTOCOL } from './schema.mjs';
import {
  CTOX_FRAME_PROTOCOL,
  FRAME_ACK_WINDOW,
  MAX_CHUNK_CHARS,
  MAX_FRAME_RETRIES,
  MAX_INLINE_FRAME_BYTES,
  MAX_TRANSFER_BYTES,
} from './frame-contract.generated.mjs';
import { CTOX_PRESENCE_RPC } from './protocol-contract.generated.mjs';

const SEND_BUFFER_HIGH_WATER = 512 * 1024;
const SEND_BUFFER_LOW_WATER = 128 * 1024;
// Hard wire invariant shared with the Rust peer (MAX_SERIALIZED_FRAME_BYTES in
// connection_handler_rs.rs): a single serialized DataChannel message must stay
// <= 16 KiB or browsers kill the channel. Chunks are budgeted by their
// JSON-ESCAPED byte length against this ceiling — NOT by UTF-16 char count.
const MAX_SERIALIZED_FRAME_BYTES = 16384;
const FRAME_ACK_TIMEOUT_MS = 30_000;
const FRAME_RESUME_TIMEOUT_MS = 1_000;
const COMPLETED_FRAME_ACK_TTL_MS = 60_000;
const SEND_PRIORITIES = ['high', 'normal', 'low'];
// TODO(rxdb-webrtc-multiplexing): the real fix is to multiplex every
// collection over a single RTCPeerConnection instead of opening one
// PeerConnection per collection. There are ~80 Business OS collections (see
// CTOX_BUSINESS_OS_SCHEMA_HASHES in schema.mjs), so a per-collection model
// always risks slot starvation when several modules are open at once. Raising
// the cap to 64 buys headroom for that workload until the multiplexing
// redesign lands; it is not the end state.
const MAX_GLOBAL_RTC_PEER_CONNECTIONS = 64;
const RTC_CONNECTION_QUEUE_TIMEOUT_MS = 45_000;
const RTC_HANDSHAKE_TIMEOUT_MS = 60_000;
const GLOBAL_RTC_CONNECTION_POOL_KEY = Symbol.for('ctox.rxdb.webrtc-rtc-pool.v1');
const RECENT_RTC_EVENT_LIMIT = 40;
// Grace window for transient ICE 'disconnected' before tearing the
// connection down (mirrors the Rust peer's keep-through-Disconnected rule).
const ICE_DISCONNECTED_GRACE_MS = 8_000;
// Signaling reconnection backoff. Post-multiplex the whole room shares ONE
// signaling socket; a clean close used to only emit `signaling-close` (which had
// no listener), so the peer could never re-discover the native side until an
// external restart. The peer now self-reconnects the socket with exponential
// backoff and re-joins the room (re-broadcasting the peer list), complementing
// sync.js's higher-level restart engine.
const SIGNALING_RECONNECT_BASE_MS = 1_000;
const SIGNALING_RECONNECT_MAX_MS = 30_000;
const TRANSPORT_STATUS_EMIT_MIN_INTERVAL_MS = 250;
// Single source of truth for the shell-critical collection set. app.js derives
// its CRITICAL_SYNC_COLLECTIONS from this exported list so the two lists cannot
// silently drift. Browser_* members only register when the Browser module is
// active; the grant logic below gates only on criticals actually requested this
// session (see criticalRequested / criticalRtcPeerConnectionsReady).
export const SHELL_CRITICAL_COLLECTIONS = new Set([
  'ctox_runtime_settings',
  'business_module_catalog',
  'business_commands',
  'ctox_queue_tasks',
  'browser_sessions',
  'browser_tabs',
  'browser_frames',
  'browser_input_events',
]);

export function createCtoxWebRtcNativePeer(options = {}) {
  return new CtoxWebRtcNativePeer(options);
}

export class CtoxWebRtcNativePeer {
  constructor({
    signalingUrl,
    room,
    roomPassword = '',
    token = '',
    tokenIssuedAt = null,
    tokenExpiresAt = null,
    clientId = randomId('browser'),
    role = 'browser',
    instanceId = '',
    capabilities = [],
    iceServers = [],
    storageToken = randomId('storage'),
    expectedNativePeerId = '',
    protocolPayload = null,
    requestHandlers = {},
  } = {}) {
    if (!signalingUrl) {
      throw new Error('signalingUrl is required');
    }
    if (!room) {
      throw new Error('room is required');
    }
    this.options = {
      signalingUrl,
      room,
      roomPassword,
      token,
      tokenIssuedAt,
      tokenExpiresAt,
      clientId,
      role,
      instanceId,
      capabilities,
      iceServers,
      storageToken,
      expectedNativePeerId,
      protocolPayload,
      requestHandlers,
    };
    this.events = new CtoxEventEmitter();
    this.socket = null;
    this.connections = new Map();
    this.peerMetadata = new Map();
    this.pending = new Map();
    this.pendingFrameAcks = new Map();
    this.incomingFrames = new Map();
    this.completedFrameAcks = new Map();
    this.observedRequests = new Map();
    this.requestWaiters = new Map();
    this.requestCounter = 0;
    this.frameCounter = 0;
    this.transportStats = {
      protocol: CTOX_FRAME_PROTOCOL,
      maxInlineFrameBytes: MAX_INLINE_FRAME_BYTES,
      maxChunkChars: MAX_CHUNK_CHARS,
      maxTransferBytes: MAX_TRANSFER_BYTES,
      ackWindow: FRAME_ACK_WINDOW,
      sendBufferHighWater: SEND_BUFFER_HIGH_WATER,
      sendBufferLowWater: SEND_BUFFER_LOW_WATER,
      activeTransfers: 0,
      pendingAcks: 0,
      incomingTransfers: 0,
      completedAckCacheSize: 0,
      sentFrames: 0,
      sentInlineFrames: 0,
      sentBytes: 0,
      receivedFrames: 0,
      receivedBytes: 0,
      retryCount: 0,
      resumeRequestCount: 0,
      resumeAckCount: 0,
      backpressureWaitCount: 0,
      queuedFrames: 0,
      sentScheduledFrames: 0,
      priorityQueueDepth: 0,
      highPriorityQueueDepth: 0,
      normalPriorityQueueDepth: 0,
      lowPriorityQueueDepth: 0,
      lastSendPriority: 'normal',
      lastAckLagMs: 0,
      lastBufferedAmount: 0,
      updatedAtMs: Date.now(),
    };
    this.lastControlPlaneError = null;
    this.recentConnectionEvents = [];
    this.recentMessages = [];
    this.transportStatusEmitTimer = null;
    this.lastTransportStatusEmitAtMs = 0;
    this.connectionRequests = new Map();
    this.forceInitiatorPeers = new Set();
    this.closed = false;
    this.signalingReconnectTimer = null;
    // Per-peer grace timers for transient ICE 'disconnected' (see
    // onconnectionstatechange below).
    this.disconnectedGraceTimers = new Map();
    this.signalingReconnectDelayMs = SIGNALING_RECONNECT_BASE_MS;
  }

  on(type, listener) {
    return this.events.on(type, listener);
  }

  connect() {
    this.closed = false;
    const url = buildSignalingUrl(this.options);
    const socket = new WebSocket(url);
    this.socket = socket;
    socket.onopen = () => {
      socket.send(JSON.stringify({ type: 'join', room: this.options.room }));
      // Backoff is reset on the `joined` broadcast (proof the server accepted
      // us), not here: an open-then-rejected socket must keep backing off.
      this.events.emit('signaling-open', { url: redactUrl(url) });
    };
    socket.onmessage = (event) => this.handleSignalingMessage(event.data);
    socket.onerror = () => this.events.emit('error', this.lastControlPlaneError || { code: 'ctox_signaling_socket_error' });
    socket.onclose = () => {
      this.events.emit('signaling-close', {});
      if (!this.closed) this.scheduleSignalingReconnect();
    };
    return this;
  }

  scheduleSignalingReconnect() {
    if (this.closed || this.signalingReconnectTimer) return;
    const delay = this.signalingReconnectDelayMs;
    this.signalingReconnectDelayMs = Math.min(delay * 2, SIGNALING_RECONNECT_MAX_MS);
    this.signalingReconnectTimer = setTimeout(() => {
      this.signalingReconnectTimer = null;
      if (this.closed) return;
      this.events.emit('signaling-reconnect', { delayMs: delay });
      // Re-open the socket; onopen re-joins the room, which makes the signaling
      // server re-broadcast the room peer list and re-drive peer (re)connection.
      this.connect();
    }, delay);
  }

  close() {
    this.closed = true;
    if (this.signalingReconnectTimer) {
      clearTimeout(this.signalingReconnectTimer);
      this.signalingReconnectTimer = null;
    }
    if (this.transportStatusEmitTimer) {
      clearTimeout(this.transportStatusEmitTimer);
      this.transportStatusEmitTimer = null;
    }
    for (const timer of this.disconnectedGraceTimers.values()) clearTimeout(timer);
    this.disconnectedGraceTimers.clear();
    cancelRtcPeerConnectionRequestsForOwner(this, 'peer-close');
    this.connectionRequests.clear();
    for (const peerId of [...this.connections.keys()]) {
      this.removeConnection(peerId, 'peer-close');
    }
    if (this.socket && this.socket.readyState <= WebSocket.OPEN) {
      this.socket.close();
    }
    this.rejectAllPending(createPeerClosedError(this.options.clientId, 'peer-close'));
    this.incomingFrames.clear();
  }

  send(remotePeerId, payload) {
    const connection = this.connections.get(remotePeerId);
    if (!connection?.channel || connection.channel.readyState !== 'open') {
      return false;
    }
    const text = JSON.stringify(payload);
    this.enqueueSendFrame(connection, {
      payload,
      text,
      inline: encodedSize(text) <= MAX_INLINE_FRAME_BYTES,
      priority: classifySendPriority(payload, text),
    });
    return true;
  }

  enqueueSendFrame(connection, item) {
    if (!connection.sendQueue) {
      connection.sendQueue = createSendQueue();
    }
    connection.sendQueue[item.priority].push({
      ...item,
      queuedAtMs: Date.now(),
      sequence: connection.sendQueue.nextSequence++,
    });
    this.recordTransportStatus({
      queuedFrames: this.transportStats.queuedFrames + 1,
      lastSendPriority: item.priority,
    });
    this.refreshSendQueueStatus(connection);
    this.drainSendQueue(connection).catch((error) => {
      this.events.emit('error', {
        code: 'ctox_webrtc_send_queue_failed',
        peerId: connection.remotePeerId,
        message: error?.message || String(error),
      });
    });
  }

  async drainSendQueue(connection) {
    if (connection.sendQueue?.draining) return;
    connection.sendQueue.draining = true;
    try {
      await Promise.resolve();
      while (!this.closed && this.connections.get(connection.remotePeerId) === connection && connection.channel?.readyState === 'open') {
        const item = nextQueuedSend(connection.sendQueue);
        if (!item) break;
        this.refreshSendQueueStatus(connection);
        this.recordTransportStatus({
          sentScheduledFrames: this.transportStats.sentScheduledFrames + 1,
          lastSendPriority: item.priority,
        });
        if (item.inline) {
          await this.waitForSendBuffer(connection.channel);
          if (this.connections.get(connection.remotePeerId) !== connection || connection.channel?.readyState !== 'open') {
            this.removeConnection(connection.remotePeerId, 'send-queue-channel-closed');
            break;
          }
          try {
            connection.channel.send(item.text);
            this.recordSentInlineFrame(item.payload, connection.channel);
          } catch (error) {
            this.removeConnection(connection.remotePeerId, 'send-queue-send-failed');
            throw error;
          }
          continue;
        }
        try {
          await this.sendFramed(connection, item.text);
        } catch (error) {
          const peerClosed = isPeerClosedError(error);
          if (this.connections.get(connection.remotePeerId) === connection && connection.channel?.readyState !== 'open') {
            this.removeConnection(connection.remotePeerId, 'frame-send-channel-closed');
          }
          this.events.emit('error', {
            code: peerClosed ? 'ctox_webrtc_peer_closed' : 'ctox_webrtc_frame_send_failed',
            peerId: connection.remotePeerId,
            priority: item.priority,
            reason: error?.reason || null,
            lifecycle: peerClosed,
            message: error?.message || String(error),
          });
        }
      }
    } finally {
      connection.sendQueue.draining = false;
      this.refreshSendQueueStatus(connection);
    }
  }

  async sendFramed(connection, text) {
    const channel = connection.channel;
    const transferId = `${this.options.clientId}|frame|${Date.now()}|${this.frameCounter++}`;
    // Byte-correct chunking (mirrors Rust `split_chunks_for_frame`): slicing
    // by UTF-16 chars let umlaut/emoji-heavy documents blow past the 16 KiB
    // SCTP-safe envelope and silently kill the DataChannel.
    const chunks = splitFrameChunks(text, transferId);
    const totalFrames = chunks.length;
    const totalBytes = encodedSize(text);
    if (totalBytes > MAX_TRANSFER_BYTES) {
      throw new Error(`WebRTC frame transfer exceeds ${MAX_TRANSFER_BYTES} bytes`);
    }
    this.recordTransportStatus({ activeTransfers: this.transportStats.activeTransfers + 1 });
    let lastError = null;
    for (let attempt = 0; attempt <= MAX_FRAME_RETRIES; attempt += 1) {
      try {
        if (this.connections.get(connection.remotePeerId) !== connection || channel?.readyState !== 'open') {
          throw createPeerClosedError(connection.remotePeerId, 'frame-send-channel-closed');
        }
        const startFrame = {
          ctoxFrame: CTOX_FRAME_PROTOCOL,
          kind: 'start',
          transferId,
          windowSize: FRAME_ACK_WINDOW,
          attempt,
          totalFrames,
          totalBytes,
        };
        channel.send(JSON.stringify(startFrame));
        this.recordSentTransportFrame(startFrame, channel);
        for (let windowStart = 0; windowStart < totalFrames; windowStart += FRAME_ACK_WINDOW) {
          await this.drainHighPriorityInlineFrames(connection);
          const windowEnd = Math.min(windowStart + FRAME_ACK_WINDOW, totalFrames) - 1;
          const ack = this.awaitFrameAck(transferId, connection.remotePeerId, windowEnd);
          for (let seq = windowStart; seq <= windowEnd; seq += 1) {
            await this.waitForSendBuffer(channel);
            if (this.connections.get(connection.remotePeerId) !== connection || channel?.readyState !== 'open') {
              throw createPeerClosedError(connection.remotePeerId, 'frame-send-channel-closed');
            }
            const chunkFrame = {
              ctoxFrame: CTOX_FRAME_PROTOCOL,
              kind: 'chunk',
              transferId,
              attempt,
              seq,
              data: chunks[seq],
            };
            channel.send(JSON.stringify(chunkFrame));
            this.recordSentTransportFrame(chunkFrame, channel);
          }
          try {
            await this.awaitFrameAckWithControlDrain(connection, ack);
          } catch (error) {
            const resumed = await this.requestFrameResume(connection, transferId, attempt, windowEnd);
            if (!resumed) throw error;
          }
        }
        this.recordTransportStatus({ activeTransfers: Math.max(0, this.transportStats.activeTransfers - 1) });
        return;
      } catch (error) {
        lastError = error;
        if (isPeerClosedError(error)) break;
        if (attempt >= MAX_FRAME_RETRIES) break;
        this.recordTransportStatus({ retryCount: this.transportStats.retryCount + 1 });
        this.events.emit('transport-retry', {
          peerId: connection.remotePeerId,
          transferId,
          attempt: attempt + 1,
        });
        await delay(Math.min(250 * (attempt + 1), 1000));
      }
    }
    this.recordTransportStatus({ activeTransfers: Math.max(0, this.transportStats.activeTransfers - 1) });
    throw lastError || new Error(`WebRTC frame transfer failed ${transferId}`);
  }

  async awaitFrameAckWithControlDrain(connection, ackPromise) {
    let settled = false;
    const wrapped = Promise.resolve(ackPromise).then(
      (value) => {
        settled = true;
        return { ok: true, value };
      },
      (error) => {
        settled = true;
        return { ok: false, error };
      },
    );
    while (!settled && this.connections.get(connection.remotePeerId) === connection && connection.channel?.readyState === 'open') {
      const result = await Promise.race([
        wrapped,
        delay(50).then(() => null),
      ]);
      if (result) {
        if (result.ok) return result.value;
        throw result.error;
      }
      await this.drainHighPriorityInlineFrames(connection);
    }
    const result = await wrapped;
    if (result.ok) return result.value;
    throw result.error;
  }

  async drainHighPriorityInlineFrames(connection) {
    const queue = connection.sendQueue;
    if (!queue) return;
    while (connection.channel?.readyState === 'open') {
      const item = nextHighPriorityInlineSend(queue);
      if (!item) break;
      this.refreshSendQueueStatus(connection);
      await this.waitForSendBuffer(connection.channel);
      connection.channel.send(item.text);
      this.recordSentInlineFrame(item.payload, connection.channel);
      this.recordTransportStatus({
        sentScheduledFrames: this.transportStats.sentScheduledFrames + 1,
        lastSendPriority: item.priority,
      });
    }
  }

  awaitFrameAck(transferId, peerId, ackSeq = null) {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pendingFrameAcks.delete(frameAckKey(transferId, ackSeq));
        reject(new Error(`Timed out waiting for WebRTC frame ack ${transferId}:${ackSeq ?? 'final'}`));
      }, FRAME_ACK_TIMEOUT_MS);
      this.pendingFrameAcks.set(frameAckKey(transferId, ackSeq), { resolve, reject, timer, peerId, transferId, ackSeq, sentAtMs: Date.now() });
      this.recordTransportStatus({ pendingAcks: this.pendingFrameAcks.size });
    });
  }

  requestFrameResume(connection, transferId, attempt, ackSeq) {
    const channel = connection.channel;
    return new Promise((resolve, reject) => {
      if (this.connections.get(connection.remotePeerId) !== connection || channel?.readyState !== 'open') {
        resolve(false);
        return;
      }
      const key = frameAckKey(transferId, ackSeq);
      const timer = setTimeout(() => {
        this.pendingFrameAcks.delete(key);
        this.recordTransportStatus({ pendingAcks: this.pendingFrameAcks.size });
        resolve(false);
      }, FRAME_RESUME_TIMEOUT_MS);
      this.pendingFrameAcks.set(key, {
        resolve: (payload) => resolve(payload || true),
        reject,
        timer,
        peerId: connection.remotePeerId,
        transferId,
        ackSeq,
        sentAtMs: Date.now(),
      });
      const resumeFrame = {
        ctoxFrame: CTOX_FRAME_PROTOCOL,
        kind: 'resume',
        transferId,
        attempt,
        ackSeq,
      };
      channel.send(JSON.stringify(resumeFrame));
      this.recordSentTransportFrame(resumeFrame, channel);
      this.recordTransportStatus({ resumeRequestCount: this.transportStats.resumeRequestCount + 1 });
    });
  }

  waitForSendBuffer(channel) {
    if (Number(channel.bufferedAmount || 0) <= SEND_BUFFER_HIGH_WATER) {
      return Promise.resolve();
    }
    this.recordTransportStatus({
      backpressureWaitCount: this.transportStats.backpressureWaitCount + 1,
      lastBufferedAmount: Number(channel.bufferedAmount || 0),
    });
    return new Promise((resolve) => {
      const previousThreshold = channel.bufferedAmountLowThreshold;
      channel.bufferedAmountLowThreshold = SEND_BUFFER_LOW_WATER;
      const done = () => {
        channel.removeEventListener?.('bufferedamountlow', done);
        channel.bufferedAmountLowThreshold = previousThreshold || 0;
        resolve();
      };
      channel.addEventListener?.('bufferedamountlow', done, { once: true });
      setTimeout(done, 250);
    });
  }

  // Phase 3 multiplex: callers tag a `collection` so one DataChannel can carry
  // every collection. The frame's `collection` is the native demux routing
  // key; responses are still correlated by request `id`.
  request(remotePeerId, method, params = [], timeoutMs = 15000, collection = null) {
    const id = `${this.options.clientId}|${Date.now()}|${this.requestCounter++}`;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        const error = new Error(`Timed out waiting for WebRTC response ${method}`);
        const peerId = String(remotePeerId || '');
        const connection = this.connections.get(peerId);
        if (connection) {
          this.recordConnectionEvent(connection, 'request-timeout', { method });
          if (shouldRecycleConnectionAfterRequestTimeout(method)) {
            this.forceInitiatorPeers.add(peerId);
            this.removeConnection(peerId, `request-timeout-${method}`);
          }
        }
        reject(error);
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timer, method, peerId: remotePeerId });
      const frame = { id, method, params };
      if (collection) frame.collection = collection;
      const sent = this.send(remotePeerId, frame);
      if (!sent) {
        this.pending.delete(id);
        clearTimeout(timer);
        this.scheduleReconnect(remotePeerId, `send-not-open-${method}`);
        reject(new Error(`WebRTC peer ${remotePeerId} is not open`));
      }
    });
  }

  scheduleReconnect(remotePeerId, reason = 'peer-reconnect') {
    const peerId = String(remotePeerId || '');
    if (!peerId || this.closed || !this.shouldConnectToRemotePeer(peerId)) return;
    setTimeout(() => {
      if (this.closed || this.connections.has(peerId) || !this.shouldConnectToRemotePeer(peerId)) return;
      try {
        this.ensureConnection(peerId);
      } catch (reconnectError) {
        this.events.emit('error', normalizePeerSignalError(reconnectError, peerId));
      }
    }, 250 + Math.floor(Math.random() * 500));
    this.events.emit('peer-state', { peerId, state: 'reconnect-scheduled', reason });
  }

  handleSignalingMessage(raw) {
    let message;
    try {
      message = JSON.parse(raw);
    } catch (error) {
      this.events.emit('error', { code: 'ctox_signaling_invalid_json', message: error.message });
      return;
    }
    if (message.type === 'init' || message.type === 'joined' || message.type === 'ctoxPresence') {
      // Only `yourPeerId` may rename us. `message.peerId` on joined/presence
      // frames plausibly names the REMOTE peer that triggered the broadcast —
      // adopting it corrupted senderPeerId on all subsequent signals and made
      // the initiator/target checks reject the native peer.
      if (message.yourPeerId && message.yourPeerId !== this.options.clientId) {
        this.options.clientId = String(message.yourPeerId);
      }
      if (message.type === 'joined') {
        // A joined broadcast proves the server ACCEPTED our join — only now
        // reset the reconnect backoff. Resetting on socket-open degenerated
        // into a 1s-interval storm when the server accepted the socket and
        // then rejected the join (e.g. control-plane errors).
        this.signalingReconnectDelayMs = SIGNALING_RECONNECT_BASE_MS;
      }
      const descriptors = signalingPeerDescriptors(message);
      const previousMetadata = new Map(this.peerMetadata);
      for (const descriptor of descriptors) {
        if (descriptor.peerId) this.rememberPeerMetadata(descriptor.peerId, descriptor);
      }
      this.pruneStaleNativeCandidateConnections(descriptors);
      const expectedNativePeerId = String(this.options.expectedNativePeerId || '').trim();
      const hasExpectedDescriptor = Boolean(expectedNativePeerId) && descriptors.some((descriptor) => (
        this.peerMatchesExpectedNativePeerId(descriptor.peerId, descriptor)
      ));
      for (const descriptor of descriptors) {
        const remotePeerId = descriptor.peerId;
        if (!remotePeerId) continue;
        if (hasExpectedDescriptor && !this.peerMatchesExpectedNativePeerId(remotePeerId, descriptor)) {
          this.removeConnection(remotePeerId, 'signaling-non-target-native-peer');
          continue;
        }
        const previousDescriptor = previousMetadata.get(remotePeerId);
        const nativePeerRejoined = message.type === 'joined'
          && remotePeerId !== this.options.clientId
          && this.connections.has(remotePeerId)
          && peerJoinedAtChanged(previousDescriptor, descriptor);
        if (nativePeerRejoined) {
          this.removeConnection(remotePeerId, 'signaling-peer-rejoined');
        }
        if (!this.shouldConnectToRemotePeer(remotePeerId)) {
          this.removeConnection(remotePeerId, 'signaling-non-native-peer');
          continue;
        }
        this.ensureConnection(remotePeerId);
      }
      this.events.emit('joined', message);
      return;
    }
    if (message.type === 'ctoxError') {
      const error = normalizeSignalingControlPlaneError(message);
      if (error.name === 'CtoxSignalingControlPlaneError') {
        this.lastControlPlaneError = error;
      }
      this.events.emit('error', error);
      return;
    }
    if (message.type === 'signal' || message.signal || message.data) {
      const remotePeerId = String(message.senderPeerId || message.sender || message.from || message.peerId || '');
      if (!remotePeerId) {
        this.events.emit('error', { code: 'ctox_signaling_missing_sender' });
        return;
      }
      if (!this.shouldConnectToRemotePeer(remotePeerId)) {
        return;
      }
      this.handlePeerSignal(remotePeerId, message.signal || message.data).catch((error) => {
        const normalized = normalizePeerSignalError(error, remotePeerId);
        if (normalized?.ignored) return;
        this.events.emit('error', normalized);
      });
    }
  }

  ensureConnection(remotePeerId) {
    if (remotePeerId === this.options.clientId) {
      return this.connections.get(remotePeerId);
    }
    if (!this.shouldConnectToRemotePeer(remotePeerId)) {
      return undefined;
    }
    let connection = this.connections.get(remotePeerId);
    if (connection) {
      return connection;
    }
    const slot = tryAcquireRtcPeerConnectionSlot(this, remotePeerId);
    if (!slot) {
      this.queueConnection(remotePeerId).catch((error) => {
        this.events.emit('error', normalizePeerSignalError(error, remotePeerId));
      });
      return undefined;
    }
    return this.createConnection(remotePeerId, slot);
  }

  queueConnection(remotePeerId) {
    if (this.closed || !this.shouldConnectToRemotePeer(remotePeerId)) {
      return Promise.resolve(undefined);
    }
    const existing = this.connections.get(remotePeerId);
    if (existing) return Promise.resolve(existing);
    const pending = this.connectionRequests.get(remotePeerId);
    if (pending) return pending;
    const request = acquireRtcPeerConnectionSlot(this, remotePeerId)
      .then((slot) => {
        if (this.closed || !this.shouldConnectToRemotePeer(remotePeerId)) {
          releaseRtcPeerConnectionSlot(slot, 'queued-peer-abandoned');
          return undefined;
        }
        const current = this.connections.get(remotePeerId);
        if (current) {
          releaseRtcPeerConnectionSlot(slot, 'queued-peer-existing');
          return current;
        }
        return this.createConnection(remotePeerId, slot);
      })
      .finally(() => {
        this.connectionRequests.delete(remotePeerId);
      });
    this.connectionRequests.set(remotePeerId, request);
    return request;
  }

  createConnection(remotePeerId, rtcPoolSlot = null) {
    let peer;
    try {
      peer = new RTCPeerConnection({ iceServers: this.options.iceServers });
    } catch (error) {
      releaseRtcPeerConnectionSlot(rtcPoolSlot, 'rtc-constructor-failed');
      throw error;
    }
    const connection = {
      peer,
      channel: null,
      remotePeerId,
      pendingCandidates: [],
      rtcPoolSlot,
      createdAtMs: Date.now(),
      lastStateChangeAtMs: Date.now(),
      lastError: null,
      signalStats: createPeerSignalStats(),
      localCandidateTypes: {},
      remoteCandidateTypes: {},
      handshakeTimer: null,
      forceInitiator: this.forceInitiatorPeers.has(remotePeerId),
    };
    this.connections.set(remotePeerId, connection);
    connection.handshakeTimer = setTimeout(() => {
      const current = this.connections.get(remotePeerId);
      if (this.closed || current !== connection) return;
      if (connection.channel?.readyState === 'open') return;
      this.recordConnectionEvent(connection, 'handshake-timeout', {
        ageMs: Date.now() - connection.createdAtMs,
        connectionState: peer.connectionState || '',
        iceConnectionState: peer.iceConnectionState || '',
        iceGatheringState: peer.iceGatheringState || '',
        signalingState: peer.signalingState || '',
      });
      this.events.emit('peer-state', { peerId: remotePeerId, state: 'handshake-timeout' });
      this.forceInitiatorPeers.add(remotePeerId);
      this.removeConnection(remotePeerId, 'rtc-handshake-timeout');
    }, RTC_HANDSHAKE_TIMEOUT_MS);
    this.recordConnectionEvent(connection, 'created', { state: peer.connectionState || 'new' });

    peer.onicecandidate = (event) => {
      if (event.candidate) {
        recordCandidateType(connection.localCandidateTypes, event.candidate?.candidate);
        connection.signalStats.candidateSent += 1;
        connection.signalStats.lastLocalCandidateType = candidateTypeFromLine(event.candidate?.candidate);
        connection.signalStats.lastSignalAtMs = Date.now();
        this.sendSignal(remotePeerId, { type: 'candidate', candidate: event.candidate.toJSON() });
        return;
      }
      connection.signalStats.localCandidateComplete = true;
      connection.signalStats.lastSignalAtMs = Date.now();
      this.recordConnectionEvent(connection, 'local-candidates-complete', { state: peer.connectionState || '' });
    };
    peer.oniceconnectionstatechange = () => {
      this.recordConnectionEvent(connection, 'ice-connection-state', {
        state: peer.iceConnectionState || '',
      });
    };
    peer.onicegatheringstatechange = () => {
      this.recordConnectionEvent(connection, 'ice-gathering-state', {
        state: peer.iceGatheringState || '',
      });
    };
    peer.onconnectionstatechange = () => {
      const state = peer.connectionState;
      this.recordConnectionEvent(connection, 'connection-state', { state });
      this.events.emit('peer-state', { peerId: remotePeerId, state });
      if (state === 'disconnected') {
        // ICE 'disconnected' is usually transient (NAT rebind, brief Wi-Fi
        // blip) and recovers on its own — the Rust peer deliberately keeps
        // the connection through it for the same reason. Tearing down
        // immediately turned every blip into a full reconnect cycle (15-45s
        // of handshake timeouts). Give ICE a grace window; tear down only if
        // it has not recovered by then. 'failed'/'closed' stay immediate.
        const existing = this.disconnectedGraceTimers.get(remotePeerId);
        if (existing) clearTimeout(existing);
        this.disconnectedGraceTimers.set(remotePeerId, setTimeout(() => {
          this.disconnectedGraceTimers.delete(remotePeerId);
          const live = this.connections.get(remotePeerId);
          const liveState = live?.peer?.connectionState || '';
          if (live === connection && ['disconnected', 'failed'].includes(liveState)) {
            this.removeConnection(remotePeerId, 'peer-disconnected-grace-expired');
          }
        }, ICE_DISCONNECTED_GRACE_MS));
        return;
      }
      const graceTimer = this.disconnectedGraceTimers.get(remotePeerId);
      if (graceTimer) {
        clearTimeout(graceTimer);
        this.disconnectedGraceTimers.delete(remotePeerId);
      }
      if (['closed', 'failed'].includes(state)) {
        this.removeConnection(remotePeerId, `peer-${state}`);
      }
    };
    peer.ondatachannel = (event) => this.attachChannel(connection, event.channel);

    if (this.shouldInitiate(remotePeerId, connection)) {
      this.attachChannel(connection, peer.createDataChannel('ctox-rxdb'));
      this.createOffer(remotePeerId, peer).catch((error) => {
        this.events.emit('error', normalizePeerSignalError(error, remotePeerId));
      });
    }
    return connection;
  }

  shouldInitiate(remotePeerId, connection = null) {
    if (connection?.forceInitiator) return true;
    const remoteRole = this.peerMetadata.get(String(remotePeerId || ''))?.role || '';
    if (this.options.role === 'browser' && remoteRole === 'ctox_instance') return true;
    if (this.options.role === 'ctox_instance' && remoteRole === 'browser') return false;
    return String(this.options.clientId) < String(remotePeerId);
  }

  async createOffer(remotePeerId, peer) {
    if (this.closed || peer.signalingState === 'closed') return;
    const offer = await peer.createOffer();
    if (this.closed || peer.signalingState === 'closed') return;
    await peer.setLocalDescription(offer);
    const connection = this.connections.get(remotePeerId);
    if (connection) {
      connection.signalStats.offerSent += 1;
      connection.signalStats.lastSignalAtMs = Date.now();
      this.recordConnectionEvent(connection, 'offer-sent', { signalingState: peer.signalingState });
    }
    this.sendSignal(remotePeerId, { type: offer.type, sdp: offer.sdp });
  }

  async handlePeerSignal(remotePeerId, signal) {
    const connection = this.ensureConnection(remotePeerId);
    if (!connection) return;
    const peer = connection.peer;
    const data = typeof signal === 'string' ? JSON.parse(signal) : signal;
    if (data.type === 'candidate') {
      recordCandidateType(connection.remoteCandidateTypes, data.candidate?.candidate);
      connection.signalStats.candidateReceived += 1;
      connection.signalStats.lastRemoteCandidateType = candidateTypeFromLine(data.candidate?.candidate);
      connection.signalStats.lastSignalAtMs = Date.now();
      await this.addIceCandidateWhenReady(connection, data.candidate);
      return;
    }
    if (data.type === 'offer') {
      connection.signalStats.offerReceived += 1;
      connection.signalStats.lastSignalAtMs = Date.now();
      this.recordConnectionEvent(connection, 'offer-received', { signalingState: peer.signalingState });
      if (this.shouldInitiate(remotePeerId, connection)) {
        this.recordConnectionEvent(connection, 'offer-ignored-local-initiator', {
          signalingState: peer.signalingState,
        });
        return;
      }
      if (peer.signalingState !== 'stable') {
        await rollbackLocalDescription(peer);
      }
      await peer.setRemoteDescription(data);
      await this.flushPendingIceCandidates(connection);
      const answer = await peer.createAnswer();
      await peer.setLocalDescription(answer);
      connection.signalStats.answerSent += 1;
      connection.signalStats.lastSignalAtMs = Date.now();
      this.recordConnectionEvent(connection, 'answer-sent', { signalingState: peer.signalingState });
      this.sendSignal(remotePeerId, { type: answer.type, sdp: answer.sdp });
      return;
    }
    if (data.type === 'answer') {
      connection.signalStats.answerReceived += 1;
      connection.signalStats.lastSignalAtMs = Date.now();
      this.recordConnectionEvent(connection, 'answer-received', { signalingState: peer.signalingState });
      if (peer.signalingState !== 'have-local-offer') {
        return;
      }
      await peer.setRemoteDescription(data);
      await this.flushPendingIceCandidates(connection);
    }
  }

  async addIceCandidateWhenReady(connection, candidate) {
    if (!candidate) return;
    const peer = connection?.peer;
    if (!peer || peer.signalingState === 'closed') return;
    if (!peer.remoteDescription) {
      connection.pendingCandidates.push(candidate);
      this.recordConnectionEvent(connection, 'candidate-queued', { pendingCandidates: connection.pendingCandidates.length });
      return;
    }
    try {
      await peer.addIceCandidate(candidate);
      this.recordConnectionEvent(connection, 'candidate-added', { pendingCandidates: connection.pendingCandidates.length });
    } catch (error) {
      if (!peer.remoteDescription && isMissingRemoteDescriptionIceError(error)) {
        connection.pendingCandidates.push(candidate);
        this.recordConnectionEvent(connection, 'candidate-queued', { pendingCandidates: connection.pendingCandidates.length });
        return;
      }
      connection.lastError = normalizePeerSignalError(error, connection.remotePeerId);
      throw error;
    }
  }

  async flushPendingIceCandidates(connection) {
    const peer = connection?.peer;
    if (!peer || peer.signalingState === 'closed' || !peer.remoteDescription) return;
    const candidates = connection.pendingCandidates.splice(0);
    for (const candidate of candidates) {
      try {
        await peer.addIceCandidate(candidate);
      } catch (error) {
        this.events.emit('error', normalizePeerSignalError(error, connection.remotePeerId));
      }
    }
  }

  attachChannel(connection, channel) {
    connection.channel = channel;
    channel.onopen = () => {
      if (connection.handshakeTimer) {
        clearTimeout(connection.handshakeTimer);
        connection.handshakeTimer = null;
      }
      markCriticalRtcPeerConnectionOpened(connection.rtcPoolSlot);
      this.forceInitiatorPeers.delete(connection.remotePeerId);
      drainRtcPeerConnectionQueue('critical-peer-opened');
      this.recordConnectionEvent(connection, 'datachannel-open', { readyState: channel.readyState || 'open' });
      this.events.emit('peer-open', { peerId: connection.remotePeerId });
    };
    channel.onmessage = (event) => {
      let payload = event.data;
      try {
        payload = JSON.parse(event.data);
      } catch {
        // Binary and text payloads are valid for future chunk streaming.
      }
      this.handleDataChannelFrame(connection.remotePeerId, payload);
    };
    channel.onerror = () => {
      connection.lastError = { code: 'ctox_data_channel_error', peerId: connection.remotePeerId };
      this.recordConnectionEvent(connection, 'datachannel-error', { readyState: channel.readyState || '' });
      this.events.emit('error', connection.lastError);
    };
    channel.onclose = () => {
      this.recordConnectionEvent(connection, 'datachannel-close', { readyState: channel.readyState || 'closed' });
      this.removeConnection(connection.remotePeerId, 'channel-close');
    };
  }

  async handleDataChannelFrame(peerId, payload) {
    if (this.closed) return;
    if (payload?.ctoxFrame === CTOX_FRAME_PROTOCOL) {
      await this.handleTransportFrame(peerId, payload);
      return;
    }
    this.recordMessageMeta(peerId, payload);
    this.events.emit('message', { peerId, payload });
    // Phase 3 multiplex: master-change pushes carry a collection-qualified id
    // (`masterChangeStream$:{collection}`) and/or a `collection` field so the
    // shared peer can fan the event to the right collection's pull. The bare
    // `masterChangeStream$` id is still accepted for V1 / single-collection
    // peers.
    const masterChangeCollection = masterChangeStreamCollection(payload);
    if (masterChangeCollection !== null) {
      this.events.emit('master-change', {
        peerId,
        result: payload.result,
        collection: masterChangeCollection || payload.collection || null,
      });
      return;
    }
    // Presence push (ctox-presence-v1): the native hub pushes the aggregate of
    // the OTHER peers' presence entries as a response frame with the reserved
    // `presence$` id. It is a server push, not a reply — intercept it before
    // the pending-response correlation (which would drop the unknown id).
    if (payload?.id === CTOX_PRESENCE_RPC.streamId) {
      this.events.emit('presence', {
        peerId,
        entries: Array.isArray(payload?.result?.entries) ? payload.result.entries : [],
      });
      return;
    }
    if (payload?.id && (Object.prototype.hasOwnProperty.call(payload, 'result') || Object.prototype.hasOwnProperty.call(payload, 'error'))) {
      const pending = this.pending.get(payload.id);
      if (!pending) return;
      this.pending.delete(payload.id);
      clearTimeout(pending.timer);
      if (payload.error) {
        pending.reject(payload.error);
      } else {
        pending.resolve(payload.result);
      }
      return;
    }
    if (payload?.id && payload.method) {
      try {
        const result = await this.handleRequest(
          peerId,
          payload.method,
          payload.params || [],
          payload.collection || null,
        );
        // Echo the routing collection back so a multiplexing remote can
        // correlate without relying solely on the request-id map.
        const response = { id: payload.id, result, error: null };
        if (payload.collection) response.collection = payload.collection;
        this.send(peerId, response);
      } catch (error) {
        const normalized = serializeFrameError(error, payload.method);
        this.events.emit('error', normalized);
        const response = { id: payload.id, result: null, error: normalized };
        if (payload.collection) response.collection = payload.collection;
        this.send(peerId, response);
      }
    }
  }

  async handleTransportFrame(peerId, payload) {
    this.recordReceivedTransportFrame(payload);
    if (payload.kind === 'ack') {
      const transferId = String(payload.transferId || '');
      const ackSeq = Number(payload.ackSeq ?? -1);
      for (const [key, pending] of [...this.pendingFrameAcks.entries()]) {
        if (pending.transferId !== transferId || pending.peerId !== peerId) continue;
        if (!(payload.final || pending.ackSeq == null || ackSeq >= pending.ackSeq)) continue;
        this.pendingFrameAcks.delete(key);
        clearTimeout(pending.timer);
        this.recordTransportStatus({
          pendingAcks: this.pendingFrameAcks.size,
          lastAckLagMs: pending.sentAtMs ? Date.now() - pending.sentAtMs : this.transportStats.lastAckLagMs,
          resumeAckCount: payload.resume ? this.transportStats.resumeAckCount + 1 : this.transportStats.resumeAckCount,
        });
        pending.resolve(payload);
      }
      return;
    }

    if (payload.kind === 'start') {
      const transferId = String(payload.transferId || '');
      const totalFrames = Number(payload.totalFrames || 0);
      const totalBytes = Number(payload.totalBytes || 0);
      if (!transferId || totalFrames < 1 || totalFrames > 100_000 || totalBytes > MAX_TRANSFER_BYTES) {
        this.events.emit('error', {
          code: 'ctox_webrtc_frame_start_invalid',
          peerId,
          transferId,
          totalBytes,
        });
        return;
      }
      this.incomingFrames.set(transferId, {
        peerId,
        totalFrames,
        totalBytes,
        received: new Map(),
        createdAt: Date.now(),
        attempt: Number(payload.attempt || 0),
        contiguousSeq: -1,
        nextAckSeq: Math.min(FRAME_ACK_WINDOW - 1, totalFrames - 1),
      });
      this.completedFrameAcks.delete(transferId);
      this.cleanupCompletedFrameAcks();
      this.recordTransportStatus({
        incomingTransfers: this.incomingFrames.size,
        completedAckCacheSize: this.completedFrameAcks.size,
      });
      return;
    }

    if (payload.kind === 'resume') {
      const transferId = String(payload.transferId || '');
      const completed = this.completedFrameAcks.get(transferId);
      if (completed && completed.peerId === peerId) {
        this.send(peerId, {
          ctoxFrame: CTOX_FRAME_PROTOCOL,
          kind: 'ack',
          transferId,
          ackSeq: completed.ackSeq,
          receivedFrames: completed.receivedFrames,
          final: true,
          resume: true,
        });
        return;
      }
      const entry = this.incomingFrames.get(transferId);
      if (entry && entry.peerId === peerId) {
        this.send(peerId, {
          ctoxFrame: CTOX_FRAME_PROTOCOL,
          kind: 'ack',
          transferId,
          ackSeq: Number(entry.contiguousSeq ?? -1),
          receivedFrames: entry.received.size,
          final: false,
          resume: true,
        });
      }
      return;
    }

    if (payload.kind !== 'chunk') return;
    const transferId = String(payload.transferId || '');
    const entry = this.incomingFrames.get(transferId);
    if (!entry || entry.peerId !== peerId) {
      this.events.emit('error', {
        code: 'ctox_webrtc_frame_chunk_without_start',
        peerId,
        transferId,
      });
      return;
    }
    const seq = Number(payload.seq);
    if (!Number.isInteger(seq) || seq < 0 || seq >= entry.totalFrames) {
      this.events.emit('error', {
        code: 'ctox_webrtc_frame_chunk_invalid',
        peerId,
        transferId,
        seq,
      });
      return;
    }
    const attempt = Number(payload.attempt || 0);
    if (attempt !== Number(entry.attempt || 0)) {
      this.events.emit('error', {
        code: 'ctox_webrtc_frame_chunk_stale_attempt',
        peerId,
        transferId,
        seq,
        attempt,
        expectedAttempt: entry.attempt,
      });
      return;
    }
    const contiguousSeq = recordReceivedFrame(entry, seq, String(payload.data || ''));
    if (entry.received.size !== entry.totalFrames) {
      if (contiguousSeq >= entry.nextAckSeq && contiguousSeq < entry.totalFrames - 1) {
        this.send(peerId, {
          ctoxFrame: CTOX_FRAME_PROTOCOL,
          kind: 'ack',
          transferId,
          ackSeq: contiguousSeq,
          receivedFrames: entry.received.size,
          final: false,
        });
        entry.nextAckSeq = Math.min(contiguousSeq + FRAME_ACK_WINDOW, entry.totalFrames - 1);
      }
      return;
    }

    this.incomingFrames.delete(transferId);
    let text = '';
    for (let index = 0; index < entry.totalFrames; index += 1) {
      text += entry.received.get(index) || '';
    }
    if (entry.totalBytes && encodedSize(text) !== entry.totalBytes) {
      this.events.emit('error', {
        code: 'ctox_webrtc_frame_size_mismatch',
        peerId,
        transferId,
        expectedBytes: entry.totalBytes,
        actualBytes: encodedSize(text),
      });
      return;
    }
    this.send(peerId, {
      ctoxFrame: CTOX_FRAME_PROTOCOL,
      kind: 'ack',
      transferId,
      ackSeq: entry.totalFrames - 1,
      receivedFrames: entry.received.size,
      final: true,
    });
    this.completedFrameAcks.set(transferId, {
      peerId,
      ackSeq: entry.totalFrames - 1,
      receivedFrames: entry.received.size,
      expiresAt: Date.now() + COMPLETED_FRAME_ACK_TTL_MS,
    });
    this.cleanupCompletedFrameAcks();
    this.recordTransportStatus({
      incomingTransfers: this.incomingFrames.size,
      completedAckCacheSize: this.completedFrameAcks.size,
    });
    try {
      await this.handleDataChannelFrame(peerId, JSON.parse(text));
    } catch (error) {
      this.events.emit('error', {
        code: 'ctox_webrtc_frame_decode_failed',
        peerId,
        transferId,
        message: error?.message || String(error),
      });
    }
  }

  async handleRequest(peerId, method, params, collection = null) {
    this.recordObservedRequest(peerId, method);
    if (method === 'token') {
      return this.options.storageToken;
    }
    if (method === 'ctoxProtocol') {
      return this.protocolPayload(peerId, params, collection);
    }
    const handler = this.options.requestHandlers?.[method];
    if (typeof handler === 'function') {
      // Phase 3 multiplex: pass the frame's collection so a shared peer can
      // route `masterChangesSince` / `masterWrite` to the right collection.
      return handler({ peerId, params, collection, peer: this });
    }
    return {
      code: 'ctox_unknown_webrtc_method',
      phase: 'replication-io',
      direction: 'unknown',
      method,
    };
  }

  recordObservedRequest(peerId, method) {
    const key = requestObservationKey(peerId, method);
    this.observedRequests.set(key, Date.now());
    const waiters = this.requestWaiters.get(key) || [];
    this.requestWaiters.delete(key);
    for (const waiter of waiters) {
      clearTimeout(waiter.timer);
      waiter.resolve();
    }
    this.events.emit('request-observed', { peerId, method });
  }

  hasObservedRequest(peerId, method) {
    return this.observedRequests.has(requestObservationKey(peerId, method));
  }

  waitForRequest(peerId, method, timeoutMs = 2000) {
    if (this.hasObservedRequest(peerId, method)) {
      return Promise.resolve();
    }
    const key = requestObservationKey(peerId, method);
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        const waiters = (this.requestWaiters.get(key) || []).filter((item) => item.resolve !== resolve);
        if (waiters.length) this.requestWaiters.set(key, waiters);
        else this.requestWaiters.delete(key);
        reject(new Error(`Timed out waiting for remote WebRTC request ${method}`));
      }, timeoutMs);
      const waiters = this.requestWaiters.get(key) || [];
      waiters.push({ resolve, reject, timer });
      this.requestWaiters.set(key, waiters);
    });
  }

  async protocolPayload(peerId, params = [], collection = null) {
    if (typeof this.options.protocolPayload === 'function') {
      return this.options.protocolPayload({ peerId, params, collection, peer: this });
    }
    return buildProtocolPayload({
      role: this.options.role,
      peerSessionId: `${this.options.role}:${this.options.clientId}`,
      peerGeneration: 1,
      capabilities: this.options.capabilities,
    });
  }

  sendSignal(remotePeerId, signal) {
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN) {
      this.events.emit('error', { code: 'ctox_signaling_socket_not_open', peerId: remotePeerId });
      return false;
    }
    this.socket.send(JSON.stringify({
      type: 'signal',
      room: this.options.room,
      senderPeerId: this.options.clientId,
      receiverPeerId: remotePeerId,
      receiver: remotePeerId,
      target: remotePeerId,
      data: signal,
    }));
    return true;
  }

  removeConnection(remotePeerId, reason = 'closed') {
    const peerId = String(remotePeerId || '');
    const connection = this.connections.get(peerId);
    if (!connection) return;
    this.connections.delete(peerId);
    this.connectionRequests.delete(peerId);
    if (connection.handshakeTimer) {
      clearTimeout(connection.handshakeTimer);
      connection.handshakeTimer = null;
    }
    try { connection.channel?.close?.(); } catch {}
    try { connection.peer?.close?.(); } catch {}
    releaseRtcPeerConnectionSlot(connection.rtcPoolSlot, reason);
    this.rejectPendingForPeer(peerId, createPeerClosedError(peerId, reason));
    this.events.emit('peer-close', { peerId, reason });
    if (reason !== 'peer-close') {
      this.scheduleReconnect(peerId, reason);
    }
  }

  rememberPeerMetadata(peerId, metadata = {}) {
    const normalized = normalizePeerMetadata({ ...metadata, peerId });
    if (!normalized.peerId || normalized.peerId === this.options.clientId) return;
    this.peerMetadata.set(normalized.peerId, {
      ...(this.peerMetadata.get(normalized.peerId) || {}),
      ...normalized,
    });
  }

  shouldConnectToRemotePeer(remotePeerId) {
    const peerId = String(remotePeerId || '');
    if (!peerId || peerId === this.options.clientId) return false;
    const metadata = this.peerMetadata.get(peerId);
    if (this.peerMatchesExpectedNativePeerId(peerId, metadata)) return true;
    if (this.nativeCandidateConnectionCount(peerId) > 0) return false;
    return this.isNativePeerCandidate(peerId, metadata);
  }

  isNativePeerCandidate(peerId, metadata = {}) {
    return this.peerMatchesExpectedNativePeerId(peerId, metadata)
      || peerId.startsWith('ctox-business-os-native')
      || peerId.startsWith('ctox-core-')
      || metadata?.role === 'ctox_instance';
  }

  pruneStaleNativeCandidateConnections(descriptors = []) {
    const liveNativePeerIds = new Set(
      descriptors
        .filter((descriptor) => descriptor?.peerId && this.isNativePeerCandidate(descriptor.peerId, descriptor))
        .map((descriptor) => descriptor.peerId),
    );
    if (!liveNativePeerIds.size) return;
    for (const peerId of [...this.connections.keys()]) {
      if (liveNativePeerIds.has(peerId)) continue;
      const metadata = this.peerMetadata.get(peerId);
      if (!this.isNativePeerCandidate(peerId, metadata)) continue;
      this.removeConnection(peerId, 'peer-close');
    }
  }

  peerMatchesExpectedNativePeerId(peerId, metadata = {}) {
    const expectedNativePeerId = String(this.options.expectedNativePeerId || '').trim();
    if (!expectedNativePeerId) return false;
    const candidates = [
      peerId,
      metadata?.peerId,
      metadata?.nativePeerId,
      metadata?.native_peer_id,
      metadata?.corePeerId,
      metadata?.core_peer_id,
      metadata?.clientId,
      metadata?.client_id,
      metadata?.client,
    ];
    return candidates.some((candidate) => String(candidate || '').trim() === expectedNativePeerId);
  }

  nativeCandidateConnectionCount(excludePeerId = '') {
    let count = 0;
    for (const peerId of this.connections.keys()) {
      if (peerId === excludePeerId) continue;
      const metadata = this.peerMetadata.get(peerId);
      if (this.isNativePeerCandidate(peerId, metadata)) {
        count += 1;
      }
    }
    return count;
  }

  rejectPendingForPeer(peerId, error) {
    for (const [id, pending] of [...this.pending.entries()]) {
      if (pending.peerId !== peerId) continue;
      this.pending.delete(id);
      clearTimeout(pending.timer);
      pending.reject(error);
    }
    for (const [transferId, pending] of [...this.pendingFrameAcks.entries()]) {
      if (pending.peerId !== peerId) continue;
      this.pendingFrameAcks.delete(transferId);
      clearTimeout(pending.timer);
      pending.reject(error);
    }
    for (const [transferId, entry] of [...this.incomingFrames.entries()]) {
      if (entry.peerId === peerId) this.incomingFrames.delete(transferId);
    }
  }

  rejectAllPending(error) {
    for (const [id, pending] of [...this.pending.entries()]) {
      this.pending.delete(id);
      clearTimeout(pending.timer);
      pending.reject(error);
    }
    for (const [key, waiters] of [...this.requestWaiters.entries()]) {
      this.requestWaiters.delete(key);
      for (const waiter of waiters) {
        clearTimeout(waiter.timer);
        waiter.reject(error);
      }
    }
    for (const [transferId, pending] of [...this.pendingFrameAcks.entries()]) {
      this.pendingFrameAcks.delete(transferId);
      clearTimeout(pending.timer);
      pending.reject(error);
    }
    this.incomingFrames.clear();
    this.completedFrameAcks.clear();
    for (const connection of this.connections.values()) {
      if (connection.sendQueue) {
        connection.sendQueue.high = [];
        connection.sendQueue.normal = [];
        connection.sendQueue.low = [];
      }
    }
    this.recordTransportStatus({
      pendingAcks: 0,
      incomingTransfers: 0,
      completedAckCacheSize: 0,
      priorityQueueDepth: 0,
      highPriorityQueueDepth: 0,
      normalPriorityQueueDepth: 0,
      lowPriorityQueueDepth: 0,
    });
  }

  getTransportStatus({ includeDiagnostics = false } = {}) {
    const base = {
      ...this.transportStats,
      collection: collectionNameFromTopic(this.options.room),
      topic: this.options.room,
      activePeerCount: this.connections.size,
      pendingAcks: this.pendingFrameAcks.size,
      pendingRequests: this.pending.size,
      incomingTransfers: this.incomingFrames.size,
      completedAckCacheSize: this.completedFrameAcks.size,
      connectionCount: this.connections.size,
      rtcConnectionPool: rtcPeerConnectionPoolCounters(),
    };
    if (!includeDiagnostics) return base;
    return {
      ...base,
      pendingRequestMethods: [...this.pending.values()].map((pending) => pending.method || '').filter(Boolean).slice(-20),
      observedRequestMethods: [...this.observedRequests.keys()].map((key) => String(key).split('|').slice(1).join('|')).slice(-20),
      rtcConnectionPool: rtcPeerConnectionPoolSnapshot(),
      rtcConnections: [...this.connections.values()].map((connection) => peerConnectionSnapshot(connection)),
      recentRtcEvents: this.recentConnectionEvents.slice(-RECENT_RTC_EVENT_LIMIT),
      connectionStates: [...this.connections.values()].map((connection) => ({
        peerId: connection.remotePeerId,
        peerConnectionState: connection.peer?.connectionState || '',
        iceConnectionState: connection.peer?.iceConnectionState || '',
        iceGatheringState: connection.peer?.iceGatheringState || '',
        signalingState: connection.peer?.signalingState || '',
        channelState: connection.channel?.readyState || '',
        channelLabel: connection.channel?.label || '',
        pendingCandidates: Array.isArray(connection.pendingCandidates)
          ? connection.pendingCandidates.length
          : 0,
      })),
      recentMessages: this.recentMessages.slice(-30),
    };
  }

  recordConnectionEvent(connection, event, detail = {}) {
    if (!connection) return;
    connection.lastStateChangeAtMs = Date.now();
    const entry = {
      atMs: connection.lastStateChangeAtMs,
      event,
      peerId: connection.remotePeerId,
      collection: collectionNameFromTopic(this.options.room),
      ...detail,
    };
    this.recentConnectionEvents.push(entry);
    if (this.recentConnectionEvents.length > RECENT_RTC_EVENT_LIMIT) {
      this.recentConnectionEvents.splice(0, this.recentConnectionEvents.length - RECENT_RTC_EVENT_LIMIT);
    }
    this.emitTransportStatus({ immediate: true });
  }

  recordSentTransportFrame(payload, channel) {
    this.recordTransportStatus({
      sentFrames: this.transportStats.sentFrames + 1,
      sentBytes: this.transportStats.sentBytes + encodedSize(JSON.stringify(payload)),
      lastBufferedAmount: Number(channel?.bufferedAmount || 0),
    });
  }

  recordSentInlineFrame(payload, channel) {
    this.recordTransportStatus({
      sentInlineFrames: this.transportStats.sentInlineFrames + 1,
      sentBytes: this.transportStats.sentBytes + encodedSize(JSON.stringify(payload)),
      lastBufferedAmount: Number(channel?.bufferedAmount || 0),
    });
  }

  recordReceivedTransportFrame(payload) {
    this.recordTransportStatus({
      receivedFrames: this.transportStats.receivedFrames + 1,
      receivedBytes: this.transportStats.receivedBytes + encodedSize(JSON.stringify(payload)),
    });
  }

  recordMessageMeta(peerId, payload) {
    if (!payload || typeof payload !== 'object') return;
    this.recentMessages.push({
      atMs: Date.now(),
      peerId: String(peerId || ''),
      id: typeof payload.id === 'string' ? payload.id.slice(0, 120) : '',
      method: typeof payload.method === 'string' ? payload.method.slice(0, 80) : '',
      collection: typeof payload.collection === 'string' ? payload.collection.slice(0, 120) : '',
      hasResult: Object.prototype.hasOwnProperty.call(payload, 'result'),
      hasError: Object.prototype.hasOwnProperty.call(payload, 'error'),
    });
    if (this.recentMessages.length > 60) {
      this.recentMessages.splice(0, this.recentMessages.length - 60);
    }
    this.emitTransportStatus();
  }

  recordTransportStatus(patch = {}) {
    Object.assign(this.transportStats, patch, { updatedAtMs: Date.now() });
    this.emitTransportStatus();
  }

  emitTransportStatus({ immediate = false } = {}) {
    if (this.closed) return;
    const now = Date.now();
    const elapsed = now - this.lastTransportStatusEmitAtMs;
    if (immediate || elapsed >= TRANSPORT_STATUS_EMIT_MIN_INTERVAL_MS) {
      if (this.transportStatusEmitTimer) {
        clearTimeout(this.transportStatusEmitTimer);
        this.transportStatusEmitTimer = null;
      }
      this.lastTransportStatusEmitAtMs = now;
      this.events.emit('transport-status', this.getTransportStatus());
      return;
    }
    if (this.transportStatusEmitTimer) return;
    this.transportStatusEmitTimer = setTimeout(() => {
      this.transportStatusEmitTimer = null;
      if (this.closed) return;
      this.lastTransportStatusEmitAtMs = Date.now();
      this.events.emit('transport-status', this.getTransportStatus());
    }, Math.max(0, TRANSPORT_STATUS_EMIT_MIN_INTERVAL_MS - elapsed));
  }

  refreshSendQueueStatus(connection = null) {
    let high = 0;
    let normal = 0;
    let low = 0;
    const connections = connection ? [connection] : this.connections.values();
    for (const entry of connections) {
      const queue = entry?.sendQueue;
      if (!queue) continue;
      high += queue.high.length;
      normal += queue.normal.length;
      low += queue.low.length;
    }
    this.recordTransportStatus({
      priorityQueueDepth: high + normal + low,
      highPriorityQueueDepth: high,
      normalPriorityQueueDepth: normal,
      lowPriorityQueueDepth: low,
    });
  }

  cleanupCompletedFrameAcks() {
    const now = Date.now();
    for (const [transferId, completed] of [...this.completedFrameAcks.entries()]) {
      if (completed.expiresAt <= now || this.completedFrameAcks.size > 512) {
        this.completedFrameAcks.delete(transferId);
      }
    }
  }
}

export function normalizeSignalingControlPlaneError(payload = {}) {
  if (!payload || typeof payload !== 'object') {
    return {
      name: 'Error',
      code: 'ctox_signaling_unknown_error',
      message: 'Unknown WebRTC signaling error.',
    };
  }
  const code = typeof payload.code === 'string' && payload.code.trim()
    ? payload.code.trim()
    : 'control_plane_rejected';
  const reason = typeof payload.reason === 'string' && payload.reason.trim()
    ? payload.reason.trim()
    : typeof payload.message === 'string' && payload.message.trim()
      ? payload.message.trim()
      : code;
  if (payload.type === 'ctoxError' && payload.scope === 'control-plane') {
    return {
      name: 'CtoxSignalingControlPlaneError',
      type: payload.type,
      scope: payload.scope,
      code,
      phase: 'signaling-control-plane',
      severity: 'error',
      retryable: false,
      message: reason,
    };
  }
  return {
    ...payload,
    code,
    message: reason,
  };
}

function createPeerClosedError(peerId, reason) {
  const error = new Error(`WebRTC peer ${peerId} closed: ${reason}`);
  error.code = 'ERR_CONNECTION_FAILURE';
  error.peerId = peerId;
  error.reason = reason;
  error.lifecycle = true;
  return error;
}

function isPeerClosedError(error) {
  if (!error) return false;
  if (error.lifecycle === true && error.code === 'ERR_CONNECTION_FAILURE') return true;
  const reason = String(error.reason || '');
  const message = String(error.message || error || '');
  return error.code === 'ERR_CONNECTION_FAILURE'
    || reason.includes('peer-close')
    || reason.includes('channel-close')
    || reason.includes('channel-closed')
    || message.includes(' closed: ')
    || message.includes('channel-close')
    || message.includes('channel-closed');
}

async function rollbackLocalDescription(peer) {
  if (!peer || peer.signalingState === 'stable' || peer.signalingState === 'closed') return;
  try {
    await peer.setLocalDescription({ type: 'rollback' });
  } catch {
    // Browsers that cannot rollback will continue with the deterministic
    // initiator rule above; the next signaling cycle replaces stale peers.
  }
}

function normalizePeerSignalError(error, peerId) {
  const message = String(error?.message || error || '');
  const name = typeof error?.name === 'string' ? error.name : 'Error';
  if (
    message.includes("Called in wrong state: stable")
    || message.includes('remote description was null')
    || message.includes('The remote description was null')
  ) {
    return {
      name: 'CtoxWebRtcPeerLifecycleEvent',
      code: 'peer_signal_stale',
      phase: 'peer-reconnect',
      severity: 'recoverable',
      retryable: true,
      lifecycle: true,
      peerId,
      message: 'Stale WebRTC signaling arrived after peer state changed; reconnect repair will keep the RxDB data channel authoritative.',
    };
  }
  return {
    name,
    code: error?.code || (isMissingRemoteDescriptionIceError(error) ? 'ERR_ADD_ICE_CANDIDATE' : 'ERR_SET_REMOTE_DESCRIPTION'),
    phase: 'peer-signaling',
    severity: 'error',
    retryable: true,
    peerId,
    message,
  };
}

function isMissingRemoteDescriptionIceError(error) {
  const message = String(error?.message || error || '');
  return message.includes('remote description was null') || message.includes('The remote description was null');
}

function serializeFrameError(error, method = '') {
  if (error && typeof error === 'object') {
    return {
      name: error.name || 'Error',
      code: error.code || 'ctox_webrtc_request_failed',
      method,
      message: error.message || String(error),
      retryable: Boolean(error.retryable),
      lifecycle: Boolean(error.lifecycle),
    };
  }
  return {
    name: 'Error',
    code: 'ctox_webrtc_request_failed',
    method,
    message: String(error || 'Unknown WebRTC request failure'),
    retryable: false,
    lifecycle: false,
  };
}

function tryAcquireRtcPeerConnectionSlot(owner, remotePeerId) {
  const pool = getRtcPeerConnectionPool();
  noteCriticalRequested(pool, owner);
  const key = rtcPeerConnectionOwnerKey(owner, remotePeerId);
  const existing = pool.active.get(key);
  if (existing) return existing;
  const priority = rtcPeerConnectionPriority(owner);
  if (priority > 0 && isBrowserRuntime() && isBusinessOsRoom(owner?.options?.room) && !criticalRtcPeerConnectionsReady(pool)) {
    return null;
  }
  if (priority === 0) preemptOptionalRtcPeerConnectionSlot(pool);
  if (pool.active.size >= pool.maxActive) return null;
  const slot = createRtcPeerConnectionSlot(owner, remotePeerId, key);
  pool.active.set(key, slot);
  return slot;
}

function acquireRtcPeerConnectionSlot(owner, remotePeerId) {
  const immediate = tryAcquireRtcPeerConnectionSlot(owner, remotePeerId);
  if (immediate) return Promise.resolve(immediate);
  const pool = getRtcPeerConnectionPool();
  const key = rtcPeerConnectionOwnerKey(owner, remotePeerId);
  const existingQueued = pool.queue.find((entry) => entry.key === key);
  if (existingQueued) {
    scheduleRtcPeerConnectionQueueDrain('existing-slot-request');
    return existingQueued.promise;
  }
  noteCriticalRequested(pool, owner);
  let resolve;
  let reject;
  const promise = new Promise((promiseResolve, promiseReject) => {
    resolve = promiseResolve;
    reject = promiseReject;
  });
  const entry = {
    key,
    owner,
    remotePeerId,
    priority: rtcPeerConnectionPriority(owner),
    enqueuedAt: Date.now(),
    resolve,
    reject,
    promise,
    timer: null,
  };
  entry.timer = setTimeout(() => {
    removeQueuedRtcPeerConnection(entry);
    reject(new Error(`Timed out waiting for browser WebRTC connection budget for ${remotePeerId}`));
  }, RTC_CONNECTION_QUEUE_TIMEOUT_MS);
  pool.queue.push(entry);
  sortRtcPeerConnectionQueue(pool);
  owner?.events?.emit?.('peer-state', { peerId: remotePeerId, state: 'queued' });
  scheduleRtcPeerConnectionQueueDrain('slot-request-queued');
  return promise;
}

function releaseRtcPeerConnectionSlot(slot, reason = 'closed') {
  if (!slot?.key) return;
  const pool = getRtcPeerConnectionPool();
  pool.active.delete(slot.key);
  drainRtcPeerConnectionQueue(reason);
}

function drainRtcPeerConnectionQueue(reason = 'slot-released') {
  const pool = getRtcPeerConnectionPool();
  sortRtcPeerConnectionQueue(pool);
  while (pool.active.size < pool.maxActive && pool.queue.length) {
    const entryIndex = nextGrantableRtcPeerConnectionQueueIndex(pool);
    if (entryIndex < 0) break;
    const [entry] = pool.queue.splice(entryIndex, 1);
    if (entry.timer) clearTimeout(entry.timer);
    if (entry.owner?.closed) continue;
    if (pool.active.has(entry.key)) {
      entry.resolve(pool.active.get(entry.key));
      continue;
    }
    const slot = createRtcPeerConnectionSlot(entry.owner, entry.remotePeerId, entry.key);
    pool.active.set(entry.key, slot);
    entry.owner?.events?.emit?.('peer-state', { peerId: entry.remotePeerId, state: 'slot-granted', reason });
    entry.resolve(slot);
  }
}

function scheduleRtcPeerConnectionQueueDrain(reason = 'slot-drain-scheduled') {
  const pool = getRtcPeerConnectionPool();
  if (pool.drainScheduled) return;
  pool.drainScheduled = true;
  const schedule = typeof queueMicrotask === 'function'
    ? queueMicrotask
    : (callback) => Promise.resolve().then(callback);
  schedule(() => {
    pool.drainScheduled = false;
    drainRtcPeerConnectionQueue(reason);
  });
}

function removeQueuedRtcPeerConnection(entry) {
  const pool = getRtcPeerConnectionPool();
  const index = pool.queue.indexOf(entry);
  if (index >= 0) pool.queue.splice(index, 1);
  if (entry?.timer) clearTimeout(entry.timer);
}

function cancelRtcPeerConnectionRequestsForOwner(owner, reason = 'owner-closed') {
  const pool = getRtcPeerConnectionPool();
  const queued = pool.queue.filter((entry) => entry.owner === owner);
  for (const entry of queued) {
    removeQueuedRtcPeerConnection(entry);
    entry.reject(new Error(`Cancelled browser WebRTC connection budget request: ${reason}`));
  }
}

function sortRtcPeerConnectionQueue(pool) {
  pool.queue.sort((left, right) => {
    if (left.priority !== right.priority) return left.priority - right.priority;
    return left.enqueuedAt - right.enqueuedAt;
  });
}

function createRtcPeerConnectionSlot(owner, remotePeerId, key = rtcPeerConnectionOwnerKey(owner, remotePeerId)) {
  return {
    key,
    owner,
    remotePeerId: String(remotePeerId || ''),
    room: String(owner?.options?.room || ''),
    priority: rtcPeerConnectionPriority(owner),
    acquiredAtMs: Date.now(),
  };
}

function getRtcPeerConnectionPool() {
  const root = globalThis || {};
  if (!root[GLOBAL_RTC_CONNECTION_POOL_KEY]) {
    root[GLOBAL_RTC_CONNECTION_POOL_KEY] = {
      maxActive: MAX_GLOBAL_RTC_PEER_CONNECTIONS,
      active: new Map(),
      queue: [],
      criticalOpened: new Set(),
      criticalRequested: new Set(),
      drainScheduled: false,
    };
  } else if (root[GLOBAL_RTC_CONNECTION_POOL_KEY].maxActive < MAX_GLOBAL_RTC_PEER_CONNECTIONS) {
    root[GLOBAL_RTC_CONNECTION_POOL_KEY].maxActive = MAX_GLOBAL_RTC_PEER_CONNECTIONS;
  }
  return root[GLOBAL_RTC_CONNECTION_POOL_KEY];
}

function rtcPeerConnectionPoolSnapshot() {
  const pool = getRtcPeerConnectionPool();
  return {
    maxActive: pool.maxActive,
    active: pool.active.size,
    queued: pool.queue.length,
    activeCritical: activeCriticalRtcPeerConnectionCount(pool),
    queuedCritical: queuedCriticalRtcPeerConnectionNames(pool).length,
    criticalOpened: [...pool.criticalOpened].sort(),
    criticalReady: criticalRtcPeerConnectionsReady(pool),
    activeConnections: [...pool.active.values()].map((slot) => rtcPeerConnectionSlotSnapshot(slot)),
    queuedConnections: pool.queue.map((entry) => ({
      collection: collectionNameFromTopic(entry.owner?.options?.room || ''),
      priority: entry.priority,
      queuedForMs: Date.now() - entry.enqueuedAt,
    })),
  };
}

function rtcPeerConnectionPoolCounters() {
  const pool = getRtcPeerConnectionPool();
  const active = pool.active.size;
  const queued = pool.queue.length;
  const activeCritical = activeCriticalRtcPeerConnectionCount(pool);
  const queuedCritical = queuedCriticalRtcPeerConnectionNames(pool).length;
  return {
    maxActive: pool.maxActive,
    active,
    queued,
    activeCritical,
    queuedCritical,
    maxConnections: pool.maxActive,
    activeConnections: active,
    queuedConnections: queued,
    criticalActiveConnections: activeCritical,
    criticalQueuedConnections: queuedCritical,
  };
}

function rtcPeerConnectionOwnerKey(owner, remotePeerId) {
  return `${String(owner?.options?.room || '')}|${String(owner?.options?.clientId || '')}|${String(remotePeerId || '')}`;
}

function rtcPeerConnectionPriority(owner) {
  // Phase 3 (single multiplexed stream): there is now exactly ONE
  // RTCPeerConnection per (browser, sync room) carrying every collection, so
  // the per-collection admission gate is obsolete. The single connection is
  // always critical/always-allowed. The SHELL_CRITICAL_COLLECTIONS set and the
  // pool machinery are kept for back-compat but no longer gate anything in the
  // multiplexed path.
  void owner;
  return 0;
}

function noteCriticalRequested(pool, owner) {
  if (!pool || !owner) return;
  const room = owner?.options?.room || '';
  if (!isBusinessOsRoom(room)) return;
  const collection = collectionNameFromTopic(room);
  if (!SHELL_CRITICAL_COLLECTIONS.has(collection)) return;
  if (!pool.criticalRequested) pool.criticalRequested = new Set();
  pool.criticalRequested.add(collection);
}

function criticalRtcPeerConnectionsReady(pool) {
  // Gate optional connections only on the shell-critical collections actually
  // requested this session, not on the full SHELL_CRITICAL_COLLECTIONS set.
  // The 4 browser_* collections only register when the Browser module is
  // active, so a Documents-only session must not wait on them forever. If no
  // critical collection has been requested yet, optional connections may
  // proceed; otherwise every requested critical must have an open DataChannel.
  const requested = pool?.criticalRequested;
  if (!requested || requested.size === 0) return true;
  for (const collection of requested) {
    if (!SHELL_CRITICAL_COLLECTIONS.has(collection)) continue;
    if (!pool.criticalOpened?.has(collection)) return false;
  }
  return true;
}

function queuedCriticalRtcPeerConnectionNames(pool) {
  const queuedCriticalRooms = new Set();
  for (const entry of pool.queue) {
    const collection = collectionNameFromTopic(entry?.owner?.options?.room || '');
    if (SHELL_CRITICAL_COLLECTIONS.has(collection)) queuedCriticalRooms.add(collection);
  }
  return [...queuedCriticalRooms].sort();
}

function activeCriticalRtcPeerConnectionCount(pool) {
  let count = 0;
  for (const slot of pool.active.values()) {
    if (SHELL_CRITICAL_COLLECTIONS.has(collectionNameFromTopic(slot.room))) count += 1;
  }
  return count;
}

function preemptOptionalRtcPeerConnectionSlot(pool) {
  if (pool.active.size < pool.maxActive) return false;
  for (const slot of pool.active.values()) {
    const collection = collectionNameFromTopic(slot.room);
    if (SHELL_CRITICAL_COLLECTIONS.has(collection)) continue;
    try {
      slot.owner?.removeConnection?.(slot.remotePeerId, 'rtc-preempted-for-shell-critical');
    } catch {}
    return true;
  }
  return false;
}

function nextGrantableRtcPeerConnectionQueueIndex(pool) {
  for (let index = 0; index < pool.queue.length; index += 1) {
    const entry = pool.queue[index];
    if (!entry) continue;
    if (entry.priority === 0 || !isBrowserRuntime() || !isBusinessOsRoom(entry.owner?.options?.room)) {
      return index;
    }
    if (criticalRtcPeerConnectionsReady(pool)) {
      return index;
    }
  }
  return -1;
}

function markCriticalRtcPeerConnectionOpened(slot) {
  if (!slot || slot.priority !== 0 || !isBusinessOsRoom(slot.room)) return;
  const collection = collectionNameFromTopic(slot.room);
  if (!SHELL_CRITICAL_COLLECTIONS.has(collection)) return;
  getRtcPeerConnectionPool().criticalOpened.add(collection);
}

function rtcPeerConnectionSlotSnapshot(slot) {
  return {
    collection: collectionNameFromTopic(slot.room),
    priority: slot.priority,
    activeForMs: Date.now() - slot.acquiredAtMs,
  };
}

function signalingPeerDescriptors(message = {}) {
  const descriptors = [];
  const append = (entry) => {
    if (typeof entry === 'string') {
      descriptors.push({ peerId: entry });
      return;
    }
    if (!entry || typeof entry !== 'object') return;
    const peerId = entry.peerId || entry.id || entry.clientId || entry.client;
    if (!peerId) return;
    descriptors.push(normalizePeerMetadata({ ...entry, peerId }));
  };
  for (const entry of Array.isArray(message.peers) ? message.peers : []) append(entry);
  for (const entry of Array.isArray(message.otherPeerIds) ? message.otherPeerIds : []) append(entry);
  const seen = new Set();
  return descriptors.filter((descriptor) => {
    if (!descriptor.peerId || seen.has(descriptor.peerId)) return false;
    seen.add(descriptor.peerId);
    return true;
  });
}

function normalizePeerMetadata(entry = {}) {
  const capabilities = Array.isArray(entry.capabilities)
    ? entry.capabilities.filter((capability) => typeof capability === 'string' && capability.trim()).map((capability) => capability.trim())
    : [];
  return {
    peerId: typeof entry.peerId === 'string' ? entry.peerId : String(entry.peerId || ''),
    role: typeof entry.role === 'string' ? entry.role.trim() : '',
    protocol: typeof entry.protocol === 'string' ? entry.protocol.trim() : '',
    instanceId: typeof entry.instanceId === 'string' ? entry.instanceId.trim() : '',
    client: typeof entry.client === 'string' ? entry.client.trim() : '',
    joinedAt: entry.joinedAt ?? null,
    capabilities,
  };
}

function peerJoinedAtChanged(previous = {}, next = {}) {
  if (!previous || !next) return false;
  if (previous.joinedAt === null || previous.joinedAt === undefined) return false;
  if (next.joinedAt === null || next.joinedAt === undefined) return false;
  return String(previous.joinedAt) !== String(next.joinedAt);
}

function createPeerSignalStats() {
  return {
    offerSent: 0,
    offerReceived: 0,
    answerSent: 0,
    answerReceived: 0,
    candidateSent: 0,
    candidateReceived: 0,
    localCandidateComplete: false,
    lastLocalCandidateType: '',
    lastRemoteCandidateType: '',
    lastSignalAtMs: 0,
  };
}

function peerConnectionSnapshot(connection) {
  const peer = connection?.peer;
  const channel = connection?.channel;
  return {
    peerId: connection?.remotePeerId || '',
    collection: collectionNameFromTopic(connection?.rtcPoolSlot?.room || ''),
    createdAtMs: connection?.createdAtMs || 0,
    ageMs: connection?.createdAtMs ? Date.now() - connection.createdAtMs : 0,
    signalingState: peer?.signalingState || '',
    iceConnectionState: peer?.iceConnectionState || '',
    iceGatheringState: peer?.iceGatheringState || '',
    connectionState: peer?.connectionState || '',
    channelReadyState: channel?.readyState || '',
    pendingCandidates: Array.isArray(connection?.pendingCandidates) ? connection.pendingCandidates.length : 0,
    hasLocalDescription: Boolean(peer?.localDescription),
    hasRemoteDescription: Boolean(peer?.remoteDescription),
    localCandidateTypes: { ...(connection?.localCandidateTypes || {}) },
    remoteCandidateTypes: { ...(connection?.remoteCandidateTypes || {}) },
    signal: { ...(connection?.signalStats || {}) },
    lastError: connection?.lastError || null,
    lastStateChangeAtMs: connection?.lastStateChangeAtMs || 0,
  };
}

function recordCandidateType(target, candidateLine) {
  const type = candidateTypeFromLine(candidateLine);
  if (!type) return;
  target[type] = Number(target[type] || 0) + 1;
}

function candidateTypeFromLine(candidateLine) {
  const match = String(candidateLine || '').match(/\styp\s+([a-z0-9-]+)/i);
  return match?.[1] ? match[1].toLowerCase() : '';
}

function isBusinessOsRoom(room) {
  return String(room || '').startsWith('ctox-business-os:');
}

function isBrowserRuntime() {
  return typeof window === 'object' && typeof document === 'object';
}

function collectionNameFromTopic(topic) {
  const parts = String(topic || '').split(':').filter(Boolean);
  return parts.length ? parts[parts.length - 1] : '';
}

// Phase 3 multiplex: detect a master-change-stream push and extract its
// collection. Returns the collection name for a qualified id
// (`masterChangeStream$:{collection}`), `''` for the legacy bare id
// (collection unknown — fall back to the frame's `collection` field), or
// `null` when the frame is not a master-change push at all.
export const MASTER_CHANGE_STREAM_ID = 'masterChangeStream$';
export function masterChangeStreamId(collection) {
  return `${MASTER_CHANGE_STREAM_ID}:${collection}`;
}
function masterChangeStreamCollection(payload) {
  const id = payload?.id;
  if (typeof id !== 'string') return null;
  if (id === MASTER_CHANGE_STREAM_ID) return '';
  const prefix = `${MASTER_CHANGE_STREAM_ID}:`;
  if (id.startsWith(prefix)) return id.slice(prefix.length);
  return null;
}

function buildSignalingUrl(options) {
  const url = new URL(options.signalingUrl);
  url.searchParams.set('room', options.room);
  url.searchParams.set('peerId', options.clientId);
  url.searchParams.set('client', options.clientId);
  url.searchParams.set('role', options.role);
  url.searchParams.set('protocol', CTOX_RXDB_PROTOCOL);
  if (options.instanceId) url.searchParams.set('instance_id', options.instanceId);
  if (options.roomPassword) url.searchParams.set('room_password', options.roomPassword);
  if (options.token) url.searchParams.set('token', options.token);
  if (options.tokenIssuedAt) url.searchParams.set('token_iat', String(options.tokenIssuedAt));
  if (options.tokenExpiresAt) url.searchParams.set('token_exp', String(options.tokenExpiresAt));
  for (const capability of options.capabilities || []) {
    url.searchParams.append('cap', capability);
  }
  // Re-stamp the token freshness window on EVERY connect attempt, keeping the
  // original TTL length. The window used to be baked into the URL once at
  // page load; a tab older than the TTL (24h) then reconnect-looped forever
  // against "control plane token expired" rejections.
  const issuedAt = Number(url.searchParams.get('token_iat') || 0);
  const expiresAt = Number(url.searchParams.get('token_exp') || 0);
  if (issuedAt > 0 && expiresAt > issuedAt) {
    const ttlSeconds = expiresAt - issuedAt;
    const now = Math.floor(Date.now() / 1000);
    url.searchParams.set('token_iat', String(now));
    url.searchParams.set('token_exp', String(now + ttlSeconds));
  }
  return url.toString();
}

function redactUrl(value) {
  const url = new URL(value);
  for (const key of ['room_password', 'token']) {
    if (url.searchParams.has(key)) {
      url.searchParams.set(key, '[redacted]');
    }
  }
  return url.toString();
}

function randomId(prefix) {
  const bytes = new Uint8Array(8);
  crypto.getRandomValues(bytes);
  const suffix = Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
  return `${prefix}-${suffix}`;
}

function requestObservationKey(peerId, method) {
  return `${peerId || ''}|${method || ''}`;
}

function encodedSize(value) {
  return utf8ByteLength(String(value || ''));
}

function utf8ByteLength(text) {
  let bytes = 0;
  const value = String(text || '');
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    if (code <= 0x7f) {
      bytes += 1;
    } else if (code <= 0x7ff) {
      bytes += 2;
    } else if (code >= 0xd800 && code <= 0xdbff) {
      const next = index + 1 < value.length ? value.charCodeAt(index + 1) : 0;
      if (next >= 0xdc00 && next <= 0xdfff) {
        bytes += 4;
        index += 1;
      } else {
        bytes += 3;
      }
    } else {
      bytes += 3;
    }
  }
  return bytes;
}

// Mirrors Rust `split_chunks_for_frame` (connection_handler_rs.rs): budget
// every chunk by its JSON-ESCAPED byte length. Code-point iteration never
// splits surrogate pairs. AGENT GUARDRAIL: do not "simplify" this back to
// `text.slice(n * CHARS, ...)` — char-based slicing overflows the SCTP frame
// limit for non-ASCII content and the browser kills the DataChannel.
function splitFrameChunks(text, transferId) {
  const envelope = JSON.stringify({
    ctoxFrame: CTOX_FRAME_PROTOCOL,
    kind: 'chunk',
    transferId,
    attempt: Number.MAX_SAFE_INTEGER,
    seq: Number.MAX_SAFE_INTEGER,
    data: '',
  });
  const overhead = encodedSize(envelope);
  // Two ceilings apply: the wire contract's per-chunk payload budget
  // (MAX_CHUNK_CHARS — historical name; the fixture value is a BYTE budget)
  // and the 16 KiB serialized-frame ceiling minus the envelope. Take the
  // stricter one so chunks honor the documented contract AND can never kill
  // the channel.
  const budget = Math.max(1, Math.min(MAX_CHUNK_CHARS, MAX_SERIALIZED_FRAME_BYTES - overhead - 64));
  const value = String(text || '');
  if (!value) return [''];
  const chunks = [];
  let cur = '';
  let curEscaped = 0;
  for (const ch of value) {
    const chEscaped = jsonEscapedCharLen(ch);
    if (curEscaped + chEscaped > budget && cur) {
      chunks.push(cur);
      cur = '';
      curEscaped = 0;
    }
    cur += ch;
    curEscaped += chEscaped;
  }
  if (cur || chunks.length === 0) chunks.push(cur);
  return chunks;
}

// Test-only surface (mirrors replicationWebRtcTestInternals): lets the smoke
// suite assert the frame-chunking invariants without reaching into private
// scope. Not part of the public CTOX DB API.
export const webrtcNativeTestInternals = Object.freeze({
  splitFrameChunks,
  jsonEscapedCharLen,
  encodedSize,
  utf8ByteLength,
  recordReceivedFrame,
  MAX_SERIALIZED_FRAME_BYTES,
});

// JSON-escaped UTF-8 byte length of one code point, matching Rust
// `json_escaped_char_len`.
function jsonEscapedCharLen(ch) {
  const code = ch.codePointAt(0);
  if (ch === '"' || ch === '\\') return 2;
  if (code === 0x08 || code === 0x09 || code === 0x0a || code === 0x0c || code === 0x0d) return 2;
  if (code < 0x20) return 6;
  if (code <= 0x7f) return 1;
  if (code <= 0x7ff) return 2;
  if (code >= 0xd800 && code <= 0xdfff) return 6; // lone surrogate -> \uXXXX
  if (code <= 0xffff) return 3;
  return 4;
}

function recordReceivedFrame(entry, seq, data) {
  const hadFrame = entry.received.has(seq);
  entry.received.set(seq, data);
  if (!hadFrame && seq === Number(entry.contiguousSeq ?? -1) + 1) {
    while (
      entry.contiguousSeq + 1 < entry.totalFrames
      && entry.received.has(entry.contiguousSeq + 1)
    ) {
      entry.contiguousSeq += 1;
    }
  }
  return Number(entry.contiguousSeq ?? -1);
}

function createSendQueue() {
  return {
    high: [],
    normal: [],
    low: [],
    draining: false,
    nextSequence: 0,
  };
}

function nextQueuedSend(queue) {
  for (const priority of SEND_PRIORITIES) {
    if (queue[priority].length) {
      return queue[priority].shift();
    }
  }
  return null;
}

function nextHighPriorityInlineSend(queue) {
  if (!queue?.high?.length) return null;
  const index = queue.high.findIndex((item) => item?.inline);
  if (index < 0) return null;
  return queue.high.splice(index, 1)[0] || null;
}

function shouldRecycleConnectionAfterRequestTimeout(method = '') {
  return ['ctoxProtocol', 'token'].includes(String(method || ''));
}

function classifySendPriority(payload = {}, text = '') {
  if (payload?.ctoxFrame === CTOX_FRAME_PROTOCOL) {
    return ['ack', 'resume', 'start'].includes(payload.kind) ? 'high' : 'normal';
  }
  const method = String(payload?.method || '');
  // Phase 2: the `rxdb.activeCollections` priority hint must reach the native
  // peer ahead of any bulk backlog so foreground prioritization takes effect
  // promptly. Treat it as a control frame (high).
  if ([
    'ctoxProtocol',
    'token',
    'rxdb.activeCollections',
    'masterChangesSince',
    'rxdb.query.fetch',
    'rxdb.query.cancel',
    'rxdb.file.fetch',
    'rxdb.file.cancel',
  ].includes(method)) return 'high';
  if (method === 'masterWrite' && encodedSize(text) > MAX_INLINE_FRAME_BYTES) return 'low';
  if (method === 'masterWrite') return 'high';
  if (payload?.id && (Object.prototype.hasOwnProperty.call(payload, 'result') || Object.prototype.hasOwnProperty.call(payload, 'error'))) {
    return 'high';
  }
  return 'normal';
}

function frameAckKey(transferId, ackSeq) {
  return `${transferId}|${ackSeq == null ? 'final' : ackSeq}`;
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
