// core/pipeline.js — pure model for the generic record-pipeline that the
// matching module layers on top of its requirements/objects/results records.
//
// No persistence, no RxDB, no business_commands, no DOM. The UI stores a
// structured stage at `match.data.pipeline.stage` and a structured requisition
// header at `requirement.data.jobOrder`; these helpers reason about both
// without depending on storage.
//
// Baukasten note: the *engine* is a generic ordered-stage pipeline plus a
// structured record header. The recruiting specifics live only in the shipped
// profile below (CANDIDATE_STAGES, JOB_ORDER_FIELD_DEFS) — another vertical
// swaps the profile and reuses the same mechanics.

/**
 * @typedef CandidateStage
 * @property {string} key   stable id, stored on the record (never localized)
 * @property {string} label default German label (UI may re-localize)
 * @property {boolean} [terminal] stage that normally ends active work
 */

/** Shipped recruiting candidate pipeline. Ordered. @type {CandidateStage[]} */
export const CANDIDATE_STAGES = [
  { key: 'neu', label: 'Neu' },
  { key: 'screening', label: 'Screening' },
  { key: 'telefoninterview', label: 'Telefoninterview' },
  { key: 'kundenvorstellung', label: 'Kundenvorstellung' },
  { key: 'vertragsangebot', label: 'Vertragsangebot' },
  { key: 'eingestellt', label: 'Eingestellt', terminal: true },
  { key: 'abgelehnt', label: 'Abgelehnt', terminal: true },
  { key: 'on-hold', label: 'On Hold' },
];

const STAGE_BY_KEY = new Map(CANDIDATE_STAGES.map((stage) => [stage.key, stage]));
const STAGE_RANK = new Map(CANDIDATE_STAGES.map((stage, index) => [stage.key, index]));

/** Map legacy hashtag/status tokens onto the structured stage keys. */
const LEGACY_STATUS_TO_STAGE = {
  prospecting: 'neu',
  prematch: 'neu',
  active: 'screening',
  interview: 'telefoninterview',
  offer: 'vertragsangebot',
  hired: 'eingestellt',
  rejected: 'abgelehnt',
  'on-hold': 'on-hold',
};

export const DEFAULT_CANDIDATE_STAGE = 'neu';

/** @param {string} key */
export function isCandidateStage(key) {
  return STAGE_BY_KEY.has(key);
}

/** @param {string} key */
export function candidateStageLabel(key) {
  return STAGE_BY_KEY.get(key)?.label || key || '';
}

/** @param {string} key */
export function candidateStageRank(key) {
  return STAGE_RANK.has(key) ? STAGE_RANK.get(key) : -1;
}

/**
 * Resolve the structured stage for a match record. Prefers the explicit
 * `data.pipeline.stage`; falls back to a legacy `status` / hashtag canonical so
 * existing records keep working; defaults to the first stage.
 * @param {{ status?: string, data?: { pipeline?: { stage?: string } } }} match
 * @param {string} [legacyCanonical] optional canonical derived from notes
 */
export function normalizeCandidateStage(match, legacyCanonical) {
  const explicit = match?.data?.pipeline?.stage;
  if (typeof explicit === 'string' && STAGE_BY_KEY.has(explicit)) {
    return explicit;
  }
  for (const candidate of [legacyCanonical, match?.status]) {
    if (typeof candidate !== 'string') continue;
    if (STAGE_BY_KEY.has(candidate)) return candidate;
    const mapped = LEGACY_STATUS_TO_STAGE[candidate];
    if (mapped) return mapped;
  }
  return DEFAULT_CANDIDATE_STAGE;
}

/**
 * Transitions are permissive (a recruiter can move a candidate anywhere), but
 * a no-op and an unknown target are rejected so callers can guard the UI.
 * @param {string} from
 * @param {string} to
 */
export function canTransitionCandidate(from, to) {
  if (!isCandidateStage(to)) return false;
  return from !== to;
}

