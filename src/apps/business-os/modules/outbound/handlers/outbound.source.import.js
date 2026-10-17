// ref: store.rs:10177 queue → importer.rs:26 handle_source_parse → import_outbound_companies :1652 — GEMISCHT.
// Fachlogik (contract, row mapping, ids, Germany filter detection) here. Native:
//   ctx.cap.browser.automation.run({session_id, steps, timeout_ms}) → {session_id, steps:[{op, ok}], completed, timed_out,
//     artifact:{ref, file_id}} (CAP-3d, §16.48 Nr. 2: navigate/wait/click/extract-Schritte, _lib/company-import-script.js);
//     Schrittergebnisse über ctx.cap.files.get({ref}) aus dem files-Datensatz der eigenen App (Strang OLG725).
//   ctx.cap.knowledge.command(argv) (DataFrames), ctx.cap.files.put({text}, {id, name, mime}) (Import-Protokoll, E58)
//     (runtime/business-os-imports/<command_id>/<name>).
import { fail, sha256Hex, asciiLower } from './_lib/outbound.js';
import { ensureOutboundKnowledgeContract, firstNonempty, strPath, appendKnowledgeRows } from './_lib/knowledge.js';
import { domainFromUrl } from './_lib/research.js';
import {
  MAX_ROWS, LETTERS_PER_RUN, SORT_LETTERS, STEPS_PER_LETTER, openSteps, letterSteps, teaserRows,
} from './_lib/company-import-script.js';

const AUTOMATION_TIMEOUT_MS = 180_000;

/**
 * Ein browser.automation.run-Aufruf + Ergebnis-Datensatz lesen → [{op, ok, result?, code?}].
 * `unavailable`/`invalid` vom Kern (Schritte extract/click/wait noch nicht gebaut) → saubere Meldung `unavailable`
 * (Zone über den Befehlsstatus), nie ein stiller Fehler und kein Rückfall auf ein Skript.
 */
async function runSteps(ctx, sessionId, steps) {
  let run;
  try {
    run = await ctx.cap.browser.automation.run({ session_id: sessionId, steps, timeout_ms: AUTOMATION_TIMEOUT_MS });
  } catch (error) {
    if (error?.code === 'unavailable' || error?.code === 'invalid') {
      fail('unavailable', `Firmen-Import nicht möglich: die Browser-Automatisierung des Kerns nimmt die Schritte (navigate/wait/click/extract) noch nicht an (${error.code}: ${error?.message || error}).`);
    }
    throw error;
  }
  const ref = run?.artifact?.ref;
  if (typeof ref !== 'string' || ref === '') fail('unavailable', 'Firmen-Import: Browser-Automatisierung lieferte keinen Ergebnis-Datensatz.');
  let file;
  try {
    file = await ctx.cap.files.get({ ref });
  } catch (error) {
    fail('unavailable', `Firmen-Import: Ergebnis-Datensatz nicht lesbar (files.get ${error?.code || 'unavailable'}).`);
  }
  let parsed;
  try {
    parsed = JSON.parse(file?.text ?? '');
  } catch {
    fail('invalid', 'Firmen-Import: Ergebnis-Datensatz der Browser-Automatisierung ist kein JSON.');
  }
  return { steps: Array.isArray(parsed?.steps) ? parsed.steps : [], summary: run, ref };
}

/** ref: importer.rs:2031 outbound_import_germany_only */
export function germanyOnly(payload) {
  const haystack = asciiLower(JSON.stringify({
    filter_prompt: payload?.filter_prompt ?? null,
    source_filter_prompt: payload?.source?.filter_prompt ?? null,
    definition: payload?.definition ?? null,
  }));
  return haystack.includes('deutschland') || haystack.includes('germany') || haystack.includes('"de"')
    || haystack.includes('sitz in deutschland') || haystack.includes('deutsche unternehmen');
}

