// App-API 4.0 (Shell v4): mount(root, api, {restored}) -> {unmount, snapshot}. Protokoll: V4-PORT.md.
import { autoWirePaneGrammar } from 'ctox:shared/pane-grammar.js';
// Eingebettete Texte (relativer Modul-Import, kein HTTP-Ladeweg); api.shell.t('creator.<key>') übersteuert.
import messagesDe from './locales/de.json' with { type: 'json' };
import messagesEn from './locales/en.json' with { type: 'json' };

const state = {
  api: null,
  root: null,
  lang: 'de',
  disposers: [],
  t: (key, fallback) => fallback ?? key,
  appId: '',
  appTitle: '',
  appDesc: '',
  appCategory: '',
  appArchetype: 'record-workbench',
  appLayout: '',
  appCollections: [],
  inspirationUrls: [],
  appVersion: '0.1.0',
  installedApps: [],
  creatorRequests: [],
  importedRequests: [],
  selectedLibraryId: null,
  selectedLibraryKind: null,
  composer: null,
  isDeploying: false
};

// Ein-Knopf-Umschalter (Betreiber-Direktive 31.08.): Icon und Beschriftung
// zeigen immer die Ansicht, in die der Klick wechselt - nie den Ist-Zustand.
const VIEW_ICON = {
  cards: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="4" y="4" width="16" height="7" rx="1.5"/><rect x="4" y="14" width="16" height="7" rx="1.5"/></svg>',
  list: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="4" y1="6" x2="20" y2="6"/><line x1="4" y1="12" x2="20" y2="12"/><line x1="4" y1="18" x2="20" y2="18"/></svg>',
};
const VIEW_ACTION_KEY = { cards: 'libViewToCards', list: 'libViewToList' };
const VIEW_ACTION_FALLBACK = { cards: 'Als Karten anzeigen', list: 'Als Liste anzeigen' };

export const CREATOR_PROMPT_EXAMPLES = Object.freeze([
  {
    id: 'crm',
    de: { title: 'Kunden & Kontakte', hint: 'CRM mit Suche, Status und Aktivitäten', prompt: 'Erstelle eine schlanke CRM-App für Kunden und Kontakte. Links stehen Suche, Statusfilter und Kundenliste. In der Hauptansicht sehe und bearbeite ich Kontaktdaten, Notizen, nächste Schritte und den aktuellen Status. Ergänze eine Aktivitätschronik und eine klare Aktion „Follow-up an CTOX delegieren“.' },
    en: { title: 'Customers & contacts', hint: 'CRM with search, status, and activity', prompt: 'Create a focused CRM app for customers and contacts. Put search, status filters, and the customer list on the left. The main view should show and edit contact details, notes, next steps, and current status. Add an activity timeline and one clear action to delegate a follow-up to CTOX.' },
  },
  {
    id: 'support',
    de: { title: 'Support Desk', hint: 'Tickets priorisieren und bearbeiten', prompt: 'Erstelle eine Support-Desk-App. Links brauche ich Ticketliste, Suche sowie Filter für offen, wartend und gelöst. Die Hauptansicht zeigt Beschreibung, Kunde, Priorität, Verlauf und Antwortentwurf. Ergänze Statuswechsel, Zuweisung und eine Aktion, mit der CTOX eine Antwort vorbereitet.' },
    en: { title: 'Support desk', hint: 'Prioritize and resolve tickets', prompt: 'Create a support desk app. The left side needs a ticket list, search, and filters for open, on hold, and resolved. The main view should show description, customer, priority, history, and reply draft. Add status changes, assignment, and an action for CTOX to prepare a reply.' },
  },
  {
    id: 'inventory',
    de: { title: 'Lager & Bestand', hint: 'Artikel, Mengen und Warnungen', prompt: 'Erstelle eine Lager-App für Artikel und Bestände. Links stehen Artikelsuche, Kategorien und Filter für niedrigen Bestand. In der Hauptansicht sehe ich Bestand, Lagerort, Mindestmenge und letzte Bewegungen. Ermögliche Zu- und Abgänge, CSV-Import und eine Warnung bei Unterschreitung.' },
    en: { title: 'Inventory', hint: 'Items, quantities, and alerts', prompt: 'Create an inventory app for items and stock. Put item search, categories, and low-stock filters on the left. The main view should show stock, location, minimum quantity, and recent movements. Support stock adjustments, CSV import, and low-stock warnings.' },
  },
  {
    id: 'projects',
    de: { title: 'Projekt-Cockpit', hint: 'Aufgaben, Termine und Fortschritt', prompt: 'Erstelle ein Projekt-Cockpit. Links stehen Projekte und Filter nach Status. Die Hauptansicht zeigt Aufgaben, Meilensteine, Verantwortliche, Fälligkeiten und Fortschritt. Ergänze eine kompakte Timeline, überfällige Hinweise und eine Aktion, mit der CTOX die nächsten Schritte plant.' },
    en: { title: 'Project cockpit', hint: 'Tasks, dates, and progress', prompt: 'Create a project cockpit. Put projects and status filters on the left. The main view should show tasks, milestones, owners, due dates, and progress. Add a compact timeline, overdue indicators, and an action for CTOX to plan the next steps.' },
  },
  {
    id: 'recruiting',
    de: { title: 'Bewerber-Pipeline', hint: 'Kandidaten durch Phasen führen', prompt: 'Erstelle eine Recruiting-App mit Kandidaten-Pipeline. Links stehen Stellen, Suche und Phasenfilter. Die Hauptansicht zeigt Profil, Bewertung, Notizen, Dokumente und Gesprächsverlauf. Kandidaten lassen sich in die nächste Phase bewegen; CTOX kann einen Interviewleitfaden erstellen.' },
    en: { title: 'Hiring pipeline', hint: 'Move candidates through stages', prompt: 'Create a recruiting app with a candidate pipeline. Put jobs, search, and stage filters on the left. The main view should show profile, score, notes, documents, and interview history. Candidates can move to the next stage, and CTOX can prepare an interview guide.' },
  },
  {
    id: 'expenses',
    de: { title: 'Ausgaben-Freigabe', hint: 'Belege prüfen und genehmigen', prompt: 'Erstelle eine App zur Ausgabenfreigabe. Links stehen offene, genehmigte und abgelehnte Belege mit Suche. Die Hauptansicht zeigt Betrag, Kategorie, Kostenstelle, Belegvorschau und Prüfhinweise. Ergänze Genehmigen, Ablehnen und Rückfrage sowie einen vollständigen Entscheidungsverlauf.' },
    en: { title: 'Expense approval', hint: 'Review and approve receipts', prompt: 'Create an expense approval app. Put searchable open, approved, and rejected receipts on the left. The main view should show amount, category, cost center, receipt preview, and review notes. Add approve, reject, and request-info actions with a complete decision history.' },
  },
  {
    id: 'content',
    de: { title: 'Content-Kalender', hint: 'Ideen planen und veröffentlichen', prompt: 'Erstelle einen Content-Kalender. Links stehen Kanäle, Suche und Statusfilter. Die Hauptansicht zeigt Inhalt, Veröffentlichungsdatum, Verantwortliche, Assets und Freigabestatus. Ergänze Kalender- und Listenansicht sowie eine Aktion, mit der CTOX einen Entwurf oder Varianten erstellt.' },
    en: { title: 'Content calendar', hint: 'Plan ideas and publishing', prompt: 'Create a content calendar. Put channels, search, and status filters on the left. The main view should show content, publish date, owner, assets, and approval status. Add calendar and list views plus an action for CTOX to draft content or variants.' },
  },
  {
    id: 'contracts',
    de: { title: 'Verträge & Fristen', hint: 'Dokumente und Termine im Blick', prompt: 'Erstelle eine Vertragsverwaltung. Links stehen Vertragspartner, Suche und Filter für aktive, auslaufende und beendete Verträge. Die Hauptansicht zeigt Eckdaten, Dokumente, Laufzeit, Kündigungsfrist und Verantwortliche. Warne vor Fristen und lasse CTOX eine Verlängerung oder Kündigung vorbereiten.' },
    en: { title: 'Contracts & deadlines', hint: 'Track documents and dates', prompt: 'Create a contract management app. Put counterparties, search, and filters for active, expiring, and ended contracts on the left. The main view should show key terms, documents, duration, notice period, and owner. Warn about deadlines and let CTOX prepare a renewal or termination.' },
  },
  {
    id: 'field-service',
    de: { title: 'Außendienst-Aufträge', hint: 'Einsätze planen und dokumentieren', prompt: 'Erstelle eine Außendienst-App. Links stehen heutige, geplante und abgeschlossene Einsätze mit Suche. Die Hauptansicht zeigt Kunde, Adresse, Termin, Checkliste, Fotos und Arbeitsbericht. Ergänze Statuswechsel, Zeiterfassung und eine mobile, schmale Darstellung.' },
    en: { title: 'Field service', hint: 'Plan and document visits', prompt: 'Create a field service app. Put today’s, planned, and completed visits with search on the left. The main view should show customer, address, appointment, checklist, photos, and work report. Add status changes, time tracking, and a compact mobile layout.' },
  },
  {
    id: 'research',
    de: { title: 'Research-Bibliothek', hint: 'Quellen sammeln und bewerten', prompt: 'Erstelle eine Research-Bibliothek. Links stehen Themen, Suche und Quellenfilter. Die Hauptansicht zeigt Zusammenfassung, Quelle, Autor, Datum, Tags, Vertrauensbewertung und Notizen. Ergänze URL-Import, Duplikaterkennung und eine Aktion, mit der CTOX Quellen zusammenfasst und vergleicht.' },
    en: { title: 'Research library', hint: 'Collect and evaluate sources', prompt: 'Create a research library. Put topics, search, and source filters on the left. The main view should show summary, source, author, date, tags, confidence score, and notes. Add URL import, duplicate detection, and an action for CTOX to summarize and compare sources.' },
  },
]);

