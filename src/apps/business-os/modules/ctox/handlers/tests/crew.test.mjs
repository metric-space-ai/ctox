import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { makeCtx, makeCommand, rejectsWithCode } from './_ctx.mjs';
import { safeProse, parseSoul } from '../_lib/crew.js';
import {
  CONTINUITY_TEMPLATES,
  applyContinuityDiff,
  normalizeContinuityDiff,
  anchorLines,
} from '../_lib/continuity.js';
import create from '../ctox.crew.member.create.js';
import update from '../ctox.crew.member.update.js';
import memory from '../ctox.crew.memory.update.js';
import assign from '../ctox.crew.assign.js';
import confirm from '../ctox.crew.learning.confirm.js';
import learningUpdate from '../ctox.crew.learning.update.js';
import learningDelete from '../ctox.crew.learning.delete.js';

const HANDLER_DIR = join(dirname(fileURLToPath(import.meta.url)), '..');
const BOS = join(HANDLER_DIR, '../../..');
const NOW = 1_760_000_000_000;

const soul = {
  gruendlichkeit_vs_tempo: 60,
  vorsicht_vs_mut: 40,
  knapp_vs_ausfuehrlich: 50,
  regeltreu_vs_kreativ: 70,
  nachfragen_vs_annehmen: 30,
  sketch: '  Ruhig   und genau ',
  voice: 'Kurz und klar.',
};

const member = (id, extra = {}) => ({
  id,
  name: id.slice(5),
  shape: 'round',
  color: '#1685ee',
  archived: false,
  state: 'home',
  soul,
  specialties: { modules: [], command_types: [], skills: [], tags: [] },
  updated_at_ms: 1,
  ...extra,
});

const learning = (id, memberId, text, extra = {}) => ({
  id,
  member_id: memberId,
  text,
  normalized_text: text.toLowerCase(),
  kind: 'insight',
  confirmed_by_owner: false,
  archived: false,
  created_at_ms: 1,
  updated_at_ms: 1,
  ...extra,
});

// Queue-Simulation des generischen Primitivs agent.queue.task.claim:
// setzt den Halter nur bei unverleaster Aufgabe mit route_status ∈ route_status_in.
function crewWorld(extra = {}) {
  const tasks = new Map([
    ['task-pending', { route_status: 'pending', lease_owner: null, holder: null }],
    ['task-blocked', { route_status: 'blocked', lease_owner: null, holder: null }],
    ['task-leased', { route_status: 'leased', lease_owner: 'worker', holder: 'crew:milo' }],
    ['task-done', { route_status: 'completed', lease_owner: null, holder: null }],
  ]);
  const { ctx, state } = makeCtx({
    now: NOW,
    collections: {
      ctox_crew_members: extra.members ?? [member('crew:ada'), member('crew:old', { archived: true })],
      ctox_crew_learnings: extra.learnings ?? [
        learning('l1', 'crew:ada', 'Erst lesen'),
        learning('l2', 'crew:ada', 'Erst messen'),
      ],
    },
    caps: {
      agent: {
        flow: { record: () => ({ ok: true }) },
        queue: {
          task: {
            claim: ({ task_id, holder, route_status_in }) => {
              const t = tasks.get(task_id);
              if (!t || t.lease_owner !== null || !route_status_in.includes(t.route_status)) return { changed: 0 };
              t.holder = holder;
              return { changed: 1 };
            },
          },
        },
      },
    },
  });
  return { ctx, state, tasks, docs: (c) => [...(state.collections.get(c)?.values() ?? [])] };
}

const nonAudit = (state) => state.capCalls.filter((c) => c.path !== 'agent.flow.record');

test('safe_prose rejects paths, credentials and mail addresses like Rust', () => {
  assert.equal(safeProse('Ruhig und genau', 60), true);
  assert.equal(safeProse('liest /Users/x/notes', 600), false);
  assert.equal(safeProse('see docs/a/b.md', 600), false);
  assert.equal(safeProse('api_key here', 600), false);
  assert.equal(safeProse('mail ada@example.org', 600), false);
  assert.equal(safeProse('apikey = 1', 600), false);
  assert.equal(safeProse('sk-abc', 600), false);
  assert.equal(safeProse('task-key=1', 600), false);
  assert.equal(safeProse('Größe über alles', 600), true);
  assert.equal(safeProse('   ', 600), false);
  assert.equal(safeProse('x'.repeat(61), 60), false);
  assert.equal(safeProse('bell\u0007', 60), false);
});

