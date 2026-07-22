import { showBusinessConfirm } from '../../shared/dialogs.js';

const state = {
  ctx: null,
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
  catalogSubscription: null,
  commandSubscription: null,
  streamGeneration: 0,
  installedApps: [],
  creatorRequests: [],
  importedRequests: [],
  selectedLibraryId: null,
  selectedLibraryKind: null,
  composer: null,
  isDeploying: false
};

export const CREATOR_PROMPT_EXAMPLES = Object.freeze([
  {
    id: 'crm',
    de: { title: 'Kunden & Kontakte', hint: 'CRM mit Suche, Status und Aktivitäten', prompt: 'Erstelle eine schlanke CRM-App für Kunden und Kontakte. Links stehen Suche, Statusfilter und Kundenliste. In der Hauptansicht sehe und bearbeite ich Kontaktdaten, Notizen, nächste Schritte und den aktuellen Status. Ergänze eine Aktivitätschronik und eine klare Aktion „Follow-up an CTOX delegieren“.' },
    en: { title: 'Customers & contacts', hint: 'CRM with search, status, and activity', prompt: 'Create a focused CRM app for customers and contacts. Put search, status filters, and the customer list on the left. The main view should show and edit contact details, notes, next steps, and current status. Add an activity timeline and one clear action to delegate a follow-up to CTOX.' },
  },
  {
    id: 'support',
    de: { title: 'Support Desk', hint: 'Tickets priorisieren und bearbeiten', prompt: 'Erstelle eine Support-Desk-App. Links brauche ich Ticketliste, Suche sowie Filter für offen, wartend und gelöst. Die Hauptansicht zeigt Beschreibung, Kunde, Priorität, Verlauf und Antwortentwurf. Ergänze Statuswechsel, Zuweisung und eine Aktion, mit der CTOX eine Antwort vorbereitet.' },
    en: { title: 'Support desk', hint: 'Prioritize and resolve tickets', prompt: 'Create a support desk app. The left side needs a ticket list, search, and filters for open, waiting, and resolved. The main view should show description, customer, priority, history, and reply draft. Add status changes, assignment, and an action for CTOX to prepare a reply.' },
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
  if (isDeploying) diagnostic = en ? 'CTOX app job is running.' : 'CTOX App-Auftrag läuft.';
  else if (hasRequest && validationErrors.length > 0) diagnostic = validationErrors[0];
  else if (deployReady) diagnostic = en ? 'Ready. CTOX builds the app from this job.' : 'Bereit. CTOX baut die App aus diesem Auftrag.';

  return { hasRequest, validationErrors, deployReady, diagnostic };
}

export function normalizeCreatorInstalledApps(catalog) {
  const modules = Array.isArray(catalog?.modules) ? catalog.modules : [];
  return modules
    .filter((mod) => {
      const entry = String(mod?.entry || '');
      const source = String(mod?.source || mod?.store?.distribution || '').toLowerCase();
      return mod?.id
        && mod.id !== 'creator'
        && (
          entry.startsWith('installed-modules/')
          || source === 'installed'
          || source.includes('installed-module')
        );
    })
    .map((mod) => ({
      id: normalizeModuleId(mod.id),
      title: String(mod.title || mod.id),
      description: String(mod.description || mod.store?.summary || ''),
      category: String(mod.category || mod.source || 'Custom'),
      version: String(mod.version || '0.1.0'),
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

export async function mount(ctx) {
  const streamGeneration = ++state.streamGeneration;
  state.ctx = ctx;
  state.appId = '';
  state.appTitle = '';
  state.appDesc = '';
  state.appCategory = '';
  state.appArchetype = 'record-workbench';
  state.appLayout = 'windowed';
  state.appCollections = [];
  state.inspirationUrls = [];

  // 1. Inject module scoped stylesheet dynamically
  await ensureStyles();

  // 1b. Load locale messages (German markup text is the fallback)
  const messages = await loadCreatorMessages(ctx.locale);
  state.t = (key, fallback) => messages[key] ?? fallback ?? key;

  // 2. Fetch and render raw index.html structure
  const markupVersion = String(import.meta.url).split('?v=')[1] || '';
  const markupHref = new URL('./index.html', import.meta.url).pathname + (markupVersion ? `?v=${markupVersion}` : '');
  const html = await fetch(markupHref).then(res => res.text());
  ctx.host.innerHTML = html;
  applyCreatorTranslations(ctx.host, state.t);

  // 3. Wire UI events
  wireUi(ctx.host);
  wireLibrary(ctx.host);

  // 4. Render the operable shell immediately. Catalog and command hydration
  // can wait for a cold WebRTC lease without blocking the window from opening.
  renderLibrary(ctx.host);
  void startCreatorDataStreams(ctx, ctx.host, streamGeneration).catch((error) => {
    if (streamGeneration !== state.streamGeneration) return;
    addConsoleLog(`[WARN] Creator-Daten konnten nicht geladen werden: ${error.message}`, 'warning');
  });

  return () => {
    if (streamGeneration === state.streamGeneration) state.streamGeneration += 1;
    cleanupSubscription(state.catalogSubscription);
    cleanupSubscription(state.commandSubscription);
    state.catalogSubscription = null;
    state.commandSubscription = null;
    console.log('[creator] Module unmounted and cleaned up.');
  };
}

async function ensureStyles() {
  const cssVersion = String(import.meta.url).split('?v=')[1] || '';
  const cssHref = new URL('./index.css', import.meta.url).pathname + (cssVersion ? `?v=${cssVersion}` : '');
  let link = document.querySelector('link[data-module-styles="creator"]');
  if (!link) {
    link = document.createElement('link');
    link.rel = 'stylesheet';
    link.dataset.moduleStyles = 'creator';
    document.head.append(link);
  }
  if (link.getAttribute('href') !== cssHref) link.href = cssHref;
}

async function startCreatorDataStreams(ctx, host, streamGeneration) {
  await Promise.allSettled([
    ctx.sync?.startCollection?.('business_module_catalog'),
    ctx.sync?.startCollection?.('business_commands'),
  ]);
  if (streamGeneration !== state.streamGeneration || !host.isConnected) return;

  const catalogColl = getCollection(ctx, 'business_module_catalog');
  const commandColl = getCollection(ctx, 'business_commands');

  try {
    const catalogDoc = await catalogColl?.findOne?.('module-catalog')?.exec?.();
    if (streamGeneration !== state.streamGeneration || !host.isConnected) return;
    state.installedApps = normalizeCreatorInstalledApps(catalogDoc?.toJSON?.() || {});
  } catch (error) {
    addConsoleLog(`[WARN] Modulkatalog konnte nicht geladen werden: ${error.message}`, 'warning');
  }

  try {
    const commandDocs = await commandColl?.find?.()?.exec?.();
    if (streamGeneration !== state.streamGeneration || !host.isConnected) return;
    state.creatorRequests = normalizeCreatorRequestSuggestions(commandDocs?.map((doc) => doc?.toJSON?.() || doc) || []);
  } catch (error) {
    addConsoleLog(`[WARN] CTOX App-Auftraege konnten nicht geladen werden: ${error.message}`, 'warning');
  }

  if (streamGeneration !== state.streamGeneration || !host.isConnected) return;
  state.catalogSubscription = catalogColl?.findOne?.('module-catalog')?.$?.subscribe?.((doc) => {
    if (streamGeneration !== state.streamGeneration || !host.isConnected) return;
    state.installedApps = normalizeCreatorInstalledApps(doc?.toJSON?.() || {});
    renderLibrary(host);
  }) || null;

  state.commandSubscription = commandColl?.find?.()?.$?.subscribe?.((docs) => {
    if (streamGeneration !== state.streamGeneration || !host.isConnected) return;
    state.creatorRequests = normalizeCreatorRequestSuggestions(docs?.map((doc) => doc?.toJSON?.() || doc) || []);
    renderLibrary(host);
  }) || null;

  renderLibrary(host);
}

function getCollection(ctx, name) {
  return ctx.db?.collection?.(name) || null;
}

function cleanupSubscription(subscription) {
  if (typeof subscription === 'function') {
    subscription();
    return;
  }
  subscription?.unsubscribe?.();
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
// is Apps). Mirrors the consent/reports reference modules.
function readLibraryGrammar(rail) {
  return {
    search: (rail?.querySelector('[data-pg-search]')?.value || '').trim().toLowerCase(),
    view: rail?.querySelector('[data-pg-view][aria-pressed="true"]')?.dataset.pgView || 'cards',
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

  const bandTotal = g.band === 'auftraege' ? requests.length : apps.length;
  const emptyKey = g.band === 'auftraege' ? 'requestsEmpty' : 'installedEmpty';
  const emptyText = bandTotal === 0 ? state.t(emptyKey) : state.t('libEmptyFiltered', 'Kein Eintrag passt zum Filter.');
  listEl.innerHTML = items.length
    ? items.map((item) => renderLibraryShard(item, g.view)).join('')
    : `<div class="ctox-empty"><strong>${escapeHtml(emptyText)}</strong></div>`;

  applyLibrarySelection(listEl);
  writeLibraryCounts(rail, creatorLibraryCounts(apps, requests));
  const bandLabel = g.band === 'auftraege' ? state.t('bandAuftraege', 'Aufträge') : state.t('bandApps', 'Apps');
  writeLibraryFooter(rail, `${items.length} ${state.t('libEntries', 'Einträge')} · ${bandLabel}`);
}

// A shard is a pure selector: title + ONE muted meta line, no per-row buttons.
function renderLibraryShard(item, view) {
  return item.kind === 'request' ? renderRequestShard(item, view) : renderAppShard(item, view);
}

function renderAppShard(app, view) {
  const selected = state.selectedLibraryKind === 'app' && state.selectedLibraryId === app.id;
  const meta = [state.t('rightKicker', 'Deine Apps'), app.category, app.version].filter(Boolean).join(' · ');
  const attrs = `class="ctox-list-item creator-shard creator-shard--${view === 'list' ? 'list' : 'cards'}${selected ? ' is-selected' : ''}"`
    + ' role="button" tabindex="0"'
    + ` aria-selected="${selected ? 'true' : 'false'}"`
    + ` data-creator-library-kind="app" data-creator-library-id="${escapeHtml(app.id)}"`
    + ` data-context-record-id="${escapeHtml(app.id)}" data-context-record-type="application" data-context-label="${escapeHtml(app.title || app.id)}"`;
  if (view === 'list') {
    return `<div ${attrs}><span class="creator-shard-title">${escapeHtml(app.title)}</span></div>`;
  }
  return `<div ${attrs}><div class="creator-shard-head"><span class="creator-shard-title">${escapeHtml(app.title)}</span></div>`
    + `<div class="creator-shard-meta">${escapeHtml(meta)}</div></div>`;
}

function renderRequestShard(item, view) {
  const selected = state.selectedLibraryKind === 'request' && state.selectedLibraryId === item.id;
  const badge = `<span class="ctox-badge">${escapeHtml(creatorStatusLabel(item.status))}</span>`;
  const meta = [state.t('requestKicker', 'Auftrag'), item.imported ? state.t('status_lokal', 'lokal') : creatorStatusLabel(item.status)].filter(Boolean).join(' · ');
  const attrs = `class="ctox-list-item creator-shard creator-shard--${view === 'list' ? 'list' : 'cards'}${selected ? ' is-selected' : ''}"`
    + ' role="button" tabindex="0"'
    + ` aria-selected="${selected ? 'true' : 'false'}"`
    + ` data-creator-library-kind="request" data-creator-library-id="${escapeHtml(item.id)}"`
    + ` data-context-record-id="${escapeHtml(item.id)}" data-context-record-type="app-request" data-context-label="${escapeHtml(item.title || item.id)}"`;
  if (view === 'list') {
    return `<div ${attrs}><span class="creator-shard-title">${escapeHtml(item.title)}</span>${badge}</div>`;
  }
  return `<div ${attrs}><div class="creator-shard-head"><span class="creator-shard-title">${escapeHtml(item.title)}</span>${badge}</div>`
    + `<div class="creator-shard-meta">${escapeHtml(meta)}</div></div>`;
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
    document.body.appendChild(a);
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
// shell-provided ctx.getActionIcon when available (same glyph set).
function creatorActionIcon(name, size = 16) {
  const shellIcon = state.ctx?.getActionIcon?.(name, size);
  if (shellIcon) return shellIcon;
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
  const syncDot = host.querySelector('#deploy-sync-dot');
  const syncText = host.querySelector('#deploy-sync-text');
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
      lang: state.ctx?.locale === 'en' ? 'en' : 'de'
    });
    btnDeploy.disabled = !actionState.deployReady;
    btnDeploy.setAttribute('aria-disabled', String(btnDeploy.disabled));
    btnDeploy.title = actionState.deployReady
      ? (state.ctx?.locale === 'en' ? 'Start app creation through CTOX' : 'App-Erstellung durch CTOX starten')
      : actionState.diagnostic;
    btnDeploy.dataset.state = actionState.deployReady ? 'ready' : 'blocked';
    if (!state.isDeploying && syncText && syncDot) {
      syncDot.style.background = '';
      syncText.textContent = actionState.diagnostic;
      syncDot.className = actionState.deployReady ? 'sync-dot is-ready' : 'sync-dot is-blocked';
    }
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
    const locale = state.ctx?.locale === 'en' ? 'en' : 'de';
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
        const confirmed = await showBusinessConfirm(`Datentabelle "${name}" aus den optionalen Vorgaben entfernen?`, {
          title: 'Datentabelle entfernen',
          confirmLabel: 'Entfernen',
          cancelLabel: 'Abbrechen',
          kind: 'danger'
        });
        if (!confirmed) return;
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
      addConsoleLog(`[INFO] Lade bestehende App für Änderung von '${id}'...`, 'info');
      try {
        const manifest = await fetch(`installed-modules/${id}/module.json`).then((res) => {
          if (!res.ok) throw new Error(`App '${id}' konnte nicht geladen werden.`);
          return res.json();
        });
        inputId.value = manifest.id || id;
        inputTitle.value = manifest.title || '';
        inputDesc.value = manifest.description || '';
        selectCategory.value = manifest.category || 'Management';
        selectArchetype.value = manifest.archetype || manifest.store?.archetype || 'record-workbench';
        selectLayout.value = manifest.layout?.shell || 'windowed';
        inputRequest.value = `Ändere ${manifest.title || id}: ${manifest.description || ''}`;
        state.appVersion = /^\d+\.\d+\.\d+$/.test(String(manifest.version || '')) ? String(manifest.version) : '0.1.0';
        state.appCollections = Array.isArray(manifest.collections) ? manifest.collections : [];
        renderCollectionsList(host);
        syncStateFromInputs();
        addConsoleLog(`[SUCCESS] App-Kontext für '${manifest.title || id}' geladen. Passe den Auftrag an und starte CTOX.`, 'success');
      } catch (err) {
        // Fall back to the catalog entry we already hold (source apps are not
        // always fetchable as an installed manifest).
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
          addConsoleLog(`[INFO] App-Kontext für '${app.title || id}' aus dem Katalog geladen.`, 'info');
        } else {
          addConsoleLog(`[ERROR] Fehler beim Laden des Upgrades: ${err.message}`, 'error');
        }
      }
      updateCreatorActionState();
      inputRequest.focus();
    },
  };

  renderCollectionsList(host);
  updateCreatorActionState();

  function addInspirationUrl() {
    const url = normalizeInspirationUrl(inspirationInput.value);
    if (!url) {
      state.ctx.notifications?.show?.({
        type: 'warning',
        title: state.t('invalidUrlTitle', 'URL prüfen'),
        message: state.t('invalidUrlMessage', 'Bitte füge eine vollständige http- oder https-URL ein.'),
      });
      inspirationInput.focus();
      return;
    }
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
    const locale = state.ctx?.locale === 'en' ? 'en' : 'de';
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
        state.ctx.notifications.show({
          title: 'Auftrag fehlt',
          message: 'Bitte beschreibe die App, bevor du den CTOX-Auftrag startest.',
          type: 'warning'
        });
        addConsoleLog('[BLOCKED] App-Auftrag verhindert: Beschreibung fehlt.', 'warning');
        updateCreatorActionState();
        return;
      }

      const actionState = updateCreatorActionState();
      if (!actionState.deployReady) {
        addConsoleLog(`[BLOCKED] App-Auftrag verhindert: ${actionState.diagnostic}`, 'warning');
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
      await triggerAppDeployment(host, updateCreatorActionState);
    } catch (e) {
      console.error('[ERROR] triggerAppDeployment failed:', e);
      state.isDeploying = false;
      updateCreatorActionState();
    }
  });

  // Hash-based upgrade entry point (e.g. #creator?upgrade=<id>) reuses the same
  // composer prefill path as selecting an app shard in the left column.
  const hash = window.location.hash || '';
  const queryStr = hash.includes('?') ? hash.split('?')[1] : '';
  const upgradeAppId = state.ctx?.args?.upgrade || new URLSearchParams(queryStr).get('upgrade');
  if (upgradeAppId) void state.composer.prefillUpgrade(upgradeAppId);
}

function addConsoleLog(text, type = '') {
  console.log(text);
  const container = document.querySelector('#console-logs-container');
  if (!container) return;
  const el = document.createElement('div');
  el.className = `console-log-entry ${type}`;
  el.textContent = text;
  container.appendChild(el);
  container.scrollTop = container.scrollHeight;
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

  return {
    command_id: `app-create-${moduleId}-${now}`,
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

async function triggerAppDeployment(host, updateCreatorActionState = () => {}) {
  const syncDot = host.querySelector('#deploy-sync-dot');
  const syncText = host.querySelector('#deploy-sync-text');
  const btnDeploy = host.querySelector('#btn-deploy-app');

  const request = host.querySelector('#app-request-input')?.value?.trim() || '';
  const appId = state.appId;
  const appTitle = state.appTitle;
  const appDesc = state.appDesc;
  const collections = state.appCollections;
  const appLayout = state.appLayout;
  const appVersion = /^\d+\.\d+\.\d+$/.test(String(state.appVersion || '').trim())
    ? String(state.appVersion).trim()
    : '0.1.0';

  if (!request) {
    state.ctx.notifications.show({
      title: 'Fehler beim Vorbereiten',
      message: 'Bitte beschreibe die gewünschte App.',
      type: 'error'
    });
    addConsoleLog('[FEHLER] App-Auftrag fehlt.', 'error');
    return;
  }

  // Visual lock UI
  state.isDeploying = true;
  btnDeploy.disabled = true;
  syncDot.className = 'sync-dot is-saving';
  syncText.textContent = state.t('deploySaving', 'Lege CTOX-Auftrag an...');
  updateCreatorActionState();

  addConsoleLog('==================================================', 'info');
  addConsoleLog('[START] Übergabe an CTOX App Creator Agent...', 'info');

  try {
    const actorContext = (session) => {
      const user = session?.user || {};
      return {
        id: user.id || 'admin',
        display_name: user.display_name || user.name || 'Admin',
        role: user.role || 'admin',
        is_admin: user.is_admin !== undefined ? Boolean(user.is_admin) : true,
      };
    };

    const command = buildAppCreateCommand({
      appId,
      appTitle,
      appDesc,
      appCategory: state.appCategory,
      appArchetype: state.appArchetype,
      appLayout,
      appCollections: collections,
      appVersion,
      inspirationUrls: state.inspirationUrls,
      instruction: request,
      actor: actorContext(state.ctx.session),
    });

    addConsoleLog(`[QUEUE] Sende ${command.command_type} für ${command.payload.module_id}...`, 'info');
    const result = await state.ctx.commandBus.dispatch(command, { until: 'terminal' });

    addConsoleLog('==================================================', 'success');
    addConsoleLog(`[SUCCESS] CTOX App-Erstellung für '${command.payload.app_title}' wurde gestartet.`, 'success');
    if (result?.task_id) addConsoleLog(`[TASK] ${result.task_id}`, 'info');

    state.ctx.notifications.show({
      title: 'App erstellt',
      message: `'${command.payload.app_title}' ist jetzt als Business-OS-App verfügbar.`,
      type: 'success'
    });

    syncDot.className = 'sync-dot';
    syncText.textContent = state.t('deployInstalled', 'App erstellt');
    state.isDeploying = false;
    updateCreatorActionState();

  } catch (error) {
    addConsoleLog(`[FEHLER] App-Auftrag konnte nicht angelegt werden: ${error.message}`, 'error');
    console.error(error);

    state.ctx.notifications.show({
      title: 'App-Erstellung fehlgeschlagen',
      message: `Der CTOX-Auftrag konnte nicht angelegt werden: ${error.message}`,
      type: 'error'
    });

    syncDot.className = 'sync-dot';
    syncDot.style.background = 'var(--danger)';
    syncText.textContent = state.t('deployFailed', 'Fehler beim Speichern');
    state.isDeploying = false;
    updateCreatorActionState();
  }
}

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}


// --- Creator module i18n -----------------------------------------------------
// Loads locales/<lang>.json for the creator UI itself (the request templates
// carry their own labels). German markup text is the fallback.
async function loadCreatorMessages(locale) {
  const lang = locale === 'en' ? 'en' : 'de';
  try {
    const response = await fetch(new URL(`./locales/${lang}.json`, import.meta.url));
    if (!response.ok) throw new Error(String(response.status));
    return await response.json();
  } catch {
    return {};
  }
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
