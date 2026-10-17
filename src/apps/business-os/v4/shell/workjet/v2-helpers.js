// Shell v4 – Workjet-Brücke: gemeinsame V2-Hilfsfunktionen (WJB-0).
// Wörtlich aus der V2-Shell (origin/main src/apps/business-os/app.js, Stand e35a116ae) übernommen; nur `state`
// ist durch den Parameter `env.state` ersetzt (v2-compat.js liefert ihn). Gehört dem Gerüst, nicht einem Strang:
// Stränge A–D importieren hier, ändern aber nichts (neue gemeinsame Hilfen nur über den Kern-Strang WJB-0).
// ref: app.js:13823-13825 (Konstanten), :15025-15089 (Listen-Schritte), :15091-15114, :15156-15169,
//      :15212-15240, :15253-15457 (Ergebnis- und Eingabeprüfung).
import { PROJECT_EXECUTION_POLICY_SCHEMA, validateProjectExecutionPolicyValue } from '../../../shared/workjet-project-execution-policy-contract.generated.mjs';
import { validateSupervisorLumaValue } from '../../../shared/workjet-supervisor-luma-contract.generated.mjs';

export const WORKJET_PROJECT_CONTROL_MAX_RESULTS = 100;
export const WORKJET_PROJECT_CONTROL_MAX_WORKING_COPIES = 500;
export const WORKJET_PROJECT_CONTROL_TIMEOUT_MS = 30_000;

export function boundedWorkjetProjectText(value, field, maxLength, { allowEmpty = false } = {}) {
  const text = String(value ?? '').trim();
  if ((!allowEmpty && !text) || text.length > maxLength || /[\u0000-\u001f\u007f]/u.test(text)) {
    throw new Error(`Invalid Workjet project ${field}.`);
  }
  return text;
}

export function assertWorkjetProjectPayloadKeys(payload, allowed) {
  const unexpected = Object.keys(payload).filter((key) => !allowed.has(key));
  if (unexpected.length) {
    throw new Error(`Unsupported Workjet project payload field: ${unexpected[0]}`);
  }
}

export function boundedWorkjetProjectIsoDate(value, field) {
  const text = boundedWorkjetProjectText(value, field, 64);
  const date = new Date(text);
  if (!Number.isFinite(date.getTime())) {
    throw new Error(`Invalid Workjet project ${field}.`);
  }
  return date.toISOString();
}

// v4: Uhr als Abhängigkeit (V2-Tests setzen Date im vm-Kontext); Standard Date.now.
export async function awaitWorkjetProjectListStep(operation, deadline, phase, now = Date.now) {
  let timer;
  const timeout = () => Object.assign(
    new Error(`Workjet project list did not finish ${phase} before its deadline.`),
    { code: 'WORKJET_PROJECT_TIMEOUT', retryable: true },
  );
  try {
    const value = await Promise.race([
      operation,
      new Promise((resolve, reject) => {
        timer = setTimeout(() => reject(timeout()), Math.max(0, deadline - now()));
      }),
    ]);
    if (now() >= deadline) throw timeout();
    return value;
  } finally {
    clearTimeout(timer);
  }
}

// V2: Abfrage über die Sync-Brücke mit Bereitschaft, Abfrage-Generation und Seitenfenstern. In v4 liefert
// v2-compat.js dieselbe Brückenform über kernel.query (Generation = Collection-Liveness des Kernels).
export async function readWorkjetProjectListRows(bridge, query, requireRevision, deadline, signal, authority = {}, now = Date.now) { // v4-allow: debt L-42
  const current = !bridge?.state && bridge?.ready
    ? await awaitWorkjetProjectListStep(bridge.ready, deadline, 'bridge', now) : bridge;
  const peer = current?.state;
  if (!peer?.awaitQueryReady || peer.cancelled) {
    throw new Error('Workjet project query readiness is unavailable.'); // v4-allow: code (V2-Fehlertext, Workjet vergleicht wörtlich)
  }
  await awaitWorkjetProjectListStep(
    peer.awaitQueryReady(Math.max(1, deadline - now())), deadline, 'query readiness', now,
  );
  const generation = peer.collectionQueryGenerationToken?.(peer.activeRemotePeerId);
  const loader = peer.collection?.demandLoader;
  if (!generation || !loader) throw new Error('Workjet project query authority is unavailable.'); // v4-allow: code (V2-Fehlertext, Workjet vergleicht wörtlich)
  if (authority.peer && (authority.peer !== peer || authority.generation !== generation
    || authority.loader !== loader)) {
    throw new Error('Workjet project query generation changed.');
  }
  Object.assign(authority, { peer, generation, loader });
  const primaryPath = peer.collection.schema?.primaryPath || 'id';
  const rows = [];
  const ids = new Set();
  while (rows.length < query.limit) {
    const limit = Math.min(200, query.limit - rows.length);
    const page = await awaitWorkjetProjectListStep(peer.collection.find({
      ...query, limit, skip: rows.length, sort: [{ [primaryPath]: 'asc' }], requireRevision, signal, // v4-allow: debt L-42
    }).exec(), deadline, 'native projection', now);
    if (peer.cancelled || peer.collection.demandLoader !== loader
      || peer.collectionQueryGenerationToken?.(peer.activeRemotePeerId) !== generation) {
      throw new Error('Workjet project query generation changed.');
    }
    if (!Array.isArray(page) || page.length > limit) {
      throw new Error('Workjet project query exceeded its bounded window.');
    }
    for (const document of page) {
      const id = (document?.toJSON?.() || document)?.[primaryPath];
      if (typeof id !== 'string' || !id || ids.has(id)) {
        throw new Error('Workjet project query returned an invalid or repeated identity.');
      }
      ids.add(id);
    }
    rows.push(...page);
    if (page.length < limit) break;
  }
  return rows;
}

