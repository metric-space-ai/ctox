const MOD_BUILD = '20260721-ia1';
const MODULE_ID = 'placements';
const PRIMARY = 'placements';
const CREATE_COMMAND = 'ats.placement.create';
const EARLY_LEAVE_COMMAND = 'ats.placement.early_leave';
// A placement is "ended" once it reaches a terminal state; everything else is
// an active placement running against its guarantee clock.
const TERMINAL_STATUSES = new Set(['early_leave', 'cancelled']);

export const COPY = {
  de: {
    kicker: 'VERMITTLUNGEN', listTitle: 'Vermittlungen', candidate: 'Kandidat-ID', client: 'Kunden-Account-ID', placementType: 'Vermittlungsart', directHire: 'Festanstellung (Personalvermittlung)', temporary: 'Arbeitnehmerüberlassung (Zeitarbeit)', qualifications: 'Pflicht-Qualifikationen (Komma)', fee: 'Honorar', guarantee: 'Garantie (Tage)', create: 'Anlegen', more: 'Weitere Angaben', entries: 'Einträge', empty: 'Noch keine Einträge.', earlyLeaveBooked: 'Frühausstieg verbucht.', clawback: 'Clawback', credit: 'Gutschrift', offlineService: 'Offline: Befehlsdienst nicht verfügbar.', offlineSend: 'Offline: Befehl konnte nicht gesendet werden.', candidateRequired: 'Kandidat-ID erforderlich.', blocked: 'Blockiert.', placementCreated: 'Placement angelegt.', placement: 'Placement', feeInvoice: 'Honorar-Rechnung', invoice: 'Rechnung', cancellation: 'Storno', earlyLeave: 'Frühausstieg', status: 'Status',
    allTypes: 'Alle Arten', viewAll: 'Alle', viewActive: 'Aktiv', viewEnded: 'Beendet', composerKicker: 'NEUE VERMITTLUNG', composerTitle: 'Vermittlung anlegen', composerHint: 'Eintrag wählen oder neue Vermittlung anlegen.', recordKicker: 'VERMITTLUNG', importDone: 'Import abgeschlossen.', exportDone: 'Export erstellt.', invalidFile: 'Datei konnte nicht gelesen werden (JSON erwartet).',
    statusConfirmed: 'bestätigt', statusEarlyLeave: 'Frühausstieg', statusCancelled: 'storniert',
  },
  en: {
    kicker: 'PLACEMENTS', listTitle: 'Placements', candidate: 'Candidate ID', client: 'Client account ID', placementType: 'Placement type', directHire: 'Permanent placement', temporary: 'Temporary staffing', qualifications: 'Required qualifications (comma-separated)', fee: 'Fee', guarantee: 'Guarantee (days)', create: 'Create', more: 'More details', entries: 'records', empty: 'No placements yet.', earlyLeaveBooked: 'Early leave recorded.', clawback: 'Clawback', credit: 'Credit note', offlineService: 'Offline: command service unavailable.', offlineSend: 'Offline: command could not be sent.', candidateRequired: 'Candidate ID is required.', blocked: 'Blocked.', placementCreated: 'Placement created.', placement: 'Placement', feeInvoice: 'Fee invoice', invoice: 'Invoice', cancellation: 'Cancellation', earlyLeave: 'Early leave', status: 'Status',
    allTypes: 'All types', viewAll: 'All', viewActive: 'Active', viewEnded: 'Ended', composerKicker: 'NEW PLACEMENT', composerTitle: 'Create placement', composerHint: 'Select a record or start a new placement.', recordKicker: 'PLACEMENT', importDone: 'Import complete.', exportDone: 'Export created.', invalidFile: 'File could not be read (JSON expected).',
    statusConfirmed: 'confirmed', statusEarlyLeave: 'early leave', statusCancelled: 'cancelled',
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

  const root = ctx.host.querySelector('[data-ats-root]');
  applyStaticCopy(root);

  const listPane = root?.querySelector('[data-placements-list-pane]');
  const listEl = root?.querySelector('[data-ats-list]');
  const formEl = root?.querySelector('[data-ats-form]');
  const gateEl = root?.querySelector('[data-ats-gate]');
  const detailEl = root?.querySelector('[data-ats-detail]');
  const wbKicker = root?.querySelector('[data-ats-wb-kicker]');
  const wbTitle = root?.querySelector('[data-ats-wb-title]');
  const wbFooter = root?.querySelector('[data-ats-wb-footer]');
  const collapseBtn = root?.querySelector('[data-ats-collapse]');
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
    if (wbTitle) wbTitle.textContent = hasSel ? placementTitle(rec) : text.composerTitle;
    if (detailEl) detailEl.innerHTML = showDetail ? renderDetail(rec, text) : '';
    if (wbFooter) wbFooter.textContent = hasSel ? `${text.placement}: ${rec.id || '—'}` : text.composerHint;
  }

  function fillForm(r) {
    if (!formEl) return;
    const candidate = formEl.querySelector('[name="candidate_id"]');
    const client = formEl.querySelector('[name="client_account_id"]');
    const type = formEl.querySelector('[name="placement_type"]');
    const fee = formEl.querySelector('[name="fee"]');
    const guarantee = formEl.querySelector('[name="guarantee_days"]');
    const required = formEl.querySelector('[name="required_types"]');
    if (candidate) candidate.value = r.candidate_id || '';
    if (client) client.value = r.client_account_id || '';
    if (type) type.value = r.placement_type === 'arbeitnehmerueberlassung' ? 'arbeitnehmerueberlassung' : '';
    if (fee) fee.value = r.fee == null ? '' : String(r.fee);
    if (guarantee) guarantee.value = r.guarantee_days == null ? '' : String(r.guarantee_days);
    if (required) required.value = ''; // required_types is a create-only input, not persisted on the record
  }

  function selectRecord(id) {
    state.selectedId = id;
    state.userCollapsed = false;
    const rec = selectedRecord();
    if (rec) fillForm(rec);
    setGate('');
    // Selection is an in-place class flip — a list rebuild resets the
    // operator's scroll (design-guide: re-renders never move the operator).
    applyListSelection();
    renderWorkbench();
  }

  function applyListSelection() {
    listEl?.querySelectorAll('[data-ats-row]').forEach((row) => {
      const on = (row.getAttribute('data-ats-row') || '') === String(state.selectedId || '');
      row.classList.toggle('is-selected', on);
      row.setAttribute('aria-selected', String(on));
    });
  }

  function startNew() {
    state.selectedId = '';
    state.userCollapsed = false;
    try { formEl?.reset(); } catch {}
    setGate('');
    renderListRegion();
    renderWorkbench();
    try { formEl?.querySelector('[name="candidate_id"]')?.focus(); } catch {}
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
      } catch (e) { console.error('[placements] load failed:', e); }
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
    const row = event.target?.closest?.('[data-ats-row]');
    if (!row || !listEl?.contains(row)) return;
    selectRecord(row.getAttribute('data-ats-row') || '');
  }
  listEl?.addEventListener('click', onListClick);

  // Re-render the list on any shell-wired grammar change (search/view/tray/band).
  function onGrammarChange(event) {
    if (!listPane || !listPane.contains(event.target)) return;
    state.grammar = event.detail || readGrammarState(listPane);
    renderListRegion();
  }
  root?.addEventListener('ctox-pane-grammar-change', onGrammarChange);

  // ---- Early-leave flow (unchanged command type + payload) ------------------
  async function onDetailClick(event) {
    const btn = event.target?.closest?.('[data-early-leave]');
    if (!btn) return;
    const placementId = btn.getAttribute('data-early-leave');
    if (!placementId) return;
    setGate('');
    try {
      const result = await ctx.commandBus?.dispatch?.({
        module: MODULE_ID,
        command_type: EARLY_LEAVE_COMMAND,
        payload: { placement_id: placementId, left_at_ms: Date.now() },
      });
      const cn = result?.credit_note_id ?? result?.data?.credit_note_id ?? null;
      const clawback = result?.clawback ?? result?.data?.clawback ?? null;
      setGate(
        `<strong>${esc(text.earlyLeaveBooked)}</strong>`
        + (clawback != null ? `<div class="ats-result-row">${esc(text.clawback)}: ` + esc(String(clawback)) + '</div>' : '')
        + (cn ? `<div class="ats-result-row">${esc(text.credit)}: ` + esc(cn) + '</div>' : ''),
        'ok',
      );
      await refresh();
    } catch (e) {
      console.error('[placements] early_leave dispatch failed:', e);
      setGate(text.offlineSend, 'offline');
    }
  }
  detailEl?.addEventListener('click', onDetailClick);

  // ---- Create flow (unchanged command type + payload) -----------------------
  async function onSubmit(event) {
    event.preventDefault();
    setGate('');
    const dispatch = ctx.commandBus?.dispatch;
    if (typeof dispatch !== 'function') {
      setGate(text.offlineService, 'offline');
      return;
    }
    const data = new FormData(formEl);
    const f = Object.fromEntries(data.entries());
    const candidate_id = String(f.candidate_id || '').trim();
    if (!candidate_id) { setGate(text.candidateRequired, 'block'); return; }
    const placementType = String(f.placement_type || '').trim();
    const requiredTypes = String(f.required_types || '')
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean);
    const payload = {
      candidate_id,
      client_account_id: String(f.client_account_id || '').trim() || null,
      placement_type: placementType || null,
      required_types: requiredTypes.length ? requiredTypes : undefined,
      fee: f.fee === '' || f.fee == null ? null : Number(f.fee),
      guarantee_days: f.guarantee_days === '' || f.guarantee_days == null ? null : Number(f.guarantee_days),
    };

    let result;
    try {
      result = await ctx.commandBus?.dispatch?.({
        module: MODULE_ID,
        command_type: CREATE_COMMAND,
        payload,
      });
    } catch (e) {
      console.error('[placements] dispatch failed:', e);
      setGate(text.offlineSend, 'offline');
      return;
    }

    if (renderBlocked(result, setGate)) return;

    const placementId = result?.placement_id ?? result?.data?.placement_id ?? null;
    const feeInvoiceId = result?.fee_invoice_id ?? result?.data?.fee_invoice_id ?? null;
    setGate(
      `<strong>${esc(text.placementCreated)}</strong>`
      + `<div class="ats-result-row">${esc(text.placement)}: ` + esc(placementId ?? '—') + '</div>'
      + `<div class="ats-result-row">${esc(text.feeInvoice)}: ` + esc(feeInvoiceId ?? '—') + '</div>',
      'ok'
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
      anchor.download = `placements-${rows.length}.json`;
      document.body.appendChild(anchor);
      anchor.click();
      anchor.remove();
      ctx.notifications?.info?.(text.exportDone);
    } catch (e) {
      console.error('[placements] export failed:', e);
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
        console.error('[placements] import parse failed:', e);
        setGate(text.invalidFile, 'block');
        return;
      }
      const items = Array.isArray(parsed) ? parsed : (Array.isArray(parsed?.records) ? parsed.records : [parsed]);
      let ok = 0;
      for (const item of items) {
        const record = normalizePlacement(item, { nowMs: Date.now() });
        if (!record.candidate_id) continue;
        try { await col.upsert(record); ok += 1; } catch (e) { console.error('[placements] import upsert failed:', e); }
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

// Coarse lifecycle band derived from the placement status field.
export function lifecycleBand(status) {
  return TERMINAL_STATUSES.has(String(status || 'confirmed')) ? 'ended' : 'active';
}

// Canonical placement-type key (records store '' / null for direct hire).
export function placementTypeKey(value) {
  return String(value || '') === 'arbeitnehmerueberlassung' ? 'arbeitnehmerueberlassung' : 'direct';
}

// Counted view band (zeros included) from the full record set.
export function bandCounts(records) {
  const counts = { all: 0, active: 0, ended: 0 };
  for (const r of records || []) {
    counts.all += 1;
    counts[lifecycleBand(r && r.status)] += 1;
  }
  return counts;
}

export function filterRecords(records, grammar = {}) {
  const search = String(grammar.search || '').trim().toLowerCase();
  const band = grammar.band || 'all';
  const type = (grammar.filters && grammar.filters.placement_type) || 'all';
  return (records || []).filter((r) => {
    if (!r) return false;
    if (band !== 'all' && lifecycleBand(r.status) !== band) return false;
    if (type !== 'all' && placementTypeKey(r.placement_type) !== type) return false;
    if (search) {
      const hay = `${r.candidate_id || ''} ${r.client_account_id || ''} ${r.id || ''} ${r.status || ''}`.toLowerCase();
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

// Normalize an imported object into a schema-valid placements record.
export function normalizePlacement(raw, opts = {}) {
  const nowMs = Number.isFinite(opts.nowMs) ? opts.nowMs : Date.now();
  const src = raw && typeof raw === 'object' ? raw : {};
  const id = String(src.id || src.placement_id || '').trim() || `placement_${nowMs}_${Math.random().toString(36).slice(2, 8)}`;
  const record = {
    id,
    candidate_id: String(src.candidate_id || '').trim(),
    client_account_id: String(src.client_account_id || '').trim(),
    status: String(src.status || 'confirmed'),
    created_at_ms: Number.isFinite(Number(src.created_at_ms)) ? Number(src.created_at_ms) : nowMs,
    updated_at_ms: Number.isFinite(Number(src.updated_at_ms)) ? Number(src.updated_at_ms) : nowMs,
  };
  if (src.placement_type) record.placement_type = String(src.placement_type);
  if (Number.isFinite(Number(src.fee))) record.fee = Number(src.fee);
  if (Number.isFinite(Number(src.guarantee_days))) record.guarantee_days = Number(src.guarantee_days);
  if (Number.isFinite(Number(src.start_ms))) record.start_ms = Number(src.start_ms);
  if (src.offer_id) record.offer_id = String(src.offer_id);
  if (src.vacancy_id) record.vacancy_id = String(src.vacancy_id);
  if (src.fee_invoice_id) record.fee_invoice_id = String(src.fee_invoice_id);
  if (src.storno_credit_note_id) record.storno_credit_note_id = String(src.storno_credit_note_id);
  return record;
}

export function renderList(records, opts = {}, t = text) {
  if (!records || !records.length) return `<div class="ctox-empty">${esc(t.empty)}</div>`;
  const view = opts.view === 'list' ? 'list' : 'cards';
  const selectedId = opts.selectedId || '';
  return records.map((r) => (view === 'list' ? shardCompact(r, selectedId, t) : shardCard(r, selectedId, t))).join('');
}

function recordKey(r) { return (r && r.id) || ''; }

function placementTitle(r) {
  const candidate = (r && r.candidate_id) || '—';
  const client = (r && r.client_account_id) || '—';
  return `${candidate} → ${client}`;
}

function typeLabel(placement_type, t) {
  return placementTypeKey(placement_type) === 'arbeitnehmerueberlassung' ? t.temporary : t.directHire;
}

function statusLabel(status, t) {
  if (status === 'early_leave') return t.statusEarlyLeave;
  if (status === 'cancelled') return t.statusCancelled;
  return t.statusConfirmed;
}

// Maps a placement status onto the kit badge states.
function statusBadgeClass(status) {
  if (status === 'confirmed') return 'is-success';
  if (status === 'early_leave') return 'is-warning';
  if (status === 'cancelled') return 'is-danger';
  return '';
}

// A shard is a pure selector: title + ONE muted meta line.
function shardCard(r, selectedId, t) {
  const key = recordKey(r);
  const status = String(r.status || 'confirmed');
  const meta = [t.kicker, typeLabel(r.placement_type, t), r.fee == null ? null : `${t.fee}: ${r.fee}`]
    .filter(Boolean).map(esc).join(' · ');
  const badge = ('ctox-badge ' + statusBadgeClass(status)).trim();
  return rowShell(r, key, selectedId,
    '<div class="placements-shard">'
    + '<div class="placements-shard-title">'
    + `<span class="${badge}" data-status="${esc(status)}">${esc(statusLabel(status, t))}</span>`
    + `<strong>${esc(placementTitle(r))}</strong>`
    + '</div>'
    + `<small class="placements-shard-meta">${meta}</small>`
    + '</div>');
}

function shardCompact(r, selectedId, t) {
  const key = recordKey(r);
  const status = String(r.status || 'confirmed');
  const badge = ('ctox-badge ' + statusBadgeClass(status)).trim();
  return rowShell(r, key, selectedId,
    '<div class="placements-row-compact">'
    + `<span class="placements-compact-title">${esc(placementTitle(r))}</span>`
    + `<span class="${badge}" data-status="${esc(status)}">${esc(statusLabel(status, t))}</span>`
    + '</div>');
}

function rowShell(r, key, selectedId, inner) {
  const label = placementTitle(r);
  const selected = key && key === selectedId;
  return '<button type="button" class="ctox-list-item placements-row' + (selected ? ' is-selected' : '') + '"'
    + ` data-ats-row="${esc(key)}" aria-selected="${selected ? 'true' : 'false'}"`
    + ` data-context-record-id="${esc(key)}" data-context-record-type="placement"`
    + ` data-context-label="${esc(label)}">${inner}</button>`;
}

function renderDetail(r, t) {
  if (!r) return '';
  const status = String(r.status || 'confirmed');
  const active = !TERMINAL_STATUSES.has(status);
  const fields = [
    detailField(t.candidate, r.candidate_id || '—'),
    detailField(t.client, r.client_account_id || '—'),
    detailField(t.placementType, typeLabel(r.placement_type, t)),
    detailField(t.fee, r.fee == null ? '—' : String(r.fee)),
    detailField(t.guarantee, r.guarantee_days == null ? '—' : String(r.guarantee_days)),
    detailField(t.status, statusLabel(status, t)),
  ];
  if (r.fee_invoice_id) fields.push(detailField(t.invoice, r.fee_invoice_id));
  if (r.storno_credit_note_id) fields.push(detailField(t.cancellation, r.storno_credit_note_id));
  return '<section class="placements-detail-card"'
    + ` data-context-record-id="${esc(r.id || '')}" data-context-record-type="placement"`
    + ` data-context-label="${esc(placementTitle(r))}">`
    + `<dl class="ctox-fields">${fields.join('')}</dl>`
    + (active
      ? `<div class="placements-detail-actions"><button type="button" class="ctox-button" data-early-leave="${esc(r.id || '')}">${esc(t.earlyLeave)}</button></div>`
      : '')
    + '</section>';
}

function detailField(k, v) {
  return `<div><dt>${esc(k)}</dt><dd>${esc(v)}</dd></div>`;
}

function bandLabel(grammar, t) {
  const parts = [{ all: t.viewAll, active: t.viewActive, ended: t.viewEnded }[grammar.band] || t.viewAll];
  const type = grammar.filters && grammar.filters.placement_type;
  if (type && type !== 'all') parts.push(type === 'arbeitnehmerueberlassung' ? t.temporary : t.directHire);
  if (grammar.search) parts.push(`"${grammar.search}"`);
  return parts.join(' · ');
}

// Shared blocked-decision renderer for the create flow.
function renderBlocked(result, setGate) {
  const decision = result?.gate || result?.decision || null;
  const blockers = result?.blockers || decision?.blockers || result?.errors || null;
  const blocked = result?.ok === false || result?.status === 'blocked'
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

function esc(v) { return String(v == null ? '' : v).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;'); }
