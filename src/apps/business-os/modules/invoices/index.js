// index.js — entry point for the invoices module (2-pane IA-Karte).
//
// LEFT  = invoice list on the shell-owned canonical column grammar
//         (search / shard·list toggle / collapsed filter tray + reset /
//         counted view band Offen·Bezahlt·Überfällig, zeros included /
//         recessed .ctox-well / one-line footer). Header actions are the
//         collected .ctox-pane-icon set: Neu, Import, Export.
// MAIN  = invoice detail / draft editor (line items, totals, status/element
//         actions as collected icons). There is NO third column — the customer
//         snapshot is folded into the detail.
//
// The chrome behaviour (search / tray / reset / active-dot / view-toggle /
// band) is SHELL-wired from the data-pg-* attributes (autoWirePaneGrammar in
// app.js). This module writes NO chrome CSS and NO chrome JS: it re-renders on
// the bubbling `ctox-pane-grammar-change` event and writes counts/footer via
// `pane.__ctoxPaneGrammar` (null-guarded). The grammar markup is assembled with
// createElement so the same code renders in the browser and in the header-less
// node test shims; the shell still discovers the data-pg-* pane and wires it.
//
// Mount contract (v5, skill `business-os-app-module-development`):
//   - mount(ctx) returns a cleanup function that detaches every collection.$
//     subscription and DOM listener opened during the mount.
//   - All reads go through `resolveCollection(name)` on the `ctx.db.collection`
//     facade. Mutations go through `ctx.commandBus.dispatch(...)`; native
//     handlers in `src/core/business_os/invoices.rs` own GoBD-immutability. The
//     command flows and collection schemas are unchanged from the prior IA.
//   - Reactive sync: we subscribe to `collection.$` for every watched
//     collection and coalesce emissions into one render via `scheduleRefresh`.
//     No manual refresh button.

import {
  buildCreateInvoiceCommand,
  buildUpdateInvoiceCommand,
  buildDeleteInvoiceCommand,
  buildXRechnungXml,
} from './commands/builders.js';
import { validateInvoice } from './core/invoice-validate.js';

const BUILD = '20260721-invoices-ia-two-pane';
const MODULE_ID = 'invoices';
const SKILL_TAG = 'product_engineering/business-os-app-module-development';

const COPY = {
  de: {
    invoices: 'Rechnungen', kicker: 'CTOX', all: 'Alle', overdue: 'Überfällig', open: 'Offen', paid: 'Bezahlt',
    newInvoice: 'Neue Rechnung', import: 'Importieren', export: 'Exportieren', closeDetail: 'Details schließen',
    unknown: 'unbekannt', newShort: 'NEU', entries: 'Einträge', search: 'Suchen...', view: 'Darstellung',
    cardsView: 'Shard-Ansicht', listView: 'Listen-Ansicht', filter: 'Filter', allTypes: 'Alle Typen', filterType: 'Typ',
    resetFilters: 'Filter zurücksetzen',
    emptyHint: 'Wähle eine Rechnung aus der Liste oder erstelle einen neuen Entwurf.', invoice: 'Rechnung', draft: 'Entwurf',
    customer: 'Kunde', chooseCustomer: '— bitte Kunde wählen —', invoiceDate: 'Rechnungsdatum', type: 'Typ',
    lines: 'Positionen', addLine: '+ Position', net: 'Netto', tax: 'USt', gross: 'Brutto',
    saveDraft: 'Entwurf speichern', deleteDraft: 'Entwurf löschen', post: 'Buchen (GoBD-post)', missingBeforePost: 'Vor dem Buchen fehlt',
    removeLine: 'Position entfernen', invoiceNumber: 'Rechnungsnummer', date: 'Datum', due: 'Fällig', journal: 'Journal',
    payments: 'Zahlungen', dunning: 'Mahnen', noJournal: 'Kein Journal-Eintrag verknüpft.', account: 'Konto', description: 'Beschreibung', debit: 'Soll', credit: 'Haben',
    downloadXml: 'XRechnung-XML herunterladen', xmlFailed: 'XRechnung-Vorschau fehlgeschlagen', amountCents: 'Betrag (Cent)', discountCents: 'Skonto (Cent)',
    paymentId: 'Zahlungs-ID', allocate: 'Zuordnen', discountHint: 'Skonto wird nur abgezogen, wenn das Zahlungsdatum vor dem Skonto-Deadline liegt. Das berechnet der native Handler.',
    dunningOnlyOverdue: 'Dunning ist nur für überfällige Rechnungen verfügbar.', dunningHint: 'Diese Rechnung ist überfällig. Starte einen Mahnlauf, um einen Brief zu erzeugen.',
    dunningRun: 'Mahnlauf für diese Rechnung', address: 'Adresse', email: 'E-Mail', noAddress: 'Keine Adresse hinterlegt.',
    dependencyTitle: 'Rechnungen benötigt weitere Module', dependencyNote: 'Bitte installiere „buchhaltung“ (FIBU/Journal) und „customers“ (Party-Stamm) im App Store, dann lade das Rechnungen-Modul neu.',
    reload: 'Neu laden', missingDb: 'Invoices-Modul kann nicht starten: ctx.db fehlt.', noCustomer: 'Kein Kunde im CRM hinterlegt. Lege zuerst einen Kunden im „customers“-Modul an, dann erstelle die Rechnung hier.',
    deleteConfirm: 'Entwurf {id} löschen?', cannotPost: 'Rechnung kann nicht gebucht werden', resizeColumn: 'Spaltenbreite anpassen',
    stateDraft: 'Entwurf', statePosted: 'Gebucht', statePartiallyPaid: 'Teilweise bezahlt', statePaid: 'Bezahlt', stateOverdue: 'Überfällig', stateCancelled: 'Storniert', stateCredited: 'Gutgeschrieben',
    pos: 'Pos', quantity: 'Menge (‰)', unit: 'Einheit', unitPrice: 'Einzelpreis (Cent)',
    importInvalid: 'Ungültige JSON-Datei.', importEmpty: 'Keine gültigen Rechnungen (Kunde + Positionen) in der Datei.', imported: '{count} importiert',
  },
  en: {
    invoices: 'Invoices', kicker: 'CTOX', all: 'All', overdue: 'Overdue', open: 'Open', paid: 'Paid',
    newInvoice: 'New invoice', import: 'Import', export: 'Export', closeDetail: 'Close details',
    unknown: 'unknown', newShort: 'NEW', entries: 'entries', search: 'Search...', view: 'View',
    cardsView: 'Shard view', listView: 'List view', filter: 'Filter', allTypes: 'All types', filterType: 'Type',
    resetFilters: 'Reset filters',
    emptyHint: 'Select an invoice from the list or create a new draft.', invoice: 'Invoice', draft: 'Draft',
    customer: 'Customer', chooseCustomer: '— select customer —', invoiceDate: 'Invoice date', type: 'Type',
    lines: 'Line items', addLine: '+ Line item', net: 'Net', tax: 'VAT', gross: 'Gross',
    saveDraft: 'Save draft', deleteDraft: 'Delete draft', post: 'Post (GoBD)', missingBeforePost: 'Required before posting',
    removeLine: 'Remove line item', invoiceNumber: 'Invoice number', date: 'Date', due: 'Due', journal: 'Journal',
    payments: 'Payments', dunning: 'Dunning', noJournal: 'No journal entry linked.', account: 'Account', description: 'Description', debit: 'Debit', credit: 'Credit',
    downloadXml: 'Download XRechnung XML', xmlFailed: 'XRechnung preview failed', amountCents: 'Amount (cents)', discountCents: 'Discount (cents)',
    paymentId: 'Payment ID', allocate: 'Allocate', discountHint: 'The native handler applies the discount only when payment occurs before the discount deadline.',
    dunningOnlyOverdue: 'Dunning is available only for overdue invoices.', dunningHint: 'This invoice is overdue. Start a dunning run to generate a letter.',
    dunningRun: 'Run dunning for this invoice', address: 'Address', email: 'Email', noAddress: 'No address recorded.',
    dependencyTitle: 'Invoices requires additional modules', dependencyNote: 'Install “buchhaltung” (ledger/journal) and “customers” (party master data) from the App Store, then reload Invoices.',
    reload: 'Reload', missingDb: 'Invoices cannot start: ctx.db is missing.', noCustomer: 'No customer exists in CRM. Create a customer in “customers” before creating an invoice.',
    deleteConfirm: 'Delete draft {id}?', cannotPost: 'Invoice cannot be posted', resizeColumn: 'Adjust column width',
    stateDraft: 'Draft', statePosted: 'Posted', statePartiallyPaid: 'Partially paid', statePaid: 'Paid', stateOverdue: 'Overdue', stateCancelled: 'Cancelled', stateCredited: 'Credited',
    pos: 'Pos', quantity: 'Quantity (‰)', unit: 'Unit', unitPrice: 'Unit price (cents)',
    importInvalid: 'Invalid JSON file.', importEmpty: 'No valid invoices (customer + line items) in the file.', imported: '{count} imported',
  },
};

