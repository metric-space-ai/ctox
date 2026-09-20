'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const { performance } = require('node:perf_hooks');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function capturePinRuntimeEvidence(page, error = null) {
  const waitForError = String(error?.message || error || '');
  const timeoutMs = 5000;
  let diagnosticTimer = null;
  const evaluate = page.evaluate(async waitForError => {
    const smoke = globalThis.ctoxBusinessOsSmoke;
    const state = smoke?.state;
    const syncDiagnostics = state?.syncDiagnostics?.collections?.desktop_layout || null;
    return {
      waitForError,
      appBuild: smoke?.appBuild || null,
      appEntryUrl: document.querySelector('script[type="module"][src*="app.js"]')?.src || null,
      rxdbBundleUrls: performance.getEntriesByType('resource')
        .map(entry => entry.name)
        .filter(name => name.includes('ctox-rxdb-js.mjs')),
      taskbar: {
        pins: [...(state?.taskbarPins || [])],
        timestamp: state?.taskbarPinsUpdatedAtMs || 0,
        known: state?.taskbarPinsKnown === true,
      },
      hydrationRetry: {
        count: state?.taskbarPinHydrationRetryCount || 0,
        startedAtMs: state?.taskbarPinHydrationRetryStartedAtMs || 0,
        lastError: state?.taskbarPinHydrationLastError || null,
      },
      hydrationAttempts: [...(state?.taskbarPinHydrationAttempts || [])],
      desktopLayoutDiagnostics: syncDiagnostics,
    };
  }, waitForError).catch(diagnosticError => ({
    diagnosticError: String(diagnosticError?.message || diagnosticError),
    waitForError,
  }));
  const timeout = new Promise((resolve) => {
    diagnosticTimer = setTimeout(() => resolve({
      diagnosticError: `diagnostic evaluate exceeded ${timeoutMs}ms`,
      diagnosticTimeoutMs: timeoutMs,
      waitForError,
    }), timeoutMs);
  });
  try {
    return await Promise.race([evaluate, timeout]);
  } finally {
    clearTimeout(diagnosticTimer);
  }
}

// Seed diagnostics must not repair/restart replication or turn a local read
// into demand traffic. Retain only fixture pin IDs and scalar health fields.
function summarizePinSeedDocument(doc) {
  if (!doc) return null;
  const pins = Array.isArray(doc.taskbar_pins) ? doc.taskbar_pins : null;
  return {
    layoutIdMatches: doc.id === 'layout',
    taskbar_pins: pins?.slice(0, 16).map(id => ['tickets', 'ctox'].includes(id) ? id : '[other]') ?? null,
    pinCount: pins?.length ?? null,
    updated_at_ms: Number.isFinite(doc.updated_at_ms) ? doc.updated_at_ms : null,
    deleted: doc._deleted === true,
  };
}