test('soul validation mirrors Soul::validate', () => {
  assert.equal(parseSoul(soul).sketch, 'Ruhig und genau');
  assert.throws(() => parseSoul({ ...soul, vorsicht_vs_mut: 101 }), (e) => e.code === 'invalid');
  assert.throws(() => parseSoul({ ...soul, voice: 'Eins. Zwei.' }), (e) => e.code === 'invalid');
  assert.throws(() => parseSoul({ ...soul, extra: 1 }), (e) => e.code === 'invalid');
  assert.throws(() => parseSoul({ ...soul, voice: '  ' }), (e) => e.code === 'invalid');
});

test('ctox.crew.member.create writes the member into ctox_crew_members', async () => {
  const { ctx, state, docs } = crewWorld();
  const out = await create(
    makeCommand('ctox.crew.member.create', { name: ' Milo ', shape: 'square', color: '#7c6df2', soul }),
    ctx,
  );
  assert.equal(out.ok, true);
  assert.match(out.result.member_id, /^crew:/);
  const doc = docs('ctox_crew_members').find((m) => m.id === out.result.member_id);
  assert.equal(doc.name, 'Milo');
  assert.equal(doc.archived, false);
  assert.equal(doc.state, 'home');
  assert.equal(doc.active_task_id, null);
  assert.equal(doc.soul.sketch, 'Ruhig und genau');
  assert.deepEqual(doc.specialties, { modules: [], command_types: [], skills: [], tags: [] });
  assert.equal(doc.stats.tasks_total, 0);
  assert.equal(doc.created_at, new Date(NOW).toISOString());
  assert.equal(doc.updated_at_ms, NOW);
  assert.deepEqual(nonAudit(state), [], 'kein Core-Aufruf außer dem Audit');
  const stages = state.capCalls.filter((c) => c.path === 'agent.flow.record').map((c) => c.args[0].metadata.stage);
  assert.deepEqual(stages, ['requested', 'succeeded']);
  await rejectsWithCode(create(makeCommand('ctox.crew.member.create', { name: 'M', shape: 'x', color: '#7c6df2', soul }), ctx), 'invalid');
  await rejectsWithCode(create(makeCommand('ctox.crew.member.create', { name: 'M', shape: 'round', color: '#999999', soul }), ctx), 'invalid');
  await rejectsWithCode(create(makeCommand('ctox.crew.member.create', { name: 'M', shape: 'round', color: '#7c6df2' }), ctx), 'invalid');
  await rejectsWithCode(create(makeCommand('ctox.crew.member.create', { name: '/Users/x', shape: 'round', color: '#7c6df2', soul }), ctx), 'invalid');
});

// Mutationsbeleg: Entfernt man in ctox.crew.member.create die Zeile
// `if (count >= CREW_MEMBER_CAPACITY) throw fail('quota', …)`, wird dieser Test rot.
test('INVARIANT ctox.crew.member.create enforces capacity 1000 (archived count too) as quota', async () => {
  const members = Array.from({ length: 1000 }, (_, i) => member(`crew:${i}`, { archived: i % 2 === 0 }));
  const { ctx, state, docs } = crewWorld({ members });
  await rejectsWithCode(create(makeCommand('ctox.crew.member.create', { name: 'M', shape: 'round', color: '#7c6df2', soul }), ctx), 'quota');
  assert.equal(docs('ctox_crew_members').length, 1000);
  const last = state.capCalls.filter((c) => c.path === 'agent.flow.record').at(-1);
  assert.equal(last.args[0].metadata.stage, 'failed');
  const below = crewWorld({ members: members.slice(1) });
  await create(makeCommand('ctox.crew.member.create', { name: 'M', shape: 'round', color: '#7c6df2', soul }), below.ctx);
  assert.equal(below.docs('ctox_crew_members').length, 1000);
});