export function normalizeModuleId(value) {
  return String(value || '')
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9-]/g, '-')
    .replace(/-+/g, '-')
    .replace(/^-|-$/g, '');
}

export function normalizeCollectionName(value) {
  return String(value || '')
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_]/g, '_')
    .replace(/_+/g, '_')
    .replace(/^_|_$/g, '');
}

export function deriveModuleIdFromRequest(request, now = Date.now()) {
  const words = String(request || '')
    .replace(/[^\p{Letter}\p{Number}\s-]/gu, ' ')
    .split(/\s+/)
    .map((word) => word.trim())
    .filter((word) => word.length > 2)
    .slice(0, 5)
    .join(' ');
  const slug = normalizeModuleId(words).slice(0, 60).replace(/-+$/g, '');
  return slug || `business-app-${now}`;
}

export function titleFromModuleId(moduleId) {
  const text = String(moduleId || '').replace(/[-_]+/g, ' ').trim();
  if (!text) return 'Business OS App';
  return text.replace(/\b\w/g, (match) => match.toUpperCase());
}

export function validateCreatorSpec({ appId, appTitle, appDesc, appCollections }) {
  const errors = [];
  if (String(appId || '').trim() && !normalizeModuleId(appId)) errors.push('Modul-ID ist ungültig.');
  if (String(appTitle || '').length > 120) errors.push('Titel ist zu lang.');
  if (String(appDesc || '').length > 500) errors.push('Beschreibung ist zu lang.');
  const collections = Array.isArray(appCollections) ? appCollections.map(normalizeCollectionName).filter(Boolean) : [];
  if (collections.length > 6) errors.push('Zu viele Datentabellen als Vorgabe.');
  return errors;
}

export function normalizeInspirationUrl(value) {
  const text = String(value || '').trim();
  if (!text) return '';
  try {
    const url = new URL(text);
    if (!['http:', 'https:'].includes(url.protocol)) return '';
    url.hash = '';
    return url.toString();
  } catch {
    return '';
  }
}

export function computeCreatorActionState({ request, appId, appTitle, appDesc, appCollections, isDeploying = false, lang = 'de' }) {
  const requestText = String(request || '').trim();
  const validationErrors = validateCreatorSpec({ appId, appTitle, appDesc, appCollections });
  const hasRequest = Boolean(requestText);
  const isBusy = Boolean(isDeploying);
  const deployReady = hasRequest && validationErrors.length === 0 && !isBusy;
  const en = lang === 'en';
  let diagnostic = en ? 'Job missing. Describe the app first.' : 'Auftrag fehlt. Beschreibe zuerst die App.';
  // v4: kein Lauf-/Bereit-Zustandstext (Befehle warten nicht; Zustand nur in der Zustandszone).
  if (isDeploying || deployReady) diagnostic = '';
  else if (hasRequest && validationErrors.length > 0) diagnostic = validationErrors[0];

  return { hasRequest, validationErrors, deployReady, diagnostic };
}

// v4 (ctox-platform-Schema business_apps: id, app_id, title, active_ver, layout, provenance, …): Eigenbau = App ohne
// Release-Basis (provenance.base_release leer), also auf dieser Instanz angelegt; Kern-Apps aus dem Release tragen sie.
// V2-Katalogzeilen (entry/source) bleiben für die V2-Linie unverändert.
function isV4AppRow(mod) {
  return mod && typeof mod === 'object' && !('entry' in mod) && !('source' in mod) && !('store' in mod)
    && ('active_ver' in mod || 'app_id' in mod || 'provenance' in mod);
}

export function normalizeCreatorInstalledApps(catalog) {
  const modules = Array.isArray(catalog?.modules) ? catalog.modules : [];
  return modules
    .filter((mod) => {
      if (!mod?.id || mod.id === 'creator') return false;
      if (isV4AppRow(mod)) return !mod.provenance?.base_release && mod.id !== 'shell' && mod.id !== 'ctox-platform' && !String(mod.id).startsWith('lib:');
      const entry = String(mod?.entry || '');
      const source = String(mod?.source || mod?.store?.distribution || '').toLowerCase();
      return entry.startsWith('installed-modules/')
          || source === 'installed'
          || source.includes('installed-module');
    })
    .map((mod) => ({
      id: normalizeModuleId(mod.id),
      title: String(mod.title || mod.id),
      description: String(mod.description || mod.store?.summary || ''),
      category: String(mod.category || mod.layout?.category || mod.source || 'Custom'),
      version: String(mod.version || mod.active_ver || '0.1.0'),
      entry: String(mod.entry || ''),
    }))
    .filter((mod) => mod.id)
    .sort((a, b) => a.title.localeCompare(b.title, 'de'));
}