export async function listProjectedWorkjetProjects(env, ownerUserId, limit, workingCopies = [], nativeDocs = null, { includeConfiguration = false, includeSupervisorLuma = false, includeExecutionPolicy = false } = {}) {
  const collection = env.state.db?.collection?.('workjet_projects');
  const docs = nativeDocs ?? await collection.find({
    selector: { owner_user_id: { $eq: ownerUserId }, status: { $eq: 'active' } },
    limit: Math.min(limit, WORKJET_PROJECT_CONTROL_MAX_RESULTS),
  }).exec();
  const projects = docs
    .map((doc) => {
      const value = doc?.toJSON?.() || doc;
      if (value?.owner_user_id !== ownerUserId) return null;
      const project = boundedWorkjetProjectResult(value, { includeConfiguration, includeSupervisorLuma, includeExecutionPolicy });
      if (!project) return null;
      return Object.freeze({
        ...project,
        workingCopies: Object.freeze(
          workingCopies.filter((copy) => copy.projectId === project.id)
            .map(publicWorkjetWorkingCopyResult),
        ),
      });
    })
    .filter(Boolean)
    .sort((left, right) => {
      const leftCreatedAt = Date.parse(left.createdAt || '') || 0;
      const rightCreatedAt = Date.parse(right.createdAt || '') || 0;
      return rightCreatedAt - leftCreatedAt || left.id.localeCompare(right.id);
    });
  return projects;
}

export async function listProjectedWorkjetWorkingCopies(env, ownerUserId, nativeDocs = null) {
  const collection = env.state.db?.collection?.('workjet_working_copies');
  const docs = nativeDocs ?? await collection.find({
    selector: { owner_user_id: { $eq: ownerUserId } },
    limit: WORKJET_PROJECT_CONTROL_MAX_WORKING_COPIES,
  }).exec();
  return docs
    .map((doc) => {
      const value = doc?.toJSON?.() || doc;
      return value?.owner_user_id === ownerUserId ? boundedWorkjetWorkingCopyResult(value) : null;
    })
    .filter(Boolean)
    .sort((left, right) => left.id.localeCompare(right.id));
}

export function boundedWorkjetWorkingCopyResult(value) {
  if (!value || typeof value !== 'object' || value._deleted === true
    || value.is_deleted === true || !['active', 'detached'].includes(value.status)) {
    return null;
  }
  return Object.freeze({
    id: boundedWorkjetProjectText(value.id, 'workingCopy.id', 160),
    projectId: boundedWorkjetProjectText(value.project_id, 'workingCopy.projectId', 128),
    computerId: boundedWorkjetProjectText(value.computer_id, 'workingCopy.computerId', 256),
    path: boundedWorkjetProjectText(value.path, 'workingCopy.path', 4096),
    status: value.status,
    ownerUserId: boundedWorkjetProjectText(value.owner_user_id, 'workingCopy.ownerUserId', 256),
  });
}

export function publicWorkjetWorkingCopyResult(value) {
  return Object.freeze({
    id: value.id,
    computerId: value.computerId,
    path: value.path,
    status: value.status,
  });
}

