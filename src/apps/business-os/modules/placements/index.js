const MODULE_ID = 'placements';
const PRIMARY = 'placements';
const CREATE_COMMAND = 'ats.placement.create';
const EARLY_LEAVE_COMMAND = 'ats.placement.early_leave';
// A placement is "ended" once it reaches a terminal state; everything else is
// an active placement running against its guarantee clock.
const TERMINAL_STATUSES = new Set(['early_leave', 'cancelled']);

export const COPY = {
  de: {
    title: 'Vermittlungen', kicker: 'ATS', listTitle: 'Vermittlungen', candidate: 'Kandidat', client: 'Kunde', placementType: 'Vermittlungsart', directHire: 'Festanstellung (Personalvermittlung)', temporary: 'Arbeitnehmerüberlassung (Zeitarbeit)', qualifications: 'Pflicht-Qualifikationen (Komma)', fee: 'Honorar', guarantee: 'Garantie (Tage)', create: 'Anlegen', more: 'Weitere Angaben', entries: 'Einträge', earlyLeaveBooked: 'Frühausstieg verbucht.', clawback: 'Clawback', credit: 'Gutschrift', candidateRequired: 'Kandidat-ID erforderlich.', blocked: 'Blockiert.', placement: 'Placement', feeInvoice: 'Honorar-Rechnung', invoice: 'Rechnung', cancellation: 'Storno', earlyLeave: 'Frühausstieg', status: 'Status',
    allTypes: 'Alle Arten', viewAll: 'Alle', viewActive: 'Aktiv', viewEnded: 'Beendet', composerKicker: 'NEUE VERMITTLUNG', composerTitle: 'Vermittlung anlegen', cancelNew: 'Abbrechen', recordKicker: 'VERMITTLUNG', importDone: 'Import:', invalidFile: 'Datei konnte nicht gelesen werden (JSON erwartet).',
    statusConfirmed: 'bestätigt', statusEarlyLeave: 'Frühausstieg', statusCancelled: 'storniert',
    showAsList: 'Als Liste anzeigen', showAsCards: 'Als Karten anzeigen', localeTag: 'de-DE', start: 'Start',
  },
  en: {
    title: 'Placements', kicker: 'ATS', listTitle: 'Placements', candidate: 'Candidate ID', client: 'Client account ID', placementType: 'Placement type', directHire: 'Permanent placement', temporary: 'Temporary staffing', qualifications: 'Required qualifications (comma-separated)', fee: 'Fee', guarantee: 'Guarantee (days)', create: 'Create', more: 'More details', entries: 'records', earlyLeaveBooked: 'Early leave recorded.', clawback: 'Clawback', credit: 'Credit note', candidateRequired: 'Candidate ID is required.', blocked: 'Blocked.', placement: 'Placement', feeInvoice: 'Fee invoice', invoice: 'Invoice', cancellation: 'Cancellation', earlyLeave: 'Early leave', status: 'Status',
    allTypes: 'All types', viewAll: 'All', viewActive: 'Active', viewEnded: 'Ended', composerKicker: 'NEW PLACEMENT', composerTitle: 'Create placement', cancelNew: 'Cancel', recordKicker: 'PLACEMENT', importDone: 'Import:', invalidFile: 'File could not be read (JSON expected).', // v4-allow: en
    statusConfirmed: 'confirmed', statusEarlyLeave: 'early leave', statusCancelled: 'cancelled',
    showAsList: 'Show as list', showAsCards: 'Show as cards', localeTag: 'en-GB', start: 'Start',
  },
};
let text = COPY.de;

// App-API 4.0 (Shell v4): mount(root, api, {restored}) -> {unmount, snapshot}.
const SNAPSHOT_VERSION = 1;
const FORM_FIELDS = ['candidate_id', 'client_account_id', 'placement_type', 'required_types', 'fee', 'guarantee_days'];