export function normalizeCreatorRequestSuggestions(commands, limit = 5) {
  const items = Array.isArray(commands) ? commands : [];
  return items
    .filter((command) => {
      const payload = command?.payload || {};
      const type = String(command?.command_type || command?.type || '');
      const module = String(command?.module || payload.module || '');
      const source = String(command?.client_context?.source || payload.source || '');
      return module === 'creator'
        || source === 'business-os-creator'
        || type === 'ctox.business_os.app.modify'
        || type === 'business_os.chat.task';
    })
    .map((command) => {
      const payload = command?.payload || {};
      const context = payload.context || {};
      const request = String(payload.instruction || payload.request || payload.user_message || command?.title || '').trim();
      return {
        id: String(command?.id || command?.command_id || `${Date.now()}-${request}`),
        title: String(payload.title || context.app_title || command?.title || 'CTOX App-Auftrag'),
        request,
        status: String(command?.status || 'pending'),
        updated_at_ms: Number(command?.updated_at_ms || command?.created_at_ms || 0),
      };
    })
    .filter((item) => item.request)
    .sort((a, b) => b.updated_at_ms - a.updated_at_ms)
    .slice(0, limit);
}

// The two real views of the left column, counted (zeros included). Apps are
// installed custom apps; Aufträge are recent CTOX app requests plus any locally
// imported request drafts.
export function creatorLibraryCounts(apps, requests) {
  return {
    apps: Array.isArray(apps) ? apps.length : 0,
    auftraege: Array.isArray(requests) ? requests.length : 0,
  };
}

// Apply the shell-wired grammar state (band + search + request status) to the
// left column. Returns the tagged items for the active band. The status filter
// only constrains Aufträge (apps carry no status).
export function filterCreatorLibrary({ apps = [], requests = [] } = {}, { band = 'apps', search = '', status = 'all' } = {}) {
  const needle = String(search || '').trim().toLowerCase();
  if (band === 'auftraege') {
    return (Array.isArray(requests) ? requests : [])
      .filter((r) => {
        if (status && status !== 'all' && String(r.status || '') !== status) return false;
        if (needle && ![r.title, r.request, r.status, r.id].filter(Boolean).join(' ').toLowerCase().includes(needle)) return false;
        return true;
      })
      .map((r) => ({ kind: 'request', ...r }));
  }
  return (Array.isArray(apps) ? apps : [])
    .filter((a) => {
      if (needle && ![a.title, a.description, a.category, a.id].filter(Boolean).join(' ').toLowerCase().includes(needle)) return false;
      return true;
    })
    .map((a) => ({ kind: 'app', ...a }));
}

// Distinct request statuses present in the data — used to populate the tray
// status filter with only statuses that actually exist (no dead options).
export function creatorRequestStatuses(requests) {
  const seen = [];
  for (const r of Array.isArray(requests) ? requests : []) {
    const status = String(r?.status || '').trim();
    if (status && !seen.includes(status)) seen.push(status);
  }
  return seen;
}

// Normalize a raw imported request into a local draft. Local drafts render in
// the Aufträge band and can be adopted into the composer, but are never
// dispatched as CTOX commands — they carry status 'lokal' and imported: true.
export function prepareCreatorRequestImport(raw, index = 0) {
  const src = raw && typeof raw === 'object' ? raw : {};
  const request = String(src.request || src.instruction || src.prompt || '').trim();
  if (!request) return null;
  return {
    id: String(src.id || `import-${index}`),
    title: String(src.title || 'Importierter Auftrag').trim() || 'Importierter Auftrag',
    request,
    status: 'lokal',
    imported: true,
    updated_at_ms: Number(src.updated_at_ms) || 0,
  };
}

export async function mount(root, api, { restored } = {}) {
  // Abos gehören dieser Montage (PORTING.md §4 Falle 1).
  const disposers = [];
  state.disposers = disposers;
  state.api = api;
  state.root = root;
  state.appId = '';
  state.appTitle = '';
  state.appDesc = '';
  state.appCategory = '';
  state.appArchetype = 'record-workbench';
  state.appLayout = 'windowed';
  state.appCollections = [];
  state.inspirationUrls = [];
  state.installedApps = [];
  state.creatorRequests = [];
  state.selectedLibraryId = null;
  state.selectedLibraryKind = null;
  if (!root.dataset.app) root.dataset.app = api.app?.id || 'creator';
  state.lang = String(api.shell.locale.get() || '').toLowerCase().startsWith('en') ? 'en' : 'de';
  state.t = creatorTranslator();

  // Markup relativ (Service Worker liefert versioniert, §16.15); CSS lädt die Shell aus dem Manifest.
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  root.innerHTML = html;
  applyCreatorTranslations(root, state.t);
  wireUi(root);
  wireLibrary(root);
  autoWirePaneGrammar(root);
  api.shell.setTitle(state.t('title', 'App Creator'));
  applyRestored(root, restored);
  renderLibrary(root);

  // App-Katalog = je App ein Dokument in business_apps (§16.14); App-Aufträge = business_commands.
  disposers.push(api.query('business_apps', { sort: [{ title: 'asc' }] }).subscribe((rows) => {
    state.installedApps = normalizeCreatorInstalledApps({ modules: Array.isArray(rows) ? rows : [] });
    renderLibrary(root);
  }));
  disposers.push(api.query('business_commands', {}).subscribe((rows) => {
    state.creatorRequests = normalizeCreatorRequestSuggestions(Array.isArray(rows) ? rows : []);
    renderLibrary(root);
  }));
  disposers.push(api.shell.locale.subscribe((locale) => {
    const lang = String(locale || '').toLowerCase().startsWith('en') ? 'en' : 'de';
    if (lang === state.lang) return;
    state.lang = lang;
    state.t = creatorTranslator();
    applyCreatorTranslations(root, state.t);
    renderLibrary(root);
  }));
  // Upgrade-Einstieg über die Route der Shell (Parameter upgrade=<app-id>).
  disposers.push(api.shell.onRoute((event) => {
    const upgrade = event?.params?.upgrade;
    if (upgrade) void state.composer?.prefillUpgrade(String(upgrade));
  }));

  return {
    unmount() {
      for (const dispose of disposers.splice(0)) {
        try { dispose(); } catch (error) { console.warn('[creator] unsubscribe failed', error); }
      }
      root.replaceChildren();
      if (state.root === root) { state.api = null; state.root = null; }
    },
    snapshot: creatorSnapshot,
  };
}

