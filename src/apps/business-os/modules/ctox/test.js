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
  clampMetric,
  deriveHarnessHealth,
  flowSourceView,
  formatRelativeAge,
  friendlyWebStackStatus,
  labels,
  normalizeFocusTask,
  progressPercent,
  resolveSelectedTaskId,
  safeTaskDisplayText,
  setFlowZoom,
  taskSteps,
  timelinePanel,
  webStackStateFromRefreshResult,
  webStackProjectionMissing,
} = hooks;

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
  assert.match(css, /grid-template-columns: var\(--ctox-left-width\) 6px minmax\(0, 1fr\)/);
  assert.match(manifest, /currentColor/);
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
