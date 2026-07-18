// index.js — entry point for the invoices module.
//
// The shell hands modules a Live-DB facade at `ctx.db`, a Live-Command-Bus
// facade at `ctx.commandBus.dispatch(...)`, and a shared event bus at
// `ctx.eventBus`. The module never reaches into data-plane internals; it
// always goes through the facade so the data plane can be rebuilt without
// breaking the module.
//
// Mount contract (v5, skill `business-os-app-module-development`):
//   - mount(ctx) returns a cleanup function. The shell calls it on unmount
//     and the cleanup detaches every collection.$ subscription we opened
//     during the lifetime of the mount.
//   - All read paths go through `resolveCollection(name)`, using the Shell's
//     explicit `ctx.db.collection(name)` facade contract.
//   - Reactive sync: we subscribe to `collection.$` for every watched
//     collection and coalesce emissions into one render via `scheduleRefresh`
//     (matches `customers/index.js:1037 wireRealtime()`). This means we no
//     longer need an explicit `eventBus.on('invoices:refresh')` loop — the
//     data plane fires the subscription natively when a replicated document
//     lands.
//   - Mutations go through `ctx.commandBus.dispatch({ module, command_type,
//     payload, ... })`. Native handlers in `src/core/business_os/invoices.rs`
//     own the GoBD-immutability of posted invoices; the browser never writes
//     accounting_* collections directly.

import {
  buildCreateInvoiceCommand,
  buildUpdateInvoiceCommand,
  buildDeleteInvoiceCommand,
  buildXRechnungXml,
} from './commands/builders.js';
import { validateInvoice } from './core/invoice-validate.js';

const BUILD = '20260711-responsive-pane-stack-v2';
const MODULE_ID = 'invoices';
const SKILL_TAG = 'product_engineering/business-os-app-module-development';
const COPY = {
  de: {
    invoices: 'Rechnungen', all: 'Alle', overdue: 'Überfällig', open: 'Offen', drafts: 'Entwürfe', newInvoice: 'Neue Rechnung', unknown: 'unbekannt', newShort: 'NEU',
    emptyHint: 'Wähle eine Rechnung aus der Liste oder erstelle einen neuen Entwurf.', invoice: 'Rechnung', draft: 'Entwurf', customer: 'Kunde', chooseCustomer: '— bitte Kunde wählen —',
    invoiceDate: 'Rechnungsdatum', type: 'Typ', lines: 'Positionen', addLine: '+ Position', net: 'Netto', tax: 'USt', gross: 'Brutto',
    saveDraft: 'Entwurf speichern', deleteDraft: 'Entwurf löschen', post: 'Buchen (GoBD-post)', missingBeforePost: 'Vor dem Buchen fehlt',
    removeLine: 'Position entfernen', invoiceNumber: 'Rechnungsnummer', date: 'Datum', due: 'Fällig', paid: 'Bezahlt', journal: 'Journal',
    payments: 'Zahlungen', dunning: 'Mahnen', noJournal: 'Kein Journal-Eintrag verknüpft.', account: 'Konto', description: 'Beschreibung', debit: 'Soll', credit: 'Haben',
    downloadXml: 'XRechnung-XML herunterladen', xmlFailed: 'XRechnung-Vorschau fehlgeschlagen', amountCents: 'Betrag (Cent)', discountCents: 'Skonto (Cent)',
    paymentId: 'Zahlungs-ID', allocate: 'Zuordnen', discountHint: 'Skonto wird nur abgezogen, wenn das Zahlungsdatum vor dem Skonto-Deadline liegt. Das berechnet der native Handler.',
    dunningOnlyOverdue: 'Dunning ist nur für überfällige Rechnungen verfügbar.', dunningHint: 'Diese Rechnung ist überfällig. Starte einen Mahnlauf, um einen Brief zu erzeugen.',
    dunningRun: 'Mahnlauf für diese Rechnung', inspectorEmpty: 'Keine Rechnung ausgewählt.', inspector: 'Inspector', inspectorShow: 'Inspector einblenden', inspectorHide: 'Inspector ausblenden', noAddress: 'Keine Adresse hinterlegt.',
    dependencyTitle: 'Rechnungen benötigt weitere Module', dependencyNote: 'Bitte installiere „buchhaltung“ (FIBU/Journal) und „customers“ (Party-Stamm) im App Store, dann lade das Rechnungen-Modul neu.',
    reload: 'Neu laden', missingDb: 'Invoices-Modul kann nicht starten: ctx.db fehlt.', noCustomer: 'Kein Kunde im CRM hinterlegt. Lege zuerst einen Kunden im „customers“-Modul an, dann erstelle die Rechnung hier.',
    deleteConfirm: 'Entwurf {id} löschen?', cannotPost: 'Rechnung kann nicht gebucht werden', resizeColumn: 'Spaltenbreite anpassen',
    stateDraft: 'Entwurf', statePosted: 'Gebucht', statePartiallyPaid: 'Teilweise bezahlt', statePaid: 'Bezahlt', stateOverdue: 'Überfällig', stateCancelled: 'Storniert', stateCredited: 'Gutgeschrieben',
    pos: 'Pos', quantity: 'Menge (‰)', unit: 'Einheit', unitPrice: 'Einzelpreis (Cent)',
  },
  en: {
    invoices: 'Invoices', all: 'All', overdue: 'Overdue', open: 'Open', drafts: 'Drafts', newInvoice: 'New invoice', unknown: 'unknown', newShort: 'NEW',
    emptyHint: 'Select an invoice from the list or create a new draft.', invoice: 'Invoice', draft: 'Draft', customer: 'Customer', chooseCustomer: '— select customer —',
    invoiceDate: 'Invoice date', type: 'Type', lines: 'Line items', addLine: '+ Line item', net: 'Net', tax: 'VAT', gross: 'Gross',
    saveDraft: 'Save draft', deleteDraft: 'Delete draft', post: 'Post (GoBD)', missingBeforePost: 'Required before posting',
    removeLine: 'Remove line item', invoiceNumber: 'Invoice number', date: 'Date', due: 'Due', paid: 'Paid', journal: 'Journal',
    payments: 'Payments', dunning: 'Dunning', noJournal: 'No journal entry linked.', account: 'Account', description: 'Description', debit: 'Debit', credit: 'Credit',
    downloadXml: 'Download XRechnung XML', xmlFailed: 'XRechnung preview failed', amountCents: 'Amount (cents)', discountCents: 'Discount (cents)',
    paymentId: 'Payment ID', allocate: 'Allocate', discountHint: 'The native handler applies the discount only when payment occurs before the discount deadline.',
    dunningOnlyOverdue: 'Dunning is available only for overdue invoices.', dunningHint: 'This invoice is overdue. Start a dunning run to generate a letter.',
    dunningRun: 'Run dunning for this invoice', inspectorEmpty: 'No invoice selected.', inspector: 'Inspector', inspectorShow: 'Show inspector', inspectorHide: 'Hide inspector', noAddress: 'No address recorded.',
    dependencyTitle: 'Invoices requires additional modules', dependencyNote: 'Install “buchhaltung” (ledger/journal) and “customers” (party master data) from the App Store, then reload Invoices.',
    reload: 'Reload', missingDb: 'Invoices cannot start: ctx.db is missing.', noCustomer: 'No customer exists in CRM. Create a customer in “customers” before creating an invoice.',
    deleteConfirm: 'Delete draft {id}?', cannotPost: 'Invoice cannot be posted', resizeColumn: 'Adjust column width',
    stateDraft: 'Draft', statePosted: 'Posted', statePartiallyPaid: 'Partially paid', statePaid: 'Paid', stateOverdue: 'Overdue', stateCancelled: 'Cancelled', stateCredited: 'Credited',
    pos: 'Pos', quantity: 'Quantity (‰)', unit: 'Unit', unitPrice: 'Unit price (cents)',
  },
};

