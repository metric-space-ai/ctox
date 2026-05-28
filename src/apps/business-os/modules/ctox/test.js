const assert = require('node:assert/strict');
const { Buffer } = require('node:buffer');
const { fileURLToPath } = require('node:url');
const { build } = require('esbuild');

async function importBrowserBundle(relativePath) {
  const bundledModule = await build({
    entryPoints: [fileURLToPath(new URL(relativePath, `file://${__dirname}/`))],
    bundle: true,
    format: 'esm',
    platform: 'browser',
    write: false,
  });

  const [{ text: bundledSource }] = bundledModule.outputFiles;
  return import(`data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`);
}

(async () => {
const { __ctoxTestHooks: hooks } = await importBrowserBundle('./index.js');

const {
  clampMetric,
  deriveHarnessHealth,
  flowSourceView,
  formatRelativeAge,
  friendlyWebStackStatus,
  labels,
  progressPercent,
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
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
