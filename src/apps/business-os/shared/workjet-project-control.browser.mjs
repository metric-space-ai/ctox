import assert from 'node:assert/strict';
import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { chromium } from 'playwright';

const app = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const tests = readFileSync(new URL('./workjet-project-control.test.mjs', import.meta.url), 'utf8');
const executionSource = readFileSync(new URL('./workjet-supervisor-execution-contract.generated.mjs', import.meta.url), 'utf8').replace(/^export /gm, '');
const kpiSource = readFileSync(new URL('./workjet-project-kpis-contract.generated.mjs', import.meta.url), 'utf8').replace(/^export /gm, '');
const meetingSource = readFileSync(new URL('./workjet-jour-fixe-contract.generated.mjs', import.meta.url), 'utf8').replace(/^export /gm, '');
const lumaSource = readFileSync(new URL('./workjet-supervisor-luma-contract.generated.mjs', import.meta.url), 'utf8').replace(/^export /gm, '');
const meetingCorpus = JSON.parse(readFileSync(new URL('../../../core/rxdb/tests/fixtures/workjet-jour-fixe-v1.json', import.meta.url), 'utf8'));
const meeting = meetingCorpus.valid_cases.find(item => item.type === 'Meeting').value;
const start = app.indexOf('const WORKJET_PROJECT_CONTROL_MAX_RESULTS');
const end = app.indexOf('async function waitForSyncBridgeReady', start);
const fixtureStart = tests.indexOf('function nativeProjectListFixture(');
const fixtureEnd = tests.indexOf("\ntest(", fixtureStart);
const detailsStart = tests.indexOf('function nativeProjectDetailsFixture(');
const detailsEnd = tests.indexOf("\ntest(", detailsStart);
const ownerStart = tests.indexOf('function nativeMeetingOwnerFixture(');
const ownerEnd = tests.indexOf("\ntest(", ownerStart);
const configurationStart = tests.indexOf('function projectConfigurationFixture(');
const configurationEnd = tests.indexOf('function nativeProjectDetailsFixture(', configurationStart);
assert.ok(start >= 0 && end > start && fixtureStart >= 0 && fixtureEnd > fixtureStart);
assert.ok(detailsStart >= 0 && detailsEnd > detailsStart);
assert.ok(ownerStart >= 0 && ownerEnd > ownerStart);
assert.ok(configurationStart >= 0 && configurationEnd > configurationStart);
const output = process.argv.includes('--output-dir')
  ? path.resolve(process.argv[process.argv.indexOf('--output-dir') + 1]) : null;
const browser = await chromium.launch({ headless: true,
  ...(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH
    ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH } : {}) });
