// Real RTCDataChannels with page timers clamped to one second. This isolates
// scheduling from browser-specific visibility exemptions and the tenant gateway.
import assert from 'node:assert/strict';
import http from 'node:http';
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const playwrightModule = process.env.PLAYWRIGHT_MODULE_PATH
  ? pathToFileURL(resolve(process.env.PLAYWRIGHT_MODULE_PATH, 'index.mjs')).href
  : '../../node_modules/playwright/index.mjs';
const { chromium } = await import(playwrightModule);
const bundle = readFileSync(new URL('../dist/ctox-rxdb-js.mjs', import.meta.url));
const server = http.createServer((request, response) => {
  response.setHeader('content-type', request.url === '/bundle.mjs' ? 'text/javascript' : 'text/html');
  response.end(request.url === '/bundle.mjs' ? bundle : '<!doctype html><title>Frame scheduling proof</title>');
});
await new Promise((ready) => server.listen(0, '127.0.0.1', ready));
const chrome = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
let browser;
try {
  browser = await chromium.launch({ headless: true, ...(existsSync(chrome) ? { executablePath: chrome } : {}) });
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  const result = await page.evaluate(async () => {
    const { createCtoxWebRtcNativePeer } = await import('/bundle.mjs');
    const peers = ['browser', 'remote'].map((clientId) => createCtoxWebRtcNativePeer({
      clientId, room: 'ctox-business-os:test:real-rtc', signalingUrl: 'ws://unused.invalid',
    }));
    const rtc = [new RTCPeerConnection(), new RTCPeerConnection()];
    const channels = [rtc[0].createDataChannel('ctox-rxdb')];
    const transportErrors = [];
    const originalTimeout = globalThis.setTimeout;
    for (const peer of peers) peer.on('error', ({ detail }) => transportErrors.push(detail.code || detail.message));
    const remoteChannel = new Promise((ready) => { rtc[1].ondatachannel = ({ channel }) => ready(channel); });
    try {
      // Gather complete local ICE descriptions, then exchange them in-process.
      const gather = (pc) => pc.iceGatheringState === 'complete' ? Promise.resolve() : new Promise((ready) => {
        pc.addEventListener('icegatheringstatechange', function listener() {
          if (pc.iceGatheringState === 'complete') { pc.removeEventListener('icegatheringstatechange', listener); ready(); }
        });
      });
      await rtc[0].setLocalDescription(await rtc[0].createOffer());
      await gather(rtc[0]);
      await rtc[1].setRemoteDescription(rtc[0].localDescription);
      await rtc[1].setLocalDescription(await rtc[1].createAnswer());
      await gather(rtc[1]);
      await rtc[0].setRemoteDescription(rtc[1].localDescription);
      channels.push(await remoteChannel);
      await Promise.all(channels.map((channel) => channel.readyState === 'open' ? Promise.resolve()
        : new Promise((ready) => channel.addEventListener('open', ready, { once: true }))));
      peers.forEach((peer, index) => {
        const remotePeerId = peers[1 - index].options.clientId;
        const connection = { remotePeerId, channel: channels[index] };
        peer.connections.set(remotePeerId, connection);
        channels[index].onmessage = ({ data }) => peer.enqueueInboundDataChannelFrame(connection, channels[index], JSON.parse(data));
      });
      let accepted = 0;
      peers[1].handleRequest = async (_peer, method, params) => {
        if (method !== 'masterWrite' || params[0].text.length !== 19000) throw new Error('unexpected command');
        accepted += 1;
        return { accepted: true };
      };
      let received = 0;
      const bulkDone = new Promise((ready) => peers.forEach((peer) => peer.on('message', ({ detail }) => {
        if (detail.payload.id !== 'bulk') return;
        if (detail.payload.result.length !== 1_840_000) throw new Error('truncated bulk payload');
        received += 1;
        if (received === 2) ready();
      })));
      globalThis.setTimeout = (callback, ms, ...args) => originalTimeout(callback, Math.max(1000, Number(ms) || 0), ...args);
      const start = performance.now();
      // A backlog of document writes shares this queue with the owner's small
      // chunked command, while the opposite direction delivers large leads.
      peers[0].send('remote', { id: 'bulk', result: 'x'.repeat(1_840_000) });
      peers[1].send('browser', { id: 'bulk', result: 'y'.repeat(1_840_000) });
      const command = peers[0].request('remote', 'masterWrite', [{ text: 'x'.repeat(19000) }], 15000, 'business_commands');
      const [receipt] = await Promise.all([command, bulkDone]);
      return { receipt, accepted, received, elapsedMs: performance.now() - start, transportErrors,
        retries: peers.reduce((sum, peer) => sum + peer.transportStats.retryCount, 0) };
    } finally {
      globalThis.setTimeout = originalTimeout;
      peers.forEach((peer) => { peer.connections.clear(); peer.close(); });
      rtc.forEach((pc) => pc.close());
    }
  });
  assert.deepEqual(result.receipt, { accepted: true });
  assert.equal(result.accepted, 1);
  assert.equal(result.received, 2);
  assert.equal(result.retries, 0);
  assert.deepEqual(result.transportErrors, []);
  assert.deepEqual(errors, []);
  assert.ok(result.elapsedMs < 10000, `command and bulk completion took ${result.elapsedMs} ms`);
  console.log('hidden transfer real RTC browser smoke OK', result);
} finally {
  await browser?.close();
  await new Promise((done) => server.close(done));
}
