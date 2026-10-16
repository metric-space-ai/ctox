// App-API 4.0 (Shell v4): mount(root, api, {restored}) -> {unmount, snapshot}.
import { normalizeApplication, applicationDedupeKey } from './core/application.js';

const MODULE_ID = 'intake';
const PRIMARY = 'applications';
const CAPTURE_COMMAND = 'ats.intake.capture';
// Application statuses that count as "closed" (out of the active funnel). The
// left column's counted band splits Alle / Offen / Abgeschlossen from this.
const CLOSED_STATUSES = new Set(['hired', 'rejected', 'duplicate']);

const COPY = {
  de: {
    kicker: 'Bewerbungseingang', listTitle: 'Bewerbungen', newTitle: 'Neue Bewerbung',
    name: 'Name', email: 'E-Mail', phone: 'Telefon', vacancy: 'Vakanz-ID', channel: 'Kanal',
    title: 'Bewerbungseingang',
    capture: 'Erfassen', more: 'Weitere Angaben', entries: 'Einträge',
    nameRequired: 'Name ist erforderlich.', blocked: 'Eingang blockiert.',
    unnamed: '(ohne Namen)', dedupe: 'Dedupe', vacancyLabel: 'Vakanz', documents: 'Dok.',
    received: 'Empfangen', closeDetail: 'Details schließen', imported: 'Importiert',
    importInvalid: 'Ungültige JSON-Datei.', importEmpty: 'Keine Datensätze in der Datei.',
    statusAll: 'Alle Status', bandAll: 'Alle', bandOpen: 'Offen', bandClosed: 'Abgeschlossen',
    status_new: 'Neu', status_screening: 'Prüfung', status_hired: 'Eingestellt',
    status_rejected: 'Abgelehnt', status_duplicate: 'Duplikat',
    viewToList: 'Als Liste anzeigen', viewToCards: 'Als Karten anzeigen',
    channel_career_site: 'Karriereseite', channel_job_board: 'Jobbörse',
    channel_easy_apply: 'Easy Apply', channel_email: 'E-Mail', channel_qr: 'QR',
    channel_walk_in: 'Walk-in', channel_referral: 'Empfehlung',
  },
  en: {
    kicker: 'Application intake', listTitle: 'Applications', newTitle: 'New application',
    name: 'Name', email: 'Email', phone: 'Phone', vacancy: 'Vacancy ID', channel: 'Channel',
    title: 'Application intake',
    capture: 'Capture', more: 'More details', entries: 'records',
    nameRequired: 'Name is required.', blocked: 'Intake blocked.',
    unnamed: '(unnamed)', dedupe: 'Dedupe', vacancyLabel: 'Vacancy', documents: 'docs',
    received: 'Received', closeDetail: 'Close details', imported: 'Imported',
    importInvalid: 'Invalid JSON file.', importEmpty: 'No records in the file.',
    statusAll: 'All statuses', bandAll: 'All', bandOpen: 'Open', bandClosed: 'Closed',
    status_new: 'New', status_screening: 'Screening', status_hired: 'Hired',
    status_rejected: 'Rejected', status_duplicate: 'Duplicate',
    viewToList: 'Show as list', viewToCards: 'Show as cards',
    channel_career_site: 'Careers page', channel_job_board: 'Job board',
    channel_easy_apply: 'Easy Apply', channel_email: 'Email', channel_qr: 'QR',
    channel_walk_in: 'Walk-in', channel_referral: 'Referral',
  },
};

// Ein-Knopf-Umschalter: das Icon zeigt IMMER die Ansicht, in die der Klick
// wechselt, nie die aktuelle.
const VIEW_ICON = {
  cards: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="4" y="4" width="16" height="7" rx="1.5"/><rect x="4" y="14" width="16" height="7" rx="1.5"/></svg>',
  list: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="4" y1="6" x2="20" y2="6"/><line x1="4" y1="12" x2="20" y2="12"/><line x1="4" y1="18" x2="20" y2="18"/></svg>',
};
const VIEW_ACTION_KEY = { cards: 'viewToCards', list: 'viewToList' };
let text = COPY.de;
let locale = 'de';

