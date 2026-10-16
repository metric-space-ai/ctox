const MODULE_ID = 'submissions';
const PRIMARY = 'submissions';
const PRESENT_COMMAND = 'ats.submission.present';
// A submission is "done" once the client outcome is terminal; anything else
// (freshly sent, awaiting feedback) is open.
const TERMINAL_STATUSES = new Set(['hired', 'withdrawn', 'rejected']);
const STATUS_KEY = {
  sent: 'statusSent', hired: 'statusHired', withdrawn: 'statusWithdrawn', rejected: 'statusRejected',
};

export const COPY = {
  de: {
    title: 'Vorstellungen', candidate: 'Kandidat', client: 'Kunde', more: 'Weitere Angaben', present: 'Vorstellen', entries: 'Einträge', idCopied: 'ID kopiert', blocked: 'Blockiert.', idsRequired: 'Kandidat-ID und Kunden-Account-ID sind erforderlich.', submission: 'Submission', vacancyLabel: 'Vakanz', contactLabel: 'Kontakt', consent: 'Consent', feedback: 'Feedback', sent: 'Gesendet', copyId: 'ID kopieren', unknown: 'unbekannt', conflict: 'Konflikt', status: 'Status',
    kicker: '', listTitle: 'Vorstellungen', allStatus: 'Alle Status', viewAll: 'Alle', viewOpen: 'Offen', viewDone: 'Erledigt', composerKicker: 'NEUE VORSTELLUNG', composerTitle: 'Kandidat vorstellen', cancelNew: 'Abbrechen', recordKicker: 'VORSTELLUNG', importDone: 'Import:', invalidFile: 'Datei konnte nicht gelesen werden (JSON erwartet).',
    statusSent: 'gesendet', statusHired: 'eingestellt', statusWithdrawn: 'zurückgezogen', statusRejected: 'abgelehnt',
    showAsList: 'Als Liste anzeigen', showAsCards: 'Als Karten anzeigen', localeTag: 'de-DE',
  },
  en: {
    title: 'Candidate presentations', candidate: 'Candidate ID', client: 'Client account ID', more: 'More details', present: 'Present candidate', entries: 'records', idCopied: 'ID copied', blocked: 'Blocked.', idsRequired: 'Candidate ID and client account ID are required.', submission: 'Submission', vacancyLabel: 'Vacancy', contactLabel: 'Contact', consent: 'Consent', feedback: 'Feedback', sent: 'Sent', copyId: 'Copy ID', unknown: 'unknown', conflict: 'Conflict', status: 'Status',
    kicker: 'SUBMISSIONS', listTitle: 'Candidates', allStatus: 'All statuses', viewAll: 'All', viewOpen: 'Open', viewDone: 'Closed', composerKicker: 'NEW SUBMISSION', composerTitle: 'Present candidate', cancelNew: 'Cancel', recordKicker: 'SUBMISSION', importDone: 'Import:', invalidFile: 'File could not be read (JSON expected).', // v4-allow: en
    statusSent: 'sent', statusHired: 'hired', statusWithdrawn: 'withdrawn', statusRejected: 'rejected',
    showAsList: 'Show as list', showAsCards: 'Show as cards', localeTag: 'en-GB',
  },
};
let text = COPY.de;
let locale = 'de';

// App-API 4.0 (Shell v4): mount(root, api, {restored}) -> {unmount, snapshot}.
const SNAPSHOT_VERSION = 1;
const FORM_FIELDS = ['candidate_id', 'client_account_id', 'vacancy_id', 'client_contact_id'];

