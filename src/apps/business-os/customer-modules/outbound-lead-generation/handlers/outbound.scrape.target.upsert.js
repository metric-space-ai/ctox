// outbound.scrape.target.upsert — Ziel der App-eigenen Scrape-Registry anlegen/ändern (Strang OLGSCR).
// ref: capabilities/scrape/registry.rs:103-219 upsert_target (ported in _lib/scrape-registry.js).
// Angeboten auch für die App outbound (ctx.command('outbound.scrape.target.upsert', {manifest}),
// ersetzt dort die Core-Fähigkeit „Ziel-Manifest schreiben“).
import { isObject } from './_lib/olg.js';
import { TARGETS, upsertTarget } from './_lib/scrape-registry.js';

export default async function handle(command, ctx) {
  const payload = isObject(command.payload) ? command.payload : {};
  const manifest = isObject(payload.manifest) ? payload.manifest : payload;
  const target = await upsertTarget(ctx, manifest);
  return {
    ok: true,
    target_key: target.target_key,
    target_id: target.target_id,
    manifest_path: `${TARGETS}/${target.target_key}`,
    script_registered: target.script_registered === true,
  };
}
