import assert from 'node:assert/strict';
import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { chromium } from 'playwright';

const app = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const tests = readFileSync(new URL('./workjet-project-control.test.mjs', import.meta.url), 'utf8');
const executionSource = readFileSync(new URL('./workjet-supervisor-execution-contract.generated.mjs', import.meta.url), 'utf8').replace(/^export /gm, '');
const start = app.indexOf('const WORKJET_PROJECT_CONTROL_MAX_RESULTS');
const end = app.indexOf('async function waitForSyncBridgeReady', start);
const fixtureStart = tests.indexOf('function nativeProjectListFixture(');
const fixtureEnd = tests.indexOf("test('project list starts", fixtureStart);
assert.ok(start >= 0 && end > start && fixtureStart >= 0 && fixtureEnd > fixtureStart);
const output = process.argv.includes('--output-dir')
  ? path.resolve(process.argv[process.argv.indexOf('--output-dir') + 1]) : null;
const browser = await chromium.launch({ headless: true,
  ...(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH
    ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH } : {}) });
try {
  const context = await browser.newContext();
  await context.route('**/*', (route) => route.abort());
  const page = await context.newPage();
  const results = await page.evaluate(async ({ controlSource, fixtureSource, executionSource }) => {
    const assert = {
      ok(value) { if (!value) throw new Error('Expected truthy'); },
      equal(left, right) { if (left !== right) throw new Error(`Expected ${right}, got ${left}`); },
      deepEqual(left, right) { this.equal(JSON.stringify(left), JSON.stringify(right)); },
      match(value, regex) { this.ok(regex.test(value)); },
      fail(message) { throw new Error(message); },
    };
    // Execute the actual app control and the same transport fixture as the
    // Node regressions, using browser Promise/AbortSignal/timer implementations.
    const vm = { runInNewContext(code, scope) {
      scope.invoke = new Function('state', 'actorContext', 'newId', 'AbortController',
        'setTimeout', 'clearTimeout', `${controlSource}\nreturn workjetProjectControl;`)(
        scope.state, scope.actorContext, scope.newId, AbortController, setTimeout, clearTimeout,
      );
    } };
    const fixture = new Function('assert', 'vm', 'controlSource',
      `${fixtureSource}\nreturn nativeProjectListFixture;`)(assert, vm, controlSource);
    const results = [];
    const live = fixture();
    const value = await live.invoke();
    assert.equal(value.projects[0].workingCopies[0].id, 'native-copy');
    assert.equal(value.count, 1);
    assert.equal(value.truncated, false);
    assert.equal(live.reads.length, 2);
    assert.ok(live.reads.every(({ query }) => query.signal.aborted));
    results.push('current native projects and copies without historical pull');
    live.rows.workjet_projects.length = 0;
    live.rows.workjet_working_copies.length = 0;
    const empty = await live.invoke();
    assert.equal(empty.projects.length, 0);
    assert.equal(empty.count, 0);
    assert.equal(empty.truncated, false);
    assert.ok(live.reads[0].query.requireRevision !== live.reads[2].query.requireRevision);
    results.push('fresh native empty result');
    const replaced = fixture({ exec: (name, query, peer) => {
      peer.generation = 'replacement'; return [];
    } });
    let rejected = false;
    try { await replaced.invoke(); } catch (error) { rejected = /generation changed/.test(error.message); }
    assert.ok(rejected);
    assert.ok(replaced.reads.every(({ query }) => query.signal.aborted));
    results.push('replaced generation rejects and aborts');
    const incomplete = fixture({ exec: () => [] });
    let incompleteRejected = false;
    try { await incomplete.invoke(); } catch (error) {
      incompleteRejected = error.code === 'WORKJET_PROJECT_LIST_INCOMPLETE';
    }
    assert.ok(incompleteRejected);
    assert.ok(incomplete.reads.every(({ query }) => query.signal.aborted));
    results.push('native nonzero count rejects an empty projection');
    const legacy = fixture({ dispatch: (receipt) => {
      delete receipt.result.count;
      return receipt;
    } });
    let legacyRejected = false;
    try { await legacy.invoke(); } catch (error) {
      legacyRejected = error.code === 'WORKJET_PROJECT_LIST_UNCONFIRMED';
    }
    assert.ok(legacyRejected);
    assert.equal(legacy.reads.length, 0);
    results.push('legacy unconfirmed native count rejects before query');
    const originalNow = Date.now;
    let now = 10_000;
    try {
      Date.now = () => now;
      const pending = fixture({
        dispatch: (receipt) => { now = 38_995; return receipt; },
        exec: (name, query) => new Promise((resolve, reject) => {
          query.signal.addEventListener('abort', () => reject(new Error('Aborted')), { once: true });
        }),
      });
      let timedOut = false;
      try { await pending.invoke(); } catch (error) { timedOut = error.code === 'WORKJET_PROJECT_TIMEOUT'; }
      assert.ok(timedOut);
      assert.equal(pending.reads.length, 2);
      assert.ok(pending.reads.every(({ query }) => query.signal.aborted));
      results.push('shared deadline aborts both browser query streams');
    } finally { Date.now = originalNow; }

    const turnId = 'actual-native-command';
    const threadId = 'cc6cfe73-2824-4360-9daf-3b3efb079931';
    let corrupt = false;
    const state = {
      session: { id: 'owner' }, db: { collection: name => name === 'business_commands' ? {} : null },
      sync: { async startCollection() {} },
      commandBus: { async dispatch(command) {
        return { command_id: command.id, ok: true, status: 'completed', target_record_id: 'project', payload: command.payload,
          result: { ok: true, contract: 'ctox.workjet.supervisor_turn.v1',
            binding: { project_id: 'project', thread_id: threadId, thread_key: `business-os/threads/${threadId}` },
            turn: { command_id: turnId, task_id: 'actual-native-task', thread_id: threadId,
              thread_key: `business-os/threads/${threadId}`, execution_phase: 'queued', status: 'queued', queue_status: 'pending',
              attempt: 0, terminal: false, result: {}, result_truncated: false },
            execution_contract: 'ctox.workjet.supervisor_execution.v1',
            execution_page: { command_id: turnId, task_id: corrupt ? 'foreign-task' : 'actual-native-task',
              attempt: { attempt_id: 'actual-native-attempt', attempt_index: 47 },
              events: [{ id: 'actual-event', sequence: 22, kind: 'worker.phase', title: 'Recorded step', created_at_ms: 1791410400000 }],
              next_cursor: { after_sequence: 22, after_event_id: 'actual-event' }, has_more: false },
          } };
      } },
    };
    const invoke = new Function('state', 'actorContext', `${executionSource}\n${controlSource}\nreturn workjetProjectControl;`)(state, session => ({ id: session.id }));
    const request = { action: 'project.supervisor.turn.watch', commandId: 'browser-watch', projectId: 'project', threadId, targetCommandId: turnId };
    const legacyWatch = await invoke(request);
    assert.deepEqual(Object.keys(legacyWatch).sort(), ['action', 'binding', 'commandId', 'contract', 'projectId', 'turn']);
    results.push('legacy watch outer contract remains exact');
    const observed = await invoke({ ...request, executionPage: { limit: 1 } });
    assert.equal(observed.executionContract, 'ctox.workjet.supervisor_execution.v1');
    assert.equal(observed.executionPage.attempt.attempt_id, 'actual-native-attempt');
    assert.equal(observed.executionPage.attempt.attempt_index, 47);
    assert.equal(observed.executionPage.events[0].id, 'actual-event');
    results.push('opted-in browser watch returns actual native attempt and event');
    corrupt = true;
    let foreignRejected = false;
    try { await invoke({ ...request, executionPage: {} }); } catch { foreignRejected = true; }
    assert.ok(foreignRejected);
    results.push('foreign native task page fails correlation');

    return results;
  }, { controlSource: app.slice(start, end), fixtureSource: tests.slice(fixtureStart, fixtureEnd), executionSource });
  assert.equal(results.length, 9);
  const report = { passed: results.length, failed: 0, cases: results,
    evidenceScope: 'Actual source control in isolated Chromium with a controlled native contract fixture; not installed native or Workjet UI acceptance',
    browserVersion: browser.version() };
  if (output) { mkdirSync(output, { recursive: true }); writeFileSync(path.join(output, 'result.json'), JSON.stringify(report, null, 2)); }
  console.log(JSON.stringify(report, null, 2));
  await context.close();
} finally { await browser.close(); }
