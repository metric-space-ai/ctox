const MOD_BUILD = '20260721-ia1';
const MODULE_ID = 'esign';
const PRIMARY = 'signature_requests';
const TITLE = 'esign';
const CREATE_COMMAND = 'ats.signature.request';
const SIGN_COMMAND = 'ats.signature.sign';
const SUBJECT_KINDS = ['arbeitsvertrag', 'vermittlungsvertrag', 'ueberlassungsvertrag'];
// A request is "done" once it reaches a terminal state; everything else is open.
const TERMINAL_STATUSES = new Set(['completed', 'declined', 'expired']);
const STATUS_KEY = {
  created: 'statusCreated', sent: 'statusSent', partially_signed: 'statusPartial',
  completed: 'statusCompleted', declined: 'statusDeclined', expired: 'statusExpired',
};

export const COPY = {
  de: {
    document: 'Dokument-ID', subjectKind: 'Vertragstyp', employment: 'Arbeitsvertrag', placement: 'Vermittlungsvertrag', staffing: 'Überlassungsvertrag', signers: 'Unterzeichner-IDs (Komma-getrennt)', create: 'Anlegen', entries: 'Einträge', empty: 'Noch keine Anfragen.', offlineService: 'Offline: Befehlsdienst nicht verfügbar.', offlineSend: 'Offline: Befehl konnte nicht gesendet werden.', signatureCaptured: 'Signatur erfasst.', request: 'Anfrage', signer: 'Unterzeichner', status: 'Status', blocked: 'Blockiert.', documentRequired: 'Dokument-ID erforderlich.', requestCreated: 'Signatur-Anfrage angelegt.', sign: 'Signieren', artifact: 'Artefakt',
    kicker: 'E-SIGNATUR', listTitle: 'Anfragen', allKinds: 'Alle Typen', viewAll: 'Alle', viewOpen: 'Offen', viewDone: 'Abgeschlossen', composerKicker: 'NEUE ANFRAGE', composerTitle: 'Signatur anfordern', composerHint: 'Eintrag wählen oder neue Anfrage anlegen.', recordKicker: 'ANFRAGE', signersHead: 'Unterzeichner', noSigners: 'Keine Unterzeichner erfasst.', importDone: 'Import abgeschlossen.', exportDone: 'Export erstellt.', invalidFile: 'Datei konnte nicht gelesen werden (JSON erwartet).',
    statusCreated: 'angelegt', statusSent: 'gesendet', statusPartial: 'teilweise signiert', statusCompleted: 'abgeschlossen', statusDeclined: 'abgelehnt', statusExpired: 'abgelaufen',
  },
  en: {
    document: 'Document ID', subjectKind: 'Agreement type', employment: 'Employment contract', placement: 'Placement agreement', staffing: 'Staffing agreement', signers: 'Signer IDs (comma-separated)', create: 'Create', entries: 'records', empty: 'No signature requests yet.', offlineService: 'Offline: command service unavailable.', offlineSend: 'Offline: command could not be sent.', signatureCaptured: 'Signature recorded.', request: 'Request', signer: 'Signer', status: 'Status', blocked: 'Blocked.', documentRequired: 'Document ID is required.', requestCreated: 'Signature request created.', sign: 'Sign', artifact: 'Artifact',
    kicker: 'E-SIGNATURE', listTitle: 'Requests', allKinds: 'All types', viewAll: 'All', viewOpen: 'Open', viewDone: 'Closed', composerKicker: 'NEW REQUEST', composerTitle: 'Request signature', composerHint: 'Select a record or start a new request.', recordKicker: 'REQUEST', signersHead: 'Signers', noSigners: 'No signers recorded.', importDone: 'Import complete.', exportDone: 'Export created.', invalidFile: 'File could not be read (JSON expected).',
    statusCreated: 'created', statusSent: 'sent', statusPartial: 'partially signed', statusCompleted: 'completed', statusDeclined: 'declined', statusExpired: 'expired',
  },
};
let text = COPY.de;

