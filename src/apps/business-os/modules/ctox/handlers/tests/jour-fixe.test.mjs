// Strang JF: project.jour_fixe.prepare (ctox-App-Handler, Owner-Regel E17).
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { makeCtx, makeCommand, rejectsWithCode } from './_ctx.mjs';
import prepare from '../project.jour_fixe.prepare.js';
import { openTopicSource } from '../_lib/jour-fixe.js';

const HANDLER_DIR = join(dirname(fileURLToPath(import.meta.url)), '..');
const BOS = join(HANDLER_DIR, '../../..');
const SCHEMA = JSON.parse(readFileSync(join(BOS, 'v4/schemas/ctox.collections.schema.json'), 'utf8'));
const MEETING_SCHEMA = SCHEMA.collections.workjet_jour_fixe_meetings;
const NOW = 1_791_440_000_000;
const TYPE = 'project.jour_fixe.prepare';

// Prüft type/enum/required/additionalProperties/items/maxLength/minimum rekursiv (Teilmenge von check-schemas.mjs).
function schemaErrors(schema, value, at = '$') {
  const errs = [];
  const types = schema.type === undefined ? [] : Array.isArray(schema.type) ? schema.type : [schema.type];
  const typeOk = (t) =>
    (t === 'null' && value === null) ||
    (t === 'array' && Array.isArray(value)) ||
    (t === 'object' && value !== null && typeof value === 'object' && !Array.isArray(value)) ||
    (t === 'integer' && Number.isInteger(value)) ||
    (t === 'number' && Number.isFinite(value)) ||
    ((t === 'string' || t === 'boolean') && typeof value === t);
  if (types.length && !types.some(typeOk)) return [`${at}: Typ ${JSON.stringify(schema.type)}`];
  if (Array.isArray(schema.enum) && value !== null && !schema.enum.includes(value)) errs.push(`${at}: enum`);
  if (typeof value === 'string' && schema.maxLength !== undefined && [...value].length > schema.maxLength) errs.push(`${at}: maxLength`);
  if (typeof value === 'number' && schema.minimum !== undefined && value < schema.minimum) errs.push(`${at}: minimum`);
  if (value && typeof value === 'object' && !Array.isArray(value)) {
    for (const key of schema.required ?? []) if (!(key in value)) errs.push(`${at}.${key}: Pflichtfeld`);
    for (const [k, v] of Object.entries(value)) {
      if (schema.properties?.[k]) errs.push(...schemaErrors(schema.properties[k], v, `${at}.${k}`));
      else if (schema.additionalProperties === false && !k.startsWith('_')) errs.push(`${at}.${k}: nicht deklariert`);
    }
  }
  if (Array.isArray(value) && schema.items) value.forEach((v, i) => errs.push(...schemaErrors(schema.items, v, `${at}[${i}]`)));
  return errs;
}

const supervisor = { workjet_thread_id: 'supervisor-1', ctox_thread_key: 'native-bound-thread-1' };
const meeting = (id, extra = {}) => ({
  id,
  project_id: 'project-1',
  owner_user_id: 'owner-1',
  supervisor,
  scheduled_at_ms: 1_791_450_000_000,
  prepare_at_ms: 1_791_442_800_000,
  timezone: 'Europe/Berlin',
  state: 'planned',
  revision: 2,
  deck_revision: 0,
  created_at_ms: 1,
  updated_at_ms: 1,
  ...extra,
});
const todo = (id, priority, extra = {}) => ({ id, title: `Thema ${id}`, acceptance: `Abnahme ${id}`, priority, evidence_ids: [], ...extra });
const WEEK = 7 * 24 * 60 * 60 * 1000;
const confirmedMeeting = (id, scheduled, items, extra = {}) =>
  meeting(id, {
    scheduled_at_ms: scheduled,
    prepare_at_ms: scheduled - 2 * 60 * 60 * 1000,
    state: 'confirmed',
    revision: 5,
    todos: { revision: 1, status: 'confirmed', items, meeting_id: id, goal: { goal_id: `goal-${id}`, revision: 3 } },
    ...extra,
  });

