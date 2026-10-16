// Systematische Recherche – Abschluss nach dem Agenten-Turn.
// Treuer Port aus store.rs (4794af03d):
//   load_systematic_research_validation_receipt   :21977-22054
//   systematic_research_receipt_covers_manifest    :22056-22066
//   promote_systematic_research_workspace_outputs  :22068-22335
//   validate_systematic_research_csv               :22337-22709
//   systematic_research_evidence_snapshot          :22711-22818
//   document_source_receipt_is_eligible            :23289-23393 (+ :23180, :23419)
// Dateizugriffe auf den Task-Workspace, Knowledge-Parquet und Snapshot-Dateien
// sind Fähigkeiten (Strang CAPSIG, §16.30/§16.32): ctx.cap.agent.task.workspace.read (Workspace der Aufgabe),
// ctx.cap.knowledge.query({argv}) (nativer Knowledge-Datenspeicher, wie `ctox knowledge data …`; dieselbe
// Kernform nutzt outbound), ctx.cap.dataframe.window (Tabellenzeilen). Katalog = Collection knowledge_tables
// der App knowledge (foreign_collections read, Export von knowledge). Snapshot-Bytes = files-Datensatz (ref).
import { invalid } from './errors.js';
import { canonicalJson, firstStringField, parseCsv, parseUrlParts, sha256Hex } from './util.js';

const fail = (message) => invalid(message);

export function commandDomain(command) {
  return firstStringField(command.payload, ['knowledge_domain', 'domain']) ??
    firstStringField(command.client_context, ['knowledge_domain', 'domain']);
}

export function commandRunId(command) {
  return firstStringField(command.payload, ['research_run_id', 'run_id']) ??
    firstStringField(command.client_context, ['research_run_id', 'run_id']);
}

export function receiptCoversManifest(receipt, manifestSha256) {
  const manifests = receipt?.manifests;
  return (
    (Array.isArray(manifests) && manifests.some((item) => item?.manifest_sha256 === manifestSha256)) ||
    receipt?.manifest?.sha256 === manifestSha256
  );
}

const RECEIPT_CANDIDATES = ['.ctox/systematic-research-validation.json', 'ctox/systematic-research-validation.json'];

export async function loadValidationReceipt(ctx, taskId, manifestSha256, runId, commandId) {
  const failures = [];
  for (const path of RECEIPT_CANDIDATES) {
    const file = await ctx.cap.agent.task.workspace.read({ task_id: taskId, path });
    if (!file) continue;
    let receipt;
    try {
      receipt = JSON.parse(file.text);
    } catch (error) {
      failures.push(`${path}: parse systematic research validation receipt ${path}: ${error.message}`);
      continue;
    }
    const status = typeof receipt?.status === 'string' ? receipt.status
      : typeof receipt?.validation_status === 'string' ? receipt.validation_status : null;
    if (receipt?.schema_version !== 'ctox.systematic-research.validation.v1' || status !== 'pass') {
      failures.push(`${path}: receipt is not a passing systematic-research validation`);
      continue;
    }
    if (receipt.research_run_id !== runId || receipt.research_command_id !== commandId) {
      failures.push(`${path}: receipt is bound to another run or command`);
      continue;
    }
    if (!receiptCoversManifest(receipt, manifestSha256)) {
      failures.push(`${path}: receipt does not cover current manifest sha256`);
      continue;
    }
    return receipt;
  }
  throw fail(
    `systematic research writeback requires a passing validation receipt for the current manifest; checked ${
      failures.length ? failures.join('; ') : 'no receipt files'
    }`,
  );
}

function requireStr(item, key, message) {
  const value = firstStringField(item, [key]);
  if (value === null) throw fail(message);
  return value;
}