// Invoice types offered in the editor + tray filter.
const INVOICE_TYPES = Object.freeze(['sale_out', 'sale_in', 'credit_note_out', 'credit_note_in', 'recurring_template']);
const TYPE_LABELS = {
  de: { sale_out: 'Ausgangsrechnung', sale_in: 'Eingangsrechnung', credit_note_out: 'Gutschrift (aus)', credit_note_in: 'Gutschrift (ein)', recurring_template: 'Abo-Vorlage' },
  en: { sale_out: 'Outgoing invoice', sale_in: 'Incoming invoice', credit_note_out: 'Credit note (out)', credit_note_in: 'Credit note (in)', recurring_template: 'Recurring template' },
};

// The counted view band (design-guide "Canonical Column Grammar"): Alle plus the
// three real financial buckets. Draft / cancelled / credited invoices only
// appear under "Alle".
const BAND_ORDER = Object.freeze(['alle', 'offen', 'bezahlt', 'ueberfaellig']);

// Editor/detail line table columns; `num` columns render right-aligned.
const LINE_COLUMNS = Object.freeze([
  { labelKey: 'pos' },
  { labelKey: 'description' },
  { labelKey: 'quantity', num: true },
  { labelKey: 'unit' },
  { labelKey: 'unitPrice', num: true },
  { label: 'USt %', num: true },
  { label: 'SKR' },
  { label: '' },
]);

// Kit-style monochrome action glyphs (viewBox 0 0 24 24, stroke 1.8, round caps).
const ICON = Object.freeze({
  new: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 5v14M5 12h14"/></svg>',
  import: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3v12M12 15l-4-4M12 15l4-4M5 21h14"/></svg>',
  export: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 21V9M12 9l-4 4M12 9l4 4M5 3h14"/></svg>',
  cards: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="4" y="4" width="16" height="7" rx="1.5"/><rect x="4" y="14" width="16" height="7" rx="1.5"/></svg>',
  list: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="4" y1="6" x2="20" y2="6"/><line x1="4" y1="12" x2="20" y2="12"/><line x1="4" y1="18" x2="20" y2="18"/></svg>',
  filter: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="4" y1="7" x2="20" y2="7"/><line x1="4" y1="12" x2="20" y2="12"/><line x1="4" y1="17" x2="20" y2="17"/><circle cx="9" cy="7" r="2.4" fill="var(--surface-2)"/><circle cx="15" cy="12" r="2.4" fill="var(--surface-2)"/><circle cx="8" cy="17" r="2.4" fill="var(--surface-2)"/></svg>',
  reset: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 10a8 8 0 1 1 2 7"/><path d="M4 5v5h5"/></svg>',
  close: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"/></svg>',
  trash: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 7h16M9 7V5h6v2M6 7l1 13h10l1-13"/></svg>',
  add: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 5v14M5 12h14"/></svg>',
});

// Collections whose mutations should re-render the invoices shell.
const WATCHED_COLLECTIONS = Object.freeze([
  'accounting_invoices',
  'accounting_invoice_lines',
  'accounting_payments',
  'accounting_payment_allocations',
  'accounting_dunning_runs',
  'accounting_dunning_letters',
  'accounting_journal_entries',
  'accounting_journal_entry_lines',
  'customer_accounts',
]);

const REQUIRED_MODULES = ['buchhaltung', 'customers'];
const STATE_LABEL_KEYS = { draft: 'stateDraft', posted: 'statePosted', partially_paid: 'statePartiallyPaid', paid: 'statePaid', overdue: 'stateOverdue', cancelled: 'stateCancelled', credited: 'stateCredited' };

const STATE = {
  ctx: null,
  cleanup: [],
  renderTimer: 0,
  invoices: [],
  parties: {},
  selectedInvoiceId: null,
  userCollapsed: false,
  search: '',
  view: 'cards',
  band: 'alle',
  filterType: 'all',
  lineDraft: null,
  busy: false,
  lastError: null,
  locale: 'de',
  frame: null,
};

// ---------------------------------------------------------------------------
// Pure helpers (exported for tests — no DOM, no command bus).
// ---------------------------------------------------------------------------

// Which counted band a real invoice status falls into. Draft / cancelled /
// credited are not one of the three financial buckets — they surface only under
// "Alle".
export function invoiceBand(inv) {
  const state = inv && inv.state;
  if (state === 'overdue') return 'ueberfaellig';
  if (state === 'paid') return 'bezahlt';
  if (state === 'posted' || state === 'partially_paid') return 'offen';
  return 'other';
}

// Counted band totals; zeros are always present (design-guide: zeros included).
export function countsFor(rows) {
  const list = Array.isArray(rows) ? rows : [];
  const counts = { alle: list.length, offen: 0, bezahlt: 0, ueberfaellig: 0 };
  for (const inv of list) {
    const band = invoiceBand(inv);
    if (band !== 'other') counts[band] += 1;
  }
  return counts;
}

// Apply the current grammar state (band + type filter + search) to the rows.
// `nameOf` optionally resolves a party name so search can match the customer.
export function filterInvoices(rows, { band = 'alle', type = 'all', search = '' } = {}, nameOf) {
  const needle = String(search || '').trim().toLowerCase();
  const resolve = typeof nameOf === 'function' ? nameOf : () => '';
  return (Array.isArray(rows) ? rows : []).filter((inv) => {
    if (band && band !== 'alle' && invoiceBand(inv) !== band) return false;
    if (type && type !== 'all' && inv.invoice_type !== type) return false;
    if (needle) {
      const hay = [inv.invoice_number, inv.party_id, resolve(inv), inv.invoice_type, inv.state]
        .filter(Boolean).join(' ').toLowerCase();
      if (!hay.includes(needle)) return false;
    }
    return true;
  });
}

// Auto-reveal model (design-guide "Progressive Disclosure", outbound idiom): the
// detail/editor is shown only when a record is selected and the user has not
// collapsed it.
export function shouldRevealDetail(hasSelection, userCollapsed) {
  return Boolean(hasSelection) && !userCollapsed;
}

// Export = JSON of the VISIBLE invoices (honest and small — the actual records).
export function buildInvoicesExport(rows, nowMs) {
  return {
    kind: 'ctox-invoices-export',
    exported_at_ms: Number(nowMs) || 0,
    invoices: (Array.isArray(rows) ? rows : []).map((inv) => ({ ...inv })),
  };
}

// Import → create payloads for the existing write path. Accepts a bare array or
// an object with an `invoices` array; keeps only entries with a customer and at
// least one line item (the native create handler validates the rest).
export function parseInvoiceImport(raw, nowMs) {
  const now = Number(nowMs) || 0;
  const src = raw && typeof raw === 'object' ? raw : {};
  const list = Array.isArray(raw)
    ? raw
    : (Array.isArray(src.invoices) ? src.invoices : (raw && typeof raw === 'object' ? [raw] : []));
  const out = [];
  for (const item of list) {
    if (!item || typeof item !== 'object') continue;
    const partyId = String(item.party_id || item.customer_id || '').trim();
    const lines = sanitizeImportLines(item.lines);
    if (!partyId || lines.length === 0) continue;
    const invoiceType = INVOICE_TYPES.includes(item.invoice_type) ? item.invoice_type : 'sale_out';
    const invoiceDate = Number.isFinite(item.invoice_date_ms) ? item.invoice_date_ms : now;
    const dueDate = Number.isFinite(item.due_date_ms) ? item.due_date_ms : (Number.isFinite(invoiceDate) ? invoiceDate + 14 * 86_400_000 : null);
    out.push({
      invoice_type: invoiceType,
      party_id: partyId,
      invoice_date_ms: invoiceDate,
      due_date_ms: dueDate,
      currency: typeof item.currency === 'string' && item.currency ? item.currency : 'EUR',
      lines,
    });
  }
  return out;
}