export default async function handle(command, ctx) {
  // Rust: only module=outbound reaches import_outbound_companies.
  if (command.app_id !== 'outbound') fail('invalid', 'source.parse is only implemented for matching');
  const payload = command.payload ?? {};
  const clientContext = command.client_context ?? {};
  const commandId = command.id ?? '';
  const url = strPath(payload, ['source', 'url']);
  if (!url.trim()) fail('invalid', 'outbound import requires source.url');
  const recordId = command.record_id ?? payload.record_id;
  const sourceId = firstNonempty([
    strPath(payload, ['source_id']),
    strPath(clientContext, ['source_id']),
    typeof recordId === 'string' ? recordId : commandId,
  ]);
  const campaignId = firstNonempty([strPath(payload, ['writeback_contract', 'campaign_id']), strPath(clientContext, ['campaign_id'])]);
  const campaignName = firstNonempty([strPath(payload, ['writeback_contract', 'campaign_name']), strPath(payload, ['title'])]);
  const domain = firstNonempty([
    strPath(payload, ['writeback_contract', 'domain']), strPath(payload, ['knowledge', 'domain']), strPath(clientContext, ['knowledge_domain']),
  ]);
  const tableKey = firstNonempty([
    strPath(payload, ['writeback_contract', 'table_key']), strPath(payload, ['knowledge', 'companies_table_key']), strPath(clientContext, ['knowledge_table_key']),
  ]);
  if (!domain || !tableKey) fail('invalid', 'outbound import requires a knowledge data writeback_contract');
  await ensureOutboundKnowledgeContract(ctx, command, domain, tableKey, campaignId, campaignName);

  const onlyGermany = germanyOnly(payload);
  // Schritte statt Skript (§16.48 Nr. 2). Eine Sitzung je Befehl, damit die Seitenschaltung auf derselben Seite bleibt.
  const sessionId = `outbound-import-${sha256Hex(`${commandId}:${url}`).slice(0, 24)}`;
  const seen = new Map();
  const addRows = (result) => {
    for (const row of teaserRows(result, onlyGermany)) {
      if (seen.size >= MAX_ROWS) break;
      const key = `${row.name}|${row.href}`;
      if (!seen.has(key)) seen.set(key, row);
    }
  };
  const runs = [];
  const first = await runSteps(ctx, sessionId, openSteps(url));
  runs.push({ ref: first.ref, completed: first.summary?.completed ?? null, timed_out: first.summary?.timed_out === true });
  const failedOpen = first.steps.find((step) => step?.ok !== true);
  if (failedOpen || first.steps.length < 3) {
    fail('unavailable', `Firmen-Import: ${url} ließ sich nicht öffnen (${failedOpen?.op ?? 'extract'}: ${failedOpen?.code ?? 'kein Ergebnis'}).`);
  }
  addRows(first.steps[2].result);
  const skipped = [];
  let next = 0;
  while (seen.size < MAX_ROWS && next < SORT_LETTERS.length) {
    const letters = SORT_LETTERS.slice(next, next + LETTERS_PER_RUN);
    const batch = await runSteps(ctx, sessionId, letterSteps(letters));
    runs.push({ ref: batch.ref, completed: batch.summary?.completed ?? null, timed_out: batch.summary?.timed_out === true });
    let done = 0;
    let stopped = false;
    for (let i = 0; i < letters.length; i += 1) {
      const [click, wait, extract] = batch.steps.slice(i * STEPS_PER_LETTER, (i + 1) * STEPS_PER_LETTER);
      if (!click) { stopped = true; break; } // Zeitbudget des Aufrufs erschöpft (timed_out): neu ansetzen
      if (click.ok !== true) { skipped.push(letters[i]); done = i + 1; stopped = true; break; } // Buchstabe fehlt
      if (wait?.ok !== true || extract?.ok !== true) {
        fail('unavailable', `Firmen-Import: Seitenschaltung ${letters[i]} abgebrochen (${(wait?.ok !== true ? wait : extract)?.op ?? 'wait'}: ${(wait?.ok !== true ? wait : extract)?.code ?? 'kein Ergebnis'}).`);
      }
      addRows(extract.result);
      done = i + 1;
      if (seen.size >= MAX_ROWS) break;
    }
    if (done === 0 && stopped) fail('unavailable', 'Firmen-Import: Browser-Automatisierung hat keinen Schritt ausgeführt.');
    next += done;
  }
  const parsedRows = [...seen.values()].slice(0, MAX_ROWS);
  const automation = { session_id: sessionId, runs, skipped_letters: skipped, count: parsedRows.length };
  if (parsedRows.length === 0) fail('not_found', `outbound importer did not extract any companies from ${url}`);

  const now = ctx.now();
  const knowledgeRows = [];
  const recordIds = [];
  for (const item of parsedRows.slice(0, 100)) {
    const name = firstNonempty([strPath(item, ['name']), strPath(item, ['company_name'])]);
    if (!name) continue;
    const website = firstNonempty([strPath(item, ['website']), strPath(item, ['href'])]);
    const detailUrl = strPath(item, ['href']);
    const companyId = `co_${sha256Hex(`${campaignId}:${name}:${website}:${detailUrl}`).slice(0, 16)}`;
    const country = firstNonempty([strPath(item, ['country']), onlyGermany ? 'Deutschland' : '']);
    knowledgeRows.push({
      company_id: companyId,
      campaign_id: campaignId,
      campaign_name: campaignName,
      source_id: sourceId,
      source_url: url,
      company_name: name,
      website,
      domain: domainFromUrl(website),
      detail_url: detailUrl,
      booth: strPath(item, ['booth']),
      event: strPath(item, ['event']),
      city: strPath(item, ['city']),
      country,
      description: strPath(item, ['description']),
      qualification_status: 'input',
      research_status: '',
      pipeline_status: '',
      imported_at_ms: now,
      updated_at_ms: now,
      raw_json: JSON.stringify(item),
    });
    recordIds.push(companyId);
  }
  if (knowledgeRows.length === 0) fail('invalid', 'outbound importer extracted rows but none had a company name');
  await appendKnowledgeRows(ctx, domain, tableKey, knowledgeRows);
  // E58: kein natives artifact.write (Handler-Wächter E17.1) – das Import-Protokoll ist eine Datei der App
  // (files.put, je Befehl eine eigene id wie früher je command_id ein Artefakt).
  const companiesJson = {
    command_id: commandId,
    // Rust: queue_task.message_key; the v4 runtime has no queue task for a handler call.
    queue_task_id: null,
    source_id: sourceId,
    source_url: url,
    campaign_id: campaignId,
    knowledge_domain: domain,
    knowledge_table_key: tableKey,
    records_count: knowledgeRows.length,
    records: knowledgeRows,
    automation,
  };
  await ctx.cap.files.put({ text: JSON.stringify(companiesJson) }, { id: `outbound_companies-${commandId}`, name: 'outbound_companies.json', mime: 'application/json' });
  return { collection: 'knowledge_data_rows', definition_id: 'outbound.companies.v1', records_count: recordIds.length, record_ids: recordIds };
}
