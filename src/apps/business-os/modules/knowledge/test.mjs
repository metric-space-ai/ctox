import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { fileURLToPath } from 'node:url';

import { build } from 'esbuild';

async function importBrowserBundle(relativePath) {
  const bundledModule = await build({
    entryPoints: [fileURLToPath(new URL(relativePath, import.meta.url))],
    bundle: true,
    format: 'esm',
    platform: 'browser',
    write: false,
  });

  const [{ text: bundledSource }] = bundledModule.outputFiles;
  return import(`data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`);
}

const { __knowledgeTestHooks: hooks } = await importBrowserBundle('./index.js');

const {
  buildKnowledgeBundles,
  canEditSelectedMarkdown,
  isKnowledgeActionFormReady,
  isKnowledgeTabDisabled,
  sourceScopeFor,
} = hooks;

const tests = [];
function test(name, fn) {
  tests.push({ name, fn });
}

test('groups unknown knowledge records instead of rendering a false empty state', () => {
  const groups = buildKnowledgeBundles([
    {
      id: 'note:ops-runner',
      kind: 'note',
      title: 'Ops Runner Notes',
      subtitle: 'User · Operations',
      summary: 'Operational knowledge that is not a skillbook.',
    },
  ], [], []);

  assert.equal(groups.length, 1);
  assert.equal(groups[0].id, 'knowledge/operations');
  assert.equal(groups[0].entries[0].id, 'note:ops-runner');
});

test('source filters classify user and system knowledge', () => {
  assert.equal(sourceScopeFor({ source_path: 'embedded:skills/system/drone.md' }), 'system');
  assert.equal(sourceScopeFor({ source_system: 'ctox_core' }), 'system');
  assert.equal(sourceScopeFor({ source_path: 'workspace/knowledge/customer.md' }), 'user');
});

test('runbooks and data tabs are disabled without a selected knowledge item', () => {
  assert.equal(isKnowledgeTabDisabled('skill', ''), false);
  assert.equal(isKnowledgeTabDisabled('runbooks', ''), true);
  assert.equal(isKnowledgeTabDisabled('data', ''), true);
  assert.equal(isKnowledgeTabDisabled('data', 'skill:drone'), false);
});

test('edit markdown requires an existing selected item', () => {
  const items = [{ id: 'skill:drone', title: 'Drone Skill' }];
  assert.equal(canEditSelectedMarkdown('', items), false);
  assert.equal(canEditSelectedMarkdown('missing', items), false);
  assert.equal(canEditSelectedMarkdown('skill:drone', items), true);
});

test('action dialogs require non-empty required fields before submit', () => {
  assert.equal(isKnowledgeActionFormReady({ title: '' }, ['title']), false);
  assert.equal(isKnowledgeActionFormReady({ title: '  ' }, ['title']), false);
  assert.equal(isKnowledgeActionFormReady({ title: 'Customer Knowledge' }, ['title']), true);
  assert.equal(isKnowledgeActionFormReady({ destination: '' }, ['destination']), false);
  assert.equal(isKnowledgeActionFormReady({ destination: 'runtime/knowledge/exports/' }, ['destination']), true);
});

let passed = 0;
for (const entry of tests) {
  try {
    await entry.fn();
    passed += 1;
    console.log(`ok - ${entry.name}`);
  } catch (error) {
    console.error(`not ok - ${entry.name}`);
    throw error;
  }
}

console.log(`${passed} knowledge tests passed`);