export async function mount(host, api, { restored } = {}) {
  if (!host.dataset.app) host.dataset.app = api.app?.id || MODULE_ID;
  setLocale(api, api.shell.locale.get());
  // CSS laedt die Shell aus dem Manifest; index.html liefert der Service Worker versioniert (§16.15).
  host.innerHTML = await loadMarkup();
  const disposers = [];
  const root = host.querySelector('[data-subs-root]');
  applyStaticCopy(root);
  api.shell.setTitle(text.title);

  const listPane = root?.querySelector('[data-subs-list-pane]');
  const listEl = root?.querySelector('[data-subs-list]');
  const formEl = root?.querySelector('[data-subs-form]');
  const detailEl = root?.querySelector('[data-subs-detail]');
  const wbTitle = root?.querySelector('[data-subs-wb-title]');
  const collapseBtn = root?.querySelector('[data-subs-collapse]');
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
    const statusSel = listPane?.querySelector('[data-pg-filter][data-pg-name="status"]');
    const status = isPlainObject(snap.filters) ? snap.filters.status : null;
    if (statusSel && typeof status === 'string' && [...statusSel.options].some((o) => o.value === status)) statusSel.value = status;
    const tray = listPane?.querySelector('[data-pg-tray]');
    if (tray && snap.trayOpen === true) { tray.hidden = false; listPane.querySelector('[data-pg-tray-toggle]')?.setAttribute('aria-expanded', 'true'); }
    if (isPlainObject(snap.form)) {
      for (const name of FORM_FIELDS) {
        const el = formEl?.querySelector('[name="' + name + '"]');
        if (el && typeof snap.form[name] === 'string') el.value = snap.form[name];
      }
    }
    const more = formEl?.querySelector('details.subs-more');
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
    if (wbTitle) wbTitle.textContent = hasSel ? refName(rec, 'candidate') : (state.composing ? text.composerTitle : '');
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
    listEl?.querySelectorAll('[data-subs-row]').forEach((row) => {
      const on = (row.getAttribute('data-subs-row') || '') === String(state.selectedId || '');
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
    list.sort((a, b) => Number(b?.updated_at_ms || b?.sent_at_ms || b?.created_at_ms || 0) - Number(a?.updated_at_ms || a?.sent_at_ms || a?.created_at_ms || 0));
    state.records = list;
    renderListRegion();
    renderWorkbench();
  }

  // ---- Selection (left list) ------------------------------------------------
  function onListClick(event) {
    const row = event.target?.closest?.('[data-subs-row]');
    if (!row || !listEl?.contains(row)) return;
    selectRecord(row.getAttribute('data-subs-row') || '');
  }

  // ---- View: ONE button, one action (Betreiber-Direktive 31.08.2026) --------
  // The grammar models the view as N aria-pressed buttons, which a single
  // action button cannot express, so the module owns the flip and republishes
  // it through the pane's data-pg-default-view.
  const viewToggleBtn = root?.querySelector('[data-subs-view-toggle]');
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
    const code = 'submissions.command.' + type;
    let off = null;
    let finished = false;
    const stop = () => { finished = true; off?.(); off = null; };
    off = handle.status.subscribe((s) => {
      if (s?.status === 'failed') {
        report('error', code, type + ': ' + (s.error || 'failed'), handle.id);
        stop();
      } else if (s?.status === 'done') {
        clear(code);
        try { onDone?.(s.result, handle); } catch (e) { console.error('[submissions] result failed:', e); }
        stop();
      }
    });
    if (finished) off?.();
    handle.local.catch((e) => report('error', code, type + ': ' + (e?.code || 'local') + ' ' + (e?.message || e), handle.id));
    disposers.push(stop);
    return handle;
  }

  // ---- Record detail: local, non-dispatching id copy (the module has exactly
  // one native command, ats.submission.present; there is no withdraw/feedback
  // handler, so we never fabricate a server command). Die Bestaetigung geht in
  // die Zustandszone (info), nicht in den Inhalt. ----------------------------
  async function onDetailClick(event) {
    const copyBtn = event.target?.closest?.('[data-copy-id]');
    if (!copyBtn) return;
    const id = copyBtn.getAttribute('data-copy-id') || '';
    if (!id) return;
    try { await navigator.clipboard?.writeText?.(id); } catch {}
    report('info', 'submissions.id.copied', `${text.idCopied}: ${id}`, id);
  }

  // ---- Present flow (unchanged command type + payload) ----------------------
  function onSubmit(event) {
    event.preventDefault();
    const data = new FormData(formEl);
    const f = Object.fromEntries(data.entries());
    const candidate_id = String(f.candidate_id || '').trim();
    const client_account_id = String(f.client_account_id || '').trim();
    if (!candidate_id || !client_account_id) { report('warn', 'submissions.form.ids_required', text.idsRequired); return; }
    clear('submissions.form.ids_required');
    const payload = {
      candidate_id,
      client_account_id,
      vacancy_id: String(f.vacancy_id || '').trim() || null,
      client_contact_id: String(f.client_contact_id || '').trim() || null,
    };
    const submitted = JSON.stringify(formValues());
    sendCommand(PRESENT_COMMAND, payload, (outcome) => {
      const blockers = presentBlockers(outcome);
      if (blockers) {
        report('warn', 'submissions.present.blocked', text.blocked + (blockers.length ? ' ' + blockers.join(' · ') : ''), candidate_id);
        return;
      }
      clear('submissions.present.blocked');
      // Die neue Vorstellung erscheint live in der Liste. Zurueck in den Anlege-Modus (present legt
      // immer neu an), aber nur, wenn seit dem Absenden nichts Neues eingegeben wurde.
      if (JSON.stringify(formValues()) === submitted) {
        state.selectedId = '';
        state.userCollapsed = false;
        state.composing = true;
        try { formEl.reset(); } catch {}
        renderListRegion();
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
      anchor.download = `submissions-${rows.length}.json`;
      anchor.style.display = 'none';
      host.appendChild(anchor);
      anchor.click();
      anchor.remove();
    } catch (e) {
      console.error('[submissions] export failed:', e);
    } finally {
      if (url) { try { URL.revokeObjectURL(url); } catch {} }
    }
  }

  async function importFile(file) {
    let parsed;
    try {
      parsed = JSON.parse(await file.text());
    } catch (e) {
      console.error('[submissions] import parse failed:', e);
      report('error', 'submissions.import.invalid', text.invalidFile);
      return;
    }
    clear('submissions.import.invalid');
    const items = Array.isArray(parsed) ? parsed : (Array.isArray(parsed?.records) ? parsed.records : [parsed]);
    let ok = 0;
    for (const item of items) {
      const record = normalizeSubmission(item, { nowMs: Date.now() });
      if (!record.candidate_id || !record.client_account_id) continue;
      try { await api.put(PRIMARY, record).local; ok += 1; } catch (e) { console.error('[submissions] import write failed:', e); }
    }
    if (ok < items.length) report('warn', 'submissions.import.partial', `${text.importDone} ${ok}/${items.length} ${text.entries}`);
    else clear('submissions.import.partial');
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
        try { dispose(); } catch (e) { console.warn('[submissions] unsubscribe failed', e); }
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
        moreOpen: Boolean(formEl?.querySelector('details.subs-more')?.open),
        listScrollTop: (listEl?.closest('.ctox-pane-body') || listEl)?.scrollTop || 0,
      };
    },
  };
}

