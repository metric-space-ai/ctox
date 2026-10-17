import { importTemplateToDb } from './templates/skr.js';
import { autoWirePaneGrammar } from 'ctox:shared/pane-grammar.js';
import { contextChatModeMarkup, normalizeContextChatMode, sendContextChat } from 'ctox:shared/context-chat.js';


// --- Native ES Module Imports for Fibu Core Engines ---
import { validateDoubleEntry, computeAccountSalden as computeAccountSaldenImported } from './core/ledger.js';
import { normalizedDelta, computeDepreciationSchedule } from './core/depreciation.js';
import { calculateMatchScore } from './core/reconciler.js';
import { parseCamt053 } from './parsers/camt.js';
import { parseMT940 } from './parsers/mt940.js';
import { getOperatingGuVResult as getOperatingGuVResultImported, buildHgbBilanzTree, buildHgbGuvTree } from './reports/hgb.js';
import { calculateElsterUstva } from './reports/elster.js';
import { generateDatevCsvString } from './exporters/datev.js';

// --- Pure Accounting Extension Core Engines ---
import { calculateRemainingSplit, validateSplitBalanced, compileSplitJournalLines } from './core/splits.js';
import { calculateDailyAllowance, generateTravelDays, calculateTotalTravelAllowance } from './core/travel_expenses.js';
import { calculateMileageReimbursement, calculateAnnualUsageShares, compileMileageJournalLines } from './core/mileage_log.js';
import { calculateEntertainmentSplit, recommendGiftAccount, calculatePrivatePhoneShare, compileEntertainmentJournalLines } from './core/tax_tricks.js';

// --- General Helper: Visually truncate GoBD archive paths ---
function truncateArchiveUrl(url) {
  if (!url) return '—';
  if (url.length <= 32) return url;
  return url.substring(0, 15) + '...' + url.substring(url.length - 15);
}

// --- Global Fibu State ---
const state = {
  mountGeneration: 0,
  lang: 'de',
  activeNav: 'skr',
  activeReportTab: 'bilanz',
  skrName: 'SKR03', // Default
  bookingGrammar: { search: '', view: 'cards', band: 'all', filters: { kind: 'all', sort: 'date', direction: 'desc' } },
  // Die Darstellung (Karten/Liste) haelt das Modul selbst: der Umschalter ist
  // ein einzelner Aktionsknopf, kein aria-pressed-Zustandspaar der Grammatik.
  bookingView: 'cards',
  selectedBookingKey: null,
  evidenceUserCollapsed: false,

  // Data lists loaded from RxDB
  accounts: [],
  journalEntries: [],
  journalEntryLines: [],
  receipts: [],
  bankStatements: [],
  bankStatementLines: [],
  assets: [],

  // Selection states
  selectedAccountId: null,
  selectedEntryId: null,
  selectedReceiptId: null,
  selectedBankLineId: null,
  selectedAssetId: null,

  // Active in-memory DataFrame for fast local queries
  ledgerDF: [],

  // Subscription cleaners
  api: null,
  root: null,
  disposers: [],
  reloadTimer: null,
  contextMenu: null,
  contextMenuCleanup: null,

  // UI Element References
  els: {},
};

const ACCOUNTING_COLLECTIONS = Object.freeze([
  'accounting_accounts',
  'accounting_journal_entries',
  'accounting_journal_entry_lines',
  'accounting_ledger_entries',
  'accounting_receipts',
  'accounting_bank_statements',
  'accounting_bank_statement_lines',
  'accounting_number_series',
]);


// App-API 4.0 (V4-PORT.md): Daten nur über api.query/get/put/patch/remove. Der kleine Speicher-Adapter hält die
// bisherigen Aufrufstellen lesbar (db.<collection>.rows/byId/add/change/drop); jede Methode ist ein direkter
// api-Aufruf, Schreiben wartet nur auf den lokalen Commit (h.local), nie auf den Core.
function fibuStoreCollection(name) {
  const api = state.api;
  return Object.freeze({
    rows: () => api.query(name, {}).get(),
    byId: (id) => api.get(name, String(id)).get(),
    add: (doc) => api.put(name, doc).local,
    change: (id, patch) => api.patch(name, String(id), patch).local,
    drop: (id) => api.remove(name, String(id)).local,
  });
}

function fibuCollection(name) {
  if (!state.api || !name) return null;
  return fibuStoreCollection(name);
}

function fibuDb(requiredCollections = ACCOUNTING_COLLECTIONS) {
  if (!state.api) return null;
  return Object.fromEntries(requiredCollections.map((name) => [name, fibuStoreCollection(name)]));
}

// Rechte entscheidet der Core (§10.2a Nr. 3); die App spiegelt keine Collection-Sperren mehr.
function canWriteCollection(name) {
  return Boolean(state.api && name);
}


// Anlagenspiegel nur aus echten Anlagen (state.assets); keine fest verdrahteten Musteranlagen in echten Büchern.
function assetRows() {
  return (Array.isArray(state.assets) ? state.assets : []).map((a) => {
    const cost = Number(a.acquisition_cost_cents ?? a.cost ?? 0) || 0;
    const prev = Number(a.accumulated_depreciation_cents ?? a.prev ?? 0) || 0;
    return {
      nr: String(a.number || a.nr || a.id || ''),
      name: String(a.name || a.id || ''),
      date: a.acquisition_date ? formatDateDe(a.acquisition_date) : '',
      cost,
      life: Number(a.useful_life_years ?? a.life ?? 0) || 0,
      method: String(a.depreciation_method || a.method || ''),
      prev,
      book: Number(a.book_value_cents ?? a.book ?? (cost - prev)) || 0,
    };
  });
}

// --- Labels for i18n ---
const labels = {
  de: {
    skrExplorer: 'SKR Kontenrahmen-Explorer',
    journal: 'Journal & Hauptbuch',
    receipts: 'Eingangsbelege',
    banking: 'Bankabgleich',
    reports: 'Berichte & Abschlüsse',
    assets: 'Anlagenspiegel',
    initSuccess: 'Kontenrahmen erfolgreich initialisiert!',
    stornoSuccess: 'Buchung erfolgreich storniert (GoBD-konform).',
    postedSuccess: 'Buchung festgeschrieben und GoBD-gesperrt.',
    // v4-allow: label (Buchhaltungsbegriff Bankabgleich)
    matchSuccess: 'Transaktion erfolgreich mit Beleg abgeglichen.',
    balancedAlert: 'Ausgeglichen (Soll = Haben)',
    unbalancedAlert: 'Ungleichgewicht! Soll und Haben müssen übereinstimmen.',
  },
  en: {
    skrExplorer: 'Chart of Accounts Explorer',
    journal: 'Journal & General Ledger',
    receipts: 'Incoming Receipts',
    banking: 'Bank Reconciliation',
    reports: 'Financial Reports',
    assets: 'Asset Depreciation (AfA)',
    initSuccess: 'Chart of Accounts initialized successfully!',
    stornoSuccess: 'Transaction reversed successfully (HGB/GoBD compliant).',
    postedSuccess: 'Transaction posted and GoBD-locked.',
    matchSuccess: 'Transaction reconciled successfully.',
    balancedAlert: 'Balanced (Debit = Credit)',
    unbalancedAlert: 'Imbalance! Debit and Credit must match.',
  }
};

let t = (key) => labels.de[key] ?? key;

function langFrom(locale) {
  return String(locale || '').toLowerCase().startsWith('en') ? 'en' : 'de';
}

// api.shell.t übersteuert die eingebetteten Texte, solange der Schlüsselraum offen ist (app-api.d.ts OFFEN-B).
function applyLocale(lang) {
  state.lang = lang;
  const api = state.api;
  t = (key) => {
    const scoped = `buchhaltung.${key}`;
    const value = api?.shell?.t?.(scoped);
    if (typeof value === 'string' && value && value !== scoped) return value;
    return labels[lang]?.[key] ?? labels.de[key] ?? key;
  };
}

async function loadModuleMarkup() {
  // index.html liefert der Service Worker versioniert (§16.15); CSS lädt die Shell aus dem Manifest.
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}

// Befehle: api.command legt das Befehlsdokument an und liefert sofort; die UI wartet nie. Fehlschläge gehen in
// die Zustandszone (code buchhaltung.command.<type>), Erfolg bleibt still.
function dispatchFibuCommand(type, payload) {
  const api = state.api;
  if (!api) return null;
  const handle = api.command(type, payload);
  const code = `buchhaltung.command.${type}`;
  let off = null;
  let finished = false;
  const stop = () => { finished = true; off?.(); off = null; };
  off = handle.status.subscribe((value) => {
    if (value?.status === 'failed') {
      api.status.report({ level: 'error', code, text: `${type}: ${value.error || 'failed'}`, ref: handle.id });
      stop();
    } else if (value?.status === 'done') {
      api.status.clear(code);
      stop();
    }
  });
  if (finished) off?.();
  handle.local.catch((error) => {
    api.status.report({ level: 'error', code, text: `${type}: ${error?.code || 'local'} ${error?.message || error}`, ref: handle.id });
  });
  state.disposers.push(stop);
  return handle;
}

// Meldungen, die früher per alert() kamen, gehen in die Zustandszone (Anf. 27: nichts außerhalb des Fensters).
function fibuNotice(code, text, level = 'info') {
  state.api?.status.report({ level, code: `buchhaltung.${code}`, text: String(text) });
}

const SNAPSHOT_VIEW_KEYS = Object.freeze([
  'activeNav', 'activeReportTab', 'skrName', 'bookingView', 'selectedBookingKey',
  'evidenceUserCollapsed', 'selectedAccountId', 'selectedEntryId', 'selectedReceiptId', 'selectedBankLineId', 'selectedAssetId',
]);
const INITIAL_VIEW = Object.freeze(Object.fromEntries(SNAPSHOT_VIEW_KEYS.map((key) => [key, state[key]])));

function fibuSnapshot() {
  const snap = { v: 1 };
  for (const key of SNAPSHOT_VIEW_KEYS) snap[key] = state[key];
  const root = state.root;
  // Offene Eingaben (Suchfelder, Formulare im Arbeitsbereich und in der Schublade) mitnehmen.
  const inputs = {};
  root?.querySelectorAll('input[id], textarea[id], select[id], [data-pg-search], [data-search-accounts], [data-search-journal]').forEach((el) => {
    if (el.type === 'file' || el.type === 'checkbox' || el.type === 'radio') return;
    const key = el.id ? `#${el.id}` : ['data-pg-search', 'data-search-accounts', 'data-search-journal'].map((a) => (el.hasAttribute(a) ? `[${a}]` : '')).find(Boolean);
    if (key && el.value) inputs[key] = el.value;
  });
  snap.inputs = inputs;
  return snap;
}

function applyRestoredView(restored) {
  if (!restored || typeof restored !== 'object') return;
  for (const key of SNAPSHOT_VIEW_KEYS) {
    if (restored[key] === undefined) continue;
    const initial = INITIAL_VIEW[key];
    if (initial === null ? (restored[key] === null || typeof restored[key] === 'string') : typeof restored[key] === typeof initial) {
      state[key] = restored[key];
    }
  }
}

function restoreInputs(restored) {
  const inputs = restored?.inputs;
  if (!inputs || typeof inputs !== 'object') return;
  for (const [selector, value] of Object.entries(inputs)) {
    if (typeof value !== 'string') continue;
    let el = null;
    try { el = state.root?.querySelector(selector); } catch { el = null; }
    if (el && 'value' in el) el.value = value;
  }
}

// =========================================================================
// 🏁 Lifecycle Mount Hook (App-API 4.0)
// =========================================================================
export async function mount(root, api, { restored } = {}) {
  const mountGeneration = ++state.mountGeneration;
  // Abos gehören dieser Montage (PORTING.md §4 Falle 1).
  const disposers = [];
  state.disposers = disposers;
  state.api = api;
  state.root = root;
  if (!root.dataset.app) root.dataset.app = api.app?.id || 'buchhaltung';
  applyLocale(langFrom(api.shell.locale.get()));
  state.selectedBookingKey = null;
  state.evidenceUserCollapsed = false;
  applyRestoredView(restored);

  root.innerHTML = await loadModuleMarkup();
  if (state.mountGeneration !== mountGeneration) return { unmount() {}, snapshot: fibuSnapshot };

  state.els.root?.removeAttribute('data-mount-ready');
  bindElements(root);
  // Pane-Grammatik verdrahtet die App selbst (PORTING.md R14), keine DOM-Heuristik der Shell.
  autoWirePaneGrammar(root);
  api.shell.setTitle('Buchhaltung');

  if (state.els.skrSelect) state.els.skrSelect.value = state.skrName || 'SKR03';

  wireEvents();
  if (state.els.root) state.els.root.dataset.mountReady = 'true';
  restoreInputs(restored);
  renderBookingList();
  switchView(state.activeNav);
  updateEvidencePaneVisibility();

  // Live-Abos statt Readiness/Ladefunktion: subscribe ruft sofort mit dem lokalen Stand.
  for (const name of FIBU_LIVE_COLLECTIONS) {
    disposers.push(api.query(name, {}).subscribe(scheduleFibuReload));
  }
  disposers.push(api.shell.locale.subscribe((locale) => {
    const lang = langFrom(locale);
    if (lang !== state.lang) { applyLocale(lang); renderActiveView(); }
  }));

  let disposed = false;
  Promise.resolve()
    .then(async () => {
      await autoInitializeAccounts();
      if (disposed || state.api !== api) return;
      await loadAllFibuData();
      if (disposed || state.api !== api) return;
      renderBookingList();
      renderActiveView();
    })
    .catch((error) => {
      if (disposed || state.api !== api) return;
      api.status.report({ level: 'error', code: 'buchhaltung.init', text: `Buchhaltung: ${error?.message || error}` });
      renderActiveView();
    });

  return {
    unmount() {
      revokeReceiptFileUrl();
      disposed = true;
      if (state.reloadTimer) clearTimeout(state.reloadTimer);
      state.reloadTimer = null;
      for (const dispose of disposers.splice(0)) {
        try { dispose(); } catch (error) { console.warn('[fibu] unsubscribe failed', error); }
      }
      if (state.mountGeneration === mountGeneration) {
        state.mountGeneration += 1;
        state.els.root?.removeAttribute('data-mount-ready');
        state.contextMenuCleanup?.();
        state.contextMenu?.remove();
        state.contextMenu = null;
        unbindEvents();
      }
      root.replaceChildren();
      if (state.root === root) {
        state.api = null;
        state.root = null;
      }
    },
    snapshot: fibuSnapshot,
  };
}

const FIBU_LIVE_COLLECTIONS = Object.freeze([
  'accounting_accounts',
  'accounting_journal_entries',
  'accounting_journal_entry_lines',
  'accounting_receipts',
  'accounting_bank_statements',
  'accounting_bank_statement_lines',
]);

// Entprellt: mehrere Abos melden sich im selben Takt (erste Emission je Abo sofort).
function scheduleFibuReload() {
  if (state.reloadTimer) return;
  state.reloadTimer = setTimeout(() => {
    state.reloadTimer = null;
    if (!state.api) return;
    loadAllFibuData().then(() => {
      renderBookingList();
      renderActiveView();
      syncOpenReceiptForm();
    });
  }, 40);
}

// =========================================================================
// 🏛️ Bind Elements & UI Resizers
// =========================================================================
function bindElements(host) {
  state.els = {
    root: host.querySelector('[data-fibu-root]'),
    bookingPane: host.querySelector('[data-booking-pane]'),
    bookingList: host.querySelector('[data-booking-list]'),
    bookingViewToggle: host.querySelector('[data-fibu-view-toggle]'),
    bookingImportInput: host.querySelector('[data-booking-import-input]'),
    workbenchView: host.querySelector('[data-workbench-view]'),
    navItems: host.querySelectorAll('[data-nav]'),
    skrSelect: host.querySelector('#skr-select'),
    centerCategory: host.querySelector('[data-active-category]'),
    centerTitle: host.querySelector('[data-active-title]'),
    centerActions: host.querySelector('[data-center-actions]'),
    evidenceToggle: host.querySelector('[data-toggle-evidence]'),
    evidenceCollapse: host.querySelector('[data-collapse-evidence]'),

    // View panels
    panels: {
      skr: host.querySelector('[data-panel="skr"]'),
      journal: host.querySelector('[data-panel="journal"]'),
      receipts: host.querySelector('[data-panel="receipts"]'),
      banking: host.querySelector('[data-panel="banking"]'),
      travel: host.querySelector('[data-panel="travel"]'),
      mileage: host.querySelector('[data-panel="mileage"]'),
      reports: host.querySelector('[data-panel="reports"]'),
      assets: host.querySelector('[data-panel="assets"]'),
    },

    // Data list containers
    accountsList: host.querySelector('[data-accounts-list]'),
    journalList: host.querySelector('[data-journal-list]'),
    receiptsList: host.querySelector('[data-receipts-list]'),
    bankingList: host.querySelector('[data-banking-list]'),
    assetsList: host.querySelector('[data-assets-list]'),
    travelExpensesList: host.querySelector('#travel-expenses-list'),
    mileageLogList: host.querySelector('#mileage-log-list'),

    // Dropzones & Inputs
    fileDropzone: host.querySelector('[data-file-dropzone]'),
    fileInput: host.querySelector('#fibu-file-input'),
    bankDropzone: host.querySelector('[data-bank-dropzone]'),
    bankInput: host.querySelector('#fibu-bank-input'),

    // Search fields
    searchAccounts: host.querySelector('[data-search-accounts]'),
    searchJournal: host.querySelector('[data-search-journal]'),
    accountsCount: host.querySelector('[data-accounts-count]'),
    journalCount: host.querySelector('[data-journal-count]'),

    // Reports sub-panels & tabs
    reportTabBtns: host.querySelectorAll('[data-report-tab]'),
    reportSubpanels: {
      bilanz: host.querySelector('[data-report-subpanel="bilanz"]'),
      guv: host.querySelector('[data-report-subpanel="guv"]'),
      ustva: host.querySelector('[data-report-subpanel="ustva"]'),
      datev: host.querySelector('[data-report-subpanel="datev"]')
    },

    // Reports containers
    bilanzAktiva: host.querySelector('[data-bilanz-aktiva-tree]'),
    bilanzPassiva: host.querySelector('[data-bilanz-passiva-tree]'),
    guvTree: host.querySelector('[data-guv-tree]'),

    // UStVA Fields
    ustva81: host.querySelector('[data-ustva-field-81]'),
    ustvaTax81: host.querySelector('[data-ustva-tax-81]'),
    ustva86: host.querySelector('[data-ustva-field-86]'),
    ustvaTax86: host.querySelector('[data-ustva-tax-86]'),
    ustva66: host.querySelector('[data-ustva-field-66]'),
    ustvaZahllast: host.querySelector('[data-ustva-zahllast]'),

    // DATEV Exporter inputs
    datevStart: host.querySelector('#datev-start'),
    datevEnd: host.querySelector('#datev-end'),

    // Bottom slide-up drawer
    drawer: host.querySelector('[data-drawer]'),
    drawerTitle: host.querySelector('[data-drawer-header-title]'),
    drawerContent: host.querySelector('[data-drawer-content-body]'),
    drawerCloseBtn: host.querySelector('[data-action="close-drawer"]'),

    // Auxiliary Panel containers
    ocrPreviewContainer: host.querySelector('[data-ocr-preview-container]')
  };
}

function setupResizers(host) {
  // Column resizing is now owned by the shell-global resizer (app.js
  // `setupModuleResizers`): the `.ctox-column-resizer[data-resizer-var]`
  // handles in index.html, inside the `[data-resize-frame]` root, get
  // drag/keyboard/persistence for free.
  void host;
  return () => {};
}

// =========================================================================
// 🔒 GoBD Hook & Security Guards
// =========================================================================
// GoBD-Sperre: RxDB-preSave/preRemove gibt es in 4.0 nicht (und die Shell-Fassade bot sie nie an – der
// frühere Aufruf lief ins catch). Festschreibung prüft der Handler (V4-PORT.md OFFEN-2).

// =========================================================================
// ⚙️ Database Auto-Initialization
// =========================================================================
async function ensureRequiredAccountsExist() {
  const db = fibuDb(['accounting_accounts']);
  if (!db || !canWriteCollection('accounting_accounts')) return;

  const requiredSKR03 = [
    { code: '1890', name: 'Privateinlage', root_type: 'equity', account_type: 'regular', parent_id: '0800_G', is_group: false },
    { code: '1370', name: 'Gesellschafter-Verrechnungskonto', root_type: 'liability', account_type: 'payable', parent_id: '1700', is_group: false },
    { code: '4650', name: 'Bewirtungskosten abzugsfähig', root_type: 'expense', account_type: 'expense', parent_id: '4000', is_group: false },
    { code: '4654', name: 'Bewirtungskosten nicht abzugsfähig', root_type: 'expense', account_type: 'expense', parent_id: '4000', is_group: false },
    { code: '4630', name: 'Geschenke abzugsfähig', root_type: 'expense', account_type: 'expense', parent_id: '4000', is_group: false },
    { code: '4635', name: 'Geschenke nicht abzugsfähig', root_type: 'expense', account_type: 'expense', parent_id: '4000', is_group: false },
    { code: '4660', name: 'Reisekosten Arbeitnehmer', root_type: 'expense', account_type: 'expense', parent_id: '4000', is_group: false },
    { code: '4673', name: 'Reisekosten Arbeitnehmer (Fahrtkosten)', root_type: 'expense', account_type: 'expense', parent_id: '4000', is_group: false },
    { code: '1800', name: 'Privatentnahme', root_type: 'equity', account_type: 'regular', parent_id: '0800_G', is_group: false },
    { code: '1880', name: 'Unentgeltliche Wertabgaben (Telefon Privatanteil)', root_type: 'revenue', account_type: 'revenue', parent_id: '8000', is_group: false },
    { code: '8921', name: 'Verwendung von Gegenständen (Kfz-Privatanteil)', root_type: 'revenue', account_type: 'revenue', parent_id: '8000', is_group: false }
  ];

  const requiredSKR04 = [
    { code: '2180', name: 'Privateinlage', root_type: 'equity', account_type: 'regular', parent_id: '2000', is_group: false },
    { code: '1486', name: 'Gesellschafter-Verrechnungskonto', root_type: 'liability', account_type: 'payable', parent_id: '3500', is_group: false },
    { code: '6640', name: 'Bewirtungskosten abzugsfähig', root_type: 'expense', account_type: 'expense', parent_id: '6000', is_group: false },
    { code: '6644', name: 'Bewirtungskosten nicht abzugsfähig', root_type: 'expense', account_type: 'expense', parent_id: '6000', is_group: false },
    { code: '6620', name: 'Geschenke abzugsfähig', root_type: 'expense', account_type: 'expense', parent_id: '6000', is_group: false },
    { code: '6625', name: 'Geschenke nicht abzugsfähig', root_type: 'expense', account_type: 'expense', parent_id: '6000', is_group: false },
    { code: '6670', name: 'Reisekosten Arbeitnehmer', root_type: 'expense', account_type: 'expense', parent_id: '6000', is_group: false },
    { code: '6680', name: 'Reisekosten Arbeitnehmer (Fahrtkosten)', root_type: 'expense', account_type: 'expense', parent_id: '6000', is_group: false },
    { code: '2100', name: 'Privatentnahme', root_type: 'equity', account_type: 'regular', parent_id: '2000', is_group: false },
    { code: '2130', name: 'Unentgeltliche Wertabgaben', root_type: 'equity', account_type: 'regular', parent_id: '2000', is_group: false },
    { code: '4645', name: 'Verwendung von Gegenständen (Kfz-Privatanteil)', root_type: 'revenue', account_type: 'revenue', parent_id: '4000', is_group: false }
  ];

  const currentList = state.skrName === 'SKR04' ? requiredSKR04 : requiredSKR03;
  const now = Date.now();

  for (const acct of currentList) {
    const id = `${state.skrName}_${acct.code}`;
    const parentId = acct.parent_id ? `${state.skrName}_${acct.parent_id}` : '';
    const existing = await db.accounting_accounts.byId(id);
    const existingData = docToRecord(existing);
    if (!existing) {
      await db.accounting_accounts.add({
        id,
        code: acct.code,
        name: acct.name,
        root_type: acct.root_type,
        account_type: acct.account_type,
        parent_id: parentId,
        is_group: acct.is_group,
        tax_rate_id: '',
        skr: state.skrName,
        updated_at_ms: now
      });
      console.log(`[fibu] Ensured account exists: ${acct.code} in ${state.skrName}`);
    } else if (existingData.parent_id !== parentId) {
      await db.accounting_accounts.change(existing.id, {
        parent_id: parentId,
        updated_at_ms: now
      });
      console.log(`[fibu] Auto-healed parent_id prefix for: ${acct.code} to ${parentId}`);
    }
  }
}

