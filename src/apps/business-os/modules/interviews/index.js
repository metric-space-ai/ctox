import { isMeetingState, isNoShow } from './core/scheduling.js';
import { scoreScorecard, isScorecardComplete } from './core/scorecard.js';

const MODULE_ID = 'interviews';
// Interview coordination has two record families, both plain RxDB collection
// writes (there is no native `ats.interview.*` business command — STT
// transcription is a native/skill effect that writes `transcript_id` directly).
// PRIMARY drives the left record list + workbench; scorecards are shown
// alongside the selected meeting's candidate.
const PRIMARY = 'interview_meetings';
const SCORECARDS = 'interview_scorecards';
const OPEN_STATES = new Set(['proposed', 'confirmed', 'rescheduled']);
const COPY = {
  de: {
    candidatePlaceholder: 'Kandidat-ID', vacancyPlaceholder: 'Vakanz-ID',
    partiesLabel: 'Parteien', partiesPlaceholder: 'Komma-getrennt', startLabel: 'Start', durationPlaceholder: 'Dauer (Min)', locationLabel: 'Ort',
    modeVideo: 'Video', modeOnsite: 'Vor Ort', modePhone: 'Telefon',
    videoLinkPlaceholder: 'Video-Link', createMeeting: 'Termin anlegen', saveMeeting: 'Änderungen speichern', meetings: 'Termine',
    title: 'Interviews', scorecards: 'Scorecards', meetingMissing: 'Termin nicht gefunden.',
    updateFailed: 'Aktualisierung fehlgeschlagen.', candidateRequired: 'Kandidat-ID erforderlich.',
    meeting: 'Termin', status: 'Status', parties: 'Parteien',
    createFailed: 'Anlegen fehlgeschlagen', party: 'Partei',
    link: 'Link', transcript: 'Transkript', confirm: 'Bestätigen', attended: 'Stattgefunden',
    noShow: 'Nicht erschienen', cancel: 'Absagen', criteria: 'Kriterien', score: 'Score',
    interviewer: 'Interviewer', complete: 'vollständig', open: 'offen', generic: 'allgemein',
    listKicker: 'Recruiting', searchPlaceholder: 'Suchen...', newMeeting: 'Neuer Termin',
    importLabel: 'Importieren', exportLabel: 'Exportieren',
    viewAsList: 'Als Liste anzeigen', viewAsCards: 'Als Karten anzeigen', minutesShort: 'Min',
    filterLabel: 'Filter', statusFilterLabel: 'Status filtern', resetLabel: 'Filter zurücksetzen',
    statusAll: 'Alle Status', stateProposed: 'Vorgeschlagen', stateConfirmed: 'Bestätigt',
    stateRescheduled: 'Verschoben', stateCompleted: 'Stattgefunden', stateNoShow: 'Nicht erschienen', stateCancelled: 'Abgesagt',
    viewAll: 'Alle', viewOpen: 'Offen', viewDone: 'Erledigt', collapseLabel: 'Einklappen',
    createKicker: 'Neuer Termin', importFailed: 'Import fehlgeschlagen',
  },
  en: {
    candidatePlaceholder: 'Candidate ID', vacancyPlaceholder: 'Vacancy ID',
    partiesLabel: 'Participants', partiesPlaceholder: 'comma-separated', startLabel: 'Start', durationPlaceholder: 'Duration (min)', locationLabel: 'Location',
    modeVideo: 'Video', modeOnsite: 'On site', modePhone: 'Phone',
    videoLinkPlaceholder: 'Video link', createMeeting: 'Schedule meeting', saveMeeting: 'Save changes', meetings: 'Meetings',
    title: 'Interviews', scorecards: 'Scorecards', meetingMissing: 'Meeting not found.',
    updateFailed: 'Update failed.', candidateRequired: 'Candidate ID is required.',
    meeting: 'Meeting', status: 'Status', parties: 'participants',
    createFailed: 'Could not schedule meeting', party: 'participant',
    link: 'Link', transcript: 'Transcript', confirm: 'Confirm', attended: 'Attended',
    noShow: 'No-show', cancel: 'Cancel', criteria: 'criteria', score: 'Score',
    interviewer: 'Interviewer', complete: 'complete', open: 'open', generic: 'generic',
    listKicker: 'Recruiting', searchPlaceholder: 'Search...', newMeeting: 'New meeting',
    importLabel: 'Import', exportLabel: 'Export',
    viewAsList: 'Show as list', viewAsCards: 'Show as cards', minutesShort: 'min',
    filterLabel: 'Filter', statusFilterLabel: 'Filter status', resetLabel: 'Reset filters',
    statusAll: 'All statuses', stateProposed: 'Proposed', stateConfirmed: 'Confirmed',
    stateRescheduled: 'Rescheduled', stateCompleted: 'Attended', stateNoShow: 'No-show', stateCancelled: 'Cancelled',
    viewAll: 'All', viewOpen: 'Open', viewDone: 'Done', collapseLabel: 'Collapse',
    createKicker: 'New meeting', importFailed: 'Import failed',
  },
};

