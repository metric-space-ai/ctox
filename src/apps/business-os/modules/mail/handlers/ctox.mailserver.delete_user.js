// Origin: CTOX
// License: Apache-2.0
// ctox.mailserver.delete_user – ref: store.rs:16852-16899 @ 4794af03d
// Fachlogik: username Pflicht, Konto-Schlüssel `email:<username klein>`.
// Fähigkeit: stalwart_users/stalwart_mailboxes löschen, Kommunikationskonto trennen → ctx.cap.mail.server.user.delete
// (Kernvertrag core-contract.mjs: caps/mail.rs route mail.server/user.delete, native Payload {username}; den
// Konto-Schlüssel leitet der native Handler selbst ab, store.rs:16879).
import { callCap, fail, payloadOf } from './_base.js';

export default async function handle(command, ctx) {
  const payload = payloadOf(command);
  const username = typeof payload.username === 'string' ? payload.username.trim() : '';
  if (username === '') fail('invalid', 'username is required');
  const accountKey = `email:${username.toLowerCase()}`;
  await callCap(ctx, 'mail.server.user.delete', { username });
  return { username, communication_account_key: accountKey, deleted: true };
}
