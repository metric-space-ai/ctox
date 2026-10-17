import { autoWirePaneGrammar } from 'ctox:shared/pane-grammar.js';

const MODULE_ID = 'esign';
const PRIMARY = 'signature_requests';
const TITLE = 'esign';
const CREATE_COMMAND = 'ats.signature.request';
const SIGN_COMMAND = 'ats.signature.sign';
const SUBJECT_KINDS = ['arbeitsvertrag', 'vermittlungsvertrag', 'ueberlassungsvertrag'];
// A request is "done" once it reaches a terminal state; everything else is open.
const TERMINAL_STATUSES = new Set(['completed', 'declined', 'expired', 'done', 'failed']);
const STATUS_KEY = {
  created: 'statusCreated', sent: 'statusSent', partially_signed: 'statusPartial',
  completed: 'statusCompleted', declined: 'statusDeclined', expired: 'statusExpired',
  // Befehls-/Kernstatus (FIX8): wie DocuSign „Abgeschlossen“ bzw. „Fehlgeschlagen“, nie roh.
  done: 'statusDone', failed: 'statusFailed',
};

export const COPY = {
  de: {
    document: 'Dokument', subjectKind: 'Vertragstyp', employment: 'Arbeitsvertrag', placement: 'Vermittlungsvertrag', staffing: 'Überlassungsvertrag', signers: 'Unterzeichner', create: 'Anlegen', entries: 'Einträge', signatureCaptured: 'Signatur erfasst.', request: 'Anfrage', signer: 'Unterzeichner', status: 'Status', blocked: 'Blockiert.', documentRequired: 'Bitte ein Dokument auswählen.', requestCreated: 'Signatur-Anfrage angelegt.', sign: 'Signieren', artifact: 'Artefakt',
    title: 'E-Signatur', kicker: '', listTitle: 'Anfragen', allKinds: 'Alle Typen', viewAll: 'Alle', viewOpen: 'Offen', viewDone: 'Abgeschlossen', composerKicker: 'NEUE ANFRAGE', composerTitle: 'Signatur anfordern', recordKicker: 'ANFRAGE', signersHead: 'Unterzeichner', noSigners: 'Keine Unterzeichner erfasst.', importDone: 'Import abgeschlossen.', exportDone: 'Export erstellt.', invalidFile: 'Datei konnte nicht gelesen werden (JSON erwartet).',
    statusCreated: 'Angelegt', statusSent: 'Gesendet', statusPartial: 'Teilweise signiert', statusCompleted: 'Abgeschlossen', statusDeclined: 'Abgelehnt', statusExpired: 'Abgelaufen', statusDone: 'Abgeschlossen', statusFailed: 'Fehlgeschlagen',
    showAsList: 'Als Liste anzeigen', showAsCards: 'Als Karten anzeigen',
    signedOf: 'signiert', updated: 'Aktualisiert', sent: 'Gesendet', expires: 'Läuft ab',
    unknown: 'Unbekannt', signer_pending: 'Offen', signer_signed: 'Unterschrieben', signer_declined: 'Abgelehnt',
    documentSearch: 'Dokument suchen', signerSearch: 'Person hinzufügen', change: 'Ändern', name: 'Name', email: 'E-Mail', role: 'Rolle',
    roleSigner: 'Unterschreibt', roleApprover: 'Genehmigt', roleCc: 'Erhält Kopie', moveUp: 'Nach oben', moveDown: 'Nach unten', remove: 'Entfernen',
  },
  en: {
    document: 'Document', subjectKind: 'Agreement type', employment: 'Employment contract', placement: 'Placement agreement', staffing: 'Staffing agreement', signers: 'Signers', create: 'Create', entries: 'records', signatureCaptured: 'Signature recorded.', request: 'Request', signer: 'Signer', status: 'Status', blocked: 'Blocked.', documentRequired: 'Please choose a document.', requestCreated: 'Signature request created.', sign: 'Sign', artifact: 'Artifact',
    title: 'E-Signature', kicker: '', listTitle: 'Requests', allKinds: 'All types', viewAll: 'All', viewOpen: 'Open', viewDone: 'Closed', composerKicker: 'NEW REQUEST', composerTitle: 'Request signature', recordKicker: 'REQUEST', signersHead: 'Signers', noSigners: 'No signers recorded.', importDone: 'Import complete.', exportDone: 'Export created.', invalidFile: 'File could not be read (JSON expected).', // v4-allow: en
    statusCreated: 'Created', statusSent: 'Sent', statusPartial: 'Partially signed', statusCompleted: 'Completed', statusDeclined: 'Declined', statusExpired: 'Expired', statusDone: 'Completed', statusFailed: 'Failed', // v4-allow: en
    showAsList: 'Show as list', showAsCards: 'Show as cards',
    signedOf: 'signed', updated: 'Updated', sent: 'Sent', expires: 'Expires',
    unknown: 'Unknown', signer_pending: 'Pending', signer_signed: 'Signed', signer_declined: 'Declined', // v4-allow: en
    documentSearch: 'Search documents', signerSearch: 'Add person', change: 'Change', name: 'Name', email: 'Email', role: 'Role',
    roleSigner: 'Needs to sign', roleApprover: 'Approves', roleCc: 'Receives a copy', moveUp: 'Move up', moveDown: 'Move down', remove: 'Remove',
  },
};
let text = COPY.de;
// Datumsformat der Karten-Metazeilen folgt der Modulsprache (kein neuer Datenpfad,
// nur Darstellung der bereits geladenen *_at_ms-Felder).
let dateLocale = 'de-DE';
// Lesbare Quellen (v4-manifest foreign_collections, nur lesen): Dokumente/Dateien und Personen, nach id.
const SIGNER_ROLES = ['signer', 'approver', 'cc'];
const ROLE_KEY = { signer: 'roleSigner', approver: 'roleApprover', cc: 'roleCc' };
const DOC_SOURCES = ['documents', 'desktop_files'];
const PERSON_SOURCES = ['customer_contacts', 'applications'];
const OPTION_LIMIT = 8;
let docIndex = new Map();
let personIndex = new Map();