/**
 * Pure patch: returns the `data` object a match should be saved with so the
 * structured stage is set. Never mutates the input.
 * @param {object} data existing match.data (may be undefined)
 * @param {string} stage
 * @param {number} nowMs
 */
export function withCandidateStage(data, stage, nowMs) {
  const base = data && typeof data === 'object' ? data : {};
  const pipeline = base.pipeline && typeof base.pipeline === 'object' ? base.pipeline : {};
  return {
    ...base,
    pipeline: {
      ...pipeline,
      stage,
      stage_changed_at_ms: Number.isFinite(nowMs) ? nowMs : pipeline.stage_changed_at_ms || 0,
    },
  };
}

/** Group an array of matches into ordered stage buckets for a Kanban board. */
export function groupByCandidateStage(matches) {
  const buckets = new Map(CANDIDATE_STAGES.map((stage) => [stage.key, []]));
  for (const match of matches || []) {
    const stage = normalizeCandidateStage(match);
    if (!buckets.has(stage)) buckets.set(stage, []);
    buckets.get(stage).push(match);
  }
  return CANDIDATE_STAGES.map((stage) => ({
    key: stage.key,
    label: stage.label,
    terminal: Boolean(stage.terminal),
    items: buckets.get(stage.key) || [],
  }));
}

/**
 * @typedef JobOrderFieldDef
 * @property {string} key
 * @property {string} label
 * @property {'text' | 'number' | 'date' | 'select'} type
 * @property {string[]} [options] for type 'select'
 */

/** Shipped recruiting requisition header. @type {JobOrderFieldDef[]} */
export const JOB_ORDER_FIELD_DEFS = [
  { key: 'department', label: 'Abteilung', type: 'text' },
  { key: 'location', label: 'Einsatzort', type: 'text' },
  { key: 'headcount', label: 'Anzahl', type: 'number' },
  { key: 'start_date', label: 'Startdatum', type: 'date' },
  {
    key: 'contract_type',
    label: 'Vertragsart',
    type: 'select',
    options: ['festanstellung', 'zeitarbeit', 'try-and-hire', 'freelance'],
  },
  {
    key: 'shift_model',
    label: 'Schichtmodell',
    type: 'select',
    options: ['tag', 'frueh-spaet', 'drei-schicht', 'nacht', 'flexibel'],
  },
  { key: 'account_id', label: 'Kunden-ID', type: 'text' },
  { key: 'account_label', label: 'Kunde', type: 'text' },
  { key: 'contact_id', label: 'Ansprechpartner-ID', type: 'text' },
];

const JOB_ORDER_KEYS = JOB_ORDER_FIELD_DEFS.map((field) => field.key);

/**
 * Coerce raw form input into a clean job-order header stored at
 * `requirement.data.jobOrder`. Unknown keys are dropped; numbers are coerced.
 * @param {Record<string, unknown>} input
 */
export function normalizeJobOrderHeader(input) {
  const source = input && typeof input === 'object' ? input : {};
  const header = {};
  for (const def of JOB_ORDER_FIELD_DEFS) {
    const raw = source[def.key];
    if (raw === undefined || raw === null || raw === '') continue;
    if (def.type === 'number') {
      const num = Number(raw);
      if (Number.isFinite(num)) header[def.key] = num;
    } else {
      header[def.key] = String(raw).trim();
    }
  }
  return header;
}

/** Short one-line summary of a job-order header for list rows. */
export function summarizeJobOrder(header) {
  if (!header || typeof header !== 'object') return '';
  const parts = [];
  if (header.account_label) parts.push(String(header.account_label));
  if (header.location) parts.push(String(header.location));
  if (header.headcount) parts.push(`${header.headcount}×`);
  if (header.contract_type) parts.push(String(header.contract_type));
  return parts.join(' · ');
}

export const __pipelineInternals = { JOB_ORDER_KEYS, LEGACY_STATUS_TO_STAGE };
