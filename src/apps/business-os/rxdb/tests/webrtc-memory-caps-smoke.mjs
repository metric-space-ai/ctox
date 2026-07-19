// M2 + M3: bound two per-peer maps in the native WebRTC transport that
// previously only ever grew.
//
//   M2 peerMetadata — one ~200-500B entry per distinct peer session ever seen
//     (every browser reload/tab is a fresh random peer id). Pruned to the live
//     room whenever a peer-list broadcast is processed; live connections kept.
//   M3 incomingFrames — a stalled multi-frame transfer retained up to
//     MAX_TRANSFER_BYTES of chunk strings until the peer was finally dropped.
//     Age-swept by time-since-progress; a transfer that keeps advancing resets.

import { CtoxWebRtcNativePeer } from '../dist/ctox-rxdb-js.mjs';
import { webrtcNativeTestInternals } from '../src/webrtc-native.mjs';

const { recordReceivedFrame } = webrtcNativeTestInternals;

function newPeer() {
  // Constructor does NOT open a socket (connect() does), so this is inert.
  return new CtoxWebRtcNativePeer({ signalingUrl: 'ws://localhost:0/ignored', room: 'room-x' });
}

// ---- M2: peerMetadata pruning -------------------------------------------
{
  const peer = newPeer();
  peer.rememberPeerMetadata('peer-A', { role: 'ctox_instance' });
  peer.rememberPeerMetadata('peer-B', { role: 'browser' });
  peer.rememberPeerMetadata('peer-C', { role: 'browser' });
  assert(peer.peerMetadata.size === 3, `three sessions remembered (got ${peer.peerMetadata.size})`);

  // peer-C has a live connection — its metadata must be preserved even though
  // it is absent from the room descriptor set below.
  peer.connections.set('peer-C', { channel: { readyState: 'open' } });

  // A peer-list broadcast now names only peer-A. peer-B is a dead session and
  // must be pruned; peer-A stays (present); peer-C stays (live connection).
  const removed = peer.prunePeerMetadata([{ peerId: 'peer-A' }]);
  assert(removed === 1, `exactly one stale session pruned (got ${removed})`);
  assert(peer.peerMetadata.has('peer-A'), 'present peer kept');
  assert(!peer.peerMetadata.has('peer-B'), 'stale session dropped');
  assert(peer.peerMetadata.has('peer-C'), 'live-connection peer kept despite absence from room set');

  // Empty/edge broadcast (no peer list) must be a no-op, never a wipe.
  const removedEmpty = peer.prunePeerMetadata([]);
  assert(removedEmpty === 0, 'empty descriptor set prunes nothing');
  assert(peer.peerMetadata.size === 2, `map holds A + C after no-op prune (got ${peer.peerMetadata.size})`);

  // Growth-then-shrink: many dead sessions accumulate, one broadcast reclaims.
  for (let i = 0; i < 50; i += 1) peer.rememberPeerMetadata(`ghost-${i}`, { role: 'browser' });
  assert(peer.peerMetadata.size === 52, `52 entries before reclaim (got ${peer.peerMetadata.size})`);
  peer.prunePeerMetadata([{ peerId: 'peer-A' }]);
  assert(peer.peerMetadata.has('peer-A') && peer.peerMetadata.has('peer-C'), 'A + C survive reclaim');
  assert(peer.peerMetadata.size === 2, `map shrinks back to the live room (got ${peer.peerMetadata.size})`);
}

// ---- M3: incomingFrames stall sweep -------------------------------------
{
  const peer = newPeer();
  const errors = [];
  peer.on('error', (event) => errors.push(event.detail));

  const base = 1_000_000;
  // A stalled transfer: last progress long ago, holding a buffered chunk.
  peer.incomingFrames.set('stalled', {
    peerId: 'p1', totalFrames: 10, totalBytes: 4,
    received: new Map([[0, 'X'.repeat(1024)]]),
    createdAt: base, lastProgressAt: base, attempt: 0, contiguousSeq: 0, nextAckSeq: 4,
  });
  // An actively-progressing transfer: recent progress, must survive.
  peer.incomingFrames.set('active', {
    peerId: 'p2', totalFrames: 10, totalBytes: 4,
    received: new Map([[0, 'Y']]),
    createdAt: base, lastProgressAt: base + 100_000, attempt: 0, contiguousSeq: 0, nextAckSeq: 4,
  });
  assert(peer.incomingFrames.size === 2, 'two incoming transfers registered');

  // Default stall window is 90s (FRAME_ACK_TIMEOUT_MS * 3). At now = base + 95s
  // the stalled one ages out; the active one (progress at +100s) does not.
  const swept = peer.sweepStalledIncomingTransfers(base + 95_000);
  assert(swept === 1, `exactly one stalled transfer swept (got ${swept})`);
  assert(!peer.incomingFrames.has('stalled'), 'stalled transfer discarded, buffer freed');
  assert(peer.incomingFrames.has('active'), 'progressing transfer retained');
  assert(errors.some((e) => e.code === 'ctox_webrtc_incoming_transfer_stalled'),
    'stall discard is observable via an error event');

  // Idempotent: a second sweep at the same clock frees nothing more.
  assert(peer.sweepStalledIncomingTransfers(base + 95_000) === 0, 'second sweep is a no-op');

  // Progress resets the stall clock: advancing the active transfer keeps it
  // alive even well past the original window.
  const activeEntry = peer.incomingFrames.get('active');
  recordReceivedFrame(activeEntry, 1, 'Z');
  assert(activeEntry.lastProgressAt >= base + 100_000, 'recordReceivedFrame stamps forward progress');
  assert(peer.sweepStalledIncomingTransfers(activeEntry.lastProgressAt + 1) === 0,
    'a transfer that just made progress is never swept');
}

console.log('ctox-rxdb-js webrtc memory caps smoke OK');

function assert(c, m) { if (!c) throw new Error(m); }