function world({ meetings, projects } = {}) {
  return makeCtx({
    now: NOW,
    collections: {
      workjet_projects: projects ?? [
        { id: 'project-1', name: 'ctox.dev', status: 'active', owner_user_id: 'owner-1', created_at_ms: 1, updated_at_ms: 1 },
        { id: 'project-2', name: 'greppy', status: 'active', owner_user_id: 'owner-1', created_at_ms: 1, updated_at_ms: 1 },
      ],
      workjet_jour_fixe_meetings: meetings ?? [
        meeting('meeting-now'),
        // ältere Bestätigung: abgelöst durch meeting-last
        confirmedMeeting('meeting-old', 1_791_450_000_000 - 2 * WEEK, [todo('old-1', 'P0')]),
        confirmedMeeting('meeting-last', 1_791_450_000_000 - WEEK, [
          todo('t-p2', 'P2'),
          todo('t-p1-late', 'P1', { due_at_ms: 1_791_900_000_000 }),
          todo('t-p0', 'P0'),
          todo('t-p1-soon', 'P1', { due_at_ms: 1_791_500_000_000 }),
          todo('t-p1-nodue', 'P1'),
        ]),
        // anderes Projekt, späterer Termin, nur Vorschlag: zählen nicht
        confirmedMeeting('meeting-foreign', 1_791_450_000_000 - 3 * 24 * 60 * 60 * 1000, [todo('foreign-1', 'P0')], { project_id: 'project-2' }),
        meeting('meeting-proposed', {
          scheduled_at_ms: 1_791_450_000_000 - 3 * 24 * 60 * 60 * 1000,
          state: 'review',
          todos: { revision: 1, status: 'proposed', items: [todo('proposed-1', 'P0')], meeting_id: 'meeting-proposed' },
        }),
      ],
    },
  });
}

const cmd = (payload, extra = {}) =>
  makeCommand(TYPE, payload, { id: 'op-1', record_id: 'meeting-now', actor: { id: 'core', role: 'system' }, ...extra });
const okPayload = { project_id: 'project-1', meeting_id: 'meeting-now' };

test('prepare patches exactly the addressed meeting with a sorted agenda of open topics', async () => {
  const { ctx, state } = world();
  const before = new Map([...state.collections.get('workjet_jour_fixe_meetings')].map(([k, v]) => [k, JSON.stringify(v)]));
  const result = await prepare(cmd(okPayload), ctx);
  assert.deepEqual(result, {
    operation_id: 'op-1',
    meeting_id: 'meeting-now',
    project_id: 'project-1',
    revision: 3,
    state: 'planned',
    source_meeting_id: 'meeting-last',
    agenda_items: 5,
    replayed: false,
  });
  // genau ein Schreibvorgang, Patch auf record_id, keine Anlage
  assert.deepEqual(state.writes, [{ op: 'patch', collection: 'workjet_jour_fixe_meetings', id: 'meeting-now' }]);
  const meetings = state.collections.get('workjet_jour_fixe_meetings');
  assert.equal(meetings.size, before.size);
  for (const [id, raw] of before) if (id !== 'meeting-now') assert.equal(JSON.stringify(meetings.get(id)), raw, `${id} unverändert`);
  const doc = meetings.get('meeting-now');
  assert.equal(doc.id, 'meeting-now');
  assert.equal(doc.project_id, 'project-1');
  assert.equal(doc.state, 'planned', 'Vorbereiten ändert den Terminstatus nicht');
  assert.equal(doc.prepared_operation_id, 'op-1');
  assert.equal(doc.prepared_at_ms, NOW);
  assert.equal(doc.updated_at_ms, NOW);
  assert.deepEqual(doc.agenda.items.map((i) => i.topic_id), ['t-p0', 't-p1-soon', 't-p1-late', 't-p1-nodue', 't-p2']);
  assert.deepEqual(doc.agenda.items.map((i) => i.position), [0, 1, 2, 3, 4]);
  assert.ok(doc.agenda.items.every((i) => i.source_meeting_id === 'meeting-last'));
  assert.deepEqual(doc.agenda.items[1], {
    position: 1,
    topic_id: 't-p1-soon',
    source_meeting_id: 'meeting-last',
    title: 'Thema t-p1-soon',
    priority: 'P1',
    due_at_ms: 1_791_500_000_000,
  });
  assert.deepEqual(doc.agenda.source_goal, { goal_id: 'goal-meeting-last', revision: 3 });
  const { _rev, ...plain } = doc;
  assert.deepEqual(schemaErrors(MEETING_SCHEMA, plain), [], 'gepatchtes Dokument erfüllt das v4-Schema');
});

