import { isMeetingState, isNoShow } from './core/scheduling.js';
import { scoreScorecard, isScorecardComplete } from './core/scorecard.js';

const MOD_BUILD = '20260721-ia1';
const MODULE_ID = 'interviews';
// Interview coordination has two record families, both plain RxDB collection
// writes (there is no native `ats.interview.*` business command — STT
// transcription is a native/skill effect that writes `transcript_id` directly).
// PRIMARY drives the left record list + workbench; scorecards are shown
// alongside the selected meeting's candidate.
const PRIMARY = 'interview_meetings';
const SCORECARDS = 'interview_scorecards';
const TITLE = 'interviews';
const OPEN_STATES = new Set(['proposed', 'confirmed', 'rescheduled']);
const COPY = {
  de: {
    candidatePlaceholder: 'Kandidat-ID', vacancyPlaceholder: 'Vakanz-ID',
    partiesLabel: 'Parteien', partiesPlaceholder: 'Komma-getrennt', startLabel: 'Start', durationPlaceholder: 'Dauer (Min)', locationLabel: 'Ort',
    modeVideo: 'Video', modeOnsite: 'Vor Ort', modePhone: 'Telefon',
    videoLinkPlaceholder: 'Video-Link', createMeeting: 'Termin anlegen', saveMeeting: 'Änderungen speichern', meetings: 'Termine',
    scorecards: 'Scorecards', entriesEmpty: 'Noch keine Einträge.', meetingMissing: 'Termin nicht gefunden.',
    meetingUpdated: 'Termin aktualisiert.', updateFailed: 'Aktualisierung fehlgeschlagen.',
    databaseOffline: 'Offline: Datenbank nicht verfügbar.', candidateRequired: 'Kandidat-ID erforderlich.',
    meetingCreated: 'Termin angelegt.', meeting: 'Termin', status: 'Status', parties: 'Parteien',
    createFailed: 'Anlegen fehlgeschlagen', withoutSchedule: 'ohne Termin', party: 'Partei',
    link: 'Link', transcript: 'Transkript', confirm: 'Bestätigen', attended: 'Stattgefunden',
    noShow: 'No-Show', cancel: 'Absagen', criteria: 'Kriterien', score: 'Score',
    interviewer: 'Interviewer', complete: 'vollständig', open: 'offen', generic: 'allgemein',
    listKicker: 'Recruiting', searchPlaceholder: 'Suchen...', newMeeting: 'Neuer Termin',
    importLabel: 'Importieren', exportLabel: 'Exportieren', viewCards: 'Shard-Ansicht', viewList: 'Listen-Ansicht',
    filterLabel: 'Filter', statusFilterLabel: 'Status filtern', resetLabel: 'Filter zurücksetzen',
    statusAll: 'Alle Status', stateProposed: 'Vorgeschlagen', stateConfirmed: 'Bestätigt',
    stateRescheduled: 'Verschoben', stateCompleted: 'Stattgefunden', stateNoShow: 'No-Show', stateCancelled: 'Abgesagt',
    viewAll: 'Alle', viewOpen: 'Offen', viewDone: 'Erledigt', collapseLabel: 'Einklappen',
    createKicker: 'Neuer Termin', importDone: 'Import abgeschlossen.', importFailed: 'Import fehlgeschlagen',
  },
  en: {
    candidatePlaceholder: 'Candidate ID', vacancyPlaceholder: 'Vacancy ID',
    partiesLabel: 'Participants', partiesPlaceholder: 'comma-separated', startLabel: 'Start', durationPlaceholder: 'Duration (min)', locationLabel: 'Location',
    modeVideo: 'Video', modeOnsite: 'On site', modePhone: 'Phone',
    videoLinkPlaceholder: 'Video link', createMeeting: 'Schedule meeting', saveMeeting: 'Save changes', meetings: 'Meetings',
    scorecards: 'Scorecards', entriesEmpty: 'No entries yet.', meetingMissing: 'Meeting not found.',
    meetingUpdated: 'Meeting updated.', updateFailed: 'Update failed.',
    databaseOffline: 'Offline: database unavailable.', candidateRequired: 'Candidate ID is required.',
    meetingCreated: 'Meeting scheduled.', meeting: 'Meeting', status: 'Status', parties: 'participants',
    createFailed: 'Could not schedule meeting', withoutSchedule: 'not scheduled', party: 'participant',
    link: 'Link', transcript: 'Transcript', confirm: 'Confirm', attended: 'Attended',
    noShow: 'No-show', cancel: 'Cancel', criteria: 'criteria', score: 'Score',
    interviewer: 'Interviewer', complete: 'complete', open: 'open', generic: 'generic',
    listKicker: 'Recruiting', searchPlaceholder: 'Search...', newMeeting: 'New meeting',
    importLabel: 'Import', exportLabel: 'Export', viewCards: 'Shard view', viewList: 'List view',
    filterLabel: 'Filter', statusFilterLabel: 'Filter status', resetLabel: 'Reset filters',
    statusAll: 'All statuses', stateProposed: 'Proposed', stateConfirmed: 'Confirmed',
    stateRescheduled: 'Rescheduled', stateCompleted: 'Attended', stateNoShow: 'No-show', stateCancelled: 'Cancelled',
    viewAll: 'All', viewOpen: 'Open', viewDone: 'Done', collapseLabel: 'Collapse',
    createKicker: 'New meeting', importDone: 'Import complete.', importFailed: 'Import failed',
  },
};