async function capturePinSeedBrowserEvidence(page) {
  let timer;
  const capture = page.evaluate(async () => {
    const state = globalThis.ctoxBusinessOsSmoke?.state;
    const collection = state?.db?.collection?.('desktop_layout');
    const diagnostics = state?.sync?.diagnostics?.collections?.desktop_layout
      || state?.syncDiagnostics?.collections?.desktop_layout;
    const resources = state?.sync?.resourceSnapshot?.();
    const scalar = value => typeof value === 'boolean' || Number.isFinite(value) ? value : null;
    const status = value => [
      'pending', 'starting', 'running', 'stopped', 'skipped', 'error', 'failed',
      'connected', 'disconnected', 'connecting', 'reconnecting', 'restarting',
      'complete', 'waiting-for-peer', 'stalled', 'stalled-waiting-for-peer',
      'demand-only', 'paused', 'ready', 'live',
    ].includes(value) ? value : value == null ? null : '[other]';
    const error = value => value ? {
      present: true,
      // Messages/stacks can contain records, URLs or credentials. Keep only
      // conventional symbolic codes, never arbitrary error text.
      code: typeof value.code === 'string' && /^(?:[A-Z][A-Z_]{0,63}|ctox_[a-z_]{1,64})$/.test(value.code)
        ? value.code : null,
      retryable: scalar(value.retryable),
    } : null;
    const result = {
      capturedAtMs: Date.now(),
      collectionPresent: Boolean(collection),
      demandLoaderPresent: Boolean(collection?.demandLoader),
      resources: resources ? {
        active: resources.activeCollections?.includes('desktop_layout') === true,
        bridge: resources.bridgeCollections?.includes('desktop_layout') === true,
        pinned: resources.pinnedCollections?.includes('desktop_layout') === true,
        leaseCount: scalar(resources.leaseCounts?.desktop_layout ?? 0),
      } : null,
      replication: diagnostics ? {
        status: status(diagnostics.status),
        connectionStatus: status(diagnostics.connectionStatus),
        initialReplicationState: status(diagnostics.initialReplicationState),
        active: scalar(diagnostics.active),
        queryReady: scalar(diagnostics.queryReady),
        activePeerCount: scalar(diagnostics.frameTransport?.activePeerCount),
        pushInProgress: scalar(diagnostics.frameTransport?.pushInProgress),
        pullInProgress: scalar(diagnostics.frameTransport?.pullInProgress),
        lastError: error(diagnostics.lastError),
        lastLifecycleEvent: error(diagnostics.lastLifecycleEvent),
      } : null,
      hydrationError: error(state?.taskbarPinHydrationLastError),
    };
    try {
      if (!collection?.storageCollection?.findDocumentsById) {
        return { ...result, documentUnavailable: 'local-reader-unavailable' };
      }
      const docs = await collection.storageCollection.findDocumentsById(['layout'], { withDeleted: true });
      const doc = docs?.layout;
      const pins = Array.isArray(doc?.taskbar_pins) ? doc.taskbar_pins : null;
      result.document = doc ? {
        layoutIdMatches: doc.id === 'layout',
        taskbar_pins: pins?.slice(0, 16).map(id => ['tickets', 'ctox'].includes(id) ? id : '[other]') ?? null,
        pinCount: pins?.length ?? null,
        updated_at_ms: Number.isFinite(doc.updated_at_ms) ? doc.updated_at_ms : null,
        deleted: doc._deleted === true,
      } : null;
    } catch {
      result.documentUnavailable = 'local-read-failed';
    }
    return result;
  }).catch(() => ({ unavailable: 'browser-evaluate-failed' }));
  try {
    return await Promise.race([capture, new Promise(resolve => {
      timer = setTimeout(() => resolve({ unavailable: 'browser-evaluate-timeout', timeoutMs: 3000 }), 3000);
    })]);
  } finally {
    clearTimeout(timer);
  }
}

async function openHeldPinContext({ browser, url, storageState, capturePinWrites = false }) {
  let held = true;
  let heldBytes = 0;
  let gateError = null;
  const messages = [];
  const release = () => {
    if (!held) return;
    held = false;
    for (const { socket, message } of messages.splice(0)) socket.send(message);
  };
  const context = await browser.newContext({ storageState });
  if (capturePinWrites) {
    await context.addInitScript(() => {
      const writes = [];
      globalThis.__ctoxPinCacheWrites = writes;
      const originalSetItem = Storage.prototype.setItem;
      Storage.prototype.setItem = function setItem(key, value) {
        if (key === 'ctox.businessOs.taskbarPins' || String(key || '').includes('.taskbarPins')) {
          writes.push({ at: Date.now(), key, value: String(value || '') });
        }
        return originalSetItem.call(this, key, value);
      };
    });
  }
  await context.routeWebSocket(/.*/, socket => {
    const server = socket.connectToServer();
    server.onMessage(message => {
      if (!held) {
        socket.send(message);
        return;
      }
      heldBytes += Buffer.byteLength(message);
      if (messages.length >= 128 || heldBytes > 1024 * 1024) {
        gateError = new Error('pin signaling delay exceeded its bounded buffer');
        void socket.close();
        return;
      }
      messages.push({ socket, message });
    });
  });
  const page = await context.newPage();
  await page.goto(url, { waitUntil: 'commit', timeout: 60000 });
  await page.waitForFunction(() => Boolean(globalThis.ctoxBusinessOsSmoke?.state?.sync),
    null, { timeout: 15000 });
  return { context, page, messages, release, gateError: () => gateError };
}

