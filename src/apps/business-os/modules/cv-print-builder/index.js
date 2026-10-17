// CV Print Builder – App-API 4.0 (Shell v4). Daten live über api.query/get, Schreiben über api.put/patch,
// Parsing als Befehl `ctox.cv_print.apply_parse` (Handler fertig, nie warten), PDF-Quellen über api.files.
// Sync-Kopplung, Readiness-Warten, Chunk-Schreiben nach desktop_files/desktop_file_chunks und der
// business_os.chat.task-Weg entfallen (V4-PORT.md).
import { autoWirePaneGrammar } from 'ctox:shared/pane-grammar.js';
import { readStoredFileFromChunks } from 'ctox:shared/file-integrity.js';

const MODULE_ID = 'cv-print-builder';
const PARSE_COMMAND = 'ctox.cv_print.apply_parse';
const PROFILE_MIME = 'application/vnd.ctox.cv-print-profile+json';
// Nur noch zum Lesen älterer Profile, deren PDF als desktop_files/desktop_file_chunks gespeichert ist.
const CONTENT_HASH_SCHEME = 'sha256-bytes-v1';
// Live-Abos: jede Änderung zeichnet neu (entprellt 80 ms).
const LIVE_COLLECTIONS = [
  'documents',
  'document_versions',
  'business_commands',
  'ctox_queue_tasks',
];
let ACTIVE_LOCALE = 'de';

function tr(de, en) {
  return ACTIVE_LOCALE === 'en' ? en : de;
}

const TEMPLATES = [
  { id: 'minimal', label: 'Minimal', description: 'Zeitloses Qualifikationsprofil ohne Schmuck.' },
  { id: 'classic', label: 'Klassisch', description: 'Formales Qualifikationsprofil mit ruhiger Typografie.' },
  { id: 'modern', label: 'Modern', description: 'Qualifikationsprofil mit Logo, Akzentlinie und kompakten Fakten.' },
];

const VIEW_LABELS = {
  original: 'Original',
  split: 'Beide',
  print: 'Druckansicht',
};

const COMMON_FIRST_NAMES = new Set([
  'anna',
  'julia',
  'sascha',
  'matthias',
  'simon',
  'mohamed',
  'mohammed',
  'muhammad',
  'ulrich',
  'michael',
  'christian',
  'thomas',
  'stefan',
  'stephan',
  'andreas',
  'martin',
  'sebastian',
  'daniel',
  'jan',
  'jens',
  'tobias',
  'patrick',
  'florian',
  'marco',
  'marcel',
  'niklas',
  'lena',
  'sarah',
  'sara',
  'sandra',
  'katharina',
  'christina',
  'nina',
  'laura',
  'melanie',
  'nadine',
  'anne',
  'maria',
]);

export async function mount(root, api, { restored } = {}) {
  ACTIVE_LOCALE = localeOf(api.shell.locale.get());
  if (!root.dataset.app) root.dataset.app = MODULE_ID;
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  root.innerHTML = html;
  applyStaticLocale(root);
  api.shell.setTitle('CV Print Builder');
  autoWirePaneGrammar(root);

  const snap = restored && typeof restored === 'object' ? restored : {};
  const state = {
    api,
    root,
    host: root.querySelector('[data-cv-print-builder]'),
    items: [],
    selectedId: typeof snap.selectedId === 'string' ? snap.selectedId : '',
    lastSelectedId: '',
    lastSelectedPhase: '',
    viewMode: ['original', 'split', 'print'].includes(snap.viewMode) ? snap.viewMode : 'original',
    userCollapsed: snap.userCollapsed === true,
    originalUrls: new Map(),
    disposers: [],
    refreshTimer: null,
    renderSerial: 0,
    importing: false,
    bulkParsing: false,
    originalErrors: new Map(),
    statusCodes: new Set(),
    mounted: true,
    // Offene (noch nicht per blur gespeicherte) Eingaben im Korrektur-Editor: überleben Live-Neuzeichnen und
    // Remount (snapshot/restored) und werden zurückgespielt, sobald das Profil gezeichnet ist.
    editorDraft: normalizeEditorDraft(snap.editorDraft),
  };
  // Restored view: lastSelected* so setzen, dass refresh die wiederhergestellte Ansicht nicht überschreibt.
  if (state.selectedId && typeof snap.selectedPhase === 'string') {
    state.lastSelectedId = state.selectedId;
    state.lastSelectedPhase = snap.selectedPhase;
  }
  restoreGrammar(state, snap.grammar);

  bindStaticEvents(state);
  subscribeToCollections(state);
  state.disposers.push(api.shell.locale.subscribe((value) => {
    const next = localeOf(value);
    if (next === ACTIVE_LOCALE) return;
    ACTIVE_LOCALE = next;
    applyStaticLocale(root);
    setViewToggle(state, readGrammar(state).view);
    render(state);
  }));
  refresh(state);

  return {
    unmount() {
      state.mounted = false;
      state.disposers.forEach((dispose) => { try { dispose?.(); } catch {} });
      state.disposers.length = 0;
      state.originalUrls.forEach((url) => URL.revokeObjectURL(url));
      state.originalUrls.clear();
      clearTimeout(state.refreshTimer);
      root.replaceChildren();
    },
    snapshot() {
      const selected = getSelectedItem(state);
      const snapOut = {
        v: 1,
        selectedId: state.selectedId,
        selectedPhase: selected ? workflowPhase(selected.model) : '',
        viewMode: state.viewMode,
        userCollapsed: state.userCollapsed,
        grammar: readGrammarSnapshot(state),
      };
      if (state.editorDraft.documentId && Object.keys(state.editorDraft.fields).length) {
        snapOut.editorDraft = structuredClone(state.editorDraft);
      }
      return snapOut;
    },
  };
}

function localeOf(value) {
  return String(value || 'de').toLowerCase().startsWith('en') ? 'en' : 'de';
}

// Listenzustand (Suche, Band, Filter, Kachel/Liste) für snapshot/restored.
function readGrammarSnapshot(state) {
  const rail = state.host?.querySelector('[data-cv-rail]');
  return {
    search: rail?.querySelector('[data-pg-search]')?.value || '',
    band: rail?.querySelector('[data-pg-band][aria-selected="true"]')?.dataset.pgBand || 'all',
    sort: rail?.querySelector('[data-pg-filter][data-pg-name="sort"]')?.value || 'updated_desc',
    template: rail?.querySelector('[data-pg-filter][data-pg-name="template"]')?.value || 'all',
    view: rail?.querySelector('[data-cv-view-toggle]')?.dataset.cvListView === 'list' ? 'list' : 'cards',
  };
}

function restoreGrammar(state, grammar) {
  if (!grammar || typeof grammar !== 'object') return;
  const rail = state.host?.querySelector('[data-cv-rail]');
  if (!rail) return;
  const search = rail.querySelector('[data-pg-search]');
  if (search && typeof grammar.search === 'string') search.value = grammar.search;
  for (const name of ['sort', 'template']) {
    const select = rail.querySelector(`[data-pg-filter][data-pg-name="${name}"]`);
    if (select && typeof grammar[name] === 'string' && [...select.options].some((o) => o.value === grammar[name])) select.value = grammar[name];
  }
  if (typeof grammar.band === 'string') {
    const tabs = [...rail.querySelectorAll('[data-pg-band]')];
    if (tabs.some((tab) => tab.dataset.pgBand === grammar.band)) {
      tabs.forEach((tab) => {
        const on = tab.dataset.pgBand === grammar.band;
        tab.setAttribute('aria-selected', String(on));
        tab.classList.toggle('is-active', on);
      });
    }
  }
  const toggle = rail.querySelector('[data-cv-view-toggle]');
  if (toggle && (grammar.view === 'list' || grammar.view === 'cards')) toggle.dataset.cvListView = grammar.view;
  rail.__ctoxPaneGrammar?.refreshDot?.();
}

// Offene Editor-Eingaben je Feld (Schlüssel = Feldpfad / Eintrag / Skill-Gruppe).
function normalizeEditorDraft(value) {
  const draft = { documentId: '', fields: {} };
  if (!value || typeof value !== 'object' || typeof value.documentId !== 'string') return draft;
  draft.documentId = value.documentId;
  for (const [key, text] of Object.entries(value.fields || {})) {
    if (typeof text === 'string') draft.fields[key] = text;
  }
  return draft;
}

// Schlüssel eines Editorfelds: Feldpfad, Eintrag (Art:Index:Feld) oder Skill-Gruppe.
function editorKey(el) {
  if (el?.dataset?.cvFieldPath) return `f:${el.dataset.cvFieldPath}`;
  if (el?.dataset?.cvSkillGroup) return `s:${el.dataset.cvSkillGroup}`;
  const row = el?.closest?.('[data-cv-entry-kind]');
  if (row && el.dataset.cvEntryField) return `e:${row.dataset.cvEntryKind}:${row.dataset.cvIndex}:${el.dataset.cvEntryField}`;
  return '';
}

function applyEditorDraft(state) {
  const draft = state.editorDraft;
  const editor = state.host?.querySelector('[data-cv-field-editor]');
  if (!editor || !draft.documentId || draft.documentId !== state.selectedId) return;
  editor.querySelectorAll('[data-cv-field-path], [data-cv-entry-field], [data-cv-skill-group]').forEach((input) => {
    const key = editorKey(input);
    if (key && typeof draft.fields[key] === 'string') input.value = draft.fields[key];
  });
}

// Statisches Markup übersetzen; beidseitig, weil die Sprache live wechseln kann (api.shell.locale).
function applyStaticLocale(host) {
  const root = host.querySelector('[data-cv-print-builder]');
  if (!root) return;
  root.setAttribute('lang', ACTIVE_LOCALE);
  const text = new Map([
    ['Kandidaten', 'Candidates'], ['Neueste', 'Newest'], ['Name', 'Name'], ['Status', 'Status'], ['Template', 'Template'],
    // v4-allow: label (Band „In Arbeit“ = Werkstatt-Phase, kein App-Zustand)
    ['Alle', 'All'], ['In Arbeit', 'In progress'], ['Korrektur', 'Review'], ['Freigegeben', 'Approved'],
    ['Alle Templates', 'All templates'], ['Minimal', 'Minimal'], ['Klassisch', 'Classic'], ['Modern', 'Modern'],
    ['CV Stage', 'CV Stage'],
  ]);
  const attrs = new Map([
    ['Kandidaten', 'Candidates'], ['Kandidaten suchen', 'Search candidates'], ['Suchen...', 'Search...'],
    ['Kandidatenspalte anpassen', 'Resize candidate column'], ['CV Arbeitsbereich', 'CV workspace'],
    ['Neuer CV', 'New CV'], ['Profile importieren', 'Import profiles'], ['Profile importieren (JSON)', 'Import profiles (JSON)'],
    ['Profile exportieren', 'Export profiles'], ['Profile exportieren (JSON)', 'Export profiles (JSON)'],
    ['Alle reparsen', 'Reparse all'], ['Alle CV-PDFs erneut parsen', 'Reparse all CV PDFs'],
    ['Filter', 'Filter'], ['Darstellung', 'View'], ['Kachelansicht', 'Card view'], ['Listenansicht', 'List view'],
    ['Filter zurücksetzen', 'Reset filters'], ['Sortieren', 'Sort'], ['Template filtern', 'Filter by template'],
    ['Status-Ansichten', 'Status views'],
  ]);
  const invert = (map) => new Map([...map].map(([de, en]) => [en, de]));
  const textMap = ACTIVE_LOCALE === 'en' ? text : invert(text);
  const attrMap = ACTIVE_LOCALE === 'en' ? attrs : invert(attrs);
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let node;
  while ((node = walker.nextNode())) {
    const raw = node.nodeValue || '';
    const value = raw.trim();
    if (textMap.has(value)) node.nodeValue = raw.replace(value, textMap.get(value));
  }
  root.querySelectorAll('[aria-label],[placeholder],[title]').forEach((element) => {
    for (const attr of ['aria-label', 'placeholder', 'title']) {
      const value = element.getAttribute(attr);
      if (attrMap.has(value)) element.setAttribute(attr, attrMap.get(value));
    }
  });
}

function bindStaticEvents(state) {
  const rail = state.host.querySelector('[data-cv-rail]');
  // The left column's chrome (search / view-toggle / tray / reset / active-dot
  // / band) is SHELL-wired from the data-pg-* markup; the module only listens
  // for the bubbling change event and re-renders the list from data.
  const onGrammar = () => render(state);
  rail?.addEventListener('ctox-pane-grammar-change', onGrammar);
  // The single view toggle is module-owned (the shell grammar only wires
  // two-button [data-pg-view] groups); sync its icon/labels to the locale.
  setViewToggle(state, readGrammar(state).view);
  // One delegated click handler for header actions, the selector list, and the
  // stage controls. Selecting a candidate is an in-place class flip, never a
  // list rebuild, so the operator's scroll offset is preserved.
  const onClick = (event) => onHostClick(state, event);
  const onChange = (event) => onHostChange(state, event);
  state.host.addEventListener('click', onClick);
  state.host.addEventListener('change', onChange);
  state.disposers.push(() => {
    rail?.removeEventListener('ctox-pane-grammar-change', onGrammar);
    state.host.removeEventListener('click', onClick);
    state.host.removeEventListener('change', onChange);
  });
}

function onHostClick(state, event) {
  const target = event.target;
  const action = target.closest?.('[data-action]');
  if (action && state.host.contains(action)) {
    if (action.dataset.action === 'new') return triggerNewCandidate(state);
    if (action.dataset.action === 'import') return triggerImportCandidates(state);
    if (action.dataset.action === 'export') return exportCandidates(state);
  }
  if (target.closest?.('[data-cv-reparse-all]')) return triggerReparseAll(state);

  const taskLink = target.closest?.('[data-cv-open-task]');
  if (taskLink) {
    event.preventDefault();
    event.stopPropagation();
    return openCtoxTask(state, taskLink.dataset.cvTaskId || '', taskLink.dataset.cvCommandId || '');
  }
  if (target.closest?.('[data-toggle-stage]')) return toggleStage(state);
  // Must precede the stage-view tabs: the list toggle is a sibling control in
  // the same delegated handler and carries its own attribute name.
  const listViewToggle = target.closest?.('[data-cv-view-toggle]');
  if (listViewToggle && state.host.contains(listViewToggle)) return toggleListView(state, listViewToggle);
  if (target.closest?.('[data-cv-action]')) return runWorkflowAction(state);
  const viewButton = target.closest?.('[data-cv-view]');
  if (viewButton) return changeStageView(state, viewButton.dataset.cvView);
  if (target.closest?.('[data-cv-toggle-anon]')) return toggleAnonymize(state);
  if (target.closest?.('[data-cv-logo-control]')) return toggleLogo(state);

  const row = target.closest?.('[data-cv-select]');
  if (row && state.host.contains(row)) return selectCandidate(state, row.dataset.cvSelect);
}

