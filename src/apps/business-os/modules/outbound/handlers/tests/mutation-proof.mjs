// Mutationsbeleg für tests/invariants.test.mjs (Strang INV, Owner-Vorgabe E13).
// Für jede Invariante wird je eine Handler-Zeile in einer TEMP-KOPIE des handlers/-Ordners per
// String-Ersetzung gebrochen; der zugehörige Invariantentest muss rot werden (kein Syntaxfehler),
// die unveränderte Kopie muss vorher und nachher grün sein. Der Arbeitsbaum wird nie verändert.
// Aufruf: node tests/mutation-proof.mjs   (Exit 0 nur, wenn jede Mutation rot und Baseline grün ist)
import { spawnSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HANDLERS = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const BOS = resolve(HANDLERS, '../../..');
const SHARED_LOADER = join(BOS, 'v4/tools/node/ctox-shared.mjs');

export const MUTATIONS = [
  { inv: 'INV1', file: '_lib/send.js',
    from: 'if (!(await hasMatchingApproval(tx, messageId, revisionId, message))) {',
    to: 'if (false) {' },
  // INV1-B1 (Strang FIXINV): content binding of the approval
  { inv: 'INV1', file: '_lib/outbound.js',
    from: 'if (approvedHash != null) return approvedHash === messageContentHash(message);',
    to: 'if (approvedHash != null) return true;' },
  { inv: 'INV1', file: '_lib/outbound.js',
    from: "return str(approval, ['revision_id']) === messageRevision(message);",
    to: 'return true;' },
  { inv: 'INV1', file: '_lib/outbound.js',
    from: '    && approvalCoversContent(payload, message));',
    to: '    && true);' },
  { inv: 'INV1', file: 'outbound.message.approve.js',
    from: 'approval.content_hash = messageContentHash(message);',
    to: 'approval.content_hash = undefined;' },
  { inv: 'INV1', file: '_lib/outbound.js',
    from: 'attachments: message?.attachments ?? message?.payload?.attachments ?? null,',
    to: 'attachments: null,' },
  { inv: 'INV1', file: '_lib/send.js',
    from: "if (str(message, ['approval_status']) !== 'approved') fail(",
    to: 'if (false) fail(' },
  { inv: 'INV2', file: 'outbound.message.send_approved.js',
    from: 'if (QUEUED.has(existingSendStatus) && existingQueueId != null',
    to: 'if (false && QUEUED.has(existingSendStatus) && existingQueueId != null' },
  { inv: 'INV2', file: 'outbound.message.send_approved.js',
    from: "idempotency_key: `${messageId}:${str(message, ['revision_id']) ?? ''}`,",
    to: 'idempotency_key: `${command.id}`,' },
  { inv: 'INV3', file: '_lib/send.js',
    from: "if (reason != null) fail('conflict'",
    to: "if (false) fail('conflict'" },
  { inv: 'INV3', file: '_lib/outbound.js',
    from: 'if (domainReason == null && domains.some((suppressed) => suppressionDomainCovers(suppressed, domain))) domainReason = reason;',
    to: 'if (false) domainReason = reason;' },
  // INV3-B3 (Strang FIXINV): schema form scope/value/active
  { inv: 'INV3', file: '_lib/outbound.js',
    from: "if (value != null && scope === 'email') emails.push(asciiLower(value.trim()));",
    to: 'if (false) emails.push(asciiLower(value.trim()));' },
  { inv: 'INV3', file: '_lib/outbound.js',
    from: "if (value != null && scope === 'domain') domains.push(normalizeSuppressionDomain(value));",
    to: 'if (false) domains.push(normalizeSuppressionDomain(value));' },
  { inv: 'INV3', file: '_lib/outbound.js',
    from: 'return recipientDomain === suppressedDomain || recipientDomain.endsWith(`.${suppressedDomain}`);',
    to: 'return recipientDomain === suppressedDomain;' },
  { inv: 'INV3', file: '_lib/outbound.js',
    from: "if (typeof active === 'number' || typeof active === 'boolean') return Number(active) !== 0;",
    to: "if (typeof active === 'number' || typeof active === 'boolean') return true;" },
  { inv: 'INV3', file: '_lib/outbound.js',
    from: '    active: 1,\n',
    to: "    status: 'active',\n" },
  { inv: 'INV4', file: 'outbound.message.send_approved.js',
    from: 'await ctx.transaction((tx) => reserveAccountSendSlot(tx, senderAccountId, now));',
    to: 'await reserveAccountSendSlot(ctx, senderAccountId, now);' },
];

// Mail-Tracking (Strang TRACK, Kern §16.35 Nr. 3): Invarianten TRK1–TRK5 in tests/outbound.tracking.test.mjs.
export const TRACKING_MUTATIONS = [
  { inv: 'TRK1', file: '_lib/tracking.js',
    from: 'const explicitlyDisabled = message?.payload?.tracking_enabled === false;',
    to: 'const explicitlyDisabled = false;' },
  { inv: 'TRK2', file: '_lib/tracking.js',
    from: 'if (previous) return { ...hit, occurred_at_ms: previous.occurred_at_ms, duplicate: true };',
    to: 'if (false) return { ...hit, occurred_at_ms: previous.occurred_at_ms, duplicate: true };' },
  { inv: 'TRK2', file: 'outbound.tracking.event.js',
    from: "payload.event_id ? payload.event_id : command.id;",
    to: "payload.event_id ? payload.event_id : String(Math.random());" },
  { inv: 'TRK3', file: '_lib/tracking.js',
    from: 'if (tracking.event_type !== expected) return null;',
    to: 'if (false) return null;' },
  { inv: 'TRK4', file: '_lib/tracking.js',
    from: "return textResponse(404, 'stored mail tracking target is not HTTP(S)');",
    to: 'return { status: 302, headers: { location: target, ...NO_STORE } };' },
  { inv: 'TRK4', file: 'outbound.tracking.event.js',
    from: "else if (!hit.target_url) response = notFound('tracking target not found');",
    to: 'else if (false) response = null;' },
  { inv: 'TRK5', file: '_lib/tracking.js',
    from: 'if (!tracking) return null;',
    to: "if (!tracking) return { message_id: null, event_type: expected, target_url: 'https://x.test/' };" },
  // TRK6 (Strang OUT3, Kern §16.38 Nr. 3): Antwortgrenzen in ingestResponse.
  { inv: 'TRK6', file: '_lib/tracking.js',
    from: "if (RESPONSE_HEADERS.has(name) && typeof value === 'string') headers[name] = value;",
    to: "if (typeof value === 'string') headers[name] = value;" },
  { inv: 'TRK6', file: '_lib/tracking.js',
    from: 'if (!ALLOWED_STATUS(status) || badRef',
    to: 'if (badRef' },
  { inv: 'TRK6', file: '_lib/tracking.js',
    from: 'const badRef = bodyRef !== undefined && !(isPlainRef(bodyRef));',
    to: 'const badRef = false;' },
  { inv: 'TRK6', file: '_lib/tracking.js',
    from: '|| (redirect && !(typeof location === \'string\' && /^https?:\\/\\/[^/\\s]/i.test(location)))) {',
    to: ') {' },
  { inv: 'TRK6', file: '_lib/tracking.js',
    from: 'bodyBytes > RESPONSE_BODY_MAX',
    to: 'bodyBytes > Infinity' },
];

// Strang OUT3 (App-Logik aus dem Core): Invarianten OUT3A–OUT3F in tests/out3.invariants.test.mjs.
export const OUT3_MUTATIONS = [
  { inv: 'OUT3A', file: '_lib/research-task.js',
    from: 'if (!isObject(original) || original.type !== expectedType) fail(',
    to: 'if (!isObject(original)) fail(' },
  { inv: 'OUT3B', file: 'outbound.company.research.done.js',
    from: "if (status !== 'completed') {",
    to: 'if (false) {' },
  { inv: 'OUT3B', file: 'outbound.pipeline.contact_research.done.js',
    from: "if (status !== 'completed') {",
    to: 'if (false) {' },
  { inv: 'OUT3C', file: '_lib/owner-app.js',
    from: "if (status === 'rejected' || status === 'failed') {",
    to: 'if (false) {' },
  { inv: 'OUT3D', file: '_lib/owner-app.js',
    from: 'export const ownerCommandId = (command) => `${command.id}:${OWNER_APP}`;',
    to: 'export const ownerCommandId = (command) => `${command.id}:${Math.random()}`;' },
  { inv: 'OUT3E', file: '_lib/olg-registry.js',
    from: "probe_in_progress: typeof s.probe_lease_owner === 'string' && s.probe_lease_until_ms > now,",
    to: "probe_in_progress: typeof s.probe_lease_owner === 'string' && s.probe_lease_until_ms >= now," },
  { inv: 'OUT3F', file: '_lib/sellify-lookup.js',
    from: "return text != null && likeContains(text, needle) && !(deleted === 1 || deleted === true || deleted === 'true');",
    to: 'return text != null && likeContains(text, needle);' },
  { inv: 'OUT3G', file: 'outbound.research_source.generate_adapter.done.js',
    from: 'if (registration?.script_registered === true) {',
    to: 'if (true) {' },
  { inv: 'OUT3H', file: 'outbound.research_source.generate_adapter.done.js',
    from: 'if (record.payload?.generation_task_outcome?.task_id === taskId) return',
    to: 'if (false) return' },
  { inv: 'OUT3I', file: 'outbound.research_source.generate_adapter.done.js',
    from: "if (typeof current === 'string' && current !== taskId) return",
    to: 'if (false) return' },
];

// Strang SKL-O (Kern §16.30): Automatisierungsknöpfe in tests/outbound.automation.sim.test.mjs (Simulation braucht das
// ganze Paket: skills/, prompts/, Handler-Liste → Spiegel von business-os, modules/outbound echte Kopie).
export const SKLO_MUTATIONS = [
  { inv: 'SKLO1', file: '_lib/campaign-setup.js',
    from: "export const SETUP_SKILL = 'outbound/campaign-setup';",
    to: "export const SETUP_SKILL = 'business-os-outbound-campaign-setup';" },
  { inv: 'SKLO1', file: 'outbound.campaign.setup.request.js',
    from: '    on_failed: SETUP_FAILED,\n',
    to: '' },
  { inv: 'SKLO1', file: 'outbound.campaign.setup.request.js',
    from: "if (isObject(previous) && previous.source_command_id === sourceCommandId && typeof previous.task_id === 'string' && previous.task_id) {",
    to: 'if (false) {' },
  { inv: 'SKLO1', file: '../prompts/campaign-setup.md',
    from: '- selected_template_id: {{template_id}}\n',
    to: '' },
  { inv: 'SKLO1', file: '../skills/campaign-setup/SKILL.md', remove: true },
  { inv: 'SKLO2', file: '_lib/campaign-setup.js',
    from: 'for (const key of ALLOWED_PATCH_KEYS) if (key in raw) patch[key] = raw[key];',
    to: 'Object.assign(patch, raw);' },
  { inv: 'SKLO2', file: 'outbound.campaign.setup.done.js',
    from: "if (!setupMatchesTask(setup, taskId, sourceCommandId) || SETUP_TERMINAL.includes(setup.status)) {",
    to: 'if (!setupMatchesTask(setup, taskId, sourceCommandId)) {' },
  { inv: 'SKLO2', file: 'outbound.campaign.apply_setup.js',
    from: '      ...(taskId != null ? { task_id: taskId } : {}),\n',
    to: '' },
  { inv: 'SKLO3', file: '_lib/campaign-setup.js',
    from: "if (!isObject(patch.campaign_setup) || str(patch.campaign_setup, ['status']) == null) {",
    to: 'if (false) {' },
  { inv: 'SKLO4', file: 'outbound.campaign.setup.failed.js',
    from: "const outcome = status === 'cancelled' ? 'cancelled' : 'failed';",
    to: "const outcome = 'failed';" },
  { inv: 'SKLO4', file: 'outbound.campaign.setup.failed.js',
    from: "if (!setupMatchesTask(setup, taskId, sourceCommandId) || SETUP_TERMINAL.includes(setup.status)) {",
    to: 'if (!setup) {' },
  { inv: 'SKLO4', file: '_lib/campaign-setup.js',
    from: "if (typeof setup.task_id === 'string' && setup.task_id) return setup.task_id === taskId;",
    to: "if (typeof setup.task_id === 'string' && setup.task_id) return true;" },
  { inv: 'SKLO5', file: '_lib/outreach-draft.js',
    from: "export const DRAFT_SKILL = 'outbound/message-drafting';",
    to: "export const DRAFT_SKILL = 'business-os-outbound-message-drafting';" },
  { inv: 'SKLO5', file: 'outbound.pipeline.outreach_draft.js',
    from: 'const book = stored ?? messageDraftingSkillbook();',
    to: 'const book = messageDraftingSkillbook();' },
  { inv: 'SKLO5', file: 'outbound.pipeline.outreach_draft.js',
    from: "if (requestId && contact.outreach_request_id === requestId && typeof contact.outreach_task_id === 'string' && contact.outreach_task_id) {",
    to: 'if (false) {' },
  { inv: 'SKLO5', file: 'outbound.pipeline.outreach_draft.js',
    from: '    c.outreach_task_id = taskId;\n',
    to: '' },
  { inv: 'SKLO5', file: '../skills/message-drafting/SKILL.md', remove: true },
  { inv: 'SKLO6', file: '_lib/outreach-draft.js',
    from: "if (typeof raw[key] !== 'string' || raw[key].trim() === '') return { error: `outreach draft reply is missing ${key}` };",
    to: "if (typeof raw[key] !== 'string') continue;" },
  { inv: 'SKLO6', file: '_lib/outreach-draft.js',
    from: 'return isObject(contact) && contact.outreach_task_id === taskId && contact.outreach_generating === true;',
    to: 'return isObject(contact) && contact.outreach_task_id === taskId;' },
  { inv: 'SKLO7', file: 'outbound.pipeline.outreach_draft.failed.js',
    from: "contact.outreach_status = outcome === 'cancelled' ? 'draft_cancelled' : 'draft_failed';",
    to: "contact.outreach_status = 'draft_failed';" },
  { inv: 'SKLO7', file: 'outbound.pipeline.outreach_draft.failed.js',
    from: '    contact.outreach_generating = false;\n',
    to: '' },
  { inv: 'SKLO7', file: '_lib/outreach-draft.js',
    from: 'return isObject(contact) && contact.outreach_task_id === taskId && contact.outreach_generating === true;',
    to: 'return isObject(contact) && contact.outreach_generating === true;' },
  { inv: 'SKLO8', file: '_lib/research-task.js',
    from: '    skill: RESEARCH_SKILL,\n',
    to: '' },
  { inv: 'SKLO8', file: '../skills/prospect-research/SKILL.md', remove: true },
];

/** Spiegel von business-os: modules/outbound echte Kopie, alles andere Symlinks (Pfade relativ zum Repo bleiben). */
function mirrorModule(handlersDir) {
  const work = mkdtempSync(join(tmpdir(), 'sklo-mutation-'));
  const moduleDir = resolve(handlersDir, '..');
  const bosDir = resolve(moduleDir, '../..');
  const bos = join(work, 'business-os');
  mkdirSync(join(bos, 'modules'), { recursive: true });
  for (const e of readdirSync(bosDir)) if (e !== 'modules') symlinkSync(join(bosDir, e), join(bos, e));
  for (const e of readdirSync(join(bosDir, 'modules'))) {
    if (e === 'outbound') cpSync(moduleDir, join(bos, 'modules/outbound'), { recursive: true });
    else symlinkSync(join(bosDir, 'modules', e), join(bos, 'modules', e));
  }
  return { work, copy: join(bos, 'modules/outbound/handlers') };
}

export function runProof(handlersDir, mutations, testFile = 'tests/invariants.test.mjs', { mirror = false } = {}) {
  const { work, copy } = mirror ? mirrorModule(handlersDir) : (() => {
    const dir = mkdtempSync(join(tmpdir(), 'inv-mutation-'));
    return { work: dir, copy: join(dir, 'handlers') };
  })();
  const runTest = (pattern) => {
    const args = [...(mirror ? ['--import', SHARED_LOADER] : []), '--test', ...(pattern ? ['--test-name-pattern', `^${pattern} `] : []), testFile];
    const r = spawnSync(process.execPath, args, { cwd: copy, encoding: 'utf8' });
    const out = `${r.stdout}\n${r.stderr}`;
    const pass = Number(/ℹ pass (\d+)/.exec(out)?.[1] ?? 0);
    const fail = Number(/ℹ fail (\d+)/.exec(out)?.[1] ?? 0);
    const syntax = /SyntaxError|ReferenceError|ERR_MODULE_NOT_FOUND/.test(out);
    const why = /AssertionError[^:]*: ([^\n]*)/.exec(out)?.[1]?.trim() ?? '';
    return { status: r.status, pass, fail, syntax, why };
  };
  const rows = [];
  let ok = true;
  try {
    if (!mirror) cpSync(handlersDir, copy, { recursive: true });
    const baseline = runTest(null);
    rows.push({ inv: 'Baseline', file: '-', mutation: 'unverändert', result: baseline.status === 0 ? `grün (${baseline.pass} pass)` : `ROT (${baseline.fail} fail)` });
    if (baseline.status !== 0) ok = false;
    for (const m of mutations) {
      const path = join(copy, m.file);
      const original = readFileSync(path, 'utf8');
      if (m.remove) {
        rmSync(path);
        const broken = runTest(m.inv);
        writeFileSync(path, original);
        const restored = runTest(m.inv);
        const red = broken.status !== 0 && broken.fail > 0 && !broken.syntax;
        const green = restored.status === 0 && restored.pass > 0;
        if (!red || !green) ok = false;
        rows.push({ inv: m.inv, file: m.file, mutation: 'Datei entfernt',
          result: `${red ? `rot („${broken.why}“)` : 'NICHT ROT'} / ${green ? 'zurück grün' : 'ZURÜCK NICHT GRÜN'}` });
        continue;
      }
      const hits = original.split(m.from).length - 1;
      if (hits !== 1) {
        rows.push({ inv: m.inv, file: m.file, mutation: m.from, result: `MUSTER ${hits}x gefunden (erwartet 1) – Beleg ungültig` });
        ok = false;
        continue;
      }
      writeFileSync(path, original.replace(m.from, m.to));
      const broken = runTest(m.inv);
      writeFileSync(path, original);
      const restored = runTest(m.inv);
      const red = broken.status !== 0 && broken.fail > 0 && !broken.syntax;
      const green = restored.status === 0 && restored.pass > 0;
      if (!red || !green) ok = false;
      rows.push({
        inv: m.inv, file: m.file, mutation: `${m.from} → ${m.to}`,
        result: `${red ? `rot („${broken.why}“)` : broken.syntax ? 'SYNTAXFEHLER (zählt nicht)' : 'NICHT ROT'} / ${green ? 'zurück grün' : 'ZURÜCK NICHT GRÜN'}`,
      });
    }
    const final = runTest(null);
    rows.push({ inv: 'Wiederhergestellt', file: '-', mutation: 'alle Dateien zurück', result: final.status === 0 ? `grün (${final.pass} pass)` : 'ROT' });
    if (final.status !== 0) ok = false;
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
  console.log('| Invariante | Datei | Mutation | Ergebnis |\n|---|---|---|---|');
  for (const r of rows) console.log(`| ${r.inv} | ${r.file} | \`\` ${r.mutation.replaceAll('|', '\\|').replaceAll('\n', '\\n')} \`\` | ${r.result} |`);
  console.log(ok ? '\nMUTATIONSBELEG: vollständig' : '\nMUTATIONSBELEG: UNVOLLSTÄNDIG');
  return ok;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const invariants = runProof(HANDLERS, MUTATIONS);
  const tracking = runProof(HANDLERS, TRACKING_MUTATIONS, 'tests/outbound.tracking.test.mjs');
  const out3 = runProof(HANDLERS, OUT3_MUTATIONS, 'tests/out3.invariants.test.mjs');
  const sklo = runProof(HANDLERS, SKLO_MUTATIONS, 'tests/outbound.automation.sim.test.mjs', { mirror: true });
  const all = invariants && tracking && out3 && sklo;
  console.log(all ? '\nMUTATIONSBELEG GESAMT (INV + TRK + OUT3 + SKLO): vollständig' : '\nMUTATIONSBELEG GESAMT (INV + TRK + OUT3 + SKLO): UNVOLLSTÄNDIG');
  process.exitCode = all ? 0 : 1;
}
