import { loadModuleMessages } from '../../shared/i18n.js';
import { CtoxResizer } from '../../shared/resizer.js';
import {
  decodeBase64Utf8,
  extractCompanyRowsFromText,
  openUniversalImporter,
} from '../../shared/universal-importer.js';

const BUILD = '20260717-kit-standard1';
const CUSTOMERS_LAYOUT_KEY = 'ctox.businessOs.customers.columnLayout';
const CUSTOMERS_COLLECTIONS = Object.freeze([
  'business_commands',
  'customer_accounts',
  'customer_contacts',
  'customer_opportunities',
  'customer_tasks',
  'customer_notes',
  'customer_activities',
  'customer_files',
  'customer_views',
  'customer_view_filters',
  'customer_view_sorts',
  'customer_import_batches',
  'customer_dedupe_candidates',
]);
const OUTBOUND_HANDOFF_COLLECTIONS = Object.freeze([
  'outbound_companies',
  'outbound_pipeline_items',
]);
const OPTIONAL_LINK_COLLECTIONS = Object.freeze([
  'communication_messages',
  'calendar_events',
  'documents',
  'notes',
  'spreadsheets',
]);
const MUTATING_ACTIONS = new Set([
  'create-account',
  'edit-account',
  'archive-account',
  'create-contact',
  'edit-contact',
  'archive-contact',
  'create-opportunity',
  'edit-opportunity',
  'move-opportunity',
  'close-won',
  'close-lost',
  'create-task',
  'edit-task',
  'complete-task',
  'create-note',
  'edit-note',
  'import-outbound',
  'dedupe-merge',
  'dedupe-keep-existing',
  'dedupe-create-new',
  'dedupe-skip',
  'import-customers',
  'save-view',
]);

const ACCOUNT_STATUS_LABELS = Object.freeze({
  active: 'Aktiv',
  inactive: 'Inaktiv',
  archived: 'Archiviert',
});

const STAGE_LABELS = Object.freeze({
  prospect: 'Prospect',
  onboarding: 'Onboarding',
  active: 'Aktiv',
  renewal: 'Renewal',
  expansion: 'Expansion',
  at_risk: 'Gefährdet',
  churned: 'Churned',
  archived: 'Archiviert',
});

const HEALTH_LABELS = Object.freeze({
  unknown: 'Unbekannt',
  healthy: 'Healthy',
  neutral: 'Neutral',
  at_risk: 'At Risk',
  critical: 'Kritisch',
});

const OPPORTUNITY_STAGE_LABELS = Object.freeze({
  qualification: 'Qualification',
  proposal: 'Proposal',
  negotiation: 'Negotiation',
  committed: 'Committed',
  closed_won: 'Closed Won',
  closed_lost: 'Closed Lost',
});

const OPPORTUNITY_TYPE_LABELS = Object.freeze({
  new_business: 'New Business',
  expansion: 'Expansion',
  renewal: 'Renewal',
});

const TASK_STATUS_LABELS = Object.freeze({
  open: 'Offen',
  in_progress: 'In Arbeit',
  completed: 'Erledigt',
  cancelled: 'Abgebrochen',
});

const labels = {
  de: {
    title: 'Kunden',
    presenceEditing: '{0} bearbeitet gerade',
    presenceViewing: '{0} sieht sich das gerade an',
    scope: 'Segmente',
    sync: 'Abgleich',
    savedViews: 'Ansichten',
    saveView: 'Ansicht speichern',
    allAccounts: 'Alle Kunden',
    activeCustomers: 'Aktive Kunden',
    renewal: 'Renewal',
    expansion: 'Expansion',
    atRisk: 'Gefährdet',
    clearFilters: 'Filter zurücksetzen',
    filters: 'Filter',
    allStages: 'Alle Stages',
    allHealth: 'Alle Status',
    visibleCount: '{0} sichtbar',
    actionKicker: 'Aktion',
    search: 'Kunden suchen',
    searchContacts: 'Kontakte suchen',
    searchOpportunities: 'Opportunities suchen',
    searchOutbound: 'Outbound suchen',
    searchDedupe: 'Dubletten suchen',
    newCustomer: 'Kunde anlegen',
    editCustomer: 'Kunde bearbeiten',
    archiveCustomer: 'Archivieren',
    newContact: 'Kontakt anlegen',
    editContact: 'Kontakt bearbeiten',
    archiveContact: 'Kontakt archivieren',
    newOpportunity: 'Opportunity anlegen',
    editOpportunity: 'Opportunity bearbeiten',
    closeWon: 'Won',
    closeLost: 'Lost',
    board: 'Board',
    table: 'Tabelle',
    refresh: 'Aktualisieren',
    linkedDataLimited: 'Verknuepfte Daten eingeschraenkt',
    accounts: 'Kunden',
    contacts: 'Kontakte',
    opportunities: 'Opportunities',
    handoff: 'Übergabe',
    dedupe: 'Dubletten',
    importFromOutbound: 'Übernehmen',
    importCustomers: 'Importieren',
    exportCustomers: 'Exportieren',
    imported: 'Importiert',
    needsReview: 'Review',
    resolveDedupe: 'Entscheiden',
    keepExisting: 'Bestehend behalten',
    createNew: 'Neu anlegen',
    merge: 'Zusammenführen',
    skip: 'Überspringen',
    matchType: 'Match',
    confidence: 'Confidence',
    source: 'Quelle',
    existingRecord: 'Bestehend',
    importBatch: 'Import Batch',
    tasks: 'Aufgaben',
    activities: 'Aktivitäten',
    overview: 'Übersicht',
    notes: 'Notizen',
    timeline: 'Timeline',
    files: 'Dateien',
    apps: 'Apps',
    linkedApps: 'Verknüpfte Apps',
    openApp: 'Öffnen',
    conversations: 'Conversations',
    calendar: 'Kalender',
    documents: 'Dokumente',
    spreadsheets: 'Tabellen',
    outbound: 'Outbound',
    available: 'Verfügbar',
    linkedRecords: 'Verknüpfungen',
    noAppLinks: 'Keine App-Verknüpfungen',
    noAppLinksBody: 'Verknüpfte Kommunikation, Termine, Dokumente, Notizen und Tabellen erscheinen hier, sobald die Ziel-App Daten mit Kundenbezug bereitstellt.',
    newTask: 'Aufgabe anlegen',
    editTask: 'Aufgabe bearbeiten',
    completeTask: 'Erledigen',
    newNote: 'Notiz anlegen',
    editNote: 'Notiz bearbeiten',
    dueDate: 'Fälligkeit',
    assignee: 'Zuständig',
    body: 'Text',
    format: 'Format',
    noTasks: 'Keine Aufgaben',
    noNotes: 'Keine Notizen',
    noFiles: 'Keine Dateien',
    noTimeline: 'Keine Timeline-Einträge',
    noCustomers: 'Noch keine Kunden',
    noCustomersBody: 'Lege einen Kunden an oder importiere qualifizierte Firmen aus Outbound.',
    noContacts: 'Noch keine Kontakte',
    noContactsBody: 'Lege Kontakte am ausgewählten Kunden an oder übernimm sie aus Outbound.',
    noOpportunities: 'Noch keine Opportunities',
    noOpportunitiesBody: 'Lege eine Opportunity am ausgewählten Kunden an oder importiere Renewals aus Bestandssignalen.',
    noOutbound: 'Keine Outbound-Übergaben',
    noOutboundBody: 'Qualifizierte Outbound-Firmen erscheinen hier, sobald Outbound-Daten lokal synchronisiert sind.',
    noDedupe: 'Keine offenen Dubletten',
    noDedupeBody: 'Domain- oder E-Mail-Treffer aus Imports landen hier zur Entscheidung.',
    inspectorEmpty: 'Kein Kunde ausgewählt',
    inspectorEmptyBody: 'Wähle einen Kunden in der Liste, um Kontakte, Opportunities und Aktivitäten zu sehen.',
    accountName: 'Name',
    domain: 'Domain',
    industry: 'Branche',
    status: 'Status',
    stage: 'Stage',
    health: 'Gesundheit',
    owner: 'Betreuung',
    arr: 'ARR',
    nextAction: 'Nächste Aktion',
    lastActivity: 'Letzte Aktivität',
    openTasks: 'Offene Aufgaben',
    primary: 'Primär',
    opportunityName: 'Opportunity',
    type: 'Typ',
    amount: 'Betrag',
    closeDate: 'Abschluss',
    probability: 'Wahrscheinlichkeit',
    moveTo: 'Verschieben nach',
    pipelineValue: 'Pipeline',
    weightedValue: 'Gewichtet',
    firstName: 'Vorname',
    lastName: 'Nachname',
    email: 'E-Mail',
    phone: 'Telefon',
    jobTitle: 'Rolle',
    city: 'Ort',
    create: 'Anlegen',
    save: 'Speichern',
    cancel: 'Abbrechen',
    requiredName: 'Name ist erforderlich.',
    requiredContact: 'Kontaktname oder E-Mail ist erforderlich.',
    requiredAccount: 'Kunde ist erforderlich.',
    requiredOpportunity: 'Opportunity-Name ist erforderlich.',
    requiredTask: 'Aufgabentitel ist erforderlich.',
    requiredNote: 'Notiztitel oder Text ist erforderlich.',
    commandPending: 'Änderung gespeichert: {0}',
    commandAudit: 'Letzte Änderungen',
    commandAuditEmpty: 'Noch keine Änderungen',
    commandAuditBody: 'Neue Aktionen erscheinen hier, sobald sie lokal gespeichert sind.',
    commandPendingStatus: 'Ausstehend',
    commandCompletedStatus: 'Abgeschlossen',
    commandFailedStatus: 'Fehlgeschlagen',
    commandFailed: 'Änderung konnte nicht gespeichert werden: {0}',
    commandUnavailable: 'Änderungen sind gerade nicht verfügbar.',
    loadingStatus: 'Aktualisiert...',
    permissionReadOnly: 'Nur Lesen',
    permissionReadOnlyBody: 'Deine Rolle darf Kundendaten ansehen, aber nicht bearbeiten.',
    permissionDenied: 'Keine Berechtigung für Kunden-Änderungen.',
    workbench: 'Arbeitsbereich',
    opportunityView: 'Opportunity Ansicht',
    recordDetails: 'Record Details',
    selectAccount: 'Kunde auswählen: {0}',
    selectContact: 'Kontakt auswählen: {0}',
    selectOpportunity: 'Opportunity auswählen: {0}',
    selectOutbound: 'Outbound-Übergabe auswählen: {0}',
    selectDedupe: 'Dublette auswählen: {0}',
    sortBy: 'Sortieren nach {0}',
    sortAscending: 'Aufsteigend sortiert',
    sortDescending: 'Absteigend sortiert',
  },
  en: {
    title: 'Customers',
    presenceEditing: '{0} is editing right now',
    presenceViewing: '{0} is viewing this',
    scope: 'Scope',
    sync: 'Sync',
    savedViews: 'Views',
    saveView: 'Save view',
    allAccounts: 'All customers',
    activeCustomers: 'Active customers',
    renewal: 'Renewal',
    expansion: 'Expansion',
    atRisk: 'At risk',
    clearFilters: 'Clear filters',
    filters: 'Filters',
    allStages: 'All stages',
    allHealth: 'All health',
    visibleCount: '{0} visible',
    actionKicker: 'Action',
    search: 'Search customers',
    searchContacts: 'Search contacts',
    searchOpportunities: 'Search opportunities',
    searchOutbound: 'Search outbound',
    searchDedupe: 'Search dedupe',
    newCustomer: 'New customer',
    editCustomer: 'Edit customer',
    archiveCustomer: 'Archive',
    newContact: 'New contact',
    editContact: 'Edit contact',
    archiveContact: 'Archive contact',
    newOpportunity: 'New opportunity',
    editOpportunity: 'Edit opportunity',
    closeWon: 'Won',
    closeLost: 'Lost',
    board: 'Board',
    table: 'Table',
    refresh: 'Refresh',
    linkedDataLimited: 'Linked data limited',
    accounts: 'Accounts',
    contacts: 'Contacts',
    opportunities: 'Opportunities',
    handoff: 'Handoff',
    dedupe: 'Dedupe',
    importFromOutbound: 'Import',
    importCustomers: 'Import',
    exportCustomers: 'Export',
    imported: 'Imported',
    needsReview: 'Review',
    resolveDedupe: 'Resolve',
    keepExisting: 'Keep existing',
    createNew: 'Create new',
    merge: 'Merge',
    skip: 'Skip',
    matchType: 'Match',
    confidence: 'Confidence',
    source: 'Source',
    existingRecord: 'Existing',
    importBatch: 'Import batch',
    tasks: 'Tasks',
    activities: 'Activities',
    overview: 'Overview',
    notes: 'Notes',
    timeline: 'Timeline',
    files: 'Files',
    apps: 'Apps',
    linkedApps: 'Linked apps',
    openApp: 'Open',
    conversations: 'Conversations',
    calendar: 'Calendar',
    documents: 'Documents',
    spreadsheets: 'Spreadsheets',
    outbound: 'Outbound',
    available: 'Available',
    linkedRecords: 'Links',
    noAppLinks: 'No app links',
    noAppLinksBody: 'Linked communication, meetings, documents, notes and spreadsheets appear here once target apps provide customer context.',
    newTask: 'New task',
    editTask: 'Edit task',
    completeTask: 'Complete',
    newNote: 'New note',
    editNote: 'Edit note',
    dueDate: 'Due date',
    assignee: 'Assignee',
    body: 'Body',
    format: 'Format',
    noTasks: 'No tasks',
    noNotes: 'No notes',
    noFiles: 'No files',
    noTimeline: 'No timeline entries',
    noCustomers: 'No customers yet',
    noCustomersBody: 'Create a customer or import qualified companies from Outbound.',
    noContacts: 'No contacts yet',
    noContactsBody: 'Create contacts on the selected account or import them from Outbound.',
    noOpportunities: 'No opportunities yet',
    noOpportunitiesBody: 'Create an opportunity on the selected account or import renewals from customer signals.',
    noOutbound: 'No outbound handoffs',
    noOutboundBody: 'Qualified outbound companies appear here once outbound data is synced locally.',
    noDedupe: 'No open dedupe cases',
    noDedupeBody: 'Domain or email matches from imports land here for review.',
    inspectorEmpty: 'No customer selected',
    inspectorEmptyBody: 'Select an account to inspect contacts, opportunities and activity.',
    accountName: 'Name',
    domain: 'Domain',
    industry: 'Industry',
    status: 'Status',
    stage: 'Stage',
    health: 'Health',
    owner: 'Owner',
    arr: 'ARR',
    nextAction: 'Next action',
    lastActivity: 'Last activity',
    openTasks: 'Open tasks',
    primary: 'Primary',
    opportunityName: 'Opportunity',
    type: 'Type',
    amount: 'Amount',
    closeDate: 'Close date',
    probability: 'Probability',
    moveTo: 'Move to',
    pipelineValue: 'Pipeline',
    weightedValue: 'Weighted',
    firstName: 'First name',
    lastName: 'Last name',
    email: 'Email',
    phone: 'Phone',
    jobTitle: 'Role',
    city: 'City',
    create: 'Create',
    save: 'Save',
    cancel: 'Cancel',
    requiredName: 'Name is required.',
    requiredContact: 'Contact name or email is required.',
    requiredAccount: 'Account is required.',
    requiredOpportunity: 'Opportunity name is required.',
    requiredTask: 'Task title is required.',
    requiredNote: 'Note title or body is required.',
    commandPending: 'Command recorded: {0}',
    commandAudit: 'Command log',
    commandAuditEmpty: 'No customer commands yet',
    commandAuditBody: 'New actions appear here once business_commands is synced locally.',
    commandPendingStatus: 'Pending',
    commandCompletedStatus: 'Completed',
    commandFailedStatus: 'Failed',
    commandFailed: 'Command could not be recorded: {0}',
    commandUnavailable: 'Command bus unavailable.',
    permissionReadOnly: 'Read-only',
    permissionReadOnlyBody: 'Your role can inspect customer data, but cannot modify it.',
    permissionDenied: 'No permission for customer changes.',
    workbench: 'Workbench',
    opportunityView: 'Opportunity view',
    recordDetails: 'Record details',
    selectAccount: 'Select account: {0}',
    selectContact: 'Select contact: {0}',
    selectOpportunity: 'Select opportunity: {0}',
    selectOutbound: 'Select outbound handoff: {0}',
    selectDedupe: 'Select dedupe case: {0}',
    sortBy: 'Sort by {0}',
    sortAscending: 'Sorted ascending',
    sortDescending: 'Sorted descending',
  },
};

const state = {
  ctx: null,
  t: translateFallback('de'),
  lang: 'de',
  search: '',
  contactSearch: '',
  opportunitySearch: '',
  outboundSearch: '',
  dedupeSearch: '',
  opportunityPreset: 'all',
  dedupeStatus: 'open',
  stage: 'all',
  health: 'all',
  centerView: 'accounts',
  opportunityMode: 'board',
  detailTab: 'overview',
  formMode: '',
  formRecordId: '',
  selectedAccountId: '',
  selectedContactId: '',
  selectedOpportunityId: '',
  selectedOutboundCompanyId: '',
  selectedDedupeCandidateId: '',
  accountSort: { field: 'last_activity_at_ms', direction: 'desc' },
  contactSort: { field: 'name', direction: 'asc' },
  opportunitySort: { field: 'close_date_ms', direction: 'asc' },
  collections: emptyCollections(),
  diagnostics: {
    loading: false,
    error: '',
    lastLoadedAt: 0,
    commandState: '',
    optionalDeniedCollections: [],
  },
  cleanup: [],
  renderTimer: 0,
};

export async function mount(ctx) {
  resetState(ctx);
  await ensureStyles();
  const messages = await loadModuleMessages(import.meta.url, ctx.locale || 'de', labels);
  state.t = (key, fallback, ...args) => {
    let value = messages[key] ?? fallback ?? key;
    args.forEach((arg, index) => {
      value = String(value).replace(`{${index}}`, arg);
    });
    return value;
  };
  state.lang = ctx.locale === 'en' ? 'en' : 'de';
  ctx.host.innerHTML = await loadModuleMarkup();
  ctx.left.replaceChildren();
  ctx.right.replaceChildren();

  const root = ctx.host.querySelector('[data-customers-root]');
  wireUi(root);
  state.cleanup.push(setupResizers(root));
  let disposed = false;
  // Presence (advisory UX): show who else is on the selected record, and
  // publish what this user is looking at / editing. Cleared on unmount so a
  // closed module leaves no stale hints on other peers.
  if (ctx.presence?.subscribe) {
    state.cleanup.push(ctx.presence.subscribe((entries) => {
      state.presenceRemote = Array.isArray(entries) ? entries : [];
      render();
    }));
    state.cleanup.push(() => { try { ctx.presence.clear(); } catch {} });
  }
  state.diagnostics.loading = true;
  render();
  refreshData()
    .then(() => {
      if (disposed || state.ctx !== ctx) return;
      state.cleanup.push(wireRealtime());
      render();
    })
    .catch((error) => {
      if (disposed || state.ctx !== ctx) return;
      reportRefreshError(error);
    });

  return () => {
    disposed = true;
    for (const cleanup of state.cleanup.splice(0)) {
      try { cleanup?.(); } catch {}
    }
    if (state.renderTimer) window.clearTimeout(state.renderTimer);
  };
}

function resetState(ctx) {
  state.ctx = ctx;
  state.t = translateFallback(ctx?.locale === 'en' ? 'en' : 'de');
  state.lang = ctx?.locale === 'en' ? 'en' : 'de';
  state.search = '';
  state.contactSearch = '';
  state.opportunitySearch = '';
  state.outboundSearch = '';
  state.dedupeSearch = '';
  state.opportunityPreset = 'all';
  state.dedupeStatus = 'open';
  state.stage = 'all';
  state.health = 'all';
  state.centerView = 'accounts';
  state.opportunityMode = 'board';
  state.detailTab = 'overview';
  state.formMode = '';
  state.formRecordId = '';
  state.selectedAccountId = '';
  state.selectedContactId = '';
  state.selectedOpportunityId = '';
  state.selectedOutboundCompanyId = '';
  state.selectedDedupeCandidateId = '';
  state.accountSort = { field: 'last_activity_at_ms', direction: 'desc' };
  state.contactSort = { field: 'name', direction: 'asc' };
  state.opportunitySort = { field: 'close_date_ms', direction: 'asc' };
  state.collections = emptyCollections();
  state.diagnostics = { loading: false, error: '', lastLoadedAt: 0, commandState: '' };
  state.cleanup = [];
  state.renderTimer = 0;
  state.presenceRemote = [];
}

function translateFallback(lang) {
  const dictionary = labels[lang] || labels.de;
  return (key, fallback, ...args) => {
    let value = dictionary[key] ?? fallback ?? key;
    args.forEach((arg, index) => {
      value = String(value).replace(`{${index}}`, arg);
    });
    return value;
  };
}

async function ensureStyles() {
  if (document.querySelector('link[data-module-styles="customers"]')) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = new URL(`./index.css?v=${BUILD}`, import.meta.url).href;
  link.dataset.moduleStyles = 'customers';
  document.head.append(link);
}

async function loadModuleMarkup() {
  const html = await fetch(new URL(`./index.html?v=${BUILD}`, import.meta.url)).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}