try {
  const context = await browser.newContext();
  // A fully intercepted static secure origin supplies the real browser crypto
  // API. Every other request is aborted; no native data travels over HTTP.
  await context.route('**/*', (route) => route.request().url() === 'https://workjet-control.test/'
    ? route.fulfill({ status: 200, contentType: 'text/html', body: '<!doctype html><title>Isolated control fixture</title>' })
    : route.abort());
  const page = await context.newPage();
  await page.goto('https://workjet-control.test/');
  const results = await page.evaluate(async ({ controlSource, fixtureSource, detailsSource, ownerSource, configurationSource, executionSource, kpiSource, meetingSource, lumaSource, meeting }) => {
    const validateSupervisorLumaValue = new Function(lumaSource + '\nreturn validateSupervisorLumaValue;')();
    const assert = {
      ok(value) { if (!value) throw new Error('Expected truthy'); },
      equal(left, right) { if (left !== right) throw new Error(`Expected ${right}, got ${left}`); },
      deepEqual(left, right) { this.equal(JSON.stringify(left), JSON.stringify(right)); },
      match(value, regex) { this.ok(regex.test(value)); },
      fail(message) { throw new Error(message); },
    };
    // Execute the actual app control and the same transport fixture as the
    // Node regressions, using browser Promise/AbortSignal/timer implementations.
    const vm = { runInNewContext(code, scope) {
      scope.invoke = new Function('state', 'actorContext', 'newId', 'AbortController',
        'setTimeout', 'clearTimeout', 'PROJECT_KPIS_SCHEMA', 'validateProjectKpiValue',
        'JOUR_FIXE_SCHEMA', 'validateJourFixeValue', 'validateSupervisorLumaValue', `${controlSource}\nreturn workjetProjectControl;`)(
        scope.state, scope.actorContext, scope.newId, AbortController, setTimeout, clearTimeout,
        scope.PROJECT_KPIS_SCHEMA, scope.validateProjectKpiValue, scope.JOUR_FIXE_SCHEMA, scope.validateJourFixeValue,
        scope.validateSupervisorLumaValue ?? validateSupervisorLumaValue,
      );
    } };
    const fixture = new Function('assert', 'vm', 'controlSource',
      `${fixtureSource}\nreturn nativeProjectListFixture;`)(assert, vm, controlSource);
    const results = [];
    const live = fixture();
    const value = await live.invoke();
    assert.equal(value.projects[0].workingCopies[0].id, 'native-copy');
    assert.equal(value.count, 1);
    assert.equal(value.truncated, false);
    assert.equal(live.reads.length, 2);
    assert.ok(live.reads.every(({ query }) => query.signal.aborted));
    live.rows.workjet_projects[0].supervisor_luma_id = 'luma-physics';
    assert.equal(Object.hasOwn((await live.invoke({ includeConfiguration: true })).projects[0], 'supervisorLumaId'), false);
    assert.equal((await live.invoke({ includeSupervisorLuma: true })).projects[0].supervisorLumaId, 'luma-physics');
    results.push('current native projects and copies without historical pull; Luma metadata is separately opted in');
    live.rows.workjet_projects.length = 0;
    live.rows.workjet_working_copies.length = 0;
    const empty = await live.invoke();
    assert.equal(empty.projects.length, 0);
    assert.equal(empty.count, 0);
    assert.equal(empty.truncated, false);
    assert.ok(live.reads[0].query.requireRevision !== live.reads[2].query.requireRevision);
    results.push('fresh native empty result');
    const replaced = fixture({ exec: (name, query, peer) => {
      peer.generation = 'replacement'; return [];
    } });
    let rejected = false;
    try { await replaced.invoke(); } catch (error) { rejected = /generation changed/.test(error.message); }
    assert.ok(rejected);
    assert.ok(replaced.reads.every(({ query }) => query.signal.aborted));
    results.push('replaced generation rejects and aborts');
    const incomplete = fixture({ exec: () => [] });
    let incompleteRejected = false;
    try { await incomplete.invoke(); } catch (error) {
      incompleteRejected = error.code === 'WORKJET_PROJECT_LIST_INCOMPLETE';
    }
    assert.ok(incompleteRejected);
    assert.ok(incomplete.reads.every(({ query }) => query.signal.aborted));
    results.push('native nonzero count rejects an empty projection');
    const legacy = fixture({ dispatch: (receipt) => {
      delete receipt.result.count;
      return receipt;
    } });
    let legacyRejected = false;
    try { await legacy.invoke(); } catch (error) {
      legacyRejected = error.code === 'WORKJET_PROJECT_LIST_UNCONFIRMED';
    }
    assert.ok(legacyRejected);
    assert.equal(legacy.reads.length, 0);
    results.push('legacy unconfirmed native count rejects before query');
    const originalNow = Date.now;
    let now = 10_000;
    try {
      Date.now = () => now;
      const pending = fixture({
        dispatch: (receipt) => { now = 38_995; return receipt; },
        exec: (name, query) => new Promise((resolve, reject) => {
          query.signal.addEventListener('abort', () => reject(new Error('Aborted')), { once: true });
        }),
      });
      let timedOut = false;
      try { await pending.invoke(); } catch (error) { timedOut = error.code === 'WORKJET_PROJECT_TIMEOUT'; }
      assert.ok(timedOut);
      assert.equal(pending.reads.length, 2);
      assert.ok(pending.reads.every(({ query }) => query.signal.aborted));
      results.push('shared deadline aborts both browser query streams');
    } finally { Date.now = originalNow; }

    const configurationFixture = new Function('vm', 'controlSource', 'validateSupervisorLumaValue',
      `${configurationSource}\nreturn projectConfigurationFixture;`)(vm, controlSource, validateSupervisorLumaValue);
    const configurationRequest = { action: 'project.configure', commandId: 'alias-save',
      projectId: 'project-1', title: 'CTOX', info: { summary: 'Saved via verified alias' } };
    const aliasConfiguration = configurationFixture(() => {}, 'owner@example.org');
    const saved = await aliasConfiguration.invoke(configurationRequest);
    assert.equal(saved.project.info.summary, configurationRequest.info.summary);
    assert.equal(aliasConfiguration.commands[0].client_context.actor.id, 'owner@example.org');
    results.push('browser configuration accepts the native canonical Owner for a verified alias');
    const lumaConfiguration = configurationFixture();
    const lumaSaved = await lumaConfiguration.invoke({ ...configurationRequest, supervisorLumaId: 'luma-physics' });
    assert.equal(lumaSaved.project.supervisorLumaId, 'luma-physics');
    assert.equal(lumaConfiguration.commands[0].payload.supervisor_luma_id, 'luma-physics');
    const lumaCleared = await lumaConfiguration.invoke({ ...configurationRequest, supervisorLumaId: null });
    assert.equal(lumaCleared.project.supervisorLumaId, null);
    assert.equal(lumaConfiguration.commands[1].payload.supervisor_luma_id, null);
    let badLumaRejected = false;
    try { await lumaConfiguration.invoke({ ...configurationRequest, supervisorLumaId: 7 }); }
    catch { badLumaRejected = true; }
    assert.ok(badLumaRejected);
    assert.equal(lumaConfiguration.commands.length, 2);
    results.push('browser selection/clear uses bounded native Luma metadata without a route or producer');
    for (const mutate of [
      receipt => { receipt.result.project.owner_user_id = 'foreign'; },
      receipt => { delete receipt.result.owner_user_id; },
    ]) {
      let denied = false;
      try { await configurationFixture(mutate, 'owner@example.org').invoke(configurationRequest); }
      catch (error) { denied = /uncorrelated/.test(error.message); }
      assert.ok(denied);
    }
    results.push('browser alias configuration rejects mismatched or unconfirmed native owners');

    const turnId = 'actual-native-command';
    const threadId = 'cc6cfe73-2824-4360-9daf-3b3efb079931';
    let corrupt = false;
    const publicText = { turn_id: 'provider-turn', item_id: 'provider-item', phase: 'final_answer',
      offset: 0, text: 'Public reply 🦊', completed: false, truncated: false };
    let lastPayload;
    const state = {
      session: { id: 'owner' }, db: { collection: name => name === 'business_commands' ? {} : null },
      sync: { async startCollection() {} },
      commandBus: { async dispatch(command) {
        lastPayload = command.payload;
        return { command_id: command.id, ok: true, status: 'completed', target_record_id: 'project', payload: command.payload,
          result: { ok: true, contract: 'ctox.workjet.supervisor_turn.v1',
            binding: { project_id: 'project', thread_id: threadId, thread_key: `business-os/threads/${threadId}` },
            turn: { command_id: turnId, task_id: 'actual-native-task', thread_id: threadId,
              thread_key: `business-os/threads/${threadId}`, execution_phase: 'queued', status: 'queued', queue_status: 'pending',
              attempt: 0, terminal: false, result: {}, result_truncated: false },
            execution_contract: 'ctox.workjet.supervisor_execution.v1',
            execution_page: { command_id: turnId, task_id: corrupt ? 'foreign-task' : 'actual-native-task',
              attempt: { attempt_id: 'actual-native-attempt', attempt_index: 47 },
              events: [{ id: 'actual-event', sequence: 22, kind: 'worker.phase', title: 'Recorded step', created_at_ms: 1791410400000,
                ...(command.payload.execution_page?.include_public_text ? { public_text: publicText, kind: 'worker.assistant_text' } : {}) }],
              ...(command.payload.execution_page?.include_public_text ? { public_text_supported: true } : {}),
              next_cursor: { after_sequence: 22, after_event_id: 'actual-event' }, has_more: false },
          } };
      } },
    };
    const invoke = new Function('state', 'actorContext', `${executionSource}\n${controlSource}\nreturn workjetProjectControl;`)(state, session => ({ id: session.id }));
    const request = { action: 'project.supervisor.turn.watch', commandId: 'browser-watch', projectId: 'project', threadId, targetCommandId: turnId };
    const legacyWatch = await invoke(request);
    assert.deepEqual(Object.keys(legacyWatch).sort(), ['action', 'binding', 'commandId', 'contract', 'projectId', 'turn']);
    results.push('legacy watch outer contract remains exact');
    const observed = await invoke({ ...request, executionPage: { limit: 1 } });
    assert.equal(observed.executionContract, 'ctox.workjet.supervisor_execution.v1');
    assert.equal(observed.executionPage.attempt.attempt_id, 'actual-native-attempt');
    assert.equal(observed.executionPage.attempt.attempt_index, 47);
    assert.equal(observed.executionPage.events[0].id, 'actual-event');
    results.push('opted-in browser watch returns actual native attempt and event');
    const streamed = await invoke({ ...request, executionPage: { limit: 1, include_public_text: true } });
    assert.equal(lastPayload.execution_page.include_public_text, true);
    assert.equal(streamed.executionPage.public_text_supported, true);
    assert.deepEqual(streamed.executionPage.events[0].public_text, publicText);
    results.push('browser public-text opt-in reaches native and returns its exact chunk');
    corrupt = true;
    let foreignRejected = false;
    try { await invoke({ ...request, executionPage: {} }); } catch { foreignRejected = true; }
    assert.ok(foreignRejected);
    results.push('foreign native task page fails correlation');

    // Keep generated validators in separate scopes, as in the app's ESM imports.
    const { PROJECT_KPIS_SCHEMA, validateProjectKpiValue } = new Function(`${kpiSource}\nreturn { PROJECT_KPIS_SCHEMA, validateProjectKpiValue };`)();
    const { JOUR_FIXE_SCHEMA, validateJourFixeValue } = new Function(`${meetingSource}\nreturn { JOUR_FIXE_SCHEMA, validateJourFixeValue };`)();
    const detailsFixture = new Function('assert', 'vm', 'controlSource', 'PROJECT_KPIS_SCHEMA',
      'validateProjectKpiValue', 'JOUR_FIXE_SCHEMA', 'validateJourFixeValue',
      `${detailsSource}\nreturn nativeProjectDetailsFixture;`)(assert, vm, controlSource,
      PROJECT_KPIS_SCHEMA, validateProjectKpiValue, JOUR_FIXE_SCHEMA, validateJourFixeValue);
    const details = detailsFixture();
    const detailRequest = { commandId: 'details', projectId: 'project-1' };
    const kpis = await details.invoke({ ...detailRequest, action: 'project.kpis.read' });
    assert.equal(kpis.kpis.revision, 0);
    assert.equal(kpis.kpis.items.length, 0);
    results.push('browser KPI read uses the typed command receipt without a projection pull');
    const configured = await details.invoke({ ...detailRequest, action: 'project.kpis.configure',
      operationId: 'configuration', expectedRevision: 0, prompts: [{ kpi_id: 'visitors', prompt: 'Visitors per week' }] });
    assert.equal(configured.kpis.items[0].result.status, 'missing_source');
    assert.equal(details.commands[1].payload.operation_id, 'configuration');
    const cleared = await details.invoke({ ...detailRequest, action: 'project.kpis.configure',
      operationId: 'clear', expectedRevision: 1, prompts: [] });
    assert.equal(cleared.kpis.revision, 2);
    assert.equal(cleared.kpis.items.length, 0);
    results.push('browser KPI configure and clear preserve the native revision and missing-source result');
    const noMeeting = await details.invoke({ ...detailRequest, action: 'project.jour_fixe.meeting.read' });
    assert.equal(noMeeting.meeting, null);
    const meetingFixture = detailsFixture(receipt => {
      receipt.result.meeting = structuredClone(meeting);
      receipt.result.preparation_task_id = 'actual-preparation';
    });
    const foundMeeting = await meetingFixture.invoke({ ...detailRequest,
      action: 'project.jour_fixe.meeting.read', meetingId: meeting.id });
    assert.equal(foundMeeting.meeting.id, meeting.id);
    assert.equal(foundMeeting.preparationTaskId, 'actual-preparation');
    const foreignMeeting = detailsFixture(receipt => {
      receipt.result.meeting = { ...structuredClone(meeting), project_id: 'foreign' };
    });
    let foreignMeetingRejected = false;
    try { await foreignMeeting.invoke({ ...detailRequest, action: 'project.jour_fixe.meeting.read' }); }
    catch { foreignMeetingRejected = true; }
    assert.ok(foreignMeetingRejected);
    results.push('browser meeting read preserves an actual native binding and rejects foreign scope');

    const ownerControl = new Function('assert', 'vm', 'controlSource', 'JOUR_FIXE_SCHEMA', 'validateJourFixeValue',
      `${ownerSource}\nreturn { fixture: nativeMeetingOwnerFixture, request: meetingOwnerRequest };`)(
      assert, vm, controlSource, JOUR_FIXE_SCHEMA, validateJourFixeValue);
    for (const suffix of ['meeting.start', 'meeting.end', 'transcript.append', 'todos.revise', 'comment.add']) {
      const ownerFixture = ownerControl.fixture();
      const request = ownerControl.request(`project.jour_fixe.${suffix}`);
      const value = await ownerFixture.invoke(request);
      assert.equal(value.mutation.operation_id, request.operationId);
      assert.equal(value.mutation.revision, request.expectedRevision + 1);
      assert.equal(ownerFixture.commands[0].command_type, `ctox.workjet.jour_fixe.${suffix}`);
      assert.equal('meeting' in value, false);
      results.push(`browser Owner ${suffix} uses a compact correlated native mutation receipt`);
    }
    const corruptOwner = ownerControl.fixture(receipt => { receipt.result.mutation.revision = 2; });
    let staleMutationRejected = false;
    try { await corruptOwner.invoke(ownerControl.request()); } catch { staleMutationRejected = true; }
    assert.ok(staleMutationRejected);
    results.push('browser Owner mutation rejects an unconfirmed revision');
    const ownerRequest = ownerControl.request('project.jour_fixe.transcript.append');
    ownerRequest.turn.source_run_id = 'forged-speech-run';
    const forgedOwner = ownerControl.fixture();
    let provenanceRejected = false;
    try { await forgedOwner.invoke(ownerRequest); } catch { provenanceRejected = true; }
    assert.ok(provenanceRejected);
    assert.equal(forgedOwner.commands.length, 0);
    results.push('browser Owner text rejects forged speech provenance before dispatch');

    const confirmRequest = ownerControl.request('project.jour_fixe.todos.confirm');
    const confirmed = await ownerControl.fixture().invoke(confirmRequest);
    assert.equal(confirmed.mutation.state, 'confirmed');
    assert.equal(confirmed.goal.revision, confirmRequest.expectedGoalRevision + 1);
    assert.equal(confirmed.goal.goal_id, confirmed.mutation.changed_id);
    results.push('browser Owner confirmation returns the native Core goal with exact revisions');
    for (const mutate of [r => { r.result.goal.revision += 1; },
      r => { r.result.mutation.changed_id = 'foreign-goal'; },
      r => { r.result.mutation.todos_revision += 1; },
      r => { r.result.mutation.state = 'review'; }]) {
      let rejected = false;
      try { await ownerControl.fixture(mutate).invoke(confirmRequest); } catch { rejected = true; }
      assert.ok(rejected);
    }
    const extraGoal = ownerControl.fixture();
    let forgedGoalRejected = false;
    try { await extraGoal.invoke({ ...confirmRequest, goal: { goal_id: 'forged' } }); } catch { forgedGoalRejected = true; }
    assert.ok(forgedGoalRejected);
    assert.equal(extraGoal.commands.length, 0);
    results.push('browser Owner confirmation rejects substituted proposals and invented goal authority');

    const localAudioRequest = ownerControl.request('project.jour_fixe.narration.local_publish');
    const localAudio = await ownerControl.fixture().invoke(localAudioRequest);
    assert.equal(localAudio.localNarration.audio.file_id, 'persisted-native-audio');
    assert.equal(localAudio.localNarration.provider_verified, false);
    assert.equal(localAudio.mutation.state, 'ready');
    results.push('browser preserves local narration file custody without provider verification');
    for (const mutate of [r => { r.result.local_narration.audio.sha256 = 'c'.repeat(64); },
      r => { r.result.local_narration.provider_verified = true; },
      (r, state) => { state.syncConfig.instance_id = 'biz_other'; }]) {
      let denied = false;
      try { await ownerControl.fixture(mutate).invoke(localAudioRequest); } catch { denied = true; }
      assert.ok(denied);
    }
    results.push('browser rejects corrupted local narration and replaced native instance');

    const localRequest = ownerControl.request('project.jour_fixe.transcript.local_candidate', {
      requestId: 'helper:final:1', deckRevision: 1, text: 'Lokaler Kandidat.',
    });
    const localFixture = ownerControl.fixture();
    const local = await localFixture.invoke(localRequest);
    assert.equal(localFixture.commands[0].payload.instance_id, 'biz_fixture');
    assert.equal(local.localCandidate.request_id, localRequest.requestId);
    assert.equal(local.localCandidate.turn_id, local.mutation.changed_id);
    assert.equal(local.localCandidate.provider_verified, false);
    assert.equal(local.localCandidate.provenance, 'authenticated_owner_local_candidate');
    results.push('browser local candidate hashes exact text and confirms its native stored scope');
    for (const corrupt of [r => { r.result.local_candidate.provider_verified = true; },
      r => { r.result.local_candidate.deck_revision = 2; },
      r => { r.result.local_candidate.request_id = 'other-final'; },
      r => { r.result.local_candidate.text_sha256 = 'a'.repeat(64); }]) {
      let denied = false;
      try { await ownerControl.fixture(corrupt).invoke(localRequest); } catch { denied = true; }
      assert.ok(denied);
    }
    results.push('browser local candidate rejects fake provider and mismatched final receipts');
    let staleLocal = false;
    try { await ownerControl.fixture((receipt, state) => { state.syncConfig.instance_id = 'biz_replaced'; }).invoke(localRequest); }
    catch { staleLocal = true; }
    assert.ok(staleLocal);
    results.push('browser local candidate rejects instance replacement during native wait');
    const oversized = ownerControl.fixture();
    let budgetRejected = false;
    try { await oversized.invoke({ ...localRequest, text: 'é'.repeat(2049) }); }
    catch { budgetRejected = true; }
    assert.ok(budgetRejected);
    assert.equal(oversized.commands.length, 0);
    results.push('browser local candidate UTF-8 byte limit rejects before native dispatch');

    return results;
  }, { controlSource: app.slice(start, end), fixtureSource: tests.slice(fixtureStart, fixtureEnd),
    detailsSource: tests.slice(detailsStart, detailsEnd), ownerSource: tests.slice(ownerStart, ownerEnd),
    configurationSource: tests.slice(configurationStart, configurationEnd), lumaSource,
    executionSource, kpiSource, meetingSource, meeting });
  assert.equal(results.length, 31);
  const report = { passed: results.length, failed: 0, cases: results,
    evidenceScope: 'Actual source control in isolated Chromium with a controlled native contract fixture; not installed native or Workjet UI acceptance',
    browserVersion: browser.version() };
  if (output) { mkdirSync(output, { recursive: true }); writeFileSync(path.join(output, 'result.json'), JSON.stringify(report, null, 2)); }
  console.log(JSON.stringify(report, null, 2));
  await context.close();
} finally { await browser.close(); }
