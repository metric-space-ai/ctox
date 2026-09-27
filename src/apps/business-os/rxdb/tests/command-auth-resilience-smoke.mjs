// REGRESSION: capability acquisition must distinguish transient control-plane
// outages from terminal authorization rejection while command submission stays
// fail-closed. Transient acquisition gets one awaited refresh retry; terminal
// rejection keeps the negative cache and never reaches local insertion.

import assert from 'node:assert/strict';
import {
  createCommandBus,
  getBusinessOsCapabilityToken,
  resetBusinessOsCapabilityTokenCacheForTests,
} from '../../shared/command-bus.js';

function clearInjectedCapabilitySessions() {
  delete globalThis.CTOX_BUSINESS_OS_SESSION;
  delete globalThis.ctoxBusinessOsSession;
  delete globalThis.ctoxBusinessOsLaunch;
  delete globalThis.CTOX_DESKTOP_SESSION;
  delete globalThis.ctoxDesktop;
}

function mockDb() {
  const documents = new Map();
  const collection = {
    documents,
    async insert(document) {
      documents.set(document.id, { ...document });
    },
    findOne(id) {
      return {
        async exec() {
          const document = documents.get(id);
          return document ? { toJSON: () => ({ ...document }) } : null;
        },
      };
    },
  };
  return {
    documents,
    db: {
      raw: {
        business_commands: collection,
        ctox_queue_tasks: collection,
      },
    },
  };
}

function capabilityResponse(token) {
  return {
    ok: true,
    status: 200,
    async json() {
      return {
        capability_token: token,
        expires_at_ms: Date.now() + 60 * 60 * 1000,
      };
    },
  };
}

function terminalResponse(status = 403) {
  return {
    ok: false,
    status,
    async json() { return {}; },
  };
}

clearInjectedCapabilitySessions();
resetBusinessOsCapabilityTokenCacheForTests();
const originalFetch = globalThis.fetch;
const nativeSetTimeout = globalThis.setTimeout;

