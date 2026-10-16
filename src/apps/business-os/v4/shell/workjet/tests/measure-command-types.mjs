// Messwerkzeug (kein Test): welche Kern-Befehlstypen sendet die Workjet-Brücke je freigeschalteter Aktion wirklich?
// Lauf: node src/apps/business-os/v4/shell/workjet/tests/measure-command-types.mjs
//   Stub-Pfad: CTOX_APP_API_STUB, sonst ~/.local/state/shell-v4/stub-pins/b6624eb.
//   Ausgabe:   ~/.local/state/shell-v4/analysis/wjb-command-types.json (Liste, je Aktion ein Eintrag)
//              ~/.local/state/shell-v4/analysis/wjb-command-types.txt  (typ<TAB>aktionen, sortiert)
//
// Aufbau: createWorkjetBridges(createV2Compat({kernel: adaptKernel(stubKernel)})). Die App-API des Kernels ist
// umhüllt: jeder api.command(type, payload) wird mit der auslösenden Aktion (AsyncLocalStorage) und dem Weg
// (v2-compat dispatch | requestNative | E26-Sitzungsbefehl, aus dem Aufrufstapel) protokolliert. Der Stub beantwortet
// Befehle erfolgreich; für Befehle mit geprüfter Antwortform liefert RESPOND eine Antwort nach dem Orakel/Prüfer.
// SITES ist die statische Gegenliste (Befehlsstellen je Aktion aus dem Quelltext, file:line wird zur Laufzeit
// aufgelöst). Fehlt einer Aktion ein erwarteter Befehl in der Messung, steht der Eintrag als source "static" mit Beleg.
import { AsyncLocalStorage } from 'node:async_hooks';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { adaptKernel } from '../../kernel-adapter.js';
import { createWorkjetBridges, INVENTORY, inventoryList } from '../index.js';
import { createV2Compat } from '../v2-compat.js';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const WJ = path.resolve(HERE, '..');
const STUB_DIR = process.env.CTOX_APP_API_STUB || path.join(os.homedir(), '.local/state/shell-v4/stub-pins/b6624eb');
const OUT_DIR = path.join(os.homedir(), '.local/state/shell-v4/analysis');
const OUT_JSON = path.join(OUT_DIR, 'wjb-command-types.json');
const OUT_TXT = path.join(OUT_DIR, 'wjb-command-types.txt');
const ACTION_TIMEOUT_MS = 5_000;
const { createStubKernel } = await import(pathToFileURL(path.join(STUB_DIR, 'ctox-app-api-stub.mjs')).href);
process.on('unhandledRejection', () => {}); // abgebrochene Aktionen laufen im Hintergrund aus

// ---- Beleg-Auflösung: file:line aus dem Quelltext ----------------------------------------------------------------
const sourceCache = new Map();
function lineOf(file, anchor, nth = 1) {
  if (!sourceCache.has(file)) sourceCache.set(file, fs.readFileSync(path.join(WJ, file), 'utf8').split('\n'));
  const lines = sourceCache.get(file);
  let seen = 0;
  for (let i = 0; i < lines.length; i += 1) {
    if (lines[i].includes(anchor) && (seen += 1) === nth) return `${file}:${i + 1}`;
  }
  throw new Error(`Anker nicht gefunden: ${file} „${anchor}“ (#${nth})`);
}
const site = (type, refs) => ({ type, evidence: refs.map(([file, anchor, nth]) => lineOf(file, anchor, nth)).join('; ') });

