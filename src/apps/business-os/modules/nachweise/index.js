// App-API 4.0 (Shell v4): mount(root, api, {restored}) -> {unmount, snapshot}.
import {
  CREDENTIAL_TYPES,
  credentialStatus,
  daysUntilExpiry,
  isDeploymentBlocking,
} from './core/credential.js';

const MODULE_ID = 'nachweise';
const PRIMARY = 'business_credentials';
const DEPLOY_COMMAND = 'ats.deployment.check';
const SIGNOFF_COMMAND = 'ats.leistungsnachweis.signoff';

const TYPE_LABEL = new Map(CREDENTIAL_TYPES.map((t) => [t.key, t.label]));
const TYPE_LABEL_EN = new Map([
  ['staplerschein', 'Forklift licence'], ['g25', 'G25 (driving/control work)'],
  ['g37', 'G37 (display-screen work)'], ['schweisserpruefung', 'Welder qualification'],
  ['fuehrerschein', 'Driving licence'], ['pflege_fortbildung', 'Care training'],
  ['aufenthaltstitel', 'Residence/work permit'], ['fuehrungszeugnis', 'Criminal-record certificate'],
]);
const COPY = {
  de: {
    title: 'Nachweise', kicker: 'ATS', listTitle: 'Nachweise', newTitle: 'Neuer Nachweis',
    actionsLabel: 'Prüfung & Sign-off', newAction: 'Neuer Nachweis', importAction: 'Importieren', exportAction: 'Exportieren',
    searchPlaceholder: 'Suchen...', closeDetail: 'Details schließen', deployCheckRow: 'Einsatz prüfen',
    showAsList: 'Als Liste anzeigen', showAsCards: 'Als Karten anzeigen',
    bandAll: 'Alle', bandValid: 'Gültig', bandExpiring: 'Ablaufend', bandCritical: 'Kritisch',
    statusAll: 'Alle Status', entries: 'Einträge',
    imported: 'Importiert', importInvalid: 'Ungültige JSON-Datei.', importEmpty: 'Keine Datensätze in der Datei.',
    subjectLabel: 'Person', credentialTypeLabel: 'Nachweistyp', validUntilLabel: 'Gültig bis', deploymentSubjectLabel: 'Einsatz-Subjekt', requiredTypesLabel: 'Pflicht-Typen', recordLabel: 'Leistungsnachweis', rateLabel: 'Verrechnungssatz', clientLabel: 'Entleiher', signatureLabel: 'Signatur-Anfrage', collectionLabel: 'Collection',
    subjectPlaceholder: 'Subjekt-ID (Kandidat/Mitarbeiter)', issuerPlaceholder: 'Aussteller', addCredential: 'Nachweis hinzufügen',
    deploymentSubjectPlaceholder: 'Subjekt-ID für Einsatz-Prüfung', requiredTypesPlaceholder: 'Pflicht-Typen (kommagetrennt)',
    checkReadiness: 'Einsatzbereitschaft prüfen', recordPlaceholder: 'Leistungsnachweis-ID (Sign-off)',
    ratePlaceholder: 'Verrechnungssatz €/h (Pflicht für Abrechnung)', clientAccountPlaceholder: 'Entleiher-Account-ID (Rechnungsempfänger)',
    signaturePlaceholder: 'Signatur-Anfrage-ID (falls Entleiher-Signatur erzwungen)', collectionPlaceholder: 'Collection (optional, Standard: planning_time_records)',
    releaseBilling: 'Entleiher-Sign-off → Abrechnung freigeben', credentials: 'Nachweise',
    blockedReason: 'blockiert', notReady: 'Nicht einsatzbereit', ready: 'Einsatzbereit',
    subjectRequired: 'Subjekt-ID erforderlich.', saveFailed: 'Nachweis konnte nicht gespeichert werden.',
    recordRequired: 'Leistungsnachweis-ID erforderlich.', signoffBlocked: 'Sign-off blockiert.',
    signoffSaved: 'Entleiher-Sign-off gespeichert.', performanceRecord: 'Leistungsnachweis', billingReleased: 'Abrechnung freigegeben',
    invoice: 'Rechnung', net: 'Netto', yes: 'ja', no: 'nein', subject: 'Person', issuer: 'Aussteller',
    expiredAgo: 'abgelaufen vor', daysRemaining: 'noch', days: 'Tage', noExpiry: 'kein Ablauf', verifiedBy: 'verifiziert von',
    deploymentCheck: 'Einsatz prüfen', deploymentBlocking: 'Einsatz blockierend', credential: 'Nachweis',
    statusValid: 'gültig', statusExpiring: 'läuft ab', statusExpired: 'abgelaufen', statusNotYetValid: 'noch nicht gültig', statusUnverified: 'nicht verifiziert',
  },
  en: {
    title: 'Credentials', kicker: 'ATS', listTitle: 'Credentials', newTitle: 'New credential', // v4-allow: en
    actionsLabel: 'Checks & sign-off', newAction: 'New credential', importAction: 'Import', exportAction: 'Export',
    searchPlaceholder: 'Search...', closeDetail: 'Close details', deployCheckRow: 'Check deployment',
    showAsList: 'Show as list', showAsCards: 'Show as cards',
    bandAll: 'All', bandValid: 'Valid', bandExpiring: 'Expiring', bandCritical: 'Critical',
    statusAll: 'All statuses', entries: 'records',
    imported: 'Imported', importInvalid: 'Invalid JSON file.', importEmpty: 'No records in the file.',
    subjectLabel: 'Subject', credentialTypeLabel: 'Credential type', validUntilLabel: 'Valid until', deploymentSubjectLabel: 'Deployment subject', requiredTypesLabel: 'Required types', recordLabel: 'Performance record', rateLabel: 'Charge rate', clientLabel: 'Client', signatureLabel: 'Signature request', collectionLabel: 'Collection',
    subjectPlaceholder: 'Subject ID (candidate/employee)', issuerPlaceholder: 'Issuer', addCredential: 'Add credential',
    deploymentSubjectPlaceholder: 'Subject ID for deployment check', requiredTypesPlaceholder: 'Required types (comma-separated)',
    checkReadiness: 'Check deployment readiness', recordPlaceholder: 'Performance record ID (sign-off)',
    ratePlaceholder: 'Charge rate €/h (required for billing)', clientAccountPlaceholder: 'Client account ID (invoice recipient)',
    signaturePlaceholder: 'Signature request ID (when client signature is enforced)', collectionPlaceholder: 'Collection (optional, default: planning_time_records)',
    releaseBilling: 'Client sign-off → release billing', credentials: 'credentials',
    blockedReason: 'blocked', notReady: 'Not ready for deployment', ready: 'Ready for deployment', // v4-allow: label
    subjectRequired: 'Subject ID is required.', saveFailed: 'Credential could not be saved.',
    recordRequired: 'Performance record ID is required.', signoffBlocked: 'Sign-off blocked.',
    signoffSaved: 'Client sign-off saved.', performanceRecord: 'Performance record', billingReleased: 'Billing released',
    invoice: 'Invoice', net: 'Net', yes: 'yes', no: 'no', subject: 'Subject', issuer: 'Issuer',
    expiredAgo: 'expired', daysRemaining: 'expires in', days: 'days', noExpiry: 'no expiry', verifiedBy: 'verified by',
    deploymentCheck: 'Check deployment', deploymentBlocking: 'Blocks deployment', credential: 'Credential',
    statusValid: 'valid', statusExpiring: 'expiring', statusExpired: 'expired', statusNotYetValid: 'not yet valid', statusUnverified: 'unverified',
  },
};
let text = COPY.de;
let locale = 'de';

