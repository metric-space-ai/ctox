import assert from 'node:assert/strict';
import { test } from 'node:test';
import { revealDetailIfStacked } from '../../customer-modules/outbound-lead-generation/stacked-detail-reveal.mjs';

const pane = (top, bottom) => {
  const calls = [];
  return { calls, getBoundingClientRect: () => ({ top, bottom }), scrollIntoView: (opts) => calls.push(opts) };
};

test('stacked layout scrolls the detail pane into view', () => {
  const leads = pane(196, 636);
  const detail = pane(644, 944);
  assert.equal(revealDetailIfStacked({ leads, detail }), true);
  assert.deepEqual(detail.calls, [{ block: 'start', behavior: 'smooth' }]);
});

test('side-by-side layout does not scroll', () => {
  const leads = pane(60, 800);
  const detail = pane(60, 800);
  assert.equal(revealDetailIfStacked({ leads, detail }), false);
  assert.deepEqual(detail.calls, []);
});

test('missing panes are ignored', () => {
  assert.equal(revealDetailIfStacked({}), false);
  assert.equal(revealDetailIfStacked(), false);
});