function creatorTranslator() {
  const messages = { ...messagesDe, ...(state.lang === 'en' ? messagesEn : {}) };
  return (key, fallback) => {
    let value = messages[key] ?? fallback ?? key;
    const shellT = state.api?.shell?.t;
    if (typeof shellT === 'function') {
      const scoped = `creator.${key}`;
      const v = shellT.call(state.api.shell, scoped);
      if (typeof v === 'string' && v && v !== scoped) value = v;
    }
    return value;
  };
}

const SNAPSHOT_FIELDS = Object.freeze({
  request: '#app-request-input', appId: '#input-app-id', appTitle: '#input-app-title', appDesc: '#input-app-desc',
  category: '#select-app-category', archetype: '#select-app-archetype', layout: '#select-app-layout',
  newCollection: '#input-new-collection', inspiration: '#creator-inspiration-url',
});

// Snapshot für den Live-Remount: der ganze offene Auftrag (Formular, Datentabellen, Inspirations-URLs) und Auswahl.
function creatorSnapshot() {
  const snap = { v: 1, fields: {} };
  for (const [key, sel] of Object.entries(SNAPSHOT_FIELDS)) {
    const el = state.root?.querySelector(sel);
    if (el) snap.fields[key] = el.value;
  }
  snap.appCollections = [...state.appCollections];
  snap.inspirationUrls = [...state.inspirationUrls];
  snap.appVersion = state.appVersion;
  snap.selectedLibraryId = state.selectedLibraryId;
  snap.selectedLibraryKind = state.selectedLibraryKind;
  snap.advancedOpen = Boolean(state.root?.querySelector('details.creator-options')?.open);
  return snap;
}

function applyRestored(root, restored) {
  if (!restored || typeof restored !== 'object') return;
  const fields = restored.fields && typeof restored.fields === 'object' ? restored.fields : {};
  for (const [key, sel] of Object.entries(SNAPSHOT_FIELDS)) {
    const el = root.querySelector(sel);
    if (el && typeof fields[key] === 'string') el.value = fields[key];
  }
  if (Array.isArray(restored.appCollections)) state.appCollections = restored.appCollections.map(String);
  if (Array.isArray(restored.inspirationUrls)) state.inspirationUrls = restored.inspirationUrls.map(String);
  if (typeof restored.appVersion === 'string') state.appVersion = restored.appVersion;
  if (typeof restored.selectedLibraryId === 'string') state.selectedLibraryId = restored.selectedLibraryId;
  if (typeof restored.selectedLibraryKind === 'string') state.selectedLibraryKind = restored.selectedLibraryKind;
  const advanced = root.querySelector('details.creator-options');
  if (advanced && restored.advancedOpen) advanced.open = true;
  state.composer?.restore?.();
}

// Merge server-synced requests with locally imported drafts (imports never
// masquerade as dispatched requests; a server request with the same id wins).
function allCreatorRequests() {
  const server = Array.isArray(state.creatorRequests) ? state.creatorRequests : [];
  const imported = (Array.isArray(state.importedRequests) ? state.importedRequests : [])
    .filter((i) => !server.some((r) => r.id === i.id));
  return [...server, ...imported];
}

function libraryRail(host) {
  return host?.querySelector?.('.creator-library') || null;
}

// Read the SHELL-wired grammar state straight from the pane DOM (band default
// is Apps). Mirrors the consent/reports reference modules. Die Ansicht steht
// auf dem EINEN Umschalter (data-pg-view = AKTUELLE Ansicht), nicht auf einem
// aria-pressed-Paar.
function readLibraryGrammar(rail) {
  return {
    search: (rail?.querySelector('[data-pg-search]')?.value || '').trim().toLowerCase(),
    view: rail?.querySelector('[data-creator-view-toggle]')?.dataset.pgView === 'list' ? 'list' : 'cards',
    band: rail?.querySelector('[data-pg-band][aria-selected="true"]')?.dataset.pgBand || 'apps',
    status: rail?.querySelector('[data-creator-status-filter]')?.value || 'all',
  };
}

// Counts/footer via the shell handle when it has wired the pane, else plain
// textContent (the shell wires asynchronously after mount).
function writeLibraryCounts(rail, counts) {
  const pg = rail?.__ctoxPaneGrammar;
  if (pg && typeof pg.setCounts === 'function') { pg.setCounts(counts); return; }
  for (const [key, value] of Object.entries(counts)) {
    const node = rail?.querySelector(`[data-pg-count="${key}"]`);
    if (node) node.textContent = ` (${value})`;
  }
}

function writeLibraryFooter(rail, str) {
  const pg = rail?.__ctoxPaneGrammar;
  if (pg && typeof pg.setFooter === 'function') { pg.setFooter(str); return; }
  const node = rail?.querySelector('[data-pg-footer]');
  if (node) node.textContent = str || '';
}

// Populate the tray status filter with only the statuses that actually exist,
// preserving the current selection (never removes the neutral "Alle Status").
function syncStatusOptions(rail, requests) {
  const select = rail?.querySelector('[data-creator-status-filter]');
  if (!select) return;
  const wanted = creatorRequestStatuses(requests);
  const current = select.value || 'all';
  const existing = new Set([...select.options].map((o) => o.value));
  for (const status of wanted) {
    if (existing.has(status)) continue;
    const option = document.createElement('option');
    option.value = status;
    option.textContent = creatorStatusLabel(status);
    select.append(option);
    existing.add(status);
  }
  // Drop stale options (except the neutral default) that no longer exist.
  for (const option of [...select.options]) {
    if (option.value !== 'all' && !wanted.includes(option.value)) option.remove();
  }
  select.value = existing.has(current) && (current === 'all' || wanted.includes(current)) ? current : 'all';
}

function creatorStatusLabel(status) {
  const key = `status_${String(status || '')}`;
  return state.t(key, String(status || ''));
}

function renderLibrary(host) {
  const rail = libraryRail(host);
  const listEl = host.querySelector('[data-creator-list]');
  if (!rail || !listEl) return;

  const apps = Array.isArray(state.installedApps) ? state.installedApps : [];
  const requests = allCreatorRequests();
  syncStatusOptions(rail, requests);

  const g = readLibraryGrammar(rail);
  const items = filterCreatorLibrary({ apps, requests }, g);

  // Leere Liste rendert nichts (Zustand nur in der Zustandszone).
  listEl.innerHTML = items.map((item) => renderLibraryShard(item, g.view)).join('');

  applyLibrarySelection(listEl);
  writeLibraryCounts(rail, creatorLibraryCounts(apps, requests));
  // Textdiät Regel 3/5: Anzahl steht im Reiter – kein Fußzähler.
  writeLibraryFooter(rail, '');
}

// Ein Shard ist ein reiner Selektor (keine Zeilenknöpfe). Die beiden Ansichten
// sind bewusst VERSCHIEDEN (Betreiber-Direktive 31.08.):
//   KARTE — Titel fett plus zwei Meta-Zeilen aus den Feldern, die die App
//           ohnehin lädt (Kategorie/Version/Beschreibung bzw. Status/Datum/
//           Auftragstext). Grosszügiges Padding.
//   LISTE — genau EINE Zeile: Titel plus höchstens ein Kurz-Meta rechts.
function renderLibraryShard(item, view) {
  return item.kind === 'request' ? renderRequestShard(item, view) : renderAppShard(item, view);
}