function onHostChange(state, event) {
  const target = event.target;
  if (target.matches?.('[data-cv-upload]')) return handleUploadChange(state, target);
  if (target.matches?.('[data-cv-import]')) return handleImportChange(state, target);
  if (target.matches?.('[data-cv-logo-upload]')) return handleLogoChange(state, target);
  if (target.matches?.('[data-cv-template]')) return changeTemplate(state, target.value);
}

function triggerNewCandidate(state) {
  state.host.querySelector('[data-cv-upload]')?.click();
}

function triggerImportCandidates(state) {
  state.host.querySelector('[data-cv-import]')?.click();
}

async function triggerReparseAll(state) {
  const candidates = reparseCandidates(state);
  if (!candidates.length) {
    notify(state, 'info', 'reparse', tr('Keine PDFs zum Parsen', 'No PDFs to parse'), tr('Es wurden keine CVs mit lokaler PDF-Quelle gefunden.', 'No CVs with a local PDF source were found.'));
    return;
  }
  clearNotice(state, 'reparse');
  // Bestätigung im App-Fenster (api.shell.dialog) statt window.confirm; ohne Antwort passiert nichts.
  const confirmed = await state.api.shell.dialog({
    kind: 'confirm',
    title: tr('Alle reparsen', 'Reparse all'),
    text: tr(
      `${candidates.length} CV-PDF${candidates.length === 1 ? '' : 's'} erneut parsen? Jeder CV erhält einen neuen Parser-Auftrag, alte bleiben.`,
      `Reparse ${candidates.length} CV PDF${candidates.length === 1 ? '' : 's'}? Each CV gets a new task, old ones remain.`,
    ),
    confirm_label: tr('Reparsen', 'Reparse'),
    default: false,
  });
  if (confirmed !== true) return;
  await reparseAllPdfs(state, candidates);
}

async function handleUploadChange(state, input) {
  const files = Array.from(input.files || []);
  input.value = '';
  if (!files.length) return;
  try {
    await importPdfs(state, files);
    clearNotice(state, 'import.pdf');
  } catch (error) {
    notify(state, 'error', 'import.pdf', tr('PDF konnte nicht importiert werden', 'PDF could not be imported'), String(error?.message || error));
  }
}

async function handleImportChange(state, input) {
  const file = input.files?.[0];
  input.value = '';
  if (!file) return;
  try {
    await importCandidates(state, file);
    clearNotice(state, 'import.profiles');
  } catch (error) {
    notify(state, 'error', 'import.profiles', tr('Import fehlgeschlagen', 'Import failed'), String(error?.message || error));
  }
}

async function handleLogoChange(state, input) {
  const file = input.files?.[0];
  input.value = '';
  if (!file) return;
  const dataUrl = await fileToDataUrl(file);
  await patchSelectedModel(state, (model) => ({
    ...model,
    print: { ...model.print, showLogo: true, logoDataUrl: dataUrl },
  }));
}

// Selection = in-place class flip across the existing rows (never a rebuild),
// then a stage re-render. Picking a candidate re-reveals the stage.
function selectCandidate(state, id) {
  const item = state.items.find((candidate) => candidate.record.id === id);
  if (!item) return;
  state.selectedId = id;
  state.userCollapsed = false;
  state.viewMode = defaultViewMode(item.model);
  state.lastSelectedId = id;
  state.lastSelectedPhase = workflowPhase(item.model);
  applyListSelection(state);
  renderStage(state);
}

async function runWorkflowAction(state) {
  const item = getSelectedItem(state);
  if (!item) return;
  const phase = workflowPhase(item.model);
  try {
    if (phase === 'uploaded' || phase === 'error') await startParsing(state, item);
    else if (phase === 'review') await approvePrint(state, item);
    else if (phase === 'approved') printApprovedSheet(state, item);
    clearNotice(state, 'action');
  } catch (error) {
    notify(state, 'error', 'action', tr('Aktion fehlgeschlagen', 'Action failed'), String(error?.message || error));
  }
}

async function changeStageView(state, requested) {
  const item = getSelectedItem(state);
  if (!item || !viewAllowed(item.model, requested)) return;
  state.viewMode = requested;
  await patchSelectedModel(state, (model) => ({
    ...model,
    workflow: { ...model.workflow, view_mode: requested, updated_at_ms: Date.now() },
  }), { quiet: true });
  renderStage(state);
}

async function changeTemplate(state, value) {
  await patchSelectedModel(state, (model) => ({
    ...model,
    print: { ...model.print, template: value },
  }));
}

async function toggleAnonymize(state) {
  await patchSelectedModel(state, (model) => ({
    ...model,
    print: { ...model.print, anonymize: !model.print?.anonymize },
  }));
}

async function toggleLogo(state) {
  const item = getSelectedItem(state);
  if (!item) return;
  if (!item.model.print?.logoDataUrl) {
    state.host.querySelector('[data-cv-logo-upload]')?.click();
    return;
  }
  await patchSelectedModel(state, (model) => ({
    ...model,
    print: { ...model.print, showLogo: !model.print?.showLogo },
  }));
}

function toggleStage(state) {
  state.userCollapsed = !state.userCollapsed;
  renderStage(state);
}

function subscribeToCollections(state) {
  const schedule = () => {
    if (!state.mounted) return;
    clearTimeout(state.refreshTimer);
    state.refreshTimer = setTimeout(() => refresh(state), 80);
  };
  // Live-Abos je Montage (PORTING §4 Falle 1); subscribe ruft sofort mit dem aktuellen Stand.
  LIVE_COLLECTIONS.forEach((name) => {
    state.disposers.push(state.api.query(name, {}).subscribe(schedule));
  });
}

function setModuleBusy(state, busy) {
  state.host.toggleAttribute('data-cv-busy', Boolean(busy));
  const upload = state.host.querySelector('[data-cv-upload]');
  const newButton = state.host.querySelector('[data-action="new"]');
  if (upload) {
    upload.disabled = Boolean(busy);
    upload.toggleAttribute('disabled', Boolean(busy));
  }
  if (newButton) {
    newButton.disabled = Boolean(busy);
    newButton.toggleAttribute('disabled', Boolean(busy));
  }
  const reparseButton = state.host.querySelector('[data-cv-reparse-all]');
  if (reparseButton) {
    const disabled = Boolean(state.importing || state.bulkParsing) || !reparseCandidates(state).length;
    reparseButton.disabled = disabled;
    reparseButton.toggleAttribute('disabled', disabled);
  }
}

function refresh(state) {
  if (!state.mounted) return;
  // Alle Lesezugriffe sind lokale Live-Abfragen (kein Netz, kein Warten, keine Readiness).
  const documents = readAll(state, 'documents');
  const versions = readAll(state, 'document_versions');
  const commands = readAll(state, 'business_commands');
  const queueTasks = readAll(state, 'ctox_queue_tasks');
  const versionById = new Map(versions.map((item) => [item.id, item]));
  const liveStatus = buildLiveStatusIndex(commands, queueTasks);
  state.items = documents
    .filter((doc) => doc.document_type === 'cv_print_profile' && !doc.is_deleted)
    .map((doc) => {
      const version = versionById.get(doc.current_version_id);
      return {
        record: doc,
        version,
        model: applyLiveWorkflowState(version?.model_json || null, doc, liveStatus),
      };
    })
    .filter((item) => item.model)
    .sort((a, b) => (b.record.updated_at_ms || 0) - (a.record.updated_at_ms || 0));

  if (!state.selectedId && state.items[0]) {
    state.selectedId = state.items[0].record.id;
    state.viewMode = defaultViewMode(state.items[0].model);
  }
  if (state.selectedId && !state.items.some((item) => item.record.id === state.selectedId)) {
    state.selectedId = state.items[0]?.record.id || '';
    state.viewMode = state.items[0] ? defaultViewMode(state.items[0].model) : 'original';
  }
  const selected = state.items.find((item) => item.record.id === state.selectedId);
  if (selected) {
    const phase = workflowPhase(selected.model);
    if (state.lastSelectedId !== selected.record.id || state.lastSelectedPhase !== phase) {
      state.viewMode = defaultViewMode(selected.model);
      state.lastSelectedId = selected.record.id;
      state.lastSelectedPhase = phase;
    } else if (!viewAllowed(selected.model, state.viewMode)) {
      state.viewMode = defaultViewMode(selected.model);
    }
  } else {
    state.lastSelectedId = '';
    state.lastSelectedPhase = '';
  }

  render(state);
}

function render(state) {
  renderList(state);
  renderStage(state);
  setModuleBusy(state, state.importing || state.bulkParsing);
}

// Shell-wired grammar state, read straight from the pane DOM (the handle may
// not be attached yet on the first mount tick).
function readGrammar(state) {
  const rail = state.host.querySelector('[data-cv-rail]');
  return {
    search: (rail?.querySelector('[data-pg-search]')?.value || '').trim().toLowerCase(),
    // One-button view toggle (operator directive 31.08.2026): the control is an
    // action, so the CURRENT view lives in its data-cv-view, not in aria-pressed.
    view: rail?.querySelector('[data-cv-view-toggle]')?.dataset.cvListView === 'list' ? 'list' : 'cards',
    band: rail?.querySelector('[data-pg-band][aria-selected="true"]')?.dataset.pgBand || 'all',
    sort: rail?.querySelector('[data-pg-filter][data-pg-name="sort"]')?.value || 'updated_desc',
    template: rail?.querySelector('[data-pg-filter][data-pg-name="template"]')?.value || 'all',
  };
}

function writeCounts(state, counts) {
  const rail = state.host.querySelector('[data-cv-rail]');
  const pg = rail?.__ctoxPaneGrammar;
  if (pg?.setCounts) { pg.setCounts(counts); return; }
  for (const [key, value] of Object.entries(counts)) {
    const node = rail?.querySelector(`[data-pg-count="${key}"]`);
    if (node) node.textContent = ` (${value})`;
  }
}

function writeFooter(state, textValue) {
  const rail = state.host.querySelector('[data-cv-rail]');
  const pg = rail?.__ctoxPaneGrammar;
  if (pg?.setFooter) { pg.setFooter(textValue); return; }
  const node = rail?.querySelector('[data-pg-footer]');
  if (node) node.textContent = textValue || '';
}

// Band = high-level workflow status. Real partition with zeros included:
// In Arbeit (uploaded/parsing/error) · Korrektur (review) · Freigegeben.
function bandOf(model) {
  const phase = workflowPhase(model);
  if (phase === 'review') return 'review';
  if (phase === 'approved') return 'approved';
  return 'progress';
}

function bandCounts(state) {
  const counts = { all: state.items.length, progress: 0, review: 0, approved: 0 };
  for (const item of state.items) counts[bandOf(item.model)] += 1;
  return counts;
}

function bandLabel(band) {
  return ({
    all: tr('Alle', 'All'),
    progress: tr('In Arbeit', 'In progress'), // v4-allow: label (Band = Werkstatt-Phase)
    review: tr('Korrektur', 'Review'),
    approved: tr('Freigegeben', 'Approved'),
  })[band] || tr('Alle', 'All');
}

// Data-driven list rebuild. Selection does NOT call this — it flips classes in
// place (applyListSelection). The row is a pure selector; the workflow controls
// live on the stage, so no per-row control listeners are bound here.
function renderList(state) {
  const list = state.host.querySelector('[data-cv-list]');
  if (!list) return;
  const grammar = readGrammar(state);
  const visible = visibleItems(state, grammar);
  list.classList.toggle('is-compact', grammar.view === 'list');
  list.innerHTML = visible.length
    ? visible.map((item) => renderCandidateCard(state, item, grammar.view)).join('')
    : renderListEmptyState();
  applyListSelection(state);
  writeCounts(state, bandCounts(state));
  writeFooter(state, `${visible.length} ${visible.length === 1 ? tr('Eintrag', 'entry') : tr('Einträge', 'entries')} · ${bandLabel(grammar.band)}`);
}

// ---------------------------------------------------------------------------
// One-button view toggle (operator directive 31.08.2026). The control is an
// ACTION, never a state: it shows the icon and the label of the view it will
// switch TO, and it carries no aria-pressed. The currently rendered view lives
// in data-cv-list-view, which readGrammar() reads.
// ---------------------------------------------------------------------------
const VIEW_TOGGLE_ICONS = {
  // Icon of the TARGET view: three lines = "show as list", stacked cards =
  // "show as cards".
  list: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="4" y1="6" x2="20" y2="6"/><line x1="4" y1="12" x2="20" y2="12"/><line x1="4" y1="18" x2="20" y2="18"/></svg>',
  cards: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="4" y="4" width="16" height="7" rx="1.5"/><rect x="4" y="14" width="16" height="7" rx="1.5"/></svg>',
};

function setViewToggle(state, view) {
  const button = state.host.querySelector('[data-cv-view-toggle]');
  if (!button) return;
  const current = view === 'list' ? 'list' : 'cards';
  const target = current === 'list' ? 'cards' : 'list';
  const label = target === 'list'
    ? tr('Als Liste anzeigen', 'Show as list')
    : tr('Als Karten anzeigen', 'Show as cards');
  button.dataset.cvListView = current;
  button.innerHTML = VIEW_TOGGLE_ICONS[target];
  button.setAttribute('aria-label', label);
  button.setAttribute('title', label);
  button.removeAttribute('aria-pressed');
}

function toggleListView(state, button) {
  setViewToggle(state, button.dataset.cvListView === 'list' ? 'cards' : 'list');
  renderList(state);
}

// Leere Liste rendert nichts (R11): weder Filter- noch Datenleere ist ein Zustandstext.
function renderListEmptyState() {
  return '';
}

