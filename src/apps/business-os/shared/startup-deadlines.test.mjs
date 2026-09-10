import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

import {
  LAUNCH_CONTEXT_DEADLINE_MS,
  SHELL_GENERATION_PROBE_DEADLINE_MS,
  cancelStartupResponseBody,
  isStartupDeadlineError,
  shouldPropagateGenerationProbeError,
  startupDeadlineError,
  withStartupDeadline,
} from './startup-deadlines.js';

function immediateTimers() {
  const cleared = [];
  return {
    cleared,
    setTimeout: () => 421,
    clearTimeout: (handle) => cleared.push(handle),
  };
}

test('startup deadlines have the launch and generation-probe budgets', () => {
  assert.equal(LAUNCH_CONTEXT_DEADLINE_MS, 30_000);
  assert.equal(SHELL_GENERATION_PROBE_DEADLINE_MS, 5_000);
});

test('startup timeouts use the agreed typed network code', () => {
  const error = startupDeadlineError('startup request timed out');

  assert.equal(error.code, 'CTOX_STARTUP_NETWORK_TIMEOUT');
  assert.equal(isStartupDeadlineError(error), true);
  assert.equal(isStartupDeadlineError(new Error('RxDB database creation timed out')), false);
});

test('generation probe timeouts propagate while ordinary probe errors retry', () => {
  const timeout = startupDeadlineError('startup request timed out');

  assert.equal(shouldPropagateGenerationProbeError(timeout, false), true);
  assert.equal(shouldPropagateGenerationProbeError(new Error('probe failed'), false), false);
  assert.equal(shouldPropagateGenerationProbeError(new Error('probe failed'), true), true);
});

test('completed probe bodies cancel without surfacing cleanup failures', async () => {
  let cancelCount = 0;

  await cancelStartupResponseBody({ body: { cancel: async () => { cancelCount += 1; } } });
  await cancelStartupResponseBody({ body: { cancel: async () => { throw new Error('cleanup failed'); } } });
  await cancelStartupResponseBody(undefined);

  assert.equal(cancelCount, 1);
});

test('the generation probe wiring propagates typed timeouts and cancels completed bodies', async () => {
  const appSource = await readFile(new URL('../app.js', import.meta.url), 'utf8');

  assert.match(appSource, /const reloadGeneration = scheduleShellGenerationReload\(generationProbe\);\s*await cancelStartupResponseBody\(generationProbe\);/);
  assert.match(appSource, /if \(shouldPropagateGenerationProbeError\(generationError, shellGenerationReloadGuard\.scheduled\)\) \{\s*throw generationError;/);
});

test('the deadline wins while the request or response body is pending', async () => {
  const timers = immediateTimers();
  let resolveBody;
  let settled = false;
  const pending = withStartupDeadline(async () => {
    await Promise.resolve();
    return await { text: () => new Promise((resolve) => { resolveBody = resolve; }) }.text();
  }, 20, 'startup request timed out');

  pending.then(() => { settled = true; }, () => { settled = true; });
  const error = await pending.catch((caught) => caught);
  assert.equal(isStartupDeadlineError(error), true);
  assert.equal(settled, true);

  resolveBody('late body');
  await new Promise((resolve) => setTimeout(resolve, 20));
  assert.equal(settled, true, 'a late body must not change the settled result');
});

test('the deadline aborts the signal and the request can be retried', async () => {
  let observedSignal;
  const first = withStartupDeadline((signal) => {
    observedSignal = signal;
    return new Promise(() => {});
  }, 20, 'startup request timed out');

  await assert.rejects(first, (error) => isStartupDeadlineError(error));
  assert.equal(observedSignal.aborted, true);

  assert.equal(await withStartupDeadline(async () => 'retried', 20, 'startup request timed out'), 'retried');
});

test('an operation that finishes first clears its timeout', async () => {
  const timers = immediateTimers();
  let observedSignal;
  const result = await withStartupDeadline((signal) => {
    observedSignal = signal;
    return 'ready';
  }, 1_000, 'startup request timed out', timers);

  assert.equal(result, 'ready');
  assert.equal(observedSignal.aborted, false);
  assert.deepEqual(timers.cleared, [421]);
});

test('a rejection after a timeout stays suppressed', async () => {
  let rejectLate;
  const first = withStartupDeadline(() => new Promise((_, reject) => { rejectLate = reject; }), 20, 'startup request timed out');
  await assert.rejects(first, (error) => isStartupDeadlineError(error));
  rejectLate(new Error('late failure'));
  await new Promise((resolve) => setTimeout(resolve, 20));
});