function setupResizers(root) {
  // Column resizing is now owned by the shell-global resizer (setupModuleResizers
  // in app.js), wired declaratively from the `.ctox-column-resizer[data-resizer-var]`
  // handles inside the `[data-resize-frame]` root. This DIY wiring is neutralised to
  // avoid double-binding the handles; call sites keep their no-op teardown ref.
  return () => {};
  // eslint-disable-next-line no-unreachable
  if (!root) return null;
  const saved = readLayout();
  if (saved.left) root.style.setProperty('--customers-left-width', `${saved.left}px`);
  if (saved.right) root.style.setProperty('--customers-right-width', `${saved.right}px`);
  const cleanups = [];
  const left = root.querySelector('[data-resizer="left"]');
  const right = root.querySelector('[data-resizer="right"]');
  if (left) {
    const resizer = new CtoxResizer({
      resizerEl: left,
      containerEl: root,
      cssVar: '--customers-left-width',
      side: 'left',
      minWidth: 236,
      maxWidth: 520,
      onResize: (width) => saveLayout({ left: width }),
    });
    cleanups.push(() => resizer.destroy());
  }
  if (right) {
    const resizer = new CtoxResizer({
      resizerEl: right,
      containerEl: root,
      cssVar: '--customers-right-width',
      side: 'right',
      minWidth: 286,
      maxWidth: 620,
      onResize: (width) => saveLayout({ right: width }),
    });
    cleanups.push(() => resizer.destroy());
  }
  return () => cleanups.forEach((cleanup) => cleanup());
}

function readLayout() {
  try {
    return JSON.parse(localStorage.getItem(CUSTOMERS_LAYOUT_KEY) || '{}') || {};
  } catch {
    return {};
  }
}

function saveLayout(patch) {
  try {
    localStorage.setItem(CUSTOMERS_LAYOUT_KEY, JSON.stringify({ ...readLayout(), ...patch }));
  } catch {}
}

function wireUi(root) {
  if (!root) return;
  root.addEventListener('click', (event) => {
    const target = event.target instanceof Element ? event.target : null;
    const actionEl = target?.closest('[data-customers-action]');
    const action = actionEl?.getAttribute('data-customers-action');
    if (action) {
      handleAction(action, actionEl).catch((error) => {
        state.diagnostics.commandState = state.t('commandFailed', labels.de.commandFailed, error?.message || String(error));
        renderRight();
      });
      return;
    }
    const sortField = target?.closest('[data-customers-sort]')?.getAttribute('data-customers-sort');
    if (sortField) {
      toggleSort(sortField);
      renderCenter();
      return;
    }
    const stage = target?.closest('[data-customers-stage]')?.getAttribute('data-customers-stage');
    if (stage) {
      state.stage = stage;
      syncSelection();
      renderCenter();
      renderRight();
      return;
    }
    const health = target?.closest('[data-customers-health]')?.getAttribute('data-customers-health');
    if (health) {
      state.health = health;
      syncSelection();
      renderCenter();
      renderRight();
      return;
    }
    const viewId = target?.closest('[data-customers-view-id]')?.getAttribute('data-customers-view-id');
    if (viewId) {
      applySavedView(viewId);
      renderCenter();
      renderRight();
      return;
    }
    const detailTab = target?.closest('[data-customers-detail-tab]')?.getAttribute('data-customers-detail-tab');
    if (detailTab) {
      state.detailTab = detailTab;
      renderRight();
      return;
    }
    const appLink = target?.closest('[data-customers-app-link]');
    if (appLink) {
      openLinkedApp(appLink.getAttribute('data-customers-app-link'), appLink.getAttribute('href'));
      return;
    }
    const selectable = target?.closest('[data-customers-selectable]');
    if (selectable && selectFromElement(selectable)) {
      return;
    }
    const opportunityPreset = target?.closest('[data-customers-opportunity-preset]')?.getAttribute('data-customers-opportunity-preset');
    if (opportunityPreset) {
      state.centerView = 'opportunities';
      state.opportunityPreset = opportunityPreset;
      renderCenter();
      renderRight();
      return;
    }
    const dedupeStatus = target?.closest('[data-customers-dedupe-status]')?.getAttribute('data-customers-dedupe-status');
    if (dedupeStatus) {
      state.centerView = 'dedupe';
      state.dedupeStatus = dedupeStatus;
      render();
      return;
    }
    selectFromElement(target);
  });
  root.addEventListener('keydown', (event) => {
    const target = event.target instanceof Element ? event.target : null;
    if (!target) return;
    const detailTab = target.closest('[data-customers-detail-tab]');
    if (detailTab && (event.key === 'ArrowRight' || event.key === 'ArrowLeft')) {
      event.preventDefault();
      moveDetailTab(event.key === 'ArrowRight' ? 1 : -1);
      return;
    }
    if (!isActivationKey(event.key) || isInteractiveTarget(target)) return;
    const selectable = target.closest('[data-customers-selectable]');
    if (selectable) {
      event.preventDefault();
      selectFromElement(selectable);
    }
  });
  root.addEventListener('input', (event) => {
    const target = event.target;
    if (!(target instanceof HTMLInputElement)) return;
    if (target.matches('[data-customers-search]')) {
      state.search = target.value || '';
      syncSelection();
      renderCenter();
      renderLeft();
    } else if (target.matches('[data-customers-contact-search]')) {
      state.contactSearch = target.value || '';
      syncSelection();
      renderCenter();
    } else if (target.matches('[data-customers-opportunity-search]')) {
      state.opportunitySearch = target.value || '';
      renderCenter();
    } else if (target.matches('[data-customers-outbound-search]')) {
      state.outboundSearch = target.value || '';
      renderCenter();
    } else if (target.matches('[data-customers-dedupe-search]')) {
      state.dedupeSearch = target.value || '';
      renderCenter();
    }
  });
  root.addEventListener('change', (event) => {
    const target = event.target;
    if (!(target instanceof HTMLSelectElement)) return;
    if (target.matches('[data-customers-stage-filter]')) {
      state.stage = target.value || 'all';
      syncSelection();
      render();
      return;
    }
    if (target.matches('[data-customers-health-filter]')) {
      state.health = target.value || 'all';
      syncSelection();
      render();
      return;
    }
    const opportunityId = target.getAttribute('data-opportunity-stage-select');
    if (!opportunityId || !target.value) return;
    handleAction('move-opportunity', {
      getAttribute: (name) => {
        if (name === 'data-opportunity-id') return opportunityId;
        if (name === 'data-stage') return target.value;
        return null;
      },
    }).catch((error) => {
      state.diagnostics.commandState = state.t('commandFailed', labels.de.commandFailed, error?.message || String(error));
      renderRight();
    });
  });
  root.addEventListener('submit', (event) => {
    const form = event.target instanceof HTMLFormElement ? event.target : null;
    if (!form?.matches('[data-customers-form]')) return;
    event.preventDefault();
    submitForm(form).catch((error) => {
      state.diagnostics.commandState = state.t('commandFailed', labels.de.commandFailed, error?.message || String(error));
      renderRight();
    });
  });
}

async function handleAction(action, element) {
  if (action === 'refresh') {
    await refreshData({ restartSync: true, renderLoading: true });
  } else if (action === 'export-customers') {
    exportCurrentView();
  } else if (action === 'clear-filters') {
    state.stage = 'all';
    state.health = 'all';
    state.search = '';
    state.contactSearch = '';
    state.opportunitySearch = '';
    state.outboundSearch = '';
    state.dedupeSearch = '';
    state.opportunityPreset = 'all';
    state.dedupeStatus = 'open';
    syncSelection();
    render();
  } else if (!guardMutableAction(action)) {
    return;
  } else if (action === 'import-customers') {
    await openCustomerImporter();
  } else if (action === 'center-accounts') {
    state.centerView = 'accounts';
    renderCenter();
  } else if (action === 'center-contacts') {
    state.centerView = 'contacts';
    renderCenter();
  } else if (action === 'center-opportunities') {
    state.centerView = 'opportunities';
    renderCenter();
  } else if (action === 'center-handoff') {
    state.centerView = 'handoff';
    renderCenter();
  } else if (action === 'center-dedupe') {
    state.centerView = 'dedupe';
    renderCenter();
  } else if (action === 'opportunity-board') {
    state.opportunityMode = 'board';
    renderCenter();
  } else if (action === 'opportunity-table') {
    state.opportunityMode = 'table';
    renderCenter();
  } else if (action === 'create-account') {
    openForm('account-create');
  } else if (action === 'edit-account') {
    openForm('account-edit', element.getAttribute('data-account-id') || state.selectedAccountId);
  } else if (action === 'archive-account') {
    await dispatchCommand(buildAccountArchiveCommand(element.getAttribute('data-account-id') || state.selectedAccountId));
    state.formMode = '';
    state.selectedAccountId = '';
    await refreshData();
  } else if (action === 'create-contact') {
    openForm('contact-create', state.selectedAccountId);
  } else if (action === 'edit-contact') {
    openForm('contact-edit', element.getAttribute('data-contact-id') || state.selectedContactId);
  } else if (action === 'archive-contact') {
    await dispatchCommand(buildContactArchiveCommand(element.getAttribute('data-contact-id') || state.selectedContactId));
    state.formMode = '';
    state.selectedContactId = '';
    await refreshData();
  } else if (action === 'create-opportunity') {
    openForm('opportunity-create', state.selectedAccountId);
  } else if (action === 'edit-opportunity') {
    openForm('opportunity-edit', element.getAttribute('data-opportunity-id') || state.selectedOpportunityId);
  } else if (action === 'move-opportunity') {
    const opportunityId = element.getAttribute('data-opportunity-id') || state.selectedOpportunityId;
    const stage = element.getAttribute('data-stage') || '';
    await dispatchCommand(buildOpportunityMoveStageCommand(opportunityId, stage));
    state.selectedOpportunityId = opportunityId;
    await refreshData();
  } else if (action === 'close-won') {
    const opportunityId = element.getAttribute('data-opportunity-id') || state.selectedOpportunityId;
    await dispatchCommand(buildOpportunityCloseCommand(opportunityId, 'won'));
    state.selectedOpportunityId = opportunityId;
    await refreshData();
  } else if (action === 'close-lost') {
    const opportunityId = element.getAttribute('data-opportunity-id') || state.selectedOpportunityId;
    await dispatchCommand(buildOpportunityCloseCommand(opportunityId, 'lost'));
    state.selectedOpportunityId = opportunityId;
    await refreshData();
  } else if (action === 'create-task') {
    openForm('task-create', activeRecordContext()?.id || state.selectedAccountId);
  } else if (action === 'edit-task') {
    openForm('task-edit', element.getAttribute('data-task-id') || '');
  } else if (action === 'complete-task') {
    const taskId = element.getAttribute('data-task-id') || '';
    await dispatchCommand(buildTaskCompleteCommand(taskId));
    await refreshData();
  } else if (action === 'create-note') {
    openForm('note-create', activeRecordContext()?.id || state.selectedAccountId);
  } else if (action === 'edit-note') {
    openForm('note-edit', element.getAttribute('data-note-id') || '');
  } else if (action === 'import-outbound') {
    const companyId = element.getAttribute('data-outbound-company-id') || state.selectedOutboundCompanyId;
    const row = outboundHandoffRowByCompanyId(companyId);
    await dispatchCommand(buildImportFromOutboundCommand(row));
    state.selectedOutboundCompanyId = companyId;
    await refreshData();
  } else if (action === 'dedupe-merge' || action === 'dedupe-keep-existing' || action === 'dedupe-create-new' || action === 'dedupe-skip') {
    const candidateId = element.getAttribute('data-dedupe-candidate-id') || state.selectedDedupeCandidateId;
    const decision = ({
      'dedupe-merge': 'merge',
      'dedupe-keep-existing': 'keep_existing',
      'dedupe-create-new': 'create_new',
      'dedupe-skip': 'skip',
    })[action];
    await dispatchCommand(buildDedupeResolveCommand(candidateId, decision));
    state.selectedDedupeCandidateId = candidateId;
    await refreshData();
  } else if (action === 'open-linked-app') {
    openLinkedApp(element.getAttribute('data-link-module'), element.getAttribute('data-link-href'));
  } else if (action === 'cancel-form') {
    state.formMode = '';
    state.formRecordId = '';
    renderRight();
  } else if (action === 'save-view') {
    await dispatchCommand(buildSaveViewCommand({
      stage: state.stage,
      health: state.health,
      search: state.search,
      sort: state.accountSort,
    }));
    await refreshData();
  }
}

function guardMutableAction(action) {
  if (!MUTATING_ACTIONS.has(action)) return true;
  if (canMutateCustomers()) return true;
  state.diagnostics.commandState = state.t('permissionDenied', labels.de.permissionDenied);
  renderRight();
  return false;
}

function selectFromElement(element) {
  const outboundCompanyId = element?.closest?.('[data-customers-outbound-company-id]')?.getAttribute('data-customers-outbound-company-id');
  if (outboundCompanyId) {
    state.selectedOutboundCompanyId = outboundCompanyId;
    state.selectedDedupeCandidateId = '';
    state.formMode = '';
    render();
    return true;
  }
  const candidateId = element?.closest?.('[data-customers-dedupe-candidate-id]')?.getAttribute('data-customers-dedupe-candidate-id');
  if (candidateId) {
    state.selectedDedupeCandidateId = candidateId;
    state.selectedOutboundCompanyId = '';
    state.formMode = '';
    render();
    return true;
  }
  const contactId = element?.closest?.('[data-customers-contact-id]')?.getAttribute('data-customers-contact-id');
  if (contactId) {
    const contact = state.collections.customer_contacts.find((item) => item.id === contactId);
    if (contact?.account_id) state.selectedAccountId = contact.account_id;
    state.selectedContactId = contactId;
    state.selectedOpportunityId = '';
    state.formMode = '';
    render();
    return true;
  }
  const opportunityId = element?.closest?.('[data-customers-opportunity-id]')?.getAttribute('data-customers-opportunity-id');
  if (opportunityId) {
    const opportunity = state.collections.customer_opportunities.find((item) => item.id === opportunityId);
    if (opportunity?.account_id) state.selectedAccountId = opportunity.account_id;
    state.selectedOpportunityId = opportunityId;
    state.selectedContactId = '';
    state.formMode = '';
    render();
    return true;
  }
  const accountId = element?.closest?.('[data-customers-account-id]')?.getAttribute('data-customers-account-id');
  if (accountId) {
    state.selectedAccountId = accountId;
    state.selectedContactId = '';
    state.selectedOpportunityId = '';
    state.formMode = '';
    render();
    return true;
  }
  return false;
}

function isActivationKey(key) {
  return key === 'Enter' || key === ' ';
}

function isInteractiveTarget(target) {
  return Boolean(target?.closest?.('button, a, input, select, textarea, summary, [contenteditable="true"]'));
}

function moveDetailTab(delta) {
  const next = nextDetailTab(state.detailTab, delta);
  if (next === state.detailTab) return;
  state.detailTab = next;
  renderRight();
  requestAnimationFrame(() => {
    state.ctx?.host
      ?.querySelector(`[data-customers-detail-tab="${cssEscape(next)}"]`)
      ?.focus?.();
  });
}

function nextDetailTab(current, delta) {
  const tabs = ['overview', 'tasks', 'notes', 'timeline', 'files', 'apps'];
  const currentIndex = Math.max(0, tabs.indexOf(current));
  const nextIndex = (currentIndex + delta + tabs.length) % tabs.length;
  return tabs[nextIndex];
}

function canMutateCustomers() {
  return canMutateCustomersContext(state.ctx);
}

function canMutateCustomersContext(ctx = {}) {
  if (ctx?.readonly === true || ctx?.permissions?.readonly === true) return false;
  if (typeof ctx?.canModifyModule === 'function' && ctx.canModifyModule()) return true;
  const user = ctx?.session?.user || {};
  if (user.is_admin === true || user.is_owner === true) return true;
  const role = String(user.role || '').trim().toLowerCase().replace(/^business_os_/, '');
  if (!role) return true;
  return ['admin', 'chef', 'owner', 'founder', 'sales', 'sales_admin', 'account_manager'].includes(role);
}

function mutableDisabledAttr() {
  return canMutateCustomers()
    ? ''
    : ` disabled title="${escapeAttribute(state.t('permissionDenied', labels.de.permissionDenied))}"`;
}

function selectableAttrs(type, id, selected, label) {
  const labelKey = {
    account: 'selectAccount',
    contact: 'selectContact',
    opportunity: 'selectOpportunity',
    outbound: 'selectOutbound',
    dedupe: 'selectDedupe',
  }[type] || 'selectAccount';
  const fallback = labels.de[labelKey] || '{0}';
  return [
    'data-customers-selectable',
    `data-context-record-id="${escapeAttribute(id || '')}"`,
    `data-context-record-type="${escapeAttribute(type || 'customer_record')}"`,
    `data-context-label="${escapeAttribute(label || id || '')}"`,
    'tabindex="0"',
    'role="button"',
    `aria-selected="${selected ? 'true' : 'false'}"`,
    `aria-label="${escapeAttribute(state.t(labelKey, fallback, label || id || ''))}"`,
  ].join(' ');
}

function openForm(mode, recordId = '') {
  if (!canMutateCustomers()) {
    state.diagnostics.commandState = state.t('permissionDenied', labels.de.permissionDenied);
    renderRight();
    return;
  }
  state.formMode = mode;
  state.formRecordId = recordId;
  state.diagnostics.commandState = '';
  renderRight();
}

function toggleSort(field) {
  const sort = state.centerView === 'contacts'
    ? state.contactSort
    : state.centerView === 'opportunities'
      ? state.opportunitySort
      : state.accountSort;
  const next = {
    field,
    direction: sort.field === field && sort.direction === 'asc' ? 'desc' : 'asc',
  };
  if (state.centerView === 'contacts') state.contactSort = next;
  else if (state.centerView === 'opportunities') state.opportunitySort = next;
  else state.accountSort = next;
}

function wireRealtime() {
  const subscriptions = [...CUSTOMERS_COLLECTIONS, ...OUTBOUND_HANDOFF_COLLECTIONS, ...OPTIONAL_LINK_COLLECTIONS]
    .map((name) => resolveCollection(name)?.$?.subscribe?.(() => scheduleRefresh()))
    .filter(Boolean);
  return () => subscriptions.forEach((sub) => {
    try { sub.unsubscribe?.(); } catch {}
  });
}

function scheduleRefresh() {
  if (state.renderTimer) return;
  state.renderTimer = window.setTimeout(() => {
    state.renderTimer = 0;
    refreshData().catch(reportRefreshError);
  }, 80);
}

async function refreshData(options = {}) {
  if (options.renderLoading) {
    state.diagnostics.loading = true;
    render();
  }
  try {
    const entries = [];
    for (const name of CUSTOMERS_COLLECTIONS) {
      entries.push([name, await readCollection(name)]);
    }
    const optionalDeniedCollections = [];
    for (const name of OUTBOUND_HANDOFF_COLLECTIONS.concat(OPTIONAL_LINK_COLLECTIONS)) {
      try {
        entries.push([name, await readCollection(name)]);
      } catch (error) {
        if (!isBusinessOsPermissionDenied(error)) throw error;
        optionalDeniedCollections.push(name);
        entries.push([name, []]);
      }
    }
    state.collections = { ...emptyCollections(), ...Object.fromEntries(entries) };
    state.diagnostics.error = '';
    state.diagnostics.optionalDeniedCollections = optionalDeniedCollections;
    state.diagnostics.lastLoadedAt = Date.now();
    syncSelection();
  } catch (error) {
    state.diagnostics.error = error?.message || String(error);
  } finally {
    state.diagnostics.loading = false;
    render();
  }
}

function reportRefreshError(error) {
  state.diagnostics.error = error?.message || String(error);
  state.diagnostics.loading = false;
  render();
}

function isBusinessOsPermissionDenied(error) {
  return error?.code === 'CTOX_BUSINESS_OS_PERMISSION_DENIED'
    || error?.name === 'BusinessOsPermissionError';
}

async function readCollection(name) {
  const collection = resolveCollection(name);
  const docs = collection?.find ? await collection.find().exec() : [];
  return docs
    .map((doc) => doc?.toJSON?.() || doc)
    .filter((doc) => doc && doc._deleted !== true && doc.is_deleted !== true)
    .sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0));
}

function resolveCollection(name) {
  return state.ctx?.db?.collection?.(name) || null;
}

function canWriteCustomerCollection(name) {
  const permissionCheck = state.ctx?.permissions?.canWriteCollection;
  return typeof permissionCheck === 'function' ? permissionCheck(name) : true;
}

function requireCustomerImportCollections() {
  const names = [
    'customer_accounts',
    'customer_dedupe_candidates',
    'customer_import_batches',
  ];
  const denied = names.filter((name) => !canWriteCustomerCollection(name));
  if (denied.length) {
    const error = new Error(`Keine Schreibberechtigung fuer Kundendaten: ${denied.join(', ')}`);
    error.name = 'BusinessOsPermissionError';
    error.code = 'CTOX_BUSINESS_OS_PERMISSION_DENIED';
    throw error;
  }
  return Object.fromEntries(names.map((name) => [name, resolveCollection(name)]));
}

function syncSelection() {
  const accounts = visibleAccountsForState();
  if (state.selectedAccountId && accounts.some((account) => account.id === state.selectedAccountId)) {
    return;
  }
  state.selectedAccountId = accounts[0]?.id || '';
  state.selectedContactId = '';
}

function render() {
  const root = state.ctx?.host?.querySelector('[data-customers-root]');
  if (!root) return;
  syncPresence();
  renderLeft();
  renderCenter();
  renderRight();
}

// Publish this user's current focus as a presence entry. Derived from the
// selection/form state on every render — the presence registry dedups
// unchanged sets, so this is idempotent and cheap. One entry, the most
// specific selected record.
function syncPresence() {
  const presence = state.ctx?.presence;
  if (!presence?.set) return;
  const focus = presenceFocus();
  presence.set(focus ? [focus] : []);
}

function presenceFocus() {
  if (state.selectedContactId) {
    return {
      collection: 'customer_contacts',
      recordId: state.selectedContactId,
      mode: state.formMode === 'contact-edit' && state.formRecordId === state.selectedContactId
        ? 'editing' : 'viewing',
    };
  }
  if (state.selectedOpportunityId) {
    return {
      collection: 'customer_opportunities',
      recordId: state.selectedOpportunityId,
      mode: state.formMode === 'opportunity-edit' && state.formRecordId === state.selectedOpportunityId
        ? 'editing' : 'viewing',
    };
  }
  if (state.selectedAccountId) {
    return {
      collection: 'customer_accounts',
      recordId: state.selectedAccountId,
      mode: state.formMode === 'account-edit' && state.formRecordId === state.selectedAccountId
        ? 'editing' : 'viewing',
    };
  }
  return null;
}