// In-place selection flip — never a rebuild, so the operator's scroll offset in
// the well is preserved (design-guide: re-renders never move the operator).
function applyListSelection(state) {
  const list = state.host.querySelector('[data-cv-list]');
  list?.querySelectorAll('[data-cv-select]').forEach((row) => {
    const on = (row.dataset.cvSelect || '') === String(state.selectedId || '');
    row.classList.toggle('is-selected', on);
    row.setAttribute('aria-selected', String(on));
  });
}

function visibleItems(state, grammar = readGrammar(state)) {
  const { search, band, template, sort } = grammar;
  let items = state.items.slice();
  if (band && band !== 'all') items = items.filter((item) => bandOf(item.model) === band);
  if (template && template !== 'all') items = items.filter((item) => normalizeTemplateId(item.model.print?.template) === template);
  if (search) items = items.filter((item) => candidateSearchText(item).includes(search));
  return items.sort((a, b) => compareCandidates(a, b, sort));
}

function candidateSearchText(item) {
  const model = item.model || {};
  const candidate = model.candidate || {};
  return [
    displayCandidateName(model),
    candidate.currentRole,
    candidate.location,
    candidate.availability,
    model.source?.filename,
    item.record?.filename,
    workflowPhase(model),
    templateLabel(model.print?.template),
  ].filter(Boolean).join(' ').toLowerCase();
}

function compareCandidates(a, b, mode) {
  if (mode === 'name_asc') return displayCandidateName(a.model).localeCompare(displayCandidateName(b.model), 'de', { sensitivity: 'base' });
  if (mode === 'phase_asc') return phaseOrder(a.model) - phaseOrder(b.model)
    || displayCandidateName(a.model).localeCompare(displayCandidateName(b.model), 'de', { sensitivity: 'base' });
  if (mode === 'template_asc') return templateLabel(a.model.print?.template).localeCompare(templateLabel(b.model.print?.template), 'de', { sensitivity: 'base' })
    || displayCandidateName(a.model).localeCompare(displayCandidateName(b.model), 'de', { sensitivity: 'base' });
  return Number(b.record.updated_at_ms || 0) - Number(a.record.updated_at_ms || 0);
}

function phaseOrder(model) {
  return ({ error: 0, uploaded: 1, parsing: 2, review: 3, approved: 4 })[workflowPhase(model)] ?? 9;
}

function buildLiveStatusIndex(commands, queueTasks) {
  const byCommandId = new Map();
  const byRecordId = new Map();
  const tasksById = new Map();
  const tasksByCommandId = new Map();
  commands
    .filter(isCvPrintParseCommandProjection)
    .sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0))
    .forEach((command) => {
      const commandId = command.command_id || command.id;
      if (commandId && !byCommandId.has(commandId)) byCommandId.set(commandId, command);
      const recordId = command.record_id || command.payload?.record_id || command.payload?.document_id || command.client_context?.document_id;
      if (recordId && !byRecordId.has(recordId)) byRecordId.set(recordId, command);
    });
  queueTasks.forEach((task) => {
    if (task.id) tasksById.set(task.id, task);
    const commandId = task.command_id || task.business_os_command_id;
    if (commandId && !tasksByCommandId.has(commandId)) tasksByCommandId.set(commandId, task);
  });
  return { byCommandId, byRecordId, tasksById, tasksByCommandId };
}

function isCvPrintParseCommandProjection(command) {
  // v4: Befehlsdokument {id, type, payload, status, app_id, result?, error?}; v3-Projektionen bleiben lesbar.
  return command?.app_id === MODULE_ID
    || command?.type === PARSE_COMMAND
    || command?.module === MODULE_ID
    || command?.client_context?.module === MODULE_ID
    || command?.client_context?.source_module === MODULE_ID
    || command?.payload?.source_module === MODULE_ID
    || command?.payload?.writeback_contract?.command_type === 'ctox.cv_print.apply_parse';
}

function applyLiveWorkflowState(model, record, liveStatus) {
  if (!model) return null;
  const next = structuredClone(model);
  next.workflow = next.workflow || {};
  const commandId = next.workflow.command_id || '';
  const taskId = next.workflow.task_id || '';
  const command = (commandId && liveStatus.byCommandId.get(commandId))
    || liveStatus.byRecordId.get(record.id)
    || null;
  // v4: der Handler liefert {task_id} als Befehlsergebnis.
  const commandTaskId = command?.task_id || command?.result?.task_id || '';
  const commandKey = command?.command_id || command?.id || '';
  const task = (taskId && liveStatus.tasksById.get(taskId))
    || (commandTaskId && liveStatus.tasksById.get(commandTaskId))
    || (commandKey && liveStatus.tasksByCommandId.get(commandKey))
    || null;
  const live = liveWorkflowStatus(command, task);
  if (commandKey && !next.workflow.command_id) next.workflow.command_id = commandKey;
  if (task?.id && !next.workflow.task_id) next.workflow.task_id = task.id;
  if (commandTaskId && !next.workflow.task_id) next.workflow.task_id = commandTaskId;
  if (live.status) next.workflow.task_status = live.status;
  if (live.error) next.workflow.error = live.error;

  const phase = workflowPhase(next);
  if (phase === 'parsing' && live.status === 'failed') {
    next.workflow.phase = 'error';
    next.workflow.view_mode = 'original';
  }
  return next;
}

function liveWorkflowStatus(command, task) {
  const rawStatus = [
    command?.status,
    command?.task_status,
    task?.status,
    task?.task_status,
    task?.route_status,
  ].find((value) => String(value || '').trim());
  const status = normalizeLiveStatus(rawStatus);
  const error = [
    command?.error,
    command?.status_note,
    command?.queue_status_note,
    task?.error,
    task?.status_note,
  ].find((value) => String(value || '').trim()) || '';
  return { status, error: String(error || '') };
}

function normalizeLiveStatus(value) {
  const status = String(value || '').trim().toLowerCase();
  if (!status) return '';
  if (['failed', 'error'].includes(status)) return 'failed';
  if (['handled', 'completed', 'complete', 'done'].includes(status)) return 'completed';
  if (['leased', 'running', 'active', 'in_progress'].includes(status)) return 'running';
  if (['pending', 'queued', 'accepted', 'pending_sync'].includes(status)) return 'queued';
  return status;
}

// Short day-precision stamp for the shard's second meta line. Day precision is
// enough for a recruiting pipeline and keeps the line from wrapping.
function shortStamp(ms) {
  const value = Number(ms || 0);
  if (!value) return '';
  try {
    return new Date(value).toLocaleDateString(ACTIVE_LOCALE === 'en' ? 'en-US' : 'de-DE', {
      day: '2-digit', month: '2-digit', year: '2-digit',
    });
  } catch { return ''; }
}

// A shard is a PURE selector: title + muted meta lines. No inline controls, no
// expansion — the selected CV's workflow lives on the stage.
//
// The two views are deliberately different shapes, not two paddings of the same
// thing (operator directive 31.08.2026):
//   cards — avatar + bold name + TWO meta lines (workflow status · role, then
//           template · location · last change), roomy padding. Scanning view.
//   list  — exactly ONE dense line: name plus one short meta (the workflow
//           status) flush right. Density view.
function renderCandidateCard(state, item, view) {
  const model = item.model;
  const candidate = model.candidate || {};
  const phase = workflowPhase(model);
  const selected = item.record.id === state.selectedId;
  const name = displayCandidateName(model);
  const attrs = `data-cv-select="${escapeAttr(item.record.id)}" data-context-record-id="${escapeAttr(item.record.id)}" data-context-record-type="cv_profile" data-context-label="${escapeAttr(item.record.title || name)}" aria-selected="${selected ? 'true' : 'false'}"`;
  if (view === 'list') {
    return `
      <button type="button" class="cv-row${selected ? ' is-selected' : ''}" ${attrs}>
        <span class="cv-row-name">${escapeHtml(name)}</span>
        <span class="cv-row-meta">${escapeHtml(phaseLabel(phase))}</span>
      </button>
    `;
  }
  const primary = [phaseLabel(phase), candidate.currentRole].filter(Boolean).join(' · ');
  const secondary = [templateLabel(model.print?.template), candidate.location, shortStamp(item.record.updated_at_ms)]
    .filter(Boolean).join(' · ');
  return `
    <article class="cv-card${selected ? ' is-selected' : ''}" ${attrs}>
      <span class="ctox-avatar">${escapeHtml(initials(candidate.name || name))}</span>
      <div class="cv-card-info">
        <h3 class="cv-card-name">${escapeHtml(name)}</h3>
        <div class="cv-card-meta cv-card-meta--phase">${escapeHtml(primary || phaseLabel(phase))}</div>
        ${secondary ? `<div class="cv-card-meta cv-card-meta--sub">${escapeHtml(secondary)}</div>` : ''}
      </div>
    </article>
  `;
}

// The stage auto-reveals on selection: visible = hasSelection && !userCollapsed
// (recomputed every render). No selection → empty state; collapsed → a compact
// re-show line; visible → header controls + the render/print pipeline.
function stageVisible(state) {
  return Boolean(getSelectedItem(state)) && !state.userCollapsed;
}

function collapsedStageHtml(title) {
  return `<div class="ctox-empty"><div><strong>${escapeHtml(title)}</strong><div><button type="button" class="ctox-button cv-stage-show" data-toggle-stage>${escapeHtml(tr('Anzeigen', 'Show'))}</button></div></div></div>`;
}

function renderStage(state) {
  const main = state.host.querySelector('[data-cv-main]');
  const head = state.host.querySelector('[data-cv-stage-head]');
  const stage = state.host.querySelector('[data-cv-stage]');
  if (!stage) return;
  const item = getSelectedItem(state);
  const visible = stageVisible(state);

  if (head) head.hidden = !visible;
  if (main) {
    if (visible && item) {
      main.setAttribute('data-context-record-id', item.record.id);
      main.setAttribute('data-context-record-type', 'cv_profile');
      main.setAttribute('data-context-label', item.record.title || displayCandidateName(item.model));
    } else {
      main.removeAttribute('data-context-record-id');
      main.removeAttribute('data-context-record-type');
      main.removeAttribute('data-context-label');
    }
  }

  // Ohne Auswahl bleibt die Bühne leer (R11). Ausgeblendet: Name + Knopf zum Wiedereinblenden.
  if (!item) {
    stage.innerHTML = '';
    return;
  }
  if (!visible) {
    stage.innerHTML = collapsedStageHtml(displayCandidateName(item.model));
    return;
  }

  const phase = workflowPhase(item.model);
  if (phase === 'approved') state.viewMode = 'print';
  if (!viewAllowed(item.model, state.viewMode)) state.viewMode = defaultViewMode(item.model);
  renderStageHead(state, item);

  const serial = ++state.renderSerial;
  const mode = state.viewMode;
  const views = [];
  if (mode === 'original' || mode === 'split') views.push(renderOriginalPane(state, item));
  if (mode === 'print' || mode === 'split') views.push(renderPrintPane(item));
  stage.innerHTML = `<div class="cv-view-grid${mode === 'split' ? ' is-split' : ''}">${views.join('')}</div>`;
  bindEditablePrintFields(state);
  bindFieldEditor(state);
  applyEditorDraft(state);
  ensureOriginalUrl(state, item)
    .then(() => {
      clearNotice(state, 'original');
      if (serial === state.renderSerial) renderOriginalFrame(state, item);
    })
    .catch((error) => {
      const key = sourceKey(item.model);
      const message = error?.message || String(error || '');
      if (key) state.originalErrors.set(key, message);
      // Fehler in die Zustandszone statt Fehlerkasten im Inhalt (R11).
      notify(state, 'error', 'original', tr('Original-PDF konnte nicht geöffnet werden', 'Original PDF could not be opened'), message);
      if (serial === state.renderSerial) renderOriginalFrame(state, item);
    });
}

// Stage header: candidate identity + element actions (anonymize, logo, the one
// primary workflow action, collapse) top-right, and a view-switch band + the
// print-template select in the controls row underneath.
function renderStageHead(state, item) {
  const model = item.model;
  const phase = workflowPhase(model);
  const approved = phase === 'approved';
  const controlsDisabled = phase === 'uploaded' || phase === 'parsing';
  const action = actionForPhase(phase);
  const kicker = state.host.querySelector('[data-cv-stage-kicker]');
  const title = state.host.querySelector('[data-cv-stage-title]');
  const actions = state.host.querySelector('[data-cv-stage-actions]');
  const controls = state.host.querySelector('[data-cv-stage-controls]');
  if (kicker) kicker.textContent = phaseLabel(phase);
  if (title) title.textContent = displayCandidateName(model);
  if (actions) {
    actions.innerHTML = `
      <button class="ctox-pane-icon${model.print?.anonymize ? ' is-active' : ''}" data-short-de="Anonymisieren" type="button" data-cv-toggle-anon title="${escapeAttr(tr('Anonymisieren', 'Anonymize'))}" aria-label="${escapeAttr(tr('Anonymisieren', 'Anonymize'))}" ${controlsDisabled || approved ? 'disabled' : ''}>${iconEyeOff()}</button>
      <button class="ctox-pane-icon${model.print?.showLogo ? ' is-active' : ''}" type="button" data-cv-logo-control title="${escapeAttr(model.print?.logoDataUrl ? tr('Logo anzeigen', 'Show logo') : tr('Logo wählen', 'Choose logo'))}" aria-label="${escapeAttr(model.print?.logoDataUrl ? tr('Logo anzeigen', 'Show logo') : tr('Logo wählen', 'Choose logo'))}" ${controlsDisabled || approved ? 'disabled' : ''}>${iconImage()}</button>
      <button class="ctox-pane-icon is-primary" type="button" data-cv-action title="${escapeAttr(action.title)}" aria-label="${escapeAttr(action.title)}" ${phase === 'parsing' ? 'disabled' : ''}>${action.icon}</button>
      <button class="ctox-pane-icon" type="button" data-toggle-stage title="${escapeAttr(tr('Bühne ausblenden', 'Hide stage'))}" aria-label="${escapeAttr(tr('Bühne ausblenden', 'Hide stage'))}" aria-pressed="false">${iconCollapse()}</button>
    `;
  }
  if (controls) {
    const template = normalizeTemplateId(model.print?.template || 'modern');
    const templateOptions = TEMPLATES.map((entry) => `<option value="${escapeHtml(entry.id)}"${entry.id === template ? ' selected' : ''}>${escapeHtml(templateLabel(entry.id))}</option>`).join('');
    controls.innerHTML = `
      <div class="ctox-pane-tabs cv-view-switch-tabs" role="tablist" aria-label="${escapeAttr(tr('Ansicht', 'View'))}">
        ${Object.entries(VIEW_LABELS).map(([key, label]) => `<button type="button" role="tab" data-cv-view="${key}" class="ctox-pane-tab${state.viewMode === key ? ' is-active' : ''}" aria-selected="${state.viewMode === key ? 'true' : 'false'}" ${viewAllowed(model, key) && !approved ? '' : 'disabled'}>${escapeHtml(key === 'split' ? tr('Beide', 'Both') : label)}</button>`).join('')}
      </div>
      <select class="ctox-select cv-template-select" data-cv-template ${controlsDisabled || approved ? 'disabled' : ''} aria-label="${escapeAttr(tr('Print Template', 'Print template'))}">${templateOptions}</select>
      <div class="cv-stage-foot">${phaseFootnoteHtml(model)}</div>
    `;
  }
}

