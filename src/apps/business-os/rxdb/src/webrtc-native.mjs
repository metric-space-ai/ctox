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

// Connection lifecycle invariants pinned by tests: token freshness re-stamp
// per connect, yourPeerId-only identity adoption, joined-based backoff reset
// (signaling-freshness-smoke), byte-budgeted frame chunking mirroring the
// Rust splitter (frame-chunking-smoke), immediate RTC connection creation
// without admission/eviction (rtc-admission-removal-smoke), and the
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
const SEND_BUFFER_STALL_TIMEOUT_MS = 30_000;
// The browser creates `ctox-rxdb`; the native peer's long-standing default is
// `rxdb`. Either side can win offer creation after a cold start, so both labels
// identify the one primary replication channel. Every other label is an
// auxiliary channel and is handed to its consumer untouched — see
// attachChannel / openAuxChannel.
const CTOX_REPLICATION_CHANNEL_LABEL = 'ctox-rxdb';
const CTOX_REPLICATION_CHANNEL_LABELS = new Set([
  CTOX_REPLICATION_CHANNEL_LABEL,
  'rxdb',
]);
const CTOX_OUTBOUND_SELLIFY_LOOKUP_METHOD = 'ctox.outbound.sellify_lookup.v1';
const MAX_PEER_SEND_QUEUE_FRAMES = 1024;
const MAX_PEER_SEND_QUEUE_BYTES = 16 * 1024 * 1024;
const FAIR_SEND_SCHEDULE = ['high', 'high', 'high', 'high', 'normal', 'normal', 'low'];
// Hard wire invariant shared with the Rust peer (MAX_SERIALIZED_FRAME_BYTES in
// connection_handler_rs.rs): a single serialized DataChannel message must stay
// <= 16 KiB or browsers kill the channel. Chunks are budgeted by their
// JSON-ESCAPED byte length against this ceiling — NOT by UTF-16 char count.
const MAX_SERIALIZED_FRAME_BYTES = 16384;
const FRAME_ACK_TIMEOUT_MS = 30_000;
// M3: an incoming multi-frame transfer that makes NO progress for this long is
// abandoned and its buffered chunks (up to MAX_TRANSFER_BYTES) are freed. A
// generous multiple of the frame-ack timeout so a slow-but-live transfer that
// keeps landing frames (each resets `lastProgressAt`) is never discarded.
const STALLED_INCOMING_TRANSFER_TIMEOUT_MS = FRAME_ACK_TIMEOUT_MS * 3;
// Reserve aggregate receive capacity when a `start` frame is admitted. The
// per-transfer wire ceiling is 8 MiB; four full-size transfers (32 MiB total)
// and at most eight concurrent transfers keep browser memory bounded while
// preserving normal multiplexed traffic headroom.
const MAX_INCOMING_FRAME_TRANSFERS = 8;
const MAX_INCOMING_FRAME_BUFFERED_BYTES = MAX_TRANSFER_BYTES * 4;
const FRAME_RESUME_TIMEOUT_MS = 1_000;
const COMPLETED_FRAME_ACK_TTL_MS = 60_000;
const RTC_HANDSHAKE_TIMEOUT_MS = 60_000;
const RECENT_RTC_EVENT_LIMIT = 40;
const TERMINAL_SIGNALING_REJECTION_CODES = new Set([
  'protocol_missing',
  'protocol_mismatch',
  'instance_mismatch',
  'role_auth_missing',
  'role_auth_binding_invalid',
  'role_credential_invalid',
  'peer_revoked',
  'role_mismatch',
  'token_invalid',
  'token_signature_invalid',
  'credentials_revoked',
]);
const RETRYABLE_SIGNALING_REJECTION_CODES = new Set([
  'control_plane_token_expired',
  'temporary_unavailable',
]);
// Grace window for transient ICE 'disconnected' before tearing the
// connection down (mirrors the Rust peer's keep-through-Disconnected rule).
// Chromium can report `disconnected` while a busy local peer drains a large
// replicated/query-fetch burst. Eight seconds was shorter than legitimate
// Business OS app fan-out and tore down all multiplexed collections at once.
// Keep the transport through a bounded recovery window; `failed` and `closed`
// remain immediate terminal states.
const ICE_DISCONNECTED_GRACE_MS = 30_000;
// Signaling reconnection backoff. Post-multiplex the whole room shares ONE
// signaling socket; a clean close used to only emit `signaling-close` (which had
// no listener), so the peer could never re-discover the native side until an
// external restart. The peer now self-reconnects the socket with exponential
// backoff and re-joins the room (re-broadcasting the peer list), complementing
// sync.js's higher-level restart engine.
const SIGNALING_RECONNECT_BASE_MS = 1_000;
const SIGNALING_RECONNECT_MAX_MS = 30_000;
// SYNC-30: TURN credential refresh before a (re)connect. The native peer mints
// ephemeral coturn credentials with a ~1h TTL and advertises a control-plane
// refresh URL; a relay-dependent session that drops after >1h reconnected with
// EXPIRED creds and could only recover via a full page reload. Before building a
// new RTCPeerConnection we refresh the ICE server list from the shell-supplied
// control-plane callback (NOT a data path — the fetch lives in shared/sync.js
// against the allowlisted sync-config endpoint) when the current credential is
// within the skew of its expiry. A failed refresh degrades to reconnecting with
// the existing (possibly STUN-only) servers rather than wedging. The min-interval
// guard bounds refresh attempts and, critically, guarantees a deferred connect
// re-drives exactly once so a failing refresh cannot loop.
const ICE_SERVERS_REFRESH_SKEW_MS = 120_000;
const ICE_SERVERS_REFRESH_MIN_INTERVAL_MS = 60_000;
const TRANSPORT_STATUS_EMIT_MIN_INTERVAL_MS = 250;
// Server-assigned signaling ids are ephemeral revocation handles. Keep the
// exact accepted value, but reject unbounded input rather than truncating it
// into a different (and therefore non-revocable) peer id.
const MAX_LOCAL_SIGNALING_PEER_ID_LENGTH = 256;
// Single source of truth for the shell-critical collection set. app.js derives
// its CRITICAL_SYNC_COLLECTIONS from this exported list so the two lists cannot
// silently drift. This is shell-priority data only; RTC connections are never
// admitted, queued, or evicted based on collection membership.
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
    iceServersRefreshUrl = '',
    refreshIceServers = null,
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
      iceServersRefreshUrl,
      refreshIceServers: typeof refreshIceServers === 'function' ? refreshIceServers : null,
      storageToken,
      expectedNativePeerId,
      protocolPayload,
      requestHandlers,
    };
    // SYNC-30: TURN-credential refresh bookkeeping. `lastIceServersRefreshAtMs`
    // advances on every attempt (success OR failure) so a deferred connect
    // re-drives exactly once and never loops on a failing refresh.
    this.iceServersRefreshInFlight = null;
    this.lastIceServersRefreshAtMs = 0;
    this.events = new CtoxEventEmitter();
    this.socket = null;
    // `options.clientId` remains the deterministic pre-handshake identity used
    // in the signaling URL and local request/session ids. The signaling server
    // assigns a separate ephemeral peer id in init.yourPeerId.
    this.localSignalingPeerId = '';
    this.connections = new Map();
    // label -> { label, options }. A standing subscription, not a one-shot:
    // survives reconnects and is replayed onto every new PeerConnection.
    this.auxChannelRegistrations = new Map();
    this.peerMetadata = new Map();
    this.pending = new Map();
    this.pendingFrameAcks = new Map();
    this.incomingFrames = new Map();
    this.auxIncomingFrames = new Map();
    this.auxMessageStats = {
      messagesReceived: 0,
      chunkMessagesReceived: 0,
      responsesResolved: 0,
      responsesWithoutPendingRequest: 0,
      parseErrors: 0,
      incompleteTransfers: 0,
      lastMessageAtMs: 0,
      lastResponseAtMs: 0,
    };
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
      maxIncomingFrameTransfers: MAX_INCOMING_FRAME_TRANSFERS,
      maxIncomingFrameBufferedBytes: MAX_INCOMING_FRAME_BUFFERED_BYTES,
      incomingFrameBufferedBytes: 0,
      incomingFrameReservedBytes: 0,
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
      backpressureStallCount: 0,
      queuedFrames: 0,
      sentScheduledFrames: 0,
      priorityQueueDepth: 0,
      highPriorityQueueDepth: 0,
      normalPriorityQueueDepth: 0,
      lowPriorityQueueDepth: 0,
      queuedBytes: 0,
      rejectedFrames: 0,
      oldestQueuedAgeMs: 0,
      turnCredentialExpiresAtMs: turnCredentialExpiryMs(iceServers),
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

  currentSignalingPeerId() {
    return this.localSignalingPeerId || String(this.options.clientId || '');
  }

  setLocalSignalingPeerId(value) {
    const next = boundedLocalSignalingPeerId(value);
    if (next === this.localSignalingPeerId) return false;
    this.localSignalingPeerId = next;
    // Identity assignment/clear is low-volume lifecycle state. Bypass the
    // metric throttle so status subscribers can revoke the exact live browser
    // peer promptly and never retain a closed socket's stale id.
    this.transportStats.updatedAtMs = Date.now();
    this.events.emit('transport-status', this.getTransportStatus());
    return true;
  }

  connect() {
    this.closed = false;
    this.setLocalSignalingPeerId('');
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
      if (this.socket === socket) {
        this.socket = null;
        this.setLocalSignalingPeerId('');
      }
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
    this.setLocalSignalingPeerId('');
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
    return this.enqueueSendFrame(connection, {
      payload,
      text,
      inline: encodedSize(text) <= MAX_INLINE_FRAME_BYTES,
      priority: classifySendPriority(payload, text),
    });
  }

  enqueueSendFrame(connection, item) {
    if (!connection.sendQueue) {
      connection.sendQueue = createSendQueue();
    }
    const queue = connection.sendQueue;
    const itemBytes = encodedSize(item.text);
    const queuedFrames = queue.high.length + queue.normal.length + queue.low.length;
    if (
      queuedFrames >= MAX_PEER_SEND_QUEUE_FRAMES
      || queue.queuedBytes + itemBytes > MAX_PEER_SEND_QUEUE_BYTES
    ) {
      this.recordTransportStatus({ rejectedFrames: this.transportStats.rejectedFrames + 1 });
      this.events.emit('error', {
        code: 'ctox_webrtc_send_queue_budget_exceeded',
        peerId: connection.remotePeerId,
        queuedFrames,
        queuedBytes: queue.queuedBytes,
        maxFrames: MAX_PEER_SEND_QUEUE_FRAMES,
        maxBytes: MAX_PEER_SEND_QUEUE_BYTES,
      });
      this.removeConnection(connection.remotePeerId, 'send-queue-budget-exceeded');
      return false;
    }
    queue[item.priority].push({
      ...item,
      byteLength: itemBytes,
      queuedAtMs: Date.now(),
      sequence: queue.nextSequence++,
    });
    queue.queuedBytes += itemBytes;
    // ACKs arriving during a bulk send must wake the control drain directly.
    // Page timers can be throttled in a hidden tab, even with an open channel.
    if (item.inline && item.priority === 'high') queue.controlWake?.();
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
    return true;
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
          await this.waitForSendBuffer(connection.channel, connection);
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
            await this.waitForSendBuffer(channel, connection);
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
    const queue = connection.sendQueue;
    // Direct sendFramed callers have no scheduler to drain.
    if (!queue) return ackPromise;
    try {
      while (!settled && this.connections.get(connection.remotePeerId) === connection && connection.channel?.readyState === 'open') {
        // Arm before draining: an enqueue during the await must not be lost.
        const enqueued = new Promise((resolve) => { queue.controlWake = resolve; });
        await this.drainHighPriorityInlineFrames(connection);
        const result = await Promise.race([wrapped, enqueued.then(() => null)]);
        if (result) {
          if (result.ok) return result.value;
          throw result.error;
        }
      }
      const result = await wrapped;
      if (result.ok) return result.value;
      throw result.error;
    } finally {
      queue.controlWake = null;
    }
  }

  async drainHighPriorityInlineFrames(connection) {
    const queue = connection.sendQueue;
    if (!queue) return;
    while (connection.channel?.readyState === 'open') {
      const item = nextHighPriorityInlineSend(queue);
      if (!item) break;
      this.refreshSendQueueStatus(connection);
      await this.waitForSendBuffer(connection.channel, connection);
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

  waitForSendBuffer(channel, connection = null) {
    if (Number(channel.bufferedAmount || 0) <= SEND_BUFFER_HIGH_WATER) {
      return Promise.resolve();
    }
    this.recordTransportStatus({
      backpressureWaitCount: this.transportStats.backpressureWaitCount + 1,
      lastBufferedAmount: Number(channel.bufferedAmount || 0),
    });
    return new Promise((resolve, reject) => {
      const previousThreshold = channel.bufferedAmountLowThreshold;
      channel.bufferedAmountLowThreshold = SEND_BUFFER_LOW_WATER;
      let timer = null;
      const cleanup = () => {
        channel.removeEventListener?.('bufferedamountlow', done);
        channel.bufferedAmountLowThreshold = previousThreshold || 0;
        if (timer) clearTimeout(timer);
      };
      const done = () => {
        cleanup();
        resolve();
      };
      channel.addEventListener?.('bufferedamountlow', done, { once: true });
      timer = setTimeout(() => {
        cleanup();
        this.recordTransportStatus({
          backpressureStallCount: this.transportStats.backpressureStallCount + 1,
          rejectedFrames: this.transportStats.rejectedFrames + 1,
        });
        const error = new Error('WebRTC send buffer remained above the high-water mark.');
        error.code = 'ctox_webrtc_send_buffer_stalled';
        error.retryable = true;
        error.peerId = connection?.remotePeerId || null;
        this.events.emit('error', error);
        if (connection?.remotePeerId) {
          this.removeConnection(
            connection.remotePeerId,
            'send-buffer-stalled',
            error,
            { reconnect: false },
          );
        }
        reject(error);
      }, SEND_BUFFER_STALL_TIMEOUT_MS);
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
      const sendPromise = (
        method === 'ctox.browser.live.v1'
        || method === CTOX_OUTBOUND_SELLIFY_LOOKUP_METHOD
      )
        ? this.sendImmediateControlFrame(remotePeerId, frame)
        : Promise.resolve(this.send(remotePeerId, frame));
      sendPromise.then((sent) => {
        if (sent) return;
        this.pending.delete(id);
        clearTimeout(timer);
        this.scheduleReconnect(remotePeerId, `send-not-open-${method}`);
        reject(new Error(`WebRTC peer ${remotePeerId} is not open`));
      }).catch((error) => {
        this.pending.delete(id);
        clearTimeout(timer);
        reject(error);
      });
    });
  }

  async requestAuxiliary(remotePeerId, label, method, params = [], timeoutMs = 15000) {
    const peerId = String(remotePeerId || '');
    let channel = null;
    const deadline = Date.now() + Math.min(5_000, timeoutMs);
    while (!channel && Date.now() < deadline) {
      channel = this.connections.get(peerId)?.auxChannels?.get(String(label || '')) || null;
      if (channel?.readyState === 'open') break;
      channel = null;
      await delay(50);
    }
    if (!channel || channel.readyState !== 'open') {
      // The Browser live stream is an auxiliary channel. Failure to open it
      // must never poison or renegotiate the primary RxDB connection; callers
      // can fall back to the bounded command/query path while the auxiliary
      // registration is re-opened on the next genuine peer reconnect.
      if (!this.connections.has(peerId)) {
        this.scheduleReconnect(peerId, `aux-channel-not-open-${method}`);
      }
      throw new Error(`WebRTC auxiliary channel ${label} is not open`);
    }
    const id = `${this.options.clientId}|aux|${Date.now()}|${this.requestCounter++}`;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        // Isolate auxiliary request failure. A slow screenshot/navigation must
        // not tear down collection replication or the command bus.
        reject(new Error(`Timed out waiting for WebRTC auxiliary response ${method}`));
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timer, method, peerId });
      try {
        channel.send(JSON.stringify({ id, method, params }));
      } catch (error) {
        this.pending.delete(id);
        clearTimeout(timer);
        reject(error);
      }
    });
  }

  // Browser input/frame exchanges are control traffic, not replication work.
  // Returning `true` from the ordinary send() only means "queued"; a wedged
  // framed transfer can therefore keep the RPC in that queue until its caller
  // times out without the native peer ever seeing it. Send this small frame on
  // the open DataChannel itself after the same bounded buffer guard. SCTP keeps
  // message boundaries and ordering while this avoids the bulk queue's head of
  // line blocking.
  async sendImmediateControlFrame(remotePeerId, payload) {
    const connection = this.connections.get(String(remotePeerId || ''));
    const channel = connection?.channel;
    if (!connection || !channel || channel.readyState !== 'open') return false;
    const text = JSON.stringify(payload);
    if (encodedSize(text) > MAX_INLINE_FRAME_BYTES) {
      throw new Error('WebRTC control frame exceeds the inline frame budget');
    }
    await this.waitForSendBuffer(channel, connection);
    if (this.connections.get(connection.remotePeerId) !== connection || channel.readyState !== 'open') {
      return false;
    }
    channel.send(text);
    this.recordSentInlineFrame(payload, channel);
    this.recordTransportStatus({
      sentScheduledFrames: this.transportStats.sentScheduledFrames + 1,
      lastSendPriority: 'high',
    });
    return true;
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
    if (message.type === 'ctoxIceServers') {
      const iceServers = signalingIceServers(message.iceServers);
      if (!iceServers.length) {
        this.events.emit('error', { code: 'ctox_signaling_ice_servers_invalid' });
        return;
      }
      // WebSocket message order is load-bearing here: the authenticated
      // signaling service sends this frame before `joined`, and `joined` is
      // what creates peer connections. Thus a compact pairing invite can stay
      // free of bulky TURN credentials without ever starting a host-only RTC
      // attempt first.
      this.options.iceServers = iceServers;
      const turnCredentialExpiresAtMs = turnCredentialExpiryMs(iceServers);
      this.recordTransportStatus({ turnCredentialExpiresAtMs });
      this.events.emit('ice-servers', {
        count: iceServers.length,
        turnCredentialExpiresAtMs,
      });
      return;
    }
    if (message.type === 'init' || message.type === 'joined' || message.type === 'ctoxPresence') {
      // Only the server's bounded init.yourPeerId assigns our ephemeral
      // signaling identity. `message.peerId` on joined/presence frames names a
      // REMOTE peer and must never overwrite either identity.
      if (message.type === 'init') {
        const assignedPeerId = boundedLocalSignalingPeerId(message.yourPeerId);
        if (assignedPeerId) this.setLocalSignalingPeerId(assignedPeerId);
      }
      if (message.type === 'joined') {
        // A joined broadcast proves the server ACCEPTED our join — only now
        // reset the reconnect backoff. Resetting on socket-open degenerated
        // into a 1s-interval storm when the server accepted the socket and
        // then rejected the join (e.g. control-plane errors). It also proves
        // any control-plane rejection retained from an older socket is stale.
        this.signalingReconnectDelayMs = SIGNALING_RECONNECT_BASE_MS;
        this.lastControlPlaneError = null;
      }
      const descriptors = signalingPeerDescriptors(message);
      const previousMetadata = new Map(this.peerMetadata);
      for (const descriptor of descriptors) {
        if (descriptor.peerId) this.rememberPeerMetadata(descriptor.peerId, descriptor);
      }
      this.pruneStaleNativeCandidateConnections(descriptors);
      const expectedNativePeerId = String(this.options.expectedNativePeerId || '').trim();
      for (const descriptor of descriptors) {
        const remotePeerId = descriptor.peerId;
        if (!remotePeerId) continue;
        // A configured native identity is authoritative. Never fall back to a
        // peer that merely self-declares role=ctox_instance while the expected
        // peer is offline or restarting.
        if (expectedNativePeerId && !this.peerMatchesExpectedNativePeerId(remotePeerId, descriptor)) {
          this.removeConnection(remotePeerId, 'signaling-non-target-native-peer');
          continue;
        }
        const previousDescriptor = previousMetadata.get(remotePeerId);
        const nativePeerRejoined = message.type === 'joined'
          && remotePeerId !== this.currentSignalingPeerId()
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
      // M2: a peer-list broadcast names the full current room, so metadata for
      // sessions no longer present is dead weight — each browser reload/tab is a
      // fresh random peer id, so without this the map grows one ~200-500B entry
      // per session ever seen. Prune entries absent from the current room set,
      // but keep any peer with a live connection (its metadata is still in use).
      this.prunePeerMetadata(descriptors);
      this.events.emit('joined', message);
      return;
    }
    if (message.type === 'ctoxError') {
      const error = normalizeSignalingControlPlaneError(message);
      if (error.name === 'CtoxSignalingControlPlaneError') {
        this.lastControlPlaneError = error;
      }
      this.events.emit('error', error);
      if (error.retryable === false) {
        // The server closes immediately after ctoxError. Mark this peer closed
        // before that close event arrives so the reconnect scheduler cannot
        // hammer a permanent auth/protocol rejection. A changed config creates
        // a new shared-room peer and is the explicit recovery edge.
        this.closed = true;
        if (this.signalingReconnectTimer) clearTimeout(this.signalingReconnectTimer);
        this.signalingReconnectTimer = null;
        this.rejectAllPending(error);
      }
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

  // SYNC-30: whether a (re)connect should first refresh the ICE server list.
  // True only when a refresh callback exists, the current TURN credential is
  // within the skew of its expiry, and we have not just attempted a refresh —
  // the min-interval guard is what lets a deferred connect re-drive exactly
  // once (a failed refresh advances `lastIceServersRefreshAtMs`, so the retry
  // sees `false` here and proceeds with the existing servers instead of
  // deferring forever).
  shouldRefreshIceServersBeforeConnect() {
    if (typeof this.options.refreshIceServers !== 'function') return false;
    if (!this.turnCredentialsNearExpiry(ICE_SERVERS_REFRESH_SKEW_MS)) return false;
    return (Date.now() - this.lastIceServersRefreshAtMs) >= ICE_SERVERS_REFRESH_MIN_INTERVAL_MS;
  }

  turnCredentialsNearExpiry(skewMs = 0) {
    const expiresAt = Number(this.transportStats.turnCredentialExpiresAtMs || 0);
    if (!(expiresAt > 0)) return false;
    return Date.now() >= expiresAt - Math.max(0, Number(skewMs) || 0);
  }

  // Refresh the ICE server list (fresh minted TURN credentials) from the
  // shell-supplied control-plane callback. Deduplicates concurrent calls; on
  // failure keeps the existing servers so sync degrades rather than wedges.
  refreshIceServersIfExpiring() {
    if (this.iceServersRefreshInFlight) return this.iceServersRefreshInFlight;
    if (typeof this.options.refreshIceServers !== 'function') return Promise.resolve(false);
    if (!this.turnCredentialsNearExpiry(ICE_SERVERS_REFRESH_SKEW_MS)) return Promise.resolve(false);
    const attempt = (async () => {
      try {
        const fresh = await this.options.refreshIceServers();
        if (Array.isArray(fresh) && fresh.length) {
          this.options.iceServers = fresh;
          this.recordTransportStatus({ turnCredentialExpiresAtMs: turnCredentialExpiryMs(fresh) });
          this.events.emit('ice-servers-refreshed', {
            iceServersConfigured: fresh.length,
            turnCredentialExpiresAtMs: turnCredentialExpiryMs(fresh),
          });
          return true;
        }
        this.events.emit('error', { code: 'ctox_ice_servers_refresh_empty', phase: 'signaling-control-plane' });
        return false;
      } catch (error) {
        // Degrade, do not wedge: fall back to the existing (possibly expired /
        // STUN-only) servers on the next connect attempt.
        this.events.emit('error', {
          code: 'ctox_ice_servers_refresh_failed',
          phase: 'signaling-control-plane',
          message: error?.message || String(error),
        });
        return false;
      } finally {
        this.lastIceServersRefreshAtMs = Date.now();
        this.iceServersRefreshInFlight = null;
      }
    })();
    this.iceServersRefreshInFlight = attempt;
    return attempt;
  }

  // Defer a peer (re)connect until an ICE refresh completes, then re-drive
  // exactly once. A failed refresh still re-drives (with the existing servers),
  // so the connection is never wedged waiting on the control plane.
  deferConnectForIceRefresh(remotePeerId) {
    this.refreshIceServersIfExpiring()
      .catch(() => {})
      .then(() => {
        if (this.closed || this.connections.has(remotePeerId)) return;
        if (!this.shouldConnectToRemotePeer(remotePeerId)) return;
        try {
          this.ensureConnection(remotePeerId);
        } catch (error) {
          this.events.emit('error', normalizePeerSignalError(error, remotePeerId));
        }
      });
  }

  ensureConnection(remotePeerId) {
    if (remotePeerId === this.currentSignalingPeerId()) {
      return this.connections.get(remotePeerId);
    }
    if (!this.shouldConnectToRemotePeer(remotePeerId)) {
      return undefined;
    }
    let connection = this.connections.get(remotePeerId);
    if (connection) {
      return connection;
    }
    if (this.shouldRefreshIceServersBeforeConnect()) {
      // SYNC-30: expiring TURN credentials — refresh before minting the new
      // RTCPeerConnection so a relay-dependent reconnect uses fresh creds
      // (no page reload). The re-driven ensureConnection proceeds regardless
      // of refresh success.
      this.deferConnectForIceRefresh(remotePeerId);
      return undefined;
    }
    return this.createConnection(remotePeerId);
  }

  createConnection(remotePeerId) {
    const peer = new RTCPeerConnection({ iceServers: this.options.iceServers });
    const connection = {
      peer,
      channel: null,
      auxChannels: new Map(),
      remotePeerId,
      pendingCandidates: [],
      createdAtMs: Date.now(),
      lastStateChangeAtMs: Date.now(),
      lastError: null,
      signalStats: createPeerSignalStats(),
      localCandidateTypes: {},
      remoteCandidateTypes: {},
      handshakeTimer: null,
      inboundFrameChain: Promise.resolve(),
      inboundFrameGeneration: 0,
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
      } else if (state === 'connected') {
        updateSelectedCandidatePair(connection).then(() => {
          this.recordConnectionEvent(connection, 'selected-candidate-pair', {
            localCandidateType: connection.signalStats.selectedLocalCandidateType,
            remoteCandidateType: connection.signalStats.selectedRemoteCandidateType,
            protocol: connection.signalStats.selectedCandidateProtocol,
          });
        }).catch(() => {});
      }
    };
    peer.ondatachannel = (event) => this.attachChannel(connection, event.channel);

    if (this.shouldInitiate(remotePeerId, connection)) {
      this.attachChannel(connection, peer.createDataChannel('ctox-rxdb'));
      // Re-open every registered auxiliary channel on THIS connection. A
      // reconnect rebuilds the PeerConnection, so a channel opened once would
      // otherwise die silently — the exact shape of defect that cost this
      // codebase four separate fixes in one night. Registration is therefore a
      // standing subscription, not a one-shot action.
      this.reopenAuxChannels(connection, peer);
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
    return this.currentSignalingPeerId() < String(remotePeerId);
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

  // Channels whose label is not the replication label are handed through
  // untouched. Before this branch existed, EVERY incoming channel was written
  // into the single `connection.channel` slot — a second channel opened by any
  // consumer silently replaced the replication channel while 25 call sites kept
  // reading the slot as if it were still theirs. Opening an auxiliary channel
  // was therefore not "unsupported", it was destructive.
  attachChannel(connection, channel) {
    if (channel?.label && !CTOX_REPLICATION_CHANNEL_LABELS.has(channel.label)) {
      this.attachAuxChannel(connection, channel);
      return;
    }
    connection.channel = channel;
    connection.inboundFrameGeneration = Number(connection.inboundFrameGeneration || 0) + 1;
    connection.inboundFrameChain = Promise.resolve();
    channel.onopen = () => {
      if (connection.handshakeTimer) {
        clearTimeout(connection.handshakeTimer);
        connection.handshakeTimer = null;
      }
      this.forceInitiatorPeers.delete(connection.remotePeerId);
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
      this.enqueueInboundDataChannelFrame(connection, channel, payload);
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

  enqueueInboundDataChannelFrame(connection, channel, payload) {
    if (
      this.closed
      || this.connections.get(connection.remotePeerId) !== connection
      || connection.channel !== channel
    ) {
      return Promise.resolve();
    }
    const generation = Number(connection.inboundFrameGeneration || 0);
    const previous = connection.inboundFrameChain || Promise.resolve();
    const current = previous.then(async () => {
      if (
        this.closed
        || this.connections.get(connection.remotePeerId) !== connection
        || connection.channel !== channel
        || Number(connection.inboundFrameGeneration || 0) !== generation
      ) return;
      await this.handleDataChannelFrame(connection.remotePeerId, payload);
    });
    connection.inboundFrameChain = current.catch((error) => {
      // Keep the per-connection queue fulfilled after a bad frame so all later
      // frames still run. Teardown/new-channel generations suppress stale work.
      if (
        this.connections.get(connection.remotePeerId) !== connection
        || connection.channel !== channel
        || Number(connection.inboundFrameGeneration || 0) !== generation
      ) return;
      connection.lastError = {
        code: 'ctox_webrtc_inbound_frame_failed',
        peerId: connection.remotePeerId,
        message: error?.message || String(error),
      };
      this.events.emit('error', connection.lastError);
    });
    return connection.inboundFrameChain;
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
      if (
        !transferId
        || totalFrames < 1
        || totalFrames > 100_000
        || !Number.isFinite(totalBytes)
        || totalBytes < 0
        || totalBytes > MAX_TRANSFER_BYTES
      ) {
        this.events.emit('error', {
          code: 'ctox_webrtc_frame_start_invalid',
          peerId,
          transferId,
          totalBytes,
        });
        return;
      }
      // Reclaim stale reservations before admission, then reserve the declared
      // transfer size. Excluding the same transfer id preserves retry semantics:
      // a repeated `start` replaces its prior attempt instead of double-counting.
      this.sweepStalledIncomingTransfers();
      const replacing = this.incomingFrames.has(transferId);
      const incomingTransfers = this.incomingFrames.size - (replacing ? 1 : 0);
      const reservedBytes = this.incomingFrameReservedBytes(transferId);
      if (
        incomingTransfers + 1 > MAX_INCOMING_FRAME_TRANSFERS
        || reservedBytes + totalBytes > MAX_INCOMING_FRAME_BUFFERED_BYTES
      ) {
        this.recordTransportStatus({ rejectedFrames: this.transportStats.rejectedFrames + 1 });
        this.events.emit('error', {
          code: 'ctox_webrtc_incoming_transfer_budget_exceeded',
          peerId,
          transferId,
          requestedBytes: totalBytes,
          incomingTransfers,
          reservedBytes,
          maxTransfers: MAX_INCOMING_FRAME_TRANSFERS,
          maxBufferedBytes: MAX_INCOMING_FRAME_BUFFERED_BYTES,
        });
        return;
      }
      this.incomingFrames.set(transferId, {
        peerId,
        totalFrames,
        totalBytes,
        bufferedBytes: 0,
        received: new Map(),
        createdAt: Date.now(),
        // M3: advanced on every genuinely-new frame (see recordReceivedFrame);
        // the stall sweep ages a transfer by time-since-progress, not birth, so
        // a slow-but-live transfer is never discarded.
        lastProgressAt: Date.now(),
        attempt: Number(payload.attempt || 0),
        contiguousSeq: -1,
        nextAckSeq: Math.min(FRAME_ACK_WINDOW - 1, totalFrames - 1),
      });
      this.completedFrameAcks.delete(transferId);
      this.cleanupCompletedFrameAcks();
      this.recordTransportStatus({
        incomingTransfers: this.incomingFrames.size,
        incomingFrameBufferedBytes: this.incomingFrameBufferedBytes(),
        incomingFrameReservedBytes: this.incomingFrameReservedBytes(),
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
      const completed = this.completedFrameAcks.get(transferId);
      if (completed && completed.peerId === peerId) {
        // The sender can repeat the last window when our final ACK was lost.
        // Re-ACK the completed transfer instead of tearing down the shared
        // multiplexed peer; the payload has already been delivered exactly once.
        this.send(peerId, {
          ctoxFrame: CTOX_FRAME_PROTOCOL,
          kind: 'ack',
          transferId,
          ackSeq: completed.ackSeq,
          receivedFrames: completed.receivedFrames,
          final: true,
        });
        return;
      }
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
    const chunkData = String(payload.data || '');
    const previousChunkBytes = entry.received.has(seq)
      ? encodedSize(entry.received.get(seq) || '')
      : 0;
    const nextBufferedBytes = Number(entry.bufferedBytes || 0) - previousChunkBytes + encodedSize(chunkData);
    if (nextBufferedBytes > entry.totalBytes) {
      this.incomingFrames.delete(transferId);
      this.recordTransportStatus({
        rejectedFrames: this.transportStats.rejectedFrames + 1,
        incomingTransfers: this.incomingFrames.size,
        incomingFrameBufferedBytes: this.incomingFrameBufferedBytes(),
        incomingFrameReservedBytes: this.incomingFrameReservedBytes(),
      });
      this.events.emit('error', {
        code: 'ctox_webrtc_frame_buffer_exceeds_declared_size',
        peerId,
        transferId,
        bufferedBytes: nextBufferedBytes,
        declaredBytes: entry.totalBytes,
      });
      return;
    }
    entry.bufferedBytes = nextBufferedBytes;
    const contiguousSeq = recordReceivedFrame(entry, seq, chunkData);
    if (entry.received.size !== entry.totalFrames) {
      this.recordTransportStatus({
        incomingFrameBufferedBytes: this.incomingFrameBufferedBytes(),
      });
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
      this.recordTransportStatus({
        incomingTransfers: this.incomingFrames.size,
        incomingFrameBufferedBytes: this.incomingFrameBufferedBytes(),
        incomingFrameReservedBytes: this.incomingFrameReservedBytes(),
      });
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
    this.sweepStalledIncomingTransfers();
    this.recordTransportStatus({
      incomingTransfers: this.incomingFrames.size,
      incomingFrameBufferedBytes: this.incomingFrameBufferedBytes(),
      incomingFrameReservedBytes: this.incomingFrameReservedBytes(),
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
      senderPeerId: this.currentSignalingPeerId(),
      receiverPeerId: remotePeerId,
      receiver: remotePeerId,
      target: remotePeerId,
      data: signal,
    }));
    return true;
  }

  // Register an auxiliary channel. Returns the live RTCDataChannel when the peer
  // is already connected, otherwise null — the 'aux-channel' event fires as soon
  // as it exists, and again after every reconnect. Callers must handle the event,
  // not the return value, if they want to survive a reconnect.
  openAuxChannel(peerId, label, options = {}) {
    const key = String(label || '').trim();
    if (!key || CTOX_REPLICATION_CHANNEL_LABELS.has(key)) {
      throw new Error(`openAuxChannel: invalid label "${label}"`);
    }
    this.auxChannelRegistrations.set(key, { label: key, options: { ...options } });
    const connection = this.connections.get(String(peerId || ''));
    if (!connection?.peer) return null;
    const existing = connection.auxChannels?.get(key);
    if (existing && existing.readyState !== 'closed') return existing;
    return this.createAuxChannel(connection, connection.peer, key, options);
  }

  closeAuxChannel(label) {
    const key = String(label || '').trim();
    this.auxChannelRegistrations.delete(key);
    for (const connection of this.connections.values()) {
      const channel = connection.auxChannels?.get(key);
      if (!channel) continue;
      try { channel.close(); } catch {}
      connection.auxChannels.delete(key);
    }
  }

  createAuxChannel(connection, peer, label, options = {}) {
    let channel = null;
    try {
      channel = peer.createDataChannel(label, options);
    } catch (error) {
      this.events.emit('error', { code: 'ctox_aux_channel_failed', label, error });
      return null;
    }
    this.attachAuxChannel(connection, channel);
    return channel;
  }

  attachAuxChannel(connection, channel) {
    const label = String(channel?.label || '');
    if (!label) return;
    connection.auxChannels ??= new Map();
    const previous = connection.auxChannels.get(label);
    if (previous && previous !== channel) {
      try { previous.close(); } catch {}
    }
    connection.auxChannels.set(label, channel);
    channel.onmessage = (event) => {
      this.handleAuxiliaryChannelMessage(connection, label, event.data).catch((error) => {
        this.events.emit("error", { code: "ctox_aux_message_failed", label, error });
      });
    };
    channel.onclose = () => {
      if (connection.auxChannels?.get(label) === channel) {
        connection.auxChannels.delete(label);
      }
    };
    this.events.emit('aux-channel', { peerId: connection.remotePeerId, label, channel });
  }

  async handleAuxiliaryChannelMessage(connection, label, raw) {
    this.auxMessageStats.messagesReceived += 1;
    this.auxMessageStats.lastMessageAtMs = Date.now();
    let payload;
    try { payload = JSON.parse(String(raw || '')); } catch {
      this.auxMessageStats.parseErrors += 1;
      return;
    }
    // The native peer answers over whichever channel it considers cheapest,
    // and for a response that exceeds the inline budget it uses the ACK-based
    // transport framing — on this auxiliary channel. Measured on a customer
    // instance: the browser sent `ctoxProtocol` on the primary channel, the
    // peer replied with a 7-frame / 96 KB transfer on `ctox-browser-live-v1`,
    // and this handler dropped every frame because it only knew the aux
    // framing. No ACK went back, the sender stalled after its first window of
    // four, the handshake timed out, and the whole data plane never came up —
    // silently, on every reconnect. Transport frames belong to the shared
    // reassembler regardless of which channel carried them; its ACKs go out on
    // the primary channel, where the peer correlates them by transfer id.
    if (payload?.ctoxFrame === CTOX_FRAME_PROTOCOL) {
      this.auxMessageStats.transportFramesReceived =
        Number(this.auxMessageStats.transportFramesReceived || 0) + 1;
      await this.handleTransportFrame(connection.remotePeerId, payload);
      return;
    }
    if (payload?.ctoxAuxFrame === 'ctox-aux-frame-v1') {
      this.auxMessageStats.chunkMessagesReceived += 1;
      const key = `${connection.remotePeerId}|${label}|${payload.transferId || ''}`;
      let entry = this.auxIncomingFrames.get(key);
      if (!entry) {
        entry = { total: Number(payload.total || 0), chunks: new Map() };
        this.auxIncomingFrames.set(key, entry);
      }
      entry.chunks.set(Number(payload.seq), String(payload.data || ''));
      this.auxMessageStats.incompleteTransfers = this.auxIncomingFrames.size;
      if (entry.total <= 0 || entry.chunks.size < entry.total) return;
      this.auxIncomingFrames.delete(key);
      this.auxMessageStats.incompleteTransfers = this.auxIncomingFrames.size;
      let text = '';
      for (let index = 0; index < entry.total; index += 1) text += entry.chunks.get(index) || '';
      try { payload = JSON.parse(text); } catch {
        this.auxMessageStats.parseErrors += 1;
        return;
      }
    }
    // The peer also *asks* over this channel, not just answers. The handshake's
    // second leg is a `token` request from the native peer, and dropping it
    // here left the browser waiting for a request that had already arrived —
    // "Timed out waiting for remote WebRTC request token" on every collection
    // while the frame counters showed the traffic. Requests belong to the same
    // dispatcher as on the primary channel; it records the observation that
    // releases the handshake and answers over the primary channel, which the
    // peer correlates by request id.
    if (payload?.method) {
      await this.handleDataChannelFrame(connection.remotePeerId, payload);
      return;
    }
    if (!payload?.id || (!Object.prototype.hasOwnProperty.call(payload, 'result') && !Object.prototype.hasOwnProperty.call(payload, 'error'))) return;
    const pending = this.pending.get(payload.id);
    if (!pending) {
      this.auxMessageStats.responsesWithoutPendingRequest += 1;
      // Not every id-carrying message answers a request of ours. The peer
      // pushes master-change notifications the same way, and they are what
      // tells a collection to pull again. Dropping them here left replication
      // "connected/complete" while it silently stopped moving: measured on a
      // customer instance, the browser sat nine minutes behind the store and
      // never saw the session it had just started. Hand anything unmatched to
      // the shared dispatcher, which knows the push shapes.
      await this.handleDataChannelFrame(connection.remotePeerId, payload);
      return;
    }
    this.pending.delete(payload.id);
    clearTimeout(pending.timer);
    this.auxMessageStats.responsesResolved += 1;
    this.auxMessageStats.lastResponseAtMs = Date.now();
    if (payload.error) pending.reject(payload.error);
    else pending.resolve(payload.result);
  }

  reopenAuxChannels(connection, peer) {
    if (!this.auxChannelRegistrations.size) return;
    for (const { label, options } of this.auxChannelRegistrations.values()) {
      this.createAuxChannel(connection, peer, label, options);
    }
  }

  closeAuxChannelsFor(connection) {
    if (!connection?.auxChannels?.size) return;
    for (const channel of connection.auxChannels.values()) {
      try { channel.close(); } catch {}
    }
    connection.auxChannels.clear();
  }

  removeConnection(remotePeerId, reason = 'closed', pendingError = null, { reconnect = true } = {}) {
    const peerId = String(remotePeerId || '');
    this.clearObservedRequestsForPeer(
      peerId,
      pendingError || createPeerClosedError(peerId, reason),
    );
    const connection = this.connections.get(peerId);
    if (!connection) return;
    this.connections.delete(peerId);
    connection.inboundFrameGeneration = Number(connection.inboundFrameGeneration || 0) + 1;
    connection.inboundFrameChain = null;
    if (connection.channel) connection.channel.onmessage = null;
    if (connection.handshakeTimer) {
      clearTimeout(connection.handshakeTimer);
      connection.handshakeTimer = null;
    }
    try { connection.channel?.close?.(); } catch {}
    this.closeAuxChannelsFor(connection);
    try { connection.peer?.close?.(); } catch {}
    this.rejectPendingForPeer(peerId, pendingError || createPeerClosedError(peerId, reason));
    this.events.emit('peer-close', { peerId, reason });
    if (reconnect && reason !== 'peer-close') {
      if (this.auxChannelRegistrations.size > 0) this.forceInitiatorPeers.add(peerId);
      this.scheduleReconnect(peerId, reason);
    }
  }

  clearObservedRequestsForPeer(peerId, error = null) {
    const prefix = `${String(peerId || '')}|`;
    for (const key of [...this.observedRequests.keys()]) {
      if (key.startsWith(prefix)) this.observedRequests.delete(key);
    }
    for (const [key, waiters] of [...this.requestWaiters.entries()]) {
      if (!key.startsWith(prefix)) continue;
      this.requestWaiters.delete(key);
      for (const waiter of waiters) {
        clearTimeout(waiter.timer);
        waiter.reject(error || createPeerClosedError(peerId, 'peer-close'));
      }
    }
  }

  rememberPeerMetadata(peerId, metadata = {}) {
    const normalized = normalizePeerMetadata({ ...metadata, peerId });
    if (!normalized.peerId || normalized.peerId === this.currentSignalingPeerId()) return;
    this.peerMetadata.set(normalized.peerId, {
      ...(this.peerMetadata.get(normalized.peerId) || {}),
      ...normalized,
    });
  }

  // M2: bound peerMetadata to the live room. Drop entries whose peer id is not
  // in the latest room descriptor set AND has no live connection. Self is never
  // stored (rememberPeerMetadata skips it). A no-op when `descriptors` is empty,
  // so a delta/edge broadcast that omits the peer list never wipes the map.
  prunePeerMetadata(descriptors = []) {
    const present = new Set();
    for (const descriptor of Array.isArray(descriptors) ? descriptors : []) {
      if (descriptor?.peerId) present.add(String(descriptor.peerId));
    }
    if (present.size === 0) return 0;
    let removed = 0;
    for (const peerId of [...this.peerMetadata.keys()]) {
      if (present.has(peerId)) continue;
      if (this.connections.has(peerId)) continue;
      this.peerMetadata.delete(peerId);
      removed += 1;
    }
    return removed;
  }

  shouldConnectToRemotePeer(remotePeerId) {
    const peerId = String(remotePeerId || '');
    if (!peerId || peerId === this.currentSignalingPeerId()) return false;
    const metadata = this.peerMetadata.get(peerId);
    const expectedNativePeerId = String(this.options.expectedNativePeerId || '').trim();
    if (expectedNativePeerId) {
      return this.peerMatchesExpectedNativePeerId(peerId, metadata);
    }
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
    this.recordTransportStatus({
      incomingTransfers: this.incomingFrames.size,
      incomingFrameBufferedBytes: this.incomingFrameBufferedBytes(),
      incomingFrameReservedBytes: this.incomingFrameReservedBytes(),
    });
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
      incomingFrameBufferedBytes: 0,
      incomingFrameReservedBytes: 0,
      completedAckCacheSize: 0,
      priorityQueueDepth: 0,
      highPriorityQueueDepth: 0,
      normalPriorityQueueDepth: 0,
      lowPriorityQueueDepth: 0,
    });
  }

  getTransportStatus({ includeDiagnostics = false } = {}) {
    const auxiliary = {
      ...this.auxMessageStats,
      registrations: [...this.auxChannelRegistrations.keys()],
      channels: [...this.connections.values()].flatMap((connection) =>
        [...(connection.auxChannels?.entries?.() || [])].map(([label, channel]) => ({
          peerId: connection.remotePeerId,
          label,
          readyState: channel?.readyState || '',
          bufferedAmount: Number(channel?.bufferedAmount || 0),
        }))),
    };
    // Read-only, secret-free field evidence for production smoke tests. The
    // advanced-status projection intentionally flattens transport fields, so
    // keep the auxiliary lane's own counters directly observable as well.
    globalThis.CTOX_RXDB_AUX_STATUS = auxiliary;
    const base = {
      ...this.transportStats,
      pageHidden: globalThis.document?.hidden === true,
      // Evidence of a delayed page timer, not proof that Chrome caused it.
      throttled: globalThis.document?.hidden === true
        && Number(this.transportStats.lastPageTimerDelayMs || 0) >= 750
        && Date.now() - Number(this.transportStats.lastPageTimerSampleAtMs || 0) < 30_000,
      collection: collectionNameFromTopic(this.options.room),
      topic: this.options.room,
      localSignalingPeerId: this.localSignalingPeerId || null,
      activePeerCount: this.connections.size,
      pendingAcks: this.pendingFrameAcks.size,
      pendingRequests: this.pending.size,
      incomingTransfers: this.incomingFrames.size,
      incomingFrameBufferedBytes: this.incomingFrameBufferedBytes(),
      incomingFrameReservedBytes: this.incomingFrameReservedBytes(),
      completedAckCacheSize: this.completedFrameAcks.size,
      connectionCount: this.connections.size,
      auxiliary,
    };
    if (!includeDiagnostics) return base;
    return {
      ...base,
      pendingRequestMethods: [...this.pending.values()].map((pending) => pending.method || '').filter(Boolean).slice(-20),
      observedRequestMethods: [...this.observedRequests.keys()].map((key) => String(key).split('|').slice(1).join('|')).slice(-20),
      rtcConnections: [...this.connections.values()].map((connection) => peerConnectionSnapshot(connection, this.options.room)),
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
    const waitMs = Math.max(0, TRANSPORT_STATUS_EMIT_MIN_INTERVAL_MS - elapsed);
    const expectedAtMs = now + waitMs;
    this.transportStatusEmitTimer = setTimeout(() => {
      this.transportStatusEmitTimer = null;
      if (this.closed) return;
      this.lastTransportStatusEmitAtMs = Date.now();
      this.transportStats.lastPageTimerDelayMs = Math.max(0, this.lastTransportStatusEmitAtMs - expectedAtMs);
      this.transportStats.lastPageTimerSampleAtMs = this.lastTransportStatusEmitAtMs;
      this.events.emit('transport-status', this.getTransportStatus());
    }, waitMs);
  }

  refreshSendQueueStatus(connection = null) {
    let high = 0;
    let normal = 0;
    let low = 0;
    let queuedBytes = 0;
    let oldestQueuedAtMs = 0;
    const connections = connection ? [connection] : this.connections.values();
    for (const entry of connections) {
      const queue = entry?.sendQueue;
      if (!queue) continue;
      high += queue.high.length;
      normal += queue.normal.length;
      low += queue.low.length;
      queuedBytes += Number(queue.queuedBytes || 0);
      for (const item of [...queue.high, ...queue.normal, ...queue.low]) {
        const queuedAtMs = Number(item?.queuedAtMs || 0);
        if (queuedAtMs > 0 && (oldestQueuedAtMs === 0 || queuedAtMs < oldestQueuedAtMs)) {
          oldestQueuedAtMs = queuedAtMs;
        }
      }
    }
    this.recordTransportStatus({
      priorityQueueDepth: high + normal + low,
      highPriorityQueueDepth: high,
      normalPriorityQueueDepth: normal,
      lowPriorityQueueDepth: low,
      queuedBytes,
      oldestQueuedAgeMs: oldestQueuedAtMs > 0 ? Math.max(0, Date.now() - oldestQueuedAtMs) : 0,
    });
  }

  incomingFrameReservedBytes(excludeTransferId = '') {
    let total = 0;
    for (const [transferId, entry] of this.incomingFrames.entries()) {
      if (excludeTransferId && transferId === excludeTransferId) continue;
      total += Math.max(0, Number(entry?.totalBytes || 0));
    }
    return total;
  }

  incomingFrameBufferedBytes() {
    let total = 0;
    for (const entry of this.incomingFrames.values()) {
      total += Math.max(0, Number(entry?.bufferedBytes || 0));
    }
    return total;
  }

  cleanupCompletedFrameAcks() {
    const now = Date.now();
    for (const [transferId, completed] of [...this.completedFrameAcks.entries()]) {
      if (completed.expiresAt <= now || this.completedFrameAcks.size > 512) {
        this.completedFrameAcks.delete(transferId);
      }
    }
  }

  // M3: drop incoming transfers that have made no progress within the stall
  // window, freeing their buffered chunks (up to MAX_TRANSFER_BYTES each).
  // Runs opportunistically from the same frame-activity path as the completed-
  // ack cleanup, so a stalled transfer is reclaimed while other traffic flows
  // rather than lingering until the peer is finally dropped. An actively-
  // progressing transfer resets `lastProgressAt` and is never discarded.
  sweepStalledIncomingTransfers(now = Date.now(), timeoutMs = STALLED_INCOMING_TRANSFER_TIMEOUT_MS) {
    if (this.incomingFrames.size === 0) return 0;
    let swept = 0;
    for (const [transferId, entry] of [...this.incomingFrames.entries()]) {
      const lastProgressAt = Number(entry.lastProgressAt ?? entry.createdAt ?? now);
      if (now - lastProgressAt < timeoutMs) continue;
      this.incomingFrames.delete(transferId);
      swept += 1;
      this.events.emit('error', {
        code: 'ctox_webrtc_incoming_transfer_stalled',
        peerId: entry.peerId,
        transferId,
        receivedFrames: entry.received?.size || 0,
        totalFrames: entry.totalFrames,
        ageMs: now - lastProgressAt,
      });
    }
    if (swept > 0) {
      this.recordTransportStatus({
        incomingTransfers: this.incomingFrames.size,
        incomingFrameBufferedBytes: this.incomingFrameBufferedBytes(),
        incomingFrameReservedBytes: this.incomingFrameReservedBytes(),
      });
    }
    return swept;
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
    const retryable = RETRYABLE_SIGNALING_REJECTION_CODES.has(code)
      ? true
      : TERMINAL_SIGNALING_REJECTION_CODES.has(code)
        ? false
        : false;
    return {
      name: 'CtoxSignalingControlPlaneError',
      type: payload.type,
      scope: payload.scope,
      code,
      phase: 'signaling-control-plane',
      severity: 'error',
      retryable,
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
    selectedLocalCandidateType: '',
    selectedRemoteCandidateType: '',
    selectedCandidateProtocol: '',
    lastSignalAtMs: 0,
  };
}

async function updateSelectedCandidatePair(connection) {
  const report = await connection?.peer?.getStats?.();
  if (!report) return;
  const values = [];
  report.forEach?.((value) => values.push(value));
  let pair = null;
  const transport = values.find((entry) => entry?.type === 'transport' && entry.selectedCandidatePairId);
  if (transport) pair = values.find((entry) => entry.id === transport.selectedCandidatePairId) || null;
  if (!pair) {
    pair = values.find((entry) => (
      entry?.type === 'candidate-pair'
      && entry.state === 'succeeded'
      && (entry.nominated || entry.selected)
    )) || null;
  }
  if (!pair) return;
  const local = values.find((entry) => entry.id === pair.localCandidateId) || null;
  const remote = values.find((entry) => entry.id === pair.remoteCandidateId) || null;
  connection.signalStats.selectedLocalCandidateType = local?.candidateType || '';
  connection.signalStats.selectedRemoteCandidateType = remote?.candidateType || '';
  connection.signalStats.selectedCandidateProtocol = local?.protocol || remote?.protocol || '';
}

function turnCredentialExpiryMs(iceServers = []) {
  const expiries = [];
  for (const server of Array.isArray(iceServers) ? iceServers : []) {
    const urls = Array.isArray(server?.urls) ? server.urls : [server?.urls];
    if (!urls.some((url) => /^turns?:/i.test(String(url || '')))) continue;
    const expirySeconds = Number.parseInt(String(server?.username || '').split(':')[0], 10);
    if (Number.isFinite(expirySeconds) && expirySeconds > 0) expiries.push(expirySeconds * 1000);
  }
  return expiries.length ? Math.min(...expiries) : 0;
}

function signalingIceServers(value) {
  if (!Array.isArray(value) || value.length < 1 || value.length > 16) return [];
  const servers = [];
  for (const entry of value) {
    if (!entry || typeof entry !== 'object') return [];
    const rawUrls = typeof entry.urls === 'string'
      ? [entry.urls]
      : Array.isArray(entry.urls)
        ? entry.urls
        : [];
    const urls = rawUrls
      .filter((url) => typeof url === 'string')
      .map((url) => url.trim())
      .filter((url) => /^(?:stun|turn|turns):[^\s]{1,2048}$/i.test(url));
    if (!urls.length || urls.length !== rawUrls.length) return [];
    const server = { urls: typeof entry.urls === 'string' ? urls[0] : urls };
    if (typeof entry.username === 'string' && entry.username.length <= 1024) {
      server.username = entry.username;
    }
    if (typeof entry.credential === 'string' && entry.credential.length <= 4096) {
      server.credential = entry.credential;
    }
    servers.push(server);
  }
  return servers;
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

function boundedLocalSignalingPeerId(value) {
  if (typeof value !== 'string') return '';
  const normalized = value.trim();
  if (!normalized || normalized.length > MAX_LOCAL_SIGNALING_PEER_ID_LENGTH) return '';
  return normalized;
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
// scope. Not part of the public CTOX Sync Engine API.
export const webrtcNativeTestInternals = Object.freeze({
  splitFrameChunks,
  jsonEscapedCharLen,
  encodedSize,
  utf8ByteLength,
  recordReceivedFrame,
  classifySendPriority,
  MAX_SERIALIZED_FRAME_BYTES,
  MAX_INCOMING_FRAME_TRANSFERS,
  MAX_INCOMING_FRAME_BUFFERED_BYTES,
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
  // M3: a genuinely-new frame is forward progress — reset the stall clock so a
  // resumed/slow transfer that keeps advancing is never aged out.
  if (!hadFrame) entry.lastProgressAt = Date.now();
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
    queuedBytes: 0,
    scheduleCursor: 0,
  };
}

function nextQueuedSend(queue) {
  for (let offset = 0; offset < FAIR_SEND_SCHEDULE.length; offset += 1) {
    const priority = FAIR_SEND_SCHEDULE[queue.scheduleCursor % FAIR_SEND_SCHEDULE.length];
    queue.scheduleCursor = (queue.scheduleCursor + 1) % FAIR_SEND_SCHEDULE.length;
    if (queue[priority].length) {
      const item = queue[priority].shift();
      queue.queuedBytes = Math.max(0, queue.queuedBytes - Number(item?.byteLength || 0));
      return item;
    }
  }
  return null;
}

function nextHighPriorityInlineSend(queue) {
  if (!queue?.high?.length) return null;
  const index = queue.high.findIndex((item) => item?.inline);
  if (index < 0) return null;
  const item = queue.high.splice(index, 1)[0] || null;
  if (item) queue.queuedBytes = Math.max(0, queue.queuedBytes - Number(item.byteLength || 0));
  return item;
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
    'ctox.browser.live.v1',
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