function langFrom(value) {
  return String(value || '').toLowerCase().startsWith('en') ? 'en' : 'de';
}

function applyLang(lang, api) {
  // api.shell.t übersteuert die eingebetteten Texte (Schlüsselraum offen, app-api.d.ts OFFEN-B).
  const base = COPY[lang];
  text = Object.fromEntries(Object.keys(base).map((key) => {
    const scoped = MODULE_ID + '.' + key;
    const value = api?.shell?.t?.(scoped);
    return [key, typeof value === 'string' && value && value !== scoped ? value : base[key]];
  }));
  dateLocale = lang === 'en' ? 'en-GB' : 'de-DE';
}

// App-API 4.0: mount(host, api, {restored}) → {unmount, snapshot}. Daten nur über api.query/put, Befehle über
// api.command (nie warten), Zustände nur in die Zustandszone (api.status.report).
export async function mount(host, api, { restored } = {}) {
  let lang = langFrom(api.shell.locale.get());
  applyLang(lang, api);
  if (!host.dataset.app) host.dataset.app = api.app?.id || MODULE_ID;
  host.innerHTML = await loadMarkup();
  host.dataset.atsModule = MODULE_ID;
  const disposers = [];

  const root = host.querySelector('[data-esign-root]');
  applyStaticCopy(root);
  // Pane-Grammatik verdrahtet die App selbst (PORTING.md R14).
  autoWirePaneGrammar(host);
  api.shell.setTitle(text.title || TITLE);

  const listPane = root?.querySelector('[data-esign-list-pane]');
  const listEl = root?.querySelector('[data-esign-list]');
  const formEl = root?.querySelector('[data-esign-form]');
  const detailEl = root?.querySelector('[data-esign-detail]');
  const wbTitle = root?.querySelector('[data-esign-wb-title]');
  const collapseBtn = root?.querySelector('[data-esign-collapse]');
  const viewToggle = root?.querySelector('[data-esign-view-toggle]');
  const newBtn = root?.querySelector('[data-action="new"]');
  const importBtn = root?.querySelector('[data-action="import"]');
  const exportBtn = root?.querySelector('[data-action="export"]');

  const state = { records: [], visible: [], selectedId: '', userCollapsed: false, grammar: readGrammarState(listPane), recipients: [], sources: {}, docIndex: new Map(), personIndex: new Map() };
  // Die reinen Helfer (docTitle, Detail) lesen die modulweiten Indizes; je Instanz vor dem Rendern einsetzen.
  const useIndex = () => { docIndex = state.docIndex; personIndex = state.personIndex; };
  const docInput = formEl?.querySelector('[name="document_id"]');
  const docSearch = formEl?.querySelector('[data-esign-doc-search]');
  const docOptions = formEl?.querySelector('[data-esign-doc-options]');
  const docPicked = formEl?.querySelector('[data-esign-doc-picked]');
  const personSearch = formEl?.querySelector('[data-esign-person-search]');
  const personOptions = formEl?.querySelector('[data-esign-person-options]');
  const recipientsEl = formEl?.querySelector('[data-esign-recipients]');
  // Zustand in die Zustandszone (stabile code je Fall, damit clear greift).
  const report = (code, message, level = 'warn') => api.status.report({ level, code: MODULE_ID + '.' + code, text: String(message) });

  // Ergebnis-Rückmeldung als Meldung in der Zone (am App-Icon), kein Ergebniskasten im Inhalt (Textdiät Regel 1).
  function setGate(html, kind) {
    if (!html) {
      api.status.clear(MODULE_ID + '.result');
      api.status.clear(MODULE_ID + '.blocked');
      return;
    }
    const tmp = document.createElement('div');
    tmp.innerHTML = String(html).replace(/<\/(div|li|strong)>/g, ' $&');
    const message = tmp.textContent.replace(/\s+/g, ' ').trim();
    if (kind === 'ok') api.status.report({ level: 'info', code: MODULE_ID + '.result', text: message });
    else api.status.report({ level: 'error', code: MODULE_ID + '.blocked', text: message });
  }

  function selectedRecord() {
    return state.records.find((r) => recordKey(r) === state.selectedId) || null;
  }

  // ---- LEFT column: reactive record list under the shell-wired grammar ------
  function renderListRegion() {
    useIndex();
    const counts = bandCounts(state.records);
    state.visible = filterRecords(state.records, state.grammar);
    if (listEl) listEl.innerHTML = renderList(state.visible, { view: state.grammar.view, selectedId: state.selectedId }, text);
    writeCounts(listPane, counts);
    writeFooter(listPane, `${state.visible.length} ${text.entries} · ${bandLabel(state.grammar, text)}`);
  }

  // ---- MAIN column: workbench bound to the selected record ------------------
  function renderWorkbench() {
    useIndex();
    const rec = selectedRecord();
    const hasSel = Boolean(rec);
    const showDetail = computeDetailVisible(hasSel, state.userCollapsed);
    if (collapseBtn) {
      collapseBtn.hidden = !hasSel; // only show the reveal control when there is something to reveal
      collapseBtn.setAttribute('aria-pressed', String(hasSel && state.userCollapsed));
    }
    if (wbTitle) wbTitle.textContent = hasSel ? docTitle(rec, text) : text.composerTitle;
    if (detailEl) detailEl.innerHTML = showDetail ? renderDetail(rec, text) : '';
  }

  function fillForm(r) {
    useIndex();
    if (!formEl) return;
    const kind = formEl.querySelector('[name="subject_kind"]');
    if (kind) kind.value = SUBJECT_KINDS.includes(r.subject_kind) ? r.subject_kind : SUBJECT_KINDS[0];
    setDocument(r.document_id || '', r.document_title || r.document_name || r.title || '');
    state.recipients = recipientsFromSigners(r.signers);
    renderRecipients();
  }

  // ---- Dokumentauswahl (Liste mit Suche statt Freitext-ID) ------------------
  function setDocument(id, fallbackTitle = '') {
    useIndex();
    if (docInput) docInput.value = id || '';
    const doc = id ? docIndex.get(id) : null;
    if (docPicked) {
      docPicked.hidden = !id;
      docPicked.innerHTML = id
        ? `<strong class="esign-picked-title">${esc(doc?.title || fallbackTitle || t_unknownDoc())}</strong>${doc?.meta ? `<small>${esc(doc.meta)}</small>` : ''}`
          + `<button type="button" class="ctox-button ctox-button--sm" data-esign-doc-change>${esc(text.change)}</button>`
        : '';
    }
    if (docSearch) { docSearch.hidden = Boolean(id); docSearch.value = ''; }
    if (docOptions) { docOptions.hidden = true; docOptions.innerHTML = ''; }
  }
  function t_unknownDoc() { return text.unknown; }
  function renderDocOptions() {
    useIndex();
    if (!docOptions || !docSearch) return;
    const items = searchItems([...docIndex.values()], docSearch.value);
    docOptions.hidden = !items.length;
    docOptions.innerHTML = items.map((d) => `<button type="button" role="option" class="esign-option" data-esign-doc-option="${esc(d.id)}"><strong>${esc(d.title)}</strong>${d.meta ? `<small>${esc(d.meta)}</small>` : ''}</button>`).join('');
  }
  function onDocPickerClick(event) {
    const opt = event.target?.closest?.('[data-esign-doc-option]');
    if (opt) { setDocument(opt.getAttribute('data-esign-doc-option') || ''); api.status.clear(MODULE_ID + '.form.document_required'); return; }
    if (event.target?.closest?.('[data-esign-doc-change]')) { setDocument(''); docSearch?.focus(); renderDocOptions(); }
  }

  // ---- Empfänger (Personenauswahl, Zeilen mit Name/E-Mail/Rolle, Reihenfolge) ----
  function renderPersonOptions() {
    useIndex();
    if (!personOptions || !personSearch) return;
    const taken = new Set(state.recipients.map((p) => p.id));
    const items = searchItems([...personIndex.values()].filter((p) => !taken.has(p.id)), personSearch.value);
    personOptions.hidden = !items.length;
    personOptions.innerHTML = items.map((p) => `<button type="button" role="option" class="esign-option" data-esign-person-option="${esc(p.id)}"><strong>${esc(p.title)}</strong>${p.email ? `<small>${esc(p.email)}</small>` : ''}</button>`).join('');
  }
  function renderRecipients() {
    if (!recipientsEl) return;
    const n = state.recipients.length;
    recipientsEl.innerHTML = state.recipients.map((p, i) => '<li class="esign-recipient" data-esign-recipient="' + i + '">'
      + `<b class="esign-recipient-order" aria-hidden="true">${i + 1}</b>`
      + `<input class="ctox-input" type="text" data-esign-r-field="name" value="${escAttr(p.name)}" aria-label="${escAttr(text.name)}" placeholder="${escAttr(text.name)}" />`
      + `<input class="ctox-input" type="email" data-esign-r-field="email" value="${escAttr(p.email)}" aria-label="${escAttr(text.email)}" placeholder="${escAttr(text.email)}" />`
      + `<select class="ctox-select" data-esign-r-field="role" aria-label="${escAttr(text.role)}">${SIGNER_ROLES.map((r) => `<option value="${r}"${r === p.role ? ' selected' : ''}>${esc(text[ROLE_KEY[r]])}</option>`).join('')}</select>`
      + `<div class="esign-recipient-actions">`
      + `<button type="button" class="ctox-pane-icon" data-esign-move="up" aria-label="${escAttr(text.moveUp)}" title="${escAttr(text.moveUp)}"${i === 0 ? ' disabled' : ''}>${ICON_UP}</button>`
      + `<button type="button" class="ctox-pane-icon" data-esign-move="down" aria-label="${escAttr(text.moveDown)}" title="${escAttr(text.moveDown)}"${i === n - 1 ? ' disabled' : ''}>${ICON_DOWN}</button>`
      + `<button type="button" class="ctox-pane-icon" data-short-de="Entfernen" data-esign-remove aria-label="${escAttr(text.remove)}" title="${escAttr(text.remove)}">${ICON_REMOVE}</button>`
      + '</div></li>').join('');
  }
  function onSignerPickerClick(event) {
    useIndex();
    const opt = event.target?.closest?.('[data-esign-person-option]');
    if (opt) {
      const person = personIndex.get(opt.getAttribute('data-esign-person-option') || '');
      if (person) state.recipients.push({ id: person.id, name: person.title, email: person.email || '', role: 'signer' });
      if (personSearch) personSearch.value = '';
      if (personOptions) { personOptions.hidden = true; personOptions.innerHTML = ''; }
      renderRecipients();
      return;
    }
    const row = event.target?.closest?.('[data-esign-recipient]');
    if (!row) return;
    const i = Number(row.getAttribute('data-esign-recipient'));
    const move = event.target.closest('[data-esign-move]')?.getAttribute('data-esign-move');
    if (move) {
      const j = move === 'up' ? i - 1 : i + 1;
      if (j < 0 || j >= state.recipients.length) return;
      [state.recipients[i], state.recipients[j]] = [state.recipients[j], state.recipients[i]];
      renderRecipients();
    } else if (event.target.closest('[data-esign-remove]')) {
      state.recipients.splice(i, 1);
      renderRecipients();
    }
  }
  function onRecipientInput(event) {
    const field = event.target?.getAttribute?.('data-esign-r-field');
    const row = event.target?.closest?.('[data-esign-recipient]');
    if (!field || !row) return;
    const p = state.recipients[Number(row.getAttribute('data-esign-recipient'))];
    if (p) p[field] = String(event.target.value || '');
  }
  function onPickerKey(event) {
    if (event.key !== 'Enter') return;
    event.preventDefault(); // Enter wählt den ersten Treffer, schickt das Formular nicht ab
    const list = event.target === docSearch ? docOptions : personOptions;
    list?.querySelector('[role="option"]')?.click();
  }
  docSearch?.addEventListener('input', renderDocOptions);
  docSearch?.addEventListener('focus', renderDocOptions);
  personSearch?.addEventListener('input', renderPersonOptions);
  personSearch?.addEventListener('focus', renderPersonOptions);
  docSearch?.addEventListener('keydown', onPickerKey);
  personSearch?.addEventListener('keydown', onPickerKey);
  formEl?.querySelector('[data-esign-doc-picker]')?.addEventListener('click', onDocPickerClick);
  formEl?.querySelector('[data-esign-signer-picker]')?.addEventListener('click', onSignerPickerClick);
  recipientsEl?.addEventListener('input', onRecipientInput);
  recipientsEl?.addEventListener('change', onRecipientInput);
  function resetComposer() {
    try { formEl?.reset(); } catch {}
    state.recipients = [];
    setDocument('');
    renderRecipients();
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
    listEl?.querySelectorAll('[data-esign-row]').forEach((row) => {
      const on = (row.getAttribute('data-esign-row') || '') === String(state.selectedId || '');
      row.classList.toggle('is-selected', on);
      row.setAttribute('aria-selected', String(on));
    });
  }

  function startNew() {
    state.selectedId = '';
    state.userCollapsed = false;
    resetComposer();
    setGate('');
    renderListRegion();
    renderWorkbench();
    try { docSearch?.focus(); } catch {}
  }

  function onCollapseToggle() {
    if (!state.selectedId) return;
    state.userCollapsed = !state.userCollapsed;
    renderWorkbench();
  }

  // Live-Abo (App-API 4.0): subscribe ruft sofort mit dem lokalen Stand und bei jeder Änderung.
  function onRows(rows) {
    state.records = [...(Array.isArray(rows) ? rows : [])].sort((a, b) => (b.updated_at_ms || 0) - (a.updated_at_ms || 0));
    renderListRegion();
    renderWorkbench();
  }

  // Befehl über api.command: legt das Befehlsdokument an und liefert sofort (nie warten). Das `done`-Ergebnis ist
  // die fachliche Antwort (Prüfbereich), `failed`/lokal ungültig → Zustandszone.
  function runCommand(type, payload, onResult) {
    const code = MODULE_ID + '.command.' + type;
    const handle = api.command(type, { ...payload, client_context: { module_id: MODULE_ID } });
    let off = null;
    let finished = false;
    const stop = () => { finished = true; off?.(); off = null; };
    off = handle.status.subscribe((value) => {
      if (value?.status === 'failed') {
        api.status.report({ level: 'error', code, text: type + ': ' + (value.error || text.statusFailed), ref: handle.id });
        stop();
      } else if (value?.status === 'done') {
        api.status.clear(code);
        stop();
        onResult(value.result);
      }
    });
    if (finished) off?.();
    handle.local.catch((error) => {
      api.status.report({ level: 'error', code, text: type + ': ' + (error?.code || 'local') + ' ' + (error?.message || error), ref: handle.id });
    });
    disposers.push(stop);
    return handle;
  }

  // ---- Selection (left list) ------------------------------------------------
  function onListClick(event) {
    const row = event.target?.closest?.('[data-esign-row]');
    if (!row || !listEl?.contains(row)) return;
    selectRecord(row.getAttribute('data-esign-row') || '');
  }
  listEl?.addEventListener('click', onListClick);

  // ---- Ein-Knopf-Ansichtsumschalter ----------------------------------------
  // `data-pg-view` traegt die AKTUELLE Ansicht; Beschriftung und Icon zeigen
  // die Ansicht, in die der Klick wechselt. `aria-pressed` gehoert nicht dazu:
  // die Shell-Grammatik setzt es beim Klick auf jeden Umschalter, hier wird es
  // wieder entfernt, damit der Knopf eine Aktion bleibt und kein Zustand.
  function syncViewToggle() {
    if (!viewToggle) return;
    viewToggle.removeAttribute('aria-pressed');
    const label = viewToggle.dataset.pgView === 'list' ? text.showAsCards : text.showAsList;
    viewToggle.setAttribute('aria-label', label);
    viewToggle.setAttribute('title', label);
  }
  function onViewToggle() {
    if (!viewToggle) return;
    viewToggle.dataset.pgView = viewToggle.dataset.pgView === 'list' ? 'cards' : 'list';
    syncViewToggle();
    // Der Modulzustand wird immer aus dem DOM gelesen, damit der Umschalter
    // unabhaengig von der Reihenfolge der Shell-Verdrahtung stimmt.
    state.grammar = readGrammarState(listPane);
    renderListRegion();
  }
  viewToggle?.addEventListener('click', onViewToggle);
  syncViewToggle();

  // Re-render the list on any shell-wired grammar change (search/view/tray/band).
  function onGrammarChange(event) {
    if (!listPane || !listPane.contains(event.target)) return;
    // DOM-abgeleitet statt event.detail: die Shell meldet fuer den einzelnen
    // Umschalter denselben Wert, aber nur das DOM ist in jedem Fall aktuell.
    state.grammar = readGrammarState(listPane);
    syncViewToggle();
    renderListRegion();
  }
  root?.addEventListener('ctox-pane-grammar-change', onGrammarChange);

  // ---- Sign flow (unchanged command type + payload) -------------------------
  function onDetailClick(event) {
    const btn = event.target?.closest?.('[data-sign]');
    if (!btn) return;
    const requestId = btn.getAttribute('data-sign');
    const signerId = btn.getAttribute('data-signer');
    if (!requestId || !signerId) return;
    setGate('');
    runCommand(SIGN_COMMAND, { request_id: requestId, signer_id: signerId, record_id: requestId }, (result) => {
      if (renderBlocked(result, setGate)) return;
      const reqId = result?.request_id ?? requestId;
      const status = result?.status ?? '';
      setGate(
        `<strong>${esc(text.signatureCaptured)}</strong>`
        + `<div class="ats-result-row">${esc(text.request)}: ` + esc(reqId) + '</div>'
        + `<div class="ats-result-row">${esc(text.signer)}: ` + esc(signerId) + '</div>'
        + (status ? `<div class="ats-result-row">${esc(text.status)}: ` + esc(text[STATUS_KEY[status]] || status) + '</div>' : ''),
        'ok',
      );
    });
  }
  detailEl?.addEventListener('click', onDetailClick);

  // ---- Create flow (unchanged command type + payload) -----------------------
  function onSubmit(event) {
    event.preventDefault();
    setGate('');
    const data = new FormData(formEl);
    const f = Object.fromEntries(data.entries());
    const document_id = String(f.document_id || '').trim();
    if (!document_id) { report('form.document_required', text.documentRequired); return; }
    api.status.clear(MODULE_ID + '.form.document_required');
    const subject_kind = SUBJECT_KINDS.includes(f.subject_kind) ? f.subject_kind : SUBJECT_KINDS[0];
    // Vertrag unverändert: dieselben IDs (document_id, signers[].id, state); Name/E-Mail/Rolle/Reihenfolge additiv.
    const signers = signersFromRecipients(state.recipients);
    const payload = { document_id, subject_kind, signers };
    resetComposer();
    runCommand(CREATE_COMMAND, { ...payload, record_id: document_id }, (result) => {
      if (renderBlocked(result, setGate)) return;
      const reqId = result?.request_id ?? result?.data?.request_id ?? null;
      const status = result?.status ?? result?.data?.status ?? null;
      setGate(
        `<strong>${esc(text.requestCreated)}</strong>`
        + `<div class="ats-result-row">${esc(text.request)}: ` + esc(reqId ?? '—') + '</div>'
        + `<div class="ats-result-row">${esc(text.status)}: ` + esc(status ? (text[STATUS_KEY[status]] || status) : '—') + '</div>',
        'ok',
      );
    });
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
      anchor.style.display = 'none';
      (host || document.documentElement).appendChild(anchor);
      anchor.click();
      anchor.remove();
      report('export.done', text.exportDone, 'info');
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
      let parsed;
      try {
        parsed = JSON.parse(await file.text());
      } catch (e) {
        console.error('[esign] import parse failed:', e);
        report('import.invalid', text.invalidFile, 'error');
        return;
      }
      const items = Array.isArray(parsed) ? parsed : (Array.isArray(parsed?.records) ? parsed.records : [parsed]);
      let ok = 0;
      for (const item of items) {
        const record = normalizeSignatureRequest(item, { nowMs: Date.now() });
        if (!record.document_id) continue;
        try { await api.put(PRIMARY, record).local; ok += 1; } catch (e) { console.error('[esign] import put failed:', e); }
      }
      report('import.done', `${text.importDone} ${ok} ${text.entries}`, 'info');
    }, { once: true });
    input.click();
  }
  importBtn?.addEventListener('click', onImport);

  // ---- Reactive backbone ----------------------------------------------------
  disposers.push(api.query(PRIMARY, {}).subscribe(onRows));
  // Fremd-Collections nur lesen; fehlt die Leseberechtigung, bleibt die Auswahl leer (kein Abbruch der App).
  function rebuildSources() {
    state.docIndex = buildDocIndex(state.sources);
    state.personIndex = buildPersonIndex(state.sources);
    useIndex();
    if (docInput?.value) setDocument(docInput.value);
    renderListRegion();
    renderWorkbench();
  }
  for (const c of [...DOC_SOURCES, ...PERSON_SOURCES]) {
    try {
      disposers.push(api.query(c, {}).subscribe((rows) => { state.sources[c] = Array.isArray(rows) ? rows : []; rebuildSources(); }));
    } catch (error) { console.warn('[esign] Quelle nicht lesbar:', c, error?.code || error); }
  }
  disposers.push(api.shell.locale.subscribe((value) => {
    const next = langFrom(value);
    if (next === lang) return;
    lang = next;
    applyLang(lang, api);
    applyStaticCopy(root);
    syncViewToggle();
    renderRecipients();
    setDocument(docInput?.value || '');
    renderListRegion();
    renderWorkbench();
  }));
  applyRestored(restored);

  // Snapshot für den Live-Remount: Auswahl, Ansicht, offene Formulareingaben.
  function snapshot() {
    const f = formEl ? Object.fromEntries(new FormData(formEl).entries()) : {};
    return {
      v: 1,
      selectedId: state.selectedId,
      userCollapsed: state.userCollapsed,
      view: viewToggle?.dataset.pgView === 'list' ? 'list' : 'cards',
      search: listPane?.querySelector('[data-pg-search]')?.value || '',
      form: { document_id: String(f.document_id || ''), subject_kind: String(f.subject_kind || ''), signers: state.recipients.map((p) => ({ ...p })) },
    };
  }
  function applyRestored(snap) {
    if (!snap || typeof snap !== 'object') return;
    if (typeof snap.selectedId === 'string') state.selectedId = snap.selectedId;
    if (typeof snap.userCollapsed === 'boolean') state.userCollapsed = snap.userCollapsed;
    if (snap.view === 'list' && viewToggle && viewToggle.dataset.pgView !== 'list') onViewToggle();
    const search = listPane?.querySelector('[data-pg-search]');
    if (search && typeof snap.search === 'string') search.value = snap.search;
    if (formEl && snap.form && typeof snap.form === 'object') {
      const kind = formEl.querySelector('[name="subject_kind"]');
      if (kind && typeof snap.form.subject_kind === 'string' && snap.form.subject_kind) kind.value = snap.form.subject_kind;
      if (typeof snap.form.document_id === 'string') setDocument(snap.form.document_id);
      if (Array.isArray(snap.form.signers)) { state.recipients = recipientsFromSigners(snap.form.signers); renderRecipients(); }
    }
    state.grammar = readGrammarState(listPane);
    renderListRegion();
    renderWorkbench();
  }

  return {
    unmount() {
      for (const dispose of disposers.splice(0)) {
        try { dispose(); } catch (error) { console.warn('[esign] unsubscribe failed', error); }
      }
      formEl?.removeEventListener('submit', onSubmit);
      if (docIndex === state.docIndex) docIndex = new Map();
      if (personIndex === state.personIndex) personIndex = new Map();
      listEl?.removeEventListener('click', onListClick);
      detailEl?.removeEventListener('click', onDetailClick);
      root?.removeEventListener('ctox-pane-grammar-change', onGrammarChange);
      host.replaceChildren();
      delete host.dataset.atsModule;
    },
    snapshot,
  };
}

