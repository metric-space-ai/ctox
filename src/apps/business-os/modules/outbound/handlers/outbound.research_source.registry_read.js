// ref: store_outbound_commands.rs:1187 → outbound_handle_research_source_registry_read :1207
// Read-only. Strang OUT3: die Scrape-Registry ist App-Logik der Eigentümer-App outbound-lead-generation; outbound
// liest ihre Collections als foreign_collections (statt Core-Fähigkeit scrape.registry.read) – Lesesichten in
// _lib/olg-registry.js (Ziel-Liste :1278/registry.rs:18, letzte Läufe :1357, Konto-Zustände :1306, Skript :1420).
// Projektion je Ziel wie bisher.
// Skripttext (include_script_target): nicht mehr über files.get (Kern §16.47 Nr. 4), sondern als Befehl an OLG mit
// Rückruf outbound.research_source.script_read.done (Strang OLG725); `script` trägt hier nur die Annahme.
import { listTargets, lastRuns, accountStates, requestLatestScript } from './_lib/olg-registry.js';

const text = (value) => (typeof value === 'string' ? value : '');

export default async function handle(command, ctx) {
  const payload = command.payload ?? {};
  const includeScript = typeof payload.include_script_target === 'string' && payload.include_script_target.trim()
    ? payload.include_script_target.trim()
    : null;
  const targets = await listTargets(ctx);
  const wanted = Array.isArray(payload.target_keys)
    ? new Set(payload.target_keys.filter((key) => typeof key === 'string'))
    : null;
  const runs = await lastRuns(ctx);
  const accounts = await accountStates(ctx);
  const entries = [];
  for (const target of targets) {
    const key = text(target.target_key).trim();
    if (!key) continue;
    if (wanted && !wanted.has(key)) continue;
    const targetId = text(target.target_id);
    entries.push({
      target_key: key,
      target_id: targetId,
      display_name: text(target.display_name),
      status: text(target.status),
      target_kind: text(target.target_kind),
      start_url: text(target.start_url),
      latest_script_revision_no: target.latest_script_revision_no ?? null,
      last_run: runs[targetId]?.last ?? null,
      last_successful_run: runs[targetId]?.last_successful ?? null,
      account_state: accounts[targetId] ?? null,
    });
  }
  return {
    ok: true,
    schema: 'ctox.outbound.research_source_registry.v1',
    targets: entries,
    script: includeScript ? await requestLatestScript(ctx, includeScript, `${command.id}:script`) : null,
  };
}