export async function mount(host, api, { restored } = {}) {
  if (!host.dataset.app) host.dataset.app = api.app?.id || MODULE_ID;
  setLocale(api, api.shell.locale.get());
  // CSS laedt die Shell aus dem Manifest; index.html liefert der Service Worker versioniert (§16.15).
  host.innerHTML = await loadMarkup();
  const disposers = [];
  const root = host.querySelector('[data-ats-root]');
  applyStaticCopy(root);
  api.shell.setTitle(text.title);

  const listPane = root?.querySelector('[data-placements-list-pane]');
  const listEl = root?.querySelector('[data-ats-list]');
  const formEl = root?.querySelector('[data-ats-form]');
  const detailEl = root?.querySelector('[data-ats-detail]');
  const wbTitle = root?.querySelector('[data-ats-wb-title]');
  const collapseBtn = root?.querySelector('[data-ats-collapse]');
  const newBtn = root?.querySelector('[data-action="new"]');
  const importBtn = root?.querySelector('[data-action="import"]');
  const exportBtn = root?.querySelector('[data-action="export"]');
  const restore = isPlainObject(restored) && restored.v === SNAPSHOT_VERSION ? restored : null;
  if (restore) restoreView(restore);

  const state = {
    records: [], visible: [],
    selectedId: restore && typeof restore.selectedId === 'string' ? restore.selectedId : '',
    // Anlegen nur über „+ Neu“ (FIX9a): das leere Formular ist ein eigener Modus, nie über einem Datensatz.
    composing: Boolean(restore && restore.composing === true),
    userCollapsed: Boolean(restore && restore.userCollapsed === true),
    grammar: readGrammarState(listPane),
  };
  let pendingScrollTop = restore && Number(restore.listScrollTop) > 0 ? Number(restore.listScrollTop) : 0;

  // Die Spalten-Grammatik verdrahtet die App selbst (keine DOM-Heuristik der Shell).
  const { autoWirePaneGrammar } = await import('ctox:shared/pane-grammar.js');
  autoWirePaneGrammar(root);

  // Zustand nur ueber die Zustandszone der Shell, stabile Codes je Fall.
  const report = (level, code, str, ref) => api.status.report(ref ? { level, code, text: str, ref } : { level, code, text: str });
  const clear = (code) => api.status.clear(code);

  function restoreView(snap) {
    const search = listPane?.querySelector('[data-pg-search]');
    if (search && typeof snap.search === 'string') search.value = snap.search;
    if (listPane && (snap.view === 'list' || snap.view === 'cards')) listPane.dataset.pgDefaultView = snap.view;
    const tabs = [...(listPane?.querySelectorAll('[data-pg-band]') || [])];
    if (typeof snap.band === 'string' && tabs.some((tab) => tab.dataset.pgBand === snap.band)) {
      tabs.forEach((tab) => {
        const on = tab.dataset.pgBand === snap.band;
        tab.setAttribute('aria-selected', String(on));
        tab.classList.toggle('is-active', on);
      });
    }
    const typeSel = listPane?.querySelector('[data-pg-filter][data-pg-name="placement_type"]');
    const type = isPlainObject(snap.filters) ? snap.filters.placement_type : null;
    if (typeSel && typeof type === 'string' && [...typeSel.options].some((o) => o.value === type)) typeSel.value = type;
    const tray = listPane?.querySelector('[data-pg-tray]');
    if (tray && snap.trayOpen === true) { tray.hidden = false; listPane.querySelector('[data-pg-tray-toggle]')?.setAttribute('aria-expanded', 'true'); }
    if (isPlainObject(snap.form)) {
      for (const name of FORM_FIELDS) {
        const el = formEl?.querySelector('[name="' + name + '"]');
        if (el && typeof snap.form[name] === 'string') el.value = snap.form[name];
      }
    }
    const more = formEl?.querySelector('details.ats-more');
    if (more && snap.moreOpen === true) more.open = true;
  }

  function formValues() {
    const out = {};
    for (const name of FORM_FIELDS) out[name] = formEl?.querySelector('[name="' + name + '"]')?.value ?? '';
    return out;
  }

  function selectedRecord() {
    return state.records.find((r) => recordKey(r) === state.selectedId) || null;
  }

  // ---- LEFT column: reactive record list under the pane grammar ------------
  function renderListRegion() {
    const counts = bandCounts(state.records);
    state.visible = filterRecords(state.records, state.grammar);
    const view = state.grammar.view === 'list' ? 'list' : 'cards';
    // Two load-bearing markers, same convention as customers: `.is-list-view`
    // on the well is the app-wide view marker, `.is-compact` on the row (set
    // in renderList) says the row carries the one-line compact markup, so the
    // dense rules can never land on a card-shaped row.
    listEl?.classList.toggle('is-list-view', view === 'list');
    if (listEl) listEl.innerHTML = renderList(state.visible, { view, selectedId: state.selectedId }, text);
    writeCounts(listPane, counts);
    writeFooter(listPane, `${state.visible.length} ${text.entries} · ${bandLabel(state.grammar, text)}`);
    const scroller = listEl?.closest('.ctox-pane-body') || listEl;
    if (pendingScrollTop && scroller && state.records.length) { scroller.scrollTop = pendingScrollTop; pendingScrollTop = 0; }
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
    if (formEl) formEl.hidden = hasSel || !state.composing;
    if (wbTitle) wbTitle.textContent = hasSel ? placementTitle(rec) : (state.composing ? text.composerTitle : '');
    if (detailEl) detailEl.innerHTML = showDetail ? renderDetail(rec, text) : '';
  }


  function selectRecord(id) {
    state.selectedId = id;
    state.userCollapsed = false;
    state.composing = false;
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
    state.composing = true;
    try { formEl?.reset(); } catch {}
    renderListRegion();
    renderWorkbench();
    try { formEl?.querySelector('[name="candidate_id"]')?.focus(); } catch {}
  }

  function cancelNew() {
    state.composing = false;
    try { formEl?.reset(); } catch {}
    renderWorkbench();
  }

  function onCollapseToggle() {
    if (!state.selectedId) return;
    state.userCollapsed = !state.userCollapsed;
    renderWorkbench();
  }

  function setRecords(rows) {
    const list = (Array.isArray(rows) ? rows : []).filter((r) => r && !r._deleted);
    list.sort((a, b) => (b.updated_at_ms || 0) - (a.updated_at_ms || 0));
    state.records = list;
    renderListRegion();
    renderWorkbench();
  }

  // ---- Selection (left list) ------------------------------------------------
  function onListClick(event) {
    const row = event.target?.closest?.('[data-ats-row]');
    if (!row || !listEl?.contains(row)) return;
    selectRecord(row.getAttribute('data-ats-row') || '');
  }

  // ---- View: ONE button, one action (Betreiber-Direktive 31.08.2026) --------
  // The grammar models the view as N aria-pressed buttons, which a single
  // action button cannot express, so the module owns the flip and republishes
  // it through the pane's data-pg-default-view.
  const viewToggleBtn = root?.querySelector('[data-placements-view-toggle]');
  function onViewToggle() {
    state.grammar = { ...state.grammar, view: state.grammar.view === 'list' ? 'cards' : 'list' };
    syncViewToggle(listPane, state.grammar.view, text);
    renderListRegion();
  }

  // Re-render the list on any grammar change (search/view/tray/band).
  function onGrammarChange(event) {
    if (!listPane || !listPane.contains(event.target)) return;
    const detail = event.detail || readGrammarState(listPane);
    // The grammar's `view` comes from the pane default we keep in sync above;
    // never let an absent/stale value reset the operator's chosen view.
    const view = detail.view === 'list' || detail.view === 'cards' ? detail.view : state.grammar.view;
    state.grammar = { ...detail, view };
    renderListRegion();
  }

  // Befehl: api.command legt das Befehlsdokument lokal an und liefert sofort; die UI wartet nie.
  // Fehlschlag → Zone (error). Ergebnis (done) wertet onDone aus.
  function sendCommand(type, payload, onDone) {
    const handle = api.command(type, payload);
    const code = 'placements.command.' + type;
    let off = null;
    let finished = false;
    const stop = () => { finished = true; off?.(); off = null; };
    off = handle.status.subscribe((s) => {
      if (s?.status === 'failed') {
        report('error', code, type + ': ' + (s.error || 'failed'), handle.id);
        stop();
      } else if (s?.status === 'done') {
        clear(code);
        try { onDone?.(s.result, handle); } catch (e) { console.error('[placements] result failed:', e); }
        stop();
      }
    });
    if (finished) off?.();
    handle.local.catch((e) => report('error', code, type + ': ' + (e?.code || 'local') + ' ' + (e?.message || e), handle.id));
    disposers.push(stop);
    return handle;
  }

  // ---- Early-leave flow (unchanged command type + payload) ------------------
  // Statuswechsel und Gutschrift erscheinen live am Datensatz; der Clawback-Betrag steht nur im
  // Befehlsergebnis → Zustandszone (info).
  function onDetailClick(event) {
    const btn = event.target?.closest?.('[data-early-leave]');
    if (!btn) return;
    const placementId = btn.getAttribute('data-early-leave');
    if (!placementId) return;
    sendCommand(EARLY_LEAVE_COMMAND, { placement_id: placementId, left_at_ms: Date.now() }, (result) => {
      const cn = result?.credit_note_id ?? result?.data?.credit_note_id ?? null;
      const clawback = result?.clawback ?? result?.data?.clawback ?? null;
      report('info', 'placements.early_leave.result', [
        text.earlyLeaveBooked,
        clawback != null ? `${text.clawback}: ${clawback}` : '',
        cn ? `${text.credit}: ${cn}` : '',
      ].filter(Boolean).join(' · '), placementId);
    });
  }

  // ---- Create flow (unchanged command type + payload) -----------------------
  function onSubmit(event) {
    event.preventDefault();
    const data = new FormData(formEl);
    const f = Object.fromEntries(data.entries());
    const candidate_id = String(f.candidate_id || '').trim();
    if (!candidate_id) { report('warn', 'placements.form.candidate_required', text.candidateRequired); return; }
    clear('placements.form.candidate_required');
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
    if (payload.required_types === undefined) delete payload.required_types;
    const submitted = JSON.stringify(formValues());
    sendCommand(CREATE_COMMAND, payload, (result) => {
      const blockers = blockedReasons(result);
      if (blockers) {
        report('warn', 'placements.create.blocked', text.blocked + (blockers.length ? ' ' + blockers.join(' · ') : ''), candidate_id);
        return;
      }
      clear('placements.create.blocked');
      // Die neue Vermittlung erscheint live in der Liste (Rechnungs-ID am Datensatz); Formular
      // nur leeren, wenn seit dem Absenden nichts Neues eingegeben wurde.
      if (JSON.stringify(formValues()) === submitted) {
        try { formEl.reset(); } catch {}
        state.composing = false;
        renderWorkbench();
      }
    });
  }

  function onExport() {
    const rows = state.visible.length ? state.visible : state.records;
    let url = '';
    try {
      const blob = new Blob([JSON.stringify(rows, null, 2)], { type: 'application/json' });
      url = URL.createObjectURL(blob);
      const anchor = document.createElement('a');
      anchor.href = url;
      anchor.download = `placements-${rows.length}.json`;
      anchor.style.display = 'none';
      host.appendChild(anchor);
      anchor.click();
      anchor.remove();
    } catch (e) {
      console.error('[placements] export failed:', e);
    } finally {
      if (url) { try { URL.revokeObjectURL(url); } catch {} }
    }
  }

  async function importFile(file) {
    let parsed;
    try {
      parsed = JSON.parse(await file.text());
    } catch (e) {
      console.error('[placements] import parse failed:', e);
      report('error', 'placements.import.invalid', text.invalidFile);
      return;
    }
    clear('placements.import.invalid');
    const items = Array.isArray(parsed) ? parsed : (Array.isArray(parsed?.records) ? parsed.records : [parsed]);
    let ok = 0;
    for (const item of items) {
      const record = normalizePlacement(item, { nowMs: Date.now() });
      if (!record.candidate_id) continue;
      try { await api.put(PRIMARY, record).local; ok += 1; } catch (e) { console.error('[placements] import write failed:', e); }
    }
    if (ok < items.length) report('warn', 'placements.import.partial', `${text.importDone} ${ok}/${items.length} ${text.entries}`);
    else clear('placements.import.partial');
  }

  function onImport() {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'application/json,.json';
    input.addEventListener('change', () => {
      const file = input.files && input.files[0];
      if (file) importFile(file);
    }, { once: true });
    input.click();
  }

  listEl?.addEventListener('click', onListClick);
  viewToggleBtn?.addEventListener('click', onViewToggle);
  syncViewToggle(listPane, state.grammar.view, text);
  root?.addEventListener('ctox-pane-grammar-change', onGrammarChange);
  detailEl?.addEventListener('click', onDetailClick);
  formEl?.addEventListener('submit', onSubmit);
  newBtn?.addEventListener('click', startNew);
  const cancelBtn = root?.querySelector('[data-action="cancel-new"]');
  cancelBtn?.addEventListener('click', cancelNew);
  collapseBtn?.addEventListener('click', onCollapseToggle);
  exportBtn?.addEventListener('click', onExport);
  importBtn?.addEventListener('click', onImport);

  // ---- Reactive backbone: Live-Abo statt Laden/Polling ----------------------
  disposers.push(api.query(PRIMARY, {}).subscribe(setRecords));
  disposers.push(api.shell.locale.subscribe((next) => {
    if (!setLocale(api, next)) return;
    applyStaticCopy(root);
    api.shell.setTitle(text.title);
    syncViewToggle(listPane, state.grammar.view, text);
    renderListRegion();
    renderWorkbench();
  }));

  return {
    unmount() {
      for (const dispose of disposers.splice(0)) {
        try { dispose(); } catch (e) { console.warn('[placements] unsubscribe failed', e); }
      }
      formEl?.removeEventListener('submit', onSubmit);
      listEl?.removeEventListener('click', onListClick);
      viewToggleBtn?.removeEventListener('click', onViewToggle);
      detailEl?.removeEventListener('click', onDetailClick);
      root?.removeEventListener('ctox-pane-grammar-change', onGrammarChange);
      newBtn?.removeEventListener('click', startNew);
      cancelBtn?.removeEventListener('click', cancelNew);
      collapseBtn?.removeEventListener('click', onCollapseToggle);
      exportBtn?.removeEventListener('click', onExport);
      importBtn?.removeEventListener('click', onImport);
      host.replaceChildren();
    },
    snapshot() {
      const g = readGrammarState(listPane);
      return {
        v: SNAPSHOT_VERSION,
        selectedId: state.selectedId,
        userCollapsed: state.userCollapsed,
        search: listPane?.querySelector('[data-pg-search]')?.value || '',
        view: state.grammar.view === 'list' ? 'list' : 'cards',
        band: g.band,
        filters: { ...g.filters },
        trayOpen: listPane?.querySelector('[data-pg-tray]')?.hidden === false,
        form: formValues(),
        composing: state.composing,
        moreOpen: Boolean(formEl?.querySelector('details.ats-more')?.open),
        listScrollTop: (listEl?.closest('.ctox-pane-body') || listEl)?.scrollTop || 0,
      };
    },
  };
}