// Runs only against the disposable native smoke host. Business records travel
// through real WebRTC; the proxy delays signaling messages without fabricating
// query responses. A fresh context shares the browser process, not IndexedDB.
async function runDesktopPinReload({ page, readNativeLayout, outputPath }) {
  const report = { schema: 'ctox.desktop_pin_reload.v1', ok: false };
  let context;
  let pendingContext;
  let sessionContext;
  let release = () => {};
  try {
    const expected = { taskbar_pins: ['tickets', 'ctox'], updated_at_ms: Date.now() };
    await page.evaluate(async expected => {
      const { state } = globalThis.ctoxBusinessOsSmoke;
      const collection = state.db.collection('desktop_layout');
      const doc = await collection.findOne('layout').exec();
      if (doc) await doc.incrementalPatch(expected);
      else await collection.insert({ id: 'layout', ...expected });
    }, expected);
    const matches = (doc, wanted = expected) => doc && doc.updated_at_ms === wanted.updated_at_ms
      && JSON.stringify(doc.taskbar_pins) === JSON.stringify(wanted.taskbar_pins);
    const deadline = Date.now() + 60000;
    report.seedDiagnostics = { expected, deadlineMs: deadline };
    report.seedDiagnostics.browserAfterMutation = await capturePinSeedBrowserEvidence(page);
    let nativeSeedObservation;
    while (!matches(nativeSeedObservation = await readNativeLayout())) {
      if (Date.now() >= deadline) {
        report.seedDiagnostics.lastNativeDocument = summarizePinSeedDocument(nativeSeedObservation);
        report.seedDiagnostics.timedOutAtMs = Date.now();
        report.seedDiagnostics.browserAtTimeout = await capturePinSeedBrowserEvidence(page);
        throw new Error('native pin seed did not converge');
      }
      await delay(100);
    }
    report.nativeSeed = await readNativeLayout();
    const pinKey = await page.evaluate(() => globalThis.ctoxBusinessOsSmoke.storageKeys().taskbarPins);
    const storageState = await page.context().storageState();
    for (const origin of storageState.origins) {
      origin.localStorage = origin.localStorage.filter(entry => entry.name !== pinKey
        && entry.name !== 'ctox.businessOs.taskbarPins');
    }
    const browser = page.context().browser();
    if (!browser) throw new Error('pin acceptance requires a reusable browser process');
    context = await browser.newContext({ storageState });
    let held = true;
    let heldBytes = 0;
    let gateError = null;
    const messages = [];
    release = () => {
      if (!held) return;
      held = false;
      for (const { socket, message } of messages.splice(0)) socket.send(message);
    };
    await context.addInitScript(() => {
      const writes = [];
      globalThis.__ctoxPinCacheWrites = writes;
      const originalSetItem = Storage.prototype.setItem;
      Storage.prototype.setItem = function setItem(key, value) {
        if (key === 'ctox.businessOs.taskbarPins' || String(key || '').includes('.taskbarPins')) {
          writes.push({
            at: Date.now(),
            key,
            value: String(value || ''),
            stack: new Error().stack || '',
          });
        }
        return originalSetItem.call(this, key, value);
      };
    });
    await context.routeWebSocket(/.*/, socket => {
      const server = socket.connectToServer();
      server.onMessage(message => {
        if (!held) { socket.send(message); return; }
        heldBytes += Buffer.byteLength(message);
        if (messages.length >= 128 || heldBytes > 1024 * 1024) {
          gateError = new Error('pin signaling delay exceeded its bounded buffer');
          void socket.close();
          return;
        }
        messages.push({ socket, message });
      });
    });
    const fresh = await context.newPage();
    const started = performance.now();
    await fresh.goto(page.url(), { waitUntil: 'commit', timeout: 60000 });
    await fresh.waitForFunction(() => Boolean(globalThis.ctoxBusinessOsSmoke?.state?.sync),
      null, { timeout: 15000 });
    report.syncRegisteredMs = performance.now() - started;
    // Exceed the removed timeout while the native peer cannot answer.
    await delay(2000);
    const pending = await fresh.evaluate(pinKey => ({
      cache: localStorage.getItem(pinKey),
      timestamp: globalThis.ctoxBusinessOsSmoke.state.taskbarPinsUpdatedAtMs,
      known: globalThis.ctoxBusinessOsSmoke.state.taskbarPinsKnown,
      pinWrites: globalThis.__ctoxPinCacheWrites || [],
    }), pinKey);
    report.pending = pending;
    assert.equal(pending.cache, null, 'unanswered native layout must not create a pin cache');
    assert.equal(Number(pending.timestamp || 0), 0, 'startup must not invent a pin timestamp');
    assert.equal(pending.known, false, 'pending authority must remain unknown');
    assert.equal(pending.pinWrites.length, 0, 'no first pin-cache writer may run before native authority');
    assert.ok(matches(await readNativeLayout()), 'native pins changed before signaling release');
    if (gateError) throw gateError;
    assert.ok(messages.length > 0, 'test must actually hold signaling messages');
    report.signalingReleasedMs = performance.now() - started;
    release();
    try {
      await fresh.waitForFunction(expected => {
        const state = globalThis.ctoxBusinessOsSmoke?.state;
        return state?.taskbarPinsUpdatedAtMs === expected.updated_at_ms
          && JSON.stringify(state.taskbarPins) === JSON.stringify(expected.taskbar_pins);
      }, expected, { timeout: 60000 });
    } catch (error) {
      report.firstPinConvergenceRuntime = await capturePinRuntimeEvidence(fresh, error);
      throw error;
    }
    report.pinConvergenceMs = performance.now() - started;
    report.firstAuthorityRuntime = await capturePinRuntimeEvidence(fresh);
    const adoption = report.firstAuthorityRuntime.hydrationAttempts?.find?.((attempt) => (
      attempt?.outcome === 'adopted'
      && Number(attempt.remoteUpdatedAtMs) === expected.updated_at_ms
      && Number(attempt.adoptedUpdatedAtMs) === expected.updated_at_ms
      && attempt.remotePinCount === expected.taskbar_pins.length
    ));
    assert.ok(adoption, 'successful acceptance must bind convergence to an adopted strict-read attempt');
    assert.equal(adoption.resolvedSource, 'remote');
    assert.equal(adoption.knownAfterRead, true);
    await fresh.waitForFunction(expected => {
      const pins = [...document.querySelectorAll('button.module-tab[data-pinned="true"]')]
        .filter(button => button.getClientRects().length > 0).map(button => button.dataset.target);
      return JSON.stringify(pins) === JSON.stringify(expected.taskbar_pins);
    }, expected, { timeout: 10000 });
    report.visiblePinsMs = performance.now() - started;
    const nativeBeforeReload = await readNativeLayout();
    report.nativeBeforeReload = nativeBeforeReload;
    assert.ok(matches(nativeBeforeReload), 'fresh browser overwrote native pins');
    await fresh.reload({ waitUntil: 'commit', timeout: 60000 });
    await fresh.waitForFunction(expected => {
      const state = globalThis.ctoxBusinessOsSmoke?.state;
      return state?.taskbarPinsUpdatedAtMs === expected.updated_at_ms
        && JSON.stringify(state.taskbarPins) === JSON.stringify(expected.taskbar_pins);
    }, expected, { timeout: 60000 });
    const nativeAfterReload = await readNativeLayout();
    report.nativeAfterReload = nativeAfterReload;
    assert.ok(
      matches(nativeAfterReload),
      `reload changed native pins: ${JSON.stringify({ expected, before: nativeBeforeReload, after: nativeAfterReload })}`,
    );
    // Exercise a real user edit while a second fresh session's signaling is
    // held. This is not a synthetic state mutation: it uses the taskbar
    // context menu's trailing pin action and the production cache path.
    const pendingStorage = await fresh.context().storageState();
    const heldEdit = await openHeldPinContext({
      browser,
      url: page.url(),
      storageState: pendingStorage,
      capturePinWrites: true,
    });
    pendingContext = heldEdit.context;
    await delay(1000);
    const pinnedButton = heldEdit.page.locator(
      'button.module-tab[data-target="ctox"][data-pinned="true"]',
    ).first();
    await pinnedButton.waitFor({ state: 'visible', timeout: 10000 });
    await pinnedButton.click({ button: 'right' });
    const toggleButton = heldEdit.page.locator('.shell-context-menu-trailing').first();
    await toggleButton.waitFor({ state: 'visible', timeout: 5000 });
    await toggleButton.click();
    const pendingEdit = await heldEdit.page.evaluate(pinKey => {
      const state = globalThis.ctoxBusinessOsSmoke.state;
      const raw = localStorage.getItem(pinKey);
      let cache = null;
      try { cache = raw ? JSON.parse(raw) : null; } catch { cache = null; }
      return {
        pins: [...state.taskbarPins],
        timestamp: state.taskbarPinsUpdatedAtMs,
        known: state.taskbarPinsKnown,
        cache,
        pinWrites: globalThis.__ctoxPinCacheWrites || [],
      };
    }, pinKey);
    const editedExpected = {
      taskbar_pins: pendingEdit.pins,
      updated_at_ms: pendingEdit.timestamp,
    };
    report.pendingUserEdit = pendingEdit;
    assert.deepEqual(pendingEdit.pins, ['tickets'], 'real context-menu edit must change the selection');
    assert.equal(pendingEdit.known, true, 'an explicit user edit is known immediately');
    assert.ok(pendingEdit.timestamp > expected.updated_at_ms, 'a real edit owns a strictly newer timestamp');
    assert.equal(pendingEdit.cache?.updated_at_ms, pendingEdit.timestamp, 'real pending edit persists locally');
    assert.equal(pendingEdit.pinWrites.length >= 1, true, 'real edit exercises the production cache writer');
    assert.ok(matches(await readNativeLayout()), 'held pending edit changed native pins');
    if (heldEdit.gateError()) throw heldEdit.gateError();
    assert.ok(heldEdit.messages.length > 0, 'pending-edit test must actually hold signaling');
    report.pendingEditReleasedMs = performance.now() - started;
    heldEdit.release();
    await heldEdit.page.waitForFunction(expected => {
      const state = globalThis.ctoxBusinessOsSmoke?.state;
      return state?.taskbarPinsUpdatedAtMs === expected.updated_at_ms
        && JSON.stringify(state.taskbarPins) === JSON.stringify(expected.taskbar_pins);
    }, editedExpected, { timeout: 60000 });
    let nativeEditDeadline = Date.now() + 60000;
    while (!matches(await readNativeLayout(), editedExpected)) {
      if (Date.now() >= nativeEditDeadline) throw new Error('pending edit did not converge in native layout');
      await delay(100);
    }
    report.pendingEditConvergenceMs = performance.now() - started;

    // A later session must not inherit the pending cache from storageState.
    // It receives the selection from native layout through the same strict read.
    const sessionStorage = await heldEdit.page.context().storageState();
    for (const origin of sessionStorage.origins) {
      origin.localStorage = origin.localStorage.filter(entry => entry.name !== pinKey
        && entry.name !== 'ctox.businessOs.taskbarPins');
    }
    sessionContext = await browser.newContext({ storageState: sessionStorage });
    const sessionPage = await sessionContext.newPage();
    await sessionPage.goto(page.url(), { waitUntil: 'commit', timeout: 60000 });
    await sessionPage.waitForFunction(() => Boolean(globalThis.ctoxBusinessOsSmoke?.state?.sync),
      null, { timeout: 15000 });
    await sessionPage.waitForFunction(expected => {
      const state = globalThis.ctoxBusinessOsSmoke?.state;
      return state?.taskbarPinsUpdatedAtMs === expected.updated_at_ms
        && JSON.stringify(state.taskbarPins) === JSON.stringify(expected.taskbar_pins);
    }, editedExpected, { timeout: 60000 });
    await sessionPage.waitForFunction(expected => {
      const pins = [...document.querySelectorAll('button.module-tab[data-pinned="true"]')]
        .filter(button => button.getClientRects().length > 0)
        .map(button => button.dataset.target);
      return JSON.stringify(pins) === JSON.stringify(expected.taskbar_pins);
    }, editedExpected, { timeout: 10000 });
    const visiblePins = await sessionPage.evaluate(expected => (
      [...document.querySelectorAll('button.module-tab[data-pinned="true"]')]
        .filter(button => button.getClientRects().length > 0)
        .map(button => button.dataset.target)
    ), editedExpected);
    assert.deepEqual(visiblePins, editedExpected.taskbar_pins, 'session switch must show native authority');
    report.sessionSwitchConvergenceMs = performance.now() - started;
    report.ok = true;
    return report;
  } catch (error) {
    report.error = error.message;
    throw error;
  } finally {
    // Closing owned contexts discards held messages on failure, without
    // releasing an unverified browser's pending writes into the fixture.
    await Promise.allSettled([context, pendingContext, sessionContext]
      .filter(Boolean)
      .map(owned => owned.close()));
    fs.writeFileSync(outputPath, JSON.stringify(report, null, 2) + '\n');
  }
}

module.exports = { runDesktopPinReload };
