// Shared helpers for the outbound v4 command handlers.
//
// Each helper is a faithful port of a Rust helper from the native snapshot
// (origin/main 4794af03d, src/core/business_os/). The `ref:` anchors name the
// original function so behavior can be re-checked line by line.
//
// No Node builtins: the app runtime gives handlers only `ctx`. The SHA-256
// below is therefore pure JS (needed for the deterministic ids/revisions the
// Rust side derives with `channels::stable_digest`).

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/** Throw a contract error `{code, message}`. */
export function fail(code, message) {
  const error = new Error(message);
  error.code = code;
  throw error;
}

// ---------------------------------------------------------------------------
// Value access — ref: store.rs outbound_string / outbound_first_string
// ---------------------------------------------------------------------------

function isObject(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}
export { isObject };

/** ref: store.rs:18292 outbound_string — trimmed, non-empty string at path, else null. */
export function str(value, path) {
  let cursor = value;
  for (const key of path) {
    if (!isObject(cursor) || !(key in cursor)) return null;
    cursor = cursor[key];
  }
  if (typeof cursor !== 'string') return null;
  const trimmed = cursor.trim();
  return trimmed ? trimmed : null;
}

/** ref: store.rs:18304 outbound_first_string */
export function firstString(values) {
  for (const value of values) {
    if (typeof value === 'string' && value !== '') return value;
  }
  return null;
}

/** ref: store.rs:18312 outbound_required_string */
export function requiredString(value, path) {
  const found = str(value, path);
  if (found == null) fail('invalid', `${path.join('.')} is required`);
  return found;
}

/** serde `Value::as_i64` — integral JSON numbers only. */
export function asI64(value) {
  return typeof value === 'number' && Number.isInteger(value) ? value : null;
}

/** serde `Value::as_bool` */
export function asBool(value) {
  return typeof value === 'boolean' ? value : null;
}

/** ref: store.rs:18285 outbound_object_payload — shallow clone of an object payload. */
export function objectPayload(payload) {
  return isObject(payload) ? clone(payload) : {};
}

/** Rust `to_ascii_lowercase` (non-ASCII untouched). */
export function asciiLower(value) {
  return String(value).replace(/[A-Z]/g, (ch) => ch.toLowerCase());
}

export function clone(value) {
  return value === undefined ? undefined : JSON.parse(JSON.stringify(value));
}

// ---------------------------------------------------------------------------
// Record mutation — ref: store.rs:18358-18410
// ---------------------------------------------------------------------------

export function putDefaultString(record, key, fallback) {
  if (str(record, [key]) == null) record[key] = fallback;
}

/** ref: outbound_put_default_i64 (as_i64 or as_u64 present => keep). */
export function putDefaultI64(record, key, fallback) {
  if (asI64(record[key]) == null) record[key] = fallback;
}

export function putDefaultObject(record, key) {
  if (!isObject(record[key])) record[key] = {};
}

/** ref: outbound_merge_fields — copy only listed keys that exist in patch. */
export function mergeFields(record, patch, keys) {
  if (!isObject(record) || !isObject(patch)) return;
  for (const key of keys) {
    if (key in patch) record[key] = clone(patch[key]);
  }
}

/** ref: store_outbound_commands.rs:5930 outbound_payload_insert */
export function payloadInsert(record, key, value) {
  if (!isObject(record.payload)) record.payload = {};
  record.payload[key] = value;
}

// ---------------------------------------------------------------------------
// Command ids — ref: store.rs:18316 outbound_id_from_command, :18340
// ---------------------------------------------------------------------------

/**
 * The v4 command contract has no top-level `record_id`. The Rust side read
 * `command.record_id`; we accept `command.record_id` (if the runtime keeps it)
 * and fall back to `payload.record_id`. See V4-HANDLERS.md "Offene Punkte".
 */
function commandRecordId(command) {
  const raw = command?.record_id ?? command?.payload?.record_id;
  return typeof raw === 'string' ? raw.trim() : null;
}

