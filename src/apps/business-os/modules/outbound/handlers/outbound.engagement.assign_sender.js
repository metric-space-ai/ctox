// ref: store_outbound_commands.rs:160-210 ("outbound.engagement.assign_sender")
import {
  requiredFromPayloadOrRecord, requiredString, loadRequired, idFromCommand, objectPayload,
  putDefaultString, putDefaultI64,
} from './_lib/outbound.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  const engagementId = requiredFromPayloadOrRecord(command, ['engagement_id', 'id'], 'engagement_id is required');
  const senderAccountId = requiredString(command.payload, ['sender_account_id']);
  return ctx.transaction(async (tx) => {
    const engagement = await loadRequired(tx, 'outbound_engagements', engagementId, 'engagement not found');
    engagement.sender_account_id = senderAccountId;
    engagement.status = 'assigned';
    engagement.updated_at_ms = now;
    await tx.put('outbound_engagements', engagement);

    // Rust: outbound_id_from_command(...).unwrap_or_else(...). With a command id
    // the derived `assign_<digest>` always wins; the fallback only applies
    // without payload id, record id and command id.
    let assignmentId;
    try {
      assignmentId = idFromCommand(command, ['assignment_id'], 'assign');
    } catch {
      assignmentId = `assign_${engagementId}_${senderAccountId}`;
    }
    const assignment = objectPayload(command.payload);
    assignment.id = assignmentId;
    assignment.engagement_id = engagementId;
    assignment.sender_account_id = senderAccountId;
    putDefaultString(assignment, 'status', 'active');
    putDefaultI64(assignment, 'created_at_ms', now);
    assignment.updated_at_ms = now;
    await tx.put('outbound_sender_assignments', assignment);
    return { ok: true, engagement, assignment };
  });
}