export async function mount(ctx) {
  text = COPY[ctx.locale === 'en' ? 'en' : 'de'];
  await ensureStyles();
  ctx.host.innerHTML = await loadMarkup();
  ctx.host.dataset.atsModule = MODULE_ID;
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();

  const root = ctx.host.querySelector('[data-esign-root]');
  applyStaticCopy(root);

  const listPane = root?.querySelector('[data-esign-list-pane]');
  const listEl = root?.querySelector('[data-esign-list]');
  const formEl = root?.querySelector('[data-esign-form]');
  const gateEl = root?.querySelector('[data-esign-gate]');
  const detailEl = root?.querySelector('[data-esign-detail]');
  const wbKicker = root?.querySelector('[data-esign-wb-kicker]');
  const wbTitle = root?.querySelector('[data-esign-wb-title]');
  const wbFooter = root?.querySelector('[data-esign-wb-footer]');
  const collapseBtn = root?.querySelector('[data-esign-collapse]');
  const newBtn = root?.querySelector('[data-action="new"]');
  const importBtn = root?.querySelector('[data-action="import"]');
  const exportBtn = root?.querySelector('[data-action="export"]');

  const state = { records: [], visible: [], selectedId: '', userCollapsed: false, grammar: readGrammarState(listPane) };
  const collection = () => { try { return ctx.db?.collection?.(PRIMARY) || null; } catch { return null; } };

  // Gate feedback renders in the kit callout; kinds map onto its variants.
  const GATE_VARIANTS = { ok: 'is-success', block: 'is-danger', offline: 'is-warning' };
  function setGate(html, kind) {
    if (!gateEl) return;
    gateEl.className = 'ctox-callout' + (GATE_VARIANTS[kind] ? ' ' + GATE_VARIANTS[kind] : '');
    gateEl.innerHTML = html || '';
  }

  function selectedRecord() {
    return state.records.find((r) => recordKey(r) === state.selectedId) || null;
  }

  // ---- LEFT column: reactive record list under the shell-wired grammar ------
  function renderListRegion() {
    const counts = bandCounts(state.records);
    state.visible = filterRecords(state.records, state.grammar);
    if (listEl) listEl.innerHTML = renderList(state.visible, { view: state.grammar.view, selectedId: state.selectedId }, text);
    writeCounts(listPane, counts);
    writeFooter(listPane, `${state.visible.length} ${text.entries} · ${bandLabel(state.grammar, text)}`);
  }

  // ---- MAIN column: workbench bound to the selected record ------------------
  function renderWorkbench() {
    const rec = selectedRecord();
    const hasSel = Boolean(rec);
    const showDetail = computeDetailVisible(hasSel, state.userCollapsed);
    if (collapseBtn) {
      collapseBtn.hidden = !hasSel; // only show the reveal control when there is something to reveal
      collapseBtn.setAttribute('aria-pressed', String(hasSel && state.userCollapsed));
    }
    if (wbKicker) wbKicker.textContent = hasSel ? text.recordKicker : text.composerKicker;
    if (wbTitle) wbTitle.textContent = hasSel ? (rec.document_id || rec.id || text.composerTitle) : text.composerTitle;
    if (detailEl) detailEl.innerHTML = showDetail ? renderDetail(rec, text) : '';
    if (wbFooter) wbFooter.textContent = hasSel ? `${text.request}: ${rec.id || rec.document_id || '—'}` : text.composerHint;
  }

  function fillForm(r) {
    if (!formEl) return;
    const doc = formEl.querySelector('[name="document_id"]');
    const kind = formEl.querySelector('[name="subject_kind"]');
    const signers = formEl.querySelector('[name="signers"]');
    if (doc) doc.value = r.document_id || '';
    if (kind) kind.value = SUBJECT_KINDS.includes(r.subject_kind) ? r.subject_kind : SUBJECT_KINDS[0];
    if (signers) signers.value = Array.isArray(r.signers) ? r.signers.map((s) => s && s.id).filter(Boolean).join(', ') : '';
  }

  function selectRecord(id) {
    state.selectedId = id;
    state.userCollapsed = false;
    const rec = selectedRecord();
    if (rec) fillForm(rec);
    setGate('');
    renderListRegion();
    renderWorkbench();
  }

  function startNew() {
    state.selectedId = '';
    state.userCollapsed = false;
    try { formEl?.reset(); } catch {}
    setGate('');
    renderListRegion();
    renderWorkbench();
    try { formEl?.querySelector('[name="document_id"]')?.focus(); } catch {}
  }

  function onCollapseToggle() {
    if (!state.selectedId) return;
    state.userCollapsed = !state.userCollapsed;
    renderWorkbench();
  }

  async function loadRecords() {
    const col = collection();
    let rows = [];
    if (col?.find) {
      try {
        const docs = await col.find({ selector: {} }).exec();
        rows = docs.map((d) => (typeof d.toJSON === 'function' ? d.toJSON() : d)).filter((r) => !r._deleted);
      } catch (e) { console.error('[esign] load failed:', e); }
    }
    rows.sort((a, b) => (b.updated_at_ms || 0) - (a.updated_at_ms || 0));
    state.records = rows;
  }

  async function refresh() {
    await loadRecords();
    renderListRegion();
    renderWorkbench();
  }

  // ---- Selection (left list) ------------------------------------------------
  function onListClick(event) {
    const row = event.target?.closest?.('[data-esign-row]');
    if (!row || !listEl?.contains(row)) return;
    selectRecord(row.getAttribute('data-esign-row') || '');
  }
  listEl?.addEventListener('click', onListClick);

  // Re-render the list on any shell-wired grammar change (search/view/tray/band).
  function onGrammarChange(event) {
    if (!listPane || !listPane.contains(event.target)) return;
    state.grammar = event.detail || readGrammarState(listPane);
    renderListRegion();
  }
  root?.addEventListener('ctox-pane-grammar-change', onGrammarChange);

  // ---- Sign flow (unchanged command type + payload) -------------------------
  async function onDetailClick(event) {
    const btn = event.target?.closest?.('[data-sign]');
    if (!btn) return;
    const requestId = btn.getAttribute('data-sign');
    const signerId = btn.getAttribute('data-signer');
    if (!requestId || !signerId) return;
    setGate('');
    const dispatch = ctx.commandBus?.dispatch;
    if (typeof dispatch !== 'function') { setGate(text.offlineService, 'offline'); return; }
    let result;
    try {
      result = await ctx.commandBus?.dispatch?.({
        module: MODULE_ID,
        command_type: SIGN_COMMAND,
        payload: { request_id: requestId, signer_id: signerId },
      });
    } catch (e) {
      console.error('[esign] sign dispatch failed:', e);
      setGate(text.offlineSend, 'offline');
      return;
    }
    if (renderBlocked(result, setGate)) return;
    const reqId = result?.request_id ?? requestId;
    const status = result?.status ?? '';
    setGate(
      `<strong>${esc(text.signatureCaptured)}</strong>`
      + `<div class="ats-result-row">${esc(text.request)}: ` + esc(reqId) + '</div>'
      + `<div class="ats-result-row">${esc(text.signer)}: ` + esc(signerId) + '</div>'
      + (status ? `<div class="ats-result-row">${esc(text.status)}: ` + esc(status) + '</div>' : ''),
      'ok',
    );
    await refresh();
  }
  detailEl?.addEventListener('click', onDetailClick);

  // ---- Create flow (unchanged command type + payload) -----------------------
  async function onSubmit(event) {
    event.preventDefault();
    setGate('');
    const dispatch = ctx.commandBus?.dispatch;
    if (typeof dispatch !== 'function') { setGate(text.offlineService, 'offline'); return; }
    const data = new FormData(formEl);
    const f = Object.fromEntries(data.entries());
    const document_id = String(f.document_id || '').trim();
    if (!document_id) { setGate(text.documentRequired, 'block'); return; }
    const subject_kind = SUBJECT_KINDS.includes(f.subject_kind) ? f.subject_kind : SUBJECT_KINDS[0];
    const signers = String(f.signers || '')
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean)
      .map((id) => ({ id, state: 'pending' }));
    const payload = { document_id, subject_kind, signers };

    let result;
    try {
      result = await ctx.commandBus?.dispatch?.({
        module: MODULE_ID,
        command_type: CREATE_COMMAND,
        payload,
      });
    } catch (e) {
      console.error('[esign] dispatch failed:', e);
      setGate(text.offlineSend, 'offline');
      return;
    }
    if (renderBlocked(result, setGate)) return;
    const reqId = result?.request_id ?? result?.data?.request_id ?? null;
    const status = result?.status ?? result?.data?.status ?? null;
    setGate(
      `<strong>${esc(text.requestCreated)}</strong>`
      + `<div class="ats-result-row">${esc(text.request)}: ` + esc(reqId ?? '—') + '</div>'
      + `<div class="ats-result-row">${esc(text.status)}: ` + esc(status ?? '—') + '</div>',
      'ok',
    );
    try { formEl.reset(); } catch {}
    await refresh();
  }
  formEl?.addEventListener('submit', onSubmit);

  // ---- Header actions: Neu / Import / Export --------------------------------
  newBtn?.addEventListener('click', startNew);
  collapseBtn?.addEventListener('click', onCollapseToggle);

  function onExport() {
    const rows = state.visible.length ? state.visible : state.records;
    let url = '';
    try {
      const blob = new Blob([JSON.stringify(rows, null, 2)], { type: 'application/json' });
      url = URL.createObjectURL(blob);
      const anchor = document.createElement('a');
      anchor.href = url;
      anchor.download = `esign-signature-requests-${rows.length}.json`;
      document.body.appendChild(anchor);
      anchor.click();
      anchor.remove();
      ctx.notifications?.info?.(text.exportDone);
    } catch (e) {
      console.error('[esign] export failed:', e);
    } finally {
      if (url) { try { URL.revokeObjectURL(url); } catch {} }
    }
  }
  exportBtn?.addEventListener('click', onExport);

  function onImport() {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'application/json,.json';
    input.addEventListener('change', async () => {
      const file = input.files && input.files[0];
      if (!file) return;
      const col = collection();
      if (!col?.upsert) { setGate(text.offlineService, 'offline'); return; }
      let parsed;
      try {
        parsed = JSON.parse(await file.text());
      } catch (e) {
        console.error('[esign] import parse failed:', e);
        setGate(text.invalidFile, 'block');
        return;
      }
      const items = Array.isArray(parsed) ? parsed : (Array.isArray(parsed?.records) ? parsed.records : [parsed]);
      let ok = 0;
      for (const item of items) {
        const record = normalizeSignatureRequest(item, { nowMs: Date.now() });
        if (!record.document_id) continue;
        try { await col.upsert(record); ok += 1; } catch (e) { console.error('[esign] import upsert failed:', e); }
      }
      setGate(`<strong>${esc(text.importDone)}</strong><div class="ats-result-row">${ok} ${esc(text.entries)}</div>`, 'ok');
      await refresh();
    }, { once: true });
    input.click();
  }
  importBtn?.addEventListener('click', onImport);

  // ---- Reactive backbone ----------------------------------------------------
  let sub = null;
  const col = collection();
  if (col?.find) { try { sub = col.find({ selector: {} }).$?.subscribe?.(() => { refresh().catch(() => {}); }); } catch {} }
  await refresh();

  return () => {
    try { sub?.unsubscribe?.(); } catch {}
    formEl?.removeEventListener('submit', onSubmit);
    listEl?.removeEventListener('click', onListClick);
    detailEl?.removeEventListener('click', onDetailClick);
    root?.removeEventListener('ctox-pane-grammar-change', onGrammarChange);
    ctx.host.replaceChildren();
    delete ctx.host.dataset.atsModule;
  };
}

