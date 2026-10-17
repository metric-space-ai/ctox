// V2-Orakel für die freigeschalteten v4-Aktionen (Harness: v2-oracle.mjs).
// Jeder Strang ergänzt hier die Testnamen seiner freigeschalteten Aktionen (Testdatei → Namen, wörtlich wie in V2).
import assert from 'node:assert/strict';
import test from 'node:test';
import { runOracle } from './v2-oracle.mjs';
import { realManifestRoutes } from '../../../tools/port/real-manifest.mjs';

// E48 (Pin 8265bd7): die Shell liest Fremd-Collections (business_commands, workjet_*) nur mit ihrem veröffentlichten
// Manifest (Deklaration) und aktiven Eigentümern mit read-Export (ctox; ctox-platform veröffentlicht der Stub selbst).
const SHELL_ROUTES = realManifestRoutes('shell');
async function publishShell(k) {
  for (const o of SHELL_ROUTES._fixture_owners) if (!(await k.sim.inspect('business_apps', o.app))?.data?.active_ver) await k.sim.publishAppVersion(o.app, o.ver, o.publish);
  await k.sim.publishAppVersion('shell', 'dev', { title: 'Shell', manifestExtra: SHELL_ROUTES });
}

export const ORACLE_TESTS = {
  'workjet-project-control.test.mjs': [
    // project.list (Lesepfad)
    'project list preserves the strict legacy shape until configuration is explicitly requested',
    'sixteen projected owner rows with four archived return exactly twelve active projects',
    'a two-project replication gap reloads only the missing active native identities',
    'a verified alias lists twelve owner projects and repairs missing rows without changing its actor',
    'an unrelated actor receives zero projects even when projections contain another owner',
    'invalid confirmed owners fail before reads and requests cannot choose an owner',
    'a confirmed owner never replaces the original alias session fence during a repair read',
    'legacy native receipts retain the authenticated actor scope without guessing an alias',
    'malformed native identity windows cannot trigger projection or repair reads',
    'repair reads preserve the first query generation and shared deadline',
    'project list rejects malformed configuration negotiation before native dispatch',
    'project list starts all bridges concurrently and skips historical command replication',
    'each project list requires new native authority and accepts a confirmed empty result',
    'nonempty native project counts cannot confirm empty or partial projections',
    'missing or malformed native completeness metadata rejects before any projection read',
    'a truncated native window cannot be delivered as a complete project list',
    'a native zero count cannot confirm extra or discarded projection rows',
    'identity remains fenced through final project serialization',
    'native working-copy reads page through 200-row windows up to the declared 500-row cap',
    'a replaced generation or duplicated page boundary cannot deliver a partial native copy list',
    'missing, rejected or replaced native project authority cannot return cached data',
    'project list rejects wrong command receipt or a changed actor before querying',
    'collection and command phases consume one list deadline rather than restarting it',
    'a query deadline aborts both native projection streams',
    'policy list is separately opt-in, retains Owner scope, and resolves the old default',
    'Project list exposes configured Supervisor selection only on a dedicated opt-in',
    // project.worker.add (Befehlspfad)
    'Workjet project.worker.add forwards native mutation and preserves receipt identity on retry',
    ...['different command', 'different project', 'different worker', 'different record', 'pending command',
      'cancelled command', 'failed command', 'failed domain mutation', 'wrong contract', 'missing native id', 'group id']
      .map((reason) => `Workjet private chat rejects ${reason} without dispatching a replacement`),
    'Workjet chat mutation propagates native policy rejection without retry',
    'Workjet discards a pending chat receipt after session replacement',
    'Workjet discards a pending chat receipt after database replacement',
    'Workjet does not dispatch after session replacement during data-plane readiness',
  ],
};

// Strang B – Supervisor bind/turn (workjet-project-control.test.mjs, Supervisor-Blöcke). Nicht übernommen: die drei
// „Supervisor history …“-Proben (project.supervisor.turn.history steht nicht in INVENTORY, Workjet ruft es nicht).
ORACLE_TESTS['workjet-project-control.test.mjs'].push(
  'supervisor registration uses authenticated command plane and returns the existing UUID',
  'supervisor registration rejects fabricated input and uncorrelated native results',
  'explicit supervisor kinds survive the shell bridge and legacy submits omit them',
  'supervisor capabilities are a scoped native control without creating a turn',
  'supervisor capabilities refuse foreign malformed stale and unsupported confirmations',
  'supervisor submit watch cancel use the native control plane on the same CodeThread',
  'supervisor controls refuse forged owner routes and unsupported execution parameters',
  'supervisor turn results reject foreign identities queue links and changed sessions',
  'supervisor cancellation never pretends a running worker acknowledged interruption',
  'legacy supervisor watch has exactly the prior outer shape even if unsolicited facts arrive',
  'opted-in supervisor watch forwards the bounded fixture contract and actual native facts',
  'public assistant text opt-in survives the Shell bridge and preserves native chunks',
  'native Messages opt-in preserves upstream identity without inventing a provider turn',
  'an opted-in queued supervisor turn preserves the absence of an actual attempt',
  'execution page invalid bounds and extra caller authority never dispatch',
  'execution pages reject foreign native identities unsafe fields and receipt substitution',
  'execution page cursor must match the ordered safe native event page',
  'Owner follow-up targets the existing native task without submitting another task',
  'Owner follow-up rejects forged approvals routes oversize text and mismatched receipts',
  'same-task input capability is opt-in and comes from the actual scoped native receipt',
);
// Ende Strang B

// Strang A – create/configure/chat.create/KPI (workjet-project-control.test.mjs) und Exit (workjet-exit-model-control).
// Nicht übernommen: Quelltext-Regex-Proben (u. a. „project.init dispatches …“), „feed settings map …“ (ruft
// v2-helpers-Funktionen direkt aus app.js), Proben mit Kern-Fixtures (laufen im Block „Strang A“ am Dateiende).
ORACLE_TESTS['workjet-project-control.test.mjs'].push(
  // project.create
  'Workjet project create/list is idempotent across optional copies and computers',
  // project.configure (Konfiguration, Policy, Luma)
  'execution policy configure uses a typed Owner CAS and native receipt, without a tool grant',
  'policy configure snapshots the CAS patch before waiting and drops a changed Owner session',
  'Supervisor selection is bounded, typed and refuses execution or account payloads',
  'Supervisor configuration rejects an unconfirmed selection or unsuccessful clear',
  'Old configure callers do not receive a stored Luma selection without sending that field',
  'project configuration forwards bounded metadata and returns native fields to Workjet',
  'configuration preserves omission versus explicit null at the command boundary',
  'configuration accepts a native verified owner alias without changing its actor',
  'configuration retains same-actor compatibility but does not infer aliases from old receipts',
  'project configuration rejects forged authority and invalid metadata before dispatch',
  'project configuration rejects wrong receipts, foreign projects and session replacement',
  // project.chat.create
  'Workjet project.chat.create forwards native mutation and preserves receipt identity on retry',
  'Workjet chat creation rejects a receipt for another title',
  'Workjet validates chat request fields before dispatch',
  // project.kpis.read|configure|refresh (die Probe „native project detail …“ prüft am Ende zusätzlich
  // project.jour_fixe.meeting.read = Strang C; bis C freischaltet, besteht dieser Teil über „nicht unterstützt“)
  'native KPI control reads, configures and clears prompts without a projection pull',
  'native project detail control rejects forged and invalid requests before dispatch',
  'native KPI receipts reject foreign scope, changed intent and session replacement',
  'native KPI refresh maps per-KPI status and rejects malformed reports',
);
ORACLE_TESTS['workjet-exit-model-control.test.mjs'] = [
  'exit refresh carries explicit resource proposal through existing native command plane',
  'exit reads reject source writes and receipt/session identity drift',
  'exit resource proposal has no implicit defaults or hidden allocations',
  'exit confirmation submits the stored run once without accepting input overrides',
];
// Ende Strang A

// Strang C – Jour fixe (Besprechungsänderungen), Kalender (workjet-project-control.test.mjs). Nicht übernommen:
// „local final …“ (4) und „local narration …“ (2): prüfen project.jour_fixe.transcript.local_candidate bzw.
// .narration.local_publish – beide nicht im gemessenen INVENTORY (Rumpf portiert, actions/meeting.js PREPARED).
// Die zwei „meeting read …“-Proben lesen das Kern-Fixture workjet-jour-fixe-v1.json und laufen in der Strang-C-Hülle
// am Dateiende.
ORACLE_TESTS['workjet-project-control.test.mjs'].push(
  'calendar reads use the authenticated guest without loading project or command windows',
  'meeting Owner controls preserve typed intent and compact native revision receipts',
  'meeting Owner controls reject caller authority and unsafe revisions before dispatch',
  'Owner text cannot manufacture supervisor or speech provenance',
  'meeting mutation rejects unsuccessful, foreign, changed-intent and stale receipts',
  'meeting mutation snapshots nested intent across the native wait',
  'slide comment rejects unsafe pins and claimed author before dispatch',
  'slide comment receipt must confirm the same comment identity',
  'Owner confirmation preserves all three native CAS revisions and returns the actual Core goal',
  'Owner confirmation rejects caller authority, replacement todos and unsafe goal revisions before dispatch',
  'Owner confirmation rejects stale or substituted native goal and meeting receipts',
);
// Ende Strang C

for (const [file, names] of Object.entries(ORACLE_TESTS)) {
  const allow = new Set(names);
  const { registered, skipped } = await runOracle(file, allow);
  test(`[V2-Orakel] ${file}: jede freigegebene V2-Probe existiert und läuft (${registered.length} gegen v4, ${skipped.length} nicht portiert)`, () => {
    const missing = names.filter((n) => !registered.includes(n));
    assert.deepEqual(missing, [], 'freigegebene Testnamen fehlen in der V2-Datei (Pin geändert?)');
  });
}

// Strang B – eigene Orakel-Hülle für V2-Dateien außerhalb des Pins bzw. mit Kern-Fixtures (v2-oracle.mjs gehört dem
// Orchestrator und materialisiert nur shared/*; diese Dateien lesen zusätzlich src/core/rxdb/tests/fixtures/*.json
// bzw. stammen aus dem V2-PR #874). Aufbau: <tmp>/src/apps/business-os/{app.js,shared/…} + <tmp>/src/core/rxdb/…,
// damit die relativen URLs der V2-Tests unverändert auflösen. Ersetzt wird nur:
//   - app.js → dieselbe Weiterleitungshülle wie v2-oracle.mjs (workjetProjectControl → v4-Brücke),
//   - shared/workjet-supervisor-route-native.mjs → Re-Export von requestSupervisorRoute aus actions/supervisor.js,
//   - shared/*.generated.mjs → die Datei aus dem v4-Arbeitsbaum (genau die Module, die v4 importiert),
//   - 'node:test' / 'node:vm' → die Orakel-Shims (Freigabe je Testname, __wjb im vm-Kontext).
{
  const { execFileSync } = await import('node:child_process');
  const fs = await import('node:fs');
  const os = await import('node:os');
  const path = await import('node:path');
  const { fileURLToPath, pathToFileURL } = await import('node:url');
  const { createWorkjetBridges } = await import('../index.js');
  const { V2_PIN } = await import('./v2-oracle.mjs');
  const HERE = path.dirname(fileURLToPath(import.meta.url));
  const BOS_DIR = path.resolve(HERE, '../../../..');
  const REPO = execFileSync('git', ['rev-parse', '--show-toplevel'], { cwd: HERE, encoding: 'utf8' }).trim();
  // V2-PR #874 (ctox.plan.step.retry), Branch shell/plan-step-retry im Klon /Volumes/tmp/ctox-m20.
  const RETRY_REPO = process.env.WJB_PLAN_STEP_RETRY_REPO || '/Volumes/tmp/ctox-m20';
  const RETRY_REF = 'f0c836569';
  const show = (repo, ref, rel) => execFileSync('git', ['show', `${ref}:${rel}`], { cwd: repo, encoding: 'utf8', maxBuffer: 64 << 20 });
  const APP_SHIM_B = `// v4-Orakel-Hülle (Strang B), kein V2-Code
globalThis.workjetProjectControl = workjetProjectControl;
const WORKJET_PROJECT_CONTROL_MAX_RESULTS = 100;
async function workjetProjectControl(request = {}, diagnostics = {}) {
  return __wjb.project(globalThis, request, diagnostics);
}
async function waitForSyncBridgeReady() {}
`;
  const factory = Object.freeze({
    project(g, request, diagnostics) {
      const env = {
        get state() { return g.state; },
        actorContext: (session) => g.actorContext(session),
        newId: () => (typeof g.newId === 'function' ? g.newId() : globalThis.crypto.randomUUID()),
        now: () => (typeof g.Date?.now === 'function' ? g.Date.now() : Date.now()),
      };
      return createWorkjetBridges(env).workjetProjectControl(request, diagnostics);
    },
  });
  const SUPERVISOR_JS = pathToFileURL(path.resolve(HERE, '../actions/supervisor.js')).href;
  function materializeB(name, { repo, ref, testFile }) {
    const root = path.join(os.tmpdir(), `wjb-b-oracle-${process.pid}-${name}`);
    const bos = path.join(root, 'src/apps/business-os');
    fs.mkdirSync(path.join(bos, 'shared'), { recursive: true });
    fs.writeFileSync(path.join(bos, 'app.js'), APP_SHIM_B);
    const seen = new Set();
    const put = (relBos, text) => {
      const out = path.join(bos, relBos);
      fs.mkdirSync(path.dirname(out), { recursive: true });
      fs.writeFileSync(out, text);
    };
    const copy = (relBos) => {
      if (seen.has(relBos)) return;
      seen.add(relBos);
      let text;
      if (relBos === 'shared/workjet-supervisor-route-native.mjs') {
        text = `export { requestSupervisorRoute } from ${JSON.stringify(SUPERVISOR_JS)};\n`;
      } else if (/^shared\/[^/]+\.generated\.mjs$/.test(relBos)) {
        text = fs.readFileSync(path.join(BOS_DIR, relBos), 'utf8');
      } else {
        text = show(repo, ref, `src/apps/business-os/${relBos}`);
      }
      if (relBos === `shared/${testFile}`) {
        text = text
          .replace(/^import test from 'node:test';$/m, `import test from ${JSON.stringify(pathToFileURL(path.join(HERE, 'oracle-test-shim.mjs')).href)};`)
          .replace(/^import vm from 'node:vm';$/m, `import vm from ${JSON.stringify(pathToFileURL(path.join(HERE, 'oracle-vm-shim.mjs')).href)};`);
      }
      put(relBos, text);
      for (const m of text.matchAll(/from '(\.\.?\/[^'?]+)(?:\?[^']*)?'/g)) {
        copy(path.posix.normalize(path.posix.join(path.posix.dirname(relBos), m[1])));
      }
      for (const m of text.matchAll(/new URL\('\.\.\/\.\.\/\.\.\/(core\/rxdb\/tests\/fixtures\/[^']+)'/g)) {
        const out = path.join(root, 'src', m[1]);
        fs.mkdirSync(path.dirname(out), { recursive: true });
        fs.writeFileSync(out, show(repo, ref, `src/${m[1]}`));
      }
    };
    copy(`shared/${testFile}`);
    return path.join(bos, 'shared', testFile);
  }
  async function runOracleB(name, source, allow) {
    const registered = [];
    const skipped = [];
    globalThis.__WJB_ORACLE = { factory, allow: (n) => allow(n), registered, skipped };
    await import(pathToFileURL(materializeB(name, source)).href);
    return { registered, skipped };
  }

  // Ausgeschlossen (mit Grund):
  //  route-native „actual Shell control uses Supervisor admission …“: injiziert eine Ersatz-Zulassung über die
  //    app.js-Globale requireWorkjetSupervisorDataPlane; v4 hat diese Globale nicht (Zulassung liegt in
  //    actions/supervisor.js). Die geprüfte Wirkung (Instanzwechsel während der Zulassung → kein Versand) belegt
  //    die v4-Probe unten.
  //  luma-contract „project migration preserves …“: prüft modules/ctox/schema.js (Strang A/Kern), nicht Strang B.
  const ROUTE_EXCLUDED = new Set(['actual Shell control uses Supervisor admission and fences instance changes during collection readiness']);
  const ORACLE_FILES_B = [
    ['route-native', { repo: REPO, ref: V2_PIN, testFile: 'workjet-supervisor-route-native.test.mjs' }, (n) => !ROUTE_EXCLUDED.has(n), 22],
    ['route-display', { repo: REPO, ref: V2_PIN, testFile: 'workjet-supervisor-route-display-contract.test.mjs' }, () => true, 2],
    ['route-computation', { repo: REPO, ref: V2_PIN, testFile: 'workjet-supervisor-route-computation-contract.test.mjs' }, () => true, 1],
    ['source', { repo: REPO, ref: V2_PIN, testFile: 'workjet-supervisor-source-contract.test.mjs' }, () => true, 2],
    ['luma', { repo: REPO, ref: V2_PIN, testFile: 'workjet-supervisor-luma-contract.test.mjs' }, (n) => n === 'native and browser share the project Supervisor Luma reference corpus', 1],
    ['worker-outcome', { repo: REPO, ref: V2_PIN, testFile: 'workjet-worker-outcome-contract.test.mjs' }, () => true, 2],
    ['plan-step-retry', { repo: RETRY_REPO, ref: RETRY_REF, testFile: 'workjet-plan-step-retry.test.mjs' }, () => true, 3],
  ];
  for (const [name, source, allowFn, expected] of ORACLE_FILES_B) {
    const { registered, skipped } = await runOracleB(name, source, allowFn);
    test(`[V2-Orakel] ${source.testFile} (Strang B): ${registered.length} gegen v4, ${skipped.length} nicht portiert`, () => {
      assert.ok(registered.length > 0, 'keine V2-Probe registriert');
      assert.equal(registered.length, expected, 'Anzahl freigegebener V2-Proben (Pin geändert?)');
    });
  }
}
// Ende Strang B