export function idFromCommand(command, payloadKeys, prefix) {
  const payload = command?.payload ?? {};
  let fromPayload = null;
  for (const key of payloadKeys) {
    fromPayload = str(payload, [key]);
    if (fromPayload != null) break;
  }
  const found = firstString([fromPayload, commandRecordId(command)]);
  if (found != null) return found;
  if (typeof command?.id === 'string') return `${prefix}_${stableDigest(command.id)}`;
  fail('invalid', 'record id is required');
}

export function requiredFromPayloadOrRecord(command, payloadKeys, message) {
  const payload = command?.payload ?? {};
  let fromPayload = null;
  for (const key of payloadKeys) {
    fromPayload = str(payload, [key]);
    if (fromPayload != null) break;
  }
  const found = firstString([fromPayload, commandRecordId(command)]);
  if (found == null) fail('invalid', message);
  return found;
}

/** ref: store.rs:18470 outbound_session_actor_id */
export function actorId(command) {
  return command?.actor?.id || 'business-os-user';
}

// ---------------------------------------------------------------------------
// Store access
// ---------------------------------------------------------------------------

/** ref: outbound_load_record — null when absent. */
export async function load(ctx, collection, id) {
  const doc = await ctx.get(collection, id);
  return doc ? clone(doc) : null;
}

/** ref: outbound_load_required */
export async function loadRequired(ctx, collection, id, message) {
  const doc = await load(ctx, collection, id);
  if (!doc) fail('not_found', message);
  return doc;
}

/** ref: outbound_load_records_by_string_field — trimmed string equality. */
export async function loadByStringField(ctx, collection, field, value) {
  const docs = await ctx.read(collection, { selector: {} });
  return docs.filter((doc) => str(doc, [field]) === value).map(clone);
}

// ---------------------------------------------------------------------------
// Outbound domain helpers — ref: store_outbound_commands.rs:6538-6760
// ---------------------------------------------------------------------------

/** ref: :6722 outbound_message_revision. Key order = serde_json preserve_order. */
export function messageRevision(message) {
  const material = {
    sender_account_id: str(message, ['sender_account_id']) ?? '',
    recipient_email: str(message, ['recipient_email']) ?? '',
    subject: str(message, ['subject']) ?? '',
    body_text: str(message, ['body_text']) ?? '',
    body_html: str(message, ['body_html']) ?? '',
  };
  return `rev_${stableDigest(JSON.stringify(material))}`;
}

/**
 * Content binding of an approval (Befund INV1-B1, Strang FIXINV). Hash over every
 * send-relevant field: channel, sender (canonical account key, so the send path's
 * own canonicalisation does not invalidate it), recipient (mail + postal address),
 * subject, text/html body, the letter document and attachments. `approve` stores it
 * on the approval; the send gate recomputes it from the message it is about to
 * send, so any content change after approval voids the approval — also one that
 * bypasses the handlers and leaves `revision_id` untouched.
 */
export function messageContentHash(message) {
  const sender = str(message, ['sender_account_id']) ?? '';
  const material = {
    channel: str(message, ['channel']) ?? 'email',
    sender_account_id: emailAccountKeyFrom(sender) ?? sender,
    recipient_email: asciiLower((str(message, ['recipient_email']) ?? '').trim()),
    recipient_address_text: str(message, ['recipient_address_text']) ?? '',
    subject: str(message, ['subject']) ?? '',
    body_text: str(message, ['body_text']) ?? '',
    body_html: str(message, ['body_html']) ?? '',
    document_id: str(message, ['document_id']) ?? '',
    document_version_id: str(message, ['document_version_id']) ?? '',
    document_pdf_url: str(message, ['document_pdf_url']) ?? '',
    attachments: message?.attachments ?? message?.payload?.attachments ?? null,
  };
  return `content_${stableDigest(JSON.stringify(material))}`;
}

/** ref: :6733 outbound_require_message_content */
export function requireMessageContent(message) {
  if (str(message, ['body_text']) == null && str(message, ['body_html']) == null) {
    fail('invalid', 'message body is required before approval');
  }
}

/** ref: :6538 outbound_latest_message (max_by_key keeps the LAST max). */
export function latestMessage(messages) {
  let best = null;
  let bestKey = -Infinity;
  for (const message of messages) {
    const key = asI64(message.updated_at_ms) ?? asI64(message.created_at_ms) ?? 0;
    if (key >= bestKey) {
      best = message;
      bestKey = key;
    }
  }
  return best ? clone(best) : null;
}

