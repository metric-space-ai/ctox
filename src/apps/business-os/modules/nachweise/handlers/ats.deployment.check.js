// Origin: CTOX
// License: Apache-2.0
// ats.deployment.check – ref: store_ats_commands.rs:54-88 @ 4794af03d (lesender Gate-Befehl)
import { blockerJson, evaluateDeploymentReadiness, fail, loadByField, payloadOf } from './_ats.js';

export default async function handle(command, ctx) {
  const p = payloadOf(command);
  if (typeof p.subject_id !== 'string') fail('invalid', 'ats.deployment.check requires subject_id');
  const required = Array.isArray(p.required_types) ? p.required_types.filter((v) => typeof v === 'string') : [];
  const credentials = await loadByField(ctx, 'business_credentials', 'subject_id', p.subject_id);
  const readiness = evaluateDeploymentReadiness(credentials, required, ctx.now());
  return { ok: true, ready: readiness.ready, blockers: blockerJson(readiness.blockers) };
}