// Remote presence hint for one record: other users (not this actor) who are
// viewing/editing it right now. Editing outranks viewing.
function presenceChip(collection, recordId) {
  if (!recordId || !Array.isArray(state.presenceRemote) || !state.presenceRemote.length) return '';
  const ownActorId = state.ctx?.actor?.id || '';
  const matches = state.presenceRemote.filter((entry) => entry
    && entry.collection === collection
    && entry.recordId === recordId
    && entry.actorId
    && entry.actorId !== ownActorId);
  if (!matches.length) return '';
  const editing = matches.filter((entry) => entry.mode === 'editing');
  const shown = editing.length ? editing : matches;
  const key = editing.length ? 'presenceEditing' : 'presenceViewing';
  const names = [...new Set(shown.map((entry) => entry.actorName || entry.actorId))].join(', ');
  const label = state.t(key, labels.de[key], names);
  return `<span class="ctox-badge ${editing.length ? 'is-danger' : 'is-info'}" title="${escapeAttribute(label)}">${escapeHtml(label)}</span>`;
}

function renderLeft() {
  const target = state.ctx.host.querySelector('[data-customers-left]');
  if (!target) return;
  const scrollTop = target.querySelector('.customers-left-scroll')?.scrollTop || 0;
  const summary = summarizeCustomersData(state.collections);
  const handoffRows = buildOutboundHandoffRows(state.collections);
  const dedupeRows = state.collections.customer_dedupe_candidates || [];
  const views = (state.collections.customer_views || []).filter((view) => view.object_type === 'account');
  target.innerHTML = `
    <header class="ctox-pane-header ctox-pane-band">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">CRM</span>
          <h2 class="ctox-pane-title">${escapeHtml(state.t('title', labels.de.title))}</h2>
        </div>
        <div class="ctox-pane-actions">
          <button class="ctox-pane-icon" type="button" data-customers-action="refresh" title="${escapeAttribute(state.t('refresh', labels.de.refresh))}" aria-label="${escapeAttribute(state.t('refresh', labels.de.refresh))}">${actionIcon('refresh')}</button>
        </div>
      </div>
    </header>
    <div class="ctox-pane-scroll customers-left-scroll">
      ${renderPermissionNotice()}
      <section class="customers-nav-section">
        <h3 class="ctox-field-label customers-section-heading">${escapeHtml(state.t('scope', labels.de.scope))}</h3>
        ${scopeButton('all', state.t('allAccounts', labels.de.allAccounts), summary.accounts)}
        ${scopeButton('active', state.t('activeCustomers', labels.de.activeCustomers), summary.stageCounts.active)}
        ${scopeButton('renewal', state.t('renewal', labels.de.renewal), summary.stageCounts.renewal)}
        ${scopeButton('expansion', state.t('expansion', labels.de.expansion), summary.stageCounts.expansion)}
        ${scopeButton('at_risk', state.t('atRisk', labels.de.atRisk), summary.stageCounts.at_risk)}
      </section>
      <section class="customers-nav-section">
        <div class="customers-section-head">
          <h3 class="ctox-field-label customers-section-heading">${escapeHtml(state.t('savedViews', labels.de.savedViews))}</h3>
          <button class="ctox-button ctox-button--sm" type="button" data-customers-action="save-view"${mutableDisabledAttr()}>${escapeHtml(state.t('saveView', labels.de.saveView))}</button>
        </div>
        ${views.length ? views.map(viewButton).join('') : `<div class="customers-muted-row">Keine Ansichten</div>`}
      </section>
      <section class="customers-nav-section">
        <h3 class="ctox-field-label customers-section-heading">Inbox</h3>
        ${handoffButton('handoff', state.t('importFromOutbound', labels.de.importFromOutbound), handoffRows.filter((row) => row.status === 'ready').length)}
        ${dedupeStatusButton('open', state.t('needsReview', labels.de.needsReview), dedupeRows.filter((row) => row.status === 'open').length)}
      </section>
    </div>
  `;
  const nextScroll = target.querySelector('.customers-left-scroll');
  if (nextScroll) nextScroll.scrollTop = scrollTop;
}

function scopeButton(stage, label, count) {
  return `
    <button class="ctox-list-item customers-scope${state.stage === stage ? ' is-selected' : ''}" type="button" data-customers-stage="${escapeHtml(stage)}">
      <span>${escapeHtml(label)}</span>
      <span class="ctox-badge">${Number(count || 0)}</span>
    </button>
  `;
}

function healthButton(health, label, count) {
  return `
    <button class="ctox-list-item customers-scope${state.health === health ? ' is-selected' : ''}" type="button" data-customers-health="${escapeHtml(health)}">
      <span>${escapeHtml(label)}</span>
      <span class="ctox-badge">${Number(count || 0)}</span>
    </button>
  `;
}

function viewButton(view) {
  return `
    <button class="ctox-list-item customers-scope" type="button" data-customers-view-id="${escapeAttribute(view.id)}">
      <span>${escapeHtml(view.name || view.id)}</span>
      <span class="ctox-badge">${escapeHtml(view.view_type || 'table')}</span>
    </button>
  `;
}

function opportunityPresetButton(preset, label, count) {
  return `
    <button class="ctox-list-item customers-scope${state.centerView === 'opportunities' && state.opportunityPreset === preset ? ' is-selected' : ''}" type="button" data-customers-opportunity-preset="${escapeAttribute(preset)}">
      <span>${escapeHtml(label)}</span>
      <span class="ctox-badge">${Number(count || 0)}</span>
    </button>
  `;
}

function handoffButton(view, label, count) {
  return `
    <button class="ctox-list-item customers-scope${state.centerView === view ? ' is-selected' : ''}" type="button" data-customers-action="center-${escapeAttribute(view)}">
      <span>${escapeHtml(label)}</span>
      <span class="ctox-badge">${Number(count || 0)}</span>
    </button>
  `;
}

function dedupeStatusButton(status, label, count) {
  return `
    <button class="ctox-list-item customers-scope${state.centerView === 'dedupe' && state.dedupeStatus === status ? ' is-selected' : ''}" type="button" data-customers-dedupe-status="${escapeAttribute(status)}">
      <span>${escapeHtml(label)}</span>
      <span class="ctox-badge">${Number(count || 0)}</span>
    </button>
  `;
}

function renderCenter() {
  const target = state.ctx.host.querySelector('[data-customers-center]');
  if (!target) return;
  const accounts = visibleAccountsForState();
  const contacts = visibleContactsForState();
  const opportunities = visibleOpportunitiesForState();
  const handoffRows = visibleOutboundHandoffRowsForState();
  const dedupeRows = visibleDedupeCandidatesForState();
  const pipeline = summarizeOpportunityPipeline(opportunities);
  const summary = summarizeCustomersData(state.collections);
  const activeTitle = centerViewTitle();
  const primaryAction = centerPrimaryAction();
  target.innerHTML = `
    <header class="ctox-pane-header ctox-pane-band">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">${escapeHtml(activeTitle)}</span>
          <h2 class="ctox-pane-title">${summary.accounts} ${escapeHtml(state.t('accounts', labels.de.accounts))} · ${summary.opportunities} ${escapeHtml(state.t('opportunities', labels.de.opportunities))} · ${escapeHtml(formatMoney(pipeline.total_cents, pipeline.currency))}</h2>
        </div>
        <div class="ctox-pane-actions">
          <button class="ctox-pane-icon" type="button" data-customers-action="import-customers" aria-label="${escapeAttribute(state.t('importCustomers', labels.de.importCustomers))}" title="${escapeAttribute(state.t('importCustomers', labels.de.importCustomers))}"${mutableDisabledAttr()}>${actionIcon('upload')}</button>
          <button class="ctox-pane-icon" type="button" data-customers-action="export-customers" aria-label="${escapeAttribute(state.t('exportCustomers', labels.de.exportCustomers))}" title="${escapeAttribute(state.t('exportCustomers', labels.de.exportCustomers))}">${actionIcon('export')}</button>
          ${primaryAction ? `<button class="ctox-pane-icon" type="button" data-customers-action="${escapeAttribute(primaryAction.action)}"${primaryAction.recordAttr || ''} aria-label="${escapeAttribute(primaryAction.label)}" title="${escapeAttribute(primaryAction.label)}"${mutableDisabledAttr()}>${actionIcon(primaryAction.icon || 'add')}</button>` : ''}
        </div>
      </div>
      <div class="ctox-pane-tools">
        ${renderCenterSearchInput()}
        <select class="ctox-pane-filter" data-customers-stage-filter aria-label="${escapeAttribute(state.t('stage', labels.de.stage))}">
          ${filterOption('all', state.t('allStages', labels.de.allStages), state.stage)}
          ${Object.entries(STAGE_LABELS).filter(([key]) => key !== 'archived').map(([key, label]) => filterOption(key, label, state.stage)).join('')}
        </select>
        <select class="ctox-pane-filter" data-customers-health-filter aria-label="${escapeAttribute(state.t('health', labels.de.health))}">
          ${filterOption('all', state.t('allHealth', labels.de.allHealth), state.health)}
          ${Object.entries(HEALTH_LABELS).map(([key, label]) => filterOption(key, label, state.health)).join('')}
        </select>
      </div>
    </header>
    <div class="ctox-toolbar customers-toolbar">
      <div class="ctox-pane-tabs" role="tablist" aria-label="${escapeAttribute(state.t('workbench', labels.de.workbench))}">
        <button class="ctox-pane-tab" type="button" role="tab" aria-selected="${state.centerView === 'accounts' ? 'true' : 'false'}" data-customers-action="center-accounts">${escapeHtml(state.t('accounts', labels.de.accounts))}</button>
        <button class="ctox-pane-tab" type="button" role="tab" aria-selected="${state.centerView === 'contacts' ? 'true' : 'false'}" data-customers-action="center-contacts">${escapeHtml(state.t('contacts', labels.de.contacts))}</button>
        <button class="ctox-pane-tab" type="button" role="tab" aria-selected="${state.centerView === 'opportunities' ? 'true' : 'false'}" data-customers-action="center-opportunities">${escapeHtml(state.t('opportunities', labels.de.opportunities))}</button>
        <button class="ctox-pane-tab" type="button" role="tab" aria-selected="${state.centerView === 'handoff' ? 'true' : 'false'}" data-customers-action="center-handoff">${escapeHtml(state.t('handoff', labels.de.handoff))}</button>
        <button class="ctox-pane-tab" type="button" role="tab" aria-selected="${state.centerView === 'dedupe' ? 'true' : 'false'}" data-customers-action="center-dedupe">${escapeHtml(state.t('dedupe', labels.de.dedupe))}</button>
      </div>
      ${state.centerView === 'opportunities' ? `
        <div class="ctox-pane-tabs" role="tablist" aria-label="${escapeAttribute(state.t('opportunityView', labels.de.opportunityView))}">
          <button class="ctox-pane-tab" type="button" role="tab" aria-selected="${state.opportunityMode === 'board' ? 'true' : 'false'}" data-customers-action="opportunity-board">${escapeHtml(state.t('board', labels.de.board))}</button>
          <button class="ctox-pane-tab" type="button" role="tab" aria-selected="${state.opportunityMode === 'table' ? 'true' : 'false'}" data-customers-action="opportunity-table">${escapeHtml(state.t('table', labels.de.table))}</button>
        </div>
      ` : ''}
      ${(state.stage !== 'all' || state.health !== 'all' || state.search || state.contactSearch || state.opportunitySearch || state.outboundSearch || state.dedupeSearch)
        ? `<button class="ctox-button ctox-button--sm" type="button" data-customers-action="clear-filters">${escapeHtml(state.t('clearFilters', labels.de.clearFilters))}</button>`
        : ''}
      <span class="ctox-badge">${escapeHtml(state.t('visibleCount', labels.de.visibleCount, visibleCenterCount({ accounts, contacts, opportunities, handoffRows, dedupeRows })))}</span>
      ${state.diagnostics.loading ? `<span class="ctox-badge">${escapeHtml(state.t('loadingStatus', labels.de.loadingStatus))}</span>` : ''}
      ${state.diagnostics.error ? `<span class="ctox-badge is-danger">${escapeHtml(state.diagnostics.error)}</span>` : ''}
      ${state.diagnostics.optionalDeniedCollections?.length ? `<span class="ctox-badge is-warning">${escapeHtml(state.t('linkedDataLimited', labels.de.linkedDataLimited))}</span>` : ''}
    </div>
    <div class="ctox-pane-scroll customers-center-scroll">
      ${state.centerView === 'contacts'
        ? renderContactTable(contacts)
        : state.centerView === 'opportunities'
          ? renderOpportunityWorkbench(opportunities)
          : state.centerView === 'handoff'
            ? renderOutboundHandoffTable(handoffRows)
            : state.centerView === 'dedupe'
              ? renderDedupeTable(dedupeRows)
              : renderAccountTable(accounts)}
    </div>
  `;
}

function centerViewTitle() {
  if (state.centerView === 'contacts') return state.t('contacts', labels.de.contacts);
  if (state.centerView === 'opportunities') return state.t('opportunities', labels.de.opportunities);
  if (state.centerView === 'handoff') return state.t('handoff', labels.de.handoff);
  if (state.centerView === 'dedupe') return state.t('dedupe', labels.de.dedupe);
  return state.t('accounts', labels.de.accounts);
}

function centerPrimaryAction() {
  if (state.centerView === 'opportunities') return { action: 'create-opportunity', label: state.t('newOpportunity', labels.de.newOpportunity), recordAttr: '', icon: 'add' };
  if (state.centerView === 'handoff' && state.selectedOutboundCompanyId) {
    const row = outboundHandoffRowByCompanyId(state.selectedOutboundCompanyId);
    if (row?.status !== 'ready') return null;
    return {
      action: 'import-outbound',
      label: state.t('importFromOutbound', labels.de.importFromOutbound),
      recordAttr: ` data-outbound-company-id="${escapeAttribute(state.selectedOutboundCompanyId)}"`,
      icon: 'download',
    };
  }
  if (state.centerView === 'dedupe') return null;
  return { action: 'create-account', label: state.t('newCustomer', labels.de.newCustomer), recordAttr: '', icon: 'add' };
}

function visibleCenterCount({ accounts, contacts, opportunities, handoffRows, dedupeRows }) {
  if (state.centerView === 'contacts') return contacts.length;
  if (state.centerView === 'opportunities') return opportunities.length;
  if (state.centerView === 'handoff') return handoffRows.length;
  if (state.centerView === 'dedupe') return dedupeRows.length;
  return accounts.length;
}

function renderCenterSearchInput() {
  if (state.centerView === 'contacts') {
    return `<input class="ctox-pane-search" type="search" data-customers-contact-search value="${escapeAttribute(state.contactSearch)}" placeholder="${escapeAttribute(state.t('searchContacts', labels.de.searchContacts))}" aria-label="${escapeAttribute(state.t('searchContacts', labels.de.searchContacts))}">`;
  }
  if (state.centerView === 'opportunities') {
    return `<input class="ctox-pane-search" type="search" data-customers-opportunity-search value="${escapeAttribute(state.opportunitySearch)}" placeholder="${escapeAttribute(state.t('searchOpportunities', labels.de.searchOpportunities))}" aria-label="${escapeAttribute(state.t('searchOpportunities', labels.de.searchOpportunities))}">`;
  }
  if (state.centerView === 'handoff') {
    return `<input class="ctox-pane-search" type="search" data-customers-outbound-search value="${escapeAttribute(state.outboundSearch)}" placeholder="${escapeAttribute(state.t('searchOutbound', labels.de.searchOutbound))}" aria-label="${escapeAttribute(state.t('searchOutbound', labels.de.searchOutbound))}">`;
  }
  if (state.centerView === 'dedupe') {
    return `<input class="ctox-pane-search" type="search" data-customers-dedupe-search value="${escapeAttribute(state.dedupeSearch)}" placeholder="${escapeAttribute(state.t('searchDedupe', labels.de.searchDedupe))}" aria-label="${escapeAttribute(state.t('searchDedupe', labels.de.searchDedupe))}">`;
  }
  return `<input class="ctox-pane-search" type="search" data-customers-search value="${escapeAttribute(state.search)}" placeholder="${escapeAttribute(state.t('search', labels.de.search))}" aria-label="${escapeAttribute(state.t('search', labels.de.search))}">`;
}

function renderAccountTable(accounts) {
  if (!accounts.length) {
    return `
      <div class="ctox-empty">
        <strong>${escapeHtml(state.t('noCustomers', labels.de.noCustomers))}</strong>
        <span>${escapeHtml(state.t('noCustomersBody', labels.de.noCustomersBody))}</span>
      </div>
    `;
  }
  return `
    <table class="ctox-table customers-table" aria-label="${escapeAttribute(state.t('accounts', labels.de.accounts))}">
      <thead>
        <tr>
          ${sortableTh('name', state.t('accountName', labels.de.accountName))}
          ${sortableTh('customer_stage', state.t('stage', labels.de.stage), '150px')}
          ${sortableTh('health_status', state.t('health', labels.de.health), '130px')}
          ${sortableTh('domain', state.t('domain', labels.de.domain), '150px')}
          ${sortableTh('annual_recurring_revenue_cents', state.t('arr', labels.de.arr), '120px', true)}
          ${sortableTh('last_activity_at_ms', state.t('lastActivity', labels.de.lastActivity), '150px')}
        </tr>
      </thead>
      <tbody>${accounts.map((account) => accountRow(account)).join('')}</tbody>
    </table>
  `;
}

function renderContactTable(contacts) {
  if (!contacts.length) {
    return `
      <div class="ctox-empty">
        <strong>${escapeHtml(state.t('noContacts', labels.de.noContacts))}</strong>
        <span>${escapeHtml(state.t('noContactsBody', labels.de.noContactsBody))}</span>
      </div>
    `;
  }
  return `
    <table class="ctox-table customers-table customers-contact-table" aria-label="${escapeAttribute(state.t('contacts', labels.de.contacts))}">
      <thead>
        <tr>
          ${sortableTh('name', state.t('contacts', labels.de.contacts))}
          ${sortableTh('account_name', state.t('accounts', labels.de.accounts), '180px')}
          ${sortableTh('job_title', state.t('jobTitle', labels.de.jobTitle), '150px')}
          ${sortableTh('email', state.t('email', labels.de.email), '190px')}
          ${sortableTh('is_primary_contact', state.t('primary', labels.de.primary), '90px')}
          <th style="width: 92px;"></th>
        </tr>
      </thead>
      <tbody>${contacts.map((contact) => contactTableRow(contact)).join('')}</tbody>
    </table>
  `;
}

function renderOpportunityWorkbench(opportunities) {
  if (!opportunities.length) {
    return `
      <div class="ctox-empty">
        <strong>${escapeHtml(state.t('noOpportunities', labels.de.noOpportunities))}</strong>
        <span>${escapeHtml(state.t('noOpportunitiesBody', labels.de.noOpportunitiesBody))}</span>
      </div>
    `;
  }
  if (state.opportunityMode === 'table') return renderOpportunityTable(opportunities);
  return renderOpportunityBoard(opportunities);
}

function renderOutboundHandoffTable(rows) {
  if (!rows.length) {
    return `
      <div class="ctox-empty">
        <strong>${escapeHtml(state.t('noOutbound', labels.de.noOutbound))}</strong>
        <span>${escapeHtml(state.t('noOutboundBody', labels.de.noOutboundBody))}</span>
      </div>
    `;
  }
  return `
    <table class="ctox-table customers-table customers-handoff-table" aria-label="${escapeAttribute(state.t('handoff', labels.de.handoff))}">
      <thead>
        <tr>
          <th>${escapeHtml(state.t('accountName', labels.de.accountName))}</th>
          <th style="width: 150px;">${escapeHtml(state.t('domain', labels.de.domain))}</th>
          <th style="width: 120px;">Outbound</th>
          <th class="is-num" style="width: 96px;">Fit</th>
          <th style="width: 120px;">Pipeline</th>
          <th style="width: 108px;"></th>
        </tr>
      </thead>
      <tbody>${rows.map(outboundHandoffRow).join('')}</tbody>
    </table>
  `;
}

function renderDedupeTable(rows) {
  if (!rows.length) {
    return `
      <div class="ctox-empty">
        <strong>${escapeHtml(state.t('noDedupe', labels.de.noDedupe))}</strong>
        <span>${escapeHtml(state.t('noDedupeBody', labels.de.noDedupeBody))}</span>
      </div>
    `;
  }
  return `
    <table class="ctox-table customers-table customers-dedupe-table" aria-label="${escapeAttribute(state.t('dedupe', labels.de.dedupe))}">
      <thead>
        <tr>
          <th>${escapeHtml(state.t('matchType', labels.de.matchType))}</th>
          <th style="width: 170px;">${escapeHtml(state.t('source', labels.de.source))}</th>
          <th style="width: 170px;">${escapeHtml(state.t('existingRecord', labels.de.existingRecord))}</th>
          <th class="is-num" style="width: 100px;">${escapeHtml(state.t('confidence', labels.de.confidence))}</th>
          <th style="width: 128px;">${escapeHtml(state.t('status', labels.de.status))}</th>
          <th style="width: 220px;"></th>
        </tr>
      </thead>
      <tbody>${rows.map(dedupeTableRow).join('')}</tbody>
    </table>
  `;
}