function renderOriginalPane(state, item) {
  return `
    <section class="cv-pane" data-cv-original-pane>
      <header class="ctox-pane-header cv-pane-head">
        <h3 class="ctox-pane-title">${escapeHtml(tr('Original PDF', 'Original PDF'))}</h3>
      </header>
      <div class="cv-pane-body" data-cv-original-body></div>
    </section>
  `;
}

function renderOriginalFrame(state, item) {
  const body = state.host.querySelector('[data-cv-original-body]');
  if (!body) return;
  const url = state.originalUrls.get(sourceKey(item.model));
  // Ohne Blob (noch nicht lokal oder Fehler → Zone) bleibt die Fläche leer.
  body.innerHTML = url
    ? `<iframe class="cv-original-frame" title="Original PDF" src="${escapeAttr(`${url}#toolbar=0&navpanes=0&view=FitH`)}"></iframe>`
    : '';
}

function renderPrintPane(item) {
  const model = item.model;
  const editor = workflowPhase(model) === 'review' ? renderFieldEditor(model) : '';
  return `
    <section class="cv-pane">
      <header class="ctox-pane-header cv-pane-head">
        <h3 class="ctox-pane-title">${escapeHtml(tr('Druckansicht', 'Print preview'))}</h3>
      </header>
      <div class="cv-pane-body">
        <div class="cv-print-workarea${editor ? ' has-editor' : ''}">
          ${editor}
          <div class="cv-print-wrap">
          ${renderPrintSheet(model)}
          </div>
        </div>
      </div>
    </section>
  `;
}

function renderPrintSheet(model) {
  const template = normalizeTemplateId(model.print?.template || 'modern');
  if (template === 'minimal') return renderMinimalPrintSheet(model);
  if (template === 'classic') return renderClassicPrintSheet(model);
  return renderModernPrintSheet(model);
}

function renderModernPrintSheet(model) {
  const data = getQualificationPrintData(model);
  return `
    <article class="cv-print-sheet cv-qprofile cv-template-modern">
      ${renderQualificationHeader(data, 'modern')}
      <main class="cv-q-body">
        ${renderQualificationProfile(data)}
        ${renderQualificationMethods(data)}
        ${renderQualificationEntries('BERUFLICHER WERDEGANG / PROJEKTKOMPETENZ', data.career, 'career')}
        ${renderQualificationEntries('AUSBILDUNG', data.education, 'education')}
      </main>
      ${renderQualificationFooter(data)}
    </article>
  `;
}

function renderMinimalPrintSheet(model) {
  const data = getQualificationPrintData(model, { minimalLogo: true });
  return `
    <article class="cv-print-sheet cv-qprofile cv-template-minimal">
      ${renderQualificationHeader(data, 'minimal')}
      <main class="cv-q-body">
        ${renderQualificationProfile(data)}
        ${renderQualificationMethods(data)}
        ${renderQualificationEntries('BERUFLICHER WERDEGANG / PROJEKTKOMPETENZ', data.career, 'career')}
        ${renderQualificationEntries('AUSBILDUNG', data.education, 'education')}
      </main>
      ${renderQualificationFooter(data)}
    </article>
  `;
}

function renderClassicPrintSheet(model) {
  const data = getQualificationPrintData(model, { classicLogo: true });
  return `
    <article class="cv-print-sheet cv-qprofile cv-template-classic">
      ${renderQualificationHeader(data, 'classic')}
      <main class="cv-q-body">
        ${renderQualificationProfile(data)}
        ${renderQualificationMethods(data)}
        ${renderQualificationEntries('BERUFLICHER WERDEGANG / PROJEKTKOMPETENZ', data.career, 'career')}
        ${renderQualificationEntries('AUSBILDUNG', data.education, 'education')}
      </main>
      ${renderQualificationFooter(data)}
    </article>
  `;
}

function getQualificationPrintData(model, options = {}) {
  const candidate = model.candidate || {};
  const phase = workflowPhase(model);
  const editable = phase === 'review';
  const anonymize = Boolean(model.print?.anonymize);
  const logo = model.print?.showLogo ? renderLogo(model, options) : '';
  const name = anonymize ? anonymizedName(candidate.name) : displayCandidateName(model);
  const additional = candidate.additional || [];
  const cvMeta = {
    ...(candidate.cv?.meta || {}),
    ...(additionalValue(additional, 'cv.meta') || {}),
  };
  const role = candidate.currentRole || candidate.desiredPosition || cvMeta.currentPosition || tr('CV Profil', 'CV profile');
  const facts = buildQualificationFacts(candidate, cvMeta, anonymize);
  const skills = normalizeCvSkillsObject(candidate);
  const methods = buildMethodsFromCvSkills(skills, {
    ...cvMeta,
    languages: normalizeLanguageItems(candidate.languages || cvMeta.languages || []),
  });
  const career = normalizeExperienceToCareerEntries(normalizeTimeline(additional, 'cv.experience'));
  const education = normalizeEducationToEntries(normalizeTimeline(additional, 'cv.education'));
  const footer = {
    userName: 'Steffen Ratschan',
    companyLine: 'pmX GmbH | Kegelenstr. 3 | 70372 Stuttgart',
    contactLine: anonymize ? tr('Anonymisiert', 'Anonymized') : contactLine(candidate),
  };
  return { candidate, editable, anonymize, logo, name, role, facts, methods, career, education, footer };
}

function renderQualificationHeader(data, theme) {
  const logo = theme === 'minimal' ? '' : data.logo;
  return `
    <header class="cv-q-head">
      <div>
        <h1>${escapeHtml(tr('QUALIFIKATIONSPROFIL', 'QUALIFICATION PROFILE'))}</h1>
        <div class="cv-q-rule"></div>
      </div>
      ${logo}
    </header>
  `;
}

function renderQualificationProfile(data) {
  return `
    <section class="cv-q-profile">
      <div class="cv-q-avatar">${escapeHtml(initials(data.name))}</div>
      <div>
        <h2>${escapeHtml(data.name)}</h2>
        <div class="cv-q-role">${escapeHtml(data.role)}</div>
        <dl class="cv-q-facts">
          ${data.facts.map((fact) => `
            <div>
              <dt>${escapeHtml(fact.label)}:</dt>
              <dd>${escapeHtml(fact.value || '-')}</dd>
            </div>
          `).join('')}
        </dl>
      </div>
    </section>
  `;
}

function renderQualificationMethods(data) {
  return `
    <section class="cv-q-section">
      <h3>${escapeHtml(tr('METHODEN- / SYSTEMKOMPETENZ / SPRACHKENNTNISSE', 'METHODS / SYSTEM EXPERTISE / LANGUAGES'))}</h3>
      <dl class="cv-q-methods">
        ${data.methods.length ? data.methods.map((item) => `
          <div>
            <dt>${escapeHtml(item.label)}</dt>
            <dd>${escapeHtml(item.value)}</dd>
          </div>
        `).join('') : '<div><dt>' + escapeHtml(tr('Kompetenzen', 'Skills')) + '</dt><dd>' + escapeHtml(tr('Nach Parsing korrigieren', 'Review after parsing')) + '</dd></div>'}
      </dl>
    </section>
  `;
}

function renderQualificationEntries(title, entries, kind) {
  return `
    <section class="cv-q-section">
      <h3>${escapeHtml(title)}</h3>
      <div class="cv-q-entries">
        ${entries.length ? entries.map((entry) => renderQualificationEntry(entry, kind)).join('') : `
          <article class="cv-q-entry">
            <div class="cv-q-when">-</div>
            <div><strong>${escapeHtml(tr('Keine Einträge hinterlegt', 'No entries recorded'))}</strong></div>
          </article>
        `}
      </div>
    </section>
  `;
}

function renderQualificationEntry(entry, kind) {
  const title = kind === 'education' ? entry.title : entry.title;
  const org = kind === 'education' ? entry.org : entry.employer;
  const bullets = Array.isArray(entry.bullets) ? entry.bullets : [];
  const extra = Array.isArray(entry.extra) ? entry.extra : [];
  return `
    <article class="cv-q-entry">
      <div class="cv-q-when">${escapeHtml([entry.from, entry.to].filter(Boolean).join(' - ') || '-')}</div>
      <div class="cv-q-entry-main">
        <strong>${escapeHtml(title || tr('Station', 'Entry'))}</strong>
        ${org ? `<span>${escapeHtml(org)}</span>` : ''}
        ${extra.map((item) => `<em>${escapeHtml(item)}</em>`).join('')}
        ${bullets.length ? `<ul>${bullets.map((bullet) => `<li>${escapeHtml(bullet.text || bullet)}</li>`).join('')}</ul>` : ''}
      </div>
    </article>
  `;
}

function renderQualificationFooter(data) {
  const line = data.footer.companyLine || '';
  return `
    <footer class="cv-q-foot">
      <div>
        <strong>${escapeHtml(data.footer.userName || '-')}</strong>
        <span>${escapeHtml(line)}</span>
        <span>${escapeHtml(data.footer.contactLine || '-')}</span>
      </div>
      <div>${escapeHtml(new Date().toLocaleDateString(ACTIVE_LOCALE === 'en' ? 'en-US' : 'de-DE'))}</div>
    </footer>
  `;
}

function buildQualificationFacts(candidate, cvMeta, anonymize) {
  const facts = [];
  const highestDegree = candidate.highestDegree || cvMeta.highestDegree || '';
  const degree = candidate.degree || cvMeta.degree || '';
  const birthDate = candidate.birthDate || cvMeta.birthDate || '';
  const nationality = candidate.nationality || cvMeta.nationality || '';
  const availability = candidate.availability || cvMeta.availabilityFrom || '';
  if (highestDegree) facts.push({ label: tr('Höchster Abschluss', 'Highest qualification'), value: highestDegree });
  if (degree) facts.push({ label: tr('Fachrichtung', 'Field of study'), value: degree });
  if (birthDate) facts.push({ label: anonymize ? tr('Geburtsjahr', 'Year of birth') : tr('Geburtsdatum', 'Date of birth'), value: anonymize ? yearFromDateText(birthDate) : formatCvDisplayDate(birthDate) });
  if (nationality) facts.push({ label: tr('Nationalität', 'Nationality'), value: nationality });
  if (availability) facts.push({ label: tr('Verfügbarkeit', 'Availability'), value: formatCvDisplayDate(availability) });
  if (!facts.length) facts.push({ label: tr('Profil', 'Profile'), value: tr('Felder prüfen', 'Review fields') });
  return facts;
}

function normalizeExperienceToCareerEntries(experience) {
  return (Array.isArray(experience) ? experience : []).map((entry) => {
    const title = firstNonEmpty(entry?.job_title, entry?.title, entry?.role, entry?.jobTitle, entry?.position) || tr('Position', 'Position');
    const employer = firstNonEmpty(entry?.employer, entry?.company, entry?.companyName, entry?.org) || '';
    const from = firstNonEmpty(entry?.start_date, entry?.startDate, entry?.start, entry?.from);
    const to = firstNonEmpty(entry?.end_date, entry?.endDate, entry?.end, entry?.to) || tr('heute', 'present');
    const bullets = valueToLines(entry?.job_description || entry?.description || entry?.summary)
      .slice(0, 12)
      .map((text) => ({ text }));
    return { from, to, title, employer, bullets };
  });
}

function normalizeEducationToEntries(education) {
  return (Array.isArray(education) ? education : []).map((entry) => {
    const title = firstNonEmpty(entry?.degree, entry?.title, entry?.program) || tr('Ausbildung', 'Education');
    const org = firstNonEmpty(entry?.institution, entry?.school, entry?.university, entry?.provider, entry?.org) || '';
    const from = firstNonEmpty(entry?.start_date, entry?.startDate, entry?.start, entry?.from);
    const to = firstNonEmpty(entry?.end_date, entry?.endDate, entry?.end, entry?.to) || tr('dato', 'present');
    const extra = [entry?.major, entry?.location, entry?.specialization].filter((item) => String(item || '').trim());
    const bullets = valueToLines(entry?.details || entry?.description || entry?.summary)
      .slice(0, 8)
      .map((text) => ({ text }));
    return { from, to, title, org, extra, bullets };
  });
}

function buildMethodsFromCvSkills(cvSkills, candidateMeta) {
  const out = [];
  const fach = cvSkills.Fachkenntnisse || cvSkills.fachkenntnisse || cvSkills.skills;
  const sprachen = cvSkills.Sprachkenntnisse || cvSkills.sprachkenntnisse || cvSkills.languages;
  const other = cvSkills['Weitere Fähigkeiten'] || cvSkills['Weitere Faehigkeiten'] || cvSkills.other_skills;
  if (fach) out.push({ label: tr('Fachkenntnisse', 'Professional skills'), value: valueToLines(fach).join(', ') });
  if (other) out.push({ label: tr('Weitere Fähigkeiten', 'Additional skills'), value: valueToLines(other).join(', ') });
  if (sprachen) out.push({ label: tr('Sprachkenntnisse', 'Languages'), value: valueToLines(sprachen).join(', ') });
  if (!sprachen) {
    const languageText = normalizeLanguageItems(candidateMeta?.languages || [])
      .map((item) => [item.label || item.code || item.language || item.name, item.level].filter(Boolean).join(' '))
      .filter(Boolean)
      .join(', ');
    if (languageText) out.push({ label: tr('Sprachkenntnisse', 'Languages'), value: languageText });
  }
  return out.filter((item) => item.value);
}