test('ctox.crew.member.update merges only given fields', async () => {
  const { ctx, docs } = crewWorld();
  await update(makeCommand('ctox.crew.member.update', { member_id: 'crew:ada', name: ' Ada L ', archived: true }), ctx);
  const ada = docs('ctox_crew_members').find((m) => m.id === 'crew:ada');
  assert.equal(ada.name, 'Ada L');
  assert.equal(ada.archived, true);
  assert.equal(ada.soul.voice, soul.voice);
  assert.equal(ada.state, 'home', 'Laufzeitfelder bleiben unberührt');
  assert.equal(ada.updated_at_ms, NOW);
  await rejectsWithCode(update(makeCommand('ctox.crew.member.update', { member_id: 'crew:none' }), ctx), 'not_found');
  await rejectsWithCode(update(makeCommand('ctox.crew.member.update', { member_id: 'crew:ada', archived: 'yes' }), ctx), 'invalid');
  await rejectsWithCode(update(makeCommand('ctox.crew.member.update', { member_id: 'crew:ada', name: null }), ctx), 'invalid');
  await rejectsWithCode(update(makeCommand('ctox.crew.member.update', { member_id: 'crew:ada', specialties: { tags: Array(21).fill('a') } }), ctx), 'invalid');
});

// Port von crew_identity_command_tests.rs:126-180 (crew_memory_update_replaces_a_long_entry_and_reports_the_success).
test('ctox.crew.memory.update replaces a long anchors entry (Rust reference case)', async () => {
  const { ctx, state, docs } = crewWorld();
  const long = `statement: ${'Ohne Werkzeugaufruf keine Blocker-Antwort. '.repeat(8)}`;
  const added = await memory(
    makeCommand('ctox.crew.memory.update', { member_id: 'crew:ada', kind: 'anchors', mode: 'diff', diff: `## Entries\n+ ${long}` }),
    ctx,
  );
  assert.equal(added.ok, true);
  assert.match(added.result.head_commit_id, /^contc_[0-9a-f]{16}$/);
  const replaced = await memory(
    makeCommand('ctox.crew.memory.update', { member_id: 'crew:ada', kind: 'anchors', mode: 'replace', find: long.trim(), replace: 'statement: korrigiert' }),
    ctx,
  );
  assert.equal(replaced.ok, true);
  const head = docs('ctox_crew_continuity').find((d) => d.id === 'contdoc_crew:crew:ada_anchors');
  assert.ok(head.content.includes('statement: korrigiert'), head.content);
  assert.ok(!head.content.includes(long.trim()), head.content);
  assert.equal(head.head_commit_id, replaced.result.head_commit_id);
  // Commit-Kette: Basis → diff → replace, je mit Elternverweis.
  const commits = docs('ctox_crew_continuity_commits').filter((c) => c.document_id === head.id);
  const byId = new Map(commits.map((c) => [c.id, c]));
  const replaceCommit = byId.get(replaced.result.head_commit_id);
  assert.equal(replaceCommit.parent_commit_id, added.result.head_commit_id);
  assert.equal(byId.get(added.result.head_commit_id).parent_commit_id, 'contbase_crew:crew:ada_anchors');
  assert.equal(byId.get('contbase_crew:crew:ada_anchors').rendered_text, CONTINUITY_TEMPLATES.anchors);
  assert.match(replaceCommit.diff_text, /^<tool:string_replace find_len=\d+ replace_len=21>$/);
  // continuity_init_documents legt auch die zweite Art an.
  assert.ok(docs('ctox_crew_continuity').some((d) => d.id === 'contdoc_crew:crew:ada_narrative'));
  // Gedächtnissicht am Mitglied (bisher Projektion) und Touch.
  const ada = docs('ctox_crew_members').find((m) => m.id === 'crew:ada');
  assert.equal(ada.memory.anchors, head.content);
  assert.equal(ada.memory.narrative, CONTINUITY_TEMPLATES.narrative);
  assert.equal(ada.updated_at_ms, NOW);
  assert.deepEqual(nonAudit(state), []);
});

