// Origin: CTOX
// License: Apache-2.0
// browser.reset — ref: rxdb_peer_browser.rs:1652 (FAEHIGKEIT: embedded browser engine).
// Thin handler: lease gate, then forward the engine op to cap.browser.
import { runEngine } from './_lib/run.js';

export default async function handle(command, ctx) {
  return runEngine(command, ctx);
}