function renderFieldEditor(model) {
  const candidate = model.candidate || {};
  const cv = getCvEditorData(model);
  return `
    <aside class="cv-field-editor" data-cv-field-editor>
      <header class="ctox-pane-header cv-field-editor-head">
        <h3 class="ctox-pane-title">${escapeHtml(tr('Felder', 'Fields'))}</h3>
      </header>
      <section class="cv-editor-block">
        <h4 class="ctox-field-label">${escapeHtml(tr('Stammdaten', 'Core data'))}</h4>
        <div class="cv-editor-grid">
          ${renderEditorInput('Name', 'candidate.name', displayCandidateName(model))}
          ${renderEditorInput(tr('Rolle', 'Role'), 'candidate.currentRole', candidate.currentRole || '')}
          ${renderEditorInput(tr('Ort', 'Location'), 'candidate.location', candidate.location || '')}
          ${renderEditorInput(tr('Verfügbarkeit', 'Availability'), 'candidate.availability', candidate.availability || '')}
          ${renderEditorInput(tr('Abschluss', 'Qualification'), 'candidate.highestDegree', candidate.highestDegree || '')}
          ${renderEditorInput(tr('Fachrichtung', 'Field of study'), 'candidate.degree', candidate.degree || '')}
          ${renderEditorInput(tr('Geburtsdatum', 'Date of birth'), 'candidate.birthDate', candidate.birthDate || '')}
          ${renderEditorInput(tr('Nationalität', 'Nationality'), 'candidate.nationality', candidate.nationality || '')}
        </div>
      </section>
      <section class="cv-editor-block">
        <div class="cv-editor-title-row">
          <h4 class="ctox-field-label">${escapeHtml(tr('Beruflicher Werdegang', 'Professional experience'))}</h4>
          <button class="ctox-icon-button ctox-icon-button--sm" type="button" data-cv-editor-action="add-experience" aria-label="${escapeAttr(tr('Station hinzufügen', 'Add entry'))}">+</button>
        </div>
        <div data-cv-editor-list="experience">
          ${renderExperienceEditorRows(cv.experience)}
        </div>
      </section>
      <section class="cv-editor-block">
        <div class="cv-editor-title-row">
          <h4 class="ctox-field-label">${escapeHtml(tr('Ausbildung', 'Education'))}</h4>
          <button class="ctox-icon-button ctox-icon-button--sm" type="button" data-cv-editor-action="add-education" aria-label="${escapeAttr(tr('Ausbildung hinzufügen', 'Add education'))}">+</button>
        </div>
        <div data-cv-editor-list="education">
          ${renderEducationEditorRows(cv.education)}
        </div>
      </section>
      <section class="cv-editor-block">
        <h4 class="ctox-field-label">Skills</h4>
        ${renderSkillEditor('Fachkenntnisse', cv.skills.Fachkenntnisse || cv.skills.skills || [])}
        ${renderSkillEditor('Sprachkenntnisse', cv.skills.Sprachkenntnisse || cv.skills.languages || [])}
        ${renderSkillEditor('Weitere Fähigkeiten', cv.skills['Weitere Fähigkeiten'] || cv.skills['Weitere Faehigkeiten'] || cv.skills.other_skills || [])}
      </section>
    </aside>
  `;
}

function renderEditorInput(label, path, value) {
  return `
    <label class="ctox-compact-field">
      <span>${escapeHtml(label)}</span>
      <input class="ctox-input" data-cv-field-path="${escapeAttr(path)}" value="${escapeAttr(value || '')}" />
    </label>
  `;
}

function renderExperienceEditorRows(items) {
  if (!items.length) return '<div class="cv-editor-empty">Keine Stationen.</div>';
  return items.map((item, index) => `
    <article class="cv-editor-row" data-cv-entry-kind="experience" data-cv-index="${index}">
      <button class="ctox-icon-button ctox-icon-button--sm is-danger" type="button" data-cv-editor-action="remove-experience" data-cv-index="${index}" aria-label="Station entfernen">×</button>
      <input class="ctox-input" data-cv-entry-field="job_title" value="${escapeAttr(firstNonEmpty(item.job_title, item.title, item.role, item.jobTitle))}" placeholder="Position" />
      <input class="ctox-input" data-cv-entry-field="employer" value="${escapeAttr(firstNonEmpty(item.employer, item.company, item.companyName))}" placeholder="Arbeitgeber" />
      <input class="ctox-input" data-cv-entry-field="start_date" value="${escapeAttr(firstNonEmpty(item.start_date, item.startDate, item.start, item.from))}" placeholder="Start" />
      <input class="ctox-input" data-cv-entry-field="end_date" value="${escapeAttr(firstNonEmpty(item.end_date, item.endDate, item.end, item.to))}" placeholder="Ende" />
      <textarea class="ctox-textarea" data-cv-entry-field="job_description" rows="2" placeholder="Aufgaben / Erfolge">${escapeHtml(valueToLines(item.job_description || item.description || item.summary).join('\n'))}</textarea>
    </article>
  `).join('');
}

function renderEducationEditorRows(items) {
  if (!items.length) return '<div class="cv-editor-empty">Keine Ausbildung.</div>';
  return items.map((item, index) => `
    <article class="cv-editor-row" data-cv-entry-kind="education" data-cv-index="${index}">
      <button class="ctox-icon-button ctox-icon-button--sm is-danger" type="button" data-cv-editor-action="remove-education" data-cv-index="${index}" aria-label="Ausbildung entfernen">×</button>
      <input class="ctox-input" data-cv-entry-field="degree" value="${escapeAttr(firstNonEmpty(item.degree, item.title, item.program))}" placeholder="Abschluss" />
      <input class="ctox-input" data-cv-entry-field="institution" value="${escapeAttr(firstNonEmpty(item.institution, item.school, item.university, item.provider))}" placeholder="Institution" />
      <input class="ctox-input" data-cv-entry-field="major" value="${escapeAttr(item.major || '')}" placeholder="Hauptfach" />
      <input class="ctox-input" data-cv-entry-field="specialization" value="${escapeAttr(item.specialization || '')}" placeholder="Schwerpunkt" />
      <input class="ctox-input" data-cv-entry-field="start_date" value="${escapeAttr(firstNonEmpty(item.start_date, item.startDate, item.start, item.from))}" placeholder="Start" />
      <input class="ctox-input" data-cv-entry-field="end_date" value="${escapeAttr(firstNonEmpty(item.end_date, item.endDate, item.end, item.to))}" placeholder="Ende" />
      <textarea class="ctox-textarea" data-cv-entry-field="details" rows="2" placeholder="Details">${escapeHtml(valueToLines(item.details || item.description || item.summary).join('\n'))}</textarea>
    </article>
  `).join('');
}

function renderSkillEditor(label, value) {
  return `
    <label class="ctox-compact-field cv-editor-skill">
      <span>${escapeHtml(label)}</span>
      <textarea class="ctox-textarea" data-cv-skill-group="${escapeAttr(label)}" rows="2">${escapeHtml(valueToLines(value).join('\n'))}</textarea>
    </label>
  `;
}

function bindFieldEditor(state) {
  const editor = state.host.querySelector('[data-cv-field-editor]');
  if (!editor) return;
  // Offene Eingabe merken (Entwurf je Profil); ein gespeichertes Feld (blur) verlässt den Entwurf.
  const documentId = state.selectedId;
  editor.addEventListener('input', (event) => {
    const key = editorKey(event.target);
    if (!key) return;
    if (state.editorDraft.documentId !== documentId) state.editorDraft = { documentId, fields: {} };
    state.editorDraft.fields[key] = event.target.value;
  });
  editor.addEventListener('focusout', (event) => {
    const key = editorKey(event.target);
    if (key && state.editorDraft.documentId === documentId) delete state.editorDraft.fields[key];
  });
  editor.querySelectorAll('[data-cv-field-path]').forEach((input) => {
    input.addEventListener('blur', async () => {
      const path = input.dataset.cvFieldPath;
      const value = input.value.trim();
      await patchSelectedModel(state, (model) => syncCandidateMetaAdditional(setPath({ ...model }, path, value)));
    });
  });
  editor.querySelectorAll('[data-cv-entry-field]').forEach((input) => {
    input.addEventListener('blur', async () => {
      await patchSelectedModel(state, (model) => patchCvEditorData(model, readCvEditorData(editor)));
    });
  });
  editor.querySelectorAll('[data-cv-skill-group]').forEach((input) => {
    input.addEventListener('blur', async () => {
      await patchSelectedModel(state, (model) => patchCvEditorData(model, readCvEditorData(editor)));
    });
  });
  editor.querySelectorAll('[data-cv-editor-action]').forEach((button) => {
    button.addEventListener('click', async () => {
      await patchSelectedModel(state, (model) => applyEditorButtonAction(model, button.dataset.cvEditorAction, Number(button.dataset.cvIndex || -1)));
    });
  });
}

function getCvEditorData(model) {
  const candidate = model.candidate || {};
  return {
    experience: normalizeTimeline(candidate.additional, 'cv.experience'),
    education: normalizeTimeline(candidate.additional, 'cv.education'),
    skills: normalizeCvSkillsObject(candidate),
  };
}

function readCvEditorData(editor) {
  const experience = Array.from(editor.querySelectorAll('[data-cv-entry-kind="experience"]')).map((row) => ({
    job_title: row.querySelector('[data-cv-entry-field="job_title"]')?.value.trim() || '',
    employer: row.querySelector('[data-cv-entry-field="employer"]')?.value.trim() || '',
    start_date: row.querySelector('[data-cv-entry-field="start_date"]')?.value.trim() || '',
    end_date: row.querySelector('[data-cv-entry-field="end_date"]')?.value.trim() || '',
    job_description: valueToLines(row.querySelector('[data-cv-entry-field="job_description"]')?.value || ''),
  })).filter((row) => Object.values(row).some((value) => Array.isArray(value) ? value.length : value));
  const education = Array.from(editor.querySelectorAll('[data-cv-entry-kind="education"]')).map((row) => ({
    degree: row.querySelector('[data-cv-entry-field="degree"]')?.value.trim() || '',
    institution: row.querySelector('[data-cv-entry-field="institution"]')?.value.trim() || '',
    major: row.querySelector('[data-cv-entry-field="major"]')?.value.trim() || '',
    specialization: row.querySelector('[data-cv-entry-field="specialization"]')?.value.trim() || '',
    start_date: row.querySelector('[data-cv-entry-field="start_date"]')?.value.trim() || '',
    end_date: row.querySelector('[data-cv-entry-field="end_date"]')?.value.trim() || '',
    details: valueToLines(row.querySelector('[data-cv-entry-field="details"]')?.value || ''),
  })).filter((row) => Object.values(row).some((value) => Array.isArray(value) ? value.length : value));
  const skills = {};
  editor.querySelectorAll('[data-cv-skill-group]').forEach((field) => {
    const key = field.dataset.cvSkillGroup;
    const values = valueToLines(field.value || '');
    if (key && values.length) skills[key] = values;
  });
  return { experience, education, skills };
}

function applyEditorButtonAction(model, action, index) {
  const candidate = { ...(model.candidate || {}) };
  const cv = getCvEditorData(model);
  if (action === 'add-experience') cv.experience.push({ job_title: '', employer: '', start_date: '', end_date: '', job_description: [] });
  if (action === 'add-education') cv.education.push({ degree: '', institution: '', major: '', specialization: '', start_date: '', end_date: '', details: [] });
  if (action === 'remove-experience' && index >= 0) cv.experience.splice(index, 1);
  if (action === 'remove-education' && index >= 0) cv.education.splice(index, 1);
  return patchCvEditorData({ ...model, candidate }, cv);
}

function patchCvEditorData(model, cv) {
  const candidate = { ...(model.candidate || {}) };
  candidate.additional = upsertAdditionalValue(candidate.additional, 'cv.experience', 'Berufserfahrung (CV)', cv.experience || []);
  candidate.additional = upsertAdditionalValue(candidate.additional, 'cv.education', 'Ausbildung (CV)', cv.education || []);
  candidate.additional = upsertAdditionalValue(candidate.additional, 'cv.skills', 'Skills (CV)', cv.skills || {});
  candidate.skills = valueToLines(cv.skills?.Fachkenntnisse || cv.skills?.skills || []);
  candidate.languages = valueToLines(cv.skills?.Sprachkenntnisse || cv.skills?.languages || []).map((label) => ({ label }));
  return syncCandidateMetaAdditional({ ...model, candidate });
}

function syncCandidateMetaAdditional(model) {
  const candidate = { ...(model.candidate || {}) };
  const currentMeta = additionalValue(candidate.additional, 'cv.meta') || {};
  const nextMeta = {
    ...currentMeta,
    birthDate: candidate.birthDate || '',
    nationality: candidate.nationality || '',
    highestDegree: candidate.highestDegree || '',
    degree: candidate.degree || '',
    availabilityFrom: candidate.availability || currentMeta.availabilityFrom || '',
    languages: normalizeLanguageItems(candidate.languages || currentMeta.languages || []),
  };
  candidate.additional = upsertAdditionalValue(candidate.additional, 'cv.meta', 'Stammdaten (CV)', nextMeta);
  return { ...model, candidate };
}

function upsertAdditionalValue(additional, key, label, value) {
  const list = Array.isArray(additional) ? structuredClone(additional) : [];
  const index = list.findIndex((item) => item?.key === key);
  const entry = { key, label, type: 'json', value };
  if (index >= 0) list[index] = { ...list[index], ...entry };
  else list.push(entry);
  return list;
}