// Mutationsbeleg: Ersetzt man in ctox.crew.memory.update die Grenze MEMORY_LIMIT (8000)
// durch eine größere Zahl, wird dieser Test rot (8001 Zeichen werden angenommen).
test('INVARIANT ctox.crew.memory.update checks kind/mode and the 8000-char bound', async () => {
  const { ctx, docs } = crewWorld();
  await rejectsWithCode(memory(makeCommand('ctox.crew.memory.update', { member_id: 'crew:none', kind: 'anchors', mode: 'full', body: 'b' }), ctx), 'not_found');
  await rejectsWithCode(memory(makeCommand('ctox.crew.memory.update', { member_id: 'crew:ada', kind: 'x', mode: 'full', body: 'b' }), ctx), 'invalid');
  await rejectsWithCode(memory(makeCommand('ctox.crew.memory.update', { member_id: 'crew:ada', kind: 'narrative', mode: 'full', body: 'b'.repeat(8001) }), ctx), 'invalid');
  await rejectsWithCode(memory(makeCommand('ctox.crew.memory.update', { member_id: 'crew:ada', kind: 'narrative', mode: 'zap' }), ctx), 'invalid');
  await rejectsWithCode(memory(makeCommand('ctox.crew.memory.update', { member_id: 'crew:ada', kind: 'narrative', mode: 'replace', find: '   ' }), ctx), 'invalid');
  assert.equal(docs('ctox_crew_continuity').length, 0, 'ungültige Befehle schreiben nichts');
  const ok = await memory(makeCommand('ctox.crew.memory.update', { member_id: 'crew:old', kind: 'narrative', mode: 'full', body: ` ${'b'.repeat(7998)} ` }), ctx);
  assert.equal(ok.ok, true, 'archivierte Mitglieder behalten ihr Gedächtnis');
  const head = docs('ctox_crew_continuity').find((d) => d.id === 'contdoc_crew:crew:old_narrative');
  assert.equal(head.content, 'b'.repeat(7998), 'full trimmt wie continuity_full_replace_document');
});

// Mutationsbeleg: Entfernt man in _lib/continuity.js den Zweig `if (matches > 1) throw …`,
// wird dieser Test rot (mehrdeutiges Ersetzen würde den ersten Treffer ändern).
test('INVARIANT continuity replace refuses missing, ambiguous and no-op replacements atomically', async () => {
  const { ctx, docs } = crewWorld();
  const call = (find, replace) =>
    memory(makeCommand('ctox.crew.memory.update', { member_id: 'crew:ada', kind: 'anchors', mode: 'replace', find, replace }), ctx);
  const missing = await rejectsWithCode(call('gibt es nicht', 'x'), 'conflict');
  assert.equal(missing.message, 'continuity_string_replace_document: find string not present in anchors document');
  // Template enthält „anchor_“ zweimal (anchor_id, anchor_type).
  const twice = await rejectsWithCode(call('anchor_', 'x'), 'conflict');
  assert.match(twice.message, /matches 2 times in anchors document; refusing ambiguous replace$/);
  await rejectsWithCode(call('anchor_id:', 'anchor_id:'), 'conflict');
  // Atomar: der abgelehnte Befehl hinterlässt weder Kopf noch Commit (Rust legt sie
  // vorab im LCM an; hier bewusst in derselben Transaktion).
  assert.equal(docs('ctox_crew_continuity').length, 0);
  assert.equal(docs('ctox_crew_continuity_commits').length, 0);
});

test('continuity diff matches lcm apply_continuity_diff on concrete cases', () => {
  // lcm/tests.rs:1288-1308 (continuity_apply_diff_accepts_headerless_anchor_entries)
  const diff = normalizeContinuityDiff(
    'anchors',
    '+ anchor_id: ANCHOR_MAIN_GATEWAY\n+ anchor_type: invariant\n+ statement: Keep the gateway mission primary.\n+ source_class: assistant_reply\n',
  );
  assert.equal(
    diff,
    '## Entries\n+ anchor_id: ANCHOR_MAIN_GATEWAY\n+ anchor_type: invariant\n+ statement: Keep the gateway mission primary.\n+ source_class: assistant_reply',
  );
  const anchors = applyContinuityDiff('anchors', CONTINUITY_TEMPLATES.anchors, diff);
  assert.equal(
    anchors,
    '# CONTINUITY ANCHORS\n\n## Entries\n- anchor_id:\n- anchor_type:\n- statement:\n- source_class:\n- source_ref:\n- observed_at:\n- confidence:\n- supersedes:\n- expires_at:\n' +
      '- anchor_id: ANCHOR_MAIN_GATEWAY\n- anchor_type: invariant\n- statement: Keep the gateway mission primary.\n- source_class: assistant_reply\n',
  );
  assert.deepEqual(anchorLines(anchors), ['[invariant] Keep the gateway mission primary.']);
  // Narrative: summary:/state: gehen nach „## Situation“, alles andere nach „## Entries“; „-“ entfernt.
  const narrativeDiff = normalizeContinuityDiff('narrative', '+ summary:  läuft   gut\n+ entry_id: e1\n- consequence:');
  assert.equal(narrativeDiff, '## Situation\n+ summary:  läuft   gut\n## Entries\n+ entry_id: e1\n- consequence:');
  const narrative = applyContinuityDiff('narrative', CONTINUITY_TEMPLATES.narrative, narrativeDiff);
  assert.equal(
    narrative,
    '# CONTINUITY NARRATIVE\n\n## Situation\n- summary:\n- state:\n- summary: läuft gut\n\n## Entries\n- entry_id:\n- event_type:\n- summary:\n- source_class:\n- source_ref:\n- observed_at:\n- entry_id: e1\n',
  );
  assert.throws(() => applyContinuityDiff('anchors', CONTINUITY_TEMPLATES.anchors, '## Nope\n+ x'), (e) => e.code === 'invalid' && e.message === 'unknown continuity section in diff: ## Nope');
  assert.throws(() => applyContinuityDiff('anchors', CONTINUITY_TEMPLATES.anchors, '## Entries\nx'), (e) => e.code === 'invalid' && e.message === 'unsupported continuity diff line: x');
  assert.throws(() => applyContinuityDiff('anchors', CONTINUITY_TEMPLATES.anchors, 'x'), (e) => e.code === 'invalid');
});