// ---------------------------------------------------------------------------
// Pure record helpers (exported for tests — no DOM, no RxDB).
// ---------------------------------------------------------------------------

// Auto-reveal model (design-guide "Progressive Disclosure", outbound idiom):
// the record detail is shown only when something is selected and the user has
// not collapsed it.
export function shouldRevealRecord(hasSelection, userCollapsed) {
  return Boolean(hasSelection) && !userCollapsed;
}

export function candidateName(r) {
  const candidate = r && typeof r.candidate === 'object' && r.candidate ? r.candidate : {};
  return candidate.name || r?.name || text.unnamed;
}

export function statusLabel(status) {
  const key = 'status_' + String(status || 'new');
  return text[key] || String(status || 'new');
}

// Der Kanal steht im Datensatz als Schluessel (career_site, job_board, ...).
// Beschriftungen sind uebersetzt; ein unbekannter Schluessel faellt auf seinen
// Rohwert zurueck, statt die Zeile leer zu lassen.
export function channelLabel(channel) {
  const raw = String(channel || '').trim();
  if (!raw) return '—';
  return text['channel_' + raw] || raw;
}

// Which counted band a record belongs to.
export function bandOf(status) {
  return CLOSED_STATUSES.has(String(status || 'new')) ? 'closed' : 'open';
}

export function countsFor(rows) {
  const list = Array.isArray(rows) ? rows : [];
  let open = 0;
  let closed = 0;
  for (const r of list) {
    if (bandOf(r.status) === 'closed') closed += 1; else open += 1;
  }
  return { all: list.length, open, closed };
}

// Apply the current grammar state (band + status filter + search) to the rows.
export function filterRows(rows, { band = 'all', status = 'all', search = '' } = {}) {
  const needle = String(search || '').trim().toLowerCase();
  return (Array.isArray(rows) ? rows : []).filter((r) => {
    const st = String(r.status || 'new');
    if (band === 'open' && bandOf(st) !== 'open') return false;
    if (band === 'closed' && bandOf(st) !== 'closed') return false;
    if (status && status !== 'all' && st !== status) return false;
    if (needle) {
      const candidate = r && typeof r.candidate === 'object' && r.candidate ? r.candidate : {};
      const hay = [candidateName(r), r.channel, r.vacancy_id, candidate.email, candidate.phone, r.email, r.phone, st, r.id]
        .filter(Boolean).join(' ').toLowerCase();
      if (!hay.includes(needle)) return false;
    }
    return true;
  });
}

// A shard is a pure selector: no inline expansion inside the selection list
// (design-guide "Canonical Column Grammar"). Karten und Liste unterscheiden
// sich in der DICHTE, nicht nur im Detailgrad (Betreiber-Direktive 31.08.):
//
//   KARTE  drei Zeilen — fetter Name + Status, darunter die Zuordnung
//          (Kanal · Vakanz · Kontakt) und die Eckdaten (Empfangen · Dok.);
//          grosszuegiges Padding.
//   LISTE  genau EINE Zeile — Name und rechts ein einziges Kurz-Meta (der
//          Status); enge Zeilenhoehe, maximale Dichte.
//
// Beide lesen ausschliesslich Felder, die die App ohnehin laedt.
export function applicationRow(r, opts = {}) {
  const view = opts.view === 'list' ? 'list' : 'cards';
  const selected = Boolean(opts.selected);
  const name = candidateName(r);
  const status = String(r.status || 'new');
  const id = r.id || '';
  const badge = '<span class="' + ('ctox-badge ' + statusBadgeClass(status)).trim()
    + '" data-status="' + esc(status) + '">' + esc(statusLabel(status)) + '</span>';
  const attrs = ' class="ctox-list-item intake-row intake-row--' + view + (selected ? ' is-selected' : '') + '"'
    + ' role="button" tabindex="0" aria-selected="' + (selected ? 'true' : 'false') + '"'
    + ' data-context-record-id="' + esc(id) + '"'
    + ' data-context-record-type="application"'
    + ' data-context-label="' + esc(name || id) + '"';
  if (view === 'list') {
    return '<div' + attrs + '>'
      + '<span class="intake-row-title">' + esc(name) + '</span>'
      + '<span class="intake-row-side">' + badge + '</span>'
      + '</div>';
  }
  const candidate = r && typeof r.candidate === 'object' && r.candidate ? r.candidate : {};
  const assignment = [text.channel + ': ' + channelLabel(r.channel)];
  if (r.vacancy_id) assignment.push(text.vacancyLabel + ': ' + refName(r, 'vacancy', locale));
  const contact = candidate.email || r.email || candidate.phone || r.phone;
  if (contact) assignment.push(String(contact));
  const ts = Number(r.received_at_ms || r.created_at_ms || 0);
  const facts = [text.received + ': ' + (ts ? fmtDay(ts) : '—')];
  const docs = Array.isArray(r.documents) ? r.documents.length : 0;
  if (docs) facts.push(text.documents + ': ' + docs);
  return '<div' + attrs + '>'
    + '<div class="intake-row-head"><span class="intake-row-title">' + esc(name) + '</span>' + badge + '</div>'
    + '<div class="intake-row-meta">' + esc(assignment.join(' · ')) + '</div>'
    + '<div class="intake-row-meta intake-row-meta--facts">' + esc(facts.join(' · ')) + '</div>'
    + '</div>';
}