// ---- statische Gegenliste: Befehlsstellen je freigeschalteter Aktion ---------------------------------------------
const P = 'actions/project.js', S = 'actions/supervisor.js', M = 'actions/meeting.js', I = 'actions/instance.js';
const C = 'actions/computer.js', SE = 'actions/session.js', D = 'actions/device.js', CC = 'actions/calendar-connect.js';
const SITES = {
  'project.list': [site('ctox.workjet.project.list', [[P, "command_type: 'ctox.workjet.project.list'"]])],
  'project.worker.add': [site('ctox.workjet.project.worker.add', [[P, 'command_type: creatingChat ?']])],
  'project.chat.create': [site('ctox.workjet.project.chat.create', [[P, 'command_type: creatingChat ?']])],
  'project.create': [
    site('ctox.workjet.project.upsert', [[P, "command_type: 'ctox.workjet.project.upsert'", 1]]),
    site('ctox.workjet.working_copy.upsert', [[P, "command_type: 'ctox.workjet.working_copy.upsert'"]]),
  ],
  'project.configure': [site('ctox.workjet.project.upsert', [[P, "command_type: 'ctox.workjet.project.upsert'", 2]])],
  'project.init': [site('ctox.workjet.project.init', [[P, "command_type: 'ctox.workjet.project.init'"]])],
  // Strang R (E26 / K-WJ-2): content.read liest das Manifest per Befehl, die Bytes per files.get (kein Befehl)
  'project.presentation.read': [site('ctox.workjet.presentation.read', [[P, "'ctox.workjet.presentation.read', payload,"]])],
  'project.presentation.content.read': [site('ctox.workjet.presentation.read', [[P, "'ctox.workjet.presentation.read', payload,"]])],
  'project.presentation.canvas.save': [site('ctox.workjet.presentation.canvas.save', [[P, "'ctox.workjet.presentation.canvas.save',"]])],
  ...Object.fromEntries(['read', 'configure', 'refresh'].map((v) => [`project.kpis.${v}`,
    [site(`ctox.workjet.project.kpis.${v}`, [[P, 'command_type: `ctox.workjet.${action}`']])]])),
  ...Object.fromEntries(['read', 'refresh', 'submit'].map((v) => [`project.exit_model.${v}`,
    [site(`ctox.workjet.exit_model.${v}`, [[P, "'ctox.workjet.exit_model.' + verb"]])]])),
  'project.supervisor.bind': [site('ctox.workjet.project.supervisor.bind', [[S, "command_type: 'ctox.workjet.project.supervisor.bind'"]])],
  ...Object.fromEntries(['submit', 'input', 'cancel', 'watch', 'capabilities'].map((v) => [`project.supervisor.turn.${v}`,
    [site(`ctox.workjet.project.supervisor.turn.${v}`, [[S, `'project.supervisor.turn.${v}': 'ctox.workjet.project.supervisor.turn.${v}'`]])]])),
  ...Object.fromEntries(['read.v1', 'read.v2', 'capabilities.v1', 'capabilities.v2'].map((v) => [`project.supervisor.route.${v}`,
    [site(`ctox.workjet.project.supervisor.route.${v}`, [[S, "const commandType = 'ctox.workjet.' + request.action"]])]])),
  'ctox.plan.step.retry': [site('ctox.plan.step.retry', [[S, "command_type: 'ctox.plan.step.retry'"]])],
  'project.jour_fixe.meeting.read': [site('ctox.workjet.jour_fixe.meeting.read', [[M, "command_type: 'ctox.workjet.jour_fixe.meeting.read'"]])],
  // Strang R: narration.read liest die Besprechung über meeting.read (zweimal), die Bytes per files.get (kein Befehl)
  'project.jour_fixe.narration.read': [site('ctox.workjet.jour_fixe.meeting.read', [[M, "command_type: 'ctox.workjet.jour_fixe.meeting.read'"],
    [M, "action: 'project.jour_fixe.meeting.read', commandId: env.newId(),"]])],
  ...Object.fromEntries(['meeting.start', 'meeting.end', 'todos.revise', 'todos.confirm', 'comment.add', 'transcript.append'].map((v) => [
    `project.jour_fixe.${v}`, [site(`ctox.workjet.jour_fixe.${v}`, [[M, "command_type: `ctox.workjet.${action.slice('project.'.length)}`"]])]])),
  'project.jour_fixe.speech': [site('ctox.workjet.jour_fixe.speech.v1', [[M, "export const JOUR_FIXE_SPEECH_METHOD"],
    [M, 'sync.requestNative(JOUR_FIXE_SPEECH_METHOD'], [M, 'const handle = api.command(method']])],
  'project.calendar.accounts.read': [site('ctox.workjet.calendar.read.v1', [[M, 'export const CALENDAR_READ_METHOD'], [M, 'sync.requestNative(CALENDAR_READ_METHOD']])],
  'project.calendar.events.read': [site('ctox.workjet.calendar.read.v1', [[M, 'export const CALENDAR_READ_METHOD'], [M, 'sync.requestNative(CALENDAR_READ_METHOD']])],
  ...Object.fromEntries(['start', 'complete'].map((v) => [`project.calendar.account.connect.${v}`,
    [site('ctox.workjet.calendar.connect.v1', [[CC, 'export const CALENDAR_CONNECT_METHOD'], [CC, 'sync.requestNative(CALENDAR_CONNECT_METHOD']])]])),
  'speech.dictation': [site('ctox.workjet.speech.dictation.v1', [[M, 'export const DICTATION_METHOD'],
    [M, 'sync.requestNative(DICTATION_METHOD'], [M, 'const handle = api.command(method']])],
  ...Object.fromEntries(['read', 'configure', 'key', 'playback', 'voices', 'check', 'check.transcription'].map((v) => [`speech.settings.${v}`,
    [site('ctox.workjet.speech.settings.v1', [[M, "sync.requestNative('ctox.workjet.speech.settings.v1'"]])]])),
  ...Object.fromEntries(['read', 'start', 'poll', 'cancel', 'check', 'remove'].map((v) => [`instance.grok.${v}`,
    [site('ctox.workjet.grok.v1', [[I, "export const GROK_METHOD = 'ctox.workjet.grok.v1'"], [I, 'sync.requestNative(GROK_METHOD']])]])),
  ...Object.fromEntries([
    ['read', 'ctox.workjet.providers.list'], ['adopt', 'ctox.workjet.providers.adopt_native'],
    ['observe', 'ctox.workjet.providers.observe_native'], ['models.select', 'ctox.workjet.providers.models.select'],
    ['models.exclude', 'ctox.workjet.providers.models.exclude'], ['account.enable', 'ctox.workjet.providers.account.enable'],
    ['account.remove', 'ctox.workjet.providers.account.remove'],
  ].map(([v, t]) => [`instance.providers.${v}`, [site(t, [[I, `['instance.providers.${v}', '${t}']`], [I, "dispatch(command, { until: 'terminal', sync_queue_tasks: false, timeoutMs })"]])]])),
  // E32: computer.list und session.list sind lokale Kernel-Abfragen (kein Befehl); Beleg = Abfragestelle im Quelltext.
  'computer.list': [],
  'computer.assign': [site('ctox.workjet.computer.assign', [[C, "command_type: 'ctox.workjet.computer.assign'"]])],
  'computer.unassign': [site('ctox.workjet.computer.unassign', [[C, "command_type: 'ctox.workjet.computer.unassign'"]])],
  'computer.endpoint.upsert': [site('ctox.workjet.computer.endpoint.upsert', [[C, "? 'ctox.workjet.computer.endpoint.upsert'"]])],
  'computer.endpoint.disable': [site('ctox.workjet.computer.endpoint.disable', [[C, ": 'ctox.workjet.computer.endpoint.disable'"]])],
  'computer.ssh_key.ensure': [site('ctox.workjet.computer.ssh_key.ensure', [[C, "command_type: 'ctox.workjet.computer.ssh_key.ensure'"]])],
  'session.list': [],
  ...Object.fromEntries(['create', 'transfer.start', 'transfer.status', 'transfer.abort', 'transfer.pause_ack'].map((v) => [`session.${v}`,
    [site(`ctox.workjet.session.${v}`, [[SE, `'ctox.workjet.session.${v}'`]])]])),
  ...Object.fromEntries(['invite.create', 'invite.revoke', 'binding.list', 'binding.revoke'].map((a) => [a,
    [site('ctox.workjet.device.v1', [[D, "env.state.sync.requestNative('ctox.workjet.device.v1'"]])]])),
};