// Kurzes Datum aus updated_at_ms; 0/ungültig ergibt '' (Meta-Zeile fällt weg).
function creatorShardDate(ms) {
  const value = Number(ms) || 0;
  if (!value) return '';
  try {
    return new Date(value).toLocaleDateString(state.lang === 'en' ? 'en-GB' : 'de-DE', {
      day: '2-digit', month: '2-digit', year: 'numeric',
    });
  } catch { return ''; }
}

// Einzeiliger Auszug für die zweite Meta-Zeile der Karte.
function creatorShardExcerpt(text, limit = 120) {
  const value = String(text || '').replace(/\s+/g, ' ').trim();
  return value.length > limit ? `${value.slice(0, limit - 1)}…` : value;
}

function shardAttrs(kind, id, label, view, selected, recordType) {
  return `class="ctox-list-item creator-shard creator-shard--${view === 'list' ? 'list' : 'cards'}${selected ? ' is-selected' : ''}"`
    + ' role="button" tabindex="0"'
    + ` aria-selected="${selected ? 'true' : 'false'}"`
    + ` data-creator-library-kind="${kind}" data-creator-library-id="${escapeHtml(id)}"`
    + ` data-context-record-id="${escapeHtml(id)}" data-context-record-type="${recordType}" data-context-label="${escapeHtml(label)}"`;
}

function renderAppShard(app, view) {
  const selected = state.selectedLibraryKind === 'app' && state.selectedLibraryId === app.id;
  const attrs = shardAttrs('app', app.id, app.title || app.id, view, selected, 'application');
  if (view === 'list') {
    // LISTE: eine kompakte Zeile — Titel plus die Version als Kurz-Meta rechts.
    const short = app.version ? `v${app.version}` : '';
    return `<div ${attrs}><span class="creator-shard-title">${escapeHtml(app.title)}</span>`
      + (short ? `<span class="creator-shard-side">${escapeHtml(short)}</span>` : '')
      + '</div>';
  }
  // KARTE: Titel plus Zuordnung/Version und, wenn vorhanden, die Beschreibung.
  const meta = [app.category, app.version ? `v${app.version}` : '']
    .filter(Boolean).join(' · ');
  const sub = creatorShardExcerpt(app.description);
  return `<div ${attrs}><div class="creator-shard-head"><span class="creator-shard-title">${escapeHtml(app.title)}</span></div>`
    + (meta ? `<div class="creator-shard-meta">${escapeHtml(meta)}</div>` : '')
    + (sub ? `<div class="creator-shard-meta creator-shard-sub">${escapeHtml(sub)}</div>` : '')
    + '</div>';
}

function renderRequestShard(item, view) {
  const selected = state.selectedLibraryKind === 'request' && state.selectedLibraryId === item.id;
  const statusLabel = item.imported ? state.t('status_lokal', 'lokal') : creatorStatusLabel(item.status);
  const attrs = shardAttrs('request', item.id, item.title || item.id, view, selected, 'app-request');
  if (view === 'list') {
    // LISTE: eine kompakte Zeile — Titel plus der Status als Kurz-Meta rechts.
    return `<div ${attrs}><span class="creator-shard-title">${escapeHtml(item.title)}</span>`
      + `<span class="creator-shard-side">${escapeHtml(statusLabel)}</span></div>`;
  }
  // KARTE: Titel mit Status-Badge, darunter Zuordnung/Status/Datum und der
  // Auftragstext als zweite Meta-Zeile.
  const badge = `<span class="ctox-badge">${escapeHtml(creatorStatusLabel(item.status))}</span>`;
  const meta = [state.t('requestKicker', 'Auftrag'), statusLabel, creatorShardDate(item.updated_at_ms)]
    .filter(Boolean).join(' · ');
  const sub = creatorShardExcerpt(item.request);
  return `<div ${attrs}><div class="creator-shard-head"><span class="creator-shard-title">${escapeHtml(item.title)}</span>${badge}</div>`
    + (meta ? `<div class="creator-shard-meta">${escapeHtml(meta)}</div>` : '')
    + (sub ? `<div class="creator-shard-meta creator-shard-sub">${escapeHtml(sub)}</div>` : '')
    + '</div>';
}

// Selection is an in-place class flip across existing rows — never a list
// rebuild (design-guide: re-renders never move the operator). Only data changes
// re-render the well.
function applyLibrarySelection(listEl) {
  listEl?.querySelectorAll('[data-creator-library-id]').forEach((row) => {
    const on = row.getAttribute('data-creator-library-kind') === state.selectedLibraryKind
      && (row.getAttribute('data-creator-library-id') || '') === String(state.selectedLibraryId || '');
    row.classList.toggle('is-selected', on);
    row.setAttribute('aria-selected', String(on));
  });
}

function selectLibraryItem(host, kind, id) {
  state.selectedLibraryKind = kind;
  state.selectedLibraryId = id || null;
  // In-place flip only — do NOT re-render the list (would reset scroll).
  applyLibrarySelection(host.querySelector('[data-creator-list]'));
  if (kind === 'app') {
    void state.composer?.prefillUpgrade?.(id);
  } else if (kind === 'request') {
    const request = allCreatorRequests().find((item) => item.id === id);
    if (request) state.composer?.adoptRequest?.(request);
  }
}

function wireLibrary(host) {
  const rail = libraryRail(host);
  const listEl = host.querySelector('[data-creator-list]');
  if (!rail || !listEl) return;

  const selectFromEvent = (event) => {
    const row = event.target?.closest?.('[data-creator-library-id]');
    if (!row || !listEl.contains(row)) return false;
    selectLibraryItem(host, row.getAttribute('data-creator-library-kind'), row.getAttribute('data-creator-library-id'));
    return true;
  };
  listEl.addEventListener('click', selectFromEvent);
  listEl.addEventListener('keydown', (event) => {
    if (event.key !== 'Enter' && event.key !== ' ') return;
    const row = event.target?.closest?.('[data-creator-library-id]');
    if (!row || !listEl.contains(row)) return;
    event.preventDefault();
    selectFromEvent(event);
  });

  // Header icon actions: import / export the Aufträge list (JSON).
  rail.addEventListener('click', (event) => {
    const btn = event.target?.closest?.('[data-action]');
    if (!btn || !rail.contains(btn)) return;
    if (btn.dataset.action === 'import') importCreatorRequests(host);
    else if (btn.dataset.action === 'export') exportCreatorRequests();
  });

  // EIN Umschalter statt zweier Zustandsknöpfe: der Klick TUT etwas, er meldet
  // keinen Zustand. data-pg-view trägt die aktuelle Ansicht, data-pg-view-alt
  // die Gegenansicht; Icon, Titel und aria-label zeigen stets das Klickziel.
  const viewToggleEl = rail.querySelector('[data-creator-view-toggle]');
  function syncViewToggle() {
    if (!viewToggleEl) return;
    const next = viewToggleEl.dataset.pgViewAlt === 'cards' ? 'cards' : 'list';
    const label = state.t(VIEW_ACTION_KEY[next], VIEW_ACTION_FALLBACK[next]);
    viewToggleEl.innerHTML = VIEW_ICON[next];
    viewToggleEl.setAttribute('aria-label', label);
    viewToggleEl.setAttribute('title', label);
    viewToggleEl.removeAttribute('aria-pressed');
  }
  viewToggleEl?.addEventListener('click', () => {
    const current = viewToggleEl.dataset.pgView === 'list' ? 'list' : 'cards';
    viewToggleEl.dataset.pgView = current === 'list' ? 'cards' : 'list';
    viewToggleEl.dataset.pgViewAlt = current;
    syncViewToggle();
    renderLibrary(host);
  });
  // Die generische Shell-Verdrahtung (shared/pane-grammar.js) setzt auf JEDEN
  // [data-pg-view]-Knopf ein aria-pressed="true". Für einen Aktionsknopf ist
  // das die falsche Semantik. Dieser Listener hängt an der SPALTE, läuft in der
  // Bubble-Phase also garantiert nach dem Knopf-Listener und räumt es ab —
  // ohne Timer und ohne Annahme über die Registrierungsreihenfolge.
  rail.addEventListener('click', () => viewToggleEl?.removeAttribute('aria-pressed'));
  syncViewToggle();

  // Re-render the well when the shell reports a grammar change (search / view /
  // tray / band). The event bubbles from the wired pane.
  rail.addEventListener('ctox-pane-grammar-change', () => renderLibrary(host));
}

