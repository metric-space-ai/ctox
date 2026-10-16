// Knowledge contract helpers for the outbound research processors.
// ref: importer.rs:240 OutboundKnowledgeRefs/outbound_refs, :1833 ensure_outbound_knowledge_contract,
//      :1958 ensure_knowledge_data_table, :2010 slug_for_key, :3519 load_contact_rows_for_pipeline,
//      :3556 append_knowledge_rows, :3583 append_run_status, :3640 str_path, :3651 first_nonempty.
// Native: the Knowledge CLI (Parquet DataFrames) via ctx.cap.knowledge.command(argv) → JSON
// (= crate::knowledge::dispatch_capturing). Argument vectors are verbatim from Rust.
import { fail } from './outbound.js';

/** ref: :3640 str_path — '' when missing, trimmed. */
export function strPath(value, path) {
  let current = value;
  for (const key of path) {
    if (current === null || typeof current !== 'object' || !(key in current)) return '';
    current = current[key];
  }
  return typeof current === 'string' ? current.trim() : '';
}

/** ref: :3651 first_nonempty */
export function firstNonempty(values) {
  for (const value of values) {
    const trimmed = String(value ?? '').trim();
    if (trimmed) return trimmed;
  }
  return '';
}

/** ref: :2010 slug_for_key */
export function slugForKey(value) {
  let out = '';
  for (const ch of String(value).toLowerCase()) {
    if (/^[a-z0-9_.-]$/.test(ch)) out += ch;
    else if (!out.endsWith('_')) out += '_';
    if (out.length >= 80) break;
  }
  const trimmed = out.replace(/^_+|_+$/g, '');
  return trimmed || 'campaign';
}

/** ref: :250 outbound_refs */
export function outboundRefs(command) {
  const payload = command.payload ?? {};
  const clientContext = command.client_context ?? {};
  const campaignId = firstNonempty([
    strPath(payload, ['writeback_contract', 'campaign_id']),
    strPath(payload, ['campaign', 'id']),
    strPath(clientContext, ['campaign_id']),
  ]);
  const campaignName = firstNonempty([
    strPath(payload, ['writeback_contract', 'campaign_name']),
    strPath(payload, ['campaign', 'name']),
    strPath(payload, ['title']),
  ]);
  const domain = firstNonempty([
    strPath(payload, ['writeback_contract', 'domain']),
    strPath(payload, ['knowledge', 'domain']),
    strPath(clientContext, ['knowledge_domain']),
    'outbound',
  ]);
  const slug = campaignId ? slugForKey(campaignId) : '';
  return {
    domain,
    companies_key: firstNonempty([strPath(payload, ['knowledge', 'companies_table_key']), campaignId ? `campaign_${slug}_companies` : '']),
    contacts_key: firstNonempty([
      strPath(payload, ['knowledge', 'contacts_table_key']),
      strPath(payload, ['writeback_contract', 'table_key']),
      campaignId ? `campaign_${slug}_contacts` : '',
    ]),
    runs_key: firstNonempty([strPath(payload, ['knowledge', 'runs_table_key']), campaignId ? `campaign_${slug}_research_runs` : '']),
    runbook_id: firstNonempty([
      strPath(payload, ['writeback_contract', 'runbook_id']),
      strPath(payload, ['knowledge', 'runbook_id']),
      campaignId ? `business-os.outbound.${slug}.runbook.v1` : '',
    ]),
    campaign_id: campaignId,
    campaign_name: campaignName,
  };
}

// §16.30: knowledge.query ist das Kernprimitiv (vormals knowledge.command); flache Argumente, die native
// Knowledge-Payload ist die Befehlszeile als argv.
async function knowledge(ctx, argv) {
  return ctx.cap.knowledge.query({ argv });
}

/** ref: :1958 ensure_knowledge_data_table */
async function ensureDataTable(ctx, domain, key, title, description) {
  let described = null;
  try {
    described = await knowledge(ctx, ['data', 'describe', '--domain', domain, '--key', key]);
  } catch {
    described = null;
  }
  if (described?.ok === true) return;
  const created = await knowledge(ctx, [
    'data', 'create', '--domain', domain, '--key', key, '--source-system', 'business-os.outbound',
    '--title', title, '--description', description,
  ]);
  if (created?.ok !== true) fail('unavailable', `knowledge data table create failed: ${JSON.stringify(created)}`);
}

async function ignoreErrors(promise) {
  try { await promise; } catch { /* Rust: let _ = ... */ }
}

