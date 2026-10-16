// customers.opportunity.close_lost – ref: store_customer_commands.rs:50 → :611-636 (customers_opportunity_close) @ 4794af03d
import { moveStage } from './customers.opportunity.move_stage.js';
import { clone, isObject, requiredStr } from './_lib.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  const payload = clone(command.payload);
  if (isObject(payload)) {
    payload.stage = 'closed_lost';
    // TREUE: lost_reason ist Pflicht, wird aber wie probability=0 nur in den Payload gelegt und von move_stage
    // nicht gespeichert (Befund B1 in V4-HANDLERS.md).
    payload.lost_reason = requiredStr(command.payload, ['lost_reason']);
    payload.probability = 0;
  }
  return ctx.transaction((tx) => moveStage(tx, { ...command, payload }, now));
}