function renderOpportunityTable(opportunities) {
  return `
    <table class="ctox-table customers-table customers-opportunity-table" aria-label="${escapeAttribute(state.t('opportunities', labels.de.opportunities))}">
      <thead>
        <tr>
          ${sortableTh('name', state.t('opportunityName', labels.de.opportunityName))}
          ${sortableTh('account_name', state.t('accounts', labels.de.accounts), '170px')}
          ${sortableTh('stage', state.t('stage', labels.de.stage), '130px')}
          ${sortableTh('opportunity_type', state.t('type', labels.de.type), '130px')}
          ${sortableTh('amount_cents', state.t('amount', labels.de.amount), '120px', true)}
          ${sortableTh('close_date_ms', state.t('closeDate', labels.de.closeDate), '120px')}
          <th style="width: 132px;"></th>
        </tr>
      </thead>
      <tbody>${opportunities.map((opportunity) => opportunityTableRow(opportunity)).join('')}</tbody>
    </table>
  `;
}

function outboundHandoffRow(row) {
  const selected = row.company.id === state.selectedOutboundCompanyId;
  return `
    <tr class="customers-account-row" data-customers-outbound-company-id="${escapeAttribute(row.company.id)}" ${selectableAttrs('outbound', row.company.id, selected, row.company.name || row.company.id)}>
      <td>
        <div class="customers-name-cell">
          <span class="ctox-avatar">${escapeHtml(initials(row.company.name))}</span>
          <span>${escapeHtml(row.company.name || row.company.id)}</span>
        </div>
      </td>
      <td>${escapeHtml(row.domain || '')}</td>
      <td><span class="ctox-badge">${escapeHtml(row.company.qualification_status || 'outbound')}</span></td>
      <td class="is-num">${Number(row.company.fit_score || 0) ? `${Number(row.company.fit_score)}%` : '—'}</td>
      <td>${escapeHtml(row.pipeline?.stage || row.pipeline?.outreach_status || '—')}</td>
      <td>
        ${row.status === 'imported'
          ? `<span class="ctox-badge is-success">${escapeHtml(state.t('imported', labels.de.imported))}</span>`
          : row.status === 'needs_review'
            ? `<span class="ctox-badge is-danger">${escapeHtml(state.t('needsReview', labels.de.needsReview))}</span>`
            : `<button class="ctox-button ctox-button--sm" type="button" data-customers-action="import-outbound" data-outbound-company-id="${escapeAttribute(row.company.id)}"${mutableDisabledAttr()}>${escapeHtml(state.t('importFromOutbound', labels.de.importFromOutbound))}</button>`}
      </td>
    </tr>
  `;
}

function dedupeTableRow(candidate) {
  const selected = candidate.id === state.selectedDedupeCandidateId;
  const existing = accountById(candidate.existing_record_id);
  return `
    <tr class="customers-account-row" data-customers-dedupe-candidate-id="${escapeAttribute(candidate.id)}" ${selectableAttrs('dedupe', candidate.id, selected, candidate.match_key || candidate.id)}>
      <td>
        <strong>${escapeHtml(candidate.match_key || candidate.id)}</strong>
        <span class="customers-muted-inline">${escapeHtml(candidate.match_type || candidate.object_type || '')}</span>
      </td>
      <td>${escapeHtml(candidate.source_record_id || '')}</td>
      <td>${escapeHtml(existing?.name || candidate.existing_record_id || '')}</td>
      <td class="is-num">${Math.round(Number(candidate.confidence || 0) * 100)}%</td>
      <td><span class="ctox-badge">${escapeHtml(candidate.status || 'open')}</span></td>
      <td>${dedupeActions(candidate)}</td>
    </tr>
  `;
}

function dedupeActions(candidate) {
  if (candidate.status === 'resolved') {
    return `<span class="ctox-badge">${escapeHtml(candidate.decision || 'resolved')}</span>`;
  }
  return `
    <div class="customers-row-actions">
      <button class="ctox-button ctox-button--sm" type="button" data-customers-action="dedupe-keep-existing" data-dedupe-candidate-id="${escapeAttribute(candidate.id)}"${mutableDisabledAttr()}>${escapeHtml(state.t('keepExisting', labels.de.keepExisting))}</button>
      <button class="ctox-button ctox-button--sm" type="button" data-customers-action="dedupe-create-new" data-dedupe-candidate-id="${escapeAttribute(candidate.id)}"${mutableDisabledAttr()}>${escapeHtml(state.t('createNew', labels.de.createNew))}</button>
      <button class="ctox-button ctox-button--sm" type="button" data-customers-action="dedupe-merge" data-dedupe-candidate-id="${escapeAttribute(candidate.id)}"${mutableDisabledAttr()}>${escapeHtml(state.t('merge', labels.de.merge))}</button>
      <button class="ctox-button ctox-button--sm is-danger" type="button" data-customers-action="dedupe-skip" data-dedupe-candidate-id="${escapeAttribute(candidate.id)}"${mutableDisabledAttr()}>${escapeHtml(state.t('skip', labels.de.skip))}</button>
    </div>
  `;
}

function renderOpportunityBoard(opportunities) {
  const groups = groupOpportunitiesByStage(opportunities);
  return `
    <div class="customers-board" role="list" aria-label="Opportunity pipeline">
      ${Object.entries(OPPORTUNITY_STAGE_LABELS).map(([stage, label]) => {
        const items = groups[stage] || [];
        const summary = summarizeOpportunityPipeline(items);
        return `
          <section class="customers-board-column" data-opportunity-stage="${escapeAttribute(stage)}" role="listitem">
            <header class="customers-board-head">
              <div>
                <strong>${escapeHtml(label)}</strong>
                <span>${items.length} · ${escapeHtml(formatMoney(summary.total_cents, summary.currency))}</span>
              </div>
            </header>
            <div class="customers-board-list">
              ${items.map((opportunity) => opportunityBoardCard(opportunity)).join('') || `<div class="customers-board-empty">Leer</div>`}
            </div>
          </section>
        `;
      }).join('')}
    </div>
  `;
}

function opportunityTableRow(opportunity) {
  const selected = opportunity.id === state.selectedOpportunityId;
  const account = accountById(opportunity.account_id);
  return `
    <tr class="customers-account-row" data-customers-opportunity-id="${escapeAttribute(opportunity.id)}" ${selectableAttrs('opportunity', opportunity.id, selected, opportunity.name || opportunity.id)}>
      <td>${escapeHtml(opportunity.name || opportunity.id)}</td>
      <td>${escapeHtml(account?.name || opportunity.account_id || '')}</td>
      <td><span class="ctox-badge${stageBadgeClass(opportunity.stage)}">${escapeHtml(labelFor(OPPORTUNITY_STAGE_LABELS, opportunity.stage))}</span></td>
      <td>${escapeHtml(labelFor(OPPORTUNITY_TYPE_LABELS, opportunity.opportunity_type))}</td>
      <td class="is-num">${escapeHtml(formatMoney(opportunity.amount_cents, opportunity.currency))}</td>
      <td>${escapeHtml(formatDate(opportunity.close_date_ms, state.lang))}</td>
      <td>${opportunityActions(opportunity)}</td>
    </tr>
  `;
}

function opportunityBoardCard(opportunity) {
  const account = accountById(opportunity.account_id);
  return `
    <article class="customers-board-card" data-customers-opportunity-id="${escapeAttribute(opportunity.id)}" ${selectableAttrs('opportunity', opportunity.id, opportunity.id === state.selectedOpportunityId, opportunity.name || opportunity.id)}>
      <div class="customers-board-card-title">${escapeHtml(opportunity.name || opportunity.id)}</div>
      <div class="customers-board-card-meta">${escapeHtml(account?.name || '')}</div>
      <div class="customers-board-card-row">
        <span>${escapeHtml(formatMoney(opportunity.amount_cents, opportunity.currency))}</span>
        <span>${escapeHtml(formatDate(opportunity.close_date_ms, state.lang))}</span>
      </div>
      ${opportunityActions(opportunity)}
    </article>
  `;
}

function opportunityActions(opportunity) {
  const isClosed = isClosedOpportunity(opportunity);
  const nextStages = Object.keys(OPPORTUNITY_STAGE_LABELS)
    .filter((stage) => stage !== opportunity.stage)
    .filter((stage) => !(opportunity.stage || '').startsWith('closed_') || stage.startsWith('closed_'));
  return `
    <div class="customers-row-actions">
      <select class="ctox-select customers-inline-select" data-opportunity-stage-select="${escapeAttribute(opportunity.id)}" aria-label="${escapeAttribute(state.t('moveTo', labels.de.moveTo))}"${isClosed ? ' disabled' : mutableDisabledAttr()}>
        <option value="">${escapeHtml(state.t('moveTo', labels.de.moveTo))}</option>
        ${nextStages.map((stage) => `<option value="${escapeAttribute(stage)}">${escapeHtml(labelFor(OPPORTUNITY_STAGE_LABELS, stage))}</option>`).join('')}
      </select>
      <button class="ctox-icon-button" type="button" data-customers-action="edit-opportunity" data-opportunity-id="${escapeAttribute(opportunity.id)}" aria-label="${escapeAttribute(state.t('editOpportunity', labels.de.editOpportunity))}"${mutableDisabledAttr()}>${actionIcon('edit')}</button>
      ${isClosed ? '' : `<button class="ctox-button ctox-button--sm" type="button" data-customers-action="close-won" data-opportunity-id="${escapeAttribute(opportunity.id)}"${mutableDisabledAttr()}>${escapeHtml(state.t('closeWon', labels.de.closeWon))}</button>`}
      ${isClosed ? '' : `<button class="ctox-button ctox-button--sm is-danger" type="button" data-customers-action="close-lost" data-opportunity-id="${escapeAttribute(opportunity.id)}"${mutableDisabledAttr()}>${escapeHtml(state.t('closeLost', labels.de.closeLost))}</button>`}
    </div>
  `;
}

function isClosedOpportunity(opportunity) {
  return String(opportunity?.stage || '').startsWith('closed_');
}

function sortableTh(field, label, width = '', numeric = false) {
  const sort = state.centerView === 'contacts'
    ? state.contactSort
    : state.centerView === 'opportunities'
      ? state.opportunitySort
      : state.accountSort;
  const active = sort.field === field;
  const direction = sort.direction;
  const ariaSort = active ? (direction === 'asc' ? 'ascending' : 'descending') : 'none';
  const sortLabel = state.t('sortBy', labels.de.sortBy, label);
  const statusLabel = active
    ? (direction === 'asc' ? state.t('sortAscending', labels.de.sortAscending) : state.t('sortDescending', labels.de.sortDescending))
    : '';
  return `<th${width ? ` style="width: ${escapeAttribute(width)};"` : ''}${numeric ? ' class="is-num"' : ''} aria-sort="${ariaSort}"><button class="ctox-table-sort${active ? ' active' : ''}" type="button" data-customers-sort="${escapeAttribute(field)}" aria-label="${escapeAttribute([sortLabel, statusLabel].filter(Boolean).join(' · '))}"><span>${escapeHtml(label)}</span>${active ? `<span aria-hidden="true">${direction === 'asc' ? '↑' : '↓'}</span>` : ''}</button></th>`;
}

function accountRow(account) {
  const selected = account.id === state.selectedAccountId;
  return `
    <tr class="customers-account-row" data-customers-account-id="${escapeAttribute(account.id)}" ${selectableAttrs('account', account.id, selected, account.name || account.id)}>
      <td>
        <div class="customers-name-cell">
          <span class="ctox-avatar">${escapeHtml(initials(account.name))}</span>
          <span>${escapeHtml(account.name || account.id)}</span>
        </div>
      </td>
      <td><span class="ctox-badge${stageBadgeClass(account.customer_stage)}">${escapeHtml(labelFor(STAGE_LABELS, account.customer_stage))}</span></td>
      <td><span class="ctox-badge${healthBadgeClass(account.health_status)}">${escapeHtml(labelFor(HEALTH_LABELS, account.health_status))}</span></td>
      <td>${escapeHtml(account.domain || account.website_url || '')}</td>
      <td class="is-num">${escapeHtml(formatMoney(account.annual_recurring_revenue_cents, account.currency))}</td>
      <td>${escapeHtml(formatDate(account.last_activity_at_ms || account.updated_at_ms, state.lang))}</td>
    </tr>
  `;
}

function contactTableRow(contact) {
  const selected = contact.id === state.selectedContactId;
  const account = accountById(contact.account_id);
  return `
    <tr class="customers-account-row" data-customers-contact-id="${escapeAttribute(contact.id)}" ${selectableAttrs('contact', contact.id, selected, contactDisplayName(contact))}>
      <td>
        <div class="customers-name-cell">
          <span class="ctox-avatar">${escapeHtml(initials(contactDisplayName(contact)))}</span>
          <span>${escapeHtml(contactDisplayName(contact))}</span>
        </div>
      </td>
      <td>${escapeHtml(account?.name || contact.account_id || '')}</td>
      <td>${escapeHtml(contact.job_title || '')}</td>
      <td>${escapeHtml(contact.email || '')}</td>
      <td>${contact.is_primary_contact ? `<span class="ctox-badge is-success">${escapeHtml(state.t('primary', labels.de.primary))}</span>` : ''}</td>
      <td>
        <div class="customers-row-actions">
          <button class="ctox-icon-button" type="button" data-customers-action="edit-contact" data-contact-id="${escapeAttribute(contact.id)}" aria-label="${escapeAttribute(state.t('editContact', labels.de.editContact))}"${mutableDisabledAttr()}>${actionIcon('edit')}</button>
          <button class="ctox-icon-button is-danger" type="button" data-customers-action="archive-contact" data-contact-id="${escapeAttribute(contact.id)}" aria-label="${escapeAttribute(state.t('archiveContact', labels.de.archiveContact))}"${mutableDisabledAttr()}>${actionIcon('archive')}</button>
        </div>
      </td>
    </tr>
  `;
}

function renderRight() {
  const target = state.ctx.host.querySelector('[data-customers-right]');
  if (!target) return;
  if (state.formMode) {
    target.innerHTML = renderForm();
    return;
  }
  if (state.selectedOutboundCompanyId && state.centerView === 'handoff') {
    target.innerHTML = renderOutboundInspector();
    return;
  }
  if (state.selectedDedupeCandidateId && state.centerView === 'dedupe') {
    target.innerHTML = renderDedupeInspector();
    return;
  }
  const account = selectedAccount();
  if (!account) {
    target.innerHTML = `
      <header class="ctox-pane-header ctox-pane-band">
        <div class="ctox-pane-title-row">
          <div class="ctox-pane-titles">
            <span class="ctox-pane-kicker">Inspector</span>
            <h2 class="ctox-pane-title">${escapeHtml(state.t('title', labels.de.title))}</h2>
          </div>
        </div>
      </header>
      <div class="ctox-empty">
        <strong>${escapeHtml(state.t('inspectorEmpty', labels.de.inspectorEmpty))}</strong>
        <span>${escapeHtml(state.t('inspectorEmptyBody', labels.de.inspectorEmptyBody))}</span>
      </div>
    `;
    return;
  }
  const context = activeRecordContext();
  const title = context?.title || account.name || account.id;
  const subtitle = context?.subtitle || [account.domain, account.industry].filter(Boolean).join(' · ') || account.website_url || '';
  target.innerHTML = `
    <header class="ctox-pane-header ctox-pane-band">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">${escapeHtml(context?.typeLabel || 'Inspector')}</span>
          <h2 class="ctox-pane-title">${escapeHtml(title)}</h2>
        </div>
        ${renderRecordHeaderActions(context)}
      </div>
    </header>
    <div class="ctox-pane-scroll customers-right-scroll">
      ${renderPermissionNotice()}
      <section class="customers-detail-block">
        <h3 class="customers-detail-title">${escapeHtml(title)}</h3>
        <p class="customers-detail-subtitle">${escapeHtml(subtitle)}</p>
        ${renderRecordChips(context)}
      </section>
      <section class="customers-detail-tabs">
        <div class="ctox-pane-tabs" role="tablist" aria-label="${escapeAttribute(state.t('recordDetails', labels.de.recordDetails))}">
        ${detailTabButton('overview', state.t('overview', labels.de.overview))}
        ${detailTabButton('tasks', state.t('tasks', labels.de.tasks))}
        ${detailTabButton('notes', state.t('notes', labels.de.notes))}
        ${detailTabButton('timeline', state.t('timeline', labels.de.timeline))}
        ${detailTabButton('files', state.t('files', labels.de.files))}
        ${detailTabButton('apps', state.t('apps', labels.de.apps))}
        </div>
      </section>
      ${renderDetailTabContent(context)}
      ${renderOperationalAuditPanel(context)}
    </div>
  `;
}

function renderOutboundInspector() {
  const row = outboundHandoffRowByCompanyId(state.selectedOutboundCompanyId);
  if (!row) return renderInspectorEmpty();
  const contacts = row.pipeline?.contacts || [];
  return `
    <header class="ctox-pane-header ctox-pane-band">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">${escapeHtml(state.t('handoff', labels.de.handoff))}</span>
          <h2 class="ctox-pane-title">${escapeHtml(row.company.name || row.company.id)}</h2>
        </div>
        ${row.status === 'ready' ? `<div class="ctox-pane-actions"><button class="ctox-pane-icon" type="button" data-customers-action="import-outbound" data-outbound-company-id="${escapeAttribute(row.company.id)}" aria-label="${escapeAttribute(state.t('importFromOutbound', labels.de.importFromOutbound))}" title="${escapeAttribute(state.t('importFromOutbound', labels.de.importFromOutbound))}"${mutableDisabledAttr()}>${actionIcon('download')}</button></div>` : ''}
      </div>
    </header>
    <div class="ctox-pane-scroll customers-right-scroll">
      ${renderPermissionNotice()}
      <section class="customers-detail-block">
        <h3 class="customers-detail-title">${escapeHtml(row.company.name || row.company.id)}</h3>
        <p class="customers-detail-subtitle">${escapeHtml([row.domain, row.company.city, row.company.country].filter(Boolean).join(' · '))}</p>
        <div class="customers-chip-row">
          <span class="ctox-badge">${escapeHtml(row.company.qualification_status || 'outbound')}</span>
          <span class="ctox-badge">${escapeHtml(row.company.research_status || 'research')}</span>
          <span class="ctox-badge">${escapeHtml(row.status)}</span>
        </div>
      </section>
      <section class="customers-detail-block">
        <dl class="ctox-fields">
          ${metric('Fit', Number(row.company.fit_score || 0) ? `${Number(row.company.fit_score)}%` : '—')}
          ${metric('Pipeline', row.pipeline?.stage || row.pipeline?.outreach_status || '—')}
          ${metric(state.t('contacts', labels.de.contacts), contacts.length)}
          ${metric(state.t('domain', labels.de.domain), row.domain || '—')}
        </dl>
      </section>
      <section class="customers-detail-block">
        <h3 class="ctox-field-label customers-section-heading">${escapeHtml(state.t('contacts', labels.de.contacts))}</h3>
        <div class="customers-mini-list">
          ${contacts.map(outboundContactRow).join('') || emptyMiniRow(state.t('noContacts', labels.de.noContacts))}
        </div>
      </section>
      ${renderOperationalAuditPanel({ type: 'outbound', id: row.company.id })}
    </div>
  `;
}

function renderDedupeInspector() {
  const candidate = (state.collections.customer_dedupe_candidates || []).find((item) => item.id === state.selectedDedupeCandidateId);
  if (!candidate) return renderInspectorEmpty();
  const existing = accountById(candidate.existing_record_id);
  const outboundCompany = candidate.payload?.outbound_company || outboundCompanyById(candidate.source_record_id) || {};
  return `
    <header class="ctox-pane-header ctox-pane-band">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">${escapeHtml(state.t('dedupe', labels.de.dedupe))}</span>
          <h2 class="ctox-pane-title">${escapeHtml(candidate.match_key || candidate.id)}</h2>
        </div>
      </div>
    </header>
    <div class="ctox-pane-scroll customers-right-scroll">
      ${renderPermissionNotice()}
      <section class="customers-detail-block">
        <h3 class="customers-detail-title">${escapeHtml(candidate.match_key || candidate.id)}</h3>
        <p class="customers-detail-subtitle">${escapeHtml([candidate.object_type, candidate.match_type, `${Math.round(Number(candidate.confidence || 0) * 100)}%`].filter(Boolean).join(' · '))}</p>
        <div class="customers-chip-row">
          <span class="ctox-badge">${escapeHtml(candidate.status || 'open')}</span>
          ${candidate.decision ? `<span class="ctox-badge">${escapeHtml(candidate.decision)}</span>` : ''}
        </div>
      </section>
      <section class="customers-detail-block">
        <dl class="ctox-fields">
          ${metric(state.t('source', labels.de.source), outboundCompany.name || candidate.source_record_id || '—')}
          ${metric(state.t('existingRecord', labels.de.existingRecord), existing?.name || candidate.existing_record_id || '—')}
          ${metric(state.t('matchType', labels.de.matchType), candidate.match_type || '—')}
          ${metric(state.t('importBatch', labels.de.importBatch), candidate.import_batch_id || '—')}
        </dl>
      </section>
      ${candidate.status === 'open' ? `
        <section class="customers-detail-block">
          <h3 class="ctox-field-label customers-section-heading">${escapeHtml(state.t('resolveDedupe', labels.de.resolveDedupe))}</h3>
          <div class="customers-command-grid">
            <button class="ctox-button ctox-button--sm" type="button" data-customers-action="dedupe-keep-existing" data-dedupe-candidate-id="${escapeAttribute(candidate.id)}"${mutableDisabledAttr()}>${escapeHtml(state.t('keepExisting', labels.de.keepExisting))}</button>
            <button class="ctox-button ctox-button--sm" type="button" data-customers-action="dedupe-create-new" data-dedupe-candidate-id="${escapeAttribute(candidate.id)}"${mutableDisabledAttr()}>${escapeHtml(state.t('createNew', labels.de.createNew))}</button>
            <button class="ctox-button ctox-button--sm" type="button" data-customers-action="dedupe-merge" data-dedupe-candidate-id="${escapeAttribute(candidate.id)}"${mutableDisabledAttr()}>${escapeHtml(state.t('merge', labels.de.merge))}</button>
            <button class="ctox-button ctox-button--sm is-danger" type="button" data-customers-action="dedupe-skip" data-dedupe-candidate-id="${escapeAttribute(candidate.id)}"${mutableDisabledAttr()}>${escapeHtml(state.t('skip', labels.de.skip))}</button>
          </div>
        </section>
      ` : ''}
      ${renderOperationalAuditPanel({ type: 'dedupe', id: candidate.id })}
    </div>
  `;
}

