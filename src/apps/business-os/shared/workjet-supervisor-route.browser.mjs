// Origin: CTOX
// License: AGPL-3.0-only
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { chromium } from 'playwright';

const base = new URL('./', import.meta.url);
const app = readFileSync(new URL('../app.js', base), 'utf8');
const start = app.indexOf('const WORKJET_PROJECT_CONTROL_MAX_RESULTS');
const end = app.indexOf('async function waitForSyncBridgeReady', start);
assert.ok(start >= 0 && end > start);
const spec = JSON.parse(readFileSync(new URL('../../../core/rxdb/tests/fixtures/workjet-supervisor-route-display-v1.json', base)));
const capability = spec.valid_cases.find(c => c.type === 'SupervisorRouteCapabilities').value;
const route = spec.valid_cases.find(c => c.type === 'SupervisorRouteDisplay' && c.value.configured).value;
const assets = new Map([
  ['/shared/workjet-supervisor-route-native.mjs', readFileSync(new URL('./workjet-supervisor-route-native.mjs', base), 'utf8')],
  ['/shared/workjet-supervisor-route-display-contract.generated.mjs', readFileSync(new URL('./workjet-supervisor-route-display-contract.generated.mjs', base), 'utf8')],
]);
const browser = await chromium.launch({ headless: true,
  ...(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH
    ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH } : {}) });
try {
  const context = await browser.newContext();
  await context.route('**/*', async request => {
    const url = new URL(request.request().url());
    if (url.origin !== 'https://supervisor-route.test') return request.abort();
    if (url.pathname === '/') return request.fulfill({
      status: 200, contentType: 'text/html', body: '<title>Isolated Supervisor route</title>',
    });
    const body = assets.get(url.pathname);
    return body === undefined ? request.abort()
      : request.fulfill({ status: 200, contentType: 'text/javascript', body });
  });
  const page = await context.newPage();
  await page.goto('https://supervisor-route.test/');
  const cases = await page.evaluate(async ({ source, capability, route }) => {
    const { requestSupervisorRoute } = await import('/shared/workjet-supervisor-route-native.mjs');
    const actor = { id: 'owner', role: 'chef' };
    const request = { commandId: 'browser-native-route', projectId: route.project_id,
      threadId: route.supervisor_thread_id };
    const results = [];
    for (const action of ['project.supervisor.route.capabilities.v1', 'project.supervisor.route.read.v1']) {
      for (const mutation of ['none', 'scope', 'private', 'session', 'instance', 'sync', 'ready-instance']) {
        const calls = [];
        const value = action.includes('capabilities') ? capability : route;
        const state = { session: actor, db: {}, sync: {}, syncConfig: { instance_id: 'native' },
          commandBus: { dispatch: async command => {
            calls.push(command);
            const receipt = { command_id: command.id, ok: true, status: 'completed',
              target_record_id: command.record_id, payload: command.payload, result: structuredClone(value) };
            if (mutation === 'scope') receipt.result.project_id = 'foreign';
            if (mutation === 'private') receipt.result.native_account = 'private';
            if (mutation === 'session') state.session = { id: 'other' };
            if (mutation === 'instance') state.syncConfig.instance_id = 'other';
            if (mutation === 'sync') state.sync = {};
            return receipt;
          } } };
        const admit = async () => {
          if (mutation === 'ready-instance') state.syncConfig.instance_id = 'other';
          return {};
        };
        const control = new Function('state', 'actorContext', 'requestSupervisorRoute', 'admit',
          source + '\nrequireWorkjetSupervisorDataPlane = admit;\n'
          + 'requireWorkjetProjectDataPlane = () => { throw new Error("wrong admission"); };\n'
          + 'return workjetProjectControl;')(state, s => s, requestSupervisorRoute, admit);
        let succeeded = false;
        try {
          const result = await control({ ...request, action });
          if (result.contract !== value.schema) throw new Error('wrong contract');
          if (action.includes('read') && result.route.actual !== null) throw new Error('unproved producer');
          succeeded = true;
        } catch (error) {
          if (mutation === 'none') throw error;
        }
        if (succeeded !== (mutation === 'none')) throw new Error(action + ': failed ' + mutation);
        if (mutation === 'ready-instance' && calls.length !== 0) throw new Error('retargeted during readiness');
        results.push(action + ': ' + mutation);
      }
    }
    return results;
  }, { source: app.slice(start, end), capability, route });
  assert.equal(cases.length, 14);
  console.log('Supervisor route isolated Chromium: 14 passed; real app control function, simulated session and native receipts. Installed WebRTC acceptance remains separate.');
} finally {
  await browser.close();
}
