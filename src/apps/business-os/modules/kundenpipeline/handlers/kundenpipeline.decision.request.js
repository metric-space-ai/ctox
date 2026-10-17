// kundenpipeline.decision.request – Port aus decision_hub.rs handle_decision_request :274,
// request_agent_decision :306, normalize_agent_decision_request :985.
// Legt genau eine idempotente Workjet-Eskalation an (gleicher decision_key + gleicher
// Inhalt -> dieselbe Entscheidung; anderer Inhalt -> conflict).
//
// Hinweis: request_agent_decision wird in Rust zusaetzlich vom MCP-Werkzeug gerufen;
// deshalb als benannter Export `requestAgentDecision` verfuegbar.

import {
  AGENT_DECISION_MAX_CONTEXT_CHARS,
  AGENT_DECISION_MAX_OPTIONS,
  AGENT_DECISION_MAX_OPTION_DESCRIPTION_CHARS,
  AGENT_DECISION_MAX_OPTION_LABEL_CHARS,
  AGENT_DECISION_MAX_QUESTION_CHARS,
  AGENT_DECISION_MAX_TITLE_CHARS,
  AGENT_DECISION_MIN_OPTIONS,
  AGENT_DECISION_TYPE,
  COL_ENTSCHEIDUNGEN,
  WRAP_WIDTH,
  actorId,
  agentDecisionResponse,
  fail,
  loadAnyRecord,
  optionalTrimmed,
  requiredTrimmed,
  sha256Hex,
  sha256Id,
  upsert,
  workspaceDecisionUserIds,
  wrapText,
} from './_lib/hub.js';

const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);

// ref: decision_hub.rs normalize_agent_decision_request :985
// Schluesselreihenfolge exakt wie im Rust-json!-Makro (serde_json mit preserve_order),
// damit request_fingerprint fuer Altbestand gleich bleibt.
export function normalizeAgentDecisionRequest(payload, now) {
  const decisionKey = requiredTrimmed(payload, 'decision_key', 200);
  const title = requiredTrimmed(payload, 'title', AGENT_DECISION_MAX_TITLE_CHARS);
  const question = requiredTrimmed(payload, 'question', AGENT_DECISION_MAX_QUESTION_CHARS);
  const context = optionalTrimmed(payload, 'context', AGENT_DECISION_MAX_CONTEXT_CHARS) ?? '';
  const recommendation = optionalTrimmed(payload, 'recommendation', 120);
  const urgency = optionalTrimmed(payload, 'urgency', 16) ?? 'normal';
  if (!['normal', 'high', 'critical'].includes(urgency)) fail('invalid', 'urgency must be normal, high, or critical');
  const source = payload?.source;
  if (!isObject(source)) fail('invalid', 'source object is required');
  const authority = typeof source.authority === 'string' && source.authority.trim() ? source.authority.trim() : 'workjet';
  const environmentId = requiredTrimmed(source, 'environment_id', 200);
  const threadId = requiredTrimmed(source, 'thread_id', 200);
  const workjetInstanceId = optionalTrimmed(source, 'workjet_instance_id', 200);
  const correlation = payload?.correlation;
  if (!isObject(correlation)) fail('invalid', 'correlation object is required');
  const turnId = optionalTrimmed(correlation, 'turn_id', 200);
  const idempotencyKey = requiredTrimmed(correlation, 'idempotency_key', 200);
  if (idempotencyKey !== decisionKey) fail('invalid', 'correlation.idempotency_key must equal decision_key');
  const options = payload?.options;
  if (!Array.isArray(options)) fail('invalid', 'options array is required');
  if (options.length < AGENT_DECISION_MIN_OPTIONS || options.length > AGENT_DECISION_MAX_OPTIONS) {
    fail('invalid', `options must contain between ${AGENT_DECISION_MIN_OPTIONS} and ${AGENT_DECISION_MAX_OPTIONS} entries`);
  }
  const normalizedOptions = [];
  const optionIds = new Set();
  for (const option of options) {
    const id = requiredTrimmed(option, 'id', 120);
    if (optionIds.has(id)) fail('invalid', 'option ids must be unique');
    optionIds.add(id);
    const label = requiredTrimmed(option, 'label', AGENT_DECISION_MAX_OPTION_LABEL_CHARS);
    const description = optionalTrimmed(option, 'description', AGENT_DECISION_MAX_OPTION_DESCRIPTION_CHARS) ?? '';
    normalizedOptions.push({ id, label, description });
  }
  if (recommendation !== undefined && !optionIds.has(recommendation)) {
    fail('invalid', 'recommendation must name one option id');
  }
  // Rust: as_i64 – nur ganze Zahlen zaehlen.
  const expiresAtMs = Number.isInteger(payload?.expires_at_ms) ? payload.expires_at_ms : null;
  if (expiresAtMs !== null) {
    if (!(expiresAtMs > now)) fail('invalid', 'expires_at_ms must be in the future');
    if (expiresAtMs > now + 30 * 24 * 60 * 60 * 1_000) fail('invalid', 'expires_at_ms may be at most 30 days in the future');
  }
  return {
    decision_key: decisionKey,
    title,
    question,
    context,
    options: normalizedOptions,
    recommendation: recommendation ?? null,
    urgency,
    expires_at_ms: expiresAtMs,
    source: {
      authority,
      environment_id: environmentId,
      thread_id: threadId,
      workjet_instance_id: workjetInstanceId ?? null,
    },
    correlation: {
      decision_key: decisionKey,
      turn_id: turnId ?? null,
      idempotency_key: idempotencyKey,
    },
  };
}