/** ref: :6659 outbound_update_engagement_status */
export async function updateEngagementStatus(store, engagementId, status, now) {
  if (!engagementId || !engagementId.trim()) return;
  const engagement = await load(store, 'outbound_engagements', engagementId);
  if (!engagement) return;
  engagement.status = status;
  engagement.updated_at_ms = now;
  await store.put('outbound_engagements', engagement);
}

/** ref: :6695 outbound_update_engagement_terminal_status */
export async function updateEngagementTerminalStatus(store, engagementId, status, reason, now) {
  if (!engagementId || !engagementId.trim()) return;
  const engagement = await load(store, 'outbound_engagements', engagementId);
  if (!engagement) return;
  engagement.status = status;
  const trimmed = typeof reason === 'string' ? reason.trim() : '';
  if (trimmed) {
    engagement[status === 'paused' ? 'paused_reason' : 'closed_reason'] = trimmed;
  }
  engagement.updated_at_ms = now;
  await store.put('outbound_engagements', engagement);
}

/** ref: :6677 outbound_send_status_for_resume */
export function sendStatusForResume(message) {
  switch (str(message, ['approval_status'])) {
    case 'approved': return 'approved_not_sent';
    case 'awaiting_approval': return 'awaiting_approval';
    case 'rejected': return 'rejected';
    default: return 'not_scheduled';
  }
}

/** ref: :6686 outbound_engagement_status_for_message_state */
export function engagementStatusForMessageState(message) {
  switch (str(message, ['approval_status'])) {
    case 'approved': return 'approved_for_send';
    case 'awaiting_approval': return 'awaiting_approval';
    case 'rejected': return 'rejected';
    default: return 'draft_prepared';
  }
}

/** ref: :6765 record_outbound_approval_decision_event → business_events row. */
export async function recordApprovalDecisionEvent(ctx, command, approval, message, now) {
  const approvalId = requiredString(approval, ['id']);
  const messageId = str(approval, ['message_id']) ?? str(message, ['id']) ?? '';
  const engagementId = str(approval, ['engagement_id']) ?? str(message, ['engagement_id']) ?? '';
  const decision = str(approval, ['decision']) ?? 'updated';
  const payload = {
    event_type: 'business_os.external_approval.decided',
    approval_id: approvalId,
    message_id: messageId,
    engagement_id: engagementId,
    decision,
    command_id: command.id ?? null,
    command_type: command.type,
    actor: command.actor ?? null,
    approval: {
      id: str(approval, ['id']),
      message_id: str(approval, ['message_id']),
      engagement_id: str(approval, ['engagement_id']),
      revision_id: str(approval, ['revision_id']),
      actor_user_id: str(approval, ['actor_user_id']),
      decision: str(approval, ['decision']),
      updated_at_ms: asI64(approval.updated_at_ms),
    },
    message: {
      id: str(message, ['id']),
      engagement_id: str(message, ['engagement_id']),
      campaign_id: str(message, ['campaign_id']),
      channel: str(message, ['channel']),
      draft_status: str(message, ['draft_status']),
      approval_status: str(message, ['approval_status']),
      send_status: str(message, ['send_status']),
      updated_at_ms: asI64(message.updated_at_ms),
    },
    observed_at_ms: now,
  };
  // Capability wish `cap.audit.event` (business_events has no app collection).
  await ctx.cap.audit.event({
    collection: 'outbound_approvals',
    record_id: approvalId,
    event_type: 'business_os.external_approval.decided',
    payload,
    observed_at_ms: now,
  });
}

// ---------------------------------------------------------------------------
// Delivery policy — ref: store_outbound_delivery_policy.rs
// ---------------------------------------------------------------------------

/**
 * Does `approval` cover exactly the content of `message`? Approvals written since
 * Strang FIXINV carry `content_hash`; older approvals (Bestand) only a revision id,
 * which is then checked against the revision RECOMPUTED from the content, never
 * against the stored `revision_id` of the message.
 */
export function approvalCoversContent(approval, message) {
  const approvedHash = str(approval, ['content_hash']);
  if (approvedHash != null) return approvedHash === messageContentHash(message);
  return str(approval, ['revision_id']) === messageRevision(message);
}

