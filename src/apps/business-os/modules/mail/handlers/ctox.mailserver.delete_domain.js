// Origin: CTOX
// License: Apache-2.0
// ctox.mailserver.delete_domain – ref: store.rs:16731-16771 @ 4794af03d
// Fachlogik: Pflichtfeld domain_name. Fähigkeit: DELETE FROM stalwart_domains → ctx.cap.mail.server.domain.delete
// (Kernvertrag core-contract.mjs: caps/mail.rs route mail.server/domain.delete, native Payload {domain_name}).
import { callCap, fail, payloadOf } from './_base.js';

export default async function handle(command, ctx) {
  const payload = payloadOf(command);
  const domainName = typeof payload.domain_name === 'string' ? payload.domain_name.trim() : '';
  if (domainName === '') fail('invalid', 'domain_name is required');
  await callCap(ctx, 'mail.server.domain.delete', { domain_name: domainName });
  return { domain_name: domainName, deleted: true };
}
