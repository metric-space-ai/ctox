import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { readFileSync } from 'node:fs';
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

const { __ctoxTestHooks: hooks } = await importBrowserBundle('./index.js');

const {
  aggregateFlowMetrics,
  applyTaskSelection,
  clampMetric,
  compactTaskFlowRow,
  deriveHarnessHealth,
  eventToNodeId,
  flowSourceView,
  formatRelativeAge,
  friendlyWebStackStatus,
  labels,
  normalizeFocusTask,
  observedDetailsFromFlow,
  progressPercent,
  renderTaskList,
  resolveSelectedTaskId,
  safeTaskDisplayText,
  setFlowZoom,
  taskColumnMarkup,
  taskPipelineStage,
  taskSteps,
  timelinePanel,
  webStackPanel,
  webStackStateFromRefreshResult,
  webStackProjectionMissing,
} = hooks;

// --- Minimal fake DOM ---------------------------------------------------------
// Just enough of the element API for the focus-safe refresh + in-place selection
// pins (no HTML parsing): attribute + class + descendant selectors, and a plain
// innerHTML string sink so we can assert that only the list node is rewritten.
function fakeEl(attrs = {}, children = []) {
  const el = {
    _attrs: { ...attrs },
    _classes: new Set(String(attrs.class || '').split(/\s+/).filter(Boolean)),
    children,
    innerHTML: attrs.innerHTML || '',
    value: attrs.value ?? '',
    className: attrs.class || '',
    __ctoxPaneGrammar: null,
    getAttribute(name) { return name in this._attrs ? this._attrs[name] : null; },
    setAttribute(name, val) { this._attrs[name] = String(val); },
    removeAttribute(name) { delete this._attrs[name]; },
    get classList() {
      return {
        add: (cls) => el._classes.add(cls),
        remove: (cls) => el._classes.delete(cls),
        contains: (cls) => el._classes.has(cls),
        toggle: (cls, on) => {
          const next = on === undefined ? !el._classes.has(cls) : on;
          if (next) el._classes.add(cls); else el._classes.delete(cls);
          return next;
        },
      };
    },
    querySelector(sel) { return fakeQueryAll(this, sel)[0] || null; },
    querySelectorAll(sel) { return fakeQueryAll(this, sel); },
  };
  return el;
}