function renderInspectorEmpty() {
  return `
    <header class="ctox-pane-header ctox-pane-band">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">Inspector</span>
          <h2 class="ctox-pane-title">${escapeHtml(state.t('title', labels.de.title))}</h2>
        </div>
      </div>
    </header>
    <div class="ctox-empty">
      <strong>${escapeHtml(state.t('inspectorEmpty', labels.de.inspectorEmpty))}</strong>
      <span>${escapeHtml(state.t('inspectorEmptyBody', labels.de.inspectorEmptyBody))}</span>
    </div>
  `;
}

function renderRecordHeaderActions(context) {
  if (!context) return '';
  if (context.type === 'contact') {
    return `
      <div class="ctox-pane-actions">
        <button class="ctox-pane-icon" type="button" data-customers-action="edit-contact" data-contact-id="${escapeAttribute(context.id)}" aria-label="${escapeAttribute(state.t('editContact', labels.de.editContact))}" title="${escapeAttribute(state.t('editContact', labels.de.editContact))}"${mutableDisabledAttr()}>${actionIcon('edit')}</button>
        <button class="ctox-pane-icon is-danger" type="button" data-customers-action="archive-contact" data-contact-id="${escapeAttribute(context.id)}" aria-label="${escapeAttribute(state.t('archiveContact', labels.de.archiveContact))}" title="${escapeAttribute(state.t('archiveContact', labels.de.archiveContact))}"${mutableDisabledAttr()}>${actionIcon('archive')}</button>
      </div>
    `;
  }
  if (context.type === 'opportunity') {
    return `
      <div class="ctox-pane-actions">
        <button class="ctox-pane-icon" type="button" data-customers-action="edit-opportunity" data-opportunity-id="${escapeAttribute(context.id)}" aria-label="${escapeAttribute(state.t('editOpportunity', labels.de.editOpportunity))}" title="${escapeAttribute(state.t('editOpportunity', labels.de.editOpportunity))}"${mutableDisabledAttr()}>${actionIcon('edit')}</button>
      </div>
    `;
  }
  return `
    <div class="ctox-pane-actions">
      <button class="ctox-pane-icon" type="button" data-customers-action="edit-account" data-account-id="${escapeAttribute(context.id)}" aria-label="${escapeAttribute(state.t('editCustomer', labels.de.editCustomer))}" title="${escapeAttribute(state.t('editCustomer', labels.de.editCustomer))}"${mutableDisabledAttr()}>${actionIcon('edit')}</button>
      <button class="ctox-pane-icon is-danger" type="button" data-customers-action="archive-account" data-account-id="${escapeAttribute(context.id)}" aria-label="${escapeAttribute(state.t('archiveCustomer', labels.de.archiveCustomer))}" title="${escapeAttribute(state.t('archiveCustomer', labels.de.archiveCustomer))}"${mutableDisabledAttr()}>${actionIcon('archive')}</button>
    </div>
  `;
}

function renderRecordChips(context) {
  if (!context) return '';
  if (context.type === 'opportunity') {
    const opportunity = context.opportunity;
    return `
      <div class="customers-chip-row">
        <span class="ctox-badge${stageBadgeClass(opportunity.stage)}">${escapeHtml(labelFor(OPPORTUNITY_STAGE_LABELS, opportunity.stage))}</span>
        <span class="ctox-badge">${escapeHtml(labelFor(OPPORTUNITY_TYPE_LABELS, opportunity.opportunity_type))}</span>
        <span class="ctox-badge">${escapeHtml(formatMoney(opportunity.amount_cents, opportunity.currency))}</span>
        ${presenceChip('customer_opportunities', context.id)}
      </div>
    `;
  }
  if (context.type === 'contact') {
    const contact = context.contact;
    return `
      <div class="customers-chip-row">
        ${contact.is_primary_contact ? `<span class="ctox-badge is-success">${escapeHtml(state.t('primary', labels.de.primary))}</span>` : ''}
        ${contact.email ? `<span class="ctox-badge">${escapeHtml(contact.email)}</span>` : ''}
        ${contact.phone ? `<span class="ctox-badge">${escapeHtml(contact.phone)}</span>` : ''}
        ${presenceChip('customer_contacts', context.id)}
      </div>
    `;
  }
  const account = context.account;
  return `
    <div class="customers-chip-row">
      <span class="ctox-badge">${escapeHtml(labelFor(ACCOUNT_STATUS_LABELS, account.account_status))}</span>
      <span class="ctox-badge${stageBadgeClass(account.customer_stage)}">${escapeHtml(labelFor(STAGE_LABELS, account.customer_stage))}</span>
      <span class="ctox-badge${healthBadgeClass(account.health_status)}">${escapeHtml(labelFor(HEALTH_LABELS, account.health_status))}</span>
      ${presenceChip('customer_accounts', context.id)}
    </div>
  `;
}

function detailTabButton(tab, label) {
  return `
    <button class="ctox-pane-tab" type="button" role="tab" aria-selected="${state.detailTab === tab ? 'true' : 'false'}" data-customers-detail-tab="${escapeAttribute(tab)}">
      ${escapeHtml(label)}
    </button>
  `;
}

function renderDetailTabContent(context) {
  if (!context) return '';
  if (state.detailTab === 'tasks') return renderTasksTab(context);
  if (state.detailTab === 'notes') return renderNotesTab(context);
  if (state.detailTab === 'timeline') return renderTimelineTab(context);
  if (state.detailTab === 'files') return renderFilesTab(context);
  if (state.detailTab === 'apps') return renderAppsTab(context);
  return renderOverviewTab(context);
}

function renderOverviewTab(context) {
  const related = relatedRecords(context.account?.id || context.id, state.collections);
  if (context.type === 'opportunity') {
    const opportunity = context.opportunity;
    return `
      <section class="customers-detail-block">
        <dl class="ctox-fields">
          ${metric(state.t('stage', labels.de.stage), labelFor(OPPORTUNITY_STAGE_LABELS, opportunity.stage))}
          ${metric(state.t('amount', labels.de.amount), formatMoney(opportunity.amount_cents, opportunity.currency))}
          ${metric(state.t('closeDate', labels.de.closeDate), formatDate(opportunity.close_date_ms, state.lang))}
          ${metric(state.t('probability', labels.de.probability), `${Number(opportunity.probability || 0)}%`)}
        </dl>
      </section>
      <section class="customers-detail-block">
        <h3 class="ctox-field-label customers-section-heading">${escapeHtml(state.t('contacts', labels.de.contacts))}</h3>
        <div class="customers-mini-list">
          ${context.contact ? contactRow(context.contact) : emptyMiniRow(state.t('noContacts', labels.de.noContacts))}
        </div>
      </section>
    `;
  }
  if (context.type === 'contact') {
    const contact = context.contact;
    return `
      <section class="customers-detail-block">
        <dl class="ctox-fields">
          ${metric(state.t('accounts', labels.de.accounts), context.account?.name || contact.account_id || '—')}
          ${metric(state.t('jobTitle', labels.de.jobTitle), contact.job_title || '—')}
          ${metric(state.t('email', labels.de.email), contact.email || '—')}
          ${metric(state.t('phone', labels.de.phone), contact.phone || '—')}
        </dl>
      </section>
      <section class="customers-detail-block">
        <h3 class="ctox-field-label customers-section-heading">${escapeHtml(state.t('opportunities', labels.de.opportunities))}</h3>
        <div class="customers-mini-list">
          ${related.opportunities.filter((item) => item.primary_contact_id === contact.id).map(opportunityRow).join('') || emptyMiniRow(state.t('noOpportunities', labels.de.noOpportunities))}
        </div>
      </section>
    `;
  }
  return `
    <section class="customers-detail-block">
      <dl class="ctox-fields">
        ${metric(state.t('contacts', labels.de.contacts), related.contacts.length)}
        ${metric(state.t('opportunities', labels.de.opportunities), related.opportunities.length)}
        ${metric(state.t('openTasks', labels.de.openTasks), related.openTasks.length)}
        ${metric(state.t('arr', labels.de.arr), formatMoney(context.account.annual_recurring_revenue_cents, context.account.currency))}
      </dl>
    </section>
      <section class="customers-detail-block">
        <div class="customers-section-head">
          <h3 class="ctox-field-label customers-section-heading">${escapeHtml(state.t('contacts', labels.de.contacts))}</h3>
        <button class="ctox-button ctox-button--sm" type="button" data-customers-action="create-contact"${mutableDisabledAttr()}>${escapeHtml(state.t('newContact', labels.de.newContact))}</button>
      </div>
      <div class="customers-mini-list">
        ${related.contacts.slice(0, 7).map(contactRow).join('') || emptyMiniRow(state.t('noContacts', labels.de.noContacts))}
      </div>
    </section>
    <section class="customers-detail-block">
      <h3 class="ctox-field-label customers-section-heading">${escapeHtml(state.t('opportunities', labels.de.opportunities))}</h3>
      <div class="customers-mini-list">
        ${related.opportunities.slice(0, 5).map(opportunityRow).join('') || emptyMiniRow(state.t('noOpportunities', labels.de.noOpportunities))}
      </div>
    </section>
  `;
}

function renderTasksTab(context) {
  const tasks = filterRelatedTasks(context, state.collections);
  return `
    <section class="customers-detail-block">
      <div class="customers-section-head">
        <h3 class="ctox-field-label customers-section-heading">${escapeHtml(state.t('tasks', labels.de.tasks))}</h3>
        <button class="ctox-button ctox-button--sm" type="button" data-customers-action="create-task"${mutableDisabledAttr()}>${escapeHtml(state.t('newTask', labels.de.newTask))}</button>
      </div>
      <div class="customers-task-list">
        ${tasks.map(taskRow).join('') || emptyMiniRow(state.t('noTasks', labels.de.noTasks))}
      </div>
    </section>
  `;
}

function renderNotesTab(context) {
  const notes = filterRelatedNotes(context, state.collections);
  return `
    <section class="customers-detail-block">
      <div class="customers-section-head">
        <h3 class="ctox-field-label customers-section-heading">${escapeHtml(state.t('notes', labels.de.notes))}</h3>
        <button class="ctox-button ctox-button--sm" type="button" data-customers-action="create-note"${mutableDisabledAttr()}>${escapeHtml(state.t('newNote', labels.de.newNote))}</button>
      </div>
      <div class="customers-note-list">
        ${notes.map(noteRow).join('') || emptyMiniRow(state.t('noNotes', labels.de.noNotes))}
      </div>
    </section>
  `;
}

function renderTimelineTab(context) {
  const rows = buildTimelineRows(context, state.collections);
  return `
    <section class="customers-detail-block">
      <h3 class="ctox-field-label customers-section-heading">${escapeHtml(state.t('timeline', labels.de.timeline))}</h3>
      <div class="customers-timeline">
        ${rows.map(timelineRow).join('') || emptyMiniRow(state.t('noTimeline', labels.de.noTimeline))}
      </div>
    </section>
  `;
}

function renderFilesTab(context) {
  const files = filterRelatedFiles(context, state.collections);
  return `
    <section class="customers-detail-block">
      <h3 class="ctox-field-label customers-section-heading">${escapeHtml(state.t('files', labels.de.files))}</h3>
      <div class="customers-mini-list">
        ${files.map(fileRow).join('') || emptyMiniRow(state.t('noFiles', labels.de.noFiles))}
      </div>
    </section>
  `;
}

function renderAppsTab(context) {
  const links = buildLinkedAppRows(context, state.collections);
  return `
    <section class="customers-detail-block">
      <h3 class="ctox-field-label customers-section-heading">${escapeHtml(state.t('linkedApps', labels.de.linkedApps))}</h3>
      <div class="customers-link-list">
        ${links.map(linkedAppRow).join('') || `
          <div class="ctox-empty">
            <strong>${escapeHtml(state.t('noAppLinks', labels.de.noAppLinks))}</strong>
            <span>${escapeHtml(state.t('noAppLinksBody', labels.de.noAppLinksBody))}</span>
          </div>
        `}
      </div>
    </section>
  `;
}

function contactRow(contact) {
  const selected = contact.id === state.selectedContactId;
  return `
    <div class="customers-mini-row" data-customers-contact-id="${escapeAttribute(contact.id)}" ${selectableAttrs('contact', contact.id, selected, contactDisplayName(contact))}>
      <span>${escapeHtml(contactDisplayName(contact))}</span>
      <span>${escapeHtml(contact.job_title || contact.email || '')}</span>
    </div>
  `;
}

function outboundContactRow(contact) {
  return `
    <div class="customers-mini-row">
      <span>${escapeHtml(contact.name || [contact.first_name, contact.last_name].filter(Boolean).join(' ') || contact.email || '')}</span>
      <span>${escapeHtml(contact.job_title || contact.title || contact.email || '')}</span>
    </div>
  `;
}

function renderForm() {
  if (state.formMode.startsWith('task')) return renderTaskForm();
  if (state.formMode.startsWith('note')) return renderNoteForm();
  if (state.formMode.startsWith('opportunity')) return renderOpportunityForm();
  if (state.formMode.startsWith('contact')) return renderContactForm();
  return renderAccountForm();
}

function renderAccountForm() {
  const account = state.formMode === 'account-edit' ? selectedAccount() : null;
  const title = state.formMode === 'account-edit'
    ? state.t('editCustomer', labels.de.editCustomer)
    : state.t('newCustomer', labels.de.newCustomer);
  return `
    <header class="ctox-pane-header ctox-pane-band">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">${escapeHtml(state.t('actionKicker', labels.de.actionKicker))}</span>
          <h2 class="ctox-pane-title">${escapeHtml(title)}</h2>
        </div>
      </div>
    </header>
    <div class="ctox-pane-scroll customers-right-scroll">
      <section class="customers-detail-block">
        <form class="customers-form" data-customers-form data-form-kind="${escapeAttribute(state.formMode)}">
          <label class="ctox-compact-field">${escapeHtml(state.t('accountName', labels.de.accountName))}<input class="ctox-input" name="name" autocomplete="organization" required value="${escapeAttribute(account?.name || '')}"></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('domain', labels.de.domain))}<input class="ctox-input" name="domain" inputmode="url" placeholder="example.com" value="${escapeAttribute(account?.domain || account?.website_url || '')}"></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('industry', labels.de.industry))}<input class="ctox-input" name="industry" value="${escapeAttribute(account?.industry || '')}"></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('owner', labels.de.owner))}<input class="ctox-input" name="account_owner_id" value="${escapeAttribute(account?.account_owner_id || '')}"></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('arr', labels.de.arr))}<input class="ctox-input" name="annual_recurring_revenue" inputmode="decimal" value="${escapeAttribute(account?.annual_recurring_revenue_cents ? String(Number(account.annual_recurring_revenue_cents) / 100) : '')}"></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('status', labels.de.status))}<select class="ctox-select" name="account_status">${optionList(ACCOUNT_STATUS_LABELS, account?.account_status || 'active')}</select></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('stage', labels.de.stage))}<select class="ctox-select" name="customer_stage">${optionList(STAGE_LABELS, account?.customer_stage || 'active', ['archived'])}</select></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('health', labels.de.health))}<select class="ctox-select" name="health_status">${optionList(HEALTH_LABELS, account?.health_status || 'unknown')}</select></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('nextAction', labels.de.nextAction))}<input class="ctox-input" name="next_action_at" type="date" value="${escapeAttribute(dateInputValue(account?.next_action_at_ms))}"></label>
          ${formActions(state.formMode === 'account-edit' ? state.t('save', labels.de.save) : state.t('create', labels.de.create))}
        </form>
      </section>
    </div>
  `;
}

function renderContactForm() {
  const contact = state.formMode === 'contact-edit'
    ? state.collections.customer_contacts.find((item) => item.id === state.formRecordId)
    : null;
  const accountId = contact?.account_id || state.selectedAccountId || state.formRecordId;
  const title = state.formMode === 'contact-edit'
    ? state.t('editContact', labels.de.editContact)
    : state.t('newContact', labels.de.newContact);
  return `
    <header class="ctox-pane-header ctox-pane-band">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">${escapeHtml(state.t('actionKicker', labels.de.actionKicker))}</span>
          <h2 class="ctox-pane-title">${escapeHtml(title)}</h2>
        </div>
      </div>
    </header>
    <div class="ctox-pane-scroll customers-right-scroll">
      <section class="customers-detail-block">
        <form class="customers-form" data-customers-form data-form-kind="${escapeAttribute(state.formMode)}">
          <input type="hidden" name="account_id" value="${escapeAttribute(accountId || '')}">
          <label class="ctox-compact-field">${escapeHtml(state.t('firstName', labels.de.firstName))}<input class="ctox-input" name="first_name" autocomplete="given-name" value="${escapeAttribute(contact?.first_name || '')}"></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('lastName', labels.de.lastName))}<input class="ctox-input" name="last_name" autocomplete="family-name" value="${escapeAttribute(contact?.last_name || '')}"></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('email', labels.de.email))}<input class="ctox-input" name="email" type="email" autocomplete="email" value="${escapeAttribute(contact?.email || '')}"></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('phone', labels.de.phone))}<input class="ctox-input" name="phone" autocomplete="tel" value="${escapeAttribute(contact?.phone || '')}"></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('jobTitle', labels.de.jobTitle))}<input class="ctox-input" name="job_title" autocomplete="organization-title" value="${escapeAttribute(contact?.job_title || '')}"></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('city', labels.de.city))}<input class="ctox-input" name="city" autocomplete="address-level2" value="${escapeAttribute(contact?.city || '')}"></label>
          <label class="customers-checkbox"><input type="checkbox" name="is_primary_contact" value="true" ${contact?.is_primary_contact ? 'checked' : ''}>${escapeHtml(state.t('primary', labels.de.primary))}</label>
          ${formActions(state.formMode === 'contact-edit' ? state.t('save', labels.de.save) : state.t('create', labels.de.create))}
        </form>
      </section>
    </div>
  `;
}

function renderOpportunityForm() {
  const opportunity = state.formMode === 'opportunity-edit'
    ? state.collections.customer_opportunities.find((item) => item.id === state.formRecordId)
    : null;
  const accountId = opportunity?.account_id || state.selectedAccountId || state.formRecordId;
  const title = state.formMode === 'opportunity-edit'
    ? state.t('editOpportunity', labels.de.editOpportunity)
    : state.t('newOpportunity', labels.de.newOpportunity);
  const contacts = (state.collections.customer_contacts || []).filter((contact) => contact.account_id === accountId);
  return `
    <header class="ctox-pane-header ctox-pane-band">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">${escapeHtml(state.t('actionKicker', labels.de.actionKicker))}</span>
          <h2 class="ctox-pane-title">${escapeHtml(title)}</h2>
        </div>
      </div>
    </header>
    <div class="ctox-pane-scroll customers-right-scroll">
      <section class="customers-detail-block">
        <form class="customers-form" data-customers-form data-form-kind="${escapeAttribute(state.formMode)}">
          <input type="hidden" name="account_id" value="${escapeAttribute(accountId || '')}">
          <label class="ctox-compact-field">${escapeHtml(state.t('opportunityName', labels.de.opportunityName))}<input class="ctox-input" name="name" required value="${escapeAttribute(opportunity?.name || '')}"></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('type', labels.de.type))}<select class="ctox-select" name="opportunity_type">${optionList(OPPORTUNITY_TYPE_LABELS, opportunity?.opportunity_type || 'renewal')}</select></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('stage', labels.de.stage))}<select class="ctox-select" name="stage">${optionList(OPPORTUNITY_STAGE_LABELS, opportunity?.stage || 'qualification')}</select></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('amount', labels.de.amount))}<input class="ctox-input" name="amount" inputmode="decimal" value="${escapeAttribute(opportunity?.amount_cents ? String(Number(opportunity.amount_cents) / 100) : '')}"></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('probability', labels.de.probability))}<input class="ctox-input" name="probability" inputmode="numeric" value="${escapeAttribute(opportunity?.probability ?? '')}"></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('closeDate', labels.de.closeDate))}<input class="ctox-input" name="close_date" type="date" value="${escapeAttribute(dateInputValue(opportunity?.close_date_ms))}"></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('owner', labels.de.owner))}<input class="ctox-input" name="owner_id" value="${escapeAttribute(opportunity?.owner_id || '')}"></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('contacts', labels.de.contacts))}
            <select class="ctox-select" name="primary_contact_id">
              <option value=""></option>
              ${contacts.map((contact) => `<option value="${escapeAttribute(contact.id)}"${contact.id === opportunity?.primary_contact_id ? ' selected' : ''}>${escapeHtml(contactDisplayName(contact))}</option>`).join('')}
            </select>
          </label>
          ${formActions(state.formMode === 'opportunity-edit' ? state.t('save', labels.de.save) : state.t('create', labels.de.create))}
        </form>
      </section>
    </div>
  `;
}

