// Scrape-registry effect of outbound.research_source.generate_adapter.
// ref: store_outbound_commands.rs:3101 outbound_apply_research_adapter_scrape_effect (generate path only),
//      :3937 outbound_register_research_scrape_target, :4239 outbound_write_research_scrape_manifest,
//      :4261 outbound_safe_path_component, :3759 outbound_queue_research_scraper_generation,
//      :4010 outbound_research_scrape_test_input, :4058 outbound_scrape_operation_timeout_ms.
// Strang OUT3 – keine Core-Fähigkeit scrape.* mehr; die Scrape-Registry gehört der App outbound-lead-generation:
//   Registrierung lesen  → foreign read outbound_lead_generation_scrape_targets (_lib/olg-registry.js targetRegistration)
//   Ziel anlegen         → ctx.command('outbound.scrape.target.upsert', {manifest}) an OLG (Annahme, §16.21)
//   Generierungsauftrag  → generisches agent.task.enqueue (flach, §16.30; vorher scrape.repair_task.create =
//                          channels::create_scrape_repair_queue_task); der Agent registriert/testet über die
//                          OLG-Befehle outbound.scrape.script.register / outbound.scrape.execute.
import { str, firstString, fail, isObject } from './outbound.js';
import { targetRegistration, requestTargetUpsert, olgWorkspaceDir, olgManifestPath } from './olg-registry.js';

export function safePathComponent(value) {
  const out = Array.from(String(value), (ch) => (/^[A-Za-z0-9._-]$/.test(ch) ? ch : '-')).join('');
  return out || 'adapter';
}

export function operationTimeoutMs(value) {
  if (value === undefined) return 90_000;
  const message = 'test_input.operation_timeout_ms must be an integer between 1000 and 300000';
  if (!(typeof value === 'number' && Number.isInteger(value) && value >= 0)) fail('invalid', message);
  if (value < 1_000 || value > 300_000) fail('invalid', message);
  return value;
}

/** ref: :4010 — returns an ordered list of [key, value] (serde preserve_order). */
export function researchScrapeTestInput(command, adapterPayload, sourceId) {
  const payload = command.payload ?? {};
  const company = firstString([str(payload, ['test_input', 'company']), str(payload, ['company'])]);
  if (company == null) fail('invalid', 'Provide a non-empty test_input.company (or company) before testing this research adapter');
  const countries = adapterPayload?.countries;
  const country = firstString([
    str(payload, ['test_input', 'country']),
    str(payload, ['country']),
    Array.isArray(countries) && typeof countries[0] === 'string' ? countries[0] : null,
  ]) ?? 'DE';
  const timeout = operationTimeoutMs(payload.test_input?.operation_timeout_ms);
  const taskId = typeof command.id === 'string' && command.id.trim() ? command.id.trim() : null;
  const entries = [
    ['company', company], ['country', country], ['source_id', sourceId], ['adapter_test', true],
    ['task_id', taskId], ['operation_timeout_ms', timeout],
  ];
  const email = str(payload, ['test_input', 'email']);
  if (email != null && new TextEncoder().encode(email).length <= 254 && email.split('@').length === 2) entries.push(['email', email]);
  return entries;
}

/** serde_json Map::remove under preserve_order = IndexMap::swap_remove. */
function swapRemove(entries, key) {
  const index = entries.findIndex(([k]) => k === key);
  if (index < 0) return entries;
  const last = entries.pop();
  if (index < entries.length) entries[index] = last;
  return entries;
}

const toObject = (entries) => Object.fromEntries(entries);

async function registerResearchScrapeTarget(ctx, command, adapterPayload, record, sourceId, targetKey) {
  const url = firstString([str(record, ['url']), str(adapterPayload, ['url'])]);
  const existing = await targetRegistration(ctx, targetKey);
  if (existing) return existing;
  const manifest = adapterPayload.target_manifest ?? record.payload?.target_manifest ?? {
    target_key: targetKey,
    display_name: firstString([str(record, ['label']), str(adapterPayload, ['label']), sourceId]) ?? sourceId,
    start_url: url ?? `https://${sourceId}/`,
    target_kind: 'prospect-research',
    status: 'active',
    config: {
      expected_provider: sourceId,
      country_hints: adapterPayload.countries !== undefined ? adapterPayload.countries : [],
      record_key_fields: ['field', 'source_url'],
    },
    output_schema: { schema_key: 'prospect.v1', primary_artifact_kind: 'field_records_json', record_key_fields: ['field', 'source_url'] },
  };
  // ref :4239 schrieb das Manifest nach outbound-adapters/<safe>/target.json + `ctox scrape upsert-target`; jetzt legt
  // OLG das Ziel an (Folge-Id je Ausgangsbefehl, Wiederholung erzeugt keinen zweiten Befehl).
  const sent = await requestTargetUpsert(ctx, manifest, `${command.id ?? targetKey}:scrape-target:${safePathComponent(targetKey)}`);
  return {
    ok: true,
    target_key: targetKey,
    registered_from: 'adapter_manifest',
    target_manifest: olgManifestPath(targetKey),
    registration_command_id: sent.command_id,
    workspace_dir: olgWorkspaceDir(targetKey),
    script_registered: false,
    next_step: 'generate and register the script with universal-scraping (outbound.scrape.script.register)',
  };
}

