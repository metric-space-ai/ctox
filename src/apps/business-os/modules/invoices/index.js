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
// App-API 4.0 (Shell v4, Protokoll V4-PORT.md):
//   - mount(root, api, {restored}) → {unmount, snapshot}. Every api.query
//     subscription and DOM listener opened during the mount is released by unmount().
//   - Reads: one api.query(...).subscribe per watched collection, coalesced into
//     one render via scheduleRefresh. Mutations: api.command(type, payload,
//     {record_id}) — never awaited; the handlers in handlers/ own GoBD-immutability.
//     Failures go to the shell's status zone (api.status.report), success is silent.
//   - The column grammar (data-pg-* markup) is wired by the app itself via
//     autoWirePaneGrammar(root); the module re-renders on the bubbling
//     `ctox-pane-grammar-change` event and writes counts/footer via
//     `pane.__ctoxPaneGrammar` (null-guarded). The programmatic frame builders stay
//     the fallback for header-less node test shims.

import {
  buildCreateInvoiceCommand,
  buildUpdateInvoiceCommand,
  buildDeleteInvoiceCommand,
  buildXRechnungXml,
} from './commands/builders.js';
import { validateInvoice } from './core/invoice-validate.js';
import { renderHtmlIfChanged } from 'ctox:shared/stable-dom.js';
import { autoWirePaneGrammar } from 'ctox:shared/pane-grammar.js';

const BUILD = '20260721-invoices-ia-two-pane';
const MODULE_ID = 'invoices';
const SKILL_TAG = 'product_engineering/business-os-app-module-development';