// Export the current Aufträge list (server requests + local drafts) as JSON.
function exportCreatorRequests() {
  const payload = allCreatorRequests().map((r) => ({ id: r.id, title: r.title, request: r.request, status: r.status }));
  let url = '';
  try {
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
    url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'creator-app-requests.json';
    a.rel = 'noopener';
    a.style.display = 'none';
    // Der Host des Moduls liegt im state, weil der Export ausserhalb von
    // mount() laeuft und dort kein ctx im Gueltigkeitsbereich ist.
    (state.root || document.documentElement).appendChild(a);
    a.click();
    a.remove();
    addConsoleLog(`[INFO] ${payload.length} App-Auftraege exportiert.`, 'info');
  } catch (error) {
    addConsoleLog(`[FEHLER] Export fehlgeschlagen: ${error.message}`, 'error');
  } finally {
    if (url) setTimeout(() => { try { URL.revokeObjectURL(url); } catch {} }, 4000);
  }
}

// Import request drafts from a JSON file into the local Aufträge list. Drafts
// are display-only (never dispatched); select one to adopt it into the composer.
function importCreatorRequests(host) {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = 'application/json,.json';
  input.addEventListener('change', async () => {
    const file = input.files && input.files[0];
    if (!file) return;
    let parsed;
    try { parsed = JSON.parse(await file.text()); } catch {
      addConsoleLog('[FEHLER] Ungültige JSON-Datei.', 'error');
      return;
    }
    const rawItems = Array.isArray(parsed) ? parsed : (parsed && typeof parsed === 'object' ? [parsed] : []);
    const base = state.importedRequests.length;
    const drafts = rawItems.map((raw, index) => prepareCreatorRequestImport(raw, base + index)).filter(Boolean);
    if (!drafts.length) {
      addConsoleLog('[WARN] Keine Auftraege in der Datei gefunden.', 'warning');
      return;
    }
    for (const draft of drafts) {
      if (!state.importedRequests.some((r) => r.id === draft.id)) state.importedRequests.push(draft);
    }
    addConsoleLog(`[INFO] ${drafts.length} App-Auftraege importiert (lokal).`, 'info');
    renderLibrary(host);
  });
  input.click();
}

// Monochrome stroke icon in the shared action-icon style. Falls back to the
// App-lokale Glyphen (4.0 hat keine Icon-API, app-api.d.ts OFFEN-D).
function creatorActionIcon(name, size = 16) {
  const paths = {
    open: 'M14 5h5v5M19 5l-8 8M11 5H5v14h14v-6',
    upload: 'M12 15V4M12 4 8 8M12 4l4 4M5 19h14',
    download: 'M12 4v11M12 15l-4-4M12 15l4-4M5 19h14',
    close: 'M6 6l12 12M18 6L6 18',
  };
  const d = paths[name] || paths.close;
  return `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="${d}"/></svg>`;
}

