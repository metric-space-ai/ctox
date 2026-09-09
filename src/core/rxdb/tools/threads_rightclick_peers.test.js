'use strict';
const { test, mock } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const os = require('os');
const path = require('path');
const vm = require('vm');
const { runThreadsRightClickPeers, runRequesterInBrowser, openContextTargetInBrowser } = require('./threads_rightclick_peers.js');

test('direct denial polling retains admission evidence without serializing command credentials', async () => {
  // Execute the actual browser predicate, including its returned timeout state.
  const source = runRequesterInBrowser.toString();
  const start = source.indexOf('const deniedDirectCommand = await waitFor(');
  const end = source.indexOf(", 30000, 'threads right-click direct native denial');", start);
  assert.ok(start >= 0 && end > start);
  const predicate = source.slice(start + 'const deniedDirectCommand = await waitFor('.length, end);
  for (const status of ['pending_sync', 'failed']) {
    const record = {
      id: 'command-1', command_id: 'command-1', status,
      command_type: 'business_os.data.modify',
      client_context: { capability_token: 'fixture-bearer', actor: { password: 'fixture-password' } },
      payload: { prompt: 'private-prompt' },
      result: { decision: { reason_code: 'role_or_scope_denied', token: 'nested-secret' } },
    };
    const state = { db: { raw: { business_commands: { find(query) {
      assert.equal(query.selector.command_id, record.command_id);
      return { exec: async () => [record] };
    } } } } };
    const result = await vm.runInNewContext('(' + predicate + ')()', {
      state, docsToJson: docs => docs, deniedCommandId: record.command_id, deniedDispatchError: '',
    });
    assert.equal(result.ok, status === 'failed');
    assert.equal(result.command.status, status);
    assert.equal(result.command.command_id, record.command_id);
    assert.equal(result.command.result.decision.reason_code, 'role_or_scope_denied');
    assert.doesNotMatch(JSON.stringify(result), /fixture-bearer|fixture-password|private-prompt|nested-secret/);
    assert.ok(record.client_context.capability_token, 'diagnostics must not mutate the stored command');
  }
});


function targetWindowDriver({ focused = true, failed = false, minimized = false, missing = false, hidden = false } = {}) {
  let marker = null;
  let staleRemoved = false;
  const host = {
    querySelector: () => marker,
    prepend(node) { marker = node; },
  };
  const root = {
    dataset: { moduleReady: 'true', moduleLoadFailed: String(failed) },
    getBoundingClientRect: () => ({ width: hidden ? 0 : 500, height: 400 }),
    querySelector(selector) { assert.equal(selector, '[data-module-content]'); return host; },
  };
  const state = {
    activeModule: { id: 'desktop' },
    async openModule(id) { assert.equal(id, 'tickets'); },
    windowManager: {
      listWindows: () => missing ? [] : [{
        id: 'tickets-window', ownerId: 'desktop-app:tickets', isFocused: focused,
        state: minimized ? 'minimized' : 'normal',
      }],
    },
  };
  const promise = vm.runInNewContext('(' + openContextTargetInBrowser.toString() + ')(args)', {
    args: { moduleId: 'tickets', recordId: 'ticket-1', timeoutMs: 25 },
    CTOX_BUSINESS_OS_APP: state, CSS: { escape: value => value },
    Date, setTimeout, console: { log() {} },
    document: {
      querySelector(selector) {
        assert.equal(selector, '[data-shell-window="true"][data-owner-id="desktop-app:tickets"] [data-module-root="tickets"]');
        return missing ? null : root;
      },
      querySelectorAll: () => [{ remove() { staleRemoved = true; } }],
      createElement: () => ({ dataset: {}, style: {}, scrollIntoView() { this.scrolled = true; } }),
    },
  });
  return { promise, marker: () => marker, staleRemoved: () => staleRemoved };
}