// ---------------------------------------------------------------------------
// Pure helpers (exported for tests) — no DOM, no RxDB.
// ---------------------------------------------------------------------------

const ICON_UP = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 19V5M5 12l7-7 7 7"/></svg>';
const ICON_DOWN = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 5v14M5 12l7 7 7-7"/></svg>';
const ICON_REMOVE = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"/></svg>';

const live = (r) => r && !r._deleted && !r.is_deleted && !r.deleted_at_ms;

// Dokumente (documents) und Dateien (desktop_files, ohne Ordner) als Auswahlliste: {id, title, meta, hay}.
export function buildDocIndex(sources = {}) {
  const out = new Map();
  for (const d of (sources.documents || []).filter(live)) {
    const title = String(d.title || d.filename || '').trim();
    if (!d.id || !title) continue;
    out.set(d.id, { id: d.id, title, meta: d.filename && d.filename !== title ? d.filename : '', at: Number(d.updated_at_ms) || 0 });
  }
  for (const f of (sources.desktop_files || []).filter(live)) {
    if (!f.id || out.has(f.id) || /^(folder|directory|dir)$/i.test(String(f.kind || ''))) continue;
    const title = String(f.name || '').trim();
    if (!title) continue;
    out.set(f.id, { id: f.id, title, meta: '', at: Number(f.updated_at_ms) || 0 });
  }
  return out;
}

