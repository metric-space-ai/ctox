import { loadModuleMessages } from '../../shared/i18n.js';
import { importTemplateToDb } from './templates/skr.js';
import { showBusinessConfirm } from '../../shared/dialogs.js';


// --- Native ES Module Imports for Fibu Core Engines ---
import { validateDoubleEntry, computeAccountSalden as computeAccountSaldenImported } from './core/ledger.js';
import { normalizedDelta, computeDepreciationSchedule } from './core/depreciation.js';
import { calculateMatchScore } from './core/reconciler.js';
import { parseCamt053 } from './parsers/camt.js';
import { parseMT940 } from './parsers/mt940.js';
import { getOperatingGuVResult as getOperatingGuVResultImported, buildHgbBilanzTree, buildHgbGuvTree } from './reports/hgb.js';
import { calculateElsterUstva } from './reports/elster.js';
import { generateDatevCsvString } from './exporters/datev.js';
import { uiTestCases } from './core/ui_e2e_tests.js';

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
  ctx: null,
  mountGeneration: 0,
  lang: 'de',
  activeNav: 'skr',
  activeReportTab: 'bilanz',
  activeRightTab: 'ocr',
  skrName: 'SKR03', // Default

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
  rxCleanup: null,
  contextMenu: null,
  contextMenuCleanup: null,

  // UI Element References
  els: {},

  // E2E Test Suite Results Cache
  uiTestResults: {}
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

const ACCOUNTING_SEED_WRITE_COLLECTIONS = Object.freeze([
  'accounting_accounts',
  'accounting_journal_entries',
  'accounting_journal_entry_lines',
  'accounting_receipts',
  'accounting_bank_statements',
  'accounting_bank_statement_lines',
]);

function fibuCollection(name) {
  const facade = state.ctx?.db;
  if (!facade || !name) return null;
  if (!canReadCollection(name)) return null;
  try {
    return facade.collection?.(name) || null;
  } catch (error) {
    if (!isBusinessOsPermissionDenied(error)) {
      console.warn(`[fibu] collection ${name} is unavailable`, error);
    }
    return null;
  }
}

function fibuDb(requiredCollections = ACCOUNTING_COLLECTIONS) {
  const entries = requiredCollections.map((name) => [name, fibuCollection(name)]);
  if (entries.some(([, collection]) => !collection)) return null;
  return Object.fromEntries(entries);
}

function canReadCollection(name) {
  const permissionCheck = state.ctx?.permissions?.canReadCollection;
  return typeof permissionCheck !== 'function' || permissionCheck(name) === true;
}

function canWriteCollection(name) {
  const permissionCheck = state.ctx?.permissions?.canWriteCollection;
  return typeof permissionCheck !== 'function' || permissionCheck(name) === true;
}

function canWriteSeedData() {
  return ACCOUNTING_SEED_WRITE_COLLECTIONS.every((name) => canWriteCollection(name));
}

function isBusinessOsPermissionDenied(error) {
  const message = String(error?.message || error || '');
  return error?.code === 'BUSINESS_OS_PERMISSION_DENIED'
    || error?.name === 'BusinessOsPermissionDeniedError'
    || /permission denied|keine datenfreigabe|datenzugriff|data\.read|data\.write/i.test(message);
}

const ASSETS_MOCK = [
  { nr: 'ANL-2026-01', name: 'MacBook Pro 16 M3 Max', date: '2026-01-15', cost: 420000, life: 3, method: 'Lineaer', prev: 58333, book: 361667 },
  { nr: 'ANL-2026-02', name: 'Herman Miller Aeron Chair', date: '2026-02-10', cost: 160000, life: 13, method: 'Lineaer', prev: 4102, book: 155898 },
  { nr: 'ANL-2026-03', name: 'Premium Office Server Cluster', date: '2026-03-01', cost: 1200000, life: 5, method: 'Degressiv (20%)', prev: 200000, book: 1000000 }
];

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
    matchSuccess: 'Transaktion erfolgreich mit Beleg abgeglichen.',
    balancedAlert: 'Ausgeglichen (Soll = Haben)',
    unbalancedAlert: 'Ungleichgewicht! Soll und Haben müssen übereinstimmen.',
    gobdLockAlert: 'Dieses Dokument ist GoBD-festgeschrieben und schreibgeschützt.',
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
    gobdLockAlert: 'This document is GoBD-locked and read-only.',
  }
};

let t = (key) => labels.de[key] ?? key;

async function ensureStyles() {
  if (document.querySelector('link[data-module-styles="buchhaltung"]')) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = new URL('./index.css', import.meta.url).href;
  link.dataset.moduleStyles = 'buchhaltung';
  document.head.append(link);
}

// =========================================================================
// 🏁 Lifecycle Mount Hook
// =========================================================================
export async function mount(ctx) {
  const mountGeneration = ++state.mountGeneration;
  state.ctx = ctx;
  state.lang = ctx.locale === 'en' ? 'en' : 'de';

  await ensureStyles();
  if (state.mountGeneration !== mountGeneration) return () => {};

  const messages = await loadModuleMessages(import.meta.url, ctx.locale, labels);
  if (state.mountGeneration !== mountGeneration) return () => {};
  t = (key) => messages[key] ?? labels.de[key] ?? key;

  // 1. Inject HTML Structure
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  if (state.mountGeneration !== mountGeneration) return () => {};
  ctx.host.innerHTML = html;
  ctx.left.replaceChildren();
  ctx.right.replaceChildren();

  // 2. Bind DOM Elements & Resizers
  state.els.root?.removeAttribute('data-mount-ready');
  bindElements(ctx.host);
  const resizerCleanup = setupResizers(ctx.host);

  // 3. Register GoBD Immutability Guards in RxDB
  registerGoBDHooks();

  // 4. Load Active Kontenrahmen Select Value
  const savedSKR = state.skrName || 'SKR03';
  state.skrName = savedSKR;
  if (state.els.skrSelect) {
    state.els.skrSelect.value = savedSKR;
  }

  // 5. Bind and render immediately. Database initialization, demand reads and
  // optional seed writes must never hold the shell's window-open lifecycle.
  wireEvents();
  if (state.els.root) state.els.root.dataset.mountReady = 'true';
  switchView(state.activeNav);
  let disposed = false;
  Promise.resolve()
    .then(async () => {
      await autoInitializeAccounts();
      if (disposed || state.ctx !== ctx) return;
      await loadAllFibuData();
      if (disposed || state.ctx !== ctx) return;
      await seedMockDataIfEmpty();
      if (disposed || state.ctx !== ctx) return;
      state.rxCleanup = wireRealtimeSubscriptions();
      renderActiveView();
    })
    .catch((error) => {
      if (disposed || state.ctx !== ctx) return;
      console.error('[fibu] background initialization failed', error);
      renderActiveView();
    });


  // Return unmount function
  return () => {
    disposed = true;
    if (state.mountGeneration !== mountGeneration) return;
    state.mountGeneration += 1;
    state.els.root?.removeAttribute('data-mount-ready');
    if (state.rxCleanup) state.rxCleanup();
    state.contextMenuCleanup?.();
    state.contextMenu?.remove();
    state.contextMenu = null;
    unbindEvents();
    resizerCleanup?.();
  };
}

