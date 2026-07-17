// REGRESSION (SYNC-30): TURN credential refresh before a (re)connect.
//
// The native peer mints ephemeral coturn TURN credentials with a ~1h TTL and
// advertises a control-plane `ice_servers_refresh_url`; the browser used to
// fetch ICE servers ONCE at bootstrap and reuse `options.iceServers` for every
// reconnect. A relay-dependent session that dropped after >1h reconnected with
// EXPIRED credentials and could only recover via a full page reload.
//
// Contract pinned here:
//   1. TURN expiry is parsed from the coturn `<expiry-epoch>:user` username, so
//      `turnCredentialsNearExpiry` fires within the skew of expiry.
//   2. `refreshIceServersIfExpiring` replaces `options.iceServers` with fresh
//      minted creds and advances the tracked expiry — no page reload.
//   3. A (re)connect with expiring creds DEFERS the new RTCPeerConnection until
//      the refresh completes and then builds it with the FRESH servers.
//   4. A FAILED refresh falls back to reconnecting with the existing servers
//      (sync degrades, never wedges); the min-interval guard prevents a defer
//      loop.

import { createCtoxWebRtcNativePeer } from '../src/webrtc-native.mjs';

globalThis.window ||= {};
globalThis.document ||= {};

const assert = (condition, message) => {
  if (!condition) throw new Error(message);
};
const flush = async () => {
  // Two macrotasks flush the refresh promise chain + the deferred re-drive.
  await new Promise((resolve) => setTimeout(resolve, 0));
  await new Promise((resolve) => setTimeout(resolve, 0));
};

const nowSec = Math.floor(Date.now() / 1000);
const expiredCreds = [{ urls: 'turn:turn.example:3478', username: `${nowSec - 60}:u`, credential: 'old' }];
const freshExpiry = nowSec + 3600;
const freshCreds = [{ urls: 'turn:turn.example:3478', username: `${freshExpiry}:u`, credential: 'new' }];

const basePeerOptions = {
  signalingUrl: 'wss://signaling.invalid/?token=t&token_iat=1&token_exp=2',
  room: 'ctox-business-os:test:room',
  role: 'browser',
};

// --- 1. expiry detection + refresh replaces the server list ------------------
{
  let refreshCalls = 0;
  const peer = createCtoxWebRtcNativePeer({
    ...basePeerOptions,
    clientId: 'browser-a',
    iceServers: expiredCreds,
    refreshIceServers: async () => { refreshCalls += 1; return freshCreds; },
  });
  assert(peer.transportStats.turnCredentialExpiresAtMs === (nowSec - 60) * 1000, 'TURN expiry parsed from username');
  assert(peer.turnCredentialsNearExpiry(120_000) === true, 'expired creds are within the refresh skew');

  const refreshed = await peer.refreshIceServersIfExpiring();
  assert(refreshed === true, 'refresh returns true when creds were replaced');
  assert(refreshCalls === 1, 'refresh callback invoked exactly once');
  assert(peer.options.iceServers === freshCreds, 'options.iceServers replaced with fresh minted servers');
  assert(peer.transportStats.turnCredentialExpiresAtMs === freshExpiry * 1000, 'tracked expiry advanced to fresh window');
  assert(peer.turnCredentialsNearExpiry(120_000) === false, 'fresh creds are no longer near expiry');
  peer.close();
}

// --- 2. reconnect defers, then builds the peer with the FRESH servers --------
{
  const nativeId = 'ctox-business-os-native-1';
  const captured = [];
  let refreshCalls = 0;
  const peer = createCtoxWebRtcNativePeer({
    ...basePeerOptions,
    clientId: 'browser-b',
    expectedNativePeerId: nativeId,
    iceServers: expiredCreds,
    refreshIceServers: async () => { refreshCalls += 1; return freshCreds; },
  });
  peer.rememberPeerMetadata(nativeId, { role: 'ctox_instance' });
  // Capture the iceServers in effect at RTCPeerConnection build time without a
  // real WebRTC stack.
  peer.createConnection = function stub(peerId) { captured.push(this.options.iceServers); return { remotePeerId: peerId }; };

  const immediate = peer.ensureConnection(nativeId);
  assert(immediate === undefined, 'ensureConnection defers the connect while creds refresh');
  assert(captured.length === 0, 'no RTCPeerConnection built before the refresh completes');
  await flush();
  assert(refreshCalls === 1, 'the deferred connect triggered a refresh');
  assert(captured.length === 1, 'the RTCPeerConnection is built after the refresh');
  assert(captured[0] === freshCreds, 'the new RTCPeerConnection uses the FRESH TURN servers');
  peer.close();
}

// --- 3. failed refresh falls back to existing servers (no wedge) -------------
{
  const nativeId = 'ctox-business-os-native-2';
  const captured = [];
  const errors = [];
  let refreshCalls = 0;
  const peer = createCtoxWebRtcNativePeer({
    ...basePeerOptions,
    clientId: 'browser-c',
    expectedNativePeerId: nativeId,
    iceServers: expiredCreds,
    refreshIceServers: async () => { refreshCalls += 1; throw new Error('control plane unreachable'); },
  });
  peer.rememberPeerMetadata(nativeId, { role: 'ctox_instance' });
  peer.on('error', (event) => errors.push(event.detail || event));
  peer.createConnection = function stub(peerId) { captured.push(this.options.iceServers); return { remotePeerId: peerId }; };

  const immediate = peer.ensureConnection(nativeId);
  assert(immediate === undefined, 'ensureConnection defers even when the refresh will fail');
  await flush();
  assert(refreshCalls === 1, 'a refresh was attempted');
  assert(captured.length === 1, 'the connection is still built after a failed refresh (no wedge)');
  assert(captured[0] === expiredCreds, 'falls back to the existing servers on refresh failure');
  assert(errors.some((error) => error?.code === 'ctox_ice_servers_refresh_failed'), 'the refresh failure is surfaced');
  assert(
    peer.lastIceServersRefreshAtMs > 0 && peer.shouldRefreshIceServersBeforeConnect() === false,
    'the min-interval guard prevents an immediate re-defer after a failed refresh',
  );
  peer.close();
}

// --- 4. no refresh callback => pre-SYNC-30 behavior (never defers) -----------
{
  const peer = createCtoxWebRtcNativePeer({
    ...basePeerOptions,
    clientId: 'browser-d',
    iceServers: expiredCreds,
  });
  assert(peer.turnCredentialsNearExpiry(120_000) === true, 'creds still expired');
  assert(peer.shouldRefreshIceServersBeforeConnect() === false, 'without a callback the peer never defers a connect');
  assert(await peer.refreshIceServersIfExpiring() === false, 'refresh is a no-op without a callback');
  peer.close();
}

console.log('ctox-rxdb TURN credential refresh smoke OK');
process.exit(0);