// Personen aus Kontakten (customer_contacts) und Bewerbungen (applications.candidate): {id, title, email}.
export function buildPersonIndex(sources = {}) {
  const out = new Map();
  for (const c of (sources.customer_contacts || []).filter(live)) {
    const title = [c.first_name, c.last_name].filter(Boolean).join(' ').trim() || String(c.name || '').trim();
    if (!c.id || !title) continue;
    out.set(c.id, { id: c.id, title, email: String(c.email || ''), meta: c.job_title || '' });
  }
  for (const a of (sources.applications || []).filter(live)) {
    const cand = a.candidate && typeof a.candidate === 'object' ? a.candidate : {};
    const id = String(cand.id || a.id || '');
    const title = String(cand.name || a.name || '').trim();
    if (!id || !title || out.has(id)) continue;
    out.set(id, { id, title, email: String(cand.email || a.email || '') });
  }
  return out;
}

// Suche über Titel/E-Mail/Zusatz; leere Suche → die neuesten Einträge. Höchstens OPTION_LIMIT Treffer.
export function searchItems(items, query) {
  const q = String(query || '').trim().toLowerCase();
  const hits = q ? items.filter((it) => `${it.title} ${it.email || ''} ${it.meta || ''}`.toLowerCase().includes(q)) : [...items].sort((a, b) => (b.at || 0) - (a.at || 0));
  return hits.slice(0, OPTION_LIMIT);
}