function wireUi(host) {
  const inputId = host.querySelector('#input-app-id');
  const inputTitle = host.querySelector('#input-app-title');
  const inputDesc = host.querySelector('#input-app-desc');
  const selectCategory = host.querySelector('#select-app-category');
  const selectArchetype = host.querySelector('#select-app-archetype');
  const selectLayout = host.querySelector('#select-app-layout');
  const btnAddColl = host.querySelector('#btn-add-collection');
  const inputNewColl = host.querySelector('#input-new-collection');
  const btnDeploy = host.querySelector('#btn-deploy-app');
  const inputRequest = host.querySelector('#app-request-input');
  const inspirationInput = host.querySelector('#creator-inspiration-url');
  const addInspirationButton = host.querySelector('#btn-add-inspiration');
  const inspirationList = host.querySelector('[data-inspiration-list]');
  const exampleList = host.querySelector('[data-example-prompts]');
  state.isDeploying = false;

  renderPromptExamples();
  renderInspirationUrls();

  const syncStateFromInputs = () => {
    state.appId = normalizeModuleId(inputId.value);
    if (inputId.value !== state.appId) inputId.value = state.appId;
    state.appTitle = inputTitle.value.trim();
    state.appDesc = inputDesc.value.trim();
    state.appCategory = selectCategory.value || '';
    state.appArchetype = selectArchetype.value || 'record-workbench';
    state.appLayout = selectLayout.value || '';

    updateCreatorActionState();
  };

  const updateCreatorActionState = () => {
    const actionState = computeCreatorActionState({
      request: inputRequest.value,
      appId: inputId.value,
      appTitle: inputTitle.value,
      appDesc: inputDesc.value,
      appCollections: state.appCollections,
      isDeploying: state.isDeploying,
      lang: state.lang === 'en' ? 'en' : 'de'
    });
    btnDeploy.disabled = !actionState.deployReady;
    btnDeploy.setAttribute('aria-disabled', String(btnDeploy.disabled));
    btnDeploy.title = actionState.deployReady
      ? (state.lang === 'en' ? 'Start app creation through CTOX' : 'App-Erstellung durch CTOX starten')
      : actionState.diagnostic;
    btnDeploy.dataset.state = actionState.deployReady ? 'ready' : 'blocked';
    // Kein Statusfeld im Inhalt: der Grund steht nur im Titel des gesperrten Knopfs.
    return actionState;
  };

  inputRequest.addEventListener('input', () => {
    updateCreatorActionState();
  });

  addInspirationButton.addEventListener('click', addInspirationUrl);
  inspirationInput.addEventListener('keydown', (event) => {
    if (event.key !== 'Enter') return;
    event.preventDefault();
    addInspirationUrl();
  });
  inspirationList.addEventListener('click', (event) => {
    const button = event.target.closest('[data-remove-inspiration]');
    if (!button) return;
    state.inspirationUrls.splice(Number(button.dataset.removeInspiration), 1);
    renderInspirationUrls();
  });
  exampleList.addEventListener('click', (event) => {
    const button = event.target.closest('[data-example-id]');
    if (!button) return;
    const example = CREATOR_PROMPT_EXAMPLES.find((item) => item.id === button.dataset.exampleId);
    if (!example) return;
    const locale = state.lang === 'en' ? 'en' : 'de';
    inputRequest.value = example[locale].prompt;
    if (!inputTitle.value.trim()) inputTitle.value = example[locale].title;
    syncStateFromInputs();
    inputRequest.focus();
    inputRequest.setSelectionRange(inputRequest.value.length, inputRequest.value.length);
  });

  [inputId, inputTitle, inputDesc, selectCategory, selectArchetype, selectLayout].forEach(el => {
    el.addEventListener('input', () => syncStateFromInputs());
  });

  // DB Collection Visual builder in advanced accordion
  const renderCollectionsList = (h) => {
    const listEl = h.querySelector('#collections-list');
    listEl.innerHTML = '';
    state.appCollections.forEach((coll, idx) => {
      const row = document.createElement('div');
      row.className = 'collection-row';
      row.innerHTML = `
        <span class="creator-collection-name">${escapeHtml(coll)}</span>
        <button type="button" class="ctox-icon-button is-danger" data-remove-idx="${idx}" aria-label="Datentabelle ${coll} entfernen" title="Datentabelle entfernen">
          ${creatorActionIcon('close')}
        </button>
      `;
      row.querySelector('[data-remove-idx]').addEventListener('click', async (e) => {
        const removeIdx = parseInt(e.currentTarget.getAttribute('data-remove-idx'), 10);
        const name = state.appCollections[removeIdx];
        const confirmed = await state.api?.shell.dialog({
          kind: 'confirm',
          title: 'Datentabelle entfernen',
          text: `Datentabelle "${name}" aus den optionalen Vorgaben entfernen?`,
          confirm_label: 'Entfernen',
          cancel_label: 'Abbrechen',
          danger: true,
          default: false,
        });
        if (confirmed !== true) return;
        state.appCollections.splice(removeIdx, 1);
        renderCollectionsList(h);
        syncStateFromInputs();
      });
      listEl.appendChild(row);
    });
  };

  btnAddColl.addEventListener('click', () => {
    const newName = normalizeCollectionName(inputNewColl.value);
    if (!newName) return;
    if (state.appCollections.includes(newName)) {
      addConsoleLog(`[WARN] Datentabelle '${newName}' existiert bereits.`, 'warning');
      return;
    }
    state.appCollections.push(newName);
    inputNewColl.value = '';
    renderCollectionsList(host);
    syncStateFromInputs();
    addConsoleLog(`[INFO] Datentabelle '${newName}' hinzugefügt.`, 'info');
  });

  inputNewColl.addEventListener('keydown', (event) => {
    if (event.key !== 'Enter') return;
    event.preventDefault();
    btnAddColl.click();
  });

  // Composer controller: the LEFT column drives the composer context. Selecting
  // an app prefills upgrade mode; selecting an Auftrag adopts its request text.
  // The hash-based upgrade entry point reuses the same prefill path.
  state.composer = {
    adoptRequest(request) {
      if (!request) return;
      inputRequest.value = request.request || '';
      updateCreatorActionState();
      const status = request.imported ? state.t('status_lokal', 'lokal') : String(request.status || '');
      addConsoleLog(`[INFO] App-Auftrag '${request.title}' übernommen${status ? ` (Status: ${status})` : ''}.`, 'info');
      inputRequest.focus();
    },
    async prefillUpgrade(appId) {
      const id = String(appId || '').trim();
      if (!id) return;
      // v4: kein HTTP-Ladeweg auf installed-modules/<id>/module.json – der Kontext kommt aus dem App-Katalog
      // (business_apps). Quelltext/Manifest einer App liest nur die Shell (kernel.source, V4-PORT.md OFFEN-1).
      const app = state.installedApps.find((entry) => entry.id === id);
      if (app) {
        inputId.value = app.id;
        inputTitle.value = app.title || '';
        inputDesc.value = app.description || '';
        if (app.category) selectCategory.value = app.category;
        inputRequest.value = `Ändere ${app.title || id}: ${app.description || ''}`;
        state.appVersion = /^\d+\.\d+\.\d+$/.test(String(app.version || '')) ? String(app.version) : '0.1.0';
        state.appCollections = [];
        renderCollectionsList(host);
        syncStateFromInputs();
      } else {
        addConsoleLog(`App '${id}' ist im Katalog nicht vorhanden.`, 'error', 'creator.upgrade.unknown');
      }
      updateCreatorActionState();
      inputRequest.focus();
    },
    // Remount: Datentabellen, Inspirations-URLs und Knopfzustand aus dem wiederhergestellten Stand zeichnen.
    restore() {
      renderCollectionsList(host);
      renderInspirationUrls();
      syncStateFromInputs();
    },
  };

  renderCollectionsList(host);
  updateCreatorActionState();

  function addInspirationUrl() {
    const url = normalizeInspirationUrl(inspirationInput.value);
    if (!url) {
      addConsoleLog(`${state.t('invalidUrlTitle', 'URL prüfen')}: ${state.t('invalidUrlMessage', 'Bitte füge eine vollständige http- oder https-URL ein.')}`, 'warning', 'creator.inspiration.invalid');
      inspirationInput.focus();
      return;
    }
    state.api?.status.clear('creator.inspiration.invalid');
    if (!state.inspirationUrls.includes(url)) state.inspirationUrls.push(url);
    inspirationInput.value = '';
    renderInspirationUrls();
  }

  function renderInspirationUrls() {
    inspirationList.innerHTML = state.inspirationUrls.map((url, index) => `
      <span class="ctox-badge creator-url-chip">
        <span title="${escapeHtml(url)}">${escapeHtml(url)}</span>
        <button type="button" data-remove-inspiration="${index}" aria-label="${escapeHtml(state.t('removeInspiration', 'URL entfernen'))}">×</button>
      </span>
    `).join('');
  }

  function renderPromptExamples() {
    const locale = state.lang === 'en' ? 'en' : 'de';
    exampleList.innerHTML = CREATOR_PROMPT_EXAMPLES.map((example) => `
      <button class="creator-example" type="button" data-example-id="${escapeHtml(example.id)}">
        <strong>${escapeHtml(example[locale].title)}</strong>
        <span>${escapeHtml(example[locale].hint)}</span>
      </button>
    `).join('');
  }

  // Install / Deploy Button
  btnDeploy.addEventListener('click', async () => {
    try {
      const currentRequest = inputRequest.value.trim();
      if (!currentRequest) {
        addConsoleLog('Auftrag fehlt: Bitte beschreibe die App, bevor du den CTOX-Auftrag startest.', 'warning', 'creator.request.missing');
        updateCreatorActionState();
        return;
      }

      const actionState = updateCreatorActionState();
      if (!actionState.deployReady) {
        addConsoleLog(`App-Auftrag verhindert: ${actionState.diagnostic}`, 'warning', 'creator.request.invalid');
        return;
      }

      const previewCommand = buildAppCreateCommand({
        appId: inputId.value,
        appTitle: inputTitle.value,
        appDesc: inputDesc.value,
        appCategory: selectCategory.value,
        appArchetype: selectArchetype.value,
        appLayout: selectLayout.value,
        appCollections: state.appCollections,
        appVersion: state.appVersion,
        inspirationUrls: state.inspirationUrls,
        instruction: currentRequest,
        actor: null,
      });
      addConsoleLog(`[INFO] Erstelle '${previewCommand.payload.app_title}' als ${previewCommand.payload.archetype}.`, 'info');
      state.api?.status.clear('creator.request.missing');
      state.api?.status.clear('creator.request.invalid');
      triggerAppDeployment(host, updateCreatorActionState);
    } catch (e) {
      console.error('[ERROR] triggerAppDeployment failed:', e);
      state.isDeploying = false;
      updateCreatorActionState();
    }
  });
}

