import assert from 'node:assert/strict';
import { browserFrameBinding, browserInputBinding, browserInputAcknowledgement } from './browser-surface-binding.js';

const current = { sessionId: 'DNB', leaseId: 'lease', epoch: 3, activeTabId: 'runner-dnb' };
const request = { ...current };
const response = { ok: true, binding: { session_id: 'DNB', tab_id: 'durable-tab',
  runtime_generation: 'generation-1', active_tab_id: 'runner-dnb' },
  screenshot: { base64: 'fixture-image' },
  nav: { active_tab_id: 'runner-dnb', url: 'https://example.invalid/dnb' } };
const frame = browserFrameBinding(response, request, current);
assert.equal(frame.tab_id, 'durable-tab');
assert.equal(frame.active_tab_id, 'runner-dnb');
assert.equal(Object.isFrozen(frame), true);
assert.deepEqual(browserInputBinding(frame, current), {
  runtime_generation: 'generation-1', active_tab_id: 'runner-dnb',
});
assert.equal(browserFrameBinding({}, request, current), null, 'persisted active status is not a frame binding');
assert.equal(browserFrameBinding({ ...response, ok: false }, request, current), null);
for (const ok of [undefined, null, 1, 'true']) {
  assert.equal(browserFrameBinding({ ...response, ok }, request, current), null,
    'a matching image/binding without explicit native success is not a confirmed frame');
}
assert.equal(browserFrameBinding({ ...response, screenshot: null }, request, current), null, 'navigation alone cannot confirm a displayed frame');
assert.equal(browserFrameBinding(response, request, { ...current, runtimeGeneration: 'generation-2' }), null);
assert.equal(browserFrameBinding(response, request, { ...current, tabId: 'wrong-durable-tab' }), null);
assert.equal(browserFrameBinding(response, request, { ...current, sessionId: 'XING' }), null);
assert.equal(browserFrameBinding(response, request, { ...current, leaseId: 'new-lease' }), null);
assert.equal(browserFrameBinding(response, request, { ...current, epoch: 4 }), null);
assert.equal(browserFrameBinding(response, request, { ...current, activeTabId: 'runner-xing' }), null);
assert.equal(browserFrameBinding({ ...response, nav: { active_tab_id: 'runner-xing' } }, request, current), null);
assert.equal(browserFrameBinding({ ...response, binding: { ...response.binding, session_id: 'XING' } }, request, current), null);
assert.equal(browserFrameBinding({ ...response, binding: { ...response.binding, runtime_generation: '' } }, request, current), null);
assert.equal(browserInputBinding(null, current), null, 'switch/reconnect invalidates input until a new confirmed frame');
assert.equal(browserInputBinding(frame, { ...current, epoch: 4 }), null);
assert.equal(browserInputBinding(frame, { ...current, leaseId: 'new-lease' }), null);
assert.equal(browserInputBinding(frame, { ...current, runtimeGeneration: 'generation-2' }), null);
const events = [1, 2].map(seq => ({ seq, session_id: 'DNB', tab_id: 'durable-tab' }));
const ack = { ...response, results: [{ index: 0, ok: true }, { index: 1, ok: true }] };
assert.deepEqual(browserInputAcknowledgement(ack, events, frame, request, current), { acceptedSeqs: [1, 2], complete: true });
assert.deepEqual(browserInputAcknowledgement({ ...ack, results: [{ index: 0, ok: true }, { index: 1, ok: false }] }, events, frame, request, current),
  { acceptedSeqs: [1], complete: false });
assert.deepEqual(browserInputAcknowledgement({ ...ack, results: [{ index: 0, ok: true }] }, events, frame, request, current),
  { acceptedSeqs: [1], complete: false });
for (const stale of [
  { ...ack, ok: false },
  { ...ack, ok: undefined },
  { ...ack, ok: 'true' },
  { ...ack, results: [{ ok: true }, { ok: true }] },
  { ...ack, results: [{ index: 0, ok: true }, { index: 0, ok: true }] },
  { ...ack, results: [{ index: 0.5, ok: true }] },
  { ...ack, results: [{ index: 99, ok: true }] },
  { ...ack, binding: { ...response.binding, runtime_generation: 'replaced-runner' } },
  { ...ack, binding: { ...response.binding, active_tab_id: 'runner-xing' } },
  { ...ack, binding: { ...response.binding, tab_id: 'wrong-durable-tab' } },
  { ...response, applied: 2 },
]) assert.deepEqual(browserInputAcknowledgement(stale, events, frame, request, current), { acceptedSeqs: [], complete: false });
assert.deepEqual(browserInputAcknowledgement({ ...ack,
  results: [{ index: 1, ok: true }, { index: 0, ok: true }] }, events, frame, request, current),
  { acceptedSeqs: [1, 2], complete: true }, 'native result indices, not response order, bind acknowledgements');
assert.deepEqual(browserInputAcknowledgement({ ...ack, results: [{ index: 1, ok: true }] }, events, frame, request, current),
  { acceptedSeqs: [2], complete: false }, 'an omitted earlier result must not acknowledge a different event');
assert.deepEqual(browserInputAcknowledgement(ack, events, frame, request, { ...current, epoch: 4 }), { acceptedSeqs: [], complete: false });
assert.deepEqual(browserInputAcknowledgement(ack, [{ ...events[0], tab_id: 'runner-dnb' }], frame, request, current),
  { acceptedSeqs: [], complete: false }, 'Runner IDs cannot substitute for durable event tab IDs');
assert.deepEqual(browserInputAcknowledgement(ack, Array.from({ length: 65 }, () => events[0]), frame, request, current),
  { acceptedSeqs: [], complete: false });
console.log('Browser native surface/epoch/lease/frame/input binding guards PASS (not installed UI proof)');