function sanitizeImportLines(lines) {
  return (Array.isArray(lines) ? lines : [])
    .filter((line) => line && typeof line === 'object')
    .map((line, index) => ({
      id: String(line.id || `line_imp_${index + 1}`),
      position: Number(line.position) || index + 1,
      description: String(line.description || ''),
      quantity: Math.round(Number(line.quantity) || 0),
      unit: String(line.unit || 'Stk'),
      unit_price_cents: Math.round(Number(line.unit_price_cents) || 0),
      tax_rate: Number.isFinite(Number(line.tax_rate)) ? Number(line.tax_rate) : 0.19,
      account_code: String(line.account_code || '8400'),
    }));
}

// A shard is a pure selector: number/draft title + ONE muted meta line + a
// status badge. No inline expansion, no per-row buttons. Selection is expressed
// as `is-selected` / aria-selected so it can be flipped in place without a
// rebuild (design-guide: re-renders never move the operator).
export function renderInvoiceRow(inv, opts = {}) {
  const view = opts.view === 'list' ? 'list' : 'cards';
  const selected = Boolean(opts.selected);
  const number = inv.invoice_number || t('newShort');
  const stateLabel = invoiceStateLabel(inv.state);
  const badge = `<span class="ctox-badge ${stateBadgeClass(inv.state)}">${escapeHtml(stateLabel)}</span>`;
  const attrs = `class="ctox-list-item invoices-row invoices-row--${view}${selected ? ' is-selected' : ''}"`
    + ' role="button" tabindex="0"'
    + ` aria-selected="${selected ? 'true' : 'false'}"`
    + ` data-invoice-id="${escapeHtml(inv.id)}"`
    + ` data-context-record-id="${escapeHtml(inv.id)}"`
    + ' data-context-record-type="accounting_invoices"'
    + ` data-context-label="${escapeHtml(number)}"`;
  if (view === 'list') {
    return `<div ${attrs}><span class="invoices-row-title">${escapeHtml(number)}</span>${badge}</div>`;
  }
  const meta = [escapeHtml(partyName(inv.party_id)), escapeHtml(formatCents(inv.total_cents))].join(' · ');
  return `<div ${attrs}>`
    + `<div class="invoices-row-head"><span class="invoices-row-title">${escapeHtml(number)}</span>${badge}</div>`
    + `<div class="invoices-row-meta">${meta}</div>`
    + '</div>';
}

export function renderInvoiceListMarkup(rows, opts = {}) {
  const list = Array.isArray(rows) ? rows : [];
  if (!list.length) {
    return `<div class="ctox-empty">${escapeHtml(opts.emptyText || t('emptyHint'))}</div>`;
  }
  const selectedId = opts.selectedId ?? null;
  return list.map((inv) => renderInvoiceRow(inv, { view: opts.view, selected: inv.id === selectedId })).join('');
}

export function computeInvoiceTotals(inv) {
  let subtotal = 0;
  let tax = 0;
  const byRate = new Map();
  for (const line of inv.lines || []) {
    const net = computeLineNetCents(line);
    const rate = Number(line.tax_rate) || 0;
    const taxCents = Math.round(net * rate);
    subtotal += net;
    tax += taxCents;
    if (rate > 0) {
      const key = rate.toFixed(4);
      const entry = byRate.get(key) || { tax_rate: rate, net_cents: 0, tax_cents: 0 };
      entry.net_cents += net;
      entry.tax_cents += taxCents;
      byRate.set(key, entry);
    }
  }
  return {
    subtotal_cents: subtotal,
    tax_cents: tax,
    total_cents: subtotal + tax,
    tax_breakdown: [...byRate.values()],
  };
}

function computeLineNetCents(line) {
  const quantity = Number(line.quantity) || 0;
  const unitPrice = Number(line.unit_price_cents) || 0;
  const discount = Number.isFinite(Number(line.discount_percent))
    ? Math.max(0, Math.min(100, Number(line.discount_percent))) / 100
    : 0;
  const discountedUnit = Math.round(unitPrice * (1 - discount));
  return Math.round((discountedUnit * quantity) / 1000);
}

// ---------------------------------------------------------------------------
// i18n + small formatting helpers
// ---------------------------------------------------------------------------

function t(key, replacements = {}) {
  let value = COPY[STATE.locale]?.[key] ?? COPY.de[key] ?? key;
  for (const [name, replacement] of Object.entries(replacements)) value = value.replace(`{${name}}`, String(replacement));
  return value;
}

function invoiceStateLabel(state) {
  return t(STATE_LABEL_KEYS[state] || 'unknown');
}

function typeLabel(type) {
  return TYPE_LABELS[STATE.locale]?.[type] ?? TYPE_LABELS.de[type] ?? type;
}

function bandLabel(band) {
  return { alle: t('all'), offen: t('open'), bezahlt: t('paid'), ueberfaellig: t('overdue') }[band] || t('all');
}

function stateBadgeClass(state) {
  if (state === 'paid') return 'is-success';
  if (state === 'overdue' || state === 'cancelled') return 'is-danger';
  if (state === 'partially_paid') return 'is-warning';
  return '';
}

function partyName(partyId) {
  return STATE.parties[partyId]?.name || partyId || '—';
}

function formatCents(cents) {
  if (!Number.isFinite(cents)) return '–';
  return `${(cents / 100).toFixed(2)} EUR`;
}

function formatMilli(quantity) {
  if (!Number.isFinite(quantity)) return '–';
  return (quantity / 1000).toFixed(3);
}

function isoDateInput(ms) {
  if (!Number.isFinite(ms)) return '';
  const d = new Date(ms);
  const y = d.getUTCFullYear();
  const m = String(d.getUTCMonth() + 1).padStart(2, '0');
  const day = String(d.getUTCDate()).padStart(2, '0');
  return `${y}-${m}-${day}`;
}

function computeDueDateMs(invoiceDateMs, netDays) {
  if (!Number.isFinite(invoiceDateMs) || !Number.isFinite(netDays)) return null;
  return invoiceDateMs + netDays * 86_400_000;
}

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

// ---------------------------------------------------------------------------
// Mount / lifecycle
// ---------------------------------------------------------------------------

export async function mount(ctx) {
  resetState(ctx);
  ensureModuleStyles();
  await ensureMountedMarkup(ctx);
  if (!ctx?.db) {
    renderError(t('missingDb'));
    return () => {};
  }
  if (!isReady()) {
    renderDependencyBlocker();
    return () => {};
  }
  await refresh();
  render();
  STATE.cleanup.push(wireRealtime());
  // Cross-module signal: a customer edit elsewhere must refresh the folded-in
  // party snapshot. eventBus survives schema-drift recovery.
  if (ctx.eventBus?.on) {
    const off = ctx.eventBus.on('customers.account.updated', () => scheduleRefresh());
    if (typeof off === 'function') STATE.cleanup.push(off);
  }
  return () => {
    for (const cleanup of STATE.cleanup.splice(0)) {
      try { cleanup?.(); } catch {}
    }
    if (STATE.renderTimer) window.clearTimeout(STATE.renderTimer);
  };
}

function resetState(ctx) {
  STATE.ctx = ctx;
  STATE.cleanup = [];
  STATE.renderTimer = 0;
  STATE.invoices = [];
  STATE.parties = {};
  STATE.selectedInvoiceId = null;
  STATE.userCollapsed = false;
  STATE.search = '';
  STATE.view = 'cards';
  STATE.band = 'alle';
  STATE.filterType = 'all';
  STATE.lineDraft = null;
  STATE.busy = false;
  STATE.lastError = null;
  STATE.locale = String(ctx.locale || globalThis.document?.documentElement?.lang || 'de').toLowerCase().startsWith('en') ? 'en' : 'de';
  STATE.frame = null;
}

function isReady() {
  if (!STATE.ctx?.modules) return false;
  for (const id of REQUIRED_MODULES) {
    const mod = STATE.ctx.modules.find?.((m) => m?.id === id);
    if (!mod || mod.installed === false) return false;
  }
  return true;
}

function resolveCollection(name) {
  if (!STATE.ctx?.db) return null;
  return STATE.ctx.db.collection?.(name) || null;
}

function wireRealtime() {
  const subscriptions = WATCHED_COLLECTIONS
    .map((name) => resolveCollection(name)?.$?.subscribe?.(() => scheduleRefresh()))
    .filter(Boolean);
  return () => subscriptions.forEach((sub) => {
    try { sub.unsubscribe?.(); } catch {}
  });
}

