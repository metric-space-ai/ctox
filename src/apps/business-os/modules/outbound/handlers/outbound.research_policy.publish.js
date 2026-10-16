// ref: store_outbound_commands.rs:1154 → outbound_handle_research_policy_publish :4529
// Fachlogik (slug, policy contract, manifest, record) here. Strang OUT3: das Ziel-Manifest legt die Eigentümer-App
// outbound-lead-generation an – ctx.command('outbound.scrape.target.upsert', {manifest}) statt Core-Fähigkeit
// scrape.target.upsert; manifest_path in OLG-Form (`outbound_lead_generation_scrape_targets/<target_key>`).
import { str, firstString, fail, objectPayload } from './_lib/outbound.js';
import { requestTargetUpsert, olgManifestPath } from './_lib/olg-registry.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  const payload = command.payload ?? {};
  // Rust also falls back to command.module; v4 carries it as app_id.
  const app = firstString([
    str(payload, ['app']),
    str(payload, ['module']),
    typeof command.app_id === 'string' && command.app_id.trim() ? command.app_id : null,
  ]) ?? 'outbound-lead-generation';
  const appSlug = Array.from(app, (ch) => (/^[A-Za-z0-9-]$/.test(ch) ? ch.toLowerCase() : '-')).join('');
  if (!appSlug) fail('invalid', 'app id is required');
  const targetKey = `${appSlug}-policy`;
  const policy = objectPayload(payload);
  policy.app = app;
  policy.published_at_ms = now;
  policy.policy_contract = 'ctox.outbound.research_policy.v1';
  const manifest = {
    target_key: targetKey,
    display_name: `${app} · Rechercheablauf und Einstellungen`,
    start_url: 'https://ctox.local/app-policy',
    target_kind: 'app-policy',
    status: 'active',
    config: policy,
    output_schema: { type: 'object' },
  };
  const effect = await requestTargetUpsert(ctx, manifest, `${command.id ?? targetKey}:scrape-target`);
  const recordId = `policy:${appSlug}`;
  const record = JSON.parse(JSON.stringify(manifest.config));
  record.id = recordId;
  await ctx.put('outbound_research_policies', record);
  return { ok: true, app, target_key: targetKey, manifest_path: olgManifestPath(targetKey), registration_command_id: effect.command_id, record_id: recordId };
}