function isPlainObject(v) { return Boolean(v) && typeof v === 'object' && !Array.isArray(v); }

// Sprache: api.shell.locale (live) + api.shell.t('placements.<key>') uebersteuert die eingebetteten Texte.
let currentLang = 'de';
function setLocale(api, value) {
  const next = String(value || '').toLowerCase().startsWith('en') ? 'en' : 'de';
  const changed = next !== currentLang || text === COPY[currentLang];
  currentLang = next;
  const base = COPY[next];
  const out = {};
  for (const key of Object.keys(base)) {
    const k = MODULE_ID + '.' + key;
    let v = null;
    try { v = api?.shell?.t?.(k); } catch {}
    out[key] = typeof v === 'string' && v && v !== k ? v : base[key];
  }
  text = out;
  return changed;
}

// Ergebnis von ats.placement.create: blockiert → Liste der Gruende (ggf. leer), sonst null.
export function blockedReasons(result) {
  const decision = result?.gate || result?.decision || null;
  const blockers = result?.blockers || decision?.blockers || result?.errors || null;
  const blocked = result?.ok === false || result?.status === 'blocked'
    || decision?.status === 'blocked' || decision?.decision === 'block'
    || (Array.isArray(blockers) && blockers.length > 0);
  if (!blocked) return null;
  return (Array.isArray(blockers) ? blockers : [blockers])
    .filter(Boolean)
    .map((b) => (typeof b === 'string' ? b : (b?.message || b?.reason || JSON.stringify(b))));
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

// Record list: rows only. An empty list renders nothing (App-API 4.0: no
// state text in the content; state lives only in the shell status zone).
export function renderList(records, opts = {}, t = text) {
  const view = opts.view === 'list' ? 'list' : 'cards';
  const selectedId = opts.selectedId || '';
  return (records || []).map((r) => (view === 'list' ? shardCompact(r, selectedId, t) : shardCard(r, selectedId, t))).join('');
}

function recordKey(r) { return (r && r.id) || ''; }

// Namen statt IDs (Textdiät Regel 4): Anzeigename aus dem Datensatz (<basis>_name / <basis>.name / <basis>_title),
// sonst „Unbekannt“ – nie die technische ID.
function refName(r, base) {
  const v = r?.[base + '_name'] ?? r?.[base]?.name ?? r?.[base + '_title'];
  return v ? String(v) : (currentLang === 'en' ? 'Unknown' : 'Unbekannt'); // v4-allow: en
}

function placementTitle(r) {
  return `${refName(r, 'candidate')} → ${refName(r, 'client')}`;
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

// Short day stamp for the shard meta lines; invalid/absent timestamps stay out
// of the line instead of rendering a placeholder.
function formatDay(ms, t) {
  const value = Number(ms);
  if (!Number.isFinite(value) || value <= 0) return '';
  try { return new Date(value).toLocaleDateString(t.localeTag || 'de-DE', { day: '2-digit', month: '2-digit', year: 'numeric' }); }
  catch { return ''; }
}

// KARTEN: the record at a glance — bold title line plus up to two muted meta
// lines built from the detail fields the module already loads (type, fee,
// guarantee clock, start day, and the booked invoice/credit note).
function shardCard(r, selectedId, t) {
  const key = recordKey(r);
  const status = String(r.status || 'confirmed');
  const badge = ('ctox-badge ' + statusBadgeClass(status)).trim();
  const primary = [
    typeLabel(r.placement_type, t),
    r.fee == null ? null : `${t.fee}: ${r.fee}`,
  ].filter(Boolean).map(esc).join(' · ');
  const secondary = [
    r.guarantee_days == null ? null : `${t.guarantee}: ${r.guarantee_days}`,
    formatDay(r.start_ms, t) ? `${t.start}: ${formatDay(r.start_ms, t)}` : null,
    r.fee_invoice_id ? `${t.invoice}: ${r.fee_invoice_id}` : null,
    r.storno_credit_note_id ? `${t.cancellation}: ${r.storno_credit_note_id}` : null,
  ].filter(Boolean).map(esc).join(' · ');
  return rowShell(r, key, selectedId, false,
    '<div class="placements-shard">'
    + '<div class="placements-shard-title">'
    + `<span class="${badge}" data-status="${esc(status)}">${esc(statusLabel(status, t))}</span>`
    + `<strong>${esc(placementTitle(r))}</strong>`
    + '</div>'
    + (primary ? `<small class="placements-shard-meta">${primary}</small>` : '')
    + (secondary ? `<small class="placements-shard-meta">${secondary}</small>` : '')
    + '</div>');
}

// LISTE: exactly ONE dense line per record — title plus a single short meta
// on the right. No badge, no second line; the density is the point.
function shardCompact(r, selectedId, t) {
  const key = recordKey(r);
  const status = String(r.status || 'confirmed');
  return rowShell(r, key, selectedId, true,
    '<div class="placements-row-compact">'
    + `<span class="placements-compact-title">${esc(placementTitle(r))}</span>`
    + `<span class="placements-compact-tag" data-status="${esc(status)}">${esc(statusLabel(status, t))}</span>`
    + '</div>');
}

function rowShell(r, key, selectedId, compact, inner) {
  const label = placementTitle(r);
  const selected = key && key === selectedId;
  return '<button type="button" class="ctox-list-item placements-row' + (selected ? ' is-selected' : '') + (compact ? ' is-compact' : '') + '"'
    + ` data-ats-row="${esc(key)}" aria-selected="${selected ? 'true' : 'false'}"`
    + ` data-context-record-id="${esc(key)}" data-context-record-type="placement"`
    + ` data-context-label="${esc(label)}">${inner}</button>`;
}

function renderDetail(r, t) {
  if (!r) return '';
  const status = String(r.status || 'confirmed');
  const active = !TERMINAL_STATUSES.has(status);
  const fields = [
    detailField(t.candidate, refName(r, 'candidate')),
    detailField(t.client, refName(r, 'client')),
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

// Ein-Knopf-Umschalter: das Icon zeigt die Ansicht, zu der gewechselt wird.
// Kein aria-pressed - der Knopf ist eine Aktion, kein Zustand.
const VIEW_TOGGLE_ICONS = Object.freeze({
  // Ziel "Liste": drei Zeilen.
  list: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="4" y1="6" x2="20" y2="6"/><line x1="4" y1="12" x2="20" y2="12"/><line x1="4" y1="18" x2="20" y2="18"/></svg>',
  // Ziel "Karten": zwei Shards.
  cards: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="4" y="4" width="16" height="7" rx="1.5"/><rect x="4" y="14" width="16" height="7" rx="1.5"/></svg>',
});

// Publishes the module-owned view back onto the pane so the shell grammar's
// `data-pg-default-view` fallback keeps reporting the view the operator sees,
// and repaints the single toggle for the view it switches TO.
function syncViewToggle(pane, view, t) {
  if (!pane) return;
  const current = view === 'list' ? 'list' : 'cards';
  pane.dataset.pgDefaultView = current;
  const button = pane.querySelector('[data-placements-view-toggle]');
  if (!button) return;
  const next = current === 'list' ? 'cards' : 'list';
  const label = next === 'list' ? t.showAsList : t.showAsCards;
  button.dataset.placementsView = current;
  button.setAttribute('aria-label', label);
  button.setAttribute('title', label);
  button.removeAttribute('aria-pressed');
  const icon = VIEW_TOGGLE_ICONS[next];
  if (icon && button.innerHTML !== icon) button.innerHTML = icon;
}

// Read the current grammar state straight from the DOM so the module never
// depends on the shell having wired __ctoxPaneGrammar yet.
function readGrammarState(pane) {
  if (!pane) return { search: '', view: 'cards', band: 'all', filters: {} };
  const search = pane.querySelector('[data-pg-search]');
  // The view belongs to the module's one-button toggle; `data-pg-default-view`
  // on the pane is the single place both sides read it from.
  const view = pane.dataset.pgDefaultView === 'list' ? 'list' : 'cards';
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

async function loadMarkup() {
  const html = await fetch(new URL('./index.html', import.meta.url)).then((r) => r.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((n) => n.remove());
  return doc.body.innerHTML;
}

function esc(v) { return String(v == null ? '' : v).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;'); }