function normalizeCvSkillsObject(candidate) {
  const raw = additionalValue(candidate.additional, 'cv.skills') || candidate.cv?.skills || {};
  if (raw && typeof raw === 'object' && !Array.isArray(raw)) return structuredClone(raw);
  const skills = {};
  const fach = Array.isArray(candidate.skills) ? candidate.skills : Array.isArray(raw) ? raw : [];
  if (fach.length) skills.Fachkenntnisse = fach.map(labelOf).filter(Boolean);
  const languages = normalizeLanguageItems(candidate.languages || []);
  if (languages.length) {
    skills.Sprachkenntnisse = languages.map((item) => [item.label || item.code || item.language || item.name, item.level].filter(Boolean).join(' ')).filter(Boolean);
  }
  return skills;
}

function normalizeLanguageItems(value) {
  if (!Array.isArray(value)) return [];
  return value.map((item) => {
    if (typeof item === 'string') return { label: item };
    if (!item || typeof item !== 'object') return null;
    return { ...item, label: item.label || item.language || item.name || item.code || '' };
  }).filter((item) => item && String(item.label || item.code || '').trim());
}

function valueToLines(value) {
  if (Array.isArray(value)) return value.map(labelOf).map((item) => String(item || '').trim()).filter(Boolean);
  if (typeof value === 'string') return value.split(/\r?\n|;|•|\u2022/g).map((item) => item.trim()).filter(Boolean);
  if (value && typeof value === 'object') return Object.values(value).flatMap(valueToLines);
  return [];
}

function firstNonEmpty(...values) {
  for (const value of values) {
    const text = String(value || '').trim();
    if (text) return text;
  }
  return '';
}

function formatCvDisplayDate(value) {
  const text = String(value || '').trim();
  if (!text) return '';
  const iso = text.match(/^(\d{4})-(\d{2})-(\d{2})$/);
  if (iso) return `${iso[3]}.${iso[2]}.${iso[1]}`;
  return text;
}

function yearFromDateText(value) {
  return String(value || '').match(/\b(19\d{2}|20\d{2}|2100)\b/)?.[1] || '';
}

function bindEditablePrintFields(state) {
  state.host.querySelectorAll('[data-cv-edit-field]').forEach((node) => {
    node.addEventListener('blur', async () => {
      const path = node.dataset.cvEditField;
      const value = node.textContent.trim();
      await patchSelectedModel(state, (model) => setPath({ ...model }, path, value), { quiet: true });
    });
  });
}

async function importPdfs(state, files) {
  const pdfs = files.filter((file) => /^application\/pdf$/i.test(file.type) || /\.pdf$/i.test(file.name));
  if (!pdfs.length) throw new Error('Bitte mindestens eine PDF-Datei auswählen.');
  const failures = [];
  const imported = [];
  state.importing = true;
  setModuleBusy(state, true);
  try {
    for (const file of pdfs) {
      try {
        imported.push(await importPdf(state, file, { refresh: false, select: false }));
      } catch (error) {
        failures.push(`${file.name}: ${error?.message || error}`);
      }
    }
    if (imported.length) {
      const selectedId = imported[imported.length - 1].documentId;
      state.selectedId = selectedId;
      state.userCollapsed = false;
      state.lastSelectedId = '';
      state.lastSelectedPhase = '';
      state.viewMode = 'original';
      refresh(state);
      if (state.selectedId !== selectedId) {
        state.selectedId = selectedId;
        state.lastSelectedId = '';
        state.lastSelectedPhase = '';
        refresh(state);
      }
    }
  } finally {
    state.importing = false;
    setModuleBusy(state, false);
  }
  if (failures.length) {
    throw new Error(failures.join('\n'));
  }
}

// Das Original-PDF geht als Datei über api.files (Metadaten in `files`, Inhalt per sha256-Ref), nicht mehr als
// desktop_files/desktop_file_chunks-Dokumente einer fremden App. Profil und Version bleiben in documents/
// document_versions.
async function importPdf(state, file, options = {}) {
  if (!/^application\/pdf$/i.test(file.type) && !/\.pdf$/i.test(file.name)) {
    throw new Error('Bitte eine PDF-Datei auswählen.');
  }
  const bytes = new Uint8Array(await file.arrayBuffer());
  const sha = await sha256Hex(bytes);
  const now = Date.now();
  const documentId = `cv_${crypto.randomUUID()}`;
  const versionId = `${documentId}_v1`;
  const fileId = `cv_file_${documentId}`;
  const mimeType = file.type || 'application/pdf';
  const upload = state.api.files.put(file, { id: fileId, name: file.name, mime: mimeType });
  await upload.local;
  const fileRef = await upload.ref;
  // Upload-Ergebnis des Core: Ablehnung in die Zustandszone, nie warten.
  upload.result.then((result) => {
    if (result?.status === 'rejected') {
      notify(state, 'error', 'file.upload', tr('PDF-Upload abgelehnt', 'PDF upload rejected'), `${file.name}: ${result.reason || result.status}`);
    }
  }, () => {});
  const identity = candidateIdentityFromFilename(file.name);
  const title = identity.name;
  const model = createInitialModel({
    documentId,
    versionId,
    fileId,
    fileRef,
    filename: file.name,
    title,
    identity,
    sha,
    size: file.size,
    now,
  });

  await writeLocal(state.api.put('document_versions', {
    id: versionId,
    document_id: documentId,
    version: 1,
    source_kind: 'cv_pdf_upload',
    blob_id: fileRef,
    model_json: model,
    diagnostics: [],
    created_at_ms: now,
    updated_at_ms: now,
  }));

  await writeLocal(state.api.put('documents', {
    id: documentId,
    title,
    filename: file.name,
    mime_type: PROFILE_MIME,
    status: 'uploaded',
    document_type: 'cv_print_profile',
    owner_id: '',
    current_version_id: versionId,
    source_sha256: sha,
    page_count: 0,
    diagnostics_count: 0,
    linked_records: [
      { collection: 'files', id: fileId, ref: fileRef, role: 'source_pdf' },
    ],
    display_cache: {
      candidate_name: title,
      phase: 'uploaded',
      template: 'minimal',
    },
    index_text: `${title} ${file.name}`,
    is_deleted: false,
    created_at_ms: now,
    updated_at_ms: now,
  }));

  state.originalUrls.set(fileRef, URL.createObjectURL(file));
  if (options.select !== false) {
    state.selectedId = documentId;
    state.viewMode = 'original';
  }
  if (options.refresh !== false) refresh(state);
  return { documentId, fileId };
}

// Export/import the structured candidate PROFILES as JSON (honest and small):
// export downloads the models; import re-creates each profile as a document +
// version (no PDF bytes are carried — those stay in desktop_files). No command
// flow or schema changes; the print pipeline is untouched.
function exportCandidates(state) {
  const items = visibleItems(state);
  const source = items.length ? items : state.items;
  const payload = {
    format: 'ctox.cv_print_profile.export.v1',
    exported_at_ms: Date.now(),
    count: source.length,
    profiles: source.map((item) => ({
      id: item.record.id,
      title: item.record.title || displayCandidateName(item.model),
      model: item.model,
    })),
  };
  let url = '';
  try {
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
    url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = `cv-profiles-${source.length}.json`;
    anchor.rel = 'noopener';
    state.host.appendChild(anchor);
    anchor.click();
    anchor.remove();
    clearNotice(state, 'export');
  } catch (error) {
    notify(state, 'error', 'export', tr('Export fehlgeschlagen', 'Export failed'), String(error?.message || error));
  } finally {
    if (url) setTimeout(() => { try { URL.revokeObjectURL(url); } catch {} }, 4000);
  }
}

async function importCandidates(state, file) {
  let parsed;
  try {
    parsed = JSON.parse(await file.text());
  } catch {
    throw new Error(tr('Ungültige JSON-Datei.', 'Invalid JSON file.'));
  }
  const entries = Array.isArray(parsed) ? parsed
    : Array.isArray(parsed?.profiles) ? parsed.profiles
      : parsed?.model || parsed?.candidate || parsed?.workflow ? [parsed] : [];
  if (!entries.length) throw new Error(tr('Keine Profile in der Datei gefunden.', 'No profiles found in the file.'));
  let lastId = '';
  let count = 0;
  for (const entry of entries) {
    const id = await persistImportedProfile(state, entry);
    if (id) { lastId = id; count += 1; }
  }
  if (!count) throw new Error(tr('Keine gültigen Profile importiert.', 'No valid profiles imported.'));
  if (lastId) {
    state.selectedId = lastId;
    state.userCollapsed = false;
    state.lastSelectedId = '';
    state.lastSelectedPhase = '';
  }
  refresh(state);
}

async function persistImportedProfile(state, entry) {
  const source = entry?.model && typeof entry.model === 'object' ? entry.model : entry;
  if (!source || typeof source !== 'object' || Array.isArray(source)) return null;
  const now = Date.now();
  const documentId = `cv_${crypto.randomUUID()}`;
  const versionId = `${documentId}_v1`;
  const model = structuredClone(source);
  // Drop live tracking ids so the fresh profile does not adopt another CV's
  // command/task projection.
  model.workflow = { ...(model.workflow || {}) };
  delete model.workflow.command_id;
  delete model.workflow.task_id;
  delete model.workflow.tracking_id;
  model.source = { ...(model.source || {}) };
  delete model.source.desktop_file_id;
  delete model.source.generation_id;
  delete model.source.file_id;
  delete model.source.file_ref;
  const title = displayCandidateName(model) || tr('Importierter CV', 'Imported CV');
  const phase = workflowPhase(model);
  await writeLocal(state.api.put('document_versions', {
    id: versionId,
    document_id: documentId,
    version: 1,
    source_kind: 'cv_profile_import',
    blob_id: '',
    model_json: model,
    diagnostics: [],
    created_at_ms: now,
    updated_at_ms: now,
  }));
  await writeLocal(state.api.put('documents', {
    id: documentId,
    title,
    filename: String(model.source?.filename || ''),
    mime_type: PROFILE_MIME,
    status: phase,
    document_type: 'cv_print_profile',
    owner_id: '',
    current_version_id: versionId,
    source_sha256: '',
    page_count: 0,
    diagnostics_count: 0,
    linked_records: [],
    display_cache: {
      candidate_name: title,
      phase,
      template: normalizeTemplateId(model.print?.template),
    },
    index_text: title,
    is_deleted: false,
    created_at_ms: now,
    updated_at_ms: now,
  }));
  return documentId;
}

// Parsing als eigener Befehl `ctox.cv_print.apply_parse` (Handler: PDF-Text über cap.doc.extract, Agenten-Task
// mit App-Skill cv-print-builder/parser – der Handler wählt ihn, nicht die UI –, Rückschreiben der neuen Version
// über den Rückruf ctox.cv_print.apply_parse.done, Fehlschlag/Abbruch über .failed). Nie warten (R5): der Status kommt
// gepusht; failed → Zustandszone, die Phase „Fehler“ ergibt sich aus der Live-Projektion des Befehls.
async function startParsing(state, item) {
  const source = item.model?.source || {};
  if (!source.file_ref && !source.desktop_file_id) {
    throw new Error('CV-Quelle ist unvollständig: PDF-Datei fehlt.');
  }
  const now = Date.now();
  const chatId = `chat_cv_${item.record.id}`;
  const taskInstruction = buildParserTaskInstruction(item);
  const handle = submitCommand(state, PARSE_COMMAND, {
    title: `CV strukturieren: ${source.filename || item.record.filename}`,
    instruction: taskInstruction,
    chat_id: chatId,
    source_module: MODULE_ID,
    // Neue Profile: Datei-Ref aus api.files. Ältere Profile: desktop_files-ID + Generation (Handler liest sie).
    ...(source.desktop_file_id ? { source_file_id: source.desktop_file_id, generation_id: source.generation_id } : {}),
    ...(source.file_ref ? {
      source_file_ref: source.file_ref,
      source_file: { kind: 'file', ref: source.file_ref, file_id: source.file_id || '', name: source.filename || '', mime_type: 'application/pdf' },
    } : {}),
    filename: source.filename || item.record.filename,
    mime_type: 'application/pdf',
    size_bytes: source.size_bytes || 0,
    sha256: source.sha256 || '',
    document_id: item.record.id,
    version_id: item.version.id,
    record_snapshot: parserRecordSnapshot(item),
    expected_model_schema: 'ctox.cv_print_profile.v1',
    record_id: item.record.id,
    client_context: {
      surface: 'business-os-cv-print-builder',
      action: 'parse_cv_pdf',
      module: MODULE_ID,
      document_id: item.record.id,
      version_id: item.version.id,
      chat_id: chatId,
    },
  }, {
    // Lokal abgelehnter Befehl (h.local) = Versand gescheitert: wie v3 „dispatch_failed“ am Profil festhalten.
    onRejected: (error) => patchItemModel(state, item, (model) => ({
      ...model,
      workflow: {
        ...model.workflow,
        phase: 'error',
        chat_id: chatId,
        task_status: 'dispatch_failed',
        error: error?.message || String(error),
        updated_at_ms: Date.now(),
      },
    }), { documentStatus: 'error', displayPhase: 'error' }).catch(() => {}),
  });
  const commandId = handle.id;
  const markParsing = (model) => ({
    ...model,
    workflow: {
      ...model.workflow,
      phase: 'parsing',
      view_mode: 'original',
      chat_id: chatId,
      command_id: commandId,
      task_id: '',
      task_status: 'queued',
      error: '',
      updated_at_ms: now,
    },
  });
  item.model = markParsing(structuredClone(item.model));
  state.viewMode = 'original';
  state.lastSelectedId = item.record.id;
  state.lastSelectedPhase = 'parsing';
  render(state);
  await patchItemModel(state, item, markParsing, {
    documentStatus: 'parsing',
    displayPhase: 'parsing',
  });
  state.viewMode = 'original';
  refresh(state);
  return handle;
}

function submitCommand(state, type, payload, { onRejected } = {}) {
  const code = `${MODULE_ID}.command.${type}`;
  // record_id zusätzlich als Befehls-Option (§16.19 Nr. 8): der Handler liest command.record_id.
  const handle = state.api.command(type, payload, payload?.record_id ? { record_id: payload.record_id } : undefined);
  const off = handle.status.subscribe((status) => {
    if (status?.status === 'failed') {
      state.api.status.report({ level: 'error', code, text: `${tr('CV-Parsing fehlgeschlagen', 'CV parsing failed')}: ${status.error || '-'}`, ref: handle.id });
      state.statusCodes.add(code);
    } else if (status?.status === 'done') {
      state.api.status.clear(code);
    }
  });
  state.disposers.push(off);
  handle.local.catch((error) => {
    state.api.status.report({ level: 'error', code, text: `${tr('CV-Parsing abgelehnt', 'CV parsing rejected')}: ${String(error?.message || error?.code || error)}`, ref: handle.id });
    state.statusCodes.add(code);
    onRejected?.(error);
  });
  return handle;
}

