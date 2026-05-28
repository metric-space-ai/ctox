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

const { filterReportItems, normalizeReportItems } = await importBrowserBundle('./index.js');

const t = (_key, fallback) => fallback;

const tests = [];
function test(name, fn) {
  tests.push({ name, fn });
}

test('renders reports that exist only in ctox_bug_reports', () => {
  const items = normalizeReportItems({
    bugs: [{
      id: 'bug-1',
      title: 'Filter bar clipped',
      status: 'open',
      module: 'reports',
      severity: 'high',
      description: 'Controls overlap in the left pane.',
      payload: {
        kind: 'bug',
        expected: 'Toolbar remains usable.',
        ctox_command_id: 'cmd-1',
        task_id: 'task-1',
      },
      updated_at_ms: 10,
    }],
    commands: [{ id: 'cmd-1', command_id: 'cmd-1', status: 'completed' }],
    queue: [{ id: 'task-1', status: 'running' }],
    t,
  });

  assert.equal(items.length, 1);
  assert.equal(items[0].id, 'bug-1');
  assert.equal(items[0].moduleId, 'reports');
  assert.equal(items[0].summary, 'Controls overlap in the left pane.');
  assert.equal(items[0].status, 'running');
});

test('merges business module reports with ctox bug payloads', () => {
  const items = normalizeReportItems({
    reports: [{
      id: 'report-1',
      report_id: 'shared-1',
      module_id: 'reports',
      kind: 'feature',
      title: 'Add diagnostics',
      status: 'open',
      updated_at_ms: 20,
    }],
    bugs: [{
      id: 'shared-1',
      severity: 'medium',
      description: 'Show sync failures.',
      payload: { expected: 'Visible diagnostic' },
      updated_at_ms: 10,
    }],
    t,
  });

  assert.equal(items.length, 1);
  assert.equal(items[0].id, 'shared-1');
  assert.equal(items[0].kind, 'feature');
  assert.equal(items[0].severity, 'medium');
  assert.equal(items[0].summary, 'Show sync failures.');
  assert.equal(items[0].expected, 'Visible diagnostic');
});

test('filters by type, normalized status, and searchable fields', () => {
  const items = normalizeReportItems({
    bugs: [
      { id: 'bug-1', title: 'Refresh fails', status: 'failed', module: 'reports', updated_at_ms: 30 },
      { id: 'feature-1', title: 'Better panes', status: 'completed', module: 'reports', payload: { kind: 'feature' }, updated_at_ms: 20 },
    ],
    t,
  });

  assert.deepEqual(filterReportItems(items, { kind: 'bug' }).map((item) => item.id), ['bug-1']);
  assert.deepEqual(filterReportItems(items, { status: 'blocked' }).map((item) => item.id), ['bug-1']);
  assert.deepEqual(filterReportItems(items, { search: 'panes' }).map((item) => item.id), ['feature-1']);
});

test('reads JSON encoded payload and client context fields', () => {
  const items = normalizeReportItems({
    reports: [{
      id: 'json-1',
      module_id: 'reports',
      title: 'Encoded feature',
      payload: JSON.stringify({
        kind: 'feature',
        expected: 'Feature fields survive projection encoding.',
        ctox_change_summary: 'Projected from JSON payload.',
      }),
      client_context: JSON.stringify({
        attachment: {
          capture_mode: 'viewport',
          data_url: 'data:image/png;base64,AAAA',
        },
      }),
      updated_at_ms: 40,
    }],
    t,
  });

  assert.equal(items.length, 1);
  assert.equal(items[0].kind, 'feature');
  assert.equal(items[0].expected, 'Feature fields survive projection encoding.');
  assert.equal(items[0].changeSummary, 'Projected from JSON payload.');
  assert.equal(items[0].attachment.capture_mode, 'viewport');
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

console.log(`${passed} reports tests passed`);
