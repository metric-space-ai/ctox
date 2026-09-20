import test from 'node:test';
import assert from 'node:assert/strict';
import { createSyncRuntime } from '../../shared/sync.js';
import { CREDENTIAL_REVEAL_METHOD } from '../../shared/native-request-privacy.mjs';

test('real sync private API never enters cross-tab relay, including leadership loss', async () => {
  let leader = false, relayHandler, proxyCalls = 0, directCalls = 0, starts = 0;
  const coordinator = {
    isLeader: () => leader,
    snapshot: () => ({ isLeader: leader }),
    start: async () => ({ isLeader: leader }), close: async () => {},
    onNativeRequest: callback => { relayHandler = callback; return () => {}; },
    requestNativeViaLeader: async () => { proxyCalls += 1; throw Error('must never proxy private request'); },
  };
  const runtime = createSyncRuntime({
    db: { mode: 'rxdb', name: 'private-fixture', rxdb: { getMultiTabSyncCoordinator: () => coordinator } },
    config: { transport: 'webrtc', sync_room: 'private-fixture', signaling_urls: ['wss://fixture.invalid'] },
  });
  try {
    // The actual follower start registers the relay callback, but creates no
    // network peer. Only the subsequent direct bridge is a controlled double.
    await runtime.startCollection('business_commands');
    runtime.startCollection = async () => {
      starts += 1;
      leader = false; // Simulate a role change while direct acquisition awaits.
      return { state: { requestNative: async () => { directCalls += 1; return { ok: true }; } } };
    };
    await assert.rejects(runtime.requestPrivateNative(CREDENTIAL_REVEAL_METHOD, {}), { code: 'credential_reveal_direct_tab_required' });
    assert.equal(starts, 0);
    assert.throws(() => relayHandler(CREDENTIAL_REVEAL_METHOD, {}, {}), { code: 'credential_reveal_direct_tab_required' });
    leader = true;
    assert.throws(() => relayHandler(CREDENTIAL_REVEAL_METHOD, {}, {}), { code: 'credential_reveal_direct_tab_required' });
    await assert.rejects(runtime.requestPrivateNative('unlisted-method', {}));
    assert.deepEqual(await runtime.requestPrivateNative(CREDENTIAL_REVEAL_METHOD, {}), { ok: true });
    assert.equal(directCalls, 1);
    assert.equal(starts, 1);
    assert.equal(proxyCalls, 0);
  } finally { await runtime.stop(); }
});