// Mutationsbeleg: Entfernt man in ctox.crew.assign die Prüfung `current.archived !== false`,
// wird dieser Test rot (archiviertes Mitglied würde über claim gebunden).
test('INVARIANT ctox.crew.assign binds only active members to unleased pending|blocked tasks via agent.queue.task.claim', async () => {
  const { ctx, state, tasks, docs } = crewWorld();
  const out = await assign(makeCommand('ctox.crew.assign', { task_id: 'task-pending', member_id: 'crew:ada' }), ctx);
  assert.deepEqual(out, { ok: true, result: { task_id: 'task-pending', member_id: 'crew:ada' } });
  assert.equal(tasks.get('task-pending').holder, 'crew:ada');
  assert.deepEqual(nonAudit(state).map((c) => c.args[0]), [
    { task_id: 'task-pending', holder: 'crew:ada', route_status_in: ['pending', 'blocked'] },
  ]);
  assert.deepEqual(docs('ctox_crew_assignments'), [
    { id: 'task-pending', task_id: 'task-pending', member_id: 'crew:ada', assigned_at: new Date(NOW).toISOString(), updated_at_ms: NOW, _rev: docs('ctox_crew_assignments')[0]._rev },
  ]);
  await assign(makeCommand('ctox.crew.assign', { task_id: 'task-blocked', member_id: 'crew:ada' }), ctx);
  const calls = nonAudit(state).length;
  const archived = await rejectsWithCode(assign(makeCommand('ctox.crew.assign', { task_id: 'task-pending', member_id: 'crew:old' }), ctx), 'not_found');
  assert.equal(archived.message, 'active member not found');
  await rejectsWithCode(assign(makeCommand('ctox.crew.assign', { task_id: 'task-pending', member_id: 'crew:none' }), ctx), 'not_found');
  assert.equal(nonAudit(state).length, calls, 'kein Claim für inaktive Mitglieder');
  for (const task of ['task-leased', 'task-done', 'task-unknown']) {
    const denied = await rejectsWithCode(assign(makeCommand('ctox.crew.assign', { task_id: task, member_id: 'crew:ada' }), ctx), 'conflict');
    assert.equal(denied.message, 'assignment requires an unleased pending or blocked task');
  }
  assert.equal(tasks.get('task-leased').holder, 'crew:milo', 'an active slice keeps its member');
  assert.equal(docs('ctox_crew_assignments').length, 2);
});