function scheduleRefresh() {
  if (STATE.renderTimer) return;
  STATE.renderTimer = window.setTimeout(() => {
    STATE.renderTimer = 0;
    refresh().then(render).catch(reportError);
  }, 80);
}

async function refresh() {
  const invoices = await readCollection('accounting_invoices');
  const parties = await readCollection('customer_accounts');
  STATE.parties = Object.fromEntries(parties.map((p) => [p.id, p]));
  STATE.invoices = (invoices || []).filter((inv) => !inv.is_deleted);
  STATE.lastError = null;
}

async function readCollection(name) {
  const c = resolveCollection(name);
  if (!c) return [];
  if (typeof c.find === 'function' && typeof c.find().exec === 'function') {
    const docs = await c.find().exec();
    return docs.map((doc) => doc?.toJSON?.() || doc).filter((doc) => doc && doc._deleted !== true && doc.is_deleted !== true);
  }
  if (typeof c.all === 'function') return c.all();
  return Array.isArray(c) ? c : [];
}

async function submitCommand(command) {
  if (STATE.busy) throw new Error('invoices: another command is in flight');
  STATE.busy = true;
  try {
    const outcome = await STATE.ctx.commandBus.dispatch(command);
    STATE.lastError = null;
    return outcome;
  } catch (err) {
    STATE.lastError = err?.message || String(err);
    throw err;
  } finally {
    STATE.busy = false;
  }
}

function reportError(err) {
  console.error('invoices error:', err);
  STATE.lastError = err?.message || String(err);
  render();
}

// ---------------------------------------------------------------------------
// Frame (built once) + rendering
// ---------------------------------------------------------------------------

function createEl(tag, className) {
  const el = document.createElement(tag);
  if (className) el.className = className;
  return el;
}

function add(parent, ...kids) {
  for (const kid of kids) if (kid) parent.appendChild(kid);
  return parent;
}

function paneIcon(svg, label, { action, extraClass } = {}) {
  const button = createEl('button', 'ctox-pane-icon' + (extraClass ? ` ${extraClass}` : ''));
  button.type = 'button';
  button.innerHTML = svg;
  button.setAttribute('aria-label', label);
  button.title = label;
  if (action) button.dataset.action = action;
  return button;
}

function buildTitleRow(kickerText, titleText, actionsEl, titleTag = 'h2') {
  const row = createEl('div', 'ctox-pane-title-row');
  const titles = createEl('div', 'ctox-pane-titles');
  const kicker = createEl('span', 'ctox-pane-kicker');
  kicker.textContent = kickerText;
  const title = createEl(titleTag, 'ctox-pane-title');
  title.textContent = titleText;
  add(titles, kicker, title);
  add(row, titles);
  if (actionsEl) row.appendChild(actionsEl);
  return row;
}

function buildColumnResizer(side, cssVar) {
  const handle = createEl('button', 'ctox-column-resizer');
  handle.type = 'button';
  handle.dataset.resizer = side;
  handle.dataset.resizerVar = cssVar;
  handle.dataset.resizerMin = '260';
  handle.dataset.resizerMax = '480';
  handle.setAttribute('aria-label', t('resizeColumn'));
  return handle;
}

// Assemble the left column's canonical grammar (declarative data-pg-* markup the
// shell wires). Returns the live refs the module updates on re-render.
function buildLeftGrammar(leftPane) {
  // Header: kicker + title, then the collected icon actions (Neu / Import /
  // Export), then the filterbar and collapsed tray.
  const actions = createEl('div', 'ctox-pane-actions');
  add(actions,
    paneIcon(ICON.new, t('newInvoice'), { action: 'new', extraClass: 'is-primary' }),
    paneIcon(ICON.import, t('import'), { action: 'import' }),
    paneIcon(ICON.export, t('export'), { action: 'export' }));

  const header = createEl('header', 'ctox-pane-header ctox-pane-band');
  add(header, buildTitleRow(t('kicker'), t('invoices'), actions));

  const filterbar = createEl('div', 'ctox-filterbar');
  const search = createEl('input', 'ctox-pane-search');
  search.type = 'search';
  search.setAttribute('data-pg-search', '');
  search.setAttribute('placeholder', t('search'));
  search.setAttribute('aria-label', t('search'));
  const viewToggle = createEl('div', 'ctox-view-toggle');
  viewToggle.setAttribute('role', 'group');
  viewToggle.setAttribute('aria-label', t('view'));
  const cardsBtn = paneIcon(ICON.cards, t('cardsView'));
  cardsBtn.setAttribute('data-pg-view', 'cards');
  cardsBtn.setAttribute('aria-pressed', 'true');
  const listBtn = paneIcon(ICON.list, t('listView'));
  listBtn.setAttribute('data-pg-view', 'list');
  listBtn.setAttribute('aria-pressed', 'false');
  add(viewToggle, cardsBtn, listBtn);
  const filterToggle = paneIcon(ICON.filter, t('filter'), { extraClass: 'ctox-filter-toggle' });
  filterToggle.setAttribute('data-pg-tray-toggle', '');
  filterToggle.setAttribute('aria-expanded', 'false');
  add(filterbar, search, viewToggle, filterToggle);
  add(header, filterbar);

  const tray = createEl('div', 'ctox-filter-tray');
  tray.setAttribute('data-pg-tray', '');
  tray.hidden = true;
  const filterRow = createEl('div', 'ctox-filter-row');
  const typeSelect = createEl('select', 'ctox-select');
  typeSelect.setAttribute('data-pg-filter', '');
  typeSelect.setAttribute('data-pg-name', 'type');
  typeSelect.setAttribute('data-pg-default', 'all');
  typeSelect.setAttribute('aria-label', t('filterType'));
  const allOption = createEl('option');
  allOption.value = 'all';
  allOption.textContent = t('allTypes');
  typeSelect.appendChild(allOption);
  for (const type of INVOICE_TYPES) {
    const opt = createEl('option');
    opt.value = type;
    opt.textContent = typeLabel(type);
    typeSelect.appendChild(opt);
  }
  const resetBtn = paneIcon(ICON.reset, t('resetFilters'), { extraClass: 'ctox-sort-dir' });
  resetBtn.classList.remove('ctox-pane-icon');
  resetBtn.setAttribute('data-pg-reset', '');
  add(filterRow, typeSelect, resetBtn);
  add(tray, filterRow);
  add(header, tray);

  // Counted view band: Alle + the three financial buckets (zeros included).
  const nav = createEl('nav', 'ctox-view-switch');
  nav.setAttribute('aria-label', t('view'));
  const tabs = createEl('div', 'ctox-pane-tabs');
  const countEls = {};
  for (const band of BAND_ORDER) {
    const tab = createEl('button', 'ctox-pane-tab' + (band === STATE.band ? ' is-active' : ''));
    tab.type = 'button';
    tab.setAttribute('role', 'tab');
    tab.dataset.pgBand = band;
    tab.setAttribute('aria-selected', band === STATE.band ? 'true' : 'false');
    const label = createEl('span');
    label.textContent = bandLabel(band);
    const count = createEl('span', 'view-count');
    count.dataset.pgCount = band;
    add(tab, label, count);
    tabs.appendChild(tab);
    countEls[band] = count;
  }
  add(nav, tabs);

  const well = createEl('div', 'ctox-pane-body ctox-well');
  const listEl = createEl('div', 'ctox-record-list');
  listEl.dataset.invoicesList = '';
  add(well, listEl);

  const footer = createEl('footer', 'ctox-pane-footer');
  const footerEl = createEl('span');
  footerEl.setAttribute('data-pg-footer', '');
  add(footer, footerEl);

  add(leftPane, header, nav, well, footer);
  return { listEl, countEls, footerEl };
}

function ensureFrame(root) {
  if (STATE.frame) return STATE.frame;

  const workspace = createEl('main', 'ctox-workspace ctox-workspace--two-pane invoices-module');
  workspace.dataset.resizeFrame = '';

  const leftPane = createEl('aside', 'ctox-pane invoices-rail');
  leftPane.dataset.leftContent = '';
  leftPane.dataset.contextModule = MODULE_ID;
  leftPane.dataset.contextSubmodule = 'list';
  leftPane.dataset.contextRecordType = 'accounting_invoices';
  const grammar = buildLeftGrammar(leftPane);

  const resizer = buildColumnResizer('left', '--ctox-left-width');

  const mainPane = createEl('section', 'ctox-pane invoices-main');
  mainPane.dataset.contextModule = MODULE_ID;
  mainPane.dataset.contextSubmodule = 'center';
  mainPane.dataset.contextRecordType = 'accounting_invoices';

  add(workspace, leftPane, resizer, mainPane);

  const banner = createEl('div', 'invoices-error-banner');
  banner.hidden = true;

  // innerHTML='' + appendChild (not replaceChildren) so the fake-DOM test shims
  // — whose replaceChildren drops its arguments — still see the frame.
  root.innerHTML = '';
  add(root, workspace, banner);

  STATE.frame = { workspace, leftPane, mainPane, banner, listEl: grammar.listEl, countEls: grammar.countEls, footerEl: grammar.footerEl };
  wireFrameEvents();
  return STATE.frame;
}