test('same operation id is idempotent: saved receipt, no second write', async () => {
  const { ctx, state } = world();
  await prepare(cmd(okPayload), ctx);
  const first = JSON.stringify(state.collections.get('workjet_jour_fixe_meetings').get('meeting-now'));
  const again = await prepare(cmd(okPayload), ctx);
  assert.equal(again.replayed, true);
  assert.equal(again.revision, 3);
  assert.equal(again.agenda_items, 5);
  assert.equal(state.writes.length, 1, 'Wiederholung schreibt nicht');
  assert.equal(JSON.stringify(state.collections.get('workjet_jour_fixe_meetings').get('meeting-now')), first);
  // neue Operation darf neu vorbereiten und erhöht die Revision
  const next = await prepare(cmd(okPayload, { id: 'op-2' }), ctx);
  assert.equal(next.replayed, false);
  assert.equal(next.revision, 4);
});

test('no open topics yields an empty agenda without error', async () => {
  const { ctx, state } = world({ meetings: [meeting('meeting-now')] });
  const result = await prepare(cmd(okPayload), ctx);
  assert.equal(result.agenda_items, 0);
  assert.equal(result.source_meeting_id, null);
  const doc = state.collections.get('workjet_jour_fixe_meetings').get('meeting-now');
  assert.deepEqual(doc.agenda.items, []);
  assert.equal(doc.agenda.source_meeting_id, null);
  assert.equal('source_goal' in doc.agenda, false);
  const { _rev, ...plain } = doc;
  assert.deepEqual(schemaErrors(MEETING_SCHEMA, plain), []);
});

test('wrong project or mismatched ids are invalid and write nothing', async () => {
  const cases = [
    cmd({ project_id: 'project-2', meeting_id: 'meeting-now' }),
    cmd({ project_id: 'project-1', meeting_id: 'meeting-old' }),
    cmd(okPayload, { record_id: 'meeting-last' }),
    cmd({ ...okPayload, owner_user_id: 'forged' }),
    cmd({ project_id: 'project-1' }),
    cmd(okPayload, { record_id: '' }),
    makeCommand(TYPE, okPayload, { id: 'op-1' }),
  ];
  for (const command of cases) {
    const { ctx, state } = world();
    await rejectsWithCode(prepare(command, ctx), 'invalid');
    assert.deepEqual(state.writes, [], JSON.stringify(command));
  }
  // Projekt eines anderen Owners
  const { ctx, state } = world({
    projects: [{ id: 'project-1', name: 'x', status: 'active', owner_user_id: 'owner-2', created_at_ms: 1, updated_at_ms: 1 }],
  });
  await rejectsWithCode(prepare(cmd(okPayload), ctx), 'invalid');
  assert.deepEqual(state.writes, []);
});

test('unknown meeting or project is not_found; live meeting or archived project is conflict', async () => {
  {
    const { ctx, state } = world();
    await rejectsWithCode(prepare(cmd({ project_id: 'project-1', meeting_id: 'nope' }, { record_id: 'nope' }), ctx), 'not_found');
    assert.deepEqual(state.writes, []);
  }
  {
    const { ctx } = world({ projects: [] });
    await rejectsWithCode(prepare(cmd(okPayload), ctx), 'not_found');
  }
  {
    const { ctx } = world({ meetings: [meeting('meeting-now', { is_deleted: true })] });
    await rejectsWithCode(prepare(cmd(okPayload), ctx), 'not_found');
  }
  {
    const { ctx, state } = world({ meetings: [meeting('meeting-now', { state: 'live' })] });
    await rejectsWithCode(prepare(cmd(okPayload), ctx), 'conflict');
    assert.deepEqual(state.writes, []);
  }
  {
    const { ctx } = world({
      projects: [{ id: 'project-1', name: 'x', status: 'archived', owner_user_id: 'owner-1', created_at_ms: 1, updated_at_ms: 1 }],
    });
    await rejectsWithCode(prepare(cmd(okPayload), ctx), 'conflict');
  }
});