export async function mount(ctx) {
  await ensureStyles();
  ctx.host.innerHTML = await loadMarkup();
  ctx.host.dataset.atsModule = MODULE_ID;
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();
  const root = ctx.host.querySelector('[data-ats-root]');
  const locale = String(ctx.locale || document.documentElement.lang || 'de').toLowerCase().startsWith('en') ? 'en' : 'de';
  const copy = COPY[locale];
  const t = (key) => copy[key] || COPY.de[key] || key;
  root?.setAttribute('lang', locale);
  root?.querySelectorAll('[data-i18n]').forEach((node) => { node.textContent = t(node.dataset.i18n); });
  root?.querySelectorAll('[data-i18n-placeholder]').forEach((node) => { node.placeholder = t(node.dataset.i18nPlaceholder); });
  root?.querySelectorAll('[data-i18n-title]').forEach((node) => { node.title = t(node.dataset.i18nTitle); node.setAttribute('aria-label', t(node.dataset.i18nTitle)); });

  const leftPane = root?.querySelector('.ats-list-pane');
  const listEl = root?.querySelector('[data-ats-list]');
  const formEl = root?.querySelector('[data-ats-form]');
  const gateEl = root?.querySelector('[data-ats-gate]');
  const titleEl = root?.querySelector('[data-ats-title]');
  const toggleWbEl = root?.querySelector('[data-toggle-workbench]');
  const importInput = root?.querySelector('[data-ats-import-input]');
  if (titleEl) titleEl.textContent = ctx.manifest?.title || ctx.module?.title || TITLE;

  // Workbench selection state — auto-reveal model (outbound idiom).
  const ui = { selectedId: null, creating: false, collapsed: false };
  // Latest loaded record snapshots (kept for grammar-only re-renders).
  let meetings = [];
  let scorecards = [];

  const collection = (name) => { try { return ctx.db?.collection?.(name) || null; } catch { return null; } };

  // Gate results render in the kit callout; kinds map onto its state variants.
  const GATE_KINDS = { ok: 'is-success', block: 'is-danger', offline: 'is-warning' };
  function setGate(html, kind) {
    if (!gateEl) return;
    gateEl.className = 'ctox-callout' + (GATE_KINDS[kind] ? ' ' + GATE_KINDS[kind] : '');
    gateEl.innerHTML = html || '';
    gateEl.hidden = !html;
  }

  async function loadRows(name) {
    const col = collection(name);
    if (!col?.find) return [];
    try {
      const docs = await col.find({ selector: {} }).exec();
      return docs.map((d) => (typeof d.toJSON === 'function' ? d.toJSON() : d)).filter((r) => !r._deleted);
    } catch (e) { console.error('[interviews] load failed:', name, e); return []; }
  }

  // Counts/footer go through the shell-wired grammar handle when present, with a
  // direct-DOM fallback for the first render before the shell wires the pane.
  function writeCounts(counts) {
    const grammar = leftPane?.__ctoxPaneGrammar;
    if (grammar?.setCounts) { grammar.setCounts(counts); return; }
    for (const [key, value] of Object.entries(counts || {})) {
      const node = leftPane?.querySelector('[data-pg-count="' + key + '"]');
      if (node) node.textContent = ' (' + value + ')';
    }
  }
  function writeFooter(text) {
    const grammar = leftPane?.__ctoxPaneGrammar;
    if (grammar?.setFooter) { grammar.setFooter(text); return; }
    const node = leftPane?.querySelector('[data-pg-footer]');
    if (node) node.textContent = text || '';
  }

  function renderList() {
    const grammar = readGrammar(leftPane);
    const now = Date.now();
    const { visible, counts } = partitionMeetings(meetings, {
      search: grammar.search, status: grammar.status, band: grammar.band, nowMs: now,
    });
    writeCounts(counts);
    if (listEl) {
      listEl.classList.toggle('is-compact', grammar.view === 'list');
      listEl.innerHTML = visible.length
        ? visible.map((r) => meetingShard(r, { t, locale, nowMs: now, selectedId: ui.selectedId })).join('')
        : '<div class="ctox-empty">' + esc(t('entriesEmpty')) + '</div>';
    }
    writeFooter(
      visible.length + ' ' + t('meetings') + ' · ' + bandLabel(grammar.band, t)
      + (grammar.status !== 'all' ? ' · ' + stateLabel(grammar.status, t) : ''),
    );
  }

  function fillForm(r) {
    if (!formEl) return;
    const set = (name, value) => { const el = formEl.elements.namedItem(name); if (el) el.value = value == null ? '' : value; };
    set('candidate_id', r.candidate_id || '');
    set('vacancy_id', r.vacancy_id || '');
    set('parties', Array.isArray(r.parties) ? r.parties.map((p) => (p && p.name != null ? p.name : p)).filter(Boolean).join(', ') : '');
    set('start', r.start ? toLocalInput(r.start) : '');
    const durationMin = r.start != null && r.end != null && Number.isFinite(Number(r.end)) && Number.isFinite(Number(r.start))
      ? Math.round((Number(r.end) - Number(r.start)) / 60000) : '';
    set('duration_min', durationMin === '' ? '' : String(durationMin));
    set('location_mode', r.location_mode || 'video');
    set('video_link', r.video_link || '');
  }

  function readForm() {
    const data = new FormData(formEl);
    const f = Object.fromEntries(data.entries());
    const candidate_id = String(f.candidate_id || '').trim();
    const startMs = f.start ? Date.parse(f.start) : NaN;
    const durationMin = f.duration_min === '' || f.duration_min == null ? null : Number(f.duration_min);
    const start = Number.isFinite(startMs) ? startMs : null;
    const end = start != null && Number.isFinite(durationMin) ? start + durationMin * 60_000 : null;
    const parties = String(f.parties || '')
      .split(',').map((s) => s.trim()).filter(Boolean).map((name) => ({ name }));
    return {
      candidate_id,
      vacancy_id: String(f.vacancy_id || '').trim() || null,
      parties, start, end,
      location_mode: String(f.location_mode || 'video').trim() || null,
      video_link: String(f.video_link || '').trim() || null,
    };
  }

  function meetingActionButtons(rec, state) {
    const acts = [];
    if (state === 'proposed' || state === 'rescheduled') acts.push(actionBtn(rec.id, 'confirmed', t('confirm')));
    if (state !== 'completed' && state !== 'cancelled') {
      acts.push(actionBtn(rec.id, 'completed', t('attended')));
      acts.push(actionBtn(rec.id, 'no_show', t('noShow')));
    }
    if (state !== 'cancelled' && state !== 'completed') acts.push(actionBtn(rec.id, 'cancelled', t('cancel')));
    return acts.join('');
  }

  function renderWorkbench() {
    const wbTitle = root?.querySelector('[data-ats-wb-title]');
    const wbKicker = root?.querySelector('[data-ats-wb-kicker]');
    const submitEl = root?.querySelector('[data-ats-submit]');
    const actionsEl = root?.querySelector('[data-ats-meeting-actions]');
    const scoreEl = root?.querySelector('[data-ats-scorecards]');
    if (ui.creating) {
      if (wbTitle) wbTitle.textContent = t('newMeeting');
      if (wbKicker) wbKicker.textContent = t('createKicker');
      if (submitEl) submitEl.textContent = t('createMeeting');
      if (actionsEl) { actionsEl.hidden = true; actionsEl.innerHTML = ''; }
      if (scoreEl) { scoreEl.hidden = true; scoreEl.innerHTML = ''; }
      return;
    }
    const rec = ui.selectedId ? meetings.find((m) => m.id === ui.selectedId) : null;
    if (!rec) {
      // Selection went stale (record deleted elsewhere) → drop it and collapse.
      ui.selectedId = null;
      if (actionsEl) { actionsEl.hidden = true; actionsEl.innerHTML = ''; }
      if (scoreEl) { scoreEl.hidden = true; scoreEl.innerHTML = ''; }
      applyReveal();
      return;
    }
    const state = effectiveState(rec, Date.now());
    if (wbTitle) wbTitle.textContent = rec.candidate_id || rec.id || t('meeting');
    if (wbKicker) wbKicker.textContent = stateLabel(state, t);
    if (submitEl) submitEl.textContent = t('saveMeeting');
    fillForm(rec);
    if (actionsEl) {
      const markup = meetingActionButtons(rec, state);
      actionsEl.innerHTML = markup;
      actionsEl.hidden = !markup;
    }
    if (scoreEl) {
      const related = scorecards.filter((s) => s.candidate_id && rec.candidate_id && s.candidate_id === rec.candidate_id);
      if (related.length) {
        scoreEl.innerHTML = '<div class="ats-scorecards-head">' + esc(t('scorecards')) + '</div>'
          + related.map((s) => scorecardRow(s, t)).join('');
        scoreEl.hidden = false;
      } else { scoreEl.innerHTML = ''; scoreEl.hidden = true; }
    }
  }

  function applyReveal() {
    const revealed = isWorkbenchRevealed(ui);
    root?.classList.toggle('is-workbench-hidden', !revealed);
    toggleWbEl?.setAttribute('aria-pressed', ui.collapsed ? 'true' : 'false');
  }

  function selectRecord(id) {
    if (!id) return;
    ui.selectedId = id; ui.creating = false; ui.collapsed = false;
    setGate('');
    renderWorkbench();
    // Selection is an in-place class flip — a list rebuild resets the
    // operator's scroll (design-guide: re-renders never move the operator).
    applyListSelection();
    applyReveal();
  }

  function applyListSelection() {
    listEl?.querySelectorAll('[data-ats-select]').forEach((row) => {
      const on = (row.getAttribute('data-ats-select') || '') === String(ui.selectedId || '');
      row.classList.toggle('is-selected', on);
      row.setAttribute('aria-selected', String(on));
    });
  }

  function startCreate() {
    ui.selectedId = null; ui.creating = true; ui.collapsed = false;
    setGate('');
    try { formEl?.reset(); } catch {}
    renderWorkbench();
    renderList();
    applyReveal();
    formEl?.querySelector('input')?.focus?.();
  }

  function toggleWorkbench() {
    ui.collapsed = !ui.collapsed;
    applyReveal();
  }

  async function render() {
    const [m, s] = await Promise.all([loadRows(PRIMARY), loadRows(SCORECARDS)]);
    meetings = m; scorecards = s;
    renderList();
    renderWorkbench();
  }

  // Meeting state transitions are plain RxDB writes — the scheduling engine owns
  // the valid MEETING_STATES, not a native command.
  async function transitionMeeting(meetingId, state, patch) {
    const col = collection(PRIMARY);
    if (!col?.find || !isMeetingState(state)) return;
    setGate('');
    try {
      const doc = await col.findOne(meetingId).exec();
      if (!doc) { setGate(t('meetingMissing'), 'block'); return; }
      const now = Date.now();
      await doc.patch(Object.assign({ state, updated_at_ms: now }, patch || {}));
      setGate('<strong>' + esc(t('meetingUpdated')) + '</strong><div class="ats-result-row">' + esc(meetingId) + ' → ' + esc(state) + '</div>', 'ok');
      await render();
    } catch (e) {
      console.error('[interviews] transition failed:', e);
      setGate(t('updateFailed'), 'offline');
    }
  }

  async function onSubmit(event) {
    event.preventDefault();
    setGate('');
    const col = collection(PRIMARY);
    if (!col?.insert) { setGate(t('databaseOffline'), 'offline'); return; }
    const fields = readForm();
    if (!fields.candidate_id) { setGate(t('candidateRequired'), 'block'); return; }
    const now = Date.now();
    try {
      if (ui.selectedId) {
        // Edit: patch the selected record's editable fields. state/attended/
        // created_at stay owned by the transition flow — payload shape unchanged.
        const doc = await col.findOne(ui.selectedId).exec();
        if (!doc) { setGate(t('meetingMissing'), 'block'); return; }
        await doc.patch(Object.assign({}, fields, { updated_at_ms: now }));
        setGate('<strong>' + esc(t('meetingUpdated')) + '</strong><div class="ats-result-row">' + esc(ui.selectedId) + '</div>', 'ok');
        await render();
      } else {
        // Create: the existing insert flow, unchanged.
        const record = Object.assign({
          id: 'imeet_' + now + '_' + Math.round(now % 1e6),
        }, fields, {
          state: 'proposed', attended: false,
          created_at_ms: now, updated_at_ms: now, _deleted: false,
        });
        await col.insert(record);
        setGate(
          '<strong>' + esc(t('meetingCreated')) + '</strong>'
          + '<div class="ats-result-row">' + esc(t('meeting')) + ': ' + esc(record.id) + '</div>'
          + '<div class="ats-result-row">' + esc(t('status')) + ': ' + esc(record.state) + (fields.parties.length ? ' · ' + fields.parties.length + ' ' + esc(t('parties')) : '') + '</div>',
          'ok',
        );
        // Bind the freshly created record in the workbench.
        ui.selectedId = record.id; ui.creating = false; ui.collapsed = false;
        await render();
        applyReveal();
      }
    } catch (e) {
      console.error('[interviews] submit failed:', e);
      setGate(t('createFailed') + ': ' + esc(e?.message || e), 'block');
    }
  }

  function openImport() { importInput?.click?.(); }

  async function onImportChange(event) {
    const file = event.target?.files?.[0];
    if (importInput) importInput.value = '';
    if (!file) return;
    const col = collection(PRIMARY);
    if (!col?.upsert) { setGate(t('databaseOffline'), 'offline'); return; }
    setGate('');
    try {
      const parsed = JSON.parse(await file.text());
      const list = Array.isArray(parsed) ? parsed
        : Array.isArray(parsed?.records) ? parsed.records
          : Array.isArray(parsed?.interview_meetings) ? parsed.interview_meetings : [];
      let ok = 0;
      for (let i = 0; i < list.length; i += 1) {
        const rec = normalizeImported(list[i], i);
        if (!rec) continue;
        await col.upsert(rec);
        ok += 1;
      }
      setGate('<strong>' + esc(t('importDone')) + '</strong><div class="ats-result-row">' + ok + ' / ' + list.length + '</div>', ok ? 'ok' : 'offline');
      await render();
    } catch (e) {
      console.error('[interviews] import failed:', e);
      setGate(t('importFailed') + ': ' + esc(e?.message || e), 'block');
    }
  }

  function exportVisible() {
    const grammar = readGrammar(leftPane);
    const { visible } = partitionMeetings(meetings, {
      search: grammar.search, status: grammar.status, band: grammar.band, nowMs: Date.now(),
    });
    const payload = { schema: PRIMARY, exported_at_ms: Date.now(), records: visible };
    downloadTextFile('interview-meetings.json', JSON.stringify(payload, null, 2), 'application/json');
  }

  async function onRootClick(event) {
    const actionEl = event.target?.closest?.('[data-action]');
    if (actionEl) {
      const action = actionEl.getAttribute('data-action');
      if (action === 'new') return startCreate();
      if (action === 'import') return openImport();
      if (action === 'export') return exportVisible();
    }
    if (event.target?.closest?.('[data-toggle-workbench]')) return toggleWorkbench();
    const shard = event.target?.closest?.('[data-ats-select]');
    if (shard) return selectRecord(shard.getAttribute('data-ats-select'));
    const mAction = event.target?.closest?.('[data-meeting-action]');
    if (mAction) {
      const id = mAction.getAttribute('data-meeting-id');
      const act = mAction.getAttribute('data-meeting-action');
      if (act === 'confirmed') return transitionMeeting(id, 'confirmed');
      if (act === 'completed') return transitionMeeting(id, 'completed', { attended: true });
      if (act === 'no_show') return transitionMeeting(id, 'no_show', { attended: false });
      if (act === 'cancelled') return transitionMeeting(id, 'cancelled');
    }
    return undefined;
  }

  function onGrammarChange() { renderList(); }

  root?.addEventListener('click', onRootClick);
  root?.addEventListener('ctox-pane-grammar-change', onGrammarChange);
  formEl?.addEventListener('submit', onSubmit);
  importInput?.addEventListener('change', onImportChange);

  const subs = [];
  for (const name of [PRIMARY, SCORECARDS]) {
    const col = collection(name);
    if (col?.find) { try { const s = col.find({ selector: {} }).$?.subscribe?.(() => { render().catch(() => {}); }); if (s) subs.push(s); } catch {} }
  }
  await render();

  return () => {
    for (const s of subs) { try { s?.unsubscribe?.(); } catch {} }
    root?.removeEventListener('click', onRootClick);
    root?.removeEventListener('ctox-pane-grammar-change', onGrammarChange);
    formEl?.removeEventListener('submit', onSubmit);
    importInput?.removeEventListener('change', onImportChange);
    ctx.host.replaceChildren();
    delete ctx.host.dataset.atsModule;
  };
}