export function csvOutputs(command) {
  const outputs = [
    { file_name: 'source_candidates.csv', table_key: 'source_candidates', title: 'Discovery Candidates', require_manifest_evidence: false, require_manifest_claim: false },
    { file_name: 'evidence_points.csv', table_key: 'evidence_points', title: 'Evidence Points', require_manifest_evidence: false, require_manifest_claim: true },
    { file_name: 'evaluation_matrix.csv', table_key: 'evaluation_matrix', title: 'Evaluation Matrix', require_manifest_evidence: false, require_manifest_claim: false },
    { file_name: 'semantic_graph_nodes.csv', table_key: 'semantic_graph_nodes', title: 'Semantic Graph Nodes', require_manifest_evidence: false, require_manifest_claim: false },
    { file_name: 'semantic_graph_edges.csv', table_key: 'semantic_graph_edges', title: 'Semantic Graph Edges', require_manifest_evidence: false, require_manifest_claim: false },
  ];
  const tables = command.payload?.writeback_contract?.dashboard_tables;
  const isObj = tables && typeof tables === 'object' && !Array.isArray(tables);
  if (isObj && Object.hasOwn(tables, 'measured_load_points')) {
    outputs.push({ file_name: 'measured_load_points.csv', table_key: 'measured_load_points', title: 'Measured Load Points', require_manifest_evidence: true, require_manifest_claim: false });
  }
  if (isObj && Object.hasOwn(tables, 'derived_bearing_loads')) {
    outputs.push({ file_name: 'derived_bearing_loads.csv', table_key: 'derived_bearing_loads', title: 'Derived Bearing Loads', require_manifest_evidence: true, require_manifest_claim: true });
  }
  // Promote the verified source registry last. It is the app-facing evidence
  // gate for every dependent table above.
  outputs.push({ file_name: 'source_catalog.csv', table_key: 'source_catalog', title: 'Verified Source Catalog', require_manifest_evidence: true, require_manifest_claim: false });
  return outputs;
}

// Prüft Manifest + Quittung; liefert {eligibleSources, validatedClaims}.
export function validateManifest(manifest, runId, commandId) {
  if (!(manifest?.schema_version === 'ctox.research.evidence.v2' && manifest.research_run_id === runId && manifest.research_command_id === commandId)) {
    throw fail('systematic research manifest binding changed after validation');
  }
  if (!Array.isArray(manifest.evidence)) throw fail('systematic research manifest has no evidence array');
  const eligibleSources = new Map();
  const evidenceById = new Map();
  for (const item of manifest.evidence) {
    const evidenceId = requireStr(item, 'evidence_id', 'systematic research evidence is missing evidence_id');
    const sourceId = requireStr(item, 'source_id', 'systematic research evidence is missing source_id');
    const snapshotId = requireStr(item, 'snapshot_id', 'systematic research evidence is missing snapshot_id');
    const canonicalUrl = requireStr(item, 'canonical_url', 'systematic research evidence is missing canonical_url');
    const snapshotHash = requireStr(item, 'snapshot_sha256', 'systematic research evidence is missing snapshot_sha256');
    eligibleSources.set(sourceId, [canonicalUrl, snapshotHash]);
    evidenceById.set(evidenceId, [sourceId, snapshotId, canonicalUrl, snapshotHash]);
  }
  if (eligibleSources.size === 0) throw fail('systematic research writeback requires eligible manifest evidence');
  if (!Array.isArray(manifest.claims)) throw fail('systematic research manifest has no claims array');
  const validatedClaims = new Map();
  for (const claim of manifest.claims) {
    const claimId = requireStr(claim, 'claim_id', 'systematic research claim is missing claim_id');
    const evidenceId = requireStr(claim, 'evidence_id', 'systematic research claim is missing evidence_id');
    const sourceId = requireStr(claim, 'source_id', 'systematic research claim is missing source_id');
    const snapshotId = requireStr(claim, 'snapshot_id', 'systematic research claim is missing snapshot_id');
    const canonicalUrl = requireStr(claim, 'canonical_url', 'systematic research claim is missing canonical_url');
    const quote = requireStr(claim, 'evidence_quote', 'systematic research claim is missing evidence_quote');
    const expected = evidenceById.get(evidenceId);
    if (!expected) throw fail(`systematic research claim ${claimId} references unknown evidence ${evidenceId}`);
    if (!(sourceId === expected[0] && snapshotId === expected[1] && canonicalUrl === expected[2])) {
      throw fail(`systematic research claim ${claimId} does not match its evidence lineage`);
    }
    validatedClaims.set(claimId, [evidenceId, sourceId, snapshotId, canonicalUrl, expected[3], quote]);
  }
  if (validatedClaims.size === 0) throw fail('systematic research writeback requires validated manifest claims');
  return { eligibleSources, validatedClaims };
}