// ---------------------------------------------------------------------------
// Pure helpers (exported for tests) — no DOM, no RxDB.
// ---------------------------------------------------------------------------

// Coarse lifecycle band derived from the request status field.
export function lifecycleBand(status) {
  return TERMINAL_STATUSES.has(String(status || 'created')) ? 'done' : 'open';
}

// Counted view band (zeros included) from the full record set.
export function bandCounts(records) {
  const counts = { all: 0, open: 0, done: 0 };
  for (const r of records || []) {
    counts.all += 1;
    counts[lifecycleBand(r && r.status)] += 1;
  }
  return counts;
}

export function filterRecords(records, grammar = {}) {
  const search = String(grammar.search || '').trim().toLowerCase();
  const band = grammar.band || 'all';
  const kind = (grammar.filters && grammar.filters.subject_kind) || 'all';
  return (records || []).filter((r) => {
    if (!r) return false;
    if (band !== 'all' && lifecycleBand(r.status) !== band) return false;
    if (kind !== 'all' && String(r.subject_kind || '') !== kind) return false;
    if (search) {
      const hay = `${r.document_id || ''} ${r.id || ''} ${r.subject_kind || ''} ${r.status || ''}`.toLowerCase();
      if (!hay.includes(search)) return false;
    }
    return true;
  });
}

