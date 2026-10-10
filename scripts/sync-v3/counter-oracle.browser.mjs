#!/usr/bin/env node
// Bounded localhost-only independent byte oracle. No tenant or data-store access.
import { readFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { once } from 'node:events';

const dependency = process.argv[2];
if (!dependency || process.argv.length !== 3) throw new Error('Supply the existing pinned Playwright module path');
const { chromium } = await import(pathToFileURL(dependency.endsWith('.mjs') ? dependency : `${dependency}/index.mjs`));
const moduleSource = await readFile(new URL('./transport-counters.mjs', import.meta.url), 'utf8');
const moduleUrl = `data:text/javascript;base64,${Buffer.from(moduleSource).toString('base64')}`;
let browser;
let deadline;
const server = createServer((_request, response) => {
  response.setHeader('Content-Type', 'text/html');
  response.end('<!doctype html><title>Sync v3 byte oracle</title>');
});
try {
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  browser = await chromium.launch({ headless: true, args: ['--disable-gpu'] });
  deadline = setTimeout(() => browser.close().catch(() => {}), 30000);
  const page = await browser.newPage();
  await page.goto(`http://127.0.0.1:${server.address().port}`);
  const result = await page.evaluate(async (moduleUrl) => {
    const { DataChannelByteMeter, counterDelta } = await import(moduleUrl);
    const sender = new RTCPeerConnection({ iceServers: [] });
    const receiver = new RTCPeerConnection({ iceServers: [] });
    try {
      const receivedChannels = [];
      const received = [];
      receiver.ondatachannel = ({ channel }) => {
        receivedChannels.push(channel);
        channel.binaryType = 'arraybuffer';
        channel.onmessage = ({ data }) => received.push(typeof data === 'string'
          ? new TextEncoder().encode(data).byteLength : data.byteLength);
      };
      sender.onicecandidate = ({ candidate }) => { if (candidate) receiver.addIceCandidate(candidate).catch(() => {}); };
      receiver.onicecandidate = ({ candidate }) => { if (candidate) sender.addIceCandidate(candidate).catch(() => {}); };
      const channels = [sender.createDataChannel('control'), sender.createDataChannel('legacy')];
      const waitFor = async (predicate) => {
        const until = performance.now() + 5000;
        while (!predicate()) {
          if (performance.now() >= until) throw new Error('Local RTC oracle timed out');
          await new Promise((resolve) => setTimeout(resolve, 20));
        }
      };
      await sender.setLocalDescription(await sender.createOffer());
      await receiver.setRemoteDescription(sender.localDescription);
      await receiver.setLocalDescription(await receiver.createAnswer());
      await sender.setRemoteDescription(receiver.localDescription);
      await waitFor(() => channels.every((channel) => channel.readyState === 'open') && receivedChannels.length === 2);
      const meter = new DataChannelByteMeter();
      const aliases = Array(20).fill(receiver); // Same connection exposed by twenty collections.
      const before = await meter.snapshot(aliases);
      const text = 'ä📦'.repeat(4096);
      const binary = new Uint8Array(4096).map((_, index) => index % 256);
      const expectedBytes = channels.length * (new TextEncoder().encode(text).byteLength + binary.byteLength);
      for (const channel of channels) {
        channel.send(text);
        channel.send(binary);
      }
      await waitFor(() => received.length === 4);
      let delta;

      const until = performance.now() + 5000;
      do {
        delta = counterDelta(before, await meter.snapshot(aliases));
        if (delta.totals.bytesReceived === expectedBytes) break;
        if (performance.now() >= until) throw new Error('RTC stats did not match independent oracle');
        await new Promise((resolve) => setTimeout(resolve, 20));
      } while (true);
      return { expectedBytes, deliveredBytes: received.reduce((sum, value) => sum + value, 0),
        connections: before.connections, delta,
        aggregationCounterexample: {
          aliases: aliases.length,
          naivePerCollectionBytes: aliases.reduce((sum) => sum + delta.totals.bytesReceived, 0),
          uniqueConnectionBytes: delta.totals.bytesReceived,
          scope: 'same RTC interval; alias aggregation reproduction, not deployed-engine before/after',
        },
        scope: 'in-browser-loopback', installedAcceptance: false };
    } finally {
      sender.close();
      receiver.close();
    }
  }, moduleUrl);
  assert.equal(result.connections, 1);
  assert.equal(result.delta.channels.length, 2);
  assert.equal(result.deliveredBytes, result.expectedBytes);
  assert.equal(result.delta.totals.bytesReceived, result.expectedBytes);
  assert.equal(result.delta.totals.messagesReceived, 4);
  assert.equal(result.delta.totals.bytesSent, 0);
  assert.equal(result.aggregationCounterexample.naivePerCollectionBytes, 1146880);
  assert.equal(result.aggregationCounterexample.uniqueConnectionBytes, 57344);
  console.log(JSON.stringify({ counterOraclePassed: true, ...result }));
} finally {
  clearTimeout(deadline);
  await browser?.close();
  await new Promise((resolve) => server.close(resolve));
}