// --- Pure, DOM-free helpers (exported for tests) -----------------------------

// Effective meeting state: a past meeting without attendance reads as no_show.
export function effectiveState(r, nowMs) {
  let state = String(r?.state || 'proposed');
  if (isNoShow(r, nowMs) && state !== 'no_show' && state !== 'cancelled') state = 'no_show';
  return state;
}

// Two real bands derived from the meeting state field: open vs done.
export function meetingBand(state) {
  return OPEN_STATES.has(String(state)) ? 'open' : 'done';
}

// Filter meetings by search/status and split into band counts; the visible set
// is further narrowed to the active band. Zeros are always included.
export function partitionMeetings(rows, { search = '', status = 'all', band = 'all', nowMs = 0 } = {}) {
  const list = Array.isArray(rows) ? rows : [];
  const q = String(search || '').trim().toLowerCase();
  const matchesSearch = (r) => !q || [r.id, r.candidate_id, r.vacancy_id, r.location_mode]
    .some((v) => String(v == null ? '' : v).toLowerCase().includes(q));
  const scoped = list.filter((r) => {
    if (!matchesSearch(r)) return false;
    if (status !== 'all' && effectiveState(r, nowMs) !== status) return false;
    return true;
  });
  const counts = { all: scoped.length, open: 0, done: 0 };
  for (const r of scoped) counts[meetingBand(effectiveState(r, nowMs))] += 1;
  const visible = band && band !== 'all'
    ? scoped.filter((r) => meetingBand(effectiveState(r, nowMs)) === band)
    : scoped;
  return { visible, counts };
}

