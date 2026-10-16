// customers.account.create – ref: store_customer_commands.rs:68-116 (customers_account_create) @ 4794af03d
import {
  idFromCommand, ensureAbsent, objectPayload, requiredStr, putDefaultString, putDefaultBool, putDefaultObject,
  putDefaultI64, validateAccount, refreshSearchText, putRecord, writeActivity, str, clone,
} from './_lib.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const accountId = idFromCommand(command, ['account_id', 'id'], 'acct');
    await ensureAbsent(tx, 'customer_accounts', accountId, 'customer account already exists');
    const account = objectPayload(command.payload);
    account.id = accountId;
    requiredStr(account, ['name']);
    putDefaultString(account, 'account_status', 'active');
    putDefaultString(account, 'customer_stage', 'active');
    putDefaultString(account, 'health_status', 'unknown');
    putDefaultString(account, 'currency', 'EUR');
    putDefaultBool(account, 'ideal_customer_profile', false);
    putDefaultBool(account, 'is_deleted', false);
    putDefaultObject(account, 'payload');
    putDefaultI64(account, 'created_at_ms', now);
    validateAccount(account);
    refreshSearchText(account, ['name', 'domain', 'industry']);
    await putRecord(tx, 'customer_accounts', accountId, now, account);
    const activity = await writeActivity(tx, command, now, {
      activity_type: 'account_created',
      name: 'Kunde erstellt',
      account_id: accountId,
      linked_record_type: 'account',
      linked_record_id: accountId,
      linked_record_name: str(account, ['name']),
      properties: { account: clone(account) },
    });
    return { ok: true, collection: 'customer_accounts', account, activity };
  });
}