export function boundedWorkjetProjectResult(value, { includeConfiguration = false, includeSupervisorLuma = false, includeExecutionPolicy = false } = {}) {
  if (!value || typeof value !== 'object' || value._deleted === true || value.is_deleted === true
    || value.status !== 'active') {
    return null;
  }
  const result = {
    id: boundedWorkjetProjectText(value.id, 'project.id', 128),
    title: boundedWorkjetProjectText(value.name, 'project.name', 256),
  };
  const createdAtMs = Number(value.created_at_ms);
  if (Number.isFinite(createdAtMs) && createdAtMs >= 0) {
    result.createdAt = new Date(createdAtMs).toISOString();
  }
  if (includeExecutionPolicy) {
    const policy = Object.hasOwn(value, 'execution_policy') ? value.execution_policy
      : { schema: PROJECT_EXECUTION_POLICY_SCHEMA, mode: 'default', revision: 0 };
    if (!validateProjectExecutionPolicyValue('ProjectExecutionPolicy', policy).ok
      || (policy.mode === 'autonomous_worktree' && policy.revision === 0)) {
      throw new Error('Invalid Workjet project execution policy.');
    }
    result.executionPolicy = Object.freeze({ ...policy });
  }
  if (!includeConfiguration && !includeSupervisorLuma) return Object.freeze(result);
  const metadata = boundedWorkjetProjectMetadata({
    ...(includeConfiguration && Object.hasOwn(value, 'description') ? { description: value.description } : {}),
    ...(includeConfiguration && Object.hasOwn(value, 'repo_url') ? { repoUrl: value.repo_url } : {}),
    ...(includeConfiguration && Object.hasOwn(value, 'public_url') ? { publicUrl: value.public_url } : {}),
    ...(includeConfiguration && Object.hasOwn(value, 'info') ? { info: value.info } : {}),
    ...(includeConfiguration && Object.hasOwn(value, 'jour_fixe') ? { jourFixe: value.jour_fixe } : {}),
    ...(includeConfiguration && Object.hasOwn(value, 'feed_settings') ? { feedSettings: workjetFeedSettingsWire(value.feed_settings) } : {}),
    ...(includeSupervisorLuma && Object.hasOwn(value, 'supervisor_luma_id') ? { supervisorLumaId: value.supervisor_luma_id } : {}),
  });
  for (const [nativeKey, key] of [
    ['description', 'description'], ['repo_url', 'repoUrl'], ['public_url', 'publicUrl'],
    ['info', 'info'], ['jour_fixe', 'jourFixe'],
    ['supervisor_luma_id', 'supervisorLumaId'],
  ]) {
    if (Object.hasOwn(metadata, nativeKey)) result[key] = metadata[nativeKey];
  }
  if (Object.hasOwn(metadata, 'feed_settings') && metadata.feed_settings !== null) {
    result.feedSettings = workjetFeedSettingsWire(metadata.feed_settings);
  }
  return Object.freeze(result);
}

const WORKJET_FEED_SOURCE_KEYS = ['disruptions', 'results', 'sessions', 'prs', 'meeting', 'setup'];
const WORKJET_FEED_CHOICES = { defaultTab: ['top', 'latest'], body: ['short', 'full'], replies: ['three', 'all'] };
const WORKJET_FEED_NATIVE = {
  title: 'title', windowDays: 'window_days', sources: 'sources', defaultTab: 'default_tab',
  body: 'body', replies: 'replies', notifyDisruptions: 'notify_disruptions',
};

/** Validates Workjet feed settings (camelCase wire, also accepts native snake_case) into one shape. */
function normalizeWorkjetFeedSettings(value, keyOf) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error('Invalid Workjet project feedSettings.');
  }
  const fields = Object.keys(WORKJET_FEED_NATIVE);
  assertWorkjetProjectPayloadKeys(value, new Set(fields.map(keyOf)));
  const get = (field) => value[keyOf(field)];
  const has = (field) => Object.hasOwn(value, keyOf(field));
  const out = {};
  if (has('title')) {
    const title = get('title');
    out.title = title === null || String(title).trim() === '' ? null
      : boundedWorkjetProjectText(title, 'feedSettings.title', 120);
  }
  if (has('windowDays')) {
    const days = get('windowDays');
    if (days !== null && ![1, 7, 30].includes(days)) throw new Error('Invalid Workjet project feedSettings.windowDays.');
    out.windowDays = days;
  }
  if (has('sources')) {
    const sources = get('sources');
    if (!sources || typeof sources !== 'object' || Array.isArray(sources)) {
      throw new Error('Invalid Workjet project feedSettings.sources.');
    }
    assertWorkjetProjectPayloadKeys(sources, new Set(WORKJET_FEED_SOURCE_KEYS));
    out.sources = {};
    for (const key of WORKJET_FEED_SOURCE_KEYS) {
      if (!Object.hasOwn(sources, key)) continue;
      if (typeof sources[key] !== 'boolean') throw new Error(`Invalid Workjet project feedSettings.sources.${key}.`);
      out.sources[key] = sources[key];
    }
  }
  for (const [field, allowed] of Object.entries(WORKJET_FEED_CHOICES)) {
    if (!has(field)) continue;
    if (!allowed.includes(get(field))) throw new Error(`Invalid Workjet project feedSettings.${field}.`);
    out[field] = get(field);
  }
  if (has('notifyDisruptions')) {
    if (typeof get('notifyDisruptions') !== 'boolean') throw new Error('Invalid Workjet project feedSettings.notifyDisruptions.');
    out.notifyDisruptions = get('notifyDisruptions');
  }
  return out;
}
export function workjetFeedSettingsNative(value) {
  const wire = normalizeWorkjetFeedSettings(value, (field) => field);
  return Object.freeze(Object.fromEntries(Object.entries(wire).map(([key, v]) => [WORKJET_FEED_NATIVE[key], v])));
}
export function workjetFeedSettingsWire(value) {
  if (value === null) return null;
  return Object.freeze(normalizeWorkjetFeedSettings(value, (field) => WORKJET_FEED_NATIVE[field]));
}