const COPY = {
  de: {
    invoices: 'Rechnungen', kicker: '', all: 'Alle', overdue: 'Überfällig', open: 'Offen', paid: 'Bezahlt',
    newInvoice: 'Neue Rechnung', import: 'Importieren', export: 'Exportieren', closeDetail: 'Details schließen',
    unknown: 'unbekannt', unknownCustomer: 'Unbekannt', more: 'Weitere Aktionen', emptyList: 'Noch keine Rechnungen.', newShort: 'NEU', entries: 'Einträge', search: 'Suchen...', view: 'Darstellung',
    cardsView: 'Kachelansicht', listView: 'Listenansicht', filter: 'Filter', allTypes: 'Alle Typen', filterType: 'Typ',
    resetFilters: 'Filter zurücksetzen', noMatches: 'Keine Treffer', cancelNew: 'Abbrechen',
    invoice: 'Rechnung', draft: 'Entwurf',
    customer: 'Kunde', chooseCustomer: '— bitte Kunde wählen —', invoiceDate: 'Rechnungsdatum', type: 'Typ',
    lines: 'Positionen', addLine: '+ Position', net: 'Netto', tax: 'USt', gross: 'Brutto',
    saveDraft: 'Entwurf speichern', deleteDraft: 'Entwurf löschen', post: 'Buchen', missingBeforePost: 'Vor dem Buchen fehlt',
    removeLine: 'Position entfernen', invoiceNumber: 'Rechnungsnummer', date: 'Datum', due: 'Fällig', journal: 'Journal',
    payments: 'Zahlungen', dunning: 'Mahnen', noJournal: 'Kein Journal-Eintrag verknüpft.', account: 'Konto', description: 'Beschreibung', debit: 'Soll', credit: 'Haben',
    downloadXml: 'XRechnung-XML herunterladen', xmlFailed: 'XRechnung-Vorschau fehlgeschlagen', amountCents: 'Betrag (€)', discountCents: 'Skonto (€)',
    paymentId: 'Zahlungs-ID', allocate: 'Zuordnen', discountHint: 'Skonto wird nur abgezogen, wenn vor Ablauf der Skontofrist gezahlt wird.',
    dunningOnlyOverdue: 'Mahnen ist nur bei überfälligen Rechnungen möglich.',
    dunningRun: 'Mahnlauf für diese Rechnung', address: 'Adresse', email: 'E-Mail', noAddress: 'Keine Adresse hinterlegt.',
    noCustomer: 'Zuerst einen Kunden in „Kunden“ anlegen.',
    deleteConfirm: 'Entwurf {id} löschen?', cannotPost: 'Rechnung kann nicht gebucht werden', resizeColumn: 'Spaltenbreite anpassen',
    stateDraft: 'Entwurf', statePosted: 'Gebucht', statePartiallyPaid: 'Teilweise bezahlt', statePaid: 'Bezahlt', stateOverdue: 'Überfällig', stateCancelled: 'Storniert', stateCredited: 'Gutgeschrieben',
    pos: 'Pos', quantity: 'Menge', unit: 'Einheit', unitPrice: 'Einzelpreis (€)', lineAmount: 'Betrag',
    importInvalid: 'Ungültige JSON-Datei.', importEmpty: 'Keine gültigen Rechnungen (Kunde + Positionen) in der Datei.',
  },
  en: {
    invoices: 'Invoices', kicker: '', all: 'All', overdue: 'Overdue', open: 'Open', paid: 'Paid', // v4-allow: en
    newInvoice: 'New invoice', import: 'Import', export: 'Export', closeDetail: 'Close details',
    unknown: 'unknown', unknownCustomer: 'Unknown', more: 'More actions', emptyList: 'No invoices yet.', newShort: 'NEW', entries: 'entries', search: 'Search...', view: 'View', // v4-allow: en
    cardsView: 'Card view', listView: 'List view', filter: 'Filter', allTypes: 'All types', filterType: 'Type',
    resetFilters: 'Reset filters', noMatches: 'No matches', cancelNew: 'Cancel', // v4-allow: en
    invoice: 'Invoice', draft: 'Draft',
    customer: 'Customer', chooseCustomer: '— select customer —', invoiceDate: 'Invoice date', type: 'Type',
    lines: 'Line items', addLine: '+ Line item', net: 'Net', tax: 'VAT', gross: 'Gross',
    saveDraft: 'Save draft', deleteDraft: 'Delete draft', post: 'Post', missingBeforePost: 'Required before posting',
    removeLine: 'Remove line item', invoiceNumber: 'Invoice number', date: 'Date', due: 'Due', journal: 'Journal',
    payments: 'Payments', dunning: 'Dunning', noJournal: 'No journal entry linked.', account: 'Account', description: 'Description', debit: 'Debit', credit: 'Credit',
    downloadXml: 'Download XRechnung XML', xmlFailed: 'XRechnung preview failed', amountCents: 'Amount (€)', discountCents: 'Discount (€)',
    paymentId: 'Payment ID', allocate: 'Allocate', discountHint: 'Discount applies only when paid before the discount deadline.',
    dunningOnlyOverdue: 'Dunning is available only for overdue invoices.',
    dunningRun: 'Run dunning for this invoice', address: 'Address', email: 'Email', noAddress: 'No address recorded.',
    noCustomer: 'Create a customer in “Customers” first.',
    deleteConfirm: 'Delete draft {id}?', cannotPost: 'Invoice cannot be posted', resizeColumn: 'Adjust column width',
    stateDraft: 'Draft', statePosted: 'Posted', statePartiallyPaid: 'Partially paid', statePaid: 'Paid', stateOverdue: 'Overdue', stateCancelled: 'Cancelled', stateCredited: 'Credited',
    pos: 'Pos', quantity: 'Quantity', unit: 'Unit', unitPrice: 'Unit price (€)', lineAmount: 'Amount',
    importInvalid: 'Invalid JSON file.', importEmpty: 'No valid invoices (customer + line items) in the file.',
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
// col = Spaltenschlüssel (data-col) für die schmale Kartenansicht der Positionen (FIX10, index.css).
const LINE_COLUMNS = Object.freeze([
  { labelKey: 'pos', col: 'pos' },
  { labelKey: 'description', col: 'desc' },
  { labelKey: 'quantity', num: true, col: 'qty' },
  { labelKey: 'unit', col: 'unit' },
  { labelKey: 'unitPrice', num: true, col: 'price' },
  { label: 'USt %', num: true, col: 'tax' },
  { label: 'SKR', col: 'account' },
  { label: '', col: 'action' },
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
  more: '<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><circle cx="5" cy="12" r="1.7"/><circle cx="12" cy="12" r="1.7"/><circle cx="19" cy="12" r="1.7"/></svg>',
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

const STATE_LABEL_KEYS = { draft: 'stateDraft', posted: 'statePosted', partially_paid: 'statePartiallyPaid', paid: 'statePaid', overdue: 'stateOverdue', cancelled: 'stateCancelled', credited: 'stateCredited' };

const SNAPSHOT_VERSION = 1;

const STATE = {
  api: null,
  root: null,
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
  // Open draft edit (clone of the selected draft invoice): survives live refreshes and remounts.
  edit: null,
  pendingRestore: null,
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

// v4 (PORTING.md R11): an empty list renders nothing — no empty/syncing copy.
export function renderInvoiceListMarkup(rows, opts = {}) {
  const selectedId = opts.selectedId ?? null;
  return (Array.isArray(rows) ? rows : [])
    .map((inv) => renderInvoiceRow(inv, { view: opts.view, selected: inv.id === selectedId }))
    .join('');
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

// api.shell.t('invoices.<key>') overrides when it returns more than the key itself (key space open,
// app-api.d.ts OFFEN-B); the embedded COPY is the fallback.
function t(key, replacements = {}) {
  const scoped = `invoices.${key}`;
  const shellValue = typeof STATE.api?.shell?.t === 'function' ? STATE.api.shell.t(scoped) : scoped;
  let value = shellValue && shellValue !== scoped ? shellValue : (COPY[STATE.locale]?.[key] ?? COPY.de[key] ?? key);
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

// Kundenname aus customer_accounts; ohne Treffer „Unbekannt“ – nie die technische ID (Textdiät Regel 4).
function partyName(partyId) {
  const p = STATE.parties[partyId];
  const name = p && (p.name || p.payload?.name || p.payload?.legal_name);
  return name ? String(name) : t('unknownCustomer');
}

// Beträge deutsch: „119,00 €“. Gespeichert bleibt in Cent.
function formatCents(cents) {
  if (!Number.isFinite(cents)) return '–';
  return (cents / 100).toLocaleString('de-DE', { style: 'currency', currency: 'EUR' });
}

// Eingabe in Euro/Stück (deutsch oder mit Punkt), intern Cent bzw. Tausendstel.
export function parseDecimalDe(value) {
  let raw = String(value ?? '').trim().replace(/\s|€/g, '');
  if (!raw) return 0;
  if (raw.includes(',')) raw = raw.replace(/\./g, '').replace(',', '.');
  const n = Number(raw);
  return Number.isFinite(n) ? n : 0;
}
export const euroToCents = (value) => Math.round(parseDecimalDe(value) * 100);
export const unitsToMilli = (value) => Math.round(parseDecimalDe(value) * 1000);
function formatDecimalDe(n, minDigits, maxDigits) {
  return Number(n).toLocaleString('de-DE', { minimumFractionDigits: minDigits, maximumFractionDigits: maxDigits, useGrouping: false });
}
const centsToEuroInput = (cents) => (Number.isFinite(Number(cents)) ? formatDecimalDe(Number(cents) / 100, 2, 2) : '');
const milliToUnitsInput = (milli) => (Number.isFinite(Number(milli)) ? formatDecimalDe(Number(milli) / 1000, 0, 3) : '');

// Adresse als eine Zeile („Straße, PLZ Ort, Land“); Objekte aus customer_accounts nie als „[object Object]“.
function formatAddress(a) {
  if (!a) return '';
  if (typeof a === 'string') return a;
  if (typeof a !== 'object') return String(a);
  const street = a.street || a.line1 || a.address_line1 || '';
  const city = [a.postal_code || a.zip || a.postcode, a.city || a.locality].filter(Boolean).join(' ');
  return [street, a.line2 || a.address_line2, city, a.country].filter(Boolean).join(', ');
}

function formatDateDe(ms) {
  if (!Number.isFinite(ms)) return '';
  return new Date(ms).toLocaleDateString('de-DE', { day: '2-digit', month: '2-digit', year: 'numeric', timeZone: 'UTC' });
}

// Menge in Stück, deutsch („1“, „1,5“) – nicht „1.000“, das im Deutschen tausend heißt.
function formatMilli(quantity) {
  if (!Number.isFinite(quantity)) return '–';
  return (quantity / 1000).toLocaleString('de-DE', { maximumFractionDigits: 3 });
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

export async function mount(root, api, { restored } = {}) {
  resetState(root, api);
  // Subscriptions + listeners belong to THIS mount (PORTING.md §4 Falle 1).
  const cleanup = STATE.cleanup;
  if (root?.dataset && !root.dataset.app) root.dataset.app = api.app?.id || MODULE_ID;
  await ensureMountedMarkup(root);
  api.shell?.setTitle?.(t('invoices'));
  STATE.pendingRestore = restored && typeof restored === 'object' && restored.v === SNAPSHOT_VERSION ? restored : null;
  // Frame FIRST, data second: the static frame stays on screen in every data state.
  render();
  applyRestoredView();
  autoWirePaneGrammar(moduleRoot());
  for (const name of WATCHED_COLLECTIONS) {
    let off = null;
    try {
      off = api.query(name, {}).subscribe((rows) => {
        STATE.data[name] = (Array.isArray(rows) ? rows : []).filter((doc) => doc && doc.is_deleted !== true);
        if (name === 'accounting_invoices') STATE.invoicesLoaded = true;
        scheduleRefresh();
      });
    } catch (error) {
      // A collection the core does not grant this app is a data condition, not a mount failure.
      api.status?.report?.({ level: 'error', code: `invoices.read.${name}`, text: `${name}: ${error?.code || ''} ${error?.message || error}`.trim() });
    }
    if (typeof off === 'function') cleanup.push(off);
  }
  if (api.shell?.locale?.subscribe) {
    cleanup.push(api.shell.locale.subscribe((locale) => {
      const next = langFrom(locale);
      if (next === STATE.locale) return;
      STATE.locale = next;
      api.shell?.setTitle?.(t('invoices'));
      relabelStaticFrame();
      render();
    }));
  }
  return {
    unmount() {
      for (const fn of cleanup.splice(0)) {
        try { fn?.(); } catch {}
      }
      if (STATE.renderTimer) window.clearTimeout(STATE.renderTimer);
      STATE.renderTimer = 0;
      if (STATE.root === root) {
        root.replaceChildren?.();
        if (root.replaceChildren === undefined) root.innerHTML = '';
        STATE.api = null;
        STATE.root = null;
        STATE.frame = null;
      }
    },
    snapshot: invoicesSnapshot,
  };
}

function langFrom(locale) {
  return String(locale || '').toLowerCase().startsWith('en') ? 'en' : 'de';
}

function resetState(root, api) {
  STATE.api = api;
  STATE.root = root;
  STATE.cleanup = [];
  STATE.renderTimer = 0;
  STATE.data = Object.fromEntries(WATCHED_COLLECTIONS.map((name) => [name, []]));
  STATE.invoices = [];
  STATE.parties = {};
  STATE.selectedInvoiceId = null;
  STATE.userCollapsed = false;
  STATE.autoSelected = false;
  STATE.invoicesLoaded = false;
  STATE.search = '';
  STATE.view = 'cards';
  STATE.band = 'alle';
  STATE.filterType = 'all';
  STATE.lineDraft = null;
  STATE.edit = null;
  STATE.pendingRestore = null;
  STATE.locale = langFrom(api?.shell?.locale?.get?.() || globalThis.document?.documentElement?.lang || 'de');
  STATE.frame = null;
}

// snapshot(): structured-clone values only; open inputs (draft editor, payment form) always included.
function invoicesSnapshot() {
  const root = moduleRoot();
  const form = root?.querySelector?.('.invoices-payment-form');
  const paymentForm = form ? {
    invoice_id: STATE.selectedInvoiceId || '',
    amount_eur: form.querySelector('[name="amount_eur"]')?.value ?? '',
    skonto_eur: form.querySelector('[name="skonto_eur"]')?.value ?? '',
    payment_id: form.querySelector('[name="payment_id"]')?.value ?? '',
  } : null;
  return {
    v: SNAPSHOT_VERSION,
    selectedInvoiceId: STATE.selectedInvoiceId || '',
    userCollapsed: STATE.userCollapsed,
    search: root?.querySelector?.('[data-pg-search]')?.value ?? STATE.search,
    view: STATE.view,
    band: STATE.band,
    filterType: STATE.filterType,
    tab: STATE.lineDraft || '',
    edit: STATE.edit ? { id: STATE.edit.id, dirty: STATE.edit.dirty, isNew: Boolean(STATE.edit.isNew), inv: structuredClone(STATE.edit.inv) } : null,
    paymentForm,
    listScrollTop: STATE.frame?.listEl?.parentElement?.scrollTop || 0,
  };
}

// View state goes into the markup BEFORE autoWirePaneGrammar reads it; selection/edit apply at once
// (the main pane shows the record as soon as it is in the live data).
function applyRestoredView() {
  const r = STATE.pendingRestore;
  if (!r) return;
  const f = STATE.frame;
  const pane = f?.leftPane;
  if (pane?.querySelector) {
    const search = pane.querySelector('[data-pg-search]');
    if (search && typeof r.search === 'string') search.value = r.search;
    if (r.view === 'cards' || r.view === 'list') {
      pane.querySelectorAll('[data-pg-view]').forEach((b) => b.setAttribute('aria-pressed', String(b.dataset.pgView === r.view)));
    }
    if (BAND_ORDER.includes(r.band)) {
      pane.querySelectorAll('[data-pg-band]').forEach((tab) => {
        const on = tab.dataset.pgBand === r.band;
        tab.setAttribute('aria-selected', String(on));
        tab.classList.toggle('is-active', on);
      });
    }
    const type = pane.querySelector('[data-pg-filter][data-pg-name="type"]');
    if (type && typeof r.filterType === 'string' && [...type.options].some((o) => o.value === r.filterType)) type.value = r.filterType;
  }
  if (typeof r.search === 'string') STATE.search = r.search;
  if (r.view === 'cards' || r.view === 'list') STATE.view = r.view;
  if (BAND_ORDER.includes(r.band)) STATE.band = r.band;
  if (typeof r.filterType === 'string') STATE.filterType = r.filterType;
  if (typeof r.selectedInvoiceId === 'string' && r.selectedInvoiceId) STATE.selectedInvoiceId = r.selectedInvoiceId;
  if (typeof r.userCollapsed === 'boolean') STATE.userCollapsed = r.userCollapsed;
  if (typeof r.tab === 'string' && r.tab) STATE.lineDraft = r.tab;
  if (r.edit && typeof r.edit === 'object' && typeof r.edit.id === 'string' && r.edit.inv && typeof r.edit.inv === 'object') {
    STATE.edit = { id: r.edit.id, dirty: Boolean(r.edit.dirty), isNew: Boolean(r.edit.isNew), inv: r.edit.inv };
  }
  render();
}

// After the first live emission that carries the restored record: put back the payment form inputs and the scroll.
function applyPendingFormRestore() {
  const r = STATE.pendingRestore;
  if (!r || !selectedInvoice()) return;
  STATE.pendingRestore = null;
  const form = moduleRoot()?.querySelector?.('.invoices-payment-form');
  if (form && r.paymentForm && r.paymentForm.invoice_id === STATE.selectedInvoiceId) {
    for (const name of ['amount_eur', 'skonto_eur', 'payment_id']) {
      const input = form.querySelector(`[name="${name}"]`);
      if (input && typeof r.paymentForm[name] === 'string') input.value = r.paymentForm[name];
    }
  }
  const well = STATE.frame?.listEl?.parentElement;
  if (well && Number.isFinite(r.listScrollTop) && r.listScrollTop > 0) {
    window.requestAnimationFrame?.(() => { well.scrollTop = r.listScrollTop; });
  }
}

function scheduleRefresh() {
  if (STATE.renderTimer || !STATE.api) return;
  STATE.renderTimer = window.setTimeout(() => {
    STATE.renderTimer = 0;
    if (!STATE.api) return;
    refresh();
    render();
    applyPendingFormRestore();
  }, 80);
}

function refresh() {
  const parties = STATE.data.customer_accounts || [];
  STATE.parties = Object.fromEntries(parties.map((p) => [p.id, p]));
  STATE.invoices = (STATE.data.accounting_invoices || []).filter((inv) => !inv.is_deleted);
}

// Commands: api.command creates the command document locally and returns at once — the UI never waits.
// The command carries exactly the handler payload (V4-HANDLERS.md) plus client_context; record_id goes
// through opts (§16.19 Nr. 8). `module`/`command_type` are no longer part of the document.
// `onDone` chains a follow-up command on the pushed status (e.g. update → post), without blocking the UI.
function submitCommand(command, { onDone, onFailed } = {}) {
  const api = STATE.api;
  if (!api) return null;
  const type = command.command_type;
  const payload = { ...(command.payload || {}) };
  if (command.client_context) payload.client_context = command.client_context;
  const opts = command.record_id ? { record_id: command.record_id } : undefined;
  const code = `invoices.command.${type}`;
  let handle;
  try {
    handle = api.command(type, payload, opts);
  } catch (error) {
    api.status.report({ level: 'error', code, text: `${type}: ${error?.code || 'local'} ${error?.message || error}` });
    onFailed?.(error);
    return null;
  }
  let off = null;
  let finished = false;
  const stop = () => { finished = true; off?.(); off = null; };
  off = handle.status.subscribe((value) => {
    if (finished) return;
    if (value?.status === 'failed') {
      api.status.report({ level: 'error', code, text: `${type}: ${value.error || 'failed'}`, ref: handle.id });
      stop();
      onFailed?.(value);
    } else if (value?.status === 'done') {
      api.status.clear(code);
      stop();
      onDone?.(value);
    }
  });
  if (finished) off?.();
  handle.local.catch((error) => {
    if (finished) return;
    api.status.report({ level: 'error', code, text: `${type}: ${error?.code || 'local'} ${error?.message || error}`, ref: handle.id });
    stop();
    onFailed?.(error);
  });
  STATE.cleanup.push(stop);
  return handle;
}

function reportStatus(level, code, text) {
  STATE.api?.status?.report?.({ level, code, text });
}

function clearStatus(code) {
  STATE.api?.status?.clear?.(code);
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
    paneIcon(ICON.more, t('more'), { action: 'more' }));

  const header = createEl('header', 'ctox-pane-header ctox-pane-band');
  const titleRow = buildTitleRow(t('kicker'), t('invoices'), actions);
  titleRow.dataset.shellV2HeaderRow = '1';
  add(header, titleRow);

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
  filterbar.dataset.shellV2HeaderRow = '2';
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
  nav.dataset.shellV2HeaderRow = '3';
  add(nav, tabs);

  const well = createEl('div', 'ctox-pane-body ctox-well');
  const listEl = createEl('div', 'ctox-record-list');
  listEl.dataset.invoicesList = '';
  add(well, listEl);

  // Keine Fußzeile „N Einträge · Band“: sie wiederholt nur den Reiterzähler (Textdiät Regel 5).
  add(leftPane, header, nav, well);
  return { listEl, countEls, footerEl: null };
}

// Adopt the STATIC Shell-V2 frame from index.html (reference: modules/tickets,
// modules/knowledge). Both pane heads are markup, so they exist in every state —
// empty, loading, selected, and even when the data plane denies a read. The JS
// only fills texts, options, counters and actions. Returns null when the host
// carries no static frame (the header-less node test shims), and the
// programmatic builders below stay the fallback for exactly that case.
function adoptFrame(root) {
  if (!root?.querySelector) return null;
  const workspace = root.querySelector('[data-invoices-workspace]');
  const leftPane = root.querySelector('[data-invoices-rail]');
  const mainPane = root.querySelector('[data-invoices-main]');
  const listEl = root.querySelector('[data-invoices-list]');
  if (!workspace || !leftPane || !mainPane || !listEl) return null;

  relabelStaticFrame(root);

  // Option list and counted band come from the module constants so markup and
  // filter logic cannot drift apart.
  const typeFilter = root.querySelector('[data-invoices-type-filter]');
  if (typeFilter && !typeFilter.options?.length) {
    const allOption = createEl('option');
    allOption.value = 'all';
    allOption.textContent = t('allTypes');
    typeFilter.appendChild(allOption);
    for (const type of INVOICE_TYPES) {
      const opt = createEl('option');
      opt.value = type;
      opt.textContent = typeLabel(type);
      typeFilter.appendChild(opt);
    }
  }

  const countEls = {};
  const tabs = root.querySelector('[data-invoices-bands] .ctox-pane-tabs');
  if (tabs) {
    tabs.replaceChildren();
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
  }

  return {
    workspace,
    leftPane,
    mainPane,
    listEl,
    countEls,
    footerEl: root.querySelector('[data-pg-footer]'),
    mainKicker: root.querySelector('[data-invoices-main-kicker]'),
    mainTitle: root.querySelector('[data-invoices-main-title]'),
    mainActions: root.querySelector('[data-invoices-main-actions]'),
    mainBody: root.querySelector('[data-invoices-main-body]'),
    staticHead: true,
  };
}

// Static markup ships German defaults; t() re-labels it for the session locale (also on live locale change).
function relabelStaticFrame(root = moduleRoot()) {
  if (!root?.querySelectorAll) return;
  for (const node of root.querySelectorAll('[data-copy]')) {
    node.textContent = t(node.dataset.copy);
  }
  for (const node of root.querySelectorAll('[data-copy-label]')) {
    const text = t(node.dataset.copyLabel);
    node.setAttribute('aria-label', text);
    if (node.title !== undefined && node.tagName === 'BUTTON') node.title = text;
  }
  for (const node of root.querySelectorAll('[data-copy-placeholder]')) {
    node.setAttribute('placeholder', t(node.dataset.copyPlaceholder));
    node.setAttribute('aria-label', t(node.dataset.copyPlaceholder));
  }
  for (const tab of root.querySelectorAll('[data-pg-band]')) {
    const label = tab.querySelector('span:not(.view-count)');
    if (label) label.textContent = bandLabel(tab.dataset.pgBand);
  }
}

function ensureFrame(root) {
  if (STATE.frame) return STATE.frame;

  const adopted = adoptFrame(root);
  if (adopted) {
    STATE.frame = adopted;
    wireFrameEvents();
    return STATE.frame;
  }

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

  // innerHTML='' + appendChild (not replaceChildren) so the fake-DOM test shims
  // — whose replaceChildren drops its arguments — still see the frame.
  root.innerHTML = '';
  add(root, workspace);

  STATE.frame = { workspace, leftPane, mainPane, listEl: grammar.listEl, countEls: grammar.countEls, footerEl: grammar.footerEl };
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
  autoSelectFirst();
  renderList();
  renderMain();
}

// Wie Outlook/lexoffice: beim Öffnen den ersten Eintrag der Liste wählen (einmal je Mount; danach entscheidet
// der Nutzer, auch über „Details schließen“).
function autoSelectFirst() {
  if (STATE.autoSelected || !STATE.invoicesLoaded) return;
  if (STATE.selectedInvoiceId || STATE.userCollapsed) { STATE.autoSelected = true; return; }
  const first = visibleInvoices()[0];
  if (!first) return;
  STATE.selectedInvoiceId = first.id;
  STATE.autoSelected = true;
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
  const live = STATE.invoices.find((i) => i.id === STATE.selectedInvoiceId);
  if (live) return live;
  // Neue Rechnung (FIX9a): bis „Entwurf speichern“/„Buchen“ nur im Editor, nicht in der Datenbank.
  return STATE.edit?.isNew && STATE.edit.id === STATE.selectedInvoiceId ? STATE.edit.inv : null;
}

// Noch nicht gespeicherte neue Rechnung (kein Datensatz in accounting_invoices).
function isUnsavedNew(inv) {
  return Boolean(inv && STATE.edit?.isNew && STATE.edit.id === inv.id && !STATE.invoices.some((i) => i.id === inv.id));
}

// Abbrechen einer neuen Rechnung: nichts bleibt zurück.
function discardNewDraft() {
  if (STATE.edit?.isNew) STATE.edit = null;
  STATE.selectedInvoiceId = null;
  STATE.userCollapsed = true;
  render();
}

function renderList() {
  const f = STATE.frame;
  if (!f?.listEl) return;
  const rows = visibleInvoices();
  if (STATE.view === 'list') f.listEl.classList.add('is-list-view');
  else f.listEl.classList.remove('is-list-view');
  // Selection is flipped in place after the write. Keeping selectedId out of
  // the signature means an unchanged data set never rebuilds the well.
  const html = renderInvoiceListMarkup(rows, {
    view: STATE.view,
    selectedId: null,
  });
  const signature = JSON.stringify({
    view: STATE.view,
    rows: rows.map((inv) => ([
      inv.id,
      inv.invoice_number || '',
      inv.state || '',
      inv.party_id || '',
      inv.total_cents || 0,
      inv.updated_at_ms || 0,
    ])),
  });
  // Suche/Filter ohne Treffer (FIX9a): „Keine Treffer“ plus Filter zurücksetzen statt leerer Spalte.
  const noMatch = !rows.length && STATE.invoices.length > 0 && Boolean(STATE.search || (STATE.filterType && STATE.filterType !== 'all'));
  const listHtml = noMatch
    ? `<div class="invoices-no-match" data-invoices-no-match><p>${escapeHtml(t('noMatches'))}</p><button type="button" class="ctox-button" data-action="reset-filters">${escapeHtml(t('resetFilters'))}</button></div>`
    : html;
  renderHtmlIfChanged(f.listEl, listHtml, { signature: noMatch ? `nomatch:${STATE.locale}` : signature });
  applyListSelection();
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

// Seltene Aktionen (Import/Export) im „…“-Menü, damit der Spaltenkopf in die Spaltenbreite passt (Regel 6).
async function openMoreMenu(btn) {
  const r = btn.getBoundingClientRect();
  const choice = await STATE.api?.shell?.menu?.({
    items: [{ id: 'import', label: t('import') }, { id: 'export', label: t('export') }],
    at: { x: r.left, y: r.bottom },
  });
  if (choice === 'import') importRecords();
  else if (choice === 'export') exportRecords();
}

function onAction(event) {
  const btn = event.target?.closest?.('[data-action]');
  if (!btn || !STATE.frame?.workspace?.contains?.(btn)) return;
  const action = btn.dataset.action;
  if (action === 'new') openNewDraft();
  else if (action === 'reset-filters') STATE.frame?.leftPane?.querySelector?.('[data-pg-reset]')?.click();
  else if (action === 'import') importRecords();
  else if (action === 'export') exportRecords();
  else if (action === 'more') openMoreMenu(btn);
  else if (action === 'collapse-detail') {
    if (isUnsavedNew(selectedInvoice())) discardNewDraft();
    else { STATE.userCollapsed = true; renderMain(); }
  }
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

function emptyStage() {
  if (!STATE.invoicesLoaded || STATE.invoices.length) return [];
  const wrap = createEl('div', 'invoices-empty-stage');
  const line = createEl('p');
  line.textContent = t('emptyList');
  const btn = createEl('button', 'ctox-button is-primary');
  btn.type = 'button';
  btn.dataset.action = 'new';
  btn.textContent = t('newInvoice');
  add(wrap, line, btn);
  return [wrap];
}

function renderMain() {
  const f = STATE.frame;
  const pane = f?.mainPane;
  if (!pane) return;
  const inv = selectedInvoice();
  const reveal = shouldRevealDetail(Boolean(inv), STATE.userCollapsed);

  const actions = createEl('div', 'ctox-pane-actions');
  if (inv && reveal) {
    const badge = createEl('span', ['ctox-badge', stateBadgeClass(inv.state)].filter(Boolean).join(' '));
    badge.dataset.state = inv.state;
    badge.textContent = invoiceStateLabel(inv.state);
    actions.appendChild(badge);
    if (inv.state === 'draft' && !isUnsavedNew(inv)) actions.appendChild(paneIcon(ICON.trash, t('deleteDraft'), { action: 'delete', extraClass: 'is-danger' }));
    actions.appendChild(paneIcon(ICON.close, t('closeDetail'), { action: 'collapse-detail' }));
  }

  // Kein Kicker, der den Titel wiederholt (Textdiät Regel 3): ohne Auswahl leer.
  const kicker = inv && reveal ? (inv.state === 'draft' ? t('draft') : t('invoice')) : '';
  const title = inv && reveal
    ? (isUnsavedNew(inv) && !inv.party_id ? t('newInvoice') : `${inv.invoice_number || t('draft')} · ${partyName(inv.party_id)}`)
    : t('invoice');

  // Static head (index.html): only texts, actions and the body are refreshed —
  // the head itself is never rebuilt, so its Shell-V2 geometry survives every
  // state including a failed data read.
  if (f.staticHead) {
    if (f.mainKicker) f.mainKicker.textContent = kicker;
    if (f.mainTitle) f.mainTitle.textContent = title;
    if (f.mainActions) f.mainActions.replaceChildren(...actions.childNodes);
    const body = f.mainBody;
    if (body) {
      if (!inv || !reveal) {
        // Ohne Auswahl bleibt die Bühne leer; nur bei geladener, leerer Liste ein Satz plus Aktion (Regel 2).
        body.className = 'ctox-pane-body';
        body.replaceChildren(...emptyStage());
        delete pane.dataset.contextRecordId;
        delete pane.dataset.contextLabel;
      } else {
        body.className = 'ctox-pane-scroll invoices-pane-scroll';
        body.replaceChildren(inv.state === 'draft' ? renderEditor(draftFor(inv)) : renderDetail(inv));
        pane.dataset.contextRecordId = inv.id;
        pane.dataset.contextLabel = inv.invoice_number || inv.id;
      }
    }
    return;
  }

  const header = createEl('header', 'ctox-pane-header ctox-pane-band');
  const titleRow = buildTitleRow(kicker, title, actions, 'h1');
  titleRow.dataset.shellV2HeaderRow = '1';
  add(header, titleRow);

  let body;
  if (!inv || !reveal) {
    body = createEl('div', 'ctox-pane-body');
    delete pane.dataset.contextRecordId;
    delete pane.dataset.contextLabel;
  } else {
    body = createEl('div', 'ctox-pane-scroll invoices-pane-scroll');
    add(body, inv.state === 'draft' ? renderEditor(draftFor(inv)) : renderDetail(inv));
    pane.dataset.contextRecordId = inv.id;
    pane.dataset.contextLabel = inv.invoice_number || inv.id;
  }
  pane.replaceChildren(header, body);
}

// The editor works on a clone of the live draft (rows from api.query are plain snapshots). Unsaved
// edits (dirty) survive live refreshes and remounts (snapshot); a clean clone follows the live row.
function draftFor(inv) {
  if (!STATE.edit || STATE.edit.id !== inv.id) {
    STATE.edit = { id: inv.id, dirty: false, inv: structuredClone(inv) };
  } else if (!STATE.edit.dirty) {
    STATE.edit.inv = structuredClone(inv);
  }
  return STATE.edit.inv;
}

function markDirty() {
  if (STATE.edit) STATE.edit.dirty = true;
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
  partySelect.addEventListener('change', () => { inv.party_id = partySelect.value; markDirty(); renderMain(); });
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
      markDirty();
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
  typeSelect.addEventListener('change', () => { inv.invoice_type = typeSelect.value; markDirty(); });
  typeLabelEl.appendChild(typeSelect);
  meta.appendChild(typeLabelEl);

  wrap.appendChild(meta);

  const linesHeader = createEl('span', 'ctox-field-label');
  linesHeader.textContent = t('lines');
  wrap.appendChild(linesHeader);

  const tableWrap = createEl('div', 'ctox-table-wrap');
  const linesTable = createEl('table', 'ctox-table invoices-lines-table invoices-lines-view');
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
    markDirty();
    renderMain();
  });
  wrap.appendChild(addLineBtn);

  wrap.appendChild(renderTotals(inv));

  const actions = createEl('div', 'invoices-actions');

  const unsaved = isUnsavedNew(inv);
  if (unsaved) {
    // Neue Rechnung: Abbrechen verwirft den Editor, es bleibt nichts zurück (lexoffice/sevDesk).
    const cancelBtn = createEl('button', 'ctox-button');
    cancelBtn.type = 'button';
    cancelBtn.dataset.action = 'cancel-new';
    cancelBtn.textContent = t('cancelNew');
    cancelBtn.addEventListener('click', () => discardNewDraft());
    actions.appendChild(cancelBtn);
  }

  const saveBtn = createEl('button', 'ctox-button');
  saveBtn.type = 'button';
  saveBtn.textContent = t('saveDraft');
  if (unsaved && !inv.party_id) {
    saveBtn.disabled = true;
    saveBtn.title = t('chooseCustomer');
  }
  saveBtn.addEventListener('click', () => (isUnsavedNew(inv) ? saveNewDraft(inv) : updateDraft(inv)));
  actions.appendChild(saveBtn);

  // The one big text button is the composer's essential action (post).
  const postBtn = createEl('button', 'ctox-button is-primary');
  postBtn.type = 'button';
  postBtn.textContent = t('post');
  const issues = computeValidationIssues(inv);
  const postDisabled = !issues.canPost;
  postBtn.disabled = postDisabled;
  postBtn.title = postDisabled
    ? `${t('missingBeforePost')}: ${issues.errors.map((i) => i.message || i.field).join('; ') || t('unknown')}`
    : '';
  postBtn.addEventListener('click', () => postInvoice(inv));
  actions.appendChild(postBtn);

  // Fehlende Angaben stehen im Tooltip des gesperrten Buchen-Knopfs, kein roter Kasten im Editor (Textdiät Regel 1).
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

function lineColumnLabel(column) {
  return column?.labelKey ? t(column.labelKey) : (column?.label || '');
}

function renderLineHeader() {
  const thead = createEl('thead');
  const row = createEl('tr');
  for (const column of LINE_COLUMNS) {
    const th = createEl('th');
    if (column.num) th.className = 'is-num';
    th.textContent = lineColumnLabel(column);
    row.appendChild(th);
  }
  thead.appendChild(row);
  return thead;
}

// Positionszelle mit Spaltenschlüssel und Beschriftung. In der einspaltigen Stufe (Fenster ≤ 768 px) wird jede Zeile
// zur Karte „Beschreibung / Menge Einheit × Einzelpreis = Betrag / USt · SKR“ ohne Seitwärts-Scroll (FIX10, Standard
// lexoffice mobil); der Kopf entfällt dort, die Beschriftung trägt data-label.
function lineCell(index) {
  const column = LINE_COLUMNS[index];
  const td = createEl('td');
  if (column?.num) td.className = 'is-num';
  if (column?.col) td.dataset.col = column.col;
  const label = lineColumnLabel(column);
  if (label) td.dataset.label = label;
  return td;
}

// Zeilenbetrag (netto) – nur in der schmalen Kartenansicht sichtbar, breit bleibt die Tabelle wie sie ist.
function lineAmountCell(line) {
  const td = createEl('td', 'is-num invoices-line-amount');
  td.dataset.col = 'amount';
  td.dataset.label = t('lineAmount');
  td.textContent = formatCents(computeLineNetCents(line));
  return td;
}

function renderLineRow(inv, line) {
  const tr = createEl('tr');
  const cells = [
    { type: 'text', value: line.position ?? '', set: (v) => (line.position = Number(v) || line.position) },
    { type: 'text', value: line.description || '', set: (v) => (line.description = v) },
    { type: 'text', decimal: true, value: line.quantity == null ? '' : milliToUnitsInput(line.quantity), set: (v) => (line.quantity = unitsToMilli(v)) },
    { type: 'text', value: line.unit || 'Stk', set: (v) => (line.unit = v) },
    { type: 'text', decimal: true, value: line.unit_price_cents == null ? '' : centsToEuroInput(line.unit_price_cents), set: (v) => (line.unit_price_cents = euroToCents(v)) },
    { type: 'number', value: ((line.tax_rate || 0) * 100).toFixed(0), set: (v) => (line.tax_rate = Math.max(0, Math.min(1, Number(v) / 100))) },
    { type: 'text', value: line.account_code || '', set: (v) => (line.account_code = v) },
  ];
  const amountTd = lineAmountCell(line);
  const refreshAmount = () => { amountTd.textContent = formatCents(computeLineNetCents(line)); };
  for (const [index, c] of cells.entries()) {
    const td = lineCell(index);
    const input = createEl('input', 'ctox-input');
    input.type = c.type;
    if (c.decimal) input.inputMode = 'decimal';
    input.value = c.value;
    if (td.dataset.label) input.setAttribute('aria-label', td.dataset.label);
    // Typed values land in the draft clone at once, so snapshot() carries open input.
    input.addEventListener('input', () => { c.set(input.value); markDirty(); refreshAmount(); });
    input.addEventListener('change', () => {
      c.set(input.value);
      markDirty();
      refreshAmount();
      // Recompute totals in place to keep the operator in flow.
      const totals = computeInvoiceTotals(inv);
      const totalsEl = moduleRoot()?.querySelector?.('.invoices-totals');
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
  tr.appendChild(amountTd);
  const removeTd = lineCell(LINE_COLUMNS.length - 1);
  const removeBtn = createEl('button', 'ctox-icon-button');
  removeBtn.type = 'button';
  removeBtn.innerHTML = ICON.close;
  removeBtn.setAttribute('aria-label', t('removeLine'));
  removeBtn.title = t('removeLine');
  removeBtn.addEventListener('click', () => {
    inv.lines = (inv.lines || []).filter((l) => l.id !== line.id);
    markDirty();
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
    [t('date'), escapeHtml(formatDateDe(inv.invoice_date_ms))],
    [t('due'), escapeHtml(formatDateDe(inv.due_date_ms))],
    [t('net'), formatCents(inv.subtotal_cents)],
    [t('tax'), formatCents(inv.tax_cents)],
    [t('gross'), formatCents(inv.total_cents)],
    [t('paid'), formatCents(inv.paid_cents)],
    [t('open'), formatCents(inv.open_cents)],
  ];
  // Fold the customer snapshot (formerly a third-pane inspector) into the detail.
  const address = formatAddress(party.address);
  if (address) rows.push([t('address'), escapeHtml(address)]);
  if (party.email) rows.push([t('email'), escapeHtml(party.email)]);
  summary.innerHTML = rows.map(([dt, dd]) => `<dt>${escapeHtml(dt)}</dt><dd>${dd}</dd>`).join('');
  wrap.appendChild(summary);

  const linesHeader = createEl('span', 'ctox-field-label');
  linesHeader.textContent = t('lines');
  wrap.appendChild(linesHeader);
  const tableWrap = createEl('div', 'ctox-table-wrap');
  const linesTable = createEl('table', 'ctox-table invoices-lines-view');
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
      const td = lineCell(index);
      td.textContent = String(value ?? '');
      tr.appendChild(td);
    }
    tr.appendChild(lineAmountCell(line));
    tr.appendChild(lineCell(LINE_COLUMNS.length - 1));
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
    // Rundgang RUND2: „Mahnen“ nur für überfällige Rechnungen anbieten – vorher stand bei allen anderen
    // ein Zustandstext („Dunning ist nur für überfällige Rechnungen verfügbar.“) im Inhalt.
    ...(inv.state === 'overdue' ? [{ id: 'dunning', label: t('dunning') }] : []),
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
    clearStatus('invoices.xrechnung.failed');
  } catch (err) {
    reportStatus('error', 'invoices.xrechnung.failed', `${t('xmlFailed')}: ${err?.message || err}`);
  }
  return wrap;
}

function renderPaymentsTab(inv) {
  const wrap = createEl('div', 'invoices-tab');
  const openCents = inv.open_cents ?? Math.max(0, (inv.total_cents || 0) - (inv.paid_cents || 0));
  wrap.innerHTML = `
    <p>${escapeHtml(t('open'))}: <strong>${formatCents(openCents)}</strong></p>
    <form class="invoices-payment-form">
      <label class="ctox-compact-field">${escapeHtml(t('amountCents'))}<input class="ctox-input" type="text" inputmode="decimal" name="amount_eur" value="${escapeHtml(centsToEuroInput(openCents))}" required /></label>
      <label class="ctox-compact-field" title="${escapeHtml(t('discountHint'))}">${escapeHtml(t('discountCents'))}<input class="ctox-input" type="text" inputmode="decimal" name="skonto_eur" value="0,00" title="${escapeHtml(t('discountHint'))}" /></label>
      <label class="ctox-compact-field">${escapeHtml(t('paymentId'))}<input class="ctox-input" type="text" name="payment_id" placeholder="pay_…" required /></label>
      <button class="ctox-button is-primary" type="submit">${escapeHtml(t('allocate'))}</button>
    </form>
  `;
  const form = wrap.querySelector('form');
  form.addEventListener('submit', (event) => {
    event.preventDefault();
    const data = new FormData(form);
    submitCommand(buildAllocatePaymentCommand(inv.id, {
      payment_id: data.get('payment_id'),
      amount_cents: euroToCents(data.get('amount_eur')),
      skonto_cents: euroToCents(data.get('skonto_eur')),
    }));
  });
  return wrap;
}

// No empty payment id: an empty string would make the allocation id `alloc__<invoice>`, so a second
// payment without id on the same invoice counted as an idempotent repeat and was dropped (V4-HANDLERS I8).
// Without an id the field is left out and the handler derives a per-command allocation id.
export function buildAllocatePaymentCommand(invoiceId, form = {}) {
  const paymentId = String(form.payment_id ?? '').trim();
  return {
    command_type: 'invoices.payment.allocate',
    record_id: invoiceId,
    payload: {
      invoice_id: invoiceId,
      ...(paymentId ? { payment_id: paymentId } : {}),
      amount_cents: Math.round(Number(form.amount_cents) || 0),
      skonto_cents: Math.round(Number(form.skonto_cents) || 0),
    },
    client_context: { surface: 'invoices.payment.allocate' },
  };
}

function renderDunningTab(inv) {
  const wrap = createEl('div', 'invoices-tab');
  if (inv.state !== 'overdue') return wrap;
  wrap.innerHTML = `
    <button type="button" class="ctox-button is-primary">${escapeHtml(t('dunningRun'))}</button>
  `;
  const btn = wrap.querySelector('button');
  btn.addEventListener('click', () => {
    const runId = `dunning_${Date.now().toString(36)}`;
    // OFFEN (V4-PORT.md OFFEN-1, V4-HANDLERS I7): the handler only reads filter.state and
    // filter.min_open_cents; the invoice selection below is sent unchanged but has no effect yet.
    submitCommand({
      command_type: 'invoices.dunning.run',
      record_id: runId,
      payload: { run_id: runId, filter: { invoice_id: inv.id } },
      client_context: { surface: 'invoices.dunning.run' },
    });
  });
  return wrap;
}

// ---------------------------------------------------------------------------
// Commands (flows + payloads unchanged from the prior IA)
// ---------------------------------------------------------------------------

// „Neue Rechnung“ (FIX9a, Standard lexoffice/sevDesk): öffnet einen leeren Editor ohne Persistenz. Erst
// „Entwurf speichern“ bzw. „Buchen“ legt den Datensatz an (invoices.invoice.create); Abbrechen hinterlässt nichts.
function openNewDraft() {
  const now = Date.now();
  const invoiceId = `inv_${now.toString(36)}`;
  STATE.edit = {
    id: invoiceId,
    dirty: true,
    isNew: true,
    inv: {
      id: invoiceId,
      state: 'draft',
      invoice_type: 'sale_out',
      party_id: '',
      invoice_number: '',
      invoice_date_ms: now,
      due_date_ms: computeDueDateMs(now, 14),
      currency: 'EUR',
      lines: [],
    },
  };
  STATE.selectedInvoiceId = invoiceId;
  STATE.userCollapsed = false;
  STATE.lineDraft = null;
  render();
}

// Erstes Speichern einer neuen Rechnung: EIN create mit dem vollständigen Editorstand; Buchen hängt post an done.
function saveNewDraft(inv, { post = false } = {}) {
  if (!inv.party_id) return;
  const edit = STATE.edit;
  submitCommand(buildCreateInvoiceCommand(inv.id, draftPatch(inv, { withBreakdown: post })), {
    onDone: () => {
      // isNew bleibt: sobald die Zeile live ist, gilt sie über isUnsavedNew() nicht mehr als neu.
      if (STATE.edit === edit) edit.dirty = false;
      if (post) {
        submitCommand({
          command_type: 'invoices.invoice.post',
          record_id: inv.id,
          payload: { invoice_id: inv.id },
          client_context: { surface: 'invoices.invoice.post' },
        });
      }
    },
  });
}

function draftPatch(inv, { withBreakdown = false } = {}) {
  const totals = computeInvoiceTotals(inv);
  return {
    invoice_type: inv.invoice_type,
    party_id: inv.party_id,
    invoice_date_ms: inv.invoice_date_ms,
    due_date_ms: inv.due_date_ms,
    currency: inv.currency || 'EUR',
    lines: inv.lines || [],
    subtotal_cents: totals.subtotal_cents,
    tax_cents: totals.tax_cents,
    total_cents: totals.total_cents,
    ...(withBreakdown ? { tax_breakdown: totals.tax_breakdown || [] } : {}),
  };
}

function updateDraft(inv) {
  const edit = STATE.edit;
  submitCommand(buildUpdateInvoiceCommand(inv.id, draftPatch(inv)), {
    // Saved: the clone follows the live row again; on failure the edits stay open.
    onDone: () => { if (STATE.edit === edit) edit.dirty = false; },
  });
}

async function deleteDraft(inv) {
  const confirmed = await STATE.api?.shell?.dialog?.({
    kind: 'confirm',
    title: t('deleteDraft'),
    text: t('deleteConfirm', { id: inv.invoice_number || inv.id }),
    confirm_label: t('deleteDraft'),
    default: false,
  });
  if (confirmed !== true) return;
  submitCommand(buildDeleteInvoiceCommand(inv.id));
  STATE.selectedInvoiceId = null;
  STATE.edit = null;
  render();
}

function postInvoice(inv) {
  // The native validator must see the same draft the operator confirmed —
  // including unsaved edits — so we send the full patch before posting. The post command is
  // chained on the update's pushed `done` (no await, the UI keeps working).
  const issues = computeValidationIssues(inv);
  if (!issues.canPost) {
    reportStatus('warn', 'invoices.post.invalid', `${t('cannotPost')}: ${issues.errors.map((i) => i.message).join('; ')}`);
    return;
  }
  clearStatus('invoices.post.invalid');
  if (isUnsavedNew(inv)) { saveNewDraft(inv, { post: true }); return; }
  const edit = STATE.edit;
  submitCommand(buildUpdateInvoiceCommand(inv.id, draftPatch(inv, { withBreakdown: true })), {
    onDone: () => {
      if (STATE.edit === edit) edit.dirty = false;
      submitCommand({
        command_type: 'invoices.invoice.post',
        record_id: inv.id,
        payload: { invoice_id: inv.id },
        client_context: { surface: 'invoices.invoice.post' },
      });
    },
  });
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
    clearStatus('invoices.export.failed');
  } catch (error) {
    console.error('[invoices] export failed:', error);
    reportStatus('error', 'invoices.export.failed', `${t('export')}: ${error?.message || error}`);
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
    importInvoicesText(await file.text());
  });
  input.click();
}

// Import = one invoices.invoice.create per valid entry; problems go to the status zone, success is silent
// (the created invoices appear in the list live).
function importInvoicesText(text) {
  let parsed;
  try { parsed = JSON.parse(text); } catch {
    clearStatus('invoices.import.empty');
    reportStatus('warn', 'invoices.import.invalid', t('importInvalid'));
    return 0;
  }
  clearStatus('invoices.import.invalid');
  const entries = parseInvoiceImport(parsed, Date.now());
  if (!entries.length) { reportStatus('warn', 'invoices.import.empty', t('importEmpty')); return 0; }
  clearStatus('invoices.import.empty');
  let count = 0;
  const stamp = Date.now().toString(36);
  for (const [index, entry] of entries.entries()) {
    const invoiceId = `inv_imp_${stamp}_${index}`;
    if (submitCommand(buildCreateInvoiceCommand(invoiceId, entry))) count += 1;
  }
  return count;
}

// ---------------------------------------------------------------------------
// Debug handle + markup bootstrap
// ---------------------------------------------------------------------------

function invoicesDebugSnapshot() {
  return {
    mounted: Boolean(STATE.api),
    invoice_count: Array.isArray(STATE.invoices) ? STATE.invoices.length : 0,
    selected_invoice_id: STATE.selectedInvoiceId || '',
    band: STATE.band || 'alle',
    view: STATE.view || 'cards',
    watched_collections: [...WATCHED_COLLECTIONS],
  };
}

if (typeof window !== 'undefined') {
  window.__ctoxInvoicesModule = Object.freeze({
    mount,
    inspect: invoicesDebugSnapshot,
  });
}

// CSS loads the shell from the manifest; index.html comes from the Service Worker, versioned (§16.15).
// Without a markup file (header-less node test shims) the programmatic frame is the fallback.
async function ensureMountedMarkup(root) {
  if (!root) return null;
  if (root.querySelector?.('#invoices-root')) return moduleRoot();
  let html = '';
  try {
    html = await fetch(new URL('./index.html', import.meta.url)).then((res) => {
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      return res.text();
    });
  } catch (error) {
    console.warn('[invoices] markup load failed; falling back to inline root', error);
  }
  root.innerHTML = html || '<div id="invoices-root" class="invoices-shell"></div>';
  return moduleRoot();
}

function moduleRoot() {
  const root = STATE.root;
  if (!root) return null;
  if (typeof root.querySelector === 'function') return root.querySelector('#invoices-root');
  // Header-less node test shims without querySelector.
  return globalThis.document?.getElementById?.('invoices-root') || null;
}

export const __invoicesTestHooks = Object.freeze({ importInvoicesText, buildAllocatePaymentCommand });
