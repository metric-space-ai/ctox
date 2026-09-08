import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { build } from 'esbuild';
import './tests/data-state.test.mjs';

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
  aggregateRunMetrics,
  crewHomeMarkup,
  confirmAnchorBody,
  memberDomainLine,
  memoryEntries,
  taskSelectionSentence,
  memberCreatureState,
  crewStripMarkup,
  memberIdentity,
  shouldShowCrewHome,
  taskCrewMember,
  changeConcernsSelectedTask,
  flowForSelectedTask,
  harnessFlowFromEvents,
  liveActivityFromEvents,
  reconcileSelection,
  withLiveActivity,
  authoritativeTaskNodeId,
  authoritativeTaskStatus,
  applyTaskSelection,
  buildHarnessModel,
  canModifyCtoxApp,
  clampMetric,
  compactTaskFlowRow,
  deriveHarnessHealth,
  eventToNodeId,
  flowCrewSvg,
  flowSvg,
  flowSourceView,
  formatRelativeAge,
  friendlyWebStackStatus,
  labels,
  mergeBundleWithCommands,
  normalizeFocusTask,
  observedDetailsFromFlow,
  progressPercent,
  renderTaskList,
  resolveSelectedTaskId,
  safeTaskDisplayText,
  setFlowZoom,
  taskColumnMarkup,
  taskListInner,
  taskPipelineStage,
  taskCrewNodeId,
  taskCrewStatus,
  taskSteps,
  timelinePanel,
  webStackPanel,
  webStackStateFromRefreshResult,
  webStackProjectionMissing,
  wireTaskSourceReadiness,
} = hooks;

test('Only configured communication accounts appear as inputs, never task-origin apps', () => {
  const tasks = [
    { module: 'omarchy-radio', status: 'running' },
    { inbound_channel: 'documents', status: 'queued' },
    { inbound_channel: 'email', status: 'queued' },
    { inbound_channel: 'email', status: 'completed' },
  ];
  assert.deepEqual(hooks.buildInboundChannels(tasks), []);
  const channels = hooks.buildInboundChannels(tasks, [
    { channel: 'email' }, { channel: 'email' }, { channel: 'slack' },
    { channel: 'queue' }, { channel: 'cron' }, { channel: 'plan' },
    { channel: 'discord', enabled: false }, { channel: 'jami', is_deleted: true },
  ]);
  assert.deepEqual(channels.map(({id, count}) => ({id, count})), [
    { id: 'email', count: 2 }, { id: 'slack', count: 0 },
  ]);
  const model = { inboundChannels: channels, nodeMap: new Map() };
  const svg = hooks.inboundEndpointFlowSvg(model, tasks[0], { lang: 'de' });
  assert.match(svg, /E-Mail/);
  assert.doesNotMatch(svg, /omarchy|documents|is-selected|report_/i);
  const empty = hooks.inboundEndpointFlowSvg({ ...model, inboundChannels: [] }, tasks[0], { lang: 'de' });
  assert.match(empty, /Keine Kanäle eingerichtet/);
  assert.doesNotMatch(empty, /ctox-flow-channel-edge/);
  assert.deepEqual(hooks.buildInboundChannels(tasks, null), []);
  const unavailable = hooks.inboundEndpointFlowSvg({ ...model, inboundChannels: [], inboundChannelsAvailable: false }, tasks[0], { lang: 'de' });
  assert.match(unavailable, /Kanäle nicht verfügbar/);
  assert.doesNotMatch(unavailable, /Keine Kanäle eingerichtet/);
});

test('Task cards explain failures while original evidence remains inspectable', () => {
  const task = { status: 'failed', failureAttemptCount: 4, statusNote: 'thread/start MCP handshake timeout <unsafe>' };
  assert.equal(hooks.taskSummaryReason(task, { lang: 'de' }), 'Die Verbindung zu einem Werkzeug konnte nicht aufgebaut werden. · 4 Versuche');
  assert.match(hooks.taskSummaryReason(task, { lang: 'en' }), /connection to a tool/);
  const details = hooks.taskDiagnosticMarkup(task, { lang: 'de' });
  assert.match(details, /<details class="ctox-task-diagnostics">/);
  assert.match(details, /thread\/start MCP handshake timeout &lt;unsafe&gt;/);
  assert.doesNotMatch(details, /<unsafe>|<details[^>]* open/);
  assert.match(hooks.taskSummaryReason({ status: 'failed', statusNote: 'CTOX chat could not continue because the model API is temporarily unavailable. The task must stay open and retry after cooldown.' }, { lang: 'de' }), /^Der Modelldienst war nicht erreichbar\.$/);
});

test('Reported task descriptions populate the order without replacing an explicit prompt', () => {
  assert.equal(hooks.taskPromptDisplay({ description: 'Die Liste lädt dauerhaft.' }).text, 'Die Liste lädt dauerhaft.');
  assert.equal(hooks.taskPromptDisplay({ prompt: 'Auftrag', description: 'Befund', summary: 'Kurzfassung' }).text, 'Auftrag');
  assert.equal(hooks.taskPromptDisplay({ summary: 'Kurzfassung' }).text, 'Kurzfassung');
  assert.equal(hooks.taskPromptDisplay({}).text, '');
});