// Auto-reveal idiom (outbound): the record detail shows when a record is
// selected and the user has not collapsed it.
export function computeDetailVisible(hasSelection, userCollapsed) {
  return Boolean(hasSelection) && !userCollapsed;
}

// Normalize an imported object into a schema-valid signature_requests record.
export function normalizeSignatureRequest(raw, opts = {}) {
  const nowMs = Number.isFinite(opts.nowMs) ? opts.nowMs : Date.now();
  const src = raw && typeof raw === 'object' ? raw : {};
  const id = String(src.id || src.request_id || '').trim() || `esign_${nowMs}_${Math.random().toString(36).slice(2, 8)}`;
  const signers = Array.isArray(src.signers)
    ? src.signers.map((s) => (typeof s === 'string' ? { id: s, state: 'pending' } : { state: 'pending', ...s }))
    : [];
  const record = {
    id,
    document_id: String(src.document_id || '').trim(),
    subject_kind: SUBJECT_KINDS.includes(src.subject_kind) ? src.subject_kind : (src.subject_kind ? String(src.subject_kind) : SUBJECT_KINDS[0]),
    signers,
    status: String(src.status || 'created'),
    created_at_ms: Number.isFinite(Number(src.created_at_ms)) ? Number(src.created_at_ms) : nowMs,
    updated_at_ms: Number.isFinite(Number(src.updated_at_ms)) ? Number(src.updated_at_ms) : nowMs,
  };
  if (Number.isFinite(Number(src.sent_at_ms))) record.sent_at_ms = Number(src.sent_at_ms);
  if (Number.isFinite(Number(src.expires_at_ms))) record.expires_at_ms = Number(src.expires_at_ms);
  if (src.signed_artifact_id) record.signed_artifact_id = String(src.signed_artifact_id);
  return record;
}

