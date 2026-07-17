// SYNC-12 REGRESSION: role-filter checkpoint skip.
//
// masterChangesSince applies a per-document read-authz filter AFTER the pull
// checkpoint is computed, and the checkpoint validity key carries NO role/token
// component. So a browser syncing with a restricted token advances its persisted
// pull checkpoint PAST documents it was not allowed to read; after an operator
// grants more access, the retained checkpoint (same storage generation + schema)
// is reused on reconnect and the now-permitted documents are NEVER delivered.
//
// The fix folds a non-secret digest of THIS browser's effective read-permission
// identity (uid + role + capability_epoch, hashed) into the checkpoint-reuse
// decision:
//   (a) same token  ⇒ same digest ⇒ retained checkpoints resume incrementally;
//   (b) a token refresh that preserves role+epoch (fresh iat/exp) ⇒ same digest;
//   (c) a role upgrade (bumped capability epoch / changed role) ⇒ digest change
//       ⇒ retained checkpoints dropped ⇒ ONE full re-pull delivers the docs.
//
// The digest is derived from the capability token payload the same way the
// native side reads it (base64url(payload).sig, payload = {uid,role,epoch,...};
// src/core/business_os/capability.rs), WITHOUT verifying the signature — this is
// a change-detector, not an authorization decision.

import {
  replicateWebRTC,
  replicationWebRtcTestInternals,
} from '../src/replication-webrtc.mjs';

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