// E32: Leseaktionen, die als lokale Kernel-Abfrage laufen, mit Abfragestelle (Collection) und dem entfallenen Kern-Typ.
const LOCAL_QUERY = {
  'computer.list': { collection: 'workjet_computers', dropped: 'ctox.workjet.computer.list',
    evidence: [[C, "awaitLocalReadView(env, 'workjet_computers'"], [C, 'async function listProjectedWorkjetComputers']] },
  'session.list': { collection: 'workjet_sessions', dropped: 'ctox.workjet.session.list',
    evidence: [[SE, "awaitLocalReadView(env, 'workjet_sessions'"], [SE, 'async function listProjectedWorkjetSessions']] },
};
for (const q of Object.values(LOCAL_QUERY)) q.where = q.evidence.map(([file, anchor]) => lineOf(file, anchor)).join('; ');
// E32: die 16 gemessenen Lesetypen vor E32 (Stand wjb-command-types.txt nach WJB-R) – Gegenliste für „entfällt/bleibt“.
const READ_TYPES_BEFORE_E32 = [
  'ctox.workjet.calendar.read.v1', 'ctox.workjet.computer.list', 'ctox.workjet.exit_model.read', 'ctox.workjet.jour_fixe.meeting.read',
  'ctox.workjet.presentation.read', 'ctox.workjet.project.kpis.read', 'ctox.workjet.project.list',
  'ctox.workjet.project.supervisor.route.capabilities.v1', 'ctox.workjet.project.supervisor.route.capabilities.v2',
  'ctox.workjet.project.supervisor.route.read.v1', 'ctox.workjet.project.supervisor.route.read.v2',
  'ctox.workjet.project.supervisor.turn.capabilities', 'ctox.workjet.project.supervisor.turn.watch', 'ctox.workjet.providers.list',
  'ctox.workjet.session.list', 'ctox.workjet.session.transfer.status',
];

// ---- gültige Anfragen je Aktion (Beispiele aus den V2-Tests e35a116ae und den Orakeln) ---------------------------
const U = (n) => `00000000-0000-4000-8000-${String(n).padStart(12, '0')}`;
const THREAD = '11111111-2222-4333-8444-555555555555';
const STREAM = 'aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee';
const ISO = '2026-10-16T08:00:00.000Z';
const meetingOwner = (action, extra = {}) => ({ action, commandId: `m-${action}`, projectId: 'p-1', operationId: 'operation-1',
  meetingId: 'meeting-1', expectedRevision: 3, ...extra });
