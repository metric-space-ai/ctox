'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const { performance } = require('node:perf_hooks');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
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
    while (!matches(await readNativeLayout())) {
      if (Date.now() >= deadline) throw new Error('native pin seed did not converge');
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
    await fresh.waitForFunction(expected => {
      const state = globalThis.ctoxBusinessOsSmoke?.state;
      return state?.taskbarPinsUpdatedAtMs === expected.updated_at_ms
        && JSON.stringify(state.taskbarPins) === JSON.stringify(expected.taskbar_pins);
    }, expected, { timeout: 60000 });
    report.pinConvergenceMs = performance.now() - started;
    await fresh.waitForFunction(expected => {
      const pins = [...document.querySelectorAll('button.module-tab[data-pinned="true"]')]
        .filter(button => button.getClientRects().length > 0).map(button => button.dataset.target);
      return JSON.stringify(pins) === JSON.stringify(expected.taskbar_pins);
    }, expected, { timeout: 10000 });
    report.visiblePinsMs = performance.now() - started;
    assert.ok(matches(await readNativeLayout()), 'fresh browser overwrote native pins');
    await fresh.reload({ waitUntil: 'commit', timeout: 60000 });
    await fresh.waitForFunction(expected => {
      const state = globalThis.ctoxBusinessOsSmoke?.state;
      return state?.taskbarPinsUpdatedAtMs === expected.updated_at_ms
        && JSON.stringify(state.taskbarPins) === JSON.stringify(expected.taskbar_pins);
    }, expected, { timeout: 60000 });
    assert.ok(matches(await readNativeLayout()), 'reload changed native pins');
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
