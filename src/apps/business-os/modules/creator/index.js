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
  contextMenu: null,
  contextMenuCleanup: null,
  resizerCleanup: null,
  catalogSubscription: null,
  commandSubscription: null,
  streamGeneration: 0,
  installedApps: [],
  creatorRequests: [],
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
  const html = await fetch(new URL('./index.html', import.meta.url)).then(res => res.text());
  ctx.host.innerHTML = html;
  applyCreatorTranslations(ctx.host, state.t);

  // 3. Wire UI events
  wireUi(ctx.host);

  // 4. Render the operable shell immediately. Catalog and command hydration
  // can wait for a cold WebRTC lease without blocking the window from opening.
  renderCreatorRightRail(ctx.host);
  void startCreatorDataStreams(ctx, ctx.host, streamGeneration).catch((error) => {
    if (streamGeneration !== state.streamGeneration) return;
    addConsoleLog(`[WARN] Creator-Daten konnten nicht geladen werden: ${error.message}`, 'warning');
  });

  // 5. Initialize CTOX unified context menu

  // 6. Setup column resizer
  state.resizerCleanup = setupResizers(ctx.host);

  return () => {
    if (streamGeneration === state.streamGeneration) state.streamGeneration += 1;
    state.contextMenuCleanup?.();
    state.resizerCleanup?.();
    cleanupSubscription(state.catalogSubscription);
    cleanupSubscription(state.commandSubscription);
    state.catalogSubscription = null;
    state.commandSubscription = null;
    state.contextMenu?.remove();
    state.contextMenu = null;
    console.log('[creator] Module unmounted and cleaned up.');
  };
}

async function ensureStyles() {
  if (document.querySelector('link[data-module-styles="creator"]')) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = new URL('./index.css', import.meta.url).href;
  link.dataset.moduleStyles = 'creator';
  document.head.append(link);
}

function setupResizers() {
  return () => {};
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
    renderCreatorRightRail(host);
  }) || null;

  state.commandSubscription = commandColl?.find?.()?.$?.subscribe?.((docs) => {
    if (streamGeneration !== state.streamGeneration || !host.isConnected) return;
    state.creatorRequests = normalizeCreatorRequestSuggestions(docs?.map((doc) => doc?.toJSON?.() || doc) || []);
    renderCreatorRightRail(host);
  }) || null;

  renderCreatorRightRail(host);
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

function renderCreatorRightRail(host) {
  const installedList = host.querySelector('[data-creator-installed-list]');
  const installedEmpty = host.querySelector('[data-creator-installed-empty]');
  const requestsList = host.querySelector('[data-creator-requests-list]');
  const requestsEmpty = host.querySelector('[data-creator-requests-empty]');

  if (installedList && installedEmpty) {
    installedList.innerHTML = state.installedApps.map(renderInstalledAppCard).join('');
    installedEmpty.hidden = state.installedApps.length > 0;
    installedList.hidden = state.installedApps.length === 0;
  }

  if (requestsList && requestsEmpty) {
    requestsList.innerHTML = state.creatorRequests.map(renderCreatorRequestCard).join('');
    requestsEmpty.hidden = state.creatorRequests.length > 0;
    requestsList.hidden = state.creatorRequests.length === 0;
  }
}