const jf = (extra) => ({ action: 'project.jour_fixe.speech', projectId: 'p-1', meetingId: 'meeting-1', deckRevision: 1, ...extra });
const REQUESTS = {
  'project.list': [{ action: 'project.list' }],
  'project.worker.add': [{ action: 'project.worker.add', commandId: 'm-wa', projectId: 'p-1', workerProfileId: 'w-1', createdAt: ISO }],
  'project.chat.create': [{ action: 'project.chat.create', commandId: 'm-cc', projectId: 'p-1', workerProfileId: 'w-1', createdAt: ISO, title: 'Chat' }],
  'project.create': [{ action: 'project.create', commandId: 'm-cr', projectId: 'p-new', title: 'Neu', createdAt: ISO,
    workingCopy: { computerId: 'mac', path: '/src/neu' } }],
  'project.configure': [{ action: 'project.configure', commandId: 'm-cf', projectId: 'p-1', title: 'Projekt p-1' }],
  'project.init': [{ action: 'project.init', commandId: 'm-in', projectId: 'p-1', operationId: 'op-1' }],
  'project.kpis.read': [{ action: 'project.kpis.read', commandId: 'm-kr', projectId: 'p-1' }],
  'project.kpis.refresh': [{ action: 'project.kpis.refresh', commandId: 'm-kf', projectId: 'p-1' }],
  'project.kpis.configure': [{ action: 'project.kpis.configure', commandId: 'm-kc', projectId: 'p-1', operationId: 'operation-1',
    expectedRevision: 0, prompts: [{ kpi_id: 'visitors', prompt: 'Visitors per week' }] }],
  'project.exit_model.read': [{ action: 'project.exit_model.read', commandId: 'm-er', projectId: 'p-1' }],
  'project.exit_model.refresh': [{ action: 'project.exit_model.refresh', commandId: 'm-ef', projectId: 'p-1', asOf: '2026-10-08',
    resources: { hoursPerWeek: 20, monthlyBudgetEur: 300, comparisonMode: 'equal_resources' } }],
  'project.exit_model.submit': [{ action: 'project.exit_model.submit', commandId: 'm-es', projectId: 'p-1', confirmRunId: 'proposal-1' },
    { action: 'project.exit_model.submit', commandId: 'm-es2', projectId: 'p-1', inputs: {} }],
  'project.supervisor.bind': [{ action: 'project.supervisor.bind', commandId: 'm-sb', projectId: 'p-1', threadId: THREAD }],
  'project.supervisor.turn.submit': [{ action: 'project.supervisor.turn.submit', commandId: 'm-ts', projectId: 'p-1', threadId: THREAD, goal: 'Ziel', turnKind: 'work' }],
  'project.supervisor.turn.input': [{ action: 'project.supervisor.turn.input', commandId: 'm-ti', projectId: 'p-1', threadId: THREAD, targetCommandId: 'm-ts', body: 'Weiter' }],
  'project.supervisor.turn.cancel': [{ action: 'project.supervisor.turn.cancel', commandId: 'm-tc', projectId: 'p-1', threadId: THREAD, targetCommandId: 'm-ts', reason: 'Stopp' }],
  'project.supervisor.turn.watch': [{ action: 'project.supervisor.turn.watch', commandId: 'm-tw', projectId: 'p-1', threadId: THREAD, targetCommandId: 'm-ts' }],
  'project.supervisor.turn.capabilities': [{ action: 'project.supervisor.turn.capabilities', commandId: 'm-tk', projectId: 'p-1', threadId: THREAD, includeInput: true }],
  ...Object.fromEntries(['read.v1', 'read.v2', 'capabilities.v1', 'capabilities.v2'].map((v) => [`project.supervisor.route.${v}`,
    [{ action: `project.supervisor.route.${v}`, commandId: `m-r-${v}`, projectId: 'p-1', threadId: THREAD }]])),
  'ctox.plan.step.retry': [{ action: 'ctox.plan.step.retry', commandId: 'm-retry', payload: { step_id: 'step-1' } }],
  'project.jour_fixe.meeting.read': [{ action: 'project.jour_fixe.meeting.read', commandId: 'm-mr', projectId: 'p-1' }],
  // Strang R (Beispiele aus shared/jour-fixe-presentation.test.mjs und jour-fixe-narration.test.mjs @ e35a116ae)
  'project.presentation.read': [{ action: 'project.presentation.read', commandId: 'm-pr', projectId: 'p-1', meetingId: 'meeting-1' }],
  'project.presentation.content.read': [{ action: 'project.presentation.content.read', commandId: 'm-pc', projectId: 'p-1', meetingId: 'meeting-1',
    presentationId: 'workjet_presentation_1', revision: 3, offset: 0, length: 4096 }],
  'project.presentation.canvas.save': [{ action: 'project.presentation.canvas.save', commandId: 'm-ps', projectId: 'p-1', meetingId: 'meeting-1',
    operationId: 'op-1', presentationId: 'workjet_presentation_1', expectedRevision: 3, slideId: 's1', sceneJson: '{"elements":[]}' }],
  'project.jour_fixe.narration.read': [{ action: 'project.jour_fixe.narration.read', commandId: 'm-nr', projectId: 'p-1', meetingId: 'meeting-1',
    deckRevision: 1, slideId: 'slide-1', offset: 0, length: 256 }],
  'project.jour_fixe.meeting.start': [meetingOwner('project.jour_fixe.meeting.start')],
  'project.jour_fixe.meeting.end': [meetingOwner('project.jour_fixe.meeting.end')],
  'project.jour_fixe.transcript.append': [meetingOwner('project.jour_fixe.transcript.append', { turn: {
    id: 'owner-text', sequence: 1, speaker: 'owner', modality: 'text', text: 'Please verify persistence.',
    started_at_ms: 1000, ended_at_ms: 1001, meeting_id: 'meeting-1' } })],
  'project.jour_fixe.comment.add': [meetingOwner('project.jour_fixe.comment.add', { commentId: 'comment-1', slideId: 'slide-1',
    deckRevision: 1, x: 0.25, y: 0.75, text: 'Please prioritize persistence.' })],
  'project.jour_fixe.todos.confirm': [meetingOwner('project.jour_fixe.todos.confirm', { proposalRevision: 2, expectedGoalRevision: 0 })],
  'project.jour_fixe.todos.revise': [meetingOwner('project.jour_fixe.todos.revise', { proposalRevision: 2, items: [{
    id: 'todo-1', title: 'Verify persistence', acceptance: 'Save survives reopen', priority: 'P1', evidence_ids: ['owner-text'] }] })],
  // Sprachsitzung: open (E26-Sitzungsbefehl) → write/finish (requestNative mit session_command_id); read sendet nichts.
  'project.jour_fixe.speech': [jf({ op: 'open', requestId: U(1) }), jf({ op: 'write', streamId: STREAM, sequence: 1, pcmBase64: 'AAAA' }),
    jf({ op: 'read', streamId: STREAM, afterSequence: 0 }), jf({ op: 'finish', streamId: STREAM })],
  'project.calendar.accounts.read': [{ action: 'project.calendar.accounts.read', commandId: 'read-1' }],
  'project.calendar.account.connect.start': [{ action: 'project.calendar.account.connect.start', commandId: 'conn-1', provider: 'google' }],
  'project.calendar.account.connect.complete': [{ action: 'project.calendar.account.connect.complete', commandId: 'conn-2', attemptId: 'att-1', callbackUrl: 'https://calendar.example.test/cb?code=x&state=y' }],
  'project.calendar.events.read': [{ action: 'project.calendar.events.read', commandId: 'events-1', accountId: 'mine', startMs: 1, endMs: 2 }],
  'speech.dictation': [
    { action: 'speech.dictation', commandId: U(11), op: 'open' },
    { action: 'speech.dictation', commandId: U(12), op: 'write', streamId: STREAM, sequence: 1, pcmBase64: 'AAAA' },
    { action: 'speech.dictation', commandId: U(13), op: 'read', streamId: STREAM, afterSequence: 0 },
    { action: 'speech.dictation', commandId: U(14), op: 'finish', streamId: STREAM },
    { action: 'speech.dictation', commandId: U(15), op: 'open' },
    { action: 'speech.dictation', commandId: U(16), op: 'cancel', streamId: STREAM },
  ],
  ...Object.fromEntries(['read', 'configure', 'key', 'playback', 'voices', 'check', 'check.transcription'].map((v, i) => [`speech.settings.${v}`,
    [{ action: `speech.settings.${v}`, commandId: U(20 + i), ...(v === 'key' ? { secret: 'private-fixture' } : {}) }]])),
  ...Object.fromEntries(['read', 'start', 'poll', 'cancel', 'check', 'remove'].map((v, i) => [`instance.grok.${v}`,
    [{ version: 1, action: `instance.grok.${v}`, operationId: U(30 + i) }]])),
  'instance.providers.read': [{ version: 1, action: 'instance.providers.read', operationId: U(40) }],
  'instance.providers.adopt': [{ version: 1, action: 'instance.providers.adopt', operationId: U(41) }],
  'instance.providers.observe': [{ version: 1, action: 'instance.providers.observe', operationId: U(42), accountId: 'acc-1', expectedAccountRevision: 1 }],
  'instance.providers.models.select': [{ version: 1, action: 'instance.providers.models.select', operationId: U(43), provider: 'openai', models: ['m-1'], expectedRevision: 0 }],
  'instance.providers.models.exclude': [{ version: 1, action: 'instance.providers.models.exclude', operationId: U(44), accountId: 'acc-1', expectedAccountRevision: 1, models: ['m-1'], expectedRevision: 0 }],
  'instance.providers.account.enable': [{ version: 1, action: 'instance.providers.account.enable', operationId: U(45), accountId: 'acc-1', expectedAccountRevision: 1, expectedRevision: 0, enabled: true }],
  'instance.providers.account.remove': [{ version: 1, action: 'instance.providers.account.remove', operationId: U(46), accountId: 'acc-1', expectedAccountRevision: 1, expectedRevision: 0 }],
  'computer.list': [{ action: 'computer.list' }],
  'computer.assign': [{ action: 'computer.assign', commandId: 'c-as', computerId: 'mac', displayName: 'Mac', hostingMode: 'workstation' }],
  'computer.unassign': [{ action: 'computer.unassign', commandId: 'c-un', computerId: 'mac' }],
  'computer.endpoint.upsert': [{ action: 'computer.endpoint.upsert', commandId: 'c-eu', computerId: 'mac', endpointRef: 'ep-1',
    connection: { protocol: 'ssh', host: 'host', port: 22, username: 'user', root: '/', host_key_sha256: 'SHA256:fixture',
      private_key: { scope: 'computer-access', name: 'fixture-key' } } }],
  'computer.endpoint.disable': [{ action: 'computer.endpoint.disable', commandId: 'c-ed', endpointRef: 'ep-1' }],
  'computer.ssh_key.ensure': [{ action: 'computer.ssh_key.ensure', commandId: 'c-sk', computerId: 'mac' }],
  'session.list': [{ action: 'session.list' }],
  'session.create': [{ action: 'session.create', commandId: 's-cr', projectId: 'p-1', workingCopyId: 'wc-1' }],
  'session.transfer.start': [{ action: 'session.transfer.start', commandId: 's-ts', sessionId: 's-1', targetComputerId: 'mac', targetPath: '/src/x', idempotencyKey: 'i-1' }],
  'session.transfer.status': [{ action: 'session.transfer.status', commandId: 's-tt', transferId: 't-1' }],
  'session.transfer.abort': [{ action: 'session.transfer.abort', commandId: 's-ta', transferId: 't-1', reason: 'Abbruch', idempotencyKey: 'i-2' }],
  'session.transfer.pause_ack': [{ action: 'session.transfer.pause_ack', commandId: 's-tp', transferId: 't-1', computerId: 'mac', fenceEpoch: 0, gitRepository: true, idempotencyKey: 'i-3' }],
  'invite.create': [{ action: 'invite.create', ttlSeconds: 600, displayName: 'Gerät' }],
  'invite.revoke': [{ action: 'invite.revoke', inviteId: 'inv-1' }],
  'binding.list': [{ action: 'binding.list' }],
  'binding.revoke': [{ action: 'binding.revoke', bindingId: 'b-1' }],
};

