import { test } from 'node:test';
import assert from 'node:assert/strict';
import { unionDuration, serialRpcChain, correlate, analyzeCase } from './phase-analysis.mjs';
import { installPhaseTrace } from './phase-trace.cjs';

test('parallel request counts do not become serial RTTs or summed wall time', () => {
  const parallel = [{ startAt: 0, endAt: 100 }, { startAt: 1, endAt: 101 }, { startAt: 2, endAt: 102 }];
  assert.equal(serialRpcChain(parallel).length, 1);
  assert.equal(unionDuration(parallel), 102);
  assert.equal(unionDuration(parallel, 50, 75), 25);
  assert.equal(serialRpcChain([...parallel, { startAt: 103, endAt: 203 }]).length, 2);
});
test('outbound ACK barriers are correlated; inbound pipelined ACKs are not serial waits', () => {
  const frame = (at, direction, frameKind, values = {}) => ({ at, kind: 'frame', channel: 'c', direction,
    frameKind, transferId: 't', attempt: 0, ...values });
  const trace = { events: [frame(0, 'out', 'start', { totalFrames: 5, windowSize: 4 }),
    frame(2, 'out', 'chunk', { seq: 3 }), frame(302, 'in', 'ack', { ackSeq: 3 }),
    frame(303, 'out', 'chunk', { seq: 4 }), frame(603, 'in', 'ack', { ackSeq: 4, final: true })] };
  const out = correlate(trace);
  assert.equal(out.windows.length, 2);
  assert.equal(unionDuration(out.windows), 600);
  const incoming = correlate({ events: trace.events.map(event => ({ ...event, direction: event.direction === 'out' ? 'in' : 'out' })) });
  assert.equal(incoming.windows.length, 0);
  assert.equal(incoming.transfers[0].ackCount, 2);
});
test('request IDs, not adjacency, bind interleaved responses', () => {
  const logical = (at, direction, id, method = null) => ({ kind: 'logical', at, startedAt: at, direction, id, method });
  const result = correlate({ events: [logical(0, 'out', 'a', 'token'), logical(1, 'out', 'b', 'ctoxProtocol'),
    logical(3, 'in', 'b'), logical(4, 'in', 'a')] });
  assert.deepEqual(result.requests.map(request => [request.method, request.endAt]), [['token', 4], ['ctoxProtocol', 3]]);
});
test('phase conservation rejects missing boundaries rather than reporting a partial pass', () => {
  assert.throws(() => analyzeCase({ phaseTrace: { errors: [], marks: {}, events: [{ kind: 'channel-open', at: 1 }] } }), /Invalid phase/);
});
test('observer preserves send behavior and excludes credential/document payloads', async () => {
  const saved = Object.fromEntries(['RTCPeerConnection', 'RTCDataChannel', '__syncV3Trace', '__syncV3Rtc', '__syncV3BootAt'].map(key => [key, globalThis[key]]));
  class Channel extends EventTarget {
    label = 'ctox-rxdb'; sent = [];
    send(value) { if (value === 'throw') throw Error('native-send-failed'); this.sent.push(value); return 123; }
  }
  class Peer extends EventTarget { createDataChannel() { return new Channel(); } }
  try {
    globalThis.RTCPeerConnection = Peer; globalThis.RTCDataChannel = Channel;
    installPhaseTrace();
    const channel = new globalThis.RTCPeerConnection().createDataChannel();
    assert.equal(channel.send(JSON.stringify({ id: 'a', method: 'ctoxProtocol', params: [{ credential: 'must-not-survive', padding: 'private-doc' }] })), 123);
    assert.throws(() => channel.send('throw'), /native-send-failed/);
    await globalThis.__syncV3Trace.drain();
    const serialized = JSON.stringify(globalThis.__syncV3Trace);
    assert.ok(!serialized.includes('must-not-survive') && !serialized.includes('private-doc'));
    assert.equal(globalThis.__syncV3Trace.events.filter(event => event.kind === 'logical').length, 1);
    assert.equal(channel.sent.length, 1);
  } finally {
    for (const [key, value] of Object.entries(saved)) { if (value === undefined) delete globalThis[key]; else globalThis[key] = value; }
  }
});