/** ref: :14 outbound_has_matching_approval — plus content binding (INV1-B1). */
export async function hasMatchingApproval(store, messageId, revisionId, message) {
  const approvals = await store.read('outbound_approvals', { selector: {} });
  return approvals.some((payload) =>
    str(payload, ['message_id']) === messageId
    && str(payload, ['revision_id']) === revisionId
    && str(payload, ['decision']) === 'approved'
    && approvalCoversContent(payload, message));
}

/** Domain as stored on a suppression entry: lower case, without a leading `@` or `.`. */
function normalizeSuppressionDomain(value) {
  return asciiLower(String(value ?? '').trim()).replace(/^[@.]+/, '');
}

/** A domain block covers the domain itself and every subdomain of it. */
export function suppressionDomainCovers(suppressedDomain, recipientDomain) {
  if (!suppressedDomain || !recipientDomain) return false;
  return recipientDomain === suppressedDomain || recipientDomain.endsWith(`.${suppressedDomain}`);
}

/**
 * Is a suppression entry in force? Schema form: `active` (0/false = inactive)
 * decides when present; otherwise the legacy `status` does.
 */
export function suppressionEntryActive(payload) {
  const active = payload?.active;
  if (typeof active === 'number' || typeof active === 'boolean') return Number(active) !== 0;
  const status = str(payload, ['status']) ?? 'active';
  return !(status === 'inactive' || status === 'deleted' || status === 'expired');
}

/**
 * What an entry blocks, read from BOTH forms (Befund INV3-B3, Strang FIXINV):
 * schema form `{scope:'email'|'domain', value, active}` (the only form written
 * now) and the legacy form `{email|recipient_email, domain, status}` (Bestand).
 */
export function suppressionEntryTargets(payload) {
  const emails = [];
  const domains = [];
  const scope = asciiLower(str(payload, ['scope']) ?? '');
  const value = str(payload, ['value']);
  if (value != null && scope === 'email') emails.push(asciiLower(value.trim()));
  if (value != null && scope === 'domain') domains.push(normalizeSuppressionDomain(value));
  const legacyEmail = str(payload, ['email']) ?? str(payload, ['recipient_email']);
  if (legacyEmail != null) emails.push(asciiLower(legacyEmail.trim()));
  const legacyDomain = str(payload, ['domain']);
  if (legacyDomain != null) domains.push(normalizeSuppressionDomain(legacyDomain));
  return { emails: emails.filter(Boolean), domains: domains.filter(Boolean) };
}

/** ref: :177 outbound_recipient_suppression_reason (email match beats a domain block). */
export async function recipientSuppressionReason(store, recipientEmail) {
  const recipient = asciiLower(String(recipientEmail ?? '').trim());
  const domain = recipient.split('@')[1] ?? '';
  const entries = await store.read('outbound_suppression_entries', { selector: {} });
  let domainReason = null;
  for (const payload of entries) {
    if (!suppressionEntryActive(payload)) continue;
    const reason = firstString([str(payload, ['reason']), str(payload, ['suppression_reason'])]) ?? 'suppressed';
    const { emails, domains } = suppressionEntryTargets(payload);
    if (emails.includes(recipient)) return reason;
    if (domainReason == null && domains.some((suppressed) => suppressionDomainCovers(suppressed, domain))) domainReason = reason;
  }
  return domainReason;
}

/** The one shape a NEW suppression entry has: the schema form (collections.schema.json). */
export function suppressionEntry({ id, scope, value, reason, source, engagementId, now }) {
  const entry = {
    id,
    scope,
    value: scope === 'domain' ? normalizeSuppressionDomain(value) : asciiLower(String(value ?? '').trim()),
    reason,
    active: 1,
    source,
    created_at_ms: now,
    updated_at_ms: now,
  };
  if (engagementId) entry.engagement_id = engagementId;
  return entry;
}

/** ref: :38 outbound_recipient_suppressed */
export async function recipientSuppressed(store, recipientEmail) {
  return (await recipientSuppressionReason(store, recipientEmail)) != null;
}