// ---- Antworten des Stubs je Befehlstyp (Orakel-/Prüferformen) -----------------------------------------------------
let kernel = null;
const OWNER = 'owner-1';
const projectRow = (id, name) => ({ id, name, status: 'active', owner_user_id: OWNER, created_at_ms: 1_760_000_000_000 });
const meetingMutationResult = (type, p, recordId) => {
  const s = type.slice('ctox.workjet.jour_fixe.'.length);
  const mutation = { operation_id: p.operation_id, meeting_id: p.meeting_id, project_id: recordId, revision: p.expected_revision + 1,
    state: s === 'todos.confirm' ? 'confirmed' : s === 'meeting.start' || s === 'transcript.append' ? 'live' : 'review',
    ...(s === 'transcript.append' ? { changed_id: p.turn.id } : {}), ...(s === 'comment.add' ? { changed_id: p.comment_id } : {}),
    ...(s === 'todos.revise' ? { todos_revision: p.proposal_revision } : {}) };
  const out = { ok: true, contract: 'ctox.workjet.jour_fixe.v1', mutation };
  if (s === 'todos.confirm') {
    out.goal = { goal_id: 'goal-confirmed', revision: p.expected_goal_revision + 1 };
    mutation.changed_id = out.goal.goal_id;
    mutation.todos_revision = p.proposal_revision;
  }
  return out;
};
const dictationResult = (p) => ({ action: 'speech.dictation', commandId: p.commandId, op: p.op, streamId: STREAM,
  state: p.op === 'finish' ? 'finished' : p.op === 'cancel' ? 'canceled' : 'open', events: [],
  text: p.op === 'finish' ? 'Diktat.' : null, error: null });