// The list body markup (shards or compact rows). An empty list renders
// nothing (App-API 4.0: no state text in the content; state lives only in the
// shell status zone).
export function renderRecordList(rows, opts = {}) {
  const list = Array.isArray(rows) ? rows : [];
  return list.map((r) => applicationRow(r, { view: opts.view, selected: r.id && r.id === opts.selectedId })).join('');
}

// Read-only detail card for the selected record.
export function recordDetailHtml(r) {
  const name = candidateName(r);
  const status = String(r.status || 'new');
  const candidate = r && typeof r.candidate === 'object' && r.candidate ? r.candidate : {};
  const rows = [];
  const contact = [candidate.email || r.email, candidate.phone || r.phone].filter(Boolean).map(esc).join(' · ');
  rows.push(field(text.channel, esc(channelLabel(r.channel))));
  if (contact) rows.push(field(text.email, contact));
  if (r.vacancy_id) rows.push(field(text.vacancyLabel, esc(refName(r, 'vacancy', locale))));
  const docs = Array.isArray(r.documents) ? r.documents.length : 0;
  if (docs) rows.push(field(text.documents, String(docs)));
  const ts = Number(r.received_at_ms || r.created_at_ms || 0);
  if (ts) rows.push(field(text.received, esc(fmtDate(ts))));
  return '<header class="intake-detail-head">'
    + '<div class="intake-detail-titles">'
    + '<span class="' + ('ctox-badge ' + statusBadgeClass(status)).trim() + '" data-status="' + esc(status) + '">' + esc(statusLabel(status)) + '</span>'
    + '<strong class="intake-detail-name">' + esc(name) + '</strong>'
    + '</div>'
    + '<button type="button" class="ctox-pane-icon" data-action="collapse-detail" aria-label="' + esc(text.closeDetail) + '" title="' + esc(text.closeDetail) + '"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"/></svg></button>'
    + '</header>'
    + '<dl class="ctox-fields ctox-fields--stacked">' + rows.join('') + '</dl>';
}

function field(label, valueHtml) {
  return '<dt>' + esc(label) + '</dt><dd>' + valueHtml + '</dd>';
}

// Reuse the intake normalizer for imported JSON, preserving id/status/timestamps
// from an exported record (round-trip friendly). Required fields: id, channel,
// updated_at_ms.
export function prepareImport(raw, nowMs, salt = '') {
  const norm = normalizeApplication(raw);
  const now = Number(nowMs) || 0;
  const id = norm.id || (raw && typeof raw === 'object' && raw.id) || ('app_' + now + '_' + salt);
  const dedupe_key = (raw && typeof raw === 'object' && raw.dedupe_key) || applicationDedupeKey(norm) || '';
  const status = raw && typeof raw === 'object' && typeof raw.status === 'string' && raw.status ? raw.status : norm.status;
  const received = norm.received_at_ms || Number(raw && raw.received_at_ms) || now;
  const created = Number(raw && raw.created_at_ms) || received;
  return { ...norm, id: String(id), dedupe_key, status, received_at_ms: received, created_at_ms: created, updated_at_ms: now };
}

