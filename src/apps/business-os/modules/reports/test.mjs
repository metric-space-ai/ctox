import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { readFile } from 'node:fs/promises';
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

const {
  filterReportItems,
  isPendingReportSyncStatus,
  normalizeReportItems,
  resolveReportsContextRecord,
} = await importBrowserBundle('./index.js');

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

test('treats transient sync states as pending data, not true empty results', () => {
  assert.equal(isPendingReportSyncStatus('connecting'), true);
  assert.equal(isPendingReportSyncStatus('reconnecting'), true);
  assert.equal(isPendingReportSyncStatus('syncing'), true);
  assert.equal(isPendingReportSyncStatus('connected'), false);
  assert.equal(isPendingReportSyncStatus('failed'), false);
});

test('right-click context resolves the clicked report before selected fallback', () => {
  const reports = [
    { id: 'selected-report', title: 'Selected' },
    { id: 'clicked-report', title: 'Clicked' },
  ];

  assert.equal(resolveReportsContextRecord({
    clickedReportId: 'clicked-report',
    selectedId: 'selected-report',
    visibleReports: reports,
    allReports: reports,
  }).id, 'clicked-report');

  assert.equal(resolveReportsContextRecord({
    clickedReportId: '',
    selectedId: 'selected-report',
    visibleReports: reports,
    allReports: reports,
  }).id, 'selected-report');
});

test('presentation layer stays compact and shell-native', async () => {
  const css = await readFile(new URL('./index.css', import.meta.url), 'utf8');
  const html = await readFile(new URL('./index.html', import.meta.url), 'utf8');
  const source = `${css}\n${html}`;
  const forbiddenSurfacePattern = new RegExp(['ctox-pane--gla' + 'ss', 'Prem' + 'ium', 'gla' + 'ss'].join('|'), 'i');

  assert.doesNotMatch(source, forbiddenSurfacePattern);
  assert.doesNotMatch(source, /border-(?:left|right)\s*:\s*(?:[2-9]|[0-9]{2,})px/);
  assert.doesNotMatch(source, /border-radius:\s*(?:10|12|14|16|18|20|24)px/);
  assert.doesNotMatch(source, /box-shadow:\s*(?:0|inset|rgba|color-mix)/);
  // 3-pane contract: collapsible actions column on the right, hidden by
  // default, toggle from the detail header. Resizers driven by the shell's
  // kit width vars.
  assert.match(html, /class="ctox-workspace reports-module[^"]*"/);
  assert.match(html, /is-actions-hidden/);
  assert.match(html, /data-toggle-actions/);
  assert.match(html, /class="ctox-pane reports-actions"/);
  assert.match(html, /data-resizer-var="--ctox-left-width"/);
  assert.match(html, /data-resizer-var="--ctox-right-width"/);
  assert.match(css, /--ctox-left-width: 320px/);
  assert.match(css, /--ctox-right-width: 340px/);
  // Collapsed actions column is two-pane; resizers hide on narrow viewports.
  assert.match(css, /\.reports-module\.is-actions-hidden[\s\S]*grid-template-columns: var\(--ctox-left-width, 320px\) 12px minmax\(0, 1fr\)/);
  assert.match(css, /\.reports-module[^\{]*\[data-resizer\][\s\S]*display: none !important/);
  assert.match(css, /@container business-app-window \(max-width: 1180px\)/);
  assert.match(css, /@container business-app-window \(max-width: 760px\)/);
  // Decorative helpers from the previous layout are gone — the icon button's
  // aria-label/title is the single source of the accessible name.
  assert.doesNotMatch(css, /\.reports-sr-only/);
  assert.doesNotMatch(html, /reports-sr-only/);
});

test('rail chrome is shell grammar: search, view toggle, tray, counted band, footer', async () => {
  const css = await readFile(new URL('./index.css', import.meta.url), 'utf8');
  const html = await readFile(new URL('./index.html', import.meta.url), 'utf8');
  const js = await readFile(new URL('./index.js', import.meta.url), 'utf8');
  const manifest = JSON.parse(await readFile(new URL('./module.json', import.meta.url), 'utf8'));

  // Canonical data-pg-* grammar markup on the rail pane.
  assert.match(html, /data-pg-search/);
  assert.match(html, /data-pg-view="cards"/);
  assert.match(html, /data-pg-view="list"/);
  assert.match(html, /data-pg-tray-toggle/);
  assert.match(html, /data-pg-tray hidden/);
  assert.match(html, /data-pg-filter data-pg-name="status" data-pg-default="all"/);
  assert.match(html, /data-pg-reset/);
  assert.match(html, /data-pg-band="all"/);
  assert.match(html, /data-pg-band="bug"/);
  assert.match(html, /data-pg-band="feature"/);
  assert.match(html, /data-pg-count="all"/);
  assert.match(html, /data-pg-count="bug"/);
  assert.match(html, /data-pg-count="feature"/);
  // Exactly two footers: rail (grammar-fed) + detail (module-fed).
  assert.equal(html.match(/data-pg-footer/g).length, 2);
  // Active-filter dot CSS survives on the tray toggle class.
  assert.match(css, /\.reports-filter-toggle\.has-active-filters::after/);

  // Old hand-rolled filter markup is gone.
  assert.doesNotMatch(html, /data-report-search|data-report-view|data-toggle-report-filters|data-report-filter-advanced|data-reset-report-filters|data-report-status|data-report-kind=|data-count-kind-|data-reports-footer|data-report-detail-footer/);
  assert.doesNotMatch(js, /syncReportFilterIndicator/);

  // The module listens to the bubbling grammar event and feeds counts/footer
  // through the pane grammar handle (with direct-markup fallbacks).
  assert.match(js, /ctox-pane-grammar-change/);
  assert.match(js, /__ctoxPaneGrammar/);

  // Dead refresh button fully removed: markup, wiring, spinner CSS.
  assert.doesNotMatch(html, /data-refresh-reports/);
  assert.doesNotMatch(js, /data-refresh-reports/);
  assert.doesNotMatch(css, /reports-refresh-button/);

  // Standing rail action: JSON export of the filtered list.
  assert.match(html, /class="ctox-pane-icon" data-action="export-json"/);
  assert.match(js, /function exportVisibleReports\(\)/);
  assert.match(js, /URL\.createObjectURL/);

  // In-place selection flip: aria-selected on rows, no list rebuild on click.
  assert.match(js, /function applyReportsSelection\(\)/);
  assert.match(js, /aria-selected/);
  assert.match(js, /renderList\(\{ resetScroll = false \} = \{\}\)/);

  // Cache-buster contract: markup + stylesheet inherit the JS ?v= buster.
  assert.match(js, /async function loadModuleMarkup\(\)/);
  assert.match(js, /\?v=\$\{version\}/);
  assert.match(js, /link\.dataset\.reportsStyle = 'true'/);

  // Manifest: semantic version + documented third pane.
  assert.match(manifest.layout.third_pane_justification, /Aktion/);
  assert.match(manifest.version, /^\d+\.\d+\.\d+$/);

  // No web-storage state — filters live in module state, data in RxDB.
  assert.doesNotMatch(js, /localStorage|sessionStorage/);
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