/** ref: :51 outbound_reply_suppression_reason */
export function replySuppressionReason(classification) {
  switch (asciiLower(String(classification ?? '').trim())) {
    case 'unsubscribe': case 'opt_out': case 'opt-out': case 'optout': case 'remove':
      return 'unsubscribe';
    case 'complaint': case 'spam_complaint': case 'spam': case 'abuse':
      return 'complaint';
    case 'bounce': case 'hard_bounce': case 'undeliverable':
      return 'bounce';
    default:
      return null;
  }
}

/** ref: :72 outbound_apply_out_of_office_wait */
export function applyOutOfOfficeWait(engagement, payload, now) {
  const OOO_DEFAULT_WAIT_MS = 3 * 24 * 60 * 60 * 1000;
  const until = asI64(payload?.ooo_until);
  const resumeAt = until != null && until > now ? until : now + OOO_DEFAULT_WAIT_MS;
  engagement.next_action_at_ms = resumeAt;
  payloadInsert(engagement, 'next_action_at_ms', resumeAt);
  payloadInsert(engagement, 'reply_wait_reason', 'out_of_office');
  payloadInsert(engagement, 'ooo_until', resumeAt);
}

/** ref: :97 outbound_apply_reply_suppression — returns suppression id or null. */
export async function applyReplySuppression(store, engagement, engagementId, classification, now) {
  const reason = replySuppressionReason(classification);
  if (!reason) return null;
  const recipientRaw = (firstString([
    str(engagement, ['recipient_email']),
    str(engagement, ['payload', 'recipient_email']),
    str(engagement, ['payload', 'contact_email']),
  ]) ?? '').trim();
  const recipient = asciiLower(recipientRaw);
  if (!recipient || !recipient.includes('@')) return null;
  if ((await recipientSuppressionReason(store, recipient)) != null) return null;
  const domain = recipient.split('@')[1] ?? '';
  const suppressionId = `supp_reply_${stableDigest(`${recipient}|${reason}`)}`;
  // Rust wrote ONE legacy entry carrying email AND domain, which the gate read as an
  // address block plus a domain block. Same effect in the schema form: two entries.
  await store.put('outbound_suppression_entries', suppressionEntry({
    id: suppressionId, scope: 'email', value: recipient, reason, source: 'reply_classification', engagementId, now,
  }));
  if (domain) {
    await store.put('outbound_suppression_entries', suppressionEntry({
      id: `supp_reply_domain_${stableDigest(`${domain}|${reason}`)}`,
      scope: 'domain', value: domain, reason, source: 'reply_classification', engagementId, now,
    }));
  }
  const stored = await load(store, 'outbound_engagements', engagementId);
  if (stored) {
    stored.status = 'stopped';
    payloadInsert(stored, 'stop_reason', reason);
    payloadInsert(stored, 'stopped_at_ms', now);
    stored.updated_at_ms = now;
    await store.put('outbound_engagements', stored);
  }
  return suppressionId;
}

/** ref: store_outbound_commands.rs:5939 outbound_email_account_key_from */
export function emailAccountKeyFrom(value) {
  if (typeof value !== 'string') return null;
  const raw = asciiLower(value.trim());
  if (!raw) return null;
  if (raw.startsWith('email:')) return raw;
  if (raw.includes('@')) return `email:${raw}`;
  return raw;
}

/** ref: :219 outbound_enforce_account_limit */
export async function enforceAccountLimit(store, senderAccountId) {
  const canonical = emailAccountKeyFrom(senderAccountId) ?? senderAccountId;
  const limit = (await load(store, 'outbound_account_limits', canonical))
    ?? (await load(store, 'outbound_account_limits', senderAccountId));
  if (!limit) return;
  if (asBool(limit.blocked) === true) fail('forbidden', 'sender account is blocked for outbound communication');
  const status = str(limit, ['status']) ?? 'active';
  if (['blocked', 'locked', 'suspended', 'disabled'].includes(status)) {
    fail('forbidden', `sender account status \`${status}\` is not eligible to send`);
  }
  const remaining = asI64(limit.remaining_today);
  if (remaining != null && !(remaining > 0)) fail('quota', 'sender account daily limit exhausted');
  const sent = asI64(limit.daily_sent_count);
  const limitValue = asI64(limit.daily_limit);
  if (sent != null && limitValue != null && limitValue > 0 && !(sent < limitValue)) {
    fail('quota', 'sender account daily limit exhausted');
  }
}

