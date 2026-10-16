// ref: store_outbound_commands.rs:4277 outbound_handle_campaign_mailbox_link — GEMISCHT.
// Native part: channels::upsert_communication_account → ctx.cap.mail.account.upsert.
import {
  requiredString, str, firstString, fail, load, asciiLower, putDefaultObject, putDefaultI64,
  putDefaultString, payloadInsert,
} from './_lib/outbound.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  const payload = command.payload ?? {};
  const campaignId = requiredString(payload, ['campaign_id']);
  const mailboxAddress = str(payload, ['mailbox_address']) ?? requiredString(payload, ['communication_account_address']);
  if (!mailboxAddress.includes('@')) fail('invalid', 'mailbox_address must be a valid email address');
  const accountKey = firstString([
    str(payload, ['communication_account_key']),
    str(payload, ['account_key']),
  ]) ?? `email:${asciiLower(mailboxAddress)}`;
  const channel = str(payload, ['channel']) ?? 'email';
  const provider = str(payload, ['provider']) ?? 'business-os.outbound';
  const mailboxStatus = str(payload, ['mailbox_status']) ?? 'ready';
  const displayName = str(payload, ['display_name']) ?? '';
  const replyTo = str(payload, ['reply_to']) ?? mailboxAddress;
  const profile = {
    address: mailboxAddress,
    campaign_id: campaignId,
    outbound_campaign_id: campaignId,
    display_name: displayName,
    reply_to: replyTo,
    mailbox_status: mailboxStatus,
    source: 'business-os.outbound.campaign.mailbox.link',
  };
  // Rust writes the core communication account FIRST; a failure aborts before any app write.
  // Kernprimitiv mail.channel.mailbox.link (native Payload des Kanal-Befehls; vormals mail.account.upsert).
  await ctx.cap.mail.channel.mailbox.link({ account_key: accountKey, channel, address: mailboxAddress, provider, profile });

  return ctx.transaction(async (tx) => {
    const campaign = (await load(tx, 'outbound_campaigns', campaignId)) ?? { id: campaignId };
    campaign.id = campaignId;
    campaign.communication_account_key = accountKey;
    campaign.communication_account_address = mailboxAddress;
    campaign.mailbox_status = mailboxStatus;
    putDefaultObject(campaign, 'payload');
    payloadInsert(campaign, 'communication_account_key', accountKey);
    payloadInsert(campaign, 'communication_account_address', mailboxAddress);
    payloadInsert(campaign, 'mailbox_status', mailboxStatus);
    putDefaultI64(campaign, 'created_at_ms', now);
    campaign.updated_at_ms = now;
    await tx.put('outbound_campaigns', campaign);

    const limit = (await load(tx, 'outbound_account_limits', accountKey)) ?? {
      id: accountKey, sender_account_id: accountKey, daily_sent_count: 0, daily_limit: 0, status: 'active', blocked: false,
    };
    limit.id = accountKey;
    limit.sender_account_id = accountKey;
    limit.campaign_id = campaignId;
    putDefaultI64(limit, 'daily_sent_count', 0);
    putDefaultI64(limit, 'daily_limit', 0);
    putDefaultString(limit, 'status', 'active');
    if (typeof limit.blocked !== 'boolean') limit.blocked = false;
    putDefaultI64(limit, 'created_at_ms', now);
    limit.updated_at_ms = now;
    await tx.put('outbound_account_limits', limit);
    return {
      ok: true, campaign, communication_account_key: accountKey, communication_account_address: mailboxAddress,
      mailbox_status: mailboxStatus, account_limit: limit,
    };
  });
}
