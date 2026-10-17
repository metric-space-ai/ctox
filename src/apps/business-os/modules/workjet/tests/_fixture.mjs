// Origin: CTOX
// License: AGPL-3.0-only
// Gemeinsame Testdaten + In-Memory-Handler-ctx (get/read/put/patch/transaction mit Rücknahme, Mango-Gleichheit/$ne/$in)
// für die workjet-Handlertests. Gleiche ctx-Form wie modules/support/handlers/tests/_ctx.mjs.

export const OWNER = 'owner-1';
export const PROJECT = 'proj-1';
export const THREAD = 'thread-1';
export const ENV = 'env-mac-1';

export const SEED = {
  business_users: [
    { id: 'bu-owner', user_id: OWNER, role: 'admin', active: true, display_name: 'Owner' },
    { id: 'bu-other', user_id: 'other-1', role: 'admin', active: true, display_name: 'Other' },
  ],
  workjet_projects: [
    { id: PROJECT, name: 'Projekt', status: 'active', owner_user_id: OWNER, created_at_ms: 1, updated_at_ms: 1 },
    { id: 'proj-other', name: 'Fremd', status: 'active', owner_user_id: 'other-1', created_at_ms: 1, updated_at_ms: 1 },
  ],
  workjet_project_chats: [
    { id: 'chat-1', project_id: PROJECT, thread_id: THREAD, owner_user_id: OWNER, kind: 'group', created_at_ms: 1, updated_at_ms: 1 },
    { id: 'chat-o', project_id: 'proj-other', thread_id: 'thread-o', owner_user_id: 'other-1', kind: 'group', created_at_ms: 1, updated_at_ms: 1 },
  ],
};

const clone = (v) => (v === undefined ? undefined : JSON.parse(JSON.stringify(v)));

function matches(selector, doc) {
  return Object.entries(selector || {}).every(([key, cond]) => {
    const value = doc[key];
    if (cond && typeof cond === 'object' && !Array.isArray(cond)) {
      return Object.entries(cond).every(([op, arg]) => {
        if (op === '$ne') return JSON.stringify(value) !== JSON.stringify(arg);
        if (op === '$in') return arg.some((a) => JSON.stringify(a) === JSON.stringify(value));
        if (op === '$eq') return JSON.stringify(value) === JSON.stringify(arg);
        throw new Error(`unsupported ${op}`);
      });
    }
    return JSON.stringify(value) === JSON.stringify(cond);
  });
}

export function makeCtx({ now = 1_760_000_000_000, tick = 1, seed = SEED } = {}) {
  let data = new Map(Object.entries(clone(seed)).map(([c, docs]) => [c, new Map(docs.map((d) => [d.id, d]))]));
  let clock = now;
  const coll = (c) => { if (!data.has(c)) data.set(c, new Map()); return data.get(c); };
  const api = {
    async get(c, id) { return clone(coll(c).get(id) ?? null); },
    async read(c, q) { return [...coll(c).values()].filter((d) => matches(q?.selector ?? q, d)).map(clone); },
    async put(c, doc) { coll(c).set(doc.id, clone(doc)); return { id: doc.id, ok: true }; },
    async patch(c, id, partial) {
      const cur = coll(c).get(id);
      if (!cur) throw Object.assign(new Error(`${c}/${id}`), { code: 'not_found' });
      coll(c).set(id, { ...cur, ...clone(partial) });
      return { id, ok: true };
    },
  };
  const ctx = {
    ...api,
    async transaction(fn) {
      const backup = new Map([...data].map(([c, m]) => [c, new Map([...m].map(([k, v]) => [k, clone(v)]))]));
      try { return await fn(api); } catch (error) { data = backup; throw error; }
    },
    now() { const v = clock; clock += tick; return v; },
  };
  return { ctx, docs: (c) => [...coll(c).values()].map(clone), doc: (c, id) => clone(coll(c).get(id) ?? null) };
}

let seq = 0;
export const command = (payload, actor = { id: OWNER, role: 'admin' }) => ({ id: `cmd-${++seq}`, type: 'workjet.worker.dispatch', payload, actor });

export function startup(intent, extra = {}) {
  return {
    schemaVersion: 1, status: 'dispatched', environmentId: ENV, workerThreadId: intent.intentId, computerId: 'mac-1',
    branch: `workjet/worker/${intent.intentId}`, worktreePath: '/tmp/wt', parent: { environmentId: intent.sourceEnvironmentId, threadId: intent.sourceSupervisorThreadId },
    modelSelection: { instanceId: 'codex', model: 'gpt' }, enabledCapabilityIds: [], ...extra,
  };
}

export function receipt(intent) {
  return {
    schema: 'ctox.workjet.worker-outcome.v1', worker_thread_id: intent.intentId, environment_id: ENV, computer_id: 'mac-1',
    branch: `workjet/worker/${intent.intentId}`, execution_stopped: true,
    pull_request: { provider: 'github', number: 7, url: 'https://github.com/metric-space-ai/ctox/pull/7', head_oid: 'a'.repeat(40), state: 'merged' },
  };
}