/** ref: :1833 ensure_outbound_knowledge_contract */
export async function ensureOutboundKnowledgeContract(ctx, command, domain, companiesKey, campaignId, campaignName) {
  const payload = command.payload ?? {};
  const slug = slugForKey(campaignId);
  const contactsKey = firstNonempty([strPath(payload, ['knowledge', 'contacts_table_key']), `campaign_${slug}_contacts`]);
  const runsKey = firstNonempty([strPath(payload, ['knowledge', 'runs_table_key']), `campaign_${slug}_research_runs`]);
  const runbookId = firstNonempty([
    strPath(payload, ['writeback_contract', 'runbook_id']),
    strPath(payload, ['knowledge', 'runbook_id']),
    `business-os.outbound.${slug}.runbook.v1`,
  ]);
  const skillbookId = firstNonempty([strPath(payload, ['knowledge', 'skillbook_id']), 'business-os.outbound.skillbook.v1']);
  await ensureDataTable(ctx, domain, companiesKey, `${campaignName} · Unternehmen`, 'Outbound Campaign Firmen, Importjobs, Qualifikation und Unternehmens-Research.');
  await ensureDataTable(ctx, domain, contactsKey, `${campaignName} · Ansprechpartner`, 'Outbound Campaign Ansprechpartner- und Lead-Qualifikation.');
  await ensureDataTable(ctx, domain, runsKey, `${campaignName} · Research Runs`, 'Outbound Campaign Research-Auftraege, Status und CTOX Command-Referenzen.');
  await ignoreErrors(knowledge(ctx, [
    'skill', 'add-skillbook', '--id', skillbookId, '--title', 'Business OS Outbound Campaigns', '--version', 'v1',
    '--mission', 'Outbound Campaigns fuehren Firmenquellen ueber Unternehmensqualifikation, Ansprechpartner-Recherche und Lead-Qualifikation.',
    '--runtime-policy', 'Nutze Knowledge DataFrames als einzige record-shaped Wissensquelle. Outbound speichert nur Workflow-State und Referenzen.',
    '--workflow-backbone', 'source-import,company-research,pipeline-contact-research,lead-qualification',
    '--linked-runbooks', runbookId,
  ]));
  await ignoreErrors(knowledge(ctx, [
    'skill', 'add-runbook', '--id', runbookId, '--skillbook', skillbookId, '--title', `${campaignName} Campaign Runbook`,
    '--version', 'v1', '--problem-domain', 'outbound-campaign', '--status', 'active',
    '--item-labels', 'CAMPAIGN-SCOPE,DATAFRAMES,FUNNEL-RUNS',
  ]));
  const runbookChunk = `Campaign: ${campaignName}\nCampaign ID: ${campaignId}\n\nRecord-shaped Knowledge:\n- Companies DataFrame: ctox knowledge data describe --domain ${domain} --key ${companiesKey}\n- Contacts DataFrame: ctox knowledge data describe --domain ${domain} --key ${contactsKey}\n- Research Runs DataFrame: ctox knowledge data describe --domain ${domain} --key ${runsKey}\n\nFunnel:\n1. Importjobs anlegen, daraus Unternehmen extrahieren und nur Unternehmen in den Companies DataFrame schreiben.\n2. Unternehmensdaten recherchieren, belegen und Firmen qualifizieren.\n3. Erst nach Unternehmensqualifikation Ansprechpartner im Contacts DataFrame recherchieren.\n4. Ansprechpartner gegen Scope/ICP qualifizieren und erst dann als Lead markieren.\n\nGrenze: Vor Pipeline-Stufe keine Personen recherchieren und keine Outreach-Nachrichten erzeugen.`;
  await ignoreErrors(knowledge(ctx, [
    'skill', 'add-item', '--id', `${runbookId}.scope`, '--runbook', runbookId, '--skillbook', skillbookId,
    '--label', 'CAMPAIGN-SCOPE', '--title', 'Campaign Scope und Datenvertrag', '--problem-class', 'outbound-campaign-scope',
    '--chunk-text', runbookChunk, '--version', 'v1', '--status', 'active', '--skip-embedding',
  ]));
}

/** ref: :3519 load_contact_rows_for_pipeline */
export async function loadContactRowsForPipeline(ctx, refs, pipelineId) {
  const payload = await knowledge(ctx, [
    'data', 'select', '--domain', refs.domain, '--key', refs.contacts_key, '--where', `pipeline_id=${pipelineId}`, '--limit', '200',
  ]);
  return Array.isArray(payload?.rows) ? payload.rows : [];
}

/** ref: :3556 append_knowledge_rows */
export async function appendKnowledgeRows(ctx, domain, key, rows) {
  const payload = await knowledge(ctx, ['data', 'append', '--domain', domain, '--key', key, '--rows', JSON.stringify(rows)]);
  if (payload?.ok !== true) fail('unavailable', `knowledge data append failed: ${JSON.stringify(payload)}`);
}

/** ref: :3583 append_run_status */
export async function appendRunStatus(ctx, refs, command, runType, status, now) {
  if (!refs.runs_key) return;
  const commandId = command.id ?? '';
  const clientContext = command.client_context ?? {};
  const recordId = command.record_id ?? command.payload?.record_id;
  await appendKnowledgeRows(ctx, refs.domain, refs.runs_key, [{
    run_id: firstNonempty([strPath(command.payload ?? {}, ['run_id']), commandId]),
    command_id: commandId,
    campaign_id: refs.campaign_id,
    runbook_id: refs.runbook_id,
    record_id: typeof recordId === 'string' ? recordId : '',
    company_id: strPath(clientContext, ['company_id']),
    pipeline_id: strPath(clientContext, ['pipeline_id']),
    run_type: runType,
    status,
    ctox_status: status,
    updated_at_ms: now,
    created_at_ms: now,
  }]);
}