test('crew labels describe work without exposing implementation terminology', () => {
  function check(value, path) {
    if (typeof value === 'string') {
      assert.doesNotMatch(value, /\b(?:CTOX|Harness|Queue|Lease|RxDB|WebRTC|Tasks?|Drawer|Runtime|Credentials|Captures|Extracts)\b/i, path);
    } else {
      for (const [key, child] of Object.entries(value)) check(child, `${path}.${key}`);
    }
  }
  check(labels.de, 'fallback.de');
  const locale = JSON.parse(readFileSync(new URL('./locales/de.json', import.meta.url), 'utf8'));
  check(locale, 'locale.de');
  for (const [key, value] of Object.entries(locale)) {
    if (typeof labels.de[key] === 'string') assert.equal(value, labels.de[key], key);
  }
  assert.equal(labels.de.harnessOpenTask, 'Aufgabe öffnen');
  assert.match(labels.de.harnessCriticalMessage, /\{count\}/);
  assert.match(labels.de.harnessCriticalMessage, /\{age\}/);
});

test('Missing authoritative task telemetry remains a safe empty state', () => {
  assert.equal(authoritativeTaskStatus(null), '');
  assert.equal(authoritativeTaskNodeId(null), '');
  assert.equal(authoritativeTaskStatus({ routeStatus: 'handled', executionPhase: 'terminal', terminalStatus: 'completed' }), 'completed');
});

test('Terminal routing failure outranks stale command and plan, remains inspectable with its error', () => {
  const bundle = mergeBundleWithCommands(
    { runs: [], queue: [], communications: [], tickets: [], tools: [] },
    [{ id: 'cmd', command_id: 'cmd', execution_task_id: 'cereda', execution_mode: 'queue',
      execution_phase: 'queued', terminal_status: 'none', status: 'accepted',
      payload: { title: 'Cereda' }, execution_progress: { phase: 'queued', steps: [] } }],
    [{ id: 'cereda', command_id: 'cmd', status: 'queued', route_status: 'failed',
      failure_class: 'terminal', failure_attempt_count: 4,
      status_note: 'thread/start MCP handshake timeout', updated_at_ms: Date.now() }],
  );
  const model = buildHarnessModel(bundle, { ok: false }, 'en');
  const task = model.tasks.find((item) => item.id === 'cereda');
  assert.ok(task, 'failed native task must remain available for inspection/retry');
  assert.equal(task.status, 'failed');
  assert.equal(authoritativeTaskStatus(task), 'failed');
  assert.equal(authoritativeTaskNodeId(task), 'model-failed');
  assert.equal(taskCrewStatus(task), 'failed');
  assert.equal(model.activeTask, null);
  const steps = taskSteps(task, { model, lang: 'en', flow: { ok: false } });
  assert.equal(steps.find((step) => step.active).id, 'model-failed');
  assert.match(steps[0].detail, /4 attempts/);
  assert.match(steps[0].detail, /final/i);
  assert.match(steps[0].detail, /thread\/start MCP handshake timeout/);
  assert.doesNotMatch(steps[0].detail, /Waiting in queue/);
  for (const phase of ['queued', 'running', 'awaiting_review']) {
    assert.equal(authoritativeTaskStatus({ ...task, executionPhase: phase }), 'failed');
    assert.equal(authoritativeTaskNodeId({ ...task, executionPhase: phase }), 'model-failed');
  }
});