// A shard is a pure selector: title (candidate → vacancy) + ONE muted meta line.
export function meetingShard(r, { t = (k) => k, locale = 'de', nowMs = 0, selectedId = null } = {}) {
  const state = effectiveState(r, nowMs);
  const badge = isMeetingState(state) ? state : 'proposed';
  const cand = esc(r.candidate_id || '—');
  const vac = r.vacancy_id ? ' → ' + esc(r.vacancy_id) : '';
  const partyCount = Array.isArray(r.parties) ? r.parties.length : 0;
  const when = r.start ? fmtTime(r.start, locale) : t('withoutSchedule');
  const meta = [
    esc(t('status')) + ': ' + esc(stateLabel(state, t)),
    esc(when),
    esc(r.location_mode || 'video'),
    partyCount + ' ' + esc(partyCount === 1 ? t('party') : t('parties')),
  ].join(' · ');
  const selected = selectedId && r.id === selectedId ? ' is-selected' : '';
  return ''
    + '<button type="button" class="ats-shard' + selected + '" data-ats-select="' + esc(r.id || '') + '"'
    + ' data-context-record-id="' + esc(r.id || '') + '" data-context-record-type="interview_meeting" data-context-label="' + cand + '">'
    + '<span class="ats-shard-main">' + badgeSpan(badge, stateLabel(state, t)) + ' ' + cand + vac + '</span>'
    + '<span class="ats-shard-meta">' + meta + '</span>'
    + '</button>';
}

