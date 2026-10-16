// Origin: CTOX
// License: Apache-2.0
// ctox.coding.models – ref: store.rs:15883-15914 @ 4794af03d → pi_sidecar::coding_model_capabilities
// Fachlogik: module_id normalisieren und Pflicht prüfen. Rechte AppsView auf das Modul (Kern-Policy).
// Fähigkeit: Modellliste des pi-Sidecars → ctx.cap.agent.coding.models (E §3).
import { callCap, fail, payloadOf } from './_base.js';
import { sourceSanitizeSlug } from './_coding.js';

export default async function handle(command, ctx) {
  const moduleId = sourceSanitizeSlug(payloadOf(command).module_id);
  if (moduleId === '') fail('invalid', 'module_id is required');
  return callCap(ctx, 'agent.coding.models', { module_id: moduleId });
}