// Strang B – v4-Proben gegen den echten Kern-Stub (Stub-Pin 8489b4e, Speicher memory): v2-compat + Router + Vertrag.
// Ergänzen die V2-Orakel dort, wo V2 nur gegen Attrappen prüft (Quittungsform des echten Befehlspfads, until
// 'accepted', unknown_command → unsupported, Instanzzaun der Route-Zulassung).
{
  const os = await import('node:os');
  const path = await import('node:path');
  const fs = await import('node:fs');
  const { fileURLToPath, pathToFileURL } = await import('node:url');
  const { adaptKernel } = await import('../../kernel-adapter.js');
  const { createWorkjetBridges, installWorkjetBridges } = await import('../index.js');
  const { createV2Compat, isUnsupported } = await import('../v2-compat.js');
  const { checkResponse } = await import('./contract-checkers.mjs');
  const HERE = path.dirname(fileURLToPath(import.meta.url));
  const STUB_DIR = process.env.CTOX_APP_API_STUB || path.join(os.homedir(), '.local/state/shell-v4/stub-pins/64acc2c');
  const { createStubKernel } = await import(pathToFileURL(path.join(STUB_DIR, 'ctox-app-api-stub.mjs')).href);
  const FIX = path.resolve(HERE, '../../../../../../core/rxdb/tests/fixtures');
  const fixture = (name) => JSON.parse(fs.readFileSync(path.join(FIX, name), 'utf8'));
  const WORKJET_UNSUPPORTED = /(?:unsupported|unknown)\s+(?:workjet\s+)?(?:project[- ]control\s+)?action/i;
  const docOf = (r) => r?.doc ?? r?.value ?? r?.data ?? r;
  let n = 0;
  const mk = async () => {
    const k = createStubKernel({ instanceId: `wjb-b-${process.pid}-${++n}`, userId: 'owner-1', latencyMs: 2, storage: 'memory',
      actor: { id: 'owner-1', display_name: 'Owner', role: 'admin', permissions: [] } });
    await k.ready;
    return { k, b: createWorkjetBridges(createV2Compat({ kernel: adaptKernel(k) })) };
  };
  const PROJECT = 'f791215c-e416-4205-8619-bbf82b999799';
  const THREAD = 'cc6cfe73-2824-4360-9daf-3b3efb079931';
  const TURN = 'cmd_74c4a208-7b2d-4b5e-a83d-f2f012d4c5a9';
  const binding = { project_id: PROJECT, thread_id: THREAD, thread_key: `business-os/threads/${THREAD}` };
  // K-WJ-B1: der Stub schreibt das Befehlsergebnis per JSON-Merge-Patch (mergePatch: null = Feld löschen); null-Werte
  // im result kommen deshalb nicht an. Die Vertragsproben hier tragen daher Nicht-null-Fehlerfelder; die null-Form
  // (V2-Normalfall error_code/error_message null, Route v1 actual null) steht unten als todo mit Kern-Lücke.
  const nativeTurn = (extra = {}) => ({
    command_id: TURN, task_id: 'queue:system::supervisor-turn', thread_id: THREAD, thread_key: binding.thread_key,
    execution_phase: 'queued', status: 'queued', queue_status: 'pending', attempt: 0, terminal: false,
    result: {}, result_truncated: false, error_code: 'none_yet', error_message: '', ...extra,
  });
  const json = (v) => JSON.parse(JSON.stringify(v));

  test('[v4 Strang B] project.supervisor.bind über kernel.command: Quittung korreliert, record_id = Projekt, Vertrag erfüllt', async () => {
    const { k, b } = await mk();
    const seen = [];
    k.sim.onCommand('ctox.workjet.project.supervisor.bind', (payload, ctx) => {
      seen.push({ payload, ctx });
      return { ok: true, contract: 'ctox.workjet.supervisor_binding.v1', binding };
    });
    const res = json(await b.workjetProjectControl({ action: 'project.supervisor.bind', commandId: 'bind-1', projectId: PROJECT, threadId: THREAD }));
    assert.deepEqual(checkResponse('project.supervisor.bind', res), []);
    assert.deepEqual(seen[0].payload, { project_id: PROJECT, thread_id: THREAD });
    assert.equal(docOf(await k.sim.inspect('business_commands', seen[0].ctx.command_id))?.record_id, PROJECT);
    await k.close();
  });

  test('[v4 Strang B] turn.submit|watch|cancel|input|capabilities über kernel.command erfüllen den Workjet-Vertrag', async () => {
    const { k, b } = await mk();
    const base = { commandId: 'c-1', projectId: PROJECT, threadId: THREAD };
    k.sim.onCommand('ctox.workjet.project.supervisor.turn.submit', () => ({ ok: true, contract: 'ctox.workjet.supervisor_turn.v1', binding, turn: nativeTurn(), message_id: 'workjet_supervisor_message_1' }));
    k.sim.onCommand('ctox.workjet.project.supervisor.turn.watch', () => ({ ok: true, contract: 'ctox.workjet.supervisor_turn.v1', binding,
      turn: nativeTurn({ execution_phase: 'running', status: 'running', queue_status: 'leased', attempt: 1 }) }));
    k.sim.onCommand('ctox.workjet.project.supervisor.turn.cancel', () => ({ ok: true, contract: 'ctox.workjet.supervisor_turn.v1', binding,
      turn: nativeTurn({ execution_phase: 'terminal', status: 'cancelled', queue_status: 'cancelled', terminal: true }),
      cancellation: { command_id: 'workjet_project_cancel_1', side_effects_may_have_started: true, worker_interrupt_acknowledged: false } }));
    k.sim.onCommand('ctox.workjet.project.supervisor.turn.input', (p) => ({ ok: true, contract: 'ctox.workjet.supervisor_input.v1', binding, turn: nativeTurn(),
      input: { input_id: 'native-input-1', sequence: 1, body: p.body, created_at: '2026-10-16T10:00:00Z' }, delivery: 'next_slice', worker_interrupted: false }));
    k.sim.onCommand('ctox.workjet.project.supervisor.turn.capabilities', () => ({ ok: true, contract: 'ctox.workjet.supervisor_turn_capabilities.v1', binding,
      turn_kinds: ['work', 'conversation'], default_turn_kind: 'work',
      input_contract: 'ctox.workjet.supervisor_input.v1', input_delivery: 'next_slice', max_input_chars: 4096 }));
    const cases = [
      ['project.supervisor.turn.submit', { goal: 'Bericht vorbereiten', turnKind: 'work' }],
      ['project.supervisor.turn.watch', { targetCommandId: TURN }],
      ['project.supervisor.turn.cancel', { targetCommandId: TURN, reason: 'Abbruch in Workjet' }],
      ['project.supervisor.turn.input', { targetCommandId: TURN, body: 'Neue Fakten zum selben Auftrag.' }],
      ['project.supervisor.turn.capabilities', { includeInput: true }],
    ];
    for (const [action, extra] of cases) {
      // WJB-R (E26 idempotency_key): je Aktion eine eigene commandId (dieselbe ID = dieselbe Operation, wie V2-Dokument-ID)
      const res = json(await b.workjetProjectControl({ action, ...base, commandId: `c-${action}`, ...extra }));
      assert.deepEqual(checkResponse(action, res), [], action);
    }
    await k.close();
  });

  test('[v4 Strang B] route.read|capabilities v1|v2 über kernel.command: Fixture-DTO durchgereicht, Vertrag erfüllt', async () => {
    const { k, b } = await mk();
    const v1 = fixture('workjet-supervisor-route-display-v1.json').valid_cases;
    const v2 = fixture('workjet-supervisor-route-computation-v2.json').valid_cases;
    const values = {
      'project.supervisor.route.read.v1': v1.find((c) => c.type === 'SupervisorRouteDisplay' && c.value.configured).value,
      'project.supervisor.route.capabilities.v1': v1.find((c) => c.type === 'SupervisorRouteCapabilities').value,
      'project.supervisor.route.read.v2': v2.find((c) => c.type === 'SupervisorRouteDisplay' && c.value.actual).value,
      'project.supervisor.route.capabilities.v2': v2.find((c) => c.type === 'SupervisorRouteCapabilities').value,
    };
    const hasNull = (v) => v === null || (typeof v === 'object' && Object.values(v).some(hasNull));
    let checked = 0;
    for (const [action, template] of Object.entries(values)) {
      const value = { ...json(template), project_id: PROJECT, supervisor_thread_id: THREAD };
      checked += 1;
      // command_plane dekoriert terminale DTOs mit status/task_status (V2 route-native.mjs:47-53).
      k.sim.onCommand(`ctox.workjet.${action}`, () => ({ ...value, status: 'completed', task_status: 'completed' }));
      const res = json(await b.workjetProjectControl({ action, commandId: `route-${action}`, projectId: PROJECT, threadId: THREAD }));
      assert.deepEqual(checkResponse(action, res), [], action);
      assert.deepEqual(res[action.includes('.capabilities.') ? 'capabilities' : 'route'], value);
    }
    assert.equal(checked, Object.keys(values).length, 'alle Route-DTOs inklusive null-Feldern über den Stub geprüft (K-WJ-B1 gelöst, Stub 637caf6)');
    await k.close();
  });

  // K-WJ-B1/K-WJ-6 (E27): result ist ein Wert, null bleibt erhalten (Stub 637caf6). Echter Kern: Transfer-Strang.
  test('[v4 Strang B] null-Felder im Befehlsergebnis erreichen Workjet (turn.errorCode null, Route v1 actual null)', async () => {
    const { k, b } = await mk();
    k.sim.onCommand('ctox.workjet.project.supervisor.turn.watch', () => ({ ok: true, contract: 'ctox.workjet.supervisor_turn.v1', binding,
      turn: nativeTurn({ error_code: null, error_message: null }) }));
    const route = fixture('workjet-supervisor-route-display-v1.json').valid_cases.find((c) => c.type === 'SupervisorRouteDisplay' && c.value.configured).value;
    k.sim.onCommand('ctox.workjet.project.supervisor.route.read.v1', () => ({ ...json(route), project_id: PROJECT, supervisor_thread_id: THREAD, status: 'completed', task_status: 'completed' }));
    try {
      const watched = json(await b.workjetProjectControl({ action: 'project.supervisor.turn.watch', commandId: 'w-null', projectId: PROJECT, threadId: THREAD, targetCommandId: TURN }));
      assert.deepEqual(checkResponse('project.supervisor.turn.watch', watched), []);
      const read = json(await b.workjetProjectControl({ action: 'project.supervisor.route.read.v1', commandId: 'r-null', projectId: PROJECT, threadId: THREAD }));
      assert.deepEqual(checkResponse('project.supervisor.route.read.v1', read), []);
    } finally { await k.close(); }
  });

  test('[v4 Strang B] ctox.plan.step.retry wartet nur auf accepted und antwortet pending (v2-compat ok:false bei accepted angeglichen)', async () => {
    const { k, b } = await mk();
    const seen = [];
    k.sim.onCommand('ctox.plan.step.retry', async (payload, ctx) => { seen.push({ payload, ctx }); return { ok: true }; });
    const res = json(await b.workjetProjectControl({ action: 'ctox.plan.step.retry', commandId: 'retry-1', payload: { step_id: 'step-7', thread_id: 't' } }));
    assert.deepEqual(res, { action: 'ctox.plan.step.retry', commandId: 'retry-1', status: 'pending' });
    assert.deepEqual(checkResponse('ctox.plan.step.retry', res), []);
    await new Promise((r) => setTimeout(r, 30));
    assert.deepEqual(seen[0]?.payload, { step_id: 'step-7' });
    assert.equal(docOf(await k.sim.inspect('business_commands', seen[0].ctx.command_id))?.record_id, 'step-7');
    // Kern ohne Route, beim Schreiben abgelehnt (write_result rejected unknown_command) → Workjet liest „nicht
    // unterstützt“ und blendet den Knopf aus.
    await k.sim.rejectNext('unknown_command: ctox.plan.step.retry');
    await assert.rejects(b.workjetProjectControl({ action: 'ctox.plan.step.retry', commandId: 'retry-2', payload: { step_id: 'step-8' } }),
      (e) => isUnsupported(e) && WORKJET_UNSUPPORTED.test(e.message));
    await k.close();
  });

  // K-WJ-B2 (E27): ein Befehl ohne registrierte Route endet beim Schreiben mit unknown_command (rejected), nie erst
  // nach accepted – die Brücke kann so kein „angenommen“ melden (Stub 637caf6, Registry je app_id/type).
  test('[v4 Strang B] ctox.plan.step.retry ohne Kernroute: beim Schreiben abgelehnt → nicht unterstützt, nie pending', async () => {
    const { k, b } = await mk();
    // Seit E42 (Pin 8265bd7) hat der simulierte Kern die Plattformroute ctox.plan.step.retry (isPlatformCommand); ein
    // Kern ohne diese Route wird deshalb nachgestellt: er lehnt beim Schreiben mit unknown_command ab (write_result).
    await k.sim.rejectNext('unknown_command: ctox.plan.step.retry');
    const call = b.workjetProjectControl({ action: 'ctox.plan.step.retry', commandId: 'retry-3', payload: { step_id: 'step-9' } });
    try { await assert.rejects(call, (e) => isUnsupported(e)); } finally { await new Promise((r) => setTimeout(r, 30)); await k.close(); }
  });

  test('[v4 Strang B] fehlende Kernroute für Supervisor-Befehle → unsupported (Workjet-Merkmal), kein „fehlgeschlagen“', async () => {
    const { k, b } = await mk();
    const requests = [
      { action: 'project.supervisor.bind', commandId: 'u-1', projectId: PROJECT, threadId: THREAD },
      { action: 'project.supervisor.turn.watch', commandId: 'u-2', projectId: PROJECT, threadId: THREAD, targetCommandId: TURN },
      { action: 'project.supervisor.route.read.v2', commandId: 'u-3', projectId: PROJECT, threadId: THREAD },
    ];
    for (const request of requests) {
      const type = request.action.startsWith('project.supervisor.route.') ? `ctox.workjet.${request.action}` : `ctox.workjet.${request.action}`;
      await k.sim.failCommand(type, `unknown_command: ${type}`);
      await assert.rejects(b.workjetProjectControl(request), (e) => isUnsupported(e) && WORKJET_UNSUPPORTED.test(e.message), request.action);
    }
    await k.close();
  });

  test('[v4 Strang B] Route-Zulassung: Instanzwechsel während business_commands-Bereitschaft verhindert den Versand', async () => {
    // Ersatz für die ausgeschlossene V2-Probe „actual Shell control uses Supervisor admission …“ (gleiche Wirkung).
    for (const changed of [false, true]) {
      const commands = [];
      const started = [];
      const state = {
        session: { id: 'owner' }, db: { collection: (name) => (name === 'business_commands' ? {} : null) },
        syncConfig: { instance_id: 'native' },
        commandBus: { async dispatch(command) { commands.push(command); throw new Error('nicht erwartet'); } },
      };
      state.sync = { async startCollection(name) { started.push(name); if (changed) state.syncConfig.instance_id = 'replaced'; return {}; } };
      const b = createWorkjetBridges({ state, actorContext: (s) => s, newId: () => 'x', now: () => Date.now() });
      const call = b.workjetProjectControl({ action: 'project.supervisor.route.capabilities.v2', commandId: 'r', projectId: PROJECT, threadId: THREAD });
      if (changed) await assert.rejects(call, /instance or authority changed/);
      else await assert.rejects(call, /nicht erwartet/);
      assert.deepEqual(started, ['business_commands']);
      assert.equal(commands.length, changed ? 0 : 1);
    }
  });

  test('[v4 Strang B] Globals: workjetProjectControl beantwortet alle 11 Strang-B-Aktionen (keine als unsupported abgewiesen)', async () => {
    const { k } = await mk();
    const target = {};
    const h = installWorkjetBridges({ kernel: adaptKernel(k), target });
    const B = ['project.supervisor.bind', 'project.supervisor.turn.submit', 'project.supervisor.turn.input',
      'project.supervisor.turn.cancel', 'project.supervisor.turn.watch', 'project.supervisor.turn.capabilities',
      'project.supervisor.route.read.v1', 'project.supervisor.route.read.v2',
      'project.supervisor.route.capabilities.v1', 'project.supervisor.route.capabilities.v2', 'ctox.plan.step.retry'];
    for (const action of B) assert.ok(h.bridges.supported('project').includes(action), action);
    h.uninstall();
    await k.close();
  });
}
// Ende Strang B (v4-Proben)

