const MOD_BUILD = '20260721-ia1';
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
    candidate: 'Kandidat-ID', client: 'Kunden-Account-ID', more: 'Weitere Angaben', present: 'Vorstellen', entries: 'Einträge', empty: 'Noch keine Vorstellungen.', idCopied: 'ID kopiert', offlineService: 'Offline: Befehlsdienst nicht verfügbar.', blocked: 'Blockiert.', idsRequired: 'Kandidat-ID und Kunden-Account-ID sind erforderlich.', offlineSend: 'Offline: Befehl konnte nicht gesendet werden.', presented: 'Vorgestellt.', submission: 'Submission', vacancyLabel: 'Vakanz', contactLabel: 'Kontakt', consent: 'Consent', feedback: 'Feedback', sent: 'Gesendet', copyId: 'ID kopieren', unknown: 'unbekannt', conflict: 'Konflikt', status: 'Status',
    kicker: 'VORSTELLUNGEN', listTitle: 'Kandidaten', allStatus: 'Alle Status', viewAll: 'Alle', viewOpen: 'Offen', viewDone: 'Erledigt', composerKicker: 'NEUE VORSTELLUNG', composerTitle: 'Kandidat vorstellen', composerHint: 'Eintrag wählen oder neue Vorstellung anlegen.', recordKicker: 'VORSTELLUNG', importDone: 'Import abgeschlossen.', exportDone: 'Export erstellt.', invalidFile: 'Datei konnte nicht gelesen werden (JSON erwartet).',
    statusSent: 'gesendet', statusHired: 'eingestellt', statusWithdrawn: 'zurückgezogen', statusRejected: 'abgelehnt',
  },
  en: {
    candidate: 'Candidate ID', client: 'Client account ID', more: 'More details', present: 'Present candidate', entries: 'records', empty: 'No submissions yet.', idCopied: 'ID copied', offlineService: 'Offline: command service unavailable.', blocked: 'Blocked.', idsRequired: 'Candidate ID and client account ID are required.', offlineSend: 'Offline: command could not be sent.', presented: 'Candidate presented.', submission: 'Submission', vacancyLabel: 'Vacancy', contactLabel: 'Contact', consent: 'Consent', feedback: 'Feedback', sent: 'Sent', copyId: 'Copy ID', unknown: 'unknown', conflict: 'Conflict', status: 'Status',
    kicker: 'SUBMISSIONS', listTitle: 'Candidates', allStatus: 'All statuses', viewAll: 'All', viewOpen: 'Open', viewDone: 'Closed', composerKicker: 'NEW SUBMISSION', composerTitle: 'Present candidate', composerHint: 'Select a record or start a new submission.', recordKicker: 'SUBMISSION', importDone: 'Import complete.', exportDone: 'Export created.', invalidFile: 'File could not be read (JSON expected).',
    statusSent: 'sent', statusHired: 'hired', statusWithdrawn: 'withdrawn', statusRejected: 'rejected',
  },
};
let text = COPY.de;
let locale = 'de';

