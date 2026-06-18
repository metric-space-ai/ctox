import test from 'node:test';
import assert from 'node:assert/strict';

import {
  chatAgentScopeViewFromMeta,
  renderChatAgentScopeHtml,
} from './business-chat.js';

const visibleScope = {
  rows: [
    { key: 'actor', label: 'Nutzer', value: 'Mira Team · user' },
    { key: 'app', label: 'App', value: 'Inventory · v1.0.0 · Team' },
    { key: 'data', label: 'Daten', value: 'Freigegeben: Inventory Items' },
    { key: 'external', label: 'Externe Aktionen', value: 'In diesem Schritt aus' },
  ],
  app: {
    module_id: 'inventory',
    module_title: 'Inventory',
    version: 'v1.0.0',
    visibility: 'team',
  },
};

test('business chat renders no agent scope panel without visible scope context', () => {
  assert.equal(chatAgentScopeViewFromMeta({}), null);
  assert.equal(renderChatAgentScopeHtml({}), '');
  assert.equal(renderChatAgentScopeHtml({ client_context: { module: 'inventory' } }), '');
});

test('business chat renders business-facing visible scope rows from client context', () => {
  const html = renderChatAgentScopeHtml({
    client_context: {
      visible_scope: {
        ...visibleScope,
        rows: [
          ...visibleScope.rows,
          { key: 'unsafe', label: '<script>x</script>', value: 'A & B' },
        ],
      },
    },
  });

  assert.match(html, /CTOX Zugriff/);
  assert.match(html, /Nutzer/);
  assert.match(html, /App/);
  assert.match(html, /Daten/);
  assert.match(html, /Externe Aktionen/);
  assert.match(html, /Inventory · v1\.0\.0 · Team/);
  assert.match(html, /&lt;script&gt;x&lt;\/script&gt;/);
  assert.match(html, /A &amp; B/);
});

test('business chat accepts normalized command scope visible scope fallback', () => {
  const view = chatAgentScopeViewFromMeta({
    client_context: {
      scope: {
        visible_scope: visibleScope,
      },
    },
  });

  assert.equal(view?.app.module_id, 'inventory');
  assert.deepEqual(view?.rows.map((row) => row.key), ['actor', 'app', 'data', 'external']);
});