function wireFrameEvents() {
  const f = STATE.frame;
  f.listEl.addEventListener('click', onListClick);
  f.listEl.addEventListener('keydown', onListKey);
  f.workspace.addEventListener('click', onAction);
  // Shell-wired chrome reports every search/view/band/tray change here.
  f.leftPane.addEventListener('ctox-pane-grammar-change', onGrammarChange);
  STATE.cleanup.push(() => {
    try { f.listEl.removeEventListener?.('click', onListClick); } catch {}
    try { f.listEl.removeEventListener?.('keydown', onListKey); } catch {}
    try { f.workspace.removeEventListener?.('click', onAction); } catch {}
    try { f.leftPane.removeEventListener?.('ctox-pane-grammar-change', onGrammarChange); } catch {}
  });
}

function render() {
  const root = moduleRoot();
  if (!root) return;
  root.classList.add('invoices-shell');
  root.dataset.contextModule = MODULE_ID;
  root.dataset.contextSubmodule = 'shell';
  root.dataset.contextSkill = SKILL_TAG;
  ensureFrame(root);
  renderList();
  renderMain();
  syncBanner();
}

function visibleInvoices() {
  const rows = filterInvoices(
    STATE.invoices,
    { band: STATE.band, type: STATE.filterType, search: STATE.search },
    (inv) => partyName(inv.party_id),
  );
  return rows.sort((a, b) => (b.updated_at_ms || 0) - (a.updated_at_ms || 0));
}

function selectedInvoice() {
  return STATE.invoices.find((i) => i.id === STATE.selectedInvoiceId) || null;
}

function renderList() {
  const f = STATE.frame;
  if (!f?.listEl) return;
  const rows = visibleInvoices();
  if (STATE.view === 'list') f.listEl.classList.add('is-list-view');
  else f.listEl.classList.remove('is-list-view');
  f.listEl.innerHTML = renderInvoiceListMarkup(rows, { view: STATE.view, selectedId: STATE.selectedInvoiceId });
  writeCounts(countsFor(STATE.invoices));
  writeFooter(`${rows.length} ${t('entries')} · ${bandLabel(STATE.band)}`);
}

function writeCounts(counts) {
  const f = STATE.frame;
  if (!f) return;
  const pg = f.leftPane?.__ctoxPaneGrammar;
  if (pg?.setCounts) { pg.setCounts(counts); return; }
  for (const [band, span] of Object.entries(f.countEls || {})) {
    if (span) span.textContent = ` (${counts[band] ?? 0})`;
  }
}

function writeFooter(textValue) {
  const f = STATE.frame;
  if (!f) return;
  const pg = f.leftPane?.__ctoxPaneGrammar;
  if (pg?.setFooter) { pg.setFooter(textValue); return; }
  if (f.footerEl) f.footerEl.textContent = textValue || '';
}

function syncBanner() {
  const f = STATE.frame;
  if (!f?.banner) return;
  f.banner.hidden = !STATE.lastError;
  f.banner.textContent = STATE.lastError || '';
}

// Selection is an in-place class flip across the existing rows — never a list
// rebuild (a rebuild resets the operator's scroll).
function applyListSelection() {
  const listEl = STATE.frame?.listEl;
  if (!listEl?.querySelectorAll) return;
  for (const row of listEl.querySelectorAll('[data-invoice-id]')) {
    const on = (row.getAttribute('data-invoice-id') || '') === String(STATE.selectedInvoiceId || '');
    row.classList.toggle('is-selected', on);
    row.setAttribute('aria-selected', on ? 'true' : 'false');
  }
}

function selectRecord(id) {
  STATE.selectedInvoiceId = id || null;
  STATE.userCollapsed = false;
  STATE.lineDraft = null;
  applyListSelection();
  renderMain();
}

function onListClick(event) {
  const row = event.target?.closest?.('[data-invoice-id]');
  if (!row || !STATE.frame?.listEl?.contains?.(row)) return;
  selectRecord(row.getAttribute('data-invoice-id'));
}

function onListKey(event) {
  if (event.key !== 'Enter' && event.key !== ' ') return;
  const row = event.target?.closest?.('[data-invoice-id]');
  if (!row || !STATE.frame?.listEl?.contains?.(row)) return;
  event.preventDefault();
  selectRecord(row.getAttribute('data-invoice-id'));
}

function onAction(event) {
  const btn = event.target?.closest?.('[data-action]');
  if (!btn || !STATE.frame?.workspace?.contains?.(btn)) return;
  const action = btn.dataset.action;
  if (action === 'new') createDraft();
  else if (action === 'import') importRecords();
  else if (action === 'export') exportRecords();
  else if (action === 'collapse-detail') { STATE.userCollapsed = true; renderMain(); }
  else if (action === 'delete') { const inv = selectedInvoice(); if (inv) deleteDraft(inv); }
}

function onGrammarChange(event) {
  const detail = event?.detail || {};
  STATE.search = String(detail.search || '');
  STATE.view = detail.view || 'cards';
  STATE.band = detail.band || 'alle';
  STATE.filterType = detail.filters?.type || 'all';
  renderList();
}

// ---------------------------------------------------------------------------
// Main pane: detail / editor (auto-reveal)
// ---------------------------------------------------------------------------

function renderMain() {
  const pane = STATE.frame?.mainPane;
  if (!pane) return;
  const inv = selectedInvoice();
  const reveal = shouldRevealDetail(Boolean(inv), STATE.userCollapsed);

  const actions = createEl('div', 'ctox-pane-actions');
  if (inv && reveal) {
    const badge = createEl('span', ['ctox-badge', stateBadgeClass(inv.state)].filter(Boolean).join(' '));
    badge.dataset.state = inv.state;
    badge.textContent = invoiceStateLabel(inv.state);
    actions.appendChild(badge);
    if (inv.state === 'draft') actions.appendChild(paneIcon(ICON.trash, t('deleteDraft'), { action: 'delete', extraClass: 'is-danger' }));
    actions.appendChild(paneIcon(ICON.close, t('closeDetail'), { action: 'collapse-detail' }));
  }

  const kicker = inv && reveal ? (inv.state === 'draft' ? t('draft') : t('invoice')) : t('invoices');
  const title = inv && reveal
    ? `${inv.invoice_number || t('draft')} · ${partyName(inv.party_id)}`
    : t('invoice');
  const header = createEl('header', 'ctox-pane-header ctox-pane-band');
  add(header, buildTitleRow(kicker, title, actions, 'h1'));

  let body;
  if (!inv || !reveal) {
    body = createEl('div', 'ctox-pane-body');
    const empty = createEl('div', 'ctox-empty');
    empty.textContent = t('emptyHint');
    add(body, empty);
    delete pane.dataset.contextRecordId;
    delete pane.dataset.contextLabel;
  } else {
    body = createEl('div', 'ctox-pane-scroll invoices-pane-scroll');
    add(body, inv.state === 'draft' ? renderEditor(inv) : renderDetail(inv));
    pane.dataset.contextRecordId = inv.id;
    pane.dataset.contextLabel = inv.invoice_number || inv.id;
  }
  pane.replaceChildren(header, body);
}