test('only the latest earlier confirmed list of this project counts', async () => {
  const later = confirmedMeeting('meeting-future', 1_791_450_000_000 + WEEK, [todo('future-1', 'P0')]);
  const { ctx, state } = world({
    meetings: [
      meeting('meeting-now'),
      later,
      confirmedMeeting('meeting-old', 1_791_450_000_000 - 2 * WEEK, [todo('old-1', 'P0')]),
      confirmedMeeting('meeting-deleted', 1_791_450_000_000 - WEEK, [todo('del-1', 'P0')], { is_deleted: true }),
    ],
  });
  const result = await prepare(cmd(okPayload), ctx);
  assert.equal(result.source_meeting_id, 'meeting-old');
  assert.deepEqual(state.collections.get('workjet_jour_fixe_meetings').get('meeting-now').agenda.items.map((i) => i.topic_id), ['old-1']);
});

test('topic source never crosses project or owner, even if the read returns more', () => {
  const now = meeting('meeting-now');
  const foreignProject = confirmedMeeting('m-foreign', 1_791_450_000_000 - 1000, [todo('x', 'P0')], { project_id: 'project-2' });
  const foreignOwner = confirmedMeeting('m-owner', 1_791_450_000_000 - 2000, [todo('y', 'P0')], { owner_user_id: 'owner-2' });
  const own = confirmedMeeting('m-own', 1_791_450_000_000 - WEEK, [todo('z', 'P0')]);
  assert.equal(openTopicSource([foreignProject, foreignOwner, own, now], now).id, 'm-own');
  assert.equal(openTopicSource([foreignProject, foreignOwner, now], now), null);
});

// Runde 2: expected_revision (Wire PrepareRequest)
test('expected_revision matching the meeting revision prepares; a stale one is stale_base without write', async () => {
  {
    const { ctx, state } = world();
    const result = await prepare(cmd({ ...okPayload, expected_revision: 2 }), ctx);
    assert.equal(result.revision, 3);
    assert.equal(state.writes.length, 1);
  }
  for (const stale of [1, 3]) {
    const { ctx, state } = world();
    await rejectsWithCode(prepare(cmd({ ...okPayload, expected_revision: stale }), ctx), 'stale_base');
    assert.deepEqual(state.writes, []);
    assert.equal(state.collections.get('workjet_jour_fixe_meetings').get('meeting-now').revision, 2);
  }
  for (const bad of [-1, 1.5, '2', null]) {
    const { ctx, state } = world();
    await rejectsWithCode(prepare(cmd({ ...okPayload, expected_revision: bad }), ctx), 'invalid');
    assert.deepEqual(state.writes, []);
  }
});

test('replay of the same operation id wins over expected_revision even after the revision moved on', async () => {
  const { ctx, state } = world();
  await prepare(cmd({ ...okPayload, expected_revision: 2 }), ctx);
  // inzwischen ändert jemand anderes den Termin (z. B. Deck-Veröffentlichung)
  await ctx.patch('workjet_jour_fixe_meetings', 'meeting-now', { revision: 7 });
  const writes = state.writes.length;
  const again = await prepare(cmd({ ...okPayload, expected_revision: 2 }), ctx);
  assert.equal(again.replayed, true);
  assert.equal(again.agenda_items, 5);
  assert.equal(state.writes.length, writes, 'Wiederholung schreibt nicht');
  // eine NEUE Operation mit der alten Revision ist dagegen veraltet
  await rejectsWithCode(prepare(cmd({ ...okPayload, expected_revision: 2 }, { id: 'op-2' }), ctx), 'stale_base');
});