// Empfängerzeilen ↔ Befehls-signers. Die IDs bleiben, was der Handler kennt; Name/E-Mail/Rolle/order additiv.
export function signersFromRecipients(recipients) {
  return (recipients || []).filter((p) => p && p.id).map((p, i) => {
    const s = { id: p.id, state: 'pending', order: i + 1, role: SIGNER_ROLES.includes(p.role) ? p.role : 'signer' };
    if (p.name) s.name = String(p.name).trim();
    if (p.email) s.email = String(p.email).trim();
    return s;
  });
}

export function recipientsFromSigners(signers) {
  return (Array.isArray(signers) ? signers : [])
    .filter((s) => s && s.id)
    .map((s) => ({ ...s, _o: Number(s.order) || 0 }))
    .sort((a, b) => a._o - b._o)
    .map((s) => {
      const person = personIndex.get(s.id);
      return { id: s.id, name: String(s.name || person?.title || ''), email: String(s.email || person?.email || ''), role: SIGNER_ROLES.includes(s.role) ? s.role : 'signer' };
    });
}

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
      const hay = `${docTitle(r, text)} ${r.document_id || ''} ${r.id || ''} ${r.subject_kind || ''} ${r.status || ''}`.toLowerCase();
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

// App-API 4.0 (R11): eine leere Liste rendert nichts – kein Sync-, Lade- oder Leertext.
export function renderList(records, opts = {}, t = text) {
  if (!records || !records.length) return '';
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
  if (status === 'completed' || status === 'done') return 'is-success';
  if (status === 'declined' || status === 'expired' || status === 'failed') return 'is-danger';
  if (status === 'sent' || status === 'partially_signed') return 'is-warning';
  return '';
}