export const GENERATION_DONE = 'outbound.research_source.generate_adapter.done';

async function queueScraperGeneration(ctx, command, adapterPayload, sourceId, targetKey, effect, adapterId) {
  const label = firstString([str(adapterPayload, ['label']), sourceId]) ?? sourceId;
  const url = str(adapterPayload, ['url']) ?? '';
  const payload = command.payload ?? {};
  const manifest = adapterPayload.target_manifest ?? adapterPayload.payload?.target_manifest ?? payload.target_manifest ?? {};
  const contract = adapterPayload.scrape_contract ?? adapterPayload.payload?.scrape_contract ?? payload.scrape_contract ?? {};
  // Gerade bei OLG angefordertes Ziel: dessen Arbeitsbereich steht im Effekt (OLG legt ihn deterministisch an).
  const registration = (await targetRegistration(ctx, targetKey))
    ?? (effect?.registered_from === 'adapter_manifest' ? effect : null);
  if (!registration) fail('conflict', 'generation requires a registered scrape target');
  const workspace = registration.workspace_dir;
  if (typeof workspace !== 'string') fail('conflict', 'registered scrape target has no workspace');
  const commandId = command.id ?? '';
  const idempotencyKey = commandId
    ? `outbound-research-adapter:${targetKey}:${commandId}`
    : `outbound-research-adapter:${targetKey}:legacy:${ctx.now()}`;
  let testInput = null;
  try {
    testInput = swapRemove(researchScrapeTestInput(command, adapterPayload, sourceId), 'task_id');
  } catch {
    testInput = null; // Rust: .ok()
  }
  let testInstruction;
  if (testInput) {
    const input = toObject(testInput);
    const runnerSeconds = Math.ceil(operationTimeoutMs(input.operation_timeout_ms) / 1000) + 30;
    // Wortlaut wie OLG _lib/research-source-adapter.js (Befehl statt `ctox scrape execute`).
    testInstruction = `Schreibe exakt diesen Testinput in eine JSON-Datei unter ${workspace}/scripts/: ${JSON.stringify(input)}. Ergaenze task_id nur aus der tatsaechlichen eigenen Queue-Aufgabe des Harness, nie aus Target-Config oder geratenen IDs. Fuehre den Business-OS-Befehl \`outbound.scrape.execute\` (App outbound-lead-generation) mit {"target_key":"${targetKey}","timeout_seconds":${runnerSeconds},"input":<Inhalt der JSON-Datei>} aus. Dokumentiere Run-ID, Quellen und Felder.`;
  } else {
    testInstruction = 'Es fehlt ein gueltiger expliziter Testinput mit company. Registriere den Adapter, aber fuehre keinen Recherche-Test aus und erfinde keine Firma. Melde input_required.';
  }
  const prompt = 'Erzeuge oder repariere einen CTOX Universal-Scraping Adapter fuer die Outbound Research Quelle.\n'
    + 'Ziel:\n'
    + `- source_id: ${sourceId}\n`
    + `- label: ${label}\n`
    + `- url: ${url}\n`
    + `- target_key: ${targetKey}\n\n`
    + 'Anforderungen:\n'
    + '- Nutze den universal-scraping Skill.\n'
    + `- Arbeite ausschliesslich im isolierten Target-Workspace ${workspace}; bearbeite scripts/ und sources/. Aendere keine Datenbank, Elternverzeichnisse oder Runtime-Symlinks. Pruefe vorhandene registrierte Revisionen vor einer Neuerstellung.\n`
    + `- Das Target ist in der App-Registry registriert. Registriere den Script-Stand mit dem Business-OS-Befehl \`outbound.scrape.script.register\` (App outbound-lead-generation) mit {"target_key":"${targetKey}","script_text":<Skriptinhalt aus ${workspace}/scripts/>,"language":"javascript"}. Registrierung und Ausfuehrung laufen nur ueber diese App-Befehle; keine direkten Datenbankschreibzugriffe.\n`
    + '- Der Scraper muss `prospect.v1` Records ausgeben: field, value, confidence, source_url, note.\n'
    + '- Verwende keine Credential-Werte im Prompt, Code oder Log. Nur Secret-Referenzen sind erlaubt.\n'
    + '- Das Skript ist JavaScript (CommonJS oder ES-Modul) oder TypeScript, nie ein Shell-Skript (bash/sh werden nicht ausgefuehrt). Es laeuft in der Sandbox ueber den App-Host (olg-scrape-js bzw. olg-scrape-ts); stdin traegt das Host-Dokument, daher stdin nicht lesen; die CTOX_SCRAPE_*-Werte stehen in process.env. Anbieter-Konfiguration und Secret-Referenzen aus CTOX_SCRAPE_MANIFEST_JSON (config), die konkrete Abfrage aus CTOX_SCRAPE_INPUT_JSON, operation_timeout_ms fuer die Browseroperation, Ausgabedateien unter CTOX_SCRAPE_OUTPUT_DIR; Ergebnis als JSON auf stdout. Config und Owner-Angaben ersetzen keine native Autorisierung; bei Auth-Ablehnung keine alternativen Routen versuchen.\n'
    + `- ${testInstruction}\n\n`
    + `target_manifest:\n${JSON.stringify(manifest)}\n\n`
    + `scrape_contract:\n${JSON.stringify(contract)}\n`;
  // ref: channels::create_scrape_repair_queue_task → generisches agent.task.enqueue (flach, §16.30); Skill und
  // Anweisung sind App-Daten, Idempotenz über idempotency_key (vorher extra_metadata.idempotency_key).
  // Rückruf an die eigene App (§16.30 Nr. 3): Ergebnis des Auftrags am Adapter-Datensatz (generate_adapter.done).
  const task = await ctx.cap.agent.task.enqueue({
    title: `Outbound Scraper Adapter erzeugen: ${label}`,
    instruction: prompt,
    skill: 'universal-scraping',
    thread_key: `business-os/outbound/research-adapter/${targetKey}`,
    priority: 'low',
    idempotency_key: idempotencyKey,
    on_complete: GENERATION_DONE,
    on_failed: GENERATION_DONE,
    payload: {
      kind: 'scrape_repair',
      adapter_id: adapterId,
      channel: 'scrape',
      target_key: targetKey,
      workspace_root: workspace,
      business_os_command_id: commandId,
      source: 'outbound.research_source.generate_adapter',
      adapter_source_id: sourceId,
      scrape_repair: { workspace_root: workspace },
    },
  });
  const taskId = task?.task_id;
  return {
    ok: true,
    task_id: taskId,
    // Neuer Auftrag ohne ausdrücklichen Status ist pending (Rust: route_status des neuen Tasks).
    task_status: task?.task_status ?? task?.status ?? (taskId ? 'pending' : undefined),
    target_key: targetKey,
    suggested_skill: 'universal-scraping',
    secret_value_in_payload: false,
  };
}

