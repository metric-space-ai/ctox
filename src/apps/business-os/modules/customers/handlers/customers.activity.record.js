// customers.activity.record – ref: store_customer_commands.rs:832-864 (customers_activity_record) @ 4794af03d
import { validateLinks, requiredStr, str, writeActivity, clone, isObject } from './_lib.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const payload = command.payload;
    await validateLinks(tx, payload, false);
    const activityType = requiredStr(payload, ['activity_type']);
    const activity = await writeActivity(tx, command, now, {
      activity_type: activityType,
      name: str(payload, ['name']) ?? activityType,
      account_id: str(payload, ['account_id']),
      contact_id: str(payload, ['contact_id']),
      opportunity_id: str(payload, ['opportunity_id']),
      linked_record_type: str(payload, ['linked_record_type']),
      linked_record_id: str(payload, ['linked_record_id']),
      linked_record_name: str(payload, ['linked_record_name']),
      // Rust: payload.get("properties").cloned().unwrap_or({}) – ein explizites null bleibt null → {value:null}
      properties: isObject(payload) && Object.prototype.hasOwnProperty.call(payload, 'properties')
        ? clone(payload.properties)
        : {},
    });
    return { ok: true, activity };
  });
}