// Left-pane scope filters (rendered as .ctox-chip pills).
const FILTERS = Object.freeze([
  { id: 'all', labelKey: 'all' },
  { id: 'overdue', labelKey: 'overdue' },
  { id: 'open', labelKey: 'open' },
  { id: 'draft', labelKey: 'drafts' },
]);

// Editor/detail line table columns; `num` columns render right-aligned
// (.ctox-table .is-num).
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

// Inline fallbacks (same paths as shared/icons.js actionIconPaths) for
// contexts without ctx.getActionIcon, e.g. the node test shims.
const FALLBACK_ICON_PATHS = Object.freeze({
  add: 'M12 5v14M5 12h14',
  close: 'M6 6l12 12M18 6L6 18',
});

// Inspector pane toggle icon (rectangle with a vertical seam — matches the
// threads/conversations/buchhaltung collapse toggle).
const INSPECTOR_TOGGLE_ICON_SVG = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M15 4v16"/></svg>';

// Per-shell-window storage key for the inspector visibility preference.
// Mirrors the buchhaltung / conversations collapse toggle persistence.
const INSPECTOR_LAYOUT_KEY = 'ctox.invoices.layout.actionsHidden';

function actionIcon(name) {
  const svg = STATE.ctx?.getActionIcon?.(name, 16);
  if (svg) return svg;
  const path = FALLBACK_ICON_PATHS[name];
  if (!path) return '';
  return `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="${path}"></path></svg>`;
}

// Maps an invoice lifecycle state onto the kit badge states.
function stateBadgeClass(state) {
  if (state === 'paid') return 'is-success';
  if (state === 'overdue' || state === 'cancelled') return 'is-danger';
  if (state === 'partially_paid') return 'is-warning';
  return '';
}

// Collections whose mutations should re-render the invoices shell. Includes
// module-owned collections plus the cross-module dependencies the inspector
// reads from.
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

const STATE = {
  ctx: null,
  cleanup: [],
  renderTimer: 0,
  invoices: [],
  parties: {},
  selectedInvoiceId: null,
  filter: 'all',
  lineDraft: null,
  busy: false,
  lastError: null,
  locale: 'de',
  frame: null,
  // Persistent inspector-toggle button (built once in ensureFrame, re-appended
  // to the center pane header on every render so it survives render cycles).
  inspectorToggle: null,
};

