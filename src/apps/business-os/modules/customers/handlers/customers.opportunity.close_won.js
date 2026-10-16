// customers.opportunity.close_won – ref: store_customer_commands.rs:47 → :611-636 (customers_opportunity_close) @ 4794af03d
import { moveStage } from './customers.opportunity.move_stage.js';
import { clone, isObject } from './_lib.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  const payload = clone(command.payload);
  if (isObject(payload)) {
    payload.stage = 'closed_won';
    // TREUE: Rust setzt probability nur in den Befehls-Payload; move_stage übernimmt nur stage und position.
    // Die 100 % landen daher NICHT auf der Opportunity (Befund B1 in V4-HANDLERS.md).
    payload.probability = 100;
  }
  return ctx.transaction((tx) => moveStage(tx, { ...command, payload }, now));
}
