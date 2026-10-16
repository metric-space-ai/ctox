// Origin: CTOX
// License: Apache-2.0
// ctox.mailserver.save_user – ref: store.rs:16772-16851 @ 4794af03d
// Fachlogik (hier): username Pflicht, owner_user_id-Rückfall (owner_user_id → user_id → username),
// Postfachadresse = username klein, Konto-Schlüssel `email:<adresse>`.
// Fähigkeit (Kern): Passwort kommt NICHT aus dem Payload, sondern aus dem Secret-Intake des Befehls
// (take_secret_intake_value) – der Handler sieht es nie. Kern legt den Mailserver-Nutzer an und pflegt das
// Kommunikationskonto (channels::upsert_communication_account) → ctx.cap.mail.server.user.save.
import { callCap, fail, payloadOf } from './_base.js';

export function normalizeUser(payload) {
  const username = typeof payload.username === 'string' ? payload.username.trim() : '';
  const ownerRaw = typeof payload.owner_user_id === 'string' ? payload.owner_user_id
    : (typeof payload.user_id === 'string' ? payload.user_id : undefined);
  const owner = typeof ownerRaw === 'string' && ownerRaw.trim() !== '' ? ownerRaw.trim() : username;
  if (username === '') fail('invalid', 'username is required');
  const mailboxAddress = username.toLowerCase();
  return { username, owner_user_id: owner, mailbox_address: mailboxAddress, account_key: `email:${mailboxAddress}` };
}

export default async function handle(command, ctx) {
  const user = normalizeUser(payloadOf(command));
  // Kern prüft "password is required" am Secret-Intake und antwortet invalid, wenn es fehlt.
  await callCap(ctx, 'mail.server.user.save', {
    command_id: command.id,
    password: { secret_intake: true },
    username: user.username,
    owner_user_id: user.owner_user_id,
    communication_account: {
      account_key: user.account_key,
      channel: 'email',
      address: user.mailbox_address,
      provider: 'ctox-mailserver',
      metadata: {
        owner_user_id: user.owner_user_id,
        mailbox_address: user.mailbox_address,
        mailbox_status: 'ready',
        mailserver_managed: true,
      },
    },
  });
  return {
    username: user.username,
    communication_account_key: user.account_key,
    owner_user_id: user.owner_user_id,
    saved: true,
  };
}
