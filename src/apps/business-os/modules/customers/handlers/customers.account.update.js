// customers.account.update – ref: store_customer_commands.rs:118-187 (customers_account_update) @ 4794af03d
import {
  requiredFromPayloadOrRecord, loadRequired, mergeFields, validateAccount, refreshSearchText, putRecord,
  writeActivity, str, clone,
} from './_lib.js';

const FIELDS = [
  'name', 'domain', 'website_url', 'linkedin_url', 'x_url', 'account_status', 'customer_stage', 'account_owner_id',
  'annual_recurring_revenue_cents', 'currency', 'employee_count', 'industry', 'address', 'ideal_customer_profile',
  'source', 'source_record_id', 'last_activity_at_ms', 'next_action_at_ms', 'health_status', 'payload',
];

export default async function handle(command, ctx) {
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const accountId = requiredFromPayloadOrRecord(command, ['account_id', 'id'], 'account_id is required');
    const account = await loadRequired(tx, 'customer_accounts', accountId, 'customer account not found');
    mergeFields(account, command.payload, FIELDS);
    validateAccount(account);
    refreshSearchText(account, ['name', 'domain', 'industry']);
    await putRecord(tx, 'customer_accounts', accountId, now, account);
    const activity = await writeActivity(tx, command, now, {
      activity_type: 'account_updated',
      name: 'Kunde aktualisiert',
      account_id: accountId,
      linked_record_type: 'account',
      linked_record_id: accountId,
      linked_record_name: str(account, ['name']),
      properties: { patch: clone(command.payload) ?? null },
    });
    return { ok: true, collection: 'customer_accounts', account, activity };
  });
}