const REQUIRED_MODULES = ['buchhaltung', 'customers'];
const STATE_LABEL_KEYS = { draft: 'stateDraft', posted: 'statePosted', partially_paid: 'statePartiallyPaid', paid: 'statePaid', overdue: 'stateOverdue', cancelled: 'stateCancelled', credited: 'stateCredited' };

function t(key, replacements = {}) {
  let value = COPY[STATE.locale]?.[key] || COPY.de[key] || key;
  for (const [name, replacement] of Object.entries(replacements)) value = value.replace(`{${name}}`, String(replacement));
  return value;
}

function invoiceStateLabel(state) {
  return t(STATE_LABEL_KEYS[state] || 'unknown');
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

export async function mount(ctx) {
  resetState(ctx);
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
  // Cross-module signal: when a customer record is updated externally, the
  // party snapshot in our inspector must refresh. eventBus survives schema
  // drift recovery, so we keep it for cross-module coupling even though
  // same-collection changes are handled by wireRealtime().
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
  STATE.filter = 'all';
  STATE.lineDraft = null;
  STATE.busy = false;
  STATE.lastError = null;
  STATE.locale = String(ctx.locale || globalThis.document?.documentElement?.lang || 'de').toLowerCase().startsWith('en') ? 'en' : 'de';
  STATE.frame = null;
  STATE.inspectorToggle = null;
}

function isReady() {
  if (!STATE.ctx?.modules) return false;
  for (const id of REQUIRED_MODULES) {
    const mod = STATE.ctx.modules.find?.((m) => m?.id === id);
    if (!mod || mod.installed === false) return false;
  }
  return true;
}

function renderDependencyBlocker() {
  const root = moduleRoot();
  if (!root) return;
  root.innerHTML = '';
  STATE.frame = null;
  const card = document.createElement('div');
  card.className = 'ctox-empty';
  const title = document.createElement('strong');
  title.textContent = t('dependencyTitle');
  card.appendChild(title);
  const list = document.createElement('ul');
  for (const id of REQUIRED_MODULES) {
    const item = document.createElement('li');
    item.textContent = id;
    list.appendChild(item);
  }
  card.appendChild(list);
  const note = document.createElement('p');
  note.textContent = t('dependencyNote');
  card.appendChild(note);
  const retry = document.createElement('button');
  retry.type = 'button';
  retry.className = 'ctox-button';
  retry.textContent = t('reload');
  retry.addEventListener('click', () => {
    if (isReady()) {
      refresh().then(render).catch(reportError);
    } else {
      renderDependencyBlocker();
    }
  });
  card.appendChild(retry);
  root.appendChild(card);
}

function renderError(message) {
  const root = moduleRoot();
  if (!root) return;
  root.innerHTML = '';
  STATE.frame = null;
  const div = document.createElement('div');
  div.className = 'ctox-empty';
  div.textContent = message;
  root.appendChild(div);
}

async function refresh() {
  STATE.invoices = await readCollection('accounting_invoices');
  const parties = await readCollection('customer_accounts');
  STATE.parties = Object.fromEntries(parties.map((p) => [p.id, p]));
  STATE.invoices = (STATE.invoices || []).filter((inv) => !inv.is_deleted);
  STATE.lastError = null;
}

async function readCollection(name) {
  const c = resolveCollection(name);
  if (!c) return [];
  if (typeof c.find === 'function' && typeof c.find().exec === 'function') {
    const docs = await c.find().exec();
    return docs.map((doc) => doc?.toJSON?.() || doc).filter((doc) => doc && doc._deleted !== true && doc.is_deleted !== true);
  }
  if (typeof c.all === 'function') {
    return c.all();
  }
  return Array.isArray(c) ? c : [];
}

async function submitCommand(command) {
  if (STATE.busy) {
    throw new Error('invoices: another command is in flight');
  }
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

function render() {
  const root = moduleRoot();
  if (!root) return;
  root.classList.add('invoices-shell');
  root.dataset.contextModule = MODULE_ID;
  root.dataset.contextSubmodule = 'shell';
  root.dataset.contextSkill = SKILL_TAG;
  const frame = ensureFrame(root);
  renderListInto(frame.leftPane);
  renderCenterInto(frame.centerPane);
  renderInspectorInto(frame.rightPane);
  frame.banner.hidden = !STATE.lastError;
  frame.banner.textContent = STATE.lastError || '';
}

// The standard 3-pane frame (.ctox-workspace + .ctox-pane columns +
// declarative .ctox-column-resizer handles) is built once per mount and kept
// alive across re-renders: the shell wires the resizer handles exactly once
// (setupModuleResizers in app.js), so recreating them on every render would
// silently kill column resizing.
function ensureFrame(root) {
  if (STATE.frame) return STATE.frame;
  // `is-actions-hidden` is the default — the inspector (right pane) is a
  // situational detail panel; the center workbench gets the full width until
  // the operator opens it via the header toggle (data-toggle-actions).
  const workspace = document.createElement('main');
  workspace.className = 'ctox-workspace invoices-module is-actions-hidden';
  workspace.dataset.resizeFrame = '';

  const leftPane = document.createElement('aside');
  leftPane.className = 'ctox-pane';
  leftPane.dataset.leftContent = '';
  leftPane.dataset.contextModule = MODULE_ID;
  leftPane.dataset.contextSubmodule = 'list';
  leftPane.dataset.contextRecordType = 'accounting_invoices';

  const centerPane = document.createElement('section');
  centerPane.className = 'ctox-pane';
  centerPane.dataset.contextModule = MODULE_ID;
  centerPane.dataset.contextSubmodule = 'center';
  centerPane.dataset.contextRecordType = 'accounting_invoices';

  const rightPane = document.createElement('aside');
  rightPane.className = 'ctox-pane';
  rightPane.dataset.rightContent = '';

  workspace.appendChild(leftPane);
  workspace.appendChild(buildColumnResizer('left', '--ctox-left-width'));
  workspace.appendChild(centerPane);
  workspace.appendChild(buildColumnResizer('right', '--ctox-right-width'));
  workspace.appendChild(rightPane);

  const banner = document.createElement('div');
  banner.className = 'invoices-error-banner';
  banner.hidden = true;

  // Persistent inspector-toggle button. Created once, re-attached to the
  // center pane header on each render so the user's collapse preference and
  // its event listener survive render cycles.
  const inspectorToggle = document.createElement('button');
  inspectorToggle.type = 'button';
  inspectorToggle.className = 'ctox-pane-icon invoices-toggle-actions';
  inspectorToggle.dataset.toggleActions = '';
  inspectorToggle.innerHTML = INSPECTOR_TOGGLE_ICON_SVG;
  STATE.inspectorToggle = inspectorToggle;

  // innerHTML='' + appendChild (not replaceChildren) so the fake-DOM test
  // shims — whose replaceChildren drops its arguments — still see the frame.
  root.innerHTML = '';
  root.appendChild(workspace);
  root.appendChild(banner);
  STATE.frame = { leftPane, centerPane, rightPane, banner, workspace };
  wireInspectorToggle(workspace, inspectorToggle);
  return STATE.frame;
}

// Wires the inspector visibility toggle. The layout choice is persisted per
// shell window so the operator's chosen layout survives across mounts.
function wireInspectorToggle(workspace, toggle) {
  const saved = STATE.ctx?.storageScope?.get?.(INSPECTOR_LAYOUT_KEY);
  if (saved === 'false') {
    workspace.classList.remove('is-actions-hidden');
  }
  syncInspectorToggleUi(workspace, toggle);
  toggle.addEventListener('click', () => {
    const willHide = workspace.classList.toggle('is-actions-hidden');
    STATE.ctx?.storageScope?.set?.(INSPECTOR_LAYOUT_KEY, String(willHide));
    syncInspectorToggleUi(workspace, toggle);
  });
}

function syncInspectorToggleUi(workspace, toggle) {
  const hidden = workspace.classList.contains('is-actions-hidden');
  toggle.setAttribute('aria-pressed', hidden ? 'false' : 'true');
  toggle.setAttribute(
    'aria-label',
    hidden ? t('inspectorShow') : t('inspectorHide'),
  );
  toggle.title = hidden ? t('inspectorShow') : t('inspectorHide');
}

function buildColumnResizer(side, cssVar) {
  const handle = document.createElement('button');
  handle.type = 'button';
  handle.className = 'ctox-column-resizer';
  handle.dataset.resizer = side;
  handle.dataset.resizerVar = cssVar;
  handle.setAttribute('aria-label', t('resizeColumn'));
  return handle;
}

// Canonical kit pane header (kicker + title row, optional icon actions).
function buildPaneHeader(kickerText, titleText, actionsEl) {
  const header = document.createElement('header');
  header.className = 'ctox-pane-header ctox-pane-band';
  const titleRow = document.createElement('div');
  titleRow.className = 'ctox-pane-title-row';
  const titles = document.createElement('div');
  titles.className = 'ctox-pane-titles';
  const kicker = document.createElement('span');
  kicker.className = 'ctox-pane-kicker';
  kicker.textContent = kickerText;
  titles.appendChild(kicker);
  const title = document.createElement('h2');
  title.className = 'ctox-pane-title';
  title.textContent = titleText;
  titles.appendChild(title);
  titleRow.appendChild(titles);
  if (actionsEl) titleRow.appendChild(actionsEl);
  header.appendChild(titleRow);
  return header;
}

function renderListInto(pane) {
  const activeFilter = FILTERS.find((f) => f.id === STATE.filter) || FILTERS[0];

  const actions = document.createElement('div');
  actions.className = 'ctox-pane-actions';
  const createBtn = document.createElement('button');
  createBtn.type = 'button';
  createBtn.className = 'ctox-pane-icon';
  createBtn.innerHTML = actionIcon('add');
  createBtn.setAttribute('aria-label', t('newInvoice'));
  createBtn.title = t('newInvoice');
  createBtn.disabled = STATE.busy;
  createBtn.addEventListener('click', () => createDraft());
  actions.appendChild(createBtn);

  const header = buildPaneHeader(t('invoices'), `${t(activeFilter.labelKey)} (${visibleInvoices().length})`, actions);

  const filterRow = document.createElement('div');
  filterRow.className = 'ctox-pane-tools invoices-filter-row';
  for (const f of FILTERS) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'ctox-chip';
    btn.textContent = t(f.labelKey);
    btn.dataset.filter = f.id;
    btn.setAttribute('aria-pressed', STATE.filter === f.id ? 'true' : 'false');
    if (STATE.filter === f.id) btn.classList.add('is-active');
    btn.addEventListener('click', () => {
      STATE.filter = f.id;
      render();
    });
    filterRow.appendChild(btn);
  }
  header.appendChild(filterRow);

  const list = document.createElement('ul');
  list.className = 'ctox-list';
  for (const inv of visibleInvoices()) {
    const item = document.createElement('li');
    item.className = 'ctox-list-item invoices-list-item';
    if (inv.id === STATE.selectedInvoiceId) item.classList.add('is-selected');
    item.dataset.invoiceId = inv.id;
    item.dataset.contextModule = MODULE_ID;
    item.dataset.contextSubmodule = 'list-item';
    item.dataset.contextRecordType = 'accounting_invoices';
    item.dataset.contextRecordId = inv.id;
    item.dataset.contextLabel = inv.invoice_number || inv.id;
    const stateLabel = invoiceStateLabel(inv.state);
    item.innerHTML = `
      <strong>${escapeHtml(inv.invoice_number || t('newShort'))}</strong>
      <small>${escapeHtml(partyName(inv.party_id))}</small>
      <small>${escapeHtml(stateLabel)}</small>`;
    item.addEventListener('click', () => {
      STATE.selectedInvoiceId = inv.id;
      STATE.lineDraft = null;
      render();
    });
    list.appendChild(item);
  }
  pane.replaceChildren(header, list);
}

function visibleInvoices() {
  let list = STATE.invoices;
  if (STATE.filter === 'overdue') {
    list = list.filter((i) => i.state === 'overdue');
  } else if (STATE.filter === 'open') {
    list = list.filter((i) => ['posted', 'partially_paid', 'overdue'].includes(i.state));
  } else if (STATE.filter === 'draft') {
    list = list.filter((i) => i.state === 'draft');
  }
  return [...list].sort((a, b) => (b.updated_at_ms || 0) - (a.updated_at_ms || 0));
}

function renderCenterInto(pane) {
  const inv = STATE.invoices.find((i) => i.id === STATE.selectedInvoiceId);
  pane.replaceChildren();
  delete pane.dataset.contextRecordId;
  delete pane.dataset.contextLabel;

  // The inspector toggle is always reachable from the center pane header so
  // the operator can reveal the inspector at any time, even with no invoice
  // selected. The persistent button (built in ensureFrame) is re-attached
  // here so render cycles don't strand its event listener.
  const headerActions = document.createElement('div');
  headerActions.className = 'ctox-pane-actions';
  if (inv) {
    // Lifecycle state is a status display, not a control — kit badge, never a chip.
    const stateBadge = document.createElement('span');
    stateBadge.className = ['ctox-badge', stateBadgeClass(inv.state)]
      .filter(Boolean).join(' ');
    stateBadge.dataset.state = inv.state;
    stateBadge.textContent = invoiceStateLabel(inv.state);
    headerActions.appendChild(stateBadge);
    pane.dataset.contextRecordId = inv.id;
    pane.dataset.contextLabel = inv.invoice_number || inv.id;
  }
  if (STATE.inspectorToggle) {
    headerActions.appendChild(STATE.inspectorToggle);
  }

  const kicker = inv ? t('invoice') : t('invoices');
  const title = inv
    ? (inv.invoice_number
      ? `${inv.invoice_number} · ${partyName(inv.party_id)}`
      : `${t('draft')} · ${partyName(inv.party_id)}`)
    : t('invoice');
  pane.appendChild(buildPaneHeader(kicker, title, headerActions));

  if (!inv) {
    const body = document.createElement('div');
    body.className = 'ctox-pane-body';
    const empty = document.createElement('div');
    empty.className = 'ctox-empty';
    empty.textContent = t('emptyHint');
    body.appendChild(empty);
    pane.appendChild(body);
    return;
  }

  const body = document.createElement('div');
  body.className = 'ctox-pane-scroll invoices-pane-scroll';
  if (inv.state === 'draft') {
    body.appendChild(renderEditor(inv));
  } else {
    body.appendChild(renderDetail(inv));
  }
  pane.appendChild(body);
}

function renderEditor(inv) {
  const wrap = document.createElement('section');
  wrap.className = 'invoices-editor invoices-stack';

  const meta = document.createElement('div');
  meta.className = 'ctox-compact-form__fields';
  const partyLabel = document.createElement('label');
  partyLabel.className = 'ctox-compact-field';
  partyLabel.textContent = t('customer');
  const partySelect = document.createElement('select');
  partySelect.className = 'ctox-select';
  // Always include a placeholder option so the user can intentionally clear
  // or re-pick a customer; otherwise the previous selection would render as a
  // non-deletable default and silently leak to the post command.
  const placeholder = document.createElement('option');
  placeholder.value = '';
  placeholder.textContent = t('chooseCustomer');
  if (!inv.party_id) placeholder.selected = true;
  partySelect.appendChild(placeholder);
  for (const p of Object.values(STATE.parties)) {
    const opt = document.createElement('option');
    opt.value = p.id;
    opt.textContent = p.name || p.id;
    if (p.id === inv.party_id) opt.selected = true;
    partySelect.appendChild(opt);
  }
  partySelect.addEventListener('change', () => {
    inv.party_id = partySelect.value;
    // Re-render so the post/save buttons pick up the new validation state.
    render();
  });
  partyLabel.appendChild(partySelect);
  meta.appendChild(partyLabel);

  const dateLabel = document.createElement('label');
  dateLabel.className = 'ctox-compact-field';
  dateLabel.textContent = t('invoiceDate');
  const dateInput = document.createElement('input');
  dateInput.className = 'ctox-input';
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

  const typeLabel = document.createElement('label');
  typeLabel.className = 'ctox-compact-field';
  typeLabel.textContent = t('type');
  const typeSelect = document.createElement('select');
  typeSelect.className = 'ctox-select';
  for (const t of ['sale_out', 'sale_in', 'credit_note_out', 'credit_note_in', 'recurring_template']) {
    const opt = document.createElement('option');
    opt.value = t;
    opt.textContent = t;
    if (t === inv.invoice_type) opt.selected = true;
    typeSelect.appendChild(opt);
  }
  typeSelect.addEventListener('change', () => {
    inv.invoice_type = typeSelect.value;
  });
  typeLabel.appendChild(typeSelect);
  meta.appendChild(typeLabel);

  wrap.appendChild(meta);

  const linesHeader = document.createElement('span');
  linesHeader.className = 'ctox-field-label';
  linesHeader.textContent = t('lines');
  wrap.appendChild(linesHeader);

  const tableWrap = document.createElement('div');
  tableWrap.className = 'ctox-table-wrap';
  const linesTable = document.createElement('table');
  linesTable.className = 'ctox-table invoices-lines-table';
  linesTable.appendChild(renderLineHeader());
  const linesBody = document.createElement('tbody');
  for (const line of inv.lines || []) {
    linesBody.appendChild(renderLineRow(inv, line));
  }
  linesTable.appendChild(linesBody);
  tableWrap.appendChild(linesTable);
  wrap.appendChild(tableWrap);

  const addLineBtn = document.createElement('button');
  addLineBtn.type = 'button';
  addLineBtn.className = 'ctox-button';
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
    render();
  });
  wrap.appendChild(addLineBtn);

  const totals = computeInvoiceTotals(inv);
  const totalsDiv = document.createElement('div');
  totalsDiv.className = 'invoices-totals';
  totalsDiv.innerHTML = `
    <span>${escapeHtml(t('net'))}: <strong>${formatCents(totals.subtotal_cents)}</strong></span>
    <span>${escapeHtml(t('tax'))}: <strong>${formatCents(totals.tax_cents)}</strong></span>
    <span>${escapeHtml(t('gross'))}: <strong>${formatCents(totals.total_cents)}</strong></span>
  `;
  wrap.appendChild(totalsDiv);

  const actions = document.createElement('div');
  actions.className = 'invoices-actions';

  const saveBtn = document.createElement('button');
  saveBtn.type = 'button';
  saveBtn.className = 'ctox-button';
  saveBtn.textContent = t('saveDraft');
  saveBtn.disabled = STATE.busy;
  saveBtn.addEventListener('click', () => updateDraft(inv));
  actions.appendChild(saveBtn);

  const deleteBtn = document.createElement('button');
  deleteBtn.type = 'button';
  deleteBtn.className = 'ctox-button is-danger';
  deleteBtn.textContent = t('deleteDraft');
  deleteBtn.disabled = STATE.busy;
  deleteBtn.addEventListener('click', () => deleteDraft(inv));
  actions.appendChild(deleteBtn);

  const postBtn = document.createElement('button');
  postBtn.type = 'button';
  postBtn.className = 'ctox-button is-primary';
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
    const issuesBox = document.createElement('div');
    issuesBox.className = 'ctox-callout is-danger';
    const issuesList = document.createElement('ul');
    issuesList.className = 'invoices-issues';
    for (const issue of issues.errors) {
      const li = document.createElement('li');
      li.textContent = issue.message;
      issuesList.appendChild(li);
    }
    issuesBox.appendChild(issuesList);
    actions.appendChild(issuesBox);
  }

  wrap.appendChild(actions);
  return wrap;
}