// ---------------------------------------------------------------------------
// Mount (App-API 4.0): mount(root, api, { restored }) -> { unmount, snapshot }
// ---------------------------------------------------------------------------

const SNAPSHOT_VERSION = 1;
const FORM_FIELDS = ['name', 'email', 'phone', 'vacancy_id', 'channel'];

export async function mount(root, api, { restored } = {}) {
  if (!root.dataset.app) root.dataset.app = api.app?.id || MODULE_ID;
  setLocale(api, api.shell.locale.get());
  // CSS laedt die Shell aus dem Manifest; index.html liefert der Service Worker versioniert (§16.15).
  root.innerHTML = await loadMarkup();
  const disposers = [];
  const appRoot = root.querySelector('[data-ats-root]');
  const rail = appRoot?.querySelector('.intake-rail');
  const listEl = appRoot?.querySelector('[data-ats-list]');
  const formEl = appRoot?.querySelector('[data-ats-form]');
  const titleEl = appRoot?.querySelector('[data-ats-title]');
  const subEl = appRoot?.querySelector('[data-ats-sub]');
  const detailEl = appRoot?.querySelector('[data-ats-detail]');
  const viewToggleEl = rail?.querySelector('.intake-view-toggle');
  const restore = isPlainObject(restored) && restored.v === SNAPSHOT_VERSION ? restored : null;
  applyStaticCopy(appRoot);
  api.shell.setTitle(text.title);

  let rowsCache = [];
  let selectedId = restore && typeof restore.selectedId === 'string' ? restore.selectedId : null;
  let userCollapsed = Boolean(restore && restore.userCollapsed === true);
  let pendingScrollTop = restore && Number(restore.listScrollTop) > 0 ? Number(restore.listScrollTop) : 0;
  if (restore) restoreView(restore);
  // Die Spalten-Grammatik verdrahtet die App selbst (kein DOM-Heuristik-Warten auf die Shell).
  const { autoWirePaneGrammar } = await import('ctox:shared/pane-grammar.js');
  autoWirePaneGrammar(appRoot);

  // Zustand nur ueber die Zustandszone der Shell, stabile Codes je Fall.
  const report = (level, code, str, ref) => api.status.report(ref ? { level, code, text: str, ref } : { level, code, text: str });
  const clear = (code) => api.status.clear(code);

  function restoreView(snap) {
    const search = rail?.querySelector('[data-pg-search]');
    if (search && typeof snap.search === 'string') search.value = snap.search;
    if (viewToggleEl && (snap.view === 'list' || snap.view === 'cards')) {
      viewToggleEl.dataset.pgView = snap.view;
      viewToggleEl.dataset.pgViewAlt = snap.view === 'list' ? 'cards' : 'list';
    }
    if (typeof snap.band === 'string') {
      const tabs = [...(rail?.querySelectorAll('[data-pg-band]') || [])];
      if (tabs.some((tab) => tab.dataset.pgBand === snap.band)) {
        tabs.forEach((tab) => {
          const on = tab.dataset.pgBand === snap.band;
          tab.setAttribute('aria-selected', String(on));
          tab.classList.toggle('is-active', on);
        });
      }
    }
    const statusSel = rail?.querySelector('[data-pg-filter][data-pg-name="status"]');
    if (statusSel && typeof snap.status === 'string' && [...statusSel.options].some((o) => o.value === snap.status)) statusSel.value = snap.status;
    const tray = rail?.querySelector('[data-pg-tray]');
    const trayToggle = rail?.querySelector('[data-pg-tray-toggle]');
    if (tray && snap.trayOpen === true) { tray.hidden = false; trayToggle?.setAttribute('aria-expanded', 'true'); }
    if (isPlainObject(snap.form)) {
      for (const name of FORM_FIELDS) if (typeof snap.form[name] === 'string') setField(name, snap.form[name]);
    }
    const more = formEl?.querySelector('details.ats-more');
    if (more && snap.moreOpen === true) more.open = true;
  }

  function readForm() {
    const out = {};
    for (const name of FORM_FIELDS) out[name] = formEl?.querySelector('[name="' + name + '"]')?.value ?? '';
    return out;
  }

  // Read the current SHELL-wired grammar state straight from the pane DOM.
  function readGrammar() {
    return {
      search: (rail?.querySelector('[data-pg-search]')?.value || '').trim().toLowerCase(),
      // Die Ansicht steht auf dem EINEN Umschalter (data-pg-view = aktuelle
      // Ansicht), nicht auf einem gedrueckten Knopf eines Zustandspaars.
      view: rail?.querySelector('[data-pg-view]')?.dataset.pgView === 'list' ? 'list' : 'cards',
      band: rail?.querySelector('[data-pg-band][aria-selected="true"]')?.dataset.pgBand || 'all',
      status: rail?.querySelector('[data-pg-filter][data-pg-name="status"]')?.value || 'all',
    };
  }

  // Counts/footer via the pane-grammar handle (wired above by the app itself).
  function writeCounts(counts) {
    const pg = rail?.__ctoxPaneGrammar;
    if (pg && typeof pg.setCounts === 'function') { pg.setCounts(counts); return; }
    for (const [key, value] of Object.entries(counts)) {
      const node = rail?.querySelector('[data-pg-count="' + key + '"]');
      if (node) node.textContent = ' (' + value + ')';
    }
  }
  function writeFooter(str) {
    const pg = rail?.__ctoxPaneGrammar;
    if (pg && typeof pg.setFooter === 'function') { pg.setFooter(str); return; }
    const node = rail?.querySelector('[data-pg-footer]');
    if (node) node.textContent = str || '';
  }

  function renderDetail() {
    if (!detailEl) return;
    const rec = selectedId ? rowsCache.find((r) => r.id === selectedId) : null;
    const show = shouldRevealRecord(Boolean(rec), userCollapsed);
    detailEl.hidden = !show;
    detailEl.innerHTML = show ? recordDetailHtml(rec) : '';
    if (show) {
      detailEl.setAttribute('data-context-record-id', rec.id || '');
      detailEl.setAttribute('data-context-record-type', 'application');
      detailEl.setAttribute('data-context-label', candidateName(rec));
    }
    if (titleEl) titleEl.textContent = rec ? candidateName(rec) : text.newTitle;
  }

  function render() {
    const g = readGrammar();
    const filtered = filterRows(rowsCache, g);
    if (listEl) listEl.innerHTML = renderRecordList(filtered, { view: g.view, selectedId });
    writeCounts(countsFor(rowsCache));
    const scope = g.band === 'all' ? text.bandAll : g.band === 'open' ? text.bandOpen : text.bandClosed;
    writeFooter(filtered.length + ' ' + text.entries + ' · ' + scope);
    renderDetail();
    const scroller = listEl?.closest('.ctox-pane-body');
    if (pendingScrollTop && scroller && rowsCache.length) {
      scroller.scrollTop = pendingScrollTop;
      pendingScrollTop = 0;
    }
  }

  function setRows(rows) {
    const list = (Array.isArray(rows) ? rows : []).filter((r) => r && !r._deleted);
    list.sort((a, b) => (Number(b.received_at_ms || b.created_at_ms || 0) - Number(a.received_at_ms || a.created_at_ms || 0)));
    rowsCache = list;
  }

  function fillForm(rec) {
    if (!formEl) return;
    const candidate = rec && typeof rec.candidate === 'object' && rec.candidate ? rec.candidate : {};
    setField('name', candidate.name || rec.name || '');
    setField('email', candidate.email || rec.email || '');
    setField('phone', candidate.phone || rec.phone || '');
    setField('vacancy_id', rec.vacancy_id || '');
    setField('channel', rec.channel || 'email');
  }
  function setField(name, value) {
    const el = formEl?.querySelector('[name="' + name + '"]');
    if (el) el.value = value == null ? '' : String(value);
  }

  function selectRecord(id) {
    selectedId = id || null;
    userCollapsed = false;
    const rec = selectedId ? rowsCache.find((r) => r.id === selectedId) : null;
    if (rec) fillForm(rec);
    // Selection is an in-place class flip — a list rebuild resets the
    // operator's scroll (design-guide: re-renders never move the operator).
    applyListSelection();
    renderDetail();
  }

  function applyListSelection() {
    listEl?.querySelectorAll('[data-context-record-id]').forEach((row) => {
      const on = (row.getAttribute('data-context-record-id') || '') === String(selectedId || '');
      row.classList.toggle('is-selected', on);
      row.setAttribute('aria-selected', String(on));
    });
  }

  function startNew() {
    selectedId = null;
    userCollapsed = false;
    try { formEl?.reset(); } catch {}
    render();
    formEl?.querySelector('[name="name"]')?.focus?.();
  }

  function exportRecords() {
    const filtered = filterRows(rowsCache, readGrammar());
    let url = '';
    try {
      const blob = new Blob([JSON.stringify(filtered, null, 2)], { type: 'application/json' });
      url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = 'intake-applications.json';
      a.rel = 'noopener';
      root.appendChild(a);
      a.click();
      a.remove();
    } catch (e) {
      console.error('[intake] export failed:', e);
    } finally {
      if (url) setTimeout(() => { try { URL.revokeObjectURL(url); } catch {} }, 4000);
    }
  }

  // Import: jede Zeile ein lokaler Write (api.put), sichtbar ueber das Live-Abo.
  async function importFile(file) {
    let parsed;
    try { parsed = JSON.parse(await file.text()); } catch {
      report('error', 'intake.import.invalid', text.importInvalid);
      return;
    }
    clear('intake.import.invalid');
    const items = Array.isArray(parsed) ? parsed : (parsed && typeof parsed === 'object' ? [parsed] : []);
    if (!items.length) { report('warn', 'intake.import.empty', text.importEmpty); return; }
    clear('intake.import.empty');
    const nowMs = Date.now();
    let count = 0;
    for (const raw of items) {
      try { await api.put(PRIMARY, prepareImport(raw, nowMs, String(count) + Math.random().toString(36).slice(2, 6))).local; count += 1; }
      catch (e) { console.error('[intake] import failed:', e); }
    }
    if (count < items.length) report('warn', 'intake.import.partial', text.imported + ': ' + count + '/' + items.length);
    else clear('intake.import.partial');
  }

  function importRecords() {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'application/json,.json';
    input.addEventListener('change', () => {
      const file = input.files && input.files[0];
      if (file) importFile(file);
    });
    input.click();
  }

  function onListClick(event) {
    const row = event.target?.closest?.('[data-context-record-id]');
    if (!row || !listEl.contains(row)) return;
    selectRecord(row.getAttribute('data-context-record-id'));
  }
  function onListKey(event) {
    if (event.key !== 'Enter' && event.key !== ' ') return;
    const row = event.target?.closest?.('[data-context-record-id]');
    if (!row || !listEl.contains(row)) return;
    event.preventDefault();
    selectRecord(row.getAttribute('data-context-record-id'));
  }

  function onAction(event) {
    const btn = event.target?.closest?.('[data-action]');
    if (!btn) return;
    const action = btn.dataset.action;
    if (action === 'new') startNew();
    else if (action === 'import') importRecords();
    else if (action === 'export') exportRecords();
    else if (action === 'collapse-detail') { userCollapsed = true; renderDetail(); }
  }

  // Befehl: api.command legt das Befehlsdokument lokal an und liefert sofort; die UI wartet nie.
  // Ergebnis (blocked/ok) und Fehlschlag kommen ueber den gepushten Status in die Zustandszone.
  function onSubmit(event) {
    event.preventDefault();
    const data = new FormData(formEl);
    const f = Object.fromEntries(data.entries());
    const name = String(f.name == null ? '' : f.name).trim();
    if (!name) { report('warn', 'intake.capture.name_required', text.nameRequired); return; }
    clear('intake.capture.name_required');
    const email = String(f.email == null ? '' : f.email).trim();
    const phone = String(f.phone == null ? '' : f.phone).trim();
    const vacancy_id = String(f.vacancy_id == null ? '' : f.vacancy_id).trim();
    const channel = String(f.channel == null ? '' : f.channel).trim() || 'email';
    const payload = {
      name,
      email: email || null,
      phone: phone || null,
      vacancy_id: vacancy_id || null,
      channel,
    };
    const submitted = JSON.stringify(readForm());
    const handle = api.command(CAPTURE_COMMAND, payload);
    const code = 'intake.command.' + CAPTURE_COMMAND;
    let off = null;
    let finished = false;
    const stop = () => { finished = true; off?.(); off = null; };
    off = handle.status.subscribe((s) => {
      if (s?.status === 'failed') {
        report('error', code, CAPTURE_COMMAND + ': ' + (s.error || 'failed'), handle.id);
        stop();
      } else if (s?.status === 'done') {
        clear(code);
        const blockers = captureBlockers(s.result);
        if (blockers) {
          report('warn', 'intake.capture.blocked', text.blocked + (blockers.length ? ' ' + blockers.join(' · ') : ''), handle.id);
        } else {
          clear('intake.capture.blocked');
          // Formular nur leeren, wenn der Nutzer seither nichts Neues eingegeben hat.
          if (JSON.stringify(readForm()) === submitted) {
            selectedId = null;
            try { formEl.reset(); } catch {}
            render();
          }
        }
        stop();
      }
    });
    if (finished) off?.();
    handle.local.catch((e) => report('error', code, CAPTURE_COMMAND + ': ' + (e?.code || 'local') + ' ' + (e?.message || e), handle.id));
    disposers.push(stop);
  }

  // Ein-Knopf-Umschalter Karten <-> Liste. Der Knopf ist eine AKTION: Icon,
  // aria-label und title benennen das Ziel des Klicks, einen Gedrueckt-Zustand
  // gibt es nicht.
  function syncViewToggle() {
    if (!viewToggleEl) return;
    const next = viewToggleEl.dataset.pgViewAlt === 'cards' ? 'cards' : 'list';
    const label = text[VIEW_ACTION_KEY[next]];
    viewToggleEl.innerHTML = VIEW_ICON[next];
    viewToggleEl.setAttribute('aria-label', label);
    viewToggleEl.setAttribute('title', label);
    viewToggleEl.removeAttribute('aria-pressed');
  }
  function onViewToggle() {
    if (!viewToggleEl) return;
    const current = viewToggleEl.dataset.pgView === 'list' ? 'list' : 'cards';
    viewToggleEl.dataset.pgView = current === 'list' ? 'cards' : 'list';
    viewToggleEl.dataset.pgViewAlt = current;
    syncViewToggle();
    render();
  }
  // Die generische Verdrahtung (shared/pane-grammar.js) setzt auf JEDEN
  // [data-pg-view]-Knopf ein aria-pressed="true". Fuer einen Aktionsknopf ist
  // das die falsche Semantik. Dieser Listener haengt an der SPALTE, laeuft also
  // in der Bubble-Phase garantiert nach beiden Knopf-Listenern, und raeumt es
  // ab — ohne Timer und ohne Annahme ueber die Registrierungsreihenfolge.
  function stripViewTogglePressed() {
    viewToggleEl?.removeAttribute('aria-pressed');
  }

  listEl?.addEventListener('click', onListClick);
  listEl?.addEventListener('keydown', onListKey);
  appRoot?.addEventListener('click', onAction);
  viewToggleEl?.addEventListener('click', onViewToggle);
  rail?.addEventListener('click', stripViewTogglePressed);
  syncViewToggle();
  formEl?.addEventListener('submit', onSubmit);
  // Re-render when the pane grammar reports a change (search / view / tray / band).
  const onGrammarChange = () => { render(); };
  rail?.addEventListener('ctox-pane-grammar-change', onGrammarChange);

  disposers.push(api.query(PRIMARY, {}).subscribe((rows) => { setRows(rows); render(); }));
  disposers.push(api.shell.locale.subscribe((next) => {
    if (!setLocale(api, next)) return;
    applyStaticCopy(appRoot);
      api.shell.setTitle(text.title);
    syncViewToggle();
    render();
  }));

  return {
    unmount() {
      for (const dispose of disposers.splice(0)) {
        try { dispose(); } catch (e) { console.warn('[intake] unsubscribe failed', e); }
      }
      listEl?.removeEventListener('click', onListClick);
      listEl?.removeEventListener('keydown', onListKey);
      appRoot?.removeEventListener('click', onAction);
      viewToggleEl?.removeEventListener('click', onViewToggle);
      rail?.removeEventListener('click', stripViewTogglePressed);
      formEl?.removeEventListener('submit', onSubmit);
      rail?.removeEventListener('ctox-pane-grammar-change', onGrammarChange);
      root.replaceChildren();
    },
    snapshot() {
      const g = readGrammar();
      return {
        v: SNAPSHOT_VERSION,
        selectedId,
        userCollapsed,
        search: rail?.querySelector('[data-pg-search]')?.value || '',
        view: g.view,
        band: g.band,
        status: g.status,
        trayOpen: rail?.querySelector('[data-pg-tray]')?.hidden === false,
        form: readForm(),
        moreOpen: Boolean(formEl?.querySelector('details.ats-more')?.open),
        listScrollTop: listEl?.closest('.ctox-pane-body')?.scrollTop || 0,
      };
    },
  };
}