function renderTaskForm() {
  const task = state.formMode === 'task-edit'
    ? state.collections.customer_tasks.find((item) => item.id === state.formRecordId)
    : null;
  const context = task ? contextFromLinkedRecord(task) : activeRecordContext();
  const title = state.formMode === 'task-edit'
    ? state.t('editTask', labels.de.editTask)
    : state.t('newTask', labels.de.newTask);
  return `
    <header class="ctox-pane-header ctox-pane-band">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">${escapeHtml(state.t('actionKicker', labels.de.actionKicker))}</span>
          <h2 class="ctox-pane-title">${escapeHtml(title)}</h2>
        </div>
      </div>
    </header>
    <div class="ctox-pane-scroll customers-right-scroll">
      <section class="customers-detail-block">
        <form class="customers-form" data-customers-form data-form-kind="${escapeAttribute(state.formMode)}">
          ${hiddenRecordInputs(context)}
          <label class="ctox-compact-field">${escapeHtml(state.t('tasks', labels.de.tasks))}<input class="ctox-input" name="title" required value="${escapeAttribute(task?.title || '')}"></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('body', labels.de.body))}<textarea class="ctox-textarea" name="body">${escapeHtml(task?.body || '')}</textarea></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('status', labels.de.status))}<select class="ctox-select" name="status">${optionList(TASK_STATUS_LABELS, task?.status || 'open')}</select></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('dueDate', labels.de.dueDate))}<input class="ctox-input" name="due_at" type="date" value="${escapeAttribute(dateInputValue(task?.due_at_ms))}"></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('assignee', labels.de.assignee))}<input class="ctox-input" name="assignee_id" value="${escapeAttribute(task?.assignee_id || '')}"></label>
          ${formActions(state.formMode === 'task-edit' ? state.t('save', labels.de.save) : state.t('create', labels.de.create))}
        </form>
      </section>
    </div>
  `;
}

function renderNoteForm() {
  const note = state.formMode === 'note-edit'
    ? state.collections.customer_notes.find((item) => item.id === state.formRecordId)
    : null;
  const context = note ? contextFromLinkedRecord(note) : activeRecordContext();
  const title = state.formMode === 'note-edit'
    ? state.t('editNote', labels.de.editNote)
    : state.t('newNote', labels.de.newNote);
  return `
    <header class="ctox-pane-header ctox-pane-band">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">${escapeHtml(state.t('actionKicker', labels.de.actionKicker))}</span>
          <h2 class="ctox-pane-title">${escapeHtml(title)}</h2>
        </div>
      </div>
    </header>
    <div class="ctox-pane-scroll customers-right-scroll">
      <section class="customers-detail-block">
        <form class="customers-form" data-customers-form data-form-kind="${escapeAttribute(state.formMode)}">
          ${hiddenRecordInputs(context)}
          <label class="ctox-compact-field">${escapeHtml(state.t('notes', labels.de.notes))}<input class="ctox-input" name="title" value="${escapeAttribute(note?.title || '')}"></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('body', labels.de.body))}<textarea class="ctox-textarea" name="body">${escapeHtml(note?.body || '')}</textarea></label>
          <label class="ctox-compact-field">${escapeHtml(state.t('format', labels.de.format))}<select class="ctox-select" name="body_format">${optionList({ markdown: 'Markdown', plain: 'Plain text' }, note?.body_format || 'markdown')}</select></label>
          ${formActions(state.formMode === 'note-edit' ? state.t('save', labels.de.save) : state.t('create', labels.de.create))}
        </form>
      </section>
    </div>
  `;
}

function hiddenRecordInputs(context) {
  return `
    <input type="hidden" name="account_id" value="${escapeAttribute(context?.account?.id || '')}">
    <input type="hidden" name="contact_id" value="${escapeAttribute(context?.type === 'contact' ? context.id : '')}">
    <input type="hidden" name="opportunity_id" value="${escapeAttribute(context?.type === 'opportunity' ? context.id : '')}">
  `;
}

function optionList(options, selected, excluded = []) {
  return Object.entries(options)
    .filter(([key]) => !excluded.includes(key))
    .map(([key, label]) => `<option value="${escapeAttribute(key)}"${key === selected ? ' selected' : ''}>${escapeHtml(label)}</option>`)
    .join('');
}

function filterOption(value, label, selected) {
  return `<option value="${escapeAttribute(value)}"${value === selected ? ' selected' : ''}>${escapeHtml(label)}</option>`;
}

function formActions(primaryLabel) {
  return `
    <div class="customers-form-actions">
      <button class="ctox-button" type="button" data-customers-action="cancel-form">${escapeHtml(state.t('cancel', labels.de.cancel))}</button>
      <button class="ctox-button is-primary" type="submit"${mutableDisabledAttr()}>${escapeHtml(primaryLabel)}</button>
    </div>
    ${state.diagnostics.commandState ? `<div class="customers-command-state">${escapeHtml(state.diagnostics.commandState)}</div>` : ''}
  `;
}

async function submitForm(form) {
  if (!canMutateCustomers()) {
    return showFormError(state.t('permissionDenied', labels.de.permissionDenied));
  }
  const kind = form.getAttribute('data-form-kind') || state.formMode;
  const draft = Object.fromEntries(new FormData(form).entries());
  let command;
  if (kind === 'account-create') {
    const validation = validateAccountDraft(draft);
    if (!validation.valid) return showFormError(validation.error);
    command = buildCreateAccountCommand(draft);
  } else if (kind === 'account-edit') {
    const validation = validateAccountDraft(draft);
    if (!validation.valid) return showFormError(validation.error);
    command = buildAccountUpdateCommand(state.selectedAccountId, draft);
  } else if (kind === 'contact-create') {
    const validation = validateContactDraft(draft);
    if (!validation.valid) return showFormError(validation.error);
    command = buildContactCreateCommand(draft);
  } else if (kind === 'contact-edit') {
    const validation = validateContactDraft(draft);
    if (!validation.valid) return showFormError(validation.error);
    command = buildContactUpdateCommand(state.formRecordId, draft);
  } else if (kind === 'opportunity-create') {
    const validation = validateOpportunityDraft(draft);
    if (!validation.valid) return showFormError(validation.error);
    command = buildOpportunityCreateCommand(draft);
  } else if (kind === 'opportunity-edit') {
    const validation = validateOpportunityDraft(draft);
    if (!validation.valid) return showFormError(validation.error);
    command = buildOpportunityUpdateCommand(state.formRecordId, draft);
  } else if (kind === 'task-create') {
    const validation = validateTaskDraft(draft);
    if (!validation.valid) return showFormError(validation.error);
    command = buildTaskCreateCommand(draft);
  } else if (kind === 'task-edit') {
    const validation = validateTaskDraft(draft);
    if (!validation.valid) return showFormError(validation.error);
    command = buildTaskUpdateCommand(state.formRecordId, draft);
  } else if (kind === 'note-create') {
    const validation = validateNoteDraft(draft);
    if (!validation.valid) return showFormError(validation.error);
    command = buildNoteCreateCommand(draft);
  } else if (kind === 'note-edit') {
    const validation = validateNoteDraft(draft);
    if (!validation.valid) return showFormError(validation.error);
    command = buildNoteUpdateCommand(state.formRecordId, draft);
  }
  await dispatchCommand(command);
  state.formMode = '';
  state.formRecordId = '';
  await refreshData();
}

function showFormError(error) {
  state.diagnostics.commandState = error;
  renderRight();
}

async function dispatchCommand(command) {
  if (!canMutateCustomers()) {
    throw new Error(state.t('permissionDenied', labels.de.permissionDenied));
  }
  if (!state.ctx?.commandBus?.dispatch) {
    throw new Error(state.t('commandUnavailable', labels.de.commandUnavailable));
  }
  await state.ctx.sync?.startCollection?.('business_commands');
  const result = await state.ctx.commandBus.dispatch(command);
  state.diagnostics.commandState = state.t(
    'commandPending',
    labels.de.commandPending,
    result?.command_id || command.id || command.type
  );
  return result;
}

async function openCustomerImporter() {
  await openUniversalImporter(state.ctx, {
    side: 'left',
    moduleId: 'customers',
    entityType: 'customer_account',
    commandType: 'customers.import.file',
    title: state.t('importCustomers', labels.de.importCustomers),
    kicker: 'Kunden Import',
    defaultTitle: `Kunden Import ${new Date().toLocaleDateString(state.lang === 'en' ? 'en-US' : 'de-DE')}`,
    helperText: 'CSV, Excel oder Text mit Firmenname, Domain, Ort und Branche importieren. Dubletten werden als Review markiert.',
    submitLabel: state.t('importCustomers', labels.de.importCustomers),
    submittingLabel: 'Import wird vorbereitet...',
    doneLabel: 'Import abgeschlossen.',
    closeOnSubmit: true,
    dispatch: false,
    definition: {
      target_collection: 'customer_accounts',
      duplicate_policy: 'domain_or_name_review',
      accepted_columns: ['name', 'company', 'firma', 'domain', 'website', 'industry', 'branche', 'city', 'stadt', 'country', 'land'],
    },
    onImport: async ({ payload }) => {
      const result = await importCustomerRowsFromPayload(payload);
      await refreshData();
      return result;
    },
  });
}

async function importCustomerRowsFromPayload(payload = {}) {
  const importCollections = requireCustomerImportCollections();
  const textParts = [];
  if (payload.source?.text) textParts.push(payload.source.text);
  for (const file of payload.source?.files || []) {
    const text = file.text || decodeBase64Utf8(file.base64);
    if (text) textParts.push(text);
  }
  const rows = textParts.flatMap((text) => extractCompanyRowsFromText(text));
  const now = Date.now();
  const batchId = `customer_import_${now}_${crypto.randomUUID()}`;
  const existingByDomain = new Map((state.collections.customer_accounts || [])
    .filter((account) => account.domain)
    .map((account) => [normalizeDomain(account.domain), account]));
  let imported = 0;
  let dedupe = 0;
  let skipped = 0;
  for (const row of rows) {
    const domain = normalizeDomain(row.domain || row.website || '');
    if (!row.name && !domain) {
      skipped += 1;
      continue;
    }
    const existing = domain ? existingByDomain.get(domain) : null;
    const accountId = `customer_import_${slugId(domain || row.name)}_${row.row_index || imported}_${now}`;
    if (existing) {
      dedupe += 1;
      await upsertLocalDoc(importCollections.customer_dedupe_candidates, `customer_dedupe_${batchId}_${existing.id}_${row.row_index || dedupe}`, {
        id: `customer_dedupe_${batchId}_${existing.id}_${row.row_index || dedupe}`,
        object_type: 'account',
        match_key: domain || row.name,
        match_type: domain ? 'domain' : 'name',
        source_record_id: accountId,
        existing_record_id: existing.id,
        import_batch_id: batchId,
        status: 'open',
        confidence: domain ? 0.98 : 0.72,
        payload: { imported_row: row, seed: false },
        is_deleted: false,
        created_at_ms: now,
        updated_at_ms: now,
      });
      continue;
    }
    const account = {
      id: accountId,
      name: row.name || domain,
      domain,
      website_url: row.website || (domain ? `https://${domain}` : ''),
      industry: row.raw?.industry || row.raw?.branche || '',
      city: row.city || '',
      country: row.country || '',
      account_status: 'active',
      customer_stage: 'prospect',
      health_status: 'neutral',
      currency: 'EUR',
      annual_recurring_revenue_cents: 0,
      search_text: [row.name, domain, row.city, row.country].filter(Boolean).join(' ').toLowerCase(),
      source: 'customers_import',
      payload: { import_batch_id: batchId, imported_row: row },
      is_deleted: false,
      created_at_ms: now,
      updated_at_ms: now,
      last_activity_at_ms: now,
    };
    await upsertLocalDoc(importCollections.customer_accounts, account.id, account);
    if (domain) existingByDomain.set(domain, account);
    imported += 1;
  }
  await upsertLocalDoc(importCollections.customer_import_batches, batchId, {
    id: batchId,
    source: payload.source_type || 'importer',
    source_record_id: payload.record_id || '',
    source_filename: payload.source?.files?.map((file) => file.name).filter(Boolean).join(', ') || '',
    status: 'completed',
    object_type: 'account',
    imported_count: imported,
    skipped_count: skipped,
    failed_count: 0,
    dedupe_count: dedupe,
    payload,
    is_deleted: false,
    created_at_ms: now,
    updated_at_ms: now,
  });
  return {
    status: 'completed',
    message: `${imported} Kunden importiert${dedupe ? `, ${dedupe} Dubletten im Review` : ''}.`,
    dispatch: false,
  };
}

function exportCurrentView() {
  const fileBase = `kunden-${state.centerView}-${new Date().toISOString().slice(0, 10)}`;
  const rows = state.centerView === 'contacts'
    ? visibleContactsForState().map((contact) => ({
      name: contactDisplayName(contact),
      account: accountById(contact.account_id)?.name || '',
      email: contact.email || '',
      phone: contact.phone || '',
      role: contact.job_title || '',
      primary: contact.is_primary_contact ? 'true' : 'false',
    }))
    : state.centerView === 'opportunities'
      ? visibleOpportunitiesForState().map((opportunity) => ({
        name: opportunity.name || '',
        account: accountById(opportunity.account_id)?.name || '',
        stage: opportunity.stage || '',
        type: opportunity.opportunity_type || '',
        amount_eur: Number(opportunity.amount_cents || 0) / 100,
        close_date: dateInputValue(opportunity.close_date_ms),
        probability: opportunity.probability ?? '',
      }))
      : visibleAccountsForState().map((account) => ({
        name: account.name || '',
        domain: account.domain || account.website_url || '',
        stage: account.customer_stage || '',
        health: account.health_status || '',
        industry: account.industry || '',
        owner: account.account_owner_id || '',
        arr_eur: Number(account.annual_recurring_revenue_cents || 0) / 100,
        last_activity: dateInputValue(account.last_activity_at_ms || account.updated_at_ms),
      }));
  downloadTextFile(`${fileBase}.csv`, rowsToCsv(rows), 'text/csv;charset=utf-8');
}

async function upsertLocalDoc(collection, id, doc) {
  if (!collection?.findOne) throw new Error('Kundendaten sind gerade nicht verfügbar.');
  const existing = await collection.findOne(id).exec();
  if (existing) {
    await existing.incrementalPatch({ ...doc, created_at_ms: existing.created_at_ms || doc.created_at_ms });
    return;
  }
  await collection.insert(doc);
}

function buildCreateAccountCommand(draft) {
  return {
    module: 'customers',
    command_type: 'customers.account.create',
    payload: compactPayload(accountPayloadFromDraft(draft)),
    client_context: { build: BUILD, surface: 'customers.account.create' },
  };
}

function buildAccountUpdateCommand(accountId, draft) {
  return {
    module: 'customers',
    command_type: 'customers.account.update',
    record_id: accountId,
    payload: { account_id: accountId, ...compactPayload(accountPayloadFromDraft(draft)) },
    client_context: { build: BUILD, surface: 'customers.account.update' },
  };
}

function buildAccountArchiveCommand(accountId) {
  return {
    module: 'customers',
    command_type: 'customers.account.archive',
    record_id: accountId,
    payload: { account_id: accountId },
    client_context: { build: BUILD, surface: 'customers.account.archive' },
  };
}

function buildContactCreateCommand(draft) {
  return {
    module: 'customers',
    command_type: 'customers.contact.create',
    payload: compactPayload(contactPayloadFromDraft(draft)),
    client_context: { build: BUILD, surface: 'customers.contact.create' },
  };
}

function buildContactUpdateCommand(contactId, draft) {
  return {
    module: 'customers',
    command_type: 'customers.contact.update',
    record_id: contactId,
    payload: { contact_id: contactId, ...compactPayload(contactPayloadFromDraft(draft)) },
    client_context: { build: BUILD, surface: 'customers.contact.update' },
  };
}

function buildContactArchiveCommand(contactId) {
  return {
    module: 'customers',
    command_type: 'customers.contact.archive',
    record_id: contactId,
    payload: { contact_id: contactId },
    client_context: { build: BUILD, surface: 'customers.contact.archive' },
  };
}

function buildOpportunityCreateCommand(draft) {
  return {
    module: 'customers',
    command_type: 'customers.opportunity.create',
    payload: compactPayload(opportunityPayloadFromDraft(draft)),
    client_context: { build: BUILD, surface: 'customers.opportunity.create' },
  };
}

function buildOpportunityUpdateCommand(opportunityId, draft) {
  return {
    module: 'customers',
    command_type: 'customers.opportunity.update',
    record_id: opportunityId,
    payload: { opportunity_id: opportunityId, ...compactPayload(opportunityPayloadFromDraft(draft)) },
    client_context: { build: BUILD, surface: 'customers.opportunity.update' },
  };
}

function buildOpportunityMoveStageCommand(opportunityId, stage) {
  return {
    module: 'customers',
    command_type: 'customers.opportunity.move_stage',
    record_id: opportunityId,
    payload: { opportunity_id: opportunityId, stage },
    client_context: { build: BUILD, surface: 'customers.opportunity.move_stage' },
  };
}

function buildOpportunityCloseCommand(opportunityId, outcome) {
  const payload = outcome === 'lost'
    ? { opportunity_id: opportunityId, lost_reason: 'Closed lost' }
    : { opportunity_id: opportunityId };
  return {
    module: 'customers',
    command_type: outcome === 'lost' ? 'customers.opportunity.close_lost' : 'customers.opportunity.close_won',
    record_id: opportunityId,
    payload,
    client_context: { build: BUILD, surface: `customers.opportunity.close_${outcome}` },
  };
}

function buildTaskCreateCommand(draft) {
  return {
    module: 'customers',
    command_type: 'customers.task.create',
    payload: compactPayload(taskPayloadFromDraft(draft)),
    client_context: { build: BUILD, surface: 'customers.task.create' },
  };
}

function buildTaskUpdateCommand(taskId, draft) {
  return {
    module: 'customers',
    command_type: 'customers.task.update',
    record_id: taskId,
    payload: { task_id: taskId, ...compactPayload(taskPayloadFromDraft(draft)) },
    client_context: { build: BUILD, surface: 'customers.task.update' },
  };
}

function buildTaskCompleteCommand(taskId) {
  return {
    module: 'customers',
    command_type: 'customers.task.complete',
    record_id: taskId,
    payload: { task_id: taskId },
    client_context: { build: BUILD, surface: 'customers.task.complete' },
  };
}

function buildNoteCreateCommand(draft) {
  return {
    module: 'customers',
    command_type: 'customers.note.create',
    payload: compactPayload(notePayloadFromDraft(draft)),
    client_context: { build: BUILD, surface: 'customers.note.create' },
  };
}

function buildNoteUpdateCommand(noteId, draft) {
  return {
    module: 'customers',
    command_type: 'customers.note.update',
    record_id: noteId,
    payload: { note_id: noteId, ...compactPayload(notePayloadFromDraft(draft)) },
    client_context: { build: BUILD, surface: 'customers.note.update' },
  };
}

function buildImportFromOutboundCommand(row) {
  const company = row?.company || {};
  return {
    module: 'customers',
    command_type: 'customers.import.from_outbound',
    record_id: company.id,
    payload: compactPayload({
      source_record_id: company.id,
      outbound_company_id: company.id,
      pipeline_id: row?.pipeline?.id,
      source: 'business-os-customers-ui',
    }),
    client_context: { build: BUILD, surface: 'customers.import.from_outbound' },
  };
}

function buildDedupeResolveCommand(candidateId, decision) {
  return {
    module: 'customers',
    command_type: 'customers.dedupe.resolve',
    record_id: candidateId,
    payload: { candidate_id: candidateId, decision },
    client_context: { build: BUILD, surface: 'customers.dedupe.resolve' },
  };
}

function openLinkedApp(moduleId, href) {
  const hash = href || buildCrossAppHref(moduleId, {});
  const normalized = String(hash || '').startsWith('#') ? String(hash) : `#${String(hash || moduleId || '')}`;
  const tab = document.querySelector(`[data-module="${cssEscape(moduleId)}"]`);
  if (tab && !normalized.includes('?')) {
    tab.click();
    return;
  }
  location.hash = normalized.slice(1);
}

function buildSaveViewCommand({ stage, health, search, sort }) {
  const name = ['Kunden', stage !== 'all' ? labelFor(STAGE_LABELS, stage) : '', health !== 'all' ? labelFor(HEALTH_LABELS, health) : '', search ? `"${search}"` : '']
    .filter(Boolean)
    .join(' · ');
  const filters = [
    stage !== 'all' ? { field_name: 'customer_stage', operator: 'eq', value: stage } : null,
    health !== 'all' ? { field_name: 'health_status', operator: 'eq', value: health } : null,
    search ? { field_name: 'search_text', operator: 'contains', value: search } : null,
  ].filter(Boolean);
  const sorts = sort?.field ? [{ field_name: sort.field, direction: sort.direction || 'asc' }] : [];
  return {
    module: 'customers',
    command_type: 'customers.view.save',
    payload: {
      view: {
        name,
        object_type: 'account',
        view_type: 'table',
        visibility: 'private',
        visible_columns: ['name', 'customer_stage', 'health_status', 'domain', 'annual_recurring_revenue_cents', 'last_activity_at_ms'],
      },
      filters,
      sorts,
    },
    client_context: { build: BUILD, surface: 'customers.view.save' },
  };
}

function accountPayloadFromDraft(draft) {
  return {
    name: cleanString(draft.name),
    domain: normalizeDomain(draft.domain),
    industry: cleanString(draft.industry),
    account_owner_id: cleanString(draft.account_owner_id),
    annual_recurring_revenue_cents: moneyToCents(draft.annual_recurring_revenue),
    account_status: cleanString(draft.account_status) || 'active',
    customer_stage: cleanString(draft.customer_stage) || 'active',
    health_status: cleanString(draft.health_status) || 'unknown',
    next_action_at_ms: dateInputToMs(draft.next_action_at),
    source: 'business-os-customers-ui',
  };
}

function contactPayloadFromDraft(draft) {
  return {
    account_id: cleanString(draft.account_id),
    first_name: cleanString(draft.first_name),
    last_name: cleanString(draft.last_name),
    email: cleanString(draft.email),
    phone: cleanString(draft.phone),
    job_title: cleanString(draft.job_title),
    city: cleanString(draft.city),
    is_primary_contact: draft.is_primary_contact === 'true' || draft.is_primary_contact === true,
    source: 'business-os-customers-ui',
  };
}

