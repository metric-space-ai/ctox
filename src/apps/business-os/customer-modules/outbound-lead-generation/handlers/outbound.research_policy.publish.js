// outbound.research_policy.publish — FACHLOGIK über die App-eigene Scrape-Registry (Strang OLGSCR)
// ref: store_outbound_commands.rs:4529-4601 outbound_handle_research_policy_publish
//
// Publishes the app's research procedure as the scrape target `<app>-policy`
// (target_kind "app-policy", policy in `config`) and mirrors the policy record
// into `outbound_research_policies` as `policy:<app-slug>`.
//
// Rust order kept: (1) manifest file + `ctox scrape upsert-target --input`
// (now: target document in outbound_lead_generation_scrape_targets, registry.rs:103 ported),
// (2) business record upsert.
import { fail, firstString, objectPayload, str } from './_lib/olg.js';
import { TARGETS, upsertTarget } from './_lib/scrape-registry.js';

/** ref: :4542-4552 — ASCII alnum and '-' lowercased, every other char (code point) → '-'. */
export function appSlug(app) {
  let slug = '';
  for (const ch of app) {
    slug += /^[A-Za-z0-9-]$/.test(ch) ? ch.toLowerCase() : '-';
  }
  return slug;
}

export default async function handle(command, ctx) {
  const payload = command.payload;
  // command.module (Rust) == command.app_id (v4); Rust filters only blank, no trim.
  const moduleId = typeof command.app_id === 'string' && command.app_id.trim() !== '' ? command.app_id : null;
  const app = firstString([str(payload, ['app']), str(payload, ['module']), moduleId]) ?? 'outbound-lead-generation';
  const slug = appSlug(app);
  if (!slug) fail('invalid', 'app id is required');
  const targetKey = `${slug}-policy`;
  const now = ctx.now();

  const policy = objectPayload(payload);
  policy.app = app;
  policy.published_at_ms = now;
  policy.policy_contract = 'ctox.outbound.research_policy.v1';

  const manifest = {
    target_key: targetKey,
    display_name: `${app} · Rechercheablauf und Einstellungen`,
    // The policy is data, not a scraped site; the start URL is only the
    // manifest's required anchor and is never fetched for a policy target.
    start_url: 'https://ctox.local/app-policy',
    target_kind: 'app-policy',
    status: 'active',
    config: policy,
    output_schema: { type: 'object' },
  };

  // Rust wrote runtime/scraping/app-policy/<target_key>.json and ran `scrape upsert-target`;
  // the manifest now IS the registry document. manifest_path names its durable location.
  const upsert = await upsertTarget(ctx, manifest);

  const recordId = `policy:${slug}`;
  const record = JSON.parse(JSON.stringify(manifest.config));
  record.id = recordId;
  // ref: store_projections.rs upsert_business_record — id + updated_at_ms are set
  // by the writer; _rev/_deleted belong to the v4 store.
  record.updated_at_ms = now;
  await ctx.put('outbound_research_policies', record);

  return {
    ok: true,
    app,
    target_key: targetKey,
    manifest_path: `${TARGETS}/${upsert.target_key}`,
    record_id: recordId,
  };
}
