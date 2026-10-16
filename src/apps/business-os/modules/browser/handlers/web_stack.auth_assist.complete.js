// Origin: CTOX
// License: Apache-2.0
// web_stack.auth_assist.complete — ref: rxdb_peer_browser.rs:1330 (GEMISCHT).
// FACHLOGIK (here): require the explicit confirmation flag and the session.
// FAEHIGKEIT: the assisted-auth completion handshake in the browser engine.
import { sessionId, assertLeaseHolder, fail } from './_lib/browser.js';

export default async function handle(command, ctx) {
  const payload = command.payload ?? {};
  const id = sessionId(payload);
  const session = await ctx.get('browser_sessions', id);
  if (!session || typeof session !== 'object') fail('not_found', 'browser session not found');
  assertLeaseHolder(session, payload, ctx.now());
  if (payload.confirmed !== true) fail('invalid', 'auth assist completion requires explicit confirmation');
  return ctx.cap.browser.auth_assist_complete({ session_id: id });
}
