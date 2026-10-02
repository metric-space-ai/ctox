#!/usr/bin/env node
// SPDX-License-Identifier: MIT OR AGPL-3.0-only

import assert from 'node:assert/strict';
import test from 'node:test';

import {
  EXCLUSION_RULES,
  FORBIDDEN_TERMS,
  INTERNAL_COPY_CONTEXTS,
  TECHNICAL_DIAGNOSTIC_CONTEXTS,
  USER_TERMS,
  auditBusinessOsContent,
  auditSourceText,
  exclusionReason,
} from './audit-business-os-content.mjs';

test('the current approved Business OS product scope passes the content guard', async () => {
  const result = await auditBusinessOsContent();

  assert.deepEqual(result.findings, []);
  assert.ok(result.filesAudited > 0);
  assert.ok(result.approvedModules.length > 0);
  assert.ok(result.excludedModules.some(({ id }) => id === 'appsec-pentest'));
  assert.ok(result.excludedModules.some(({ id }) => id === 'ctox'));
});

test('visible text and accessibility attributes are audited', () => {
  const source = '<section aria-label="WebRTC"><img alt="Native"><input placeholder="Room"><button title="Binary">Guest</button></section>';
  const findings = auditSourceText(source, 'src/apps/business-os/modules/example/index.js');

  assert.deepEqual(
    findings.map(({ term }) => term),
    ['WebRTC', 'Native', 'Room', 'Binary', 'Guest'],
  );
  assert.deepEqual(
    findings.map(({ kind }) => kind),
    ['user-facing-literal', 'user-facing-literal', 'user-facing-literal', 'user-facing-literal', 'rendered-text'],
  );
});

test('HTML asset URLs do not inherit a preceding accessibility label', () => {
  const source = `<aside aria-label="Native"></aside>
<script type="module" src="shared/shell-release-status.js?v=crew-native-sync-dock-v424"></script>`;
  const findings = auditSourceText(source, 'src/apps/business-os/index.html');

  assert.deepEqual(findings.map(({ term, kind }) => [term, kind]), [['Native', 'user-facing-literal']]);
});

test('implementation tokens and technical predicates are not product-copy findings', () => {
  assert.deepEqual(
    auditSourceText("const protocol='WebRTC'; const id='ctox-rxdb-js'; const ready = message.includes('RxDB Error-Code: DB6');", 'src/apps/business-os/modules/example/index.js'),
    [],
  );
  assert.deepEqual(
    auditSourceText('<h2>WebRTC Sync</h2>', 'src/apps/business-os/app.js'),
    [],
  );
  assert.ok(auditSourceText('<h2>WebRTC Sync</h2>', 'src/apps/business-os/modules/notes/index.js').length > 0);
  assert.deepEqual(
    auditSourceText("setTimeout(() => reject(new Error('replication did not become ready')), 1);", 'src/apps/business-os/desktop-apps/example/app.js').map(({ term }) => term),
    ['replication'],
  );
});

test('manifest metadata is audited while HTML implementation attributes are not', () => {
  const metadataFindings = auditSourceText('{"title":"Native App","description":"A local app"}', 'src/apps/business-os/modules/example/module.json', { metadata: true });
  assert.deepEqual(metadataFindings.map(({ term }) => term), ['Native']);

  const htmlFindings = auditSourceText('<input value="native" data-mode="WebRTC" aria-label="App name">', 'src/apps/business-os/modules/example/index.js');
  assert.deepEqual(htmlFindings, []);
});

test('quoted metadata keys inside JavaScript fallback catalogs are audited', () => {
  const source = `const fallback = { "description": "Native app", "store": { "summary": "WebRTC replication" } };`;
  assert.deepEqual(
    auditSourceText(source, 'src/apps/business-os/app.js').map(({ term }) => term),
    ['Native', 'WebRTC', 'replication'],
  );
});

test('every exclusion is explicit and diagnostic exceptions stay path-scoped', () => {
  for (const rule of EXCLUSION_RULES) assert.ok(rule.reason, `missing reason for ${rule.prefix}`);
  assert.match(exclusionReason('src/apps/business-os/vendor/library.js'), /third-party/i);
  assert.match(exclusionReason('src/apps/business-os/rxdb/src/index.mjs'), /data-plane/i);
  assert.match(exclusionReason('src/apps/business-os/installed-modules/customer/index.js'), /customer/i);
  assert.equal(exclusionReason('src/apps/business-os/modules/notes/index.js'), null);
  for (const exception of [...TECHNICAL_DIAGNOSTIC_CONTEXTS, ...INTERNAL_COPY_CONTEXTS]) {
    assert.ok(exception.path);
    assert.ok(exception.context instanceof RegExp);
    assert.ok(exception.reason);
  }
});

test('the guard is sourced from the Workjet vocabulary contract', () => {
  assert.ok(FORBIDDEN_TERMS.includes('RxDB'));
  assert.ok(FORBIDDEN_TERMS.includes('WebRTC'));
  assert.ok(USER_TERMS.includes('Workjet'));
});
