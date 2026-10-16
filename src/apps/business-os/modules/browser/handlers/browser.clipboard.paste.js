// Origin: CTOX
// License: Apache-2.0
// browser.clipboard.paste — ref: rxdb_peer_browser.rs:1697 (FAEHIGKEIT).
// The pasted value comes from the session's in-memory clipboard, which the
// capability holds (it is deliberately never projected into RxDB, because a
// copied value can be a credential). The handler forwards the op; the
// capability injects the clipboard value.
import { sessionId, tabId, assertLeaseHolder } from './_lib/browser.js';

export default async function handle(command, ctx) {
  const payload = command.payload ?? {};
  const id = sessionId(payload);
  const session = await ctx.get('browser_sessions', id);
  assertLeaseHolder(session, payload, ctx.now());
  return ctx.cap.browser.op({ session_id: id, tab_id: tabId(payload) || null, op: 'clipboard_paste', params: {} });
}
