// Origin: CTOX
// License: Apache-2.0
// browser.clipboard.clear — ref: rxdb_peer_browser.rs:1701 (FAEHIGKEIT).
// Clears the session's in-memory clipboard (capability-held); Rust takes the
// nav_state answer shape afterwards with no runner call.
import { sessionId, tabId, assertLeaseHolder } from './_lib/browser.js';

export default async function handle(command, ctx) {
  const payload = command.payload ?? {};
  const id = sessionId(payload);
  const session = await ctx.get('browser_sessions', id);
  assertLeaseHolder(session, payload, ctx.now());
  return ctx.cap.browser.op({ session_id: id, tab_id: tabId(payload) || null, op: 'clipboard_clear', params: {} });
}