function opportunityPayloadFromDraft(draft) {
  return {
    account_id: cleanString(draft.account_id),
    name: cleanString(draft.name),
    primary_contact_id: cleanString(draft.primary_contact_id),
    owner_id: cleanString(draft.owner_id),
    opportunity_type: cleanString(draft.opportunity_type) || 'renewal',
    stage: cleanString(draft.stage) || 'qualification',
    amount_cents: moneyToCents(draft.amount),
    currency: 'EUR',
    close_date_ms: dateInputToMs(draft.close_date),
    probability: boundedInteger(draft.probability, 0, 100),
    source: 'business-os-customers-ui',
  };
}

function taskPayloadFromDraft(draft) {
  return {
    title: cleanString(draft.title),
    body: cleanString(draft.body),
    status: cleanString(draft.status) || 'open',
    due_at_ms: dateInputToMs(draft.due_at),
    assignee_id: cleanString(draft.assignee_id),
    account_id: cleanString(draft.account_id),
    contact_id: cleanString(draft.contact_id),
    opportunity_id: cleanString(draft.opportunity_id),
    source: 'business-os-customers-ui',
  };
}

function notePayloadFromDraft(draft) {
  return {
    title: cleanString(draft.title),
    body: cleanString(draft.body),
    body_format: cleanString(draft.body_format) || 'markdown',
    account_id: cleanString(draft.account_id),
    contact_id: cleanString(draft.contact_id),
    opportunity_id: cleanString(draft.opportunity_id),
    source: 'business-os-customers-ui',
  };
}

function validateAccountDraft(draft) {
  if (!cleanString(draft?.name)) return { valid: false, error: labels.de.requiredName };
  return { valid: true, error: '' };
}

function validateContactDraft(draft) {
  if (!cleanString(draft?.account_id)) return { valid: false, error: labels.de.requiredAccount };
  const hasName = cleanString(draft?.first_name) || cleanString(draft?.last_name);
  if (!hasName && !cleanString(draft?.email)) return { valid: false, error: labels.de.requiredContact };
  return { valid: true, error: '' };
}

function validateOpportunityDraft(draft) {
  if (!cleanString(draft?.account_id)) return { valid: false, error: labels.de.requiredAccount };
  if (!cleanString(draft?.name)) return { valid: false, error: labels.de.requiredOpportunity };
  return { valid: true, error: '' };
}

function validateTaskDraft(draft) {
  if (!cleanString(draft?.title)) return { valid: false, error: labels.de.requiredTask };
  if (!cleanString(draft?.account_id) && !cleanString(draft?.contact_id) && !cleanString(draft?.opportunity_id)) {
    return { valid: false, error: labels.de.requiredAccount };
  }
  return { valid: true, error: '' };
}

function validateNoteDraft(draft) {
  if (!cleanString(draft?.title) && !cleanString(draft?.body)) return { valid: false, error: labels.de.requiredNote };
  if (!cleanString(draft?.account_id) && !cleanString(draft?.contact_id) && !cleanString(draft?.opportunity_id)) {
    return { valid: false, error: labels.de.requiredAccount };
  }
  return { valid: true, error: '' };
}

function compactPayload(payload) {
  const next = {};
  for (const [key, value] of Object.entries(payload || {})) {
    if (value === '' || value === null || value === undefined) continue;
    next[key] = value;
  }
  return next;
}

function visibleAccountsForState() {
  return filterAndSortAccounts(state.collections.customer_accounts || [], {
    search: state.search,
    stage: state.stage,
    health: state.health,
    sort: state.accountSort,
  });
}

function visibleContactsForState() {
  return filterAndSortContacts(state.collections.customer_contacts || [], state.collections.customer_accounts || [], {
    search: state.contactSearch,
    accountId: state.stage === 'all' && state.health === 'all' ? '' : state.selectedAccountId,
    sort: state.contactSort,
  });
}

function visibleOpportunitiesForState() {
  return filterAndSortOpportunities(state.collections.customer_opportunities || [], state.collections.customer_accounts || [], {
    search: state.opportunitySearch,
    accountId: state.stage === 'all' && state.health === 'all' ? '' : state.selectedAccountId,
    preset: state.opportunityPreset,
    sort: state.opportunitySort,
  });
}

function visibleOutboundHandoffRowsForState() {
  return filterOutboundHandoffRows(buildOutboundHandoffRows(state.collections), {
    search: state.outboundSearch,
  });
}

function visibleDedupeCandidatesForState() {
  return filterDedupeCandidates(state.collections.customer_dedupe_candidates || [], state.collections.customer_accounts || [], {
    search: state.dedupeSearch,
    status: state.dedupeStatus,
  });
}

function filterAndSortAccounts(accounts, options = {}) {
  const query = normalizeSearch(options.search);
  const stage = options.stage || 'all';
  const health = options.health || 'all';
  return sortRows(
    accounts
      .filter((account) => stage === 'all' || account.customer_stage === stage)
      .filter((account) => health === 'all' || account.health_status === health)
      .filter((account) => {
        if (!query) return true;
        return normalizeSearch([
          account.name,
          account.domain,
          account.website_url,
          account.industry,
          account.account_owner_id,
          account.search_text,
        ].filter(Boolean).join(' ')).includes(query);
      }),
    options.sort || { field: 'updated_at_ms', direction: 'desc' },
  );
}

function filterAndSortContacts(contacts, accounts, options = {}) {
  const query = normalizeSearch(options.search);
  const accountNames = new Map(accounts.map((account) => [account.id, account.name || '']));
  return sortRows(
    contacts
      .filter((contact) => !options.accountId || contact.account_id === options.accountId)
      .filter((contact) => {
        if (!query) return true;
        return normalizeSearch([
          contact.first_name,
          contact.last_name,
          contact.email,
          contact.phone,
          contact.job_title,
          contact.city,
          accountNames.get(contact.account_id),
          contact.search_text,
        ].filter(Boolean).join(' ')).includes(query);
      })
      .map((contact) => ({ ...contact, name: contactDisplayName(contact), account_name: accountNames.get(contact.account_id) || '' })),
    options.sort || { field: 'updated_at_ms', direction: 'desc' },
  );
}

function filterAndSortOpportunities(opportunities, accounts, options = {}) {
  const query = normalizeSearch(options.search);
  const accountNames = new Map(accounts.map((account) => [account.id, account.name || '']));
  return sortRows(
    opportunities
      .filter((opportunity) => !options.accountId || opportunity.account_id === options.accountId)
      .filter((opportunity) => opportunityMatchesPreset(opportunity, options.preset || 'all'))
      .filter((opportunity) => {
        if (!query) return true;
        return normalizeSearch([
          opportunity.name,
          opportunity.stage,
          opportunity.opportunity_type,
          opportunity.owner_id,
          accountNames.get(opportunity.account_id),
          opportunity.search_text,
        ].filter(Boolean).join(' ')).includes(query);
      })
      .map((opportunity) => ({ ...opportunity, account_name: accountNames.get(opportunity.account_id) || '' })),
    options.sort || { field: 'updated_at_ms', direction: 'desc' },
  );
}

function buildOutboundHandoffRows(collections) {
  const companies = collections.outbound_companies || [];
  const pipelines = collections.outbound_pipeline_items || [];
  const accounts = collections.customer_accounts || [];
  const batches = collections.customer_import_batches || [];
  const candidates = collections.customer_dedupe_candidates || [];
  const pipelineByCompany = new Map(pipelines.map((pipeline) => [pipeline.company_id, pipeline]));
  const importedSourceIds = new Set(accounts
    .filter((account) => account.source === 'outbound' && account.source_record_id)
    .map((account) => account.source_record_id));
  const batchStatusBySource = new Map(batches.map((batch) => [batch.source_record_id, batch.status || 'completed']));
  const openDedupeBySource = new Set(candidates
    .filter((candidate) => candidate.status === 'open' && candidate.source_record_id)
    .map((candidate) => candidate.source_record_id));
  return companies
    .filter((company) => company.qualification_status === 'qualified' || company.pipeline_status === 'pipeline' || pipelineByCompany.has(company.id))
    .map((company) => {
      const pipeline = pipelineByCompany.get(company.id) || null;
      const domain = normalizeDomain(company.domain || company.website || '');
      const status = openDedupeBySource.has(company.id)
        ? 'needs_review'
        : importedSourceIds.has(company.id)
          ? 'imported'
          : batchStatusBySource.get(company.id) === 'needs_review'
            ? 'needs_review'
            : batchStatusBySource.has(company.id)
              ? 'imported'
              : 'ready';
      return { company, pipeline, domain, status };
    })
    .sort((a, b) => {
      const statusRank = { ready: 0, needs_review: 1, imported: 2 };
      const byStatus = (statusRank[a.status] ?? 9) - (statusRank[b.status] ?? 9);
      if (byStatus) return byStatus;
      return Number(b.company.fit_score || 0) - Number(a.company.fit_score || 0)
        || String(a.company.name || '').localeCompare(String(b.company.name || ''));
    });
}

function filterOutboundHandoffRows(rows, options = {}) {
  const query = normalizeSearch(options.search);
  return rows.filter((row) => {
    if (!query) return true;
    return normalizeSearch([
      row.company.name,
      row.domain,
      row.company.city,
      row.company.country,
      row.company.qualification_status,
      row.pipeline?.stage,
      row.pipeline?.outreach_status,
    ].filter(Boolean).join(' ')).includes(query);
  });
}

function filterDedupeCandidates(candidates, accounts, options = {}) {
  const query = normalizeSearch(options.search);
  const accountNames = new Map(accounts.map((account) => [account.id, account.name || '']));
  return candidates
    .filter((candidate) => !options.status || options.status === 'all' || candidate.status === options.status)
    .filter((candidate) => {
      if (!query) return true;
      return normalizeSearch([
        candidate.match_key,
        candidate.match_type,
        candidate.object_type,
        candidate.source_record_id,
        candidate.existing_record_id,
        accountNames.get(candidate.existing_record_id),
        candidate.decision,
      ].filter(Boolean).join(' ')).includes(query);
    })
    .sort((a, b) => Number(b.confidence || 0) - Number(a.confidence || 0)
      || Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0));
}

function opportunityMatchesPreset(opportunity, preset) {
  if (preset === 'renewals') return opportunity.opportunity_type === 'renewal';
  if (preset === 'closed') return ['closed_won', 'closed_lost'].includes(opportunity.stage);
  if (preset === 'closing_month') {
    const close = Number(opportunity.close_date_ms || 0);
    if (!close) return false;
    const date = new Date(close);
    const now = new Date();
    return date.getUTCFullYear() === now.getUTCFullYear() && date.getUTCMonth() === now.getUTCMonth();
  }
  return true;
}

function countClosingThisMonth(opportunities = []) {
  return opportunities.filter((opportunity) => opportunityMatchesPreset(opportunity, 'closing_month')).length;
}

function groupOpportunitiesByStage(opportunities) {
  const groups = Object.fromEntries(Object.keys(OPPORTUNITY_STAGE_LABELS).map((stage) => [stage, []]));
  for (const opportunity of opportunities) {
    const stage = opportunity.stage && groups[opportunity.stage] ? opportunity.stage : 'qualification';
    groups[stage].push(opportunity);
  }
  return groups;
}

function summarizeOpportunityPipeline(opportunities) {
  return opportunities.reduce((acc, opportunity) => {
    const amount = Number(opportunity.amount_cents || 0);
    const probability = Number.isFinite(Number(opportunity.probability))
      ? Number(opportunity.probability)
      : defaultProbabilityForStage(opportunity.stage);
    acc.total_cents += amount;
    acc.weighted_cents += Math.round(amount * probability / 100);
    acc.currency = opportunity.currency || acc.currency || 'EUR';
    acc.stageCounts[opportunity.stage || 'qualification'] = (acc.stageCounts[opportunity.stage || 'qualification'] || 0) + 1;
    return acc;
  }, { total_cents: 0, weighted_cents: 0, currency: 'EUR', stageCounts: {} });
}

function defaultProbabilityForStage(stage) {
  return ({
    qualification: 20,
    proposal: 40,
    negotiation: 60,
    committed: 85,
    closed_won: 100,
    closed_lost: 0,
  })[stage] ?? 20;
}

function sortRows(rows, sort) {
  const field = sort?.field || 'updated_at_ms';
  const direction = sort?.direction === 'asc' ? 1 : -1;
  return [...rows].sort((a, b) => {
    const av = comparableValue(a[field]);
    const bv = comparableValue(b[field]);
    if (av < bv) return -1 * direction;
    if (av > bv) return 1 * direction;
    return String(a.id || '').localeCompare(String(b.id || ''));
  });
}

function comparableValue(value) {
  if (typeof value === 'number') return value;
  if (typeof value === 'boolean') return value ? 1 : 0;
  return String(value || '').toLowerCase();
}

function applySavedView(viewId) {
  const view = state.collections.customer_views.find((item) => item.id === viewId);
  if (!view) return;
  const filters = state.collections.customer_view_filters.filter((item) => item.view_id === viewId);
  const sorts = state.collections.customer_view_sorts.filter((item) => item.view_id === viewId).sort((a, b) => Number(a.position || 0) - Number(b.position || 0));
  state.stage = filters.find((item) => item.field_name === 'customer_stage')?.value || 'all';
  state.health = filters.find((item) => item.field_name === 'health_status')?.value || 'all';
  state.search = filters.find((item) => item.field_name === 'search_text')?.value || '';
  if (sorts[0]?.field_name) state.accountSort = { field: sorts[0].field_name, direction: sorts[0].direction || 'asc' };
  syncSelection();
}

function selectedAccount() {
  return (state.collections.customer_accounts || []).find((account) => account.id === state.selectedAccountId) || null;
}

function selectedContact() {
  return (state.collections.customer_contacts || []).find((contact) => contact.id === state.selectedContactId) || null;
}

function selectedOpportunity() {
  return (state.collections.customer_opportunities || []).find((opportunity) => opportunity.id === state.selectedOpportunityId) || null;
}

function accountById(accountId) {
  return (state.collections.customer_accounts || []).find((account) => account.id === accountId) || null;
}

function outboundCompanyById(companyId) {
  return (state.collections.outbound_companies || []).find((company) => company.id === companyId) || null;
}

function outboundHandoffRowByCompanyId(companyId) {
  return buildOutboundHandoffRows(state.collections).find((row) => row.company.id === companyId) || null;
}

function activeRecordContext() {
  const opportunity = selectedOpportunity();
  if (opportunity) {
    const account = accountById(opportunity.account_id);
    const contact = (state.collections.customer_contacts || []).find((item) => item.id === opportunity.primary_contact_id) || null;
    return {
      type: 'opportunity',
      typeLabel: state.t('opportunityName', labels.de.opportunityName),
      id: opportunity.id,
      title: opportunity.name || opportunity.id,
      subtitle: [account?.name, labelFor(OPPORTUNITY_STAGE_LABELS, opportunity.stage)].filter(Boolean).join(' · '),
      account,
      contact,
      opportunity,
    };
  }
  const contact = selectedContact();
  if (contact) {
    const account = accountById(contact.account_id);
    return {
      type: 'contact',
      typeLabel: state.t('contacts', labels.de.contacts),
      id: contact.id,
      title: contactDisplayName(contact),
      subtitle: [contact.job_title, account?.name].filter(Boolean).join(' · '),
      account,
      contact,
      opportunity: null,
    };
  }
  const account = selectedAccount();
  if (!account) return null;
  return {
    type: 'account',
    typeLabel: state.t('accounts', labels.de.accounts),
    id: account.id,
    title: account.name || account.id,
    subtitle: [account.domain, account.industry].filter(Boolean).join(' · ') || account.website_url || '',
    account,
    contact: null,
    opportunity: null,
  };
}

function contextFromLinkedRecord(record) {
  const opportunity = record?.opportunity_id
    ? (state.collections.customer_opportunities || []).find((item) => item.id === record.opportunity_id)
    : null;
  if (opportunity) {
    const account = accountById(opportunity.account_id || record.account_id);
    return {
      type: 'opportunity',
      id: opportunity.id,
      account,
      contact: null,
      opportunity,
      title: opportunity.name || opportunity.id,
    };
  }
  const contact = record?.contact_id
    ? (state.collections.customer_contacts || []).find((item) => item.id === record.contact_id)
    : null;
  if (contact) {
    const account = accountById(contact.account_id || record.account_id);
    return {
      type: 'contact',
      id: contact.id,
      account,
      contact,
      opportunity: null,
      title: contactDisplayName(contact),
    };
  }
  const account = accountById(record?.account_id) || selectedAccount();
  if (!account) return activeRecordContext();
  return {
    type: 'account',
    id: account.id,
    account,
    contact: null,
    opportunity: null,
    title: account.name || account.id,
  };
}

function relatedRecords(accountId, collections) {
  const contacts = (collections.customer_contacts || []).filter((contact) => contact.account_id === accountId);
  const opportunities = (collections.customer_opportunities || []).filter((item) => item.account_id === accountId);
  const tasks = (collections.customer_tasks || []).filter((task) => task.account_id === accountId);
  const activities = (collections.customer_activities || [])
    .filter((activity) => activity.account_id === accountId)
    .sort((a, b) => Number(b.happens_at_ms || b.updated_at_ms || 0) - Number(a.happens_at_ms || a.updated_at_ms || 0));
  return {
    contacts,
    opportunities,
    tasks,
    openTasks: tasks.filter((task) => !['completed', 'cancelled'].includes(task.status)),
    activities,
  };
}

function filterRelatedTasks(context, collections) {
  return filterRelatedRecords(context, collections, collections.customer_tasks || [])
    .sort((a, b) => taskSortValue(a) - taskSortValue(b));
}

function filterRelatedNotes(context, collections) {
  return filterRelatedRecords(context, collections, collections.customer_notes || [])
    .sort((a, b) => Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0));
}

function filterRelatedActivities(context, collections) {
  return filterRelatedRecords(context, collections, collections.customer_activities || [])
    .sort((a, b) => Number(b.happens_at_ms || b.updated_at_ms || 0) - Number(a.happens_at_ms || a.updated_at_ms || 0));
}

function filterRelatedFiles(context, collections) {
  return filterRelatedRecords(context, collections, collections.customer_files || [])
    .sort((a, b) => Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0));
}

function buildLinkedAppRows(context, collections) {
  if (!context?.id) return [];
  const params = recordLinkParams(context);
  const conversations = relatedCommunicationMessages(context, collections);
  const calendar = relatedCalendarEvents(context, collections);
  const documents = relatedDocuments(context, collections);
  const notes = relatedExternalNotes(context, collections);
  const spreadsheets = relatedSpreadsheets(context, collections);
  const outbound = relatedOutboundRows(context, collections);
  return [
    linkedAppSummary('conversations', state.t('conversations', labels.de.conversations), conversations, params, summarizeMessagePreview(conversations[0])),
    linkedAppSummary('calendar', state.t('calendar', labels.de.calendar), calendar, params, calendar[0]?.title || ''),
    linkedAppSummary('documents', state.t('documents', labels.de.documents), documents, params, documents[0]?.title || documents[0]?.filename || ''),
    linkedAppSummary('notes', state.t('notes', labels.de.notes), notes, params, notes[0]?.title || ''),
    linkedAppSummary('spreadsheets', state.t('spreadsheets', labels.de.spreadsheets), spreadsheets, params, spreadsheets[0]?.title || spreadsheets[0]?.filename || ''),
    linkedAppSummary('outbound', state.t('outbound', labels.de.outbound), outbound, params, outbound[0]?.company?.name || outbound[0]?.name || ''),
  ];
}

function linkedAppSummary(moduleId, label, records, params, preview = '') {
  const count = records?.length || 0;
  return {
    moduleId,
    label,
    count,
    countLabel: `${count} ${count === 1 ? state.t('linkedRecords', labels.de.linkedRecords) : state.t('linkedRecords', labels.de.linkedRecords)}`,
    status: count ? state.t('available', labels.de.available) : '',
    preview,
    href: buildCrossAppHref(moduleId, params),
  };
}

function recordLinkParams(context) {
  const account = context.account || null;
  const contact = context.contact || null;
  const opportunity = context.opportunity || null;
  const customerName = context.type === 'contact'
    ? contactDisplayName(contact)
    : context.type === 'opportunity'
      ? opportunity?.name
      : account?.name;
  return compactPayload({
    source_module: 'customers',
    customer_type: context.type,
    account_id: account?.id,
    contact_id: contact?.id,
    opportunity_id: opportunity?.id,
    customer_name: customerName || context.title,
    domain: account?.domain || account?.website_url || '',
    email: contact?.email || '',
  });
}

function buildCrossAppHref(moduleId, params = {}) {
  const search = new URLSearchParams();
  Object.entries(params || {}).forEach(([key, value]) => {
    if (value !== '' && value !== null && value !== undefined) search.set(key, String(value));
  });
  const query = search.toString();
  return `#${moduleId}${query ? `?${query}` : ''}`;
}

function relatedCommunicationMessages(context, collections) {
  const needles = recordSearchNeedles(context);
  return (collections.communication_messages || [])
    .filter((message) => textMatchesNeedles([
      message.sender_display,
      message.sender_address,
      ...(message.recipient_addresses_json || []),
      ...(message.cc_addresses_json || []),
      message.subject,
      message.preview,
      message.body_text,
    ], needles))
    .sort((a, b) => Date.parse(b.external_created_at || b.observed_at || 0) - Date.parse(a.external_created_at || a.observed_at || 0));
}

function relatedCalendarEvents(context, collections) {
  const needles = recordSearchNeedles(context);
  return (collections.calendar_events || [])
    .filter((event) => textMatchesNeedles([
      event.title,
      event.description,
      event.location,
      event.meeting_url,
      ...(event.attendees || []).flatMap((attendee) => [attendee.name, attendee.email, attendee.address]),
    ], needles))
    .sort((a, b) => Number(b.start_time || b.updated_at_ms || 0) - Number(a.start_time || a.updated_at_ms || 0));
}