// ref: decision_hub.rs request_agent_decision :306
export async function requestAgentDecision(ctx, payload, actor) {
  const now = ctx.now();
  const normalized = normalizeAgentDecisionRequest(payload, now);
  const s = normalized.source;
  const identity = `${s.authority ?? 'workjet'}\0${s.environment_id ?? ''}\0${s.thread_id ?? ''}\0${normalized.decision_key ?? ''}`;
  const decisionId = sha256Id('kpl-e-agent', identity);
  const requestFingerprint = sha256Hex(JSON.stringify(normalized));

  const existing = await loadAnyRecord(ctx, COL_ENTSCHEIDUNGEN, decisionId);
  if (existing) {
    if (existing.request_fingerprint !== requestFingerprint) {
      fail('conflict', 'decision_key_conflict: the same Decision Hub key was already used with different content');
    }
    return agentDecisionResponse(existing, now);
  }

  const lines = wrapText(normalized.question, WRAP_WIDTH);
  if (normalized.context) {
    lines.push('');
    lines.push(...wrapText(normalized.context, WRAP_WIDTH));
  }
  const users = await workspaceDecisionUserIds(ctx);
  const owner = users[0] ?? '';
  const actions = normalized.options.map((option) => ({
    wert: option.id ?? '',
    label: option.label ?? '',
    description: option.description ?? '',
  }));
  const decision = {
    id: decisionId,
    vorgang_id: '',
    typ: AGENT_DECISION_TYPE,
    titel: normalized.title,
    title: normalized.title,
    frage_json: {
      question: normalized.question,
      context: normalized.context,
      recommendation: normalized.recommendation ?? null,
      urgency: normalized.urgency ?? 'normal',
    },
    zeilen_json: lines,
    detail_seiten_json: [],
    aktionen_json: actions,
    backing_ref: '',
    source_json: normalized.source ?? {},
    correlation_json: normalized.correlation ?? {},
    request_fingerprint: requestFingerprint,
    status: 'offen',
    antwort_json: {},
    expires_at_ms: normalized.expires_at_ms,
    requested_by: actor,
    owner_user_id: owner,
    assigned_user_id: owner,
    participant_ids: users,
    is_deleted: false,
    created_at_ms: now,
    updated_at_ms: now,
  };
  if (decision.expires_at_ms === null) delete decision.expires_at_ms;
  await upsert(ctx, COL_ENTSCHEIDUNGEN, decision);
  return agentDecisionResponse(decision, now);
}

export default async function handle(command, ctx) {
  return requestAgentDecision(ctx, command.payload ?? {}, actorId(command, 'decision-hub-agent'));
}