function renderLineHeader() {
  const thead = document.createElement('thead');
  const row = document.createElement('tr');
  for (const column of LINE_COLUMNS) {
    const th = document.createElement('th');
    if (column.num) th.className = 'is-num';
    th.textContent = column.labelKey ? t(column.labelKey) : (column.label || '');
    row.appendChild(th);
  }
  thead.appendChild(row);
  return thead;
}

function renderLineRow(inv, line) {
  const tr = document.createElement('tr');
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
    const td = document.createElement('td');
    if (LINE_COLUMNS[index]?.num) td.className = 'is-num';
    const input = document.createElement('input');
    input.className = 'ctox-input';
    input.type = c.type;
    input.value = c.value;
    input.addEventListener('change', () => {
      c.set(input.value);
      // Recompute totals without a full re-render to keep the user in flow.
      const totals = computeInvoiceTotals(inv);
      const totalsEl = document.querySelector('.invoices-totals');
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
  const removeTd = document.createElement('td');
  const removeBtn = document.createElement('button');
  removeBtn.type = 'button';
  removeBtn.className = 'ctox-icon-button';
  removeBtn.innerHTML = actionIcon('close');
  removeBtn.setAttribute('aria-label', t('removeLine'));
  removeBtn.title = t('removeLine');
  removeBtn.addEventListener('click', () => {
    inv.lines = (inv.lines || []).filter((l) => l.id !== line.id);
    render();
  });
  removeTd.appendChild(removeBtn);
  tr.appendChild(removeTd);
  return tr;
}

export function computeInvoiceTotals(inv) {
  let subtotal = 0;
  let tax = 0;
  const byRate = new Map();
  for (const line of inv.lines || []) {
    const net = computeLineNetCents(line);
    const rate = Number(line.tax_rate) || 0;
    const t = Math.round(net * rate);
    subtotal += net;
    tax += t;
    if (rate > 0) {
      const key = rate.toFixed(4);
      const entry = byRate.get(key) || { tax_rate: rate, net_cents: 0, tax_cents: 0 };
      entry.net_cents += net;
      entry.tax_cents += t;
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

function renderDetail(inv) {
  const wrap = document.createElement('section');
  wrap.className = 'invoices-detail invoices-stack';

  const summary = document.createElement('dl');
  summary.className = 'ctox-fields';
  summary.innerHTML = `
    <dt>${escapeHtml(t('invoiceNumber'))}</dt><dd>${escapeHtml(inv.invoice_number || '—')}</dd>
    <dt>${escapeHtml(t('customer'))}</dt><dd>${escapeHtml(partyName(inv.party_id))}</dd>
    <dt>${escapeHtml(t('date'))}</dt><dd>${escapeHtml(isoDateInput(inv.invoice_date_ms))}</dd>
    <dt>${escapeHtml(t('due'))}</dt><dd>${escapeHtml(isoDateInput(inv.due_date_ms))}</dd>
    <dt>${escapeHtml(t('net'))}</dt><dd>${formatCents(inv.subtotal_cents)}</dd>
    <dt>${escapeHtml(t('tax'))}</dt><dd>${formatCents(inv.tax_cents)}</dd>
    <dt>${escapeHtml(t('gross'))}</dt><dd>${formatCents(inv.total_cents)}</dd>
    <dt>${escapeHtml(t('paid'))}</dt><dd>${formatCents(inv.paid_cents)}</dd>
    <dt>${escapeHtml(t('open'))}</dt><dd>${formatCents(inv.open_cents)}</dd>
  `;
  wrap.appendChild(summary);

  const linesHeader = document.createElement('span');
  linesHeader.className = 'ctox-field-label';
  linesHeader.textContent = t('lines');
  wrap.appendChild(linesHeader);
  const tableWrap = document.createElement('div');
  tableWrap.className = 'ctox-table-wrap';
  const linesTable = document.createElement('table');
  linesTable.className = 'ctox-table';
  linesTable.appendChild(renderLineHeader());
  const linesBody = document.createElement('tbody');
  for (const line of inv.lines || []) {
    const tr = document.createElement('tr');
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
      const td = document.createElement('td');
      if (LINE_COLUMNS[index]?.num) td.className = 'is-num';
      td.textContent = String(value ?? '');
      tr.appendChild(td);
    }
    const emptyTd = document.createElement('td');
    tr.appendChild(emptyTd);
    linesBody.appendChild(tr);
  }
  linesTable.appendChild(linesBody);
  tableWrap.appendChild(linesTable);
  wrap.appendChild(tableWrap);

  const tabs = document.createElement('div');
  tabs.className = 'ctox-pane-tabs';
  const tabButtons = [
    { id: 'journal', label: t('journal') },
    { id: 'xrechnung', label: 'XRechnung' },
    { id: 'payments', label: t('payments') },
    { id: 'dunning', label: t('dunning') },
  ];
  for (const t of tabButtons) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'ctox-pane-tab';
    btn.dataset.tab = t.id;
    btn.textContent = t.label;
    btn.setAttribute('aria-selected', STATE.lineDraft === t.id ? 'true' : 'false');
    if (STATE.lineDraft === t.id) btn.classList.add('active');
    btn.addEventListener('click', () => {
      STATE.lineDraft = STATE.lineDraft === t.id ? null : t.id;
      render();
    });
    tabs.appendChild(btn);
  }
  wrap.appendChild(tabs);

  if (STATE.lineDraft === 'journal') {
    wrap.appendChild(renderJournalTab(inv));
  } else if (STATE.lineDraft === 'xrechnung') {
    wrap.appendChild(renderXRechnungTab(inv));
  } else if (STATE.lineDraft === 'payments') {
    wrap.appendChild(renderPaymentsTab(inv));
  } else if (STATE.lineDraft === 'dunning') {
    wrap.appendChild(renderDunningTab(inv));
  }
  return wrap;
}

function renderJournalTab(inv) {
  const wrap = document.createElement('div');
  wrap.className = 'invoices-tab';
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
  const wrap = document.createElement('div');
  wrap.className = 'invoices-tab';
  try {
    const xml = buildXRechnungXml(inv, STATE.parties[inv.party_id] || {}, { name: 'CTOX' });
    const pre = document.createElement('pre');
    pre.className = 'ctox-pre';
    pre.textContent = xml;
    wrap.appendChild(pre);
    const download = document.createElement('button');
    download.type = 'button';
    download.className = 'ctox-button';
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
  const wrap = document.createElement('div');
  wrap.className = 'invoices-tab';
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
  const wrap = document.createElement('div');
  wrap.className = 'invoices-tab';
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

function renderInspectorInto(pane) {
  const inv = STATE.invoices.find((i) => i.id === STATE.selectedInvoiceId);
  pane.replaceChildren();
  pane.appendChild(buildPaneHeader(t('invoices'), t('inspector')));
  if (!inv) {
    const body = document.createElement('div');
    body.className = 'ctox-pane-body';
    const empty = document.createElement('div');
    empty.className = 'ctox-empty';
    empty.textContent = t('inspectorEmpty');
    body.appendChild(empty);
    pane.appendChild(body);
    return;
  }

  // Inspector content stays minimal: only the party card carries data the
  // center pane summary does not already show (address + email). Totals,
  // status and lines live in the workbench; an "actions" placeholder card
  // was pure dead UI and has been removed.
  const body = document.createElement('div');
  body.className = 'ctox-pane-scroll invoices-pane-scroll invoices-stack';

  const party = STATE.parties[inv.party_id] || {};
  const partyCard = document.createElement('div');
  partyCard.className = 'ctox-card';
  partyCard.innerHTML = `
    <header>${escapeHtml(t('customer'))}</header>
    <div class="ctox-card-body">
      <h3>${escapeHtml(party.name || inv.party_id)}</h3>
      <p>${escapeHtml(party.address || t('noAddress'))}</p>
      <p>${escapeHtml(party.email || '')}</p>
    </div>
  `;
  body.appendChild(partyCard);

  pane.appendChild(body);
}

async function createDraft() {
  // Hard guard: without a customer the native handler will reject the create
  // and the user gets a confusing failure. Refuse early and surface a hint.
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
    })
  );
  STATE.selectedInvoiceId = invoiceId;
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
    })
  );
  await refresh();
  render();
}