// Ergebnis von ats.intake.capture: blockiert → Liste der Gruende (ggf. leer), sonst null.
export function captureBlockers(result) {
  const decision = result?.gate || result?.decision || null;
  const blockers = result?.blockers || decision?.blockers || result?.errors || null;
  const blocked = result?.ok === false || result?.status === 'blocked' || result?.allowed === false
    || decision?.status === 'blocked' || decision?.decision === 'block'
    || (Array.isArray(blockers) && blockers.length > 0);
  if (!blocked) return null;
  return (Array.isArray(blockers) ? blockers : [blockers])
    .filter(Boolean)
    .map((b) => (typeof b === 'string' ? b : (b?.message || b?.reason || JSON.stringify(b))));
}

function isPlainObject(v) { return Boolean(v) && typeof v === 'object' && !Array.isArray(v); }

// Sprache: api.shell.locale (live) + api.shell.t('intake.<key>') uebersteuert die eingebetteten Texte.
function setLocale(api, value) {
  const next = String(value || '').toLowerCase().startsWith('en') ? 'en' : 'de';
  const changed = next !== locale || text === COPY[locale];
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

// Maps an application status onto the kit badge states
// (hired/success, rejected/danger, in-flight/warning, new/info).
function statusBadgeClass(status) {
  if (status === 'hired') return 'is-success';
  if (status === 'rejected') return 'is-danger';
  if (status === 'screening' || status === 'duplicate') return 'is-warning';
  if (status === 'new') return 'is-info';
  return '';
}

// Namen statt IDs (Textdiät Regel 4): Anzeigename aus dem Datensatz (<basis>_name / <basis>.name / <basis>_title),
// sonst „Unbekannt“ – nie die technische ID.
function refName(r, base, lang = 'de') {
  const v = r?.[base + '_name'] ?? r?.[base]?.name ?? r?.[base + '_title'];
  return v ? String(v) : (lang === 'en' ? 'Unknown' : 'Unbekannt'); // v4-allow: en
}

function fmtDate(ms) {
  // Datum ohne Sekunden (TT.MM.JJJJ, HH:MM).
  try { return new Date(ms).toLocaleString(locale === 'en' ? 'en' : 'de', { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' }); } catch { return ''; }
}

// Kurzform fuer die Meta-Zeilen einer Karte: dort zaehlt der Tag, nicht die
// Sekunde (die volle Zeitangabe bleibt im Detail-Readout).
function fmtDay(ms) {
  try { return new Date(ms).toLocaleDateString(locale === 'en' ? 'en' : 'de'); } catch { return ''; }
}

function applyStaticCopy(root) {
  root.querySelectorAll('[data-copy]').forEach((node) => { node.textContent = text[node.dataset.copy] || node.textContent; });
}

async function loadMarkup() {
  const html = await fetch(new URL('./index.html', import.meta.url)).then((r) => r.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((n) => n.remove());
  return doc.body.innerHTML;
}
function esc(v) { return String(v == null ? '' : v).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;'); }