function isPlainObject(v) { return Boolean(v) && typeof v === 'object' && !Array.isArray(v); }

// Sprache: api.shell.locale (live) + api.shell.t('submissions.<key>') uebersteuert die eingebetteten Texte.
let currentLang = 'de';
function setLocale(api, value) {
  const next = String(value || '').toLowerCase().startsWith('en') ? 'en' : 'de';
  const changed = next !== currentLang || text === COPY[currentLang];
  currentLang = next;
  locale = next;
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

// Ergebnis von ats.submission.present (`outcome.result ?? outcome`, wie vorher): blockiert → Gruende, sonst null.
export function presentBlockers(outcome) {
  const result = outcome?.result ?? outcome ?? {};
  const decision = result?.gate || result?.decision || null;
  const blockers = result?.blockers || decision?.blockers || result?.errors || null;
  const blocked = result?.ok === false
    || result?.allowed === false
    || result?.status === 'blocked'
    || decision?.decision === 'block'
    || (Array.isArray(blockers) && blockers.length > 0);
  if (!blocked) return null;
  return (Array.isArray(blockers) ? blockers : [blockers]).filter(Boolean).map(blockerText);
}

// ---------------------------------------------------------------------------
// Pure helpers (exported for tests) — no DOM, no RxDB.
// ---------------------------------------------------------------------------

// Coarse lifecycle band derived from the submission status field.
export function lifecycleBand(status) {
  return TERMINAL_STATUSES.has(String(status || 'sent')) ? 'done' : 'open';
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
  const status = (grammar.filters && grammar.filters.status) || 'all';
  return (records || []).filter((r) => {
    if (!r) return false;
    if (band !== 'all' && lifecycleBand(r.status) !== band) return false;
    if (status !== 'all' && String(r.status || 'sent') !== status) return false;
    if (search) {
      const hay = `${r.candidate_id || ''} ${r.client_account_id || ''} ${r.id || ''} ${r.vacancy_id || ''} ${r.client_contact_id || ''} ${r.status || ''}`.toLowerCase();
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

// Normalize an imported object into a schema-valid submissions record.
export function normalizeSubmission(raw, opts = {}) {
  const nowMs = Number.isFinite(opts.nowMs) ? opts.nowMs : Date.now();
  const src = raw && typeof raw === 'object' ? raw : {};
  const id = String(src.id || src.submission_id || '').trim() || `sub_${nowMs}_${Math.random().toString(36).slice(2, 8)}`;
  const record = {
    id,
    candidate_id: String(src.candidate_id || '').trim(),
    client_account_id: String(src.client_account_id || '').trim(),
    status: String(src.status || 'sent'),
    created_at_ms: Number.isFinite(Number(src.created_at_ms)) ? Number(src.created_at_ms) : nowMs,
    updated_at_ms: Number.isFinite(Number(src.updated_at_ms)) ? Number(src.updated_at_ms) : nowMs,
  };
  if (String(src.vacancy_id || '').trim()) record.vacancy_id = String(src.vacancy_id).trim();
  if (String(src.client_contact_id || '').trim()) record.client_contact_id = String(src.client_contact_id).trim();
  if (String(src.consent_id || '').trim()) record.consent_id = String(src.consent_id).trim();
  if (Number.isFinite(Number(src.sent_at_ms))) record.sent_at_ms = Number(src.sent_at_ms);
  if (src.feedback && typeof src.feedback === 'object') record.feedback = src.feedback;
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

function statusLabel(status, t) {
  const key = STATUS_KEY[status];
  return (key && t[key]) || status || '—';
}

// Maps a submission status onto the kit badge states.
function statusBadgeClass(status) {
  if (status === 'hired') return 'is-success';
  if (status === 'sent') return 'is-warning';
  if (status === 'rejected') return 'is-danger';
  if (status === 'withdrawn') return 'is-warning';
  return '';
}

// KARTEN: 2 to 3 lines — a bold title line carrying the status badge, then up
// to two muted meta lines built from the record's own detail fields (client,
// vacancy, contact, sent date, client feedback). Roomier padding, so a record
// can be read without selecting it (Betreiber-Direktive 31.08.2026).
function shardCard(r, selectedId, t) {
  const key = recordKey(r);
  const status = String(r.status || 'sent');
  const outcome = r.feedback && typeof r.feedback === 'object' ? r.feedback.outcome : '';
  // The account id is self-describing (ACC-…), so it carries no label; the
  // selector column is 380px and a labelled chip would ellipsis the vacancy.
  const primary = [
    r.client_account_id ? refName(r, 'client') : null,
    r.vacancy_id ? `${t.vacancyLabel}: ${refName(r, 'vacancy')}` : null,
  ].filter(Boolean).map(esc).join(' · ');
  const secondary = [
    fmtDay(r.sent_at_ms || r.created_at_ms) ? `${t.sent}: ${fmtDay(r.sent_at_ms || r.created_at_ms)}` : null,
    outcome ? `${t.feedback}: ${outcome}` : null,
    r.client_contact_id ? `${t.contactLabel}: ${refName(r, 'client_contact')}` : null,
  ].filter(Boolean).map(esc).join(' · ');
  const badge = ('ctox-badge ' + statusBadgeClass(status)).trim();
  return rowShell(r, key, selectedId, false,
    '<div class="subs-shard">'
    + '<div class="subs-shard-title">'
    + `<span class="${badge}" data-status="${esc(status)}">${esc(statusLabel(status, t))}</span>`
    + `<strong>${esc(refName(r, 'candidate'))}</strong>`
    + '</div>'
    + (primary ? `<small class="subs-shard-meta">${primary}</small>` : '')
    + (secondary ? `<small class="subs-shard-meta">${secondary}</small>` : '')
    + '</div>');
}

// LISTE: exactly ONE dense line per record — title left, one short meta on the
// right. No badge, no second line; the density is the point.
function shardCompact(r, selectedId, t) {
  const key = recordKey(r);
  const status = String(r.status || 'sent');
  return rowShell(r, key, selectedId, true,
    '<div class="subs-row-compact">'
    + `<span class="subs-compact-title">${esc(refName(r, 'candidate'))}</span>`
    + `<span class="subs-compact-tag" data-status="${esc(status)}">${esc(statusLabel(status, t))}</span>`
    + '</div>');
}

// Namen statt IDs (Textdiät Regel 4): Anzeigename aus dem Datensatz (<basis>_name / <basis>.name / <basis>_title),
// sonst „Unbekannt“ – nie die technische ID.
function refName(r, base) {
  const v = r?.[base + '_name'] ?? r?.[base]?.name ?? r?.[base + '_title'];
  return v ? String(v) : (locale === 'en' ? 'Unknown' : 'Unbekannt'); // v4-allow: en
}

function rowShell(r, key, selectedId, compact, inner) {
  const label = refName(r, 'candidate');
  const selected = key && key === selectedId;
  return '<button type="button" class="ctox-list-item subs-row' + (selected ? ' is-selected' : '') + (compact ? ' is-compact' : '') + '"'
    + ` data-subs-row="${esc(key)}" aria-selected="${selected ? 'true' : 'false'}"`
    + ` data-context-record-id="${esc(key)}" data-context-record-type="submission"`
    + ` data-context-label="${esc(label)}">${inner}</button>`;
}

function renderDetail(r, t) {
  if (!r) return '';
  const status = String(r.status || 'sent');
  const outcome = r.feedback && typeof r.feedback === 'object' ? r.feedback.outcome : '';
  const sentAt = fmtTime(r.sent_at_ms || r.created_at_ms);
  const fields = [
    detailField(t.candidate, refName(r, 'candidate')),
    detailField(t.client, refName(r, 'client')),
    detailField(t.status, statusLabel(status, t)),
  ];
  if (outcome) fields.push(detailField(t.feedback, outcome));
  if (sentAt) fields.push(detailField(t.sent, sentAt));
  if (r.vacancy_id) fields.push(detailField(t.vacancyLabel, refName(r, 'vacancy')));
  if (r.client_contact_id) fields.push(detailField(t.contactLabel, refName(r, 'client_contact')));
  // Technische Kennungen (Vorstellungs-ID, Consent-ID, „ID kopieren“) stehen nicht im Detail (Regel 4).
  const copy = '';
  return '<section class="subs-detail-card"'
    + ` data-context-record-id="${esc(r.id || '')}" data-context-record-type="submission"`
    + ` data-context-label="${esc(refName(r, 'candidate'))}">`
    + `<div class="subs-detail-actions">${copy}</div>`
    + `<dl class="ctox-fields">${fields.join('')}</dl>`
    + '</section>';
}

function detailField(k, v) {
  return `<div><dt>${esc(k)}</dt><dd>${esc(v)}</dd></div>`;
}

function bandLabel(grammar, t) {
  const parts = [{ all: t.viewAll, open: t.viewOpen, done: t.viewDone }[grammar.band] || t.viewAll];
  const status = grammar.filters && grammar.filters.status;
  if (status && status !== 'all') parts.push(statusLabel(status, t));
  if (grammar.search) parts.push(`"${grammar.search}"`);
  return parts.join(' · ');
}

function blockerText(b) {
  if (b == null) return text.unknown;
  if (typeof b === 'string') return b;
  const reason = b.reason || b.message || text.unknown;
  if (b.conflicting_submission_id) return reason + ` (${text.conflict}: ` + b.conflicting_submission_id + ')';
  return reason;
}

function fmtTime(ms) {
  const n = Number(ms);
  if (!Number.isFinite(n) || n <= 0) return '';
  try { return new Date(n).toLocaleString(locale === 'en' ? 'en' : 'de', { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' }); } catch { return ''; }
}

// Card meta lines carry the day only — a full timestamp would wrap the line.
function fmtDay(ms) {
  const n = Number(ms);
  if (!Number.isFinite(n) || n <= 0) return '';
  try { return new Date(n).toLocaleDateString(locale === 'en' ? 'en-GB' : 'de-DE'); } catch { return ''; }
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
  const button = pane.querySelector('[data-subs-view-toggle]');
  if (!button) return;
  const next = current === 'list' ? 'cards' : 'list';
  const label = next === 'list' ? t.showAsList : t.showAsCards;
  button.dataset.subsView = current;
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

function esc(v) { return String(v == null ? '' : v).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;'); }