// ---------------------------------------------------------------------------
// Pure record helpers (exported for tests — no DOM, no RxDB).
// ---------------------------------------------------------------------------

// Auto-reveal model (design-guide "Progressive Disclosure", outbound idiom):
// the record detail is shown only when something is selected and the user has
// not collapsed it.
export function shouldRevealRecord(hasSelection, userCollapsed) {
  return Boolean(hasSelection) && !userCollapsed;
}

// Derived live status of a credential (core/credential.js): valid | expiring |
// expired | not_yet_valid | unverified.
export function statusOf(r, nowMs) {
  return credentialStatus(r, Number(nowMs) || 0);
}

// Which counted band a derived status belongs to. The left column's band splits
// Alle / Gültig / Ablaufend / Kritisch — "critical" = needs action now (expired
// or unverified).
export function credentialBand(status) {
  const st = String(status || 'unverified');
  if (st === 'expired' || st === 'unverified') return 'critical';
  if (st === 'expiring') return 'expiring';
  return 'valid'; // 'valid' | 'not_yet_valid'
}

export function statusLabel(status) {
  const key = { valid: 'statusValid', expiring: 'statusExpiring', expired: 'statusExpired', not_yet_valid: 'statusNotYetValid', unverified: 'statusUnverified' }[status];
  return (key && text[key]) || String(status || '');
}

export function countsFor(rows, nowMs) {
  const list = Array.isArray(rows) ? rows : [];
  const counts = { all: list.length, valid: 0, expiring: 0, critical: 0 };
  for (const r of list) {
    const band = credentialBand(statusOf(r, nowMs));
    counts[band] += 1;
  }
  return counts;
}

// Apply the current grammar state (band + status filter + search) to the rows.
export function filterRows(rows, { band = 'all', status = 'all', search = '' } = {}, nowMs) {
  const needle = String(search || '').trim().toLowerCase();
  return (Array.isArray(rows) ? rows : []).filter((r) => {
    const st = statusOf(r, nowMs);
    if (band && band !== 'all' && credentialBand(st) !== band) return false;
    if (status && status !== 'all' && st !== status) return false;
    if (needle) {
      const hay = [typeLabel(r.credential_type, locale), r.subject_id, r.issuer, r.verified_by, st, r.id]
        .filter(Boolean).join(' ').toLowerCase();
      if (!hay.includes(needle)) return false;
    }
    return true;
  });
}

// Absolute expiry date of a credential in the module locale, or '' when the
// record carries none. Used by the shard view, which shows the hard date next
// to the derived remaining-days figure.
export function expiryDate(r, loc = locale) {
  const ms = Number(r?.valid_until_ms);
  if (!Number.isFinite(ms) || ms <= 0) return '';
  try { return new Date(ms).toLocaleDateString(loc === 'en' ? 'en-GB' : 'de-DE'); }
  catch { return ''; }
}

