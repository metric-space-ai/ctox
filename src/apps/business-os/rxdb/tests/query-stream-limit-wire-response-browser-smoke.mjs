// Native RPC errors are strings on the wire. Exercise the real peer response
// routing and demand retry in Chromium over local RTCDataChannels; no tenant.
import assert from 'node:assert/strict';
import http from 'node:http';
import { readFileSync } from 'node:fs';
import { dirname, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const sourceRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../src');
const playwrightPath = process.env.PLAYWRIGHT_MODULE_PATH
  ? resolve(process.env.PLAYWRIGHT_MODULE_PATH, 'index.mjs')
  : resolve(sourceRoot, '../../node_modules/playwright/index.mjs');
const { chromium } = await import(pathToFileURL(playwrightPath).href);
const server = http.createServer((request, response) => {
  if (request.url === '/') {
    response.setHeader('content-type', 'text/html');
    response.end('<!doctype html><title>Query RPC retry regression</title>');
    return;
  }
  const path = resolve(sourceRoot, '.' + new URL(request.url, 'http://localhost').pathname);
  if (!path.startsWith(sourceRoot + sep)) { response.writeHead(404).end(); return; }
  try {
    response.setHeader('content-type', 'text/javascript');
    response.end(readFileSync(path));
  } catch { response.writeHead(404).end(); }
});
await new Promise(ready => server.listen(0, '127.0.0.1', ready));
let browser;
const warnings = [], errors = [];
try {
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  page.on('console', message => { if (message.type() === 'warning') warnings.push(message.text()); });
  page.on('pageerror', error => errors.push(error.message));
  await page.goto('http://127.0.0.1:' + server.address().port);
  const result = await page.evaluate(async () => {
    const { createCtoxWebRtcNativePeer } = await import('/webrtc-native.mjs');
    const { createDemandLoadingTransport } = await import('/demand-loading-transport.mjs');
    const { CTOX_QUERY_RPC } = await import('/protocol-contract.generated.mjs');
    const transport = createDemandLoadingTransport({ getPeerId: () => 'native' });
    const client = createCtoxWebRtcNativePeer({ clientId: 'browser', room: 'query-rpc-regression',
      signalingUrl: 'ws://unused.invalid', requestHandlers: transport.requestHandlers });
    const native = createCtoxWebRtcNativePeer({ clientId: 'native', room: 'query-rpc-regression',
      signalingUrl: 'ws://unused.invalid' });
    const rtc = [new RTCPeerConnection(), new RTCPeerConnection()];
    const channels = [rtc[0].createDataChannel('ctox-rxdb')];
    let attempts = 0, rateAttempts = 0;
    const requests = [];
    const limit = 'STREAM_LIMIT_EXCEEDED: max in-flight query streams reached';
    const denied = 'UNAUTHORIZED: peer is not authorized for collection';
    try {
      const remoteChannel = new Promise(ready => { rtc[1].ondatachannel = ({ channel }) => ready(channel); });
      const gather = pc => pc.iceGatheringState === 'complete' ? Promise.resolve() : new Promise(ready => {
        pc.addEventListener('icegatheringstatechange', function listener() {
          if (pc.iceGatheringState === 'complete') { pc.removeEventListener('icegatheringstatechange', listener); ready(); }
        });
      });
      await rtc[0].setLocalDescription(await rtc[0].createOffer()); await gather(rtc[0]);
      await rtc[1].setRemoteDescription(rtc[0].localDescription);
      await rtc[1].setLocalDescription(await rtc[1].createAnswer()); await gather(rtc[1]);
      await rtc[0].setRemoteDescription(rtc[1].localDescription);
      channels.push(await remoteChannel);
      await Promise.all(channels.map(channel => channel.readyState === 'open' ? Promise.resolve()
        : new Promise(ready => channel.addEventListener('open', ready, { once: true }))));
      [client, native].forEach((peer, index) => {
        const connection = { remotePeerId: index === 0 ? 'native' : 'browser', channel: channels[index] };
        peer.connections.set(connection.remotePeerId, connection);
        channels[index].onmessage = ({ data }) =>
          peer.enqueueInboundDataChannelFrame(connection, channels[index], JSON.parse(data));
      });
      transport.attach(client);
      // Rust's WebRTCResponse.error is Option<String>. The JS fixture peer
      // serializes Error objects, so reproduce Rust's scalar at the wire boundary.
      const send = native.send.bind(native);
      native.send = (peerId, frame) => send(peerId, frame.error && typeof frame.error === 'object'
        ? { ...frame, error: frame.error.message } : frame);
      native.handleRequest = async (_peerId, method, [envelope]) => {
        if (method !== CTOX_QUERY_RPC.fetch) return { ack: true };
        requests.push(envelope.requestId);
        if (envelope.collectionName === 'denied') throw new Error(denied);
        const rateLimited = envelope.collectionName === 'rate_limited';
        const attempt = rateLimited ? ++rateAttempts : ++attempts;
        if (attempt === 1) throw new Error(rateLimited ? 'RATE_LIMITED: native query rate limit' : limit);
        setTimeout(() => native.send('browser', { id: 'chunk-' + envelope.requestId,
          method: CTOX_QUERY_RPC.chunk, params: [{ requestId: envelope.requestId,
            sequence: 0, documents: [{ id: 'approval', status: 'approved' }], complete: true }] }), 0);
        return { ack: true };
      };
      const envelope = { requestId: 'approval-read', collectionName: 'ctox_task_approval_requests',
        queryFingerprint: 'approval', query: { selector: { id: 'approval' } }, window: { offset: 0, limit: 1 } };
      let value, failure;
      try { value = await transport.requestQueryFetch(envelope, { authorityKey: 'fixture-owner' }); }
      catch (error) { failure = String(error?.message || error); console.warn('[threads] action failed', error); }
      let rateValue, rateFailure;
      try { rateValue = await transport.requestQueryFetch({ ...envelope, requestId: 'rate-read', collectionName: 'rate_limited' }); }
      catch (error) { rateFailure = String(error?.message || error); console.warn('[threads] action failed', error); }
      const deniedBefore = requests.length;
      let deniedError;
      try { await transport.requestQueryFetch({ ...envelope, requestId: 'denied-read', collectionName: 'denied' }); }
      catch (error) { deniedError = String(error?.message || error); }
      return { attempts, rateAttempts, requests, value, failure, rateValue, rateFailure, deniedError, deniedAttempts: requests.length - deniedBefore,
        channels: channels.map(channel => channel.readyState) };
    } finally {
      [client, native].forEach(peer => { peer.connections.clear(); peer.close(); });
      rtc.forEach(pc => pc.close());
    }
  });
  console.log('query RPC retry evidence', JSON.stringify({ result, warnings, errors }));
  assert.deepEqual(warnings, [], 'transient native stream pressure must recover without a Threads action warning');
  assert.deepEqual(errors, []);
  assert.equal(result.failure, undefined);
  assert.equal(result.attempts, 2);
  assert.equal(result.rateAttempts, 2);
  assert.equal(result.rateFailure, undefined);
  assert.deepEqual(result.rateValue.documents, [{ id: 'approval', status: 'approved' }]);
  assert.deepEqual(result.requests.slice(0, 2), ['approval-read', 'approval-read|retry-1']);
  assert.deepEqual(result.value.documents, [{ id: 'approval', status: 'approved' }]);
  assert.equal(result.deniedError, 'UNAUTHORIZED: peer is not authorized for collection');
  assert.equal(result.deniedAttempts, 1, 'permission denials must remain terminal');
  assert.deepEqual(result.channels, ['open', 'open']);
  console.log('query stream-limit wire response browser smoke OK', JSON.stringify(result));
} finally {
  await browser?.close();
  await new Promise(done => server.close(done));
}
