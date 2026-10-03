// Reusing a valid command capability does not refresh an already-open native
// peer. Exercise the shared-room change detector and full reconnect boundary.
// The fake handshake is orchestration evidence only; native proof validation
// remains covered by its existing tests and the actual browser/native probe.
import assert from 'node:assert/strict';
import { replicationWebRtcTestInternals } from '../src/replication-webrtc.mjs';
const SharedRoomPeer = replicationWebRtcTestInternals.getSharedRoomPeerClass();
const token = (patch = {}) => Buffer.from(JSON.stringify({
  uid: 'same-user', role: 'admin', epoch: 1, iat: Date.now(),
  exp: Date.now() + 60_000, ...patch,
})).toString('base64url') + '.test-signature';
const connection = () => ({ channel: { readyState: 'open' }, peer: { connectionState: 'connected' } });
async function room(initial) {
  const shared = new SharedRoomPeer({ key: 'authority', room: 'authority-room',
    signalingUrl: 'wss://signaling.invalid', iceServers: [], expectedNativePeerId: 'native' });
  let supplied = initial;
  let reconnects = 0;
  let handshakes = 0;
  let releaseHandshake = null;
  let holdHandshake = false;
  shared.negotiated = { peerId: 'native' };
  shared.peer = {
    connections: new Map([['native', connection()]]),
    removeConnection(peerId, reason) {
      assert.equal(peerId, 'native');
      assert.equal(reason, 'capability-authority-changed');
      reconnects++;
      this.connections.delete(peerId);
      shared.negotiated = null;
    },
  };
  shared.buildProtocolPayloadUncached = async () => ({ peerSession: { capabilityToken: supplied } });
  await shared.buildProtocolPayload('business_commands', [], 'native');
  shared.ensureNegotiatedPeer = async () => {
    if (shared.negotiated) return shared.negotiated;
    handshakes++;
    if (holdHandshake) await new Promise(resolve => { releaseHandshake = resolve; });
    shared.peer.connections.set('native', connection());
    await shared.buildProtocolPayload('business_commands', [], 'native');
    shared.negotiated = { peerId: 'native' };
    return shared.negotiated;
  };
  return { shared, setToken(value) { supplied = value; },
    hold() { holdHandshake = true; }, release() { releaseHandshake?.(); },
    counts: () => ({ reconnects, handshakes }) };
}

{
  const first = token();
  const r = await room(first);
  const renewed = token({ iat: Date.now() + 5, exp: Date.now() + 120_000 });
  assert.equal(await r.shared.ensurePeerAuthority(renewed), false,
    'ordinary same-authority token renewal does not reconnect');
  assert.deepEqual(r.counts(), { reconnects: 0, handshakes: 0 });
}
for (const patch of [{ epoch: 2 }, { role: 'member' }]) {
  const r = await room(token());
  const current = token(patch);
  const oldConnection = r.shared.peer.connections.get('native');
  r.setToken(current);
  assert.equal(await r.shared.ensurePeerAuthority(current), true);
  assert.notEqual(r.shared.peer.connections.get('native'), oldConnection, 'requires a fresh connection');
  assert.equal(await r.shared.ensurePeerAuthority(current), false, 'new handshake converges');
  assert.deepEqual(r.counts(), { reconnects: 1, handshakes: 1 });
}
{
  const r = await room(token({ exp: Date.now() - 1 }));
  const current = token();
  r.setToken(current);
  assert.equal(await r.shared.ensurePeerAuthority(current), true, 'expired captured authority renews');
}
{
  const r = await room(token());
  await assert.rejects(r.shared.ensurePeerAuthority(token({ uid: 'other-user' })),
    error => error.code === 'auth_required', 'a different account must reopen its own DB scope');
  assert.equal(r.counts().reconnects, 0, 'never silently rebind an existing room to another account');
}
{
  const binding = { device_pairing_id: 'pair', device_id: 'device', cnf: { jkt: 'key' } };
  const r = await room(token(binding));
  const current = token({ ...binding, epoch: 2 });
  r.setToken(current);
  assert.equal(await r.shared.ensurePeerAuthority(current), true, 'bound authority uses full reconnect');
  await assert.rejects(r.shared.ensurePeerAuthority(token({ ...binding, cnf: { jkt: 'other-key' } })),
    error => error.code === 'auth_required', 'new device identity needs its own scope');
}
{
  const r = await room(token());
  const current = token({ epoch: 2 });
  r.setToken(current); r.hold();
  const first = r.shared.ensurePeerAuthority(current);
  const second = r.shared.ensurePeerAuthority(current);
  await new Promise(resolve => setTimeout(resolve, 0));
  r.release();
  await Promise.all([first, second]);
  assert.deepEqual(r.counts(), { reconnects: 1, handshakes: 1 }, 'room-wide renewal is coalesced');
}
{
  const r = await room(token());
  const current = token({ epoch: 2 });
  r.setToken(current);
  r.shared.ensureNegotiatedPeer = async () => { throw new Error('native rejected fresh proof'); };
  await assert.rejects(r.shared.ensurePeerAuthority(current), /native rejected fresh proof/);
  assert.equal(r.shared.negotiated, null, 'failed handshake does not revive old authority');
}
{
  const r = await room(token());
  const oldConnection = r.shared.peer.connections.get('native');
  let release;
  r.shared.buildProtocolPayloadUncached = () => new Promise(resolve => { release = resolve; });
  const stale = r.shared.buildProtocolPayload('business_commands', [], 'native');
  const replacement = connection();
  r.shared.peer.connections.set('native', replacement);
  release({ peerSession: { capabilityToken: token({ epoch: 99 }) } });
  await stale;
  assert.equal(r.shared.peerAuthorities.has(replacement), false, 'late old payload cannot bind replacement');
  assert.notEqual(oldConnection, replacement);
}
{
  const r = await room(token());
  const current = token({ epoch: 2 });
  r.setToken(current);
  await r.shared.buildProtocolPayload('business_commands', [], 'native');
  assert.equal(await r.shared.ensurePeerAuthority(current), true,
    'a payload on the old connection does not prove renewed native authority');
}
{
  const r = await room(token());
  const current = token({ epoch: 2 });
  r.setToken(current); r.hold();
  const pending = r.shared.ensurePeerAuthority(current);
  await new Promise(resolve => setTimeout(resolve, 0));
  await assert.rejects(r.shared.ensurePeerAuthority(token({ uid: 'another-account' })),
    error => error.code === 'auth_required', 'coalescing cannot adopt another account');
  r.release();
  await pending;
}
{
  const r = await room(token());
  r.setToken(token({ uid: 'another-account' }));
  await assert.rejects(r.shared.ensurePeerAuthority(token({ epoch: 2 })),
    error => error.code === 'auth_required', 'identity change during handshake is rejected');
  assert.equal(r.shared.negotiated, null);
}
console.log('ctox-rxdb peer authority renewal smoke OK');