export async function mount(ctx) {
  locale = ctx.locale === 'en' ? 'en' : 'de';
  text = COPY[locale];
  await ensureStyles();
  ctx.host.innerHTML = await loadMarkup();
  ctx.host.dataset.atsModule = MODULE_ID;
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();

  const root = ctx.host.querySelector('[data-subs-root]');
  applyStaticCopy(root);

  const listPane = root?.querySelector('[data-subs-list-pane]');
  const listEl = root?.querySelector('[data-subs-list]');
  const formEl = root?.querySelector('[data-subs-form]');
  const gateEl = root?.querySelector('[data-subs-gate]');
  const detailEl = root?.querySelector('[data-subs-detail]');
  const wbKicker = root?.querySelector('[data-subs-wb-kicker]');
  const wbTitle = root?.querySelector('[data-subs-wb-title]');
  const wbFooter = root?.querySelector('[data-subs-wb-footer]');
  const collapseBtn = root?.querySelector('[data-subs-collapse]');
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
      collapseBtn.hidden = !hasSel; // only reveal the control when there is something to reveal
      collapseBtn.setAttribute('aria-pressed', String(hasSel && state.userCollapsed));
    }
    if (wbKicker) wbKicker.textContent = hasSel ? text.recordKicker : text.composerKicker;
    if (wbTitle) wbTitle.textContent = hasSel ? (rec.candidate_id || rec.id || text.composerTitle) : text.composerTitle;
    if (detailEl) detailEl.innerHTML = showDetail ? renderDetail(rec, text) : '';
    if (wbFooter) wbFooter.textContent = hasSel ? `${text.submission}: ${rec.id || '—'}` : text.composerHint;
  }

  function fillForm(r) {
    if (!formEl) return;
    const set = (name, value) => { const el = formEl.querySelector(`[name="${name}"]`); if (el) el.value = value || ''; };
    set('candidate_id', r.candidate_id);
    set('client_account_id', r.client_account_id);
    set('vacancy_id', r.vacancy_id);
    set('client_contact_id', r.client_contact_id);
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
      } catch (e) { console.error('[submissions] load failed:', e); }
    }
    rows.sort((a, b) => Number(b?.updated_at_ms || b?.sent_at_ms || b?.created_at_ms || 0) - Number(a?.updated_at_ms || a?.sent_at_ms || a?.created_at_ms || 0));
    state.records = rows;
  }

  async function refresh() {
    await loadRecords();
    renderListRegion();
    renderWorkbench();
  }

  // ---- Selection (left list) ------------------------------------------------
  function onListClick(event) {
    const row = event.target?.closest?.('[data-subs-row]');
    if (!row || !listEl?.contains(row)) return;
    selectRecord(row.getAttribute('data-subs-row') || '');
  }
  listEl?.addEventListener('click', onListClick);

  // Re-render the list on any shell-wired grammar change (search/view/tray/band).
  function onGrammarChange(event) {
    if (!listPane || !listPane.contains(event.target)) return;
    state.grammar = event.detail || readGrammarState(listPane);
    renderListRegion();
  }
  root?.addEventListener('ctox-pane-grammar-change', onGrammarChange);

  // ---- Record detail: local, non-dispatching id copy (the module has exactly
  // one native command, ats.submission.present; there is no withdraw/feedback
  // handler, so we never fabricate a server command). ------------------------
  async function onDetailClick(event) {
    const copyBtn = event.target?.closest?.('[data-copy-id]');
    if (!copyBtn) return;
    const id = copyBtn.getAttribute('data-copy-id') || '';
    if (!id) return;
    try { await navigator.clipboard?.writeText?.(id); } catch {}
    setGate(`<strong>${esc(text.idCopied)}:</strong> <span class="ats-result-row">` + esc(id) + '</span>', 'ok');
  }
  detailEl?.addEventListener('click', onDetailClick);

  // ---- Present flow (unchanged command type + payload) ----------------------
  async function onSubmit(event) {
    event.preventDefault();
    setGate('');
    const dispatch = ctx.commandBus?.dispatch;
    if (typeof dispatch !== 'function') { setGate(text.offlineService, 'offline'); return; }
    const data = new FormData(formEl);
    const f = Object.fromEntries(data.entries());
    const candidate_id = String(f.candidate_id || '').trim();
    const client_account_id = String(f.client_account_id || '').trim();
    if (!candidate_id || !client_account_id) {
      setGate(`<strong>${esc(text.blocked)}</strong><div class="ats-result-row">${esc(text.idsRequired)}</div>`, 'block');
      return;
    }
    const payload = {
      candidate_id,
      client_account_id,
      vacancy_id: String(f.vacancy_id || '').trim() || null,
      client_contact_id: String(f.client_contact_id || '').trim() || null,
    };

    let outcome;
    try {
      outcome = await ctx.commandBus?.dispatch?.({
        module: MODULE_ID,
        command_type: PRESENT_COMMAND,
        payload,
      });
    } catch (e) {
      console.error('[submissions] present dispatch failed:', e);
      setGate(text.offlineSend, 'offline');
      return;
    }

    const result = outcome?.result ?? outcome ?? {};
    const decision = result?.gate || result?.decision || null;
    const blockers = result?.blockers || decision?.blockers || result?.errors || null;
    const blocked = result?.ok === false
      || result?.allowed === false
      || result?.status === 'blocked'
      || decision?.decision === 'block'
      || (Array.isArray(blockers) && blockers.length > 0);

    if (blocked) {
      const items = (Array.isArray(blockers) ? blockers : [blockers])
        .filter(Boolean)
        .map((b) => '<li>' + esc(blockerText(b)) + '</li>')
        .join('');
      setGate(`<strong>${esc(text.blocked)}</strong>` + (items ? '<ul class="ats-blockers">' + items + '</ul>' : ''), 'block');
      return;
    }

    const submissionId = result?.submission_id ?? result?.data?.submission_id ?? null;
    setGate(
      `<strong>${esc(text.presented)}</strong>`
      + `<div class="ats-result-row">${esc(text.submission)}: ` + esc(submissionId ?? '—') + '</div>',
      'ok',
    );
    // Return to create-mode (present always creates) while preserving the
    // success gate — do NOT route through startNew(), which clears the gate.
    state.selectedId = '';
    state.userCollapsed = false;
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
      anchor.download = `submissions-${rows.length}.json`;
      document.body.appendChild(anchor);
      anchor.click();
      anchor.remove();
      ctx.notifications?.info?.(text.exportDone);
    } catch (e) {
      console.error('[submissions] export failed:', e);
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
        console.error('[submissions] import parse failed:', e);
        setGate(text.invalidFile, 'block');
        return;
      }
      const items = Array.isArray(parsed) ? parsed : (Array.isArray(parsed?.records) ? parsed.records : [parsed]);
      let ok = 0;
      for (const item of items) {
        const record = normalizeSubmission(item, { nowMs: Date.now() });
        if (!record.candidate_id || !record.client_account_id) continue;
        try { await col.upsert(record); ok += 1; } catch (e) { console.error('[submissions] import upsert failed:', e); }
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

export function renderList(records, opts = {}, t = text) {
  if (!records || !records.length) return `<div class="ctox-empty">${esc(t.empty)}</div>`;
  const view = opts.view === 'list' ? 'list' : 'cards';
  const selectedId = opts.selectedId || '';
  return records.map((r) => (view === 'list' ? shardCompact(r, selectedId, t) : shardCard(r, selectedId, t))).join('');
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

// A shard is a pure selector: title + ONE muted meta line.
function shardCard(r, selectedId, t) {
  const key = recordKey(r);
  const status = String(r.status || 'sent');
  const outcome = r.feedback && typeof r.feedback === 'object' ? r.feedback.outcome : '';
  const meta = [t.kicker, r.client_account_id ? r.client_account_id : null, outcome ? `${t.feedback}: ${outcome}` : null]
    .filter(Boolean).map(esc).join(' · ');
  const badge = ('ctox-badge ' + statusBadgeClass(status)).trim();
  return rowShell(r, key, selectedId,
    '<div class="subs-shard">'
    + '<div class="subs-shard-title">'
    + `<span class="${badge}" data-status="${esc(status)}">${esc(statusLabel(status, t))}</span>`
    + `<strong>${esc(r.candidate_id || key || '—')}</strong>`
    + '</div>'
    + `<small class="subs-shard-meta">${meta}</small>`
    + '</div>');
}

function shardCompact(r, selectedId, t) {
  const key = recordKey(r);
  const status = String(r.status || 'sent');
  const badge = ('ctox-badge ' + statusBadgeClass(status)).trim();
  return rowShell(r, key, selectedId,
    '<div class="subs-row-compact">'
    + `<span class="subs-compact-title">${esc(r.candidate_id || key || '—')}</span>`
    + `<span class="${badge}" data-status="${esc(status)}">${esc(statusLabel(status, t))}</span>`
    + '</div>');
}

function rowShell(r, key, selectedId, inner) {
  const label = r.candidate_id || key || '—';
  const selected = key && key === selectedId;
  return '<button type="button" class="ctox-list-item subs-row' + (selected ? ' is-selected' : '') + '"'
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
    detailField(t.candidate, r.candidate_id || '—'),
    detailField(t.client, r.client_account_id || '—'),
    detailField(t.status, statusLabel(status, t)),
  ];
  if (outcome) fields.push(detailField(t.feedback, outcome));
  if (sentAt) fields.push(detailField(t.sent, sentAt));
  fields.push(detailField(t.submission, r.id || '—'));
  if (r.vacancy_id) fields.push(detailField(t.vacancyLabel, r.vacancy_id));
  if (r.client_contact_id) fields.push(detailField(t.contactLabel, r.client_contact_id));
  if (r.consent_id) fields.push(detailField(t.consent, r.consent_id));
  const copy = r.id
    ? `<button type="button" class="ctox-pane-icon" data-copy-id="${esc(r.id)}" aria-label="${esc(t.copyId)}" title="${esc(t.copyId)}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15V5a2 2 0 0 1 2-2h10"/></svg></button>`
    : '';
  return '<section class="subs-detail-card"'
    + ` data-context-record-id="${esc(r.id || '')}" data-context-record-type="submission"`
    + ` data-context-label="${esc(r.candidate_id || r.id || '—')}">`
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
  try { return new Date(n).toLocaleString(locale === 'en' ? 'en' : 'de'); } catch { return ''; }
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
