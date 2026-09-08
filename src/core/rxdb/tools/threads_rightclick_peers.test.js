'use strict';
const { test, mock } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const os = require('os');
const path = require('path');
const vm = require('vm');
const { runThreadsRightClickPeers } = require('./threads_rightclick_peers.js');

// Driver-contract tests only: actual shell/WebRTC behavior is tested by the
// full native smoke mode. These guard credential routing and teardown.
function driver({ wrongSecondActor = false, failReview = false, deliveredToken = null } = {}) {
  const contexts = [];
  const chromium = {
    async launchPersistentContext(profile) {
      const index = contexts.length;
      const actor = index === 0 ? 'threads-requester' : 'threads-reviewer';
      const role = index === 0 ? 'user' : 'admin';
      const state = {
        session: { authenticated: true, user: { id: wrongSecondActor && index ? 'local-dev' : actor, role } },
        db: { raw: { business_commands: {} } }, sync: {},
      };
      const page = {
        listeners: {}, bindings: {}, calls: [],
        on(name, callback) { this.listeners[name] = callback; },
        async goto() {
          if (deliveredToken) this.listeners.response({
            status: () => 200,
            url: () => 'http://127.0.0.1:8879/api/business-os/auth/capability',
            json: async () => ({ capability_token: deliveredToken }),
          });
        },
        async screenshot() {},
        async waitForFunction(predicate, args) {
          const ready = vm.runInNewContext('(' + predicate.toString() + ')(args)', {
            args, CTOX_BUSINESS_OS_APP: state, CTOX_BUSINESS_OS_STATUS: {},
          });
          if (!ready) throw new Error('authenticated actor did not match');
        },
        async evaluate(fn, args) {
          this.calls.push(fn.name);
          if (fn.name === 'runRequesterInBrowser') {
            return this.bindings.__ctoxReviewThreadsApproval({ reviewerId: 'threads-reviewer' });
          }
          if (fn.name === 'runReviewerInBrowser') {
            if (failReview) throw new Error('native review failed');
            assert.equal(args.reviewerId, actor);
            return { approvalDecision: 'approved' };
          }
          return { actorId: state.session.user.id, role, instanceId: 'same-instance' };
        },
        async exposeFunction(name, callback) { this.bindings[name] = callback; },
        locator() { return { innerText: async () => 'synthetic failure evidence' }; },
        isClosed() { return false; },
      };
      const context = {
        profile, page, closed: false,
        async route(matcher, handler) { this.matcher = matcher; this.handler = handler; },
        async newPage() { return page; },
        pages() { return [page]; },
        async close() { this.closed = true; },
      };
      contexts.push(context);
      return context;
    },
  };
  return { chromium, contexts };
}

async function run(options, assertions) {
  // Mock-driver output must never look like real native/browser CI evidence.
  const log = mock.method(console, 'log', () => {});
  const error = mock.method(console, 'error', () => {});
  const runtimeRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'ctox-peer-driver-'));
  const fake = driver(options);
  const promise = runThreadsRightClickPeers({
    chromium: fake.chromium, launchOptions: {}, runtimeRoot,
    smokeUrl: 'http://127.0.0.1:8879/index.html?rxdbSmoke=1',
    capabilities: { requester: { token: 'requester-test-token' }, reviewer: { token: 'reviewer-test-token' } },
    smokeMode: 'business-os-threads-rightclick-ui', threadsScaleSeed: null,
    browserDiagnostics: { warnings: 0, errors: 0, requestFailures: 0, assetResponseErrors: 0 },
    evidenceDir: options.evidence ? path.join(runtimeRoot, 'evidence') : undefined,
    readNativeAuthorizationState: options.readNativeAuthorizationState,
  });
  try { await assertions(promise, fake.contexts, runtimeRoot); }
  finally {
    log.mock.restore();
    error.mock.restore();
    fs.rmSync(runtimeRoot, { recursive: true, force: true });
  }
}

test('distinct actor profiles route credentials only to their control origin and close', async () => {
  await run({}, async (result, contexts) => {
    const evidence = await result;
    assert.equal(evidence.approvalDecision, 'approved');
    assert.equal(evidence.isolatedBrowserProfiles, true);
    assert.notEqual(contexts[0].profile, contexts[1].profile);
    assert.deepEqual(evidence.authenticatedPeers.map((p) => p.actorId), ['threads-requester', 'threads-reviewer']);
    for (const [index, context] of contexts.entries()) {
      assert.ok(context.matcher(new URL('http://127.0.0.1:8879/api/business-os/auth/capability')));
      assert.equal(context.matcher(new URL('https://example.com/')), false);
      assert.equal(context.matcher(new URL('http://127.0.0.1:18878/')), false);
      let headers;
      await context.handler({
        request: () => ({ headers: () => ({ accept: 'application/json' }) }),
        continue: async (request) => { headers = request.headers; },
      });
      assert.equal(headers.authorization, 'Bearer ' + (index ? 'reviewer' : 'requester') + '-test-token');
      assert.equal(headers.accept, 'application/json');
      assert.equal(context.closed, true);
    }
    assert.ok(contexts[0].page.calls.includes('runRequesterInBrowser'));
    assert.ok(contexts[1].page.calls.includes('runReviewerInBrowser'));
  });
});

test('a server actor mismatch fails before review and closes both profiles', async () => {
  await run({ wrongSecondActor: true }, async (result, contexts) => {
    await assert.rejects(result, /authenticated actor did not match/);
    assert.ok(contexts.every((context) => context.closed));
    assert.equal(contexts[0].page.calls.includes('runRequesterInBrowser'), false);
  });
});

test('a reviewer failure propagates and closes both profiles', async () => {
  await run({ failReview: true }, async (result, contexts) => {
    await assert.rejects(result, /native review failed/);
    assert.ok(contexts.every((context) => context.closed));
  });
});

test('authorization failure evidence preserves epochs without bearer secrets', async () => {
  const payload = {
    uid: 'threads-requester', role: 'user', epoch: 7, iat: 10, exp: 9999999999999,
    email: 'must-not-persist@example.test', cnf: { jkt: 'secret-key-thumbprint' },
  };
  const token = Buffer.from(JSON.stringify(payload)).toString('base64url') + '.secret-signature';
  let epoch = 7;
  await run({
    evidence: true, failReview: true, deliveredToken: token,
    readNativeAuthorizationState: () => [{ userId: 'threads-requester', role: 'user', active: 1, epoch: epoch++ }],
  }, async (result, contexts, runtimeRoot) => {
    await assert.rejects(result, /native review failed/);
    const text = fs.readFileSync(path.join(runtimeRoot, 'evidence/threads-authorization.json'), 'utf8');
    const evidence = JSON.parse(text);
    assert.equal(evidence.snapshots[0].native[0].epoch, 7);
    assert(evidence.snapshots.some(snapshot => snapshot.phase === 'workflow-failed'));
    assert.equal(evidence.snapshots.at(-1).phase, 'profiles-closed');
    assert.equal(evidence.deliveredCapabilityCount, 2);
    assert.equal(evidence.deliveredCapabilities.length, 2);
    assert.equal(evidence.deliveredCapabilities[0].claims.epoch, 7);
    assert.equal(evidence.deliveredCapabilities[0].claims.unverifiedPayload, true);
    assert.equal(evidence.deliveredCapabilities[0].claims.deviceBound, true);
    for (const secret of [token, 'secret-signature', payload.email, payload.cnf.jkt]) {
      assert.equal(text.includes(secret), false);
    }
    assert(contexts.every(context => context.closed));
  });
});