const base64Url = (obj) => Buffer.from(JSON.stringify(obj), 'utf8')
  .toString('base64')
  .replace(/\+/g, '-')
  .replace(/\//g, '_')
  .replace(/=+$/, '');

// Mint a capability token shaped exactly like the native issuer. The signature
// segment is arbitrary here — the browser digest never verifies it.
function mintToken({ uid = 'alice', role = 'user', epoch = 0, iat = 1, exp = 9_999_999_999_999 } = {}) {
  return `${base64Url({ uid, role, epoch, iat, exp })}.sig-${Math.random().toString(36).slice(2)}`;
}

const {
  decodeCapabilityTokenClaims,
  readPermissionDigestFromCapabilityToken,
  readPermissionDigestMatches,
} = replicationWebRtcTestInternals;

// --- 1. digest is stable across same-identity refresh, changes on role/epoch --
{
  const baseline = await readPermissionDigestFromCapabilityToken(
    mintToken({ uid: 'alice', role: 'user', epoch: 3, iat: 100, exp: 200 }),
  );
  assert(baseline, 'a valid token must yield a non-empty digest');

  // Same uid/role/epoch, DIFFERENT iat/exp (ordinary token refresh) ⇒ same digest.
  const refreshed = await readPermissionDigestFromCapabilityToken(
    mintToken({ uid: 'alice', role: 'user', epoch: 3, iat: 5_000, exp: 6_000 }),
  );
  assert(refreshed === baseline, 'token refresh with same role+epoch must not change the digest');

  // Bumped capability epoch (grant change) ⇒ different digest.
  const bumpedEpoch = await readPermissionDigestFromCapabilityToken(
    mintToken({ uid: 'alice', role: 'user', epoch: 4, iat: 100, exp: 200 }),
  );
  assert(bumpedEpoch !== baseline, 'a bumped capability epoch must change the digest');

  // Changed role (role upgrade) ⇒ different digest.
  const upgradedRole = await readPermissionDigestFromCapabilityToken(
    mintToken({ uid: 'alice', role: 'manager', epoch: 3, iat: 100, exp: 200 }),
  );
  assert(upgradedRole !== baseline, 'a changed role must change the digest');

  // Different user on the same browser storage ⇒ different digest.
  const otherUser = await readPermissionDigestFromCapabilityToken(
    mintToken({ uid: 'bob', role: 'user', epoch: 3, iat: 100, exp: 200 }),
  );
  assert(otherUser !== baseline, 'a different uid must change the digest');

  // Absent / malformed token ⇒ empty digest (unknown identity).
  assert(await readPermissionDigestFromCapabilityToken(null) === '', 'absent token ⇒ empty digest');
  assert(await readPermissionDigestFromCapabilityToken('') === '', 'empty token ⇒ empty digest');
  assert(await readPermissionDigestFromCapabilityToken('not-a-token') === '', 'garbage token ⇒ empty digest');

  // Digest excludes the raw signature (only the payload claims matter).
  const claims = decodeCapabilityTokenClaims(mintToken({ uid: 'alice', role: 'user', epoch: 3 }));
  assert(claims && claims.uid === 'alice' && claims.role === 'user' && claims.epoch === 3, 'claims decode');
}

// --- 2. match helper: empty CURRENT digest never forces a re-pull -------------
{
  assert(readPermissionDigestMatches('abc', ''), 'unknown current identity must not invalidate (transient blip)');
  assert(readPermissionDigestMatches('', ''), 'both empty ⇒ match (no token anywhere)');
  assert(readPermissionDigestMatches('abc', 'abc'), 'identical digests match');
  assert(!readPermissionDigestMatches('abc', 'def'), 'different digests mismatch');
  assert(!readPermissionDigestMatches(undefined, 'def'), 'pre-SYNC-12 checkpoint (no digest) mismatches a known identity');
}

// --- 3. end-to-end: role upgrade drops retained checkpoints, full re-pull -----
function mockCollection(name) {
  return {
    name,
    schema: { version: 0, hash: async () => `hash-${name}` },
    observe() { return { unsubscribe() {} }; },
    storageCollection: {
      replicationCheckpointStatus: async () => ({ epoch: 'checkpoint-epoch-1', schemaHash: `hash-${name}`, state: 'ready' }),
      getChangedDocumentsSince: async () => ({ documents: [], checkpoint: null }),
      bulkWrite: async () => ({}),
    },
  };
}

async function makeState(name, capabilityTokenProvider) {
  const state = await replicateWebRTC({
    collection: mockCollection(name),
    topic: `room-${name}-777777`,
    connectionHandlerCreator: {
      kind: 'ctox-native-webrtc',
      signalingServerUrl: 'wss://signaling.invalid/?token=t&token_iat=1&token_exp=2',
      config: {},
    },
    pull: { batchSize: 5 },
    push: { batchSize: 5 },
    retryTime: 60,
    ctox: { capabilityTokenProvider },
  });
  state.initialReplication?.catch?.(() => {});
  return state;
}

{
  // The native storage generation and schema are unchanged across the whole
  // scenario (same daemon run) — the ONLY thing that changes is the browser's
  // own read-permission identity. Under v2 checkpoint generation the validity
  // key is storageGeneration|collection|schemaHash, so without SYNC-12 the
  // retained checkpoint would always be reused.
  const proto = {
    storageGeneration: 'gen-A',
    checkpoint: { epoch: 'checkpoint-epoch-1' },
    peerSession: { sessionId: 'rxdb-rs-run-A', role: 'ctox_instance' },
    collection: { name: 'sync12', schemaHash: 'hash-sync12' },
    capabilities: ['ctox-checkpoint-generation-v2'],
  };

  // Restricted token first (role=user, epoch=3).
  let token = mintToken({ uid: 'alice', role: 'user', epoch: 3 });
  const state = await makeState('sync12', async () => token);
  state.pullFromRemotePeers = async () => {};
  state.pushToRemotePeers = async () => {};
  state.remoteProtocolForPeer = () => proto;

  // Connect, accumulate a pull checkpoint (advanced PAST filtered docs), drop.
  await state.runPeerReady('peer-1', proto, false);
  state.pullCheckpointsByPeer.set('peer-1', { lwt: 111 });
  state.pushCheckpointsByPeer.set('peer-1', { lwt: 222 });
  state.peerStates$.next(new Map([['peer-1', { peerId: 'peer-1' }]]));
  state.removePeer('peer-1', 'test-drop');
  assert(state.retainedCheckpoints, 'checkpoints retained on drop');
  assert(state.retainedCheckpoints.permissionDigest, 'retained checkpoint carries a permission digest');

  // (a) Reconnect with the SAME token (a fresh iat/exp refresh) ⇒ resume.
  token = mintToken({ uid: 'alice', role: 'user', epoch: 3, iat: 42, exp: 4242 });
  await state.runPeerReady('peer-2', proto, false);
  assert(state.pullCheckpointsByPeer.get('peer-2')?.lwt === 111, 'same identity ⇒ pull checkpoint resumed');
  assert(state.pushCheckpointsByPeer.get('peer-2')?.lwt === 222, 'same identity ⇒ push checkpoint resumed');

  // Drop again with the resumed checkpoints retained.
  state.peerStates$.next(new Map([['peer-2', { peerId: 'peer-2' }]]));
  state.removePeer('peer-2', 'test-drop');
  assert(state.retainedCheckpoints, 'checkpoints retained after resume+drop');

  // (b) Operator grants more access ⇒ new token, bumped epoch. Reconnect must
  //     DROP the retained checkpoints and re-pull from scratch so the docs the
  //     old restricted filter excluded finally arrive.
  token = mintToken({ uid: 'alice', role: 'user', epoch: 4 });
  await state.runPeerReady('peer-3', proto, false);
  assert(
    !state.pullCheckpointsByPeer.has('peer-3'),
    'role/epoch upgrade ⇒ retained pull checkpoint dropped (full re-pull)',
  );
  assert(
    !state.pushCheckpointsByPeer.has('peer-3'),
    'role/epoch upgrade ⇒ retained push checkpoint dropped',
  );
  assert(state.retainedCheckpoints === null, 'role/epoch upgrade clears persistent checkpoints');

  await state.cancel();
}

console.log('ctox-rxdb checkpoint role-digest smoke OK');
process.exit(0);