// The two views are deliberately DIFFERENT shapes, not two paddings of the same
// row (Betreiber-Direktive 31.08.):
//
//   cards / shards — 3 lines: bold title + status, then the two meta lines that
//                    actually decide a credential (Subjekt/Aussteller and
//                    Gültig-bis/Restlaufzeit), on generous padding.
//   list           — exactly ONE dense line: "Typ · Subjekt" as the title, one
//                    short meta (the status badge) hard right, nothing else.
//
// Both stay pure selectors: no inline expansion, no per-row buttons
// (design-guide "Canonical Column Grammar"). Fields come from the records the
// module already loads — no new data path.
export function credentialRow(r, opts = {}) {
  const view = opts.view === 'list' ? 'list' : 'cards';
  const selected = Boolean(opts.selected);
  const now = Number(opts.nowMs) || 0;
  const status = statusOf(r, now);
  const title = typeLabel(r.credential_type, locale);
  const subject = refName(r, 'subject', locale);
  const badge = '<span class="ctox-badge' + badgeStateClass(status) + '" data-status="' + esc(status) + '">' + esc(statusLabel(status)) + '</span>';
  const id = r.id || '';
  const attrs = ' class="ctox-list-item nachweise-row nachweise-row--' + view + (selected ? ' is-selected' : '') + '"'
    + ' role="button" tabindex="0" aria-selected="' + (selected ? 'true' : 'false') + '"'
    + ' data-context-record-id="' + esc(id) + '"'
    + ' data-context-record-type="nachweis"'
    + ' data-context-label="' + esc((title + ' · ' + subject) || id) + '"';
  if (view === 'list') {
    // One line: the identity ("Typ · Subjekt") plus a single short meta right.
    return '<div' + attrs + '>'
      + '<span class="nachweise-row-title">' + esc(title + ' · ' + subject) + '</span>'
      + badge
      + '</div>';
  }
  const days = daysUntilExpiry(r, now);
  const daysLabel = Number.isFinite(days)
    ? (days < 0 ? text.expiredAgo + ' ' + Math.abs(days) + ' ' + text.days : text.daysRemaining + ' ' + days + ' ' + text.days)
    : text.noExpiry;
  const date = expiryDate(r);
  const primary = [text.subject + ': ' + subject];
  if (r.issuer) primary.push(text.issuer + ': ' + r.issuer);
  // Line 2 stays at two facts: the hard date and the derived distance to it.
  // Verifier and record id belong to the detail card, not to a selector row.
  const secondary = [text.validUntilLabel + ': ' + (date || '—'), daysLabel];
  return '<div' + attrs + '>'
    + '<div class="nachweise-row-head"><span class="nachweise-row-title">' + esc(title) + '</span>' + badge + '</div>'
    + '<div class="nachweise-row-meta">' + esc(primary.join(' · ')) + '</div>'
    + '<div class="nachweise-row-meta">' + esc(secondary.join(' · ')) + '</div>'
    + '</div>';
}

// The list body markup (shards or compact rows). An empty list renders
// nothing (App-API 4.0: no state text in the content; state lives only in the
// shell status zone).
export function renderRecordList(rows, opts = {}) {
  return (Array.isArray(rows) ? rows : [])
    .map((r) => credentialRow(r, { view: opts.view, nowMs: opts.nowMs, selected: r.id && r.id === opts.selectedId }))
    .join('');
}

// Read-only detail card for the selected credential (auto-reveal target).
export function recordDetailHtml(r, nowMs) {
  const now = Number(nowMs) || 0;
  const status = statusOf(r, now);
  const blocking = isDeploymentBlocking(r, now);
  const days = daysUntilExpiry(r, now);
  const daysLabel = Number.isFinite(days)
    ? (days < 0 ? text.expiredAgo + ' ' + Math.abs(days) + ' ' + text.days : text.daysRemaining + ' ' + days + ' ' + text.days)
    : text.noExpiry;
  const rows = [];
  rows.push(field(text.subject, esc(refName(r, 'subject', locale))));
  if (r.issuer) rows.push(field(text.issuer, esc(r.issuer)));
  rows.push(field(text.validUntilLabel, esc(daysLabel)));
  if (r.verified_by) rows.push(field(text.verifiedBy, esc(r.verified_by)));
  return '<header class="nachweise-detail-head">'
    + '<div class="nachweise-detail-titles">'
    + '<span class="ctox-badge' + badgeStateClass(status) + '" data-status="' + esc(status) + '">' + esc(statusLabel(status)) + '</span>'
    + (blocking ? '<span class="ctox-badge is-danger">' + esc(text.deploymentBlocking) + '</span>' : '')
    + '<strong class="nachweise-detail-name">' + esc(typeLabel(r.credential_type, locale)) + '</strong>'
    + '</div>'
    + '<div class="nachweise-detail-actions">'
    + (r.subject_id ? '<button type="button" class="ctox-pane-icon" data-action="deploy-check" aria-label="' + esc(text.deployCheckRow) + '" title="' + esc(text.deployCheckRow) + '"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 12l2 2 4-4"/><path d="M12 3l7 3v6c0 4-3 7-7 9-4-2-7-5-7-9V6z"/></svg></button>' : '')
    + '<button type="button" class="ctox-pane-icon" data-action="collapse-detail" aria-label="' + esc(text.closeDetail) + '" title="' + esc(text.closeDetail) + '"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"/></svg></button>'
    + '</div>'
    + '</header>'
    + '<dl class="ctox-fields ctox-fields--stacked">' + rows.join('') + '</dl>';
}

// Namen statt IDs (Textdiät Regel 4): Anzeigename aus dem Datensatz (<basis>_name / <basis>.name / <basis>_title),
// sonst „Unbekannt“ – nie die technische ID.
function refName(r, base, lang = 'de') {
  const v = r?.[base + '_name'] ?? r?.[base]?.name ?? r?.[base + '_title'];
  return v ? String(v) : (lang === 'en' ? 'Unknown' : 'Unbekannt'); // v4-allow: en
}

