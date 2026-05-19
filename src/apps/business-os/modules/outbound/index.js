import {
  decodeBase64Utf8,
  extractCompanyRowsFromText,
  normalizeCompanyRow,
  openUniversalImporter,
  parseDelimitedText,
} from '../../shared/universal-importer.js';
import { showBusinessAlert, showBusinessConfirm, showBusinessPrompt } from '../../shared/dialogs.js';

const BUILD = '20260519-outbound-actions6';
const DEFAULT_CAMPAIGN_ID = 'outbound_default_campaign';
const DEFAULT_CAMPAIGN_NAME = 'Outbound Firmenqualifizierung';
const OUTBOUND_LAYOUT_KEY = 'ctox.businessOs.outbound.columnLayout';
const OUTBOUND_CENTER_SPLIT_KEY = 'ctox.businessOs.outbound.centerSplit';
const OUTBOUND_COL_MIN = Object.freeze({ left: 260, center: 420 });
const OUTBOUND_COL_LEFT_MAX = 760;
const OUTBOUND_CENTER_MIN = Object.freeze({ left: 360, right: 360 });
const OUTBOUND_EXPORT_NS = 'urn:schemas-microsoft-com:office:spreadsheet';
const OUTBOUND_TABLE_RENDER_LIMIT = 250;
const OUTBOUND_BATCH_LIMIT = 100;
const OUTBOUND_BATCH_DEFAULT = 50;
const OUTBOUND_KNOWLEDGE_DOMAIN = 'outbound';
const OUTBOUND_KNOWLEDGE_SKILLBOOK = 'business-os.outbound.campaigns.v1';
const OUTBOUND_KNOWLEDGE_APPEND_CHUNK = 250;
const AUTOMATION_STAGES = Object.freeze({
  company_research: {
    label: 'Company Research',
    description: 'Unternehmensdaten mit CTOX Web Research nachrecherchieren.',
    cta: 'Company Research starten',
  },
  pipeline: {
    label: 'Pipeline vorbereiten',
    description: 'Qualifizierte Unternehmen in die Ansprechpartner-Stufe übernehmen.',
    cta: 'In Pipeline übernehmen',
  },
  contact_research: {
    label: 'Ansprechpartner Research',
    description: 'Relevante Ansprechpartner und Rollen mit CTOX recherchieren.',
    cta: 'Ansprechpartner Research starten',
  },
  lead_qualification: {
    label: 'Lead-Qualifizierung',
    description: 'Recherchierte Ansprechpartner gegen Scope/ICP als Lead qualifizieren.',
    cta: 'Lead-Qualifizierung starten',
  },
});
const RESEARCH_FIELD_DEFS = Object.freeze([
  ['legal_form', 'Rechtsform'],
  ['country', 'Land'],
  ['postal_code', 'PLZ'],
  ['city', 'Ort'],
  ['street', 'Strasse'],
  ['registry_court', 'Firmenbuchgericht'],
  ['registry_id', 'Register-ID'],
  ['status', 'Status'],
  ['phone', 'Tel'],
  ['fax', 'Fax'],
  ['email', 'E-Mail'],
  ['domain', 'Domain'],
  ['vat_id', 'USt-Id'],
  ['industry_wz', 'Branche (WZ)'],
  ['representative_1', 'Ges. Vertreter 1'],
  ['representative_2', 'Ges. Vertreter 2'],
  ['representative_3', 'Ges. Vertreter 3'],
  ['business_purpose', 'Gegenstand'],
  ['tickers', 'Tickers'],
  ['financials_date', 'Finanzkennzahlen Datum'],
  ['share_capital_eur', 'Stamm-/Grundkapital EUR'],
  ['balance_sheet_total_eur', 'Bilanzsumme EUR'],
  ['profit_eur', 'Gewinn EUR'],
  ['revenue_eur', 'Umsatz EUR'],
  ['equity_eur', 'Eigenkapital EUR'],
  ['employee_count', 'Mitarbeiterzahl'],
]);
const DEFAULT_RESEARCH_FIELD_IDS = Object.freeze(RESEARCH_FIELD_DEFS.map(([id]) => id));
const CONTACT_FIELD_DEFS = Object.freeze([
  ['contact.people', 'Ansprechpartner', 'Gefundene relevante Personen, Rollen und kurze Einordnung.'],
  ['contact.role', 'Rolle', 'Funktion, Senioritaet und Verantwortungsbereich der wichtigsten Ansprechpartner.'],
  ['contact.email', 'E-Mail', 'Oeffentlich belegbare E-Mail-Adressen der Ansprechpartner.'],
  ['contact.linkedin', 'LinkedIn', 'Oeffentlich belegbare LinkedIn- oder Profil-URLs.'],
  ['contact.phone', 'Telefon', 'Oeffentlich belegbare Direktwahl oder zentrale Telefonnummer fuer den Kontakt.'],
  ['contact.fit', 'Kontakt Fit', 'Warum diese Person fuer den Campaign Scope relevant ist.'],
  ['contact.status', 'Kontakt', 'Status der Ansprechpartner-Qualifizierung.'],
  ['lead.reason', 'Lead Grund', 'Warum der Ansprechpartner als Lead qualifiziert oder abgelehnt wurde.'],
  ['lead.status', 'Lead', 'Status der Lead-Qualifizierung.'],
]);
const DEFAULT_CONTACT_FIELD_IDS = Object.freeze(['contact.people', 'contact.status', 'lead.status']);

const state = {
  ctx: null,
  campaigns: [],
  sources: [],
  companies: [],
  pipeline: [],
  runs: [],
  commands: [],
  queueTasks: [],
  selectedCampaignId: '',
  selectedCompanyId: '',
  selectedPipelineId: '',
  activeView: 'companies',
  filter: 'all',
  search: '',
  tableFilters: {},
  tableSort: null,
  editingCampaignId: '',
  knowledgeProjectionDisabled: false,
  knowledgeProjectionSignature: '',
  refreshTimer: null,
  knowledgeWatchTimer: null,
  centerRenderTimer: null,
  operationalRefreshPending: false,
  lastOperationalRefreshMs: 0,
  cleanup: [],
  centerResizeCleanup: null,
};

export async function mount(ctx) {
  state.ctx = ctx;
  await ensureStyles();
  ctx.host.innerHTML = await loadModuleMarkup();
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();
  await ensureDefaultCampaign();
  await loadAll({ hydrateKnowledge: false });
  wireEvents(ctx.host);
  wireRealtime();
  const resizeCleanup = setupOutboundColumnResizing();
  if (resizeCleanup) state.cleanup.push(resizeCleanup);
  render();
  ensureCampaignKnowledge(selectedCampaign())
    .then(async () => {
      await loadAll({ hydrateKnowledge: false });
      render();
    })
    .catch((error) => {
      console.warn('[outbound] selected campaign knowledge setup failed', error);
    });
  return () => {
    state.centerResizeCleanup?.();
    state.centerResizeCleanup = null;
    if (state.centerRenderTimer) window.clearTimeout(state.centerRenderTimer);
    state.centerRenderTimer = null;
    state.cleanup.forEach((fn) => fn?.());
    state.cleanup = [];
    ctx.host.replaceChildren();
  };
}

async function ensureStyles() {
  const href = `${new URL('./index.css', import.meta.url).pathname}?v=${BUILD}`;
  if (document.querySelector(`link[href="${href}"]`)) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  document.head.append(link);
}

async function loadModuleMarkup() {
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  return doc.body.innerHTML;
}

function campaignKnowledgeRefs(campaign) {
  const slug = slugId(campaign?.id || campaign?.name || 'campaign');
  const current = campaign?.payload?.knowledge || {};
  return {
    domain: current.domain || OUTBOUND_KNOWLEDGE_DOMAIN,
    skillbookId: current.skillbook_id || OUTBOUND_KNOWLEDGE_SKILLBOOK,
    runbookId: current.runbook_id || `business-os.outbound.${slug}.runbook.v1`,
    companiesKey: current.companies_table_key || `campaign_${slug}_companies`,
    contactsKey: current.contacts_table_key || `campaign_${slug}_contacts`,
    runsKey: current.runs_table_key || `campaign_${slug}_research_runs`,
  };
}

function slugId(value) {
  const normalized = String(value || '')
    .toLowerCase()
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9_.-]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .slice(0, 80);
  return normalized || `campaign_${Date.now()}`;
}

async function knowledgeCommand(args) {
  if (!state.ctx?.commandBus?.dispatch) {
    throw new Error('RxDB command bus is not available');
  }
  const commandId = `cmd_knowledge_${crypto.randomUUID()}`;
  return state.ctx.commandBus.dispatch({
    id: commandId,
    module: 'knowledge',
    type: 'knowledge.command',
    record_id: commandId,
    inbound_channel: 'business_os.outbound',
    payload: {
      title: 'Knowledge command',
      args,
    },
    client_context: {
      source_module: 'outbound',
      command_path: 'knowledge',
    },
  });
}

async function ensureKnowledgeDataTable(refs, key, title, description) {
  const describeArgs = ['data', 'describe', '--domain', refs.domain, '--key', key];
  try {
    return await knowledgeCommand(describeArgs);
  } catch (_) {
    return knowledgeCommand([
      'data', 'create',
      '--domain', refs.domain,
      '--key', key,
      '--source-system', 'business-os.outbound',
      '--title', title,
      '--description', description,
    ]);
  }
}

async function ensureCampaignKnowledge(campaign) {
  if (!campaign?.id || state.ctx?.sync?.config?.http_bridge_available === false) return null;
  const refs = campaignKnowledgeRefs(campaign);
  await ensureKnowledgeDataTable(refs, refs.companiesKey, `${campaign.name} · Unternehmen`, 'Outbound Campaign Firmen, Importjobs, Qualifikation und Unternehmens-Research.');
  await ensureKnowledgeDataTable(refs, refs.contactsKey, `${campaign.name} · Ansprechpartner`, 'Outbound Campaign Ansprechpartner- und Lead-Qualifikation.');
  await ensureKnowledgeDataTable(refs, refs.runsKey, `${campaign.name} · Research Runs`, 'Outbound Campaign Research-Auftraege, Status und CTOX Command-Referenzen.');
  await knowledgeCommand([
    'skill', 'add-skillbook',
    '--id', refs.skillbookId,
    '--title', 'Business OS Outbound Campaigns',
    '--version', 'v1',
    '--mission', 'Outbound Campaigns fuehren Firmenquellen ueber Unternehmensqualifikation, Ansprechpartner-Recherche und Lead-Qualifikation.',
    '--runtime-policy', 'Nutze Knowledge DataFrames als einzige record-shaped Wissensquelle. Outbound speichert nur Workflow-State und Referenzen.',
    '--workflow-backbone', 'source-import,company-research,pipeline-contact-research,lead-qualification',
    '--linked-runbooks', refs.runbookId,
  ]);
  await knowledgeCommand([
    'skill', 'add-runbook',
    '--id', refs.runbookId,
    '--skillbook', refs.skillbookId,
    '--title', `${campaign.name} Campaign Runbook`,
    '--version', 'v1',
    '--problem-domain', 'outbound-campaign',
    '--status', 'active',
    '--item-labels', 'CAMPAIGN-SCOPE,DATAFRAMES,FUNNEL-RUNS',
  ]);
  await knowledgeCommand([
    'skill', 'add-item',
    '--id', `${refs.runbookId}.scope`,
    '--runbook', refs.runbookId,
    '--skillbook', refs.skillbookId,
    '--label', 'CAMPAIGN-SCOPE',
    '--title', 'Campaign Scope und Datenvertrag',
    '--problem-class', 'outbound-campaign-scope',
    '--chunk-text', campaignRunbookChunk(campaign, refs),
    '--version', 'v1',
    '--status', 'active',
    '--skip-embedding',
  ]);
  const payload = {
    ...(campaign.payload || {}),
    knowledge: {
      domain: refs.domain,
      skillbook_id: refs.skillbookId,
      runbook_id: refs.runbookId,
      companies_table_key: refs.companiesKey,
      contacts_table_key: refs.contactsKey,
      runs_table_key: refs.runsKey,
    },
  };
  await patchDoc(state.ctx.db.raw.outbound_campaigns, campaign.id, {
    payload,
    updated_at_ms: Date.now(),
  });
  return refs;
}

function campaignRunbookChunk(campaign, refs) {
  return [
    `Campaign: ${campaign.name}`,
    `Market: ${campaign.market || 'DACH'}`,
    `Scope/ICP: ${campaign.payload?.scope || campaign.objective || 'nicht gesetzt'}`,
    '',
    'Record-shaped Knowledge:',
    `- Companies DataFrame: ctox knowledge data describe --domain ${refs.domain} --key ${refs.companiesKey}`,
    `- Contacts DataFrame: ctox knowledge data describe --domain ${refs.domain} --key ${refs.contactsKey}`,
    `- Research Runs DataFrame: ctox knowledge data describe --domain ${refs.domain} --key ${refs.runsKey}`,
    '',
    'Funnel:',
    '1. Importjobs anlegen, daraus Unternehmen extrahieren und nur Unternehmen in den Companies DataFrame schreiben.',
    '2. Unternehmensdaten recherchieren, belegen und Firmen qualifizieren.',
    '3. Erst nach Unternehmensqualifikation Ansprechpartner im Contacts DataFrame recherchieren.',
    '4. Ansprechpartner gegen Scope/ICP qualifizieren und erst dann als Lead markieren.',
    '',
    'Grenze: Vor Pipeline-Stufe keine Personen recherchieren und keine Outreach-Nachrichten erzeugen.',
  ].join('\n');
}

async function appendKnowledgeRows(campaign, tableKey, rows) {
  if (!rows.length || state.ctx?.sync?.config?.http_bridge_available === false) return null;
  const refs = await ensureCampaignKnowledge(campaign);
  if (!refs) return null;
  for (let index = 0; index < rows.length; index += OUTBOUND_KNOWLEDGE_APPEND_CHUNK) {
    const chunk = rows.slice(index, index + OUTBOUND_KNOWLEDGE_APPEND_CHUNK);
    await knowledgeCommand([
      'data', 'append',
      '--domain', refs.domain,
      '--key', tableKey,
      '--rows', JSON.stringify(chunk),
    ]);
  }
  return true;
}

async function readKnowledgeRows(refs, tableKey, limit = 5000) {
  if (state.knowledgeProjectionDisabled || state.ctx?.sync?.config?.http_bridge_available === false) return [];
  try {
    const result = await knowledgeCommand([
      'data', 'select',
      '--domain', refs.domain,
      '--key', tableKey,
      '--limit', String(limit),
    ]);
    return Array.isArray(result.rows) ? result.rows : [];
  } catch (error) {
    const message = String(error?.message || error);
    if (message.includes('405') || message.includes('404') || message.includes('Failed to fetch')) {
      state.knowledgeProjectionDisabled = true;
    }
    console.warn('[outbound] knowledge projection unavailable; using local projections', error);
    return [];
  }
}

function openCampaignRunbook(campaignId) {
  const campaign = state.campaigns.find((item) => item.id === campaignId) || selectedCampaign();
  const runbookId = campaignKnowledgeRefs(campaign).runbookId;
  if (!runbookId) return;
  sessionStorage.setItem('ctox.businessOs.knowledge.openId', `runbook:${runbookId}`);
  location.hash = 'knowledge';
}

async function ensureDefaultCampaign() {
  const collection = state.ctx?.db?.raw?.outbound_campaigns;
  if (!collection) return;
  const existing = await collection.find().exec();
  if (existing.some((doc) => doc.id === DEFAULT_CAMPAIGN_ID)) return;
  const defaults = existing
    .map((doc) => doc.toJSON ? doc.toJSON() : doc)
    .filter((doc) => doc.name === DEFAULT_CAMPAIGN_NAME);
  if (defaults.length) return;
  const now = Date.now();
  const campaign = {
    id: DEFAULT_CAMPAIGN_ID,
    name: DEFAULT_CAMPAIGN_NAME,
    objective: 'Unternehmen importieren, qualifizieren und erst danach in die Ansprechpartner-Pipeline übergeben.',
    market: 'DACH',
    status: 'active',
    owner_id: state.ctx?.session?.user?.id || '',
    source_count: 0,
    company_count: 0,
    qualified_count: 0,
    pipeline_count: 0,
    payload: { outbound_only: true },
    created_at_ms: now,
    updated_at_ms: now,
  };
  await collection.insert(campaign);
  ensureCampaignKnowledge(campaign).catch((error) => {
    console.warn('[outbound] default campaign knowledge setup failed', error);
  });
}

async function loadAll(options = {}) {
  const raw = state.ctx?.db?.raw || {};
  const [campaigns, sources, companies, pipeline, runs] = await Promise.all([
    findAll(raw.outbound_campaigns),
    findAll(raw.outbound_sources),
    findAll(raw.outbound_companies),
    findAll(raw.outbound_pipeline_items),
    findAll(raw.outbound_research_runs),
  ]);
  state.campaigns = campaigns;
  state.sources = sources;
  state.companies = companies;
  state.pipeline = dedupePipelineItems(pipeline);
  state.runs = runs;
  refreshOperationalStateInBackground();
  if (!state.selectedCampaignId && state.campaigns[0]) state.selectedCampaignId = state.campaigns[0].id;
  const visible = visibleCampaigns();
  if (visible.length && !visible.some((campaign) => campaign.id === state.selectedCampaignId)) {
    state.selectedCampaignId = visible[0].id;
  }
  if (options.hydrateKnowledge === true) await hydrateSelectedCampaignFromKnowledge();
  if (!state.selectedCompanyId && currentCompanies()[0]) state.selectedCompanyId = currentCompanies()[0].id;
  if (!state.selectedPipelineId && currentPipeline()[0]) state.selectedPipelineId = currentPipeline()[0].id;
}

function refreshOperationalStateInBackground() {
  const raw = state.ctx?.db?.raw || {};
  if (!raw.business_commands || !raw.ctox_queue_tasks) return;
  const now = Date.now();
  if (state.operationalRefreshPending || now - state.lastOperationalRefreshMs < 10000) return;
  state.operationalRefreshPending = true;
  state.lastOperationalRefreshMs = now;
  withTimeout(Promise.all([
    findAll(raw.business_commands),
    findAll(raw.ctox_queue_tasks),
  ]), 2500, null)
    .then((result) => {
      if (!result) return;
      const [commands, queueTasks] = result;
      state.commands = commands;
      state.queueTasks = queueTasks;
      render();
    })
    .catch((error) => console.warn('[outbound] CTOX activity refresh skipped', error))
    .finally(() => {
      state.operationalRefreshPending = false;
    });
}

function withTimeout(promise, timeoutMs, fallback) {
  return Promise.race([
    promise,
    new Promise((resolve) => window.setTimeout(() => resolve(fallback), timeoutMs)),
  ]);
}

async function hydrateSelectedCampaignFromKnowledge() {
  const campaign = selectedCampaign();
  if (!campaign || state.knowledgeProjectionDisabled || !campaign.payload?.knowledge) return;
  const refs = campaignKnowledgeRefs(campaign);
  const [companyRows, contactRows, runRows] = await Promise.all([
    readKnowledgeRows(refs, refs.companiesKey),
    readKnowledgeRows(refs, refs.contactsKey),
    readKnowledgeRows(refs, refs.runsKey, 1000),
  ]);
  if (companyRows.length) {
    const projectedCompanies = projectCompaniesFromKnowledgeRows(campaign, companyRows);
    state.companies = mergeProjectionById(state.companies, projectedCompanies);
  }
  if (contactRows.length) {
    const projectedPipeline = projectPipelineFromKnowledgeRows(campaign, contactRows);
    state.pipeline = dedupePipelineItems(mergeProjectionById(state.pipeline, projectedPipeline));
  }
  if (runRows.length) {
    const projectedRuns = projectRunsFromKnowledgeRows(campaign, runRows);
    state.runs = mergeProjectionById(state.runs, projectedRuns);
  }
}

function projectCompaniesFromKnowledgeRows(campaign, rows) {
  const latest = latestKnowledgeRows(rows, (row) => companyIdentityKeyFromKnowledgeRow(campaign, row));
  return latest.map((row) => {
    const raw = parseJsonObject(row.raw_json || row.imported_row_json || '{}');
    const research = parseJsonObject(row.company_data_json || row.research_json || row.result_json || '{}');
    const evidence = parseJsonArray(row.evidence_json || research.evidence_json || research.evidence || []);
    const name = stringValue(row.company_name || row.name || raw.company || raw.name || raw.Company || raw.Firma);
    const website = stringValue(row.website || raw.website || raw.url || raw.URL || raw.Website);
    const domain = stringValue(row.domain || raw.domain || domainFromUrl(website));
    const now = Date.now();
    return {
      id: companyIdFromKnowledgeRow(campaign, row),
      campaign_id: stringValue(row.campaign_id || campaign.id),
      source_id: stringValue(row.source_id),
      row_index: Number(row.row_index || 0),
      name: name || domain || website || 'Unbenanntes Unternehmen',
      website,
      domain,
      city: stringValue(row.city || row.ort || row.location || raw.city || raw.Ort),
      country: stringValue(row.country || row.land || raw.country || raw.Land),
      qualification_status: stringValue(row.qualification_status || row.company_qualification_status || 'new'),
      research_status: stringValue(row.research_status || statusFromKnowledgeResult(row) || 'pending'),
      pipeline_status: stringValue(row.pipeline_status || 'not_started'),
      fit_score: Number(row.fit_score || row.company_fit_score || 0),
      fit_status: stringValue(row.fit_status || 'unqualified'),
      company_data: { ...raw, ...research },
      evidence,
      payload: {
        imported_row: raw,
        knowledge_row: row,
        knowledge_projection: true,
      },
      created_at_ms: Number(row.created_at_ms || row.imported_at_ms || row.updated_at_ms || now),
      updated_at_ms: Number(row.updated_at_ms || row.researched_at_ms || row.imported_at_ms || now),
    };
  });
}