test('ctox.crew.learning.* confirm/update/delete on ctox_crew_learnings', async () => {
  const { ctx, docs } = crewWorld();
  const get = (id) => docs('ctox_crew_learnings').find((l) => l.id === id);
  await confirm(makeCommand('ctox.crew.learning.confirm', { learning_id: 'l1' }), ctx);
  assert.equal(get('l1').confirmed_by_owner, true);
  await learningUpdate(makeCommand('ctox.crew.learning.update', { learning_id: 'l1', text: '  Erst  LESEN gründlich ', archived: false }), ctx);
  assert.equal(get('l1').text, 'Erst LESEN gründlich');
  assert.equal(get('l1').normalized_text, 'erst lesen gründlich');
  assert.equal(get('l1').archived, false);
  assert.equal(get('l1').updated_at_ms, NOW);
  const before = get('l1')._rev;
  await learningUpdate(makeCommand('ctox.crew.learning.update', { learning_id: 'l1' }), ctx);
  assert.equal(get('l1')._rev, before, 'leeres Update ist ein No-op');
  await learningDelete(makeCommand('ctox.crew.learning.delete', { learning_id: 'l1' }), ctx);
  assert.equal(get('l1'), undefined);
  await rejectsWithCode(confirm(makeCommand('ctox.crew.learning.confirm', { learning_id: 'l1' }), ctx), 'not_found');
  await rejectsWithCode(confirm(makeCommand('ctox.crew.learning.confirm', { learning_id: 'nope' }), ctx), 'not_found');
  await rejectsWithCode(learningUpdate(makeCommand('ctox.crew.learning.update', { learning_id: 'l2', text: 'password=1' }), ctx), 'invalid');
  await rejectsWithCode(learningUpdate(makeCommand('ctox.crew.learning.update', { learning_id: 'l2', text: 7 }), ctx), 'invalid');
  await rejectsWithCode(learningUpdate(makeCommand('ctox.crew.learning.update', { learning_id: 'l2', archived: 'no' }), ctx), 'invalid');
});

// Mutationsbeleg: Entfernt man in _lib/learning.js die Zeile
// `if (twins.some((t) => t.id !== id)) throw fail('conflict', …)`, wird dieser Test rot.
test('INVARIANT learning text stays unique per member (UNIQUE(member_id, normalized_text))', async () => {
  const { ctx, docs } = crewWorld({
    learnings: [learning('l1', 'crew:ada', 'Erst lesen'), learning('l2', 'crew:ada', 'Erst messen'), learning('l3', 'crew:old', 'Erst lesen')],
  });
  const twin = await rejectsWithCode(learningUpdate(makeCommand('ctox.crew.learning.update', { learning_id: 'l2', text: ' ERST   lesen ', archived: true }), ctx), 'conflict');
  assert.equal(twin.message, 'learning text already exists for this member');
  assert.equal(docs('ctox_crew_learnings').find((l) => l.id === 'l2').archived, false, 'Rust-Transaktion: archived bleibt bei Fehler unverändert');
  await learningUpdate(makeCommand('ctox.crew.learning.update', { learning_id: 'l1', text: 'Erst LESEN' }), ctx);
  await learningUpdate(makeCommand('ctox.crew.learning.update', { learning_id: 'l3', text: 'Erst messen' }), ctx);
});

test('crew audit never copies arbitrary payload', async () => {
  const { ctx, state } = crewWorld();
  await confirm(makeCommand('ctox.crew.learning.confirm', { learning_id: 'l1', soul: { secret: 'x' }, foreign: 'y' }), ctx);
  assert.deepEqual(state.capCalls[0].args[0].metadata.payload, { learning_id: 'l1' });
});

test('no handler calls an agent.crew.* capability any more', () => {
  const files = [];
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      if (entry.name === 'tests') continue;
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.endsWith('.js')) files.push(path);
    }
  };
  walk(HANDLER_DIR);
  for (const file of files) assert.doesNotMatch(readFileSync(file, 'utf8'), /agent\.crew\./, file);
});

test('migration ctox_crew_learnings v0 → v1 derives normalized_text like crew::normalized', async () => {
  const schema = JSON.parse(readFileSync(join(BOS, 'v4/schemas/ctox.collections.schema.json'), 'utf8'));
  const entry = schema.migrations.ctox_crew_learnings;
  assert.equal(entry.from_ver, schema._base_release);
  const migrate = (await import(pathToFileURL(join(BOS, 'v4/schemas/migrations/ctox/ctox_crew_learnings.js')).href)).default;
  const legacy = { id: 'l9', member_id: 'crew:ada', text: ' Erst  MESSEN\tdann ', kind: 'pitfall', confirmed_by_owner: true, archived: false, created_at_ms: 1, updated_at_ms: 1 };
  const next = migrate(legacy);
  assert.equal(next.normalized_text, 'erst messen dann');
  const required = schema.collections.ctox_crew_learnings.required;
  for (const key of required) assert.ok(key in next, `${key} fehlt nach Migration`);
  assert.equal(migrate(next), next, 'idempotent');
});