function renderInstalledAppCard(app) {
  return `
    <article class="ctox-list-item creator-mini-card" data-creator-installed-app="${escapeHtml(app.id)}" data-context-record-id="${escapeHtml(app.id)}" data-context-record-type="application" data-context-label="${escapeHtml(app.title || app.id)}">
      <div class="creator-mini-card-main">
        <strong>${escapeHtml(app.title)}</strong>
        <span class="creator-mini-card-meta">${escapeHtml(app.category)} · ${escapeHtml(app.version)}</span>
        ${app.description ? `<p>${escapeHtml(app.description)}</p>` : ''}
      </div>
      <div class="creator-mini-actions">
        <button type="button" class="ctox-icon-button" data-open-installed-app="${escapeHtml(app.id)}" title="App öffnen" aria-label="${escapeHtml(app.title)} öffnen">
          ${creatorActionIcon('open')}
        </button>
        <button type="button" class="ctox-icon-button" data-upgrade-installed-app="${escapeHtml(app.id)}" title="Upgrade vorbereiten" aria-label="${escapeHtml(app.title)} Upgrade vorbereiten">
          ${creatorActionIcon('upload')}
        </button>
      </div>
    </article>
  `;
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

function renderCreatorRequestCard(item) {
  const request = item.request.length > 140 ? `${item.request.slice(0, 137)}...` : item.request;
  return `
    <article class="ctox-list-item creator-mini-card" data-creator-request="${escapeHtml(item.id)}" data-context-record-id="${escapeHtml(item.id)}" data-context-record-type="app-request" data-context-label="${escapeHtml(item.title || item.id)}">
      <div class="creator-mini-card-main">
        <strong>${escapeHtml(item.title)}</strong>
        <span class="ctox-badge">${escapeHtml(item.status)}</span>
        <p>${escapeHtml(request)}</p>
      </div>
      <div class="creator-mini-actions">
        <button type="button" class="ctox-icon-button" data-use-creator-request="${escapeHtml(item.id)}" title="Auftrag uebernehmen" aria-label="Auftrag uebernehmen">
          ${creatorActionIcon('download')}
        </button>
      </div>
    </article>
  `;
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
  const requestDiagnostics = host.querySelector('#creator-request-diagnostics');
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
    if (requestDiagnostics) {
      requestDiagnostics.textContent = actionState.diagnostic;
      requestDiagnostics.dataset.state = actionState.deployReady ? 'ready' : actionState.hasRequest ? 'pending' : 'blocked';
    }
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

  host.querySelector('[data-creator-right-body]')?.addEventListener('click', (event) => {
    const openButton = event.target.closest('[data-open-installed-app]');
    const upgradeButton = event.target.closest('[data-upgrade-installed-app]');
    const requestButton = event.target.closest('[data-use-creator-request]');

    if (openButton) {
      window.location.hash = `#${encodeURIComponent(openButton.dataset.openInstalledApp || '')}`;
      return;
    }

    if (upgradeButton) {
      window.location.hash = `#creator?upgrade=${encodeURIComponent(upgradeButton.dataset.upgradeInstalledApp || '')}`;
      return;
    }

    if (requestButton) {
      const request = state.creatorRequests.find((item) => item.id === requestButton.dataset.useCreatorRequest);
      if (!request) return;
      inputRequest.value = request.request;
      updateCreatorActionState();
      addConsoleLog(`[INFO] CTOX App-Auftrag '${request.title}' uebernommen.`, 'info');
      inputRequest.focus();
    }
  });

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

  // Intercept and parse hash parameters for Upgrade preloading
  (async () => {
    const hash = window.location.hash || '';
    const queryStr = hash.includes('?') ? hash.split('?')[1] : '';
    const params = new URLSearchParams(queryStr);
    const upgradeAppId = state.ctx?.args?.upgrade || params.get('upgrade');

    if (upgradeAppId) {
      try {
        addConsoleLog(`[INFO] Lade bestehende App für Änderung von '${upgradeAppId}'...`, 'info');
        const manifestUrl = `installed-modules/${upgradeAppId}/module.json`;
        const manifest = await fetch(manifestUrl).then(res => {
          if (!res.ok) throw new Error(`App '${upgradeAppId}' konnte nicht geladen werden.`);
          return res.json();
        });

        if (inputId) inputId.value = manifest.id || upgradeAppId;
        if (inputTitle) inputTitle.value = manifest.title || '';
        if (inputDesc) inputDesc.value = manifest.description || '';
        if (selectCategory) selectCategory.value = manifest.category || 'Management';
        if (selectArchetype) selectArchetype.value = manifest.archetype || manifest.store?.archetype || 'record-workbench';
        if (selectLayout) selectLayout.value = manifest.layout?.shell || 'windowed';
        if (inputRequest) inputRequest.value = `Ändere ${manifest.title || upgradeAppId}: ${manifest.description || ''}`;
        state.appVersion = /^\d+\.\d+\.\d+$/.test(String(manifest.version || ''))
          ? String(manifest.version)
          : '0.1.0';
        const baseCollections = Array.isArray(manifest.collections) ? manifest.collections : [];
        state.appCollections = baseCollections;

        renderCollectionsList(host);
        syncStateFromInputs();

        addConsoleLog(`[SUCCESS] App-Kontext für '${manifest.title || upgradeAppId}' geladen. Passe den Auftrag an und starte CTOX.`, 'success');
        updateCreatorActionState();
      } catch (err) {
        addConsoleLog(`[ERROR] Fehler beim Laden des Upgrades: ${err.message}`, 'error');
        updateCreatorActionState();
      }
    }
  })();
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

function initCreatorContextMenu(state) {
  state.contextMenu?.remove();
  const menu = document.createElement('div');
  menu.className = 'ctox-context-menu creator-context-menu';
  menu.hidden = true;
  document.body.append(menu);
  state.contextMenu = menu;

  const handleContextMenu = (event) => {
    if (state.ctx.module?.id !== 'creator') return;
    const context = creatorCommandContextFromElement(state, event.target);
    event.preventDefault();
    event.stopPropagation();
    renderCreatorContextMenu(state, context, event.clientX, event.clientY);
  };
  const handleOutsideClick = (event) => {
    if (state.contextMenu?.contains(event.target)) return;
    hideCreatorContextMenu(state);
  };
  const handleEscape = (event) => {
    if (event.key === 'Escape') hideCreatorContextMenu(state);
  };

  window.addEventListener('click', handleOutsideClick, { capture: true });
  window.addEventListener('keydown', handleEscape);

  return () => {
    window.removeEventListener('click', handleOutsideClick, { capture: true });
    window.removeEventListener('keydown', handleEscape);
    hideCreatorContextMenu(state);
  };
}

function hideCreatorContextMenu(state) {
  if (state.contextMenu) state.contextMenu.hidden = true;
}

function canModifyCreatorApp(state) {
  if (typeof state.ctx.canModifyModule === 'function' && state.ctx.canModifyModule()) return true;
  const user = state.ctx.session?.user || {};
  const role = String(user.role || (user.is_admin ? 'admin' : 'user')).trim().toLowerCase().replace(/^business_os_/, '');
  return ['admin', 'chef'].includes(role);
}

function creatorCommandContextFromElement(state, target) {
  const element = target?.nodeType === Node.ELEMENT_NODE ? target : target?.parentElement;

  return {
    module: 'creator',
    column: 'workspace',
    record_type: 'app-request',
    record_id: state.appId || 'creator',
    label: state.appTitle || 'Creator App Request',
    app_id: state.appId || '',
    app_title: state.appTitle || '',
    app_desc: state.appDesc || '',
    app_category: state.appCategory || '',
    app_layout: state.appLayout || '',
    app_collections: state.appCollections || [],
    selected_text: String(window.getSelection?.()?.toString?.() || '').trim().slice(0, 1000),
    clicked_text: String(element?.innerText || element?.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 500),
  };
}

function renderCreatorContextMenu(state, context, x, y) {
  const canModifyApp = canModifyCreatorApp(state);
  state.contextMenu.innerHTML = `
    <form class="creator-context-chat" data-creator-context-chat-form>
      <header>
        <div>
          <strong>Chat to CTOX</strong>
          <span>${escapeHtml(context.label || 'Creator')}</span>
        </div>
        <button type="button" data-creator-context-close aria-label="Schließen">×</button>
      </header>
      ${canModifyApp ? `
        <div class="ctox-context-mode" role="radiogroup" aria-label="CTOX Aufgabe">
          <label><input type="radio" name="contextMode" value="data" checked /> Mit Daten arbeiten</label>
          <label><input type="radio" name="contextMode" value="app" /> App modifizieren</label>
        </div>
      ` : ''}
      <textarea data-creator-context-message placeholder="Was soll CTOX mit diesem App-Auftrag tun?"></textarea>
      <footer>
        <span data-creator-context-status></span>
        <button type="submit">Senden</button>
      </footer>
    </form>
  `;
  state.contextMenu.hidden = false;
  state.contextMenu.style.left = '0px';
  state.contextMenu.style.top = '0px';
  const rect = state.contextMenu.getBoundingClientRect();
  const clampNumber = (val, min, max) => Math.min(max, Math.max(min, val));
  const maxLeft = Math.max(8, window.innerWidth - rect.width - 8);
  const maxTop = Math.max(8, window.innerHeight - rect.height - 8);
  state.contextMenu.style.left = `${clampNumber(x, 8, maxLeft)}px`;
  state.contextMenu.style.top = `${clampNumber(y, 8, maxTop)}px`;

  const form = state.contextMenu.querySelector('[data-creator-context-chat-form]');
  const textarea = state.contextMenu.querySelector('[data-creator-context-message]');
  state.contextMenu.querySelector('[data-creator-context-close]')?.addEventListener('click', () => hideCreatorContextMenu(state));
  form?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const mode = canModifyApp ? (new FormData(form).get('contextMode') || 'data') : 'data';
    await dispatchCreatorContextChat(state, context, textarea?.value || '', mode);
  });
  requestAnimationFrame(() => textarea?.focus());
}