// =========================================================================
// 🏛️ Bind Elements & UI Resizers
// =========================================================================
function bindElements(host) {
  state.els = {
    root: host.querySelector('[data-fibu-root]'),
    navContainer: host.querySelector('[data-fibu-nav]'),
    navItems: host.querySelectorAll('[data-nav]'),
    skrSelect: host.querySelector('#skr-select'),
    centerCategory: host.querySelector('[data-active-category]'),
    centerTitle: host.querySelector('[data-active-title]'),
    centerActions: host.querySelector('[data-center-actions]'),

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
      tests: host.querySelector('[data-panel="tests"]')
    },

    // Data list containers
    accountsList: host.querySelector('[data-accounts-list]'),
    journalList: host.querySelector('[data-journal-list]'),
    receiptsList: host.querySelector('[data-receipts-list]'),
    bankingList: host.querySelector('[data-banking-list]'),
    assetsList: host.querySelector('[data-assets-list]'),
    uiTestsList: host.querySelector('[data-ui-tests-list]'),
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

    // Right Pane Tab Selectors
    rightTabBtns: host.querySelectorAll('[data-right-tab]'),
    rightSubpanels: {
      ocr: host.querySelector('[data-right-subpanel="ocr"]'),
      agent: host.querySelector('[data-right-subpanel="agent"]')
    },

    // Auxiliary Panel containers
    ocrPreviewContainer: host.querySelector('[data-ocr-preview-container]'),
    chatMessages: host.querySelector('[data-chat-messages]'),
    chatInput: host.querySelector('[data-chat-input]'),
    chatSendBtn: host.querySelector('[data-action="send-chat-msg"]')
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
function registerGoBDHooks() {
  const db = fibuDb(['accounting_journal_entries']);
  if (!db) return;

  const entriesCol = db.accounting_journal_entries;
  if (entriesCol) {
    try {
      entriesCol.preSave(function(data, doc) {
        if (doc && doc.posted_at) {
          throw new Error("GoBD-Verstoß: Bereits festgeschriebene Buchungen können nicht bearbeitet werden!");
        }
      }, false);
      entriesCol.preRemove(function(data, doc) {
        if (doc && doc.posted_at) {
          throw new Error("GoBD-Verstoß: Bereits festgeschriebene Buchungen können nicht gelöscht werden!");
        }
      }, false);
    } catch (e) {
      // Hook might already be registered in multi-mount scenario
    }
  }
}

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
    const existing = await db.accounting_accounts.findOne(id).exec();
    const existingData = docToRecord(existing);
    if (!existing) {
      await db.accounting_accounts.insert({
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
      await existing.patch({
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
  const docs = await collection?.find?.().exec();
  return Array.isArray(docs) ? docs.map(docToRecord) : [];
}

function docToRecord(doc) {
  return doc?.toJSON?.() || doc || {};
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
function wireRealtimeSubscriptions() {
  const db = fibuDb([
    'accounting_accounts',
    'accounting_journal_entries',
    'accounting_journal_entry_lines',
    'accounting_receipts',
    'accounting_bank_statement_lines',
  ]);
  if (!db) return null;

  const subs = [];

  // Subscribe to all changes in our Fibu RxDB collections to trigger reactivity
  const triggerReload = () => {
    loadAllFibuData().then(() => {
      renderActiveView();
    });
  };

  subs.push(db.accounting_accounts.$.subscribe(triggerReload));
  subs.push(db.accounting_journal_entries.$.subscribe(triggerReload));
  subs.push(db.accounting_journal_entry_lines.$.subscribe(triggerReload));
  subs.push(db.accounting_receipts.$.subscribe(triggerReload));
  subs.push(db.accounting_bank_statement_lines.$.subscribe(triggerReload));

  // Initial load
  triggerReload();

  return () => {
    subs.forEach(s => s.unsubscribe());
  };
}

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

  // Left Nav Click Events
  els.navItems.forEach(item => {
    item.addEventListener('click', () => {
      els.navItems.forEach(i => i.classList.remove('active'));
      item.classList.add('active');
      const target = item.getAttribute('data-nav');
      switchView(target);
    });
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

  // Right auxiliary tabs
  els.rightTabBtns.forEach(btn => {
    btn.addEventListener('click', () => {
      els.rightTabBtns.forEach(b => b.setAttribute('aria-selected', String(b === btn)));
      const target = btn.getAttribute('data-right-tab');
      switchRightSubpanel(target);
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
  els.panels.tests?.querySelector('[data-action="run-all-ui-tests"]')?.addEventListener('click', () => window.runAllUiTests());

  // DATEV Export Click
  els.panels.reports?.querySelector('[data-action="export-datev"]')?.addEventListener('click', triggerDatevExport);

  // Drawer close
  els.drawerCloseBtn?.addEventListener('click', closeDrawer);

  // AI Agent message send
  els.chatSendBtn?.addEventListener('click', handleSendAgentMsg);
  els.chatInput?.addEventListener('keypress', (e) => {
    if (e.key === 'Enter') handleSendAgentMsg();
  });
}

async function requestNewAsset(event) {
  await state.ctx.contextActions.dispatch('data', {
    target: event?.currentTarget,
    title: 'Anlagegut erfassen',
    prompt: 'Erfasse ein neues Anlagegut. Frage die fehlenden Stammdaten ab und lege den Datensatz nach Freigabe an.',
  });
}

async function triggerDepreciationRun(event) {
  await state.ctx.contextActions.dispatch('data', {
    target: event?.currentTarget,
    title: 'AfA-Abschreibungslauf starten',
    prompt: 'Starte den AfA-Abschreibungslauf für die aktuelle Buchungsperiode und liefere den Laufstatus sowie die erzeugten Buchungen.',
  });
}

function unbindEvents() {
  // Standard garbage collection handles most, but we can clean timers if any.
}

// =========================================================================
// 🚀 View Routing Engine
// =========================================================================
function switchView(navId) {
  state.activeNav = navId;

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
    skr: { cat: 'KONTENRAHMEN', title: `${state.skrName} Kontenrahmen-Explorer` },
    journal: { cat: 'BUCHUNGEN', title: 'Journal & Hauptbuch (GoBD-Immutable Ledger)' },
    receipts: { cat: 'BELEGE & OCR', title: 'Eingangsbelege & Vorkontierungs-Inbox' },
    banking: { cat: 'BANK & KASSE', title: 'Automatisierter SEPA Bankabgleich' },
    travel: { cat: 'SPESENABRECHNUNG', title: 'Dienstreisen & Verpflegungsmehraufwand (VMA)' },
    mileage: { cat: 'FAHRTENBUCH', title: 'Fahrtenbuch & Kilometererstattung' },
    reports: { cat: 'JAHRESABSCHLUSS', title: 'Bilanz, GuV & Umsatzsteuer-Voranmeldung' },
    assets: { cat: 'ANLAGENVERWALTUNG', title: 'Anlagenspiegel & Abschreibungen (AfA)' },
    tests: { cat: 'QUALITÄTSSICHERUNG', title: 'Interaktive UI E2E-Testsuite' }
  };

  if (state.els.centerCategory && state.els.centerTitle) {
    state.els.centerCategory.textContent = titles[navId].cat;
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

function switchRightSubpanel(subId) {
  state.activeRightTab = subId;
  Object.values(state.els.rightSubpanels).forEach(p => {
    if (p) p.hidden = true;
  });
  const active = state.els.rightSubpanels[subId];
  if (active) active.hidden = false;
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
    case 'tests':
      renderUiTestsList();
      break;
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
    container.innerHTML = `<tr><td colspan="6" class="fibu-empty-state">Keine Konten gefunden. Bitte initialisieren.</td></tr>`;
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
      <tr class="${isGroup ? 'group-row' : 'regular-row'} ${state.selectedAccountId === acct.id ? 'is-selected' : ''}" data-account-click-id="${acct.id}" aria-selected="${state.selectedAccountId === acct.id ? 'true' : 'false'}" tabindex="0">
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
        <td><span style="font-size:11px; opacity:0.85;">${taxRate}</span></td>
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

  const confirmed = await showBusinessConfirm(
    `Der Kontenrahmen ${state.skrName} wird gelöscht und aus der Vorlage neu aufgebaut.\n\nAlle lokalen Konten dieses Kontenrahmens werden überschrieben. Journalbuchungen bleiben bestehen, können danach aber auf geänderte Konten verweisen.`,
    {
      title: `${state.skrName} neu initialisieren`,
      confirmLabel: 'Kontenrahmen neu initialisieren',
      cancelLabel: 'Abbrechen',
      requireText: state.skrName,
      kind: 'danger'
    }
  );

  if (confirmed) {
    // Delete existing
    const allAccountDocs = await db.accounting_accounts.find().exec();
    const existing = allAccountDocs
      .filter((doc) => accountMatchesCurrentSkr(normalizeAccountRecord(docToRecord(doc))));
    for (const doc of existing) {
      await doc.remove();
    }
    await importTemplateToDb(db, state.skrName);
    await ensureRequiredAccountsExist();
    await loadAllFibuData();
    renderActiveView();
    alert(t('initSuccess'));
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
    container.innerHTML = `<tr><td colspan="7" class="fibu-empty-state">Keine Buchungen im Journal vorhanden.</td></tr>`;
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
      <tr class="${isStorno ? 'fibu-storno-indicator' : ''} ${state.selectedEntryId === entry.id ? 'is-selected' : ''}" data-entry-click-id="${entry.id}" aria-selected="${state.selectedEntryId === entry.id ? 'true' : 'false'}" tabindex="0">
        <td><span class="fibu-mono">${entry.posting_date}</span></td>
        <td><span class="fibu-mono">${entry.number || 'Entwurf'}</span></td>
        <td>${escapeHtml(entry.narration || 'Unbenannte Buchung')}</td>
        <td><span style="font-size:11px; opacity:0.8;">${receiptLabel}</span></td>
        <td class="is-num" style="font-weight: 600;">${formattedTotal}</td>
        <td class="fibu-cell-center">
          <span class="ctox-badge ${isPosted ? 'is-success' : ''}">
            ${isPosted ? (isStorno ? 'Storniert' : 'Posted 🔒') : 'Entwurf'}
          </span>
        </td>
        <td class="fibu-cell-center" onclick="event.stopPropagation();">
          ${isPosted && !isStorno ? `<button class="ctox-button is-danger fibu-btn-xs" onclick="triggerStorno('${entry.id}')">Storno</button>` : '—'}
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
  setSelectedRow(state.els.journalList, 'data-entry-click-id', id);
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
// 📁 Rendering View: Receipts & OCR Inbox
// =========================================================================
function renderReceiptsList() {
  const container = state.els.receiptsList;
  if (!container) return;

  if (state.receipts.length === 0) {
    container.innerHTML = `<tr><td colspan="9" class="fibu-empty-state">Keine Belege vorhanden. Ziehen Sie Dokumente per Drag & Drop in den Importer.</td></tr>`;
    return;
  }

  let html = '';
  state.receipts.forEach(r => {
    const status = r.status || 'draft';
    const hasSuggested = !!r.suggested_account_id;
    const suggestedAcct = hasSuggested ? state.accounts.find(a => a.id === r.suggested_account_id) : null;
    const suggestedLabel = suggestedAcct ? `${suggestedAcct.code} ${suggestedAcct.name}` : '—';

    html += `
      <tr class="${state.selectedReceiptId === r.id ? 'is-selected' : ''}" data-receipt-click-id="${r.id}" aria-selected="${state.selectedReceiptId === r.id ? 'true' : 'false'}" tabindex="0">
        <td style="font-weight: 500; color: var(--text-strong);">📄 ${escapeHtml(r.filename)}</td>
        <td>${escapeHtml(r.supplier_name || 'Unbekannt')}</td>
        <td><span class="fibu-mono">${r.invoice_date || '—'}</span></td>
        <td class="is-num">${formatCents(r.net_amount || 0)}</td>
        <td class="is-num">${r.tax_amount ? '19%' : '0%'}</td>
        <td class="is-num" style="font-weight: 600;">${formatCents(r.gross_amount || 0)}</td>
        <td><span style="font-size:11.5px; opacity:0.85;">${suggestedLabel}</span></td>
        <td class="fibu-cell-center">
          <span class="ctox-badge ${status === 'posted' ? 'is-success' : 'is-warning'}">${status}</span>
        </td>
        <td class="fibu-cell-center" onclick="event.stopPropagation();">
          ${status !== 'posted' ? `<button class="ctox-button is-primary fibu-btn-xs" onclick="postReceiptDirectly('${r.id}')">Buchen</button>` : '🔒 Fibu'}
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
  const receipt = state.receipts.find(r => r.id === id);
  if (!receipt) return;

  // Highlight row in Center
  state.els.receiptsList.querySelectorAll('tr').forEach(tr => {
    tr.classList.remove('is-selected');
    tr.setAttribute('aria-selected', 'false');
    if (tr.getAttribute('data-receipt-click-id') === id) {
      tr.classList.add('is-selected');
      tr.setAttribute('aria-selected', 'true');
    }
  });

  // Render Simulated OCR in the Right Auxiliary pane!
  renderOcrInvoicePreview(receipt);
  openReceiptDrawer(receipt);
}

function renderOcrInvoicePreview(receipt) {
  const container = state.els.ocrPreviewContainer;
  if (!container) return;

  const ocrHtml = `
    <div class="fibu-simulated-ocr">
      <div class="ocr-invoice-header">
        <div>
          <div class="ocr-invoice-title">${escapeHtml(receipt.supplier_name || 'EINGANGSRECHNUNG')}</div>
          <div>StNr / UStID: DE814712499</div>
        </div>
        <div class="ocr-invoice-meta">
          <div>RECHNUNG</div>
          <div style="font-weight: 700;"># ${escapeHtml(receipt.invoice_number || 'INV-98172')}</div>
        </div>
      </div>

      <div class="ocr-invoice-details">
        <table style="width: 100%; font-size: 11px;">
          <thead>
            <tr>
              <th>Pos</th>
              <th>Beschreibung</th>
              <th style="text-align:right;">Gesamt</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td>1</td>
              <td>Dienstleistungen laut Leistungsbeschreibung</td>
              <td style="text-align:right;">${formatCents(receipt.net_amount)}</td>
            </tr>
            <tr style="border-top: 1px solid #cbd5e1;">
              <td colspan="2" style="text-align:right;">Netto:</td>
              <td style="text-align:right;">${formatCents(receipt.net_amount)}</td>
            </tr>
            <tr>
              <td colspan="2" style="text-align:right;">MwSt 19%:</td>
              <td style="text-align:right;">${formatCents(receipt.tax_amount)}</td>
            </tr>
            <tr class="ocr-invoice-total-row">
              <td colspan="2" style="text-align:right;">Bruttobetrag (EUR):</td>
              <td style="text-align:right;">${formatCents(receipt.gross_amount)}</td>
            </tr>
          </tbody>
        </table>
      </div>

      <div class="ocr-raw-meta-badge">
        <div class="ocr-meta-header">
          <span class="ocr-meta-badge-title">🤖 CTOX-OCR Metadaten</span>
          <span class="ocr-confidence-badge">99.4% Konfidenz</span>
        </div>
        <div class="ocr-meta-archive-row">
          <span class="ocr-meta-archive-label">GoBD-Archivpfad</span>
          <div class="ocr-archive-path-wrapper">
            <code class="ocr-archive-path" title="${escapeHtml(receipt.file_storage_url || '')}">${truncateArchiveUrl(receipt.file_storage_url || '')}</code>
            <button type="button" class="ocr-copy-btn" onclick="navigator.clipboard.writeText('${escapeHtml(receipt.file_storage_url || '')}'); alert('Archivpfad in die Zwischenablage kopiert!');" title="Archivpfad kopieren">📋</button>
          </div>
        </div>
      </div>
    </div>
  `;
  container.innerHTML = ocrHtml;
}

// =========================================================================
// 🏦 Rendering View: Bank Reconciliation
// =========================================================================
function renderBankingList() {
  const container = state.els.bankingList;
  if (!container) return;

  if (state.bankStatementLines.length === 0) {
    container.innerHTML = `<tr><td colspan="7" class="fibu-empty-state">Keine Banktransaktionen vorhanden. Importieren Sie eine camt.053 XML-Datei.</td></tr>`;
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
      <tr class="${state.selectedBankLineId === line.id ? 'is-selected' : ''}" data-bankline-click-id="${line.id}" aria-selected="${state.selectedBankLineId === line.id ? 'true' : 'false'}" tabindex="0">
        <td><span class="fibu-mono">${line.value_date}</span></td>
        <td>
          <div style="font-weight:600; color:var(--text-strong);">${escapeHtml(line.counterparty_name || 'Unbekannter Empfänger')}</div>
          <div style="font-size:10.5px; opacity:0.75; font-family:monospace;">${line.counterparty_iban || '—'}</div>
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
        <td class="fibu-cell-center">
          <span class="ctox-badge ${isMatched ? 'is-success' : (isProposed ? 'is-warning' : '')}">
            ${line.match_status}
          </span>
        </td>
        <td class="fibu-cell-center" onclick="event.stopPropagation();">
          ${!isMatched && matchedReceipt ? `<button class="ctox-button is-primary fibu-btn-xs" onclick="matchBankLineDirectly('${line.id}', '${matchedReceipt.id}')">Einbuchen</button>` : (isMatched ? '🔒 Reconciled' : '—')}
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
  setSelectedRow(state.els.bankingList, 'data-bankline-click-id', id);
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

    <div class="fibu-tree-row" style="margin-top: 30px; border-top: 2px solid var(--fibu-primary); font-weight:700;">
      <span>BILANZSUMME AKTIVA</span>
      <span class="fibu-tree-val" style="color:var(--fibu-primary); font-size:16px;">${formatCents(totalAktivaCents)}</span>
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
      <div class="fibu-tree-row" style="color:var(--fibu-accent);">
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

    <div class="fibu-tree-row" style="margin-top: 30px; border-top: 2px solid var(--fibu-accent); font-weight:700;">
      <span>BILANZSUMME PASSIVA</span>
      <span class="fibu-tree-val" style="color:var(--fibu-accent); font-size:16px;">${formatCents(passivaTotalCents)}</span>
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

    <div class="fibu-tree-row" style="margin-top: 24px; border-top: 2px solid var(--fibu-accent); font-weight:700; font-size:15px;">
      <span>4. JAHRESÜBERSCHUSS / JAHRESFEHLBETRAG</span>
      <span class="fibu-tree-val" style="color:var(--fibu-accent);">${formatCents(resultCents)}</span>
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
  ASSETS_MOCK.forEach(as => {
    html += `
      <tr class="${state.selectedAssetId === as.nr ? 'is-selected' : ''}" data-asset-click-nr="${as.nr}" aria-selected="${state.selectedAssetId === as.nr ? 'true' : 'false'}" tabindex="0">
        <td><span class="fibu-mono">${as.nr}</span></td>
        <td style="font-weight: 500; color: var(--text-strong);">🏢 ${escapeHtml(as.name)}</td>
        <td><span class="fibu-mono">${as.date}</span></td>
        <td class="is-num">${formatCents(as.cost)}</td>
        <td class="fibu-cell-center">${as.life}</td>
        <td><span class="ctox-badge">${as.method}</span></td>
        <td class="fibu-text-credit is-num">${formatCents(as.prev)}</td>
        <td class="fibu-text-debit is-num">${formatCents(as.book)}</td>
        <td class="fibu-cell-center">
          <button type="button" class="ctox-button fibu-btn-xs" data-asset-plan="${as.nr}">Plan</button>
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
  const asset = ASSETS_MOCK.find(as => as.nr === nr);
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
            <th class="is-num">Soll (Debit)</th>
            <th class="is-num">Haben (Credit)</th>
          </tr>
        </thead>
        <tbody>
    `;
    ledgerLines.forEach(l => {
      linesHtml += `
        <tr>
          <td><span class="fibu-mono">${l.posting_date}</span></td>
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
    <div style="display:flex; justify-content:space-between; margin-bottom:16px;">
      <div>
        <div style="font-size:11px; color:var(--muted); font-weight:700; text-transform:uppercase;">Buchungstext</div>
        <div style="font-size:14px; font-weight:600; color:var(--text-strong);">${escapeHtml(entry.narration || '')}</div>
      </div>
      <div>
        <div style="font-size:11px; color:var(--muted); font-weight:700; text-transform:uppercase;">Buchungsdatum</div>
        <div style="font-size:14px; font-weight:600;" class="fibu-mono">${entry.posting_date}</div>
      </div>
    </div>
  `;

  html += `
    <table class="ctox-table">
      <thead>
        <tr>
          <th>Konto Code</th>
          <th>Kontenbezeichnung</th>
          <th class="is-num">Soll (Debit)</th>
          <th class="is-num">Haben (Credit)</th>
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
      <div class="fibu-security-badge-card" style="margin-top:20px;">
        <span class="security-badge-icon">🛡️</span>
        <div class="security-badge-text-wrap">
          <span class="security-badge-title">GoBD-Festgeschrieben (Unveränderbar)</span>
          <span class="security-badge-desc">Dieser Belegeintrag ist steuerrechtlich gesperrt. Nachträgliche Korrekturen können nur per Storno vorgenommen werden.</span>
        </div>
      </div>
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
      <div class="fibu-form-row">
        <div class="fibu-form-group">
          <label>Lieferant</label>
          <input type="text" class="ctox-input" value="${escapeHtml(receipt.supplier_name || '')}" readonly />
        </div>
        <div class="fibu-form-group">
          <label>Rechnungsnummer</label>
          <input type="text" class="ctox-input" value="${escapeHtml(receipt.invoice_number || '')}" readonly />
        </div>
        <div class="fibu-form-group">
          <label>Belegdatum</label>
          <input type="text" class="ctox-input" value="${receipt.invoice_date || ''}" readonly />
        </div>
      </div>

      <div class="fibu-form-row" style="margin-top:15px; background:var(--surface-2); padding:15px; border-radius:8px; border:1px solid var(--line);">
        <div class="fibu-form-group">
          <label class="fibu-text-warning" style="font-weight:700;">Zahlungsweg (Konto-Gegenbuchung)</label>
          <select id="rcpt-payment-method" class="ctox-select" onchange="updateReceiptDrawerOptions('${receipt.id}')" ${isPosted ? 'disabled' : ''}>
            <option value="kreditor">Geschäftskonto (Standard-Kreditor Verbindlichkeiten)</option>
            <option value="privat">Privat bezahlt (Privatauslage des Inhabers)</option>
            <option value="verrechnung">Gesellschafter-Verrechnungskonto</option>
          </select>
        </div>
      </div>

      <div style="margin-top:15px;">
        <label class="fibu-text-warning" style="display:block; margin-bottom:8px; font-weight:700;">Steuerliche Sonderregeln & Steuerberater-Tricks:</label>
        <div style="display:flex; flex-direction:column; gap:8px; background:var(--surface-2); padding:12px; border-radius:8px; border:1px solid var(--line);">
          <label style="display:flex; align-items:center; gap:8px; cursor:pointer; color:var(--text); font-size:12.5px;">
            <input type="checkbox" id="rcpt-bewirtung" onchange="updateReceiptDrawerOptions('${receipt.id}')" ${isPosted ? 'disabled' : ''} />
            Geschäftliche Bewirtung (70/30 Netto-Split, 100% Vorsteuer abzugsfähig)
          </label>
          <label style="display:flex; align-items:center; gap:8px; cursor:pointer; color:var(--text); font-size:12.5px;">
            <input type="checkbox" id="rcpt-geschenk" onchange="updateReceiptDrawerOptions('${receipt.id}')" ${isPosted ? 'disabled' : ''} />
            Geschenk an Partner (schaltet bei &gt; 35 € automatisch auf nicht abzugsfähig)
          </label>
          <label style="display:flex; align-items:center; gap:8px; cursor:pointer; color:var(--text); font-size:12.5px;">
            <input type="checkbox" id="rcpt-phone-private" onchange="updateReceiptDrawerOptions('${receipt.id}')" ${isPosted ? 'disabled' : ''} />
            Telefon/Internet (zieht automatisch 20% Privatanteil heraus)
          </label>
        </div>
      </div>

      <h4 style="margin:20px 0 10px 0; color:var(--text-strong);">Vorgeschlagener GoBD-Buchungssatz (Live-Vorschau):</h4>
      <div id="receipt-preview-table-container">
        <!-- Table loaded dynamically -->
      </div>

      <div style="margin-top:20px; display:flex; justify-content:flex-end; gap:10px;">
        ${isPosted ? `
          <div class="fibu-security-badge-card" style="width:100%;">
            <span class="security-badge-icon">🛡️</span>
            <div class="security-badge-text-wrap">
              <span class="security-badge-title">GoBD-Festgeschrieben (Unveränderbar)</span>
              <span class="security-badge-desc">Dieser Beleg wurde bereits festgeschrieben und verbucht.</span>
            </div>
          </div>
        ` : `
          <button class="ctox-button is-primary" onclick="postReceiptWithAdvancedOptions('${receipt.id}')">✔️ Vorkontierung freigeben & Buchen</button>
        `}
      </div>
    </div>
  `;
  state.els.drawerContent.innerHTML = html;

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
    <div style="display:flex; justify-content:space-between; margin-bottom:16px;">
      <div>
        <div style="font-size:11px; color:var(--muted); font-weight:700; text-transform:uppercase;">Bankbuchung Verwendungszweck</div>
        <div style="font-size:13.5px; font-weight:600; color:var(--text-strong);">${escapeHtml(line.narration)}</div>
      </div>
      <div>
        <div style="font-size:11px; color:var(--muted); font-weight:700; text-transform:uppercase;">Wertstellung</div>
        <div style="font-size:13.5px; font-weight:600;" class="fibu-mono">${line.value_date}</div>
      </div>
    </div>
  `;

  if (line.match_status === 'matched') {
    html += `
      <div class="fibu-security-badge-card" style="margin-top:20px; width:100%;">
        <span class="security-badge-icon">🛡️</span>
        <div class="security-badge-text-wrap">
          <span class="security-badge-title">Abgeglichen & Verbucht</span>
          <span class="security-badge-desc">Diese Banktransaktion wurde GoBD-konform mit einem Beleg verknüpft und festgeschrieben.</span>
        </div>
      </div>
    `;
  } else {
    if (matchedReceipt) {
      html += `
        <div style="background:var(--accent-soft); border:1px solid var(--accent); padding:16px; border-radius:10px; margin-top:20px;">
          <h4 class="fibu-text-warning" style="margin-bottom:8px;">🤖 Automatischer Abgleichsvorschlag:</h4>
          <p style="font-size:12.5px; margin-bottom:12px;">Wir haben einen offenen Beleg mit exakt demselben Betrag (${formatCents(Math.abs(amountVal))}) gefunden:</p>
          <div style="display:flex; justify-content:space-between; align-items:center;">
            <div>
              <div style="font-weight:700; color:var(--text-strong);">📄 ${escapeHtml(matchedReceipt.filename)}</div>
              <div style="font-size:11px; opacity:0.8;">Lieferant: ${escapeHtml(matchedReceipt.supplier_name)} | Datum: ${matchedReceipt.invoice_date}</div>
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
        <button class="ctox-button fibu-btn-dashed-warning" style="width:100%;" onclick="toggleBankSplitEditor('${line.id}')">
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
    <div style="margin-bottom:16px;">
      <span style="font-size:11.5px; color:var(--muted); font-weight:700; text-transform:uppercase;">Anschaffungskosten:</span>
      <span style="font-size:14px; font-weight:700; color:var(--text-strong); margin-left:8px;">${formatCents(cost)}</span>
      <span style="font-size:11.5px; color:var(--muted); font-weight:700; text-transform:uppercase; margin-left:24px;">Nutzungsdauer:</span>
      <span style="font-size:14px; font-weight:700; color:var(--text-strong); margin-left:8px;">${life} Jahre</span>
    </div>

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
          <label for="new-entry-date">Belegdatum</label>
          <input type="date" id="new-entry-date" class="ctox-input" value="2026-05-22" required aria-describedby="new-entry-validation" />
        </div>
        <div class="fibu-form-group" style="flex:2;">
          <label for="new-entry-narration">Buchungstext</label>
          <input type="text" id="new-entry-narration" class="ctox-input" placeholder="z.B. Miete Büroräume Mai" required />
        </div>
      </div>

      <div class="fibu-form-row">
        <div class="fibu-form-group">
          <label for="new-entry-soll">Soll-Konto (Debit)</label>
          <select id="new-entry-soll" class="ctox-select" required>
            ${state.accounts.filter(a => !a.is_group).map(a => `<option value="${a.id}" ${a.id === defaults.soll ? 'selected' : ''}>${a.code} ${escapeHtml(a.name)}</option>`).join('')}
          </select>
        </div>
        <div class="fibu-form-group">
          <label for="new-entry-haben">Haben-Konto (Credit)</label>
          <select id="new-entry-haben" class="ctox-select" required>
            ${state.accounts.filter(a => !a.is_group).map(a => `<option value="${a.id}" ${a.id === defaults.haben ? 'selected' : ''}>${a.code} ${escapeHtml(a.name)}</option>`).join('')}
          </select>
        </div>
        <div class="fibu-form-group">
          <label for="new-entry-amount">Betrag (Netto in EUR)</label>
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
    await db.accounting_journal_entries.insert({
      id: entryId,
      posting_date: date,
      type: 'journal',
      number: '',
      narration: narration,
      updated_at_ms: Date.now()
    });

    // Insert Soll Line
    await db.accounting_journal_entry_lines.insert({
      id: entryId + '-l1',
      journal_entry_id: entryId,
      account_id: soll,
      debit: cents,
      credit: 0,
      line_no: 1,
      updated_at_ms: Date.now()
    });

    // Insert Haben Line
    await db.accounting_journal_entry_lines.insert({
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
  await db.accounting_bank_statements.insert({
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
    await db.accounting_bank_statement_lines.insert({
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

  for (const file of files) {
    const id = 'rcpt-' + Math.random().toString(36).substring(2, 9);

    // Simulate OCR results
    let supplier = 'Hetzner Online GmbH';
    let net = 10000; // 100.00 EUR
    let gross = 11900; // 119.00 EUR
    let tax = 1900;
    let invNum = 'HET-2026-98127';
    let suggestedCode = state.skrName === 'SKR03' ? '4930' : '6815'; // Software / IT-Kosten

    if (file.name.toLowerCase().includes('telekom') || file.name.toLowerCase().includes('phone')) {
      supplier = 'Deutsche Telekom AG';
      invNum = 'TEL-7816-12';
      suggestedCode = state.skrName === 'SKR03' ? '4600' : '6600'; // Werbekosten / Telefon
    }

    const suggestedAcct = state.accounts.find(a => a.code === suggestedCode);

    await db.accounting_receipts.insert({
      id,
      file_storage_url: `runtime/business-os/buchhaltung/storage/${file.name}`,
      filename: file.name,
      supplier_name: supplier,
      invoice_date: '2026-05-22',
      invoice_number: invNum,
      net_amount: net,
      tax_amount: tax,
      gross_amount: gross,
      suggested_account_id: suggestedAcct?.id || '',
      status: 'draft',
      updated_at_ms: Date.now()
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
        const doc = await db.accounting_bank_statement_lines.findOne(line.id).exec();
        if (doc) {
          await doc.patch({ match_status: 'proposed' });
          matchCount++;
        }
      }
    }
  }

  alert(`🤖 Auto-Reconciliation beendet. ${matchCount} matches automatisch vorgeschlagen!`);
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
    expenseCode = suggestedAcct ? suggestedAcct.code : (skr === 'SKR03' ? '4930' : '6815');
  }
  const expenseAcct = state.accounts.find(a => a.code === expenseCode);
  const expenseName = expenseAcct ? expenseAcct.name : 'Aufwandskonto';

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
  const receipt = state.receipts.find(r => r.id === receiptId);
  if (!receipt) return;

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
          <th class="is-num">Soll (Debit)</th>
          <th class="is-num">Haben (Credit)</th>
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

  const receipt = state.receipts.find(r => r.id === receiptId);
  if (!receipt) return;

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
  await db.accounting_journal_entries.insert({
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
    await db.accounting_journal_entry_lines.insert({
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
  const rcptDoc = await db.accounting_receipts.findOne(receiptId).exec();
  if (rcptDoc) {
    await rcptDoc.patch({ status: 'posted' });
  }

  closeDrawer();
  switchView('journal');
  alert(t('postedSuccess'));
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
    <div style="background:var(--surface-2); border:1px solid var(--line); padding:16px; border-radius:10px; margin-top:16px;">
      <h4 class="fibu-text-warning" style="margin-bottom:12px;">🥞 Bank-Transaktions-Splits (Aufteilungsbuchung):</h4>
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
        <button class="ctox-button is-danger fibu-btn-xs" onclick="deleteBankSplit(${idx})">🗑️</button>
      </div>
    `;
  });

  const isBalanced = remaining === 0;

  html += `
      </div>
      <div style="margin-top:12px; display:flex; justify-content:space-between; align-items:center;">
        <button class="ctox-button fibu-btn-xs" onclick="addBankSplitRow()">➕ Zeile hinzufügen</button>
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
  await db.accounting_journal_entries.insert({
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
  await db.accounting_journal_entry_lines.insert({
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
    await db.accounting_journal_entry_lines.insert({
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
  const bankLineDoc = await db.accounting_bank_statement_lines.findOne(lineId).exec();
  if (bankLineDoc) {
    await bankLineDoc.patch({ match_status: 'matched', reconciled_entry_id: entryId });
  }

  closeDrawer();
  switchView('banking');
  alert(t('matchSuccess'));
};

// =========================================================================
// ✔️ Action Direct Controllers (GoBD Immutability execution)
// =========================================================================
window.postReceiptDirectly = async (receiptId) => {
  const db = fibuDb();
  if (!db) return;

  const receipt = state.receipts.find(r => r.id === receiptId);
  if (!receipt) return;

  const entryId = 'j-' + Math.random().toString(36).substring(2, 9);
  const now = Date.now();

  const payablesCode = state.skrName === 'SKR03' ? '1600' : '3300';
  const payablesAcct = state.accounts.find(a => a.code === payablesCode);

  const vorsteuerCode = state.skrName === 'SKR03' ? '1576' : '1406';
  const vorsteuerAcct = state.accounts.find(a => a.code === vorsteuerCode);

  // Save unique number
  const nextNum = 'J-2026-' + String(state.journalEntries.filter(e => e.posted_at).length + 1).padStart(4, '0');

  // 1. Insert Journal Entry
  await db.accounting_journal_entries.insert({
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
  await db.accounting_journal_entry_lines.insert({
    id: entryId + '-l1',
    journal_entry_id: entryId,
    account_id: receipt.suggested_account_id,
    debit: receipt.net_amount,
    credit: 0,
    line_no: 1,
    updated_at_ms: now
  });

  // Line 2: Vorsteuer (Tax)
  await db.accounting_journal_entry_lines.insert({
    id: entryId + '-l2',
    journal_entry_id: entryId,
    account_id: vorsteuerAcct?.id || '',
    debit: receipt.tax_amount,
    credit: 0,
    line_no: 2,
    updated_at_ms: now
  });

  // Line 3: Kreditoren (Gross)
  await db.accounting_journal_entry_lines.insert({
    id: entryId + '-l3',
    journal_entry_id: entryId,
    account_id: payablesAcct?.id || '',
    debit: 0,
    credit: receipt.gross_amount,
    line_no: 3,
    updated_at_ms: now
  });

  // 3. Mark receipt as posted
  const rcptDoc = await db.accounting_receipts.findOne(receiptId).exec();
  if (rcptDoc) {
    await rcptDoc.patch({ status: 'posted' });
  }

  closeDrawer();
  switchView('journal');
  alert(t('postedSuccess'));
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
  const lineDoc = await db.accounting_bank_statement_lines.findOne(lineId).exec();
  if (lineDoc) {
    await lineDoc.patch({
      match_status: 'matched',
      reconciled_entry_id: receiptId
    });
  }

  closeDrawer();
  switchView('banking');
  alert(t('matchSuccess'));
};

window.triggerStorno = async (entryId) => {
  const db = fibuDb();
  if (!db) return;

  const entry = state.journalEntries.find(e => e.id === entryId);
  if (!entry || entry.reversed_by_id) return;

  if (confirm('Möchten Sie diese Buchung wirklich GoBD-konform stornieren? Es wird eine automatische Gegenbuchung (Storno) erzeugt.')) {
    const lines = state.journalEntryLines.filter(l => l.journal_entry_id === entryId);

    const nextNum = 'J-2026-' + String(state.journalEntries.filter(e => e.posted_at).length + 1).padStart(4, '0');
    const stornoId = 'storno-' + Math.random().toString(36).substring(2, 9);
    const now = Date.now();

    // 1. Insert Storno Entry
    await db.accounting_journal_entries.insert({
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
      await db.accounting_journal_entry_lines.insert({
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
    const entryDoc = await db.accounting_journal_entries.findOne(entryId).exec();
    if (entryDoc) {
      await entryDoc.patch({ reversed_by_id: stornoId });
    }

    closeDrawer();
    switchView('journal');
    alert(t('stornoSuccess'));
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
    alert("Keine festgeschriebenen Buchungssätze im gewählten Datumsbereich vorhanden.");
    return;
  }

  const blob = new Blob([csvContentStr], { type: "text/csv;charset=utf-8;" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.setAttribute("href", url);
  link.setAttribute("download", `DATEV_EXTF_${start}_${end}.csv`);
  document.body.appendChild(link);
  link.click();
  setTimeout(() => {
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
  }, 1000);
}

// =========================================================================
// 🤖 Fibu AI Agent Chat Dialog Interaction
// =========================================================================
async function handleSendAgentMsg() {
  const input = state.els.chatInput;
  const msg = input.value.trim();
  if (!msg) return;

  // Add User Message
  appendChatMessage('👤', msg, 'user');
  input.value = '';

  // Simulate AI Agent processing with beautiful response
  appendChatMessage('🤖', 'Analysiere Anfrage...', 'system');

  setTimeout(() => {
    // Remove analysis indicator
    const lastMsg = state.els.chatMessages.lastElementChild;
    if (lastMsg && lastMsg.classList.contains('system')) lastMsg.remove();

    // Generate intelligent responses
    const lower = msg.toLowerCase();
    if (lower.includes('rechnung') || lower.includes('hetzner') || lower.includes('telekom')) {
      appendChatMessage('🤖', `Ich habe das Dokument analysiert und folgendes Vorkontierungs-Entwurf vorgeschlagen:<br>
      <strong>Hetzner Online GmbH (Rechnung HET-2026-98127)</strong><br>
      • Soll: Konto <strong>4930 Softwarekosten</strong> (${formatCents(10000)})<br>
      • Soll: Konto <strong>1576 Vorsteuer 19%</strong> (${formatCents(1900)})<br>
      • Haben: Konto <strong>1600 Verbindlichkeiten</strong> (${formatCents(11900)})<br>
      Du findest den Entwurf in der Belegs-Inbox. Klicke dort einfach auf "Buchen" zum Festschreiben.`, 'system');
      switchView('receipts');
    } else if (lower.includes('abgleich') || lower.includes('bank')) {
      triggerAutoReconcile().then(() => {
        appendChatMessage('🤖', `Bankabgleich erfolgreich durchgeführt! Ich habe 2 neue passende Belege mit exakten Beträgen im camt.053 Auszug gefunden und vorgeschlagen.`, 'system');
      });
    } else if (lower.includes('bilanz') || lower.includes('hgb')) {
      appendChatMessage('🤖', `Die HGB-Bilanz wurde aktualisiert. Aktiva und Passiva gleichen sich perfekt aus (Aktiva = Passiva = ${formatCents(1216000)}).`, 'system');
      switchView('reports');
    } else {
      appendChatMessage('🤖', `Entschuldigung, ich verstehe diese Anweisung im Moment nicht. Du kannst mich bitten: "Buche Hetzner", "Gleiche die Bank aus" oder "Erstelle die Bilanz".`, 'system');
    }
  }, 1000);
}

function appendChatMessage(avatar, text, type) {
  const container = state.els.chatMessages;
  if (!container) return;

  const div = document.createElement('div');
  div.className = `chat-message ${type}`;
  div.innerHTML = `
    <span class="avatar">${avatar}</span>
    <div class="message-bubble">${text}</div>
  `;
  container.appendChild(div);
  container.scrollTop = container.scrollHeight;
}

// =========================================================================
// 🧪 Seed Mock Data (Ensure high fidelity first load)
// =========================================================================
async function seedMockDataIfEmpty() {
  const db = fibuDb(ACCOUNTING_SEED_WRITE_COLLECTIONS);
  if (!db || !canWriteSeedData()) return;

  if (state.accounts.length === 0) {
    await loadAllFibuData();
  }

  const existingEntries = await db.accounting_journal_entries.find().exec();
  if (existingEntries.length > 0) return;

  console.log('[fibu] seeding beautiful initial mock data...');
  const now = Date.now();

  const bankCode = state.skrName === 'SKR03' ? '1200' : '1800';
  const capitalCode = state.skrName === 'SKR03' ? '0800' : '2900';

  const bankAcct = state.accounts.find(a => a.code === bankCode);
  const capitalAcct = state.accounts.find(a => a.code === capitalCode);

  if (!bankAcct || !capitalAcct) return;

  // Seed 1: Initial capital deposit (10.000,00 EUR)
  await db.accounting_journal_entries.insert({
    id: 'mock-j1',
    posting_date: '2026-01-01',
    type: 'journal',
    number: 'J-2026-0001',
    narration: 'Einzahlung Stammkapital (Gründung)',
    posted_at: now,
    updated_at_ms: now
  });

  await db.accounting_journal_entry_lines.insert({
    id: 'mock-j1-l1',
    journal_entry_id: 'mock-j1',
    account_id: bankAcct.id,
    debit: 1000000, // 10,000.00 EUR
    credit: 0,
    line_no: 1,
    updated_at_ms: now
  });

  await db.accounting_journal_entry_lines.insert({
    id: 'mock-j1-l2',
    journal_entry_id: 'mock-j1',
    account_id: capitalAcct.id,
    debit: 0,
    credit: 1000000,
    line_no: 2,
    updated_at_ms: now
  });

  // Seed 2: Revenue 19% (2500,00 EUR gross -> 2100,84 net / 399,16 VAT)
  const revCode = state.skrName === 'SKR03' ? '8400' : '4400';
  const revAcct = state.accounts.find(a => a.code === revCode);
  const ustCode = state.skrName === 'SKR03' ? '1776' : '3806';
  const ustAcct = state.accounts.find(a => a.code === ustCode);

  if (revAcct && ustAcct) {
    await db.accounting_journal_entries.insert({
      id: 'mock-j2',
      posting_date: '2026-02-15',
      type: 'invoice',
      number: 'J-2026-0002',
      narration: 'Umsatzerlöse aus Beratung Dienstleistung',
      posted_at: now,
      updated_at_ms: now
    });

    await db.accounting_journal_entry_lines.insert({
      id: 'mock-j2-l1',
      journal_entry_id: 'mock-j2',
      account_id: bankAcct.id,
      debit: 250000, // 2500.00 EUR
      credit: 0,
      line_no: 1,
      updated_at_ms: now
    });

    await db.accounting_journal_entry_lines.insert({
      id: 'mock-j2-l2',
      journal_entry_id: 'mock-j2',
      account_id: revAcct.id,
      debit: 0,
      credit: 210084, // Netto
      line_no: 2,
      updated_at_ms: now
    });

    await db.accounting_journal_entry_lines.insert({
      id: 'mock-j2-l3',
      journal_entry_id: 'mock-j2',
      account_id: ustAcct.id,
      debit: 0,
      credit: 39916, // MwSt
      line_no: 3,
      updated_at_ms: now
    });
  }

  // Seed 3: Draft Receipt in Vorrat
  const suggestedCode = state.skrName === 'SKR03' ? '4930' : '6815';
  const suggestedAcct = state.accounts.find(a => a.code === suggestedCode);

  await db.accounting_receipts.insert({
    id: 'mock-rcpt1',
    file_storage_url: 'runtime/business-os/buchhaltung/storage/Hetzner_Cloud_Bill.pdf',
    filename: 'Hetzner_Cloud_Bill.pdf',
    supplier_name: 'Hetzner Online GmbH',
    invoice_date: '2026-05-18',
    invoice_number: 'HET-981724',
    net_amount: 10000, // 100 EUR
    tax_amount: 1900,  // 19 EUR
    gross_amount: 11900,
    suggested_account_id: suggestedAcct?.id || '',
    status: 'draft',
    updated_at_ms: now
  });

  await db.accounting_receipts.insert({
    id: 'mock-rcpt2',
    file_storage_url: 'runtime/business-os/buchhaltung/storage/Telekom_DSL_May.pdf',
    filename: 'Telekom_DSL_May.pdf',
    supplier_name: 'Deutsche Telekom AG',
    invoice_date: '2026-05-20',
    invoice_number: 'TEL-87216-A',
    net_amount: 5000,
    tax_amount: 950,
    gross_amount: 5950,
    suggested_account_id: state.accounts.find(a => a.code === (state.skrName === 'SKR03' ? '4600' : '6600'))?.id || '',
    status: 'draft',
    updated_at_ms: now
  });

  const statementId = 'mock-stmt1';
  await db.accounting_bank_statements.insert({
    id: statementId,
    account_number: 'DE90370400440532013000',
    updated_at_ms: now
  });

  await db.accounting_bank_statement_lines.insert({
    id: 'mock-bankline1',
    statement_id: statementId,
    value_date: '2026-05-22',
    narration: 'SEPA UEBERWEISUNG HETZNER HET-981724',
    amount: -11900,
    counterparty_name: 'Hetzner Online GmbH',
    counterparty_iban: 'DE81200505501234567890',
    match_status: 'proposed',
    updated_at_ms: now
  });

  const travelExpenseAcct = state.accounts.find(a => a.code === (state.skrName === 'SKR03' ? '4660' : '6670'));
  const mileageExpenseAcct = state.accounts.find(a => a.code === (state.skrName === 'SKR03' ? '4673' : '6680'));
  const privateContraAcct = state.accounts.find(a => a.code === (state.skrName === 'SKR03' ? '1890' : '2180'));

  if (travelExpenseAcct && privateContraAcct) {
    const travelMetadata = {
      startDate: '2026-05-06',
      endDate: '2026-05-08',
      totalAllowance: 5600,
      days: [
        { date: '2026-05-06', type: 'arrival' },
        { date: '2026-05-07', type: 'full', breakfast: true },
        { date: '2026-05-08', type: 'departure' }
      ]
    };

    await db.accounting_journal_entries.insert({
      id: 'mock-travel1',
      posting_date: '2026-05-08',
      type: 'travel',
      ref_type: 'travel',
      ref_id: JSON.stringify(travelMetadata),
      number: '',
      narration: 'Reisekostenabrechnung: Kundenworkshop Berlin',
      posted_at: 0,
      updated_at_ms: now
    });

    await db.accounting_journal_entry_lines.insert({
      id: 'mock-travel1-l1',
      journal_entry_id: 'mock-travel1',
      account_id: travelExpenseAcct.id,
      debit: travelMetadata.totalAllowance,
      credit: 0,
      line_no: 1,
      updated_at_ms: now
    });

    await db.accounting_journal_entry_lines.insert({
      id: 'mock-travel1-l2',
      journal_entry_id: 'mock-travel1',
      account_id: privateContraAcct.id,
      debit: 0,
      credit: travelMetadata.totalAllowance,
      line_no: 2,
      updated_at_ms: now
    });
  }

  if (mileageExpenseAcct && privateContraAcct) {
    const mileageMetadata = {
      date: '2026-05-13',
      purpose: 'business',
      startKm: 12400,
      endKm: 12550,
      km: 150,
      destination: 'Büro -> Kundentermin Potsdam -> Büro',
      contactPerson: 'Müller GmbH',
      reimbursement: 4500
    };

    await db.accounting_journal_entries.insert({
      id: 'mock-mileage1',
      posting_date: '2026-05-13',
      type: 'mileage',
      ref_type: 'mileage',
      ref_id: JSON.stringify(mileageMetadata),
      number: '',
      narration: 'Kundenpräsentation und Vertragsabstimmung',
      posted_at: 0,
      updated_at_ms: now
    });

    await db.accounting_journal_entry_lines.insert({
      id: 'mock-mileage1-l1',
      journal_entry_id: 'mock-mileage1',
      account_id: mileageExpenseAcct.id,
      debit: mileageMetadata.reimbursement,
      credit: 0,
      line_no: 1,
      updated_at_ms: now
    });

    await db.accounting_journal_entry_lines.insert({
      id: 'mock-mileage1-l2',
      journal_entry_id: 'mock-mileage1',
      account_id: privateContraAcct.id,
      debit: 0,
      credit: mileageMetadata.reimbursement,
      line_no: 2,
      updated_at_ms: now
    });
  }

  // Reload
  await loadAllFibuData();
}

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
// 🧪 E2E UI Test Suite Controllers
// =========================================================================
function renderUiTestsList() {
  const container = state.els.uiTestsList;
  if (!container) return;

  const total = uiTestCases.length;
  let passed = 0;
  let failed = 0;

  uiTestCases.forEach(tc => {
    const res = state.uiTestResults[tc.id];
    if (res === 'passed') passed++;
    if (res === 'failed') failed++;
  });

  const totalEl = document.getElementById('ui-test-total');
  const passedEl = document.getElementById('ui-test-passed');
  const failedEl = document.getElementById('ui-test-failed');
  if (totalEl) totalEl.textContent = total;
  if (passedEl) passedEl.textContent = passed;
  if (failedEl) failedEl.textContent = failed;

  let html = '';
  uiTestCases.forEach(tc => {
    const status = state.uiTestResults[tc.id] || 'pending';
    let statusClass = '';
    let statusText = 'Bereit';

    if (status === 'running') {
      statusClass = 'is-warning';
      statusText = '⏳ Läuft...';
    } else if (status === 'passed') {
      statusClass = 'is-success';
      statusText = '✔️ Bestanden';
    } else if (status === 'failed') {
      statusClass = 'is-danger';
      statusText = '❌ Fehlgeschlagen';
    }

    const stepsHtml = tc.steps.map((step, idx) => `
      <li id="step-${tc.id}-${idx}" style="margin-bottom: 2px; padding: 2px 4px; border-radius: 4px; transition: all 0.3s ease;">
        ${escapeHtml(step)}
      </li>
    `).join('');

    html += `
      <tr id="tc-row-${tc.id}" style="transition: all 0.3s ease;">
        <td><span class="fibu-mono">${tc.id}</span></td>
        <td>
          <div style="font-weight:600; color:var(--text-strong); font-size:12px;">${escapeHtml(tc.name)}</div>
          <div style="font-size:11px; color:var(--text-muted); margin-top:2px;">${escapeHtml(tc.description)}</div>
        </td>
        <td>
          <ol style="margin:0; padding-left:14px; font-size:11px; color:var(--text-muted);">
            ${stepsHtml}
          </ol>
        </td>
        <td class="fibu-cell-center">
          <span class="ctox-badge ${statusClass}">${statusText}</span>
        </td>
        <td class="fibu-cell-center">
          <button class="ctox-button fibu-btn-xs" onclick="window.runSingleUiTest('${tc.id}')" ${status === 'running' ? 'disabled' : ''}>
            ⚡ Run
          </button>
        </td>
      </tr>
    `;
  });

  container.innerHTML = html;
}

window.runSingleUiTest = async function(testId) {
  const tc = uiTestCases.find(t => t.id === testId);
  if (!tc) return;

  // Set status
  state.uiTestResults[testId] = 'running';
  renderUiTestsList();

  // Show running indicator
  const ind = document.getElementById('ui-test-running-indicator');
  const stepSpan = document.getElementById('ui-test-active-step');
  if (ind) ind.style.display = 'inline-block';

  // Clear/setup log element
  const logEl = document.getElementById('e2e-live-log');
  if (logEl && logEl.innerHTML.includes('Bereit für Testausführung')) {
    logEl.innerHTML = '';
  }

  const log = (msg) => {
    if (logEl) {
      const time = new Date().toLocaleTimeString('de-DE');
      logEl.innerHTML += `<div style="margin-bottom:4px;"><span style="color:var(--muted);">[${time}]</span> <span style="color:#a855f7; font-weight:bold;">[${testId}]</span> ${escapeHtml(msg)}</div>`;
      logEl.scrollTop = logEl.scrollHeight;
    }
    if (stepSpan) stepSpan.textContent = msg;
  };

  log(`▶️ Starte E2E Test: ${tc.name}...`);

  // Scroll row into view
  const row = document.getElementById(`tc-row-${testId}`);
  if (row) {
    row.style.background = 'rgba(168, 85, 247, 0.05)';
    row.scrollIntoView({ behavior: 'smooth', block: 'center' });
  }

  try {
    // Run the actual test
    await tc.run(state, log, switchView);

    state.uiTestResults[testId] = 'passed';
    log(`✔️ E2E Test ${testId} ERFOLGREICH bestanden!`);
  } catch (err) {
    state.uiTestResults[testId] = 'failed';
    log(`❌ E2E Test ${testId} FEHLGESCHLAGEN: ${err.message}`);
    console.error(err);
  } finally {
    if (row) {
      row.style.background = '';
    }
    if (ind) ind.style.display = 'none';
    renderUiTestsList();
  }
};

window.runAllUiTests = async function() {
  const btn = state.els.panels.tests?.querySelector('[data-action="run-all-ui-tests"]');
  if (btn) btn.disabled = true;

  const logEl = document.getElementById('e2e-live-log');
  if (logEl) {
    logEl.innerHTML = `<div style="color:#a855f7; font-weight:bold; margin-bottom:8px; border-bottom:1px solid rgba(168,85,247,0.2); padding-bottom:6px;">⚡ Starte Massen-Testausführung (Alle 8 E2E Test Cases)...</div>`;
  }

  // Reset all test results first
  uiTestCases.forEach(tc => {
    state.uiTestResults[tc.id] = 'pending';
  });
  renderUiTestsList();

  for (const tc of uiTestCases) {
    await window.runSingleUiTest(tc.id);
    await new Promise(resolve => setTimeout(resolve, 1000)); // Brief pause between tests
  }

  if (logEl) {
    const passedCount = uiTestCases.filter(tc => state.uiTestResults[tc.id] === 'passed').length;
    const totalCount = uiTestCases.length;
    logEl.innerHTML += `<div class="${passedCount === totalCount ? 'fibu-text-debit' : 'fibu-text-credit'}" style="font-weight:bold; margin-top:12px; border-top:1px solid var(--line); padding-top:6px;">🏁 Massenausführung beendet. Ergebnis: ${passedCount} / ${totalCount} bestanden.</div>`;
    logEl.scrollTop = logEl.scrollHeight;
  }

  if (btn) btn.disabled = false;
};

// =========================================================================
// ✈️ Reisekosten & Spesenabrechnung
// =========================================================================
window.renderTravelList = function() {
  const container = state.els.travelExpensesList;
  if (!container) return;

  const travels = state.journalEntries.filter(e => e.type === 'travel');

  if (travels.length === 0) {
    container.innerHTML = `<tr><td colspan="5" class="fibu-empty-state">Keine Reisekostenabrechnungen erfasst.</td></tr>`;
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
      <tr data-travel-click-id="${t.id}">
        <td><strong>✈️ ${escapeHtml(t.narration)}</strong></td>
        <td><span class="fibu-mono">${dateRange}</span></td>
        <td class="fibu-text-warning is-num" style="font-weight: 600;">${formatCents(totalAllowance)}</td>
        <td>
          <span class="ctox-badge ${isPosted ? 'is-success' : ''}">
            ${isPosted ? 'Verbucht 🔒' : 'Entwurf'}
          </span>
        </td>
        <td class="fibu-cell-center" onclick="event.stopPropagation();">
          ${!isPosted ? `<button class="ctox-button is-primary fibu-btn-xs" onclick="postTravelEntryDirectly('${t.id}')">Buchen</button>` : '🔒'}
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
          <label>Reisezweck / Destination</label>
          <input type="text" id="travel-purpose" class="ctox-input" placeholder="z.B. IT-Messe Berlin" required />
        </div>
      </div>
      <div class="fibu-form-row" style="margin-top:10px;">
        <div class="fibu-form-group">
          <label>Beginn (Datum & Uhrzeit)</label>
          <input type="datetime-local" id="travel-start" class="ctox-input" value="${todayStr}T08:00" required onchange="updateTravelVmaCalculation()" />
        </div>
        <div class="fibu-form-group">
          <label>Ende (Datum & Uhrzeit)</label>
          <input type="datetime-local" id="travel-end" class="ctox-input" value="${todayStr}T18:00" required onchange="updateTravelVmaCalculation()" />
        </div>
      </div>

      <div class="fibu-form-row" style="margin-top:15px; background:var(--surface-2); padding:15px; border-radius:8px; border:1px solid var(--line);">
        <div class="fibu-form-group">
          <label class="fibu-text-warning" style="font-weight:700;">Zahlungsweg (Ausgleichs-Gegenkonto)</label>
          <select id="travel-payment-method" class="ctox-select">
            <option value="privat">Privat getragen (Privatauslage des Inhabers - Gegenbuchung auf 1890)</option>
            <option value="verrechnung">Gesellschafter-Verrechnungskonto (Gegenbuchung auf 1370)</option>
          </select>
        </div>
      </div>

      <div style="margin-top:20px;">
        <h4 style="color:var(--text-strong); margin-bottom:8px;">Verpflegungsmehraufwand (VMA) Live-Berechnung:</h4>
        <div id="travel-days-container" style="display:flex; flex-direction:column; gap:10px; background:var(--surface-2); padding:12px; border-radius:8px; border:1px solid var(--line);">
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
            <div style="font-size:11px; opacity:0.8;">Pauschale: ${formatCents(dayAllowance)}</div>
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
    alert("Der berechnete Verpflegungsmehraufwand beträgt 0,00 €. Es kann keine Buchung erzeugt werden.");
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
  await db.accounting_journal_entries.insert({
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

  await db.accounting_journal_entry_lines.insert({
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

  await db.accounting_journal_entry_lines.insert({
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
    alert("Reisekostenabrechnung erfolgreich als Entwurf angelegt!");
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

  const doc = await db.accounting_journal_entries.findOne(entryId).exec();
  if (doc) {
    await doc.patch({
      number: nextNum,
      posted_at: now,
      updated_at_ms: now
    });
  }

  loadAllFibuData().then(() => {
    switchView('travel');
    alert("Reisekostenabrechnung erfolgreich verbucht und GoBD-festgeschrieben!");
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
    actualContainer.innerHTML = `<tr><td colspan="8" class="fibu-empty-state">Keine Fahrten im Fahrtenbuch eingetragen.</td></tr>`;
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
      <tr data-mileage-click-id="${t.id}">
        <td><span class="fibu-mono">${t.posting_date}</span></td>
        <td><strong>🚗 ${escapeHtml(metadata.destination || '')}</strong> <div style="font-size:11px; opacity:0.75;">Zweck: ${escapeHtml(t.narration || '')}</div></td>
        <td>${escapeHtml(metadata.contactPerson || '—')}</td>
        <td class="fibu-mono is-num">${km} km</td>
        <td class="fibu-text-warning is-num" style="font-weight: 600;">${formatCents(reimbursement)}</td>
        <td><span class="ctox-badge ${metadata.purpose === 'business' ? 'is-success' : ''}">${typeLabel}</span></td>
        <td>
          <span class="ctox-badge ${isPosted ? 'is-success' : ''}">
            ${isPosted ? 'Verbucht 🔒' : 'Entwurf'}
          </span>
        </td>
        <td class="fibu-cell-center" onclick="event.stopPropagation();">
          ${!isPosted && metadata.purpose === 'business' ? `<button class="ctox-button is-primary fibu-btn-xs" onclick="postMileageEntryDirectly('${t.id}')">Buchen</button>` : (isPosted ? '🔒' : '—')}
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
          <label>Datum</label>
          <input type="date" id="mileage-date" class="ctox-input" value="${todayStr}" required />
        </div>
        <div class="fibu-form-group">
          <label>Fahrttyp</label>
          <select id="mileage-purpose-type" class="ctox-select" onchange="updateMileageReimbursementLive()">
            <option value="business">Geschäftlich (Kilometerpauschale 0,30 €)</option>
            <option value="private">Privatfahrt (Keine Erstattung)</option>
            <option value="commute">Arbeitsweg (Wohnung/Arbeitsstätte)</option>
          </select>
        </div>
      </div>
      <div class="fibu-form-row">
        <div class="fibu-form-group">
          <label>Anfangs-Kilometerstand</label>
          <input type="number" id="mileage-start-km" class="ctox-input" placeholder="z.B. 12400" required oninput="updateMileageReimbursementLive()" />
        </div>
        <div class="fibu-form-group">
          <label>End-Kilometerstand</label>
          <input type="number" id="mileage-end-km" class="ctox-input" placeholder="z.B. 12550" required oninput="updateMileageReimbursementLive()" />
        </div>
      </div>
      <div class="fibu-form-row">
        <div class="fibu-form-group" style="flex:2;">
          <label>Route / Reiseziel (Start -> Ziel)</label>
          <input type="text" id="mileage-route" class="ctox-input" placeholder="Büro -> Kundengespräch Berlin -> Büro" required />
        </div>
        <div class="fibu-form-group">
          <label>Besuchter Partner / Kontakt</label>
          <input type="text" id="mileage-contact" class="ctox-input" placeholder="Firma Müller GmbH" />
        </div>
      </div>
      <div class="fibu-form-row">
        <div class="fibu-form-group" style="flex:2;">
          <label>Fahrtzweck / Notiz</label>
          <input type="text" id="mileage-purpose-detail" class="ctox-input" placeholder="Kundenpräsentation & Vertragsverhandlung" required />
        </div>
      </div>

      <div class="fibu-form-row" id="mileage-reimbursement-payment-wrap" style="margin-top:15px; background:var(--surface-2); padding:15px; border-radius:8px; border:1px solid var(--line);">
        <div class="fibu-form-group">
          <label class="fibu-text-warning" style="font-weight:700;">Zahlungsweg (Gegenbuchung)</label>
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
    alert("Der End-Kilometerstand muss größer als der Anfangs-Kilometerstand sein.");
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
  await db.accounting_journal_entries.insert({
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

    await db.accounting_journal_entry_lines.insert({
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

    await db.accounting_journal_entry_lines.insert({
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
    alert("Fahrt erfolgreich im Fahrtenbuch eingetragen!");
  });
};

window.postMileageEntryDirectly = async function(entryId) {
  const db = fibuDb();
  if (!db) return;

  const entry = state.journalEntries.find(e => e.id === entryId);
  if (!entry) return;

  const now = Date.now();
  const nextNum = 'J-2026-' + String(state.journalEntries.filter(e => e.posted_at).length + 1).padStart(4, '0');

  const doc = await db.accounting_journal_entries.findOne(entryId).exec();
  if (doc) {
    await doc.patch({
      number: nextNum,
      posted_at: now,
      updated_at_ms: now
    });
  }

  loadAllFibuData().then(() => {
    switchView('mileage');
    alert("Dienstreise-Erstattung erfolgreich verbucht und GoBD-festgeschrieben!");
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
    alert("Keine Fahrten vorhanden, um den jährlichen Privatanteil zu berechnen.");
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
          <label>Berechnungsmethode</label>
          <select id="car-share-method" class="ctox-select" onchange="updateCarShareLiveCalculations()">
            <option value="logbook">Fahrtenbuch-Methode (Ist-Kosten-Anteil: Privat-km * 0,30 €)</option>
            <option value="one_percent">1%-Pauschalmethode (30.000 € Listenpreis * 1% = 300,00 €)</option>
          </select>
        </div>
      </div>

      <div class="fibu-form-row">
        <div class="fibu-form-group">
          <label>Zu versteuernder Betrag (€)</label>
          <input type="number" step="0.01" id="car-share-amount" value="${defaultVal.toFixed(2)}" class="ctox-input" oninput="updateCarShareLiveCalculations()" />
        </div>
      </div>

      <div style="margin-top:15px; background:var(--surface-2); padding:15px; border-radius:8px; border:1px solid var(--line);">
        <h4 class="fibu-text-warning" style="margin-bottom:8px;">Steuerberater-Buchungsvorschlag (inkl. 19% USt):</h4>
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
  await db.accounting_journal_entries.insert({
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
  await db.accounting_journal_entry_lines.insert({
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
  await db.accounting_journal_entry_lines.insert({
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
  await db.accounting_journal_entry_lines.insert({
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
    alert("Jährliche Privatnutzung Firmenwagen erfolgreich verbucht!");
  });
};

function initBuchhaltungContextMenu(state) {
  state.contextMenu?.remove();
  const menu = document.createElement('div');
  menu.className = 'ctox-context-menu buchhaltung-context-menu';
  menu.hidden = true;
  document.body.append(menu);
  state.contextMenu = menu;

  const handleContextMenu = (event) => {
    if (state.ctx.module?.id !== 'buchhaltung') return;
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
  if (typeof state.ctx.canModifyModule === 'function' && state.ctx.canModifyModule()) return true;
  const user = state.ctx.session?.user || {};
  const role = String(user.role || (user.is_admin ? 'admin' : 'user')).trim().toLowerCase().replace(/^business_os_/, '');
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
        <label><input type="radio" name="contextMode" value="data" checked /> ${escapeHtml(chatWorkDataLabel)}</label>
        <label><input type="radio" name="contextMode" value="ask" /> ${escapeHtml(chatAnswerLabel)}</label>
        ${canModifyApp ? `<label><input type="radio" name="contextMode" value="app" /> ${escapeHtml(chatModifyAppLabel)}</label>` : ''}
      </div>
      <textarea data-buchhaltung-context-message placeholder="${escapeHtml(chatPlaceholder)}"></textarea>
      <footer>
        <span data-buchhaltung-context-status></span>
        <button type="submit">${escapeHtml(sendText)}</button>
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
  const status = state.contextMenu?.querySelector('[data-buchhaltung-context-status]');
  if (!trimmed) {
    if (status) status.textContent = getBuchhaltungText(state, 'chatMissingMessage', 'Nachricht fehlt.');
    return;
  }

  const safeMode = mode === 'app' && canModifyBuchhaltungApp(state) ? 'app' : (mode === 'ask' ? 'ask' : 'data');
  if (!document.querySelector('[data-ctox-chat-root]')) {
    if (status) status.textContent = getBuchhaltungText(state, 'chatNotReady', 'Chat ist noch nicht bereit.');
    return;
  }
  if (status) status.textContent = getBuchhaltungText(state, 'chatOpening', 'Oeffne Chat...');
  const titlePrefix = safeMode === 'app'
    ? 'Buchhaltung App modifizieren'
    : safeMode === 'ask'
      ? getBuchhaltungText(state, 'chatAnswerLabel', 'Frage beantworten')
      : 'Fibu bearbeiten';
  const title = `${titlePrefix} · ${context.label || 'Buchhaltung'}`;
  const instruction = safeMode === 'app'
    ? `Modifiziere die Buchhaltung-App anhand dieser Admin-Anweisung. Kontext nur als UI-Bezug verwenden, Buchhaltungsdaten selbst nicht als primäres Ziel verändern.\n\n${trimmed}`
    : safeMode === 'ask'
      ? `Beantworte die folgende Frage ausschließlich lesend. Nutze nur vorhandene Daten und Kontext; führe keine Änderungen an Daten, Records, Dateien oder der App aus. Antworte knapp und direkt.\n\n${trimmed}`
      : trimmed;

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

  const commandId = `cmd_buchhaltung_context_${crypto.randomUUID?.() || Date.now()}`;
  await state.ctx.commandBus.dispatch({
    id: commandId,
    module: 'buchhaltung',
    command_type: safeMode === 'app' ? 'ctox.business_os.app.modify' : 'business_os.chat.task',
    record_id: safeMode === 'app' ? 'buchhaltung' : (activeRecord?.id || 'buchhaltung'),
    inbound_channel: 'business_os.buchhaltung',
    payload: {
      title,
      instruction,
      prompt: trimmed,
      user_message: trimmed,
      mode: safeMode,
      target: safeMode === 'app' ? 'app' : (safeMode === 'ask' ? 'read' : 'data'),
      selected_record: activeRecord,
      context,
      thread_key: 'business-os/buchhaltung',
    },
    client_context: {
      action: 'context-chat',
      mode: safeMode,
      column: context.column,
      record_type: context.record_type,
      record_id: activeRecord?.id || '',
      source_module: 'buchhaltung',
    },
  });
  hideBuchhaltungContextMenu(state);
}