// Auto-reveal: workbench is visible when a record is selected or being created,
// unless the user has collapsed it (outbound idiom).
export function isWorkbenchRevealed({ selectedId = null, creating = false, collapsed = false } = {}) {
  return Boolean((selectedId || creating) && !collapsed);
}

function stateLabel(state, t) {
  const map = {
    proposed: 'stateProposed', confirmed: 'stateConfirmed', rescheduled: 'stateRescheduled',
    completed: 'stateCompleted', no_show: 'stateNoShow', cancelled: 'stateCancelled',
  };
  return map[state] ? t(map[state]) : String(state);
}

function bandLabel(band, t) {
  if (band === 'open') return t('viewOpen');
  if (band === 'done') return t('viewDone');
  return t('viewAll');
}

function readGrammar(pane) {
  return {
    search: (pane?.querySelector('[data-pg-search]')?.value || '').trim().toLowerCase(),
    view: pane?.querySelector('[data-pg-view][aria-pressed="true"]')?.getAttribute('data-pg-view') || 'cards',
    band: pane?.querySelector('[data-pg-band][aria-selected="true"]')?.getAttribute('data-pg-band') || 'all',
    status: pane?.querySelector('[data-pg-filter][data-pg-name="status"]')?.value || 'all',
  };
}

function normalizeImported(raw, index) {
  if (!raw || typeof raw !== 'object') return null;
  const candidate_id = String(raw.candidate_id || '').trim();
  if (!candidate_id) return null;
  const now = Date.now();
  const parties = Array.isArray(raw.parties)
    ? raw.parties.map((p) => (typeof p === 'string' ? { name: p } : (p && typeof p === 'object' ? p : null))).filter(Boolean)
    : [];
  const record = {
    id: String(raw.id || '').trim() || ('imeet_' + now + '_' + index),
    candidate_id,
    vacancy_id: raw.vacancy_id != null && raw.vacancy_id !== '' ? String(raw.vacancy_id) : null,
    parties,
    start: Number.isFinite(Number(raw.start)) ? Number(raw.start) : null,
    end: Number.isFinite(Number(raw.end)) ? Number(raw.end) : null,
    location_mode: raw.location_mode ? String(raw.location_mode) : 'video',
    video_link: raw.video_link ? String(raw.video_link) : null,
    state: isMeetingState(raw.state) ? String(raw.state) : 'proposed',
    attended: Boolean(raw.attended),
    created_at_ms: Number.isFinite(Number(raw.created_at_ms)) ? Number(raw.created_at_ms) : now,
    updated_at_ms: now,
    _deleted: false,
  };
  if (raw.transcript_id) record.transcript_id = String(raw.transcript_id);
  return record;
}