const jourFixeSpeechResult = (p) => ({ streamId: STREAM, state: p.op === 'finish' ? 'finished' : 'open', events: [], receipt: null, error: null,
  ...(p.op === 'open' ? { requestId: p.requestId } : {}) });
const SESSION_OPEN = { 'ctox.workjet.speech.dictation.v1': dictationResult, 'ctox.workjet.jour_fixe.speech.v1': jourFixeSpeechResult };

async function respond(type, p, recordId) {
  if (type === 'ctox.workjet.project.list') return { ok: true, collection: 'workjet_projects', owner_user_id: OWNER, count: 0, truncated: false, project_ids: [] };
  if (type === 'ctox.workjet.project.upsert') {
    await kernel.sim.push('workjet_projects', [projectRow(p.project_id, p.name)]);
    return { ok: true, collection: 'workjet_projects', owner_user_id: OWNER, project: projectRow(p.project_id, p.name) };
  }
  if (type === 'ctox.workjet.working_copy.upsert') {
    await kernel.sim.push('workjet_working_copies', [{ id: `wc-${p.project_id}`, project_id: p.project_id, computer_id: p.computer_id,
      path: p.path, status: 'active', owner_user_id: OWNER }]);
    return { ok: true };
  }
  if (type === 'ctox.workjet.project.worker.add') return { ok: true, contract: 'workjet-project-chats.v1', first_chat_id: 'workjet_private_measure_1' };
  if (type === 'ctox.workjet.project.chat.create') return { ok: true, contract: 'workjet-project-chats.v1', chat_id: 'workjet_private_measure_2' };
  if (type === 'ctox.workjet.project.init') return { ok: true, project_id: p.project_id, steps: [] };
  if (type.startsWith('ctox.workjet.project.kpis.')) return { ok: true, kpis: { project_id: p.project_id, revision: 1, items: [] }, refresh: { completed: true, items: [] } };
  if (type.startsWith('ctox.workjet.exit_model.')) return { ok: true, assessment: { contract: 'ctox.workjet.exit_model.v1', project_id: p.project_id } };
  if (type === 'ctox.workjet.project.supervisor.bind') return { ok: true, contract: 'ctox.workjet.supervisor_binding.v1',
    binding: { project_id: p.project_id, thread_id: p.thread_id, thread_key: `business-os/threads/${p.thread_id}` } };
  if (type === 'ctox.workjet.jour_fixe.meeting.read') return { ok: true, meeting: null };
  // Strang R: Manifest null (keine Präsentation) bzw. bestätigte Speicherung
  if (type === 'ctox.workjet.presentation.read') return { ok: true, contract: 'ctox.workjet.presentation.v1', presentation: null };
  if (type === 'ctox.workjet.presentation.canvas.save') {
    const manifest = { presentation_id: p.presentation_id, project_id: recordId, meeting_id: 'meeting-1', owner_user_id: OWNER, title: 'Regeltermin',
      revision: p.expected_revision + 1, document_schema: 'learnordie.slide.v1', document_file_id: 'deck-file', document_generation_id: 'deck-generation',
      document_sha256: 'a'.repeat(64), document_bytes: 64, slide_ids: [p.slide_id], source: 'owner', updated_by: OWNER, updated_at_ms: 1 };
    return { ok: true, contract: 'ctox.workjet.presentation.v1', presentation: manifest, mutation: { operation_id: p.operation_id,
      presentation_id: p.presentation_id, project_id: recordId, meeting_id: 'meeting-1', revision: p.expected_revision + 1,
      document_sha256: manifest.document_sha256, document_bytes: 64, slide_ids: manifest.slide_ids } };
  }
  if (/^ctox\.workjet\.jour_fixe\.(meeting\.(start|end)|transcript\.append|comment\.add|todos\.(revise|confirm))$/.test(type)) return meetingMutationResult(type, p, recordId);
  if (type === 'ctox.workjet.calendar.read.v1') return { schema: 'ctox.workjet.calendar.v1', request_id: p.request_id, action: p.action,
    data: p.action === 'accounts' ? { ok: true, accounts: [], truncated: false } : { ok: true, events: [], truncated: false, synced_at_ms: 1 } };
  if (type === 'ctox.workjet.speech.settings.v1') return { action: p.action, commandId: p.commandId };
  if (SESSION_OPEN[type]) return SESSION_OPEN[type](p);
  if (type === 'ctox.workjet.grok.v1') return { version: 1, action: p.action, operationId: p.operationId };
  if (type.startsWith('ctox.workjet.providers.')) return { ok: true, schema: 'ctox.provider-federation-registry.v1', revision: 0, accounts: [], providers: [] };
  return { ok: true };
}

