// Origin: CTOX
// License: Apache-2.0
// ctox.ticket.publish_clarification – ref: mission/tickets/cases.rs:1360-1380 → publish_ticket_clarification_request
// (cases.rs:625-774). Nur Entwurf/Sendefehler, nur Anfragende über Ticket-Kommentar. Lokales System: öffentlicher
// Kommentar in der App, danach waiting_for_response; Fremdsystem (zammad): agent.task, Abschluss in _writeback.js.
import { fail, payloadOf, payloadString, requiredString } from './_base.js';
import {
  COLLECTIONS, WRITEBACK_SYSTEMS, clock, enqueueRemoteWriteback, loadClarification, loadTicket, localWriteback,
  recordAudit, seedOf, stableDigest,
} from './_tickets.js';

const DEFAULT_REVIEW_SUMMARY = 'Clarification question reviewed for this ticket.';

/** ref: cases.rs:920-936 list_inbound_ticket_event_keys */
export async function inboundEventKeys(db, ticketKey) {
  const rows = await db.read(COLLECTIONS.events, {
    selector: { ticket_key: ticketKey, direction: 'inbound' },
    sort: [{ external_created_at: 'asc' }, { observed_at: 'asc' }, { event_key: 'asc' }],
  });
  return rows.filter((row) => !row.is_deleted).map((row) => row.event_key);
}

/** ref: cases.rs:700-773 – Rückfrage nach erfolgreichem Rückschrieb auf waiting_for_response setzen. */
export async function markClarificationSent(db, time, clarification, { writebackId, remoteEventIds, reviewedBy, reviewSummary }) {
  const outboundKey = remoteEventIds[0] ?? writebackId;
  await db.patch(COLLECTIONS.clarifications, clarification.id, {
    status: 'waiting_for_response',
    outbound_message_key: outboundKey,
    metadata: {
      previous: clarification.metadata ?? {},
      reviewed_by: reviewedBy,
      review_summary: reviewSummary,
      writeback_id: writebackId,
      response_baseline_event_keys: await inboundEventKeys(db, clarification.ticket_key),
    },
    updated_at: time.iso,
    sent_at: time.iso,
    updated_at_ms: time.ms,
  });
  return outboundKey;
}

export default async function handle(command, ctx) {
  const p = payloadOf(command);
  const clarificationId = requiredString(p, 'clarification_id');
  const reviewedBy = payloadString(p, 'reviewed_by') ?? 'business-os';
  const reviewSummary = payloadString(p, 'review_summary') ?? DEFAULT_REVIEW_SUMMARY;
  const time = clock(ctx);
  const seed = seedOf(command);
  const writebackId = `clarification-writeback:${clarificationId}:${await stableDigest(seed)}`;
  const outcome = await ctx.transaction(async (tx) => {
    const clarification = await loadClarification(tx, clarificationId);
    if (clarification.status !== 'draft' && clarification.status !== 'send_failed') {
      fail('conflict', `clarification ${clarification.clarification_id} cannot be published from status ${clarification.status}`);
    }
    if (reviewedBy.trim() === '' || reviewSummary.trim() === '') {
      fail('invalid', 'publishing a clarification requires reviewed_by and review_summary');
    }
    if (clarification.target_type !== 'requester' || clarification.target_channel !== 'ticket') {
      fail('invalid', 'automatic clarification publish currently supports requester ticket comments only');
    }
    const ticket = await loadTicket(tx, clarification.ticket_key, 'ticket not found for clarification');
    if (!WRITEBACK_SYSTEMS.includes(ticket.source_system)) {
      fail('invalid', `unsupported ticket system for clarification publish: ${ticket.source_system}`);
    }
    const local = ticket.source_system === 'local';
    const result = local
      ? await localWriteback(tx, time, seed, ticket, { operation: 'comment', body: clarification.question, internal: false })
      : { remote_event_ids: [] };
    await tx.put(COLLECTIONS.writebacks, {
      id: writebackId, writeback_id: writebackId, case_id: clarification.case_id ?? '', ticket_key: clarification.ticket_key,
      operation: 'clarification_request',
      payload: {
        clarification_id: clarification.clarification_id, body: clarification.question, reviewed_by: reviewedBy,
        review_summary: reviewSummary, remote_event_ids: result.remote_event_ids,
      },
      status: local ? 'ok' : 'queued', created_at: time.iso, updated_at_ms: time.ms, is_deleted: false,
    });
    const outboundKey = local
      ? await markClarificationSent(tx, time, clarification, { writebackId, remoteEventIds: result.remote_event_ids, reviewedBy, reviewSummary })
      : null;
    return { clarification, ticket, local, outboundKey, after: await tx.get(COLLECTIONS.clarifications, clarificationId) };
  });
  const { clarification, ticket, after } = outcome;
  const base = { clarification: after, clarification_id: clarificationId, case_id: after.case_id ?? null, ticket_key: after.ticket_key };
  if (!outcome.local) {
    const taskId = await enqueueRemoteWriteback(ctx, ticket, writebackId, {
      operation: 'clarification_request', clarification_id: clarificationId, body: clarification.question, internal: false,
      reviewed_by: reviewedBy, review_summary: reviewSummary,
    });
    await ctx.patch(COLLECTIONS.writebacks, writebackId, { task_id: taskId });
    return { ...base, writeback_id: writebackId, writeback_status: 'queued', task_id: taskId };
  }
  await recordAudit(ctx, time, {
    ticket_key: ticket.ticket_key, case_id: clarification.case_id ?? null, actor_type: 'clarification_engine',
    action_type: 'clarification_published',
    details: { clarification_id: clarificationId, reviewed_by: reviewedBy, review_summary: reviewSummary, outbound_message_key: outcome.outboundKey },
  });
  return base;
}