function downloadTextFile(filename, content, mimeType) {
  const blob = new Blob([content], { type: mimeType || 'text/plain;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = filename;
  document.body.append(link);
  link.click();
  link.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}

// Maps a meeting/scorecard state onto the kit badge states.
function statusBadgeClass(state) {
  if (state === 'completed') return 'is-success';
  if (state === 'no_show') return 'is-danger';
  if (state === 'proposed' || state === 'rescheduled') return 'is-warning';
  if (state === 'confirmed') return 'is-info';
  return '';
}

function badgeSpan(state, label) {
  const cls = ('ctox-badge ' + statusBadgeClass(state)).trim();
  return '<span class="' + cls + '" data-status="' + esc(state) + '">' + esc(label == null ? state : label) + '</span>';
}

function scorecardRow(r, t) {
  // Recompute the weighted score from the engine so the row reflects the real
  // criteria/ratings, not just a stored `overall`.
  let computed = null;
  try { computed = scoreScorecard(r, r.ratings); } catch { computed = null; }
  const overall = computed ? computed.overall : (Number.isFinite(Number(r.overall)) ? Number(r.overall) : 0);
  const complete = computed ? computed.complete : isScorecardComplete(r, r.ratings);
  const badge = complete ? 'completed' : 'proposed';
  const cand = esc(r.candidate_id || '—');
  const vac = r.vacancy_id ? ' → ' + esc(r.vacancy_id) : '';
  const critCount = Array.isArray(r.criteria) ? r.criteria.length : 0;
  const metaBits = [
    esc(r.id || ''),
    esc(r.role_template || t('generic')),
    critCount + ' ' + esc(t('criteria')),
    esc(t('score')) + ' ' + overall + '/100',
  ];
  if (r.interviewer) metaBits.push(esc(t('interviewer')) + ': ' + esc(r.interviewer));
  return ''
    + '<div class="ats-scorecard-row" data-context-record-id="' + esc(r.id || '') + '" data-context-record-type="interview_scorecard" data-context-label="' + cand + '">'
    + '<div class="ats-scorecard-main">' + badgeSpan(badge, complete ? t('complete') : t('open')) + ' ' + cand + vac
    + '<div class="ats-scorecard-meta">' + metaBits.join(' · ') + '</div></div>'
    + '<div class="ats-score">' + overall + '</div>'
    + '</div>';
}

function actionBtn(meetingId, action, label) {
  return '<button type="button" class="ctox-button" data-meeting-action="' + esc(action) + '" data-meeting-id="' + esc(meetingId || '') + '">' + esc(label) + '</button>';
}

function toLocalInput(ms) {
  const n = Number(ms);
  if (!Number.isFinite(n)) return '';
  const d = new Date(n);
  const pad = (x) => String(x).padStart(2, '0');
  return d.getFullYear() + '-' + pad(d.getMonth() + 1) + '-' + pad(d.getDate()) + 'T' + pad(d.getHours()) + ':' + pad(d.getMinutes());
}

function fmtTime(ms, locale) {
  const n = Number(ms);
  if (!Number.isFinite(n)) return 'ohne Termin';
  try { return new Date(n).toLocaleString(locale === 'en' ? 'en-US' : 'de-DE', { dateStyle: 'short', timeStyle: 'short' }); }
  catch { return new Date(n).toISOString(); }
}

async function ensureStyles() {
  const href = new URL('./index.css', import.meta.url).pathname + '?v=' + MOD_BUILD;
  if (document.querySelector('link[href="' + href + '"]')) return;
  const link = document.createElement('link'); link.rel = 'stylesheet'; link.href = href; document.head.append(link);
}
async function loadMarkup() {
  const html = await fetch(new URL('./index.html?v=' + MOD_BUILD, import.meta.url)).then((r) => r.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((n) => n.remove());
  return doc.body.innerHTML;
}
function esc(v) { return String(v == null ? '' : v).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;'); }
