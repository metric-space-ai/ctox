import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import { createRequire } from 'node:module';
import { homedir } from 'node:os';
import { join as joinPath } from 'node:path';
// Lader (OLDTEST): esbuild aus dem Klon, sonst ESBUILD_MODULE, sonst aus dem Haupt-Checkout
// ~/Documents/ctox/src/apps/business-os/node_modules (wie die v4-api-Tests mit PLAYWRIGHT_MODULE). Nicht installieren.
const requireDev = createRequire(import.meta.url);
function loadDev(name, envVar) {
  try { return requireDev(name); } catch {
    return requireDev(process.env[envVar] || joinPath(homedir(), 'Documents/ctox/src/apps/business-os/node_modules', name));
  }
}
const { build } = loadDev('esbuild', 'ESBUILD_MODULE');

async function importBrowserBundle(relativePath) {
  const bundledModule = await build({
    entryPoints: [fileURLToPath(new URL(relativePath, import.meta.url))],
    bundle: true,
    format: 'esm',
    platform: 'browser',
    write: false,
    // Wie der Service Worker (§16.15): ctox:shared/x.js → Shell-Pfad.
    plugins: [{
      name: 'ctox-shared',
      setup(b) {
        b.onResolve({ filter: /^ctox:shared\// }, (args) => ({ path: fileURLToPath(new URL(`../../shared/${args.path.slice('ctox:shared/'.length)}`, import.meta.url)) }));
      },
    }],
  });

  const [{ text: bundledSource }] = bundledModule.outputFiles;
  return import(`data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`);
}

const {
  codeViewTargetFor,
  filterReportItems,
  isCodingRunning,
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

test('v4: empty data renders no state text; feedback goes to the status zone only', async () => {
  const js = await readFile(new URL('./index.js', import.meta.url), 'utf8');
  // Leere Liste rendert nichts, kein Syncing-/Leer-Kasten (PORTING.md R11).
  assert.doesNotMatch(js, /ctox-syncing|ctox-empty|role="status"/);
  assert.doesNotMatch(js, /collectionReadiness|subscribeCollectionReadiness|startCollection|restartCollections/);
  assert.match(js, /list\.replaceChildren\(\)/);
  // Rückmeldungen nur über api.status.report / clear, nie über ctx.notifications.
  assert.doesNotMatch(js, /notifications/);
  assert.match(js, /status\.report\(\{ level: 'error', code, text: failText/);
  assert.match(js, /api\.status\.clear\(code\)/);
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

test('v4: data only through api.query, commands through api.command without waiting', async () => {
  const js = await readFile(new URL('./index.js', import.meta.url), 'utf8');
  assert.match(js, /export async function mount\(root, api, \{ restored \} = \{\}\)/);
  assert.match(js, /api\.query\(name, \{\}\)\.subscribe/);
  assert.match(js, /api\.command\(commandType,/);
  assert.doesNotMatch(js, /commandBus|until: 'accepted'|\.find\(\)\.exec\(\)|state\.ctx/);
  // LOOP Runde 2: Übergabe an den Coding Agent nur über die Code-Ansicht der Ziel-App, keine threads-Freigabe mehr.
  assert.match(js, /state\.api\?\.shell\.openSource\(appId, options\)/);
  assert.doesNotMatch(js, /threads\.ctox_approval\.request|target_command_type|ctox_task_approval_requests/);
});

test('LOOP Runde 2: Übergabe öffnet die Code-Ansicht der Ziel-App auf der Meldung; „In Arbeit“ aus coding_turn_id', () => {
  const [item] = normalizeReportItems({ reports: [{ id: 'rep-7', report_id: 'rep-7', module_id: 'notes', kind: 'bug', title: 'x', status: 'open', coding_turn_id: 'turn-9' }] });
  assert.equal(item.status, 'running');
  assert.equal(item.codingTurnId, 'turn-9');
  assert.equal(isCodingRunning(item), true);
  assert.deepEqual(codeViewTargetFor(item), { appId: 'notes', options: { tab: 'reports', reportId: 'rep-7' } });
  const [plain] = normalizeReportItems({ reports: [{ id: 'rep-8', module_id: 'notes', status: 'open' }] });
  assert.equal(plain.status, 'open');
  assert.equal(isCodingRunning(plain), false);
  const [done] = normalizeReportItems({ reports: [{ id: 'rep-9', module_id: 'notes', status: 'completed', coding_turn_id: 'turn-1' }] });
  assert.equal(done.status, 'completed');
  assert.equal(isCodingRunning(done), false);
});

test('presentation layer stays compact and shell-native', async () => {
  const css = (await readFile(new URL('./index.css', import.meta.url), 'utf8')).replaceAll('[data-app="reports"] ', ''); // v4-Scope (scope-css.mjs --check prüft ihn)
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
  // Shell-owned breakpoints, and ONLY those two (contract §6): 1024 three
  // columns -> two, 768 two -> one. The column definition is reset at each
  // step so no empty reserved track survives.
  const containerBreakpoints = [...css.matchAll(/@container business-app-window \(max-width: (\d+)px\)/g)].map((m) => m[1]);
  assert.deepEqual([...new Set(containerBreakpoints)].sort(), ['1024', '768']);
  assert.doesNotMatch(css, /@media \(max-width/);
  const at1024 = css.split('@container business-app-window (max-width: 1024px)')[1].split('@container business-app-window (max-width: 768px)')[0];
  assert.match(at1024, /\.reports-module,\s*\.reports-module\.is-actions-hidden\s*\{\s*grid-template-columns: var\(--ctox-left-width, 320px\) 12px minmax\(0, 1fr\)/);
  assert.match(at1024, /\[data-resizer="right"\][\s\S]*display: none !important/);
  const at768 = css.split('@container business-app-window (max-width: 768px)').slice(1).join('\n');
  assert.match(at768, /\.reports-module,\s*\.reports-module\.is-actions-hidden\s*\{\s*grid-template-columns: minmax\(0, 1fr\)/);
  assert.match(at768, /grid-template-rows: none/);
  assert.match(at768, /grid-auto-rows/);
  assert.match(at768, /\[data-resizer\][\s\S]*display: none !important/);
  // Stacked, the rail header is the ONLY head of the top row: it has to keep
  // the icon zone (80px, left) and the window controls (74x74, right) free.
  // v4: der Abstand kommt aus der Shell-Variable mit 70px-Rückfall (Shell-v2-Kopfzone, DEBT D-004).
  assert.match(at768, /\.reports-rail > \.ctox-pane-header,[\s\S]{0,120}?\.reports-rail > \.ctox-filterbar \{\s*padding-right: (?:70px|var\(--shell-v2-control-inset, 70px\));/);
  // Decorative helpers from the previous layout are gone — the icon button's
  // aria-label/title is the single source of the accessible name.
  assert.doesNotMatch(css, /\.reports-sr-only/);
  assert.doesNotMatch(html, /reports-sr-only/);
});

test('rail chrome is shell grammar: search, view toggle, tray, counted band, footer', async () => {
  const css = (await readFile(new URL('./index.css', import.meta.url), 'utf8')).replaceAll('[data-app="reports"] ', ''); // v4-Scope (scope-css.mjs --check prüft ihn)
  const html = await readFile(new URL('./index.html', import.meta.url), 'utf8');
  const js = await readFile(new URL('./index.js', import.meta.url), 'utf8');
  const manifest = JSON.parse(await readFile(new URL('./module.json', import.meta.url), 'utf8'));

  // Canonical data-pg-* grammar markup on the rail pane.
  assert.match(html, /data-pg-search/);
  // ONE view control, not two (Betreiber-Direktive 31.08.2026). The button is
  // an action: no aria-pressed, no two-button group; `data-pg-view` stays as
  // the shell grammar's state channel and always holds the CURRENT view, and
  // the module flips it plus icon/label on click.
  assert.equal(html.match(/data-pg-view=/g).length, 1);
  assert.match(html, /data-reports-view-toggle data-pg-view="cards"/);
  assert.doesNotMatch(html, /data-pg-view="list"/);
  const toggleTag = html.match(/<button[^>]*data-reports-view-toggle[^>]*>/)[0];
  assert.doesNotMatch(toggleTag, /aria-pressed/);
  assert.match(toggleTag, /aria-label="Als Liste anzeigen"/);
  assert.match(toggleTag, /title="Als Liste anzeigen"/);
  assert.doesNotMatch(html, /ctox-view-toggle/);
  assert.match(js, /function syncViewToggleButton\(root\)/);
  assert.match(js, /button\.removeAttribute\('aria-pressed'\)/);
  assert.match(js, /showAsList/);
  assert.match(js, /showAsCards/);
  // The click is bound on the button, never delegated: the grammar listener
  // re-renders the icon on the same click and a delegated handler would then
  // see an already-detached SVG target.
  assert.match(js, /root\.querySelector\('\[data-reports-view-toggle\]'\)\?\.addEventListener\('click'/);
  assert.match(js, /state\.viewMode = state\.viewMode === 'list' \? 'cards' : 'list'/);
  // Cards and list are genuinely different presentations, not one row style:
  // shard = bold title + badge meta + module/date sub-line; list = one dense
  // line with a single short meta.
  assert.match(js, /class="ctox-list-item report-row .*\n\s*<strong class="report-row-title">/);
  assert.match(js, /report-row-sub/);
  assert.match(js, /reports-compact-meta/);
  assert.doesNotMatch(js, /report-row-compact[\s\S]{0,400}reports-badges/);
  assert.match(css, /\.reports-module \.report-row \{[^}]*padding: 12px 14px/);
  assert.match(css, /\.report-row-compact \{[^}]*padding: 4px 14px/);
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
  // The shared pane-grammar icon owns the compact filter trigger. Legacy
  // report-specific shell controls must not leak back into the module.
  assert.match(html, /class="ctox-pane-icon ctox-filter-toggle"/);
  assert.doesNotMatch(css, /\.reports-(?:filterbar|view-toggle|filter-toggle|filter-advanced|filter-row|select|sort-dir|view-switch|well|footer)\b/);

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

  // Standing rail action: JSON export of the filtered list. It is the rail
  // head's ONE dominant flow action and therefore carries the filled
  // `is-primary` variant (UI-Review 31.08.2026) — the class pin moved with the
  // contract, it did not loosen.
  assert.match(html, /class="ctox-pane-icon is-primary" data-action="export-json"/);
  assert.match(js, /function exportVisibleReports\(\)/);
  assert.match(js, /URL\.createObjectURL/);

  // Exactly ONE filled primary per pane head (UI-Review 31.08.2026): rail =
  // export, detail = handover to the coding agent. The actions column is
  // collapsed by default, so the detail head is where that flow action lives.
  // The disclosure toggle beside it stays a plain icon.
  assert.equal(html.match(/ctox-pane-icon is-primary/g).length, 2);
  assert.match(html, /class="ctox-pane-icon is-primary reports-head-delegate" data-head-delegate/);
  assert.doesNotMatch(html, /reports-toggle-actions[^>]*is-primary/);
  assert.match(js, /root\.querySelector\('\[data-head-delegate\]'\)\?\.addEventListener\('click'/);
  assert.match(js, /function syncHeadDelegateButton\(report\)/);

  // Missing attachments must not render an empty screenshot card: the
  // normalizer hands back `{}` (truthy) for absent client-context attachments,
  // so the detail gates on the actual image source.
  assert.match(js, /report\.attachment\?\.data_url \? report\.attachment : null/);

  // In-place selection flip: aria-selected on rows, no list rebuild on click.
  assert.match(js, /function applyReportsSelection\(\)/);
  assert.match(js, /aria-selected/);
  assert.match(js, /renderList\(\{ resetScroll = false \} = \{\}\)/);

  // v4 (§16.15): markup relativ ohne Cache-Buster; das Stylesheet lädt die Shell aus dem Manifest.
  assert.match(js, /async function loadModuleMarkup\(\)/);
  assert.match(js, /fetch\(new URL\('\.\/index\.html', import\.meta\.url\)\)/);
  assert.doesNotMatch(js, /\?v=|ensureStyles|data-reports-style/);

  // Manifest: semantic version + documented third pane.
  assert.match(manifest.layout.third_pane_justification, /Aktion/);
  assert.match(manifest.version, /^\d+\.\d+\.\d+$/);

  // No web-storage state — filters live in module state, data in RxDB.
  assert.doesNotMatch(js, /localStorage|sessionStorage/);

  // Nichts außerhalb des App-Fensters: kein eigener Dialog mehr (Übergabe = Code-Ansicht der Ziel-App, LOOP Runde 2),
  // keine Fenster-/Dokument-Ereignisse.
  assert.doesNotMatch(js, /data-delegate-modal|ctox-modal/);
  assert.doesNotMatch(js, /document\.body\.append|window\.addEventListener|window\.dispatchEvent|location\.hash/);
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
