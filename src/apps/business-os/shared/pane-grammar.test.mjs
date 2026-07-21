import test from 'node:test';
import assert from 'node:assert/strict';
import { wirePaneGrammar, recordPaneScroll, clearPaneScroll, restorePaneScroll } from './pane-grammar.js';

// Minimal DOM stand-ins: enough surface for the helper's queries + events.
function el(attrs = {}, tag = 'DIV') {
  const listeners = {};
  return {
    tagName: tag,
    dataset: attrs.dataset || {},
    value: attrs.value ?? '',
    hidden: attrs.hidden ?? false,
    textContent: '',
    attrs: { ...(attrs.attrs || {}) },
    classes: new Set(),
    classList: {
      toggle(name, on) { if (on) this._s.add(name); else this._s.delete(name); },
      contains(name) { return this._s.has(name); },
      _s: new Set(),
    },
    setAttribute(name, value) { this.attrs[name] = value; },
    getAttribute(name) { return this.attrs[name] ?? null; },
    addEventListener(type, fn) { (listeners[type] ||= []).push(fn); },
    fire(type) { (listeners[type] || []).forEach((fn) => fn()); },
  };
}

function paneWith(nodes) {
  return {
    querySelector(sel) {
      for (const [key, node] of Object.entries(nodes)) {
        if (sel.includes(key)) return Array.isArray(node) ? node[0] : node;
      }
      return null;
    },
    querySelectorAll(sel) {
      for (const [key, node] of Object.entries(nodes)) {
        if (sel.includes(key)) return Array.isArray(node) ? node : [node];
      }
      return [];
    },
  };
}

test('pane grammar wires search, tray, reset, dot, band and counts', () => {
  const search = el({ value: '' }, 'INPUT');
  const trayToggle = el();
  const tray = el({ hidden: true });
  const reset = el();
  const statusFilter = el({ value: 'all', dataset: { pgName: 'status' } }, 'SELECT');
  const tabAll = el({ dataset: { pgBand: 'all' }, attrs: { 'aria-selected': 'true' } });
  const tabBug = el({ dataset: { pgBand: 'bug' }, attrs: { 'aria-selected': 'false' } });
  const countAll = el();
  const footer = el();
  const cards = el({ dataset: { pgView: 'cards' }, attrs: { 'aria-pressed': 'true' } });
  const list = el({ dataset: { pgView: 'list' }, attrs: { 'aria-pressed': 'false' } });

  const events = [];
  const pane = paneWith({
    'data-pg-search': search,
    'data-pg-view': [cards, list],
    'data-pg-tray-toggle': trayToggle,
    'data-pg-tray]': tray,
    'data-pg-reset': reset,
    'data-pg-filter': [statusFilter],
    'data-pg-band=': [tabAll, tabBug],
    'data-pg-band]': [tabAll, tabBug],
    'data-pg-count="all"': countAll,
    'data-pg-footer': footer,
  });
  const grammar = wirePaneGrammar(pane, { onChange: (s) => events.push(s) });

  // tray toggles + aria
  trayToggle.fire('click');
  assert.equal(tray.hidden, false);
  assert.equal(trayToggle.getAttribute('aria-expanded'), 'true');

  // filter change → dot on + onChange carries filters
  statusFilter.value = 'open';
  statusFilter.fire('change');
  assert.equal(trayToggle.classList._s.has('has-active-filters'), true);
  assert.equal(events.at(-1).filters.status, 'open');

  // reset clears search + filters, dot off
  search.value = 'foo';
  reset.fire('click');
  assert.equal(search.value, '');
  assert.equal(statusFilter.value, 'all');
  assert.equal(trayToggle.classList._s.has('has-active-filters'), false);

  // view + band switching reflected in state
  list.fire('click');
  assert.equal(events.at(-1).view, 'list');
  tabBug.fire('click');
  assert.equal(events.at(-1).band, 'bug');

  // counts + footer render (zeros included)
  grammar.setCounts({ all: 0 });
  assert.equal(countAll.textContent, ' (0)');
  grammar.setFooter('12 Einträge');
  assert.equal(footer.textContent, '12 Einträge');
});

test('scroll guard restores a clamped rebuild but honors intentional resets', () => {
  const pane = {};
  const well = { isConnected: true, scrollTop: 800, scrollHeight: 1400, clientHeight: 360 };

  // Rebuild clamps scrollTop to 0 → the recorded offset is restored.
  recordPaneScroll(pane, well);
  well.scrollTop = 0;
  restorePaneScroll(pane);
  assert.equal(well.scrollTop, 800);

  // A recorded 0 (operator scrolled to top) is never "restored" anywhere.
  well.scrollTop = 0;
  recordPaneScroll(pane, well);
  well.scrollTop = 0;
  restorePaneScroll(pane);
  assert.equal(well.scrollTop, 0);

  // Grammar change (search/view/band/filter) clears offsets → reset stays.
  well.scrollTop = 500;
  recordPaneScroll(pane, well);
  clearPaneScroll(pane);
  well.scrollTop = 0;
  restorePaneScroll(pane);
  assert.equal(well.scrollTop, 0);

  // A non-zero current position (module scrolled somewhere on purpose,
  // e.g. transcript pinned to bottom) is left alone.
  well.scrollTop = 700;
  recordPaneScroll(pane, well);
  well.scrollTop = 340;
  restorePaneScroll(pane);
  assert.equal(well.scrollTop, 340);

  // Disconnected containers are dropped, not touched.
  const gone = { isConnected: false, scrollTop: 0, scrollHeight: 900, clientHeight: 300 };
  recordPaneScroll(pane, gone);
  restorePaneScroll(pane);
  assert.equal(gone.scrollTop, 0);
});