function field(label, valueHtml) {
  return '<dt>' + esc(label) + '</dt><dd>' + valueHtml + '</dd>';
}

// Prepare an imported credential JSON record for upsert, preserving id/status/
// timestamps from an exported record (round-trip friendly).
export function prepareImport(raw, nowMs, salt = '') {
  const src = raw && typeof raw === 'object' ? raw : {};
  const now = Number(nowMs) || 0;
  const id = String(src.id || ('cred_' + now + '_' + salt));
  const validFrom = Number(src.valid_from_ms);
  const validUntil = Number(src.valid_until_ms);
  return {
    id,
    subject_id: String(src.subject_id || ''),
    subject_type: String(src.subject_type || 'candidate'),
    credential_type: String(src.credential_type || ''),
    issuer: String(src.issuer || ''),
    valid_from_ms: Number.isFinite(validFrom) ? validFrom : 0,
    valid_until_ms: Number.isFinite(validUntil) ? validUntil : 0,
    document_id: String(src.document_id || ''),
    verified: src.verified === true,
    verified_by: String(src.verified_by || ''),
    status: String(src.status || (src.verified === true ? 'valid' : 'unverified')),
    created_at_ms: Number(src.created_at_ms) || now,
    updated_at_ms: now,
    // _deleted ist reserviert (Kern-Tombstone, E47) und wird nie von der App geschrieben.
  };
}

// ---------------------------------------------------------------------------
// Mount (App-API 4.0): mount(root, api, { restored }) -> { unmount, snapshot }
// ---------------------------------------------------------------------------

const SNAPSHOT_VERSION = 1;
const FORM_FIELDS = {
  form: ['subject_id', 'credential_type', 'issuer', 'valid_until'],
  gate: ['subject_id', 'required_types'],
  signoff: ['record_id', 'charge_rate', 'entleiher_account_id', 'signature_request_id', 'collection'],
};