// Protokollzeilen gehen in die Konsole; Warnungen und Fehler in die Zustandszone der Shell (kein Protokoll-
// oder Statusfeld im Inhalt).
function addConsoleLog(text, type = '', code = '') {
  console.log(text);
  if (type !== 'warning' && type !== 'error') return;
  state.api?.status.report({ level: type === 'error' ? 'error' : 'warn', code: code || `creator.${type}`, text: String(text) });
}

export function buildAppCreateCommand({
  appId,
  appTitle,
  appDesc,
  appCategory,
  appArchetype,
  appLayout,
  appCollections,
  appVersion,
  inspirationUrls = [],
  instruction,
  actor,
  now = Date.now(),
}) {
  const request = String(instruction || appDesc || appTitle || '').trim();
  if (!request) throw new Error('App request is required');
  const moduleId = normalizeModuleId(appId) || deriveModuleIdFromRequest(request, now);
  const collections = Array.isArray(appCollections)
    ? appCollections.map(normalizeCollectionName).filter(Boolean)
    : [];
  const version = /^\d+\.\d+\.\d+$/.test(String(appVersion || '').trim())
    ? String(appVersion).trim()
    : '0.1.0';
  const title = String(appTitle || titleFromModuleId(moduleId)).trim();
  const description = String(appDesc || request.slice(0, 220)).trim();
  const references = [...new Set((Array.isArray(inspirationUrls) ? inspirationUrls : [])
    .map(normalizeInspirationUrl)
    .filter(Boolean))];

  // v4: die Befehls-ID vergibt api.command.
  void now;
  return {
    module: 'creator',
    command_type: 'ctox.business_os.app.create',
    record_id: moduleId,
    payload: {
      title: `Create ${title}`,
      instruction: request,
      module_id: moduleId,
      app_id: moduleId,
      app_title: title,
      description,
      category: String(appCategory || '').trim(),
      archetype: String(appArchetype || 'record-workbench').trim(),
      layout_hint: String(appLayout || '').trim(),
      presentation: {
        default_mode: 'window',
        supported_modes: ['window', 'maximized', 'focus'],
        initial_size: { width: 960, height: 680 },
        minimum_size: { width: 640, height: 480 },
        multi_instance: false,
        auto_restore: false,
      },
      collections_hint: collections,
      desired_version: version,
      inspiration_urls: references,
      install_target: 'runtime-installed-module',
      target: 'app',
      mode: 'app',
      required_skills: ['business-os-app-module-development'],
    },
    client_context: {
      source: 'business-os-creator',
      target: 'app',
      mode: 'app',
      module_id: moduleId,
      app_id: moduleId,
      archetype: String(appArchetype || 'record-workbench').trim(),
      install_target: 'runtime-installed-module',
      actor: actor || null,
      inspiration_urls: references,
    },
  };
}

// Befehl absetzen, nie warten (R5): der Auftrag erscheint live unter „Aufträge“, Fehler in der Zustandszone.
function triggerAppDeployment(host, updateCreatorActionState = () => {}) {
  const api = state.api;
  if (!api) return;
  const request = host.querySelector('#app-request-input')?.value?.trim() || '';
  const appVersion = /^\d+\.\d+\.\d+$/.test(String(state.appVersion || '').trim())
    ? String(state.appVersion).trim()
    : '0.1.0';
  if (!request) {
    addConsoleLog('Bitte beschreibe die gewünschte App.', 'error', 'creator.request.missing');
    return;
  }
  const command = buildAppCreateCommand({
    appId: state.appId,
    appTitle: state.appTitle,
    appDesc: state.appDesc,
    appCategory: state.appCategory,
    appArchetype: state.appArchetype,
    appLayout: state.appLayout,
    appCollections: state.appCollections,
    appVersion,
    inspirationUrls: state.inspirationUrls,
    instruction: request,
    actor: {
      id: api.actor?.id || '',
      display_name: api.actor?.display_name || api.actor?.id || '',
      role: api.actor?.role || 'user',
      is_admin: api.actor?.role === 'admin',
    },
  });
  const code = `creator.command.${command.command_type}`;
  const failText = (error) => `App-Erstellung fehlgeschlagen: Der CTOX-Auftrag konnte nicht angelegt werden${error ? `: ${error}` : '.'}`;
  let handle;
  try {
    handle = api.command(command.command_type, {
      ...command.payload,
      module: command.module,
      record_id: command.record_id,
      client_context: command.client_context,
    }, { record_id: command.record_id });
  } catch (error) {
    api.status.report({ level: 'error', code, text: failText(error?.message || String(error)) });
    return;
  }
  console.log(`[QUEUE] ${command.command_type} für ${command.payload.module_id} (${handle.id})`);
  handle.local?.catch?.((error) => api.status.report({ level: 'error', code, text: failText(error?.message || String(error)), ref: handle.id }));
  let off = null;
  let finished = false;
  off = handle.status.subscribe((st) => {
    if (finished) return;
    if (st?.status === 'failed') { finished = true; api.status.report({ level: 'error', code, text: failText(st.error || ''), ref: handle.id }); off?.(); }
    else if (st?.status === 'done') { finished = true; api.status.clear(code); off?.(); }
  });
  if (finished) off?.();
  else state.disposers.push(() => off?.());
  updateCreatorActionState();
}

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}



function applyCreatorTranslations(root, t) {
  root.querySelectorAll('[data-t]').forEach((el) => {
    el.textContent = t(el.dataset.t, el.textContent.trim());
  });
  root.querySelectorAll('[data-t-placeholder]').forEach((el) => {
    el.setAttribute('placeholder', t(el.dataset.tPlaceholder, el.getAttribute('placeholder') || ''));
  });
  root.querySelectorAll('[data-t-title]').forEach((el) => {
    el.setAttribute('title', t(el.dataset.tTitle, el.getAttribute('title') || ''));
  });
  root.querySelectorAll('[data-t-aria]').forEach((el) => {
    el.setAttribute('aria-label', t(el.dataset.tAria, el.getAttribute('aria-label') || ''));
  });
}