// =========================================================================
// ⚙️ Database Auto-Initialization
// =========================================================================
async function autoInitializeAccounts() {
  const db = fibuDb(['accounting_accounts']);
  if (!db || !canWriteCollection('accounting_accounts')) return;

  try {
    const existingAccounts = (await collectionRecords(db.accounting_accounts))
      .map(normalizeAccountRecord)
      .filter(accountMatchesCurrentSkr);
    if (existingAccounts.length === 0) {
      await importTemplateToDb(db, state.skrName);
      console.log(`[fibu] initialized chart of accounts: ${state.skrName}`);
    }
    await ensureRequiredAccountsExist();
  } catch (err) {
    console.error('[fibu] failed to auto-initialize accounts', err);
  }
}

async function collectionRecords(collection) {
  const docs = collection?.rows?.();
  return Array.isArray(docs) ? docs.map(docToRecord) : [];
}

function docToRecord(doc) {
  return doc || {};
}

function sortRecordsDesc(records, field) {
  return [...records].sort((left, right) => {
    const a = left?.[field] ?? '';
    const b = right?.[field] ?? '';
    if (typeof a === 'number' || typeof b === 'number') {
      return Number(b || 0) - Number(a || 0);
    }
    return String(b).localeCompare(String(a));
  });
}

function normalizeAccountRecord(account) {
  const id = String(account?.id || '');
  const inferredSkr = /skr04/i.test(id) ? 'SKR04' : (/skr03/i.test(id) ? 'SKR03' : '');
  const classification = String(account?.classification || account?.root_type || account?.account_type || '').toLowerCase();
  return {
    ...account,
    name: account?.name || account?.title || account?.label || account?.code || id,
    root_type: account?.root_type || classification || 'asset',
    account_type: account?.account_type || classification || 'regular',
    parent_id: account?.parent_id || '',
    is_group: Boolean(account?.is_group || account?.isGroup || account?.group),
    tax_rate_id: account?.tax_rate_id || '',
    skr: account?.skr || inferredSkr,
    updated_at_ms: Number(account?.updated_at_ms || account?.updatedAtMs || account?.lastWriteTime || 0),
  };
}

function accountMatchesCurrentSkr(account) {
  if (account?.skr) return account.skr === state.skrName;
  const marker = state.skrName.toLowerCase();
  return String(account?.id || '').toLowerCase().includes(marker);
}

// =========================================================================
// 📡 Realtime Subscriptions & Aggregations
// =========================================================================

async function loadAllFibuData() {
  const db = fibuDb([
    'accounting_accounts',
    'accounting_journal_entries',
    'accounting_journal_entry_lines',
    'accounting_receipts',
    'accounting_bank_statements',
    'accounting_bank_statement_lines',
  ]);
  if (!db) return;

  try {
    // 1. Fetch from RxDB
    state.accounts = (await collectionRecords(db.accounting_accounts))
      .map(normalizeAccountRecord)
      .filter(accountMatchesCurrentSkr)
      .sort((a, b) => String(a.code || '').localeCompare(String(b.code || '')));

    state.journalEntries = sortRecordsDesc(
      await collectionRecords(db.accounting_journal_entries),
      'posting_date'
    );

    state.journalEntryLines = await collectionRecords(db.accounting_journal_entry_lines);

    state.receipts = sortRecordsDesc(
      await collectionRecords(db.accounting_receipts),
      'updated_at_ms'
    );

    state.bankStatements = await collectionRecords(db.accounting_bank_statements);

    state.bankStatementLines = sortRecordsDesc(
      await collectionRecords(db.accounting_bank_statement_lines),
      'value_date'
    );

    // 2. Build local DataFrame for instant ledger indexing
    rebuildLedgerDataFrame();

    // 3. Compute recursive rolls for accounts list view
    computeAccountSalden();

  } catch (err) {
    console.error('[fibu] failed loading local data', err);
  }
}

// =========================================================================
// 🚀 Rebuild In-Memory Ledger DataFrame (Ultra-Fast Aggregation Engine)
// =========================================================================
function rebuildLedgerDataFrame() {
  state.ledgerDF = [];

  // Join posted entries with their lines to form denormalized journal transactions
  const postedEntries = state.journalEntries.filter(e => e.posted_at);

  postedEntries.forEach(entry => {
    const lines = state.journalEntryLines.filter(l => l.journal_entry_id === entry.id);
    lines.forEach(line => {
      state.ledgerDF.push({
        journal_entry_id: entry.id,
        posting_date: entry.posting_date,
        type: entry.type,
        ref_id: entry.ref_id,
        number: entry.number,
        narration: entry.narration,
        account_id: line.account_id,
        debit: line.debit || 0, // In cents
        credit: line.credit || 0 // In cents
      });
    });
  });
}

// Compute dynamic recursive rolled up salden for both SKR03 and SKR04
// Compute dynamic recursive rolled up salden for both SKR03 and SKR04
function computeAccountSalden() {
  state.accounts = computeAccountSaldenImported(state.accounts, state.ledgerDF);
}

// =========================================================================
// 🎨 Event Handlers & Event Bus
// =========================================================================
function wireEvents() {
  const els = state.els;
  if (!els.root) return;

  // Shell-owned pane grammar reports its current filter/view state. Only data
  // or grammar changes rebuild this list; selecting a row never does.
  els.bookingPane?.addEventListener('ctox-pane-grammar-change', (event) => {
    // Suche, Band und Filter kommen aus der Grammatik; die Darstellung nicht --
    // sie gehoert dem Ein-Knopf-Umschalter und wird hier bewahrt.
    state.bookingGrammar = { ...(event.detail || state.bookingGrammar), view: state.bookingView };
    renderBookingList();
  });

  els.bookingViewToggle?.addEventListener('click', () => {
    setBookingView(state.bookingView === 'list' ? 'cards' : 'list');
  });
  syncBookingViewToggle();

  els.bookingList?.addEventListener('click', (event) => {
    const row = event.target.closest('[data-booking-key]');
    if (row) selectBookingRecord(row.dataset.bookingKey);
  });

  els.root.querySelector('[data-action="new-booking"]')?.addEventListener('click', () => {
    switchView('journal');
    openManualJournalDrawer();
  });
  els.root.querySelector('[data-action="import-bookings"]')?.addEventListener('click', () => els.bookingImportInput?.click());
  els.root.querySelector('[data-action="export-bookings"]')?.addEventListener('click', exportBookingRecords);
  els.bookingImportInput?.addEventListener('change', importBookingRecords);

  els.workbenchView?.addEventListener('change', (event) => switchView(event.target.value));

  els.evidenceToggle?.addEventListener('click', () => {
    state.evidenceUserCollapsed = !state.evidenceUserCollapsed;
    updateEvidencePaneVisibility();
  });
  els.evidenceCollapse?.addEventListener('click', () => {
    state.evidenceUserCollapsed = true;
    updateEvidencePaneVisibility();
  });

  // SKR Selector change
  els.skrSelect?.addEventListener('change', async (e) => {
    state.skrName = e.target.value;
    await autoInitializeAccounts();
    await loadAllFibuData();
    renderActiveView();
  });

  // Search account typing
  els.searchAccounts?.addEventListener('input', (e) => {
    const q = e.target.value.toLowerCase().trim();
    filterAccountsView(q);
  });

  // Search journal typing
  els.searchJournal?.addEventListener('input', (e) => {
    const q = e.target.value.toLowerCase().trim();
    filterJournalView(q);
  });

  // Report Sub-Tabs switcher
  els.reportTabBtns.forEach(btn => {
    btn.addEventListener('click', () => {
      els.reportTabBtns.forEach(b => b.setAttribute('aria-selected', String(b === btn)));
      const target = btn.getAttribute('data-report-tab');
      switchReportSubpanel(target);
    });
  });

  // Bank import file input
  els.bankDropzone?.addEventListener('click', () => els.bankInput?.click());
  els.bankInput?.addEventListener('change', handleBankStatementImport);

  // Belege Drag & Drop or Click upload
  els.fileDropzone?.addEventListener('click', () => els.fileInput?.click());
  els.fileInput?.addEventListener('change', handleBelegeUpload);

  // Trigger auto reconciliation
  els.centerActions?.addEventListener('click', handleToolbarActions);
  els.panels.skr?.querySelector('[data-action="init-skr"]')?.addEventListener('click', forceReInitSKR);
  const newEntryButton = els.panels.journal?.querySelector('[data-action="new-entry"]');
  if (newEntryButton) {
    newEntryButton.onclick = () => {
      // A shell remount can leave the previous window DOM connected briefly.
      // Keep this visible action bound to the element set it was wired with so
      // the drawer is always opened in the window the operator clicked.
      state.els = els;
      els.root.dataset.newEntryInvoked = 'true';
      try {
        openManualJournalDrawer();
        delete els.root.dataset.newEntryError;
      } catch (error) {
        els.root.dataset.newEntryError = error?.message || String(error);
        console.error('[fibu] failed to open manual journal entry', error);
      }
    };
  }
  els.panels.banking?.querySelector('[data-action="run-auto-reconciliation"]')?.addEventListener('click', triggerAutoReconcile);
  els.panels.assets?.querySelector('[data-action="new-asset"]')?.addEventListener('click', requestNewAsset);
  els.panels.assets?.querySelector('[data-action="trigger-depreciations"]')?.addEventListener('click', triggerDepreciationRun);

  // DATEV Export Click
  els.panels.reports?.querySelector('[data-action="export-datev"]')?.addEventListener('click', triggerDatevExport);

  // Drawer close
  els.drawerCloseBtn?.addEventListener('click', closeDrawer);
}

// Früher ctx.contextActions.dispatch('data', …) → Befehl business_os.data.modify (app.js
// createContextActionsFacade); jetzt derselbe Befehl direkt über api.command.
function dispatchDataTask(event, title, prompt) {
  const context = buchhaltungCommandContextFromElement(state, event?.currentTarget);
  return dispatchFibuCommand('business_os.data.modify', {
    record_id: context.record_id || 'buchhaltung',
    title,
    instruction: prompt,
    prompt,
    user_message: prompt,
    mode: 'data',
    target: 'data',
    context,
    source_context: context,
    thread_key: `business-os/buchhaltung/${context.record_id || 'module'}`,
    response_channel: 'business_os_chat',
    outbound_channel: 'business_os_chat',
    client_context: { action: 'context-data', module_id: 'buchhaltung' },
  });
}

function requestNewAsset(event) {
  // v4-allow: text (Agenten-Auftrag, kein Oberflächentext)
  dispatchDataTask(event, 'Anlagegut erfassen', 'Erfasse ein neues Anlagegut. Frage die fehlenden Stammdaten ab und lege den Datensatz nach Freigabe an.');
}

function triggerDepreciationRun(event) {
  // v4-allow: text (Agenten-Auftrag, kein Oberflächentext)
  dispatchDataTask(event, 'AfA-Abschreibungslauf starten', 'Starte den AfA-Abschreibungslauf für die aktuelle Buchungsperiode und liefere den Laufstatus sowie die erzeugten Buchungen.');
}

function unbindEvents() {
  // Standard garbage collection handles most, but we can clean timers if any.
}

// =========================================================================
// 🚀 View Routing Engine
// =========================================================================
function switchView(navId) {
  // Ein Snapshot kann eine inzwischen entfernte Ansicht tragen (z. B. 'tests'): dann die Standardansicht.
  const known = state.els.workbenchView ? [...state.els.workbenchView.options].map((o) => o.value) : null;
  if (known && !known.includes(navId)) navId = INITIAL_VIEW.activeNav;
  state.activeNav = navId;
  if (state.els.workbenchView) state.els.workbenchView.value = navId;

  // Update active navigation items styling
  if (state.els.navItems) {
    state.els.navItems.forEach(item => {
      if (item.getAttribute('data-nav') === navId) {
        item.classList.add('active');
      } else {
        item.classList.remove('active');
      }
    });
  }

  // Hide all panels
  Object.values(state.els.panels).forEach(p => {
    if (p) p.hidden = true;
  });

  // Show active panel
  const activePanel = state.els.panels[navId];
  if (activePanel) {
    activePanel.hidden = false;
  }

  // Dynamic header text
  const titles = {
    skr: { cat: 'KONTENRAHMEN', title: `Kontenrahmen ${state.skrName}` },
    journal: { cat: 'BUCHUNGEN', title: 'Journal & Hauptbuch' },
    receipts: { cat: 'BELEGE', title: 'Eingangsbelege' },
    banking: { cat: 'BANK & KASSE', title: 'Bankabgleich' },
    travel: { cat: 'SPESENABRECHNUNG', title: 'Dienstreisen' },
    mileage: { cat: 'FAHRTENBUCH', title: 'Fahrtenbuch' },
    reports: { cat: 'JAHRESABSCHLUSS', title: 'Auswertungen' },
    assets: { cat: 'ANLAGENVERWALTUNG', title: 'Anlagenspiegel' },
  };

  if (state.els.centerCategory && state.els.centerTitle) {
    state.els.centerCategory.textContent = '';
    state.els.centerTitle.textContent = titles[navId].title;
  }

  // Render details for the specific view
  renderActiveView();
}

function switchReportSubpanel(subId) {
  state.activeReportTab = subId;
  Object.values(state.els.reportSubpanels).forEach(p => {
    if (p) p.hidden = true;
  });
  const active = state.els.reportSubpanels[subId];
  if (active) active.hidden = false;

  renderActiveView();
}

function renderActiveView() {
  switch (state.activeNav) {
    case 'skr':
      filterAccountsView(state.els.searchAccounts?.value?.toLowerCase().trim() || '');
      break;
    case 'journal':
      filterJournalView(state.els.searchJournal?.value?.toLowerCase().trim() || '');
      break;
    case 'receipts':
      renderReceiptsList();
      break;
    case 'banking':
      renderBankingList();
      break;
    case 'travel':
      renderTravelList();
      break;
    case 'mileage':
      renderMileageList();
      break;
    case 'reports':
      renderReports();
      break;
    case 'assets':
      renderAssetsList();
      break;
  }
}

function bookingRecords() {
  const receipts = state.receipts.map((receipt) => ({
    key: `receipt:${receipt.id}`,
    id: receipt.id,
    kind: 'receipt',
    status: receipt.status === 'posted' ? 'posted' : 'draft',
    date: receipt.invoice_date || '',
    label: receipt.filename || receipt.invoice_number || receipt.id,
    description: receipt.supplier_name || '',
    amount: hasAmount(receipt.gross_amount) ? Number(receipt.gross_amount) : null,
    source: receipt,
  }));
  const bookings = state.journalEntries.map((entry) => ({
    key: `booking:${entry.id}`,
    id: entry.id,
    kind: 'booking',
    status: entry.posted_at ? 'posted' : 'draft',
    date: entry.posting_date || '',
    label: entry.number || 'Buchungsentwurf',
    description: entry.narration || 'Unbenannte Buchung',
    amount: state.journalEntryLines
      .filter((line) => line.journal_entry_id === entry.id)
      .reduce((sum, line) => sum + Number(line.debit || 0), 0),
    source: entry,
  }));
  return [...receipts, ...bookings];
}

function bookingStatusLabel(status) {
  return status === 'posted' ? 'Gebucht' : 'Offen';
}

function bookingKindLabel(kind) {
  return kind === 'receipt' ? 'Beleg' : 'Buchung';
}

// Ein-Knopf-Umschalter: das Icon zeigt die Ansicht, zu der gewechselt wird,
// Beschriftung und Tooltip benennen genau diese Aktion. Der Knopf traegt
// keinen aria-pressed-Zustand.
function syncBookingViewToggle() {
  const toggle = state.els?.bookingViewToggle;
  if (!toggle) return;
  const view = state.bookingView === 'list' ? 'list' : 'cards';
  const label = view === 'list' ? 'Als Karten anzeigen' : 'Als Liste anzeigen';
  toggle.dataset.fibuView = view;
  toggle.setAttribute('aria-label', label);
  toggle.setAttribute('title', label);
  toggle.removeAttribute('aria-pressed');
}

function setBookingView(view) {
  const next = view === 'list' ? 'list' : 'cards';
  if (state.bookingView === next) return;
  state.bookingView = next;
  state.bookingGrammar = { ...(state.bookingGrammar || {}), view: next };
  syncBookingViewToggle();
  renderBookingList();
}

function renderBookingList() {
  const pane = state.els.bookingPane;
  const container = state.els.bookingList;
  if (!pane || !container) return;

  const grammar = state.bookingGrammar || {};
  const filters = grammar.filters || {};
  const allRecords = bookingRecords();
  const scoped = filters.kind && filters.kind !== 'all'
    ? allRecords.filter((record) => record.kind === filters.kind)
    : allRecords;
  const counts = {
    all: scoped.length,
    draft: scoped.filter((record) => record.status === 'draft').length,
    posted: scoped.filter((record) => record.status === 'posted').length,
  };
  const search = String(grammar.search || '').trim().toLowerCase();
  const band = grammar.band || 'all';
  const filtered = scoped.filter((record) => {
    if (band !== 'all' && record.status !== band) return false;
    if (!search) return true;
    return `${record.label} ${record.description} ${record.date}`.toLowerCase().includes(search);
  });
  const sortField = filters.sort || 'date';
  const direction = filters.direction === 'asc' ? 1 : -1;
  filtered.sort((left, right) => {
    const a = sortField === 'amount' ? left.amount : String(left[sortField] || '');
    const b = sortField === 'amount' ? right.amount : String(right[sortField] || '');
    return direction * (typeof a === 'number' ? a - b : a.localeCompare(b, state.lang));
  });

  container.classList.toggle('is-compact', grammar.view === 'list');
  if (filtered.length === 0) {
    // Leere Liste rendert nichts (R11); kein Lade- oder Leertext.
    container.innerHTML = '';
  } else {
    // Zwei echte Dichten statt zweier Varianten derselben Zeile:
    // KARTEN tragen Titel plus zwei Meta-Zeilen (Zuordnung/Status/Datum, dann
    // Beschreibung und Betrag), LISTE genau eine Zeile aus Titel und einem
    // Kurz-Meta rechts.
    const compact = grammar.view === 'list';
    container.innerHTML = filtered.map((record) => {
      const selected = state.selectedBookingKey === record.key;
      const head = `<button type="button" class="ctox-list-item fibu-booking-row ${compact ? 'is-compact' : ''} ${selected ? 'is-selected' : ''}"
        data-booking-key="${escapeHtml(record.key)}"
        data-context-record-id="${escapeHtml(record.id)}"
        data-context-record-type="${record.kind === 'receipt' ? 'accounting_receipt' : 'accounting_journal_entry'}"
        data-context-label="${escapeHtml(record.label)}"
        aria-selected="${selected ? 'true' : 'false'}">`;
      const body = compact
        ? `<strong>${escapeHtml(record.label)}</strong>
        <span class="fibu-booking-lead">${formatAmountOrDash(record.amount)}</span>`
        : `<strong>${escapeHtml(record.label)}</strong>
        <span class="fibu-booking-meta">${bookingKindLabel(record.kind)} · <em class="fibu-booking-status is-${record.status}">${bookingStatusLabel(record.status)}</em> · ${escapeHtml(formatDateDe(record.date) || 'ohne Datum')}</span>
        <span class="fibu-booking-meta fibu-booking-meta-split"><span class="fibu-booking-desc">${escapeHtml(record.description)}</span><span class="fibu-booking-amount">${formatAmountOrDash(record.amount)}</span></span>`;
      return `${head}${body}</button>`;
    }).join('');
  }

  const grammarHandle = pane.__ctoxPaneGrammar;
  grammarHandle?.setCounts?.(counts);
  if (!grammarHandle) {
    Object.entries(counts).forEach(([key, value]) => {
      const node = pane.querySelector(`[data-pg-count="${key}"]`);
      if (node) node.textContent = ` (${value})`;
    });
  }
  const scopeLabel = filters.kind === 'receipt'
    ? 'Nur Belege'
    : filters.kind === 'booking'
      ? 'Nur Buchungen'
      : 'Belege und Buchungen';
  const footer = `${filtered.length} Einträge · ${scopeLabel}`;
  grammarHandle?.setFooter?.(footer);
  if (!grammarHandle) {
    const footerNode = pane.querySelector('[data-pg-footer]');
    if (footerNode) footerNode.textContent = footer;
  }
}

function setSelectedBookingRow(selectedKey) {
  state.els.bookingList?.querySelectorAll('[data-booking-key]').forEach((row) => {
    const selected = row.dataset.bookingKey === selectedKey;
    row.classList.toggle('is-selected', selected);
    row.setAttribute('aria-selected', String(selected));
  });
}

function selectBookingRecord(key) {
  const record = bookingRecords().find((item) => item.key === key);
  if (!record) return;
  state.selectedBookingKey = key;
  setSelectedBookingRow(key);
  if (record.kind === 'receipt') {
    switchView('receipts');
    selectReceipt(record.id);
  } else {
    switchView('journal');
    selectJournalEntry(record.id);
  }
}

function hasEvidenceSelection() {
  return Boolean(state.selectedBookingKey);
}

function updateEvidencePaneVisibility() {
  const hasSelection = hasEvidenceSelection();
  const visible = hasSelection && !state.evidenceUserCollapsed;
  state.els.root?.classList.toggle('is-evidence-hidden', !visible);
  if (state.els.evidenceToggle) {
    state.els.evidenceToggle.hidden = !hasSelection;
    state.els.evidenceToggle.setAttribute('aria-pressed', String(visible));
    state.els.evidenceToggle.setAttribute('aria-label', visible ? 'Evidenz ausblenden' : 'Evidenz einblenden');
    state.els.evidenceToggle.setAttribute('title', visible ? 'Evidenz ausblenden' : 'Evidenz einblenden');
  }
}