function renderEditor(inv) {
  const wrap = createEl('section', 'invoices-editor invoices-stack');

  const meta = createEl('div', 'ctox-compact-form__fields');

  const partyLabel = createEl('label', 'ctox-compact-field');
  partyLabel.textContent = t('customer');
  const partySelect = createEl('select', 'ctox-select');
  const placeholder = createEl('option');
  placeholder.value = '';
  placeholder.textContent = t('chooseCustomer');
  if (!inv.party_id) placeholder.selected = true;
  partySelect.appendChild(placeholder);
  for (const p of Object.values(STATE.parties)) {
    const opt = createEl('option');
    opt.value = p.id;
    opt.textContent = p.name || p.id;
    if (p.id === inv.party_id) opt.selected = true;
    partySelect.appendChild(opt);
  }
  partySelect.addEventListener('change', () => { inv.party_id = partySelect.value; renderMain(); });
  partyLabel.appendChild(partySelect);
  meta.appendChild(partyLabel);

  const dateLabel = createEl('label', 'ctox-compact-field');
  dateLabel.textContent = t('invoiceDate');
  const dateInput = createEl('input', 'ctox-input');
  dateInput.type = 'date';
  dateInput.value = isoDateInput(inv.invoice_date_ms || Date.now());
  dateInput.addEventListener('change', () => {
    const ms = Date.parse(dateInput.value);
    if (Number.isFinite(ms)) {
      inv.invoice_date_ms = ms;
      inv.due_date_ms = computeDueDateMs(ms, inv.payment_terms?.net_days || 14);
    }
  });
  dateLabel.appendChild(dateInput);
  meta.appendChild(dateLabel);

  const typeLabelEl = createEl('label', 'ctox-compact-field');
  typeLabelEl.textContent = t('type');
  const typeSelect = createEl('select', 'ctox-select');
  for (const type of INVOICE_TYPES) {
    const opt = createEl('option');
    opt.value = type;
    opt.textContent = typeLabel(type);
    if (type === inv.invoice_type) opt.selected = true;
    typeSelect.appendChild(opt);
  }
  typeSelect.addEventListener('change', () => { inv.invoice_type = typeSelect.value; });
  typeLabelEl.appendChild(typeSelect);
  meta.appendChild(typeLabelEl);

  wrap.appendChild(meta);

  const linesHeader = createEl('span', 'ctox-field-label');
  linesHeader.textContent = t('lines');
  wrap.appendChild(linesHeader);

  const tableWrap = createEl('div', 'ctox-table-wrap');
  const linesTable = createEl('table', 'ctox-table invoices-lines-table');
  linesTable.appendChild(renderLineHeader());
  const linesBody = createEl('tbody');
  for (const line of inv.lines || []) linesBody.appendChild(renderLineRow(inv, line));
  linesTable.appendChild(linesBody);
  tableWrap.appendChild(linesTable);
  wrap.appendChild(tableWrap);

  const addLineBtn = createEl('button', 'ctox-button');
  addLineBtn.type = 'button';
  addLineBtn.textContent = t('addLine');
  addLineBtn.addEventListener('click', () => {
    inv.lines = inv.lines || [];
    inv.lines.push({
      id: `line_${Date.now().toString(36)}`,
      position: (inv.lines.length || 0) + 1,
      description: '',
      quantity: 1000,
      unit: 'Stk',
      unit_price_cents: 0,
      tax_rate: 0.19,
      account_code: inv.invoice_type === 'sale_in' ? '3400' : '8400',
    });
    renderMain();
  });
  wrap.appendChild(addLineBtn);

  wrap.appendChild(renderTotals(inv));

  const actions = createEl('div', 'invoices-actions');

  const saveBtn = createEl('button', 'ctox-button');
  saveBtn.type = 'button';
  saveBtn.textContent = t('saveDraft');
  saveBtn.disabled = STATE.busy;
  saveBtn.addEventListener('click', () => updateDraft(inv));
  actions.appendChild(saveBtn);

  // The one big text button is the composer's essential action (post).
  const postBtn = createEl('button', 'ctox-button is-primary');
  postBtn.type = 'button';
  postBtn.textContent = t('post');
  const issues = computeValidationIssues(inv);
  const postDisabled = STATE.busy || !issues.canPost;
  postBtn.disabled = postDisabled;
  postBtn.title = postDisabled && !STATE.busy
    ? `${t('missingBeforePost')}: ${issues.errors.map((i) => i.field).join(', ') || t('unknown')}`
    : '';
  postBtn.addEventListener('click', () => postInvoice(inv));
  actions.appendChild(postBtn);

  if (issues.errors.length > 0) {
    const issuesBox = createEl('div', 'ctox-callout is-danger');
    const issuesList = createEl('ul', 'invoices-issues');
    for (const issue of issues.errors) {
      const li = createEl('li');
      li.textContent = issue.message;
      issuesList.appendChild(li);
    }
    issuesBox.appendChild(issuesList);
    actions.appendChild(issuesBox);
  }

  wrap.appendChild(actions);
  return wrap;
}

function renderTotals(inv) {
  const totals = computeInvoiceTotals(inv);
  const totalsDiv = createEl('div', 'invoices-totals');
  totalsDiv.innerHTML = `
    <span>${escapeHtml(t('net'))}: <strong>${formatCents(totals.subtotal_cents)}</strong></span>
    <span>${escapeHtml(t('tax'))}: <strong>${formatCents(totals.tax_cents)}</strong></span>
    <span>${escapeHtml(t('gross'))}: <strong>${formatCents(totals.total_cents)}</strong></span>
  `;
  return totalsDiv;
}

function renderLineHeader() {
  const thead = createEl('thead');
  const row = createEl('tr');
  for (const column of LINE_COLUMNS) {
    const th = createEl('th');
    if (column.num) th.className = 'is-num';
    th.textContent = column.labelKey ? t(column.labelKey) : (column.label || '');
    row.appendChild(th);
  }
  thead.appendChild(row);
  return thead;
}

function renderLineRow(inv, line) {
  const tr = createEl('tr');
  const cells = [
    { type: 'text', value: line.position ?? '', set: (v) => (line.position = Number(v) || line.position) },
    { type: 'text', value: line.description || '', set: (v) => (line.description = v) },
    { type: 'number', value: line.quantity ?? '', set: (v) => (line.quantity = Math.round(Number(v) || 0)) },
    { type: 'text', value: line.unit || 'Stk', set: (v) => (line.unit = v) },
    { type: 'number', value: line.unit_price_cents ?? '', set: (v) => (line.unit_price_cents = Math.round(Number(v) || 0)) },
    { type: 'number', value: ((line.tax_rate || 0) * 100).toFixed(0), set: (v) => (line.tax_rate = Math.max(0, Math.min(1, Number(v) / 100))) },
    { type: 'text', value: line.account_code || '', set: (v) => (line.account_code = v) },
  ];
  for (const [index, c] of cells.entries()) {
    const td = createEl('td');
    if (LINE_COLUMNS[index]?.num) td.className = 'is-num';
    const input = createEl('input', 'ctox-input');
    input.type = c.type;
    input.value = c.value;
    input.addEventListener('change', () => {
      c.set(input.value);
      // Recompute totals in place to keep the operator in flow.
      const totals = computeInvoiceTotals(inv);
      const totalsEl = document.querySelector?.('.invoices-totals');
      if (totalsEl) {
        totalsEl.innerHTML = `
          <span>${escapeHtml(t('net'))}: <strong>${formatCents(totals.subtotal_cents)}</strong></span>
          <span>${escapeHtml(t('tax'))}: <strong>${formatCents(totals.tax_cents)}</strong></span>
          <span>${escapeHtml(t('gross'))}: <strong>${formatCents(totals.total_cents)}</strong></span>
        `;
      }
    });
    td.appendChild(input);
    tr.appendChild(td);
  }
  const removeTd = createEl('td');
  const removeBtn = createEl('button', 'ctox-icon-button');
  removeBtn.type = 'button';
  removeBtn.innerHTML = ICON.close;
  removeBtn.setAttribute('aria-label', t('removeLine'));
  removeBtn.title = t('removeLine');
  removeBtn.addEventListener('click', () => {
    inv.lines = (inv.lines || []).filter((l) => l.id !== line.id);
    renderMain();
  });
  removeTd.appendChild(removeBtn);
  tr.appendChild(removeTd);
  return tr;
}