const normalizedHash = (value) => {
  const t = value.trim();
  return t.startsWith('sha256:') ? t.slice(7) : t;
};
const hashEq = (a, b) => normalizedHash(a).toLowerCase() === normalizedHash(b).toLowerCase();
const isHex64 = (value) => /^[0-9a-fA-F]{64}$/.test(normalizedHash(value));
// Rust f64::parse akzeptiert auch "inf"/"NaN", .is_finite() filtert sie.
const finiteNumber = (value) => {
  if (value == null || value === '') return null;
  if (!/^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/.test(value)) return null;
  const n = Number(value);
  return Number.isFinite(n) ? n : null;
};

// validate_systematic_research_csv – liefert die Zeilenzahl.
export function validateCsv(text, output, runId, commandId, eligibleSources, validatedClaims) {
  const { headers, rows: records } = parseCsv(text);
  const has = (field) => headers.includes(field);
  const value = (record, field) => {
    const index = headers.indexOf(field);
    if (index < 0 || index >= record.length) return null;
    return record[index].trim();
  };
  const requireHeader = (field) => {
    if (!has(field)) throw fail(`${output.file_name} is missing required column ${field}`);
  };
  const requireAnyHeader = (fields, purpose) => {
    if (!fields.some(has)) throw fail(`${output.file_name} is missing ${purpose}; expected one of ${fields.join(', ')}`);
  };
  ['research_run_id', 'research_command_id'].forEach(requireHeader);
  if (output.require_manifest_evidence) ['source_id', 'canonical_url', 'snapshot_hash'].forEach(requireHeader);
  if (output.require_manifest_claim) {
    ['claim_id', 'evidence_id', 'source_id', 'snapshot_id', 'canonical_url', 'snapshot_hash', 'quote'].forEach(requireHeader);
  }
  if (output.table_key === 'measured_load_points') {
    ['source_row_ref', 'measurement_kind', 'is_derived', 'rpm', 'propeller_size', 'prop_diameter_in', 'prop_pitch_in', 'torque_Nm', 'archive_manifest_hash', 'archive_member_path', 'archive_member_hash'].forEach(requireHeader);
    requireAnyHeader(['thrust_N', 'force_N', 'axial_load_N'], 'an axial force channel in newtons');
    if (has('fact_label') || has('fact_value')) {
      throw fail(`${output.file_name} uses generic fact columns instead of row-based measurements`);
    }
  } else if (output.table_key === 'derived_bearing_loads') {
    ['source_row_ref', 'derivation_method', 'assumption_text', 'archive_manifest_hash', 'archive_member_path', 'archive_member_hash'].forEach(requireHeader);
    requireAnyHeader(['bearing_radial_load_N', 'radial_load_N', 'axial_load_N', 'torque_Nm', 'moment_Nm', 'value'], 'a derived physical load or moment channel');
  }

  let rows = 0;
  const f = output.file_name;
  for (const record of records) {
    rows += 1;
    if (!(value(record, 'research_run_id') === runId && value(record, 'research_command_id') === commandId)) {
      throw fail(`${f} row ${rows} is bound to another research run or command`);
    }
    if (output.require_manifest_evidence) {
      const sourceId = value(record, 'source_id');
      if (!sourceId) throw fail(`${f} row ${rows} has no source_id`);
      const expected = eligibleSources.get(sourceId);
      if (!expected) throw fail(`${f} row ${rows} source ${sourceId} is absent from validated manifest evidence`);
      const hash = value(record, 'snapshot_hash');
      if (!(value(record, 'canonical_url') === expected[0] && hash !== null && hashEq(hash, expected[1]))) {
        throw fail(`${f} row ${rows} does not match validated evidence for source ${sourceId}`);
      }
    }
    if (output.require_manifest_claim) {
      const claimId = value(record, 'claim_id');
      if (!claimId) throw fail(`${f} row ${rows} has no claim_id`);
      const c = validatedClaims.get(claimId);
      if (!c) throw fail(`${f} row ${rows} claim ${claimId} is absent from validated manifest claims`);
      const hash = value(record, 'snapshot_hash');
      if (!(
        value(record, 'evidence_id') === c[0] &&
        value(record, 'source_id') === c[1] &&
        value(record, 'snapshot_id') === c[2] &&
        value(record, 'canonical_url') === c[3] &&
        hash !== null && hashEq(hash, c[4]) &&
        value(record, 'quote') === c[5]
      )) {
        throw fail(`${f} row ${rows} does not match validated lineage for claim ${claimId}`);
      }
    }
    if (output.table_key === 'measured_load_points') {
      const ref = value(record, 'source_row_ref');
      if (!ref) throw fail(`${f} row ${rows} has no original source-row reference`);
      if (ref.toLowerCase() === 'unknown' || ref.toLowerCase() === 'n/a') {
        throw fail(`${f} row ${rows} has a non-auditable source-row reference`);
      }
      const kind = (value(record, 'measurement_kind') ?? '').toLowerCase();
      const derived = (value(record, 'is_derived') ?? '').toLowerCase();
      if (!(['measured', 'direct', 'experimental'].includes(kind) && !['true', '1', 'yes'].includes(derived))) {
        throw fail(`${f} row ${rows} is derived or is not identified as a direct measurement`);
      }
      const rpm = finiteNumber(value(record, 'rpm'));
      if (!(rpm !== null && rpm > 0)) throw fail(`${f} row ${rows} has no positive machine-readable RPM value`);
      if (!['thrust_N', 'force_N', 'axial_load_N'].some((k) => finiteNumber(value(record, k)) !== null)) {
        throw fail(`${f} row ${rows} has no machine-readable axial force value in newtons`);
      }
      const dia = finiteNumber(value(record, 'prop_diameter_in'));
      const pitch = finiteNumber(value(record, 'prop_pitch_in'));
      if (!(dia !== null && dia > 0 && pitch !== null && pitch > 0 && value(record, 'propeller_size'))) {
        throw fail(`${f} row ${rows} has incomplete propeller diameter/pitch provenance`);
      }
      const torque = value(record, 'torque_Nm');
      if (torque && finiteNumber(torque) === null) throw fail(`${f} row ${rows} has a non-numeric torque_Nm value`);
      for (const k of ['archive_manifest_hash', 'archive_member_hash']) {
        const v = value(record, k);
        if (!(v !== null && isHex64(v))) throw fail(`${f} row ${rows} has no valid ${k}`);
      }
      if (!value(record, 'archive_member_path')) throw fail(`${f} row ${rows} has no archive_member_path`);
    } else if (output.table_key === 'derived_bearing_loads') {
      for (const k of ['source_row_ref', 'derivation_method', 'assumption_text', 'archive_member_path']) {
        if (!value(record, k)) throw fail(`${f} row ${rows} has no ${k}`);
      }
      for (const k of ['archive_manifest_hash', 'archive_member_hash']) {
        const v = value(record, k);
        if (!(v !== null && isHex64(v))) throw fail(`${f} row ${rows} has no valid ${k}`);
      }
      const physical = ['bearing_radial_load_N', 'radial_load_N', 'axial_load_N', 'torque_Nm', 'moment_Nm', 'value']
        .map((k) => finiteNumber(value(record, k))).find((n) => n !== null);
      if (physical === undefined) throw fail(`${f} row ${rows} has no machine-readable derived load or moment value`);
      const unit = value(record, 'unit') ?? value(record, 'units');
      if (has('value')) {
        if (!(unit !== null && ['n', 'newton', 'nm', 'n m', 'n·m', 'newton metre'].includes(unit.trim().toLowerCase()))) {
          throw fail(`${f} row ${rows} generic value is not identified as a physical load or moment`);
        }
      }
      const rowText = record.join(' ').toLowerCase();
      if (['axis count', 'axes', 'directions', 'direction count'].some((m) => rowText.includes(m))) {
        throw fail(`${f} row ${rows} describes axes or directions, not a bearing load`);
      }
    }
  }
  if (rows === 0) throw fail(`systematic research output ${f} is empty`);
  return rows;
}