/** Generate-path of outbound_apply_research_adapter_scrape_effect. */
export async function generateAdapterScrapeEffect({ command, ctx, adapterPayload, adapterId, sourceId, record }) {
  record.test_ok = false;
  const adapterKind = str(record, ['adapter_kind']) ?? str(adapterPayload, ['adapter_kind']) ?? 'custom_url';
  if (adapterKind !== 'scrape_target' && adapterKind !== 'custom_url') {
    return { ok: true, skipped: true, reason: 'source_is_not_scrape_target', adapter_kind: adapterKind };
  }
  const targetKey = firstString([str(record, ['target_key']), str(adapterPayload, ['target_key']), str(command.payload ?? {}, ['target_key'])]);
  if (targetKey == null) {
    const message = 'target_key is required for scrape adapter';
    record.status = 'adapter_failed';
    record.scrape_status = 'target_key_missing';
    record.last_error = message;
    return { ok: false, error: message };
  }
  let effect;
  try {
    effect = await registerResearchScrapeTarget(ctx, command, adapterPayload, record, sourceId, targetKey);
  } catch (error) {
    const message = error?.message ?? String(error);
    record.status = 'adapter_failed';
    record.scrape_status = 'registration_failed';
    record.last_error = message;
    return { ok: false, phase: 'register', target_key: targetKey, error: message };
  }
  const hasScript = isObject(effect) && effect.script_registered === true;
  if (!hasScript) {
    try {
      const taskEffect = await queueScraperGeneration(ctx, command, adapterPayload, sourceId, targetKey, effect, adapterId);
      const runnable = ['pending', 'leased', 'running', 'review_rework'].includes(taskEffect.task_status);
      effect.generation_task = taskEffect;
      const status = runnable ? 'generation_queued' : 'generation_blocked';
      record.status = status;
      record.scrape_status = status;
      return effect;
    } catch (error) {
      const message = error?.message ?? String(error);
      effect.generation_task = { ok: false, error: message };
      record.status = 'adapter_failed';
      record.scrape_status = 'generation_failed';
      record.last_error = message;
      return effect;
    }
  }
  record.status = 'adapter_ready';
  record.scrape_status = 'registered';
  return effect;
}