test('Harness diagram renders complete nodes with and without a selected task', () => {
  const model = buildHarnessModel(
    { runs: [], queue: [], communications: [], tickets: [], tools: [] },
    { ok: false },
    'en',
  );
  const trace = { nodeStrength: new Map(), edgeStrength: new Map() };
  const working = { id: 'flow-render-task', status: 'running', executionPhase: 'running' };
  for (const selectedTask of [null, working]) {
    const html = flowSvg(model, model.nodeMap.get('queued'), trace, selectedTask, { lang: 'en' });
    assert.match(html, /class="ctox-flow-diagram"/);
    assert.equal((html.match(/class="ctox-flow-node-g /g) || []).length, model.nodes.length);
    if (selectedTask) {
      assert.match(html, /class="ctox-flow-node-g [^"]*is-crew-hier[^>]*\sdata-node-id="running"/);
      assert.equal((html.match(/is-crew-hier/g) || []).length, 1);
    } else {
      assert.doesNotMatch(html, /is-crew-hier/);
    }
  }
});

test('CTOX flow map places the same crew on waiting, working, and failed task nodes', () => {
  const working = {
    id: 'task-working',
    commandId: 'cmd-working',
    title: 'Working task',
    status: 'queued',
    executionProgress: {
      phase: 'working',
      percent: 60,
      currentStep: 2,
      completedSteps: 2,
      totalSteps: 3,
      steps: [
        { position: 1, label: 'Read', status: 'completed', activityTurns: 2 },
        { position: 2, label: 'Check', status: 'in_progress', activityTurns: 5 },
        { position: 3, label: 'Write', status: 'pending', activityTurns: 0 },
      ],
      activityTurns: { total: 7, thinking: 4, tools: 3, lastKind: 'tool' },
      updatedAtMs: 1720000000123,
    },
  };
  const waiting = { id: 'task-waiting', commandId: 'cmd-waiting', title: 'Waiting task', status: 'queued', executionPhase: 'queued' };
  const failed = { id: 'task-failed', commandId: 'cmd-failed', title: 'Failed task', status: 'failed', executionPhase: 'terminal', terminalStatus: 'failed' };
  const model = {
    activeTask: working,
    activeNodeId: 'running',
    tasks: [waiting, failed, working],
    nodeMap: new Map([
      ['queued', { id: 'queued', x: 120, y: 160 }],
      ['running', { id: 'running', x: 420, y: 160 }],
      ['model-failed', { id: 'model-failed', x: 720, y: 360 }],
    ]),
  };
  const workingHtml = flowCrewSvg(model, working, { lang: 'de' });
  const waitingHtml = flowCrewSvg(model, waiting, { lang: 'de' });
  const failedHtml = flowCrewSvg(model, failed, { lang: 'de' });
  assert.equal((workingHtml.match(/ctox-flow-creature-slot/g) || []).length, 1);
  assert.equal((waitingHtml.match(/ctox-flow-creature-slot/g) || []).length, 2);
  assert.equal((failedHtml.match(/ctox-flow-creature-slot/g) || []).length, 2);
  assert.doesNotMatch(workingHtml, /data-task-id="task-(waiting|failed)"/);
  assert.doesNotMatch(waitingHtml, /data-task-id="task-failed"/);
  assert.doesNotMatch(failedHtml, /data-task-id="task-waiting"/);
  for (const [html, id] of [[workingHtml, working.id], [waitingHtml, waiting.id], [failedHtml, failed.id]]) {
    assert.match(html, new RegExp(`class="ctox-flow-creature-slot is-selected"[^>]+data-task-id="${id}"`));
    assert.match(html, /data-task-id="task-working"/);
  }
  const html = workingHtml + waitingHtml + failedHtml;
  assert.match(html, /data-task-id="task-working"[^>]+data-creature-node-id="running"/);
  assert.match(html, /data-task-id="task-waiting"[^>]+data-creature-node-id="queued"/);
  assert.match(html, /data-task-id="task-failed"[^>]+data-creature-node-id="model-failed"/);
  const noSelectionHtml = flowCrewSvg(model, null, { lang: 'de' });
  assert.equal((noSelectionHtml.match(/ctox-flow-creature-slot/g) || []).length, 1);
  assert.doesNotMatch(noSelectionHtml, /data-task-id="task-(waiting|failed)"/);
  assert.match(noSelectionHtml, /data-task-id="task-working"[^>]+data-creature-node-id="running"/);
  const failedSelected = flowCrewSvg(model, failed, { lang: 'de' });
  assert.equal((failedSelected.match(/ctox-flow-creature-slot/g) || []).length, 2);
  assert.match(failedSelected, /data-task-id="task-failed"[^>]+data-creature-node-id="model-failed"/);
  assert.match(failedSelected, /data-task-id="task-working"[^>]+data-creature-node-id="running"/);
  assert.match(html, /is-working/);
  assert.match(html, /data-activity-turns="7"/);
  assert.match(html, /data-activity-kind="tool"/);
  assert.match(html, /--ctox-progress-angle:216deg/);
  assert.doesNotMatch(workingHtml, /is-sleeping/);
  assert.doesNotMatch(noSelectionHtml, /is-sleeping/);
  assert.match(waitingHtml, /is-sleeping/);
  assert.match(failedSelected, /is-failed/);
  assert.equal(taskCrewNodeId(working, model), 'running');
  assert.equal(taskCrewStatus(working), 'running');
  assert.equal(taskCrewStatus(waiting), 'queued');
});

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

function withoutExpectedWarnings(fn) {
  const warn = console.warn;
  console.warn = () => {};
  try {
    return fn();
  } finally {
    console.warn = warn;
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
  assert.match(css, /\.shell-window\[data-shell-contract="v2"\] \.ctox-harness-app \.ctox-pane-header[\s\S]*grid-template-rows:/);
  assert.match(css, /\.shell-window\[data-shell-contract="v2"\] \.ctox-harness-app \.ctox-filterbar[\s\S]*flex-wrap: nowrap/);
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
  // Betreiber-Direktive 31.08.2026: the shard/list switch is ONE button, not a
  // pressed pair. It stays a data-pg-view node (the shell reads the pane's view
  // from it) and that attribute carries the CURRENT view, so an unrelated
  // grammar emit — a search keystroke, a filter, a band tab — cannot flip the
  // mode; the icon and the label name the view the click switches TO.
  assert.match(markup, /class="ctox-filterbar"[\s\S]*data-pg-search[\s\S]*data-ctox-view-toggle data-pg-view="cards"[\s\S]*data-pg-tray-toggle/);
  assert.equal((markup.match(/data-pg-view=/g) || []).length, 1, 'exactly one view control');
  assert.match(markup, /data-ctox-view-toggle[^>]*aria-label="Show as list" title="Show as list"/);
  assert.doesNotMatch(markup, /data-ctox-view-toggle[^>]*aria-pressed/);
  const listMarkup = taskColumnMarkup(tasks, { ...state, taskViewMode: 'list' });
  assert.match(listMarkup, /data-ctox-view-toggle data-pg-view="list"[^>]*aria-label="Show as cards"/);
  assert.equal((listMarkup.match(/data-pg-view=/g) || []).length, 1, 'exactly one view control');
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
  assert.match(html, /<aside class="ctox-pane ctox-harness-left" data-ctox-left aria-label="Aufgaben der Crew"><\/aside>/);
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

test('Task list empty-state follows collection readiness of the backing sources', () => {
  const base = {
    ctx: { ...noopActionIcon },
    lang: 'en',
    taskSearch: '',
    taskViewMode: 'cards',
    taskPrimaryView: 'all',
    taskSourceFilter: 'all',
    taskPinFilter: 'all',
    taskSort: 'updated',
    taskSortDirection: 'desc',
    pinnedTaskIds: new Set(),
  };
  const withReadiness = (snapshot) => ({
    ...base,
    ctx: {
      ...base.ctx,
      sync: { collectionReadiness: (name) => ({ collection: name, updatedAt: 0, ...snapshot }) },
    },
  });
  const unready = withReadiness({ state: 'catching-up', ready: false, syncing: true });
  const ready = withReadiness({ state: 'live', ready: true, syncing: false });

  // Data-driven empty + any source not yet initially synced ⇒ syncing shell.
  const syncingHtml = taskListInner([], unready);
  assert.match(syncingHtml, /class="ctox-syncing"/);
  assert.match(syncingHtml, /role="status"/);
  assert.doesNotMatch(syncingHtml, /ctox-empty/);

  // Data-driven empty + all sources live ⇒ honest empty.
  const emptyHtml = taskListInner([], ready);
  assert.match(emptyHtml, /class="ctox-empty"/);
  assert.doesNotMatch(emptyHtml, /ctox-syncing/);

  // No readiness API at all (legacy shell) ⇒ falls back to the plain empty.
  assert.match(taskListInner([], base), /class="ctox-empty"/);

  // Filter-empty (rows exist, the filter hides them) is NOT gated: even with
  // an unready source it stays the plain empty.
  const hidden = {
    id: 'task-hidden-1',
    taskId: 'task-hidden-1',
    title: 'Queued review work',
    status: 'queued',
    timestamp: new Date().toISOString(),
  };
  const filteredHtml = taskListInner([hidden], { ...unready, taskSearch: 'no-such-match' });
  assert.match(filteredHtml, /class="ctox-empty"/);
  assert.doesNotMatch(filteredHtml, /ctox-syncing/);
});

test('Readiness wiring stays fail-soft when subscribeCollectionReadiness throws', () => {
  // Reproduces the shell recovery path: openWindowedModule treats any thrown
  // mount error as "CTOX konnte nicht geladen werden". A throwing readiness
  // subscription used to abort mount after the harness markup was already set.
  const throwsClosed = new Error('Business OS module sync context is no longer active.');
  throwsClosed.code = 'CTOX_BUSINESS_OS_MODULE_CONTEXT_CLOSED';
  const state = {
    disposed: false,
    ctx: {
      sync: {
        subscribeCollectionReadiness() {
          throw throwsClosed;
        },
      },
    },
  };

  withoutExpectedWarnings(() => {
    assert.doesNotThrow(() => {
      const cleanup = wireTaskSourceReadiness(state);
      assert.equal(typeof cleanup, 'function');
      cleanup();
    });
  });
});

test('Readiness wiring ignores a listener re-render throw and still unsubscribes', () => {
  let listener = null;
  let unsubscribed = 0;
  const state = {
    disposed: false,
    model: { tasks: [] },
    lang: 'en',
    taskSearch: '',
    taskViewMode: 'cards',
    taskPrimaryView: 'all',
    taskSourceFilter: 'all',
    taskPinFilter: 'all',
    taskSort: 'updated',
    taskSortDirection: 'desc',
    pinnedTaskIds: new Set(),
    ctx: {
      host: {
        // renderTaskList looks for [data-ctox-left] and rebuilds when missing;
        // force the list path to throw by returning a host without queryable panes.
        querySelector() {
          throw new Error('host disconnected during readiness render');
        },
      },
      sync: {
        subscribeCollectionReadiness(_name, next) {
          listener = next;
          // Immediate emit mirrors the live shell facade.
          next({ collection: _name, state: 'live', ready: true, syncing: false });
          return () => { unsubscribed += 1; };
        },
      },
    },
  };

  withoutExpectedWarnings(() => {
    const cleanup = wireTaskSourceReadiness(state);
    assert.equal(typeof listener, 'function');
    assert.doesNotThrow(() => listener({ collection: 'ctox_queue_tasks', state: 'live', ready: true }));
    cleanup();
  });
  assert.ok(unsubscribed >= 1);
});

test('local-dev keeps modify affordances even when role is only user', () => {
  assert.equal(canModifyCtoxApp({
    ctx: { session: { user: { id: 'local-dev', role: 'user', is_admin: false } } },
  }), true);
  assert.equal(canModifyCtoxApp({
    ctx: { user: { id: 'local-dev', role: 'user' } },
  }), true);
  assert.equal(canModifyCtoxApp({
    ctx: { session: { user: { id: 'viewer-1', role: 'user', is_admin: false } } },
  }), false);
  assert.equal(canModifyCtoxApp({
    ctx: {
      canModifyModule: () => true,
      session: { user: { id: 'viewer-1', role: 'user' } },
    },
  }), true);
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
  assert.match(open, /data-webstack-check-projection[^>]*aria-label="Reload access details"[^>]*title="Reload access details"/);
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
  assert.match(markup, /data-context-label="Reference-grade CTOX console"/);
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

test('Task display copy is shown as written (no regex redaction, no underscore mangling)', () => {
  // Slice 5: the operator's own words stay intact; secrets are never projected
  // by the server, so the client has nothing to hide and must not rewrite.
  assert.equal(
    safeTaskDisplayText('Fix src/core/harness_flow.rs for pi-sidecar', 'de'),
    'Fix src/core/harness_flow.rs for pi-sidecar'
  );
  assert.equal(
    safeTaskDisplayText('```js\nconst token = "x";\n```', 'en'),
    '```js const token = "x"; ```'
  );
  assert.equal(safeTaskDisplayText('   ', 'en', { fallback: '–' }), '–');
  assert.equal(safeTaskDisplayText('a'.repeat(400), 'en', { max: 20 }), `${'a'.repeat(19)}...`);
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

test('Authoritative running command lifecycle overrides a stale queued task projection', () => {
  const bundle = mergeBundleWithCommands(
    { runs: [], queue: [], communications: [], tickets: [], tools: [] },
    [{
      id: 'command-runtime-1',
      command_id: 'command-runtime-1',
      contract_version: 2,
      module: 'example-module',
      command_type: 'example.work.execute',
      execution_mode: 'queue',
      execution_task_id: 'queue-runtime-1',
      execution_phase: 'running',
      terminal_status: 'none',
      projection_version: 7,
      status: 'accepted',
      payload: { title: 'Execute example work' },
      updated_at_ms: Date.now(),
    }],
    [{
      id: 'queue-runtime-1',
      command_id: 'command-runtime-1',
      title: 'Execute example work',
      status: 'queued',
      route_status: 'pending',
      module: 'example-module',
      updated_at_ms: Date.now() - 30_000,
    }],
  );

  assert.equal(bundle.queue.length, 1);
  assert.equal(bundle.queue[0].id, 'queue-runtime-1');
  assert.equal(bundle.queue[0].commandId, 'command-runtime-1');
  assert.equal(bundle.queue[0].status, 'running');
  assert.equal(bundle.queue[0].routeStatus, 'running');
  assert.equal(bundle.queue[0].executionPhase, 'running');
});

test('Running command remains active at the running station when flow telemetry is stale', () => {
  const bundle = mergeBundleWithCommands(
    { runs: [], queue: [], communications: [], tickets: [], tools: [] },
    [{
      id: 'command-runtime-2',
      command_id: 'command-runtime-2',
      contract_version: 2,
      module: 'example-module',
      command_type: 'example.work.execute',
      execution_mode: 'queue',
      execution_task_id: 'queue-runtime-2',
      execution_phase: 'running',
      terminal_status: 'none',
      projection_version: 9,
      status: 'accepted',
      payload: { title: 'Run current work' },
      updated_at_ms: Date.now(),
    }],
    [{
      id: 'queue-runtime-2',
      command_id: 'command-runtime-2',
      title: 'Run current work',
      status: 'queued',
      route_status: 'pending',
      module: 'example-module',
      updated_at_ms: Date.now() - 45_000,
    }],
  );
  const staleFlow = {
    ok: true,
    flow: {
      source: { message_key: 'queue-runtime-2', work_id: 'queue-runtime-2' },
      blocks: [{
        kind: 'task',
        title: 'Queued',
        lines: ['Queue projection has not advanced.'],
        branches: [{
          kind: 'queue_pickup',
          title: 'Reload status',
          lines: ['Current queue state: completed'],
        }],
      }],
      ledger_events: [],
    },
  };
  const model = buildHarnessModel(bundle, staleFlow, 'en');
  const health = deriveHarnessHealth({ model, flow: staleFlow, ctx: { sync: { mode: 'webrtc' } } });

  assert.equal(model.activeTask.id, 'queue-runtime-2');
  assert.equal(model.activeNodeId, 'running');
  assert.equal(model.timeline.at(-1).id, 'running');
  assert.equal(model.nodeMap.get('running').status, 'active');
  assert.equal(health.severity, 'ok');
  assert.equal(health.waitingCount, 0);
  assert.equal(health.activeCount, 1);
});

test('Running command without a queue projection is synthesized from its execution link', () => {
  const bundle = mergeBundleWithCommands(
    { runs: [], queue: [], communications: [], tickets: [], tools: [] },
    [{
      id: 'command-runtime-3',
      command_id: 'command-runtime-3',
      contract_version: 2,
      module: 'example-module',
      command_type: 'example.work.execute',
      execution_mode: 'queue',
      execution_task_id: 'queue-runtime-3',
      execution_phase: 'running',
      terminal_status: 'none',
      projection_version: 4,
      status: 'accepted',
      payload: { title: 'Recover projected work' },
      updated_at_ms: Date.now(),
    }],
    [],
  );
  const missingFlow = { ok: false, error: 'rxdb_flow_projection_unavailable' };
  const model = buildHarnessModel(bundle, missingFlow, 'en');

  assert.equal(bundle.queue.length, 1);
  assert.equal(bundle.queue[0].id, 'queue-runtime-3');
  assert.equal(bundle.queue[0].status, 'running');
  assert.equal(model.activeNodeId, 'running');
  assert.deepEqual(model.timeline.map((node) => node.id), ['queued', 'leased', 'running']);
});

test('Synchronous control commands do not become task overview items', () => {
  const bundle = mergeBundleWithCommands(
    { runs: [], queue: [], communications: [], tickets: [], tools: [] },
    [{
      id: 'command-control-1',
      command_id: 'command-control-1',
      contract_version: 2,
      module: 'example-module',
      command_type: 'example.control.refresh',
      execution_mode: 'control',
      execution_phase: 'terminal',
      terminal_status: 'completed',
      status: 'completed',
      updated_at_ms: Date.now(),
    }],
    [],
  );

  assert.deepEqual(bundle.queue, []);
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

// --- Scheibe 3: Live-Daten des gewaehlten Tasks --------------------------------

test('Projected harness events rebuild a ledger-shaped flow for the selected task', () => {
  const task = { id: 'queue-task-1', taskId: 'task-1', commandId: 'cmd-1', status: 'running', routeStatus: 'running' };
  const events = [
    { id: 'e1', task_id: 'task-1', kind: 'phase', title: 'turn started', created_at_ms: 1000 },
    { id: 'e2', task_id: 'task-1', kind: 'tool_started', title: 'read', tool_name: 'read_file', tool_type: 'function', call_id: 'c1', created_at_ms: 2000 },
    { id: 'e3', task_id: 'task-1', kind: 'token_usage', title: 'usage', usage: { input: 1200, output: 300, reasoning: 40, total: 1500 }, created_at_ms: 3000 },
    { id: 'e4', task_id: 'task-1', kind: 'token_usage', title: 'usage', usage: { input: 2400, output: 500, reasoning: 90, total: 2900 }, created_at_ms: 4000 },
    { id: 'e5', task_id: 'task-1', kind: 'crew_selected', title: 'selected: Milo', created_at_ms: 5000 },
  ];
  const flow = harnessFlowFromEvents(task, events);
  assert.equal(flow.ok, true);
  assert.equal(flow.flow.source.message_key, 'task-1');
  assert.equal(flow.flow.ledger_events.length, 5);
  assert.equal(flow.flow.ledger_events[1].event_kind, 'worker.tool_started');
  assert.equal(JSON.parse(flow.flow.ledger_events[1].metadata_json).tool.name, 'read_file');
  assert.equal(JSON.parse(flow.flow.ledger_events[3].metadata_json).metrics_mode, 'cumulative');
  // Cumulative usage takes the maximum, never the sum.
  const metrics = aggregateFlowMetrics(flow);
  assert.equal(metrics.inputTokens, 2400);
  assert.equal(metrics.outputTokens, 500);
  assert.equal(eventToNodeId(flow.flow.ledger_events[4].event_kind, flow.flow.ledger_events[4].title), null);
  assert.equal(harnessFlowFromEvents(task, []), null);
});

test('Live activity from events refreshes only a newer plan and keeps its steps', () => {
  const events = [
    { kind: 'thinking', created_at_ms: 10 },
    { kind: 'tool_started', created_at_ms: 20 },
    { kind: 'tool_completed', created_at_ms: 30 },
    { kind: 'thinking', created_at_ms: 40 },
  ];
  assert.deepEqual(liveActivityFromEvents(events), { total: 3, thinking: 2, tools: 1, last_kind: 'thinking', updated_at_ms: 40 });
  const plan = { phase: 'working', percent: 50, steps: [{ position: 1, label: 'A', status: 'in_progress' }], activity_turns: { total: 1, thinking: 1, tools: 0, last_kind: 'thinking' }, updated_at_ms: 5 };
  const task = { id: 'queue-task-1', taskId: 'task-1', executionProgress: plan };
  const live = { key: 'task-1', events, runs: [] };
  const fresh = withLiveActivity(task, live);
  assert.equal(fresh.executionProgress.activity_turns.total, 3);
  assert.equal(fresh.executionProgress.updated_at_ms, 40);
  assert.equal(fresh.executionProgress.steps.length, 1);
  // Older events never overwrite a newer plan; a foreign key never applies.
  assert.equal(withLiveActivity({ ...task, executionProgress: { ...plan, updated_at_ms: 99 } }, live).executionProgress.updated_at_ms, 99);
  assert.equal(withLiveActivity(task, { ...live, key: 'other' }), task);
});

test('Run metrics sum finished attempts and ignore unknown values', () => {
  assert.equal(aggregateRunMetrics([]), null);
  const runs = [
    { metrics: { input_tokens: 100, output_tokens: 20, tool_calls: 3, thinking_turns: 2, elapsed_ms: 4000 } },
    { metrics: { input_tokens: 50, output_tokens: null, tool_calls: 1, thinking_turns: null, elapsed_ms: 2500 } },
  ];
  assert.deepEqual(aggregateRunMetrics(runs), { inputTokens: 150, outputTokens: 20, toolCalls: 4, thinkingTurns: 2, seconds: 7 });
});

test('Selected task never borrows another task\'s flow', () => {
  const blob = { ok: true, mode: 'ctox_core', flow: { source: { message_key: 'task-other', work_id: null }, ledger_events: [], blocks: [] } };
  const own = harnessFlowFromEvents({ id: 'queue-task-1', taskId: 'task-1' }, [{ id: 'e1', kind: 'thinking', created_at_ms: 1 }]);
  const tasks = [{ id: 'queue-task-1', taskId: 'task-1', commandId: 'cmd-1', status: 'queued', routeStatus: 'queued' }];
  const state = { blobFlow: blob, selectedLive: { key: 'task-1', events: [], runs: [], flow: own }, selectedTaskId: 'queue-task-1', model: { tasks } };
  assert.equal(flowForSelectedTask(state), own);
  state.selectedLive = null;
  assert.equal(flowForSelectedTask(state).ok, false);
  state.blobFlow = { ...blob, flow: { ...blob.flow, source: { message_key: 'task-1', work_id: null } } };
  assert.equal(flowForSelectedTask(state), state.blobFlow);
  // Change events for other tasks do not trigger a rebuild; unknown shapes do.
  assert.equal(changeConcernsSelectedTask(state, { documentData: { task_id: 'task-1' } }), true);
  assert.equal(changeConcernsSelectedTask(state, { documentData: { task_id: 'task-9', command_id: 'cmd-9' } }), false);
  assert.equal(changeConcernsSelectedTask(state, { documentData: { command_id: 'cmd-1' } }), true);
  assert.equal(changeConcernsSelectedTask(state, 'opaque'), true);
});

test('A deep-linked task is consumed once it is on screen', () => {
  const tasks = [
    { id: 'queue-task-a', taskId: 'task-a', commandId: 'cmd-a', status: 'queued', routeStatus: 'queued' },
    { id: 'queue-task-b', taskId: 'task-b', commandId: 'cmd-b', status: 'queued', routeStatus: 'queued' },
  ];
  const state = { model: { tasks, timeline: [], nodeMap: new Map() }, focusTask: { taskId: 'task-b', commandId: '' }, focusTaskConsumed: false, selectedTaskId: null, selectedStepIndex: 0, userNavigatedTimeline: false };
  reconcileSelection(state);
  assert.equal(state.selectedTaskId, 'queue-task-b');
  assert.equal(state.focusTaskConsumed, true);
  // After consumption the operator's own choice survives the next data render.
  state.focusTask = null;
  state.selectedTaskId = 'queue-task-a';
  reconcileSelection(state);
  assert.equal(state.selectedTaskId, 'queue-task-a');
});

// --- Scheibe 4: Wesen = Mitglieder, Crew zu Hause ---------------------------------

const crewFixture = [
  { id: 'crew:milo', name: 'Milo', shape: 'blob', color: '#00aa9a', archived: false, state: 'on_duty', active_task_id: 'task-working' },
  { id: 'crew:nori', name: 'Nori', shape: 'square', color: '#7c6df2', archived: false, state: 'home', active_task_id: null },
  { id: 'crew:tavi', name: 'Tavi', shape: 'triangle', color: '#e97255', archived: false, state: 'resting_after_failure', active_task_id: null },
  { id: 'crew:old', name: 'Old', shape: 'round', color: '#7d7f84', archived: true, state: 'home', active_task_id: null },
];

test('Task creatures carry the crew member identity, unassigned tasks stay neutral', () => {
  const working = { id: 'queue-task-working', taskId: 'task-working', commandId: 'cmd-working', title: 'Working task', status: 'running', routeStatus: 'running', crewMemberId: 'crew:milo', executionProgress: { phase: 'working', percent: 20, steps: [{ position: 1, label: 'A', status: 'in_progress' }] } };
  const orphan = { id: 'queue-task-orphan', taskId: 'task-orphan', commandId: 'cmd-orphan', title: 'Orphan', status: 'running', routeStatus: 'running' };
  const model = { activeTask: working, activeNodeId: 'running', tasks: [working, orphan], nodeMap: new Map([['running', { id: 'running', x: 400, y: 160 }], ['queued', { id: 'queued', x: 100, y: 160 }]]) };
  const state = { lang: 'de', crewMembers: crewFixture, model };
  assert.equal(taskCrewMember(working, state).name, 'Milo');
  assert.equal(taskCrewMember(orphan, state), null);
  assert.deepEqual(memberIdentity(crewFixture[0]), { name: 'Milo', color: '#00aa9a', shape: 'blob' });
  const html = flowCrewSvg(model, working, state);
  assert.match(html, /data-task-id="queue-task-working"[^>]*aria-label="Milo · /);
  assert.match(html, /--crew-color:#00aa9a/);
  assert.match(html, /is-blob/);
  assert.match(html, /data-crew-key="crew:milo:map"/);
  assert.match(html, /data-task-id="queue-task-orphan"[^>]*aria-label="ohne Crew-Zuordnung · /);
});

test('Crew at home shows every active member with its state, only while nothing runs', () => {
  const state = { lang: 'de', crewMembers: crewFixture, model: { liveWork: false, tasks: [{ id: 'queue-task-working', taskId: 'task-working', title: 'Recherche Kunde X', status: 'queued', routeStatus: 'queued' }] } };
  assert.equal(shouldShowCrewHome(state), true);
  assert.equal(shouldShowCrewHome({ ...state, model: { ...state.model, liveWork: true } }), false);
  assert.equal(shouldShowCrewHome({ ...state, crewMembers: [] }), false);
  const html = crewHomeMarkup(state);
  assert.equal((html.match(/data-crew-member-id=/g) || []).length, 3);
  assert.doesNotMatch(html, /crew:old/);
  assert.match(html, /data-crew-member-id="crew:milo"[^>]*aria-label="Milo: Recherche Kunde X"/);
  assert.match(html, /data-crew-member-id="crew:nori"[^>]*aria-label="Nori: zu Hause"/);
  assert.match(html, /data-crew-member-id="crew:tavi"[^>]*aria-label="Tavi: erholt sich nach einem Fehlschlag"/);
  assert.equal(memberCreatureState(crewFixture[0]), 'running');
  assert.equal(memberCreatureState(crewFixture[1]), 'idle');
  assert.equal(memberCreatureState(crewFixture[2]), 'failed');
  // Stamps from the projection: reading right after the memory was read (on
  // duty), learning right after the tick (at home); both decay.
  const now = Date.now();
  assert.equal(memberCreatureState({ ...crewFixture[0], last_memory_read_at_ms: now - 5000 }, now), 'reading');
  assert.equal(memberCreatureState({ ...crewFixture[0], last_memory_read_at_ms: now - 60000 }, now), 'running');
  assert.equal(memberCreatureState({ ...crewFixture[1], last_learning_at_ms: now - 5000 }, now), 'learning');
  assert.equal(memberCreatureState({ ...crewFixture[1], last_learning_at_ms: now - 600000 }, now), 'idle');
  assert.equal(memberCreatureState({ ...crewFixture[2], last_learning_at_ms: now - 5000 }, now), 'failed');
  const reading = crewHomeMarkup({ ...state, crewMembers: [{ ...crewFixture[0], last_memory_read_at_ms: now - 1000 }] });
  assert.match(reading, /aria-label="Milo: liest sein Gedächtnis"/);
  assert.match(reading, /is-reading[\s\S]*?ctox-crew-eyes-reading/);
  // During work the crew stays visible as one row (Review B7): every member,
  // the same drawer hook, the on-duty one carries its task.
  const strip = crewStripMarkup({ ...state, model: { ...state.model, liveWork: true } });
  assert.equal((strip.match(/data-crew-member-id=/g) || []).length, 3);
  assert.match(strip, /class="ctox-crew-strip"/);
  assert.match(strip, /Milo: Recherche Kunde X/);
  assert.equal(crewStripMarkup({ ...state, crewMembers: [] }), '');
  const learning = crewHomeMarkup({ ...state, crewMembers: [{ ...crewFixture[1], last_learning_at_ms: now - 1000 }] });
  assert.match(learning, /aria-label="Nori: lernt aus dem Einsatz"/);
  assert.match(learning, /is-learning[\s\S]*?ctox-crew-eyes-learning/);
  // The expressions are the existing creature modes: working, sleeping, failed (X eyes).
  assert.match(html, /crew:milo[\s\S]*?is-working/);
  assert.match(html, /crew:nori[\s\S]*?is-sleeping/);
  assert.match(html, /crew:tavi[\s\S]*?ctox-crew-eyes-x/);
});

// --- H3: memory documents, field of work, takeover sentence ---------------------

test('Memory documents render one line per entry and confirmation rewrites only that entry', () => {
  const anchors = [
    '# Anchors', '', '## Entries',
    '- anchor_id: a1', '- anchor_type: hypothesis', '- statement: Vor dem Import das Schema prüfen.', '- scope: module=reports', '- source_ref: attempt-1',
    '- anchor_id: a2', '- anchor_type: owner_confirmed', '- statement: Nie ohne Backup migrieren.',
  ].join('\n');
  const entries = memoryEntries(anchors, 'anchor_id', 'anchor_type', 'statement');
  assert.equal(entries.length, 2);
  assert.deepEqual(entries[0], { id: 'a1', tag: 'hypothesis', text: 'Vor dem Import das Schema prüfen.', more: '', scope: 'module=reports', source: 'attempt-1' });
  const confirmed = confirmAnchorBody(anchors, 'a1');
  assert.match(confirmed, /anchor_id: a1\n- anchor_type: owner_confirmed/);
  assert.equal((confirmed.match(/owner_confirmed/g) || []).length, 2);
  assert.equal(confirmAnchorBody(anchors, 'a2'), anchors);
  const narrative = memoryEntries('## Entries\n- entry_id: e1\n- event_type: success\n- summary: Import lief.\n- consequence: Schema zuerst.\n', 'entry_id', 'event_type', 'summary', 'consequence');
  assert.equal(narrative[0].more, 'Schema zuerst.');
});

test('Field of work is derived, and the takeover sentence reads the router event', () => {
  const state = { lang: 'de', selectedLive: { key: 'task-1', events: [
    { kind: 'phase', title: 'x' },
    { kind: 'crew_selected', title: 'routed: Milo (crew:milo): hat zuletzt drei ähnliche Importe sauber abgeschlossen' },
  ] } };
  assert.equal(memberDomainLine({ domain: ['reports', 'imports'], stats: { tasks_total: 14 } }, state), 'Reports, Imports · 14 Einsätze');
  assert.equal(memberDomainLine({ domain: [], stats: { tasks_total: 0 } }, state), 'noch ohne Fachgebiet');
  const task = { id: 'queue-task-1', taskId: 'task-1' };
  assert.equal(taskSelectionSentence(task, state), 'Milo: hat zuletzt drei ähnliche Importe sauber abgeschlossen');
  assert.equal(taskSelectionSentence({ id: 'queue-task-2', taskId: 'task-2' }, state), '');
  state.selectedLive.events[1].title = 'assigned: Manuelle Zuordnung vor dem Lease: Nori (crew:nori)';
  assert.equal(taskSelectionSentence(task, state), 'Nori: Manuelle Zuordnung vor dem Lease');
});