export async function mount(host, api, { restored } = {}) {
  if (!host.dataset.app) host.dataset.app = api.app?.id || MODULE_ID;
  setLocale(api, api.shell.locale.get());
  // CSS laedt die Shell aus dem Manifest; index.html liefert der Service Worker versioniert (§16.15).
  host.innerHTML = await loadMarkup();
  const disposers = [];
  const root = host.querySelector('[data-ats-root]');
  const t = (key) => text[key] || COPY.de[key] || key;

  const rail = root?.querySelector('.nachweise-rail');
  const listEl = root?.querySelector('[data-ats-list]');
  const detailEl = root?.querySelector('[data-ats-detail]');
  const formEl = root?.querySelector('[data-ats-form]');
  const gateFormEl = root?.querySelector('[data-ats-gate-form]');
  const signoffFormEl = root?.querySelector('[data-ats-signoff-form]');
  const titleEl = root?.querySelector('[data-ats-title]');
  const subEl = root?.querySelector('[data-ats-sub]');
  const toggleActionsEl = root?.querySelector('[data-toggle-actions]');
  const viewToggleEl = root?.querySelector('[data-ats-view-toggle]');
  const typeSelect = root?.querySelector('[data-credential-type]');
  const forms = { form: formEl, gate: gateFormEl, signoff: signoffFormEl };
  const restore = isPlainObject(restored) && restored.v === SNAPSHOT_VERSION ? restored : null;
  applyCopy();

  let rowsCache = [];
  let selectedId = restore && typeof restore.selectedId === 'string' ? restore.selectedId : null;
  let userCollapsed = Boolean(restore && restore.userCollapsed === true);
  let pendingScrollTop = restore && Number(restore.listScrollTop) > 0 ? Number(restore.listScrollTop) : 0;
  const nowMs = () => Date.now();
  if (restore) restoreView(restore);

  // Die Spalten-Grammatik verdrahtet die App selbst (keine DOM-Heuristik der Shell).
  const { autoWirePaneGrammar } = await import('ctox:shared/pane-grammar.js');
  autoWirePaneGrammar(root);

  // Zustand nur ueber die Zustandszone der Shell, stabile Codes je Fall.
  const report = (level, code, str, ref) => api.status.report(ref ? { level, code, text: str, ref } : { level, code, text: str });
  const clear = (code) => api.status.clear(code);

  function applyCopy() {
    root?.setAttribute('lang', locale);
    root?.querySelectorAll('[data-i18n]').forEach((node) => { node.textContent = t(node.dataset.i18n); });
    root?.querySelectorAll('[data-i18n-placeholder]').forEach((node) => { node.placeholder = t(node.dataset.i18nPlaceholder); });
    root?.querySelectorAll('[data-i18n-title]').forEach((node) => { node.title = node.ariaLabel = t(node.dataset.i18nTitle); });
    const select = root?.querySelector('[data-credential-type]');
    if (select) {
      const current = select.value;
      select.innerHTML = CREDENTIAL_TYPES
        .map((type) => '<option value="' + esc(type.key) + '">' + esc(typeLabel(type.key, locale)) + '</option>')
        .join('');
      if (current) select.value = current;
    }
    api.shell.setTitle(t('title'));
  }

  function restoreView(snap) {
    const search = rail?.querySelector('[data-pg-search]');
    if (search && typeof snap.search === 'string') search.value = snap.search;
    if (rail && (snap.view === 'list' || snap.view === 'cards')) rail.dataset.pgDefaultView = snap.view;
    const tabs = [...(rail?.querySelectorAll('[data-pg-band]') || [])];
    if (typeof snap.band === 'string' && tabs.some((tab) => tab.dataset.pgBand === snap.band)) {
      tabs.forEach((tab) => {
        const on = tab.dataset.pgBand === snap.band;
        tab.setAttribute('aria-selected', String(on));
        tab.classList.toggle('is-active', on);
      });
    }
    const statusSel = rail?.querySelector('[data-pg-filter][data-pg-name="status"]');
    if (statusSel && typeof snap.status === 'string' && [...statusSel.options].some((o) => o.value === snap.status)) statusSel.value = snap.status;
    const tray = rail?.querySelector('[data-pg-tray]');
    if (tray && snap.trayOpen === true) { tray.hidden = false; rail.querySelector('[data-pg-tray-toggle]')?.setAttribute('aria-expanded', 'true'); }
    if (snap.actionsOpen === true) {
      root?.classList.remove('is-actions-hidden');
      toggleActionsEl?.setAttribute('aria-pressed', 'true');
    }
    if (isPlainObject(snap.forms)) {
      for (const [key, names] of Object.entries(FORM_FIELDS)) {
        const values = snap.forms[key];
        if (!isPlainObject(values)) continue;
        for (const name of names) {
          const el = forms[key]?.querySelector('[name="' + name + '"]');
          if (el && typeof values[name] === 'string') el.value = values[name];
        }
      }
    }
  }

  function formValues() {
    const out = {};
    for (const [key, names] of Object.entries(FORM_FIELDS)) {
      out[key] = {};
      for (const name of names) out[key][name] = forms[key]?.querySelector('[name="' + name + '"]')?.value ?? '';
    }
    return out;
  }

  // Read the grammar state straight from the pane DOM. The view lives on the
  // pane as [data-pg-default-view] — the same attribute wirePaneGrammar() falls
  // back to, so declarative consumers stay in sync.
  function readGrammar() {
    return {
      search: (rail?.querySelector('[data-pg-search]')?.value || '').trim().toLowerCase(),
      view: rail?.dataset.pgDefaultView === 'list' ? 'list' : 'cards',
      band: rail?.querySelector('[data-pg-band][aria-selected="true"]')?.dataset.pgBand || 'all',
      status: rail?.querySelector('[data-pg-filter][data-pg-name="status"]')?.value || 'all',
    };
  }

  // One button, one action. The icon and the label always name the view the
  // click switches TO; the button carries no aria-pressed because it is not a
  // state control (Betreiber-Direktive 31.08.).
  function applyViewToggle() {
    if (!viewToggleEl) return;
    const next = readGrammar().view === 'list' ? 'cards' : 'list';
    const label = next === 'list' ? t('showAsList') : t('showAsCards');
    viewToggleEl.setAttribute('aria-label', label);
    viewToggleEl.title = label;
    viewToggleEl.removeAttribute('aria-pressed');
    viewToggleEl.querySelectorAll('[data-ats-view-icon]').forEach((icon) => {
      // toggleAttribute, not `.hidden`: these are SVG elements (no HTMLElement `hidden` IDL property).
      icon.toggleAttribute('hidden', icon.dataset.atsViewIcon !== next);
    });
  }

  function onViewToggle() {
    if (!rail) return;
    rail.dataset.pgDefaultView = readGrammar().view === 'list' ? 'cards' : 'list';
    applyViewToggle();
    // Reuse the grammar change channel: it clears the recorded scroll offsets (a
    // view switch is an intentional reset) and drives render() via onGrammarChange.
    try {
      rail.dispatchEvent(new CustomEvent('ctox-pane-grammar-change', { detail: readGrammar(), bubbles: true }));
    } catch { render(); }
  }

  // Counts/footer via the pane-grammar handle (wired above by the app itself).
  function writeCounts(counts) {
    const pg = rail?.__ctoxPaneGrammar;
    if (pg && typeof pg.setCounts === 'function') { pg.setCounts(counts); return; }
    for (const [key, value] of Object.entries(counts)) {
      const node = rail?.querySelector('[data-pg-count="' + key + '"]');
      if (node) node.textContent = ' (' + value + ')';
    }
  }
  function writeFooter(str) {
    const pg = rail?.__ctoxPaneGrammar;
    if (pg && typeof pg.setFooter === 'function') { pg.setFooter(str); return; }
    const node = rail?.querySelector('[data-pg-footer]');
    if (node) node.textContent = str || '';
  }

  function renderDetail() {
    if (!detailEl) return;
    const now = nowMs();
    const rec = selectedId ? rowsCache.find((r) => r.id === selectedId) : null;
    const show = shouldRevealRecord(Boolean(rec), userCollapsed);
    detailEl.hidden = !show;
    detailEl.innerHTML = show ? recordDetailHtml(rec, now) : '';
    if (show) {
      detailEl.setAttribute('data-context-record-id', rec.id || '');
      detailEl.setAttribute('data-context-record-type', 'nachweis');
      detailEl.setAttribute('data-context-label', typeLabel(rec.credential_type, locale) + ' · ' + refName(rec, 'subject', locale));
    }
    if (titleEl) titleEl.textContent = rec ? (typeLabel(rec.credential_type, locale) + ' · ' + refName(rec, 'subject', locale)) : text.newTitle;
  }

  function render() {
    const g = readGrammar();
    const now = nowMs();
    const filtered = filterRows(rowsCache, g, now);
    if (listEl) listEl.innerHTML = renderRecordList(filtered, { view: g.view, selectedId, nowMs: now });
    writeCounts(countsFor(rowsCache, now));
    const scope = { all: text.bandAll, valid: text.bandValid, expiring: text.bandExpiring, critical: text.bandCritical }[g.band] || text.bandAll;
    writeFooter(filtered.length + ' ' + text.entries + ' · ' + scope);
    renderDetail();
    const scroller = listEl?.closest('.ctox-pane-body');
    if (pendingScrollTop && scroller && rowsCache.length) { scroller.scrollTop = pendingScrollTop; pendingScrollTop = 0; }
  }

  function setRows(rows) {
    const list = (Array.isArray(rows) ? rows : []).filter((r) => r && !r._deleted);
    list.sort((a, b) => (Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0)));
    rowsCache = list;
  }

  function fillForm(rec) {
    if (!formEl) return;
    setField('subject_id', rec.subject_id || '');
    setField('credential_type', rec.credential_type || '');
    setField('issuer', rec.issuer || '');
    const until = Number(rec.valid_until_ms);
    setField('valid_until', Number.isFinite(until) && until > 0 ? new Date(until).toISOString().slice(0, 10) : '');
  }
  function setField(name, value) {
    const el = formEl?.querySelector('[name="' + name + '"]');
    if (el) el.value = value == null ? '' : String(value);
  }

  function selectRecord(id) {
    selectedId = id || null;
    userCollapsed = false;
    const rec = selectedId ? rowsCache.find((r) => r.id === selectedId) : null;
    if (rec) fillForm(rec);
    // Selection is an in-place class flip — a list rebuild resets the
    // operator's scroll (design-guide: re-renders never move the operator).
    applyListSelection();
    renderDetail();
  }

  function applyListSelection() {
    listEl?.querySelectorAll('[data-context-record-id]').forEach((row) => {
      const on = (row.getAttribute('data-context-record-id') || '') === String(selectedId || '');
      row.classList.toggle('is-selected', on);
      row.setAttribute('aria-selected', String(on));
    });
  }

  function startNew() {
    selectedId = null;
    userCollapsed = false;
    try { formEl?.reset(); } catch {}
    render();
    formEl?.querySelector('[name="subject_id"]')?.focus?.();
  }

  function exportRecords() {
    const filtered = filterRows(rowsCache, readGrammar(), nowMs());
    let url = '';
    try {
      const blob = new Blob([JSON.stringify(filtered, null, 2)], { type: 'application/json' });
      url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = 'nachweise-credentials.json';
      a.rel = 'noopener';
      host.appendChild(a);
      a.click();
      a.remove();
    } catch (e) {
      console.error('[nachweise] export failed:', e);
    } finally {
      if (url) setTimeout(() => { try { URL.revokeObjectURL(url); } catch {} }, 4000);
    }
  }

  // Import: jede Zeile ein lokaler Write (api.put), sichtbar ueber das Live-Abo.
  async function importFile(file) {
    let parsed;
    try { parsed = JSON.parse(await file.text()); } catch { report('error', 'nachweise.import.invalid', text.importInvalid); return; }
    clear('nachweise.import.invalid');
    const items = Array.isArray(parsed) ? parsed : (parsed && typeof parsed === 'object' ? [parsed] : []);
    if (!items.length) { report('warn', 'nachweise.import.empty', text.importEmpty); return; }
    clear('nachweise.import.empty');
    const stamp = nowMs();
    let count = 0;
    for (const raw of items) {
      try { await api.put(PRIMARY, prepareImport(raw, stamp, String(count))).local; count += 1; }
      catch (e) { console.error('[nachweise] import failed:', e); }
    }
    if (count < items.length) report('warn', 'nachweise.import.partial', text.imported + ': ' + count + '/' + items.length);
    else clear('nachweise.import.partial');
  }

  function importRecords() {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'application/json,.json';
    input.addEventListener('change', () => {
      const file = input.files && input.files[0];
      if (file) importFile(file);
    });
    input.click();
  }

  // Befehl: api.command legt das Befehlsdokument lokal an und liefert sofort; die UI wartet nie.
  // Fehlschlag → Zone (error). Ergebnis (done) wertet onDone aus.
  function sendCommand(type, payload, onDone) {
    const handle = api.command(type, payload);
    const code = 'nachweise.command.' + type;
    let off = null;
    let finished = false;
    const stop = () => { finished = true; off?.(); off = null; };
    off = handle.status.subscribe((s) => {
      if (s?.status === 'failed') {
        report('error', code, type + ': ' + (s.error || 'failed'), handle.id);
        stop();
      } else if (s?.status === 'done') {
        clear(code);
        try { onDone?.(s.result, handle); } catch (e) { console.error('[nachweise] result failed:', e); }
        stop();
      }
    });
    if (finished) off?.();
    handle.local.catch((e) => report('error', code, type + ': ' + (e?.code || 'local') + ' ' + (e?.message || e), handle.id));
    disposers.push(stop);
    return handle;
  }

  // Server-authoritative deployment-readiness gate (ats.deployment.check). Das Ergebnis ist die
  // Antwort auf die Pruefung und hat sonst keine sichtbare Wirkung → Zustandszone (info/warn).
  function runDeploymentCheck(subjectId, requiredTypes) {
    sendCommand(DEPLOY_COMMAND, { subject_id: subjectId, required_types: requiredTypes }, (raw) => {
      const result = raw?.result || raw;
      const blockers = result?.blockers || [];
      const ready = result?.ready === true && !(Array.isArray(blockers) && blockers.length);
      if (!ready) {
        const items = (Array.isArray(blockers) ? blockers : [blockers])
          .filter(Boolean)
          .map((b) => typeLabel(b?.credential_type, locale) + ' — ' + (b?.reason || t('blockedReason')));
        report('warn', 'nachweise.deployment.check', '✗ ' + t('notReady') + ': ' + subjectId + (items.length ? ' · ' + items.join(' · ') : ''), subjectId);
        return;
      }
      report('info', 'nachweise.deployment.check', '✓ ' + t('ready') + ': ' + subjectId, subjectId);
    });
  }

  // Create a credential (plain collection write — no native command for capture).
  // The insert flow is unchanged: submit always creates a fresh record; select
  // pre-fills the form as a starting point, "Neu" clears it.
  function onSubmit(event) {
    event.preventDefault();
    const data = new FormData(formEl);
    const f = Object.fromEntries(data.entries());
    const subjectId = String(f.subject_id || '').trim();
    if (!subjectId) { report('warn', 'nachweise.form.subject_required', t('subjectRequired')); return; }
    clear('nachweise.form.subject_required');
    const validUntil = String(f.valid_until || '');
    const now = Date.now();
    const record = {
      id: 'cred_' + now + '_' + Math.round(now % 1e6),
      subject_id: subjectId,
      subject_type: 'candidate',
      credential_type: String(f.credential_type || ''),
      issuer: String(f.issuer || '').trim(),
      valid_until_ms: validUntil ? Date.parse(validUntil) : 0,
      verified: false,
      status: 'unverified',
      created_at_ms: now,
      updated_at_ms: now,
    };
    const handle = api.put(PRIMARY, record);
    handle.local.then(() => {
      clear('nachweise.credential.save_failed');
      selectedId = null;
      try { formEl.reset(); } catch {}
      render();
    }, (e) => {
      console.error('[nachweise] insert failed:', e);
      report('error', 'nachweise.credential.save_failed', t('saveFailed') + ' ' + (e?.code || '') + ' ' + (e?.message || ''));
    });
  }

  function onGateCheck(event) {
    event.preventDefault();
    const data = new FormData(gateFormEl);
    const subjectId = String(data.get('subject_id') || '').trim();
    if (!subjectId) { report('warn', 'nachweise.form.subject_required', t('subjectRequired')); return; }
    clear('nachweise.form.subject_required');
    const requiredTypes = String(data.get('required_types') || '')
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean);
    runDeploymentCheck(subjectId, requiredTypes);
  }

  // Entleiher sign-off → billing release (ats.leistungsnachweis.signoff).
  function onSignoff(event) {
    event.preventDefault();
    const data = new FormData(signoffFormEl);
    const recordId = String(data.get('record_id') || '').trim();
    if (!recordId) { report('warn', 'nachweise.form.record_required', t('recordRequired')); return; }
    clear('nachweise.form.record_required');
    const collectionName = String(data.get('collection') || '').trim();
    const payload = { record_id: recordId };
    if (collectionName) payload.collection = collectionName;
    // Billing-critical fields the signoff handler reads (store_ats_commands.rs):
    // charge_rate (Verrechnungssatz, must be a finite number > 0 unless already
    // on the record), entleiher_account_id (invoice recipient), and
    // signature_request_id (only consulted when Entleiher-signature is enforced).
    const chargeRateRaw = String(data.get('charge_rate') || '').trim();
    if (chargeRateRaw !== '') {
      const chargeRate = Number(chargeRateRaw);
      if (Number.isFinite(chargeRate)) payload.charge_rate = chargeRate;
    }
    const entleiherId = String(data.get('entleiher_account_id') || '').trim();
    if (entleiherId) payload.entleiher_account_id = entleiherId;
    const signatureRequestId = String(data.get('signature_request_id') || '').trim();
    if (signatureRequestId) payload.signature_request_id = signatureRequestId;
    const submitted = JSON.stringify(formValues().signoff);
    sendCommand(SIGNOFF_COMMAND, payload, (raw) => {
      const result = raw?.result || raw;
      const blockers = result?.blockers || result?.errors || null;
      const blocked = result?.ok === false || result?.status === 'blocked'
        || (Array.isArray(blockers) && blockers.length > 0);
      if (blocked) {
        const items = (Array.isArray(blockers) ? blockers : [blockers])
          .filter(Boolean)
          .map((b) => (typeof b === 'string' ? b : (b?.message || b?.reason || JSON.stringify(b))));
        report('warn', 'nachweise.signoff.result', t('signoffBlocked') + (items.length ? ' ' + items.join(' · ') : ''), recordId);
        return;
      }
      const signedId = result?.record_id ?? recordId;
      const released = result?.billing_released === true;
      const invoiceId = result?.invoice_id ? String(result.invoice_id) : '';
      const netTotal = result?.net_total;
      report('info', 'nachweise.signoff.result', [
        t('signoffSaved'),
        t('performanceRecord') + ': ' + signedId,
        t('billingReleased') + ': ' + (released ? t('yes') : t('no')),
        invoiceId ? t('invoice') + ': ' + invoiceId : '',
        netTotal != null ? t('net') + ': ' + String(netTotal) + ' €' : '',
      ].filter(Boolean).join(' · '), String(signedId));
      // Formular nur leeren, wenn seit dem Absenden nichts Neues eingegeben wurde.
      if (JSON.stringify(formValues().signoff) === submitted) { try { signoffFormEl.reset(); } catch {} }
    });
  }

  // Row selection → fill form + auto-reveal detail (design-guide outbound idiom).
  function onListClick(event) {
    const row = event.target?.closest?.('[data-context-record-id]');
    if (!row || !listEl.contains(row)) return;
    selectRecord(row.getAttribute('data-context-record-id'));
  }
  function onListKey(event) {
    if (event.key !== 'Enter' && event.key !== ' ') return;
    const row = event.target?.closest?.('[data-context-record-id]');
    if (!row || !listEl.contains(row)) return;
    event.preventDefault();
    selectRecord(row.getAttribute('data-context-record-id'));
  }

  // Header + detail icon actions (create / import / export / collapse / per-record deploy check).
  function onAction(event) {
    const btn = event.target?.closest?.('[data-action]');
    if (!btn || !root?.contains(btn)) return;
    const action = btn.dataset.action;
    if (action === 'new') startNew();
    else if (action === 'import') importRecords();
    else if (action === 'export') exportRecords();
    else if (action === 'collapse-detail') { userCollapsed = true; renderDetail(); }
    else if (action === 'deploy-check') {
      const rec = selectedId ? rowsCache.find((r) => r.id === selectedId) : null;
      if (rec?.subject_id) runDeploymentCheck(rec.subject_id, []);
    }
  }

  // Einsatz-Gate and Sign-off forms are hidden by default (is-actions-hidden on
  // root); the header toggle reveals them on demand — threads' / consent's idiom.
  function onToggleActions() {
    const hidden = root?.classList.toggle('is-actions-hidden');
    toggleActionsEl?.setAttribute('aria-pressed', hidden ? 'false' : 'true');
  }

  formEl?.addEventListener('submit', onSubmit);
  gateFormEl?.addEventListener('submit', onGateCheck);
  signoffFormEl?.addEventListener('submit', onSignoff);
  listEl?.addEventListener('click', onListClick);
  listEl?.addEventListener('keydown', onListKey);
  root?.addEventListener('click', onAction);
  toggleActionsEl?.addEventListener('click', onToggleActions);
  viewToggleEl?.addEventListener('click', onViewToggle);
  applyViewToggle();
  // Re-render when the pane grammar reports a change (search / view / tray / band).
  const onGrammarChange = () => { render(); };
  rail?.addEventListener('ctox-pane-grammar-change', onGrammarChange);

  disposers.push(api.query(PRIMARY, {}).subscribe((rows) => { setRows(rows); render(); }));
  disposers.push(api.shell.locale.subscribe((next) => {
    if (!setLocale(api, next)) return;
    applyCopy();
    applyViewToggle();
    render();
  }));

  return {
    unmount() {
      for (const dispose of disposers.splice(0)) {
        try { dispose(); } catch (e) { console.warn('[nachweise] unsubscribe failed', e); }
      }
      formEl?.removeEventListener('submit', onSubmit);
      gateFormEl?.removeEventListener('submit', onGateCheck);
      signoffFormEl?.removeEventListener('submit', onSignoff);
      toggleActionsEl?.removeEventListener('click', onToggleActions);
      viewToggleEl?.removeEventListener('click', onViewToggle);
      listEl?.removeEventListener('click', onListClick);
      listEl?.removeEventListener('keydown', onListKey);
      root?.removeEventListener('click', onAction);
      rail?.removeEventListener('ctox-pane-grammar-change', onGrammarChange);
      host.replaceChildren();
    },
    snapshot() {
      const g = readGrammar();
      return {
        v: SNAPSHOT_VERSION,
        selectedId,
        userCollapsed,
        search: rail?.querySelector('[data-pg-search]')?.value || '',
        view: g.view,
        band: g.band,
        status: g.status,
        trayOpen: rail?.querySelector('[data-pg-tray]')?.hidden === false,
        actionsOpen: Boolean(root && !root.classList.contains('is-actions-hidden')),
        forms: formValues(),
        listScrollTop: listEl?.closest('.ctox-pane-body')?.scrollTop || 0,
      };
    },
  };
}