// ---- Stub-Kernel mit protokollierender App-API -------------------------------------------------------------------
const als = new AsyncLocalStorage();
const calls = []; // {bridge, action, type, via}
const registered = new Set();
function viaOf(stack) {
  if (/openNativeSession/.test(stack)) return 'requestNative';
  if (/at (?:async )?(?:Object\.)?requestNative \(.*v2-compat\.js/.test(stack)) return 'requestNative';
  if (/at (?:async )?(?:Object\.)?dispatch \(.*v2-compat\.js/.test(stack)) return 'dispatch';
  return 'direct';
}
function viaDetail(stack) { return /openNativeSession/.test(stack) ? 'e26-session-open' : null; }

kernel = createStubKernel({ instanceId: 'wjb-measure', userId: OWNER, latencyMs: 1, storage: 'memory',
  actor: { id: OWNER, display_name: 'Owner', role: 'admin', permissions: [] } });
await kernel.ready;
const adapted = adaptKernel(kernel);
const logging = Object.create(adapted, { appApi: { value(appId, ver) {
  const api = adapted.appApi(appId, ver);
  return { ...api, command(type, payload = {}, opts = {}) {
    const stack = new Error().stack || '';
    const ctx = als.getStore() || { bridge: '?', action: '?' };
    calls.push({ bridge: ctx.bridge, action: ctx.action, type, via: viaOf(stack), detail: viaDetail(stack), op: payload?.op ?? null });
    if (!registered.has(type)) {
      registered.add(type);
      kernel.sim.onCommand(type, (p, c) => respond(type, p, kernel.__records.get(c.command_id) ?? null));
    }
    const handle = api.command(type, payload, opts);
    kernel.__records.set(handle.id, opts.record_id ?? null);
    // E26-Sitzung: der Kern meldet einen Zwischenstand (running mit result), erst dann registriert die Brücke den Strom.
    if (SESSION_OPEN[type] && payload?.op === 'open') {
      const inject = (s) => (s && s.status === 'running' ? { ...s, result: SESSION_OPEN[type](payload) } : s);
      return { id: handle.id, local: handle.local, result: handle.result, cancel: handle.cancel,
        status: { get: () => inject(handle.status.get()), subscribe: (cb) => handle.status.subscribe((s) => cb(inject(s))) } };
    }
    return handle;
  } };
} } });
kernel.__records = new Map();
const compat = createV2Compat({ kernel: logging });
const bridges = createWorkjetBridges(compat);
const FN = { project: bridges.workjetProjectControl, computer: bridges.workjetComputerControl,
  session: bridges.workjetSessionControl, device: bridges.workjetBusinessOsDeviceControl };

// ---- Lauf --------------------------------------------------------------------------------------------------------
const entries = [];
const writeJson = () => {
  fs.mkdirSync(OUT_DIR, { recursive: true });
  fs.writeFileSync(OUT_JSON, `${JSON.stringify(entries, null, 2)}\n`);
};
const uniq = (xs) => [...new Set(xs)];
const outcomes = [];
for (const bridge of ['project', 'computer', 'session', 'device']) {
  for (const action of bridges.supported(bridge)) {
    const requests = REQUESTS[action];
    const sites = SITES[action];
    const runs = [];
    if (!requests || !sites) {
      runs.push({ error: `keine Anfrage/kein Beleg im Messwerkzeug für ${action}` });
    } else {
      for (const request of requests) {
        let error = null;
        try {
          await als.run({ bridge, action }, () => Promise.race([FN[bridge](JSON.parse(JSON.stringify(request))),
            new Promise((_, reject) => setTimeout(() => reject(new Error(`TIMEOUT ${ACTION_TIMEOUT_MS} ms`)), ACTION_TIMEOUT_MS))]));
        } catch (e) { error = `${e?.code ? `${e.code}: ` : ''}${e?.message ?? e}`; }
        runs.push({ op: request.op ?? null, error });
      }
    }
    await new Promise((r) => setTimeout(r, 20));
    const mine = calls.filter((c) => c.bridge === bridge && c.action === action);
    const measured = uniq(mine.map((c) => c.type));
    const expected = uniq((sites || []).map((s) => s.type));
    const missing = expected.filter((t) => !measured.includes(t));
    const unexpected = measured.filter((t) => !expected.includes(t));
    const vias = uniq(mine.map((c) => c.via));
    const local = LOCAL_QUERY[action];
    const via = vias.length ? vias.join('|') : local ? `query (E32, lokale Kernel-Abfrage ${local.collection})` : 'unbekannt (kein Befehl gemessen)';
    const entry = {
      bridge, action,
      command_types: missing.length ? uniq([...measured, ...missing]) : measured,
      via,
      source: missing.length ? 'static' : 'measured',
      evidence: local ? `E32 ${local.collection} @ ${local.where} (entfallen: ${local.dropped})` : (sites || []).map((s) => `${s.type} @ ${s.evidence}`).join(' | '),
    };
    if (local) entry.e32 = { query: local.collection, dropped_command_type: local.dropped };
    if (missing.length) entry.measured_command_types = measured;
    const notes = [];
    if (unexpected.length) notes.push(`nicht in der statischen Liste: ${unexpected.join(', ')}`);
    const opNotes = mine.filter((c) => c.op).map((c) => `${c.op}${c.detail ? `(${c.detail})` : ''}`);
    if (opNotes.length) notes.push(`Ops mit Befehl: ${opNotes.join(', ')}; Ops ohne Befehl: ${runs.filter((r) => r.op && !mine.some((c) => c.op === r.op)).map((r) => r.op).join(', ') || '–'}`);
    const errs = runs.filter((r) => r.error).map((r) => `${r.op ? `${r.op}: ` : ''}${r.error.slice(0, 160)}`);
    if (errs.length) notes.push(`Aktion endete nach den Befehlen mit Fehler (Antwortform/Projektion im Stub): ${errs.join(' / ')}`);
    if (notes.length) entry.note = notes.join(' ; ');
    entries.push(entry);
    outcomes.push({ bridge, action, sent: mine.length, ok: runs.every((r) => !r.error) });
  }
  writeJson(); // nach jeder Brücke fortschreiben
}

// ---- Typenliste --------------------------------------------------------------------------------------------------
const byType = new Map();
for (const e of entries) for (const t of e.command_types) {
  if (!byType.has(t)) byType.set(t, []);
  byType.get(t).push(e.action);
}
const types = [...byType.keys()].sort();
fs.writeFileSync(OUT_TXT, types.map((t) => `${t}\t${byType.get(t).join(',')}`).join('\n') + '\n');

// ---- Gegenprobe --------------------------------------------------------------------------------------------------
const inventoryAll = Object.keys(INVENTORY).flatMap(inventoryList);
const enabled = entries.map((e) => e.action);
console.log(`Aktionen: ${entries.length} (erwartet 76), gemessen ${entries.filter((e) => e.source === 'measured').length}, statisch ${entries.filter((e) => e.source === 'static').length}`);
console.log(`Inventar ${inventoryAll.length}, nicht freigeschaltet: ${inventoryAll.filter((a) => !enabled.includes(a)).join(', ')}`);
console.log(`Aktionen ohne Befehl: ${outcomes.filter((o) => o.sent === 0).map((o) => o.action).join(', ') || '–'}`);
console.log(`Aktionen vollständig erfolgreich im Stub: ${outcomes.filter((o) => o.ok).length}/${outcomes.length}`);
console.log(`Unterschiedliche Typen: ${types.length}`);
// E32: Lesetypen entfallen/bleiben (Abnahmeliste für den Kern). Ein Typ entfällt nur, wenn KEINE Aktion ihn mehr sendet.
const e32Dropped = READ_TYPES_BEFORE_E32.filter((t) => !types.includes(t));
const e32Kept = READ_TYPES_BEFORE_E32.filter((t) => types.includes(t));
const e32Expected = Object.values(LOCAL_QUERY).map((q) => q.dropped).sort();
const e32LocalWithCommand = entries.filter((e) => LOCAL_QUERY[e.action] && e.command_types.length);
console.log(`E32 lokale Abfragen: ${Object.keys(LOCAL_QUERY).join(', ')}; mit Befehl: ${e32LocalWithCommand.map((e) => e.action).join(', ') || '–'}`);
console.log(`E32 entfallende Kern-Typen (${e32Dropped.length}): ${e32Dropped.join(', ') || '–'}`);
console.log(`E32 bleibende Lesetypen (${e32Kept.length}): ${e32Kept.join(', ')}`);
const e32Mismatch = JSON.stringify(e32Dropped) !== JSON.stringify(e32Expected) || e32LocalWithCommand.length > 0;
if (e32Mismatch) console.log(`E32 ABWEICHUNG: erwartet entfallend ${e32Expected.join(', ')}`);
const EXCLUDED = {
  "'ctox.workjet.exit_model.'": 'Präfix des dynamischen Typs ctox.workjet.exit_model.<verb>; read/refresh/submit gemessen',
  "'ctox.workjet.exit_model.v1'": 'Vertragskennung im Ergebnis (assessment.contract), kein Befehlstyp',
  "'ctox.workjet.project.supervisor.route.read.'": 'Präfix in der Ergebnisprüfung result.read_command, kein gesendeter Befehl',
  "'ctox.workjet.supervisor_binding.v1'": 'Vertragskennung (contract) im Ergebnis, kein Befehlstyp',
  "'ctox.workjet.supervisor_input.v1'": 'Vertragskennung (input_contract) im Ergebnis, kein Befehlstyp',
  "'ctox.workjet.supervisor_turn.v1'": 'Vertragskennung (contract) im Ergebnis, kein Befehlstyp',
  "'ctox.workjet.supervisor_turn_capabilities.v1'": 'Vertragskennung (contract) im Ergebnis, kein Befehlstyp',
};
const grepHits = new Map();
for (const f of fs.readdirSync(path.join(WJ, 'actions')).filter((n) => n.endsWith('.js'))) {
  const lines = fs.readFileSync(path.join(WJ, 'actions', f), 'utf8').split('\n');
  lines.forEach((line, i) => {
    for (const m of line.matchAll(/'ctox\.(workjet|plan)\.[a-z0-9_.]+'/g)) {
      if (!grepHits.has(m[0])) grepHits.set(m[0], []);
      grepHits.get(m[0]).push(`actions/${f}:${i + 1}`);
    }
  });
}
let unexplained = 0;
for (const [lit, where] of [...grepHits].sort()) {
  const t = lit.slice(1, -1);
  const status = types.includes(t) ? 'in Liste' : EXCLUDED[lit] ? `ausgenommen: ${EXCLUDED[lit]}` : (unexplained += 1, 'UNBEGRÜNDET');
  console.log(`  ${lit}  ${status}  [${where.join(', ')}]`);
}
console.log(`grep-Gegenprobe: ${grepHits.size} Literale, unbegründet ${unexplained}`);
console.log(`Ausgabe: ${OUT_JSON}\n         ${OUT_TXT}`);
await kernel.close?.();
process.exit(unexplained || entries.length !== 76 || e32Mismatch ? 1 : 0); // Strang R: 72 → 76 (presentation.* + narration.read)