// promote_systematic_research_workspace_outputs – liefert Zahl projizierter Tabellen.
export async function promoteWorkspaceOutputs(ctx, commandId, command, queueTask) {
  const taskId = queueTask?.message_key;
  if (!taskId || !queueTask?.workspace_root) throw fail('systematic research writeback requires the leased task workspace');
  const domain = commandDomain(command);
  if (!domain) throw fail('systematic research writeback requires a Knowledge domain');
  const runId = commandRunId(command);
  if (!runId) throw fail('systematic research writeback requires an immutable research_run_id');

  const manifestFile = await ctx.cap.agent.task.workspace.read({ task_id: taskId, path: 'validation/evidence-manifest.json' });
  if (!manifestFile) throw fail('read evidence manifest validation/evidence-manifest.json');
  // Hash über die exakten Bytes: die Fähigkeit liefert ihn mit (Text-Dekodierung
  // wäre nicht byte-treu).
  const manifestSha256 = manifestFile.sha256;
  let manifest;
  try {
    manifest = JSON.parse(manifestFile.text);
  } catch {
    throw fail('parse systematic research manifest');
  }
  const receipt = await loadValidationReceipt(ctx, taskId, manifestSha256, runId, commandId);
  if (!receiptCoversManifest(receipt, manifestSha256)) {
    throw fail('systematic research validation receipt does not cover the current evidence manifest');
  }
  const { eligibleSources, validatedClaims } = validateManifest(manifest, runId, commandId);

  const validated = [];
  for (const output of csvOutputs(command)) {
    const path = `dashboard/knowledge/${output.file_name}`;
    // Fähigkeit kanonisiert und verweigert Pfade außerhalb des Workspace.
    const file = await ctx.cap.agent.task.workspace.read({ task_id: taskId, path });
    if (!file) throw fail(`canonicalize research output ${output.file_name}`);
    const rows = validateCsv(file.text, output, runId, commandId, eligibleSources, validatedClaims);
    validated.push({ output, path, rows });
  }
  // ref: store.rs:22282-22330 – knowledge::dispatch_capturing(describe | create, import --from-file … --mode
  // replace). --from-file ist relativ zum Workspace der Aufgabe (workspace_task_id), nie ein Hostpfad.
  for (const { output, path, rows } of validated) {
    const described = await knowledgeQuery(ctx, ['data', 'describe', '--domain', domain, '--key', output.table_key]).catch(() => null);
    if (!described || described.ok === false) {
      await knowledgeQuery(ctx, [
        'data', 'create', '--domain', domain, '--key', output.table_key, '--source-system', 'systematic-research',
        '--title', output.title, '--description', `Validated systematic research output for ${domain}; run ${runId}`,
      ]);
    }
    const imported = await knowledgeQuery(ctx,
      ['data', 'import', '--domain', domain, '--key', output.table_key, '--from-file', path, '--mode', 'replace'], taskId);
    if (!(imported?.ok === true && imported?.row_count === rows)) {
      throw fail(`Knowledge import row-count mismatch for ${domain}/${output.table_key}: expected ${rows}, got ${JSON.stringify(imported)}`);
    }
  }
  // Früher knowledge.project(): die Projektion des Katalogs nach knowledge_tables ist live (§16.30, wie
  // projection.refresh) – gezählt werden die importierten Tabellen.
  return validated.length;
}