function fakeMatch(sel) {
  const attrConds = [...sel.matchAll(/\[([\w-]+)(?:="([^"]*)")?\]/g)].map((m) => ({ name: m[1], value: m[2] }));
  const classConds = [...sel.matchAll(/\.([\w-]+)/g)].map((m) => m[1]);
  return (el) => attrConds.every((c) => (c.value === undefined ? el._attrs[c.name] !== undefined : el._attrs[c.name] === c.value))
    && classConds.every((c) => el._classes.has(c));
}

function fakeDescendants(el, acc = []) {
  for (const child of el.children || []) { acc.push(child); fakeDescendants(child, acc); }
  return acc;
}

function fakeQueryAll(root, sel) {
  let ctxNodes = [root];
  for (const part of sel.trim().split(/\s+/)) {
    const pred = fakeMatch(part);
    const next = [];
    for (const node of ctxNodes) for (const cand of fakeDescendants(node)) if (pred(cand)) next.push(cand);
    ctxNodes = next;
  }
  return ctxNodes;
}

const noopActionIcon = { getActionIcon: (name) => `<svg data-icon="${name}"></svg>` };

function test(name, fn) {
  try {
    fn();
    console.log(`ok - ${name}`);
  } catch (error) {
    console.error(`not ok - ${name}`);
    throw error;
  }
}

test('Presentation layer stays compact and shell-native', () => {
  const css = readFileSync(new URL('./index.css', import.meta.url), 'utf8');
  const js = readFileSync(new URL('./index.js', import.meta.url), 'utf8');
  const html = readFileSync(new URL('./index.html', import.meta.url), 'utf8');
  const manifest = readFileSync(new URL('./module.json', import.meta.url), 'utf8');
  const icon = readFileSync(new URL('./icon.svg', import.meta.url), 'utf8');
  const source = `${css}\n${js}\n${html}\n${manifest}\n${icon}`;
  const surfacePattern = new RegExp(['ctox-pane--gla' + 'ss', 'gla' + 'ss', 'Prem' + 'ium'].join('|'), 'i');
  const sidePattern = new RegExp('border-' + '(?:left|right)\\s*:\\s*(?:[2-9]|[0-9]{2,})px');
  const radiusPattern = new RegExp('border-' + 'radius:\\s*(?:8|10|12|14|16|18|20|24)px');
  const shadowPattern = new RegExp('box-' + 'shadow:\\s*(?:0|inset|rgba|color-mix|var\\(--panel-shadow\\)|var\\(--shadow-sm\\)|var\\(--shadow-md\\))');
  const gradientPattern = new RegExp(['linear-grad' + 'ient', 'radial-grad' + 'ient'].join('|'));
  const hardNeutralPattern = new RegExp(['#00' + '0', '#ff' + 'f'].join('|'), 'i');

  assert.doesNotMatch(source, surfacePattern);
  assert.doesNotMatch(source, sidePattern);
  assert.doesNotMatch(source, radiusPattern);
  assert.doesNotMatch(source, shadowPattern);
  assert.doesNotMatch(source, gradientPattern);
  assert.doesNotMatch(source, hardNeutralPattern);
  // The module frame rides on the standard kit workspace: .ctox-workspace
  // columns, .ctox-pane panels and the declarative shell resizer — the module
  // must not re-declare its own column grid or resizer chrome.
  assert.match(html, /class="ctox-workspace ctox-workspace--two-pane ctox-harness-app"/);
  assert.match(html, /class="ctox-pane ctox-harness-left"/);
  assert.match(html, /class="ctox-pane ctox-harness-main"/);
  assert.match(html, /class="ctox-column-resizer"[^>]*data-resizer-var="--ctox-left-width"/);
  assert.doesNotMatch(css, /\.ctox-column-resizer\s*\{/);
  assert.doesNotMatch(css, /grid-template-columns:\s*var\(--ctox-left-width\)/);
  assert.match(manifest, /currentColor/);
});

test('Task column pins the shell-owned canonical grammar contract', () => {
  const js = readFileSync(new URL('./index.js', import.meta.url), 'utf8');
  const html = readFileSync(new URL('./index.html', import.meta.url), 'utf8');
  const state = {
    ctx: noopActionIcon,
    lang: 'en',
    selectedTaskId: 'task-working',
    taskSearch: '',
    taskViewMode: 'cards',
    taskPrimaryView: 'all',
    taskSourceFilter: 'all',
    taskPinFilter: 'all',
    taskSort: 'updated',
    taskSortDirection: 'desc',
    pinnedTaskIds: new Set(),
  };
  const tasks = [
    { id: 'task-working', title: 'Working task', status: 'running', source: 'ctox', updatedAt: '2026-07-21T10:00:00Z' },
    { id: 'task-done', title: 'Done task', status: 'completed', source: 'threads', updatedAt: '2026-07-21T09:00:00Z' },
  ];
  const markup = taskColumnMarkup(tasks, state);

  // SHELL-owned data-pg-* grammar with the kit classes (no bespoke chrome).
  assert.match(markup, /class="ctox-filterbar"[\s\S]*data-pg-search[\s\S]*data-pg-view="cards"[\s\S]*data-pg-view="list"[\s\S]*data-pg-tray-toggle/);
  assert.match(markup, /class="ctox-filter-tray" data-pg-tray hidden[\s\S]*data-pg-name="source"[\s\S]*data-pg-name="pin"[\s\S]*data-pg-name="sort"[\s\S]*data-pg-reset/);
  assert.match(markup, /class="ctox-view-switch"/);
  assert.doesNotMatch(markup, /ctox-task-filterbar|ctox-task-filter-tray|ctox-task-view-switch|data-task-search|data-toggle-task-filters|data-task-primary-view/);
  const bands = markup.match(/data-pg-band="[a-z]+"/g) || [];
  assert.ok(bands.length >= 2, 'counted view band must have at least two real views');
  assert.match(markup, /data-pg-band="all"[\s\S]*data-pg-count="all"> \(2\)</);
  assert.match(markup, /data-pg-band="working"[\s\S]*data-pg-count="working"> \(1\)</);
  assert.match(markup, /data-pg-band="waiting"[\s\S]*data-pg-count="waiting"> \(0\)</);
  assert.match(markup, /data-pg-band="done"[\s\S]*data-pg-count="done"> \(1\)</);
  assert.match(markup, /class="ctox-pane-body ctox-well"/);
  assert.match(markup, /<footer class="ctox-pane-footer"><span data-pg-footer>2 entries · All<\/span><\/footer>/);
  assert.doesNotMatch(markup, /ctox-badge/);
  // index.html carries an empty left pane — the module builds the localized
  // chrome once (never a second static, drift-prone copy).
  assert.match(html, /<aside class="ctox-pane ctox-harness-left" data-ctox-left aria-label="CTOX Tasks"><\/aside>/);
  assert.doesNotMatch(js, /localStorage/);
  assert.match(js, /moduleAssetUrl\('\.\/index\.html'\)/);
  assert.match(js, /moduleAssetUrl\('\.\/index\.css'\)/);
});

test('Data refresh re-renders only the list content, never the search input node', () => {
  const searchNode = fakeEl({ 'data-pg-search': '', value: '' });
  const sourceSelect = fakeEl({ 'data-pg-filter': '', 'data-pg-name': 'source', 'data-pg-default': 'all', value: 'all' });
  const countAll = fakeEl({ 'data-pg-count': 'all' });
  const footer = fakeEl({ 'data-pg-footer': '' });
  const list = fakeEl({ 'data-task-list': '', class: 'ctox-list ctox-task-list is-cards', innerHTML: 'STALE' });
  const well = fakeEl({ class: 'ctox-pane-body ctox-well' }, [list]);
  const left = fakeEl({ 'data-ctox-left': '', class: 'ctox-pane ctox-harness-left' }, [searchNode, sourceSelect, countAll, footer, well]);
  const host = fakeEl({}, [left]);
  const state = {
    ctx: { ...noopActionIcon, host },
    lang: 'en',
    selectedTaskId: '',
    taskSearch: '',
    taskViewMode: 'cards',
    taskPrimaryView: 'all',
    taskSourceFilter: 'all',
    taskPinFilter: 'all',
    taskSort: 'updated',
    taskSortDirection: 'desc',
    pinnedTaskIds: new Set(),
    model: { tasks: [] },
  };

  renderTaskList(state);

  // The exact search input object survives the refresh (no focus/caret loss).
  assert.equal(host.querySelector('[data-pg-search]'), searchNode);
  assert.equal(searchNode.value, '');
  // Only the list content was rewritten.
  assert.notEqual(list.innerHTML, 'STALE');
  assert.match(list.innerHTML, /ctox-empty/);
  // Counts + footer flowed through the null-guarded (no grammar handle) fallback.
  assert.equal(countAll.textContent, ' (0)');
  assert.equal(footer.textContent, '0 entries · All');
});

test('Selecting a task is an in-place class flip across the existing rows', () => {
  const rowA = fakeEl({ 'data-task-id': 'a', class: 'ctox-list-item ctox-task-card is-selected' });
  const rowB = fakeEl({ 'data-task-id': 'b', class: 'ctox-list-item ctox-task-card' });
  const list = fakeEl({ 'data-task-list': '' }, [rowA, rowB]);
  const left = fakeEl({ 'data-ctox-left': '' }, [list]);
  const host = fakeEl({}, [left]);
  const state = { ctx: { host }, selectedTaskId: 'b' };

  applyTaskSelection(state);

  // Same row objects, only the selection classes/attrs flipped in place.
  assert.equal(list.children[0], rowA);
  assert.equal(list.children[1], rowB);
  assert.equal(rowA.classList.contains('is-selected'), false);
  assert.equal(rowA.getAttribute('aria-selected'), 'false');
  assert.equal(rowB.classList.contains('is-selected'), true);
  assert.equal(rowB.getAttribute('aria-selected'), 'true');
});

test('Web Stack panel is hidden by default and the toggle reveals it', () => {
  const js = readFileSync(new URL('./index.js', import.meta.url), 'utf8');
  const base = {
    ctx: noopActionIcon,
    lang: 'en',
    model: { tasks: [] },
    webStack: { loading: false, error: '', notice: '', data: null },
  };
  const closed = webStackPanel({ ...base, webStackPanelOpen: false });
  const open = webStackPanel({ ...base, webStackPanelOpen: true });

  assert.match(closed, /<section class="ctox-web-stack-panel[^"]*" data-webstack-panel[^>]*hidden>/);
  assert.doesNotMatch(open, /data-webstack-panel[^>]*hidden>/);
  assert.match(open, /data-webstack-panel/);
  // Restored on-demand machinery: collected header toggle + credential/auth wiring.
  assert.match(js, /data-webstack-toggle/);
  assert.match(open, /<header class="ctox-pane-title-row ctox-web-stack-head">/);
  assert.match(open, /class="ctox-pane-actions ctox-web-stack-head-actions"[\s\S]*data-webstack-check-projection/);
  assert.match(open, /data-webstack-check-projection[^>]*aria-label="Reload Web Stack projection"[^>]*title="Reload Web Stack projection"/);
  assert.match(open, /data-webstack-check-projection[\s\S]*data-icon="refresh"/);
  assert.doesNotMatch(open, /data-webstack-refresh/);
  assert.match(js, /data-webstack-auth-source/);
  assert.match(js, /function requestWebStackAuthAssist/);
});

test('Compact task rendering shows the four-stage live flow and session pins', () => {
  const state = {
    ctx: { getActionIcon: (name) => `<svg data-icon="${name}"></svg>` },
    lang: 'en',
    selectedTaskId: 'task-review',
    pinnedTaskIds: new Set(['task-review']),
  };
  const task = {
    id: 'task-review',
    title: 'Reference-grade CTOX console',
    status: 'review',
    source: 'ctox',
    updatedAt: '2026-07-21T10:00:00Z',
  };
  const markup = compactTaskFlowRow(task, state);

  assert.equal(taskPipelineStage({ status: 'queued' }), 0);
  assert.equal(taskPipelineStage({ status: 'running' }), 1);
  assert.equal(taskPipelineStage({ status: 'review' }), 2);
  assert.equal(taskPipelineStage({ status: 'completed' }), 3);
  assert.match(markup, /data-compact-flow/);
  assert.match(markup, /Queued[\s\S]*Working[\s\S]*Review[\s\S]*Done/);
  assert.match(markup, /data-flow-stage="2"/);
  assert.match(markup, /data-pin-task-id="task-review"[^>]*aria-pressed="true"/);
  assert.match(markup, /data-context-record-id="task-review"/);
  assert.match(markup, /data-context-record-type="ctox_task"/);
  assert.match(markup, /data-context-label="Reference grade CTOX console"/);
  assert.equal(state.pinnedTaskIds.has('task-review'), true);
});

test('Task focus normalizes launch args and shell events consistently', () => {
  assert.deepEqual(normalizeFocusTask({ task_id: 'queue-42', command_id: 'cmd-42', open_drawer: true }), {
    taskId: 'queue-42',
    commandId: 'cmd-42',
    taskStatus: '',
    sourceModule: 'business-os',
    openDrawer: true,
  });
  assert.deepEqual(
    normalizeFocusTask({ taskId: 'queue-42', commandId: 'cmd-42', openDrawer: true }),
    normalizeFocusTask({ task_id: 'queue-42', command_id: 'cmd-42', open_drawer: true }),
  );
});

test('A focused task that appears later replaces the previous fallback selection', () => {
  const model = {
    tasks: [
      { id: 'queue-old', commandId: 'cmd-old', status: 'running' },
      { id: 'queue-target', commandId: 'cmd-target', status: 'queued' },
    ],
  };
  assert.equal(
    resolveSelectedTaskId(model, { taskId: 'queue-target', commandId: 'cmd-target' }, 'queue-old'),
    'queue-target',
  );
});

test('WebRTC status does not claim CTOX flow is connected when projection is missing', () => {
  const view = flowSourceView({
    lang: 'de',
    flow: { ok: false, mode: 'unavailable' },
    runtimeStatus: 'RxDB WebRTC',
    ctx: { sync: { mode: 'webrtc' } },
  });
  assert.equal(view.mode, 'RxDB WebRTC');
  assert.equal(view.status, labels.de.flowProjectionMissing);
  assert.notEqual(view.status, labels.de.connected);
});

test('Web Stack projection failures render as actionable sync diagnostics', () => {
  const webStack = { loading: false, error: 'Web Stack projection is not available in RxDB' };
  assert.equal(webStackProjectionMissing(webStack), true);
  assert.equal(friendlyWebStackStatus(webStack, labels.de), labels.de.webStackConnecting);
});

test('Web Stack refresh preserves projection-missing diagnostics', () => {
  const webStack = webStackStateFromRefreshResult(
    { notice: '', data: null },
    { ok: false, error: 'Web Stack projection is not available in RxDB' }
  );
  assert.equal(webStack.error, 'Web Stack projection is not available in RxDB');
  assert.equal(webStackProjectionMissing(webStack), true);
  assert.equal(friendlyWebStackStatus(webStack, labels.de), labels.de.webStackConnecting);
});

test('Task display copy redacts source code and Web Stack internals', () => {
  assert.equal(
    safeTaskDisplayText('```js\nconst token = "secret";\n```', 'de'),
    labels.de.redactedTechnicalDetail
  );
  assert.equal(
    safeTaskDisplayText('browser_context frame_data capture_script payload', 'en'),
    labels.en.redactedTechnicalDetail
  );
  assert.equal(
    safeTaskDisplayText('Queue state is waiting for review', 'en'),
    'Queue state is waiting for review'
  );
});

test('Queued work with missing flow projection is a critical harness health state', () => {
  const health = deriveHarnessHealth({
    lang: 'de',
    flow: { ok: false, error: 'rxdb_flow_projection_unavailable' },
    ctx: { sync: { mode: 'webrtc' } },
    model: {
      tasks: [
        {
          id: 'queue:system::1e204',
          title: 'Olaf CTOX MCP Skill Install',
          status: 'queued',
          routeStatus: 'pending',
          createdAt: new Date(Date.now() - 5 * 60 * 1000).toISOString(),
        },
      ],
    },
  });
  assert.equal(health.severity, 'critical');
  assert.equal(health.reason, 'flow_projection_missing');
  assert.equal(health.waitingCount, 1);
  assert.equal(health.activeCount, 0);
  assert.equal(health.focusTaskId, 'queue:system::1e204');
});

test('Queued work without a lease becomes critical after the stall grace window', () => {
  const health = deriveHarnessHealth({
    lang: 'de',
    flow: { ok: true },
    ctx: { sync: { mode: 'webrtc' } },
    model: {
      tasks: [
        {
          id: 'queue:stalled',
          status: 'queued',
          routeStatus: 'pending',
          createdAt: new Date(Date.now() - 3 * 60 * 1000).toISOString(),
        },
      ],
    },
  });
  assert.equal(health.severity, 'critical');
  assert.equal(health.reason, 'queue_stalled');
});

test('Empty CTOX task selection does not crash task step rendering', () => {
  assert.deepEqual(taskSteps(null, { model: { timeline: [] } }), []);
});

test('Task-bound worker telemetry activates the running node with live tool details', () => {
  const flow = {
    ok: true,
    flow: {
      blocks: [],
      ledger_events: [
        {
          event_kind: 'worker.turn_started',
          title: 'Agent turn started',
          body_text: '',
          created_at: '2026-07-17T08:00:00Z',
          metadata_json: JSON.stringify({
            runtime: { seconds: 0 },
            tool_call_count: 0,
            metrics_mode: 'cumulative',
          }),
        },
        {
          event_kind: 'worker.token_usage',
          title: 'Model usage updated',
          body_text: '',
          created_at: '2026-07-17T08:00:10Z',
          metadata_json: JSON.stringify({
            usage: { input_tokens: 1200, output_tokens: 340 },
            runtime: { seconds: 10 },
            tool_call_count: 1,
            metrics_mode: 'cumulative',
          }),
        },
        {
          event_kind: 'worker.tool_started',
          title: 'Tool started: web.search',
          body_text: '',
          created_at: '2026-07-17T08:00:12Z',
          metadata_json: JSON.stringify({
            runtime: { seconds: 12 },
            tool_call_count: 2,
            metrics_mode: 'cumulative',
            tool: { type: 'mcp', name: 'web.search', call_id: 'call-2' },
          }),
        },
      ],
    },
  };

  assert.equal(eventToNodeId('worker.tool_started', ''), 'running');
  const details = observedDetailsFromFlow(flow, 'de').get('running');
  assert.equal(details.inputTokens, 1200);
  assert.equal(details.outputTokens, 340);
  assert.equal(details.toolCalls, 2);
  assert.equal(details.seconds, 12);
  assert.deepEqual(details.tools, ['web.search']);
  assert.match(details.lines.at(-1), /Werkzeug gestartet: web\.search/);

  assert.deepEqual(aggregateFlowMetrics(flow), {
    inputTokens: 1200,
    outputTokens: 340,
    toolCalls: 2,
    seconds: 12,
  });
});

test('Flow zoom is symmetric and clamped', () => {
  const state = { zoom: 1 };
  setFlowZoom(state, state.zoom + 0.12);
  assert.equal(state.zoom, 1.12);
  setFlowZoom(state, state.zoom - 0.12);
  assert.equal(state.zoom, 1);
  setFlowZoom(state, -20);
  assert.equal(state.zoom, 0.72);
  setFlowZoom(state, 20);
  assert.equal(state.zoom, 1.8);
});

test('Single-event timeline is diagnostic and disabled', () => {
  const node = {
    id: 'queued',
    label: 'Waiting in queue',
    phase: 'Queued',
    lines: ['Work is queued.'],
    inputTokens: null,
    outputTokens: null,
  };
  const html = timelinePanel({
    lang: 'de',
    selectedStepIndex: 0,
    model: { timeline: [node] },
  }, null, node, {});
  assert.match(html, /is-disabled/);
  assert.match(html, /disabled aria-disabled="true"/);
  assert.match(html, new RegExp(labels.de.timelineUnavailable));
  assert.equal(progressPercent(0, 0), 100);
  assert.equal(clampMetric(999, 0, 10), 10);
  assert.equal(formatRelativeAge(30_000, 'de'), 'unter 1 Min.');
});