// App-API 4.0 (Shell v4): mount(root, api, {restored}) -> {unmount, snapshot}.
const SNAPSHOT_VERSION = 1;
const FORM_FIELDS = ['candidate_id', 'vacancy_id', 'parties', 'start', 'duration_min', 'location_mode', 'video_link'];

export async function mount(host, api, { restored } = {}) {
  if (!host.dataset.app) host.dataset.app = api.app?.id || MODULE_ID;
  // CSS laedt die Shell aus dem Manifest; index.html liefert der Service Worker versioniert (§16.15).
  host.innerHTML = await loadMarkup();
  const disposers = [];
  const root = host.querySelector('[data-ats-root]');
  let locale = langFrom(api.shell.locale.get());
  let copy = copyFor(api, locale);
  const t = (key) => copy[key] || COPY.de[key] || key;
  applyCopy();

  const leftPane = root?.querySelector('.ats-list-pane');
  const listEl = root?.querySelector('[data-ats-list]');
  const formEl = root?.querySelector('[data-ats-form]');
  const toggleWbEl = root?.querySelector('[data-toggle-workbench]');
  const viewToggleEl = root?.querySelector('[data-ats-view-toggle]');
  const importInput = root?.querySelector('[data-ats-import-input]');
  const restore = isPlainObject(restored) && restored.v === SNAPSHOT_VERSION ? restored : null;

  // Workbench selection state — auto-reveal model (outbound idiom).
  const ui = { selectedId: null, creating: false, collapsed: false };
  if (restore) {
    if (typeof restore.selectedId === 'string') ui.selectedId = restore.selectedId;
    ui.creating = restore.creating === true;
    ui.collapsed = restore.collapsed === true;
    restoreGrammar(restore);
  }
  let pendingDraft = restore && isPlainObject(restore.form) ? restore.form : null;
  let pendingScrollTop = restore && Number(restore.listScrollTop) > 0 ? Number(restore.listScrollTop) : 0;
  // Latest live record snapshots (kept for grammar-only re-renders).
  let meetings = [];
  let scorecards = [];
  let meetingsSeen = false;
  let filledSig = null;

  // Die Spalten-Grammatik verdrahtet die App selbst (keine DOM-Heuristik der Shell).
  const { autoWirePaneGrammar } = await import('ctox:shared/pane-grammar.js');
  autoWirePaneGrammar(root);

  // Zustand nur ueber die Zustandszone der Shell, stabile Codes je Fall.
  const report = (level, code, text, ref) => api.status.report(ref ? { level, code, text, ref } : { level, code, text });
  const clear = (code) => api.status.clear(code);
  function followWrite(handle, code, prefix) {
    handle.local.then(() => clear(code), (e) => report('error', code, prefix + ': ' + (e?.code || 'local') + ' ' + (e?.message || e)));
  }

  function applyCopy() {
    root?.setAttribute('lang', locale);
    root?.querySelectorAll('[data-i18n]').forEach((node) => { node.textContent = t(node.dataset.i18n); });
    root?.querySelectorAll('[data-i18n-placeholder]').forEach((node) => { node.placeholder = t(node.dataset.i18nPlaceholder); });
    root?.querySelectorAll('[data-i18n-title]').forEach((node) => { node.title = t(node.dataset.i18nTitle); node.setAttribute('aria-label', t(node.dataset.i18nTitle)); });
    const title = root?.querySelector('[data-ats-title]');
    if (title) title.textContent = t('title');
    api.shell.setTitle(t('title'));
  }

  function restoreGrammar(snap) {
    const pane = root?.querySelector('.ats-list-pane');
    const search = pane?.querySelector('[data-pg-search]');
    if (search && typeof snap.search === 'string') search.value = snap.search;
    const toggle = pane?.querySelector('[data-ats-view-toggle]');
    if (toggle && (snap.view === 'list' || snap.view === 'cards')) {
      toggle.setAttribute('data-ats-view-toggle', snap.view);
      const label = t(snap.view === 'list' ? 'viewAsCards' : 'viewAsList');
      toggle.title = label;
      toggle.setAttribute('aria-label', label);
    }
    const tabs = [...(pane?.querySelectorAll('[data-pg-band]') || [])];
    if (typeof snap.band === 'string' && tabs.some((tab) => tab.dataset.pgBand === snap.band)) {
      tabs.forEach((tab) => {
        const on = tab.dataset.pgBand === snap.band;
        tab.setAttribute('aria-selected', String(on));
        tab.classList.toggle('is-active', on);
      });
    }
    const statusSel = pane?.querySelector('[data-pg-filter][data-pg-name="status"]');
    if (statusSel && typeof snap.status === 'string' && [...statusSel.options].some((o) => o.value === snap.status)) statusSel.value = snap.status;
    const tray = pane?.querySelector('[data-pg-tray]');
    if (tray && snap.trayOpen === true) { tray.hidden = false; pane.querySelector('[data-pg-tray-toggle]')?.setAttribute('aria-expanded', 'true'); }
  }

  function formValues() {
    const out = {};
    for (const name of FORM_FIELDS) out[name] = formEl?.elements?.namedItem(name)?.value ?? '';
    return out;
  }
  function applyDraft() {
    if (!pendingDraft || !formEl) return;
    if (!ui.creating && !(ui.selectedId && meetings.some((m) => m.id === ui.selectedId))) return;
    for (const name of FORM_FIELDS) {
      const el = formEl.elements.namedItem(name);
      if (el && typeof pendingDraft[name] === 'string') el.value = pendingDraft[name];
    }
    pendingDraft = null;
  }

  // Counts/footer go through the pane-grammar handle (wired above by the app itself).
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
      // The well carries the view; the two row shapes are rendered, not just
      // re-padded (cards = title + badge + two meta lines, list = one dense line).
      listEl.dataset.atsView = grammar.view;
      listEl.innerHTML = meetingListBody(visible, { t, locale, nowMs: now, selectedId: ui.selectedId, view: grammar.view });
    }
    writeFooter(
      visible.length + ' ' + t('meetings') + ' · ' + bandLabel(grammar.band, t)
      + (grammar.status !== 'all' ? ' · ' + stateLabel(grammar.status, t) : ''),
    );
    const scroller = listEl?.closest('.ctox-pane-body') || listEl;
    if (pendingScrollTop && scroller && meetings.length) { scroller.scrollTop = pendingScrollTop; pendingScrollTop = 0; }
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
      applyDraft();
      applyReveal();
      return;
    }
    // api.get sieht eigene Writes synchron (frisch angelegter Termin vor der naechsten Emission).
    const rec = ui.selectedId ? (meetings.find((m) => m.id === ui.selectedId) || api.get(PRIMARY, ui.selectedId).get()) : null;
    if (!rec) {
      // Selection went stale (record deleted elsewhere) → drop it and collapse.
      // Vor der ersten Emission der Live-Abfrage ist nichts „stale“ (Remount mit restored).
      if (meetingsSeen) ui.selectedId = null;
      if (actionsEl) { actionsEl.hidden = true; actionsEl.innerHTML = ''; }
      if (scoreEl) { scoreEl.hidden = true; scoreEl.innerHTML = ''; }
      applyReveal();
      return;
    }
    const state = effectiveState(rec, Date.now());
    if (wbTitle) wbTitle.textContent = refName(rec, 'candidate');
    if (wbKicker) wbKicker.textContent = stateLabel(state, t);
    if (submitEl) submitEl.textContent = t('saveMeeting');
    // Formular nur neu fuellen, wenn sich Auswahl oder Datensatz geaendert haben: ein Live-Update
    // einer anderen Collection (Scorecards) darf die laufende Eingabe nicht ueberschreiben.
    const sig = JSON.stringify(rec);
    if (sig !== filledSig) { fillForm(rec); filledSig = sig; }
    applyDraft();
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
    applyReveal();
  }

  function applyReveal() {
    const revealed = isWorkbenchRevealed(ui);
    root?.classList.toggle('is-workbench-hidden', !revealed);
    toggleWbEl?.setAttribute('aria-pressed', ui.collapsed ? 'true' : 'false');
  }

  function selectRecord(id) {
    if (!id) return;
    ui.selectedId = id; ui.creating = false; ui.collapsed = false;
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
    filledSig = null;
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

  // ONE control switches the record view. It is an action, not a state: the
  // icon and the label name the view it switches TO, and no aria-pressed is
  // set. A view switch is an intentional reset, so it is announced through the
  // canonical grammar-change event — that drops the recorded scroll offset and
  // re-renders the list through `onGrammarChange`.
  function toggleView() {
    if (!viewToggleEl) return;
    const next = viewToggleEl.getAttribute('data-ats-view-toggle') === 'list' ? 'cards' : 'list';
    viewToggleEl.setAttribute('data-ats-view-toggle', next);
    const label = t(next === 'list' ? 'viewAsCards' : 'viewAsList');
    viewToggleEl.title = label;
    viewToggleEl.setAttribute('aria-label', label);
    let announced = false;
    try {
      announced = Boolean(leftPane?.dispatchEvent(
        new CustomEvent('ctox-pane-grammar-change', { detail: { view: next }, bubbles: true }),
      ));
    } catch { announced = false; }
    if (!announced) renderList();
  }

  function render() {
    renderList();
    renderWorkbench();
  }

  // Meeting state transitions are plain collection writes — the scheduling engine owns
  // the valid MEETING_STATES, not a native command.
  function transitionMeeting(meetingId, state, patch) {
    if (!isMeetingState(state)) return;
    if (!api.get(PRIMARY, meetingId).get()) { report('warn', 'interviews.meeting.missing', t('meetingMissing'), meetingId); return; }
    clear('interviews.meeting.missing');
    const now = Date.now();
    followWrite(api.patch(PRIMARY, meetingId, Object.assign({ state, updated_at_ms: now }, patch || {})), 'interviews.meeting.update_failed', t('updateFailed'));
  }

  function onSubmit(event) {
    event.preventDefault();
    const fields = readForm();
    if (!fields.candidate_id) { report('warn', 'interviews.form.candidate_required', t('candidateRequired')); return; }
    clear('interviews.form.candidate_required');
    const now = Date.now();
    if (ui.selectedId) {
      // Edit: patch the selected record's editable fields. state/attended/
      // created_at stay owned by the transition flow — payload shape unchanged.
      if (!api.get(PRIMARY, ui.selectedId).get()) { report('warn', 'interviews.meeting.missing', t('meetingMissing'), ui.selectedId); return; }
      clear('interviews.meeting.missing');
      followWrite(api.patch(PRIMARY, ui.selectedId, Object.assign({}, fields, { updated_at_ms: now })), 'interviews.meeting.update_failed', t('updateFailed'));
      return;
    }
    // Create: the existing insert flow, unchanged.
    const record = Object.assign({
      id: 'imeet_' + now + '_' + Math.round(now % 1e6),
    }, fields, {
      state: 'proposed', attended: false,
      created_at_ms: now, updated_at_ms: now, _deleted: false,
    });
    followWrite(api.put(PRIMARY, record), 'interviews.meeting.create_failed', t('createFailed'));
    // Bind the freshly created record in the workbench (lokal sofort sichtbar).
    ui.selectedId = record.id; ui.creating = false; ui.collapsed = false;
    render();
  }

  function openImport() { importInput?.click?.(); }

  async function onImportChange(event) {
    const file = event.target?.files?.[0];
    if (importInput) importInput.value = '';
    if (!file) return;
    let list;
    try {
      const parsed = JSON.parse(await file.text());
      list = Array.isArray(parsed) ? parsed
        : Array.isArray(parsed?.records) ? parsed.records
          : Array.isArray(parsed?.interview_meetings) ? parsed.interview_meetings : [];
    } catch (e) {
      report('error', 'interviews.import.failed', t('importFailed') + ': ' + (e?.message || e));
      return;
    }
    clear('interviews.import.failed');
    let ok = 0;
    for (let i = 0; i < list.length; i += 1) {
      const rec = normalizeImported(list[i], i);
      if (!rec) continue;
      try { await api.put(PRIMARY, rec).local; ok += 1; } catch (e) { console.error('[interviews] import failed:', e); }
    }
    if (ok < list.length) report('warn', 'interviews.import.partial', t('importFailed') + ': ' + ok + ' / ' + list.length);
    else clear('interviews.import.partial');
  }

  function exportVisible() {
    const grammar = readGrammar(leftPane);
    const { visible } = partitionMeetings(meetings, {
      search: grammar.search, status: grammar.status, band: grammar.band, nowMs: Date.now(),
    });
    const payload = { schema: PRIMARY, exported_at_ms: Date.now(), records: visible };
    downloadTextFile('interview-meetings.json', JSON.stringify(payload, null, 2), 'application/json', host);
  }

  function onRootClick(event) {
    const actionEl = event.target?.closest?.('[data-action]');
    if (actionEl) {
      const action = actionEl.getAttribute('data-action');
      if (action === 'new') return startCreate();
      if (action === 'import') return openImport();
      if (action === 'export') return exportVisible();
    }
    if (event.target?.closest?.('[data-toggle-workbench]')) return toggleWorkbench();
    if (event.target?.closest?.('[data-ats-view-toggle]')) return toggleView();
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

  disposers.push(api.query(PRIMARY, {}).subscribe((rows) => {
    meetings = (rows || []).filter((r) => r && !r._deleted);
    meetingsSeen = true;
    render();
  }));
  disposers.push(api.query(SCORECARDS, {}).subscribe((rows) => {
    scorecards = (rows || []).filter((r) => r && !r._deleted);
    renderWorkbench();
  }));
  disposers.push(api.shell.locale.subscribe((next) => {
    const lang = langFrom(next);
    if (lang === locale) return;
    locale = lang;
    copy = copyFor(api, lang);
    applyCopy();
    render();
  }));

  return {
    unmount() {
      for (const dispose of disposers.splice(0)) {
        try { dispose(); } catch (e) { console.warn('[interviews] unsubscribe failed', e); }
      }
      root?.removeEventListener('click', onRootClick);
      root?.removeEventListener('ctox-pane-grammar-change', onGrammarChange);
      formEl?.removeEventListener('submit', onSubmit);
      importInput?.removeEventListener('change', onImportChange);
      host.replaceChildren();
    },
    snapshot() {
      const g = readGrammar(leftPane);
      return {
        v: SNAPSHOT_VERSION,
        selectedId: ui.selectedId,
        creating: ui.creating,
        collapsed: ui.collapsed,
        search: leftPane?.querySelector('[data-pg-search]')?.value || '',
        view: g.view,
        band: g.band,
        status: g.status,
        trayOpen: leftPane?.querySelector('[data-pg-tray]')?.hidden === false,
        form: formValues(),
        listScrollTop: (listEl?.closest('.ctox-pane-body') || listEl)?.scrollTop || 0,
      };
    },
  };
}

function isPlainObject(v) { return Boolean(v) && typeof v === 'object' && !Array.isArray(v); }
function langFrom(value) { return String(value || '').toLowerCase().startsWith('en') ? 'en' : 'de'; }
// Sprache: eingebettete Texte, api.shell.t('interviews.<key>') uebersteuert, wenn es nicht den Schluessel liefert.
function copyFor(api, lang) {
  const base = COPY[lang];
  const out = {};
  for (const key of Object.keys(base)) {
    const k = MODULE_ID + '.' + key;
    let v = null;
    try { v = api?.shell?.t?.(k); } catch {}
    out[key] = typeof v === 'string' && v && v !== k ? v : base[key];
  }
  return out;
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

// Record-list body: rows only. An empty list renders nothing (App-API 4.0: no
// state text in the content; state lives only in the shell status zone).
export function meetingListBody(visible, { t = (k) => k, locale = 'de', nowMs = 0, selectedId = null, view = 'cards' } = {}) {
  return (Array.isArray(visible) ? visible : []).map((r) => meetingShard(r, { t, locale, nowMs, selectedId, view })).join('');
}

// A shard is a pure selector, rendered in one of TWO deliberately different
// shapes (operator directive 31.08.):
//   cards — three lines: bold title + state badge, then schedule and setup meta
//   list  — exactly ONE dense line: title plus a single short trailing meta
// The outer button (class, selection marker, agent-context trio) is identical
// in both, so selection and the context menu do not depend on the view.
// Namen statt IDs (Textdiät Regel 4): Anzeigename aus dem Datensatz (<basis>_name / <basis>.name / <basis>_title),
// sonst „Unbekannt“ – nie die technische ID.
function refName(r, base, lang = 'de') {
  const v = r?.[base + '_name'] ?? r?.[base]?.name ?? r?.[base + '_title'];
  return v ? String(v) : (lang === 'en' ? 'Unknown' : 'Unbekannt'); // v4-allow: en
}

export function meetingShard(r, { t = (k) => k, locale = 'de', nowMs = 0, selectedId = null, view = 'cards' } = {}) {
  const state = effectiveState(r, nowMs);
  const badge = isMeetingState(state) ? state : 'proposed';
  const label = stateLabel(state, t);
  const cand = esc(refName(r, 'candidate', locale));
  const vac = r.vacancy_id ? ' → ' + esc(refName(r, 'vacancy', locale)) : '';
  const selected = selectedId && r.id === selectedId ? ' is-selected' : '';
  const open = ''
    + '<button type="button" class="ats-shard' + selected + '" data-ats-select="' + esc(r.id || '') + '"'
    + ' data-context-record-id="' + esc(r.id || '') + '" data-context-record-type="interview_meeting" data-context-label="' + cand + '">';

  if (view === 'list') {
    // Maximum density: the state survives as a colour dot that carries the
    // state word for screen readers, so colour is never the only channel.
    return open
      + '<span class="ats-shard-line">'
      + '<span class="ats-shard-dot" data-status="' + esc(badge) + '"><span class="ats-sr">' + esc(label) + '</span></span>'
      + '<span class="ats-shard-title">' + cand + vac + '</span>'
      + '<span class="ats-shard-trail">' + esc(fmtTimeShort(r.start, locale)) + '</span>'
      + '</span></button>';
  }

  const partyCount = Array.isArray(r.parties) ? r.parties.length : 0;
  // Fehlt ein Datum, steht nichts da (FIX9a): kein „01.01.70“, kein Ersatztext.
  const schedule = [esc(fmtTime(r.start, locale))].filter(Boolean);
  const minutes = durationMinutes(r);
  if (minutes != null) schedule.push(minutes + ' ' + esc(t('minutesShort')));
  const setup = [
    esc(locationLabel(r.location_mode, t)),
    partyCount + ' ' + esc(partyCount === 1 ? t('party') : t('parties')),
  ];
  if (r.video_link) setup.push(esc(t('link')));
  return open
    + '<span class="ats-shard-head">'
    + '<span class="ats-shard-title">' + cand + vac + '</span>'
    + badgeSpan(badge, label)
    + '</span>'
    + '<span class="ats-shard-meta">' + schedule.join(' · ') + '</span>'
    + '<span class="ats-shard-meta">' + setup.join(' · ') + '</span>'
    + '</button>';
}

// Scheduled length in whole minutes, or null when start/end do not both exist.
function durationMinutes(r) {
  const start = toEpochMs(r?.start);
  const end = toEpochMs(r?.end);
  if (start == null || end == null || end <= start) return null;
  return Math.round((end - start) / 60_000);
}

function locationLabel(mode, t) {
  const map = { video: 'modeVideo', onsite: 'modeOnsite', phone: 'modePhone' };
  const key = map[String(mode || 'video')];
  return key ? t(key) : String(mode);
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
    // Single-control toggle: the button holds the view it currently shows.
    view: pane?.querySelector('[data-ats-view-toggle]')?.getAttribute('data-ats-view-toggle') || 'cards',
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

// `host` ist der App-Host des Moduls und wird vom Aufrufer durchgereicht -
// der Anker darf nie auf document.body und damit ausserhalb des Fensters landen.
function downloadTextFile(filename, content, mimeType, host) {
  const blob = new Blob([content], { type: mimeType || 'text/plain;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = filename;
  link.style.display = 'none';
  (host || document.documentElement).append(link);
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
  const cand = esc(refName(r, 'candidate'));
  const vac = r.vacancy_id ? ' → ' + esc(refName(r, 'vacancy')) : '';
  const critCount = Array.isArray(r.criteria) ? r.criteria.length : 0;
  const metaBits = [
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

// Zeitpunkt → Millisekunden oder null (FIX9a): Sekunden (< 1e11) werden umgerechnet, Text über Date.parse;
// 0, Ungültiges und alles vor 2000 gilt als „kein Datum“ – nie „01.01.70“.
const MIN_PLAUSIBLE_MS = Date.UTC(2000, 0, 1);
export function toEpochMs(value) {
  if (value == null || value === '' || typeof value === 'boolean') return null;
  let n = typeof value === 'number' ? value : (/^\s*-?\d+(\.\d+)?\s*$/.test(String(value)) ? Number(value) : Date.parse(String(value)));
  if (!Number.isFinite(n) || n <= 0) return null;
  if (n < 1e11) n *= 1000;
  return n >= MIN_PLAUSIBLE_MS ? n : null;
}

function toLocalInput(ms) {
  const n = toEpochMs(ms);
  if (n == null) return '';
  const d = new Date(n);
  const pad = (x) => String(x).padStart(2, '0');
  return d.getFullYear() + '-' + pad(d.getMonth() + 1) + '-' + pad(d.getDate()) + 'T' + pad(d.getHours()) + ':' + pad(d.getMinutes());
}

function fmtTime(ms, locale) {
  const n = toEpochMs(ms);
  if (n == null) return '';
  try { return new Date(n).toLocaleString(locale === 'en' ? 'en-US' : 'de-DE', { dateStyle: 'short', timeStyle: 'short' }); }
  catch { return new Date(n).toISOString(); }
}

// Dense list trail: day/month + time only — the year and the seconds cost more
// width than they inform in a one-line row.
function fmtTimeShort(ms, locale) {
  const n = toEpochMs(ms);
  if (n == null) return '';
  try {
    return new Date(n).toLocaleString(locale === 'en' ? 'en-US' : 'de-DE', {
      month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit',
    });
  } catch { return new Date(n).toISOString().slice(0, 16).replace('T', ' '); }
}

async function loadMarkup() {
  const html = await fetch(new URL('./index.html', import.meta.url)).then((r) => r.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((n) => n.remove());
  return doc.body.innerHTML;
}
function esc(v) { return String(v == null ? '' : v).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;'); }