/** Nativer Knowledge-Datenspeicher (Kernprimitiv knowledge.query, flach §16.30). */
export async function knowledgeQuery(ctx, argv, workspaceTaskId) {
  if (workspaceTaskId) return ctx.cap.knowledge.query({ argv, workspace_task_id: workspaceTaskId });
  return ctx.cap.knowledge.query({ argv });
}

/** ref: knowledge_active_tables(root, Some(domain)) – Katalog aus knowledge_tables, geordnet nach table_id. */
export async function activeKnowledgeTables(ctx, domain) {
  const docs = await ctx.read('knowledge_tables', { selector: {} });
  return docs
    .map((doc) => {
      const p = doc.payload && typeof doc.payload === 'object' ? doc.payload : {};
      const catalogId = String(p.catalog_id ?? p.table_id ?? doc.id ?? '').replace(/^table:/, '');
      return {
        catalog_id: catalogId,
        domain: p.domain ?? doc.domain,
        table_key: p.table_key ?? p.key ?? doc.table_key,
        archived: p.archived === true || p.status === 'archived' || doc.status === 'archived',
      };
    })
    .filter((t) => t.catalog_id && t.domain === domain && !t.archived && typeof t.table_key === 'string')
    .sort((a, b) => (a.catalog_id < b.catalog_id ? -1 : a.catalog_id > b.catalog_id ? 1 : 0))
    .map(({ archived: _a, ...t }) => t);
}