function renderDetail(inv) {
  const wrap = createEl('section', 'invoices-detail invoices-stack');
  const party = STATE.parties[inv.party_id] || {};

  const summary = createEl('dl', 'ctox-fields');
  const rows = [
    [t('invoiceNumber'), escapeHtml(inv.invoice_number || '—')],
    [t('customer'), escapeHtml(partyName(inv.party_id))],
    [t('date'), escapeHtml(isoDateInput(inv.invoice_date_ms))],
    [t('due'), escapeHtml(isoDateInput(inv.due_date_ms))],
    [t('net'), formatCents(inv.subtotal_cents)],
    [t('tax'), formatCents(inv.tax_cents)],
    [t('gross'), formatCents(inv.total_cents)],
    [t('paid'), formatCents(inv.paid_cents)],
    [t('open'), formatCents(inv.open_cents)],
  ];
  // Fold the customer snapshot (formerly a third-pane inspector) into the detail.
  if (party.address) rows.push([t('address'), escapeHtml(party.address)]);
  if (party.email) rows.push([t('email'), escapeHtml(party.email)]);
  summary.innerHTML = rows.map(([dt, dd]) => `<dt>${escapeHtml(dt)}</dt><dd>${dd}</dd>`).join('');
  wrap.appendChild(summary);

  const linesHeader = createEl('span', 'ctox-field-label');
  linesHeader.textContent = t('lines');
  wrap.appendChild(linesHeader);
  const tableWrap = createEl('div', 'ctox-table-wrap');
  const linesTable = createEl('table', 'ctox-table');
  linesTable.appendChild(renderLineHeader());
  const linesBody = createEl('tbody');
  for (const line of inv.lines || []) {
    const tr = createEl('tr');
    const values = [
      line.position,
      line.description,
      formatMilli(line.quantity),
      line.unit,
      formatCents(line.unit_price_cents),
      `${((line.tax_rate || 0) * 100).toFixed(0)}%`,
      line.account_code,
    ];
    for (const [index, value] of values.entries()) {
      const td = createEl('td');
      if (LINE_COLUMNS[index]?.num) td.className = 'is-num';
      td.textContent = String(value ?? '');
      tr.appendChild(td);
    }
    tr.appendChild(createEl('td'));
    linesBody.appendChild(tr);
  }
  linesTable.appendChild(linesBody);
  tableWrap.appendChild(linesTable);
  wrap.appendChild(tableWrap);

  const tabs = createEl('div', 'ctox-pane-tabs');
  const tabButtons = [
    { id: 'journal', label: t('journal') },
    { id: 'xrechnung', label: 'XRechnung' },
    { id: 'payments', label: t('payments') },
    { id: 'dunning', label: t('dunning') },
  ];
  for (const tab of tabButtons) {
    const btn = createEl('button', 'ctox-pane-tab');
    btn.type = 'button';
    btn.dataset.tab = tab.id;
    btn.textContent = tab.label;
    const active = STATE.lineDraft === tab.id;
    btn.setAttribute('aria-selected', active ? 'true' : 'false');
    if (active) btn.classList.add('is-active');
    btn.addEventListener('click', () => {
      STATE.lineDraft = STATE.lineDraft === tab.id ? null : tab.id;
      renderMain();
    });
    tabs.appendChild(btn);
  }
  wrap.appendChild(tabs);

  if (STATE.lineDraft === 'journal') wrap.appendChild(renderJournalTab(inv));
  else if (STATE.lineDraft === 'xrechnung') wrap.appendChild(renderXRechnungTab(inv));
  else if (STATE.lineDraft === 'payments') wrap.appendChild(renderPaymentsTab(inv));
  else if (STATE.lineDraft === 'dunning') wrap.appendChild(renderDunningTab(inv));
  return wrap;
}

function renderJournalTab(inv) {
  const wrap = createEl('div', 'invoices-tab');
  if (!inv.post_journal_entry_id) {
    wrap.textContent = t('noJournal');
    return wrap;
  }
  const lines = (inv.lines || []).map((line) => {
    const net = computeLineNetCents(line);
    const tax = Math.round(net * (Number(line.tax_rate) || 0));
    return `
      <tr>
        <td>${escapeHtml(line.account_code || '8400')}</td>
        <td>${escapeHtml(line.description || '')}</td>
        <td class="is-num">${formatCents(net)}</td>
        <td class="is-num">—</td>
        <td class="is-num">${formatCents(tax)}</td>
      </tr>
    `;
  }).join('');
  wrap.innerHTML = `
    <span class="ctox-field-label">Journal ${escapeHtml(inv.post_journal_entry_id)}</span>
    <div class="ctox-table-wrap">
      <table class="ctox-table">
        <thead><tr><th>${escapeHtml(t('account'))}</th><th>${escapeHtml(t('description'))}</th><th class="is-num">${escapeHtml(t('debit'))}</th><th class="is-num">${escapeHtml(t('credit'))}</th><th class="is-num">${escapeHtml(t('tax'))}</th></tr></thead>
        <tbody>${lines}</tbody>
      </table>
    </div>
  `;
  return wrap;
}

