// customers.opportunity.move_stage – ref: store_customer_commands.rs:555-609 (customers_opportunity_move_stage) @ 4794af03d
import {
  requiredFromPayloadOrRecord, requiredStr, validateAllowed, CUSTOMER_OPPORTUNITY_STAGES, loadRequired, str,
  validateStageTransition, i64, refreshSearchText, putRecord, writeActivity, touchAccountLastActivity,
} from './_lib.js';

// Auch von close_won/close_lost genutzt (Rust: customers_opportunity_close ruft move_stage).
export async function moveStage(tx, command, now) {
  const opportunityId = requiredFromPayloadOrRecord(command, ['opportunity_id', 'id'], 'opportunity_id is required');
  const stage = requiredStr(command.payload, ['stage']);
  validateAllowed('opportunity stage', stage, CUSTOMER_OPPORTUNITY_STAGES);
  const opportunity = await loadRequired(tx, 'customer_opportunities', opportunityId, 'customer opportunity not found');
  const fromStage = str(opportunity, ['stage']) ?? '';
  validateStageTransition(fromStage, stage);
  opportunity.stage = stage;
  opportunity.last_stage_changed_at_ms = now;
  const position = i64(command.payload, ['position']);
  if (position !== undefined) opportunity.position = position;
  refreshSearchText(opportunity, ['name', 'stage', 'opportunity_type']);
  await putRecord(tx, 'customer_opportunities', opportunityId, now, opportunity);
  // Rust prüft account_id erst NACH dem Schreiben; in v4 rollt die Transaktion das Schreiben zurück.
  const accountId = requiredStr(opportunity, ['account_id']);
  const activity = await writeActivity(tx, command, now, {
    activity_type: 'opportunity_stage_changed',
    name: 'Opportunity verschoben',
    account_id: accountId,
    contact_id: str(opportunity, ['primary_contact_id']),
    opportunity_id: opportunityId,
    linked_record_type: 'opportunity',
    linked_record_id: opportunityId,
    linked_record_name: str(opportunity, ['name']),
    properties: { from_stage: fromStage, to_stage: stage },
  });
  await touchAccountLastActivity(tx, accountId, now);
  return { ok: true, opportunity, activity };
}

export default async function handle(command, ctx) {
  const now = ctx.now();
  return ctx.transaction((tx) => moveStage(tx, command, now));
}