// Strang A – eigene Orakel-Hülle für (1) Proben mit Kern-Fixtures aus workjet-project-control.test.mjs und
// (2) workjet-projection-readiness.test.mjs. Aufbau wie Strang B: <tmp>/src/apps/business-os/{app.js,shared/…} +
// <tmp>/src/core/rxdb/tests/fixtures/*.json, damit die relativen URLs der V2-Tests unverändert auflösen.
// app.js enthält die Weiterleitungshülle (workjetProjectControl → v4-Brücke) und danach den QUELLTEXT der v4-Warter
// aus actions/project.js (waitForSyncBridgeReady, waitForProjectedWorkjetProject, waitForProjectedWorkjetWorkingCopy).
// Einzige Umformung: der erste Parameter `env` entfällt, stattdessen bindet eine Zeile env an den vm-Kontext
// ({state, now: Date.now}) – das ist die V2-Umgebung, die der Readiness-Test stellt. Die V2-Warter für Rechner und
// Sitzung (Strang D) stehen als V2-Text daneben, weil der Test beim Laden alle fünf verlangt; deren Proben sind NICHT
// freigegeben.
{
  const { execFileSync } = await import('node:child_process');
  const fs = await import('node:fs');
  const os = await import('node:os');
  const path = await import('node:path');
  const { fileURLToPath, pathToFileURL } = await import('node:url');
  const { createWorkjetBridges } = await import('../index.js');
  const { V2_PIN } = await import('./v2-oracle.mjs');
  const HERE = path.dirname(fileURLToPath(import.meta.url));
  const BOS_DIR = path.resolve(HERE, '../../../..');
  const REPO = execFileSync('git', ['rev-parse', '--show-toplevel'], { cwd: HERE, encoding: 'utf8' }).trim();
  const show = (rel) => execFileSync('git', ['show', `${V2_PIN}:${rel}`], { cwd: REPO, encoding: 'utf8', maxBuffer: 64 << 20 });
  const fnText = (src, name) => src.match(new RegExp(`^(?:async )?function ${name}\\([^]*?^\\}`, 'm'))?.[0];
  const projectSrc = fs.readFileSync(path.resolve(HERE, '../actions/project.js'), 'utf8');
  const { PROJECTION_WAIT_SOURCES } = await import('../actions/project.js');
  const v4Waits = PROJECTION_WAIT_SOURCES.map((name) => {
    const text = fnText(projectSrc, name);
    assert.ok(text, `v4-Quelltext ${name} in actions/project.js`);
    return text.replace(/^(async function \w+)\(env, ([^\n]*)\) \{\n/,
      '$1($2) {\n  const env = { state, now: () => Date.now() }; // Orakel-Bindung an den vm-Kontext\n');
  });
  const appV2 = show('src/apps/business-os/app.js');
  const v2Waits = ['waitForProjectedWorkjetComputer', 'waitForProjectedWorkjetSession'].map((name) => fnText(appV2, name));
  const APP_SHIM_A = `// v4-Orakel-Hülle (Strang A)
globalThis.workjetProjectControl = workjetProjectControl;
const WORKJET_PROJECT_CONTROL_MAX_RESULTS = 100;
async function workjetProjectControl(request = {}, diagnostics = {}) {
  return __wjb.project(globalThis, request, diagnostics);
}
${v4Waits[0]}

// --- v4-Quelltext (actions/project.js), env an den vm-Kontext gebunden
${v4Waits.slice(1).join('\n\n')}

// --- V2-Text (Strang D, nicht freigegeben), nur damit der Readiness-Test lädt
${v2Waits.join('\n\n')}
`;
  const factory = Object.freeze({
    project(g, request, diagnostics) {
      const env = {
        get state() { return g.state; },
        actorContext: (session) => g.actorContext(session),
        newId: () => (typeof g.newId === 'function' ? g.newId() : globalThis.crypto.randomUUID()),
        now: () => (typeof g.Date?.now === 'function' ? g.Date.now() : Date.now()),
      };
      return createWorkjetBridges(env).workjetProjectControl(request, diagnostics);
    },
  });
  function materializeA(name, testFile) {
    const root = path.join(os.tmpdir(), `wjb-a-oracle-${process.pid}-${name}`);
    const bos = path.join(root, 'src/apps/business-os');
    fs.mkdirSync(path.join(bos, 'shared'), { recursive: true });
    fs.writeFileSync(path.join(bos, 'app.js'), APP_SHIM_A);
    const seen = new Set();
    const put = (relBos, text) => {
      const out = path.join(bos, relBos);
      fs.mkdirSync(path.dirname(out), { recursive: true });
      fs.writeFileSync(out, text);
    };
    const copy = (relBos) => {
      if (seen.has(relBos)) return;
      seen.add(relBos);
      // *.generated.mjs aus dem v4-Arbeitsbaum = genau die Verträge, die actions/project.js importiert.
      let text = /^shared\/[^/]+\.generated\.mjs$/.test(relBos)
        ? fs.readFileSync(path.join(BOS_DIR, relBos), 'utf8') : show(`src/apps/business-os/${relBos}`);
      if (relBos === `shared/${testFile}`) {
        text = text
          .replace(/^import test from 'node:test';$/m, `import test from ${JSON.stringify(pathToFileURL(path.join(HERE, 'oracle-test-shim.mjs')).href)};`)
          .replace(/^import vm from 'node:vm';$/m, `import vm from ${JSON.stringify(pathToFileURL(path.join(HERE, 'oracle-vm-shim.mjs')).href)};`);
      }
      put(relBos, text);
      for (const m of text.matchAll(/from '(\.\.?\/[^'?]+)(?:\?[^']*)?'/g)) {
        copy(path.posix.normalize(path.posix.join(path.posix.dirname(relBos), m[1])));
      }
      for (const m of text.matchAll(/new URL\('\.\.\/\.\.\/\.\.\/(core\/rxdb\/tests\/fixtures\/[^']+)'/g)) {
        const out = path.join(root, 'src', m[1]);
        fs.mkdirSync(path.dirname(out), { recursive: true });
        fs.writeFileSync(out, show(`src/${m[1]}`));
      }
    };
    copy(`shared/${testFile}`);
    return path.join(bos, 'shared', testFile);
  }
  const ORACLE_FILES_A = [
    ['corpus', 'workjet-project-control.test.mjs', [
      'execution policy rejects unqualified overrides and uncorrelated policy receipts',
      'Supervisor selection uses the generated native corpus without selecting a producer',
      'project info summary follows the same configuration corpus as native upsert',
    ]],
    ['readiness', 'workjet-projection-readiness.test.mjs', ['project', 'working copy'].flatMap((kind) =>
      ['awaitInSync', 'awaitInitialReplication'].map((m) => `${kind} waits for bridge.state.${m} before reading the projection`))],
  ];
  for (const [name, testFile, names] of ORACLE_FILES_A) {
    const registered = [];
    const skipped = [];
    const allow = new Set(names);
    globalThis.__WJB_ORACLE = { factory, allow: (n) => allow.has(n), registered, skipped };
    await import(pathToFileURL(materializeA(name, testFile)).href);
    test(`[V2-Orakel] ${testFile} (Strang A, ${name}): ${registered.length} gegen v4, ${skipped.length} nicht portiert`, () => {
      assert.deepEqual(names.filter((n) => !registered.includes(n)), [], 'freigegebene Testnamen fehlen in der V2-Datei (Pin geändert?)');
    });
  }
  // Die importierten shared-Verträge (nicht generiert) müssen byte-gleich zum V2-Pin sein, sonst prüft das Orakel
  // anderen Code als den, den actions/project.js lädt.
  test('[V2-Orakel] Strang A: shared/jour-fixe-presentation.mjs und file-integrity.js byte-gleich zum V2-Pin', () => {
    for (const rel of ['shared/jour-fixe-presentation.mjs', 'shared/file-integrity.js']) {
      assert.equal(fs.readFileSync(path.join(BOS_DIR, rel), 'utf8'), show(`src/apps/business-os/${rel}`), rel);
    }
  });
}
// Ende Strang A

// Strang C – eigene Orakel-Hülle (v2-oracle.mjs gehört dem Orchestrator): V2-Testdateien der Sprach-/Kalendermodule
// und die Fixture-Proben „meeting read …“. Aufbau <tmp>/src/apps/business-os/{app.js,shared/…} + <tmp>/src/core/rxdb/…,
// damit relative URLs der V2-Tests unverändert auflösen. Ersetzt wird nur:
//   - app.js → Weiterleitungshülle wie v2-oracle.mjs (workjetProjectControl → v4-Brücke),
//   - shared/{speech-dictation,speech-settings-control,workjet-calendar-native,jour-fixe-speech,
//     jour-fixe-speech-contract}.mjs → Re-Export der wörtlichen Kopie in actions/meeting.js (die V2-Tests prüfen also
//     den v4-Code),
//   - shared/*.generated.mjs → die Datei aus dem v4-Arbeitsbaum (genau die Module, die v4 importiert),
//   - 'node:test' / 'node:vm' → die Orakel-Shims.
// Zusätzlich: E26-Proben der Sprachsitzungen gegen den echten Kern-Stub (langlebiger open-Befehl, Zwischenstände).
{
  const { execFileSync } = await import('node:child_process');
  const fs = await import('node:fs');
  const os = await import('node:os');
  const path = await import('node:path');
  const { fileURLToPath, pathToFileURL } = await import('node:url');
  const { createWorkjetBridges } = await import('../index.js');
  const { V2_PIN } = await import('./v2-oracle.mjs');
  const HERE = path.dirname(fileURLToPath(import.meta.url));
  const BOS_DIR = path.resolve(HERE, '../../../..');
  const REPO = execFileSync('git', ['rev-parse', '--show-toplevel'], { cwd: HERE, encoding: 'utf8' }).trim();
  const show = (rel) => execFileSync('git', ['show', `${V2_PIN}:${rel}`], { cwd: REPO, encoding: 'utf8', maxBuffer: 64 << 20 });
  const APP_SHIM_C = `// v4-Orakel-Hülle (Strang C), kein V2-Code
globalThis.workjetProjectControl = workjetProjectControl;
const WORKJET_PROJECT_CONTROL_MAX_RESULTS = 100;
async function workjetProjectControl(request = {}, diagnostics = {}) {
  return __wjb.project(globalThis, request, diagnostics);
}
async function waitForSyncBridgeReady() {}
`;
  const factory = Object.freeze({
    project(g, request, diagnostics) {
      const env = {
        get state() { return g.state; },
        actorContext: (session) => g.actorContext(session),
        newId: () => (typeof g.newId === 'function' ? g.newId() : globalThis.crypto.randomUUID()),
        now: () => (typeof g.Date?.now === 'function' ? g.Date.now() : Date.now()),
      };
      return createWorkjetBridges(env).workjetProjectControl(request, diagnostics);
    },
  });
  const MEETING_JS = JSON.stringify(pathToFileURL(path.resolve(HERE, '../actions/meeting.js')).href);
  const REDIRECT_C = {
    'shared/speech-dictation.mjs': `export { requestDictation, DICTATION_METHOD, DICTATION_CAPABILITY } from ${MEETING_JS};\n`,
    'shared/speech-settings-control.mjs': `export { requestSpeechSettings, speechSettingsFailure } from ${MEETING_JS};\n`,
    'shared/workjet-calendar-native.mjs': `export { readWorkjetCalendar, CALENDAR_READ_METHOD } from ${MEETING_JS};\n`,
    'shared/jour-fixe-speech.mjs': `export { requestJourFixeSpeech } from ${MEETING_JS};\n`,
    'shared/jour-fixe-speech-contract.mjs': `export { JOUR_FIXE_SPEECH_METHOD, JOUR_FIXE_SPEECH_CAPABILITY } from ${MEETING_JS};\n`,
  };
  function materializeC(name, testFile) {
    const root = path.join(os.tmpdir(), `wjb-c-oracle-${process.pid}-${name}`);
    const bos = path.join(root, 'src/apps/business-os');
    fs.mkdirSync(path.join(bos, 'shared'), { recursive: true });
    fs.writeFileSync(path.join(bos, 'app.js'), APP_SHIM_C);
    const seen = new Set();
    const put = (relBos, text) => {
      const out = path.join(bos, relBos);
      fs.mkdirSync(path.dirname(out), { recursive: true });
      fs.writeFileSync(out, text);
    };
    const copy = (relBos) => {
      if (seen.has(relBos)) return;
      seen.add(relBos);
      let text;
      if (Object.hasOwn(REDIRECT_C, relBos)) text = REDIRECT_C[relBos];
      else if (/^shared\/[^/]+\.generated\.mjs$/.test(relBos)) text = fs.readFileSync(path.join(BOS_DIR, relBos), 'utf8');
      else text = show(`src/apps/business-os/${relBos}`);
      if (relBos === `shared/${testFile}`) {
        text = text
          .replace(/^import \{?\s*test\s*\}? from ["']node:test["'];$/m, `import test from ${JSON.stringify(pathToFileURL(path.join(HERE, 'oracle-test-shim.mjs')).href)};`)
          .replace(/^import vm from 'node:vm';$/m, `import vm from ${JSON.stringify(pathToFileURL(path.join(HERE, 'oracle-vm-shim.mjs')).href)};`);
      }
      put(relBos, text);
      for (const m of text.matchAll(/from ["'](\.\.?\/[^'"?]+)(?:\?[^'"]*)?["']/g)) {
        copy(path.posix.normalize(path.posix.join(path.posix.dirname(relBos), m[1])));
      }
      for (const m of text.matchAll(/new URL\('\.\.\/\.\.\/\.\.\/(core\/rxdb\/tests\/fixtures\/[^']+)'/g)) {
        const out = path.join(root, 'src', m[1]);
        fs.mkdirSync(path.dirname(out), { recursive: true });
        fs.writeFileSync(out, show(`src/${m[1]}`));
      }
    };
    copy(`shared/${testFile}`);
    return path.join(bos, 'shared', testFile);
  }
  async function runOracleC(name, testFile, allow) {
    const registered = [];
    const skipped = [];
    globalThis.__WJB_ORACLE = { factory, allow: (n) => allow(n), registered, skipped };
    await import(pathToFileURL(materializeC(name, testFile)).href);
    return { registered, skipped };
  }
  const MEETING_READ = new Set([
    'meeting read returns null or the authorized typed native meeting for a verified alias',
    'meeting read rejects foreign, malformed and uncorrelated confirmations',
  ]);
  // [Name, V2-Testdatei, Freigabe, erwartete Anzahl]. jour-fixe-narration.test.mjs nicht: narration.read ist
  // unsupported (K-WJ-2 Bereichslesen fehlt im Stub/Vertrag).
  const ORACLE_FILES_C = [
    ['meeting-read', 'workjet-project-control.test.mjs', (n) => MEETING_READ.has(n), 2],
    ['dictation', 'speech-dictation.test.mjs', () => true, 7],
    ['speech-settings', 'speech-settings-control.test.mjs', () => true, 5],
    ['jour-fixe-speech', 'jour-fixe-speech.test.mjs', () => true, 2],
    ['calendar', 'workjet-calendar-native.test.mjs', () => true, 5],
  ];
  for (const [name, testFile, allowFn, expected] of ORACLE_FILES_C) {
    const { registered, skipped } = await runOracleC(name, testFile, allowFn);
    test(`[V2-Orakel] ${testFile} (Strang C): ${registered.length} gegen v4, ${skipped.length} nicht portiert`, () => {
      assert.equal(registered.length, expected, 'Anzahl freigegebener V2-Proben (Pin geändert?)');
    });
  }

  // ---- E26 gegen den echten Kern-Stub (Pin 8489b4e): Sprachsitzung = langlebiger open-Befehl ----
  const { adaptKernel } = await import('../../kernel-adapter.js');
  const { createV2Compat, isUnsupported } = await import('../v2-compat.js');
  const { checkResponse } = await import('./contract-checkers.mjs');
  const STUB_DIR = process.env.CTOX_APP_API_STUB || path.join(os.homedir(), '.local/state/shell-v4/stub-pins/64acc2c');
  const { createStubKernel } = await import(pathToFileURL(path.join(STUB_DIR, 'ctox-app-api-stub.mjs')).href);
  let nC = 0;
  const mkC = async () => {
    const k = createStubKernel({ instanceId: `wjbc-${process.pid}-${++nC}`, userId: 'owner-1', latencyMs: 2, storage: 'memory',
      actor: { id: 'owner-1', display_name: 'Owner', role: 'admin', permissions: [] } });
    await k.ready;
    return k;
  };
  const docOf = (r) => r?.doc ?? r?.value ?? r?.data ?? r;
  const until = async (fn, ms = 3000) => {
    const end = Date.now() + ms;
    for (;;) { const v = await fn(); if (v) return v; if (Date.now() > end) throw new Error('until: Frist'); await new Promise((r) => setTimeout(r, 5)); }
  };
  const C1 = '11111111-1111-4111-8111-111111111111';
  const C2 = '33333333-3333-4333-8333-333333333333';
  const C3 = '44444444-4444-4444-8444-444444444444';
  const C4 = '55555555-5555-4555-8555-555555555555';
  const S1 = '22222222-2222-4222-8222-222222222222';

  // Simuliert die E26-Kernroute: open läuft, bis finish/cancel ihn freigibt; Zwischenstände schreibt der „Kern“ als
  // running+result in das open-Befehlsdokument.
  // Stub-Befund K-WJ-6: der Stub legt das Handler-Ergebnis als JSON-Merge-Patch ab (mergePatch, null = Feld löschen),
  // explizite null-Felder (text/error/meeting: null, von den V2-Verträgen verlangt) verschwinden. Umgehung nur hier im
  // Test: das volle Ergebnis vorab als Dokument schreiben (sim.push ersetzt, behält null) und ohne null-Felder zurückgeben.
  const stripNulls = (v) => (v && typeof v === 'object' && !Array.isArray(v)
    ? Object.fromEntries(Object.entries(v).filter(([, x]) => x !== null).map(([key, x]) => [key, stripNulls(x)])) : v);
  async function respondKeepingNulls(k, id, result) {
    const cur = docOf(await k.sim.inspect('business_commands', id));
    await k.sim.push('business_commands', [{ ...cur, status: 'running', result }]);
    // Stub ≥ 637caf6 (E27, K-WJ-6): result ist ein Wert, null bleibt erhalten – kein Umweg mehr über ein gestripptes Ende.
    return result;
  }
  function simulateSession(k, type, openResult) {
    const seen = [];
    let release;
    const released = new Promise((r) => { release = r; });
    let openId = null;
    const partial = async (result) => {
      const cur = docOf(await k.sim.inspect('business_commands', openId));
      await k.sim.push('business_commands', [{ ...cur, status: 'running', result }]);
    };
    k.sim.onCommand(type, async (payload, ctx) => {
      seen.push({ payload, id: ctx.command_id });
      if (payload.op === 'open') {
        openId = ctx.command_id;
        // erster Zwischenstand, sobald der Befehl läuft
        setTimeout(() => { partial(openResult).catch(() => {}); }, 10);
        return released;
      }
      if (payload.op === 'finish' || payload.op === 'cancel') {
        const final = payload.op === 'finish' ? { ...openResult, op: 'finish', commandId: payload.commandId, state: 'finished', text: 'fertiger Text' }
          : { ...openResult, op: 'cancel', commandId: payload.commandId, state: 'canceled' };
        setTimeout(() => release(final), 5);
        return respondKeepingNulls(k, ctx.command_id, final);
      }
      return respondKeepingNulls(k, ctx.command_id, { ...openResult, op: payload.op, commandId: payload.commandId, events: [] });
    });
    return { seen, partial, get openId() { return openId; } };
  }

  test('[Strang C] Vertragsabgleich: jede freigeschaltete Aktion der Gruppe C hat einen Prüfer und steht in INVENTORY.project.C', async () => {
    const { MEETING_ACTIONS, PREPARED: PREPARED_C } = await import('../actions/meeting.js');
    const { INVENTORY } = await import('../index.js');
    const { CHECKERS } = await import('./contract-checkers.mjs');
    for (const action of Object.keys(MEETING_ACTIONS)) {
      assert.ok(CHECKERS[action], `Prüfer für ${action}`);
      assert.ok(INVENTORY.project.C.includes(action), `${action} in INVENTORY.project.C`);
    }
    const open = INVENTORY.project.C.filter((a) => !Object.hasOwn(MEETING_ACTIONS, a));
    // WJB-R: narration.read ist mit E26 (files.get range) freigeschaltet → keine offene C-Aktion mehr
    assert.deepEqual(open, [], 'alle C-Aktionen freigeschaltet');
    assert.equal(Object.hasOwn(PREPARED_C, 'project.jour_fixe.narration.read'), false);
  });

  test('[Strang C · E26] speech.dictation: open startet langlebigen Befehl, write/finish tragen session_command_id, read liest den Zwischenstand', async () => {
    const k = await mkC();
    try {
    const b = createWorkjetBridges(createV2Compat({ kernel: adaptKernel(k) }));
    const opened = { action: 'speech.dictation', commandId: C1, op: 'open', streamId: S1, state: 'open', events: [], text: null, error: null };
    const sim = simulateSession(k, 'ctox.workjet.speech.dictation.v1', opened);
    const o = await b.workjetProjectControl({ action: 'speech.dictation', commandId: C1, op: 'open' });
    assert.deepEqual(o, opened);
    assert.deepEqual(checkResponse('speech.dictation', o), []);
    assert.deepEqual(Object.keys(sim.seen[0].payload.scope), ['instanceId']);
    assert.equal(typeof sim.seen[0].payload.scope.instanceId, 'string');
    const openDoc = docOf(await k.sim.inspect('business_commands', sim.openId));
    assert.equal(openDoc.status, 'running', 'open-Befehl bleibt nach der open-Antwort aktiv');
    const w = await b.workjetProjectControl({ action: 'speech.dictation', commandId: C2, op: 'write', streamId: S1, sequence: 1, pcmBase64: 'AAAA' });
    assert.equal(w.op, 'write');
    assert.equal(sim.seen[1].payload.session_command_id, sim.openId);
    await sim.partial({ ...opened, events: [{ sequence: 3, text: 'Hallo' }] });
    const r = await until(async () => {
      const x = await b.workjetProjectControl({ action: 'speech.dictation', commandId: C3, op: 'read', streamId: S1, afterSequence: 0 });
      return x.events.length ? x : null;
    });
    assert.deepEqual(r, { ...opened, commandId: C3, op: 'read', events: [{ sequence: 3, text: 'Hallo' }] });
    assert.equal(sim.seen.length, 2, 'read schreibt keinen Befehl');
    const empty = await b.workjetProjectControl({ action: 'speech.dictation', commandId: C3, op: 'read', streamId: S1, afterSequence: 3 });
    assert.deepEqual(empty.events, []);
    const f = await b.workjetProjectControl({ action: 'speech.dictation', commandId: C4, op: 'finish', streamId: S1 });
    assert.equal(f.state, 'finished');
    assert.equal(f.text, 'fertiger Text');
    assert.equal(sim.seen[2].payload.session_command_id, sim.openId);
    assert.deepEqual(checkResponse('speech.dictation', f), []);
    await until(async () => docOf(await k.sim.inspect('business_commands', sim.openId))?.status === 'done');
    await assert.rejects(b.workjetProjectControl({ action: 'speech.dictation', commandId: C3, op: 'read', streamId: S1, afterSequence: 0 }),
      (e) => e.code === 'SPEECH_STREAM_CLOSED');
    } finally { await k.close(); }
  });

  test('[Strang C · E26] project.jour_fixe.speech: Sitzung über denselben Weg, read ohne requestId, Ereignisse nach afterSequence', async () => {
    const k = await mkC();
    try {
    const b = createWorkjetBridges(createV2Compat({ kernel: adaptKernel(k) }));
    const scopeReq = { action: 'project.jour_fixe.speech', projectId: 'p-1', meetingId: 'm-1', deckRevision: 2 };
    const opened = { streamId: S1, state: 'open', requestId: C1, events: [], receipt: null, error: null };
    const sim = simulateSession(k, 'ctox.workjet.jour_fixe.speech.v1', opened);
    const o = await b.workjetProjectControl({ ...scopeReq, op: 'open', requestId: C1 });
    assert.deepEqual(o, { ...scopeReq, op: 'open', ...opened });
    assert.deepEqual(checkResponse('project.jour_fixe.speech', o), []);
    assert.deepEqual({ ...sim.seen[0].payload.scope, instanceId: 'x' }, { instanceId: 'x', projectId: 'p-1', meetingId: 'm-1', deckRevision: 2 });
    await sim.partial({ ...opened, events: [{ sequence: 1, producerSequence: 1, text: 'a' }, { sequence: 2, producerSequence: 2, text: 'b' }] });
    const r = await until(async () => {
      const x = await b.workjetProjectControl({ ...scopeReq, op: 'read', streamId: S1, afterSequence: 1 });
      return x.events.length ? x : null;
    });
    assert.equal(Object.hasOwn(r, 'requestId'), false);
    assert.deepEqual(r.events, [{ sequence: 2, producerSequence: 2, text: 'b' }]);
    assert.deepEqual(checkResponse('project.jour_fixe.speech', r), []);
    const c = await b.workjetProjectControl({ ...scopeReq, op: 'cancel', streamId: S1 });
    assert.equal(c.state, 'canceled');
    assert.equal(sim.seen.at(-1).payload.session_command_id, sim.openId);
    } finally { await k.close(); }
  });

  test('[Strang C · E26] fehlende Kernroute: Sprache, Einstellungen, Kalender melden „nicht unterstützt“ in Workjet-Form', async () => {
    const k = await mkC();
    try {
    const b = createWorkjetBridges(createV2Compat({ kernel: adaptKernel(k) }));
    const workjetRegex = /(?:unsupported|unknown)\s+(?:workjet\s+)?(?:project[- ]control\s+)?action/i;
    const cases = [
      ['ctox.workjet.speech.dictation.v1', { action: 'speech.dictation', commandId: C1, op: 'open' }],
      ['ctox.workjet.jour_fixe.speech.v1', { action: 'project.jour_fixe.speech', projectId: 'p', meetingId: 'm', deckRevision: 1, op: 'open', requestId: C1 }],
      ['ctox.workjet.speech.settings.v1', { action: 'speech.settings.key', commandId: C1, secret: 'geheim-fixture' }],
      ['ctox.workjet.calendar.read.v1', { action: 'project.calendar.accounts.read', commandId: 'read-1' }],
    ];
    for (const [type, request] of cases) {
      await k.sim.failCommand(type, `unknown_command: ${type}`);
      await assert.rejects(b.workjetProjectControl(request), (e) => isUnsupported(e) && workjetRegex.test(e.message) && !e.message.includes('geheim-fixture'), type);
    }
    // WJB-R: narration.read ist freigeschaltet (E26); ohne Kernroute für meeting.read → „nicht unterstützt“ (Probe in Strang R)
    } finally { await k.close(); }
  });

  test('[Strang C] Kalender, Einstellungen, Besprechung über kernel.command: V2-Antwortform und Workjet-Vertrag', async () => {
    const k = await mkC();
    try {
    const b = createWorkjetBridges(createV2Compat({ kernel: adaptKernel(k) }));
    const { CALENDAR_SCHEMA } = await import('../../../../shared/workjet-calendar-contract.generated.mjs');
    const { JOUR_FIXE_SCHEMA } = await import('../../../../shared/workjet-jour-fixe-contract.generated.mjs');
    k.sim.onCommand('ctox.workjet.calendar.read.v1', (p) => ({ schema: CALENDAR_SCHEMA, request_id: p.request_id, action: 'accounts',
      data: { ok: true, truncated: false, accounts: [{ id: 'mine@example.test', calendar_id: 'account:mine', label: 'Kalender', supported: true }] } }));
    const cal = await b.workjetProjectControl({ action: 'project.calendar.accounts.read', commandId: 'read-1' });
    assert.deepEqual(checkResponse('project.calendar.accounts.read', cal), []);
    k.sim.onCommand('ctox.workjet.speech.settings.v1', (p) => ({ action: p.action, commandId: p.commandId, rate: 1 }));
    const pb = await b.workjetProjectControl({ action: 'speech.settings.playback', commandId: C1 });
    assert.deepEqual(checkResponse('speech.settings.playback', pb), []);
    k.sim.onCommand('ctox.workjet.jour_fixe.meeting.read', (p, ctx) => respondKeepingNulls(k, ctx.command_id, { ok: true, contract: JOUR_FIXE_SCHEMA, meeting: null }));
    const mr = await b.workjetProjectControl({ action: 'project.jour_fixe.meeting.read', commandId: 'mr-1', projectId: 'p-1' });
    assert.deepEqual(mr, { action: 'project.jour_fixe.meeting.read', commandId: 'mr-1', projectId: 'p-1', contract: JOUR_FIXE_SCHEMA, meeting: null });
    assert.deepEqual(checkResponse('project.jour_fixe.meeting.read', mr), []);
    k.sim.onCommand('ctox.workjet.jour_fixe.meeting.start', (p, ctx) => ({ ok: true, contract: JOUR_FIXE_SCHEMA,
      mutation: { operation_id: p.operation_id, meeting_id: p.meeting_id, project_id: 'p-1', revision: p.expected_revision + 1, state: 'live' } }));
    const ms = await b.workjetProjectControl({ action: 'project.jour_fixe.meeting.start', commandId: 'ms-1', projectId: 'p-1',
      operationId: 'op-1', meetingId: 'm-1', expectedRevision: 3 });
    assert.equal(ms.mutation.revision, 4);
    assert.deepEqual(checkResponse('project.jour_fixe.meeting.start', ms), []);
    } finally { await k.close(); }
  });
}
// Ende Strang C

// Strang A (v4-Proben) – dieselben Aktionen über den echten Kern-Stub (v2-compat → kernel.command/query/get):
// Antwortform = Workjet-Vertrag (contract-checkers.mjs), Projektion über findOne/find, fehlende Route → unsupported.
// Jede Probe schließt den Kernel in finally (sonst hält ein roter Fall den Testprozess offen).
{
  const os = await import('node:os');
  const path = await import('node:path');
  const { pathToFileURL } = await import('node:url');
  const { adaptKernel } = await import('../../kernel-adapter.js');
  const { createWorkjetBridges } = await import('../index.js');
  const { createV2Compat, isUnsupported } = await import('../v2-compat.js');
  const { checkResponse } = await import('./contract-checkers.mjs');
  const STUB_DIR = process.env.CTOX_APP_API_STUB || path.join(os.homedir(), '.local/state/shell-v4/stub-pins/64acc2c');
  const { createStubKernel } = await import(pathToFileURL(path.join(STUB_DIR, 'ctox-app-api-stub.mjs')).href);
  let seq = 0;
  async function withKernel(fn) {
    const k = createStubKernel({ instanceId: `wjb-a-${process.pid}-${++seq}`, userId: 'owner-1', latencyMs: 2, storage: 'memory',
      actor: { id: 'owner-1', display_name: 'Owner', role: 'admin', permissions: [] } });
    try {
      await k.ready;
      return await fn(k, createWorkjetBridges(createV2Compat({ kernel: adaptKernel(k) })));
    } finally { await k.close(); }
  }
  const exitAssessment = (projectId) => ({ contract: 'ctox.workjet.exit_model.v1', project_id: projectId, run_id: null,
    as_of: null, exit_date: null, refresh_due: null, status: 'blocked', missing_inputs: ['confirmed_resource_plan'],
    findings: [], sources: [], plan_summary: null, result: null, scenarios: [], history: [] });

  test('[v4 Strang A] project.create mit Arbeitskopie: upsert → Projektion (findOne/find) → Vertrag', () => withKernel(async (k, b) => {
    const types = [];
    k.sim.onCommand('ctox.workjet.project.upsert', async (payload) => {
      types.push('project.upsert');
      await k.sim.push('workjet_projects', [{ id: payload.project_id, name: payload.name, status: 'active', owner_user_id: 'owner-1', created_at_ms: 1_700_000_000_000 }]);
      return { ok: true, collection: 'workjet_projects' };
    });
    k.sim.onCommand('ctox.workjet.working_copy.upsert', async (payload) => {
      types.push('working_copy.upsert');
      await k.sim.push('workjet_working_copies', [{ id: `wc-${payload.project_id}`, project_id: payload.project_id, computer_id: payload.computer_id, path: payload.path, status: 'active', owner_user_id: 'owner-1' }]);
      return { ok: true };
    });
    const res = JSON.parse(JSON.stringify(await b.workjetProjectControl({ action: 'project.create', commandId: 'c-1', projectId: 'p-1',
      title: 'Projekt 1', createdAt: '2026-10-16T08:00:00.000Z', workingCopy: { computerId: 'mac', path: '/src/p1' } })));
    assert.deepEqual(checkResponse('project.create', res), []);
    assert.deepEqual(types, ['project.upsert', 'working_copy.upsert']);
    assert.equal(res.project.createdAt, new Date(1_700_000_000_000).toISOString());
    assert.deepEqual(res.project.workingCopies, [{ id: 'wc-p-1', computerId: 'mac', path: '/src/p1', status: 'active' }]);
  }));

  test('[v4 Strang A] project.configure, kpis.read|configure|refresh, exit_model.read|refresh|submit: Vertrag je Aktion', () => withKernel(async (k, b) => {
    k.sim.onCommand('ctox.workjet.project.upsert', (p) => ({ ok: true, collection: 'workjet_projects', owner_user_id: 'owner-1',
      project: { id: p.project_id, name: p.name, owner_user_id: 'owner-1', status: 'active', ...(p.repo_url ? { repo_url: p.repo_url } : {}) } }));
    const kpis = (p) => ({ ok: true, kpis: { project_id: p.project_id, revision: p.expected_revision === undefined ? 0 : p.expected_revision + 1, items: [] } });
    k.sim.onCommand('ctox.workjet.project.kpis.read', kpis);
    k.sim.onCommand('ctox.workjet.project.kpis.configure', kpis);
    k.sim.onCommand('ctox.workjet.project.kpis.refresh', (p) => ({ ...kpis(p), refresh: { completed: true, items: [] } }));
    for (const verb of ['read', 'refresh', 'submit']) {
      k.sim.onCommand(`ctox.workjet.exit_model.${verb}`, (p) => ({ ok: true, assessment: exitAssessment(p.project_id) }));
    }
    const cases = [
      { action: 'project.configure', commandId: 'c-1', projectId: 'p-1', title: 'Projekt 1', repoUrl: 'https://example.org/p1' },
      { action: 'project.kpis.read', commandId: 'c-2', projectId: 'p-1' },
      { action: 'project.kpis.configure', commandId: 'c-3', projectId: 'p-1', operationId: 'op-1', expectedRevision: 0, prompts: [] },
      { action: 'project.kpis.refresh', commandId: 'c-4', projectId: 'p-1' },
      { action: 'project.exit_model.read', commandId: 'c-5', projectId: 'p-1' },
      { action: 'project.exit_model.refresh', commandId: 'c-6', projectId: 'p-1', asOf: '2026-10-16' },
      { action: 'project.exit_model.submit', commandId: 'c-7', projectId: 'p-1', confirmRunId: 'run-1' },
    ];
    for (const request of cases) {
      const res = JSON.parse(JSON.stringify(await b.workjetProjectControl(request)));
      // Exit: der Stub streicht null-Felder aus Befehlsergebnissen (K-WJ-6, Probe unten); bis dahin nur Hülle + Korrelation.
      const errors = checkResponse(request.action, res)
        .filter((e) => !(request.action.startsWith('project.exit_model.') && / Pflichtfeld fehlt$/.test(e)));
      assert.deepEqual(errors, [], request.action);
      assert.equal(res.action, request.action);
    }
  }));

  // K-WJ-6 (Befund Strang A): der Kern-Stub wendet Merge-Patch-Semantik auf command.result an – Schlüssel mit null
  // verschwinden. Das Exit-Assessment hat Pflichtfelder „null erlaubt“ (run_id, as_of, exit_date, refresh_due,
  // plan_summary, result); Workjet würde die Antwort dann als Vertragsbruch („fehlgeschlagen“) lesen. Seit Stub
  // 637caf6 (E27, K-WJ-6) bleibt null erhalten; die Probe läuft scharf. Der echte Kern folgt (Transfer-Strang).
  test('[v4 Strang A] K-WJ-6: exit_model-Antwort über den Kern erfüllt den vollen Workjet-Vertrag (null-Felder erhalten)',
    () => withKernel(async (k, b) => {
      k.sim.onCommand('ctox.workjet.exit_model.read', (p) => ({ ok: true, assessment: exitAssessment(p.project_id) }));
      const res = JSON.parse(JSON.stringify(await b.workjetProjectControl({ action: 'project.exit_model.read', commandId: 'c-1', projectId: 'p-1' })));
      assert.deepEqual(checkResponse('project.exit_model.read', res), []);
    }));

  test('[v4 Strang A] fehlende Kernroute → „nicht unterstützt“ (Workjet-Merkmal); presentation.* ohne Kernroute ebenso (WJB-R)', () => withKernel(async (k, b) => {
    const workjetProjectRegex = /(?:unsupported|unknown)\s+(?:workjet\s+)?(?:project[- ]control\s+)?action/i;
    await k.sim.failCommand('ctox.workjet.exit_model.read', 'unknown_command: ctox.workjet.exit_model.read');
    await assert.rejects(b.workjetProjectControl({ action: 'project.exit_model.read', commandId: 'c-1', projectId: 'p-1' }),
      (e) => isUnsupported(e) && workjetProjectRegex.test(e.message));
    // WJB-R: presentation.* sind freigeschaltet (E26). Ohne Kernroute (unknown_command) melden sie „nicht unterstützt“.
    await k.sim.failCommand('ctox.workjet.presentation.read', 'unknown_command: ctox.workjet.presentation.read');
    await assert.rejects(b.workjetProjectControl({ action: 'project.presentation.read', commandId: 'c-pr', projectId: 'p-1', meetingId: 'm-1' }),
      (e) => isUnsupported(e) && workjetProjectRegex.test(e.message), 'project.presentation.read');
  }));

  // project.init: V2 hat nur eine Quelltext-Regex-Probe (workjet-project-control.test.mjs Z. 2039). Orchestrator-
  // Entscheid: Freigabe mit dieser v4-Probe. Erwartung wörtlich aus dem V2-Rumpf:
  //   app.js:14647-14669  Schlüssel {action, commandId, projectId, operationId}; Befehl ctox.workjet.project.init,
  //                       record_id = projectId, payload {project_id, operation_id}; Quittung korreliert über command_id,
  //                       ok/status completed, result.ok, result.project_id, result.steps Array, sonst
  //                       „Project init returned an uncorrelated or unsuccessful response.“; Antwort
  //                       {action, commandId, projectId, operationId, steps: ≤ 12 × {id, state, detail ≤ 256}} (String()).
  //   app.js:15002-15008  Datenebene ohne commandBus/business_commands → „Workjet supervisor control is not ready.“
  //   app.js:13906-13915  Sitzungs-/Akteurprüfung nach der Quittung („uncorrelated“).
  const INIT_FAIL = /^Project init returned an uncorrelated or unsuccessful response\.$/;
  const initRequest = { action: 'project.init', commandId: 'init-1', projectId: 'p-1', operationId: 'op-1' };
  test('[v4 Strang A] project.init: Befehl, Payload, Antwortform und Schrittkappung wie V2 (app.js:14647-14669)', () => withKernel(async (k, b) => {
    const seen = [];
    const steps = Array.from({ length: 13 }, (_, i) => ({ id: `step-${i}`, state: i ? 'pending' : 'done', detail: 'x'.repeat(300), extra: true }));
    steps[1] = { id: 7, state: 'running' };
    k.sim.onCommand('ctox.workjet.project.init', (payload, ctx) => { seen.push({ payload, ctx }); return { ok: true, project_id: payload.project_id, steps }; });
    const res = JSON.parse(JSON.stringify(await b.workjetProjectControl(initRequest)));
    assert.deepEqual(checkResponse('project.init', res), []);
    assert.deepEqual(Object.keys(res), ['action', 'commandId', 'projectId', 'operationId', 'steps']);
    assert.deepEqual({ ...res, steps: undefined }, { action: 'project.init', commandId: 'init-1', projectId: 'p-1', operationId: 'op-1', steps: undefined });
    assert.equal(res.steps.length, 12);
    assert.deepEqual(res.steps[0], { id: 'step-0', state: 'done', detail: 'x'.repeat(256) });
    assert.deepEqual(res.steps[1], { id: '7', state: 'running', detail: '' });
    assert.deepEqual(seen.map((s) => s.payload), [{ project_id: 'p-1', operation_id: 'op-1' }]);
    const doc = await k.sim.inspect('business_commands', seen[0].ctx.command_id);
    const stored = doc?.doc ?? doc?.value ?? doc?.data ?? doc;
    assert.equal(stored.type, 'ctox.workjet.project.init');
    assert.equal(stored.record_id, 'p-1');
  }));

  test('[v4 Strang A] project.init: fremde Felder vor dem Versand, unkorrelierte/gescheiterte Quittung, fehlende Datenebene', () => withKernel(async (k, b) => {
    let calls = 0;
    let reply = (p) => ({ ok: true, project_id: p.project_id, steps: [] });
    k.sim.onCommand('ctox.workjet.project.init', (p) => { calls += 1; return reply(p); });
    for (const extra of [{ ownerUserId: 'foreign' }, { steps: [] }]) {
      await assert.rejects(b.workjetProjectControl({ ...initRequest, ...extra }), /^Error: Unsupported Workjet project payload field: /);
    }
    await assert.rejects(b.workjetProjectControl({ ...initRequest, operationId: '' }), /Invalid Workjet project operationId\./);
    assert.equal(calls, 0, 'ungültige Anfragen erreichen den Kern nicht');
    for (const bad of [(p) => ({ ok: true, project_id: 'foreign', steps: [] }), (p) => ({ ok: false, project_id: p.project_id, steps: [] }),
      (p) => ({ ok: true, project_id: p.project_id }), (p) => ({ ok: true, project_id: p.project_id, steps: {} })]) {
      reply = bad;
      // WJB-R (E26 idempotency_key): je Antwortvariante eine eigene commandId (dieselbe ID = dieselbe Operation)
      await assert.rejects(b.workjetProjectControl({ ...initRequest, commandId: `init-bad-${calls}` }), (e) => INIT_FAIL.test(e.message));
    }
    reply = (p) => ({ ok: true, project_id: p.project_id, steps: [] });
    await k.sim.failCommand('ctox.workjet.project.init', 'forbidden: policy');
    await assert.rejects(b.workjetProjectControl({ ...initRequest, commandId: 'init-forbidden' }), (e) => INIT_FAIL.test(e.message)); // WJB-R
    await k.sim.failCommand('ctox.workjet.project.init', 'unknown_command: ctox.workjet.project.init');
    await assert.rejects(b.workjetProjectControl({ ...initRequest, commandId: 'init-unknown' }), (e) => isUnsupported(e) && /Unsupported Workjet project control action/.test(e.message)); // WJB-R
    const compat = createV2Compat({ kernel: adaptKernel(k) });
    const noPlane = createWorkjetBridges(compat);
    compat.state.commandBus = null; // v4-allow: debt L-42
    const before = calls;
    await assert.rejects(noPlane.workjetProjectControl(initRequest), (e) => e.message === 'Workjet supervisor control is not ready.'); // v4-allow: code (V2-Fehlertext, Workjet vergleicht wörtlich)
    assert.equal(calls, before);
  }));

  test('[v4 Strang A] project.init: Sitzungswechsel während der Quittung → „uncorrelated“ (app.js:14660)', () => withKernel(async (k) => {
    const compat = createV2Compat({ kernel: adaptKernel(k) });
    const b = createWorkjetBridges(compat);
    k.sim.onCommand('ctox.workjet.project.init', (p) => { compat.state.session = Object.freeze({ replaced: true }); return { ok: true, project_id: p.project_id, steps: [] }; });
    await assert.rejects(b.workjetProjectControl(initRequest), (e) => INIT_FAIL.test(e.message));
  }));
}
// Ende Strang A (v4-Proben)

// Strang D – eigene Orakel-Hülle (v2-oracle.mjs gehört dem Orchestrator). Aufbau wie Strang B:
// <tmp>/src/apps/business-os/{app.js, mobile-host.js, shared/…}. Ersetzt wird nur:
//   - app.js → Weiterleitungshülle mit den Abschnittsmarken, an denen die V2-Tests den Quelltext schneiden
//     (Ereignisse, Rechner, Sitzung); jeder Abschnitt ruft die v4-Brücke (__wjb.<brücke>) mit dem vm-`state`.
//   - shared/workjet-grok-native.mjs, shared/workjet-provider-native.mjs → Re-Export aus actions/instance.js.
//   - mobile-host.js → Platzhalter (die Tests lesen die Datei beim Laden; ihre Quelltext-Proben sind nicht portiert).
//   - 'node:test' / 'node:vm' → die Orakel-Shims (Freigabe je Testname, __wjb im vm-Kontext).
// Nicht übernommen (Quelltext-Regex auf app.js/mobile-host.js, keine Verhaltensorakel):
//   computer „Workjet guest computer control is installed and WebRTC/RxDB-only“, „… project control stays …“,
//   session „Workjet session control is installed …“, „Mobile host bridges session.control …“,
//   events „Workjet session transfer events install a bounded host API and mobile output“,
//   provider „shell integration captures the existing command bus and instance authority“.
{
  const { execFileSync } = await import('node:child_process');
  const fs = await import('node:fs');
  const os = await import('node:os');
  const path = await import('node:path');
  const vm = (await import('node:vm')).default;
  const { fileURLToPath, pathToFileURL } = await import('node:url');
  const { createWorkjetBridges } = await import('../index.js');
  const { V2_PIN } = await import('./v2-oracle.mjs');
  const { createWorkjetSessionEvents } = await import('../actions/session.js');
  const HERE = path.dirname(fileURLToPath(import.meta.url));
  const REPO = execFileSync('git', ['rev-parse', '--show-toplevel'], { cwd: HERE, encoding: 'utf8' }).trim();
  const show = (rel) => execFileSync('git', ['show', `${V2_PIN}:src/apps/business-os/${rel}`], { cwd: REPO, encoding: 'utf8', maxBuffer: 64 << 20 });
  const INSTANCE_JS = pathToFileURL(path.resolve(HERE, '../actions/instance.js')).href;
  // Reihenfolge wie app.js (Ereignisse 472, Rechner 13364, Projekt 13823, waitForSyncBridgeReady 15458, Sitzung 15575).
  const APP_SHIM_D = `// v4-Orakel-Hülle (Strang D), kein V2-Code
const WORKJET_SESSION_EVENT_BUFFER_LIMIT = 64;
function createWorkjetSessionEvents() {
  return __wjb.events(globalThis);
}
function installAdvancedStatusInterface() {}
const WORKJET_COMPUTER_CONTROL_MAX_RESULTS = 100;
async function workjetComputerControl(request = {}) {
  return __wjb.computer(globalThis, request);
}
const WORKJET_PROJECT_CONTROL_MAX_RESULTS = 100;
async function waitForSyncBridgeReady() {}
const WORKJET_SESSION_CONTROL_MAX_RESULTS = 100;
async function workjetSessionControl(request = {}) {
  return __wjb.session(globalThis, request);
}
function isStaleDataPlaneGeneration() {}
`;
  // V2-Umgebung live aus dem vm-Global des Tests (state, actorContext, newId, Uhr, V2-Hilfen außerhalb der Abschnitte).
  const envOf = (g) => ({
    get state() { return g.state; },
    actorContext: (session) => g.actorContext(session),
    newId: () => (typeof g.newId === 'function' ? g.newId() : globalThis.crypto.randomUUID()),
    now: () => (typeof g.Date?.now === 'function' ? g.Date.now() : Date.now()),
    waitForSyncBridgeReady: (bridge, ms) => (typeof g.waitForSyncBridgeReady === 'function' ? g.waitForSyncBridgeReady(bridge, ms) : undefined),
    kernelReady: () => (typeof g.waitForDataPlaneReady === 'function' ? g.waitForDataPlaneReady() : undefined),
    get hostBridge() { return g.workjetHostBridge; },
  });
  // Das vm-Shim liest __WJB_ORACLE.factory erst beim Testlauf (nach allen Blöcken): die Fabrik der vorigen Blöcke
  // (project) bleibt deshalb erhalten, Strang D ergänzt nur computer/session/events.
  const factory = Object.freeze({
    ...(globalThis.__WJB_ORACLE?.factory || {}),
    computer: (g, request) => createWorkjetBridges(envOf(g)).workjetComputerControl(request),
    session: (g, request) => createWorkjetBridges(envOf(g)).workjetSessionControl(request),
    events: (g) => createWorkjetSessionEvents(envOf(g)),
  });
  function materializeD(name, testFile) {
    const root = path.join(os.tmpdir(), `wjb-d-oracle-${process.pid}-${name}`);
    const bos = path.join(root, 'src/apps/business-os');
    fs.mkdirSync(path.join(bos, 'shared'), { recursive: true });
    fs.writeFileSync(path.join(bos, 'app.js'), APP_SHIM_D);
    fs.writeFileSync(path.join(bos, 'mobile-host.js'), '// v4-Orakel-Hülle (Strang D): Platzhalter, Port in v4/shell/workjet/host-bridge.js\n');
    const seen = new Set();
    const copy = (rel) => {
      if (seen.has(rel)) return;
      seen.add(rel);
      let text;
      if (rel === 'workjet-grok-native.mjs') text = `export { requestWorkjetGrok, GROK_METHOD, GROK_CAPABILITY } from ${JSON.stringify(INSTANCE_JS)};\n`;
      else if (rel === 'workjet-provider-native.mjs') text = `export { projectNativeProviderRegistry, requestNativeProviders } from ${JSON.stringify(INSTANCE_JS)};\n`;
      else text = show(`shared/${rel}`);
      if (rel === testFile) {
        text = text
          .replace(/^import test from 'node:test';$/m, `import test from ${JSON.stringify(pathToFileURL(path.join(HERE, 'oracle-test-shim.mjs')).href)};`)
          .replace(/^import vm from 'node:vm';$/m, `import vm from ${JSON.stringify(pathToFileURL(path.join(HERE, 'oracle-vm-shim.mjs')).href)};`);
      }
      fs.writeFileSync(path.join(bos, 'shared', rel), text);
      for (const m of text.matchAll(/from '\.\/([^']+?)(?:\?[^']*)?'/g)) copy(m[1]);
    };
    copy(testFile);
    return path.join(bos, 'shared', testFile);
  }
  async function runOracleD(name, testFile, allow) {
    const registered = [];
    const skipped = [];
    globalThis.__WJB_ORACLE = { factory, allow: (n) => allow(n), registered, skipped };
    await import(pathToFileURL(materializeD(name, testFile)).href);
    return { registered, skipped };
  }
  const SOURCE_ONLY_D = new Set([
    'Workjet guest computer control is installed and WebRTC/RxDB-only',
    'Workjet guest project control stays WebRTC/RxDB-only without host details',
    'Workjet session control is installed and stays on the projected RxDB command plane',
    'Mobile host bridges session.control with bounded result errors',
    'Workjet session transfer events install a bounded host API and mobile output',
    'shell integration captures the existing command bus and instance authority',
  ]);
  const ORACLE_FILES_D = [
    ['grok', 'workjet-grok-native.test.mjs', 4],
    ['provider', 'workjet-provider-native.test.mjs', 14],
    ['computer', 'workjet-computer-control.test.mjs', 10],
    ['session', 'workjet-session-control.test.mjs', 0], // E32: die eine Verhaltensprobe läuft abgeleitet (E32_DERIVED_D)
    ['events', 'workjet-session-events.test.mjs', 3],
  ];
  // E32: V2-Proben, die für eine auf lokale Kernel-Abfrage umgestellte Leseaktion einen Kern-Befehl erwarten, sind für
  // v4 überholt. Sie laufen nicht unverändert, sondern als abgeleitetes Orakel (unten, E32_DERIVED_D): derselbe
  // V2-Testtext, nur die Befehls-Erwartung der Leseaktion ist durch „kein Befehl“ ersetzt (Liste in E32.md).
  const E32_EXCEPTIONS_D = new Set([
    'Workjet session create/list is idempotent and transfer outcomes pass through terminal documents',
  ]);
  for (const [name, testFile, expected] of ORACLE_FILES_D) {
    const { registered, skipped } = await runOracleD(name, testFile, (n) => !SOURCE_ONLY_D.has(n) && !E32_EXCEPTIONS_D.has(n));
    test(`[V2-Orakel] ${testFile} (Strang D): ${registered.length} gegen v4, ${skipped.length} nicht portiert`, () => {
      assert.equal(registered.length, expected, 'Anzahl freigegebener V2-Proben (Pin geändert?)');
      assert.ok(skipped.every((n) => SOURCE_ONLY_D.has(n) || E32_EXCEPTIONS_D.has(n)), `nur Quelltext-Proben und E32-Ausnahmen ausgelassen: ${skipped.join(' | ')}`);
    });
  }
  // E32 abgeleitetes Orakel: V2-Testtext workjet-session-control.test.mjs @ e35a116ae mit genau zwei Ersetzungen
  // (Testname mit Präfix; die vier Zeilen zur session.list-Befehlserwartung → „kein session.list-Befehl“). Alle übrigen
  // Erwartungen (create idempotent, list = publicSession, transfer.start/pause_ack, Fehlerfälle) bleiben wörtlich V2.
  const E32_DERIVED_D = [{
    file: 'workjet-session-control.test.mjs',
    name: 'Workjet session create/list is idempotent and transfer outcomes pass through terminal documents',
    replace: [[
      "  const listCommand = dispatched.find(({ command }) => command.command_type === 'ctox.workjet.session.list').command;\n"
      + "  assert.equal(listCommand.record_id, 'owner-1');\n"
      + "  assert.equal(listCommand.payload.limit, 100);\n"
      + "  assert.deepEqual(Object.keys(listCommand.payload), ['limit']);\n",
      "  assert.equal(dispatched.some(({ command }) => command.command_type === 'ctox.workjet.session.list'), false); // E32\n",
    ]],
  }];
  for (const { file: testFile, name: v2Name, replace } of E32_DERIVED_D) {
    const derivedName = `[E32 abgeleitet] ${v2Name}`;
    const target = materializeD(`e32-${testFile}`, testFile);
    let text = fs.readFileSync(target, 'utf8');
    const ops = [[`test(${JSON.stringify(v2Name).replace(/"/g, "'")},`, `test(${JSON.stringify(derivedName).replace(/"/g, "'")},`], ...replace];
    const applied = ops.map(([from, to]) => {
      const count = text.split(from).length - 1;
      text = text.replace(from, to);
      return count;
    });
    fs.writeFileSync(target, text);
    const registered = [];
    const skipped = [];
    globalThis.__WJB_ORACLE = { factory, allow: (n) => n === derivedName, registered, skipped };
    await import(pathToFileURL(target).href);
    test(`[E32] ${testFile}: abgeleitetes Orakel „${v2Name}“ – je Ersetzung genau 1 Fundstelle, 1 Probe registriert`, () => {
      assert.deepEqual(applied, ops.map(() => 1), 'Ersetzung passt nicht mehr zum V2-Text (Pin geändert?)');
      assert.deepEqual(registered, [derivedName]);
    });
  }

  // Gerät: V2 hat keinen Verhaltenstest für workjetBusinessOsDeviceControl (nur Quelltext-Proben in
  // rxdb/tests/workjet-device-control-smoke.mjs). Orakel ist deshalb der V2-Rumpf selbst (app.js:7844-7877 am Pin):
  // gleiche Eingabe → gleiche Antwort bzw. gleicher Fehler (name, message, code) und derselbe requestNative-Aufruf.
  const appV2 = show('app.js');
  const deviceStart = appV2.indexOf('const WORKJET_DEVICE_CONTROL_ACTIONS');
  const deviceEnd = appV2.indexOf('function encodeWorkjetPairingPayload', deviceStart);
  const deviceSource = appV2.slice(deviceStart, deviceEnd);
  const deviceCases = [
    { action: 'binding.list' },
    { action: 'binding.revoke', bindingId: 'b-1' },
    { action: 'invite.create', ttlSeconds: 300, displayName: 'Mac' },
    { action: 'invite.revoke', inviteId: 'i-1' },
    { action: 'binding.list', extra: 1 },
    { action: 'invite.create', bindingId: 'b-1' },
    { action: 'binding.revoke', inviteId: 'x' },
  ];
  const outcome = async (fn) => {
    try { return { ok: JSON.parse(JSON.stringify(await fn())) }; } catch (e) { return { error: { name: e?.name, message: e?.message, code: e?.code } }; }
  };
  const syncVariants = {
    echo: () => ({ calls: [], async requestNative(method, params, opts) { this.calls.push(JSON.parse(JSON.stringify([method, params, opts]))); return { revoked: true, schema: 'ctox.workjet-device-bindings.v1', bindings: [], echo: params }; } }),
    peer: () => ({ calls: [], async requestNative(...a) { this.calls.push(JSON.parse(JSON.stringify(a))); throw Object.assign(new Error('Native WebRTC peer is not connected'), { code: 'PEER_UNAVAILABLE' }); } }),
    forbidden: () => ({ calls: [], async requestNative(...a) { this.calls.push(JSON.parse(JSON.stringify(a))); throw new Error('workjet device management is not allowed'); } }),
    none: () => ({ calls: [] }),
  };
  test('[V2-Orakel · Differenz] workjetBusinessOsDeviceControl: v4 = V2-Rumpf (Antwort, Fehler, Nativaufruf) für alle vier Aktionen', async () => {
    let compared = 0;
    for (const [variant, makeSync] of Object.entries(syncVariants)) {
      for (const request of deviceCases) {
        const v2Sync = makeSync();
        const v2 = { state: { sync: variant === 'none' ? {} : v2Sync }, globalThis: null };
        const ctx = vm.createContext({ state: v2.state, TypeError, Error, Set, String, Object });
        vm.runInContext(`${deviceSource}\nthis.__v2 = globalThis.workjetBusinessOsDeviceControl;`, ctx);
        const v4Sync = makeSync();
        const v4 = createWorkjetBridges({ state: { sync: variant === 'none' ? {} : v4Sync }, actorContext: () => ({ id: '' }), newId: () => 'x', now: () => Date.now() });
        const a = await outcome(() => ctx.__v2(JSON.parse(JSON.stringify(request))));
        const b = await outcome(() => v4.workjetBusinessOsDeviceControl(JSON.parse(JSON.stringify(request))));
        assert.deepEqual(b, a, `${variant} ${JSON.stringify(request)}`);
        assert.deepEqual(v4Sync.calls, v2Sync.calls, `${variant} ${JSON.stringify(request)}: Nativaufruf`);
        compared += 1;
      }
    }
    assert.equal(compared, Object.keys(syncVariants).length * deviceCases.length);
  });
  test('[v4 Strang D] Gerät: fehlende Kernroute → code CTOX_WEBRTC_CAPABILITY_MISSING (Workjet: unsupported); Abweichungen nur außerhalb des Workjet-Pfads', async () => {
    const { unsupportedError } = await import('../v2-compat.js');
    const sync = { async requestNative(method) { throw unsupportedError('project', method, 'unknown_command'); } };
    const b = createWorkjetBridges({ state: { sync }, actorContext: () => ({ id: '' }), newId: () => 'x', now: () => Date.now() });
    for (const request of deviceCases.slice(0, 4)) {
      await assert.rejects(b.workjetBusinessOsDeviceControl(request), (e) => e.code === 'CTOX_WEBRTC_CAPABILITY_MISSING');
    }
    // Router-Ebene (index.js, nicht Strang D): unbekannte Aktion und Nicht-Objekt tragen andere Texte als V2
    // („Workjet device action is unsupported.“ / „Workjet device request must be an object.“). Workjet sendet beides
    // nie (Schema WorkjetDeviceWebRtcRequestV1) und läse beide Formen gleich (guest_failed); offen an den Orchestrator.
    await assert.rejects(b.workjetBusinessOsDeviceControl({ action: 'binding.unknown' }));
    await assert.rejects(b.workjetBusinessOsDeviceControl(null), TypeError);
  });
}
// Ende Strang D

// Strang D – v4-Proben gegen den echten Kern-Stub (Stub-Pin 8489b4e, Speicher memory): v2-compat + Router + Vertrag +
// Einhängen. Ergänzen die V2-Orakel dort, wo V2 nur gegen Attrappen prüft (echte Quittung ohne transport/record_id,
// Enddokument unter der Kern-Befehls-ID, unknown_command → Workjet-Merkmal je Brücke, Ereignisstrom über kernel.query).
// Ergebnisse ohne null-Felder: der Stub streicht null aus command.result (K-WJ-6/K-WJ-B1, Kern-Lücke der Stränge A/B).
{
  const os = await import('node:os');
  const path = await import('node:path');
  const { pathToFileURL } = await import('node:url');
  const { adaptKernel } = await import('../../kernel-adapter.js');
  const { createWorkjetBridges, installWorkjetBridges, INVENTORY, inventoryList } = await import('../index.js');
  const { createV2Compat, isUnsupported } = await import('../v2-compat.js');
  const { checkResponse, CHECKERS, sessionTransferEventD } = await import('./contract-checkers.mjs');
  const STUB_DIR = process.env.CTOX_APP_API_STUB || path.join(os.homedir(), '.local/state/shell-v4/stub-pins/64acc2c');
  const { createStubKernel } = await import(pathToFileURL(path.join(STUB_DIR, 'ctox-app-api-stub.mjs')).href);
  const WORKJET_UNSUPPORTED = /(?:unsupported|unknown)\s+(?:workjet\s+)?(?:project[- ]control\s+)?action/i;
  const docOf = (r) => r?.doc ?? r?.value ?? r?.data ?? r;
  const json = (v) => JSON.parse(JSON.stringify(v));
  let n = 0;
  const mkKernel = async () => {
    const k = createStubKernel({ instanceId: `wjb-d-${process.pid}-${++n}`, userId: 'owner-1', latencyMs: 2, storage: 'memory',
      actor: { id: 'owner-1', display_name: 'Owner', role: 'admin', permissions: [] } });
    await k.ready;
    await publishShell(k);
    return k;
  };
  const withKernel = (fn) => async () => {
    const k = await mkKernel();
    const compat = createV2Compat({ kernel: adaptKernel(k) });
    try { await fn(k, createWorkjetBridges(compat), compat); } finally { compat.dispose(); await k.close(); }
  };
  const OP = '5fa0a379-6d66-4b73-8bea-cc6591b31542';
  const D_ACTIONS = { project: INVENTORY.project.D, computer: INVENTORY.computer.D, session: INVENTORY.session.D, device: INVENTORY.device.D };
  // instance.providers.secret / .models.check bleiben unsupported (WJB-D.md).
  const D_UNSUPPORTED = new Set(['instance.providers.secret', 'instance.providers.models.check']);

  test('[v4 Strang D] Inventar: alle D-Aktionen freigeschaltet (außer 2 begründeten), jede mit Vertragsprüfer', () => {
    const b = createWorkjetBridges({ state: {}, actorContext: () => ({ id: '' }), newId: () => 'x', now: () => Date.now() });
    for (const [bridge, list] of Object.entries(D_ACTIONS)) {
      for (const action of list) {
        if (D_UNSUPPORTED.has(action)) { assert.equal(b.supported(bridge).includes(action), false, action); continue; }
        assert.ok(b.supported(bridge).includes(action), `${action} freigeschaltet`);
        assert.ok(CHECKERS[action], `Prüfer für ${action}`);
      }
    }
    assert.deepEqual(b.supported('computer').sort(), [...inventoryList('computer')].sort());
    assert.deepEqual(b.supported('session').sort(), [...inventoryList('session')].sort());
    assert.deepEqual(b.supported('device').sort(), [...inventoryList('device')].sort());
  });

  test('[v4 Strang D] Einhängen: alle fünf Globals, Rechner meldet supportsOperationalDetails, uninstall räumt samt Ereignisstrom', async () => {
    const k = await mkKernel();
    const target = {};
    const h = installWorkjetBridges({ kernel: adaptKernel(k), target });
    assert.deepEqual([...h.installed].sort(), ['workjetBusinessOsDeviceControl', 'workjetComputerControl', 'workjetProjectControl', 'workjetSessionControl', 'workjetSessionEvents']);
    assert.equal(target.workjetComputerControl.supportsOperationalDetails, true);
    assert.equal(typeof target.workjetSessionEvents.register, 'function');
    assert.deepEqual(json(target.workjetSessionEvents.register({ computerIds: ['c-1'] })), { registered: 1 });
    h.uninstall();
    for (const name of h.installed) assert.equal(Object.hasOwn(target, name), false, name);
    await k.close();
  });

  test('[v4 Strang D] Sitzung nur vollständig: fehlt eine Sitzungsaktion, bleiben workjetSessionControl und workjetSessionEvents weg', async () => {
    const k = await mkKernel();
    const target = {};
    const { SESSION_ACTIONS } = await import('../actions/session.js');
    const partial = { ...SESSION_ACTIONS };
    delete partial['session.transfer.abort'];
    const h = installWorkjetBridges({ kernel: adaptKernel(k), target, actions: { project: {}, computer: {}, session: partial, device: {} } });
    assert.deepEqual(h.installed, []);
    h.uninstall();
    await k.close();
  });

  test('[v4 Strang D] instance.grok.* über Kern-Befehl ctox.workjet.grok.v1: korreliert, Vertrag erfüllt; fehlende Route → unsupported', withKernel(async (k, b) => {
    const seen = [];
    k.sim.onCommand('ctox.workjet.grok.v1', (payload, ctx) => {
      seen.push({ payload, ctx });
      return { ...payload, installed: true, accountLabel: 'Grok (Owner)',
        login: { loginId: '9b2f8a30-5a24-4ba0-92da-5fdf499d3760', phase: 'pending', verificationUri: 'https://accounts.x.ai/device', userCode: 'ABCD-EFGH', expiresAt: 1_700_000_600_000 },
        models: ['grok-4.7'],
        check: { modelId: 'grok-4.7', status: 'error', checkedAt: 1_700_000_000_000, latencyMs: 12, errorCode: 'timeout' } };
    });
    for (const [action, extra] of [['instance.grok.read', {}], ['instance.grok.check', { modelId: 'grok-4.7' }], ['instance.grok.poll', { loginId: '9b2f8a30-5a24-4ba0-92da-5fdf499d3760' }]]) {
      const res = json(await b.workjetProjectControl({ version: 1, action, operationId: OP, ...extra }));
      assert.equal(res.operationId, OP);
      assert.deepEqual(checkResponse(action, res), [], action);
    }
    assert.deepEqual(seen[0].payload, { version: 1, action: 'instance.grok.read', operationId: OP });
    await k.sim.failCommand('ctox.workjet.grok.v1', 'unknown_command: ctox.workjet.grok.v1');
    await assert.rejects(b.workjetProjectControl({ version: 1, action: 'instance.grok.read', operationId: OP }),
      (e) => isUnsupported(e) && WORKJET_UNSUPPORTED.test(e.message));
    // Andere Kernfehler bleiben V2-Text (Workjet: core_outdated), kein falsches „nicht unterstützt“.
    await k.sim.failCommand('ctox.workjet.grok.v1', 'forbidden: owner only');
    await assert.rejects(b.workjetProjectControl({ version: 1, action: 'instance.grok.read', operationId: OP }),
      (e) => !isUnsupported(e) && e.message.startsWith('Grok control unavailable.'));
  }));

  const registry = () => ({ ok: true, schema: 'ctox.provider-federation-registry.v1', revision: 1,
    accounts: [{ id: 'acc-1', holder: { kind: 'ctox_instance', id: 'holder-1' }, revision: 2, provider: 'claude', enabled: true,
      credentialReady: true, observedAtMs: 1000, nativeAccountReference: { accountId: 'acc-1', holderInstanceId: 'holder-1', accountRevision: 2 },
      modelCatalogObserved: true, inferenceVerified: false,
      modelCatalog: { observed: true, fresh: false, models: ['claude-opus-5-5'], lastSuccessAtMs: 900,
        lastAttempt: { checkedAtMs: 1000, httpStatus: 503, elapsedMs: 30, retryAfterSeconds: 5, failure: 'transport_unavailable', success: false } },
      excludedModels: [], effectiveModels: ['claude-opus-5-5'] }],
    providers: [{ provider: 'claude', selection: ['claude-opus-5-5'] }] });

  test('[v4 Strang D] instance.providers.* über kernel.command: ohne record_id, Quittung angenommen, Vertrag erfüllt; fehlende Route → unsupported', withKernel(async (k, b) => {
    const seen = [];
    for (const type of ['ctox.workjet.providers.list', 'ctox.workjet.providers.observe_native', 'ctox.workjet.providers.account.enable']) {
      k.sim.onCommand(type, (payload, ctx) => { seen.push({ type, payload, ctx }); return registry(); });
    }
    const read = json(await b.workjetProjectControl({ version: 1, action: 'instance.providers.read', operationId: OP }));
    assert.deepEqual(checkResponse('instance.providers.read', read), []);
    // WJB-R (E26 idempotency_key): V2-Befehls-ID = operationId; je Operation eine eigene ID (OP_2..OP_4)
    const OP_2 = '5fa0a379-6d66-4b73-8bea-cc6591b31543', OP_3 = '5fa0a379-6d66-4b73-8bea-cc6591b31544', OP_4 = '5fa0a379-6d66-4b73-8bea-cc6591b31545';
    assert.equal(read.registry.accounts[0].inferenceVerified, false);
    const observe = json(await b.workjetProjectControl({ version: 1, action: 'instance.providers.observe', operationId: OP_2, accountId: 'acc-1', expectedAccountRevision: 2 }));
    assert.deepEqual(checkResponse('instance.providers.observe', observe), []);
    const enable = json(await b.workjetProjectControl({ version: 1, action: 'instance.providers.account.enable', operationId: OP_3, accountId: 'acc-1', expectedAccountRevision: 2, expectedRevision: 1, enabled: false }));
    assert.deepEqual(checkResponse('instance.providers.account.enable', enable), []);
    assert.deepEqual(seen.map((s) => s.payload), [{}, { account_id: 'acc-1', expected_account_revision: 2 },
      { account_id: 'acc-1', expected_account_revision: 2, expected_revision: 1, enabled: false }]);
    const doc = docOf(await k.sim.inspect('business_commands', seen[0].ctx.command_id));
    assert.equal(doc.type, 'ctox.workjet.providers.list');
    assert.equal(Object.hasOwn(doc, 'record_id'), false, 'record_id:null der V2-Quittung wird nicht als „null“ gesendet');
    await k.sim.failCommand('ctox.workjet.providers.list', 'unknown_command: ctox.workjet.providers.list');
    await assert.rejects(b.workjetProjectControl({ version: 1, action: 'instance.providers.read', operationId: OP_4 }),
      (e) => isUnsupported(e) && WORKJET_UNSUPPORTED.test(e.message));
    for (const action of D_UNSUPPORTED) {
      await assert.rejects(b.workjetProjectControl({ version: 1, action, operationId: OP }), (e) => isUnsupported(e) && WORKJET_UNSUPPORTED.test(e.message), action);
    }
  }));

  const computerRow = (id, extra = {}) => ({ id, display_name: `Rechner ${id}`, hosting_mode: 'workstation', status: 'assigned',
    capabilities: ['codex'], capability_config: [{ kind: 'gpu', model: 'A4500', vram_gib: 20 }], agentless: false,
    owner_user_id: 'owner-1', self_hosted_colocation: false, ...extra });

  test('[v4 Strang D] computer.* über kernel.command + kernel.query: Liste, Zuweisung, SSH-Schlüssel, Vertrag erfüllt', withKernel(async (k, b) => {
    const seen = [];
    k.sim.onCommand('ctox.workjet.computer.list', (payload, ctx) => { seen.push({ payload, ctx }); return { ok: true }; });
    k.sim.onCommand('ctox.workjet.computer.assign', (payload) => ({ ok: true, computer: computerRow(payload.computer_id, {
      display_name: payload.display_name, capabilities: payload.capabilities, capability_config: payload.capability_config, agentless: payload.agentless }) }));
    k.sim.onCommand('ctox.workjet.computer.ssh_key.ensure', (payload) => ({ ok: true, contract: 'ctox.workjet.computer-ssh-key.v1',
      computer_id: payload.computer_id, private_key: { scope: 'computer-access', name: `workjet-ssh-${'a'.repeat(64)}` },
      public_key: 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIHRlc3Q=', public_key_sha256: `SHA256:${'A'.repeat(43)}` }));
    await k.sim.push('workjet_computers', [computerRow('gpu3'), computerRow('foreign', { owner_user_id: 'other' }), computerRow('old', { status: 'unassigned' })]);
    const list = json(await b.workjetComputerControl({ action: 'computer.list', includeOperationalDetails: true }));
    assert.deepEqual(checkResponse('computer.list', list), []);
    assert.deepEqual(list.computers.map((c) => c.id), ['gpu3']);
    assert.deepEqual(list.computers[0].capabilityConfig, [{ kind: 'gpu', model: 'A4500', vram_gib: 20 }]);
    // E32: computer.list ist eine lokale Kernel-Abfrage, kein Befehl (vorher: seen[0].payload {limit:100}, record_id owner-1).
    assert.equal(seen.length, 0, 'E32: computer.list sendet keinen Kern-Befehl');
    const assign = json(await b.workjetComputerControl({ action: 'computer.assign', commandId: 'assign-1', computerId: 'nas-1', displayName: 'NAS',
      hostingMode: 'self_hosted', capabilities: [], agentless: true,
      capabilityConfig: [{ kind: 'storage', endpoint_ref: 'endpoint-nas', protocol: 'ssh', root: '/volume1', quota_gib: 10, purposes: ['artifacts'] }] }));
    assert.deepEqual(checkResponse('computer.assign', assign), []);
    const key = json(await b.workjetComputerControl({ action: 'computer.ssh_key.ensure', commandId: 'key-1', computerId: 'gpu3' }));
    assert.deepEqual(checkResponse('computer.ssh_key.ensure', key), []);
  }));

  test('[v4 Strang D] computer.*: fehlende Kernroute → „workjet_computers collection is not registered.“ (einziges Workjet-Merkmal)', withKernel(async (k, b) => {
    // E32: computer.list erreicht den Kern nicht mehr; die fehlende Kernroute prüft jetzt computer.unassign
    // (vorher computer.list). „Lesesicht nicht bereit“ für computer.list prüft der E32-Block.
    await k.sim.failCommand('ctox.workjet.computer.unassign', 'unknown_command: ctox.workjet.computer.unassign');
    await assert.rejects(b.workjetComputerControl({ action: 'computer.unassign', commandId: 'unassign-1', computerId: 'gpu3' }),
      (e) => isUnsupported(e) && e.message === 'workjet_computers collection is not registered.');
    // Fachlicher Kernfehler bleibt „fehlgeschlagen“ (V2-Text), nicht „nicht unterstützt“.
    await k.sim.failCommand('ctox.workjet.computer.ssh_key.ensure', 'forbidden: owner only');
    await assert.rejects(b.workjetComputerControl({ action: 'computer.ssh_key.ensure', commandId: 'key-1', computerId: 'gpu3' }),
      (e) => !isUnsupported(e) && e.message === 'Workjet computer command did not complete successfully.');
  }));

  const sessionRow = (extra = {}) => ({ id: 'workjet-session-1', project_id: 'project-1', working_copy_id: 'copy-1', computer_id: 'computer-1',
    thread_id: 'thread-1', coding_session_id: 'coding-1', run_status: 'running', fence_epoch: 0, owner_user_id: 'owner-1',
    created_at_ms: 1_699_999_000_000, updated_at_ms: 1_700_000_000_000, ...extra });

  test('[v4 Strang D] session.* über kernel.command: Enddokument unter der Kern-Befehls-ID, Projektion, Vertrag erfüllt', withKernel(async (k, b) => {
    const seen = [];
    k.sim.onCommand('ctox.workjet.session.create', (payload, ctx) => { seen.push({ payload, ctx }); return { ok: true, collection: 'workjet_sessions', session: sessionRow() }; });
    k.sim.onCommand('ctox.workjet.session.list', () => ({ ok: true }));
    k.sim.onCommand('ctox.workjet.session.transfer.start', () => ({ ok: false, transfer_id: 'workjet-transfer-1', error_code: 'session_not_running',
      retryable: false, message: 'Workjet session is not running' }));
    k.sim.onCommand('ctox.workjet.session.transfer.status', () => ({ outcome: { ok: true, transfer_id: 'workjet-transfer-1', state: 'packing', retryable: false } }));
    await k.sim.push('workjet_sessions', [sessionRow(), sessionRow({ id: 'foreign', owner_user_id: 'other' })]);
    const created = json(await b.workjetSessionControl({ action: 'session.create', commandId: 'create-1', projectId: 'project-1', workingCopyId: 'copy-1', threadId: 'thread-1' }));
    assert.deepEqual(checkResponse('session.create', created), []);
    assert.equal(created.session.id, 'workjet-session-1');
    assert.deepEqual(seen[0].payload, { project_id: 'project-1', working_copy_id: 'copy-1', thread_id: 'thread-1' });
    assert.equal(docOf(await k.sim.inspect('business_commands', seen[0].ctx.command_id)).record_id, 'project-1');
    const listed = json(await b.workjetSessionControl({ action: 'session.list' }));
    assert.deepEqual(checkResponse('session.list', listed), []);
    assert.deepEqual(listed.sessions.map((s) => s.id), ['workjet-session-1']);
    const start = json(await b.workjetSessionControl({ action: 'session.transfer.start', commandId: 'start-1', sessionId: 'workjet-session-1',
      targetComputerId: 'computer-2', targetPath: '/work/p1', idempotencyKey: 'key-1' }));
    assert.deepEqual(start, { action: 'session.transfer.start', outcome: { ok: false, retryable: false, transferId: 'workjet-transfer-1',
      errorCode: 'session_not_running', message: 'Workjet session is not running' } });
    assert.deepEqual(checkResponse('session.transfer.start', start), []);
    const status = json(await b.workjetSessionControl({ action: 'session.transfer.status', commandId: 'status-1', transferId: 'workjet-transfer-1' }));
    assert.deepEqual(status.outcome, { ok: true, retryable: false, transferId: 'workjet-transfer-1', state: 'packing' });
    assert.deepEqual(checkResponse('session.transfer.status', status), []);
    // Kernfehler: das Enddokument trägt kein Ergebnis → V2-Fehler „outcome is unavailable“ (kein Absturz, kein Fremdwert).
    await k.sim.failCommand('ctox.workjet.session.transfer.abort', 'forbidden: owner only');
    await assert.rejects(b.workjetSessionControl({ action: 'session.transfer.abort', commandId: 'abort-1', transferId: 'workjet-transfer-1', reason: 'x', idempotencyKey: 'k' }),
      /Terminal Workjet session command outcome is unavailable for abort-1/);
  }));

  test('[v4 Strang D] workjetSessionEvents über kernel.query: Registrierung, Live-Ereignis an workjetHostBridge, Momentaufnahme', async () => {
    const k = await mkKernel();
    const target = {};
    const posted = [];
    target.workjetHostBridge = { postSessionTransferEvent(event) { posted.push(json(event)); } };
    const transfer = (extra = {}) => ({ id: 'transfer-1', session_id: 'session-1', state: 'pause_requested', fence_epoch: 1,
      source_computer_id: 'computer-source', target_computer_id: 'computer-target', deadline_at_ms: 1_700_000_060_000, updated_at_ms: 1_700_000_000_000, ...extra });
    await k.sim.push('workjet_session_transfers', [transfer(), transfer({ id: 'other', source_computer_id: 'x', target_computer_id: 'y' })]);
    const h = installWorkjetBridges({ kernel: adaptKernel(k), target });
    const events = target.workjetSessionEvents;
    assert.deepEqual(json(events.register({ computerIds: ['computer-source'] })), { registered: 1 });
    const snapshot = json(await events.snapshot());
    assert.deepEqual(snapshot.map((e) => e.transferId), ['transfer-1']);
    assert.deepEqual(sessionTransferEventD(snapshot[0]), []);
    await k.sim.push('workjet_session_transfers', [transfer({ state: 'packing', updated_at_ms: 1_700_000_000_001 })]);
    const deadline = Date.now() + 2000;
    while (!posted.some((e) => e.state === 'packing') && Date.now() < deadline) await new Promise((r) => setTimeout(r, 10));
    assert.ok(posted.some((e) => e.state === 'packing'), `Live-Ereignis gepostet: ${JSON.stringify(posted)}`);
    for (const e of posted) assert.deepEqual(sessionTransferEventD(e), []);
    assert.ok(posted.every((e) => e.transferId === 'transfer-1'), 'nur registrierte Rechner');
    h.uninstall();
    const count = posted.length;
    await k.sim.push('workjet_session_transfers', [transfer({ state: 'packed', updated_at_ms: 1_700_000_000_002 })]);
    await new Promise((r) => setTimeout(r, 50));
    assert.equal(posted.length, count, 'nach uninstall keine Ereignisse mehr');
    await k.close();
  });

  test('[v4 Strang D] Gerät über Kern-Befehl ctox.workjet.device.v1: Antwort durchgereicht, Vertrag erfüllt; fehlende Route → CTOX_WEBRTC_CAPABILITY_MISSING', withKernel(async (k, b) => {
    const seen = [];
    k.sim.onCommand('ctox.workjet.device.v1', (payload) => {
      seen.push(payload);
      if (payload.action === 'binding.list') return { schema: 'ctox.workjet-device-bindings.v1', bindings: [{ id: 'b-1', deviceId: 'dev-1', displayName: 'iPhone', createdAtMs: 1, pairedAtMs: 2 }] };
      return { revoked: true };
    });
    const list = json(await b.workjetBusinessOsDeviceControl({ action: 'binding.list' }));
    assert.deepEqual(checkResponse('binding.list', list), []);
    const revoked = json(await b.workjetBusinessOsDeviceControl({ action: 'binding.revoke', bindingId: 'b-1' }));
    assert.deepEqual(checkResponse('binding.revoke', revoked), []);
    assert.deepEqual(seen, [{ action: 'binding.list' }, { action: 'binding.revoke', bindingId: 'b-1' }]);
    await k.sim.failCommand('ctox.workjet.device.v1', 'unknown_command: ctox.workjet.device.v1');
    await assert.rejects(b.workjetBusinessOsDeviceControl({ action: 'binding.list' }), (e) => e.code === 'CTOX_WEBRTC_CAPABILITY_MISSING');
  }));
}
// Ende Strang D (v4-Proben)

// Strang D – Hostbrücke (host-bridge.js): V2 hat für mobile-host.js nur Quelltext-Proben. Orakel ist deshalb der
// V2-Rumpf selbst (mobile-host.js am Pin), in derselben Attrappen-Umgebung ausgeführt: gleiche Host-Befehle →
// gleiche Nachrichten an workjetBusinessOsPostMessage und gleiche data-workjet-mobile-*-Attribute. Unterschied nur
// dort, wo v4 den Rahmen anders anspricht: V2 setzt location.hash, v4 ruft shell().navigate/showDesktop.
{
  const { execFileSync } = await import('node:child_process');
  const path = await import('node:path');
  const vm = (await import('node:vm')).default;
  const { fileURLToPath } = await import('node:url');
  const { V2_PIN } = await import('./v2-oracle.mjs');
  const { installWorkjetHostBridge } = await import('../host-bridge.js');
  const HERE = path.dirname(fileURLToPath(import.meta.url));
  const REPO = execFileSync('git', ['rev-parse', '--show-toplevel'], { cwd: HERE, encoding: 'utf8' }).trim();
  const mobileV2 = execFileSync('git', ['show', `${V2_PIN}:src/apps/business-os/mobile-host.js`], { cwd: REPO, encoding: 'utf8' });
  const CATALOG = { type: 'workjet.business-os-mobile-apps.v1', revision: 'rev-1', apps: [
    { id: 'app-store', title: 'App Store', mobilePresentation: 'feed' },
    { id: 'appsec-pentest', title: 'Penetration Testing', mobilePresentation: 'canvas', desktopOnly: true },
    { id: 'notes', title: 'Notizen' },
  ] };
  const settle = () => new Promise((r) => setTimeout(r, 5));
  // Attrappe: Dokument mit .shell-window-Elementen (Klassen wie shared/window-manager.js), Fokus über .is-focused.
  function fakeDom(ownerIdOf) {
    const windows = [];
    const docEvents = new EventTarget();
    const doc = {
      documentElement: { dataset: { workjetMobileHost: 'true' } },
      body: { dataset: {} },
      querySelectorAll(selector) {
        if (selector === '.shell-window') return windows;
        if (selector === '.shell-window[data-workjet-mobile-active]') return windows.filter((w) => w.dataset.workjetMobileActive !== undefined);
        if (selector === '.shell-window.is-focused') return windows.filter((w) => w.classList.has('is-focused'));
        throw new Error(`Attrappe kennt ${selector} nicht`);
      },
      querySelector(selector) { return this.querySelectorAll(selector)[0] || null; },
      addEventListener: (...a) => docEvents.addEventListener(...a),
      removeEventListener: (...a) => docEvents.removeEventListener(...a),
    };
    const addWindow = (appId, focused = false) => {
      for (const w of windows) w.classList.delete('is-focused');
      const el = { dataset: { ...ownerIdOf(appId) }, classList: new Set(focused ? ['is-focused'] : []), closest: () => null };
      windows.push(el);
      return el;
    };
    return { doc, windows, addWindow, docEvents };
  }
  class FakeObserver { constructor(cb) { this.cb = cb; FakeObserver.last = this; } observe() {} disconnect() { this.cb = null; } }
  const results = (env) => ({ posts: env.posts, body: { ...env.dom.doc.body.dataset }, html: { ...env.dom.doc.documentElement.dataset },
    active: env.dom.windows.map((w) => w.dataset.workjetMobileActive ?? null) });
  const controls = (posts) => ({
    workjetBusinessOsDeviceControl: async (r) => { if (r?.action === 'boom') throw Object.assign(new Error('Native WebRTC peer is not connected'), { code: 'PEER_UNAVAILABLE' }); return { revoked: true }; },
    workjetProjectControl: async (r) => ({ action: r.action, ok: true }),
    workjetSessionControl: async () => { throw new TypeError('Unsupported Workjet session payload field: x'); },
    workjetBusinessOsPostMessage: (m) => posts.push(JSON.parse(JSON.stringify(m))),
  });
  function startV2() {
    const posts = [];
    const dom = fakeDom((appId) => ({ ownerId: `module:${appId}` }));
    const events = new EventTarget();
    const actions = [];
    events.addEventListener('workjet-business-os-action', (e) => actions.push(e.detail));
    const ctx = vm.createContext({
      document: dom.doc, location: { hash: '', pathname: '/' }, history: { back() { ctx.__back = (ctx.__back || 0) + 1; } },
      fetch: async () => ({ ok: true, json: async () => JSON.parse(JSON.stringify(CATALOG)) }),
      MutationObserver: FakeObserver, TextEncoder, CustomEvent, queueMicrotask, setTimeout, console,
      addEventListener: (...a) => events.addEventListener(...a), dispatchEvent: (e) => events.dispatchEvent(e), ...controls(posts),
    });
    vm.runInContext(mobileV2, ctx);
    return { posts, dom, actions, send: (detail) => events.dispatchEvent(new CustomEvent('workjet-business-os-host-command', { detail })),
      bridge: () => ctx.workjetHostBridge, opened: () => ctx.location.hash, back: () => ctx.__back || 0 };
  }
  function startV4() {
    const posts = [];
    const dom = fakeDom((appId) => ({ ownerId: `v4app:${appId}#1`, windowApp: appId }));
    const events = new EventTarget();
    const actions = [];
    events.addEventListener('workjet-business-os-action', (e) => actions.push(e.detail));
    let opened = '';
    let back = 0;
    const target = { addEventListener: (...a) => events.addEventListener(...a), removeEventListener: (...a) => events.removeEventListener(...a),
      dispatchEvent: (e) => events.dispatchEvent(e), history: { back() { back += 1; } }, ...controls(posts) };
    const shell = { navigate(appId) { opened = appId; dom.addWindow(appId, true); return true; }, showDesktop() { opened = 'desktop'; } };
    const h = installWorkjetHostBridge({ target, doc: dom.doc, shell: () => shell, catalog: async () => JSON.parse(JSON.stringify(CATALOG)), MutationObserverImpl: FakeObserver });
    return { posts, dom, actions, h, target, send: (detail) => events.dispatchEvent(new CustomEvent('workjet-business-os-host-command', { detail })),
      bridge: () => target.workjetHostBridge, opened: () => opened, back: () => back };
  }
  const P = 'workjet.business-os-shell.v1';
  const event = { type: 'workjet.session.transfer', transferId: 'transfer-1', sessionId: 'session-1', state: 'packing', fenceEpoch: 1,
    sourceComputerId: 'computer-source', targetComputerId: 'computer-target', deadlineAtMs: 10, updatedAtMs: 5 };
  async function script(env, v2) {
    await settle();
    const steps = [];
    const step = async (name, fn) => { await fn(); await settle(); steps.push([name, results(env)]); };
    await step('ready', () => {});
    await step('catalog.request', () => env.send({ protocol: P, type: 'catalog.request' }));
    await step('host.configure', () => env.send({ protocol: P, type: 'host.configure', platform: 'ios', windowClass: 'compact' }));
    await step('foreign protocol', () => env.send({ protocol: 'other', type: 'catalog.request' }));
    await step('app.open desktopOnly', () => env.send({ protocol: P, type: 'app.open', appId: 'appsec-pentest' }));
    await step('app.open ohne Mobilansicht', () => env.send({ protocol: P, type: 'app.open', appId: 'notes' }));
    await step('app.open', () => {
      env.send({ protocol: P, type: 'app.open', appId: 'app-store' });
      // V2 öffnet über location.hash (V2-Router legt das Fenster an); die Attrappe spielt den Router nach.
      if (v2) env.dom.addWindow('app-store', true);
    });
    await step('action.invoke', () => env.send({ protocol: P, type: 'action.invoke', appId: 'app-store', actionId: 'refresh' }));
    await step('device.control', () => env.send({ protocol: P, type: 'device.control', requestId: 'r1', request: { action: 'binding.revoke' } }));
    await step('device.control Fehler', () => env.send({ protocol: P, type: 'device.control', requestId: 'r2', request: { action: 'boom' } }));
    await step('project.control', () => env.send({ protocol: P, type: 'project.control', requestId: 'r3', request: { action: 'project.list' } }));
    await step('session.control Fehler', () => env.send({ protocol: P, type: 'session.control', requestId: 'r4', request: { action: 'session.list' } }));
    await step('unsichere requestId', () => env.send({ protocol: P, type: 'project.control', requestId: 'Bad Id', request: {} }));
    await step('Sitzungsereignis', () => env.bridge().postSessionTransferEvent(event));
    await step('Sitzungsereignis ungültig', () => env.bridge().postSessionTransferEvent({ ...event, fenceEpoch: -1 }));
    await step('Fokus fremdes Fenster', () => { env.dom.addWindow('notes', true); FakeObserver.last?.cb?.(); });
    await step('app.suspend', () => env.send({ protocol: P, type: 'app.suspend', appId: 'notes' }));
    await step('app.close', () => env.send({ protocol: P, type: 'app.close', appId: 'notes' }));
    await step('navigation.back', () => env.send({ protocol: P, type: 'navigation.back' }));
    await step('app.open desktop', () => {
      env.send({ protocol: P, type: 'app.open', appId: 'desktop' });
      // V2 öffnet Home über location.hash; der V2-Router räumt dabei die Fensterebene (DOM-Mutation → Beobachter →
      // markActiveWindow). Die Attrappe spielt das nach, wie beim app.open-Schritt. v4 (WJB-M-1) ruft markActiveWindow
      // selbst, weil Minimieren dort nur style ändert.
      if (v2) FakeObserver.last?.cb?.();
    });
    return { steps, actions: env.actions, opened: env.opened(), back: env.back() };
  }

  test('[V2-Orakel · Differenz] host-bridge.js = mobile-host.js: gleiche Nachrichten und Attribute für alle Host-Befehle', async () => {
    const v2 = await script(startV2(), true);
    const v4env = startV4();
    const v4 = await script(v4env, false);
    assert.equal(v4env.h.installed, true);
    for (let i = 0; i < v2.steps.length; i += 1) assert.deepEqual(v4.steps[i], v2.steps[i], `Schritt ${v2.steps[i][0]}`);
    assert.deepEqual(JSON.parse(JSON.stringify(v4.actions)), JSON.parse(JSON.stringify(v2.actions))); // vm-Realm: Prototyp verschieden
    assert.equal(v4.back, v2.back);
    assert.equal(v2.opened, 'desktop'); // V2: location.hash
    assert.equal(v4.opened, 'desktop'); // v4: shell().showDesktop()
    v4env.h.uninstall();
    assert.equal(Object.hasOwn(v4env.target, 'workjetHostBridge'), false, 'uninstall stellt den vorherigen Zustand her');
  });

  test('[v4 Strang D] Hostbrücke: ohne Mobil-Host kein Einhängen; ohne Katalog V2-Fehlerpfad; ohne Rahmen-Griff keine falsche Öffnung', async () => {
    const off = installWorkjetHostBridge({ target: {}, doc: { documentElement: { dataset: {} } } });
    assert.equal(off.installed, false);
    const posts = [];
    const events = new EventTarget();
    const dom = fakeDom((appId) => ({ ownerId: `v4app:${appId}#1`, windowApp: appId }));
    const target = { addEventListener: (...a) => events.addEventListener(...a), removeEventListener: (...a) => events.removeEventListener(...a),
      workjetBusinessOsPostMessage: (m) => posts.push(m), workjetHostBridge: { existing: true } };
    const h = installWorkjetHostBridge({ target, doc: dom.doc, MutationObserverImpl: FakeObserver });
    await settle();
    assert.deepEqual(posts, [{ protocol: P, type: 'shell.error', code: 'catalog-unavailable', retryable: true }]);
    assert.equal(target.workjetHostBridge.existing, true, 'vorhandene Hostbrücke bleibt erhalten');
    h.uninstall();
    assert.deepEqual(target.workjetHostBridge, { existing: true });
    const posts2 = [];
    const target2 = { addEventListener: (...a) => events.addEventListener(...a), removeEventListener: (...a) => events.removeEventListener(...a), workjetBusinessOsPostMessage: (m) => posts2.push(m) };
    const h2 = installWorkjetHostBridge({ target: target2, doc: fakeDom(() => ({})).doc, catalog: async () => CATALOG, MutationObserverImpl: FakeObserver });
    await settle();
    events.dispatchEvent(new CustomEvent('workjet-business-os-host-command', { detail: { protocol: P, type: 'app.open', appId: 'app-store' } }));
    await settle();
    assert.deepEqual(posts2.at(-1), { protocol: P, type: 'shell.error', code: 'mobile-presentation-unavailable', retryable: false });
    h2.uninstall();
  });

  test('[v4 WJB-M] Hostbrücke: Katalog folgt der App-Liste (neue Revision → catalog.replace, gleiche Revision → nichts)', async () => {
    const posts = [];
    const events = new EventTarget();
    const target = { addEventListener: (...a) => events.addEventListener(...a), removeEventListener: (...a) => events.removeEventListener(...a), workjetBusinessOsPostMessage: (m) => posts.push(JSON.parse(JSON.stringify(m))) };
    let current = CATALOG;
    const listeners = new Set();
    const h = installWorkjetHostBridge({ target, doc: fakeDom(() => ({})).doc, catalog: async () => current,
      catalogChanges: (cb) => { listeners.add(cb); return () => listeners.delete(cb); }, MutationObserverImpl: FakeObserver });
    await settle();
    const replaces = () => posts.filter((m) => m.type === 'catalog.replace');
    assert.equal(replaces().length, 1, 'Startkatalog');
    for (const cb of listeners) cb(); // gleiche Revision
    await settle();
    assert.equal(replaces().length, 1, 'gleiche Revision schickt nichts');
    current = { ...CATALOG, revision: 'rev-2', apps: [...CATALOG.apps, { ...CATALOG.apps[0], id: 'neu-app', title: 'Neu' }] };
    for (const cb of listeners) cb();
    await settle();
    assert.equal(replaces().length, 2);
    assert.equal(replaces().at(-1).catalog.revision, 'rev-2');
    h.uninstall();
    assert.equal(listeners.size, 0, 'uninstall meldet sich ab');
  });
}
// Ende Strang D (Hostbrücke)

// ---- Workjet #784 / Kern #875: „Google Kalender verbinden“ ----------------------------------------------------------
// V2-Verhaltensprobe shared/workjet-calendar-connect.test.mjs (Branch shell/plan-step-retry 3b8675aad) wörtlich gegen das
// v4-Modul actions/calendar-connect.js; dazu der Weg über die Brücke (Prüfer + Stub-Kernel).
{
  const { execFileSync } = await import('node:child_process');
  const { pathToFileURL } = await import('node:url');
  const fs = await import('node:fs');
  const os = await import('node:os');
  const path = await import('node:path');
  const HERE = path.dirname(new URL(import.meta.url).pathname);
  const { createWorkjetBridges } = await import('../index.js');
  const REPO_CC = process.env.WJB_PLAN_STEP_RETRY_REPO || '/Volumes/tmp/ctox-m20';
  const REF_CC = '3b8675aad';
  const src = execFileSync('git', ['show', `${REF_CC}:src/apps/business-os/shared/workjet-calendar-connect.test.mjs`], { cwd: REPO_CC, encoding: 'utf8' });
  const mod = pathToFileURL(path.resolve(HERE, '../actions/calendar-connect.js')).href;
  const text = src.replace(/from '\.\/workjet-calendar-connect\.mjs'/, `from ${JSON.stringify(mod)}`);
  assert.notEqual(text, src, 'Importpfad der V2-Probe nicht gefunden');
  const out = path.join(os.tmpdir(), `wjb-cc-oracle-${process.pid}.test.mjs`);
  fs.writeFileSync(out, text);
  await import(pathToFileURL(out).href);

  const { adaptKernel } = await import('../../kernel-adapter.js');
  const { createV2Compat } = await import('../v2-compat.js');
  const { checkResponse } = await import('./contract-checkers.mjs');
  const STUB_DIR_CC = process.env.CTOX_APP_API_STUB || path.join(os.homedir(), '.local/state/shell-v4/stub-pins/64acc2c');
  const { createStubKernel } = await import(pathToFileURL(path.join(STUB_DIR_CC, 'ctox-app-api-stub.mjs')).href);
  test('[v4] Kalender verbinden über die Brücke: Kern-Befehl ctox.workjet.calendar.connect.v1, Fehler als Code', async () => {
    const k = createStubKernel({ instanceId: `wjbcc-${process.pid}`, userId: 'owner-1', latencyMs: 2, storage: 'memory',
      actor: { id: 'owner-1', display_name: 'Owner', role: 'admin', permissions: [] } });
    const seen = [];
    k.sim.onCommand('ctox.workjet.calendar.connect.v1', (payload) => { seen.push(payload); return { authorizeUrl: 'https://accounts.google.com/o', attemptId: 'att-1' }; });
    const b = createWorkjetBridges(createV2Compat({ kernel: adaptKernel(k) }));
    const start = { action: 'project.calendar.account.connect.start', commandId: 'conn-1', provider: 'capability-probe' };
    const ok = await b.workjetProjectControl(start);
    assert.deepEqual(seen, [{ action: start.action, provider: 'capability-probe' }]);
    assert.deepEqual(ok, { action: start.action, commandId: 'conn-1', authorizeUrl: 'https://accounts.google.com/o', attemptId: 'att-1' });
    assert.deepEqual(checkResponse(start.action, ok), []);
    await k.sim.failCommand('ctox.workjet.calendar.connect.v1', 'permission_denied: not the owner');
    const denied = await b.workjetProjectControl(start);
    assert.deepEqual(denied, { action: start.action, commandId: 'conn-1', error: 'permission_denied' }, 'Kern-Code mit Detail → Code');
    await k.sim.failCommand('ctox.workjet.calendar.connect.v1', 'unknown_command: ctox.workjet.calendar.connect.v1');
    const old = await b.workjetProjectControl({ action: 'project.calendar.account.connect.complete', commandId: 'conn-2', attemptId: 'att-1', callbackUrl: 'https://x.test/cb?code=1' });
    assert.deepEqual(old, { action: 'project.calendar.account.connect.complete', commandId: 'conn-2', error: 'unsupported' });
    assert.deepEqual(checkResponse(old.action, old), []);
    await k.close?.();
  });
}

// Strang R – Präsentation und Narration (E26 / K-WJ-2) und idempotency_key (E26 / K-WJ-4).
// (1) V2-Orakel shared/jour-fixe-presentation.test.mjs und shared/jour-fixe-narration.test.mjs am Pin e35a116ae,
//     unverändert. Aufbau wie Strang C: <tmp>/src/apps/business-os/{app.js = Weiterleitungshülle, shared/… vom Pin}
//     + <tmp>/src/core/rxdb/tests/fixtures/…; *.generated.mjs aus dem v4-Arbeitsbaum. Ersetzt wird nur 'node:test'
//     und 'node:vm' durch die Orakel-Shims; die Narrationsdatei lädt vm dynamisch (await import('node:vm')), daher
//     auch diese Zeile. Die Modulproben laufen gegen genau die shared-Module, die v4 importiert (byte-gleich, geprüft).
//     Die Hüllenprobe „Shell narration read uses …“ ruft workjetProjectControl → v4 meeting.js narrationRead.
// (2) v4-Proben gegen den echten Kern-Stub: echte Datei per files.put, Bereich über v2-compat demandFileLoader →
//     api.files.get({id, range}), Antwortform = Workjet-Vertrag (Prüfer + Korrelation), Gegenproben mit falschem Bereich.
// (3) idempotency_key: dieselbe Workjet-Anfrage zweimal → ein Befehlsdokument, ein Handlerlauf; andere commandId → zwei.
{
  const { execFileSync } = await import('node:child_process');
  const fs = await import('node:fs');
  const os = await import('node:os');
  const path = await import('node:path');
  const { webcrypto } = await import('node:crypto');
  const { fileURLToPath, pathToFileURL } = await import('node:url');
  const { createWorkjetBridges } = await import('../index.js');
  const { V2_PIN } = await import('./v2-oracle.mjs');
  const HERE = path.dirname(fileURLToPath(import.meta.url));
  const BOS_DIR = path.resolve(HERE, '../../../..');
  const REPO = execFileSync('git', ['rev-parse', '--show-toplevel'], { cwd: HERE, encoding: 'utf8' }).trim();
  const show = (rel) => execFileSync('git', ['show', `${V2_PIN}:${rel}`], { cwd: REPO, encoding: 'utf8', maxBuffer: 64 << 20 });
  const TEST_SHIM = JSON.stringify(pathToFileURL(path.join(HERE, 'oracle-test-shim.mjs')).href);
  const VM_SHIM = JSON.stringify(pathToFileURL(path.join(HERE, 'oracle-vm-shim.mjs')).href);
  const APP_SHIM_R = `// v4-Orakel-Hülle (Strang R), kein V2-Code
globalThis.workjetProjectControl = workjetProjectControl;
const WORKJET_PROJECT_CONTROL_MAX_RESULTS = 100;
async function workjetProjectControl(request = {}, diagnostics = {}) {
  return __wjb.project(globalThis, request, diagnostics);
}
async function waitForSyncBridgeReady() {}
`;
  const factory = Object.freeze({
    project(g, request, diagnostics) {
      const env = {
        get state() { return g.state; },
        actorContext: (session) => g.actorContext(session),
        newId: () => (typeof g.newId === 'function' ? g.newId() : globalThis.crypto.randomUUID()),
        now: () => (typeof g.Date?.now === 'function' ? g.Date.now() : Date.now()),
      };
      return createWorkjetBridges(env).workjetProjectControl(request, diagnostics);
    },
  });
  function materializeR(name, testFile) {
    const root = path.join(os.tmpdir(), `wjb-r-oracle-${process.pid}-${name}`);
    const bos = path.join(root, 'src/apps/business-os');
    fs.mkdirSync(path.join(bos, 'shared'), { recursive: true });
    fs.writeFileSync(path.join(bos, 'app.js'), APP_SHIM_R);
    const seen = new Set();
    const copy = (relBos) => {
      if (seen.has(relBos)) return;
      seen.add(relBos);
      let text = /^shared\/[^/]+\.generated\.mjs$/.test(relBos)
        ? fs.readFileSync(path.join(BOS_DIR, relBos), 'utf8') : show(`src/apps/business-os/${relBos}`);
      if (relBos === `shared/${testFile}`) {
        const before = text;
        text = text
          .replace(/^import test from 'node:test';$/m, `import test from ${TEST_SHIM};`)
          .replace(/^import vm from 'node:vm';$/m, `import vm from ${VM_SHIM};`)
          .replace("const vm = await import('node:vm');", `const vm = (await import(${VM_SHIM})).default;`);
        assert.notEqual(text, before, `${testFile}: node:test-Import nicht gefunden`);
      }
      const out = path.join(bos, relBos);
      fs.mkdirSync(path.dirname(out), { recursive: true });
      fs.writeFileSync(out, text);
      for (const m of text.matchAll(/(?:from |import\()'(\.\.?\/[^'?]+)(?:\?[^']*)?'/g)) {
        copy(path.posix.normalize(path.posix.join(path.posix.dirname(relBos), m[1])));
      }
      for (const m of text.matchAll(/new URL\('\.\.\/\.\.\/\.\.\/(core\/rxdb\/tests\/fixtures\/[^']+)'/g)) {
        const fx = path.join(root, 'src', m[1]);
        fs.mkdirSync(path.dirname(fx), { recursive: true });
        fs.writeFileSync(fx, show(`src/${m[1]}`));
      }
    };
    copy(`shared/${testFile}`);
    return path.join(bos, 'shared', testFile);
  }
  // [Name, V2-Testdatei, erwartete Anzahl]: alle Proben der Datei.
  const ORACLE_FILES_R = [
    ['presentation', 'jour-fixe-presentation.test.mjs', 5],
    ['narration', 'jour-fixe-narration.test.mjs', 12],
  ];
  for (const [name, testFile, expected] of ORACLE_FILES_R) {
    const registered = [];
    const skipped = [];
    // Die vm-Shim liest die Fabrik erst zur Laufzeit: die zuletzt gesetzte gilt für ALLE Orakel. Daher die Fabrik der
    // vorigen Blöcke (Strang D: computer/session/…) behalten und nur project ergänzen (identisch zu den anderen).
    const previous = globalThis.__WJB_ORACLE?.factory;
    globalThis.__WJB_ORACLE = { factory: Object.freeze({ ...(previous || {}), project: previous?.project || factory.project }), allow: () => true, registered, skipped };
    await import(pathToFileURL(materializeR(name, testFile)).href);
    test(`[V2-Orakel] ${testFile} (Strang R): ${registered.length} gegen v4, ${skipped.length} nicht portiert`, () => {
      assert.equal(registered.length, expected, 'Anzahl V2-Proben (Pin geändert?)');
    });
  }
  test('[V2-Orakel] Strang R: shared/jour-fixe-narration.mjs, jour-fixe-presentation.mjs, file-integrity.js byte-gleich zum V2-Pin', () => {
    for (const rel of ['shared/jour-fixe-narration.mjs', 'shared/jour-fixe-presentation.mjs', 'shared/file-integrity.js']) {
      assert.equal(fs.readFileSync(path.join(BOS_DIR, rel), 'utf8'), show(`src/apps/business-os/${rel}`), rel);
    }
  });

  // ---- (2)+(3) gegen den echten Kern-Stub -----------------------------------------------------------------------
  const { adaptKernel } = await import('../../kernel-adapter.js');
  const { createV2Compat, isUnsupported } = await import('../v2-compat.js');
  const { checkResponse, correlateR } = await import('./contract-checkers.mjs');
  const { readStoredFileFromDemandChunks, sha256Hex } = await import('../../../../shared/file-integrity.js');
  const STUB_DIR = process.env.CTOX_APP_API_STUB || path.join(os.homedir(), '.local/state/shell-v4/stub-pins/64acc2c');
  const { createStubKernel } = await import(pathToFileURL(path.join(STUB_DIR, 'ctox-app-api-stub.mjs')).href);
  const workjetRegex = /(?:unsupported|unknown)\s+(?:workjet\s+)?(?:project[- ]control\s+)?action/i;
  const OWNER = 'owner-1';
  let seqR = 0;
  // Kernel + Brücke; files.get wird mitprotokolliert (Bereichsaufrufe), commandBus-Quittungen (kernel_command_id) auch.
  // E30: files.get liest eine fremde Datei nur über eine lesbare desktop_files-Zeile. Die Shell braucht dafür die
  // Deklaration aus ihrem echten Manifest (foreign_collections), desktop den read-Export. Wie in Produktion legt die
  // Desktop-App die Datei an; die Shell kennt nur die desktop_files-ID.
  const SHELL_FOREIGN_R = Object.fromEntries(Object.entries(JSON.parse(fs.readFileSync(path.join(BOS_DIR, 'v4/manifests/shell.manifest.json'), 'utf8'))
    .foreign_collections).map(([name, { owner_app, ops }]) => [name, { owner_app, ops }]));
  const E30_SCHEMA = { schema: { type: 'object', properties: { id: { type: 'string' } } } };
  async function publishE30Apps(k, { shellForeign = SHELL_FOREIGN_R } = {}) {
    await k.sim.publishAppVersion('desktop', '1', { collections: { desktop_files: E30_SCHEMA }, manifestExtra: { exports: { desktop_files: ['read'] } } });
    await k.sim.publishAppVersion('shell', 'dev', { collections: {}, manifestExtra: { foreign_collections: shellForeign } });
  }
  async function desktopFile(k, bytes, meta) {
    const stored = await k.appApi('desktop', '1').files.put(bytes, meta);
    await k.sim.settle?.();
    return { id: `desktop-${stored.id}`, ref: stored.ref };
  }

  async function withKernelR(fn) {
    const k = createStubKernel({ instanceId: `wjb-r-${process.pid}-${++seqR}`, userId: OWNER, latencyMs: 2, storage: 'memory',
      actor: { id: OWNER, display_name: 'Owner', role: 'admin', permissions: [] } });
    try {
      await k.ready;
      await publishE30Apps(k);
      const adapted = adaptKernel(k);
      const fileGets = [];
      const spy = Object.create(adapted, { appApi: { value(appId, ver) {
        const api = adapted.appApi(appId, ver);
        const files = { ...api.files, get: (sel) => { fileGets.push(JSON.parse(JSON.stringify(sel))); return api.files.get(sel); } };
        return { ...api, files };
      } } });
      const compat = createV2Compat({ kernel: spy });
      const receipts = [];
      const dispatch = compat.state.commandBus.dispatch; // v4-allow: debt L-42
      compat.state.commandBus = { async dispatch(...args) { const r = await dispatch(...args); receipts.push(r); return r; } }; // v4-allow: debt L-42
      return await fn({ k, compat, b: createWorkjetBridges(compat), fileGets, receipts });
    } finally { await k.close(); }
  }
  const commandDocs = async (k, type) => (await k.sim.inspect('business_commands'))
    ?.filter?.((d) => (d?.doc ?? d)?.type === type) ?? null;
  const docOf = (r) => r?.doc ?? r?.value ?? r?.data ?? r;
  const b64 = (bytes) => Buffer.from(bytes).toString('base64');

  // Präsentation: echte Datei im Kern, desktop_files-Metadaten wie V2 (jour-fixe-presentation.test.mjs fixture()).
  async function presentationFixture(compat, k, { sizeDelta = 0 } = {}) {
    const bytes = new TextEncoder().encode(JSON.stringify({ schemaVersion: 'learnordie.slide.v1', title: 'Regeltermin', slides: [{ id: 's1' }] }));
    const file = await desktopFile(k, bytes, { name: 'deck.json', mime: 'application/json' });
    const sha = await sha256Hex(bytes);
    const size = bytes.length + sizeDelta;
    const manifest = {
      presentation_id: 'workjet_presentation_1', project_id: 'p-1', meeting_id: 'm-1', owner_user_id: OWNER,
      title: 'Regeltermin', revision: 3, document_schema: 'learnordie.slide.v1', document_file_id: file.id,
      document_generation_id: 'deck-generation', document_sha256: sha, document_bytes: size,
      slide_ids: ['s1'], source: 'owner', updated_by: OWNER, updated_at_ms: 1,
    };
    await k.sim.push('desktop_files', [{ id: file.id, content_ref: file.ref, owner_id: OWNER, linked_collection: 'workjet_presentations',
      linked_record_id: 'workjet_presentation_1', kind: 'file', content_state: 'available', mime_type: 'application/json',
      content_hash_scheme: 'sha256-bytes-v1', content_hash: sha, content_generation_id: 'deck-generation', size_bytes: size }]);
    const reads = [];
    k.sim.onCommand('ctox.workjet.presentation.read', (p, ctx) => { reads.push({ p, ctx }); return { ok: true, contract: 'ctox.workjet.presentation.v1', presentation: manifest }; });
    return { bytes, file, manifest, reads };
  }

  test('[v4 Strang R] demandFileLoader.fetchFile → files.get({id, range:{start, end}}): V2-Chunkliste, Integritätsprüfung, falscher Bereich', () => withKernelR(async ({ k, compat, fileGets }) => {
    const data = Uint8Array.from({ length: 70_000 }, (_, i) => i % 251);
    const file = await compat.api.files.put(data, { name: 'gross.bin' });
    await k.sim.settle?.();
    const { state: peer } = await compat.state.sync.startCollection('desktop_files'); // v4-allow: debt L-42
    const chunks = await peer.demandFileLoader.fetchFile(file.id, { range: { offset: 65_530, length: 15 } });
    assert.deepEqual(fileGets.at(-1), { id: file.id, range: { start: 65_530, end: 65_545 } }, 'end exklusiv = offset + length');
    assert.equal(chunks.length, 1);
    assert.deepEqual(Object.keys(chunks[0]).sort(), ['bytesBase64', 'hash', 'sequence']);
    assert.equal(chunks[0].sequence, 0);
    assert.deepEqual(Buffer.from(chunks[0].bytesBase64, 'base64'), Buffer.from(data.slice(65_530, 65_545)));
    assert.equal(chunks[0].hash, await sha256Hex(data.slice(65_530, 65_545)));
    const blob = await readStoredFileFromDemandChunks(chunks, 'application/octet-stream'); // V2-Leser akzeptiert die Form
    assert.equal(blob.size, 15);
    // Gegenproben: Bereich über das Dateiende, negative Länge → Kern lehnt mit invalid/path range ab (kein unsupported)
    for (const range of [{ offset: data.length - 2, length: 5 }, { offset: 4, length: -1 }]) {
      await assert.rejects(peer.demandFileLoader.fetchFile(file.id, { range }), (e) => e.code === 'invalid' && e.path === 'range' && !isUnsupported(e), JSON.stringify(range));
    }
    // unbekannte Datei: E30 verrät die Existenz nicht und lehnt mit forbidden ab; der Fehler geht unverändert an den
    // Aufrufer (kein stilles „Dateiinhalt fehlt“, damit eine fehlende Deklaration sichtbar bleibt).
    await assert.rejects(peer.demandFileLoader.fetchFile('gibt-es-nicht', { range: { offset: 0, length: 1 } }), (e) => e.code === 'forbidden' && !isUnsupported(e));
    // Kern ohne E30 (found:false) → leere Liste → V2-Leser meldet „Dateiinhalt fehlt.“
    await assert.rejects(readStoredFileFromDemandChunks([]), { code: 'ctox_file_chunk_missing' });
  }));

  test('[v4 Strang R] project.presentation.read über kernel.command: Manifest bzw. null, Vertrag und Korrelation erfüllt', () => withKernelR(async ({ k, compat, b }) => {
    const { manifest, reads } = await presentationFixture(compat, k);
    const request = { action: 'project.presentation.read', commandId: 'c-pr-1', projectId: 'p-1', meetingId: 'm-1' };
    const res = JSON.parse(JSON.stringify(await b.workjetProjectControl(request)));
    assert.deepEqual(res, { ...request, contract: 'ctox.workjet.presentation.v1', presentation: manifest });
    assert.deepEqual(correlateR(request, res), []);
    assert.deepEqual(reads[0].p, { project_id: 'p-1', meeting_id: 'm-1' });
    assert.equal(docOf(await k.sim.inspect('business_commands', reads[0].ctx.command_id)).record_id, 'p-1');
    k.sim.onCommand('ctox.workjet.presentation.read', () => ({ ok: true, contract: 'ctox.workjet.presentation.v1', presentation: null }));
    const empty = await b.workjetProjectControl({ ...request, commandId: 'c-pr-2' });
    assert.equal(empty.presentation, null);
    assert.deepEqual(checkResponse(request.action, empty), []);
  }));

  test('[v4 Strang R] project.presentation.content.read: echte Datei, Bereich über files.get, Bytes/Hash/Vertrag; falscher Bereich → Kernfehler', () => withKernelR(async ({ k, compat, b, fileGets }) => {
    const { bytes, file } = await presentationFixture(compat, k);
    const request = { action: 'project.presentation.content.read', commandId: 'c-pc-1', projectId: 'p-1', meetingId: 'm-1',
      presentationId: 'workjet_presentation_1', revision: 3, offset: 5, length: 20 };
    const res = JSON.parse(JSON.stringify(await b.workjetProjectControl(request)));
    assert.deepEqual(correlateR(request, res), []);
    assert.deepEqual(Buffer.from(res.range.data_base64, 'base64'), Buffer.from(bytes.subarray(5, 25)));
    assert.equal(res.rangeSha256, await sha256Hex(bytes.subarray(5, 25)));
    assert.equal(res.range.total_bytes, bytes.length);
    assert.deepEqual(fileGets, [{ id: file.id, range: { start: 5, end: 25 } }], 'genau ein Bereichsabruf, end exklusiv');
    // Länge über das Dateiende wird wie V2 auf die Restlänge gekappt
    const tail = await b.workjetProjectControl({ ...request, commandId: 'c-pc-2', offset: bytes.length - 3, length: 4096 });
    assert.equal(tail.range.length, 3);
    assert.deepEqual(correlateR({ ...request, commandId: 'c-pc-2', offset: bytes.length - 3, length: 4096 }, tail), []);
    // Offset außerhalb der Datei → V2-Fehler vor jedem Bytezugriff
    const before = fileGets.length;
    await assert.rejects(b.workjetProjectControl({ ...request, commandId: 'c-pc-3', offset: bytes.length }), { code: 'PRESENTATION_INVALID_REQUEST' });
    assert.equal(fileGets.length, before);
  }));

  test('[v4 Strang R] content.read Gegenprobe: Metadaten größer als die Kerndatei → end > Größe → invalid/path range, nie „nicht unterstützt“', () => withKernelR(async ({ k, compat, b }) => {
    const { bytes } = await presentationFixture(compat, k, { sizeDelta: 8 });
    const request = { action: 'project.presentation.content.read', commandId: 'c-pc-x', projectId: 'p-1', meetingId: 'm-1',
      presentationId: 'workjet_presentation_1', revision: 3, offset: bytes.length - 2, length: 20 };
    await assert.rejects(b.workjetProjectControl(request), (e) => e.code === 'invalid' && e.path === 'range' && !isUnsupported(e));
  }));

  const canvasRequest = (extra = {}) => ({ action: 'project.presentation.canvas.save', commandId: 'c-cs-1', projectId: 'p-1', meetingId: 'm-1',
    operationId: 'op-1', presentationId: 'workjet_presentation_1', expectedRevision: 3, slideId: 's1', sceneJson: '{"elements":[]}', ...extra });
  function canvasRoute(k, manifest, calls) {
    k.sim.onCommand('ctox.workjet.presentation.canvas.save', (p, ctx) => {
      calls.push({ p, ctx });
      const presentation = { ...manifest, revision: p.expected_revision + 1 };
      return { ok: true, contract: 'ctox.workjet.presentation.v1', presentation,
        mutation: { operation_id: p.operation_id, presentation_id: p.presentation_id, project_id: 'p-1', meeting_id: 'm-1',
          revision: p.expected_revision + 1, document_sha256: manifest.document_sha256, document_bytes: manifest.document_bytes, slide_ids: manifest.slide_ids } };
    });
  }

  test('[v4 Strang R] project.presentation.canvas.save: V2-Befehl ctox.workjet.presentation.canvas.save, Payload, record_id, Vertrag', () => withKernelR(async ({ k, compat, b }) => {
    const { manifest } = await presentationFixture(compat, k);
    const calls = [];
    canvasRoute(k, manifest, calls);
    const request = canvasRequest();
    const res = JSON.parse(JSON.stringify(await b.workjetProjectControl(request)));
    assert.deepEqual(correlateR(request, res), []);
    assert.deepEqual(calls[0].p, { operation_id: 'op-1', presentation_id: 'workjet_presentation_1', expected_revision: 3, slide_id: 's1', scene_json: '{"elements":[]}' });
    const doc = docOf(await k.sim.inspect('business_commands', calls[0].ctx.command_id));
    assert.equal(doc.type, 'ctox.workjet.presentation.canvas.save');
    assert.equal(doc.record_id, 'p-1');
    // Quittung mit falscher Revision → V2-Fehler, kein Erfolg
    k.sim.onCommand('ctox.workjet.presentation.canvas.save', (p) => ({ ok: true, contract: 'ctox.workjet.presentation.v1', presentation: manifest,
      mutation: { operation_id: p.operation_id, presentation_id: p.presentation_id, project_id: 'p-1', meeting_id: 'm-1', revision: 9,
        document_sha256: manifest.document_sha256, document_bytes: manifest.document_bytes, slide_ids: manifest.slide_ids } }));
    await assert.rejects(b.workjetProjectControl(canvasRequest({ commandId: 'c-cs-2' })), { code: 'PRESENTATION_INTEGRITY_FAILED' });
    // ohne Kernroute → nicht unterstützt
    await k.sim.failCommand('ctox.workjet.presentation.canvas.save', 'unknown_command: ctox.workjet.presentation.canvas.save');
    await assert.rejects(b.workjetProjectControl(canvasRequest({ commandId: 'c-cs-3' })), (e) => isUnsupported(e) && workjetRegex.test(e.message));
  }));

  test('[v4 Strang R · E26] idempotency_key: dieselbe commandId zweimal → ein Befehlsdokument, ein Handlerlauf, gleiche kernel_command_id', () => withKernelR(async ({ k, compat, b, receipts }) => {
    const { manifest } = await presentationFixture(compat, k);
    const calls = [];
    canvasRoute(k, manifest, calls);
    const first = await b.workjetProjectControl(canvasRequest({ commandId: 'c-idem-1' }));
    const again = await b.workjetProjectControl(canvasRequest({ commandId: 'c-idem-1' }));
    assert.deepEqual(again, first, 'Wiederholung liefert dieselbe Antwort');
    assert.equal(calls.length, 1, 'Handler läuft nur einmal');
    assert.equal(receipts.length, 2);
    assert.equal(receipts[0].kernel_command_id, receipts[1].kernel_command_id, 'kernel_command_id gleich');
    const docs = await commandDocs(k, 'ctox.workjet.presentation.canvas.save');
    if (docs) assert.equal(docs.length, 1, 'genau ein Befehlsdokument');
    else assert.ok(docOf(await k.sim.inspect('business_commands', receipts[0].kernel_command_id)));
    // Gegenprobe: andere commandId → zweites Dokument, zweiter Handlerlauf
    await b.workjetProjectControl(canvasRequest({ commandId: 'c-idem-2' }));
    assert.equal(calls.length, 2);
    assert.notEqual(receipts[2].kernel_command_id, receipts[0].kernel_command_id);
    const docs2 = await commandDocs(k, 'ctox.workjet.presentation.canvas.save');
    if (docs2) assert.equal(docs2.length, 2, 'zwei Befehlsdokumente');
    // Schlüssel = V2-Befehls-ID im Dokument nachvollziehbar: UUIDv8 aus SHA-256(app|user|key)
    const { createHash } = await import('node:crypto');
    const h = createHash('sha256').update(`shell|${OWNER}|c-idem-1`).digest('hex');
    const expected = `${h.slice(0, 8)}-${h.slice(8, 12)}-8${h.slice(13, 16)}-${((parseInt(h[16], 16) & 3) | 8).toString(16)}${h.slice(17, 20)}-${h.slice(20, 32)}`;
    assert.equal(receipts[0].kernel_command_id, expected);
  }));

  test('[v4 · Workjet-Retry] Lesebefehle ohne Schlüssel: gleiche commandId liest neu (retryJourFixeRead), Schreibbefehle bleiben entdoppelt', () => withKernelR(async ({ k, compat }) => {
    const { READ_ONLY_COMMAND } = await import('../v2-compat.js');
    for (const t of ['ctox.workjet.jour_fixe.meeting.read', 'ctox.workjet.presentation.read', 'ctox.workjet.project.list', 'ctox.workjet.project.supervisor.turn.watch',
      'ctox.workjet.project.supervisor.route.read.v2', 'ctox.workjet.project.supervisor.route.capabilities.v1', 'ctox.workjet.session.transfer.status']) assert.ok(READ_ONLY_COMMAND.test(t), t);
    for (const t of ['ctox.workjet.project.upsert', 'ctox.workjet.presentation.canvas.save', 'ctox.workjet.exit_model.submit', 'ctox.workjet.project.supervisor.bind',
      'ctox.workjet.project.supervisor.turn.input', 'ctox.workjet.session.transfer.start', 'ctox.plan.step.retry']) assert.ok(!READ_ONLY_COMMAND.test(t), t);
    const seen = [];
    let n = 0;
    k.sim.onCommand('ctox.workjet.probe.read', (p, ctx) => { seen.push(ctx.command_id); return { ok: true, n: ++n }; });
    const r1 = await compat.state.commandBus.dispatch({ command_type: 'ctox.workjet.probe.read', command_id: 'same-read', payload: {} }, { until: 'terminal' }); // v4-allow: debt L-42
    const r2 = await compat.state.commandBus.dispatch({ command_type: 'ctox.workjet.probe.read', command_id: 'same-read', payload: {} }, { until: 'terminal' }); // v4-allow: debt L-42
    assert.equal(seen.length, 2, 'zweiter Lesevorgang läuft wirklich');
    assert.notEqual(r1.kernel_command_id, r2.kernel_command_id);
    assert.deepEqual([r1.result.n, r2.result.n], [1, 2], 'neuer Stand statt erstem Ergebnis');
  }));

  test('[v4 Strang R · E26] requestNative bleibt ohne Schlüssel: zwei gleiche Nativaufrufe → zwei Befehle', () => withKernelR(async ({ k, compat }) => {
    const seen = [];
    k.sim.onCommand('ctox.workjet.calendar.read.v1', (p, ctx) => { seen.push(ctx.command_id); return { ok: true }; });
    await compat.state.sync.requestNative('ctox.workjet.calendar.read.v1', { commandId: 'same' }); // v4-allow: debt L-42
    await compat.state.sync.requestNative('ctox.workjet.calendar.read.v1', { commandId: 'same' }); // v4-allow: debt L-42
    assert.equal(seen.length, 2);
    assert.notEqual(seen[0], seen[1]);
  }));

  // Narration: Besprechung aus dem Kern-Fixture (wie V2 „Shell narration read …“), Audio als echte Kerndatei.
  test('[v4 Strang R / E30] Gegenprobe: ohne desktop_files-Deklaration im Shell-Manifest lehnt der Kern den Präsentationsinhalt mit forbidden ab', async () => {
    assert.deepEqual(SHELL_FOREIGN_R.desktop_files, { owner_app: 'desktop', ops: ['read'] }, 'echtes Shell-Manifest deklariert das Lesen');
    const k = createStubKernel({ instanceId: `wjb-r-e30-${process.pid}`, userId: OWNER, latencyMs: 2, storage: 'memory',
      actor: { id: OWNER, display_name: 'Owner', role: 'admin', permissions: [] } });
    try {
      await k.ready;
      const { desktop_files: _omit, ...rest } = SHELL_FOREIGN_R;
      await publishE30Apps(k, { shellForeign: rest });
      const compat = createV2Compat({ kernel: adaptKernel(k) });
      await presentationFixture(compat, k);
      await assert.rejects(createWorkjetBridges(compat).workjetProjectControl({ action: 'project.presentation.content.read', commandId: 'c-e30', projectId: 'p-1',
        meetingId: 'm-1', presentationId: 'workjet_presentation_1', revision: 3, offset: 0, length: 8 }), (e) => e.code === 'forbidden' && !isUnsupported(e));
    } finally { await k.close(); }
  });

  async function narrationFixture(compat, k, { sizeDelta = 0 } = {}) {
    const bytes = new TextEncoder().encode('RIFF-bounded-audio-fixture-for-v4');
    const file = await desktopFile(k, bytes, { name: 'slide.wav', mime: 'audio/wav' });
    const corpus = JSON.parse(show('src/core/rxdb/tests/fixtures/workjet-jour-fixe-v1.json'));
    const meeting = structuredClone(corpus.valid_cases.find((item) => item.type === 'Meeting').value);
    const body = 'Slide text';
    const audio = { file_id: file.id, generation_id: 'immutable-generation', mime_type: 'audio/wav', format: 'wav',
      sha256: await sha256Hex(bytes), narration_text_sha256: await sha256Hex(body), provenance: 'native_gateway',
      duration_ms: 1200, source_run_id: 'retained-run', model: 'grok-4.7', synthesis_duration_ms: 50 };
    Object.assign(meeting, { id: 'm-1', project_id: 'p-1', owner_user_id: OWNER, deck_revision: 4, revision: 9, state: 'live',
      slides: [{ id: 'slide', meeting_id: 'm-1', title: 'Slide', position: 0, body_markdown: body, audio }] });
    for (const list of ['comments', 'transcript']) if (Array.isArray(meeting[list])) for (const x of meeting[list]) x.meeting_id = 'm-1';
    if (Array.isArray(meeting.comments)) meeting.comments = [];
    await k.sim.push('desktop_files', [{ id: file.id, content_ref: file.ref, owner_id: OWNER, linked_collection: 'workjet_jour_fixe_meetings', linked_record_id: 'm-1',
      kind: 'file', content_state: 'available', mime_type: 'audio/wav', content_hash_scheme: 'sha256-bytes-v1', content_hash: audio.sha256,
      content_generation_id: audio.generation_id, size_bytes: bytes.length + sizeDelta }]);
    const meetingReads = [];
    k.sim.onCommand('ctox.workjet.jour_fixe.meeting.read', (p, ctx) => { meetingReads.push({ p, ctx }); return { ok: true, meeting }; });
    return { bytes, file, audio, meeting, meetingReads };
  }
  const narrationRequest = (extra = {}) => ({ action: 'project.jour_fixe.narration.read', commandId: 'c-nr-1', projectId: 'p-1',
    meetingId: 'm-1', deckRevision: 4, slideId: 'slide', offset: 3, length: 8, ...extra });

  test('[v4 Strang R] project.jour_fixe.narration.read: Besprechung über kernel.command, Bereich über files.get, Vertrag und Korrelation', () => withKernelR(async ({ k, compat, b, fileGets }) => {
    const { bytes, file, audio, meetingReads } = await narrationFixture(compat, k);
    const request = narrationRequest();
    const res = JSON.parse(JSON.stringify(await b.workjetProjectControl(request)));
    assert.deepEqual(correlateR(request, res), []);
    assert.deepEqual(Buffer.from(res.bytesBase64, 'base64'), Buffer.from(bytes.subarray(3, 11)));
    assert.equal(res.rangeSha256, await sha256Hex(bytes.subarray(3, 11)));
    assert.equal(res.totalBytes, bytes.length);
    assert.equal(res.meetingRevision, 9);
    assert.deepEqual(res.audio, audio);
    assert.deepEqual(fileGets, [{ id: file.id, range: { start: 3, end: 11 } }]);
    assert.equal(meetingReads.length, 2, 'Besprechung vor und nach dem Bereich frisch gelesen (V2)');
    assert.notEqual(meetingReads[0].ctx.command_id, meetingReads[1].ctx.command_id);
    assert.deepEqual(meetingReads[0].p, { project_id: 'p-1', meeting_id: 'm-1' });
  }));

  test('[v4 Strang R] narration.read Gegenproben: end > Größe → invalid/path range; fehlende Kernroute → „nicht unterstützt“', () => withKernelR(async ({ k, compat, b }) => {
    const { bytes } = await narrationFixture(compat, k, { sizeDelta: 8 });
    await assert.rejects(b.workjetProjectControl(narrationRequest({ offset: bytes.length - 2, length: 20 })),
      (e) => e.code === 'invalid' && e.path === 'range' && !isUnsupported(e));
    await k.sim.failCommand('ctox.workjet.jour_fixe.meeting.read', 'unknown_command: ctox.workjet.jour_fixe.meeting.read');
    await assert.rejects(b.workjetProjectControl(narrationRequest({ commandId: 'c-nr-2' })), (e) => isUnsupported(e) && workjetRegex.test(e.message));
  }));
  test('[v4 Strang R] Kernel ohne files.get: Narration und Präsentationsinhalt melden „nicht unterstützt“ statt „fehlgeschlagen“', async () => {
    const k = createStubKernel({ instanceId: `wjb-r-nofiles-${process.pid}`, userId: OWNER, latencyMs: 2, storage: 'memory',
      actor: { id: OWNER, display_name: 'Owner', role: 'admin', permissions: [] } });
    try {
      await k.ready;
      await publishE30Apps(k);
      const adapted = adaptKernel(k);
      const full = createV2Compat({ kernel: adapted });
      const { bytes } = await narrationFixture(full, k);
      await presentationFixture(full, k);
      const noFiles = Object.create(adapted, { appApi: { value: (appId, ver) => ({ ...adapted.appApi(appId, ver), files: undefined }) } });
      const compat = createV2Compat({ kernel: noFiles });
      assert.ok(compat.gaps.has('files'));
      const b = createWorkjetBridges(compat);
      await assert.rejects(b.workjetProjectControl(narrationRequest({ length: bytes.length })), (e) => isUnsupported(e) && workjetRegex.test(e.message));
      await assert.rejects(b.workjetProjectControl({ action: 'project.presentation.content.read', commandId: 'c-nf', projectId: 'p-1', meetingId: 'm-1',
        presentationId: 'workjet_presentation_1', revision: 3, offset: 0, length: 16 }), (e) => isUnsupported(e) && workjetRegex.test(e.message));
    } finally { await k.close(); }
  });
  void webcrypto; void b64;
}
// Ende Strang R
