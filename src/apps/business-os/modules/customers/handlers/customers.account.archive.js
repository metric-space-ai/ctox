// customers.account.archive – ref: store_customer_commands.rs:189-230 (customers_account_archive) @ 4794af03d
import {
  requiredFromPayloadOrRecord, loadRequired, refreshSearchText, putRecord, writeActivity, str,
} from './_lib.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const accountId = requiredFromPayloadOrRecord(command, ['account_id', 'id'], 'account_id is required');
    const account = await loadRequired(tx, 'customer_accounts', accountId, 'customer account not found');
    account.account_status = 'archived';
    account.customer_stage = 'archived';
    account.is_deleted = true;
    account.deleted_at_ms = now;
    refreshSearchText(account, ['name', 'domain', 'industry']);
    await putRecord(tx, 'customer_accounts', accountId, now, account);
    const activity = await writeActivity(tx, command, now, {
      activity_type: 'account_archived',
      name: 'Kunde archiviert',
      account_id: accountId,
      linked_record_type: 'account',
      linked_record_id: accountId,
      linked_record_name: str(account, ['name']),
      properties: { account_id: accountId },
    });
    return { ok: true, account, activity };
  });
}