function relatedDocuments(context, collections) {
  const linkedIds = new Set(filterRelatedFiles(context, collections).map((file) => file.document_id).filter(Boolean));
  return (collections.documents || [])
    .filter((doc) => linkedIds.has(doc.id) || linkedRecordsMatchContext(doc.linked_records || [], context))
    .sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0));
}

function relatedExternalNotes(context, collections) {
  const linkedNoteIds = new Set(filterRelatedNotes(context, collections).map((note) => note.linked_note_id).filter(Boolean));
  const needles = recordSearchNeedles(context);
  return (collections.notes || [])
    .filter((note) => linkedNoteIds.has(note.id) || textMatchesNeedles([note.title, note.content, note.tags, note.notebook], needles))
    .sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0));
}

function relatedSpreadsheets(context, collections) {
  return (collections.spreadsheets || [])
    .filter((sheet) => linkedRecordsMatchContext(sheet.linked_records || [], context))
    .sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0));
}

function relatedOutboundRows(context, collections) {
  const account = context.account || {};
  const domain = normalizeDomain(account.domain || account.website_url || '');
  const sourceId = account.source === 'outbound' ? account.source_record_id : '';
  return buildOutboundHandoffRows(collections)
    .filter((row) => row.company.id === sourceId || (domain && row.domain === domain));
}

function linkedRecordsMatchContext(linkedRecords, context) {
  const accountId = context.account?.id || '';
  const contactId = context.contact?.id || '';
  const opportunityId = context.opportunity?.id || '';
  return (linkedRecords || []).some((record) => {
    const type = String(record.type || record.record_type || record.module || '').toLowerCase();
    const id = String(record.id || record.record_id || record.source_record_id || '');
    return (accountId && id === accountId && (!type || type.includes('account') || type.includes('customer')))
      || (contactId && id === contactId && type.includes('contact'))
      || (opportunityId && id === opportunityId && type.includes('opportunity'));
  });
}

function recordSearchNeedles(context) {
  const account = context.account || {};
  const contact = context.contact || {};
  const opportunity = context.opportunity || {};
  return [
    account.domain,
    normalizeDomain(account.website_url),
    contact.email,
    contactDisplayName(contact),
    account.name,
    opportunity.name,
  ].map(normalizeSearch).filter(Boolean);
}

function textMatchesNeedles(values, needles) {
  if (!needles.length) return false;
  const haystack = normalizeSearch((values || []).filter(Boolean).join(' '));
  return needles.some((needle) => needle && haystack.includes(needle));
}

function summarizeMessagePreview(message) {
  if (!message) return '';
  return message.subject || message.preview || message.sender_address || '';
}

function filterRelatedRecords(context, collections, records) {
  if (!context?.id) return [];
  const accountId = context.account?.id || context.id;
  const accountContacts = new Set((collections.customer_contacts || [])
    .filter((contact) => contact.account_id === accountId)
    .map((contact) => contact.id));
  const accountOpportunities = new Set((collections.customer_opportunities || [])
    .filter((opportunity) => opportunity.account_id === accountId)
    .map((opportunity) => opportunity.id));
  return records.filter((record) => {
    if (context.type === 'opportunity') return record.opportunity_id === context.id;
    if (context.type === 'contact') return record.contact_id === context.id;
    return record.account_id === context.id
      || accountContacts.has(record.contact_id)
      || accountOpportunities.has(record.opportunity_id);
  });
}

function buildTimelineRows(context, collections) {
  const activities = filterRelatedActivities(context, collections).map((activity) => ({
    id: activity.id,
    kind: 'activity',
    title: activity.name || activity.activity_type || 'Aktivität',
    body: activity.body || activity.description || '',
    at: Number(activity.happens_at_ms || activity.updated_at_ms || 0),
  }));
  const tasks = filterRelatedTasks(context, collections).map((task) => ({
    id: task.id,
    kind: 'task',
    title: task.title || task.id,
    body: labelFor(TASK_STATUS_LABELS, task.status),
    at: Number(task.completed_at_ms || task.due_at_ms || task.updated_at_ms || task.created_at_ms || 0),
  }));
  const notes = filterRelatedNotes(context, collections).map((note) => ({
    id: note.id,
    kind: 'note',
    title: note.title || 'Notiz',
    body: note.body || '',
    at: Number(note.updated_at_ms || note.created_at_ms || 0),
  }));
  return [...activities, ...tasks, ...notes]
    .filter((row) => Number.isFinite(row.at) && row.at > 0)
    .sort((a, b) => b.at - a.at);
}

function taskSortValue(task) {
  if (['completed', 'cancelled'].includes(task.status)) return Number.MAX_SAFE_INTEGER - Number(task.updated_at_ms || 0);
  return Number(task.due_at_ms || task.position || task.updated_at_ms || Number.MAX_SAFE_INTEGER);
}

function summarizeCustomersData(collections) {
  const accounts = collections.customer_accounts || [];
  const contacts = collections.customer_contacts || [];
  const opportunities = collections.customer_opportunities || [];
  const tasks = collections.customer_tasks || [];
  const activities = collections.customer_activities || [];
  return {
    accounts: accounts.length,
    contacts: contacts.length,
    opportunities: opportunities.length,
    tasks: tasks.length,
    activities: activities.length,
    stageCounts: countBy(accounts, 'customer_stage'),
    healthCounts: countBy(accounts, 'health_status'),
  };
}

function countBy(items, field) {
  return items.reduce((acc, item) => {
    const key = item?.[field] || 'unknown';
    acc[key] = (acc[key] || 0) + 1;
    return acc;
  }, {});
}

function buildSyncRows(diagnostics) {
  const collectionDiagnostics = diagnostics?.collections || {};
  return CUSTOMERS_COLLECTIONS
    .filter((name) => name.startsWith('customer_'))
    .slice(0, 6)
    .map((name) => {
      const status = collectionDiagnostics[name]?.connectionStatus
        || collectionDiagnostics[name]?.status
        || (resolveCollection(name) ? 'local' : 'pending');
      return {
        label: name.replace(/^customer_/, '').replaceAll('_', ' '),
        status,
        tone: collectionDiagnostics[name]?.lastError ? 'error' : (status === 'connected' || status === 'local' ? 'ok' : 'warn'),
      };
    });
}

function renderPermissionNotice() {
  if (canMutateCustomers()) return '';
  return `
    <section class="ctox-callout is-warning customers-permission-notice">
      <strong>${escapeHtml(state.t('permissionReadOnly', labels.de.permissionReadOnly))}</strong>
      <span>${escapeHtml(state.t('permissionReadOnlyBody', labels.de.permissionReadOnlyBody))}</span>
    </section>
  `;
}

function renderCommandAuditPanel(context) {
  const commands = filterCustomerCommands(state.collections.business_commands || [], context);
  const summary = summarizeCustomerCommands(commands);
  return `
    <section class="customers-detail-block">
      <div class="customers-section-head">
        <h3 class="ctox-field-label customers-section-heading">${escapeHtml(state.t('commandAudit', labels.de.commandAudit))}</h3>
        <div class="customers-chip-row" aria-label="${escapeAttribute(state.t('commandAudit', labels.de.commandAudit))}">
          <span class="ctox-badge is-warning">${summary.pending}</span>
          <span class="ctox-badge is-success">${summary.completed}</span>
          <span class="ctox-badge is-danger">${summary.failed}</span>
        </div>
      </div>
      ${commands.length ? `
        <div class="customers-command-list">
          ${commands.slice(0, 5).map(commandAuditRow).join('')}
        </div>
      ` : `
        <div class="ctox-empty">
          <strong>${escapeHtml(state.t('commandAuditEmpty', labels.de.commandAuditEmpty))}</strong>
          <span>${escapeHtml(state.t('commandAuditBody', labels.de.commandAuditBody))}</span>
        </div>
      `}
    </section>
  `;
}

function renderOperationalAuditPanel(context) {
  if (!isCustomersDebugMode()) return '';
  return renderCommandAuditPanel(context);
}

function isCustomersDebugMode() {
  const user = state.ctx?.session?.user || {};
  return Boolean(
    state.ctx?.debug === true
    || new URLSearchParams(location.search).has('customersDebug')
    || localStorage.getItem('ctox.businessOs.customers.debug') === '1'
    || user.is_admin === true && localStorage.getItem('ctox.businessOs.debugPanels') === '1'
  );
}

function commandAuditRow(command) {
  const tone = commandStatusTone(command.status);
  const statusLabel = commandStatusLabel(command.status);
  const error = command?.error || command?.result?.error || command?.payload?.error || '';
  return `
    <article class="customers-command-row ${escapeAttribute(tone)}">
      <div>
        <strong>${escapeHtml(command.command_type || command.type || command.id || 'customers.command')}</strong>
        <span>${escapeHtml([statusLabel, formatDate(command.updated_at_ms || command.created_at_ms, state.lang)].filter(Boolean).join(' · '))}</span>
        ${error ? `<p>${escapeHtml(error)}</p>` : ''}
      </div>
    </article>
  `;
}

function summarizeCustomerCommands(commands = []) {
  return commands.reduce((acc, command) => {
    const tone = commandStatusTone(command?.status);
    acc[tone] = (acc[tone] || 0) + 1;
    return acc;
  }, { pending: 0, completed: 0, failed: 0 });
}

function filterCustomerCommands(commands = [], context = null) {
  return commands
    .filter(isCustomerCommand)
    .filter((command) => commandMatchesContext(command, context))
    .sort((a, b) => Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0));
}

function isCustomerCommand(command) {
  const type = String(command?.command_type || command?.type || '');
  return command?.module === 'customers'
    || type.startsWith('customers.')
    || String(command?.inbound_channel || '').includes('customers')
    || String(command?.client_context?.surface || '').startsWith('customers.');
}

function commandMatchesContext(command, context = null) {
  if (!context?.id) return true;
  const payload = command?.payload || {};
  const values = new Set([
    command?.record_id,
    payload.account_id,
    payload.contact_id,
    payload.opportunity_id,
    payload.task_id,
    payload.note_id,
    payload.candidate_id,
    payload.source_record_id,
    payload.outbound_company_id,
  ].filter(Boolean));
  if (values.has(context.id)) return true;
  if (context.type === 'account') return values.has(context.account?.id);
  if (context.type === 'contact') return values.has(context.contact?.account_id);
  if (context.type === 'opportunity') return values.has(context.opportunity?.account_id);
  return false;
}

function commandStatusTone(status) {
  const normalized = String(status || 'pending_sync').toLowerCase();
  if (['failed', 'error', 'rejected'].includes(normalized)) return 'failed';
  if (['completed', 'complete', 'succeeded', 'success', 'applied', 'synced'].includes(normalized)) return 'completed';
  return 'pending';
}

function commandStatusLabel(status) {
  const tone = commandStatusTone(status);
  if (tone === 'failed') return state.t('commandFailedStatus', labels.de.commandFailedStatus);
  if (tone === 'completed') return state.t('commandCompletedStatus', labels.de.commandCompletedStatus);
  return state.t('commandPendingStatus', labels.de.commandPendingStatus);
}

function metric(label, value) {
  return `
    <dt>${escapeHtml(label)}</dt>
    <dd>${escapeHtml(String(value ?? ''))}</dd>
  `;
}

function opportunityRow(opportunity) {
  const selected = opportunity.id === state.selectedOpportunityId;
  return `
    <div class="customers-mini-row" data-customers-opportunity-id="${escapeAttribute(opportunity.id)}" ${selectableAttrs('opportunity', opportunity.id, selected, opportunity.name || opportunity.id)}>
      <span>${escapeHtml(opportunity.name || opportunity.id)}</span>
      <span>${escapeHtml(labelFor(OPPORTUNITY_STAGE_LABELS, opportunity.stage))}</span>
    </div>
  `;
}

function taskRow(task) {
  const isDone = ['completed', 'cancelled'].includes(task.status);
  return `
    <article class="customers-record-row ${isDone ? 'is-muted' : ''}">
      <div>
        <strong>${escapeHtml(task.title || task.id)}</strong>
        <span>${escapeHtml([labelFor(TASK_STATUS_LABELS, task.status), formatDate(task.due_at_ms, state.lang), task.assignee_id].filter((item) => item && item !== '—').join(' · ') || '—')}</span>
        ${task.body ? `<p>${escapeHtml(task.body)}</p>` : ''}
      </div>
      <div class="customers-row-actions">
        <button class="ctox-icon-button" type="button" data-customers-action="edit-task" data-task-id="${escapeAttribute(task.id)}" aria-label="${escapeAttribute(state.t('editTask', labels.de.editTask))}"${mutableDisabledAttr()}>${actionIcon('edit')}</button>
        ${isDone ? '' : `<button class="ctox-button ctox-button--sm" type="button" data-customers-action="complete-task" data-task-id="${escapeAttribute(task.id)}"${mutableDisabledAttr()}>${escapeHtml(state.t('completeTask', labels.de.completeTask))}</button>`}
      </div>
    </article>
  `;
}

function noteRow(note) {
  return `
    <article class="customers-record-row">
      <div>
        <strong>${escapeHtml(note.title || 'Notiz')}</strong>
        <span>${escapeHtml(formatDate(note.updated_at_ms || note.created_at_ms, state.lang))}</span>
        ${note.body ? `<p>${escapeHtml(note.body)}</p>` : ''}
      </div>
      <button class="ctox-icon-button" type="button" data-customers-action="edit-note" data-note-id="${escapeAttribute(note.id)}" aria-label="${escapeAttribute(state.t('editNote', labels.de.editNote))}"${mutableDisabledAttr()}>${actionIcon('edit')}</button>
    </article>
  `;
}

function fileRow(file) {
  return `
    <div class="customers-mini-row">
      <span>${escapeHtml(file.name || file.filename || file.id)}</span>
      <span>${escapeHtml(file.mime_type || file.file_type || formatDate(file.updated_at_ms, state.lang))}</span>
    </div>
  `;
}

function linkedAppRow(row) {
  return `
    <article class="customers-link-row">
      <div>
        <strong>${escapeHtml(row.label)}</strong>
        <span>${escapeHtml([row.countLabel, row.status, row.preview].filter(Boolean).join(' · '))}</span>
      </div>
      <button class="ctox-button ctox-button--sm" type="button" data-customers-action="open-linked-app" data-link-module="${escapeAttribute(row.moduleId)}" data-link-href="${escapeAttribute(row.href)}">${escapeHtml(state.t('openApp', labels.de.openApp))}</button>
    </article>
  `;
}

function timelineRow(row) {
  return `
    <div class="customers-timeline-item">
      <div>
        <strong>${escapeHtml(row.title)}</strong>
        <span>${escapeHtml([row.kind, formatDate(row.at, state.lang)].filter(Boolean).join(' · '))}</span>
        ${row.body ? `<p>${escapeHtml(row.body)}</p>` : ''}
      </div>
    </div>
  `;
}

function activityRow(activity) {
  return `
    <div class="customers-timeline-item">
      <div>
        <strong>${escapeHtml(activity.name || activity.activity_type || 'Aktivität')}</strong>
        <span>${escapeHtml(formatDate(activity.happens_at_ms || activity.updated_at_ms, state.lang))}</span>
      </div>
    </div>
  `;
}

function emptyMiniRow(label) {
  return `<div class="customers-mini-row"><span>${escapeHtml(label)}</span><span></span></div>`;
}

function labelFor(labelsByKey, key) {
  if (!key) return labelsByKey.unknown || 'Unknown';
  return labelsByKey[key] || key;
}

function formatMoney(cents, currency = 'EUR') {
  if (!Number.isFinite(Number(cents))) return '—';
  try {
    return new Intl.NumberFormat(state.lang === 'en' ? 'en-US' : 'de-DE', {
      style: 'currency',
      currency: currency || 'EUR',
      maximumFractionDigits: 0,
    }).format(Number(cents) / 100);
  } catch {
    return `${Math.round(Number(cents) / 100)} ${currency || 'EUR'}`;
  }
}

function moneyToCents(value) {
  if (value === '' || value === null || value === undefined) return null;
  const normalized = String(value).trim().replace(/\./g, '').replace(',', '.');
  const num = Number(normalized);
  return Number.isFinite(num) ? Math.round(num * 100) : null;
}

function boundedInteger(value, min, max) {
  if (value === '' || value === null || value === undefined) return null;
  const num = Math.round(Number(value));
  if (!Number.isFinite(num)) return null;
  return Math.max(min, Math.min(max, num));
}

function formatDate(value, lang = 'de') {
  const ms = Number(value || 0);
  if (!Number.isFinite(ms) || ms <= 0) return '—';
  try {
    return new Intl.DateTimeFormat(lang === 'en' ? 'en-US' : 'de-DE', {
      day: '2-digit',
      month: '2-digit',
      year: '2-digit',
    }).format(new Date(ms));
  } catch {
    return new Date(ms).toISOString().slice(0, 10);
  }
}

function dateInputValue(value) {
  const ms = Number(value || 0);
  if (!Number.isFinite(ms) || ms <= 0) return '';
  return new Date(ms).toISOString().slice(0, 10);
}

function dateInputToMs(value) {
  if (!value) return null;
  const ms = Date.parse(`${value}T00:00:00.000Z`);
  return Number.isFinite(ms) ? ms : null;
}

function initials(name) {
  const parts = String(name || '').trim().split(/\s+/).filter(Boolean);
  if (!parts.length) return 'K';
  return parts.slice(0, 2).map((part) => part[0]).join('').toUpperCase();
}

function contactDisplayName(contact) {
  return [contact?.first_name, contact?.last_name].filter(Boolean).join(' ') || contact?.email || contact?.id || '';
}

function normalizeDomain(value) {
  let domain = cleanString(value).toLowerCase();
  domain = domain.replace(/^https?:\/\//, '').replace(/^www\./, '').split('/')[0];
  return domain;
}

function normalizeSearch(value) {
  return String(value || '').trim().toLowerCase();
}

function slugId(value) {
  return normalizeSearch(value)
    .replace(/ä/g, 'ae')
    .replace(/ö/g, 'oe')
    .replace(/ü/g, 'ue')
    .replace(/ß/g, 'ss')
    .replace(/[^a-z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .slice(0, 80) || 'record';
}

function cleanString(value) {
  return String(value || '').trim();
}

function rowsToCsv(rows = []) {
  const columns = Array.from(rows.reduce((set, row) => {
    Object.keys(row || {}).forEach((key) => set.add(key));
    return set;
  }, new Set()));
  const encode = (value) => `"${String(value ?? '').replace(/"/g, '""')}"`;
  return [
    columns.map(encode).join(','),
    ...rows.map((row) => columns.map((column) => encode(row?.[column])).join(',')),
  ].join('\n');
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

function emptyCollections() {
  return {
    business_commands: [],
    customer_accounts: [],
    customer_contacts: [],
    customer_opportunities: [],
    customer_tasks: [],
    customer_notes: [],
    customer_activities: [],
    customer_files: [],
    customer_views: [],
    customer_view_filters: [],
    customer_view_sorts: [],
    customer_import_batches: [],
    customer_dedupe_candidates: [],
    outbound_companies: [],
    outbound_pipeline_items: [],
    communication_messages: [],
    calendar_events: [],
    documents: [],
    notes: [],
    spreadsheets: [],
  };
}

function cssEscape(value) {
  if (window.CSS?.escape) return window.CSS.escape(String(value || ''));
  return String(value || '').replace(/["\\]/g, '\\$&');
}

function escapeHtml(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

function escapeAttribute(value) {
  return escapeHtml(value);
}

// Kit action icons: monochrome stroke glyphs delivered by the shell
// (shared/icons.js actionIconPaths) via the module context.
function actionIcon(name) {
  return state.ctx?.getActionIcon?.(name) || '';
}

// Status badge mapping onto the shared .ctox-badge variants. Parity with the
// previous module-local palette: active/healthy -> success, at_risk/critical
// -> danger, in-flight commercial stages -> module accent extra.
function stageBadgeClass(stage) {
  if (stage === 'active') return ' is-success';
  if (['renewal', 'expansion', 'proposal', 'negotiation', 'committed'].includes(stage)) return ' is-info';
  return '';
}

function healthBadgeClass(health) {
  if (health === 'healthy') return ' is-success';
  if (health === 'at_risk' || health === 'critical') return ' is-danger';
  return '';
}

export const __customersTestHooks = {
  buildAccountArchiveCommand,
  buildAccountUpdateCommand,
  buildContactArchiveCommand,
  buildContactCreateCommand,
  buildContactUpdateCommand,
  buildCreateAccountCommand,
  buildOpportunityCloseCommand,
  buildOpportunityCreateCommand,
  buildOpportunityMoveStageCommand,
  buildOpportunityUpdateCommand,
  buildDedupeResolveCommand,
  buildImportFromOutboundCommand,
  buildNoteCreateCommand,
  buildNoteUpdateCommand,
  buildSaveViewCommand,
  buildSyncRows,
  buildCrossAppHref,
  buildLinkedAppRows,
  buildTaskCompleteCommand,
  buildTaskCreateCommand,
  buildTaskUpdateCommand,
  buildTimelineRows,
  buildOutboundHandoffRows,
  canMutateCustomersContext,
  commandMatchesContext,
  commandStatusTone,
  recordLinkParams,
  filterAndSortAccounts,
  filterAndSortContacts,
  filterAndSortOpportunities,
  filterCustomerCommands,
  filterDedupeCandidates,
  filterOutboundHandoffRows,
  filterRelatedActivities,
  filterRelatedFiles,
  filterRelatedNotes,
  filterRelatedTasks,
  formatMoney,
  groupOpportunitiesByStage,
  isClosedOpportunity,
  isActivationKey,
  isBusinessOsPermissionDenied,
  moneyToCents,
  nextDetailTab,
  normalizeDomain,
  relatedRecords,
  summarizeCustomerCommands,
  summarizeOpportunityPipeline,
  summarizeCustomersData,
  validateAccountDraft,
  validateContactDraft,
  validateNoteDraft,
  validateOpportunityDraft,
  validateTaskDraft,
};