function renderJournalEvidence(entry) {
  const receipt = entry?.ref_type === 'receipt'
    ? state.receipts.find((candidate) => candidate.id === entry.ref_id)
    : null;
  if (receipt) {
    renderOcrInvoicePreview(receipt);
    return;
  }
  const container = state.els.ocrPreviewContainer;
  if (!container || !entry) return;
  const lines = state.journalEntryLines.filter((line) => line.journal_entry_id === entry.id);
  container.innerHTML = `
    <section class="ctox-card fibu-evidence-summary">
      <header><h3>${escapeHtml(entry.number || 'Buchungsentwurf')}</h3></header>
      <div class="ctox-card-body">
        <dl class="ctox-fields">
          <div><dt>Buchungstext</dt><dd>${escapeHtml(entry.narration || '—')}</dd></div>
          <div><dt>Datum</dt><dd>${escapeHtml(formatDateDe(entry.posting_date) || '—')}</dd></div>
          <div><dt>Prüfstatus</dt><dd>${entry.posted_at ? 'GoBD-festgeschrieben' : 'Entwurf'}</dd></div>
          <div><dt>Evidenz</dt><dd>${lines.length} Buchungszeilen · kein Belegdokument verknüpft</dd></div>
        </dl>
      </div>
    </section>`;
}