try {
  // A grant change can precede any reconnect/handshake. The mutation boundary
  // must obtain current authority even when the earlier token has not expired.
  for (const revoked of [false, true]) {
    resetBusinessOsCapabilityTokenCacheForTests();
    let epochChanged = false;
    let calls = 0;
    globalThis.fetch = async () => {
      calls++;
      return epochChanged
        ? revoked ? terminalResponse(403) : capabilityResponse('current-mutation-authority')
        : capabilityResponse('cached-before-grant-change');
    };
    const { db, documents } = mockDb();
    const sync = { async startCollection() { epochChanged = true; return null; } };
    const submission = createCommandBus({ db, sync }).submit({
      id: 'cmd-grant-change-without-reconnect', command_type: 'business_os.test',
      sync_queue_tasks: false,
    });
    if (revoked) {
      await assert.rejects(submission, (error) => error.code === 'auth_required' && !error.transient);
      assert.equal(documents.size, 0, 'revoked authority never reaches the local insert');
    } else {
      await submission;
      assert.equal(documents.get('cmd-grant-change-without-reconnect').client_context.capability_token,
        'current-mutation-authority', 'preinsert must not reuse the prior epoch cache');
    }
    assert.equal(calls, 2, 'initial authority plus one mutation-boundary renewal');
  }

  // 1. A real timeout on the first POST is retried once inside the same submit.
  {
    resetBusinessOsCapabilityTokenCacheForTests();
    const { db, documents } = mockDb();
    let calls = 0;
    globalThis.fetch = (_url, options = {}) => {
      calls += 1;
      if (calls === 1) {
        return new Promise((_, reject) => {
          options.signal?.addEventListener('abort', () => reject(new Error('capability POST aborted')), { once: true });
        });
      }
      return Promise.resolve(capabilityResponse('capability-after-timeout'));
    };
    globalThis.setTimeout = (callback, delay, ...args) => (
      nativeSetTimeout(callback, delay === 120_000 ? 20 : delay, ...args)
    );

    const bus = createCommandBus({ db });
    const receipt = await bus.submit({
      id: 'cmd-auth-timeout-retry',
      command_type: 'business_os.test',
    });

    globalThis.setTimeout = nativeSetTimeout;
    assert.equal(calls, 3, 'one initial timeout retry plus mutation-boundary renewal');
    assert.equal(receipt.ok, true);
    assert.equal(documents.get(receipt.command_id)?.client_context?.capability_token, 'capability-after-timeout');
  }

  // 2. Two transient failures exhaust one submit, but its short anti-storm cache
  // must not impose the terminal 10-second negative-cache window on the next.
  {
    resetBusinessOsCapabilityTokenCacheForTests();
    const { db, documents } = mockDb();
    let calls = 0;
    globalThis.fetch = async () => {
      calls += 1;
      if (calls <= 2) throw new TypeError('temporary network outage');
      return capabilityResponse('capability-after-outage');
    };

    const bus = createCommandBus({ db });
    await assert.rejects(
      bus.submit({ id: 'cmd-auth-transient-exhausted', command_type: 'business_os.test' }),
      (error) => error?.code === 'auth_required'
        && error?.transient === true
        && error?.retryable === true,
    );
    assert.equal(documents.size, 0, 'fail-closed rejects before local insertion');

    const startedAt = Date.now();
    const receipt = await bus.submit({
      id: 'cmd-auth-immediate-follow-up',
      command_type: 'business_os.test',
    });
    const elapsedMs = Date.now() - startedAt;

    assert.equal(receipt.ok, true, 'the immediately following submit can refresh and succeed');
    assert.equal(calls, 4, 'the follow-up acquires and renews authority instead of retaining the negative cache');
    assert.ok(elapsedMs < 2_000, `transient cache recovery is short (observed ${elapsedMs}ms)`);
  }

  // 3. A 403 is terminal: no in-submit retry, and the next submit is rejected
  // from the existing negative cache with transient:false.
  {
    resetBusinessOsCapabilityTokenCacheForTests();
    const { db, documents } = mockDb();
    let calls = 0;
    globalThis.fetch = async () => {
      calls += 1;
      return terminalResponse(403);
    };

    const bus = createCommandBus({ db });
    for (const id of ['cmd-auth-terminal-first', 'cmd-auth-terminal-cached']) {
      await assert.rejects(
        bus.submit({ id, command_type: 'business_os.test' }),
        (error) => error?.code === 'auth_required'
          && error?.transient === false
          && error?.retryable === true,
      );
    }
    assert.equal(calls, 1, 'terminal rejection retains the negative cache');
    assert.equal(documents.size, 0, 'terminal rejection remains fail-closed');
  }

  // Reconfiguration can revoke a token before its wall-clock expiry. Only a
  // handshake and mutation boundary request refresh; permission reads retain the cache.
  {
    resetBusinessOsCapabilityTokenCacheForTests();
    let calls = 0;
    let releaseRefresh;
    globalThis.fetch = async () => {
      calls += 1;
      if (calls === 1) return capabilityResponse('epoch-before-install');
      if (calls === 2) await new Promise((resolve) => { releaseRefresh = resolve; });
      return capabilityResponse('epoch-after-install');
    };
    assert.equal(await getBusinessOsCapabilityToken(), 'epoch-before-install');
    assert.equal(await getBusinessOsCapabilityToken(), 'epoch-before-install');
    assert.equal(calls, 1, 'local permission reads reuse the positive cache');
    const handshake = getBusinessOsCapabilityToken({ refresh: true });
    const concurrentHandshake = getBusinessOsCapabilityToken({ refresh: true });
    const concurrentRead = getBusinessOsCapabilityToken();
    assert.equal(calls, 2, 'handshakes coalesce one actual refresh');
    releaseRefresh();
    assert.deepEqual(await Promise.all([handshake, concurrentHandshake, concurrentRead]),
      ['epoch-after-install', 'epoch-after-install', 'epoch-after-install']);
    const { db, documents } = mockDb();
    await createCommandBus({ db }).submit({
      id: 'cmd-auth-after-reconfiguration', command_type: 'business_os.test',
    });
    assert.equal(documents.get('cmd-auth-after-reconfiguration').client_context.capability_token,
      'epoch-after-install', 'subsequent commands use the renewed capability');
    assert.equal(calls, 3, 'the later mutation independently renews authority');
  }

  // A refresh is not permission to retain a rejected old token or bypass the
  // negative cache. No local command may be inserted with that old authority.
  {
    resetBusinessOsCapabilityTokenCacheForTests();
    let calls = 0;
    globalThis.fetch = async () => ++calls === 1
      ? capabilityResponse('revoked-token') : terminalResponse(403);
    await getBusinessOsCapabilityToken();
    assert.equal(await getBusinessOsCapabilityToken({ refresh: true }), null);
    assert.equal(await getBusinessOsCapabilityToken({ refresh: true }), null);
    const { db, documents } = mockDb();
    await assert.rejects(createCommandBus({ db }).submit({
      id: 'cmd-auth-revoked', command_type: 'business_os.test',
    }), (error) => error.code === 'auth_required' && error.transient === false);
    assert.equal(calls, 2, 'forced refresh cannot bypass terminal rejection caching');
    assert.equal(documents.size, 0);
  }

  // A command can begin acquiring authority before its bridge reconnects.
  // Never insert the pre-handshake token after readiness renewed or rejected it.
  for (const rejected of [false, true]) {
    resetBusinessOsCapabilityTokenCacheForTests();
    let calls = 0;
    globalThis.fetch = async () => ++calls === 1
      ? capabilityResponse('before-bridge-reconnect')
      : rejected ? terminalResponse(403) : capabilityResponse('after-bridge-reconnect');
    const { db, documents } = mockDb();
    const sync = {
      async startCollection(name) {
        assert.equal(name, 'business_commands');
        await getBusinessOsCapabilityToken({ refresh: true });
        return null;
      },
    };
    const submission = createCommandBus({ db, sync }).submit({
      id: 'cmd-auth-reconnect-before-insert',
      command_type: 'business_os.test', sync_queue_tasks: false,
    });
    if (rejected) {
      await assert.rejects(submission, (error) =>
        error.code === 'auth_required' && error.transient === false);
      assert.equal(documents.size, 0, 'rejected reconnect never inserts stale authority');
    } else {
      await submission;
      assert.equal(documents.get('cmd-auth-reconnect-before-insert').client_context.capability_token,
        'after-bridge-reconnect', 'insert binds authority after bridge readiness');
    }
    assert.equal(calls, rejected ? 2 : 3, 'mutation renews positive authority but respects terminal negative cache');
  }

  // A host-provided device identity must not silently become the HTTP session
  // identity on reconnect. The host remains responsible for renewing it.
  {
    resetBusinessOsCapabilityTokenCacheForTests();
    globalThis.CTOX_DESKTOP_SESSION = {
      capability_token: 'paired-device-token',
      capability_expires_at_ms: Date.now() + 3_600_000,
    };
    globalThis.fetch = async () => { throw new Error('must not replace device identity'); };
    assert.equal(await getBusinessOsCapabilityToken(), 'paired-device-token');
    globalThis.CTOX_DESKTOP_SESSION.capability_token = 'renewed-paired-device-token';
    assert.equal(await getBusinessOsCapabilityToken({ refresh: true }), 'renewed-paired-device-token');
    clearInjectedCapabilitySessions();
  }
} finally {
  globalThis.fetch = originalFetch;
  globalThis.setTimeout = nativeSetTimeout;
  clearInjectedCapabilitySessions();
  resetBusinessOsCapabilityTokenCacheForTests();
}

console.log('ctox-rxdb command auth resilience smoke OK');
process.exit(0);