function isPlainObject(v) { return Boolean(v) && typeof v === 'object' && !Array.isArray(v); }

// Sprache: api.shell.locale (live) + api.shell.t('nachweise.<key>') uebersteuert die eingebetteten Texte.
function setLocale(api, value) {
  const next = String(value || '').toLowerCase().startsWith('en') ? 'en' : 'de';
  const changed = next !== locale || text === COPY[locale];
  locale = next;
  const base = COPY[next];
  const out = {};
  for (const key of Object.keys(base)) {
    const k = MODULE_ID + '.' + key;
    let v = null;
    try { v = api?.shell?.t?.(k); } catch {}
    out[key] = typeof v === 'string' && v && v !== k ? v : base[key];
  }
  text = out;
  return changed;
}

// Credential status → kit badge state (base.css .ctox-badge modifiers).
function badgeStateClass(status) {
  switch (status) {
    case 'valid':
      return ' is-success';
    case 'expiring':
      return ' is-warning';
    case 'expired':
      return ' is-danger';
    case 'not_yet_valid':
      return ' is-info';
    default:
      return ''; // unverified & unknown states stay neutral
  }
}

function typeLabel(key, loc = 'de') {
  return (loc === 'en' ? TYPE_LABEL_EN.get(key) : TYPE_LABEL.get(key)) || key || COPY[loc]?.credential || COPY.de.credential;
}

async function loadMarkup() {
  const html = await fetch(new URL('./index.html', import.meta.url)).then((r) => r.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((n) => n.remove());
  return doc.body.innerHTML;
}

function esc(v) {
  return String(v == null ? '' : v)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}