function reparseCandidates(state) {
  return state.items.filter((item) => {
    const source = item.model?.source || {};
    return Boolean(item.record?.id && item.version?.id && (source.file_ref || source.desktop_file_id));
  });
}

async function reparseAllPdfs(state, candidates = reparseCandidates(state)) {
  const originalSelection = state.selectedId;
  const originalViewMode = state.viewMode;
  const failures = [];
  let started = 0;
  state.bulkParsing = true;
  setModuleBusy(state, true);
  try {
    for (const candidate of candidates) {
      const item = state.items.find((entry) => entry.record.id === candidate.record.id) || candidate;
      try {
        state.selectedId = item.record.id;
        await startParsing(state, item);
        started += 1;
      } catch (error) {
        failures.push(`${displayCandidateName(item.model)}: ${error?.message || error}`);
      }
    }
  } finally {
    state.bulkParsing = false;
    state.selectedId = state.items.some((item) => item.record.id === originalSelection)
      ? originalSelection
      : state.items[0]?.record.id || '';
    state.viewMode = originalViewMode || 'original';
    refresh(state);
    setModuleBusy(state, false);
  }
  if (failures.length) {
    notify(state, 'error', 'reparse', `${started}/${candidates.length} ${tr('Parser-Aufträge gesendet', 'parser commands sent')}`, failures.slice(0, 3).join('\n'));
    return;
  }
  clearNotice(state, 'reparse');
}

function parserRecordSnapshot(item) {
  const model = item.model || {};
  const candidate = model.candidate || {};
  return {
    document_id: item.record?.id || '',
    version_id: item.version?.id || '',
    source: {
      desktop_file_id: model.source?.desktop_file_id || '',
      generation_id: model.source?.generation_id || '',
      file_ref: model.source?.file_ref || '',
      filename: model.source?.filename || item.record?.filename || '',
      mime_type: model.source?.mime_type || 'application/pdf',
      size_bytes: model.source?.size_bytes || 0,
      sha256: model.source?.sha256 || '',
    },
    candidate: {
      name: displayCandidateName(model),
      firstName: candidate.firstName || '',
      lastName: candidate.lastName || '',
      currentRole: candidate.currentRole || '',
      location: candidate.location || '',
      availability: candidate.availability || '',
      highestDegree: candidate.highestDegree || '',
      degree: candidate.degree || '',
    },
    workflow: {
      phase: workflowPhase(model),
      template: normalizeTemplateId(model.print?.template || 'minimal'),
    },
    field_contract: {
      schema: 'ctox.cv_print_profile.v1',
      additional_keys: ['cv.experience', 'cv.education', 'cv.skills', 'cv.meta'],
      reference: 'NinjaWorkflowTool_Extension/find-job-for-candidate qualification profile',
    },
  };
}

function buildParserPrompt(item) {
  return buildParserTaskInstruction(item);
}

function buildParserTaskInstruction(item) {
  const source = item.model.source || {};
  return [
    'Skill: `cv-print-builder/parser`.',
    'Strukturiere den CTOX-vorextrahierten Abschnitt "CV PDF extracted text" als Ninja-kompatibles Qualifikationsprofil.', // v4-allow: text (Parser-Auftrag)
    'Ausfuehrung nur ueber CTOX desktop_files/desktop_file_chunks, RxDB/WebRTC und CTOX PDF-Stack.', // v4-allow: text (Parser-Auftrag)
    'Keine Tools, keine Shell, kein PDF erneut oeffnen, keine HTTP- oder Ninja-Services.', // v4-allow: text (Parser-Auftrag)
    'Antwort exakt als ein minifiziertes JSON-Objekt, kein Markdown, keine Analyse.',
    'Schema: {"schema":"ctox.cv_print_profile.v1","workflow":{"phase":"review","diagnostics":[]},',
    '"candidate":{name,firstName,lastName,currentRole,location,availability,email,phone,birthDate,nationality,highestDegree,degree,languages[],skills[],additional[]}}.',
    'candidate.additional muss exakt diese CV-Keys enthalten:',
    '- cv.experience: Array mit {job_title,employer,location,start_date,end_date,job_description[]}.',
    '- cv.education: Array mit {degree,institution,major,specialization,location,start_date,end_date,details[]}.',
    '- cv.skills: Objekt mit Gruppen, mindestens Fachkenntnisse[], Sprachkenntnisse[], Weitere Fähigkeiten[].', // v4-allow: text (Parser-Auftrag)
    '- cv.meta: Objekt mit birthDate,nationality,highestDegree,degree,availabilityFrom,languages[],source_filename.',
    'Alle erkennbaren Stationen behalten; nicht künstlich auf wenige Einträge kürzen.',
    'Fehlende Werte leer lassen, nichts erfinden.',
    `- document_id: ${item.record.id}`,
    `- version_id: ${item.version.id}`,
    `- desktop_file_id: ${source.desktop_file_id || ''}`,
    `- source_file_ref: ${source.file_ref || ''}`,
    `- filename: ${source.filename || item.record.filename || ''}`,
    'Der CTOX Writeback `ctox.cv_print.apply_parse` persistiert das JSON als neue document_versions-Version.', // v4-allow: text (Parser-Auftrag)
  ].join('\n');
}

async function approvePrint(state, item) {
  const approveModel = (model) => ({
    ...model,
    workflow: {
      ...model.workflow,
      phase: 'approved',
      view_mode: 'print',
      approved_at_ms: Date.now(),
      updated_at_ms: Date.now(),
    },
  });
  await patchItemModel(state, item, approveModel, {
    documentStatus: 'approved',
    displayPhase: 'approved',
  });
  const approvedModel = approveModel(structuredClone(item.model));
  item.model = approvedModel;
  if (item.version) {
    item.version.model_json = approvedModel;
    item.version.diagnostics = approvedModel.workflow?.diagnostics || item.version.diagnostics || [];
    item.version.updated_at_ms = Date.now();
  }
  item.record.status = 'approved';
  item.record.display_cache = {
    ...(item.record.display_cache || {}),
    phase: 'approved',
    candidate_name: displayCandidateName(approvedModel),
    template: normalizeTemplateId(approvedModel.print?.template || 'minimal'),
  };
  item.record.updated_at_ms = Date.now();
  state.viewMode = 'print';
  state.lastSelectedId = item.record.id;
  state.lastSelectedPhase = 'approved';
  render(state);
}

// Original-PDF lokal als Blob: neue Profile über api.files.get(ref), ältere Profile aus den lokal vorhandenen
// desktop_file_chunks (gewöhnliche Live-Abfrage statt Sync-Demand-Loader, R4).
async function ensureOriginalUrl(state, item) {
  const source = item.model.source || {};
  const key = sourceKey(item.model);
  if (!key || state.originalUrls.has(key)) return;
  let blob;
  if (source.file_ref) {
    blob = await state.api.files.get(source.file_ref);
  } else {
    const chunks = state.api.query('desktop_file_chunks', { selector: { file_id: source.desktop_file_id } }).get();
    blob = await readStoredFileFromChunks(chunks, source.desktop_file_id, 'application/pdf', {
      contentGenerationId: source.generation_id || '',
      contentHash: source.sha256 || '',
      contentHashScheme: source.sha256 ? CONTENT_HASH_SCHEME : '',
    });
  }
  if (!state.mounted) return;
  state.originalErrors.delete(key);
  state.originalUrls.set(key, URL.createObjectURL(blob));
}

function sourceKey(model) {
  const source = model?.source || {};
  return String(source.file_ref || source.desktop_file_id || '');
}

async function patchSelectedModel(state, updater, options = {}) {
  const item = getSelectedItem(state);
  if (!item) return;
  try {
    await patchItemModel(state, item, updater, options);
    clearNotice(state, 'write');
  } catch (error) {
    notify(state, 'error', 'write', tr('Änderung nicht gespeichert', 'Change not saved'), String(error?.message || error));
    return;
  }
  if (!options.quiet) refresh(state);
}

async function patchItemModel(state, item, updater, options = {}) {
  const now = Date.now();
  const versionJson = state.api.get('document_versions', item.version.id).get();
  if (!versionJson) throw new Error('CV Version konnte nicht aktualisiert werden.');
  const current = structuredClone(versionJson.model_json || item.model);
  const next = updater(current);
  next.workflow = {
    ...next.workflow,
    updated_at_ms: now,
  };
  await writeLocal(state.api.patch('document_versions', item.version.id, {
    model_json: next,
    diagnostics: next.workflow?.diagnostics || versionJson.diagnostics || item.version.diagnostics || [],
    updated_at_ms: now,
  }));
  const documentJson = state.api.get('documents', item.record.id).get();
  if (documentJson) {
    await writeLocal(state.api.patch('documents', item.record.id, {
      title: displayCandidateName(next),
      status: options.documentStatus || documentJson.status || workflowPhase(next),
      diagnostics_count: (next.workflow?.diagnostics || []).length,
      display_cache: {
        ...(documentJson.display_cache || {}),
        candidate_name: displayCandidateName(next),
        phase: options.displayPhase || workflowPhase(next),
        template: normalizeTemplateId(next.print?.template || 'minimal'),
      },
      index_text: buildIndexText(next),
      updated_at_ms: now,
    }));
  }
}

// Lokaler Commit eines Schreibvorgangs (WriteHandle.local). Das Core-Ergebnis wird nicht abgewartet; eine
// Ablehnung durch den Core erscheint als Zoneneintrag.
async function writeLocal(handle) {
  await handle.local;
  return handle;
}

function readAll(state, name) {
  try {
    return state.api.query(name, {}).get();
  } catch (error) {
    console.warn(`[${MODULE_ID}] ${name} query failed`, error);
    return [];
  }
}

function createInitialModel(input) {
  return {
    schema: 'ctox.cv_print_profile.v1',
    source: {
      file_id: input.fileId,
      file_ref: input.fileRef,
      filename: input.filename,
      mime_type: 'application/pdf',
      size_bytes: input.size,
      sha256: input.sha,
      imported_at_ms: input.now,
    },
    candidate: {
      id: input.documentId,
      name: input.title,
      firstName: input.identity?.firstName || '',
      lastName: input.identity?.lastName || '',
      currentRole: '',
      desiredPosition: '',
      location: '',
      availability: '',
      email: '',
      phone: '',
      highestDegree: '',
      degree: '',
      nationality: '',
      birthDate: '',
      languages: [],
      skills: [],
      softSkills: [],
      summary: '',
      additional: [
        { key: 'cv.education', label: 'Ausbildung (CV)', value: [] },
        { key: 'cv.experience', label: 'Berufserfahrung (CV)', value: [] },
        { key: 'cv.skills', label: 'Skills (CV)', value: {} },
        { key: 'cv.meta', label: 'Stammdaten (CV)', value: {} },
      ],
    },
    print: {
      template: 'minimal',
      anonymize: false,
      showLogo: true,
      logoDataUrl: '',
      preset: 'standard',
      overrides: {},
    },
    workflow: {
      phase: 'uploaded',
      view_mode: 'original',
      chat_id: '',
      command_id: '',
      task_id: '',
      task_status: '',
      diagnostics: [],
      created_at_ms: input.now,
      updated_at_ms: input.now,
    },
  };
}

function getSelectedItem(state) {
  return state.items.find((item) => item.record.id === state.selectedId) || null;
}

function workflowPhase(model) {
  return model?.workflow?.phase || 'uploaded';
}

function defaultViewMode(model) {
  const phase = workflowPhase(model);
  if (phase === 'review') return model.workflow?.view_mode || 'split';
  if (phase === 'approved') return 'print';
  return 'original';
}

function viewAllowed(model, view) {
  const phase = workflowPhase(model);
  if (phase === 'approved') return view === 'print';
  if (view === 'original') return true;
  return phase === 'review';
}

function actionForPhase(phase) {
  if (phase === 'review') return { title: tr('Freigeben', 'Approve'), icon: iconCheck() };
  if (phase === 'approved') return { title: tr('Drucken', 'Print'), icon: iconPrinter() };
  return { title: tr('Analysieren', 'Analyze'), icon: iconPlay() };
}

function phaseLabel(phase) {
  return ({
    uploaded: tr('PDF geladen', 'PDF uploaded'),
    parsing: tr('Parsing läuft', 'Parsing'), // v4-allow: label (Phase des CV-Datensatzes, fachlich)
    review: tr('Korrektur', 'Review'),
    approved: tr('Freigegeben', 'Approved'),
    error: tr('Fehler', 'Error'),
  })[phase] || phase;
}

function phaseFootnoteHtml(model) {
  const phase = workflowPhase(model);
  // Textdiät: keine Hinweis-/Anleitungssätze im Kopf; Phase steht im Kicker, Hauptaktion ist beschriftet.
  if (phase === 'uploaded') return '';
  if (phase === 'parsing') {
    // v4: 'syncing' (v3-Transport-Zeitüberschreitung) und „Flow wird angelegt“ entfallen – der Befehl hat ab dem
    // Absenden eine ID; ohne ID gibt es nichts anzuzeigen.
    const tracking = taskTracking(model);
    if (!tracking.taskId && !tracking.commandId) return '';
    const label = tracking.taskId ? shortTaskId(tracking.taskId) : shortTaskId(tracking.commandId);
    // v4-allow: label (Status des Parser-Tasks am Datensatz + Link in die CTOX-App, fachlich)
    return `${escapeHtml(tr('Task läuft', 'Task running'))} · <button class="cv-task-link" type="button" data-cv-open-task data-cv-task-id="${escapeAttr(tracking.taskId)}" data-cv-command-id="${escapeAttr(tracking.commandId)}" title="${escapeAttr([tracking.taskId, tracking.commandId].filter(Boolean).join(' · '))}">${escapeHtml(tr('Flow öffnen', 'Open flow'))} <span>${escapeHtml(label)}</span></button>`;
  }
  if (phase === 'review' || phase === 'approved') return escapeHtml(templateLabel(model.print?.template));
  if (phase === 'error') {
    const tracking = taskTracking(model);
    const error = clipText(model.workflow?.error || tr('Parser-Task fehlgeschlagen.', 'Parser task failed.'), 74);
    const link = tracking.taskId || tracking.commandId
      ? ` · <button class="cv-task-link is-error" type="button" data-cv-open-task data-cv-task-id="${escapeAttr(tracking.taskId)}" data-cv-command-id="${escapeAttr(tracking.commandId)}" title="${escapeAttr([tracking.taskId, tracking.commandId].filter(Boolean).join(' · '))}">${escapeHtml(tr('Flow öffnen', 'Open flow'))}</button>`
      : '';
    return `${escapeHtml(tr('Parsing fehlgeschlagen', 'Parsing failed'))}: ${escapeHtml(error)}${link}`;
  }
  return '';
}