function projectPipelineFromKnowledgeRows(campaign, rows) {
  const grouped = new Map();
  for (const row of rows) {
    const companyId = stringValue(row.company_id) || companyIdFromKnowledgeRow(campaign, row);
    const pipelineId = stringValue(row.pipeline_id || row.record_id) || `pipe_${fingerprint(`${campaign.id}:${companyId}`)}`;
    const group = grouped.get(pipelineId) || {
      rows: [],
      contacts: [],
      latest: row,
      pipelineId,
      companyId,
    };
    group.rows.push(row);
    group.latest = newerKnowledgeRow(group.latest, row);
    const contact = contactFromKnowledgeRow(row);
    if (contact) group.contacts.push(contact);
    grouped.set(pipelineId, group);
  }
  return Array.from(grouped.values()).map((group) => {
    const row = group.latest || {};
    const now = Date.now();
    return {
      id: group.pipelineId,
      campaign_id: stringValue(row.campaign_id || campaign.id),
      company_id: group.companyId,
      company_name: stringValue(row.company_name || row.name || 'Unternehmen'),
      stage: stringValue(row.stage || (isLeadKnowledgeRow(row) ? 'lead_qualified' : 'contact_research')),
      contact_research_status: stringValue(row.contact_research_status || (group.contacts.length ? 'researched' : 'pending')),
      outreach_status: stringValue(row.outreach_status || row.lead_status || (isLeadKnowledgeRow(row) ? 'qualified' : 'not_started')),
      priority: stringValue(row.priority || 'normal'),
      contacts: dedupeContacts(group.contacts),
      payload: {
        knowledge_rows: group.rows,
        knowledge_projection: true,
      },
      created_at_ms: Number(row.created_at_ms || row.imported_at_ms || now),
      updated_at_ms: Number(row.updated_at_ms || row.researched_at_ms || now),
    };
  });
}

function projectRunsFromKnowledgeRows(campaign, rows) {
  const latest = latestKnowledgeRows(rows, (row) => stringValue(row.run_id || row.command_id || row.record_id || fingerprint(JSON.stringify(row))));
  return latest.map((row) => ({
    id: stringValue(row.run_id || row.command_id || `run_${fingerprint(JSON.stringify(row))}`),
    campaign_id: stringValue(row.campaign_id || campaign.id),
    company_id: stringValue(row.company_id || row.record_id),
    pipeline_id: stringValue(row.pipeline_id),
    run_type: stringValue(row.run_type || 'research'),
    status: stringValue(row.status || row.ctox_status || 'queued'),
    command_id: stringValue(row.command_id),
    request: parseJsonObject(row.request_json || '{}'),
    result: parseJsonObject(row.result_json || '{}'),
    error: stringValue(row.error || ''),
    created_at_ms: Number(row.created_at_ms || Date.now()),
    updated_at_ms: Number(row.updated_at_ms || row.created_at_ms || Date.now()),
  }));
}

function latestKnowledgeRows(rows, idForRow) {
  const map = new Map();
  for (const row of rows) {
    const id = idForRow(row);
    if (!id) continue;
    const existing = map.get(id);
    map.set(id, existing ? newerKnowledgeRow(existing, row) : row);
  }
  return Array.from(map.values());
}

function newerKnowledgeRow(a, b) {
  const timeA = Number(a?.updated_at_ms || a?.researched_at_ms || a?.created_at_ms || a?.imported_at_ms || 0);
  const timeB = Number(b?.updated_at_ms || b?.researched_at_ms || b?.created_at_ms || b?.imported_at_ms || 0);
  return timeB >= timeA ? b : a;
}

function mergeProjectionById(localRows, projectedRows) {
  if (!projectedRows.length) return localRows;
  const map = new Map(localRows.map((row) => [row.id, row]));
  for (const projected of projectedRows) {
    const existing = map.get(projected.id);
    map.set(projected.id, existing ? mergeProjectedRow(existing, projected) : projected);
  }
  return Array.from(map.values()).sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0));
}

function dedupeCompanies(companies) {
  const map = new Map();
  for (const company of companies || []) {
    const key = companyIdentityKey(company);
    const current = map.get(key);
    map.set(key, current ? mergeDuplicateCompany(current, company) : { ...company, duplicate_company_ids: [company.id] });
  }
  return Array.from(map.values()).sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0));
}

function mergeDuplicateCompany(a, b) {
  const winner = companyQualityScore(b) > companyQualityScore(a) ? b : a;
  const other = winner === b ? a : b;
  const ids = new Set([...(a.duplicate_company_ids || [a.id]), ...(b.duplicate_company_ids || [b.id])].filter(Boolean));
  const researchStatus = mergedResearchStatus(a.research_status, b.research_status);
  const qualificationStatus = mergedQualificationStatus(a.qualification_status, b.qualification_status);
  return {
    ...other,
    ...winner,
    id: winner.id,
    source_id: winner.source_id || other.source_id || '',
    website: betterCompanyUrl(winner.website, other.website),
    domain: betterCompanyDomain(winner.domain, other.domain),
    city: winner.city || other.city || '',
    country: winner.country || other.country || '',
    qualification_status: qualificationStatus,
    research_status: researchStatus,
    pipeline_status: ['pipeline', 'sent', 'queued'].includes(String(a.pipeline_status || '').toLowerCase()) ? a.pipeline_status : b.pipeline_status || a.pipeline_status,
    company_data: { ...(other.company_data || {}), ...(winner.company_data || {}) },
    evidence: [...(other.evidence || []), ...(winner.evidence || [])],
    payload: {
      ...(other.payload || {}),
      ...(winner.payload || {}),
      duplicate_company_ids: Array.from(ids),
    },
    duplicate_company_ids: Array.from(ids),
    updated_at_ms: Math.max(Number(a.updated_at_ms || 0), Number(b.updated_at_ms || 0)),
  };
}

function companyQualityScore(company) {
  let score = 0;
  if (isCompanyResearchDone(company)) score += 100;
  if (String(company.research_status || '').toLowerCase() === 'queued') score += 25;
  if (String(company.qualification_status || '').toLowerCase() === 'qualified') score += 40;
  if (company.domain) score += isEventListDomain(company.domain) ? 1 : 15;
  if (company.website) score += isEventListDomain(domainFromUrl(company.website)) ? 1 : 8;
  if (company.city) score += 3;
  return score + Math.min(10, Number(company.updated_at_ms || 0) / 1000000000000);
}

function mergedResearchStatus(a, b) {
  const statuses = [a, b].map((value) => String(value || '').toLowerCase());
  if (statuses.some((status) => ['researched', 'completed', 'done'].includes(status))) return 'researched';
  if (statuses.some((status) => isQueuedStatus(status))) return 'queued';
  return a || b || 'pending';
}

function mergedQualificationStatus(a, b) {
  const statuses = [a, b].map((value) => String(value || '').toLowerCase());
  if (statuses.includes('qualified')) return 'qualified';
  if (statuses.includes('rejected')) return 'rejected';
  return a || b || 'new';
}

function betterCompanyDomain(a, b) {
  if (!a) return b || '';
  if (!b) return a || '';
  if (isEventListDomain(a) && !isEventListDomain(b)) return b;
  return a;
}

function betterCompanyUrl(a, b) {
  if (!a) return b || '';
  if (!b) return a || '';
  if (isEventListDomain(domainFromUrl(a)) && !isEventListDomain(domainFromUrl(b))) return b;
  return a;
}

function isEventListDomain(domain) {
  return ['intersolar.de', 'thesmartere.de', 'messe-muenchen.de'].includes(String(domain || '').toLowerCase().replace(/^www\./, ''));
}

function mergeProjectedRow(existing, projected) {
  const merged = {
    ...existing,
    ...projected,
    payload: { ...(existing.payload || {}), ...(projected.payload || {}) },
  };
  for (const field of ['research_status', 'contact_research_status', 'outreach_status', 'pipeline_status']) {
    if (isQueuedStatus(existing[field]) && !isDoneLikeStatus(projected[field])) merged[field] = existing[field];
  }
  if (isQueuedStatus(existing.research_status)
    || isQueuedStatus(existing.contact_research_status)
    || isQueuedStatus(existing.outreach_status)
    || isQueuedStatus(existing.pipeline_status)) {
    merged.updated_at_ms = Math.max(Number(existing.updated_at_ms || 0), Number(projected.updated_at_ms || 0));
  }
  return merged;
}

function companyIdFromKnowledgeRow(campaign, row) {
  const explicit = stringValue(row.company_id || row.record_id || row.id);
  if (explicit) return explicit;
  const identity = companyIdentityKeyFromKnowledgeRow(campaign, row);
  if (identity) return `co_${fingerprint(identity)}`;
  const name = stringValue(row.company_name || row.name);
  const locator = stringValue(row.domain || row.website || row.row_index || '');
  return `co_${fingerprint(`${campaign.id}:${name}:${locator}`)}`;
}

function companyIdentityKeyFromKnowledgeRow(campaign, row) {
  const raw = parseJsonObject(row.raw_json || row.imported_row_json || '{}');
  return companyIdentityKey({
    campaign_id: stringValue(row.campaign_id || campaign.id),
    name: stringValue(row.company_name || row.name || raw.company || raw.name || raw.Company || raw.Firma),
    domain: stringValue(row.domain || raw.domain),
    website: stringValue(row.website || raw.website || raw.url || raw.URL || raw.Website),
    country: stringValue(row.country || row.land || raw.country || raw.Land),
  });
}

function companyIdentityKey(company) {
  const campaignId = stringValue(company?.campaign_id || state.selectedCampaignId);
  const name = normalizeCompanyIdentityName(company?.name || company?.company_name || '');
  if (name && name.length >= 3) return `${campaignId}:name:${name}`;
  const domain = normalizeCompanyIdentityDomain(company?.domain || domainFromUrl(company?.website || ''));
  if (domain) return `${campaignId}:domain:${domain}`;
  return `${campaignId}:id:${company?.id || fingerprint(JSON.stringify(company || {}))}`;
}

function normalizeCompanyIdentityName(value) {
  return String(value || '')
    .toLowerCase()
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/&/g, 'und')
    .replace(/\b(gmbh|ag|kg|kgaa|ug|ohg|ev|e\\.v\\.|co|ltd|limited|inc|corp|corporation|llc)\b/g, '')
    .replace(/[^a-z0-9]+/g, '')
    .trim();
}