// Kurzdatum aus einem bereits geladenen *_at_ms-Feld. Kein neuer Datenpfad.
function shortDate(ms) {
  const n = Number(ms);
  if (!Number.isFinite(n) || n <= 0) return '';
  try {
    return new Date(n).toLocaleDateString(dateLocale, { day: '2-digit', month: '2-digit', year: 'numeric' });
  } catch { return ''; }
}

// KARTE: Titel fett, darunter zwei Metazeilen mit den tragenden Detailfeldern
// (Vertragstyp, Unterzeichner-Fortschritt, Datum, Artefakt). Bewusst
// grosszuegiger als die Listenzeile — das ist der Unterschied der beiden
// Ansichten (Betreiber-Direktive 31.08.).
function shardCard(r, selectedId, t) {
  const key = recordKey(r);
  const status = String(r.status || 'created');
  const progress = signerProgress(r);
  const badge = ('ctox-badge ' + statusBadgeClass(status)).trim();
  const metaPrimary = [
    r.subject_kind && docTitle(r, t) !== subjectLabel(r.subject_kind, t) ? subjectLabel(r.subject_kind, t) : null,
    progress.total ? `${progress.done}/${progress.total} ${t.signedOf}` : null,
  ].filter(Boolean).map(esc).join(' · ');
  const stamp = shortDate(r.updated_at_ms) || shortDate(r.created_at_ms);
  const metaSecondary = [
    stamp ? `${t.updated} ${stamp}` : null,
    r.expires_at_ms ? `${t.expires} ${shortDate(r.expires_at_ms)}` : null,
  ].filter(Boolean).map(esc).join(' · ');
  return rowShell(r, key, selectedId, 'cards',
    '<div class="esign-shard">'
    + '<div class="esign-shard-title">'
    + `<strong>${esc(docTitle(r, t))}</strong>`
    + `<span class="${badge}" data-status="${esc(status)}">${esc(statusLabel(status, t))}</span>`
    + '</div>'
    + (metaPrimary ? `<small class="esign-shard-meta">${metaPrimary}</small>` : '')
    + (metaSecondary ? `<small class="esign-shard-meta esign-shard-meta--secondary">${metaSecondary}</small>` : '')
    + '</div>');
}