export function renderList(records, opts = {}, t = text) {
  if (!records || !records.length) return `<div class="ctox-empty">${esc(t.empty)}</div>`;
  const view = opts.view === 'list' ? 'list' : 'cards';
  const selectedId = opts.selectedId || '';
  return records.map((r) => (view === 'list' ? shardCompact(r, selectedId, t) : shardCard(r, selectedId, t))).join('');
}

function recordKey(r) { return (r && (r.id || r.document_id)) || ''; }

function signerProgress(r) {
  const signers = Array.isArray(r.signers) ? r.signers : [];
  return { done: signers.filter((s) => s && s.state === 'signed').length, total: signers.length };
}

function subjectLabel(kind, t) {
  if (kind === 'arbeitsvertrag') return t.employment;
  if (kind === 'vermittlungsvertrag') return t.placement;
  if (kind === 'ueberlassungsvertrag') return t.staffing;
  return kind || '—';
}

function statusLabel(status, t) {
  const key = STATUS_KEY[status];
  return (key && t[key]) || status || '—';
}

// Maps a signature-request status onto the kit badge states.
function statusBadgeClass(status) {
  if (status === 'completed') return 'is-success';
  if (status === 'declined' || status === 'expired') return 'is-danger';
  if (status === 'sent' || status === 'partially_signed') return 'is-warning';
  return '';
}