const ROW_PAGE = 1000;
/** ref: knowledge_table_all_rows – alle Zeilen über dataframe.window seitenweise (Kern: limit ≤ 1000). */
export async function allTableRows(ctx, catalogId) {
  const rows = [];
  for (let skip = 0; ; skip += ROW_PAGE) {
    const page = await ctx.cap.dataframe.window({ table: catalogId, skip, limit: ROW_PAGE });
    const chunk = Array.isArray(page?.rows) ? page.rows : [];
    rows.push(...chunk);
    const total = Number.isInteger(page?.row_count) ? page.row_count : rows.length;
    if (chunk.length < ROW_PAGE || rows.length >= total) return rows;
  }
}

const METADATA_URL_PREFIXES = [
  'https://doi.org/', 'http://doi.org/', 'https://api.crossref.org/', 'https://api.openalex.org/',
  'https://api.semanticscholar.org/', 'https://www.semanticscholar.org/', 'https://scholar.google.',
  'https://www.researchgate.net/', 'https://www.academia.edu/',
];

function validSnapshotHash(value) {
  const t = value.trim();
  return t.startsWith('sha256:') && /^[0-9a-fA-F]{64}$/.test(t.slice(7));
}

// document_source_receipt_is_eligible
export function receiptIsEligible(value) {
  const str = (k) => (typeof value?.[k] === 'string' ? value[k] : null);
  const lowerTrim = (k) => (str(k) ?? '').trim().toLowerCase();
  const snapshotHash = str('snapshot_hash') !== null && validSnapshotHash(str('snapshot_hash'));
  const url = str('canonical_url');
  let canonicalUrl = false;
  if (url !== null && url === url.trim()) {
    const parsed = parseUrlParts(url);
    canonicalUrl = parsed !== null && (parsed.scheme === 'http' || parsed.scheme === 'https') && parsed.host !== '';
  }
  const tier = lowerTrim('source_tier');
  const type = lowerTrim('source_type');
  const statuses = ['review_status', 'source_status', 'relevance_status', 'status']
    .map(str).filter((s) => s !== null).map((s) => s.trim().toLowerCase());
  const hasId = (k) => str(k) !== null && str(k).trim() !== '';
  const urlRole = lowerTrim('url_role');
  const scope = lowerTrim('content_scope');
  const int = (k) => (Number.isInteger(value?.[k]) ? value[k] : null);
  const score = int('evidence_relevance_score');
  const http = int('http_status');
  return (
    snapshotHash &&
    value.evidence_eligible === true &&
    value.verification_status === 'verified' &&
    value.transport_verified === true &&
    value.content_extracted === true &&
    value.actual_full_text_or_data === true &&
    score !== null && score >= 8 &&
    http !== null && http >= 200 && http <= 299 && http !== 204 &&
    canonicalUrl &&
    tier !== '' && !tier.includes('metadata') && !tier.includes('aggregat') &&
    !type.includes('metadata') && type !== 'aggregator' &&
    !(url !== null && METADATA_URL_PREFIXES.some((p) => url.trim().toLowerCase().startsWith(p))) &&
    !(value.metadata_only === true) &&
    !(str('evidence_rejection_reason') !== null && str('evidence_rejection_reason').trim() !== '') &&
    hasId('source_id') &&
    (hasId('claim_id') || hasId('evidence_id')) &&
    hasId('snapshot_id') &&
    hasId('retrieved_at') &&
    ['original_content', 'original_data', 'publisher_full_text', 'dataset_archive'].includes(urlRole) &&
    ['full_text', 'original_data', 'full_dataset', 'dataset_archive'].includes(scope) &&
    !statuses.some((s) => ['rejected', 'off_topic', 'off-topic', 'irrelevant', 'fachfremd'].includes(s))
  );
}

