// Strang JF Runde 2: project.jour_fixe.create (ctox-App-Handler, nur Kern-Zustellung).
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { makeCtx, makeCommand, rejectsWithCode } from './_ctx.mjs';
import create from '../project.jour_fixe.create.js';
import prepare from '../project.jour_fixe.prepare.js';

const HANDLER_DIR = join(dirname(fileURLToPath(import.meta.url)), '..');
const BOS = join(HANDLER_DIR, '../../..');
const SCHEMA = JSON.parse(readFileSync(join(BOS, 'v4/schemas/ctox.collections.schema.json'), 'utf8'));
const MEETING_SCHEMA = SCHEMA.collections.workjet_jour_fixe_meetings;
const NOW = 1_791_440_000_000;
const MEETINGS = 'workjet_jour_fixe_meetings';

// type/enum/required/additionalProperties/items/maxLength/minimum (Teilmenge von check-schemas.mjs)
function schemaErrors(schema, value, at = '$') {
  const errs = [];
  const types = schema.type === undefined ? [] : Array.isArray(schema.type) ? schema.type : [schema.type];
  const typeOk = (t) =>
    (t === 'null' && value === null) || (t === 'array' && Array.isArray(value)) ||
    (t === 'object' && value !== null && typeof value === 'object' && !Array.isArray(value)) ||
    (t === 'integer' && Number.isInteger(value)) || (t === 'number' && Number.isFinite(value)) ||
    ((t === 'string' || t === 'boolean') && typeof value === t);
  if (types.length && !types.some(typeOk)) return [`${at}: Typ`];
  if (Array.isArray(schema.enum) && !schema.enum.includes(value)) errs.push(`${at}: enum`);
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

const project = (id, extra = {}) => ({ id, name: id, status: 'active', owner_user_id: 'owner-1', created_at_ms: 1, updated_at_ms: 1, ...extra });
function world({ projects, meetings } = {}) {
  return makeCtx({
    now: NOW,
    collections: {
      workjet_projects: projects ?? [project('project-1'), project('project-archived', { status: 'archived' }), project('project-gone', { is_deleted: true })],
      [MEETINGS]: meetings ?? [],
    },
  });
}
const head = {
  project_id: 'project-1',
  owner_user_id: 'owner-1',
  supervisor: { workjet_thread_id: 'supervisor-1', ctox_thread_key: 'native-bound-thread-1' },
  scheduled_at_ms: 1_791_450_000_000,
  prepare_at_ms: 1_791_442_800_000,
  timezone: 'Europe/Berlin',
};
const cmd = (payload = head, extra = {}) =>
  makeCommand('project.jour_fixe.create', payload, { id: 'op-create-1', record_id: 'meeting-new', actor: { id: 'core', role: 'system' }, ...extra });

test('create puts one planned meeting under the sender-assigned record_id', async () => {
  const { ctx, state } = world();
  const result = await create(cmd({ ...head, meeting_id: 'meeting-new', previous_goal: { goal_id: 'goal-1', revision: 2 } }), ctx);
  assert.deepEqual(result, { operation_id: 'op-create-1', meeting_id: 'meeting-new', project_id: 'project-1', revision: 0, state: 'planned', replayed: false });
  assert.deepEqual(state.writes, [{ op: 'put', collection: MEETINGS, id: 'meeting-new' }]);
  assert.deepEqual(state.commands, [], 'kein Befehl, kein Agentenauftrag');
  assert.deepEqual(state.capCalls, [], 'keine Capability');
  const { _rev, ...doc } = state.collections.get(MEETINGS).get('meeting-new');
  assert.deepEqual(doc, {
    id: 'meeting-new', ...head, previous_goal: { goal_id: 'goal-1', revision: 2 },
    state: 'planned', revision: 0, deck_revision: 0, created_operation_id: 'op-create-1',
    created_at_ms: NOW, updated_at_ms: NOW, is_deleted: false,
  });
  assert.deepEqual(schemaErrors(MEETING_SCHEMA, doc), []);
});

test('create is idempotent: same operation and same content returns the stored meeting without writing', async () => {
  const { ctx, state } = world();
  await create(cmd(), ctx);
  // Termin wird inzwischen vorbereitet; die Wiederholung darf ihn nicht zurücksetzen
  await prepare(makeCommand('project.jour_fixe.prepare', { project_id: 'project-1', meeting_id: 'meeting-new' }, { id: 'op-prep', record_id: 'meeting-new' }), ctx);
  const before = JSON.stringify(state.collections.get(MEETINGS).get('meeting-new'));
  const writes = state.writes.length;
  const again = await create(cmd(), ctx);
  assert.equal(again.replayed, true);
  assert.equal(again.revision, 1);
  assert.equal(state.writes.length, writes);
  assert.equal(JSON.stringify(state.collections.get(MEETINGS).get('meeting-new')), before);
  assert.equal(state.collections.get(MEETINGS).size, 1);
});

test('same meeting_id with different content is conflict; same operation id for another meeting is conflict', async () => {
  const changes = [
    { scheduled_at_ms: head.scheduled_at_ms + 60_000 },
    { prepare_at_ms: head.prepare_at_ms - 60_000 },
    { timezone: 'UTC' },
    { supervisor: { ...head.supervisor, ctox_thread_key: 'other' } },
    { previous_goal: { goal_id: 'goal-x', revision: 1 } },
  ];
  for (const change of changes) {
    const { ctx, state } = world();
    await create(cmd(), ctx);
    const before = JSON.stringify(state.collections.get(MEETINGS).get('meeting-new'));
    await rejectsWithCode(create(cmd({ ...head, ...change }, { id: 'op-create-2' }), ctx), 'conflict');
    assert.equal(JSON.stringify(state.collections.get(MEETINGS).get('meeting-new')), before, JSON.stringify(change));
  }
  {
    const { ctx, state } = world();
    await create(cmd(), ctx);
    await rejectsWithCode(create(cmd(head, { record_id: 'meeting-other' }), ctx), 'conflict');
    assert.equal(state.collections.get(MEETINGS).has('meeting-other'), false);
  }
  {
    const { ctx } = world({ meetings: [{ id: 'meeting-new', ...head, state: 'cancelled', revision: 0, deck_revision: 0, updated_at_ms: 1, is_deleted: true }] });
    await rejectsWithCode(create(cmd(), ctx), 'conflict');
  }
});

test('project must exist, belong to the owner and be active', async () => {
  for (const [projectId, code] of [['project-missing', 'not_found'], ['project-gone', 'not_found'], ['project-archived', 'conflict']]) {
    const { ctx, state } = world();
    await rejectsWithCode(create(cmd({ ...head, project_id: projectId }), ctx), code);
    assert.deepEqual(state.writes, [], projectId);
  }
  const { ctx, state } = world();
  await rejectsWithCode(create(cmd({ ...head, owner_user_id: 'owner-2' }), ctx), 'invalid');
  assert.deepEqual(state.writes, []);
});

test('payload outside the meeting head or with broken bounds is invalid', async () => {
  const cases = [
    cmd({ ...head, state: 'ready' }),
    cmd({ ...head, revision: 5 }),
    cmd({ ...head, slides: [] }),
    cmd({ ...head, meeting_id: 'other' }),
    cmd({ ...head, id: 'other' }),
    cmd({ ...head, supervisor: { workjet_thread_id: 'supervisor-1' } }),
    cmd({ ...head, supervisor: { ...head.supervisor, extra: 1 } }),
    cmd({ ...head, scheduled_at_ms: -1 }),
    cmd({ ...head, prepare_at_ms: head.scheduled_at_ms + 1 }),
    cmd({ ...head, timezone: '' }),
    cmd({ ...head, project_id: 'x'.repeat(129) }),
    cmd({ ...head, previous_goal: { goal_id: 'g' } }),
    cmd(head, { record_id: '' }),
    makeCommand('project.jour_fixe.create', head, { id: 'op-create-1' }),
  ];
  for (const command of cases) {
    const { ctx, state } = world();
    await rejectsWithCode(create(command, ctx), 'invalid');
    assert.deepEqual(state.writes, [], JSON.stringify(command.payload));
  }
});