// A shard is a pure selector: title + ONE muted meta line.
function shardCard(r, selectedId, t) {
  const key = recordKey(r);
  const status = String(r.status || 'created');
  const progress = signerProgress(r);
  const meta = [t.kicker, r.subject_kind ? subjectLabel(r.subject_kind, t) : null,
    progress.total ? `${progress.done}/${progress.total} ${t.sign.toLowerCase()}` : null]
    .filter(Boolean).map(esc).join(' · ');
  const badge = ('ctox-badge ' + statusBadgeClass(status)).trim();
  return rowShell(r, key, selectedId,
    '<div class="esign-shard">'
    + '<div class="esign-shard-title">'
    + `<span class="${badge}" data-status="${esc(status)}">${esc(statusLabel(status, t))}</span>`
    + `<strong>${esc(r.document_id || key || '—')}</strong>`
    + '</div>'
    + `<small class="esign-shard-meta">${meta}</small>`
    + '</div>');
}

function shardCompact(r, selectedId, t) {
  const key = recordKey(r);
  const status = String(r.status || 'created');
  const badge = ('ctox-badge ' + statusBadgeClass(status)).trim();
  return rowShell(r, key, selectedId,
    '<div class="esign-row-compact">'
    + `<span class="esign-compact-title">${esc(r.document_id || key || '—')}</span>`
    + `<span class="${badge}" data-status="${esc(status)}">${esc(statusLabel(status, t))}</span>`
    + '</div>');
}

function rowShell(r, key, selectedId, inner) {
  const label = r.document_id || key || '—';
  const selected = key && key === selectedId;
  return '<button type="button" class="ctox-list-item esign-row' + (selected ? ' is-selected' : '') + '"'
    + ` data-esign-row="${esc(key)}" aria-selected="${selected ? 'true' : 'false'}"`
    + ` data-context-record-id="${esc(key)}" data-context-record-type="signature_request"`
    + ` data-context-label="${esc(label)}">${inner}</button>`;
}

function renderDetail(r, t) {
  if (!r) return '';
  const status = String(r.status || 'created');
  const signers = Array.isArray(r.signers) ? r.signers : [];
  const terminal = TERMINAL_STATUSES.has(status);
  const fields = [
    detailField(t.request, r.id || r.document_id || '—'),
    detailField(t.document, r.document_id || '—'),
    detailField(t.subjectKind, subjectLabel(r.subject_kind, t)),
    detailField(t.status, statusLabel(status, t)),
  ];
  if (r.signed_artifact_id) fields.push(detailField(t.artifact, r.signed_artifact_id));
  const signerRows = signers.map((s) => {
    const st = String((s && s.state) || 'pending');
    const canSign = !terminal && s && s.id && st !== 'signed' && st !== 'declined';
    const action = canSign
      ? `<button type="button" class="ctox-button ctox-button--sm" data-sign="${esc(r.id || '')}" data-signer="${esc(s.id)}">${esc(t.sign)}</button>`
      : '';
    return '<li class="esign-signer">'
      + `<span class="esign-signer-id">${esc((s && s.id) || '—')}</span>`
      + `<span class="esign-signer-state">${esc(st)}</span>`
      + action
      + '</li>';
  }).join('');
  return '<section class="esign-detail-card"'
    + ` data-context-record-id="${esc(r.id || '')}" data-context-record-type="signature_request"`
    + ` data-context-label="${esc(r.document_id || r.id || '—')}">`
    + `<dl class="ctox-fields">${fields.join('')}</dl>`
    + (signers.length
      ? `<div class="esign-signers-head">${esc(t.signersHead)}</div><ul class="esign-signers">${signerRows}</ul>`
      : `<div class="ctox-empty">${esc(t.noSigners)}</div>`)
    + '</section>';
}