// LISTE: genau EINE dichte Zeile — Titel plus ein einziges Kurz-Meta rechts.
function shardCompact(r, selectedId, t) {
  const key = recordKey(r);
  const status = String(r.status || 'created');
  const badge = ('ctox-badge ' + statusBadgeClass(status)).trim();
  return rowShell(r, key, selectedId, 'list',
    '<div class="esign-row-compact">'
    + `<span class="esign-compact-title">${esc(docTitle(r, t))}</span>`
    + `<span class="${badge}" data-status="${esc(status)}">${esc(statusLabel(status, t))}</span>`
    + '</div>');
}

// Namen statt IDs (Textdiät Regel 4): Dokumenttitel aus dem Datensatz, sonst der Vertragstyp – nie die Dokument-ID.
function docTitle(r, t) {
  const v = r?.document_title || r?.document_name || r?.title || docIndex.get(r?.document_id)?.title;
  if (v) return String(v);
  return r?.subject_kind ? subjectLabel(r.subject_kind, t) : t.unknown;
}

function rowShell(r, key, selectedId, view, inner) {
  const label = docTitle(r, COPY.de);
  const selected = key && key === selectedId;
  const variant = view === 'list' ? ' esign-row--list' : ' esign-row--card';
  return '<button type="button" class="ctox-list-item esign-row' + variant + (selected ? ' is-selected' : '') + '"'
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
    detailField(t.document, docTitle(r, t)),
    detailField(t.subjectKind, subjectLabel(r.subject_kind, t)),
    detailField(t.status, statusLabel(status, t)),
  ];
  const signerRows = signers.map((s) => {
    const st = String((s && s.state) || 'pending');
    const canSign = !terminal && s && s.id && st !== 'signed' && st !== 'declined';
    const action = canSign
      ? `<button type="button" class="ctox-button ctox-button--sm" data-sign="${esc(r.id || '')}" data-signer="${esc(s.id)}">${esc(t.sign)}</button>`
      : '';
    return '<li class="esign-signer">'
      + `<span class="esign-signer-id">${esc((s && (s.name || s.display_name || personIndex.get(s.id)?.title)) || t.unknown)}</span>`
      + `<span class="esign-signer-state">${esc(t['signer_' + st] || st)}</span>`
      + action
      + '</li>';
  }).join('');
  return '<section class="esign-detail-card"'
    + ` data-context-record-id="${esc(r.id || '')}" data-context-record-type="signature_request"`
    + ` data-context-label="${esc(docTitle(r, t))}">`
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
  // Ein Umschalter, ein Wert: `data-pg-view` ist die aktuelle Ansicht. Das ist
  // genau der Wert, den auch shared/pane-grammar.js liest (Fallback auf den
  // ersten [data-pg-view]-Knopf, wenn kein aria-pressed gesetzt ist).
  const view = pane.querySelector('[data-pg-view]')?.dataset.pgView === 'list' ? 'list' : 'cards';
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
  root?.querySelectorAll?.('[data-copy-placeholder]').forEach((node) => {
    const v = text[node.dataset.copyPlaceholder];
    if (v) { node.setAttribute('placeholder', v); node.setAttribute('aria-label', v); }
  });
}

async function loadMarkup() {
  // index.html liefert der Service Worker versioniert (§16.15) – kein Cache-Buster; CSS lädt die Shell.
  const html = await fetch(new URL('./index.html', import.meta.url)).then((r) => r.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((n) => n.remove());
  return doc.body.innerHTML;
}

function escAttr(v) { return esc(v).replace(/"/g, '&quot;'); }

function esc(v) { return String(v == null ? '' : v).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;'); }
