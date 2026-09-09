'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const { performance } = require('node:perf_hooks');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));

// Runs only against the disposable native smoke host. Business records travel
// through real WebRTC; the proxy delays signaling messages without fabricating
// query responses. A fresh context shares the browser process, not IndexedDB.
async function runDesktopPinReload({ page, readNativeLayout, outputPath }) {
  const report = { schema: 'ctox.desktop_pin_reload.v1', ok: false };
  let context;
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
    const matches = doc => doc && doc.updated_at_ms === expected.updated_at_ms
      && JSON.stringify(doc.taskbar_pins) === JSON.stringify(expected.taskbar_pins);
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
    }), pinKey);
    report.pending = pending;
    assert.equal(pending.cache, null, 'unanswered native layout must not create a pin cache');
    assert.equal(Number(pending.timestamp || 0), 0, 'startup must not invent a pin timestamp');
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
    report.ok = true;
    return report;
  } catch (error) {
    report.error = error.message;
    throw error;
  } finally {
    // Closing the owned context discards held messages on failure, without
    // releasing an unverified browser's pending writes into the fixture.
    await context?.close();
    fs.writeFileSync(outputPath, JSON.stringify(report, null, 2) + '\n');
  }
}

module.exports = { runDesktopPinReload };