export function boundedWorkjetProjectMetadata(request) {
  const result = {};
  if (Object.hasOwn(request, 'executionPolicy')) {
    const value = request.executionPolicy;
    if (!validateProjectExecutionPolicyValue('ProjectExecutionPolicyUpdate', value).ok) {
      throw new Error('Invalid Workjet project executionPolicy.');
    }
    // Copy before awaiting dispatch; caller mutation cannot change the CAS patch.
    result.execution_policy = Object.freeze({ ...value });
  }
  if (Object.hasOwn(request, 'supervisorLumaId')) {
    const value = request.supervisorLumaId;
    const valid = validateSupervisorLumaValue('ProjectSupervisorLuma', { supervisor_luma_id: value });
    if (!valid.ok || value === undefined || (typeof value === 'string'
      && (!value.trim() || /[\u0000-\u001f\u007f]/u.test(value)))) {
      throw new Error('Invalid Workjet project supervisorLumaId.');
    }
    result.supervisor_luma_id = value === null ? null : value.trim();
  }
  if (Object.hasOwn(request, 'feedSettings')) {
    result.feed_settings = request.feedSettings === null ? null : workjetFeedSettingsNative(request.feedSettings);
  }
  for (const [key, nativeKey] of [
    ['description', 'description'], ['repoUrl', 'repo_url'], ['publicUrl', 'public_url'],
    ['info', 'info'], ['jourFixe', 'jour_fixe'],
  ]) {
    if (!Object.hasOwn(request, key)) continue;
    const value = request[key];
    if (value === null) { result[nativeKey] = null; continue; }
    if (key === 'info') {
      if (!value || typeof value !== 'object' || Array.isArray(value)) {
        throw new Error('Invalid Workjet project info.');
      }
      assertWorkjetProjectPayloadKeys(value, new Set(['summary', 'description', 'goal', 'phase', 'status']));
      const info = {};
      for (const [field, limit] of [['summary', 4096], ['description', 4096], ['goal', 4096], ['phase', 128], ['status', 128]]) {
        if (Object.hasOwn(value, field) && value[field] !== null) {
          const text = value[field];
          const multiline = field === 'summary' || field === 'description' || field === 'goal';
          if (typeof text !== 'string' || [...text.trim()].length > limit
            || (multiline ? /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/u : /[\u0000-\u001f\u007f]/u).test(text)) {
            throw new Error(`Invalid Workjet project info.${field}.`);
          }
          info[field] = text.trim();
        }
      }
      result[nativeKey] = Object.freeze(info);
    } else if (key === 'jourFixe') {
      if (!value || typeof value !== 'object' || Array.isArray(value)) {
        throw new Error('Invalid Workjet project jourFixe.');
      }
      assertWorkjetProjectPayloadKeys(value, new Set(['weekday', 'time', 'timezone']));
      const time = boundedWorkjetProjectText(value.time, 'jourFixe.time', 5);
      const timezone = Object.hasOwn(value, 'timezone')
        ? boundedWorkjetProjectText(value.timezone, 'jourFixe.timezone', 128) : 'Europe/Berlin';
      if (!Number.isInteger(value.weekday) || value.weekday < 1 || value.weekday > 7
        || !/^([01][0-9]|2[0-3]):[0-5][0-9]$/.test(time)) {
        throw new Error('Invalid Workjet project jourFixe weekday or time.');
      }
      // The native typed handler validates the IANA zone and owns scheduling.
      result[nativeKey] = Object.freeze({ weekday: value.weekday, time, timezone });
    } else {
      const text = boundedWorkjetProjectText(value, key, key === 'description' ? 4096 : 2048,
        { allowEmpty: key === 'description' });
      if (key !== 'description') {
        const url = new URL(text);
        if (!['http:', 'https:'].includes(url.protocol) || !url.host || url.username || url.password) {
          throw new Error(`Invalid Workjet project ${key}.`);
        }
      }
      result[nativeKey] = text;
    }
  }
  return result;
}
