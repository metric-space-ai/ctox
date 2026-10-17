// Origin: CTOX
// License: Apache-2.0
// browser.controller.renew — ref: rxdb_peer_browser.rs:1222 (FACHLOGIK: controller lease bookkeeping).
import { runController } from './_lib/run.js';

export default async function handle(command, ctx) {
  return runController(command, ctx);
}
