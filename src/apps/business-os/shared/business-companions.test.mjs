import assert from 'node:assert/strict';
import test from 'node:test';

import { createBusinessCompanionScheduler } from './business-companions.js';

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function createHarness({ session }) {
  const reporterImport = deferred();
  const chatImport = deferred();
  const schemaRegistration = deferred();
  const reporterInitializations = [];
  const chatInitializations = [];
  const state = { session };
  const reporterModule = {
    initBusinessReporter: (context) => reporterInitializations.push(context),
  };
  const chatModule = {
    initBusinessChat: (context) => chatInitializations.push(context),
  };

  const scheduler = createBusinessCompanionScheduler({
    loadBusinessReporterModule: () => reporterImport.promise.then(() => reporterModule),
    loadBusinessChatModule: () => chatImport.promise.then(() => chatModule),
    getSession: () => state.session,
    findCtoxModule: () => ({ id: 'ctox' }),
    registerModuleSchemas: () => schemaRegistration.promise,
    createReporterContext: (capturedSession) => ({ capturedSession }),
    createChatContext: (capturedSession) => ({
      capturedSession,
      schemaRan: false,
    }),
    onError: (error) => { throw error; },
  });

  return {
    scheduler,
    state,
    reporterModule,
    chatModule,
    reporterImport,
    chatImport,
    schemaRegistration,
    reporterInitializations,
    chatInitializations,
  };
}

async function settle() {
  await Promise.resolve();
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 0));
}

test('an authenticated replacement initializes only the fresh session', async () => {
  const sessionA = { authenticated: true, user: { id: 'a' } };
  const sessionB = { authenticated: true, user: { id: 'b' } };
  const harness = createHarness({ session: sessionA });

  harness.scheduler.schedule();
  harness.state.session = sessionB;
  const runB = harness.scheduler.schedule();

  harness.reporterImport.resolve();
  harness.chatImport.resolve();
  harness.schemaRegistration.resolve();
  await settle();

  assert.deepEqual(harness.reporterInitializations.map(({ session }) => session), [sessionB]);
  assert.deepEqual(harness.chatInitializations.map(({ session }) => session), [sessionB]);
  assert.equal(runB.session, sessionB);
});

test('logout while imports are pending initializes neither companion', async () => {
  const harness = createHarness({ session: { authenticated: true, user: { id: 'a' } } });

  assert.notEqual(harness.scheduler.schedule(), null);
  harness.scheduler.cancel();

  harness.reporterImport.resolve();
  harness.chatImport.resolve();
  harness.schemaRegistration.resolve();
  await settle();

  assert.deepEqual(harness.reporterInitializations, []);
  assert.deepEqual(harness.chatInitializations, []);
});

test('logout during awaited chat schema registration prevents stale initialization', async () => {
  const sessionA = { authenticated: true, user: { id: 'a' } };
  const harness = createHarness({ session: sessionA });

  harness.scheduler.schedule();
  harness.reporterImport.resolve();
  harness.chatImport.resolve();

  // Let the chat branch reach its awaited schema registration before logout.
  await settle();
  harness.scheduler.cancel();
  harness.schemaRegistration.resolve();
  await settle();

  assert.deepEqual(
    harness.reporterInitializations.map(({ session }) => session),
    [sessionA],
  );
  assert.deepEqual(harness.chatInitializations, []);
});

test('an unauthenticated schedule captures nothing', () => {
  const harness = createHarness({ session: { authenticated: false } });

  assert.equal(harness.scheduler.schedule(), null);
});