function detailField(k, v) {
  return `<div><dt>${esc(k)}</dt><dd>${esc(v)}</dd></div>`;
}

function bandLabel(grammar, t) {
  const parts = [{ all: t.viewAll, open: t.viewOpen, done: t.viewDone }[grammar.band] || t.viewAll];
  const kind = grammar.filters && grammar.filters.subject_kind;
  if (kind && kind !== 'all') parts.push(subjectLabel(kind, t));
  if (grammar.search) parts.push(`"${grammar.search}"`);
  return parts.join(' · ');
}

// Shared blocked-decision renderer for both create and sign flows.
function renderBlocked(result, setGate) {
  const decision = result?.gate || result?.decision || null;
  const blockers = result?.blockers || decision?.blockers || result?.errors || null;
  const blocked = !result || result?.ok === false || result?.status === 'blocked'
    || decision?.status === 'blocked' || decision?.decision === 'block'
    || (Array.isArray(blockers) && blockers.length > 0);
  if (!blocked) return false;
  const items = (Array.isArray(blockers) ? blockers : [blockers])
    .filter(Boolean)
    .map((b) => '<li>' + esc(typeof b === 'string' ? b : (b?.message || b?.reason || JSON.stringify(b))) + '</li>')
    .join('');
  setGate(`<strong>${esc(text.blocked)}</strong>` + (items ? '<ul class="ats-blockers">' + items + '</ul>' : ''), 'block');
  return true;
}

// Read the current grammar state straight from the DOM so the module never
// depends on the shell having wired __ctoxPaneGrammar yet.
function readGrammarState(pane) {
  if (!pane) return { search: '', view: 'cards', band: 'all', filters: {} };
  const search = pane.querySelector('[data-pg-search]');
  const view = [...pane.querySelectorAll('[data-pg-view]')].find((b) => b.getAttribute('aria-pressed') === 'true')?.dataset.pgView || 'cards';
  const band = [...pane.querySelectorAll('[data-pg-band]')].find((b) => b.getAttribute('aria-selected') === 'true')?.dataset.pgBand || 'all';
  const filters = {};
  pane.querySelectorAll('[data-pg-filter]').forEach((el) => { filters[el.dataset.pgName || el.name || 'filter'] = el.value; });
  return { search: (search?.value || '').trim().toLowerCase(), view, band, filters };
}

// Counts/footer go through the shell handle when wired, else straight to the
// declarative targets (guarding for the pre-wire window).
function writeCounts(pane, counts) {
  const handle = pane && pane.__ctoxPaneGrammar;
  if (handle && typeof handle.setCounts === 'function') { handle.setCounts(counts); return; }
  for (const [key, value] of Object.entries(counts || {})) {
    const node = pane?.querySelector(`[data-pg-count="${key}"]`);
    if (node) node.textContent = ` (${value})`;
  }
}

function writeFooter(pane, txt) {
  const handle = pane && pane.__ctoxPaneGrammar;
  if (handle && typeof handle.setFooter === 'function') { handle.setFooter(txt); return; }
  const node = pane?.querySelector('[data-pg-footer]');
  if (node) node.textContent = txt || '';
}

function applyStaticCopy(root) {
  root?.querySelectorAll?.('[data-copy]').forEach((node) => { node.textContent = text[node.dataset.copy] || node.textContent; });
}

async function ensureStyles() {
  const href = new URL('./index.css', import.meta.url).pathname + '?v=' + MOD_BUILD;
  if (document.querySelector('link[href="' + href + '"]')) return;
  const link = document.createElement('link'); link.rel = 'stylesheet'; link.href = href; document.head.append(link);
}

async function loadMarkup() {
  const url = new URL('./index.html', import.meta.url).pathname + '?v=' + MOD_BUILD;
  const html = await fetch(url).then((r) => r.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((n) => n.remove());
  return doc.body.innerHTML;
}

function esc(v) { return String(v == null ? '' : v).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;'); }