function taskTracking(model) {
  return {
    taskId: String(model?.workflow?.task_id || model?.workflow?.tracking_id || '').trim(),
    commandId: String(model?.workflow?.command_id || '').trim(),
  };
}

function shortTaskId(value) {
  const text = String(value || '').trim();
  if (!text) return 'Task';
  const tail = text.includes('::') ? text.split('::').pop() : text;
  return tail.length > 12 ? tail.slice(0, 6) + '…' + tail.slice(-4) : tail;
}

function clipText(value, max) {
  const text = String(value || '').replace(/\s+/g, ' ').trim();
  return text.length > max ? text.slice(0, Math.max(0, max - 1)) + '…' : text;
}

// Flow öffnen: Sprung in die CTOX-App per api.shell.navigate (ctox liest task_id/command_id aus der Route),
// statt sessionStorage-Übergabe und Hash-Navigation des Elternfensters (R13).
function openCtoxTask(state, taskId, commandId) {
  if (!taskId && !commandId) return;
  state.api.shell.navigate('ctox', {
    ...(taskId ? { task_id: taskId } : {}),
    ...(commandId ? { command_id: commandId } : {}),
    source_module: MODULE_ID,
  });
}

function templateLabel(id) {
  const normalized = normalizeTemplateId(id);
  if (ACTIVE_LOCALE === 'en') return ({ minimal: 'Minimal', classic: 'Classic', modern: 'Modern' })[normalized] || 'Modern';
  return TEMPLATES.find((entry) => entry.id === normalized)?.label || 'Modern';
}

function normalizeTemplateId(id) {
  if (id === 'neutral') return 'minimal';
  if (id === 'klassisch') return 'classic';
  if (id === 'modern' || id === 'minimal' || id === 'classic') return id;
  return 'modern';
}

function displayCandidateName(model) {
  const candidate = model?.candidate || {};
  return candidate.name || [candidate.firstName, candidate.lastName].filter(Boolean).join(' ') || model?.source?.filename?.replace(/\.pdf$/i, '') || 'Neuer CV';
}

function titleFromFilename(filename) {
  return candidateIdentityFromFilename(filename).name;
}

function candidateIdentityFromFilename(filename) {
  const rawBase = String(filename || 'Neuer CV').replace(/\.[^.]+$/, '');
  const base = rawBase
    .replace(/\([^)]*\)/g, ' ')
    .replace(/[_-]+/g, ' ')
    .replace(/\b(?:lebenslauf|curriculum|vitae|cv|resume|jan|januar|feb|februar|mar|maerz|märz|apr|april|mai|jun|juni|jul|juli|aug|august|sep|sept|september|okt|oktober|nov|november|dez|dezember)\d*\b/gi, ' ')
    .replace(/\b(?:kein|upload|final|neu|new|copy|kopie|version|stand|profil)\b/gi, ' ')
    .replace(/\b\d{2,8}\b/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
  const words = base
    .split(/\s+/)
    .map((word) => word.replace(/^[^\p{L}]+|[^\p{L}]+$/gu, ''))
    .filter((word) => word && /\p{L}/u.test(word));
  const usable = words.length ? words.slice(0, 4) : ['Neuer', 'CV'];
  const ordered = maybeFlipLastFirst(usable);
  const display = ordered.map(formatNameToken).join(' ').trim() || 'Neuer CV';
  return {
    name: display,
    firstName: ordered[0] ? formatNameToken(ordered[0]) : '',
    lastName: ordered.length > 1 ? ordered.slice(1).map(formatNameToken).join(' ') : '',
  };
}

function maybeFlipLastFirst(words) {
  if (words.length !== 2) return words;
  const first = normalizeNameToken(words[0]);
  const second = normalizeNameToken(words[1]);
  if (COMMON_FIRST_NAMES.has(second) && !COMMON_FIRST_NAMES.has(first)) {
    return [words[1], words[0]];
  }
  return words;
}

function normalizeNameToken(value) {
  return String(value || '')
    .normalize('NFD')
    .replace(/\p{Diacritic}/gu, '')
    .toLowerCase();
}

function formatNameToken(value) {
  return String(value || '')
    .split(/([^\p{L}]+)/u)
    .map((part) => /\p{L}/u.test(part) ? part.charAt(0).toUpperCase() + part.slice(1) : part)
    .join('');
}

function initials(name) {
  const parts = String(name || 'CV').trim().split(/\s+/).filter(Boolean);
  return (parts[0]?.[0] || 'C') + (parts[1]?.[0] || parts[0]?.[1] || 'V');
}

function anonymizedName(name) {
  const parts = String(name || '').trim().split(/\s+/).filter(Boolean);
  if (!parts.length) return 'Kandidat';
  return `${parts[0]} ${parts[1] ? parts[1][0] + '.' : ''}`.trim();
}

function contactLine(candidate) {
  return [candidate.email, candidate.phone].filter(Boolean).join(' · ') || 'Kontakt nachtragen';
}

function renderLogo(model, options = {}) {
  const logo = model.print?.logoDataUrl;
  if (logo) return `<div class="cv-print-logo"><img alt="Logo" src="${escapeAttr(logo)}"></div>`;
  if (options.minimalLogo) return '';
  if (options.classicLogo) return '<div class="cv-print-logo cv-print-logo-classic">CV</div>';
  return '<div class="cv-print-logo">ctox</div>';
}

function normalizeArray(value) {
  if (!Array.isArray(value)) return [];
  return value.filter((item) => item !== null && item !== undefined && String(labelOf(item)).trim());
}

function normalizeTimeline(additional, key) {
  const value = additionalValue(additional, key);
  return Array.isArray(value) ? value : [];
}

function additionalValue(additional, key) {
  if (Array.isArray(additional)) {
    return additional.find((item) => item?.key === key)?.value;
  }
  if (additional && typeof additional === 'object') {
    return additional[key];
  }
  return null;
}

function renderTimeline(items, fallback) {
  if (!items.length) return `<li>${escapeHtml(fallback)}</li>`;
  return items.map((item) => {
    const title = item.title || item.position || item.degree || item.school || item.company || item.name || labelOf(item);
    const meta = [item.company, item.org, item.institution, item.location, item.from || item.start, item.to || item.end || item.until].filter(Boolean).join(' · ');
    const description = item.details || item.description || item.summary || '';
    return `<li><strong>${escapeHtml(title)}</strong>${meta ? `<span>${escapeHtml(meta)}</span>` : ''}${description ? `<p>${escapeHtml(description)}</p>` : ''}</li>`;
  }).join('');
}

function renderClassicTimeline(items, fallback) {
  if (!items.length) return `<li>${escapeHtml(fallback)}</li>`;
  return items.map((item) => {
    const title = item.title || item.position || item.degree || item.school || item.company || item.name || labelOf(item);
    const period = [item.from || item.start, item.to || item.end || item.until].filter(Boolean).join(' - ');
    const place = [item.company, item.org, item.institution, item.location].filter(Boolean).join(' · ');
    const description = item.details || item.description || item.summary || '';
    return `
      <li>
        <span class="cv-classic-period">${escapeHtml(period || 'Zeitraum')}</span>
        <span>
          <strong>${escapeHtml(title)}</strong>
          ${place ? `<em>${escapeHtml(place)}</em>` : ''}
          ${description ? `<p>${escapeHtml(description)}</p>` : ''}
        </span>
      </li>
    `;
  }).join('');
}

function renderTags(items, fallback) {
  return items.map((item) => `<li>${escapeHtml(labelOf(item))}</li>`).join('') || `<li>${escapeHtml(fallback)}</li>`;
}

function renderSimpleList(items, fallback) {
  return items.map((item) => `<li>${escapeHtml(labelOf(item))}</li>`).join('') || `<li>${escapeHtml(fallback)}</li>`;
}

function labelOf(item) {
  if (typeof item === 'string') return item;
  return item?.label || item?.name || item?.title || item?.skill || item?.language || JSON.stringify(item);
}

function editableAttr(editable, path) {
  return editable ? `contenteditable="true" spellcheck="false" data-cv-edit-field="${escapeAttr(path)}"` : '';
}

function setPath(model, path, value) {
  const parts = safePathParts(path);
  let target = model;
  for (const part of parts.slice(0, -1)) {
    const current = ownValue(target, part);
    const next = current && typeof current === 'object' ? { ...current } : {};
    defineOwnValue(target, part, next);
    target = next;
  }
  defineOwnValue(target, parts.at(-1), value);
  return model;
}

function safePathParts(path) {
  const parts = String(path || '').split('.').filter(Boolean);
  if (!parts.length || parts.some(isUnsafePathSegment)) {
    throw new Error('editable field path contains unsafe prototype segment');
  }
  return parts;
}

function isUnsafePathSegment(segment) {
  return segment === '__proto__' || segment === 'prototype' || segment === 'constructor';
}

function ownValue(object, key) {
  if (!object || typeof object !== 'object' || !Object.hasOwn(object, key)) return undefined;
  return Object.getOwnPropertyDescriptor(object, key)?.value;
}

function defineOwnValue(object, key, value) {
  Object.defineProperty(object, key, {
    value,
    enumerable: true,
    configurable: true,
    writable: true,
  });
}

function buildIndexText(model) {
  const candidate = model.candidate || {};
  return [
    displayCandidateName(model),
    candidate.currentRole,
    candidate.location,
    candidate.availability,
    candidate.summary,
    ...normalizeArray(candidate.skills).map(labelOf),
    ...normalizeArray(candidate.languages).map(labelOf),
  ].filter(Boolean).join(' ').slice(0, 20000);
}

async function sha256Hex(bytes) {
  const hash = await crypto.subtle.digest('SHA-256', bytes);
  return Array.from(new Uint8Array(hash)).map((value) => value.toString(16).padStart(2, '0')).join('');
}


function fileToDataUrl(file) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result || ''));
    reader.onerror = () => reject(reader.error || new Error('Datei konnte nicht gelesen werden.'));
    reader.readAsDataURL(file);
  });
}

// Meldungen gehen in die Zustandszone der Shell (R11), mit stabilem Code je Bereich, damit clear greift.
function notify(state, type, area, title, message) {
  const code = `${MODULE_ID}.${area}`;
  const level = type === 'error' ? 'error' : type === 'warn' ? 'warn' : 'info';
  state.api.status.report({ level, code, text: message ? `${title}: ${message}` : title });
  state.statusCodes.add(code);
}

function clearNotice(state, area) {
  const code = `${MODULE_ID}.${area}`;
  if (!state.statusCodes.has(code)) return;
  state.statusCodes.delete(code);
  state.api.status.clear(code);
}

// Druck der freigegebenen Druckansicht im App-Fenster: verstecktes iframe in root mit den Stilen des Dokuments
// und nur dem Druckbogen (statt window.print() über die ganze Shell).
function printApprovedSheet(state, item) {
  const sheet = state.host.querySelector('.cv-print-sheet');
  const html = sheet ? sheet.outerHTML : renderPrintSheet(item.model);
  const frame = document.createElement('iframe');
  frame.setAttribute('aria-hidden', 'true');
  frame.tabIndex = -1;
  frame.style.cssText = 'position:absolute;width:0;height:0;border:0;visibility:hidden';
  state.root.appendChild(frame);
  const doc = frame.contentDocument;
  if (!doc) {
    frame.remove();
    return;
  }
  const styles = [...document.querySelectorAll('link[rel="stylesheet"], style')]
    .map((node) => (node.tagName === 'LINK' ? `<link rel="stylesheet" href="${escapeAttr(node.href)}">` : node.outerHTML))
    .join('');
  doc.open();
  doc.write(`<!doctype html><html lang="${ACTIVE_LOCALE}"><head><meta charset="utf-8"><title>${escapeHtml(displayCandidateName(item.model))}</title>${styles}<style>@page{size:A4;margin:0}html,body{margin:0;background:#fff}</style></head><body><div data-app="${MODULE_ID}"><div class="cv-print-frame">${html}</div></div></body></html>`);
  doc.close();
  setTimeout(() => {
    try { frame.contentWindow?.print(); } catch { /* ignore */ }
    setTimeout(() => frame.remove(), 1000);
  }, 250);
}

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

function escapeAttr(value) {
  return escapeHtml(value).replace(/`/g, '&#096;');
}

function iconPlay() {
  return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M8 5v14l11-7z"/></svg>';
}

function iconCheck() {
  return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m5 12 4 4L19 6"/></svg>';
}

function iconPrinter() {
  return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M7 8V4h10v4"/><path d="M7 17H5a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2h-2"/><path d="M7 14h10v6H7z"/></svg>';
}

function iconImage() {
  return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="4" y="5" width="16" height="14" rx="2"/><path d="m8 15 3-3 2 2 2-3 3 4"/><circle cx="9" cy="9" r="1"/></svg>';
}

function iconEyeOff() {
  return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m3 3 18 18"/><path d="M10.6 10.6a2 2 0 0 0 2.8 2.8"/><path d="M9.5 5.3A10.4 10.4 0 0 1 12 5c5 0 8.5 4.5 9.5 7a12 12 0 0 1-3 4.1"/><path d="M6.2 6.8A12 12 0 0 0 2.5 12c1 2.5 4.5 7 9.5 7a10 10 0 0 0 4-.8"/></svg>';
}

function iconCollapse() {
  return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M13 5l-6 7 6 7"/><path d="M19 5v14"/></svg>';
}