function renderXRechnungTab(inv) {
  const wrap = createEl('div', 'invoices-tab');
  try {
    const xml = buildXRechnungXml(inv, STATE.parties[inv.party_id] || {}, { name: 'CTOX' });
    const pre = createEl('pre', 'ctox-pre');
    pre.textContent = xml;
    wrap.appendChild(pre);
    const download = createEl('button', 'ctox-button');
    download.type = 'button';
    download.textContent = t('downloadXml');
    download.addEventListener('click', () => {
      const blob = new Blob([xml], { type: 'application/xml' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `${inv.invoice_number || inv.id}.xml`;
      a.click();
      URL.revokeObjectURL(url);
    });
    wrap.appendChild(download);
  } catch (err) {
    wrap.textContent = `${t('xmlFailed')}: ${err?.message || err}`;
  }
  return wrap;
}

function renderPaymentsTab(inv) {
  const wrap = createEl('div', 'invoices-tab');
  const openCents = inv.open_cents ?? Math.max(0, (inv.total_cents || 0) - (inv.paid_cents || 0));
  wrap.innerHTML = `
    <p>${escapeHtml(t('open'))}: <strong>${formatCents(openCents)}</strong></p>
    <form class="invoices-payment-form">
      <label class="ctox-compact-field">${escapeHtml(t('amountCents'))}<input class="ctox-input" type="number" name="amount_cents" value="${openCents}" min="0" required /></label>
      <label class="ctox-compact-field">${escapeHtml(t('discountCents'))}<input class="ctox-input" type="number" name="skonto_cents" value="0" min="0" /></label>
      <label class="ctox-compact-field">${escapeHtml(t('paymentId'))}<input class="ctox-input" type="text" name="payment_id" placeholder="pay_…" required /></label>
      <button class="ctox-button is-primary" type="submit" ${STATE.busy ? 'disabled' : ''}>${escapeHtml(t('allocate'))}</button>
    </form>
    <p class="invoices-hint">${escapeHtml(t('discountHint'))}</p>
  `;
  const form = wrap.querySelector('form');
  form.addEventListener('submit', async (event) => {
    event.preventDefault();
    const data = new FormData(form);
    await submitCommand({
      module: 'invoices',
      command_type: 'invoices.payment.allocate',
      record_id: inv.id,
      payload: {
        invoice_id: inv.id,
        payment_id: String(data.get('payment_id') || '').trim(),
        amount_cents: Math.round(Number(data.get('amount_cents')) || 0),
        skonto_cents: Math.round(Number(data.get('skonto_cents')) || 0),
      },
      client_context: { surface: 'invoices.payment.allocate' },
    });
    await refresh();
    render();
  });
  return wrap;
}

function renderDunningTab(inv) {
  const wrap = createEl('div', 'invoices-tab');
  if (inv.state !== 'overdue') {
    wrap.textContent = t('dunningOnlyOverdue');
    return wrap;
  }
  wrap.innerHTML = `
    <p>${escapeHtml(t('dunningHint'))}</p>
    <button type="button" class="ctox-button is-primary" ${STATE.busy ? 'disabled' : ''}>${escapeHtml(t('dunningRun'))}</button>
  `;
  const btn = wrap.querySelector('button');
  btn.addEventListener('click', async () => {
    const runId = `dunning_${Date.now().toString(36)}`;
    await submitCommand({
      module: 'invoices',
      command_type: 'invoices.dunning.run',
      record_id: runId,
      payload: { run_id: runId, filter: { invoice_id: inv.id } },
      client_context: { surface: 'invoices.dunning.run' },
    });
    await refresh();
    render();
  });
  return wrap;
}

// ---------------------------------------------------------------------------
// Commands (flows + payloads unchanged from the prior IA)
// ---------------------------------------------------------------------------

async function createDraft() {
  const partyId = Object.keys(STATE.parties)[0] || '';
  if (!partyId) {
    STATE.lastError = t('noCustomer');
    render();
    return;
  }
  const invoiceId = `inv_${Date.now().toString(36)}`;
  await submitCommand(
    buildCreateInvoiceCommand(invoiceId, {
      invoice_type: 'sale_out',
      party_id: partyId,
      invoice_date_ms: Date.now(),
      due_date_ms: computeDueDateMs(Date.now(), 14),
      currency: 'EUR',
      lines: [],
    }),
  );
  STATE.selectedInvoiceId = invoiceId;
  STATE.userCollapsed = false;
  await refresh();
  render();
}

async function updateDraft(inv) {
  const totals = computeInvoiceTotals(inv);
  await submitCommand(
    buildUpdateInvoiceCommand(inv.id, {
      invoice_type: inv.invoice_type,
      party_id: inv.party_id,
      invoice_date_ms: inv.invoice_date_ms,
      due_date_ms: inv.due_date_ms,
      currency: inv.currency || 'EUR',
      lines: inv.lines || [],
      subtotal_cents: totals.subtotal_cents,
      tax_cents: totals.tax_cents,
      total_cents: totals.total_cents,
    }),
  );
  await refresh();
  render();
}

async function deleteDraft(inv) {
  if (typeof confirm === 'function' && !confirm(t('deleteConfirm', { id: inv.invoice_number || inv.id }))) return;
  await submitCommand(buildDeleteInvoiceCommand(inv.id));
  STATE.selectedInvoiceId = null;
  await refresh();
  render();
}

async function postInvoice(inv) {
  // The native validator must see the same draft the operator confirmed —
  // including unsaved edits — so we send the full patch before posting.
  const totals = computeInvoiceTotals(inv);
  const issues = computeValidationIssues(inv);
  if (!issues.canPost) {
    STATE.lastError = `${t('cannotPost')}: ${issues.errors.map((i) => i.message).join('; ')}`;
    render();
    return;
  }
  await submitCommand(
    buildUpdateInvoiceCommand(inv.id, {
      invoice_type: inv.invoice_type,
      party_id: inv.party_id,
      invoice_date_ms: inv.invoice_date_ms,
      due_date_ms: inv.due_date_ms,
      currency: inv.currency || 'EUR',
      lines: inv.lines || [],
      subtotal_cents: totals.subtotal_cents,
      tax_cents: totals.tax_cents,
      total_cents: totals.total_cents,
      tax_breakdown: totals.tax_breakdown || [],
    }),
  );
  await submitCommand({
    module: 'invoices',
    command_type: 'invoices.invoice.post',
    record_id: inv.id,
    payload: { invoice_id: inv.id },
    client_context: { surface: 'invoices.invoice.post' },
  });
  await refresh();
  render();
}

function computeValidationIssues(inv) {
  // Pure JS mirror of src/core/business_os/invoices.rs::validate_invoice_for_post
  // so a draft the UI accepts cannot be rejected by the native handler.
  const issues = validateInvoice(inv || {});
  return {
    errors: issues.filter((i) => (i.severity || 'error') === 'error'),
    warnings: issues.filter((i) => i.severity === 'warning'),
    canPost: issues.every((i) => (i.severity || 'error') !== 'error')
      && Boolean(inv?.party_id)
      && Array.isArray(inv?.lines) && inv.lines.length > 0,
  };
}

// ---------------------------------------------------------------------------
// Import / Export (Export = visible invoices as JSON; Import = existing write path)
// ---------------------------------------------------------------------------

function exportRecords() {
  const payload = buildInvoicesExport(visibleInvoices(), Date.now());
  let url = '';
  try {
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
    url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'invoices.json';
    a.rel = 'noopener';
    (STATE.frame?.workspace || document.body)?.appendChild?.(a);
    a.click();
    a.remove?.();
  } catch (error) {
    console.error('[invoices] export failed:', error);
  } finally {
    if (url) setTimeout(() => { try { URL.revokeObjectURL(url); } catch {} }, 4000);
  }
}

function importRecords() {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = 'application/json,.json';
  input.addEventListener('change', async () => {
    const file = input.files && input.files[0];
    if (!file) return;
    let parsed;
    try { parsed = JSON.parse(await file.text()); } catch { STATE.lastError = t('importInvalid'); render(); return; }
    const entries = parseInvoiceImport(parsed, Date.now());
    if (!entries.length) { STATE.lastError = t('importEmpty'); render(); return; }
    let count = 0;
    const stamp = Date.now().toString(36);
    for (const [index, entry] of entries.entries()) {
      const invoiceId = `inv_imp_${stamp}_${index}`;
      try {
        await submitCommand(buildCreateInvoiceCommand(invoiceId, entry));
        count += 1;
      } catch (error) {
        console.error('[invoices] import create failed:', error);
      }
    }
    STATE.lastError = null;
    STATE.ctx?.notifications?.show?.({ type: 'success', title: t('invoices'), message: t('imported', { count }) });
    await refresh();
    render();
  });
  input.click();
}

// ---------------------------------------------------------------------------
// Blocking / error states
// ---------------------------------------------------------------------------

function renderDependencyBlocker() {
  const root = moduleRoot();
  if (!root) return;
  root.innerHTML = '';
  STATE.frame = null;
  const card = createEl('div', 'ctox-empty');
  const title = createEl('strong');
  title.textContent = t('dependencyTitle');
  card.appendChild(title);
  const list = createEl('ul');
  for (const id of REQUIRED_MODULES) {
    const item = createEl('li');
    item.textContent = id;
    list.appendChild(item);
  }
  card.appendChild(list);
  const note = createEl('p');
  note.textContent = t('dependencyNote');
  card.appendChild(note);
  const retry = createEl('button', 'ctox-button');
  retry.type = 'button';
  retry.textContent = t('reload');
  retry.addEventListener('click', () => {
    if (isReady()) refresh().then(render).catch(reportError);
    else renderDependencyBlocker();
  });
  card.appendChild(retry);
  root.appendChild(card);
}

function renderError(message) {
  const root = moduleRoot();
  if (!root) return;
  root.innerHTML = '';
  STATE.frame = null;
  const div = createEl('div', 'ctox-empty');
  div.textContent = message;
  root.appendChild(div);
}

// ---------------------------------------------------------------------------
// Debug handle + markup bootstrap
// ---------------------------------------------------------------------------

function invoicesDebugSnapshot() {
  return {
    mounted: Boolean(STATE.ctx),
    invoice_count: Array.isArray(STATE.invoices) ? STATE.invoices.length : 0,
    selected_invoice_id: STATE.selectedInvoiceId || '',
    band: STATE.band || 'alle',
    view: STATE.view || 'cards',
    busy: Boolean(STATE.busy),
    last_error: STATE.lastError || '',
    watched_collections: [...WATCHED_COLLECTIONS],
  };
}

if (typeof window !== 'undefined') {
  window.__ctoxInvoicesModule = Object.freeze({
    mount,
    inspect: invoicesDebugSnapshot,
  });
}

function ensureModuleStyles() {
  if (typeof document === 'undefined' || String(import.meta.url).startsWith('data:') || (!document.head?.appendChild && !document.head?.append)) return;
  const cssVersion = String(import.meta.url).split('?v=')[1] || '';
  const cssHref = new URL('./index.css', import.meta.url).pathname + (cssVersion ? `?v=${cssVersion}` : '');
  let link = document.querySelector?.('link[data-invoices-style]');
  if (!link) {
    link = document.createElement('link');
    link.rel = 'stylesheet';
    link.dataset.invoicesStyle = 'true';
    if (document.head.appendChild) document.head.appendChild(link);
    else document.head.append(link);
  }
  if (link.getAttribute?.('href') !== cssHref) link.href = cssHref;
}

async function ensureMountedMarkup(ctx) {
  if (!ctx?.host?.querySelector) return moduleRoot();
  if (ctx.host.querySelector('#invoices-root')) return moduleRoot();
  try {
    const markupVersion = String(import.meta.url).split('?v=')[1] || '';
    const markupHref = new URL('./index.html', import.meta.url).pathname + (markupVersion ? `?v=${markupVersion}` : '');
    const html = await fetch(markupHref).then((res) => {
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      return res.text();
    });
    if (typeof DOMParser !== 'undefined') {
      const doc = new DOMParser().parseFromString(html, 'text/html');
      ctx.host.innerHTML = doc.body.innerHTML;
    } else {
      ctx.host.innerHTML = '<div id="invoices-root" class="invoices-shell"></div>';
    }
  } catch (error) {
    console.warn('[invoices] markup load failed; falling back to inline root', error);
    ctx.host.innerHTML = '<div id="invoices-root" class="invoices-shell"></div>';
  }
  return moduleRoot();
}

function moduleRoot() {
  return STATE.ctx?.host?.querySelector?.('#invoices-root')
    || document.getElementById('invoices-root');
}