// document_source_receipt_snapshot_matches – §16.32: Snapshot-Bytes liegen inhaltsadressiert in files
// (ref = sha256, vom Kern beim Empfang geprüft). „Bytes passen zum Beleg-Hash“ = files-Datensatz mit dieser ref.
// Altbestand (snapshot_path auf der Platte) kommt über den Kern-Import nach files (V4-HANDLERS.md).
async function receiptSnapshotMatches(ctx, receipt) {
  const hash = typeof receipt.snapshot_hash === 'string' ? receipt.snapshot_hash.trim() : '';
  if (!validSnapshotHash(hash)) return false;
  const [doc] = await ctx.read('files', { selector: { ref: hash.slice('sha256:'.length).toLowerCase() }, limit: 1 });
  return Boolean(doc);
}

// systematic_research_evidence_snapshot
export async function evidenceSnapshot(ctx, commandId, command) {
  const domain = commandDomain(command);
  if (!domain) throw fail('systematic research completion requires a Knowledge domain');
  const runId = commandRunId(command);
  if (!runId) throw fail('systematic research completion requires an immutable research_run_id');
  // [{catalog_id, domain, table_key}] geordnet nach table_id (knowledge_active_tables).
  const domainTables = await activeKnowledgeTables(ctx, domain);
  if (!Array.isArray(domainTables) || domainTables.length === 0) {
    throw fail(`systematic research completion requires authoritative Knowledge tables for ${domain}`);
  }
  let identifiedCount = 0;
  const sourceReceiptLinks = [];
  const snapshotHashes = [];
  const runManifest = [];
  for (const { catalog_id: catalogId, table_key: tableKey } of domainTables) {
    const tableId = `table:${catalogId}`;
    const all = await allTableRows(ctx, catalogId);
    const runRows = all.filter(
      (row) =>
        firstStringField(row, ['research_run_id', 'run_id']) === runId &&
        firstStringField(row, ['research_command_id', 'command_id']) === commandId,
    );
    if (runRows.length === 0) continue;
    runManifest.push({ table_id: tableId, table_key: tableKey, rows: runRows });
    if (tableKey !== 'source_catalog') continue;
    for (const receipt of runRows) {
      identifiedCount += 1;
      if (!receiptIsEligible(receipt) || !(await receiptSnapshotMatches(ctx, receipt))) continue;
      const sourceId = firstStringField(receipt, ['source_id']);
      if (!sourceId) throw fail('eligible source receipt is missing source_id');
      const snapshotHash = firstStringField(receipt, ['snapshot_hash']);
      if (!snapshotHash) throw fail('eligible source receipt is missing snapshot_hash');
      sourceReceiptLinks.push({
        kind: 'source_receipt',
        id: sourceId,
        source_id: sourceId,
        evidence_id: firstStringField(receipt, ['evidence_id']),
        claim_id: firstStringField(receipt, ['claim_id']),
        snapshot_id: firstStringField(receipt, ['snapshot_id']),
        snapshot_hash: snapshotHash,
        table_id: tableId,
        research_run_id: runId,
        research_command_id: commandId,
      });
      if (!snapshotHashes.includes(snapshotHash)) snapshotHashes.push(snapshotHash);
    }
  }
  if (sourceReceiptLinks.length === 0) {
    throw fail('systematic research completion requires at least one persisted, eligible source receipt');
  }
  const contentHash = `sha256:${sha256Hex(canonicalJson(runManifest))}`;
  return {
    identified_count: identifiedCount,
    accepted_count: sourceReceiptLinks.length,
    used_count: sourceReceiptLinks.length,
    source_receipt_links: sourceReceiptLinks,
    source_receipt_snapshot_hashes: snapshotHashes,
    knowledge_version: {
      version_id: contentHash,
      content_hash: contentHash,
      domain,
      research_run_id: runId,
      research_command_id: commandId,
      table_ids: runManifest.map((e) => e.table_id),
    },
  };
}
