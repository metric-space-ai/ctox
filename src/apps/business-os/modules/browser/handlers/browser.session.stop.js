// Origin: CTOX
// License: Apache-2.0
// browser.session.stop — ref: rxdb_peer_browser.rs:937 (runtime stop path).
// Stopping the embedded browser session is a pure engine FAEHIGKEIT.
import { sessionId } from './_lib/browser.js';

export default async function handle(command, ctx) {
  const id = sessionId(command.payload ?? {});
  return ctx.cap.browser.session_stop({ session_id: id });
}
