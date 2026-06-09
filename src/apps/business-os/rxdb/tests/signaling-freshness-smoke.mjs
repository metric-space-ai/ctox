// REGRESSION: signaling-connection freshness + identity rules.
//
// 1. Token re-stamp: token_iat/token_exp are re-derived on EVERY connect
//    attempt, preserving the TTL length. They used to be baked into the URL
//    once at page load — a tab older than the TTL (24h) then reconnect-looped
//    forever against "control plane token expired" rejections.
// 2. Identity: only `yourPeerId` may rename this client. Adopting
//    `message.peerId` from joined/presence frames corrupted senderPeerId on
//    all subsequent signals (it names the REMOTE peer).
// 3. Backoff: the reconnect backoff resets on the `joined` broadcast (proof
//    the server ACCEPTED the join), not on socket-open — an open-then-rejected
//    socket must keep backing off instead of hammering at 1s.

import { createCtoxWebRtcNativePeer } from '../src/webrtc-native.mjs';

const STALE_IAT = 1_000_000; // ancient: must be re-stamped on connect
const TTL_SECONDS = 24 * 60 * 60;

const peer = createCtoxWebRtcNativePeer({
  signalingUrl: `wss://signaling.invalid/?room=x&token=secret-token&token_iat=${STALE_IAT}&token_exp=${STALE_IAT + TTL_SECONDS}`,
  room: 'ctox-business-os:test:abcdef',
  clientId: 'browser-test-client',
  role: 'browser',
});

try {
  // --- 1. token re-stamp on connect ---------------------------------------
  peer.connect();
  const url = new URL(peer.socket.url);
  const iat = Number(url.searchParams.get('token_iat'));
  const exp = Number(url.searchParams.get('token_exp'));
  const now = Math.floor(Date.now() / 1000);
  assert(Math.abs(iat - now) <= 60, `token_iat re-stamped to now (got ${iat}, now ${now})`);
  assert(exp - iat === TTL_SECONDS, `TTL length preserved (got ${exp - iat})`);
  assert(url.searchParams.get('token') === 'secret-token', 'token value itself unchanged');

  // --- 2. only yourPeerId renames the client ------------------------------
  peer.handleSignalingMessage(JSON.stringify({ type: 'ctoxPresence', peerId: 'remote-native-peer' }));
  assert(peer.options.clientId === 'browser-test-client', 'message.peerId must NOT rename this client');
  peer.handleSignalingMessage(JSON.stringify({ type: 'init', yourPeerId: 'server-assigned-id' }));
  assert(peer.options.clientId === 'server-assigned-id', 'init.yourPeerId renames this client');

  // --- 3. backoff resets on joined, not on open ----------------------------
  peer.signalingReconnectDelayMs = 30_000; // pretend we backed off heavily
  peer.handleSignalingMessage(JSON.stringify({ type: 'joined', otherPeerIds: [] }));
  assert(
    peer.signalingReconnectDelayMs < 30_000,
    `joined broadcast resets the reconnect backoff (still ${peer.signalingReconnectDelayMs})`,
  );
} finally {
  peer.close();
}

console.log('ctox-rxdb signaling freshness smoke OK');
process.exit(0);

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
