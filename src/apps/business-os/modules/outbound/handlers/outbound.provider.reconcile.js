// ref: store_outbound_commands.rs:1151 → outbound_handle_provider_reconcile :5508 (GEMISCHT)
import { reconcileProviderOutcomes } from './_lib/reconcile.js';

export default async function handle(_command, ctx) {
  return reconcileProviderOutcomes(ctx, ctx.now());
}