test('context target uses the focused mounted app window while the shell remains desktop', async () => {
  const target = targetWindowDriver();
  const result = await target.promise;
  assert.equal(result.ok, true);
  assert.equal(result.activeModule, 'desktop');
  assert.equal(result.windowId, 'tickets-window');
  assert.equal(target.marker().dataset.contextRecordId, 'ticket-1');
  assert.equal(target.marker().dataset.moduleRoot, 'tickets');
  assert.equal(target.marker().scrolled, true);
  assert.equal(target.staleRemoved(), true);
});

test('context target rejects missing, background, failed, minimized or hidden app windows', { timeout: 2000 }, async () => {
  for (const options of [{ missing: true }, { focused: false }, { failed: true }, { minimized: true }, { hidden: true }]) {
    const target = targetWindowDriver(options);
    await assert.rejects(target.promise, /target window open timed out/);
    assert.equal(target.marker(), null);
  }
});

// Driver-contract tests only: actual shell/WebRTC behavior is tested by the
// full native smoke mode. These guard credential routing and teardown.
function driver({ wrongSecondActor = false, failReview = false, deliveredToken = null, hangRequester = false, hangClose = false,
  cachedSync = null, requesterStatus = null, reviewerStatus = null, hangDiagnostic = false } = {}) {
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
            await this.bindings.__ctoxReportThreadsPhase('open-threads-module');
            if (requesterStatus) await this.bindings.__ctoxRecordThreadsStatus(requesterStatus);
            if (hangRequester) return new Promise(() => {});
            return this.bindings.__ctoxReviewThreadsApproval({ reviewerId: 'threads-reviewer' });
          }
          if (fn.name === 'runReviewerInBrowser') {
            if (reviewerStatus) await this.bindings.__ctoxRecordThreadsStatus(reviewerStatus);
            if (failReview) throw new Error('native review failed');
            assert.equal(args.reviewerId, actor);
            return { approvalDecision: 'approved' };
          }
          if (fn.name === 'readCachedThreadFailureState') {
            if (hangDiagnostic) return new Promise(() => {});
            return vm.runInNewContext('(' + fn.toString() + ')()', {
              CTOX_BUSINESS_OS_APP: state, ctoxBusinessOsSyncDiagnostics: cachedSync, Date,
            });
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
        async close() {
          this.closeAttempted = true;
          if (hangClose) return new Promise(() => {});
          this.closed = true;
        },
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
    readNativeSyncState: options.readNativeSyncState,
    diagnosticTimeoutMs: options.diagnosticTimeoutMs,
    workflowTimeoutMs: options.workflowTimeoutMs,
    closeTimeoutMs: options.closeTimeoutMs,
  });
  try { await assertions(promise, fake.contexts, runtimeRoot); }
  finally {
    log.mock.restore();
    error.mock.restore();
    fs.rmSync(runtimeRoot, { recursive: true, force: true });
  }
}

test('complete status artifacts survive an error larger than the CI log line limit', async () => {
  const requesterStatus = { version: 'business-os-advanced-status-v1', ok: false, detail: 'x'.repeat(100000), tail: 'requester-end' };
  const reviewerStatus = { version: 'business-os-advanced-status-v1', ok: false, tail: 'reviewer-end' };
  const cachedSync = { phase: 'collection-sync', receivedFrames: 159, detail: 'y'.repeat(100000), tail: 'sync-end' };
  let nativeReads = 0;
  await run({
    evidence: true, failReview: true, requesterStatus, reviewerStatus, cachedSync,
    readNativeSyncState: () => { nativeReads++; return { available: true, fileMtimeMs: 123, status: { pendingAcks: 4 } }; },
  }, async (result, contexts, runtimeRoot) => {
    await assert.rejects(result, /native review failed/);
    const read = name => JSON.parse(fs.readFileSync(path.join(runtimeRoot, 'evidence', name + '.json'), 'utf8'));
    assert.deepEqual(read('threads-requester-command-status'), requesterStatus);
    assert.deepEqual(read('threads-reviewer-result-status'), reviewerStatus);
    assert.equal(nativeReads, 1);
    assert.deepEqual(read('threads-native-failure').evidence, { available: true, fileMtimeMs: 123, status: { pendingAcks: 4 } });
    for (const actor of ['threads-requester', 'threads-reviewer']) {
      const snapshot = read(actor + '-failure-state').evidence;
      assert.equal(snapshot.actorId, actor);
      assert.deepEqual(snapshot.sync, cachedSync);
    }
    assert(contexts.every(context => context.closed));
  });
});