function downloadJson(filename, value) {
  const blob = new Blob([JSON.stringify(value, null, 2)], { type: 'application/json;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = filename;
  link.style.display = 'none';
  (state.root || document.documentElement).appendChild(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 0);
}

function exportBookingRecords() {
  downloadJson(`buchhaltung-${new Date().toISOString().slice(0, 10)}.json`, {
    format: 'ctox-buchhaltung-records-v1',
    exported_at: new Date().toISOString(),
    journal_entries: state.journalEntries,
    journal_entry_lines: state.journalEntryLines,
    receipts: state.receipts,
  });
}

async function insertMissingRecords(collectionName, records) {
  if (!Array.isArray(records) || !canWriteCollection(collectionName)) return 0;
  const collection = fibuCollection(collectionName);
  if (!collection) return 0;
  let inserted = 0;
  for (const record of records) {
    if (!record?.id) continue;
    const existing = await collection.byId(record.id);
    if (existing) continue;
    await collection.add(record);
    inserted += 1;
  }
  return inserted;
}

async function importBookingRecords(event) {
  const input = event.currentTarget;
  const file = input?.files?.[0];
  if (!file) return;
  try {
    const payload = JSON.parse(await file.text());
    if (payload?.format !== 'ctox-buchhaltung-records-v1') throw new Error('Unbekanntes Buchhaltungsformat.');
    const inserted = await Promise.all([
      insertMissingRecords('accounting_journal_entries', payload.journal_entries),
      insertMissingRecords('accounting_journal_entry_lines', payload.journal_entry_lines),
      insertMissingRecords('accounting_receipts', payload.receipts),
    ]);
    await loadAllFibuData();
    renderBookingList();
    renderActiveView();
    fibuNotice('import.done', `${inserted.reduce((sum, value) => sum + value, 0)} Datensätze importiert.`);
  } catch (error) {
    fibuNotice('import.failed', `Import fehlgeschlagen: ${error?.message || error}`, 'error');
  } finally {
    input.value = '';
  }
}

// =========================================================================
// 📊 Rendering View: SKR Accounts Explorer
// =========================================================================
function renderAccountsList(acctsArray) {
  const container = state.els.accountsList;
  if (!container) return;
  updateListCount(state.els.accountsCount, acctsArray.length, state.accounts.length, 'Konten');

  if (acctsArray.length === 0) {
    // Leere Liste rendert nichts (R11); kein Lade- oder Leertext.
    container.innerHTML = '';
    return;
  }

  // Sort accounts hierarchical
  const sorted = [...acctsArray].sort((a, b) => a.code.localeCompare(b.code));

  let html = '';
  sorted.forEach(acct => {
    const isGroup = acct.is_group;
    const formatSoll = formatCents(acct.debit_saldo);
    const formatHaben = formatCents(acct.credit_saldo);
    const taxRate = acct.tax_rate_id ? (acct.tax_rate_id === 'DE_19' ? '19% Vor/USt' : '7% Vor/USt') : '—';

    html += `
      <tr class="${isGroup ? 'group-row' : 'regular-row'} ${state.selectedAccountId === acct.id ? 'is-selected' : ''}" data-account-click-id="${acct.id}" data-context-record-id="${escapeHtml(acct.id)}" data-context-record-type="accounting_account" data-context-label="${escapeHtml(`${acct.code} ${acct.name}`)}" aria-selected="${state.selectedAccountId === acct.id ? 'true' : 'false'}" tabindex="0">
        <td><span class="fibu-mono">${acct.code}</span></td>
        <td>
          <span style="padding-left: ${acct.parent_id ? '16px' : '0px'};">
            ${isGroup ? '📂' : '📄'} ${acct.name}
          </span>
        </td>
        <td><span class="ctox-badge">${{
          bank: 'Bank',
          cash: 'Kasse',
          receivable: 'Debitoren (Forderung)',
          payable: 'Kreditoren (Verbindlichkeit)',
          expense: 'Aufwand',
          revenue: 'Erlöse',
          tax: 'Steuern',
          regular: 'Standard',
          group: 'Gruppe',
          fixed_asset: 'Anlagevermögen',
          equity: 'Eigenkapital',
          provisions: 'Rückstellungen'
        }[acct.account_type] || acct.account_type}</span></td>
        <td><span style="font-size:11px; color: var(--muted);">${taxRate}</span></td>
        <td class="is-num" style="font-weight: ${isGroup ? '700' : 'normal'};">${formatSoll}</td>
        <td class="is-num" style="font-weight: ${isGroup ? '700' : 'normal'};">${formatHaben}</td>
      </tr>
    `;
  });

  container.innerHTML = html;

  // Bind click details in bottom drawer
  container.querySelectorAll('tr').forEach(tr => {
    tr.addEventListener('click', () => {
      const id = tr.getAttribute('data-account-click-id');
      selectAccount(id);
    });
    tr.addEventListener('keydown', (event) => {
      if (event.key !== 'Enter' && event.key !== ' ') return;
      event.preventDefault();
      selectAccount(tr.getAttribute('data-account-click-id'));
    });
  });
}

function selectAccount(id) {
  state.selectedAccountId = id;
  setSelectedRow(state.els.accountsList, 'data-account-click-id', id);
  openAccountLedgerDrawer(id);
}

function filterAccountsView(query) {
  if (!query) {
    renderAccountsList(state.accounts);
    return;
  }
  const filtered = state.accounts.filter(a =>
    a.code.includes(query) ||
    a.name.toLowerCase().includes(query) ||
    a.account_type.toLowerCase().includes(query)
  );
  renderAccountsList(filtered);
}

async function forceReInitSKR() {
  const db = fibuDb(['accounting_accounts']);
  if (!db || !canWriteCollection('accounting_accounts')) return;

  // Bestätigung im App-Fenster (api.shell.dialog, Form OFFEN-A); destruktiv → default:false.
  const confirmed = await state.api.shell.dialog({
    kind: 'confirm',
    tone: 'danger',
    title: `${state.skrName} neu initialisieren`,
    // v4-allow: text (Pflichtangabe vor destruktiver Aktion)
    text: `Alle Konten von ${state.skrName} werden aus der Vorlage neu aufgebaut. Buchungen bleiben, können aber auf geänderte Konten verweisen.`,
    confirm_label: 'Kontenrahmen neu initialisieren',
    cancel_label: 'Abbrechen',
    require_text: state.skrName,
    default: false,
  });

  if (confirmed === true) {
    // Delete existing
    const allAccountDocs = await db.accounting_accounts.rows();
    const existing = allAccountDocs
      .filter((doc) => accountMatchesCurrentSkr(normalizeAccountRecord(docToRecord(doc))));
    for (const doc of existing) {
      await db.accounting_accounts.drop(doc.id);
    }
    await importTemplateToDb(db, state.skrName);
    await ensureRequiredAccountsExist();
    await loadAllFibuData();
    renderActiveView();
    fibuNotice('skr.init', t('initSuccess'));
  }
}

// =========================================================================
// 📖 Rendering View: Journal & Ledger
// =========================================================================
function renderJournalList() {
  renderJournalRows(state.journalEntries);
}

function renderJournalRows(entries) {
  const container = state.els.journalList;
  if (!container) return;
  updateListCount(state.els.journalCount, entries.length, state.journalEntries.length, 'Buchungen');

  if (entries.length === 0) {
    // Leere Liste rendert nichts (R11); kein Lade- oder Leertext.
    container.innerHTML = '';
    return;
  }

  let html = '';
  entries.forEach(entry => {
    const isPosted = !!entry.posted_at;
    const isStorno = !!entry.reversed_by_id || entry.type === 'storno';

    // Sum total debit lines
    const lines = state.journalEntryLines.filter(l => l.journal_entry_id === entry.id);
    const totalDebitCents = lines.reduce((acc, curr) => acc + (curr.debit || 0), 0);
    const formattedTotal = formatCents(totalDebitCents);

    // Link to receipt filename if present
    const receipt = entry.ref_type === 'receipt' ? state.receipts.find(r => r.id === entry.ref_id) : null;
    const receiptLabel = receipt ? `📄 ${receipt.filename}` : '—';

    html += `
      <tr class="${isStorno ? 'fibu-storno-indicator' : ''} ${state.selectedEntryId === entry.id ? 'is-selected' : ''}" data-entry-click-id="${entry.id}" data-context-record-id="${escapeHtml(entry.id)}" data-context-record-type="accounting_journal_entry" data-context-label="${escapeHtml(entry.number || entry.narration || entry.id)}" aria-selected="${state.selectedEntryId === entry.id ? 'true' : 'false'}" tabindex="0">
        <td><span class="fibu-mono">${formatDateDe(entry.posting_date)}</span></td>
        <td><span class="fibu-mono">${entry.number || 'Entwurf'}</span></td>
        <td>${escapeHtml(entry.narration || 'Unbenannte Buchung')}</td>
        <td><span style="font-size:11px; color: var(--muted);">${receiptLabel}</span></td>
        <td class="is-num" style="font-weight: 600;">${formattedTotal}</td>
        <td class="is-center">
          <span class="ctox-badge ${isPosted ? 'is-success' : ''}">
            ${isPosted ? (isStorno ? 'Storniert' : 'Posted 🔒') : 'Entwurf'}
          </span>
        </td>
        <td class="is-center" onclick="event.stopPropagation();">
          ${isPosted && !isStorno ? `<button class="ctox-button is-danger ctox-button--sm" onclick="triggerStorno('${entry.id}')">Storno</button>` : '—'}
        </td>
      </tr>
    `;
  });

  container.innerHTML = html;

  container.querySelectorAll('tr').forEach(tr => {
    tr.addEventListener('click', () => {
      const id = tr.getAttribute('data-entry-click-id');
      selectJournalEntry(id);
    });
    tr.addEventListener('keydown', (event) => {
      if (event.key !== 'Enter' && event.key !== ' ') return;
      event.preventDefault();
      selectJournalEntry(tr.getAttribute('data-entry-click-id'));
    });
  });
}

function selectJournalEntry(id) {
  state.selectedEntryId = id;
  state.selectedBookingKey = `booking:${id}`;
  state.evidenceUserCollapsed = false;
  setSelectedBookingRow(state.selectedBookingKey);
  setSelectedRow(state.els.journalList, 'data-entry-click-id', id);
  const entry = state.journalEntries.find((candidate) => candidate.id === id);
  renderJournalEvidence(entry);
  updateEvidencePaneVisibility();
  openJournalEntryDrawer(id);
}

function filterJournalView(query) {
  if (!query) {
    renderJournalRows(state.journalEntries);
    return;
  }
  const filtered = state.journalEntries.filter(e =>
    (e.narration && e.narration.toLowerCase().includes(query)) ||
    (e.number && e.number.toLowerCase().includes(query)) ||
    (e.posting_date && e.posting_date.includes(query))
  );

  renderJournalRows(filtered);
}

// =========================================================================
// 📁 Rendering View: Receipts Inbox
// =========================================================================
function renderReceiptsList() {
  const container = state.els.receiptsList;
  if (!container) return;

  if (state.receipts.length === 0) {
    // Leere Liste rendert nichts (R11); kein Lade- oder Leertext.
    container.innerHTML = '';
    return;
  }

  let html = '';
  state.receipts.forEach(r => {
    const status = r.status || 'draft';
    const hasSuggested = !!r.suggested_account_id;
    const suggestedAcct = hasSuggested ? state.accounts.find(a => a.id === r.suggested_account_id) : null;
    const suggestedLabel = suggestedAcct ? `${suggestedAcct.code} ${suggestedAcct.name}` : '—';

    html += `
      <tr class="${state.selectedReceiptId === r.id ? 'is-selected' : ''}" data-receipt-click-id="${r.id}" data-context-record-id="${escapeHtml(r.id)}" data-context-record-type="accounting_receipt" data-context-label="${escapeHtml(r.filename || r.invoice_number || r.id)}" aria-selected="${state.selectedReceiptId === r.id ? 'true' : 'false'}" tabindex="0">
        <td style="font-weight: 500; color: var(--text-strong);">📄 ${escapeHtml(r.filename)}</td>
        <td>${escapeHtml(r.supplier_name || '—')}</td>
        <td><span class="fibu-mono">${formatDateDe(r.invoice_date) || '—'}</span></td>
        <td class="is-num">${formatAmountOrDash(r.net_amount)}</td>
        <td class="is-num">${hasAmount(r.tax_amount) && hasAmount(r.net_amount) && Number(r.net_amount) ? `${Math.round((Number(r.tax_amount) / Number(r.net_amount)) * 100)}%` : '—'}</td>
        <td class="is-num" style="font-weight: 600;">${formatAmountOrDash(r.gross_amount)}</td>
        <td><span style="font-size:11.5px; color: var(--muted);">${suggestedLabel}</span></td>
        <td class="is-center">
          <span class="ctox-badge ${status === 'posted' ? 'is-success' : ''}">${status === 'posted' ? 'Gebucht' : 'Offen'}</span>
        </td>
        <td class="is-center" onclick="event.stopPropagation();">
          ${status !== 'posted' ? `<button class="ctox-button is-primary ctox-button--sm" onclick="postReceiptDirectly('${r.id}')">Buchen</button>` : '🔒 Fibu'}
        </td>
      </tr>
    `;
  });

  container.innerHTML = html;

  container.querySelectorAll('tr').forEach(tr => {
    tr.addEventListener('click', () => {
      const id = tr.getAttribute('data-receipt-click-id');
      selectReceipt(id);
    });
    tr.addEventListener('keydown', (event) => {
      if (event.key !== 'Enter' && event.key !== ' ') return;
      event.preventDefault();
      selectReceipt(tr.getAttribute('data-receipt-click-id'));
    });
  });
}

function selectReceipt(id) {
  state.selectedReceiptId = id;
  state.selectedBookingKey = `receipt:${id}`;
  state.evidenceUserCollapsed = false;
  const receipt = state.receipts.find(r => r.id === id);
  if (!receipt) return;

  // Selection is an in-place flip in both existing lists; neither list is rebuilt.
  setSelectedBookingRow(state.selectedBookingKey);
  setSelectedRow(state.els.receiptsList, 'data-receipt-click-id', id);

  renderOcrInvoicePreview(receipt);
  updateEvidencePaneVisibility();
  openReceiptDrawer(receipt);
}

// =========================================================================
// Belegkontierung (BUCH-3): Felder untereinander, Speichern je Änderung per api.patch. Konfliktprüfung über die
// bestätigte Revision (E47 _rev, E35 expected_revision): beim Öffnen gemerkt, jedes Speichern sendet sie mit; der Core
// lehnt eine veraltete Revision mit revision_conflict ab, dann steht der Grund in der Zone. Eigene Speichervorgänge je
// Beleg laufen nacheinander, damit der nächste die vom Core bestätigte neue Revision trägt (BUCH-REV).
// =========================================================================
const RECEIPT_TAX_RATES = Object.freeze([19, 7, 0]);
const RECEIPT_TEXT_FIELDS = Object.freeze(['supplier_name', 'invoice_number', 'invoice_date', 'suggested_account_id']);
const RECEIPT_AMOUNT_FIELDS = Object.freeze(['net_amount', 'tax_amount', 'gross_amount']);

function parseEuroCents(text) {
  const raw = String(text ?? '').replace(/[\s€]/g, '');
  if (!raw) return null;
  const norm = raw.includes(',') ? raw.replace(/\./g, '').replace(',', '.') : raw;
  const n = Number(norm);
  return Number.isFinite(n) ? Math.round(n * 100) : NaN;
}
function centsInputValue(cents) {
  return hasAmount(cents) ? (Number(cents) / 100).toFixed(2).replace('.', ',') : '';
}
function suggestedTax(net, rate) {
  return hasAmount(net) && hasAmount(rate) ? Math.round((Number(net) * Number(rate)) / 100) : null;
}

function receiptFormMarkup(receipt, isPosted) {
  const dis = isPosted ? ' disabled' : '';
  const accounts = (state.accounts || []).filter((a) => !a.is_group && a.account_type !== 'group');
  const accountOptions = accounts.map((a) => `<option value="${escapeHtml(a.id)}"${a.id === receipt.suggested_account_id ? ' selected' : ''}>${escapeHtml(`${a.code} ${a.name}`)}</option>`).join('');
  const rate = hasAmount(receipt.tax_rate) ? Number(receipt.tax_rate) : null;
  const rateOptions = RECEIPT_TAX_RATES.map((r) => `<option value="${r}"${rate === r ? ' selected' : ''}>${r} %</option>`).join('');
  const row = (label, control) => `<label class="fibu-receipt-row"><span class="ctox-field-label">${label}</span>${control}</label>`;
  const text = (name, value, extra = '') => `<input class="ctox-input" name="${name}" value="${escapeHtml(value || '')}" autocomplete="off"${extra}${dis} />`;
  const amount = (name) => `<input class="ctox-input is-num" name="${name}" inputmode="decimal" value="${centsInputValue(receipt[name])}" autocomplete="off"${dis} />`;
  return `
      <form class="fibu-receipt-form" data-receipt-form data-receipt-id="${escapeHtml(receipt.id)}" data-base-rev="${escapeHtml(receipt._rev ?? '')}" data-base-updated="${Number(receipt.updated_at_ms) || 0}" novalidate>
        ${row('Lieferant', text('supplier_name', receipt.supplier_name))}
        ${row('Rechnungsnummer', text('invoice_number', receipt.invoice_number))}
        ${row('Belegdatum', `<input class="ctox-input" type="date" name="invoice_date" value="${escapeHtml(receipt.invoice_date || '')}"${dis} />`)}
        ${row('Konto', `<select class="ctox-select" name="suggested_account_id"${dis}><option value=""${receipt.suggested_account_id ? '' : ' selected'}>—</option>${accountOptions}</select>`)}
        ${row('Netto', amount('net_amount'))}
        ${row('Steuersatz', `<select class="ctox-select" name="tax_rate"${dis}><option value=""${rate == null ? ' selected' : ''}>—</option>${rateOptions}</select>`)}
        ${row('Steuer', amount('tax_amount'))}
        ${row('Brutto', amount('gross_amount'))}
      </form>`;
}

function receiptFormFor(receiptId) {
  const form = state.els.drawerContent?.querySelector('[data-receipt-form]');
  return form && form.dataset.receiptId === String(receiptId) ? form : null;
}

// Formularwerte im Speicherformat: Texte getrimmt ('' = leer), Beträge in Cent (null = leer, NaN = ungültig).
function readReceiptForm(form) {
  const out = {};
  for (const name of RECEIPT_TEXT_FIELDS) out[name] = String(form.elements[name]?.value ?? '').trim();
  const rate = form.elements.tax_rate?.value;
  out.tax_rate = rate === '' || rate == null ? null : Number(rate);
  for (const name of RECEIPT_AMOUNT_FIELDS) out[name] = parseEuroCents(form.elements[name]?.value);
  return out;
}

// Steuer und Brutto als Vorschlag, solange der Nutzer sie nicht selbst geändert hat (data-manual).
function applyReceiptSuggestions(form) {
  const v = readReceiptForm(form);
  const tax = form.elements.tax_amount;
  const gross = form.elements.gross_amount;
  if (tax && tax.dataset.manual !== '1') {
    const t = suggestedTax(v.net_amount, v.tax_rate);
    tax.value = t == null ? tax.value : centsInputValue(t);
  }
  const taxCents = parseEuroCents(tax?.value);
  if (gross && gross.dataset.manual !== '1' && hasAmount(v.net_amount) && hasAmount(taxCents)) {
    gross.value = centsInputValue(Number(v.net_amount) + Number(taxCents));
  }
}

function bindReceiptForm(form, receipt) {
  if (!form) return;
  // Gespeicherte Abweichungen gelten als Nutzereintrag und bleiben stehen.
  const rate = hasAmount(receipt.tax_rate) ? Number(receipt.tax_rate) : null;
  if (hasAmount(receipt.tax_amount) && receipt.tax_amount !== suggestedTax(receipt.net_amount, rate)) form.elements.tax_amount.dataset.manual = '1';
  if (hasAmount(receipt.gross_amount) && !(hasAmount(receipt.net_amount) && hasAmount(receipt.tax_amount) && Number(receipt.net_amount) + Number(receipt.tax_amount) === Number(receipt.gross_amount))) {
    form.elements.gross_amount.dataset.manual = '1';
  }
  form.addEventListener('submit', (event) => event.preventDefault());
  form.addEventListener('input', (event) => {
    form.dataset.dirty = '1';
    const name = event.target?.name;
    if (name === 'tax_amount' || name === 'gross_amount') event.target.dataset.manual = event.target.value.trim() ? '1' : '';
    if (name === 'net_amount' || name === 'tax_amount') applyReceiptSuggestions(form);
    updateReceiptDrawerOptions(form.dataset.receiptId);
  });
  form.addEventListener('change', (event) => {
    if (event.target?.name === 'tax_rate') applyReceiptSuggestions(form);
    form.dataset.dirty = '1';
    updateReceiptDrawerOptions(form.dataset.receiptId);
    void saveReceiptForm(form);
  });
}

// Speichert die geänderten Felder. false = nicht gespeichert (Konflikt, ungültig, Fehler); Grund in der Zone.
const receiptSaveChains = new Map();
// Je Beleg nacheinander: der nächste Speichervorgang startet erst, wenn der vorige vom Core bestätigt (neue _rev) oder
// abgelehnt ist. Offen bleibende Eingaben stehen solange im Formular (dirty), Live-Daten überschreiben sie nicht.
function saveReceiptForm(form) {
  const id = form?.dataset.receiptId;
  const prev = receiptSaveChains.get(id) || Promise.resolve();
  const next = prev.catch(() => {}).then(() => saveReceiptFormNow(form));
  receiptSaveChains.set(id, next);
  return next;
}

function receiptConflict(current, id) {
  fibuNotice('receipt.conflict', `${current?.filename || id}: Beleg wurde inzwischen geändert. Eingabe nicht gespeichert, Beleg neu öffnen.`, 'error');
  return false;
}

async function saveReceiptFormNow(form) {
  const api = state.api;
  const id = form?.dataset.receiptId;
  if (!api || !id) return false;
  const current = api.get('accounting_receipts', id).get();
  if (!current || current.status === 'posted') return false;
  const baseRev = form.dataset.baseRev || '';
  // Bestätigte Revision weicht ab: jemand anderes hat den Beleg geändert (ohne Schreibversuch).
  if (baseRev && current._rev && current._rev !== baseRev) return receiptConflict(current, id);
  // Kern ohne E47 (_rev fehlt): Rückfall auf updated_at_ms als Revision, wie vor BUCH-REV.
  const legacyRevision = current._rev === undefined;
  if (legacyRevision && (Number(current.updated_at_ms) || 0) !== (Number(form.dataset.baseUpdated) || 0)) return receiptConflict(current, id);
  const values = readReceiptForm(form);
  if (RECEIPT_AMOUNT_FIELDS.some((name) => Number.isNaN(values[name]))) {
    fibuNotice('receipt.invalid_amount', `${current.filename || id}: Betrag ungültig.`, 'warn');
    return false;
  }
  api.status.clear('buchhaltung.receipt.invalid_amount');
  const patch = {};
  for (const [name, value] of Object.entries(values)) {
    const empty = value === '' || value == null;
    const had = current[name] !== undefined && current[name] !== null && current[name] !== '';
    if (empty ? had : current[name] !== value) patch[name] = empty ? null : value;
  }
  if (!Object.keys(patch).length) { delete form.dataset.dirty; return true; }
  patch.updated_at_ms = Math.max(Date.now(), (Number(current.updated_at_ms) || 0) + 1);
  if (legacyRevision) form.dataset.baseUpdated = String(patch.updated_at_ms);
  let result;
  try {
    const handle = api.patch('accounting_receipts', id, patch, baseRev ? { expected_revision: baseRev } : undefined);
    await handle.local;
    result = await handle.result;
  } catch (error) {
    fibuNotice('receipt.save_failed', `${current.filename || id}: Speichern fehlgeschlagen (${error?.code || error?.message || error})`, 'error');
    return false;
  }
  if (result?.status === 'rejected') {
    if (result.code === 'revision_conflict') return receiptConflict(current, id);
    fibuNotice('receipt.save_rejected', `${current.filename || id}: Speichern abgelehnt (${result.reason || result.code || 'rejected'})`, 'error');
    return false;
  }
  if (result?.rev) form.dataset.baseRev = result.rev;
  delete form.dataset.dirty;
  api.status.clear('buchhaltung.receipt.conflict');
  api.status.clear('buchhaltung.receipt.save_failed');
  return true;
}

// Live-Daten: ein unverändertes Formular übernimmt den neuen Stand (Werte + Revision); ein Formular mit offenen
// Eingaben bleibt unangetastet, damit nichts überschrieben wird (Konflikt meldet dann das Speichern).
function syncOpenReceiptForm() {
  const form = state.els.drawerContent?.querySelector('[data-receipt-form]');
  if (!form || form.dataset.dirty === '1') return;
  const doc = state.api?.get('accounting_receipts', form.dataset.receiptId).get();
  // Nur bestätigte fremde Stände übernehmen (eigene ausstehende Änderung: _pending, Revision noch die alte); ohne E47
  // (_rev fehlt) wie früher über updated_at_ms.
  if (!doc || doc._pending) return;
  if (doc._rev === undefined ? (Number(doc.updated_at_ms) || 0) === (Number(form.dataset.baseUpdated) || 0)
    : (doc._rev ?? '') === (form.dataset.baseRev || '')) return;
  if (form.contains(document.activeElement)) return;
  for (const name of RECEIPT_TEXT_FIELDS) if (form.elements[name]) form.elements[name].value = doc[name] || '';
  if (form.elements.tax_rate) form.elements.tax_rate.value = hasAmount(doc.tax_rate) ? String(Number(doc.tax_rate)) : '';
  for (const name of RECEIPT_AMOUNT_FIELDS) if (form.elements[name]) form.elements[name].value = centsInputValue(doc[name]);
  form.dataset.baseRev = doc._rev ?? '';
  form.dataset.baseUpdated = String(Number(doc.updated_at_ms) || 0);
}

// Buchen nur mit erfassten Angaben – ein leerer Beleg ergäbe eine Buchung über 0 € ohne Datum.
async function markReceiptFileRejected(receiptId, filename, reason) {
  const text = String(reason || 'rejected');
  fibuNotice(`receipt.upload_rejected.${receiptId}`, `${filename}: Datei vom Core abgelehnt (${text})`, 'error');
  const db = fibuDb();
  if (!db) return;
  try {
    await db.accounting_receipts.change(receiptId, { file_state: 'rejected', file_error: text, updated_at_ms: Date.now() });
  } catch (error) {
    fibuNotice(`receipt.upload_rejected.${receiptId}`, `${filename}: Datei vom Core abgelehnt (${text}); Markierung nicht gespeichert (${error?.code || error?.message || error})`, 'error');
  }
}

function receiptIsComplete(receipt) {
  if (receipt?.file_state === 'rejected') return false; // abgelehnte Datei: nicht buchbar
  return Boolean(receipt?.invoice_date) && Boolean(receipt?.suggested_account_id)
    && hasAmount(receipt.net_amount) && hasAmount(receipt.tax_amount) && hasAmount(receipt.gross_amount)
    && Number(receipt.net_amount) + Number(receipt.tax_amount) === Number(receipt.gross_amount);
}

// Beleg-Evidenz: nur gespeicherte Angaben, keine nachgebildete Rechnung (keine Belegerkennung vorhanden).
function hasAmount(value) {
  return value !== undefined && value !== null && value !== '' && Number.isFinite(Number(value));
}
function formatAmountOrDash(value) {
  return hasAmount(value) ? formatCents(Number(value)) : '—';
}
function formatFileSize(bytes) {
  const n = Number(bytes);
  if (!Number.isFinite(n) || n <= 0) return '';
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toLocaleString('de-DE', { maximumFractionDigits: 1 })} KB`;
  return `${(n / (1024 * 1024)).toLocaleString('de-DE', { maximumFractionDigits: 1 })} MB`;
}

function renderOcrInvoicePreview(receipt) {
  const container = state.els.ocrPreviewContainer;
  if (!container) return;
  const field = (label, value) => `<div><dt>${escapeHtml(label)}</dt><dd>${escapeHtml(value || '—')}</dd></div>`;
  const uploaded = Number(receipt.uploaded_at_ms) > 0 ? new Date(Number(receipt.uploaded_at_ms)).toLocaleString('de-DE') : '';
  container.innerHTML = `
    <section class="ctox-card fibu-evidence-summary" data-receipt-evidence>
      <header><h3>${escapeHtml(receipt.filename || receipt.id)}</h3></header>
      <div class="ctox-card-body">
        <dl class="ctox-fields">
          ${field('Dateityp', receipt.mime_type)}
          ${field('Größe', formatFileSize(receipt.file_size))}
          ${field('Hochgeladen', uploaded)}
          ${field('Lieferant', receipt.supplier_name)}
          ${field('Rechnungsnummer', receipt.invoice_number)}
          ${field('Belegdatum', formatDateDe(receipt.invoice_date))}
          ${field('Netto', formatAmountOrDash(receipt.net_amount))}
          ${field('Steuer', formatAmountOrDash(receipt.tax_amount))}
          ${field('Brutto', formatAmountOrDash(receipt.gross_amount))}
        </dl>
      </div>
    </section>
    <section class="ctox-card fibu-receipt-file" data-receipt-file></section>
    <div class="ctox-card ocr-raw-meta-badge">
      <div class="ctox-card-body">
        <span class="ctox-field-label">Datei-Referenz</span>
        <div class="ocr-archive-path-wrapper">
          <code class="ocr-archive-path" title="${escapeHtml(receipt.file_storage_url || '')}">${truncateArchiveUrl(receipt.file_storage_url || '')}</code>
          <button type="button" class="ctox-pane-icon" data-short-de="Kopieren" onclick="window.fibuCopyArchivePath('${escapeHtml(receipt.file_storage_url || '')}')" aria-label="Archivpfad kopieren" title="Archivpfad kopieren">📋</button>
        </div>
      </div>
    </div>
  `;
  renderReceiptFile(container.querySelector('[data-receipt-file]'), receipt);
}

// Datei des Belegs aus api.files: PDF und Bild als Vorschau, sonst Name + Öffnen (file-viewer).
const FILE_REF_RE = /^[0-9a-f]{64}$/;
function revokeReceiptFileUrl() {
  if (state.receiptFileUrl) URL.revokeObjectURL(state.receiptFileUrl);
  state.receiptFileUrl = null;
}
function renderReceiptFile(host, receipt) {
  revokeReceiptFileUrl();
  if (!host) return;
  const ref = String(receipt.file_storage_url || '');
  if (!FILE_REF_RE.test(ref) || !state.api?.files) { host.remove(); return; }
  const token = (state.receiptFileToken = (state.receiptFileToken || 0) + 1);
  const name = receipt.filename || receipt.id;
  const mime = String(receipt.mime_type || '');
  const openButton = `<button type="button" class="ctox-button ctox-button--sm" data-receipt-file-open>Öffnen</button>`;
  host.innerHTML = `<header><h3>${escapeHtml(name)}</h3>${openButton}</header>`;
  host.querySelector('[data-receipt-file-open]')?.addEventListener('click', () => {
    state.api.shell?.navigate?.('file-viewer', { ref, file_id: receipt.file_id || '', name, mime, size: receipt.file_size });
  });
  if (!/^application\/pdf$|^image\//.test(mime)) return;
  Promise.resolve().then(() => state.api.files.get(ref)).then((blob) => {
    if (token !== state.receiptFileToken || !host.isConnected || !blob) return;
    state.api.status.clear('buchhaltung.receipt.file_unavailable');
    const url = URL.createObjectURL(blob);
    state.receiptFileUrl = url;
    const body = document.createElement('div');
    body.className = 'fibu-receipt-file-view';
    if (mime.startsWith('image/')) {
      const img = document.createElement('img');
      img.src = url;
      img.alt = name;
      body.append(img);
    } else {
      const frame = document.createElement('iframe');
      frame.src = url;
      frame.title = name;
      body.append(frame);
    }
    host.append(body);
  }, (error) => {
    if (token !== state.receiptFileToken) return;
    fibuNotice('receipt.file_unavailable', `${name}: Datei nicht geladen (${error?.code || error?.message || error})`, 'warn');
  });
}

// =========================================================================
// 🏦 Rendering View: Bank Reconciliation
// =========================================================================
function renderBankingList() {
  const container = state.els.bankingList;
  if (!container) return;

  if (state.bankStatementLines.length === 0) {
    // Leere Liste rendert nichts (R11); kein Lade- oder Leertext.
    container.innerHTML = '';
    return;
  }

  let html = '';
  state.bankStatementLines.forEach(line => {
    const isMatched = line.match_status === 'matched';
    const isProposed = line.match_status === 'proposed';
    const amountVal = line.amount || 0;
    const formattedAmount = formatCents(Math.abs(amountVal));
    const isCredit = amountVal >= 0;

    // Find matching suggested receipt
    const matchedReceipt = state.receipts.find(r => r.gross_amount === Math.abs(amountVal) && r.status !== 'posted');
    const proposedLabel = matchedReceipt ? `📄 Vorschlag: ${matchedReceipt.filename}` : 'Keine Belegübereinstimmung';

    html += `
      <tr class="${state.selectedBankLineId === line.id ? 'is-selected' : ''}" data-bankline-click-id="${line.id}" data-context-record-id="${escapeHtml(line.id)}" data-context-record-type="accounting_bank_statement_line" data-context-label="${escapeHtml(line.counterparty_name || line.narration || line.id)}" aria-selected="${state.selectedBankLineId === line.id ? 'true' : 'false'}" tabindex="0">
        <td><span class="fibu-mono">${line.value_date}</span></td>
        <td>
          <div style="font-weight:600; color:var(--text-strong);">${escapeHtml(line.counterparty_name || 'Unbekannter Empfänger')}</div>
          <div style="font-size:10.5px; color: var(--muted); font-family:monospace;">${line.counterparty_iban || '—'}</div>
        </td>
        <td style="font-size:12px; max-width: 250px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
          ${escapeHtml(line.narration || '')}
        </td>
        <td class="is-num ${isCredit ? 'fibu-text-debit' : 'fibu-text-credit'}">
          ${isCredit ? '+' : '-'}${formattedAmount}
        </td>
        <td>
          <span style="font-size:11px; font-style:italic; color:${matchedReceipt ? 'var(--accent)' : 'var(--muted)'};">
            ${proposedLabel}
          </span>
        </td>
        <td class="is-center">
          <span class="ctox-badge ${isMatched ? 'is-success' : (isProposed ? 'is-warning' : '')}">
            ${line.match_status}
          </span>
        </td>
        <td class="is-center" onclick="event.stopPropagation();">
          ${!isMatched && matchedReceipt ? `<button class="ctox-button is-primary ctox-button--sm" onclick="matchBankLineDirectly('${line.id}', '${matchedReceipt.id}')">Einbuchen</button>` : (isMatched ? '🔒 Reconciled' : '—')}
        </td>
      </tr>
    `;
  });

  container.innerHTML = html;

  container.querySelectorAll('tr').forEach(tr => {
    tr.addEventListener('click', () => {
      const id = tr.getAttribute('data-bankline-click-id');
      selectBankLine(id);
    });
    tr.addEventListener('keydown', (event) => {
      if (event.key !== 'Enter' && event.key !== ' ') return;
      event.preventDefault();
      selectBankLine(tr.getAttribute('data-bankline-click-id'));
    });
  });
}

function selectBankLine(id) {
  state.selectedBankLineId = id;
  state.selectedBookingKey = `bank:${id}`;
  state.evidenceUserCollapsed = false;
  setSelectedBookingRow(state.selectedBookingKey);
  setSelectedRow(state.els.bankingList, 'data-bankline-click-id', id);
  const line = state.bankStatementLines.find((candidate) => candidate.id === id);
  const receipt = line
    ? state.receipts.find((candidate) => candidate.gross_amount === Math.abs(line.amount || 0))
    : null;
  if (receipt) renderOcrInvoicePreview(receipt);
  else if (state.els.ocrPreviewContainer) {
    state.els.ocrPreviewContainer.innerHTML = '<div class="ctox-empty">Keine Belegevidenz für diese Banktransaktion verknüpft.</div>';
  }
  updateEvidencePaneVisibility();
  openBankReconciliationDrawer(id);
}

// =========================================================================
// 📈 Rendering View: Financial Reports Workbench (HGB Bilanz, GuV, UStVA)
// =========================================================================
function renderReports() {
  if (state.activeReportTab === 'bilanz') {
    renderHgbBilanz();
  } else if (state.activeReportTab === 'guv') {
    renderHgbGuV();
  } else if (state.activeReportTab === 'ustva') {
    renderElsterUStVA();
  }
}

function renderHgbBilanz() {
  const aktivaTree = state.els.bilanzAktiva;
  const passivaTree = state.els.bilanzPassiva;
  if (!aktivaTree || !passivaTree) return;

  // Calculate total profit/loss from GuV to inject into Eigenkapital
  const operatingResult = getOperatingGuVResult();

  // 1. Gather Aktiva accounts
  const anlagevermoegen = state.accounts.find(a => a.code === '0100' || a.code === '0100'); // Group 0100
  const umlaufvermoegen = state.accounts.find(a => a.code === '1100' || a.code === '1300'); // Group 1100

  const anlageNet = (anlagevermoegen?.debit_saldo || 0) - (anlagevermoegen?.credit_saldo || 0);
  const umlaufNet = (umlaufvermoegen?.debit_saldo || 0) - (umlaufvermoegen?.credit_saldo || 0);
  const totalAktivaCents = anlageNet + umlaufNet;

  let aktivaHtml = `
    <div class="fibu-tree-item level-0">
      <div class="fibu-tree-row">
        <div class="fibu-tree-label-wrap">
          <span class="fibu-tree-toggle">▼</span>
          <span class="fibu-tree-label">A. Anlagevermögen</span>
        </div>
        <span class="fibu-tree-val">${formatCents(anlageNet)}</span>
      </div>
      <div class="fibu-tree-children">
  `;

  // Anlagevermögen sub-accounts
  const anlageSubs = state.accounts.filter(a => a.parent_id === anlagevermoegen?.id);
  anlageSubs.forEach(sub => {
    aktivaHtml += `
      <div class="fibu-tree-item level-1">
        <div class="fibu-tree-row">
          <div class="fibu-tree-label-wrap">
            <span class="fibu-tree-label">📄 ${sub.code} ${sub.name}</span>
          </div>
          <span class="fibu-tree-val">${formatCents(sub.netto_saldo)}</span>
        </div>
      </div>
    `;
  });

  aktivaHtml += `
      </div>
    </div>

    <div class="fibu-tree-item level-0" style="margin-top: 16px;">
      <div class="fibu-tree-row">
        <div class="fibu-tree-label-wrap">
          <span class="fibu-tree-toggle">▼</span>
          <span class="fibu-tree-label">B. Umlaufvermögen</span>
        </div>
        <span class="fibu-tree-val">${formatCents(umlaufNet)}</span>
      </div>
      <div class="fibu-tree-children">
  `;

  // Umlaufvermögen children
  const umlaufSubs = state.accounts.filter(a => a.parent_id === umlaufvermoegen?.id);
  umlaufSubs.forEach(sub => {
    aktivaHtml += `
      <div class="fibu-tree-item level-1">
        <div class="fibu-tree-row">
          <div class="fibu-tree-label-wrap">
            <span class="fibu-tree-label">📂 ${sub.code} ${sub.name}</span>
          </div>
          <span class="fibu-tree-val">${formatCents(sub.debit_saldo - sub.credit_saldo)}</span>
        </div>
        <div class="fibu-tree-children">
    `;

    // Add leaf accounts
    const leafs = state.accounts.filter(a => a.parent_id === sub.id);
    leafs.forEach(leaf => {
      aktivaHtml += `
        <div class="fibu-tree-row" style="padding-left:12px;">
          <span class="fibu-tree-label">📄 ${leaf.code} ${leaf.name}</span>
          <span class="fibu-mono">${formatCents(leaf.netto_saldo)}</span>
        </div>
      `;
    });

    aktivaHtml += `
        </div>
      </div>
    `;
  });

  aktivaHtml += `
      </div>
    </div>

    <div class="fibu-tree-row" style="margin-top: 30px; border-top: 2px solid var(--accent); font-weight:700;">
      <span>BILANZSUMME AKTIVA</span>
      <span class="fibu-tree-val" style="color:var(--accent); font-size:16px;">${formatCents(totalAktivaCents)}</span>
    </div>
  `;

  aktivaTree.innerHTML = aktivaHtml;

  // 2. Gather Passiva accounts
  const eigenkapital = state.accounts.find(a => a.code === '0800_G' || a.code === '2000'); // Group Eigenkapital
  const rueckstellungen = state.accounts.find(a => a.code === '0900' || a.code === '3000_G');
  const verbindlichkeiten = state.accounts.find(a => a.code === '1700' || a.code === '3500');

  const ekNet = (eigenkapital?.credit_saldo || 0) - (eigenkapital?.debit_saldo || 0) + operatingResult;
  const rNet = (rueckstellungen?.credit_saldo || 0) - (rueckstellungen?.debit_saldo || 0);
  const vNet = (verbindlichkeiten?.credit_saldo || 0) - (verbindlichkeiten?.debit_saldo || 0);
  const passivaTotalCents = ekNet + rNet + vNet;

  let passivaHtml = `
    <div class="fibu-tree-item level-0">
      <div class="fibu-tree-row">
        <div class="fibu-tree-label-wrap">
          <span class="fibu-tree-toggle">▼</span>
          <span class="fibu-tree-label">A. Eigenkapital</span>
        </div>
        <span class="fibu-tree-val">${formatCents(ekNet)}</span>
      </div>
      <div class="fibu-tree-children">
  `;

  // Show Gezeichnetes Kapital
  const ekSubs = state.accounts.filter(a => a.parent_id === eigenkapital?.id);
  ekSubs.forEach(sub => {
    passivaHtml += `
      <div class="fibu-tree-item level-1">
        <div class="fibu-tree-row">
          <div class="fibu-tree-label-wrap">
            <span class="fibu-tree-label">📄 ${sub.code} ${sub.name}</span>
          </div>
          <span class="fibu-tree-val">${formatCents(sub.credit_saldo - sub.debit_saldo)}</span>
        </div>
      </div>
    `;
  });

  // Inject profit/loss calculated from stateless GuV!
  passivaHtml += `
    <div class="fibu-tree-item level-1">
      <div class="fibu-tree-row" style="color:var(--accent);">
        <div class="fibu-tree-label-wrap">
          <span class="fibu-tree-label">📈 Jahresüberschuss / Fehlbetrag</span>
        </div>
        <span class="fibu-tree-val">${formatCents(operatingResult)}</span>
      </div>
    </div>
  `;

  passivaHtml += `
      </div>
    </div>

    <div class="fibu-tree-item level-0" style="margin-top: 16px;">
      <div class="fibu-tree-row">
        <div class="fibu-tree-label-wrap">
          <span class="fibu-tree-toggle">▼</span>
          <span class="fibu-tree-label">B. Rückstellungen</span>
        </div>
        <span class="fibu-tree-val">${formatCents(rNet)}</span>
      </div>
      <div class="fibu-tree-children">
  `;

  const rSubs = state.accounts.filter(a => a.parent_id === rueckstellungen?.id);
  rSubs.forEach(sub => {
    passivaHtml += `
      <div class="fibu-tree-item level-1">
        <div class="fibu-tree-row">
          <span class="fibu-tree-label">📄 ${sub.code} ${sub.name}</span>
          <span class="fibu-tree-val">${formatCents(sub.credit_saldo - sub.debit_saldo)}</span>
        </div>
      </div>
    `;
  });

  passivaHtml += `
      </div>
    </div>

    <div class="fibu-tree-item level-0" style="margin-top: 16px;">
      <div class="fibu-tree-row">
        <div class="fibu-tree-label-wrap">
          <span class="fibu-tree-toggle">▼</span>
          <span class="fibu-tree-label">C. Verbindlichkeiten</span>
        </div>
        <span class="fibu-tree-val">${formatCents(vNet)}</span>
      </div>
      <div class="fibu-tree-children">
  `;

  const vSubs = state.accounts.filter(a => a.parent_id === verbindlichkeiten?.id);
  vSubs.forEach(sub => {
    passivaHtml += `
      <div class="fibu-tree-item level-1">
        <div class="fibu-tree-row">
          <div class="fibu-tree-label-wrap">
            <span class="fibu-tree-label">📂 ${sub.code} ${sub.name}</span>
          </div>
          <span class="fibu-tree-val">${formatCents(sub.credit_saldo - sub.debit_saldo)}</span>
        </div>
        <div class="fibu-tree-children">
    `;

    // Add leaf accounts
    const leafs = state.accounts.filter(a => a.parent_id === sub.id);
    leafs.forEach(leaf => {
      passivaHtml += `
        <div class="fibu-tree-row" style="padding-left:12px;">
          <span class="fibu-tree-label">📄 ${leaf.code} ${leaf.name}</span>
          <span class="fibu-mono">${formatCents(leaf.netto_saldo)}</span>
        </div>
      `;
    });

    passivaHtml += `
        </div>
      </div>
    `;
  });

  passivaHtml += `
      </div>
    </div>

    <div class="fibu-tree-row" style="margin-top: 30px; border-top: 2px solid var(--accent); font-weight:700;">
      <span>BILANZSUMME PASSIVA</span>
      <span class="fibu-tree-val" style="color:var(--accent); font-size:16px;">${formatCents(passivaTotalCents)}</span>
    </div>
  `;

  passivaTree.innerHTML = passivaHtml;
}

function renderHgbGuV() {
  const container = state.els.guvTree;
  if (!container) return;

  const revenueGroup = state.accounts.find(a => a.code === '8000' || a.code === '4000');
  const revenueCents = revenueGroup?.credit_saldo || 0;

  const materialGroup = state.accounts.find(a => a.code === '3000' || a.code === '5000');
  const materialCents = materialGroup?.debit_saldo || 0;

  const expenseGroup = state.accounts.find(a => a.code === '4000' || a.code === '6000');
  const expenseCents = expenseGroup?.debit_saldo || 0;

  const resultCents = revenueCents - materialCents - expenseCents;

  let html = `
    <div class="fibu-tree-item level-0">
      <div class="fibu-tree-row">
        <span>1. Umsatzerlöse (§ 275 HGB)</span>
        <span class="fibu-tree-val fibu-text-debit">+ ${formatCents(revenueCents)}</span>
      </div>
      <div class="fibu-tree-children">
  `;

  // Revenue childs
  const revChilds = state.accounts.filter(a => a.parent_id === revenueGroup?.id);
  revChilds.forEach(child => {
    html += `
      <div class="fibu-tree-row">
        <span>📄 ${child.code} ${child.name}</span>
        <span class="fibu-mono">${formatCents(child.credit_saldo - child.debit_saldo)}</span>
      </div>
    `;
  });

  html += `
      </div>
    </div>

    <div class="fibu-tree-item level-0" style="margin-top: 12px;">
      <div class="fibu-tree-row">
        <span>2. Materialaufwand / Wareneingang</span>
        <span class="fibu-tree-val fibu-text-credit">- ${formatCents(materialCents)}</span>
      </div>
      <div class="fibu-tree-children">
  `;

  const matChilds = state.accounts.filter(a => a.parent_id === materialGroup?.id);
  matChilds.forEach(child => {
    html += `
      <div class="fibu-tree-row">
        <span>📄 ${child.code} ${child.name}</span>
        <span class="fibu-mono">${formatCents(child.debit_saldo - child.credit_saldo)}</span>
      </div>
    `;
  });

  html += `
      </div>
    </div>

    <div class="fibu-tree-item level-0" style="margin-top: 12px;">
      <div class="fibu-tree-row">
        <span>3. Sonstige betriebliche Aufwendungen</span>
        <span class="fibu-tree-val fibu-text-credit">- ${formatCents(expenseCents)}</span>
      </div>
      <div class="fibu-tree-children">
  `;

  const expChilds = state.accounts.filter(a => a.parent_id === expenseGroup?.id);
  expChilds.forEach(child => {
    html += `
      <div class="fibu-tree-row">
        <span>📄 ${child.code} ${child.name}</span>
        <span class="fibu-mono">${formatCents(child.debit_saldo - child.credit_saldo)}</span>
      </div>
    `;
  });

  html += `
      </div>
    </div>

    <div class="fibu-tree-row" style="margin-top: 24px; border-top: 2px solid var(--accent); font-weight:700; font-size:15px;">
      <span>4. JAHRESÜBERSCHUSS / JAHRESFEHLBETRAG</span>
      <span class="fibu-tree-val" style="color:var(--accent);">${formatCents(resultCents)}</span>
    </div>
  `;

  container.innerHTML = html;
}

function getOperatingGuVResult() {
  return getOperatingGuVResultImported(state.accounts);
}

function renderElsterUStVA() {
  const report = calculateElsterUstva(state.accounts, state.skrName);

  state.els.ustva81.textContent = formatCents(report.feld81.base);
  state.els.ustvaTax81.textContent = formatCents(report.feld81.tax);
  state.els.ustva86.textContent = formatCents(report.feld86.base);
  state.els.ustvaTax86.textContent = formatCents(report.feld86.tax);
  state.els.ustva66.textContent = formatCents(report.feld66);

  state.els.ustvaZahllast.textContent = formatCents(report.zahllast);
  state.els.ustvaZahllast.style.color = report.zahllast >= 0 ? 'var(--danger, #ef4444)' : 'var(--accent, #0b8a6f)';
}

// =========================================================================
// 🏢 Rendering View: Assets & Depreciation (AfA)
// =========================================================================
function renderAssetsList() {
  const container = state.els.assetsList;
  if (!container) return;

  let html = '';
  assetRows().forEach(as => {
    html += `
      <tr class="${state.selectedAssetId === as.nr ? 'is-selected' : ''}" data-asset-click-nr="${as.nr}" data-context-record-id="${escapeHtml(as.nr)}" data-context-record-type="accounting_asset" data-context-label="${escapeHtml(as.name)}" aria-selected="${state.selectedAssetId === as.nr ? 'true' : 'false'}" tabindex="0">
        <td><span class="fibu-mono">${as.nr}</span></td>
        <td style="font-weight: 500; color: var(--text-strong);">🏢 ${escapeHtml(as.name)}</td>
        <td><span class="fibu-mono">${as.date}</span></td>
        <td class="is-num">${formatCents(as.cost)}</td>
        <td class="is-center">${as.life}</td>
        <td><span class="ctox-badge">${as.method}</span></td>
        <td class="fibu-text-credit is-num">${formatCents(as.prev)}</td>
        <td class="fibu-text-debit is-num">${formatCents(as.book)}</td>
        <td class="is-center">
          <button type="button" class="ctox-button ctox-button--sm" data-asset-plan="${as.nr}">Plan</button>
        </td>
      </tr>
    `;
  });

  container.innerHTML = html;

  container.querySelectorAll('tr').forEach(tr => {
    tr.addEventListener('click', () => selectAsset(tr.getAttribute('data-asset-click-nr')));
    tr.addEventListener('keydown', (event) => {
      if (event.key !== 'Enter' && event.key !== ' ') return;
      event.preventDefault();
      selectAsset(tr.getAttribute('data-asset-click-nr'));
    });
  });

  container.querySelectorAll('[data-asset-plan]').forEach(btn => {
    btn.addEventListener('click', (event) => {
      event.stopPropagation();
      selectAsset(btn.getAttribute('data-asset-plan'));
    });
  });
}

function selectAsset(nr) {
  const asset = assetRows().find(as => as.nr === nr);
  if (!asset) return;
  state.selectedAssetId = nr;
  setSelectedRow(state.els.assetsList, 'data-asset-click-nr', nr);
  openAssetDrawer(asset.nr, asset.name, asset.cost, asset.life);
}

// =========================================================================
// 📂 Bottom Drawer Dialogs (Slide-up Glassmorphism Panel)
// =========================================================================
function openDrawer() {
  state.els.drawer?.classList.add('is-open');
  state.els.drawer?.setAttribute('aria-hidden', 'false');
  window.requestAnimationFrame(() => {
    const focusTarget = state.els.drawer?.querySelector('input, select, textarea, button, [tabindex]:not([tabindex="-1"])') || state.els.drawer;
    focusTarget?.focus?.();
  });
}

function closeDrawer() {
  state.els.drawer?.classList.remove('is-open');
  state.els.drawer?.setAttribute('aria-hidden', 'true');
}

function openAccountLedgerDrawer(accountId) {
  const account = state.accounts.find(a => a.id === accountId);
  if (!account) return;

  state.els.drawerTitle.textContent = `Sachkonto-Kontoauszug: ${account.code} - ${account.name}`;
  openDrawer();

  // Filter ledger entries for this account
  const ledgerLines = state.ledgerDF.filter(l => l.account_id === accountId);

  let linesHtml = '';
  if (ledgerLines.length === 0) {
    linesHtml = `<div class="ctox-empty">Keine Hauptbuchungen für dieses Sachkonto vorhanden.</div>`;
  } else {
    linesHtml = `
      <table class="ctox-table">
        <thead>
          <tr>
            <th>Datum</th>
            <th>Belegnr</th>
            <th>Buchungstext</th>
            <th class="is-num">Soll</th>
            <th class="is-num">Haben</th>
          </tr>
        </thead>
        <tbody>
    `;
    ledgerLines.forEach(l => {
      linesHtml += `
        <tr>
          <td><span class="fibu-mono">${formatDateDe(l.posting_date)}</span></td>
          <td><span class="fibu-mono">${l.number}</span></td>
          <td>${escapeHtml(l.narration)}</td>
          <td class="fibu-text-debit is-num">${l.debit ? formatCents(l.debit) : '—'}</td>
          <td class="fibu-text-credit is-num">${l.credit ? formatCents(l.credit) : '—'}</td>
        </tr>
      `;
    });
    linesHtml += `</tbody></table>`;
  }

  state.els.drawerContent.innerHTML = linesHtml;
}

function openJournalEntryDrawer(entryId) {
  const entry = state.journalEntries.find(e => e.id === entryId);
  if (!entry) return;

  const isPosted = !!entry.posted_at;
  state.els.drawerTitle.textContent = `Belegjournal-Details: ${entry.number || 'Entwurf'}`;
  openDrawer();

  const lines = state.journalEntryLines.filter(l => l.journal_entry_id === entryId);

  let html = `
    <dl class="ctox-fields" style="margin-bottom:16px;">
      <dt>Buchungstext</dt>
      <dd>${escapeHtml(entry.narration || '')}</dd>
      <dt>Buchungsdatum</dt>
      <dd class="fibu-mono">${formatDateDe(entry.posting_date)}</dd>
    </dl>
  `;

  html += `
    <table class="ctox-table">
      <thead>
        <tr>
          <th>Konto Code</th>
          <th>Kontenbezeichnung</th>
          <th class="is-num">Soll</th>
          <th class="is-num">Haben</th>
        </tr>
      </thead>
      <tbody>
  `;

  lines.forEach(l => {
    const acct = state.accounts.find(a => a.id === l.account_id);
    html += `
      <tr>
        <td><span class="fibu-mono">${acct?.code || '—'}</span></td>
        <td>${escapeHtml(acct?.name || 'Unbekanntes Konto')}</td>
        <td class="fibu-text-debit is-num">${l.debit ? formatCents(l.debit) : '—'}</td>
        <td class="fibu-text-credit is-num">${l.credit ? formatCents(l.credit) : '—'}</td>
      </tr>
    `;
  });

  html += `</tbody></table>`;

  if (!isPosted) {
    html += `
      <div style="margin-top:20px; display:flex; justify-content:flex-end; gap:10px;">
        <button class="ctox-button is-primary" onclick="postEntryDirectly('${entry.id}')">🔒 GoBD Festschreiben</button>
      </div>
    `;
  } else {
    html += `
      <span class="ctox-badge is-success fibu-locked-badge" title="Dieser Belegeintrag ist steuerrechtlich gesperrt. Nachträgliche Korrekturen können nur per Storno vorgenommen werden.">Festgeschrieben</span>
    `;
  }

  state.els.drawerContent.innerHTML = html;
}

function openReceiptDrawer(receipt) {
  state.els.drawerTitle.textContent = `Belegkontierung: ${escapeHtml(receipt.filename)}`;
  openDrawer();

  const isPosted = receipt.status === 'posted';

  let html = `
    <div id="receipt-drawer-advanced-container">
      ${receiptFormMarkup(receipt, isPosted)}

      <div style="margin-top:15px;">
        <div class="fibu-form-group">
          <label class="ctox-field-label">Zahlungsweg</label>
          <select id="rcpt-payment-method" class="ctox-select" onchange="updateReceiptDrawerOptions('${receipt.id}')" ${isPosted ? 'disabled' : ''}>
            <option value="kreditor">Geschäftskonto (Standard-Kreditor Verbindlichkeiten)</option>
            <option value="privat">Privat bezahlt (Privatauslage des Inhabers)</option>
            <option value="verrechnung">Gesellschafter-Verrechnungskonto</option>
          </select>
        </div>
      </div>

      <div style="margin-top:15px;">
        <span class="ctox-field-label" style="margin-bottom:8px;">Steuerliche Sonderregeln</span>
        <div style="display:flex; flex-direction:column; gap:8px;">
          <label title="70/30 Netto-Split, 100 % Vorsteuer abzugsfähig" style="display:flex; align-items:center; gap:8px; cursor:pointer; color:var(--text); font-size:12.5px;">
            <input type="checkbox" id="rcpt-bewirtung" onchange="updateReceiptDrawerOptions('${receipt.id}')" ${isPosted ? 'disabled' : ''} />
            Geschäftliche Bewirtung
          </label>
          <label title="Über 35 € automatisch nicht abzugsfähig" style="display:flex; align-items:center; gap:8px; cursor:pointer; color:var(--text); font-size:12.5px;">
            <input type="checkbox" id="rcpt-geschenk" onchange="updateReceiptDrawerOptions('${receipt.id}')" ${isPosted ? 'disabled' : ''} />
            Geschenk an Partner
          </label>
          <label title="20 % Privatanteil werden automatisch herausgerechnet" style="display:flex; align-items:center; gap:8px; cursor:pointer; color:var(--text); font-size:12.5px;">
            <input type="checkbox" id="rcpt-phone-private" onchange="updateReceiptDrawerOptions('${receipt.id}')" ${isPosted ? 'disabled' : ''} />
            Telefon/Internet
          </label>
        </div>
      </div>

      <h4 class="ctox-field-label" style="margin:20px 0 10px 0;">Buchungssatz</h4>
      <div id="receipt-preview-table-container">
        <!-- Table loaded dynamically -->
      </div>

      <div style="margin-top:20px; display:flex; justify-content:flex-end; gap:10px;">
        ${isPosted ? `
          <span class="ctox-badge is-success fibu-locked-badge" title="Dieser Beleg wurde bereits festgeschrieben und verbucht.">Festgeschrieben</span>
        ` : `
          <button class="ctox-button is-primary" onclick="postReceiptWithAdvancedOptions('${receipt.id}')">✔️ Vorkontierung freigeben & Buchen</button>
        `}
      </div>
    </div>
  `;
  state.els.drawerContent.innerHTML = html;
  bindReceiptForm(state.els.drawerContent.querySelector('[data-receipt-form]'), receipt);

  // Initial draw
  updateReceiptDrawerOptions(receipt.id);
}

function openBankReconciliationDrawer(lineId) {
  const line = state.bankStatementLines.find(l => l.id === lineId);
  if (!line) return;

  state.selectedBankLineId = lineId;
  state.els.drawerTitle.textContent = `Transaktionsabgleich: Bankkonto`;
  openDrawer();

  const amountVal = line.amount || 0;
  const matchedReceipt = state.receipts.find(r => r.gross_amount === Math.abs(amountVal) && r.status !== 'posted');

  let html = `
    <dl class="ctox-fields" style="margin-bottom:16px;">
      <dt>Verwendungszweck</dt>
      <dd>${escapeHtml(line.narration)}</dd>
      <dt>Wertstellung</dt>
      <dd class="fibu-mono">${line.value_date}</dd>
    </dl>
  `;

  if (line.match_status === 'matched') {
    html += `
      <!-- v4-allow: label (Buchhaltungsbegriff Bankabgleich) -->
      <span class="ctox-badge is-success fibu-locked-badge" title="Diese Banktransaktion wurde GoBD-konform mit einem Beleg verknüpft und festgeschrieben.">Abgeglichen & Verbucht</span>
    `;
  } else {
    if (matchedReceipt) {
      html += `
        <div class="ctox-card" style="margin-top:20px; padding:12px;">
          <h4 class="ctox-field-label" style="margin:0 0 8px 0;">Passender Beleg · ${formatCents(Math.abs(amountVal))}</h4>
          <div style="display:flex; justify-content:space-between; align-items:center;">
            <div>
              <div style="font-weight:700; color:var(--text-strong);">📄 ${escapeHtml(matchedReceipt.filename)}</div>
              <div style="font-size:11px; color: var(--muted);">Lieferant: ${escapeHtml(matchedReceipt.supplier_name)} | Datum: ${formatDateDe(matchedReceipt.invoice_date)}</div>
            </div>
            <button class="ctox-button is-primary" onclick="matchBankLineDirectly('${line.id}', '${matchedReceipt.id}')">Abgleich bestätigen</button>
          </div>
        </div>
      `;
    } else {
      html += `
        <div class="ctox-empty" style="margin-top:20px; padding:15px; min-height:0;">
          Kein passender Beleg mit dem Betrag ${formatCents(Math.abs(amountVal))} im Vorrat gefunden.
        </div>
      `;
    }

    html += `
      <div style="margin-top:20px; display:flex; justify-content:center;">
        <button class="ctox-button" style="width:100%;" onclick="toggleBankSplitEditor('${line.id}')">
          🥞 Transaktionsbetrag aufteilen (Split-Aufteilungsbuchung)
        </button>
      </div>
      <div id="bank-split-editor-container">
        <!-- Split editor injected here on click -->
      </div>
    `;
  }

  state.els.drawerContent.innerHTML = html;
}

function openAssetDrawer(nr, name, cost, life) {
  state.els.drawerTitle.textContent = `AfA-Abschreibungsplan: ${name}`;
  openDrawer();

  // Calculate stateless linear AfA schedule
  const ratePerFullYear = Math.round(cost / life);

  let html = `
    <dl class="ctox-fields" style="margin-bottom:16px;">
      <dt>Anschaffungskosten</dt>
      <dd>${formatCents(cost)}</dd>
      <dt>Nutzungsdauer</dt>
      <dd>${life} Jahre</dd>
    </dl>

    <table class="ctox-table">
      <thead>
        <tr>
          <th>Kalenderjahr</th>
          <th class="is-num">Buchwert (Anfang)</th>
          <th class="is-num">AfA-Rate (Jahr)</th>
          <th class="is-num">Kumulierte Abschreibung</th>
          <th class="is-num">Buchwert (Ende)</th>
        </tr>
      </thead>
      <tbody>
  `;

  let currentBookVal = cost;
  let accumulated = 0;
  const startYear = 2026;

  for (let year = 0; year < life; year++) {
    const isLastYear = year === life - 1;
    const yearRate = isLastYear ? currentBookVal - 100 : ratePerFullYear; // Restwert 1 EUR (100 cents)
    const endVal = currentBookVal - yearRate;
    accumulated += yearRate;

    html += `
      <tr>
        <td><span class="fibu-mono">${startYear + year}</span></td>
        <td class="is-num">${formatCents(currentBookVal)}</td>
        <td class="fibu-text-credit is-num">${formatCents(yearRate)}</td>
        <td class="is-num">${formatCents(accumulated)}</td>
        <td class="fibu-text-debit is-num" style="font-weight:600;">${formatCents(endVal)}</td>
      </tr>
    `;
    currentBookVal = endVal;
  }

  html += `</tbody></table>`;
  state.els.drawerContent.innerHTML = html;
}

function openManualJournalDrawer() {
  state.els.drawerTitle.textContent = `Neue manuelle Journalbuchung`;
  openDrawer();

  const defaults = getManualEntryDefaultAccountIds();
  let html = `
    <form id="fibu-new-entry-form" novalidate>
      <div class="fibu-form-row">
        <div class="fibu-form-group">
          <label class="ctox-field-label" for="new-entry-date">Belegdatum</label>
          <input type="date" id="new-entry-date" class="ctox-input" value="2026-05-22" required aria-describedby="new-entry-validation" />
        </div>
        <div class="fibu-form-group" style="flex:2;">
          <label class="ctox-field-label" for="new-entry-narration">Buchungstext</label>
          <input type="text" id="new-entry-narration" class="ctox-input" placeholder="z.B. Miete Büroräume Mai" required />
        </div>
      </div>

      <div class="fibu-form-row">
        <div class="fibu-form-group">
          <label class="ctox-field-label" for="new-entry-soll">Soll-Konto</label>
          <select id="new-entry-soll" class="ctox-select" required>
            ${state.accounts.filter(a => !a.is_group).map(a => `<option value="${a.id}" ${a.id === defaults.soll ? 'selected' : ''}>${a.code} ${escapeHtml(a.name)}</option>`).join('')}
          </select>
        </div>
        <div class="fibu-form-group">
          <label class="ctox-field-label" for="new-entry-haben">Haben-Konto</label>
          <select id="new-entry-haben" class="ctox-select" required>
            ${state.accounts.filter(a => !a.is_group).map(a => `<option value="${a.id}" ${a.id === defaults.haben ? 'selected' : ''}>${a.code} ${escapeHtml(a.name)}</option>`).join('')}
          </select>
        </div>
        <div class="fibu-form-group">
          <label class="ctox-field-label" for="new-entry-amount">Betrag (Netto in EUR)</label>
          <input type="number" step="0.01" min="0.01" id="new-entry-amount" class="ctox-input" placeholder="0.00" required />
        </div>
      </div>

      <div id="new-entry-validation" class="fibu-validation-summary" role="status" aria-live="polite"></div>

      <div class="fibu-drawer-actions">
        <button type="submit" id="new-entry-submit" class="ctox-button is-primary" disabled aria-disabled="true">💾 Als Entwurf buchen</button>
      </div>
    </form>
  `;

  state.els.drawerContent.innerHTML = html;
  const form = document.getElementById('fibu-new-entry-form');
  form?.addEventListener('input', validateManualEntryForm);
  form?.addEventListener('change', validateManualEntryForm);
  form?.addEventListener('submit', (event) => {
    event.preventDefault();
    if (!validateManualEntryForm()) return;
    window.saveManualEntry();
  });
  validateManualEntryForm();

  // Wire save button
  window.saveManualEntry = async () => {
    const date = document.getElementById('new-entry-date').value;
    const narration = document.getElementById('new-entry-narration').value.trim();
    const soll = document.getElementById('new-entry-soll').value;
    const haben = document.getElementById('new-entry-haben').value;
    const valAmount = parseFloat(document.getElementById('new-entry-amount').value);

    if (!validateManualEntryForm()) return;

    const cents = Math.round(valAmount * 100);
    const db = fibuDb();

    const entryId = 'manual-' + Math.random().toString(36).substring(2, 9);

    // Insert Entry
    await db.accounting_journal_entries.add({
      id: entryId,
      posting_date: date,
      type: 'journal',
      number: '',
      narration: narration,
      updated_at_ms: Date.now()
    });

    // Insert Soll Line
    await db.accounting_journal_entry_lines.add({
      id: entryId + '-l1',
      journal_entry_id: entryId,
      account_id: soll,
      debit: cents,
      credit: 0,
      line_no: 1,
      updated_at_ms: Date.now()
    });

    // Insert Haben Line
    await db.accounting_journal_entry_lines.add({
      id: entryId + '-l2',
      journal_entry_id: entryId,
      account_id: haben,
      debit: 0,
      credit: cents,
      line_no: 2,
      updated_at_ms: Date.now()
    });

    closeDrawer();
    switchView('journal');
  };
}

function getManualEntryDefaultAccountIds() {
  const leafAccounts = state.accounts.filter(a => !a.is_group);
  const soll = leafAccounts.find(a => ['expense', 'fixed_asset'].includes(a.account_type)) || leafAccounts[0];
  const haben = leafAccounts.find(a => a.id !== soll?.id && ['bank', 'cash', 'payable'].includes(a.account_type))
    || leafAccounts.find(a => a.id !== soll?.id)
    || null;
  return { soll: soll?.id || '', haben: haben?.id || '' };
}

function validateManualEntryForm() {
  const date = document.getElementById('new-entry-date')?.value || '';
  const narration = document.getElementById('new-entry-narration')?.value.trim() || '';
  const soll = document.getElementById('new-entry-soll')?.value || '';
  const haben = document.getElementById('new-entry-haben')?.value || '';
  const amountValue = document.getElementById('new-entry-amount')?.value || '';
  const amount = Number.parseFloat(amountValue);

  const errors = [];
  if (!date) errors.push('Belegdatum fehlt.');
  if (!narration) errors.push('Buchungstext fehlt.');
  if (!soll) errors.push('Soll-Konto fehlt.');
  if (!haben) errors.push('Haben-Konto fehlt.');
  if (soll && haben && soll === haben) errors.push('Soll und Haben müssen unterschiedliche Konten sein.');
  if (!Number.isFinite(amount) || amount <= 0) errors.push('Betrag muss größer als 0,00 € sein.');

  const submit = document.getElementById('new-entry-submit');
  const summary = document.getElementById('new-entry-validation');
  const valid = errors.length === 0;
  if (submit) {
    submit.disabled = !valid;
    submit.setAttribute('aria-disabled', String(!valid));
  }
  if (summary) {
    summary.textContent = valid ? 'Buchung ist vollständig und ausgeglichen.' : errors.join(' ');
    summary.classList.toggle('is-valid', valid);
  }
  return valid;
}

// =========================================================================
// 📥 SEPA / camt.053 & MT940 Document Upload Actions
// =========================================================================
async function handleBankStatementImport(e) {
  const file = e.target.files[0];
  if (!file) return;

  const text = await file.text();
  const db = fibuDb();
  if (!db) return;

  const statementId = 'stmt-' + Date.now();

  // 1. Insert Statement
  await db.accounting_bank_statements.add({
    id: statementId,
    account_number: 'DE90370400440532013000',
    updated_at_ms: Date.now()
  });

  let parsedLines = [];

  // 2. Parse file using the appropriate modular parser
  if (file.name.endsWith('.xml')) {
    parsedLines = parseCamt053(text);
  } else {
    parsedLines = parseMT940(text);
  }

  // 3. Insert parsed transaction lines
  let count = 0;
  for (const line of parsedLines) {
    await db.accounting_bank_statement_lines.add({
      id: `bankline-${statementId}-${count++}`,
      statement_id: statementId,
      value_date: line.value_date,
      narration: line.narration,
      amount: line.amount,
      counterparty_name: line.counterparty_name,
      counterparty_iban: line.counterparty_iban || 'DE1002000000000000000',
      match_status: 'unmatched',
      updated_at_ms: Date.now()
    });
  }

  switchView('banking');
}

async function handleBelegeUpload(e) {
  const files = e.target.files;
  if (!files || files.length === 0) return;

  const db = fibuDb();
  if (!db) return;

  // Nur echte Angaben der Datei (Strang BUCH-2): keine erkannten Lieferanten, Nummern, Daten oder Beträge – es gibt
  // keine Belegerkennung. Die Datei geht über api.files (BUCH-3, Muster cv-print-builder/app-store); file_storage_url
  // trägt die Datei-Referenz (sha256), file_id das files-Dokument. Ohne gespeicherte Datei kein Beleg.
  for (const file of files) {
    const id = 'rcpt-' + Math.random().toString(36).substring(2, 9);
    const fileId = `fibu_receipt_file_${id}`;
    const mime = file.type || 'application/octet-stream';
    let ref = '';
    try {
      const upload = state.api.files.put(file, { id: fileId, name: file.name, mime });
      await upload.local;
      ref = String(await upload.ref || '');
      // E35/STUB-E30: lehnt der Core die Datei nach der lokalen Quittung ab, wird done mit {code, reason} abgelehnt
      // (ältere Kerne: result rejected). Der Beleg bleibt erhalten und wird markiert (file_state 'rejected'); buchbar
      // ist er erst wieder mit einer gespeicherten Datei (receiptIsComplete).
      const onRejected = (reason) => markReceiptFileRejected(id, file.name, reason);
      if (upload.done?.then) upload.done.catch((error) => onRejected(error?.reason || error?.code || error?.message));
      else upload.result?.then?.((result) => { if (result?.status === 'rejected') onRejected(result.reason || result.code); }, () => {});
    } catch (error) {
      fibuNotice('receipt.upload_failed', `${file.name}: Datei nicht gespeichert (${error?.code || error?.message || error})`, 'error');
      continue;
    }
    if (!ref) {
      fibuNotice('receipt.upload_failed', `${file.name}: Datei nicht gespeichert (keine Referenz)`, 'error');
      continue;
    }
    state.api.status.clear('buchhaltung.receipt.upload_failed');
    const now = Date.now();
    await db.accounting_receipts.add({
      id,
      file_storage_url: ref,
      file_id: fileId,
      filename: file.name,
      file_size: Number(file.size) || 0,
      mime_type: file.type || '',
      uploaded_at_ms: now,
      status: 'draft',
      updated_at_ms: now
    });
  }

  switchView('receipts');
}

// =========================================================================
// 🤖 Stateless Business Matcher & Auto Reconciler Heuristics
// =========================================================================
async function triggerAutoReconcile() {
  const db = fibuDb();
  if (!db) return;

  let matchCount = 0;
  for (const line of state.bankStatementLines) {
    if (line.match_status !== 'matched') {
      let bestReceipt = null;
      let highestScore = 0;

      for (const r of state.receipts) {
        if (r.status !== 'posted') {
          const score = calculateMatchScore(line, r);
          if (score > highestScore) {
            highestScore = score;
            bestReceipt = r;
          }
        }
      }

      // If we have a robust match proposal (exact amount matched is 50+ score)
      if (bestReceipt && highestScore >= 50) {
        const doc = await db.accounting_bank_statement_lines.byId(line.id);
        if (doc) {
          await db.accounting_bank_statement_lines.change(doc.id, { match_status: 'proposed' });
          matchCount++;
        }
      }
    }
  }

  fibuNotice('bank.auto_reconcile', `🤖 Auto-Reconciliation beendet. ${matchCount} matches automatisch vorgeschlagen!`);
}

// =========================================================================
// 🥞 Advanced Pure Accounting Split & Tax Tricks Controllers
// =========================================================================
function compileReceiptPreviewLines(receipt, paymentMethod, isBewirtung, isGeschenk, isPhonePrivate) {
  const lines = [];
  const skr = state.skrName; // 'SKR03' or 'SKR04'

  // Contra account (Credit)
  let contraCode = skr === 'SKR03' ? '1600' : '3300'; // Default Kreditor
  if (paymentMethod === 'privat') {
    contraCode = skr === 'SKR03' ? '1890' : '2180'; // Privateinlage
  } else if (paymentMethod === 'verrechnung') {
    contraCode = skr === 'SKR03' ? '1370' : '1486'; // Gesellschafter-Verrechnungskonto
  }
  const contraAcct = state.accounts.find(a => a.code === contraCode);
  const contraName = contraAcct ? contraAcct.name : (paymentMethod === 'privat' ? 'Privateinlage' : 'Verrechnungskonto');

  const vorsteuerCode = skr === 'SKR03' ? '1576' : '1406'; // Vorsteuer 19%
  const vorsteuerAcct = state.accounts.find(a => a.code === vorsteuerCode);
  const vorsteuerName = vorsteuerAcct ? vorsteuerAcct.name : 'Vorsteuer 19%';

  // Deductible expense code
  let expenseCode = '';
  const suggestedAcct = state.accounts.find(a => a.id === receipt.suggested_account_id);

  if (isBewirtung) {
    expenseCode = skr === 'SKR04' ? '6640' : '4650'; // Bewirtungskosten abzugsfähig
  } else if (isGeschenk) {
    expenseCode = recommendGiftAccount(receipt.net_amount, skr);
  } else {
    // Ohne erfasstes Konto kein vorgeschlagenes Aufwandskonto (BUCH-2: nichts erfinden).
    expenseCode = suggestedAcct ? suggestedAcct.code : '';
  }
  const expenseAcct = expenseCode ? state.accounts.find(a => a.code === expenseCode) : null;
  const expenseName = expenseAcct ? expenseAcct.name : (expenseCode ? 'Aufwandskonto' : '—');

  if (isBewirtung) {
    // 70/30 split
    const splits = calculateEntertainmentSplit(receipt.gross_amount, 19);
    const nonDeductibleCode = skr === 'SKR04' ? '6644' : '4654'; // Bewirtungskosten nicht abzugsfähig
    const nonDeductibleAcct = state.accounts.find(a => a.code === nonDeductibleCode);
    const nonDeductibleName = nonDeductibleAcct ? nonDeductibleAcct.name : 'Bewirtungskosten nicht abzugsfähig';

    lines.push({ code: expenseCode, name: expenseName, debit: splits.deductibleNet, credit: 0 });
    lines.push({ code: nonDeductibleCode, name: nonDeductibleName, debit: splits.nonDeductibleNet, credit: 0 });
    lines.push({ code: vorsteuerCode, name: vorsteuerName, debit: splits.vatAmount, credit: 0 });
  } else if (isPhonePrivate) {
    // Phone 20% private share split (80% net business expense, 20% net private withdrawal)
    const privateNet = calculatePrivatePhoneShare(receipt.net_amount, 20);
    const businessNet = receipt.net_amount - privateNet;
    const privateCode = skr === 'SKR04' ? '2100' : '1880'; // Privatentnahme / Unentgeltliche Wertabgabe
    const privateAcct = state.accounts.find(a => a.code === privateCode);
    const privateName = privateAcct ? privateAcct.name : 'Privatentnahme';

    lines.push({ code: expenseCode, name: expenseName, debit: businessNet, credit: 0 });
    lines.push({ code: privateCode, name: privateName, debit: privateNet, credit: 0 });
    lines.push({ code: vorsteuerCode, name: vorsteuerName, debit: receipt.tax_amount, credit: 0 });
  } else {
    // Standard posting
    lines.push({ code: expenseCode, name: expenseName, debit: receipt.net_amount, credit: 0 });
    lines.push({ code: vorsteuerCode, name: vorsteuerName, debit: receipt.tax_amount, credit: 0 });
  }

  // Credit Line (payment)
  lines.push({ code: contraCode, name: contraName, debit: 0, credit: receipt.gross_amount });

  return lines;
}

window.updateReceiptDrawerOptions = (receiptId) => {
  const stored = state.receipts.find(r => r.id === receiptId);
  if (!stored) return;
  // Vorschau aus dem Formular (auch ungespeicherte Eingaben), sonst aus dem gespeicherten Beleg.
  const form = receiptFormFor(receiptId);
  const receipt = form ? { ...stored, ...readReceiptForm(form) } : stored;

  const paymentMethod = document.getElementById('rcpt-payment-method').value;
  const isBewirtung = document.getElementById('rcpt-bewirtung').checked;
  const isGeschenk = document.getElementById('rcpt-geschenk').checked;
  const isPhonePrivate = document.getElementById('rcpt-phone-private').checked;

  // Recalculate preview lines
  const lines = compileReceiptPreviewLines(receipt, paymentMethod, isBewirtung, isGeschenk, isPhonePrivate);

  // Render table
  let tableHtml = `
    <table class="ctox-table" style="margin-top:10px;">
      <thead>
        <tr>
          <th>Konto</th>
          <th>Kontenbezeichnung</th>
          <th class="is-num">Soll</th>
          <th class="is-num">Haben</th>
        </tr>
      </thead>
      <tbody>
  `;

  lines.forEach(l => {
    tableHtml += `
      <tr>
        <td><span class="fibu-mono">${l.code}</span></td>
        <td>${escapeHtml(l.name)}</td>
        <td class="fibu-text-debit is-num">${l.debit > 0 ? formatCents(l.debit) : '—'}</td>
        <td class="fibu-text-credit is-num">${l.credit > 0 ? formatCents(l.credit) : '—'}</td>
      </tr>
    `;
  });

  tableHtml += `
      </tbody>
    </table>
  `;

  // Update DOM
  document.getElementById('receipt-preview-table-container').innerHTML = tableHtml;
};

window.postReceiptWithAdvancedOptions = async (receiptId) => {
  const db = fibuDb();
  if (!db) return;

  // Offene Eingaben zuerst speichern; gebucht wird der gespeicherte Stand (nicht die evtl. veraltete Liste).
  const form = receiptFormFor(receiptId);
  if (form && (await saveReceiptForm(form)) === false) return;
  const receipt = db.accounting_receipts.byId(receiptId) || state.receipts.find(r => r.id === receiptId);
  if (!receipt) return;
  if (receipt.status === 'posted') return;

  if (!receiptIsComplete(receipt)) {
    fibuNotice('receipt.incomplete', 'Beleg unvollständig oder Brutto ≠ Netto + Steuer.', 'warn');
    return;
  }
  state.api?.status.clear('buchhaltung.receipt.incomplete');

  const paymentMethod = document.getElementById('rcpt-payment-method').value;
  const isBewirtung = document.getElementById('rcpt-bewirtung').checked;
  const isGeschenk = document.getElementById('rcpt-geschenk').checked;
  const isPhonePrivate = document.getElementById('rcpt-phone-private').checked;

  const lines = compileReceiptPreviewLines(receipt, paymentMethod, isBewirtung, isGeschenk, isPhonePrivate);

  const entryId = 'j-' + Math.random().toString(36).substring(2, 9);
  const now = Date.now();

  // Unique number
  const nextNum = 'J-2026-' + String(state.journalEntries.filter(e => e.posted_at).length + 1).padStart(4, '0');

  // 1. Insert Journal Entry
  await db.accounting_journal_entries.add({
    id: entryId,
    posting_date: receipt.invoice_date,
    type: 'invoice',
    ref_type: 'receipt',
    ref_id: receiptId,
    number: nextNum,
    narration: `Eingangsbeleg ${receipt.invoice_number} - ${receipt.supplier_name}` +
      (isBewirtung ? ' (Bewirtung 70/30)' : '') +
      (isGeschenk ? ' (Geschenk)' : '') +
      (isPhonePrivate ? ' (inkl. 20% Privatanteil)' : ''),
    posted_at: now,
    updated_at_ms: now
  });

  // 2. Insert Lines
  for (let i = 0; i < lines.length; i++) {
    const l = lines[i];
    const acct = state.accounts.find(a => a.code === l.code);
    await db.accounting_journal_entry_lines.add({
      id: `${entryId}-l${i+1}`,
      journal_entry_id: entryId,
      account_id: acct?.id || '',
      debit: l.debit,
      credit: l.credit,
      line_no: i + 1,
      updated_at_ms: now
    });
  }

  // 3. Mark receipt as posted
  const rcptDoc = await db.accounting_receipts.byId(receiptId);
  if (rcptDoc) {
    await db.accounting_receipts.change(rcptDoc.id, { status: 'posted' });
  }

  closeDrawer();
  switchView('journal');
  fibuNotice('booking.posted', t('postedSuccess'));
};

window.toggleBankSplitEditor = (lineId) => {
  const line = state.bankStatementLines.find(l => l.id === lineId);
  if (!line) return;

  // Set up local state for splits
  state.activeBankSplits = [
    { accountCode: state.accounts[0]?.code || '4930', amount: 0, narration: 'Split 1' }
  ];

  renderBankSplitEditor(line);
};

function renderBankSplitEditor(line) {
  const container = document.getElementById('bank-split-editor-container');
  if (!container) return;

  const totalCents = Math.abs(line.amount || 0);
  const remaining = calculateRemainingSplit(totalCents, state.activeBankSplits);

  let html = `
    <div style="margin-top:16px;">
      <h4 class="ctox-field-label" style="margin:0 0 12px 0;">Aufteilung</h4>
      <div style="display:flex; flex-direction:column; gap:10px;" id="bank-split-rows-list">
  `;

  state.activeBankSplits.forEach((split, idx) => {
    html += `
      <div style="display:flex; gap:8px; align-items:center;">
        <select style="font-size:12px; flex:2;" class="ctox-select" onchange="updateBankSplit(${idx}, 'accountCode', this.value)">
          ${state.accounts.map(a => `<option value="${a.code}" ${a.code === split.accountCode ? 'selected' : ''}>${a.code} - ${escapeHtml(a.name)}</option>`).join('')}
        </select>
        <input type="number" style="font-size:12px; flex:1;" class="ctox-input" placeholder="Betrag €" value="${split.amount > 0 ? (split.amount / 100).toFixed(2) : ''}" oninput="updateBankSplit(${idx}, 'amount', parseFloat(this.value) * 100)" />
        <input type="text" style="font-size:12px; flex:2;" class="ctox-input" placeholder="Zweck" value="${escapeHtml(split.narration || '')}" oninput="updateBankSplit(${idx}, 'narration', this.value)" />
        <button class="ctox-button is-danger ctox-button--sm" onclick="deleteBankSplit(${idx})">🗑️</button>
      </div>
    `;
  });

  const isBalanced = remaining === 0;

  html += `
      </div>
      <div style="margin-top:12px; display:flex; justify-content:space-between; align-items:center;">
        <button class="ctox-button ctox-button--sm" onclick="addBankSplitRow()">➕ Zeile hinzufügen</button>
        <div class="${isBalanced ? 'fibu-text-debit' : 'fibu-text-credit'}" style="font-size:13px; font-weight:700;">
          ${isBalanced ? '✔️ Ausgeglichen (0,00 € Rest)' : `Restwert: ${(remaining / 100).toFixed(2)} €`}
        </div>
      </div>
      <div style="margin-top:16px; display:flex; justify-content:flex-end;">
        <button class="ctox-button is-primary" ${!isBalanced ? 'disabled' : ''} onclick="postBankSplitReconciliation('${line.id}')">
          ✔️ Split-Abgleich verbuchen
        </button>
      </div>
    </div>
  `;

  container.innerHTML = html;
}

window.updateBankSplit = (idx, field, value) => {
  if (state.activeBankSplits[idx]) {
    if (field === 'amount') {
      state.activeBankSplits[idx][field] = isNaN(value) ? 0 : Math.round(value);
    } else {
      state.activeBankSplits[idx][field] = value;
    }
    const line = state.bankStatementLines.find(l => l.id === state.selectedBankLineId);
    renderBankSplitEditor(line);
  }
};

window.addBankSplitRow = () => {
  state.activeBankSplits.push({ accountCode: state.accounts[0]?.code || '4930', amount: 0, narration: `Split ${state.activeBankSplits.length + 1}` });
  const line = state.bankStatementLines.find(l => l.id === state.selectedBankLineId);
  renderBankSplitEditor(line);
};

window.deleteBankSplit = (idx) => {
  state.activeBankSplits.splice(idx, 1);
  const line = state.bankStatementLines.find(l => l.id === state.selectedBankLineId);
  renderBankSplitEditor(line);
};

window.postBankSplitReconciliation = async (lineId) => {
  const db = fibuDb();
  if (!db) return;

  const line = state.bankStatementLines.find(l => l.id === lineId);
  if (!line) return;

  const entryId = 'j-' + Math.random().toString(36).substring(2, 9);
  const now = Date.now();

  const bankCode = state.skrName === 'SKR03' ? '1200' : '1800'; // Bank account
  const nextNum = 'J-2026-' + String(state.journalEntries.filter(e => e.posted_at).length + 1).padStart(4, '0');

  // 1. Insert Journal Entry
  await db.accounting_journal_entries.add({
    id: entryId,
    posting_date: line.value_date,
    type: 'bank',
    ref_type: 'bank_statement_line',
    ref_id: lineId,
    number: nextNum,
    narration: `Bank-Split-Abgleich: ${line.narration}`,
    posted_at: now,
    updated_at_ms: now
  });

  // 2. Insert Lines
  const bankAmount = line.amount || 0;
  const isReceipt = bankAmount > 0;

  // Bank Asset Line
  const bankAcctDoc = state.accounts.find(a => a.code === bankCode);
  await db.accounting_journal_entry_lines.add({
    id: `${entryId}-bank`,
    journal_entry_id: entryId,
    account_id: bankAcctDoc?.id || '',
    debit: isReceipt ? Math.abs(bankAmount) : 0,
    credit: !isReceipt ? Math.abs(bankAmount) : 0,
    line_no: 1,
    updated_at_ms: now
  });

  // Split lines
  for (let i = 0; i < state.activeBankSplits.length; i++) {
    const split = state.activeBankSplits[i];
    const splitAcct = state.accounts.find(a => a.code === split.accountCode);
    await db.accounting_journal_entry_lines.add({
      id: `${entryId}-split-${i}`,
      journal_entry_id: entryId,
      account_id: splitAcct?.id || '',
      debit: !isReceipt ? Math.abs(split.amount) : 0,
      credit: isReceipt ? Math.abs(split.amount) : 0,
      line_no: i + 2,
      updated_at_ms: now
    });
  }

  // 3. Mark bank statement line as matched
  const bankLineDoc = await db.accounting_bank_statement_lines.byId(lineId);
  if (bankLineDoc) {
    await db.accounting_bank_statement_lines.change(bankLineDoc.id, { match_status: 'matched', reconciled_entry_id: entryId });
  }

  closeDrawer();
  switchView('banking');
  fibuNotice('bank.matched', t('matchSuccess'));
};

// =========================================================================
// ✔️ Action Direct Controllers (GoBD Immutability execution)
// =========================================================================
window.postReceiptDirectly = async (receiptId) => {
  const db = fibuDb();
  if (!db) return;

  // Offene Eingaben zuerst speichern; gebucht wird der gespeicherte Stand (nicht die evtl. veraltete Liste).
  const form = receiptFormFor(receiptId);
  if (form && (await saveReceiptForm(form)) === false) return;
  const receipt = db.accounting_receipts.byId(receiptId) || state.receipts.find(r => r.id === receiptId);
  if (!receipt) return;
  if (receipt.status === 'posted') return;

  if (!receiptIsComplete(receipt)) {
    fibuNotice('receipt.incomplete', 'Beleg unvollständig oder Brutto ≠ Netto + Steuer.', 'warn');
    return;
  }
  state.api?.status.clear('buchhaltung.receipt.incomplete');

  const entryId = 'j-' + Math.random().toString(36).substring(2, 9);
  const now = Date.now();

  const payablesCode = state.skrName === 'SKR03' ? '1600' : '3300';
  const payablesAcct = state.accounts.find(a => a.code === payablesCode);

  const vorsteuerCode = state.skrName === 'SKR03' ? '1576' : '1406';
  const vorsteuerAcct = state.accounts.find(a => a.code === vorsteuerCode);

  // Save unique number
  const nextNum = 'J-2026-' + String(state.journalEntries.filter(e => e.posted_at).length + 1).padStart(4, '0');

  // 1. Insert Journal Entry
  await db.accounting_journal_entries.add({
    id: entryId,
    posting_date: receipt.invoice_date,
    type: 'invoice',
    ref_type: 'receipt',
    ref_id: receiptId,
    number: nextNum,
    narration: `Eingangsbeleg ${receipt.invoice_number} - ${receipt.supplier_name}`,
    posted_at: now,
    updated_at_ms: now
  });

  // 2. Insert Lines
  // Line 1: Expense (Netto)
  await db.accounting_journal_entry_lines.add({
    id: entryId + '-l1',
    journal_entry_id: entryId,
    account_id: receipt.suggested_account_id,
    debit: receipt.net_amount,
    credit: 0,
    line_no: 1,
    updated_at_ms: now
  });

  // Line 2: Vorsteuer (Tax)
  await db.accounting_journal_entry_lines.add({
    id: entryId + '-l2',
    journal_entry_id: entryId,
    account_id: vorsteuerAcct?.id || '',
    debit: receipt.tax_amount,
    credit: 0,
    line_no: 2,
    updated_at_ms: now
  });

  // Line 3: Kreditoren (Gross)
  await db.accounting_journal_entry_lines.add({
    id: entryId + '-l3',
    journal_entry_id: entryId,
    account_id: payablesAcct?.id || '',
    debit: 0,
    credit: receipt.gross_amount,
    line_no: 3,
    updated_at_ms: now
  });

  // 3. Mark receipt as posted
  const rcptDoc = await db.accounting_receipts.byId(receiptId);
  if (rcptDoc) {
    await db.accounting_receipts.change(rcptDoc.id, { status: 'posted' });
  }

  closeDrawer();
  switchView('journal');
  fibuNotice('booking.posted', t('postedSuccess'));
};

window.matchBankLineDirectly = async (lineId, receiptId) => {
  const db = fibuDb();
  if (!db) return;

  const line = state.bankStatementLines.find(l => l.id === lineId);
  const receipt = state.receipts.find(r => r.id === receiptId);
  if (!line || !receipt) return;

  // First, post the receipt to Fibu if not already posted
  if (receipt.status !== 'posted') {
    await postReceiptDirectly(receiptId);
  }

  // Link bank statement line to the posted transaction
  const lineDoc = await db.accounting_bank_statement_lines.byId(lineId);
  if (lineDoc) {
    await db.accounting_bank_statement_lines.change(lineDoc.id, {
      match_status: 'matched',
      reconciled_entry_id: receiptId
    });
  }

  closeDrawer();
  switchView('banking');
  fibuNotice('bank.matched', t('matchSuccess'));
};

window.fibuCopyArchivePath = async (path) => {
  // Erfolg erst nach bestätigtem Schreiben melden; verweigerte Zwischenablage ist ein Fehler in der Zone statt einer
  // unbehandelten Ablehnung mit falscher Erfolgsmeldung (Rundgang RUND1).
  state.api?.status.clear?.('buchhaltung.archive.copied');
  state.api?.status.clear?.('buchhaltung.archive.copy_failed');
  try {
    if (!navigator.clipboard?.writeText) throw new Error('Zwischenablage nicht verfügbar'); // v4-allow: zone (Text geht über fibuNotice in api.status.report)
    await navigator.clipboard.writeText(String(path || ''));
  } catch (error) {
    const reason = error?.name === 'NotAllowedError' ? 'Zugriff auf die Zwischenablage verweigert' : (error?.message || String(error));
    fibuNotice('archive.copy_failed', `Archivpfad nicht kopiert: ${reason}`, 'error');
    return;
  }
  fibuNotice('archive.copied', 'Archivpfad in die Zwischenablage kopiert!');
};

window.triggerStorno = async (entryId) => {
  const db = fibuDb();
  if (!db) return;

  const entry = state.journalEntries.find(e => e.id === entryId);
  if (!entry || entry.reversed_by_id) return;

  const confirmed = await state.api.shell.dialog({
    kind: 'confirm',
    title: 'Buchung stornieren',
    text: 'Es wird eine Gegenbuchung (Storno) erzeugt.',
    confirm_label: 'Stornieren',
    default: false,
  });
  if (confirmed === true) {
    const lines = state.journalEntryLines.filter(l => l.journal_entry_id === entryId);

    const nextNum = 'J-2026-' + String(state.journalEntries.filter(e => e.posted_at).length + 1).padStart(4, '0');
    const stornoId = 'storno-' + Math.random().toString(36).substring(2, 9);
    const now = Date.now();

    // 1. Insert Storno Entry
    await db.accounting_journal_entries.add({
      id: stornoId,
      posting_date: entry.posting_date,
      type: 'storno',
      ref_type: 'storno',
      ref_id: entryId,
      number: nextNum,
      narration: `STORNO: ${entry.narration}`,
      posted_at: now,
      updated_at_ms: now
    });

    // 2. Insert Inverted Lines (Soll/Haben reversed)
    for (let i = 0; i < lines.length; i++) {
      const l = lines[i];
      await db.accounting_journal_entry_lines.add({
        id: `${stornoId}-l${i}`,
        journal_entry_id: stornoId,
        account_id: l.account_id,
        debit: l.credit, // Debit becomes credit
        credit: l.debit, // Credit becomes debit
        line_no: l.line_no,
        updated_at_ms: now
      });
    }

    // 3. Mark original entry as reversed
    const entryDoc = await db.accounting_journal_entries.byId(entryId);
    if (entryDoc) {
      await db.accounting_journal_entries.change(entryDoc.id, { reversed_by_id: stornoId });
    }

    closeDrawer();
    switchView('journal');
    fibuNotice('booking.storno', t('stornoSuccess'));
  }
};

// =========================================================================
// 📥 DATEV EXTF CSV-Stapel-Exporter (v7.0/v8.0 Spezifikation)
// =========================================================================
function triggerDatevExport() {
  const start = state.els.datevStart.value;
  const end = state.els.datevEnd.value;

  const csvContentStr = generateDatevCsvString(state.ledgerDF, start, end);
  if (!csvContentStr) {
    fibuNotice('datev.empty', 'Keine festgeschriebenen Buchungssätze im gewählten Datumsbereich vorhanden.', 'warn');
    return;
  }

  const blob = new Blob([csvContentStr], { type: "text/csv;charset=utf-8;" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.setAttribute("href", url);
  link.setAttribute("download", `DATEV_EXTF_${start}_${end}.csv`);
  link.style.display = "none";
  (state.root || document.documentElement).appendChild(link);
  link.click();
  setTimeout(() => {
    link.remove();
    URL.revokeObjectURL(url);
  }, 1000);
}

// Fragen an den Agenten: Standard-Kontext-Chat (Rechtsklick → ctox.chat.task, siehe dispatchBuchhaltungContextChat).
// Die frühere Chat-Attrappe der Evidenzspalte (feste Antworten, simuliertes Tippen) ist entfernt (BUCH.md).

function handleToolbarActions() {
  // Can be used for custom global button bindings
}

// =========================================================================
// 🧮 Pure Mathematical & Formatting Helpers
// =========================================================================
function formatCents(cents) {
  if (cents === undefined || cents === null) return '0,00 €';
  return (cents / 100).toLocaleString('de-DE', { style: 'currency', currency: 'EUR' });
}

// Deutsches Datum (TT.MM.JJJJ) aus ISO-Datum oder Millisekunden.
function formatDateDe(value) {
  if (value == null || value === '') return '';
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(String(value));
  if (m) return `${m[3]}.${m[2]}.${m[1]}`;
  const d = new Date(Number(value));
  return Number.isFinite(d.getTime()) ? d.toLocaleDateString('de-DE', { day: '2-digit', month: '2-digit', year: 'numeric' }) : String(value);
}

// Anteil (0..1 oder 0..100) als „99,4 %“.
function escapeHtml(str) {
  if (!str) return '';
  return String(str).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;").replace(/'/g, "&#039;");
}

function updateListCount(el, visible, total, label) {
  if (!el) return;
  el.textContent = visible === total
    ? `${total} ${label}`
    : `${visible} / ${total} ${label}`;
}

function setSelectedRow(container, attrName, selectedId) {
  if (!container) return;
  container.querySelectorAll('tr').forEach(tr => {
    const isSelected = tr.getAttribute(attrName) === selectedId;
    tr.classList.toggle('is-selected', isSelected);
    tr.setAttribute('aria-selected', String(isSelected));
  });
}

// =========================================================================
// ✈️ Reisekosten & Spesenabrechnung
// =========================================================================
window.renderTravelList = function() {
  const container = state.els.travelExpensesList;
  if (!container) return;

  const travels = state.journalEntries.filter(e => e.type === 'travel');

  if (travels.length === 0) {
    // Leere Liste rendert nichts (R11); kein Lade- oder Leertext.
    container.innerHTML = '';
    return;
  }

  let html = '';
  travels.forEach(t => {
    const isPosted = !!t.posted_at;
    let metadata = {};
    try {
      metadata = JSON.parse(t.ref_id || '{}');
    } catch(e) {}
    const totalAllowance = metadata.totalAllowance || 0;
    const dateRange = `${metadata.startDate || ''} bis ${metadata.endDate || ''}`;

    html += `
      <tr data-travel-click-id="${escapeHtml(t.id)}" data-context-record-id="${escapeHtml(t.id)}" data-context-record-type="accounting_journal_entry" data-context-label="${escapeHtml(t.narration || `Travel expense ${t.id}`)}">
        <td><strong>✈️ ${escapeHtml(t.narration)}</strong></td>
        <td><span class="fibu-mono">${dateRange}</span></td>
        <td class="is-num" style="font-weight: 600;">${formatCents(totalAllowance)}</td>
        <td>
          <span class="ctox-badge ${isPosted ? 'is-success' : ''}">
            ${isPosted ? 'Verbucht 🔒' : 'Entwurf'}
          </span>
        </td>
        <td class="is-center" onclick="event.stopPropagation();">
          ${!isPosted ? `<button class="ctox-button is-primary ctox-button--sm" onclick="postTravelEntryDirectly('${t.id}')">Buchen</button>` : '🔒'}
        </td>
      </tr>
    `;
  });

  container.innerHTML = html;

  container.querySelectorAll('tr').forEach(tr => {
    tr.addEventListener('click', () => {
      const id = tr.getAttribute('data-travel-click-id');
      openJournalEntryDrawer(id);
    });
  });
};

window.openNewTravelDrawer = function() {
  state.els.drawerTitle.textContent = `Neue Dienstreise erfassen`;
  openDrawer();

  const todayStr = new Date().toISOString().split('T')[0];

  let html = `
    <form id="fibu-new-travel-form" onsubmit="event.preventDefault(); saveTravelExpenseDraft();">
      <div class="fibu-form-row">
        <div class="fibu-form-group" style="flex:2;">
          <label class="ctox-field-label">Reisezweck / Destination</label>
          <input type="text" id="travel-purpose" class="ctox-input" placeholder="z.B. IT-Messe Berlin" required />
        </div>
      </div>
      <div class="fibu-form-row" style="margin-top:10px;">
        <div class="fibu-form-group">
          <label class="ctox-field-label">Beginn (Datum & Uhrzeit)</label>
          <input type="datetime-local" id="travel-start" class="ctox-input" value="${todayStr}T08:00" required onchange="updateTravelVmaCalculation()" />
        </div>
        <div class="fibu-form-group">
          <label class="ctox-field-label">Ende (Datum & Uhrzeit)</label>
          <input type="datetime-local" id="travel-end" class="ctox-input" value="${todayStr}T18:00" required onchange="updateTravelVmaCalculation()" />
        </div>
      </div>

      <div style="margin-top:15px;">
        <div class="fibu-form-group">
          <label class="ctox-field-label">Zahlungsweg</label>
          <select id="travel-payment-method" class="ctox-select">
            <option value="privat">Privat getragen (Privatauslage des Inhabers - Gegenbuchung auf 1890)</option>
            <option value="verrechnung">Gesellschafter-Verrechnungskonto (Gegenbuchung auf 1370)</option>
          </select>
        </div>
      </div>

      <div style="margin-top:20px;">
        <h4 class="ctox-field-label" style="margin-bottom:8px;">Verpflegungsmehraufwand</h4>
        <div id="travel-days-container" style="display:flex; flex-direction:column; gap:10px;">
          <!-- Days list with meal checkboxes loaded live here -->
        </div>
      </div>

      <div style="margin-top:20px; display:flex; justify-content:space-between; align-items:center; border-top:1px solid var(--line); padding-top:15px;">
        <div class="fibu-text-debit" style="font-size:14px; font-weight:700;">
          Erstattungsbetrag: <span id="travel-total-allowance-label">0,00 €</span>
        </div>
        <button type="submit" class="ctox-button is-primary">💾 Als Entwurf buchen</button>
      </div>
    </form>
  `;

  state.els.drawerContent.innerHTML = html;
  state.activeTravelDays = [];
  updateTravelVmaCalculation();
};

window.updateTravelVmaCalculation = function() {
  const startVal = document.getElementById('travel-start').value;
  const endVal = document.getElementById('travel-end').value;
  if (!startVal || !endVal) return;

  try {
    const generated = generateTravelDays(startVal, endVal);

    // Merge meal preferences if they already existed for these dates
    state.activeTravelDays = generated.map(newDay => {
      const existing = state.activeTravelDays.find(d => d.date === newDay.date);
      if (existing) {
        newDay.breakfast = existing.breakfast;
        newDay.lunch = existing.lunch;
        newDay.dinner = existing.dinner;
      }
      return newDay;
    });

    const res = calculateTotalTravelAllowance(state.activeTravelDays);
    document.getElementById('travel-total-allowance-label').textContent = formatCents(res.totalAllowance);

    // Render days checklist
    const container = document.getElementById('travel-days-container');
    if (!container) return;

    let daysHtml = '';
    state.activeTravelDays.forEach((day, idx) => {
      const dateFormatted = new Date(day.date).toLocaleDateString('de-DE', { weekday: 'short', day: '2-digit', month: '2-digit' });
      const typeLabel = day.type === 'single' ? 'Eintägig' : (day.type === 'arrival' ? 'Anreise' : (day.type === 'departure' ? 'Abreise' : 'Ganztägig'));
      const dayAllowance = res.breakdown[idx].allowance;

      daysHtml += `
        <div style="display:flex; justify-content:space-between; align-items:center; background:var(--surface); padding:8px 12px; border-radius:6px; border:1px solid var(--line);">
          <div>
            <div style="font-weight:700; color:var(--text-strong); font-size:12.5px;">${dateFormatted} (${typeLabel})</div>
            <div style="font-size:11px; color: var(--muted);">Pauschale: ${formatCents(dayAllowance)}</div>
          </div>
          <div style="display:flex; gap:12px; font-size:11.5px;">
            <label style="display:flex; align-items:center; gap:4px; cursor:pointer;">
              <input type="checkbox" ${day.breakfast ? 'checked' : ''} onchange="toggleTravelMeal(${idx}, 'breakfast')" /> Fr. (-20%)
            </label>
            <label style="display:flex; align-items:center; gap:4px; cursor:pointer;">
              <input type="checkbox" ${day.lunch ? 'checked' : ''} onchange="toggleTravelMeal(${idx}, 'lunch')" /> Mi. (-40%)
            </label>
            <label style="display:flex; align-items:center; gap:4px; cursor:pointer;">
              <input type="checkbox" ${day.dinner ? 'checked' : ''} onchange="toggleTravelMeal(${idx}, 'dinner')" /> Ab. (-40%)
            </label>
          </div>
        </div>
      `;
    });

    container.innerHTML = daysHtml;
  } catch(e) {
    const container = document.getElementById('travel-days-container');
    if (container) {
      container.innerHTML = `<div class="fibu-text-credit" style="font-size:12px;">Ungültiger Zeitraum: ${escapeHtml(e.message)}</div>`;
    }
  }
};

window.toggleTravelMeal = function(idx, meal) {
  if (state.activeTravelDays[idx]) {
    state.activeTravelDays[idx][meal] = !state.activeTravelDays[idx][meal];
    updateTravelVmaCalculation();
  }
};

window.saveTravelExpenseDraft = async function() {
  const db = fibuDb();
  if (!db) return;

  const purpose = document.getElementById('travel-purpose').value;
  const startVal = document.getElementById('travel-start').value;
  const endVal = document.getElementById('travel-end').value;
  const paymentMethod = document.getElementById('travel-payment-method').value;

  const res = calculateTotalTravelAllowance(state.activeTravelDays);
  if (res.totalAllowance <= 0) {
    fibuNotice('travel.zero', 'Verpflegungsmehraufwand 0,00 € – keine Buchung möglich.', 'warn');
    return;
  }

  const travelId = 'trv-' + Math.random().toString(36).substring(2, 9);
  const now = Date.now();

  const metadata = {
    startDate: startVal.split('T')[0],
    endDate: endVal.split('T')[0],
    totalAllowance: res.totalAllowance,
    days: state.activeTravelDays
  };

  // 1. Insert Journal Entry with travel type
  await db.accounting_journal_entries.add({
    id: travelId,
    posting_date: startVal.split('T')[0],
    type: 'travel',
    ref_type: 'travel',
    ref_id: JSON.stringify(metadata),
    number: '', // Drafts have empty number
    narration: `Reisekostenabrechnung: ${purpose}`,
    posted_at: 0,
    updated_at_ms: now
  });

  // 2. Soll: Reisekosten (4660 / 6670)
  const expenseCode = state.skrName === 'SKR03' ? '4660' : '6670';
  const expenseAcct = state.accounts.find(a => a.code === expenseCode);

  await db.accounting_journal_entry_lines.add({
    id: `${travelId}-l1`,
    journal_entry_id: travelId,
    account_id: expenseAcct?.id || '',
    debit: res.totalAllowance,
    credit: 0,
    line_no: 1,
    updated_at_ms: now
  });

  // 3. Haben: Privateinlage / Verrechnung
  const contraCode = paymentMethod === 'privat'
    ? (state.skrName === 'SKR03' ? '1890' : '2180')
    : (state.skrName === 'SKR03' ? '1370' : '1486');
  const contraAcct = state.accounts.find(a => a.code === contraCode);

  await db.accounting_journal_entry_lines.add({
    id: `${travelId}-l2`,
    journal_entry_id: travelId,
    account_id: contraAcct?.id || '',
    debit: 0,
    credit: res.totalAllowance,
    line_no: 2,
    updated_at_ms: now
  });

  closeDrawer();
  loadAllFibuData().then(() => {
    switchView('travel');
    fibuNotice('travel.draft', 'Reisekostenabrechnung erfolgreich als Entwurf angelegt!');
  });
};

window.postTravelEntryDirectly = async function(entryId) {
  const db = fibuDb();
  if (!db || !db.accounting_journal_entries) {
    console.warn('[fibu] accounting_journal_entries collection not ready yet.');
    return;
  }

  const entry = state.journalEntries.find(e => e.id === entryId);
  if (!entry) return;

  const now = Date.now();
  const nextNum = 'J-2026-' + String(state.journalEntries.filter(e => e.posted_at).length + 1).padStart(4, '0');

  const doc = await db.accounting_journal_entries.byId(entryId);
  if (doc) {
    await db.accounting_journal_entries.change(doc.id, {
      number: nextNum,
      posted_at: now,
      updated_at_ms: now
    });
  }

  loadAllFibuData().then(() => {
    switchView('travel');
    fibuNotice('travel.posted', 'Reisekostenabrechnung erfolgreich verbucht und GoBD-festgeschrieben!');
  });
};


// =========================================================================
// 🚗 Fahrtenbuch & Kilometererstattung
// =========================================================================
window.renderMileageList = function() {
  const actualContainer = state.els.mileageLogList;
  if (!actualContainer) return;

  const trips = state.journalEntries.filter(e => e.type === 'mileage');

  if (trips.length === 0) {
    // Leere Liste rendert nichts (R11); kein Lade- oder Leertext.
    actualContainer.innerHTML = '';
    updateMileageDashboard();
    return;
  }

  let html = '';
  trips.forEach(t => {
    const isPosted = !!t.posted_at;
    let metadata = {};
    try {
      metadata = JSON.parse(t.ref_id || '{}');
    } catch(e) {}
    const km = metadata.km || 0;
    const reimbursement = metadata.reimbursement || 0;
    const typeLabel = metadata.purpose === 'business' ? 'Geschäftlich' : (metadata.purpose === 'private' ? 'Privat' : 'Arbeitsweg');

    html += `
      <tr data-mileage-click-id="${escapeHtml(t.id)}" data-context-record-id="${escapeHtml(t.id)}" data-context-record-type="accounting_journal_entry" data-context-label="${escapeHtml(t.narration || metadata.destination || `Mileage entry ${t.id}`)}">
        <td><span class="fibu-mono">${formatDateDe(t.posting_date)}</span></td>
        <td><strong>🚗 ${escapeHtml(metadata.destination || '')}</strong> <div style="font-size:11px; color: var(--muted);">Zweck: ${escapeHtml(t.narration || '')}</div></td>
        <td>${escapeHtml(metadata.contactPerson || '—')}</td>
        <td class="fibu-mono is-num">${km} km</td>
        <td class="is-num" style="font-weight: 600;">${formatCents(reimbursement)}</td>
        <td><span class="ctox-badge ${metadata.purpose === 'business' ? 'is-success' : ''}">${typeLabel}</span></td>
        <td>
          <span class="ctox-badge ${isPosted ? 'is-success' : ''}">
            ${isPosted ? 'Verbucht 🔒' : 'Entwurf'}
          </span>
        </td>
        <td class="is-center" onclick="event.stopPropagation();">
          ${!isPosted && metadata.purpose === 'business' ? `<button class="ctox-button is-primary ctox-button--sm" onclick="postMileageEntryDirectly('${t.id}')">Buchen</button>` : (isPosted ? '🔒' : '—')}
        </td>
      </tr>
    `;
  });

  actualContainer.innerHTML = html;
  updateMileageDashboard();

  actualContainer.querySelectorAll('tr').forEach(tr => {
    tr.addEventListener('click', () => {
      const id = tr.getAttribute('data-mileage-click-id');
      openJournalEntryDrawer(id);
    });
  });
};

window.updateMileageDashboard = function() {
  const dash = document.getElementById('mileage-dashboard-summary');
  if (!dash) return;

  const trips = state.journalEntries.filter(e => e.type === 'mileage').map(t => {
    try {
      return JSON.parse(t.ref_id || '{}');
    } catch(e) {
      return {};
    }
  });

  const shares = calculateAnnualUsageShares(trips);

  dash.innerHTML = `
    <div class="mileage-dash-wrapper">
      <div class="mileage-dash-metric main-metric">
        <span class="mileage-metric-label">Gesamtlaufleistung</span>
        <span class="mileage-metric-value">${shares.totalKm} <span class="unit">km</span></span>
      </div>
      <div class="mileage-dash-divider"></div>
      <div class="mileage-dash-shares">
        <div class="mileage-share-item business">
          <span class="share-label">
            <span class="share-color-dot"></span>Geschäftlich
          </span>
          <span class="share-val">${shares.ratios.business}% <span class="share-sub">(${shares.businessKm} km)</span></span>
        </div>
        <div class="mileage-share-item private">
          <span class="share-label">
            <span class="share-color-dot"></span>Privat
          </span>
          <span class="share-val">${shares.ratios.private}% <span class="share-sub">(${shares.privateKm} km)</span></span>
        </div>
        <div class="mileage-share-item commute">
          <span class="share-label">
            <span class="share-color-dot"></span>Arbeitsweg
          </span>
          <span class="share-val">${shares.ratios.commute}% <span class="share-sub">(${shares.commuteKm} km)</span></span>
        </div>
      </div>
    </div>
  `;
};

window.openNewMileageDrawer = function() {
  state.els.drawerTitle.textContent = `Neue Fahrt im Fahrtenbuch eintragen`;
  openDrawer();

  const todayStr = new Date().toISOString().split('T')[0];

  let html = `
    <form id="fibu-new-mileage-form" onsubmit="event.preventDefault(); saveMileageLogDraft();">
      <div class="fibu-form-row">
        <div class="fibu-form-group">
          <label class="ctox-field-label">Datum</label>
          <input type="date" id="mileage-date" class="ctox-input" value="${todayStr}" required />
        </div>
        <div class="fibu-form-group">
          <label class="ctox-field-label">Fahrttyp</label>
          <select id="mileage-purpose-type" class="ctox-select" onchange="updateMileageReimbursementLive()">
            <option value="business">Geschäftlich (Kilometerpauschale 0,30 €)</option>
            <option value="private">Privatfahrt (Keine Erstattung)</option>
            <option value="commute">Arbeitsweg (Wohnung/Arbeitsstätte)</option>
          </select>
        </div>
      </div>
      <div class="fibu-form-row">
        <div class="fibu-form-group">
          <label class="ctox-field-label">Anfangs-Kilometerstand</label>
          <input type="number" id="mileage-start-km" class="ctox-input" placeholder="z.B. 12400" required oninput="updateMileageReimbursementLive()" />
        </div>
        <div class="fibu-form-group">
          <label class="ctox-field-label">End-Kilometerstand</label>
          <input type="number" id="mileage-end-km" class="ctox-input" placeholder="z.B. 12550" required oninput="updateMileageReimbursementLive()" />
        </div>
      </div>
      <div class="fibu-form-row">
        <div class="fibu-form-group" style="flex:2;">
          <label class="ctox-field-label">Route / Reiseziel (Start -> Ziel)</label>
          <input type="text" id="mileage-route" class="ctox-input" placeholder="Büro -> Kundengespräch Berlin -> Büro" required />
        </div>
        <div class="fibu-form-group">
          <label class="ctox-field-label">Besuchter Partner / Kontakt</label>
          <input type="text" id="mileage-contact" class="ctox-input" placeholder="Firma Müller GmbH" />
        </div>
      </div>
      <div class="fibu-form-row">
        <div class="fibu-form-group" style="flex:2;">
          <label class="ctox-field-label">Fahrtzweck / Notiz</label>
          <input type="text" id="mileage-purpose-detail" class="ctox-input" placeholder="Kundenpräsentation & Vertragsverhandlung" required />
        </div>
      </div>

      <div id="mileage-reimbursement-payment-wrap" style="margin-top:15px;">
        <div class="fibu-form-group">
          <label class="ctox-field-label">Zahlungsweg</label>
          <select id="mileage-payment-method" class="ctox-select">
            <option value="privat">Privatauslage des Inhabers (Einlage - Gegenbuchung auf 1890)</option>
            <option value="verrechnung">Gesellschafter-Verrechnungskonto (Gegenbuchung auf 1370)</option>
          </select>
        </div>
      </div>

      <div style="margin-top:20px; display:flex; justify-content:space-between; align-items:center; border-top:1px solid var(--line); padding-top:15px;">
        <div class="fibu-text-debit" style="font-size:14px; font-weight:700;">
          Erstattungssumme: <span id="mileage-reimbursement-label">0,00 €</span>
        </div>
        <button type="submit" class="ctox-button is-primary">💾 Als Fahrt buchen</button>
      </div>
    </form>
  `;

  state.els.drawerContent.innerHTML = html;
  updateMileageReimbursementLive();
};

window.updateMileageReimbursementLive = function() {
  const startKm = parseFloat(document.getElementById('mileage-start-km').value || 0);
  const endKm = parseFloat(document.getElementById('mileage-end-km').value || 0);
  const purpose = document.getElementById('mileage-purpose-type').value;

  const diff = Math.max(0, endKm - startKm);
  const reimbursement = purpose === 'business' ? calculateMileageReimbursement(diff) : 0;

  document.getElementById('mileage-reimbursement-label').textContent = formatCents(reimbursement);

  const wrap = document.getElementById('mileage-reimbursement-payment-wrap');
  if (wrap) {
    wrap.style.display = purpose === 'business' ? 'block' : 'none';
  }
};

window.saveMileageLogDraft = async function() {
  const db = fibuDb();
  if (!db) return;

  const date = document.getElementById('mileage-date').value;
  const purposeType = document.getElementById('mileage-purpose-type').value;
  const startKm = parseFloat(document.getElementById('mileage-start-km').value || 0);
  const endKm = parseFloat(document.getElementById('mileage-end-km').value || 0);
  const route = document.getElementById('mileage-route').value;
  const contact = document.getElementById('mileage-contact').value;
  const purposeDetail = document.getElementById('mileage-purpose-detail').value;
  const paymentMethod = document.getElementById('mileage-payment-method').value;

  const diff = endKm - startKm;
  if (diff <= 0) {
    fibuNotice('mileage.invalid', 'Der End-Kilometerstand muss größer als der Anfangs-Kilometerstand sein.', 'warn');
    return;
  }

  const reimbursement = purposeType === 'business' ? calculateMileageReimbursement(diff) : 0;
  const entryId = 'mil-' + Math.random().toString(36).substring(2, 9);
  const now = Date.now();

  const metadata = {
    date,
    purpose: purposeType,
    startKm,
    endKm,
    km: diff,
    destination: route,
    contactPerson: contact,
    reimbursement
  };

  // 1. Insert Journal Entry
  await db.accounting_journal_entries.add({
    id: entryId,
    posting_date: date,
    type: 'mileage',
    ref_type: 'mileage',
    ref_id: JSON.stringify(metadata),
    number: '',
    narration: purposeDetail,
    posted_at: 0,
    updated_at_ms: now
  });

  // Only create ledger lines for business travels which are reimbursed!
  if (purposeType === 'business') {
    // 2. Soll: Fahrtkosten (4673 / 6680)
    const expenseCode = state.skrName === 'SKR03' ? '4673' : '6680';
    const expenseAcct = state.accounts.find(a => a.code === expenseCode);

    await db.accounting_journal_entry_lines.add({
      id: `${entryId}-l1`,
      journal_entry_id: entryId,
      account_id: expenseAcct?.id || '',
      debit: reimbursement,
      credit: 0,
      line_no: 1,
      updated_at_ms: now
    });

    // 3. Haben: Privateinlage / Verrechnung
    const contraCode = paymentMethod === 'privat'
      ? (state.skrName === 'SKR03' ? '1890' : '2180')
      : (state.skrName === 'SKR03' ? '1370' : '1486');
    const contraAcct = state.accounts.find(a => a.code === contraCode);

    await db.accounting_journal_entry_lines.add({
      id: `${entryId}-l2`,
      journal_entry_id: entryId,
      account_id: contraAcct?.id || '',
      debit: 0,
      credit: reimbursement,
      line_no: 2,
      updated_at_ms: now
    });
  }

  closeDrawer();
  loadAllFibuData().then(() => {
    switchView('mileage');
    fibuNotice('mileage.saved', 'Fahrt erfolgreich im Fahrtenbuch eingetragen!');
  });
};

window.postMileageEntryDirectly = async function(entryId) {
  const db = fibuDb();
  if (!db) return;

  const entry = state.journalEntries.find(e => e.id === entryId);
  if (!entry) return;

  const now = Date.now();
  const nextNum = 'J-2026-' + String(state.journalEntries.filter(e => e.posted_at).length + 1).padStart(4, '0');

  const doc = await db.accounting_journal_entries.byId(entryId);
  if (doc) {
    await db.accounting_journal_entries.change(doc.id, {
      number: nextNum,
      posted_at: now,
      updated_at_ms: now
    });
  }

  loadAllFibuData().then(() => {
    switchView('mileage');
    fibuNotice('mileage.posted', 'Dienstreise-Erstattung erfolgreich verbucht und GoBD-festgeschrieben!');
  });
};

window.triggerAnnualPrivateCarShare = function() {
  const trips = state.journalEntries.filter(e => e.type === 'mileage').map(t => {
    try {
      return JSON.parse(t.ref_id || '{}');
    } catch(e) {
      return {};
    }
  });

  const shares = calculateAnnualUsageShares(trips);
  if (shares.totalKm <= 0) {
    fibuNotice('mileage.none', 'Keine Fahrten vorhanden, um den jährlichen Privatanteil zu berechnen.', 'warn');
    return;
  }

  state.els.drawerTitle.textContent = `Jährliche Firmenwagen-Privatnutzung versteuern`;
  openDrawer();

  // Suggest standard 1% method or actual private log split
  const defaultVal = shares.privateKm * 0.30;

  let html = `
    <div style="padding:4px 0;">
      <p style="font-size:12.5px; margin-bottom:12px;">Laut Fahrtenbuch beträgt der private Fahrtenanteil dieses Jahr <strong>${shares.ratios.private}%</strong> (${shares.privateKm} km).</p>

      <div class="fibu-form-row">
        <div class="fibu-form-group">
          <label class="ctox-field-label">Berechnungsmethode</label>
          <select id="car-share-method" class="ctox-select" onchange="updateCarShareLiveCalculations()">
            <option value="logbook">Fahrtenbuch-Methode (Ist-Kosten-Anteil: Privat-km * 0,30 €)</option>
            <option value="one_percent">1%-Pauschalmethode (30.000 € Listenpreis * 1% = 300,00 €)</option>
          </select>
        </div>
      </div>

      <div class="fibu-form-row">
        <div class="fibu-form-group">
          <label class="ctox-field-label">Zu versteuernder Betrag (€)</label>
          <input type="number" step="0.01" id="car-share-amount" value="${defaultVal.toFixed(2)}" class="ctox-input" oninput="updateCarShareLiveCalculations()" />
        </div>
      </div>

      <div style="margin-top:15px;">
        <h4 class="ctox-field-label" style="margin:0 0 8px 0;">Buchungssatz (inkl. 19 % USt)</h4>
        <div style="font-size:12px; margin-bottom:10px;">
          Soll: <strong>${state.skrName === 'SKR03' ? '1800' : '2100'} (Privatentnahme)</strong> = <span id="car-share-gross-label">0,00 €</span><br/>
          Haben: <strong>${state.skrName === 'SKR03' ? '8921' : '4645'} (Verw. Gegenstände ohne USt)</strong> = <span id="car-share-net-label">0,00 €</span><br/>
          Haben: <strong>${state.skrName === 'SKR03' ? '1776' : '3806'} (Umsatzsteuer 19%)</strong> = <span id="car-share-tax-label">0,00 €</span>
        </div>
      </div>

      <div style="margin-top:20px; display:flex; justify-content:flex-end;">
        <button class="ctox-button is-primary" onclick="postAnnualCarShare()">✔️ Steuererklärung abschließen & buchen</button>
      </div>
    </div>
  `;

  state.els.drawerContent.innerHTML = html;
  window.updateCarShareLiveCalculations();
};

window.updateCarShareLiveCalculations = function() {
  const method = document.getElementById('car-share-method').value;
  const inputEl = document.getElementById('car-share-amount');

  if (method === 'one_percent') {
    inputEl.value = "300.00";
    inputEl.disabled = true;
  } else {
    inputEl.disabled = false;
  }

  const amtVal = parseFloat(inputEl.value || 0);
  const netCents = Math.round((amtVal / 1.19) * 100);
  const grossCents = Math.round(amtVal * 100);
  const taxCents = grossCents - netCents;

  document.getElementById('car-share-gross-label').textContent = formatCents(grossCents);
  document.getElementById('car-share-net-label').textContent = formatCents(netCents);
  document.getElementById('car-share-tax-label').textContent = formatCents(taxCents);
};

window.postAnnualCarShare = async function() {
  const db = fibuDb();
  if (!db) {
    closeDrawer();
    return;
  }

  const amtVal = parseFloat(document.getElementById('car-share-amount').value || 0);
  if (amtVal <= 0) {
    closeDrawer();
    return;
  }

  const grossCents = Math.round(amtVal * 100);
  const netCents = Math.round((amtVal / 1.19) * 100);
  const taxCents = grossCents - netCents;

  const entryId = 'car-' + Math.random().toString(36).substring(2, 9);
  const now = Date.now();
  const nextNum = 'J-2026-' + String(state.journalEntries.filter(e => e.posted_at).length + 1).padStart(4, '0');

  // 1. Insert Journal Entry
  await db.accounting_journal_entries.add({
    id: entryId,
    posting_date: '2026-12-31',
    type: 'mileage_share',
    ref_type: 'mileage_annual',
    ref_id: String(grossCents),
    number: nextNum,
    narration: 'Jahres-Privatnutzung Firmenwagen versteuert (Ist-Versteuerung)',
    posted_at: now,
    updated_at_ms: now
  });

  // 2. Soll: Privatentnahme (1800 / 2100)
  const privatCode = state.skrName === 'SKR03' ? '1800' : '2100';
  const privatAcct = state.accounts.find(a => a.code === privatCode);
  await db.accounting_journal_entry_lines.add({
    id: `${entryId}-l1`,
    journal_entry_id: entryId,
    account_id: privatAcct?.id || '',
    debit: grossCents,
    credit: 0,
    line_no: 1,
    updated_at_ms: now
  });

  // 3. Haben: Verwendung von Gegenständen (8921 / 4645)
  const useCode = state.skrName === 'SKR03' ? '8921' : '4645';
  const useAcct = state.accounts.find(a => a.code === useCode);
  await db.accounting_journal_entry_lines.add({
    id: `${entryId}-l2`,
    journal_entry_id: entryId,
    account_id: useAcct?.id || '',
    debit: 0,
    credit: netCents,
    line_no: 2,
    updated_at_ms: now
  });

  // 4. Haben: Umsatzsteuer 19% (1776 / 3806)
  const ustCode = state.skrName === 'SKR03' ? '1776' : '3806';
  const ustAcct = state.accounts.find(a => a.code === ustCode);
  await db.accounting_journal_entry_lines.add({
    id: `${entryId}-l3`,
    journal_entry_id: entryId,
    account_id: ustAcct?.id || '',
    debit: 0,
    credit: taxCents,
    line_no: 3,
    updated_at_ms: now
  });

  closeDrawer();
  loadAllFibuData().then(() => {
    switchView('mileage');
    fibuNotice('car.private_posted', 'Jährliche Privatnutzung Firmenwagen erfolgreich verbucht!');
  });
};

function initBuchhaltungContextMenu(state) {
  state.contextMenu?.remove();
  const menu = document.createElement('div');
  menu.className = 'ctox-context-menu buchhaltung-context-menu';
  menu.hidden = true;
  (state.els.root || state.root)?.append(menu);
  state.contextMenu = menu;

  const handleContextMenu = (event) => {
    const context = buchhaltungCommandContextFromElement(state, event.target);
    event.preventDefault();
    event.stopPropagation();
    renderBuchhaltungContextMenu(state, context, event.clientX, event.clientY);
  };
  const handleOutsideClick = (event) => {
    if (state.contextMenu?.contains(event.target)) return;
    hideBuchhaltungContextMenu(state);
  };
  const handleEscape = (event) => {
    if (event.key === 'Escape') hideBuchhaltungContextMenu(state);
  };

  window.addEventListener('click', handleOutsideClick, { capture: true });
  window.addEventListener('keydown', handleEscape);

  return () => {
    window.removeEventListener('click', handleOutsideClick, { capture: true });
    window.removeEventListener('keydown', handleEscape);
    hideBuchhaltungContextMenu(state);
    state.contextMenu?.remove();
    state.contextMenu = null;
  };
}

function hideBuchhaltungContextMenu(state) {
  if (state.contextMenu) state.contextMenu.hidden = true;
}

function canModifyBuchhaltungApp(state) {
  // Spiegel für die Darstellung (§10.2a Nr. 3); die Mutation entscheidet der Core.
  const actor = state.api?.actor;
  if (actor?.permissions?.get?.()?.includes?.('apps.source.edit')) return true;
  const role = String(actor?.role || 'user').trim().toLowerCase().replace(/^business_os_/, '');
  return ['admin', 'chef'].includes(role);
}

function getBuchhaltungText(state, key, fallback) {
  if (typeof t === 'function') {
    const res = t(key);
    if (res && res !== key) return res;
  }
  return fallback;
}

function buchhaltungCommandContextFromElement(state, target) {
  const element = target?.nodeType === Node.ELEMENT_NODE ? target : target?.parentElement;
  let recordType = 'module';
  let recordId = '';
  let label = '';
  let bodySnippet = '';

  const activeNav = state.activeNav || 'skr';
  if (activeNav === 'skr' && state.selectedAccountId) {
    const acct = state.accounts.find((a) => a.id === state.selectedAccountId);
    if (acct) {
      recordType = 'account';
      recordId = acct.id;
      label = `${acct.code} - ${acct.name}`;
      bodySnippet = `Type: ${acct.account_type}, Debit Saldo: ${acct.debit_saldo || 0}, Credit Saldo: ${acct.credit_saldo || 0}`;
    }
  } else if (activeNav === 'journal' && state.selectedEntryId) {
    const entry = state.journalEntries.find((e) => e.id === state.selectedEntryId);
    if (entry) {
      recordType = 'entry';
      recordId = entry.id;
      label = entry.number || entry.id;
      bodySnippet = `Narration: ${entry.narration || ''}, Posted: ${!!entry.posted_at}`;
    }
  } else if (activeNav === 'receipts' && state.selectedReceiptId) {
    const receipt = state.receipts.find((r) => r.id === state.selectedReceiptId);
    if (receipt) {
      recordType = 'receipt';
      recordId = receipt.id;
      label = receipt.document_number || receipt.id;
      bodySnippet = `Supplier: ${receipt.supplier_name || ''}, Net: ${receipt.amount_net_cents || 0}, Tax: ${receipt.amount_tax_cents || 0}`;
    }
  } else if (activeNav === 'banking' && state.selectedBankLineId) {
    const line = state.bankStatementLines.find((l) => l.id === state.selectedBankLineId);
    if (line) {
      recordType = 'bank_line';
      recordId = line.id;
      label = line.purpose || line.id;
      bodySnippet = `Partner: ${line.partner_name || ''}, Amount: ${line.amount_cents || 0}, Status: ${line.status || ''}`;
    }
  } else if (activeNav === 'assets' && state.selectedAssetId) {
    const asset = state.assets?.find?.((a) => a.id === state.selectedAssetId);
    if (asset) {
      recordType = 'asset';
      recordId = asset.id;
      label = asset.name || asset.id;
      bodySnippet = `Cost: ${asset.acquisition_cost_cents || 0}, Useful Life: ${asset.useful_life_years || 0}`;
    }
  }

  return {
    module: 'buchhaltung',
    column: activeNav,
    record_type: recordType,
    record_id: recordId,
    label: label || 'Buchhaltung',
    body_snippet: bodySnippet,
    selected_text: String(window.getSelection?.()?.toString?.() || '').trim().slice(0, 1000),
    clicked_text: String(element?.innerText || element?.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 500),
  };
}

function renderBuchhaltungContextMenu(state, context, x, y) {
  const canModifyApp = canModifyBuchhaltungApp(state);
  const chatToCtoxText = getBuchhaltungText(state, 'chatToCtox', 'Chat to CTOX');
  const closeText = getBuchhaltungText(state, 'close', 'Schließen');
  const chatActionLabel = getBuchhaltungText(state, 'chatActionLabel', 'CTOX Aufgabe');
  const chatWorkDataLabel = getBuchhaltungText(state, 'chatWorkDataLabel', 'Mit Daten arbeiten');
  const chatAnswerLabel = getBuchhaltungText(state, 'chatAnswerLabel', 'Frage beantworten');
  const chatModifyAppLabel = getBuchhaltungText(state, 'chatModifyAppLabel', 'App modifizieren');
  const chatPlaceholder = getBuchhaltungText(state, 'chatPlaceholder', 'Was soll CTOX hier tun oder prüfen?');
  const sendText = getBuchhaltungText(state, 'send', 'Senden');

  state.contextMenu.innerHTML = `
    <form class="buchhaltung-context-chat" data-buchhaltung-context-chat-form>
      <header>
        <div>
          <strong>${escapeHtml(chatToCtoxText)}</strong>
          <span>${escapeHtml(context.label || 'Buchhaltung')}</span>
        </div>
        <button type="button" data-buchhaltung-context-close aria-label="${escapeHtml(closeText)}">×</button>
      </header>
      <div class="ctox-context-mode" role="radiogroup" aria-label="${escapeHtml(chatActionLabel)}">
        ${contextChatModeMarkup({ canModifyApp, escape: escapeHtml, labelClass: 'ctox-field-label', labels: { data: chatWorkDataLabel, ask: chatAnswerLabel, app: chatModifyAppLabel } })}
      </div>
      <textarea data-buchhaltung-context-message placeholder="${escapeHtml(chatPlaceholder)}"></textarea>
      <footer>
        <button type="submit">${escapeHtml(sendText)}</button>
      </footer>
    </form>
  `;
  state.contextMenu.hidden = false;
  state.contextMenu.style.left = '0px';
  state.contextMenu.style.top = '0px';
  const rect = state.contextMenu.getBoundingClientRect();
  const clampNumber = (val, min, max) => Math.min(max, Math.max(min, val));
  const rootRect = state.els.root?.getBoundingClientRect?.() || { left: 0, top: 0, width: window.innerWidth, height: window.innerHeight };
  const maxLeft = Math.max(8, rootRect.width - rect.width - 8);
  const maxTop = Math.max(8, rootRect.height - rect.height - 8);
  state.contextMenu.style.left = `${clampNumber(x - rootRect.left, 8, maxLeft)}px`;
  state.contextMenu.style.top = `${clampNumber(y - rootRect.top, 8, maxTop)}px`;

  const form = state.contextMenu.querySelector('[data-buchhaltung-context-chat-form]');
  const textarea = state.contextMenu.querySelector('[data-buchhaltung-context-message]');
  state.contextMenu.querySelector('[data-buchhaltung-context-close]')?.addEventListener('click', () => hideBuchhaltungContextMenu(state));
  form?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const mode = new FormData(form).get('contextMode') || 'data';
    await dispatchBuchhaltungContextChat(state, context, textarea?.value || '', mode);
  });
  requestAnimationFrame(() => textarea?.focus());
}

async function dispatchBuchhaltungContextChat(state, context, message, mode = 'data') {
  const trimmed = String(message || '').trim();
  if (!trimmed) {
    state.api?.status.report({ level: 'warn', code: 'buchhaltung.chat.message_missing', text: getBuchhaltungText(state, 'chatMissingMessage', 'Nachricht fehlt.') });
    return;
  }
  state.api?.status.clear('buchhaltung.chat.message_missing');

  const canModifyApp = canModifyBuchhaltungApp(state);
  const safeMode = normalizeContextChatMode(mode, { canModifyApp });
  const titlePrefix = safeMode === 'app'
    ? 'Buchhaltung App modifizieren'
    : safeMode === 'ask'
      ? getBuchhaltungText(state, 'chatAnswerLabel', 'Frage beantworten')
      : 'Fibu bearbeiten';
  const title = `${titlePrefix} · ${context.label || 'Buchhaltung'}`;
  let activeRecord = null;
  const activeNav = state.activeNav || 'skr';
  if (activeNav === 'skr' && state.selectedAccountId) {
    activeRecord = state.accounts.find((a) => a.id === state.selectedAccountId);
  } else if (activeNav === 'journal' && state.selectedEntryId) {
    activeRecord = state.journalEntries.find((e) => e.id === state.selectedEntryId);
  } else if (activeNav === 'receipts' && state.selectedReceiptId) {
    activeRecord = state.receipts.find((r) => r.id === state.selectedReceiptId);
  } else if (activeNav === 'banking' && state.selectedBankLineId) {
    activeRecord = state.bankStatementLines.find((l) => l.id === state.selectedBankLineId);
  } else if (activeNav === 'assets' && state.selectedAssetId) {
    activeRecord = state.assets?.find?.((a) => a.id === state.selectedAssetId);
  }

  // Kontext-Auftrag an die Chat-App ctox über den Baustein (Strang SKL-D, Kern §16.40 commands_out): Anweisungstext,
  // Skill und Rückweg liegen bei den Handlern ctox.chat.task / ctox.app.modify.request.
  sendContextChat(state.api, {
    source_app: 'buchhaltung',
    record_ref: activeRecord?.id || context.record_id || '',
    selection: context.selected_text || '',
    text: trimmed,
    mode: safeMode,
    canModifyApp,
    title,
    thread_key: 'business-os/buchhaltung',
    context: { ...context, active_nav: activeNav },
  }, { send: (type, payload, opts) => dispatchFibuCommand(type, { ...payload, record_id: opts.record_id, client_context: { action: 'context-chat', mode: safeMode, source_module: 'buchhaltung' } }), navigate: false });
  hideBuchhaltungContextMenu(state);
}