function normalizeCompanyIdentityDomain(value) {
  return String(value || '').toLowerCase().replace(/^https?:\/\//, '').replace(/^www\./, '').split('/')[0].trim();
}

function contactFromKnowledgeRow(row) {
  const name = stringValue(row.contact_name || row.person_name || row.name);
  const role = stringValue(row.role || row.title || row.position);
  const email = stringValue(row.email || row.e_mail);
  const linkedIn = stringValue(row.linkedin || row.linkedin_url);
  if (!name && !role && !email && !linkedIn) return null;
  return {
    id: stringValue(row.contact_id || row.person_id || `contact_${fingerprint(`${name}:${role}:${email}:${linkedIn}`)}`),
    name,
    role,
    email,
    linkedin_url: linkedIn,
    qualification_status: stringValue(row.contact_qualification_status || row.qualification_status || ''),
    evidence: parseJsonArray(row.evidence_json || row.evidence || []),
  };
}

function dedupeContacts(contacts) {
  const map = new Map();
  for (const contact of contacts) {
    const id = contact.id || fingerprint(JSON.stringify(contact));
    map.set(id, { ...(map.get(id) || {}), ...contact });
  }
  return Array.from(map.values());
}

function dedupePipelineItems(items) {
  const map = new Map();
  for (const item of items || []) {
    const id = stringValue(item.company_id || item.id);
    if (!id) continue;
    const existing = map.get(id);
    map.set(id, existing ? newerKnowledgeRow(existing, item) : item);
  }
  return Array.from(map.values());
}

function isLeadKnowledgeRow(row) {
  return ['qualified', 'lead_qualified', 'ready'].includes(String(row.lead_status || row.outreach_status || row.stage || '').toLowerCase());
}

function statusFromKnowledgeResult(row) {
  if (row.researched_at_ms || row.company_data_json || row.research_json || row.result_json) return 'researched';
  return '';
}

function parseJsonObject(value) {
  if (!value) return {};
  if (typeof value === 'object' && !Array.isArray(value)) return value;
  try {
    const parsed = JSON.parse(String(value));
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? parsed : {};
  } catch {
    return {};
  }
}

function parseJsonArray(value) {
  if (!value) return [];
  if (Array.isArray(value)) return value;
  try {
    const parsed = JSON.parse(String(value));
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

function stringValue(value) {
  if (value == null) return '';
  return String(value).trim();
}

function wireRealtime() {
  const raw = state.ctx?.db?.raw || {};
  const collections = [
    raw.outbound_campaigns,
    raw.outbound_sources,
    raw.outbound_companies,
    raw.outbound_pipeline_items,
    raw.outbound_research_runs,
    raw.business_commands,
    raw.ctox_queue_tasks,
  ].filter(Boolean);
  for (const collection of collections) {
    const subscription = collection.$?.subscribe?.(() => scheduleDataRefresh(20));
    if (subscription?.unsubscribe) state.cleanup.push(() => subscription.unsubscribe());
  }
  startKnowledgeProjectionWatch();
}

function scheduleDataRefresh(delay = 80) {
  if (state.refreshTimer) window.clearTimeout(state.refreshTimer);
  state.refreshTimer = window.setTimeout(async () => {
    state.refreshTimer = null;
    await loadAll({ hydrateKnowledge: false });
    render();
  }, delay);
}

function startKnowledgeProjectionWatch() {
  if (state.knowledgeWatchTimer) window.clearInterval(state.knowledgeWatchTimer);
  const tick = async () => {
    const changed = await refreshKnowledgeProjectionIfChanged();
    if (changed) render();
  };
  state.knowledgeWatchTimer = window.setInterval(tick, 2500);
  state.cleanup.push(() => {
    if (state.knowledgeWatchTimer) window.clearInterval(state.knowledgeWatchTimer);
    state.knowledgeWatchTimer = null;
    if (state.refreshTimer) window.clearTimeout(state.refreshTimer);
    state.refreshTimer = null;
  });
}

async function refreshKnowledgeProjectionIfChanged() {
  const campaign = selectedCampaign();
  if (!campaign || state.knowledgeProjectionDisabled || !campaign.payload?.knowledge) return false;
  const refs = campaignKnowledgeRefs(campaign);
  try {
    const [companyRows, contactRows, runRows] = await Promise.all([
      readKnowledgeRows(refs, refs.companiesKey),
      readKnowledgeRows(refs, refs.contactsKey),
      readKnowledgeRows(refs, refs.runsKey, 1000),
    ]);
    const signature = knowledgeRowsSignature(campaign.id, companyRows, contactRows, runRows);
    if (signature === state.knowledgeProjectionSignature) return false;
    state.knowledgeProjectionSignature = signature;
    if (companyRows.length) state.companies = mergeProjectionById(state.companies, projectCompaniesFromKnowledgeRows(campaign, companyRows));
    if (contactRows.length) state.pipeline = dedupePipelineItems(mergeProjectionById(state.pipeline, projectPipelineFromKnowledgeRows(campaign, contactRows)));
    if (runRows.length) state.runs = mergeProjectionById(state.runs, projectRunsFromKnowledgeRows(campaign, runRows));
    return true;
  } catch (error) {
    console.warn('[outbound] knowledge projection watch failed', error);
    return false;
  }
}

function knowledgeRowsSignature(campaignId, companyRows, contactRows, runRows) {
  const compact = (rows, idFn) => rows.map((row) => [
    idFn(row),
    row.updated_at_ms || row.researched_at_ms || row.imported_at_ms || row.created_at_ms || '',
    row.company_name || row.name || row.contact_name || row.run_type || '',
    row.research_status || row.qualification_status || row.contact_research_status || row.lead_status || row.status || '',
  ].join(':')).sort().join('|');
  return [
    campaignId,
    compact(companyRows, (row) => row.company_id || companyIdFromKnowledgeRow({ id: campaignId }, row)),
    compact(contactRows, (row) => row.pipeline_id || row.contact_id || row.company_id || ''),
    compact(runRows, (row) => row.run_id || row.command_id || row.company_id || ''),
  ].join('::');
}

function wireEvents(root) {
  root.addEventListener('click', async (event) => {
    const action = event.target.closest('[data-action]')?.dataset.action;
    const view = event.target.closest('[data-view]')?.dataset.view;
    const filter = event.target.closest('[data-filter]')?.dataset.filter;
    if (!action && !view && !filter) return;
    const id = event.target.closest('[data-id]')?.dataset.id || '';
    if (action === 'select-campaign') {
      state.selectedCampaignId = id;
      state.selectedCompanyId = currentCompanies()[0]?.id || '';
      render();
    }
    if (action === 'new-campaign') await createCampaign();
    if (action === 'import-source') {
      if (id) state.selectedCampaignId = id;
      await openCompanyImporter();
    }
    if (action === 'open-campaign-runbook') openCampaignRunbook(id || state.selectedCampaignId);
    if (action === 'edit-campaign') {
      state.editingCampaignId = id || state.selectedCampaignId;
      renderLeft();
    }
    if (action === 'cancel-campaign-edit') {
      state.editingCampaignId = '';
      renderLeft();
    }
    if (action === 'save-campaign-edit') await saveCampaignInlineEdit(id || state.selectedCampaignId);
    if (action === 'delete-campaign') await deleteCampaign(id || state.selectedCampaignId);
    if (action === 'select-company') {
      state.selectedCompanyId = id;
      render();
    }
    if (action === 'research-company') await queueCompanyResearch(id || state.selectedCompanyId);
    if (action === 'qualify-company') await setCompanyQualification(id || state.selectedCompanyId, 'qualified');
    if (action === 'reject-company') await setCompanyQualification(id || state.selectedCompanyId, 'rejected');
    if (action === 'send-pipeline') await sendCompanyToPipeline(id || state.selectedCompanyId);
    if (action === 'open-automation') openAutomationDrawer(event.target.closest('[data-stage]')?.dataset.stage, event.target.closest('[data-campaign-id]')?.dataset.campaignId || state.selectedCampaignId);
    if (action === 'close-automation') closeAutomationDrawer();
    if (action === 'start-automation') await startAutomationBatch();
    if (action === 'select-pipeline') {
      state.selectedPipelineId = id;
      render();
    }
    if (action === 'research-contacts') await queueContactResearch(id || state.selectedPipelineId);
    if (action === 'open-research-settings') openResearchSettingsDrawer();
    if (action === 'export-table') exportQualificationTable();
    if (action === 'sort-table') {
      const column = event.target.closest('[data-column]')?.dataset.column;
      if (column) {
        setTableSort(column);
        renderCenter();
      }
    }
    if (action === 'clear-table-filters') {
      state.tableFilters = {};
      state.tableSort = null;
      renderCenter();
    }
    if (action === 'close-research-settings') closeResearchSettingsDrawer();
    if (action === 'save-research-settings') await saveResearchSettings();
    if (action === 'research-settings-all') setResearchSettingsSelection(true);
    if (action === 'research-settings-core') setResearchSettingsCoreSelection();
    if (action === 'research-settings-add-field') addResearchSettingsField();
    if (action === 'research-settings-add-contact-field') addResearchSettingsField('contact');
    if (action === 'research-settings-remove-field') removeResearchSettingsField(event.target.closest('[data-custom-field-id]')?.dataset.customFieldId);
    if (action === 'research-settings-remove-contact-field') removeResearchSettingsField(event.target.closest('[data-custom-field-id]')?.dataset.customFieldId, 'contact');
    if (view) {
      state.activeView = view;
      render();
    }
    if (filter) {
      state.filter = filter;
      const campaignId = event.target.closest('[data-campaign-id]')?.dataset.campaignId;
      if (campaignId) state.selectedCampaignId = campaignId;
      ensureSelectedCompanyInFilter();
      render();
    }
  });
  root.addEventListener('input', (event) => {
    if (event.target.matches('[data-search]')) {
      state.search = event.target.value;
      scheduleCenterRenderPreservingInput(event.target);
    }
    if (event.target.matches('[data-table-filter]')) {
      const column = event.target.dataset.tableFilter;
      if (!column) return;
      const value = event.target.value.trim();
      if (value) state.tableFilters[column] = value;
      else delete state.tableFilters[column];
      ensureSelectedCompanyInFilter();
      scheduleCenterRenderPreservingInput(event.target);
    }
  });
}

function render() {
  renderLeft();
  renderCenter();
  renderRight();
}

function renderLeft() {
  const root = state.ctx.host.querySelector('.outbound-left');
  const campaigns = visibleCampaigns();
  root.innerHTML = `
    <header class="outbound-pane-header">
      <div><span>Outbound</span><h2>Campaigns</h2></div>
      <div class="outbound-actions">
        <button class="outbound-icon-button" type="button" data-action="new-campaign" title="Neue Campaign" aria-label="Neue Campaign">+</button>
      </div>
    </header>
    <div class="outbound-scroll">
      <section class="outbound-section" aria-label="Campaigns">
        ${campaigns.map(renderCampaignItem).join('') || '<div class="outbound-empty">Keine Campaigns vorhanden.</div>'}
      </section>
    </div>
  `;
}

function renderCampaignItem(campaign) {
  if (state.editingCampaignId === campaign.id) return renderCampaignEditItem(campaign);
  const metrics = campaignFunnelMetrics(campaign.id);
  const subtitle = campaign.payload?.subtitle || `${campaign.market || 'DACH'} · ${campaign.status || 'active'}`;
  const scope = campaign.payload?.scope || campaign.objective || '';
  const knowledge = campaignKnowledgeRefs(campaign);
  return `
    <article class="outbound-campaign-item" aria-current="${campaign.id === state.selectedCampaignId}">
      <div class="outbound-campaign-top">
        <button class="outbound-campaign-select" type="button" data-action="select-campaign" data-id="${escapeHtml(campaign.id)}">
          <strong>${escapeHtml(campaign.name)}</strong>
          <span>${escapeHtml(subtitle)}</span>
          ${scope ? `<em>${escapeHtml(scope)}</em>` : ''}
        </button>
        <div class="outbound-shard-actions" aria-label="Campaign Aktionen">
          <button type="button" data-icon="runbook" data-action="open-campaign-runbook" data-id="${escapeHtml(campaign.id)}" title="Campaign Runbook öffnen" aria-label="Campaign Runbook öffnen" ${knowledge.runbookId ? '' : 'disabled'}></button>
          <button class="is-primary-action" type="button" data-icon="import" data-action="import-source" data-id="${escapeHtml(campaign.id)}" title="Importjob anlegen" aria-label="Importjob anlegen"><b>Import</b></button>
          <button type="button" data-icon="edit" data-action="edit-campaign" data-id="${escapeHtml(campaign.id)}" title="Campaign bearbeiten" aria-label="Campaign bearbeiten"></button>
          <button type="button" data-icon="delete" data-action="delete-campaign" data-id="${escapeHtml(campaign.id)}" title="Campaign löschen" aria-label="Campaign löschen"></button>
        </div>
      </div>
      <div class="outbound-funnel" aria-label="Campaign Funnel">
        ${renderInputFunnelStage(campaign, metrics)}
        ${renderConversion(metrics.input, metrics.companyResearchDone, campaign, 'company_research')}
        ${renderFunnelStage(campaign, 'research', 'Research', 'offen', `${metrics.companyResearchDone} / ${metrics.companyResearchTotal}`, metrics.companyResearchOpen)}
        ${renderConversion(metrics.companyResearchDone, metrics.companyQualified, campaign, 'pipeline')}
        ${renderFunnelStage(campaign, 'qualified', 'Firmen', 'qualifiziert', `${metrics.companyQualified} / ${metrics.companyQualifiedTotal}`, metrics.pipelineOpen, 'good')}
        ${renderConversion(metrics.companyQualified, metrics.contactQualified, campaign, 'contact_research')}
        ${renderFunnelStage(campaign, 'contact_qualified', 'Kontakte', 'qualifiziert', `${metrics.contactQualified} / ${metrics.contactQualifiedTotal}`, metrics.contactOpen)}
        ${renderConversion(metrics.contactQualified, metrics.leadQualified, campaign, 'lead_qualification')}
        ${renderFunnelStage(campaign, 'lead_qualified', 'Leads', 'qualifiziert', `${metrics.leadQualified} / ${metrics.leadQualifiedTotal}`, metrics.leadOpen)}
      </div>
    </article>
  `;
}

function renderInputFunnelStage(campaign, metrics) {
  const active = campaign.id === state.selectedCampaignId && state.filter === 'all';
  const progress = metrics.sourceCount ? Math.round((metrics.parsedSourceCount / metrics.sourceCount) * 100) : 0;
  const status = inputImportStatusLabel(metrics);
  const runningClass = metrics.runningSourceCount ? ' is-running' : '';
  return `
    <button
      class="outbound-funnel-stage outbound-funnel-input${runningClass}"
      type="button"
      data-filter="all"
      data-campaign-id="${escapeHtml(campaign.id)}"
      aria-pressed="${active}"
      title="Input Unternehmen filtern"
    >
      <span><b>Input</b><small>Unternehmen</small></span>
      <i>${formatCount(metrics.input)}</i>
      <em>${escapeHtml(status)}</em>
      <span class="outbound-funnel-progress" aria-hidden="true">
        <span style="width:${Math.max(0, Math.min(100, progress))}%"></span>
      </span>
    </button>
  `;
}

function inputImportStatusLabel(metrics) {
  if (!metrics.sourceCount) return 'Noch kein Import';
  const parts = [`${formatCount(metrics.parsedSourceCount)} / ${formatCount(metrics.sourceCount)} verarbeitet`];
  if (metrics.runningSourceCount) parts.push(`${formatCount(metrics.runningSourceCount)} läuft`);
  if (metrics.failedSourceCount) parts.push(`${formatCount(metrics.failedSourceCount)} Fehler`);
  return parts.join(' · ');
}

function renderCampaignEditItem(campaign) {
  return `
    <article class="outbound-campaign-item outbound-campaign-edit" aria-current="${campaign.id === state.selectedCampaignId}" data-id="${escapeHtml(campaign.id)}">
      <div class="outbound-campaign-edit-grid">
        <label>
          <span>Titel</span>
          <input data-campaign-edit-field="name" value="${escapeHtml(campaign.name)}" />
        </label>
        <label>
          <span>Untertitel</span>
          <input data-campaign-edit-field="subtitle" value="${escapeHtml(campaign.payload?.subtitle || `${campaign.market || 'DACH'} · ${campaign.status || 'active'}`)}" />
        </label>
        <label>
          <span>Scope / ICP</span>
          <textarea data-campaign-edit-field="scope" rows="3" placeholder="z.B. DACH SaaS, 50-500 MA, hoher Energieverbrauch, kaufkräftige Operations-Teams">${escapeHtml(campaign.payload?.scope || campaign.objective || '')}</textarea>
        </label>
      </div>
      <div class="outbound-campaign-edit-actions">
        <button class="outbound-button" type="button" data-action="cancel-campaign-edit" data-id="${escapeHtml(campaign.id)}">Abbrechen</button>
        <button class="outbound-button primary" type="button" data-action="save-campaign-edit" data-id="${escapeHtml(campaign.id)}">Speichern</button>
      </div>
    </article>
  `;
}

function renderFunnelStage(campaign, filter, label, sublabel, value, openCount = 0, tone = '') {
  const active = campaign.id === state.selectedCampaignId && state.filter === filter;
  return `
    <button
      class="outbound-funnel-stage ${tone}"
      type="button"
      data-filter="${escapeHtml(filter)}"
      data-campaign-id="${escapeHtml(campaign.id)}"
      aria-pressed="${active}"
      title="${escapeHtml(label)} ${escapeHtml(sublabel || '')} filtern"
    >
      <span><b>${escapeHtml(label)}</b>${sublabel ? `<small>${escapeHtml(sublabel)}</small>` : ''}</span>
      <i>${typeof value === 'number' ? formatCount(value) : escapeHtml(value)}</i>
      ${openCount ? `<em>${formatCount(openCount)} offen</em>` : ''}
    </button>
  `;
}

function renderConversion(from, to, campaign, stage) {
  const openCount = automationOpenRecords(campaign.id, stage).length;
  return `
    <div class="outbound-conversion">
      <span>${conversionRate(from, to)}</span>
      <button
        type="button"
        data-action="open-automation"
        data-stage="${escapeHtml(stage)}"
        data-campaign-id="${escapeHtml(campaign.id)}"
        title="${escapeHtml(`${AUTOMATION_STAGES[stage]?.cta || 'Automatisierung starten'} · ${formatCount(openCount)} offen`)}"
        aria-label="${escapeHtml(`${AUTOMATION_STAGES[stage]?.cta || 'Automatisierung starten'} · ${formatCount(openCount)} offen`)}"
        ${openCount ? '' : 'disabled'}
      >${openCount ? '<span aria-hidden="true">▶</span>' : ''}<b>${escapeHtml(automationShortLabel(stage, openCount))}</b></button>
    </div>
  `;
}

function currentFilterLabel() {
  return ({
    all: 'Input Unternehmen',
    research: 'Research offen',
    qualified: 'Unternehmen qualifiziert',
    contact_qualified: 'Kontakt qualifiziert',
    lead_qualified: 'Lead qualifiziert',
    pipeline: 'Pipeline',
    rejected: 'Nicht passend',
  })[state.filter] || 'Alle';
}

function automationShortLabel(stage, openCount) {
  if (!openCount) return 'fertig';
  return ({
    company_research: 'Research',
    pipeline: 'Pipeline',
    contact_research: 'Kontakte',
    lead_qualification: 'Leads',
  })[stage] || 'Start';
}

function formatCount(value) {
  const number = Number(value || 0);
  if (number >= 1000000) return number.toLocaleString('de-DE', { notation: 'compact', maximumFractionDigits: 1 });
  return number.toLocaleString('de-DE');
}

function renderSourceItem(source) {
  return `
    <div class="outbound-source-item">
      <strong>${escapeHtml(source.title)}</strong>
      <span>${escapeHtml(source.source_type)} · ${escapeHtml(source.status)} · ${source.imported_count || 0}/${source.row_count || 0} Firmen</span>
    </div>
  `;
}

function renderCenter() {
  const root = state.ctx.host.querySelector('.outbound-center');
  state.centerResizeCleanup?.();
  state.centerResizeCleanup = null;
  const campaign = selectedCampaign();
  if (!campaign) {
    root.innerHTML = '<div class="outbound-empty">Keine Campaign ausgewählt.</div>';
    return;
  }
  const settings = getCampaignResearchSettings(campaign);
  root.innerHTML = `
    <header class="outbound-pane-header">
      <div><span>${escapeHtml(campaign.market || 'DACH')}</span><h2>${escapeHtml(campaign.name)}</h2></div>
      <div class="outbound-header-actions">
        ${renderActiveResearchSummary(campaign)}
        <button
          class="outbound-icon-button outbound-icon-mask"
          type="button"
          data-icon="export"
          data-action="export-table"
          title="Tabelle als Excel exportieren"
          aria-label="Tabelle als Excel exportieren"
        ></button>
        <button
          class="outbound-icon-button outbound-icon-mask"
          type="button"
          data-icon="settings"
          data-action="open-research-settings"
          title="Research-Felder einstellen"
          aria-label="Research-Felder einstellen"
        ></button>
        <div class="outbound-muted">${currentSources().length} Importjobs · ${currentCompanies().length} Firmen · ${settings.fields.length + settings.contactFields.length} Spalten</div>
      </div>
    </header>
    ${renderResearchActivityPanel(campaign)}
    ${renderQualificationSplit(campaign)}
  `;
  setupCenterSplitResizing(root);
}

function renderQualificationSplit(campaign) {
  const rows = filteredQualificationRows();
  const visibleRows = rows.slice(0, OUTBOUND_TABLE_RENDER_LIMIT);
  const pipelineItems = currentPipeline();
  const settings = getCampaignResearchSettings(campaign);
  const visibleFields = researchFieldsForPrompt(settings)
    .filter((field) => field.id !== 'domain');
  const companyColumns = companyTableColumns(visibleFields);
  const contactColumns = contactTableColumns(settings);
  const emptyCompanyMessage = state.filter === 'all'
    ? 'Noch keine Unternehmen in dieser Campaign.'
    : `Keine Unternehmen für Filter: ${currentFilterLabel()}.`;
  return `
    <div class="outbound-split-workbench" data-outbound-center-split>
      <section class="outbound-table-pane outbound-company-table-pane" aria-label="Unternehmens-Stammdaten">
        <div class="outbound-table-head">
          <div>
            <span>Stammdaten</span>
            <strong>Unternehmen</strong>
          </div>
          <div class="outbound-table-tools">
            <input class="outbound-search" data-search placeholder="Firma suchen" value="${escapeHtml(state.search)}" />
            ${hasTableControls() ? `<button class="outbound-button" type="button" data-action="clear-table-filters" title="Tabellenfilter und Sortierung zurücksetzen">Reset</button>` : ''}
          </div>
        </div>
        <div class="outbound-table-scroll">
          <table class="outbound-data-table">
            <thead>
              <tr>${companyColumns.map(renderTableHeaderCell).join('')}</tr>
            </thead>
            <tbody>
              ${visibleRows.map((row) => renderCompanyDataRow(row, companyColumns)).join('') || `
                <tr><td colspan="${companyColumns.length}" class="outbound-table-empty">${escapeHtml(emptyCompanyMessage)}</td></tr>
              `}
              ${renderTableLimitRow(rows.length, companyColumns.length)}
            </tbody>
          </table>
        </div>
      </section>
      <div class="outbound-center-resizer" role="separator" aria-orientation="vertical" aria-label="Tabellenbreite anpassen" data-outbound-center-resizer></div>
      <section class="outbound-table-pane outbound-contact-table-pane" aria-label="Ansprechpartner-Qualifizierung">
        <div class="outbound-table-head">
          <div>
            <span>Pipeline</span>
            <strong>Ansprechpartner Qualifizierung</strong>
          </div>
          <div class="outbound-muted">${pipelineItems.length} Pipeline</div>
        </div>
        <div class="outbound-table-scroll">
          <table class="outbound-data-table">
            <thead>
              <tr>${contactColumns.map(renderTableHeaderCell).join('')}</tr>
            </thead>
            <tbody>
              ${visibleRows.map((row) => renderContactQualificationRow(row, contactColumns)).join('') || `
                <tr><td colspan="${contactColumns.length}" class="outbound-table-empty">Erst Unternehmen importieren und qualifizieren.</td></tr>
              `}
              ${renderTableLimitRow(rows.length, contactColumns.length)}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  `;
}

function renderActiveResearchSummary(campaign) {
  const counts = researchActivityCounts(campaign.id);
  const active = [
    ['läuft', counts.running],
    ['wartet', counts.waiting],
    ['Fehler', counts.failed],
    ['abgebrochen', counts.cancelled],
  ].filter(([, count]) => count > 0);
  if (!active.length) return '';
  const tone = counts.failed || counts.cancelled ? 'warn' : counts.running ? 'running' : 'waiting';
  return `
    <div class="outbound-active-research ${tone}" title="CTOX Research Status">
      <span></span>
      <b>${active.map(([label, count]) => `${formatCount(count)} ${label}`).join(' · ')}</b>
    </div>
  `;
}

function renderResearchActivityPanel(campaign) {
  const activity = researchActivityRows(campaign.id);
  const counts = researchActivityCounts(campaign.id, activity);
  if (!counts.total) return '';
  const isProblem = counts.failed || counts.cancelled;
  const title = counts.running
    ? `CTOX arbeitet: ${formatCount(counts.running)} läuft${counts.waiting ? ` · ${formatCount(counts.waiting)} wartet` : ''}${isProblem ? ` · ${formatCount(counts.failed + counts.cancelled)} Fehler` : ''}`
    : `CTOX arbeitet gerade nicht: ${formatCount(counts.waiting)} wartet${isProblem ? ` · ${formatCount(counts.failed + counts.cancelled)} Fehler` : ''}`;
  const items = activity
    .filter((item) => item.kind !== 'done' && item.kind !== 'idle')
    .slice(0, 5);
  return `
    <section class="outbound-research-activity ${isProblem ? 'warn' : counts.running ? 'running' : 'waiting'}" aria-label="CTOX Research Aktivität">
      <div>
        <span>CTOX Research</span>
        <strong>${escapeHtml(title)}</strong>
      </div>
      <div class="outbound-research-activity-counts">
        ${renderActivityCount('läuft', counts.running)}
        ${renderActivityCount('wartet', counts.waiting)}
        ${renderActivityCount('Fehler', counts.failed, counts.failed ? 'warn' : '')}
        ${renderActivityCount('abgebrochen', counts.cancelled, counts.cancelled ? 'warn' : '')}
      </div>
      ${items.length ? `
        <ol>
          ${items.map((item) => `
            <li class="${escapeHtml(item.kind)}">
              <b>${escapeHtml(item.label)}</b>
              <span>${escapeHtml(item.statusLabel)}</span>
            </li>
          `).join('')}
        </ol>
      ` : ''}
    </section>
  `;
}

function renderActivityCount(label, count, tone = '') {
  return `<span class="${escapeHtml(tone)}"><b>${formatCount(count)}</b> ${escapeHtml(label)}</span>`;
}

function researchActivityCounts(campaignId, rows = researchActivityRows(campaignId)) {
  return rows.reduce((counts, item) => {
    if (item.kind === 'running') counts.running += 1;
    else if (item.kind === 'waiting') counts.waiting += 1;
    else if (item.kind === 'failed' || item.kind === 'blocked') counts.failed += 1;
    else if (item.kind === 'cancelled') counts.cancelled += 1;
    counts.total += item.kind === 'idle' || item.kind === 'done' ? 0 : 1;
    return counts;
  }, { running: 0, waiting: 0, failed: 0, cancelled: 0, total: 0 });
}

function researchActivityRows(campaignId) {
  const companies = dedupeCompanies(state.companies.filter((item) => item.campaign_id === campaignId));
  const pipeline = dedupePipelineItems(state.pipeline.filter((item) => item.campaign_id === campaignId));
  const rows = [
    ...companies.map((company) => activityRowFromStatus(company.name, 'Company', companyResearchStatus(company))),
    ...pipeline.map((item) => activityRowFromStatus(item.company_name, 'Kontakt', pipelineResearchStatus(item, 'contact_research'))),
    ...pipeline.map((item) => activityRowFromStatus(item.company_name, 'Lead', pipelineResearchStatus(item, 'lead_qualification'))),
  ];
  const priority = { running: 0, failed: 1, blocked: 1, cancelled: 2, waiting: 3, idle: 4, done: 5 };
  return rows.sort((a, b) => (priority[a.kind] ?? 9) - (priority[b.kind] ?? 9));
}

function activityRowFromStatus(name, stageLabel, status) {
  const kind = automationStatusKind(status);
  return {
    label: `${stageLabel}: ${name || 'Datensatz'}`,
    kind,
    statusLabel: automationStatusLabel(status),
  };
}

function renderTableLimitRow(total, columnCount) {
  if (total <= OUTBOUND_TABLE_RENDER_LIMIT) return '';
  return `
    <tr>
      <td colspan="${columnCount}" class="outbound-table-empty">
        ${escapeHtml(`${OUTBOUND_TABLE_RENDER_LIMIT} von ${formatCount(total)} Zeilen sichtbar. Suche oder Filter eingrenzen.`)}
      </td>
    </tr>
  `;
}

function companyTableColumns(fields) {
  return [
    { id: 'company.name', label: 'Unternehmen', value: (row) => row.company.name, primary: true },
    { id: 'company.domain', label: 'Domain', value: (row) => row.company.domain || domainFromUrl(row.company.website) || '' },
    ...fields.map((field) => ({
      id: `field.${field.id}`,
      label: field.label,
      value: (row) => companyResearchValue(row.company, field.id),
    })),
    { id: 'company.qualification', label: 'Qualifizierung', value: (row) => labelQualification(row.company.qualification_status), badge: true },
  ];
}

function contactTableColumns(settings = getCampaignResearchSettings(selectedCampaign())) {
  const fields = contactFieldsForPrompt(settings);
  return [
    ...fields.map((field) => ({
      id: field.id,
      label: field.label,
      value: (row) => contactColumnValue(row.item, field.id),
      primary: field.id === 'contact.people',
    })),
    { id: 'contact.action', label: 'Aktion', value: () => '', action: true },
  ];
}

function renderTableHeaderCell(column) {
  const sorted = state.tableSort?.column === column.id ? state.tableSort.direction : '';
  const filterValue = state.tableFilters[column.id] || '';
  const sortLabel = sorted === 'asc' ? 'Aufsteigend sortiert' : sorted === 'desc' ? 'Absteigend sortiert' : 'Sortieren';
  return `
    <th>
      <div class="outbound-table-column-head">
        <button
          type="button"
          data-action="sort-table"
          data-column="${escapeHtml(column.id)}"
          aria-label="${escapeHtml(`${column.label} ${sortLabel}`)}"
          aria-sort="${sorted === 'asc' ? 'ascending' : sorted === 'desc' ? 'descending' : 'none'}"
          ${column.action ? 'disabled' : ''}
        >
          <span>${escapeHtml(column.label)}</span>
          ${column.action ? '' : `<i>${sorted === 'asc' ? '↑' : sorted === 'desc' ? '↓' : '↕'}</i>`}
        </button>
        ${column.action ? '<span class="outbound-table-filter-placeholder"></span>' : `
          <input
            data-table-filter="${escapeHtml(column.id)}"
            value="${escapeHtml(filterValue)}"
            placeholder="Filtern"
            aria-label="${escapeHtml(`${column.label} filtern`)}"
          />
        `}
      </div>
    </th>
  `;
}

function renderCompanyDataRow(row, columns) {
  const company = row.company;
  const status = companyResearchStatus(company);
  const rowClass = tableRowStatusClass(status);
  return `
    <tr${rowClass} data-action="select-company" data-id="${escapeHtml(company.id)}" aria-current="${company.id === state.selectedCompanyId}">
      ${columns.map((column) => {
        if (column.primary) {
          return `<td><strong>${escapeHtml(column.value(row) || '-')}</strong>${renderInlineResearchStatus(labelResearch(status), status)}</td>`;
        }
        if (column.badge) {
          return `<td><span class="outbound-badge ${company.qualification_status === 'qualified' ? 'good' : company.qualification_status === 'rejected' ? 'warn' : ''}">${escapeHtml(column.value(row) || '-')}</span></td>`;
        }
        return `<td>${escapeHtml(column.value(row) || '-')}</td>`;
      }).join('')}
    </tr>
  `;
}

function renderContactQualificationRow(row, columns) {
  const { company, item } = row;
  const contactStatus = pipelineResearchStatus(item, 'contact_research');
  const leadStatus = pipelineResearchStatus(item, 'lead_qualification');
  const rowClass = tableRowStatusClass(combinedAutomationStatus(contactStatus, leadStatus));
  const action = item
    ? `<button class="outbound-button" type="button" data-action="research-contacts" data-id="${escapeHtml(item.id)}">Ansprechpartner</button>`
    : `<button class="outbound-button" type="button" data-action="send-pipeline" data-id="${escapeHtml(company.id)}">Pipeline</button>`;
  return `
    <tr${rowClass} data-action="${item ? 'select-pipeline' : 'select-company'}" data-id="${escapeHtml(item?.id || company.id)}" aria-current="${company.id === state.selectedCompanyId || item?.id === state.selectedPipelineId}">
      ${columns.map((column) => {
        if (column.action) return `<td>${action}</td>`;
        if (column.primary) {
          return `<td><strong>${escapeHtml(column.value(row) || '-')}</strong>${renderInlineResearchStatus(item?.stage || labelPipeline(company.pipeline_status), combinedAutomationStatus(contactStatus, leadStatus))}</td>`;
        }
        if (column.id === 'contact.status') return `<td><span class="outbound-badge ${badgeClassForStatus(contactStatus, isContactQualified(item))}">${escapeHtml(column.value(row))}</span></td>`;
        if (column.id === 'lead.status') return `<td><span class="outbound-badge ${badgeClassForStatus(leadStatus, isLeadQualified(item))}">${escapeHtml(column.value(row))}</span></td>`;
        return `<td>${escapeHtml(column.value(row) || '-')}</td>`;
      }).join('')}
    </tr>
  `;
}

function tableRowStatusClass(status) {
  if (isAutomationActive(status)) return ' class="is-updating"';
  if (isFailedStatus(status) || isCancelledStatus(status)) return ' class="is-failed"';
  return '';
}

function combinedAutomationStatus(...statuses) {
  return statuses.find(isFailedStatus)
    || statuses.find(isCancelledStatus)
    || statuses.find((status) => automationStatusKind(status) === 'running')
    || statuses.find((status) => automationStatusKind(status) === 'waiting')
    || statuses.find(Boolean)
    || '';
}

function contactPeopleLabel(item) {
  const contactCount = Array.isArray(item?.contacts) ? item.contacts.length : 0;
  return contactCount ? `${contactCount} gefunden` : 'offen';
}

function contactStatusLabel(item) {
  const status = pipelineResearchStatus(item, 'contact_research');
  if (isAutomationActive(status) || isFailedStatus(status) || isCancelledStatus(status)) return automationStatusLabel(status);
  return isContactQualified(item) ? 'qualifiziert' : 'offen';
}

function leadStatusLabel(item) {
  const status = pipelineResearchStatus(item, 'lead_qualification');
  if (isAutomationActive(status) || isFailedStatus(status) || isCancelledStatus(status)) return automationStatusLabel(status);
  return isLeadQualified(item) ? 'qualifiziert' : 'offen';
}

function contactColumnValue(item, fieldId) {
  if (fieldId === 'contact.people') return contactPeopleLabel(item);
  if (fieldId === 'contact.status') return contactStatusLabel(item);
  if (fieldId === 'lead.status') return leadStatusLabel(item);
  const contacts = Array.isArray(item?.contacts) ? item.contacts : [];
  const first = contacts[0] || {};
  const payload = item?.payload || {};
  const knowledgeRows = Array.isArray(payload.knowledge_rows) ? payload.knowledge_rows : [];
  const direct = {
    'contact.role': first.role || first.title || first.position,
    'contact.email': first.email || first.e_mail,
    'contact.linkedin': first.linkedin_url || first.linkedin || first.profile_url,
    'contact.phone': first.phone || first.telephone,
    'contact.fit': first.qualification_status || first.fit_reason || first.relevance || payload.contact_fit,
    'lead.reason': item?.lead_reason || item?.qualification_reason || payload.lead_reason,
  };
  const value = direct[fieldId]
    || firstObjectValue(first, [fieldId, fieldId.replace(/^contact\./, ''), fieldId.replace(/^lead\./, '')])
    || firstObjectValue(payload, [fieldId, fieldId.replace(/^contact\./, ''), fieldId.replace(/^lead\./, '')])
    || firstKnowledgeRowValue(knowledgeRows, fieldId);
  if (Array.isArray(value)) return value.filter(Boolean).join(', ') || '-';
  return String(value || '').trim() || '-';
}

function firstKnowledgeRowValue(rows, fieldId) {
  const aliases = [fieldId, fieldId.replace(/^contact\./, ''), fieldId.replace(/^lead\./, '')];
  for (const row of rows || []) {
    const value = firstObjectValue(row, aliases);
    if (value) return value;
  }
  return '';
}

function automationOpenRecords(campaignId, stage) {
  const companies = dedupeCompanies(state.companies.filter((item) => item.campaign_id === campaignId));
  const pipeline = dedupePipelineItems(state.pipeline.filter((item) => item.campaign_id === campaignId));
  if (stage === 'company_research') {
    return companies.filter((company) => !isCompanyResearchDone(company) && !isQueuedStatus(company.research_status));
  }
  if (stage === 'pipeline') {
    const existingCompanyIds = new Set(pipeline.map((item) => item.company_id));
    return companies.filter((company) => (
      !existingCompanyIds.has(company.id)
      && !isQueuedStatus(company.pipeline_status)
      && (company.qualification_status === 'qualified' || isCompanyResearchDone(company))
    ));
  }
  if (stage === 'contact_research') {
    return pipeline.filter((item) => !isContactQualified(item) && !isQueuedStatus(item.contact_research_status));
  }
  if (stage === 'lead_qualification') {
    return pipeline.filter((item) => isContactQualified(item) && !isLeadQualified(item) && !isQueuedStatus(item.outreach_status));
  }
  return [];
}

function openAutomationDrawer(stage, campaignId) {
  const campaign = state.campaigns.find((item) => item.id === campaignId) || selectedCampaign();
  const definition = AUTOMATION_STAGES[stage];
  const root = state.ctx.host.querySelector('[data-outbound-root]');
  if (!campaign || !definition || !root) return;
  state.selectedCampaignId = campaign.id;
  closeAutomationDrawer();
  const records = automationOpenRecords(campaign.id, stage);
  const batchSize = Math.min(OUTBOUND_BATCH_DEFAULT, OUTBOUND_BATCH_LIMIT, records.length || OUTBOUND_BATCH_DEFAULT);
  const drawer = document.createElement('aside');
  drawer.className = 'outbound-automation-drawer';
  drawer.setAttribute('role', 'dialog');
  drawer.setAttribute('aria-modal', 'true');
  drawer.innerHTML = renderAutomationDrawer(campaign, stage, records.length, batchSize);
  root.append(drawer);
}

function closeAutomationDrawer() {
  state.ctx?.host?.querySelector('.outbound-automation-drawer')?.remove();
}

function renderAutomationDrawer(campaign, stage, openCount, batchSize) {
  const definition = AUTOMATION_STAGES[stage];
  const nextCount = Math.min(openCount, batchSize, OUTBOUND_BATCH_LIMIT);
  const selectedSize = [25, 50, 100].find((size) => size >= nextCount) || OUTBOUND_BATCH_LIMIT;
  const allOption = openCount > OUTBOUND_BATCH_LIMIT
    ? `<option value="all">${formatCount(openCount)} offene in 100er-Runs starten</option>`
    : '';
  return `
    <div class="outbound-automation-backdrop" data-action="close-automation"></div>
    <section class="outbound-automation-panel" data-stage="${escapeHtml(stage)}" data-campaign-id="${escapeHtml(campaign.id)}">
      <header class="outbound-automation-header">
        <div>
          <span>CTOX Automation</span>
          <h3>${escapeHtml(definition.label)}</h3>
          <p>${escapeHtml(definition.description)}</p>
        </div>
        <button class="outbound-icon-button" type="button" data-action="close-automation" aria-label="Schließen">×</button>
      </header>
      <div class="outbound-automation-body">
        <dl>
          <div><dt>Campaign</dt><dd>${escapeHtml(campaign.name)}</dd></div>
          <div><dt>Offen</dt><dd>${formatCount(openCount)}</dd></div>
          <div><dt>Nächster Run</dt><dd>${formatCount(nextCount)} Datensätze</dd></div>
          <div><dt>Sicherheit</dt><dd>${OUTBOUND_BATCH_LIMIT} pro Run</dd></div>
        </dl>
        <label>
          <span>Umfang</span>
          <select data-automation-batch-size ${openCount ? '' : 'disabled'}>
            ${[25, 50, 100].map((size) => `<option value="${size}" ${size === selectedSize ? 'selected' : ''}>${size} Datensätze</option>`).join('')}
            ${allOption}
          </select>
        </label>
      </div>
      <footer class="outbound-automation-footer">
        <span class="outbound-muted">${openCount > OUTBOUND_BATCH_LIMIT ? `Alle offenen Researches laufen als kontrollierte CTOX Runs mit je maximal ${OUTBOUND_BATCH_LIMIT} Datensätzen.` : 'Kontrollierter CTOX Batch-Run.'}</span>
        <button class="outbound-button primary" type="button" data-action="start-automation" ${openCount ? '' : 'disabled'}>${escapeHtml(definition.cta)}</button>
      </footer>
    </section>
  `;
}

async function startAutomationBatch() {
  const drawer = state.ctx?.host?.querySelector('.outbound-automation-panel');
  if (!drawer) return;
  const stage = drawer.dataset.stage;
  const campaignId = drawer.dataset.campaignId;
  const campaign = state.campaigns.find((item) => item.id === campaignId);
  if (!campaign || !AUTOMATION_STAGES[stage]) return;
  const requested = drawer.querySelector('[data-automation-batch-size]')?.value || String(OUTBOUND_BATCH_DEFAULT);
  const openRecords = automationOpenRecords(campaign.id, stage);
  const records = requested === 'all'
    ? openRecords
    : openRecords.slice(0, clampNumber(Number(requested) || OUTBOUND_BATCH_DEFAULT, 1, OUTBOUND_BATCH_LIMIT));
  if (!records.length) return;
  const button = drawer.querySelector('[data-action="start-automation"]');
  if (button) {
    button.setAttribute('disabled', 'disabled');
    button.textContent = 'Run wird gestartet...';
  }
  state.selectedCampaignId = campaign.id;
  closeAutomationDrawer();
  window.setTimeout(() => {
    markAutomationRecordsQueued(stage, records);
    render();
    window.setTimeout(() => {
      loadAll().then(render).catch((error) => console.warn('[outbound] refresh after automation start failed', error));
    }, 0);
    runAutomationBatchInBackground(stage, campaign, records);
  }, 0);
}

function markAutomationRecordsQueued(stage, records) {
  const now = Date.now();
  const ids = new Set(records.flatMap((record) => [record.id, ...(record.duplicate_company_ids || [])].filter(Boolean)));
  if (stage === 'company_research') {
    state.companies = state.companies.map((company) => (
      ids.has(company.id) ? { ...company, research_status: 'queued', updated_at_ms: now } : company
    ));
  }
  if (stage === 'pipeline') {
    state.companies = state.companies.map((company) => (
      ids.has(company.id) ? { ...company, pipeline_status: 'queued', qualification_status: 'qualified', updated_at_ms: now } : company
    ));
  }
  if (stage === 'contact_research' || stage === 'lead_qualification') {
    state.pipeline = state.pipeline.map((item) => {
      if (!ids.has(item.id)) return item;
      if (stage === 'contact_research') return { ...item, contact_research_status: 'queued', updated_at_ms: now };
      return { ...item, outreach_status: 'queued', updated_at_ms: now };
    });
  }
  persistAutomationQueueMarkers(stage, records, now);
}

function persistAutomationQueueMarkers(stage, records, now) {
  (async () => {
    for (const record of records) {
      if (stage === 'company_research') {
        await upsertDoc(state.ctx.db.raw.outbound_companies, record.id, {
          ...record,
          research_status: 'queued',
          updated_at_ms: now,
        });
      }
      if (stage === 'pipeline') {
        await upsertDoc(state.ctx.db.raw.outbound_companies, record.id, {
          ...record,
          pipeline_status: 'queued',
          qualification_status: 'qualified',
          updated_at_ms: now,
        });
      }
      if (stage === 'contact_research') {
        await upsertDoc(state.ctx.db.raw.outbound_pipeline_items, record.id, {
          ...record,
          contact_research_status: 'queued',
          updated_at_ms: now,
        });
      }
      if (stage === 'lead_qualification') {
        await upsertDoc(state.ctx.db.raw.outbound_pipeline_items, record.id, {
          ...record,
          outreach_status: 'queued',
          updated_at_ms: now,
        });
      }
    }
  })().catch((error) => console.warn('[outbound] local automation queue markers failed', error));
}

function runAutomationBatchInBackground(stage, campaign, records) {
  (async () => {
    const concurrency = stage === 'pipeline' ? 1 : 5;
    for (let batchIndex = 0; batchIndex < records.length; batchIndex += OUTBOUND_BATCH_LIMIT) {
      const batch = records.slice(batchIndex, batchIndex + OUTBOUND_BATCH_LIMIT);
      for (let index = 0; index < batch.length; index += concurrency) {
        const chunk = batch.slice(index, index + concurrency);
        await Promise.all(chunk.map((record) => runAutomationRecord(stage, campaign, record)));
      }
      await loadAll().catch(() => {});
      render();
    }
    await loadAll();
    render();
  })().catch(async (error) => {
    console.warn('[outbound] automation batch failed', error);
    await loadAll().catch(() => {});
    render();
  });
}

async function runAutomationRecord(stage, campaign, record) {
  if (stage === 'company_research') return queueCompanyResearch(record.id, { forceCampaign: campaign });
  if (stage === 'pipeline') return sendCompanyToPipeline(record.id, { forceCampaign: campaign, keepView: true });
  if (stage === 'contact_research') return queueContactResearch(record.id);
  if (stage === 'lead_qualification') return queueLeadQualification(record.id);
  return null;
}

function exportQualificationTable() {
  const campaign = selectedCampaign();
  if (!campaign) return;
  const settings = getCampaignResearchSettings(campaign);
  const visibleFields = researchFieldsForPrompt(settings)
    .filter((field) => field.id !== 'domain');
  const columns = [
    ...companyTableColumns(visibleFields),
    ...contactTableColumns(settings).filter((column) => !column.action),
  ];
  const rows = filteredQualificationRows();
  const worksheetRows = [
    columns.map((column) => column.label),
    ...rows.map((row) => columns.map((column) => tableColumnValue(row, column.id))),
  ];
  const xmlRows = worksheetRows.map((row) => `
    <Row>${row.map((cell) => `<Cell><Data ss:Type="String">${escapeXml(cell || '')}</Data></Cell>`).join('')}</Row>
  `).join('');
  const xml = `<?xml version="1.0" encoding="UTF-8"?>
<?mso-application progid="Excel.Sheet"?>
<Workbook xmlns="${OUTBOUND_EXPORT_NS}" xmlns:ss="${OUTBOUND_EXPORT_NS}">
  <Worksheet ss:Name="Outbound">
    <Table>${xmlRows}</Table>
  </Worksheet>
</Workbook>`;
  const blob = new Blob([xml], { type: 'application/vnd.ms-excel;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = `${slugifyFileName(campaign.name || 'outbound')}-tabelle.xls`;
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 500);
}

function companyResearchValue(company, fieldId) {
  const data = company.company_data || {};
  const payload = company.payload?.imported_row || {};
  const direct = {
    country: company.country,
    city: company.city,
    domain: company.domain || domainFromUrl(company.website),
    email: data.email || payload.email,
    phone: data.phone || payload.phone,
  };
  const aliases = {
    legal_form: ['legal_form', 'rechtsform'],
    postal_code: ['postal_code', 'plz', 'zip'],
    street: ['street', 'strasse', 'address'],
    registry_court: ['registry_court', 'firmenbuchgericht', 'register_court'],
    registry_id: ['registry_id', 'register_id', 'company_register_id'],
    status: ['status'],
    fax: ['fax'],
    vat_id: ['vat_id', 'ust_id', 'ustid'],
    industry_wz: ['industry_wz', 'branche_wz', 'industry'],
    representative_1: ['representative_1', 'ges_vertreter_1', 'management_public'],
    representative_2: ['representative_2', 'ges_vertreter_2'],
    representative_3: ['representative_3', 'ges_vertreter_3'],
    business_purpose: ['business_purpose', 'gegenstand', 'services'],
    tickers: ['tickers', 'ticker'],
    financials_date: ['financials_date', 'finanzkennzahlen_datum'],
    share_capital_eur: ['share_capital_eur', 'stammkapital_eur', 'grundkapital_eur'],
    balance_sheet_total_eur: ['balance_sheet_total_eur', 'bilanzsumme_eur'],
    profit_eur: ['profit_eur', 'gewinn_eur'],
    revenue_eur: ['revenue_eur', 'umsatz_eur'],
    equity_eur: ['equity_eur', 'eigenkapital_eur'],
    employee_count: ['employee_count', 'mitarbeiterzahl', 'company_size'],
  };
  const value = direct[fieldId] || firstObjectValue(data, aliases[fieldId] || [fieldId]) || firstObjectValue(payload, aliases[fieldId] || [fieldId]);
  if (Array.isArray(value)) return value.filter(Boolean).join(', ') || '-';
  return String(value || '').trim() || '-';
}

function firstObjectValue(object, keys) {
  if (!object || typeof object !== 'object') return '';
  for (const key of keys) {
    if (object[key] != null && String(object[key]).trim()) return object[key];
  }
  return '';
}

function pipelineItemForCompany(companyOrId) {
  const company = typeof companyOrId === 'object' ? companyOrId : state.companies.find((item) => item.id === companyOrId);
  const ids = new Set([typeof companyOrId === 'string' ? companyOrId : company?.id, ...(company?.duplicate_company_ids || [])].filter(Boolean));
  return state.pipeline.find((item) => ids.has(item.company_id));
}

function openResearchSettingsDrawer() {
  const campaign = selectedCampaign();
  const root = state.ctx.host.querySelector('[data-outbound-root]');
  if (!campaign || !root) return;
  closeResearchSettingsDrawer();
  const drawer = document.createElement('aside');
  drawer.className = 'outbound-research-drawer';
  drawer.setAttribute('role', 'dialog');
  drawer.setAttribute('aria-modal', 'true');
  drawer.setAttribute('aria-label', 'Research-Felder einstellen');
  drawer.innerHTML = renderResearchSettingsDrawer(campaign);
  root.append(drawer);
  drawer.querySelector('[data-research-setting-custom]')?.focus();
}

function closeResearchSettingsDrawer() {
  state.ctx?.host?.querySelector('.outbound-research-drawer')?.remove();
}

function renderResearchSettingsDrawer(campaign) {
  const settings = getCampaignResearchSettings(campaign);
  return `
    <div class="outbound-research-backdrop" data-action="close-research-settings"></div>
    <section class="outbound-research-panel">
      <header class="outbound-research-header">
        <div>
          <span>Funnel Tabellen</span>
          <h3>Spalten und Research-Felder</h3>
        </div>
        <button class="outbound-icon-button" type="button" data-action="close-research-settings" aria-label="Schließen">×</button>
      </header>
      <div class="outbound-research-body">
        <div class="outbound-research-toolbar">
          <button class="outbound-button" type="button" data-action="research-settings-core">Kernfelder</button>
          <button class="outbound-button" type="button" data-action="research-settings-all">Alle Felder</button>
        </div>
        <div class="outbound-column-settings-grid">
          ${renderResearchColumnSection('company', 'Unternehmenshälfte', 'Stammdaten und Firmenqualifizierung', RESEARCH_FIELD_DEFS, settings.fields, settings.customFields, settings)}
          ${renderResearchColumnSection('contact', 'Personenhälfte', 'Ansprechpartner und Lead-Qualifizierung', CONTACT_FIELD_DEFS, settings.contactFields, settings.customContactFields, settings)}
        </div>
        <label class="outbound-research-notes">
          <span>Zusätzliche Hinweise</span>
          <textarea data-research-setting-custom rows="3" placeholder="z.B. Belege immer mit URL ausgeben, nur öffentlich belegbare Unternehmensdaten, keine Personen recherchieren">${escapeHtml(settings.customInstruction)}</textarea>
        </label>
      </div>
      <footer class="outbound-research-footer">
        <span class="outbound-muted">Gilt für diese Campaign. Neue aktive Unternehmensspalten werden für bestehende Unternehmen mit CTOX nachrecherchiert; Personen-Spalten werden in der Pipeline verwendet.</span>
        <button class="outbound-button primary" type="button" data-action="save-research-settings">Speichern</button>
      </footer>
    </section>
  `;
}

function renderResearchColumnSection(side, title, subtitle, baseDefs, selectedIds, customFields, settings) {
  const selected = new Set(selectedIds || []);
  const rows = [
    ...baseDefs.map(([id, fallbackLabel, fallbackPrompt]) => ({
      id,
      label: settings.columnLabels?.[id] || fallbackLabel,
      prompt: settings.fieldPrompts?.[id] || fallbackPrompt || '',
      custom: false,
    })),
    ...(customFields || []).map((field) => ({
      ...field,
      label: settings.columnLabels?.[field.id] || field.label,
      prompt: settings.fieldPrompts?.[field.id] || field.prompt || '',
      custom: true,
    })),
  ];
  const addAction = side === 'contact' ? 'research-settings-add-contact-field' : 'research-settings-add-field';
  return `
    <section class="outbound-column-settings" data-column-settings-side="${escapeHtml(side)}" aria-label="${escapeHtml(title)}">
      <div class="outbound-custom-research-head">
        <div>
          <span>${escapeHtml(title)}</span>
          <strong>${escapeHtml(subtitle)}</strong>
        </div>
      </div>
      <div class="outbound-column-settings-labels" aria-hidden="true">
        <span></span>
        <span>Spalte</span>
        <span>CTOX Research-Anweisung</span>
        <span></span>
      </div>
      <div class="outbound-column-settings-list" data-custom-field-list="${escapeHtml(side)}">
        ${rows.map((field) => renderResearchColumnRow(side, field, selected.has(field.id))).join('')}
      </div>
      <div class="outbound-custom-field-form">
        <input data-custom-field-label="${escapeHtml(side)}" placeholder="${side === 'contact' ? 'z.B. Buying Committee, Entscheider-Relevanz' : 'z.B. Zertifizierungen, Zielkunden, Technologien'}" aria-label="Neue Spalte" />
        <button class="outbound-button" type="button" data-action="${addAction}">Hinzufügen</button>
      </div>
    </section>
  `;
}

function renderResearchColumnRow(side, field, checked = true) {
  const removeAction = side === 'contact' ? 'research-settings-remove-contact-field' : 'research-settings-remove-field';
  return `
    <div class="outbound-column-setting-row" data-column-setting-id="${escapeHtml(field.id)}" data-column-setting-side="${escapeHtml(side)}" ${field.custom ? `data-custom-field-id="${escapeHtml(field.id)}"` : ''}>
      <label class="outbound-column-setting-toggle">
        <input type="checkbox" data-column-setting-field="${escapeHtml(field.id)}" data-column-setting-kind="${escapeHtml(side)}" ${checked ? 'checked' : ''} />
        <span></span>
      </label>
      <input data-column-setting-label value="${escapeHtml(field.label)}" aria-label="Spaltenname" />
      <input data-column-setting-prompt value="${escapeHtml(field.prompt || '')}" placeholder="Was soll CTOX dafür recherchieren?" aria-label="Research-Anweisung" />
      ${field.custom ? `<button type="button" data-action="${removeAction}" aria-label="${escapeHtml(`${field.label} löschen`)}">×</button>` : '<i></i>'}
    </div>
  `;
}

async function saveResearchSettings() {
  const campaign = selectedCampaign();
  const drawer = state.ctx?.host?.querySelector('.outbound-research-drawer');
  if (!campaign || !drawer) return;
  const beforeSettings = getCampaignResearchSettings(campaign);
  const fields = Array.from(drawer.querySelectorAll('[data-column-setting-kind="company"][data-column-setting-field]:checked'))
    .map((input) => input.dataset.columnSettingField)
    .filter(Boolean);
  const contactFields = Array.from(drawer.querySelectorAll('[data-column-setting-kind="contact"][data-column-setting-field]:checked'))
    .map((input) => input.dataset.columnSettingField)
    .filter(Boolean);
  const companyRows = Array.from(drawer.querySelectorAll('[data-column-setting-side="company"][data-column-setting-id]'));
  const contactRows = Array.from(drawer.querySelectorAll('[data-column-setting-side="contact"][data-column-setting-id]'));
  const customFields = companyRows
    .filter((node) => node.dataset.customFieldId)
    .map((node) => ({
      id: node.dataset.columnSettingId,
      label: node.querySelector('[data-column-setting-label]')?.value?.trim() || '',
      prompt: node.querySelector('[data-column-setting-prompt]')?.value?.trim() || '',
    }))
    .filter((field) => field.id && field.label);
  const customContactFields = contactRows
    .filter((node) => node.dataset.customFieldId)
    .map((node) => ({
      id: node.dataset.columnSettingId,
      label: node.querySelector('[data-column-setting-label]')?.value?.trim() || '',
      prompt: node.querySelector('[data-column-setting-prompt]')?.value?.trim() || '',
    }))
    .filter((field) => field.id && field.label);
  const columnLabels = {};
  const fieldPrompts = {};
  [...companyRows, ...contactRows].forEach((node) => {
    const id = node.dataset.columnSettingId;
    const label = node.querySelector('[data-column-setting-label]')?.value?.trim() || '';
    const prompt = node.querySelector('[data-column-setting-prompt]')?.value?.trim() || '';
    if (id && label) columnLabels[id] = label;
    if (id && prompt) fieldPrompts[id] = prompt;
  });
  const customInstruction = drawer.querySelector('[data-research-setting-custom]')?.value?.trim() || '';
  const researchSettings = normalizeResearchSettings({
    fields,
    contactFields,
    customFields,
    customContactFields,
    columnLabels,
    fieldPrompts,
    customInstruction,
  });
  const nextPayload = {
    ...(campaign.payload || {}),
    research_settings: researchSettings,
  };
  await patchDoc(state.ctx.db.raw.outbound_campaigns, campaign.id, {
    payload: nextPayload,
    updated_at_ms: Date.now(),
  });
  state.campaigns = state.campaigns.map((item) => (
    item.id === campaign.id
      ? { ...item, payload: nextPayload, updated_at_ms: Date.now() }
      : item
  ));
  closeResearchSettingsDrawer();
  render();
  window.setTimeout(() => {
    loadAll()
      .then(() => queueResearchForNewFields(campaign.id, beforeSettings, researchSettings))
      .then(loadAll)
      .then(render)
      .catch((error) => console.warn('[outbound] research settings follow-up queue failed', error));
  }, 0);
}

function setResearchSettingsSelection(checked) {
  const drawer = state.ctx?.host?.querySelector('.outbound-research-drawer');
  drawer?.querySelectorAll('[data-column-setting-field]').forEach((input) => {
    input.checked = checked;
  });
}

function setResearchSettingsCoreSelection() {
  const core = new Set([
    'legal_form',
    'country',
    'postal_code',
    'city',
    'street',
    'registry_id',
    'phone',
    'email',
    'domain',
    'vat_id',
    'industry_wz',
    'representative_1',
    'business_purpose',
    'revenue_eur',
    'employee_count',
  ]);
  const drawer = state.ctx?.host?.querySelector('.outbound-research-drawer');
  drawer?.querySelectorAll('[data-column-setting-field]').forEach((input) => {
    if (input.dataset.columnSettingKind === 'contact') {
      input.checked = DEFAULT_CONTACT_FIELD_IDS.includes(input.dataset.columnSettingField);
    } else {
      input.checked = core.has(input.dataset.columnSettingField);
    }
  });
}

function addResearchSettingsField(side = 'company') {
  const drawer = state.ctx?.host?.querySelector('.outbound-research-drawer');
  const input = drawer?.querySelector(`[data-custom-field-label="${cssEscape(side)}"]`);
  const list = drawer?.querySelector(`[data-custom-field-list="${cssEscape(side)}"]`);
  if (!drawer || !input || !list) return;
  const label = input.value.trim();
  if (!label) return;
  const id = side === 'contact' ? customContactFieldId(label) : customResearchFieldId(label);
  if (drawer.querySelector(`[data-column-setting-id="${cssEscape(id)}"]`)) {
    showBusinessAlert('Diese Spalte gibt es bereits.');
    return;
  }
  list.insertAdjacentHTML('beforeend', renderResearchColumnRow(side, { id, label, prompt: '', custom: true }, true));
  input.value = '';
  drawer.querySelector(`[data-column-setting-id="${cssEscape(id)}"] [data-column-setting-label]`)?.focus();
}

function removeResearchSettingsField(fieldId, side = 'company') {
  if (!fieldId) return;
  const drawer = state.ctx?.host?.querySelector('.outbound-research-drawer');
  const row = drawer?.querySelector(`[data-column-setting-side="${cssEscape(side)}"][data-custom-field-id="${cssEscape(fieldId)}"]`);
  row?.remove();
}

function getCampaignResearchSettings(campaign) {
  return normalizeResearchSettings(campaign?.payload?.research_settings || {});
}

function normalizeResearchSettings(raw = {}) {
  const customFields = normalizeCustomResearchFields(raw.customFields || raw.custom_fields || []);
  const customContactFields = normalizeCustomContactFields(raw.customContactFields || raw.custom_contact_fields || []);
  const columnLabels = normalizeStringMap(raw.columnLabels || raw.column_labels || {});
  const fieldPrompts = normalizeStringMap(raw.fieldPrompts || raw.field_prompts || {});
  const known = new Set([...RESEARCH_FIELD_DEFS.map(([id]) => id), ...customFields.map((field) => field.id)]);
  const fields = Array.isArray(raw.fields)
    ? raw.fields.filter((id) => known.has(id))
    : [...DEFAULT_RESEARCH_FIELD_IDS, ...customFields.map((field) => field.id)];
  const knownContact = new Set([...CONTACT_FIELD_DEFS.map(([id]) => id), ...customContactFields.map((field) => field.id)]);
  const contactFields = Array.isArray(raw.contactFields || raw.contact_fields)
    ? (raw.contactFields || raw.contact_fields).filter((id) => knownContact.has(id))
    : [...DEFAULT_CONTACT_FIELD_IDS, ...customContactFields.map((field) => field.id)];
  return {
    fields: fields.length ? fields : DEFAULT_RESEARCH_FIELD_IDS,
    contactFields: contactFields.length ? contactFields : DEFAULT_CONTACT_FIELD_IDS,
    customFields,
    customContactFields,
    columnLabels,
    fieldPrompts,
    customInstruction: String(raw.customInstruction || raw.custom_instruction || '').trim(),
  };
}

function researchFieldsForPrompt(settings) {
  const selected = new Set(settings.fields);
  const customFields = settings.customFields || [];
  return [
    ...RESEARCH_FIELD_DEFS.map(([id, label]) => ({ id, label: settings.columnLabels?.[id] || label, prompt: settings.fieldPrompts?.[id] || '' })),
    ...customFields.map((field) => ({ id: field.id, label: settings.columnLabels?.[field.id] || field.label, prompt: settings.fieldPrompts?.[field.id] || field.prompt || '', custom: true })),
  ]
    .filter((field) => selected.has(field.id));
}

function contactFieldsForPrompt(settings) {
  const selected = new Set(settings.contactFields || DEFAULT_CONTACT_FIELD_IDS);
  const customFields = settings.customContactFields || [];
  return [
    ...CONTACT_FIELD_DEFS.map(([id, label, prompt]) => ({ id, label: settings.columnLabels?.[id] || label, prompt: settings.fieldPrompts?.[id] || prompt || '' })),
    ...customFields.map((field) => ({ id: field.id, label: settings.columnLabels?.[field.id] || field.label, prompt: settings.fieldPrompts?.[field.id] || field.prompt || '', custom: true })),
  ].filter((field) => selected.has(field.id));
}

function normalizeCustomResearchFields(fields) {
  if (!Array.isArray(fields)) return [];
  const seen = new Set(RESEARCH_FIELD_DEFS.map(([id]) => id));
  const normalized = [];
  for (const field of fields) {
    const label = String(field?.label || field?.name || '').replace(/\s+/g, ' ').trim();
    if (!label) continue;
    const id = String(field?.id || customResearchFieldId(label)).trim();
    if (!id || seen.has(id)) continue;
    seen.add(id);
    normalized.push({ id, label, prompt: String(field?.prompt || field?.instruction || '').trim() });
  }
  return normalized;
}

function normalizeCustomContactFields(fields) {
  if (!Array.isArray(fields)) return [];
  const seen = new Set(CONTACT_FIELD_DEFS.map(([id]) => id));
  const normalized = [];
  for (const field of fields) {
    const label = String(field?.label || field?.name || '').replace(/\s+/g, ' ').trim();
    if (!label) continue;
    const id = String(field?.id || customContactFieldId(label)).trim();
    if (!id || seen.has(id)) continue;
    seen.add(id);
    normalized.push({ id, label, prompt: String(field?.prompt || field?.instruction || '').trim() });
  }
  return normalized;
}

function normalizeStringMap(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return {};
  return Object.fromEntries(Object.entries(value)
    .map(([key, item]) => [String(key), String(item || '').trim()])
    .filter(([key, item]) => key && item));
}

function customResearchFieldId(label) {
  const slug = slugifyFileName(label).replaceAll('-', '_');
  return `custom_${slug || fingerprint(label).slice(0, 8)}`;
}

function customContactFieldId(label) {
  const slug = slugifyFileName(label).replaceAll('-', '_');
  return `contact_custom_${slug || fingerprint(label).slice(0, 8)}`;
}

async function queueResearchForNewFields(campaignId, beforeSettings, nextSettings) {
  const before = new Set(beforeSettings.fields || []);
  const beforeContact = new Set(beforeSettings.contactFields || []);
  const addedCustomFields = (nextSettings.customFields || [])
    .filter((field) => nextSettings.fields.includes(field.id) && !before.has(field.id));
  const addedCustomContactFields = (nextSettings.customContactFields || [])
    .filter((field) => nextSettings.contactFields.includes(field.id) && !beforeContact.has(field.id));
  if (!addedCustomFields.length && !addedCustomContactFields.length) return;
  const campaign = state.campaigns.find((item) => item.id === campaignId);
  if (!campaign) return;
  const companies = state.companies
    .filter((item) => item.campaign_id === campaignId && !isQueuedStatus(item.research_status))
    .slice(0, OUTBOUND_BATCH_LIMIT);
  const pipelineItems = state.pipeline
    .filter((item) => item.campaign_id === campaignId && !isQueuedStatus(item.contact_research_status))
    .slice(0, OUTBOUND_BATCH_LIMIT);
  for (const company of addedCustomFields.length ? companies : []) {
    await queueCompanyResearch(company.id, {
      forceCampaign: campaign,
      forceSettings: nextSettings,
      fieldsOverride: addedCustomFields,
      reason: 'custom_fields_added',
    });
  }
  for (const item of addedCustomContactFields.length ? pipelineItems : []) {
    await queueContactResearch(item.id);
  }
}

function visibleCampaigns() {
  const defaultCandidates = state.campaigns.filter((campaign) => campaign.name === DEFAULT_CAMPAIGN_NAME);
  const emptyDefaultIds = new Set(defaultCandidates.filter((campaign) => !campaignHasData(campaign.id)).map((campaign) => campaign.id));
  const preferredDefault = defaultCandidates.find((campaign) => campaign.id === DEFAULT_CAMPAIGN_ID)
    || defaultCandidates.find((campaign) => !emptyDefaultIds.has(campaign.id))
    || defaultCandidates[0];
  return state.campaigns.filter((campaign) => {
    if (campaign.name !== DEFAULT_CAMPAIGN_NAME) return true;
    if (!emptyDefaultIds.has(campaign.id)) return true;
    return campaign.id === preferredDefault?.id;
  });
}

function campaignHasData(campaignId) {
  return state.sources.some((item) => item.campaign_id === campaignId)
    || state.companies.some((item) => item.campaign_id === campaignId)
    || state.pipeline.some((item) => item.campaign_id === campaignId);
}

function campaignFunnelMetrics(campaignId) {
  const sources = state.sources.filter((item) => item.campaign_id === campaignId);
  const rawCompanies = state.companies.filter((item) => item.campaign_id === campaignId);
  const companies = dedupeCompanies(rawCompanies);
  const pipeline = dedupePipelineItems(state.pipeline.filter((item) => item.campaign_id === campaignId));
  const companySourceIds = new Set(rawCompanies.map((item) => item.source_id).filter(Boolean));
  const parsedSourceIds = new Set();
  const parsedSourceCount = sources.filter((source) => {
    const status = String(source.status || '').toLowerCase();
    const commandStatus = sourceImportCommandStatus(source);
    const parsed = ['imported', 'parsed', 'completed', 'done'].includes(status)
      || ['completed', 'done', 'imported', 'parsed'].includes(commandStatus)
      || companySourceIds.has(source.id);
    if (parsed) parsedSourceIds.add(source.id);
    return parsed;
  }).length;
  const runningSourceCount = sources.filter((source) => !parsedSourceIds.has(source.id) && isImportSourceRunning(source)).length;
  const failedSourceCount = sources.filter((source) => !parsedSourceIds.has(source.id) && isImportSourceFailed(source)).length;
  const companyResearchDone = companies.filter(isCompanyResearchDone).length;
  const companyQualified = companies.filter((item) => item.qualification_status === 'qualified').length;
  const contactQualified = pipeline.filter(isContactQualified).length;
  const leadQualified = pipeline.filter(isLeadQualified).length;
  return {
    input: companies.length,
    sourceCount: sources.length,
    parsedSourceCount,
    runningSourceCount,
    failedSourceCount,
    companyResearchDone,
    companyResearchTotal: companies.length,
    companyResearchOpen: automationOpenRecords(campaignId, 'company_research').length,
    companyQualified,
    companyQualifiedTotal: Math.max(companyResearchDone, companyQualified),
    pipelineOpen: automationOpenRecords(campaignId, 'pipeline').length,
    contactQualified,
    contactQualifiedTotal: Math.max(companyQualified, contactQualified),
    contactOpen: automationOpenRecords(campaignId, 'contact_research').length,
    leadQualified,
    leadQualifiedTotal: Math.max(contactQualified, leadQualified),
    leadOpen: automationOpenRecords(campaignId, 'lead_qualification').length,
  };
}

function isImportSourceRunning(source) {
  const status = String(source?.status || '').toLowerCase();
  if (['queued_parser', 'queued', 'accepted', 'running', 'processing', 'working'].includes(status)) return true;
  const commandStatus = sourceImportCommandStatus(source);
  return ['accepted', 'queued', 'running', 'working', 'leased'].includes(commandStatus);
}

function isImportSourceFailed(source) {
  const status = String(source?.status || '').toLowerCase();
  if (['failed', 'failed_parser', 'error'].includes(status)) return true;
  return sourceImportCommandStatus(source) === 'failed';
}

function sourceImportCommandStatus(source) {
  const commandId = source?.payload?.record_id || source?.payload?.command_id || source?.command_id || '';
  if (!commandId) return '';
  const command = state.commands.find((item) => item.command_id === commandId || item.id === commandId);
  return commandStatusForCommand(command);
}

function isCompanyResearchDone(company) {
  const status = String(company?.research_status || '').toLowerCase();
  return ['researched', 'completed', 'done'].includes(status)
    || ['qualified', 'rejected'].includes(String(company?.qualification_status || '').toLowerCase());
}

function isQueuedStatus(value) {
  return ['queued', 'accepted', 'pending', 'scheduled', 'running', 'in_progress', 'processing', 'working', 'leased'].includes(String(value || '').toLowerCase());
}

function isDoneLikeStatus(value) {
  return ['researched', 'qualified', 'completed', 'done', 'handled', 'lead_qualified'].includes(String(value || '').toLowerCase());
}

function isAutomationActive(value) {
  const kind = automationStatusKind(value);
  return kind === 'running' || kind === 'waiting';
}

function isFailedStatus(value) {
  const kind = automationStatusKind(value);
  return kind === 'failed' || kind === 'blocked';
}

function isCancelledStatus(value) {
  return automationStatusKind(value) === 'cancelled';
}

function automationStatusKind(value) {
  const status = String(value || '').toLowerCase();
  if (['running', 'in_progress', 'processing', 'working', 'leased'].includes(status)) return 'running';
  if (['queued', 'accepted', 'pending', 'scheduled'].includes(status)) return 'waiting';
  if (['failed', 'error', 'pending_sync_failed'].includes(status)) return 'failed';
  if (['blocked', 'stale_missing_native'].includes(status)) return 'blocked';
  if (['cancelled', 'canceled'].includes(status)) return 'cancelled';
  if (['completed', 'done', 'handled', 'researched', 'qualified'].includes(status)) return 'done';
  return 'idle';
}

function automationStatusLabel(value) {
  const kind = automationStatusKind(value);
  if (kind === 'running') return 'läuft gerade';
  if (kind === 'waiting') return 'wartet in CTOX';
  if (kind === 'failed') return 'Fehler';
  if (kind === 'blocked') return 'blockiert';
  if (kind === 'cancelled') return 'abgebrochen';
  if (kind === 'done') return 'fertig';
  return 'nicht gestartet';
}

function badgeClassForStatus(status, isDone) {
  if (isDone) return 'good';
  if (isAutomationActive(status)) return 'is-running';
  if (isFailedStatus(status) || isCancelledStatus(status)) return 'warn';
  return '';
}

function renderInlineResearchStatus(label, status) {
  const active = isAutomationActive(status);
  const failed = isFailedStatus(status) || isCancelledStatus(status);
  const className = ['outbound-inline-status', active ? 'is-running' : '', failed ? 'warn' : ''].filter(Boolean).join(' ');
  const statusLabel = active || failed ? automationStatusLabel(status) : label;
  return `<small class="${className}">${active ? '<i></i>' : ''}${escapeHtml(statusLabel || 'offen')}</small>`;
}

function companyResearchStatus(company) {
  const run = latestAutomationRun('company_research', companyRecordIds(company));
  return commandStatusForRun(run) || run?.status || company?.research_status || '';
}

function pipelineResearchStatus(item, stage) {
  if (!item) return '';
  const run = latestAutomationRun(stage, new Set([item.id, item.pipeline_id, item.company_id].filter(Boolean)));
  if (stage === 'contact_research') return commandStatusForRun(run) || run?.status || item.contact_research_status || '';
  if (stage === 'lead_qualification') return commandStatusForRun(run) || run?.status || item.outreach_status || '';
  return commandStatusForRun(run) || run?.status || '';
}

function latestAutomationRun(runType, recordIds) {
  const ids = recordIds instanceof Set ? recordIds : new Set(recordIds || []);
  if (!ids.size) return null;
  return state.runs
    .filter((run) => run.run_type === runType && [run.company_id, run.pipeline_id, run.record_id, run.id].some((id) => ids.has(id)))
    .sort((a, b) => Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0))[0] || null;
}

function commandStatusForRun(run) {
  if (!run?.command_id) return '';
  const command = state.commands.find((item) => item.command_id === run.command_id || item.id === run.command_id);
  return commandStatusForCommand(command || run);
}

function commandStatusForCommand(command) {
  const task = queueTaskForCommand(command);
  return String(task?.status || task?.route_status || command?.task_status || command?.status || '').toLowerCase();
}

function queueTaskForCommand(commandOrRun) {
  if (!commandOrRun) return null;
  const commandId = commandOrRun.command_id || commandOrRun.id || '';
  const taskId = commandOrRun.task_id || '';
  return state.queueTasks
    .filter((task) => (
      (commandId && (task.command_id === commandId || task.client_command_id === commandId))
      || (taskId && task.id === taskId)
    ))
    .sort((a, b) => Number(b.updated_at_ms || b.updated_at || 0) - Number(a.updated_at_ms || a.updated_at || 0))[0] || null;
}

function companyRecordIds(company) {
  return new Set([company?.id, ...(company?.duplicate_company_ids || []), ...(company?.payload?.duplicate_company_ids || [])].filter(Boolean));
}

function isContactQualified(item) {
  if (!item) return false;
  const status = String(item.contact_research_status || '').toLowerCase();
  const stage = String(item.stage || '').toLowerCase();
  return (Array.isArray(item.contacts) && item.contacts.length > 0)
    || ['qualified', 'researched', 'completed', 'done'].includes(status)
    || ['contact_qualified', 'lead_qualified', 'outreach', 'conversation'].includes(stage);
}

function isLeadQualified(item) {
  if (!item) return false;
  const status = String(item.outreach_status || '').toLowerCase();
  const stage = String(item.stage || '').toLowerCase();
  return ['lead_qualified', 'qualified', 'ready', 'completed'].includes(status)
    || ['lead_qualified', 'lead-ready', 'lead_ready', 'conversation'].includes(stage);
}

function conversionRate(from, to) {
  const denominator = Number(from || 0);
  if (denominator <= 0) return '0%';
  return `${Math.round((Number(to || 0) / denominator) * 100)}%`;
}

function renderCompanyWorkbench() {
  const companies = filteredCompanies();
  return `
    <div class="outbound-workbench">
      <div class="outbound-filter">
        ${[
          ['all', 'Alle'],
          ['research', 'Research offen'],
          ['qualified', 'Qualifiziert'],
          ['pipeline', 'Pipeline'],
          ['rejected', 'Nicht passend'],
        ].map(([id, label]) => `<button type="button" data-filter="${id}" aria-pressed="${state.filter === id}">${label}</button>`).join('')}
        <input class="outbound-search" data-search placeholder="Firma suchen" value="${escapeHtml(state.search)}" />
      </div>
      <div class="outbound-scroll">
        <div class="outbound-list">
          ${companies.map(renderCompanyRow).join('') || '<div class="outbound-empty">Keine Firmen für diesen Filter.</div>'}
        </div>
      </div>
    </div>
  `;
}

function renderCompanyRow(company) {
  return `
    <div class="outbound-company-row" data-action="select-company" data-id="${escapeHtml(company.id)}" aria-current="${company.id === state.selectedCompanyId}">
      <div>
        <div class="outbound-title">${escapeHtml(company.name)}</div>
        <div class="outbound-muted">${escapeHtml(company.domain || company.website || 'Website offen')}</div>
      </div>
      <div>
        <span class="outbound-badge ${company.research_status === 'researched' ? 'good' : 'warn'}">${labelResearch(company.research_status)}</span>
      </div>
      <div>
        <span class="outbound-badge ${company.qualification_status === 'qualified' ? 'good' : company.qualification_status === 'rejected' ? 'warn' : ''}">${labelQualification(company.qualification_status)}</span>
      </div>
      <div class="outbound-row-actions">
        <button class="outbound-button" type="button" data-action="research-company" data-id="${escapeHtml(company.id)}">Research</button>
        <button class="outbound-button" type="button" data-action="qualify-company" data-id="${escapeHtml(company.id)}">Qualifizieren</button>
        <button class="outbound-button" type="button" data-action="send-pipeline" data-id="${escapeHtml(company.id)}">Pipeline</button>
      </div>
    </div>
  `;
}

function renderPipelineWorkbench() {
  const items = currentPipeline();
  return `
    <div class="outbound-workbench">
      <div class="outbound-filter">
        <span class="outbound-muted">Nur qualifizierte Unternehmen gehen hier in die Ansprechpartner-Stufe.</span>
      </div>
      <div class="outbound-scroll">
        <div class="outbound-list">
          ${items.map(renderPipelineItem).join('') || '<div class="outbound-empty">Noch keine Firmen in der Pipeline.</div>'}
        </div>
      </div>
    </div>
  `;
}

function renderPipelineItem(item) {
  return `
    <div class="outbound-pipeline-item" data-action="select-pipeline" data-id="${escapeHtml(item.id)}" aria-current="${item.id === state.selectedPipelineId}">
      <div class="outbound-item-top">
        <div>
          <div class="outbound-title">${escapeHtml(item.company_name)}</div>
          <div class="outbound-muted">${escapeHtml(item.stage)} · ${escapeHtml(item.contact_research_status)}</div>
        </div>
        <button class="outbound-button" type="button" data-action="research-contacts" data-id="${escapeHtml(item.id)}">Ansprechpartner</button>
      </div>
      <div class="outbound-badges">
        <span class="outbound-badge">${escapeHtml(item.priority || 'normal')}</span>
        <span class="outbound-badge">${escapeHtml(item.outreach_status || 'not_started')}</span>
      </div>
    </div>
  `;
}

function renderRight() {
  const root = state.ctx.host.querySelector('.outbound-right');
  if (!root) return;
  if (state.activeView === 'pipeline') {
    root.innerHTML = renderPipelineDetail();
    return;
  }
  root.innerHTML = renderCompanyDetail();
}

function renderCompanyDetail() {
  const company = selectedCompany();
  if (!company) return '<div class="outbound-empty">Firma auswählen.</div>';
  const runs = state.runs.filter((run) => run.company_id === company.id);
  return `
    <header class="outbound-pane-header">
      <div><span>Company Research</span><h2>${escapeHtml(company.name)}</h2></div>
    </header>
    <div class="outbound-detail">
      <div class="outbound-detail-block">
        <div class="outbound-field-list">
          ${field('Website', company.website || company.company_data?.website || 'offen')}
          ${field('Ort', [company.city, company.country].filter(Boolean).join(', ') || 'offen')}
          ${field('Fit', `${labelQualification(company.qualification_status)} · ${company.fit_score || 0}/100`)}
          ${field('Pipeline', labelPipeline(company.pipeline_status))}
        </div>
      </div>
      <div class="outbound-detail-block">
        <div class="outbound-row-actions">
          <button class="outbound-button primary" type="button" data-action="research-company" data-id="${escapeHtml(company.id)}">Unternehmen recherchieren</button>
          <button class="outbound-button" type="button" data-action="qualify-company" data-id="${escapeHtml(company.id)}">Qualifizieren</button>
          <button class="outbound-button" type="button" data-action="reject-company" data-id="${escapeHtml(company.id)}">Ablehnen</button>
          <button class="outbound-button" type="button" data-action="send-pipeline" data-id="${escapeHtml(company.id)}">In Pipeline</button>
        </div>
      </div>
      <div class="outbound-detail-block">
        <div class="outbound-kicker">Evidence</div>
        ${renderEvidence(company)}
      </div>
      <div class="outbound-detail-block">
        <div class="outbound-kicker">Research Runs</div>
        ${runs.map((run) => `<div class="outbound-muted">${escapeHtml(run.run_type)} · ${escapeHtml(run.status)} · ${new Date(run.updated_at_ms).toLocaleString()}</div>`).join('') || '<div class="outbound-muted">Noch keine Research Runs.</div>'}
      </div>
    </div>
  `;
}

function renderPipelineDetail() {
  const item = selectedPipelineItem();
  if (!item) return '<div class="outbound-empty">Pipeline-Eintrag auswählen.</div>';
  return `
    <header class="outbound-pane-header">
      <div><span>Pipeline</span><h2>${escapeHtml(item.company_name)}</h2></div>
    </header>
    <div class="outbound-detail">
      <div class="outbound-detail-block">
        ${field('Stage', item.stage)}
        ${field('Contact Research', item.contact_research_status)}
        ${field('Outreach', item.outreach_status)}
      </div>
      <div class="outbound-detail-block">
        <button class="outbound-button primary" type="button" data-action="research-contacts" data-id="${escapeHtml(item.id)}">Ansprechpartner recherchieren</button>
      </div>
      <div class="outbound-detail-block">
        <div class="outbound-kicker">Kontakte</div>
        ${(item.contacts || []).map((contact) => `<div class="outbound-muted">${escapeHtml(contact.name || 'Kontakt')} · ${escapeHtml(contact.role || '')}</div>`).join('') || '<div class="outbound-muted">Kontakte werden erst in dieser Pipeline-Stufe recherchiert.</div>'}
      </div>
    </div>
  `;
}

async function createCampaign() {
  const name = await showBusinessPrompt('Name der Outbound Campaign', {
    title: 'Campaign anlegen',
    defaultValue: 'Neue Outbound Campaign',
  });
  if (!name) return;
  const now = Date.now();
  const id = `camp_${crypto.randomUUID()}`;
  const campaign = {
    id,
    name,
    objective: 'Outbound Firmenqualifizierung',
    market: 'DACH',
    status: 'active',
    owner_id: state.ctx?.session?.user?.id || '',
    source_count: 0,
    company_count: 0,
    qualified_count: 0,
    pipeline_count: 0,
    payload: { outbound_only: true },
    created_at_ms: now,
    updated_at_ms: now,
  };
  await state.ctx.db.raw.outbound_campaigns.insert(campaign);
  await ensureCampaignKnowledge(campaign).catch((error) => {
    console.warn('[outbound] campaign knowledge setup failed', error);
  });
  state.selectedCampaignId = id;
  await loadAll();
  render();
}

async function saveCampaignInlineEdit(campaignId) {
  const campaign = state.campaigns.find((item) => item.id === campaignId);
  if (!campaign) return;
  const editor = state.ctx?.host?.querySelector(`.outbound-campaign-edit[data-id="${cssEscape(campaign.id)}"]`);
  if (!editor) return;
  const name = editor.querySelector('[data-campaign-edit-field="name"]')?.value?.trim() || campaign.name;
  const subtitle = editor.querySelector('[data-campaign-edit-field="subtitle"]')?.value?.trim() || '';
  const scope = editor.querySelector('[data-campaign-edit-field="scope"]')?.value?.trim() || '';
  const payload = {
    ...(campaign.payload || {}),
    subtitle,
    scope,
  };
  await patchDoc(state.ctx.db.raw.outbound_campaigns, campaign.id, {
    name,
    objective: scope,
    payload,
    updated_at_ms: Date.now(),
  });
  await ensureCampaignKnowledge({ ...campaign, name, objective: scope, payload }).catch((error) => {
    console.warn('[outbound] campaign runbook update failed', error);
  });
  state.editingCampaignId = '';
  await loadAll();
  render();
}

async function deleteCampaign(campaignId) {
  const campaign = state.campaigns.find((item) => item.id === campaignId);
  if (!campaign) return;
  const ok = await showBusinessConfirm(`Campaign "${campaign.name}" löschen? Importjobs, Firmen und Pipeline-Einträge dieser Campaign werden entfernt.`, {
    title: 'Campaign löschen',
    confirmLabel: 'Löschen',
  });
  if (!ok) return;
  await removeWhere(state.ctx.db.raw.outbound_sources, (item) => item.campaign_id === campaign.id);
  await removeWhere(state.ctx.db.raw.outbound_companies, (item) => item.campaign_id === campaign.id);
  await removeWhere(state.ctx.db.raw.outbound_pipeline_items, (item) => item.campaign_id === campaign.id);
  await removeWhere(state.ctx.db.raw.outbound_research_runs, (item) => item.campaign_id === campaign.id);
  await removeDoc(state.ctx.db.raw.outbound_campaigns, campaign.id);
  state.selectedCampaignId = '';
  await ensureDefaultCampaign();
  await loadAll();
  render();
}

async function openCompanyImporter() {
  const campaign = selectedCampaign();
  if (!campaign) return;
  await openUniversalImporter(state.ctx, {
    side: 'left',
    moduleId: 'outbound',
    entityType: 'company_source',
    commandType: 'outbound.source.import',
    title: 'Importjob anlegen',
    kicker: 'Outbound Import',
    defaultTitle: `${campaign.name} Import`,
    helperText: 'URL, PDF, Text oder Excel liefern Unternehmen fuer den Input-Funnel. Der Importer extrahiert daraus Unternehmen; Personen werden erst spaeter in der Pipeline recherchiert.',
    filterPromptLabel: 'Importfilter',
    filterPromptPlaceholder: 'z.B. nur Firmen mit Sitz in Deutschland',
    defaultFilterPrompt: 'Nur Firmen mit Sitz in Deutschland importieren.',
    submitLabel: 'Importjob starten',
    submittingLabel: 'Importjob wird angelegt...',
    doneLabel: 'Importjob angelegt.',
    closeOnSubmit: true,
    dispatch: false,
    definition: {
      target_collection: 'outbound_companies',
      pipeline_boundary: 'company_first_contact_later',
      import_filter: {
        company_country_codes: ['DE'],
        company_country_labels: ['Deutschland', 'Germany'],
        rule: 'Nur Unternehmen mit Sitz in Deutschland importieren.',
      },
    },
    clientContext: { campaign_id: campaign.id },
    onImport: async ({ payload }) => {
      const result = await importCompaniesFromPayload(campaign, payload);
      if (result?.status === 'queued_parser') {
        window.setTimeout(() => {
          loadAll().then(render).catch((error) => console.warn('[outbound] refresh after async import failed', error));
        }, 0);
        return result;
      }
      await loadAll();
      render();
      return result;
    },
  });
}

async function importCompaniesFromPayload(campaign, payload) {
  const now = Date.now();
  const sourceId = `src_${crypto.randomUUID()}`;
  const rows = extractRowsFromPayload(payload);
  const sourceStatus = rows.length ? 'imported' : 'queued_parser';
  await state.ctx.db.raw.outbound_sources.insert({
    id: sourceId,
    campaign_id: campaign.id,
    title: payload.title,
    source_type: payload.source_type,
    status: sourceStatus,
    file_name: payload.source?.files?.[0]?.name || '',
    row_count: rows.length,
    imported_count: rows.length,
    payload,
    created_at_ms: now,
    updated_at_ms: now,
  });
  if (!rows.length) {
    runOutboundImportInBackground(campaign, payload, sourceId);
    return {
      status: sourceStatus,
      message: 'Importjob gestartet.',
      detail: 'CTOX extrahiert Unternehmen im Hintergrund.',
      dispatch: false,
    };
  }
  const refs = await ensureCampaignKnowledge(campaign).catch((error) => {
    console.warn('[outbound] knowledge campaign setup failed', error);
    return campaignKnowledgeRefs(campaign);
  });
  const filteredRows = filterRowsForCampaignImport(campaign, payload, rows);
  for (const row of filteredRows) {
    const companyId = companyIdFromImportRow(campaign, row);
    const company = {
      id: companyId,
      campaign_id: campaign.id,
      source_id: sourceId,
      row_index: row.row_index || 0,
      name: row.name,
      website: row.website || '',
      domain: row.domain || '',
      city: row.city || '',
      country: row.country || '',
      qualification_status: 'new',
      research_status: 'pending',
      pipeline_status: 'not_started',
      fit_score: 0,
      fit_status: 'unqualified',
      company_data: {},
      evidence: [],
      payload: { imported_row: row.raw || row },
      created_at_ms: now,
      updated_at_ms: now,
    };
    await upsertDoc(state.ctx.db.raw.outbound_companies, company.id, company);
  }
  await appendKnowledgeRows(campaign, refs.companiesKey, filteredRows.map((row) => companyKnowledgeRow(campaign, sourceId, row, now))).catch((error) => {
    console.warn('[outbound] knowledge companies append failed', error);
  });
  await updateCampaignCounts(campaign.id);
  return {
    status: sourceStatus,
    message: `${filteredRows.length} Firmen importiert.`,
    detail: '',
    dispatch: false,
  };
}

function runOutboundImportInBackground(campaign, payload, sourceId) {
  queueOutboundImportCommand(campaign, payload, sourceId)
    .then(async (result) => {
      const refs = campaignKnowledgeRefs(campaign);
      const importedCount = await countKnowledgeRowsForSource(refs, sourceId).catch(() => 0);
      await patchDoc(state.ctx.db.raw.outbound_sources, sourceId, {
        status: result?.status === 'completed' ? 'imported' : result?.status || 'queued_parser',
        row_count: importedCount,
        imported_count: importedCount,
        payload: {
          ...payload,
          command_id: result?.command_id || payload.record_id || '',
          task_id: result?.task_id || '',
          task_status: result?.task_status || result?.status || '',
        },
        updated_at_ms: Date.now(),
      });
      await updateCampaignCounts(campaign.id);
      await loadAll();
      render();
    })
    .catch(async (error) => {
      console.warn('[outbound] CTOX source import failed', error);
      await patchDoc(state.ctx.db.raw.outbound_sources, sourceId, {
        status: 'failed_parser',
        payload: {
          ...payload,
          error: error?.message || String(error),
        },
        updated_at_ms: Date.now(),
      }).catch(() => {});
      await loadAll().catch(() => {});
      render();
    });
}

async function countKnowledgeRowsForSource(refs, sourceId) {
  const rows = await readKnowledgeRows(refs, refs.companiesKey, 10000);
  return rows.filter((row) => row.source_id === sourceId).length;
}

function filterRowsForCampaignImport(campaign, payload, rows) {
  const allowed = importCountryFilter(campaign, payload);
  if (!allowed.length) return rows;
  return rows.filter((row) => {
    const country = normalizeCountryCode(row.country || row.raw?.country || row.raw?.land || '');
    return !country || allowed.includes(country);
  });
}

function importCountryFilter(campaign, payload) {
  const raw = payload?.definition?.import_filter?.company_country_codes
    || campaign?.payload?.import_filter?.company_country_codes
    || ['DE'];
  return Array.isArray(raw)
    ? raw.map(normalizeCountryCode).filter(Boolean)
    : [];
}

function normalizeCountryCode(value) {
  const text = String(value || '').trim().toLowerCase();
  if (!text) return '';
  if (['de', 'deu', 'ger', 'germany', 'deutschland', 'bundesrepublik deutschland'].includes(text)) return 'DE';
  if (['at', 'aut', 'austria', 'österreich', 'oesterreich'].includes(text)) return 'AT';
  if (['ch', 'che', 'switzerland', 'schweiz', 'suisse'].includes(text)) return 'CH';
  return text.toUpperCase();
}

function companyKnowledgeRow(campaign, sourceId, row, now) {
  return {
    company_id: companyIdFromImportRow(campaign, row),
    campaign_id: campaign.id,
    campaign_name: campaign.name,
    source_id: sourceId,
    row_index: row.row_index || 0,
    company_name: row.name || '',
    website: row.website || '',
    domain: row.domain || '',
    city: row.city || '',
    country: row.country || '',
    qualification_status: 'new',
    research_status: 'pending',
    pipeline_status: 'not_started',
    fit_score: 0,
    fit_status: 'unqualified',
    imported_at_ms: now,
    updated_at_ms: now,
    raw_json: JSON.stringify(row.raw || row),
  };
}

function companyIdFromImportRow(campaign, row) {
  return `co_${fingerprint(companyIdentityKey({
    campaign_id: campaign.id,
    name: row.name || '',
    domain: row.domain || '',
    website: row.website || '',
    country: row.country || '',
  }))}`;
}

function companyStatusKnowledgeRow(campaign, company, now) {
  return {
    company_id: company.id,
    campaign_id: campaign.id,
    campaign_name: campaign.name,
    source_id: company.source_id || '',
    row_index: company.row_index || 0,
    company_name: company.name || '',
    website: company.website || '',
    domain: company.domain || domainFromUrl(company.website),
    city: company.city || '',
    country: company.country || '',
    qualification_status: company.qualification_status || 'new',
    research_status: company.research_status || 'pending',
    pipeline_status: company.pipeline_status || 'not_started',
    fit_score: Number(company.fit_score || 0),
    fit_status: company.fit_status || 'unqualified',
    company_data_json: JSON.stringify(company.company_data || {}),
    evidence_json: JSON.stringify(company.evidence || []),
    updated_at_ms: now,
    raw_json: JSON.stringify(company.payload?.imported_row || {}),
  };
}

function pipelineSeedKnowledgeRow(campaign, company, pipelineItem, now) {
  return {
    pipeline_id: pipelineItem.id,
    company_id: company.id,
    campaign_id: campaign.id,
    campaign_name: campaign.name,
    company_name: company.name || '',
    stage: pipelineItem.stage || 'contact_research',
    contact_research_status: pipelineItem.contact_research_status || 'pending',
    outreach_status: pipelineItem.outreach_status || 'not_started',
    priority: pipelineItem.priority || 'normal',
    updated_at_ms: now,
  };
}

function researchWritebackContract(campaign, refs, stage, record, fields = []) {
  const base = {
    system: 'ctox knowledge data',
    mode: 'append_latest_row',
    domain: refs.domain,
    runbook_id: refs.runbookId,
    campaign_id: campaign.id,
    campaign_name: campaign.name,
    rule: 'Append a new row with the same stable id and updated_at_ms. Do not create a separate datastore. Do not research persons before the pipeline contact stage.',
  };
  if (stage === 'company_research') {
    return {
      ...base,
      table_key: refs.companiesKey,
      stable_id_column: 'company_id',
      stable_id_value: record.id,
      required_columns: [
        'company_id',
        'campaign_id',
        'company_name',
        'website',
        'domain',
        'qualification_status',
        'research_status',
        'fit_score',
        'fit_status',
        'company_data_json',
        'evidence_json',
        'updated_at_ms',
      ],
      requested_fields: fields.map((field) => ({ id: field.id, label: field.label })),
      append_command: `ctox knowledge data append --domain ${refs.domain} --key ${refs.companiesKey} --rows '<json-array>'`,
    };
  }
  if (stage === 'contact_research' || stage === 'lead_qualification') {
    return {
      ...base,
      table_key: refs.contactsKey,
      stable_id_column: 'pipeline_id',
      stable_id_value: record.id,
      company_id: record.company_id,
      required_columns: [
        'pipeline_id',
        'company_id',
        'campaign_id',
        'company_name',
        'contact_id',
        'contact_name',
        'role',
        'email',
        'linkedin_url',
        'contact_research_status',
        'lead_status',
        'evidence_json',
        'updated_at_ms',
      ],
      append_command: `ctox knowledge data append --domain ${refs.domain} --key ${refs.contactsKey} --rows '<json-array>'`,
    };
  }
  return base;
}

async function queueOutboundImportCommand(campaign, payload, sourceId) {
  const refs = await ensureCampaignKnowledge(campaign).catch(() => campaignKnowledgeRefs(campaign));
  const commandId = payload.record_id || `import_${Date.now()}_${crypto.randomUUID()}`;
  const command = {
    id: commandId,
    module: 'outbound',
    type: 'outbound.source.import',
    record_id: commandId,
    inbound_channel: 'business_os.outbound',
    payload: {
      ...payload,
      source_id: sourceId,
      instruction: [
        'Lies den Importjob als Firmenliste, folge bei Listen/Verzeichnissen den noetigen Unternehmensdetailseiten und schreibe alle gefundenen Unternehmen als record-shaped Knowledge in den Campaign Companies DataFrame. Keine Personen recherchieren.',
        payload.filter_prompt ? `Importfilter strikt anwenden: ${payload.filter_prompt}` : '',
      ].filter(Boolean).join('\n'),
      writeback_contract: {
        system: 'ctox knowledge data',
        mode: 'append_rows',
        domain: refs.domain,
        table_key: refs.companiesKey,
        runbook_id: refs.runbookId,
        campaign_id: campaign.id,
        source_id: sourceId,
        filter_prompt: payload.filter_prompt || '',
        required_columns: [
          'company_id',
          'campaign_id',
          'source_id',
          'company_name',
          'website',
          'domain',
          'city',
          'country',
          'qualification_status',
          'research_status',
          'pipeline_status',
          'imported_at_ms',
          'updated_at_ms',
          'raw_json',
        ],
        append_command: `ctox knowledge data append --domain ${refs.domain} --key ${refs.companiesKey} --rows '<json-array>'`,
      },
      knowledge: {
        domain: refs.domain,
        companies_table_key: refs.companiesKey,
        contacts_table_key: refs.contactsKey,
        runs_table_key: refs.runsKey,
        runbook_id: refs.runbookId,
      },
    },
    client_context: {
      source_module: 'outbound',
      entity_type: 'company_source',
      campaign_id: campaign.id,
      source_id: sourceId,
      writeback_required: true,
      knowledge_domain: refs.domain,
      knowledge_table_key: refs.companiesKey,
    },
  };
  return state.ctx.commandBus.dispatch(command);
}

function extractRowsFromPayload(payload) {
  if (payload.source_type === 'text') {
    return extractCompanyRowsFromText(payload.source?.text || '');
  }
  if (payload.source_type === 'url') {
    return [];
  }
  const rows = [];
  for (const file of payload.source?.files || []) {
    const text = file.text || decodeBase64Utf8(file.base64 || '');
    if (!text || !/\.(csv|tsv|txt)$/i.test(file.name)) continue;
    rows.push(...parseDelimitedText(text).map((row, index) => normalizeCompanyRow(row, index)));
  }
  return rows.filter((row) => row.name);
}

async function queueCompanyResearch(companyId, options = {}) {
  const company = state.companies.find((item) => item.id === companyId);
  const campaign = options.forceCampaign || selectedCampaign();
  if (!company || !campaign) return;
  const refs = await ensureCampaignKnowledge(campaign).catch(() => campaignKnowledgeRefs(campaign));
  const researchSettings = options.forceSettings || getCampaignResearchSettings(campaign);
  const researchFields = options.fieldsOverride || researchFieldsForPrompt(researchSettings);
  const runId = `run_${crypto.randomUUID()}`;
  const command = {
    id: `cmd_${runId}`,
    module: 'outbound',
    type: 'outbound.company.research',
    record_id: company.id,
    inbound_channel: 'business_os.outbound',
    payload: {
      title: `Unternehmensdaten recherchieren: ${company.name}`,
      instruction: options.reason === 'custom_fields_added'
        ? 'Recherchiere die neu hinzugefuegten Unternehmensdaten-Kategorien fuer die Outbound-Qualifizierung. Schreibe das Ergebnis ausschliesslich in den angegebenen Knowledge DataFrame. Keine Personen adressieren und keine Outreach-Nachricht erstellen.'
        : 'Recherchiere nur Unternehmensdaten fuer die Outbound-Qualifizierung. Schreibe das Ergebnis ausschliesslich in den angegebenen Knowledge DataFrame. Keine Personen adressieren und keine Outreach-Nachricht erstellen.',
      research_request: {
        tool: 'ctox web research',
        mode: 'new_record',
        company: company.name,
        country: company.country || 'DE',
        fields: researchFields,
        custom_instruction: [
          researchSettings.customInstruction,
          options.reason === 'custom_fields_added' ? 'Nur fehlende oder neue Kategorien nachrecherchieren und bestehende Unternehmensdaten nicht entfernen.' : '',
        ].filter(Boolean).join('\n'),
        include_private: [],
      },
      company: serializeCompany(company),
      campaign: { id: campaign.id, name: campaign.name, objective: campaign.objective },
      writeback_contract: researchWritebackContract(campaign, refs, 'company_research', company, researchFields),
      knowledge: {
        domain: refs.domain,
        companies_table_key: refs.companiesKey,
        runs_table_key: refs.runsKey,
        runbook_id: refs.runbookId,
      },
    },
    client_context: {
      source_module: 'outbound',
      campaign_id: campaign.id,
      company_id: company.id,
      research_boundary: 'company_only_before_pipeline',
      writeback_required: true,
      knowledge_domain: refs.domain,
      knowledge_table_key: refs.companiesKey,
      knowledge_runbook_id: refs.runbookId,
    },
  };
  const result = await state.ctx.commandBus.dispatch(command).catch((error) => ({ ok: false, status: 'pending_sync_failed', error: error?.message || String(error) }));
  const now = Date.now();
  await state.ctx.db.raw.outbound_research_runs.insert({
    id: runId,
    campaign_id: campaign.id,
    company_id: company.id,
    pipeline_id: '',
    run_type: 'company_research',
    status: result.status || 'queued',
    command_id: result.command_id || command.id,
    request: command.payload.research_request,
    result: {},
    error: result.error || '',
    created_at_ms: now,
    updated_at_ms: now,
  });
  await appendKnowledgeRows(campaign, refs.runsKey, [{
    run_id: runId,
    command_id: result.command_id || command.id,
    campaign_id: campaign.id,
    record_id: company.id,
    run_type: 'company_research',
    status: result.status || 'queued',
    ctox_status: result.status || 'pending_sync',
    created_at_ms: now,
    request_json: JSON.stringify(command.payload.research_request),
  }]).catch((error) => console.warn('[outbound] knowledge runs append failed', error));
  await patchDoc(state.ctx.db.raw.outbound_companies, company.id, {
    research_status: result.status || 'pending_sync',
    updated_at_ms: now,
  });
}

async function queueContactResearch(pipelineId) {
  const item = state.pipeline.find((entry) => entry.id === pipelineId);
  if (!item) return;
  const campaign = state.campaigns.find((entry) => entry.id === item.campaign_id);
  const researchSettings = getCampaignResearchSettings(campaign);
  const contactFields = contactFieldsForPrompt(researchSettings);
  const refs = campaign
    ? await ensureCampaignKnowledge(campaign).catch(() => campaignKnowledgeRefs(campaign))
    : campaignKnowledgeRefs({ id: item.campaign_id, name: item.company_name });
  const runId = `run_${crypto.randomUUID()}`;
  const command = {
    id: `cmd_${runId}`,
    module: 'outbound',
    type: 'outbound.pipeline.contact_research',
    record_id: item.id,
    inbound_channel: 'business_os.outbound',
    payload: {
      title: `Ansprechpartner recherchieren: ${item.company_name}`,
      instruction: 'Jetzt beginnt die Pipeline-Stufe: Recherchiere relevante oeffentlich belegbare Ansprechpartner und Rollen fuer eine spaetere Ansprache. Schreibe Kontakte ausschliesslich in den angegebenen Knowledge Contacts DataFrame.',
      company_id: item.company_id,
      pipeline_id: item.id,
      contact_fields: contactFields,
      custom_instruction: researchSettings.customInstruction || '',
      writeback_contract: researchWritebackContract(campaign || { id: item.campaign_id, name: item.company_name }, refs, 'contact_research', item),
      knowledge: {
        domain: refs.domain,
        contacts_table_key: refs.contactsKey,
        runs_table_key: refs.runsKey,
        runbook_id: refs.runbookId,
      },
    },
    client_context: {
      source_module: 'outbound',
      campaign_id: item.campaign_id,
      pipeline_id: item.id,
      research_boundary: 'pipeline_contact_stage',
      writeback_required: true,
      knowledge_domain: refs.domain,
      knowledge_table_key: refs.contactsKey,
      knowledge_runbook_id: refs.runbookId,
    },
  };
  const result = await state.ctx.commandBus.dispatch(command).catch((error) => ({ ok: false, status: 'pending_sync_failed', error: error?.message || String(error) }));
  const now = Date.now();
  await state.ctx.db.raw.outbound_research_runs.insert({
    id: runId,
    campaign_id: item.campaign_id,
    company_id: item.company_id,
    pipeline_id: item.id,
    run_type: 'contact_research',
    status: result.status || 'queued',
    command_id: result.command_id || command.id,
    request: command.payload,
    result: {},
    error: result.error || '',
    created_at_ms: now,
    updated_at_ms: now,
  });
  await appendKnowledgeRows(campaign, refs.runsKey, [{
    run_id: runId,
    command_id: result.command_id || command.id,
    campaign_id: item.campaign_id,
    record_id: item.id,
    company_id: item.company_id,
    run_type: 'contact_research',
    status: result.status || 'queued',
    ctox_status: result.status || 'pending_sync',
    created_at_ms: now,
    request_json: JSON.stringify(command.payload),
  }]).catch((error) => console.warn('[outbound] knowledge runs append failed', error));
  await patchDoc(state.ctx.db.raw.outbound_pipeline_items, item.id, {
    contact_research_status: result.status || 'pending_sync',
    updated_at_ms: now,
  });
}

async function queueLeadQualification(pipelineId) {
  const item = state.pipeline.find((entry) => entry.id === pipelineId);
  const campaign = state.campaigns.find((entry) => entry.id === item?.campaign_id);
  if (!item || !campaign) return;
  const researchSettings = getCampaignResearchSettings(campaign);
  const contactFields = contactFieldsForPrompt(researchSettings);
  const refs = await ensureCampaignKnowledge(campaign).catch(() => campaignKnowledgeRefs(campaign));
  const runId = `run_${crypto.randomUUID()}`;
  const command = {
    id: `cmd_${runId}`,
    module: 'outbound',
    type: 'outbound.pipeline.lead_qualification',
    record_id: item.id,
    inbound_channel: 'business_os.outbound',
    payload: {
      title: `Lead qualifizieren: ${item.company_name}`,
      instruction: 'Qualifiziere die recherchierten Ansprechpartner gegen den Campaign Scope/ICP. Schreibe die Lead-Qualifikation ausschliesslich in den Knowledge Contacts DataFrame. Keine Outreach-Nachricht senden.',
      company_id: item.company_id,
      pipeline_id: item.id,
      campaign: { id: campaign.id, name: campaign.name, objective: campaign.objective, scope: campaign.payload?.scope || '' },
      contacts: item.contacts || [],
      contact_fields: contactFields,
      custom_instruction: researchSettings.customInstruction || '',
      qualification_goal: 'lead_qualified_or_rejected',
      writeback_contract: researchWritebackContract(campaign, refs, 'lead_qualification', item),
      knowledge: {
        domain: refs.domain,
        contacts_table_key: refs.contactsKey,
        runs_table_key: refs.runsKey,
        runbook_id: refs.runbookId,
      },
    },
    client_context: {
      source_module: 'outbound',
      campaign_id: item.campaign_id,
      pipeline_id: item.id,
      research_boundary: 'lead_qualification_stage',
      writeback_required: true,
      knowledge_domain: refs.domain,
      knowledge_table_key: refs.contactsKey,
      knowledge_runbook_id: refs.runbookId,
    },
  };
  const result = await state.ctx.commandBus.dispatch(command).catch((error) => ({ ok: false, status: 'pending_sync_failed', error: error?.message || String(error) }));
  const now = Date.now();
  await state.ctx.db.raw.outbound_research_runs.insert({
    id: runId,
    campaign_id: item.campaign_id,
    company_id: item.company_id,
    pipeline_id: item.id,
    run_type: 'lead_qualification',
    status: result.status || 'queued',
    command_id: result.command_id || command.id,
    request: command.payload,
    result: {},
    error: result.error || '',
    created_at_ms: now,
    updated_at_ms: now,
  });
  await appendKnowledgeRows(campaign, refs.runsKey, [{
    run_id: runId,
    command_id: result.command_id || command.id,
    campaign_id: item.campaign_id,
    record_id: item.id,
    company_id: item.company_id,
    run_type: 'lead_qualification',
    status: result.status || 'queued',
    ctox_status: result.status || 'pending_sync',
    created_at_ms: now,
    request_json: JSON.stringify(command.payload),
  }]).catch((error) => console.warn('[outbound] knowledge runs append failed', error));
  await patchDoc(state.ctx.db.raw.outbound_pipeline_items, item.id, {
    outreach_status: result.status || 'pending_sync',
    stage: 'lead_qualification',
    updated_at_ms: now,
  });
}

async function setCompanyQualification(companyId, status) {
  const company = state.companies.find((item) => item.id === companyId);
  const campaign = state.campaigns.find((item) => item.id === company?.campaign_id) || selectedCampaign();
  const now = Date.now();
  await patchDoc(state.ctx.db.raw.outbound_companies, companyId, {
    qualification_status: status,
    fit_status: status === 'qualified' ? 'fit' : status === 'rejected' ? 'not_fit' : 'unqualified',
    fit_score: status === 'qualified' ? 75 : status === 'rejected' ? 10 : 0,
    updated_at_ms: now,
  });
  if (company && campaign) {
    const nextCompany = {
      ...company,
      qualification_status: status,
      fit_status: status === 'qualified' ? 'fit' : status === 'rejected' ? 'not_fit' : 'unqualified',
      fit_score: status === 'qualified' ? 75 : status === 'rejected' ? 10 : 0,
      updated_at_ms: now,
    };
    const refs = campaignKnowledgeRefs(campaign);
    await appendKnowledgeRows(campaign, refs.companiesKey, [companyStatusKnowledgeRow(campaign, nextCompany, now)]).catch((error) => {
      console.warn('[outbound] knowledge company status append failed', error);
    });
  }
  await updateCampaignCounts(campaign?.id || state.selectedCampaignId);
}

async function sendCompanyToPipeline(companyId, options = {}) {
  const company = state.companies.find((item) => item.id === companyId);
  const campaign = options.forceCampaign || selectedCampaign();
  if (!company || !campaign) return;
  if (company.qualification_status !== 'qualified') {
    await setCompanyQualification(company.id, 'qualified');
  }
  const existing = state.pipeline.find((item) => item.company_id === company.id);
  const now = Date.now();
  let pipelineItem = existing;
  if (!existing) {
    pipelineItem = {
      id: `pipe_${crypto.randomUUID()}`,
      campaign_id: campaign.id,
      company_id: company.id,
      company_name: company.name,
      stage: 'contact_research',
      contact_research_status: 'pending',
      outreach_status: 'not_started',
      priority: company.fit_score >= 80 ? 'high' : 'normal',
      contacts: [],
      payload: { company_snapshot: serializeCompany(company) },
      created_at_ms: now,
      updated_at_ms: now,
    };
    await state.ctx.db.raw.outbound_pipeline_items.insert(pipelineItem);
  }
  await patchDoc(state.ctx.db.raw.outbound_companies, company.id, {
    pipeline_status: 'pipeline',
    qualification_status: 'qualified',
    updated_at_ms: now,
  });
  const refs = campaignKnowledgeRefs(campaign);
  const nextCompany = { ...company, pipeline_status: 'pipeline', qualification_status: 'qualified', updated_at_ms: now };
  await Promise.all([
    appendKnowledgeRows(campaign, refs.companiesKey, [companyStatusKnowledgeRow(campaign, nextCompany, now)]),
    pipelineItem ? appendKnowledgeRows(campaign, refs.contactsKey, [pipelineSeedKnowledgeRow(campaign, nextCompany, pipelineItem, now)]) : Promise.resolve(null),
  ]).catch((error) => {
    console.warn('[outbound] knowledge pipeline append failed', error);
  });
  if (!options.keepView) state.activeView = 'pipeline';
  await updateCampaignCounts(campaign.id);
}

async function updateCampaignCounts(campaignId) {
  const raw = state.ctx?.db?.raw || {};
  const companies = dedupeCompanies((await findAll(raw.outbound_companies)).filter((item) => item.campaign_id === campaignId));
  const sources = (await findAll(raw.outbound_sources)).filter((item) => item.campaign_id === campaignId);
  const pipeline = (await findAll(raw.outbound_pipeline_items)).filter((item) => item.campaign_id === campaignId);
  await patchDoc(state.ctx.db.raw.outbound_campaigns, campaignId, {
    source_count: sources.length,
    company_count: companies.length,
    qualified_count: companies.filter((item) => item.qualification_status === 'qualified').length,
    pipeline_count: pipeline.length,
    updated_at_ms: Date.now(),
  });
}

function selectedCampaign() {
  return state.campaigns.find((item) => item.id === state.selectedCampaignId) || visibleCampaigns()[0] || state.campaigns[0] || null;
}

function currentSources() {
  return state.sources.filter((item) => item.campaign_id === state.selectedCampaignId);
}

function currentCompanies() {
  return dedupeCompanies(state.companies.filter((item) => item.campaign_id === state.selectedCampaignId));
}

function currentPipeline() {
  return dedupePipelineItems(state.pipeline.filter((item) => item.campaign_id === state.selectedCampaignId));
}

function selectedCompany() {
  return currentCompanies().find((item) => item.id === state.selectedCompanyId || item.duplicate_company_ids?.includes(state.selectedCompanyId)) || currentCompanies()[0] || null;
}

function selectedPipelineItem() {
  return state.pipeline.find((item) => item.id === state.selectedPipelineId) || currentPipeline()[0] || null;
}

function filteredCompanies() {
  return filteredQualificationRows().map((row) => row.company);
}

function filteredQualificationRows() {
  const search = state.search.trim().toLowerCase();
  const rows = currentCompanies().map((company) => ({
    company,
    item: pipelineItemForCompany(company),
  })).filter((row) => {
    const { company, item } = row;
    if (search && !`${company.name} ${company.domain} ${company.city}`.toLowerCase().includes(search)) return false;
    if (state.filter === 'research' && !(company.research_status === 'pending' || company.research_status === 'queued')) return false;
    if (state.filter === 'qualified' && company.qualification_status !== 'qualified') return false;
    if (state.filter === 'contact_qualified' && !isContactQualified(item)) return false;
    if (state.filter === 'lead_qualified' && !isLeadQualified(item)) return false;
    if (state.filter === 'pipeline' && company.pipeline_status !== 'pipeline') return false;
    if (state.filter === 'rejected' && company.qualification_status !== 'rejected') return false;
    return rowMatchesTableFilters(row);
  });
  return sortQualificationRows(rows);
}

function rowMatchesTableFilters(row) {
  const filters = Object.entries(state.tableFilters || {}).filter(([, value]) => String(value || '').trim());
  if (!filters.length) return true;
  return filters.every(([columnId, value]) => normalizeFilterText(tableColumnValue(row, columnId)).includes(normalizeFilterText(value)));
}

function sortQualificationRows(rows) {
  const sort = state.tableSort;
  if (!sort?.column || !sort?.direction) return rows;
  const direction = sort.direction === 'desc' ? -1 : 1;
  return [...rows].sort((a, b) => compareTableValues(tableColumnValue(a, sort.column), tableColumnValue(b, sort.column)) * direction);
}

function tableColumnValue(row, columnId) {
  if (!row) return '';
  if (columnId === 'company.name') return row.company.name;
  if (columnId === 'company.domain') return row.company.domain || domainFromUrl(row.company.website) || '';
  if (columnId?.startsWith('field.')) return companyResearchValue(row.company, columnId.slice(6));
  if (columnId === 'company.qualification') return labelQualification(row.company.qualification_status);
  if (columnId?.startsWith('contact.') || columnId?.startsWith('lead.')) return contactColumnValue(row.item, columnId);
  return '';
}

function setTableSort(column) {
  if (column === 'contact.action') return;
  if (state.tableSort?.column !== column) {
    state.tableSort = { column, direction: 'asc' };
    return;
  }
  if (state.tableSort.direction === 'asc') {
    state.tableSort = { column, direction: 'desc' };
    return;
  }
  state.tableSort = null;
}

function hasTableControls() {
  return Object.keys(state.tableFilters || {}).length > 0 || !!state.tableSort;
}

function renderCenterPreservingInput(input) {
  const filterKey = input.dataset.tableFilter || '';
  const isSearch = input.matches('[data-search]');
  const selectionStart = input.selectionStart;
  const selectionEnd = input.selectionEnd;
  renderCenter();
  const selector = isSearch ? '[data-search]' : `[data-table-filter="${cssEscape(filterKey)}"]`;
  const next = state.ctx?.host?.querySelector(selector);
  next?.focus?.();
  if (next && selectionStart != null && selectionEnd != null) next.setSelectionRange?.(selectionStart, selectionEnd);
}

function scheduleCenterRenderPreservingInput(input) {
  const filterKey = input.dataset.tableFilter || '';
  const isSearch = input.matches('[data-search]');
  const selectionStart = input.selectionStart;
  const selectionEnd = input.selectionEnd;
  const selector = isSearch ? '[data-search]' : `[data-table-filter="${cssEscape(filterKey)}"]`;
  if (state.centerRenderTimer) window.clearTimeout(state.centerRenderTimer);
  state.centerRenderTimer = window.setTimeout(() => {
    state.centerRenderTimer = null;
    renderCenter();
    const next = state.ctx?.host?.querySelector(selector);
    next?.focus?.();
    if (next && selectionStart != null && selectionEnd != null) next.setSelectionRange?.(selectionStart, selectionEnd);
  }, 120);
}

function compareTableValues(a, b) {
  const numericA = parseTableNumber(a);
  const numericB = parseTableNumber(b);
  if (Number.isFinite(numericA) && Number.isFinite(numericB)) return numericA - numericB;
  return normalizeFilterText(a).localeCompare(normalizeFilterText(b), 'de', { numeric: true, sensitivity: 'base' });
}

function parseTableNumber(value) {
  const text = String(value ?? '').trim().replace(/\s+/g, '');
  if (!text) return Number.NaN;
  const match = text.match(/^-?\d+(?:[.,]\d+)?/);
  if (!match) return Number.NaN;
  return Number(match[0].replace(',', '.'));
}

function normalizeFilterText(value) {
  return String(value ?? '').toLowerCase().normalize('NFD').replace(/[\u0300-\u036f]/g, '').trim();
}

function ensureSelectedCompanyInFilter() {
  const companies = filteredCompanies();
  if (!companies.some((company) => company.id === state.selectedCompanyId)) {
    state.selectedCompanyId = companies[0]?.id || '';
  }
}

async function findAll(collection) {
  if (!collection) return [];
  const docs = await collection.find().exec();
  return docs.map((doc) => doc.toJSON ? doc.toJSON() : doc).sort((a, b) => (b.updated_at_ms || 0) - (a.updated_at_ms || 0));
}

async function upsertDoc(collection, id, doc) {
  const existing = await collection.findOne(id).exec();
  if (existing) {
    await existing.incrementalPatch({ ...doc, created_at_ms: existing.created_at_ms || doc.created_at_ms });
    return;
  }
  await collection.insert(doc);
}

async function patchDoc(collection, id, patch) {
  const existing = await collection.findOne(id).exec();
  if (existing) await existing.incrementalPatch(patch);
}

async function removeDoc(collection, id) {
  const existing = await collection?.findOne(id).exec();
  if (existing) await existing.remove();
}

async function removeWhere(collection, predicate) {
  if (!collection) return;
  const docs = await collection.find().exec();
  for (const doc of docs) {
    const json = doc.toJSON ? doc.toJSON() : doc;
    if (predicate(json)) await doc.remove();
  }
}

function field(label, value) {
  return `<div><span class="outbound-field-label">${escapeHtml(label)}</span><div>${escapeHtml(value || '')}</div></div>`;
}

function renderEvidence(company) {
  const evidence = company.evidence || [];
  if (!evidence.length) return '<div class="outbound-muted">Noch keine belastbaren Quellen hinterlegt.</div>';
  return `<ul class="outbound-evidence">${evidence.map((item) => `<li>${escapeHtml(item.title || item.url || item.note || 'Quelle')}</li>`).join('')}</ul>`;
}

function labelResearch(value) {
  if (isAutomationActive(value) || isFailedStatus(value) || isCancelledStatus(value)) return automationStatusLabel(value);
  if (value === 'researched') return 'recherchiert';
  return 'offen';
}

function labelQualification(value) {
  if (value === 'qualified') return 'qualifiziert';
  if (value === 'rejected') return 'nicht passend';
  return 'neu';
}

function labelPipeline(value) {
  if (value === 'pipeline') return 'Pipeline';
  return 'nicht gestartet';
}

function serializeCompany(company) {
  return {
    id: company.id,
    name: company.name,
    website: company.website,
    domain: company.domain,
    city: company.city,
    country: company.country,
    company_data: company.company_data || {},
  };
}

function domainFromUrl(value) {
  try {
    const input = String(value || '').trim();
    if (!input) return '';
    return new URL(input.startsWith('http') ? input : `https://${input}`).hostname.replace(/^www\./, '');
  } catch {
    return '';
  }
}

function fingerprint(value) {
  let hash = 0;
  const text = String(value || '');
  for (let index = 0; index < text.length; index += 1) {
    hash = ((hash << 5) - hash + text.charCodeAt(index)) | 0;
  }
  return Math.abs(hash).toString(36);
}

function setupOutboundColumnResizing() {
  const root = state.ctx?.host?.querySelector?.('[data-outbound-root]');
  if (!root) return null;

  const handle = document.createElement('div');
  handle.className = 'outbound-col-resizer';
  handle.setAttribute('role', 'separator');
  handle.setAttribute('aria-orientation', 'vertical');
  handle.setAttribute('aria-label', 'Spaltenbreite anpassen');
  root.append(handle);

  let activeWidths = null;
  let persistedRatios = readOutboundColumnLayout();
  let dragState = null;
  let resizeRaf = 0;

  const applyWidths = (widths) => {
    if (!widths) return;
    root.style.gridTemplateColumns = `${widths.left}px ${widths.center}px`;
    root.dataset.leftCompact = widths.left <= 360 ? 'true' : 'false';
  };
  const placeHandle = (metrics, widths) => {
    if (!metrics || !widths) return;
    handle.style.left = `${Math.round(widths.left + (metrics.gap / 2) + metrics.padLeft)}px`;
  };
  const persistCurrentLayout = () => {
    const ratios = columnPixelsToRatios(activeWidths);
    if (!ratios) return;
    persistedRatios = ratios;
    writeOutboundColumnLayout(ratios);
  };
  const syncLayout = () => {
    const metrics = getOutboundGridMetrics(root);
    if (!metrics || metrics.trackTotal < OUTBOUND_COL_MIN.left + OUTBOUND_COL_MIN.center) {
      root.style.removeProperty('grid-template-columns');
      delete root.dataset.leftCompact;
      handle.hidden = true;
      return;
    }
    let nextWidths = persistedRatios ? columnRatiosToPixels(persistedRatios, metrics.trackTotal) : null;
    if (!nextWidths) nextWidths = clampOutboundColumns(readOutboundGridTrackPixels(root), metrics.trackTotal);
    if (!nextWidths) return;
    activeWidths = nextWidths;
    applyWidths(activeWidths);
    placeHandle(metrics, activeWidths);
    handle.hidden = false;
  };
  const stopDrag = () => {
    if (!dragState) return;
    dragState = null;
    handle.classList.remove('is-active');
    document.body.classList.remove('is-outbound-col-resizing');
    persistCurrentLayout();
  };
  const startDrag = (event) => {
    const metrics = getOutboundGridMetrics(root);
    if (!metrics || metrics.trackTotal < OUTBOUND_COL_MIN.left + OUTBOUND_COL_MIN.center) return;
    const initial = activeWidths || clampOutboundColumns(readOutboundGridTrackPixels(root), metrics.trackTotal);
    if (!initial) return;
    activeWidths = initial;
    dragState = {
      appRect: root.getBoundingClientRect(),
      metrics,
      widths: { ...initial },
    };
    handle.classList.add('is-active');
    document.body.classList.add('is-outbound-col-resizing');
    event.preventDefault();
  };
  const handleDragMove = (event) => {
    if (!dragState) return;
    const { appRect, metrics } = dragState;
    const pointerX = event.clientX - appRect.left - metrics.padLeft;
    const rawLeft = clampNumber(pointerX - (metrics.gap / 2), OUTBOUND_COL_MIN.left, metrics.trackTotal - OUTBOUND_COL_MIN.center);
    const left = clampNumber(rawLeft, OUTBOUND_COL_MIN.left, Math.min(OUTBOUND_COL_LEFT_MAX, metrics.trackTotal - OUTBOUND_COL_MIN.center));
    activeWidths = clampOutboundColumns({ left, center: metrics.trackTotal - left }, metrics.trackTotal);
    if (!activeWidths) return;
    applyWidths(activeWidths);
    placeHandle(metrics, activeWidths);
  };
  const handleResize = () => {
    if (resizeRaf) cancelAnimationFrame(resizeRaf);
    resizeRaf = requestAnimationFrame(() => {
      resizeRaf = 0;
      syncLayout();
    });
  };

  handle.addEventListener('pointerdown', startDrag);
  window.addEventListener('pointermove', handleDragMove);
  window.addEventListener('pointerup', stopDrag);
  window.addEventListener('pointercancel', stopDrag);
  window.addEventListener('blur', stopDrag);
  window.addEventListener('resize', handleResize);
  syncLayout();

  return () => {
    if (resizeRaf) cancelAnimationFrame(resizeRaf);
    window.removeEventListener('pointermove', handleDragMove);
    window.removeEventListener('pointerup', stopDrag);
    window.removeEventListener('pointercancel', stopDrag);
    window.removeEventListener('blur', stopDrag);
    window.removeEventListener('resize', handleResize);
    document.body.classList.remove('is-outbound-col-resizing');
    delete root.dataset.leftCompact;
    handle.remove();
  };
}

function setupCenterSplitResizing(root) {
  state.centerResizeCleanup?.();
  state.centerResizeCleanup = null;
  const split = root?.querySelector?.('[data-outbound-center-split]');
  const handle = root?.querySelector?.('[data-outbound-center-resizer]');
  if (!split || !handle) return;

  let activeWidths = null;
  let persistedRatio = readCenterSplitRatio();
  let dragState = null;
  let resizeRaf = 0;

  const applyWidths = (widths) => {
    if (!widths) return;
    split.style.gridTemplateColumns = `${widths.left}px 12px ${widths.right}px`;
  };

  const readCurrentWidths = () => {
    const tracks = String(getComputedStyle(split).gridTemplateColumns || '')
      .split(/\s+/)
      .map((part) => Number.parseFloat(part))
      .filter((number) => Number.isFinite(number) && number > 0);
    if (tracks.length < 3) return null;
    return { left: tracks[0], right: tracks[2] };
  };

  const clampWidths = (left, total) => {
    if (!Number.isFinite(total) || total < OUTBOUND_CENTER_MIN.left + OUTBOUND_CENTER_MIN.right) return null;
    const safeLeft = Math.round(clampNumber(left, OUTBOUND_CENTER_MIN.left, total - OUTBOUND_CENTER_MIN.right));
    return { left: safeLeft, right: Math.round(total - safeLeft) };
  };

  const totalWidth = () => {
    const cs = getComputedStyle(split);
    const gap = Number.parseFloat(cs.columnGap || cs.gap || '0') || 0;
    return Math.max(0, split.clientWidth - 12 - (gap * 2));
  };

  const syncLayout = () => {
    const total = totalWidth();
    if (total < OUTBOUND_CENTER_MIN.left + OUTBOUND_CENTER_MIN.right) {
      split.style.gridTemplateColumns = 'minmax(0, 1fr)';
      handle.hidden = true;
      return;
    }
    const current = readCurrentWidths();
    const left = persistedRatio ? total * persistedRatio : current?.left || total * 0.56;
    activeWidths = clampWidths(left, total);
    applyWidths(activeWidths);
    handle.hidden = false;
  };

  const stopDrag = () => {
    if (!dragState) return;
    dragState = null;
    handle.classList.remove('is-active');
    document.body.classList.remove('is-outbound-center-resizing');
    if (activeWidths) {
      const total = activeWidths.left + activeWidths.right;
      persistedRatio = total > 0 ? activeWidths.left / total : persistedRatio;
      writeCenterSplitRatio(persistedRatio);
    }
  };

  const startDrag = (event) => {
    const total = totalWidth();
    const initial = activeWidths || clampWidths(readCurrentWidths()?.left || total * 0.56, total);
    if (!initial) return;
    activeWidths = initial;
    dragState = {
      rect: split.getBoundingClientRect(),
      total,
    };
    handle.classList.add('is-active');
    document.body.classList.add('is-outbound-center-resizing');
    event.preventDefault();
  };

  const handleMove = (event) => {
    if (!dragState) return;
    const left = event.clientX - dragState.rect.left - 6;
    activeWidths = clampWidths(left, dragState.total);
    applyWidths(activeWidths);
  };

  const handleResize = () => {
    if (resizeRaf) cancelAnimationFrame(resizeRaf);
    resizeRaf = requestAnimationFrame(() => {
      resizeRaf = 0;
      syncLayout();
    });
  };

  handle.addEventListener('pointerdown', startDrag);
  window.addEventListener('pointermove', handleMove);
  window.addEventListener('pointerup', stopDrag);
  window.addEventListener('pointercancel', stopDrag);
  window.addEventListener('blur', stopDrag);
  window.addEventListener('resize', handleResize);
  syncLayout();

  state.centerResizeCleanup = () => {
    if (resizeRaf) cancelAnimationFrame(resizeRaf);
    window.removeEventListener('pointermove', handleMove);
    window.removeEventListener('pointerup', stopDrag);
    window.removeEventListener('pointercancel', stopDrag);
    window.removeEventListener('blur', stopDrag);
    window.removeEventListener('resize', handleResize);
    document.body.classList.remove('is-outbound-center-resizing');
  };
}

function readCenterSplitRatio() {
  try {
    const value = Number(window.localStorage.getItem(OUTBOUND_CENTER_SPLIT_KEY));
    return Number.isFinite(value) && value > 0.25 && value < 0.78 ? value : null;
  } catch {
    return null;
  }
}

function writeCenterSplitRatio(value) {
  try {
    if (Number.isFinite(value)) window.localStorage.setItem(OUTBOUND_CENTER_SPLIT_KEY, String(value));
  } catch {
    // Ignore unavailable storage.
  }
}

function getOutboundGridMetrics(root) {
  if (!root) return null;
  const cs = getComputedStyle(root);
  const gap = Number.parseFloat(cs.columnGap || cs.gap || '0') || 0;
  const padLeft = Number.parseFloat(cs.paddingLeft || '0') || 0;
  const padRight = Number.parseFloat(cs.paddingRight || '0') || 0;
  const contentWidth = Math.max(0, root.clientWidth - padLeft - padRight);
  const trackTotal = Math.max(0, contentWidth - gap);
  return { gap, padLeft, contentWidth, trackTotal };
}

function readOutboundGridTrackPixels(root) {
  if (!root) return null;
  const tracks = String(getComputedStyle(root).gridTemplateColumns || '')
    .split(/\s+/)
    .map((part) => Number.parseFloat(part))
    .filter((number) => Number.isFinite(number) && number > 0);
  if (tracks.length < 2) return null;
  return { left: tracks[0], center: tracks[1] };
}

function clampOutboundColumns(widths, trackTotal) {
  if (!widths || !Number.isFinite(trackTotal) || trackTotal <= 0) return null;
  if (trackTotal < OUTBOUND_COL_MIN.left + OUTBOUND_COL_MIN.center) return null;
  const maxLeft = Math.max(OUTBOUND_COL_MIN.left, Math.min(OUTBOUND_COL_LEFT_MAX, trackTotal - OUTBOUND_COL_MIN.center));
  const left = Math.round(clampNumber(Number(widths.left) || OUTBOUND_COL_MIN.left, OUTBOUND_COL_MIN.left, maxLeft));
  const center = Math.round(trackTotal - left);
  if (center < OUTBOUND_COL_MIN.center) return null;
  return { left, center };
}

function columnPixelsToRatios(widths) {
  if (!widths) return null;
  const left = Number(widths.left) || 0;
  const center = Number(widths.center) || 0;
  const sum = left + center;
  if (sum <= 0) return null;
  return {
    left: Number((left / sum).toFixed(6)),
    center: Number((center / sum).toFixed(6)),
  };
}

function sanitizeOutboundColumnLayout(raw) {
  if (!raw || typeof raw !== 'object') return null;
  const left = Number(raw.left);
  const center = Number(raw.center);
  if (![left, center].every(Number.isFinite)) return null;
  if (left <= 0 || center <= 0) return null;
  const sum = left + center;
  if (sum <= 0) return null;
  return { left: left / sum, center: center / sum };
}

function columnRatiosToPixels(ratios, trackTotal) {
  const safe = sanitizeOutboundColumnLayout(ratios);
  if (!safe) return null;
  return clampOutboundColumns({
    left: safe.left * trackTotal,
    center: safe.center * trackTotal,
  }, trackTotal);
}

function readOutboundColumnLayout() {
  try {
    return sanitizeOutboundColumnLayout(JSON.parse(window.localStorage.getItem(OUTBOUND_LAYOUT_KEY) || 'null'));
  } catch {
    return null;
  }
}

function writeOutboundColumnLayout(ratios) {
  try {
    window.localStorage.setItem(OUTBOUND_LAYOUT_KEY, JSON.stringify(ratios));
  } catch {
    // Ignore unavailable storage.
  }
}

function clampNumber(value, min, max) {
  return Math.max(min, Math.min(max, value));
}

function cssEscape(value) {
  if (window.CSS?.escape) return window.CSS.escape(value);
  return String(value ?? '').replaceAll('"', '\\"').replaceAll('\\', '\\\\');
}

function slugifyFileName(value) {
  return String(value || 'outbound')
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    || 'outbound';
}

function escapeXml(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&apos;');
}

function escapeHtml(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#039;');
}
