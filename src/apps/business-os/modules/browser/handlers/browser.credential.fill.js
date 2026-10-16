// Origin: CTOX
// License: Apache-2.0
// browser.credential.fill — ref: rxdb_peer_browser.rs:1703 (FAEHIGKEIT).
// The credential (selector/field_role/username/password) is resolved from the
// credentials vault inside the capability; the handler never sees the secret.
// It forwards the credential reference and the lease.
import { sessionId, tabId, assertLeaseHolder, payloadString, fail } from './_lib/browser.js';

export default async function handle(command, ctx) {
  const payload = command.payload ?? {};
  const id = sessionId(payload);
  const session = await ctx.get('browser_sessions', id);
  assertLeaseHolder(session, payload, ctx.now());
  const credentialRef = payloadString(payload, 'credential_id') ?? payloadString(payload, 'credential_ref');
  if (credentialRef === undefined) fail('invalid', 'browser.credential.fill requires credential_id');
  return ctx.cap.browser.op({
    session_id: id,
    tab_id: tabId(payload) || null,
    op: 'credential_fill',
    params: {
      credential_ref: credentialRef,
      selector: payloadString(payload, 'selector') ?? null,
      field_role: payloadString(payload, 'field_role') ?? null,
    },
  });
}