async function deleteDraft(inv) {
  if (!confirm(t('deleteConfirm', { id: inv.invoice_number || inv.id }))) return;
  await submitCommand(buildDeleteInvoiceCommand(inv.id));
  STATE.selectedInvoiceId = null;
  await refresh();
  render();
}

async function postInvoice(inv) {
  // Native validator must see the same draft the user sees in the editor —
  // including unsaved mutations to date, type, lines, party_id, etc. Sending
  // only the totals would let the Rust post-hook write a row that disagrees
  // with what the user just confirmed.
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
    })
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
  // Lazy-import to keep the editor hot path cheap on first render. The
  // validator is a pure function — it must mirror the rules in
  // src/core/business_os/invoices.rs::validate_invoice_for_post so a draft
  // that the UI accepts cannot be rejected by the native handler.
  const issues = validateInvoice(inv || {});
  return {
    errors: issues.filter((i) => (i.severity || 'error') === 'error'),
    warnings: issues.filter((i) => i.severity === 'warning'),
    canPost: issues.every((i) => (i.severity || 'error') !== 'error')
      && Boolean(inv?.party_id)
      && Array.isArray(inv?.lines) && inv.lines.length > 0,
  };
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

function invoicesDebugSnapshot() {
  return {
    mounted: Boolean(STATE.ctx),
    invoice_count: Array.isArray(STATE.invoices) ? STATE.invoices.length : 0,
    selected_invoice_id: STATE.selectedInvoiceId || '',
    filter: STATE.filter || 'all',
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

async function ensureMountedMarkup(ctx) {
  if (!ctx?.host?.querySelector) return moduleRoot();
  if (ctx.host.querySelector('#invoices-root')) return moduleRoot();
  try {
    const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => {
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