async function dispatchCreatorContextChat(state, context, message, mode = 'data') {
  const trimmed = String(message || '').trim();
  const status = state.contextMenu?.querySelector('[data-creator-context-status]');
  if (!trimmed) {
    if (status) status.textContent = 'Nachricht fehlt.';
    return;
  }

  const safeMode = mode === 'app' && canModifyCreatorApp(state) ? 'app' : 'data';
  if (!document.querySelector('[data-ctox-chat-root]')) {
    if (status) status.textContent = 'Chat ist noch nicht bereit.';
    return;
  }
  if (status) status.textContent = 'Oeffne Chat...';
  const title = `${safeMode === 'app' ? 'Creator App modifizieren' : 'App-Auftrag bearbeiten'} · ${context.label || 'Creator'}`;
  const instruction = safeMode === 'app'
    ? `Modifiziere die App-Creator-App anhand dieser Admin-Anweisung. Kontext nur als UI-Bezug verwenden, App-Auftraege selbst nicht als primäres Ziel verändern.\n\n${trimmed}`
    : trimmed;

  window.dispatchEvent(new CustomEvent('ctox-business-os-chat-submit', {
    detail: {
      text: trimmed,
      module: 'creator',
      source_title: 'App Creator',
      command_type: safeMode === 'app' ? 'ctox.business_os.app.modify' : 'business_os.chat.task',
      record_id: safeMode === 'app' ? 'creator' : (context.record_id || 'creator'),
      title,
      instruction,
      payload: {
        title,
        instruction,
        request: trimmed,
        user_message: trimmed,
        mode: safeMode,
        target: safeMode === 'app' ? 'app' : 'data',
        context,
        thread_key: 'business-os/creator',
      },
      client_context: {
        action: 'context-chat',
        mode: safeMode,
        column: context.column,
        record_type: context.record_type,
        app_id: context.app_id,
        app_title: context.app_title,
      },
    },
  }));
  hideCreatorContextMenu(state);
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