test('stalled diagnostic reads preserve the original failure and bounded profile teardown', { timeout: 2000 }, async () => {
  await run({
    evidence: true, failReview: true, hangDiagnostic: true, diagnosticTimeoutMs: 15,
    readNativeSyncState: () => new Promise(() => {}),
  }, async (result, contexts, runtimeRoot) => {
    await assert.rejects(result, /native review failed/);
    for (const name of ['threads-native-failure', 'threads-requester-failure-state', 'threads-reviewer-failure-state']) {
      const data = JSON.parse(fs.readFileSync(path.join(runtimeRoot, 'evidence', name + '.json'), 'utf8'));
      assert.equal(data.evidence.available, false);
      assert.equal(data.evidence.reason, 'capture-failed');
      assert.match(data.evidence.error, /capture deadline exceeded/);
    }
    assert(contexts.every(context => context.closed));
  });
});

test('oversized diagnostics are explicit unavailable records, not truncated JSON snapshots', async () => {
  await run({
    evidence: true, failReview: true, requesterStatus: { detail: 'x'.repeat(4 * 1024 * 1024 + 1) },
  }, async (result, contexts, runtimeRoot) => {
    await assert.rejects(result, /native review failed/);
    const data = JSON.parse(fs.readFileSync(path.join(runtimeRoot, 'evidence/threads-requester-command-status.json'), 'utf8'));
    assert.equal(data.available, false);
    assert.equal(data.reason, 'diagnostic-size-limit');
    assert(data.bytes > 4 * 1024 * 1024);
    assert(contexts.every(context => context.closed));
  });
});

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

test('an unresolved browser evaluation fails with its phase and closes both profiles', { timeout: 3000 }, async () => {
  await run({ evidence: true, hangRequester: true, workflowTimeoutMs: 25 }, async (result, contexts, runtimeRoot) => {
    await assert.rejects(result, /threads workflow exceeded 25 ms in open-threads-module/);
    assert.equal(contexts.length, 2);
    assert.ok(contexts.every(context => context.closed));
    const evidence = JSON.parse(fs.readFileSync(path.join(runtimeRoot, 'evidence/threads-authorization.json'), 'utf8'));
    assert.ok(evidence.snapshots.some(snapshot => snapshot.phase === 'workflow:open-threads-module'));
    assert.ok(evidence.snapshots.some(snapshot => snapshot.phase === 'workflow-failed'));
    assert.equal(evidence.snapshots.at(-1).phase, 'profiles-closed');
  });
});

test('an unresolved profile close fails acceptance without claiming cleanup succeeded', { timeout: 3000 }, async () => {
  await run({ evidence: true, hangClose: true, closeTimeoutMs: 25 }, async (result, contexts, runtimeRoot) => {
    await assert.rejects(result, /threads browser profile cleanup failed/);
    assert.ok(contexts.every(context => context.closeAttempted && !context.closed));
    const evidence = JSON.parse(fs.readFileSync(path.join(runtimeRoot, 'evidence/threads-authorization.json'), 'utf8'));
    assert.equal(evidence.snapshots.at(-1).phase, 'profile-close-failed');
    assert.equal(evidence.snapshots.some(snapshot => snapshot.phase === 'profiles-closed'), false);
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