// ---------------------------------------------------------------------------
// stable_digest — ref: mission/channels/account_helpers.rs:277
// sha256 hex, first 24 chars. Pure JS (no node:crypto in the app runtime).
// ---------------------------------------------------------------------------

const K = new Uint32Array([
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]);

function utf8(input) {
  const out = [];
  for (const ch of String(input)) {
    let cp = ch.codePointAt(0);
    if (cp >= 0xd800 && cp <= 0xdfff) cp = 0xfffd; // lone surrogate (Rust &str cannot hold one)
    if (cp < 0x80) out.push(cp);
    else if (cp < 0x800) out.push(0xc0 | (cp >> 6), 0x80 | (cp & 63));
    else if (cp < 0x10000) out.push(0xe0 | (cp >> 12), 0x80 | ((cp >> 6) & 63), 0x80 | (cp & 63));
    else out.push(0xf0 | (cp >> 18), 0x80 | ((cp >> 12) & 63), 0x80 | ((cp >> 6) & 63), 0x80 | (cp & 63));
  }
  return out;
}

export function sha256Hex(input) {
  const bytes = utf8(input);
  const bitLength = bytes.length * 8;
  bytes.push(0x80);
  while (bytes.length % 64 !== 56) bytes.push(0);
  const hi = Math.floor(bitLength / 0x100000000);
  const lo = bitLength >>> 0;
  bytes.push((hi >>> 24) & 255, (hi >>> 16) & 255, (hi >>> 8) & 255, hi & 255);
  bytes.push((lo >>> 24) & 255, (lo >>> 16) & 255, (lo >>> 8) & 255, lo & 255);
  const h = new Uint32Array([
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
  ]);
  const w = new Uint32Array(64);
  const rotr = (x, n) => (x >>> n) | (x << (32 - n));
  for (let offset = 0; offset < bytes.length; offset += 64) {
    for (let i = 0; i < 16; i += 1) {
      const j = offset + i * 4;
      w[i] = (bytes[j] << 24) | (bytes[j + 1] << 16) | (bytes[j + 2] << 8) | bytes[j + 3];
    }
    for (let i = 16; i < 64; i += 1) {
      const s0 = rotr(w[i - 15], 7) ^ rotr(w[i - 15], 18) ^ (w[i - 15] >>> 3);
      const s1 = rotr(w[i - 2], 17) ^ rotr(w[i - 2], 19) ^ (w[i - 2] >>> 10);
      w[i] = (w[i - 16] + s0 + w[i - 7] + s1) >>> 0;
    }
    let [a, b, c, d, e, f, g, hh] = h;
    for (let i = 0; i < 64; i += 1) {
      const s1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25);
      const ch = (e & f) ^ (~e & g);
      const t1 = (hh + s1 + ch + K[i] + w[i]) >>> 0;
      const s0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22);
      const maj = (a & b) ^ (a & c) ^ (b & c);
      const t2 = (s0 + maj) >>> 0;
      hh = g; g = f; f = e; e = (d + t1) >>> 0;
      d = c; c = b; b = a; a = (t1 + t2) >>> 0;
    }
    h[0] += a; h[1] += b; h[2] += c; h[3] += d; h[4] += e; h[5] += f; h[6] += g; h[7] += hh;
  }
  return Array.from(h, (x) => x.toString(16).padStart(8, '0')).join('');
}

export function stableDigest(input) {
  return sha256Hex(input).slice(0, 24);
}

// ---------------------------------------------------------------------------
// Fresh ids — Rust uses Uuid::new_v4().simple(). The v4 ctx has no uuid
// source; we use Web Crypto when the runtime exposes it, else a digest of
// (command id, salt, now). Capability wish: ctx.uuid() (see V4-HANDLERS.md).
// ---------------------------------------------------------------------------

export function newSimpleUuid(ctx, command, salt) {
  const webCrypto = globalThis.crypto;
  if (webCrypto && typeof webCrypto.randomUUID === 'function') return webCrypto.randomUUID().replaceAll('-', '');
  return sha256Hex(`${command?.id ?? ''}|${salt}|${ctx.now()}`).slice(0, 32);
}
