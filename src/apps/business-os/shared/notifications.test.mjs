import assert from 'node:assert/strict';
import test from 'node:test';

import { createNotifications, normalizeSystemNotification, toPlainText } from './notifications.js';

test('normalizes a bounded Decision Hub system notification', () => {
  assert.deepEqual(normalizeSystemNotification({
    kind: 'decision_hub',
    title: '  Architektur   freigeben ',
    message: 'Decision Hub wartet auf deine Entscheidung.',
    tag: 'decision-hub:kpl-e-1',
    recordId: 'kpl-e-1',
    urgency: 'critical',
  }), {
    kind: 'decision_hub',
    title: 'Architektur freigeben',
    body: 'Decision Hub wartet auf deine Entscheidung.',
    tag: 'decision-hub:kpl-e-1',
    urgency: 'critical',
    recordId: 'kpl-e-1',
  });
});

test('rejects empty content and strips unsafe routing tokens', () => {
  assert.equal(normalizeSystemNotification({ title: '', message: '' }), null);
  assert.deepEqual(normalizeSystemNotification({
    title: 'Entscheidung',
    message: 'Bitte prüfen.',
    tag: 'unsafe token',
    recordId: '../../secret',
  }), {
    kind: 'business_os',
    title: 'Entscheidung',
    body: 'Bitte prüfen.',
    urgency: 'normal',
  });
});

test('delivers only the normalized payload through the Workjet mobile bridge', () => {
  let delivered = null;
  globalThis.workjetBusinessOsNotify = (payload) => {
    delivered = payload;
    return true;
  };
  try {
    const notifications = createNotifications({ container: {} });
    assert.equal(notifications.showSystem({
      kind: 'decision_hub',
      title: 'Freigabe',
      message: 'Bitte entscheiden.',
      recordId: 'kpl-e-1',
      context: 'must not cross the bridge',
      action: { callback() { throw new Error('must stay local'); } },
    }), true);
    assert.deepEqual(delivered, {
      kind: 'decision_hub',
      title: 'Freigabe',
      body: 'Bitte entscheiden.',
      urgency: 'normal',
      recordId: 'kpl-e-1',
    });
  } finally {
    delete globalThis.workjetBusinessOsNotify;
  }
});

function fakeDocument() {
  const makeElement = () => {
    const classes = new Set();
    return {
      children: [],
      dataset: {},
      id: '',
      className: '',
      textContent: '',
      classList: {
        add: (name) => classes.add(name),
        contains: (name) => classes.has(name),
      },
      setAttribute() {},
      addEventListener() {},
      appendChild(child) { this.children.push(child); },
      append(child) { this.children.push(child); },
    };
  };
  return { createElement: makeElement };
}

function fakeContainer() {
  const children = [];
  return {
    children,
    get childElementCount() { return children.length; },
    get firstElementChild() { return children[0] ?? null; },
    appendChild(child) { children.push(child); },
    querySelector: () => null,
  };
}

test('toPlainText turns editor HTML fragments into readable text', () => {
  assert.equal(
    toPlainText('Fonts are not loaded.<br>Please contact your Document Server administrator.'),
    'Fonts are not loaded. Please contact your Document Server administrator.',
  );
  assert.equal(toPlainText('Tom &amp; Jerry<br/><b>fett</b>'), 'Tom & Jerry fett');
});

test('shows an untranslated title key as the fallback title', () => {
  const previousDocument = globalThis.document;
  globalThis.document = fakeDocument();
  try {
    const container = fakeContainer();
    const notifications = createNotifications({ container, t: (key, fallback) => key });
    notifications.show({ type: 'error', message: 'Gespeichert.', time: 0 });
    assert.equal(container.children[0].children[1].children[0].textContent, 'Benachrichtigung');
  } finally {
    globalThis.document = previousDocument;
  }
});

test('repeated identical messages stack as one toast while it is visible', () => {
  const previousDocument = globalThis.document;
  globalThis.document = fakeDocument();
  try {
    const container = fakeContainer();
    const notifications = createNotifications({ container, t: (key, fallback) => fallback ?? key });
    const first = notifications.show({ type: 'error', message: 'Fonts are not loaded.<br>Contact admin.', time: 0 });
    const second = notifications.show({ type: 'error', message: 'Fonts are not loaded.<br>Contact admin.', time: 0 });
    assert.equal(second, first);
    assert.equal(container.children.length, 1);
    assert.equal(container.children[0].children[1].children[1].textContent, 'Fonts are not loaded. Contact admin.');
  } finally {
    globalThis.document = previousDocument;
  }
});
