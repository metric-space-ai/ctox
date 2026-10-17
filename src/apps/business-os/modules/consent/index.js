import { autoWirePaneGrammar } from 'ctox:shared/pane-grammar.js';


const MODULE_ID = 'consent';
const PRIMARY = 'business_consents';
const CHECK_COMMAND = 'ats.consent.check';
const EXPORT_COMMAND = 'ats.subject.export';
const ERASE_COMMAND = 'ats.subject.erase';
const COPY = {
  de: {
    title: 'Einwilligungen', listTitle: 'Einwilligungen', newTitle: 'Neue Prüfung',
    newAction: 'Neue Prüfung', importAction: 'Importieren', exportAction: 'Exportieren',
    searchPlaceholder: 'Suchen...', closeDetail: 'Details schließen',
    bandAll: 'Alle', bandValid: 'Gültig', bandOpen: 'Offen', bandEnded: 'Erloschen',
    subjectLabel: 'Person', purposeLabel: 'Zweck', rightsLabel: 'Betroffenenrechte', subjectPlaceholder: 'Kandidaten-ID', purposePlaceholder: 'optional',
    checkConsent: 'Einwilligung prüfen', rightsSubjectPlaceholder: 'Kandidaten-ID',
    exportArticle15: 'Auskunft (Art. 15)', eraseArticle17: 'Löschen (Art. 17)',
    exportTitle: 'Recht auf Auskunft (DSGVO Art. 15)', eraseTitle: 'Recht auf Löschung (DSGVO Art. 17)',
    entries: 'Einträge',
    statusAll: 'Alle Status',
    dataLocked: 'Datenzugriff gesperrt.',
    dataLockedHint: 'Einwilligungsdaten im App Store freigeben.',
    eraseConfirmTitle: 'Daten endgültig löschen?', eraseConfirmText: 'Alle gespeicherten Daten zu „{subject}“ werden app-übergreifend gelöscht. Das lässt sich nicht rückgängig machen.', eraseConfirm: 'Endgültig löschen',
    basisConsent: 'Einwilligung (Art. 6 Abs. 1 a)', basisContract: 'Vertrag (Art. 6 Abs. 1 b)', basisLegal: 'Rechtliche Pflicht (Art. 6 Abs. 1 c)', basisInterest: 'Berechtigtes Interesse (Art. 6 Abs. 1 f)', unlimited: 'unbefristet',
    imported: 'Importiert', importInvalid: 'Ungültige JSON-Datei.', importEmpty: 'Keine Datensätze in der Datei.',
    subjectRequired: 'Kandidaten-ID erforderlich.',
    checkFailed: 'Prüfung fehlgeschlagen.', consentPresent: 'Einwilligung vorhanden.', consentMissing: 'Keine gültige Einwilligung.',
    subject: 'Person', purpose: 'Zweck', existenceOnly: '—', decision: 'Entscheidung',
    allowed: 'erlaubt', denied: 'verweigert', blocked: 'blockiert', executed: 'ausgeführt',
    deletedRecords: 'Gelöschte Datensätze', affectedRecords: 'Betroffene Datensätze', auditEntries: 'Audit-Einträge',
    exportLabel: 'Auskunft (Art. 15)', valid: 'gültig', withdrawn: 'widerrufen', expired: 'abgelaufen', open: 'offen',
    legalBasis: 'Rechtsgrundlage', validUntil: 'gültig bis', granted: 'erteilt', exportShort: 'Auskunft', eraseShort: 'Löschen',
    viewToList: 'Als Liste anzeigen', viewToCards: 'Als Karten anzeigen',
  },
  en: {
    title: 'Consent', listTitle: 'Consent', newTitle: 'New check',
    newAction: 'New check', importAction: 'Import', exportAction: 'Export',
    searchPlaceholder: 'Search...', closeDetail: 'Close details',
    bandAll: 'All', bandValid: 'Valid', bandOpen: 'Pending', bandEnded: 'Ended',
    subjectLabel: 'Person', purposeLabel: 'Purpose', rightsLabel: 'Data-subject rights', subjectPlaceholder: 'Candidate ID', purposePlaceholder: 'optional',
    checkConsent: 'Check consent', rightsSubjectPlaceholder: 'Subject ID',
    exportArticle15: 'Access request (Art. 15)', eraseArticle17: 'Erase (Art. 17)',
    exportTitle: 'Right of access (GDPR Art. 15)', eraseTitle: 'Right to erasure (GDPR Art. 17)',
    entries: 'entries',
    statusAll: 'All statuses',
    dataLocked: 'Data access is locked.',
    dataLockedHint: 'Grant consent data in the App Store.',
    eraseConfirmTitle: 'Erase data permanently?', eraseConfirmText: 'All stored data for “{subject}” will be erased across apps. This cannot be undone.', eraseConfirm: 'Erase permanently',
    basisConsent: 'Consent (Art. 6(1)(a))', basisContract: 'Contract (Art. 6(1)(b))', basisLegal: 'Legal obligation (Art. 6(1)(c))', basisInterest: 'Legitimate interest (Art. 6(1)(f))', unlimited: 'unlimited',
    imported: 'Imported', importInvalid: 'Invalid JSON file.', importEmpty: 'No records in the file.',
    subjectRequired: 'Subject ID is required.',
    checkFailed: 'Consent check failed.', consentPresent: 'Valid consent exists.', consentMissing: 'No valid consent.',
    subject: 'Person', purpose: 'Purpose', existenceOnly: '—', decision: 'Decision',
    allowed: 'allowed', denied: 'denied', blocked: 'blocked', executed: 'completed',
    deletedRecords: 'Deleted records', affectedRecords: 'Affected records', auditEntries: 'Audit entries',
    exportLabel: 'Access request (Art. 15)', valid: 'valid', withdrawn: 'withdrawn', expired: 'expired', open: 'pending',
    legalBasis: 'Legal basis', validUntil: 'valid until', granted: 'granted', exportShort: 'Access', eraseShort: 'Erase',
    viewToList: 'Show as list', viewToCards: 'Show as cards',
  },
};
let text = COPY.de;
let locale = 'de';

// Ein-Knopf-Umschalter: das Icon zeigt IMMER die Ansicht, zu der der Klick
// wechselt (Kacheln <-> Liste), nie die aktuelle.
const VIEW_ICON = {
  cards: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="4" y="4" width="16" height="7" rx="1.5"/><rect x="4" y="14" width="16" height="7" rx="1.5"/></svg>',
  list: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="4" y1="6" x2="20" y2="6"/><line x1="4" y1="12" x2="20" y2="12"/><line x1="4" y1="18" x2="20" y2="18"/></svg>',
};
const VIEW_ACTION_KEY = { cards: 'viewToCards', list: 'viewToList' };

// ---------------------------------------------------------------------------
// Pure record helpers (exported for tests — no DOM, no RxDB).
// ---------------------------------------------------------------------------

// Auto-reveal model (design-guide "Progressive Disclosure", outbound idiom):
// the record detail is shown only when something is selected and the user has
// not collapsed it.
export function shouldRevealRecord(hasSelection, userCollapsed) {
  return Boolean(hasSelection) && !userCollapsed;
}

// Derived live status of a consent record — mirrors core/consent.js
// isConsentValid: withdrawn > expired > pending/active.
export function consentStatus(r, nowMs) {
  const now = Number(nowMs) || 0;
  const record = r && typeof r === 'object' ? r : {};
  if (Number.isFinite(Number(record.withdrawn_at_ms)) && Number(record.withdrawn_at_ms) > 0 && Number(record.withdrawn_at_ms) <= now) return 'withdrawn';
  if (Number.isFinite(Number(record.expires_at_ms)) && Number(record.expires_at_ms) > 0 && Number(record.expires_at_ms) <= now) return 'expired';
  const basis = String(record.legal_basis || '');
  const consentBasis = basis === '' || basis === 'consent' || basis === 'special_category_consent';
  if (consentBasis && !(Number.isFinite(Number(record.granted_at_ms)) && Number(record.granted_at_ms) > 0)) return 'pending';
  return 'active';
}

export function statusOf(r, nowMs) {
  return consentStatus(r, nowMs);
}

// Which counted band a derived status belongs to. The left column's band splits
// Alle / Gültig (active) / Offen (pending) / Erloschen (withdrawn or expired).
export function consentBand(status) {
  const st = String(status || 'pending');
  if (st === 'active') return 'valid';
  if (st === 'withdrawn' || st === 'expired') return 'ended';
  return 'open'; // 'pending'
}

export function statusLabel(status) {
  const key = { active: 'valid', pending: 'open', withdrawn: 'withdrawn', expired: 'expired' }[status];
  return (key && text[key]) || String(status || '');
}

export function countsFor(rows, nowMs) {
  const list = Array.isArray(rows) ? rows : [];
  const counts = { all: list.length, valid: 0, open: 0, ended: 0 };
  for (const r of list) counts[consentBand(statusOf(r, nowMs))] += 1;
  return counts;
}

// Apply the current grammar state (band + status filter + search) to the rows.
export function filterRows(rows, { band = 'all', status = 'all', search = '' } = {}, nowMs) {
  const needle = String(search || '').trim().toLowerCase();
  return (Array.isArray(rows) ? rows : []).filter((r) => {
    const st = statusOf(r, nowMs);
    if (band && band !== 'all' && consentBand(st) !== band) return false;
    if (status && status !== 'all' && st !== status) return false;
    if (needle) {
      const hay = [r.subject_id, r.purpose, r.legal_basis, st, r.id]
        .filter(Boolean).join(' ').toLowerCase();
      if (!hay.includes(needle)) return false;
    }
    return true;
  });
}

// Beide Ansichten sind reine Selektoren (kein Inline-Aufklappen, keine
// Zeilenknoepfe — design-guide "Canonical Column Grammar"), unterscheiden sich
// aber in der Informationsdichte (Betreiber-Direktive 31.08.):
//
//   KARTE  drei Zeilen — fetter Titel + Status, darunter Zuordnung
//          (Subjekt · Rechtsgrundlage) und die Fristen (erteilt · gueltig bis
//          [· widerrufen]); grosszuegiges Padding.
//   LISTE  genau EINE Zeile — Titel und rechts ein einziges Kurz-Meta (der
//          Status); enge Zeilenhoehe, maximale Dichte.
//
// Beide lesen ausschliesslich Felder, die die App ohnehin laedt.
export function consentRow(r, opts = {}) {
  const view = opts.view === 'list' ? 'list' : 'cards';
  const selected = Boolean(opts.selected);
  const now = Number(opts.nowMs) || 0;
  const status = statusOf(r, now);
  const subject = refName(r, 'subject', locale);
  const title = r.purpose || subject;
  const badge = '<span class="ctox-badge' + badgeStateClass(status) + '" data-status="' + esc(status) + '">' + esc(statusLabel(status)) + '</span>';
  const id = r.id || '';
  const attrs = ' class="ctox-list-item consent-row consent-row--' + view + (selected ? ' is-selected' : '') + '"'
    + ' role="button" tabindex="0" aria-selected="' + (selected ? 'true' : 'false') + '"'
    + ' data-context-record-id="' + esc(id) + '"'
    + ' data-context-record-type="consent"'
    + ' data-context-label="' + esc((title + ' · ' + subject) || id) + '"';
  if (view === 'list') {
    return '<div' + attrs + '>'
      + '<span class="consent-row-title">' + esc(title) + '</span>'
      + '<span class="consent-row-side">' + badge + '</span>'
      + '</div>';
  }
  const assignment = [text.subject + ': ' + subject, text.legalBasis + ': ' + legalBasisLabel(r.legal_basis)];
  const dates = [text.granted + ': ' + fmtDate(r.granted_at_ms), text.validUntil + ': ' + fmtValidUntil(r.expires_at_ms)];
  if (Number.isFinite(Number(r.withdrawn_at_ms)) && Number(r.withdrawn_at_ms) > 0) {
    dates.push(text.withdrawn + ': ' + fmtDate(r.withdrawn_at_ms));
  }
  return '<div' + attrs + '>'
    + '<div class="consent-row-head"><span class="consent-row-title">' + esc(title) + '</span>' + badge + '</div>'
    + '<div class="consent-row-meta">' + esc(assignment.join(' · ')) + '</div>'
    + '<div class="consent-row-meta consent-row-meta--dates">' + esc(dates.join(' · ')) + '</div>'
    + '</div>';
}

// The list body markup (shards or compact rows). App-API 4.0 (R11): eine leere Liste rendert nichts – kein
// Lade-, Sync- oder Leertext; Zustände gehen nur in die Zustandszone der Shell.
export function renderRecordList(rows, opts = {}) {
  const list = Array.isArray(rows) ? rows : [];
  if (!list.length) return '';
  return list.map((r) => consentRow(r, { view: opts.view, nowMs: opts.nowMs, selected: r.id && r.id === opts.selectedId })).join('');
}

// Read-only detail card for the selected consent record (auto-reveal target).
export function recordDetailHtml(r, nowMs) {
  const now = Number(nowMs) || 0;
  const status = statusOf(r, now);
  const subject = refName(r, 'subject', locale);
  const title = r.purpose || subject;
  const rows = [];
  rows.push(field(text.subject, esc(subject)));
  rows.push(field(text.purpose, esc(r.purpose || text.existenceOnly)));
  rows.push(field(text.legalBasis, esc(legalBasisLabel(r.legal_basis))));
  rows.push(field(text.validUntil, esc(fmtValidUntil(r.expires_at_ms))));
  rows.push(field(text.granted, esc(fmtDate(r.granted_at_ms))));
  if (Number.isFinite(Number(r.withdrawn_at_ms)) && Number(r.withdrawn_at_ms) > 0) {
    rows.push(field(text.withdrawn, esc(fmtDate(r.withdrawn_at_ms))));
  }
  const rightsIcon = r.subject_id
    ? '<button type="button" class="ctox-pane-icon" data-short-de="Export" data-action="rights-export" aria-label="' + esc(text.exportTitle) + '" title="' + esc(text.exportTitle) + '"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3v12M12 15l-4-4M12 15l4-4M5 21h14"/></svg></button>'
      + '<button type="button" class="ctox-pane-icon" data-short-de="Löschen" data-action="rights-erase" aria-label="' + esc(text.eraseTitle) + '" title="' + esc(text.eraseTitle) + '"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 7h16M9 7V5h6v2M6 7l1 13h10l1-13"/></svg></button>'
    : '';
  return '<header class="consent-detail-head">'
    + '<div class="consent-detail-titles">'
    + '<span class="ctox-badge' + badgeStateClass(status) + '" data-status="' + esc(status) + '">' + esc(statusLabel(status)) + '</span>'
    + '<strong class="consent-detail-name">' + esc(title) + '</strong>'
    + '</div>'
    + '<div class="consent-detail-actions">'
    + rightsIcon
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

// Prepare an imported consent JSON record for upsert, preserving id/timestamps
// from an exported record (round-trip friendly). Fills the schema-required
// fields (id, subject_id, purpose, updated_at_ms).
export function prepareImport(raw, nowMs, salt = '') {
  const src = raw && typeof raw === 'object' ? raw : {};
  const now = Number(nowMs) || 0;
  const id = String(src.id || ('consent_' + now + '_' + salt));
  const num = (v) => (Number.isFinite(Number(v)) ? Number(v) : 0);
  return {
    id,
    subject_id: String(src.subject_id || ''),
    subject_type: String(src.subject_type || ''),
    purpose: String(src.purpose || ''),
    legal_basis: String(src.legal_basis || ''),
    granted_at_ms: num(src.granted_at_ms),
    withdrawn_at_ms: num(src.withdrawn_at_ms),
    expires_at_ms: num(src.expires_at_ms),
    source: String(src.source || 'import'),
    created_at_ms: num(src.created_at_ms) || now,
    updated_at_ms: now,
    // _deleted ist reserviert (Kern-Tombstone, E47) und wird nie von der App geschrieben.
  };
}

// ---------------------------------------------------------------------------
// Mount
// ---------------------------------------------------------------------------

function langFrom(value) {
  return String(value || '').toLowerCase().startsWith('en') ? 'en' : 'de';
}

// App-API 4.0: mount(host, api, {restored}) → {unmount, snapshot}. Daten nur über api.query/put, Befehle über
// api.command (nie warten), Zustände nur in die Zustandszone (api.status.report).
export async function mount(host, api, { restored } = {}) {
  locale = langFrom(api.shell.locale.get());
  text = COPY[locale];
  if (!host.dataset.app) host.dataset.app = api.app?.id || MODULE_ID;
  host.innerHTML = await loadMarkup();
  host.dataset.atsModule = MODULE_ID;
  const disposers = [];

  const root = host.querySelector('[data-ats-root]');
  root?.setAttribute('lang', locale);
  // api.shell.t übersteuert die eingebetteten Texte (Schlüsselraum offen, app-api.d.ts OFFEN-B).
  const t = (key) => {
    const scoped = MODULE_ID + '.' + key;
    const value = api.shell.t?.(scoped);
    if (typeof value === 'string' && value && value !== scoped) return value;
    return text[key] || COPY.de[key] || key;
  };
  const applyLabels = () => {
    root?.querySelectorAll('[data-i18n]').forEach((node) => { node.textContent = t(node.dataset.i18n); });
    root?.querySelectorAll('[data-i18n-placeholder]').forEach((node) => { node.placeholder = t(node.dataset.i18nPlaceholder); });
    root?.querySelectorAll('[data-i18n-title]').forEach((node) => { node.title = node.ariaLabel = t(node.dataset.i18nTitle); });
  };
  applyLabels();
  // Pane-Grammatik verdrahtet die App selbst (PORTING.md R14), keine DOM-Heuristik der Shell.
  autoWirePaneGrammar(host);
  api.shell.setTitle(t('title'));

  const rail = root?.querySelector('.consent-rail');
  const listEl = root?.querySelector('[data-ats-list]');
  const detailEl = root?.querySelector('[data-ats-detail]');
  const formEl = root?.querySelector('[data-ats-form]');
  const subjectFormEl = root?.querySelector('[data-ats-subject-form]');
  const gateEl = root?.querySelector('[data-ats-gate]');
  const titleEl = root?.querySelector('[data-ats-title]');
  const toggleRightsEl = root?.querySelector('[data-toggle-rights]');
  const viewToggleEl = rail?.querySelector('.consent-view-toggle');
  if (titleEl) titleEl.textContent = t('title');

  let rowsCache = [];
  let selectedId = null;
  let userCollapsed = false;
  const nowMs = () => Date.now();
  // Zustand in die Zustandszone (stabile codes je Fall, damit clear greift).
  const report = (code, key, level = 'warn', extra = '') => api.status.report({ level, code: MODULE_ID + '.' + code, text: t(key) + (extra ? ': ' + extra : '') });
  const clear = (code) => api.status.clear(MODULE_ID + '.' + code);

  // Gate callout → kit .ctox-callout variants (base.css).
  // Ergebnis der Prüfung/Auskunft steht als neutrale Karte am Arbeitsbereich (kein farbiger Hinweiskasten);
  // Sperren/Fehler gehen in die Zustandszone (Textdiät Regel 1).
  function setGate(html, kind) {
    if (!gateEl) return;
    if (kind === 'block' || kind === 'offline') {
      const tmp = document.createElement('div');
      tmp.innerHTML = html || '';
      api.status.report({ level: 'error', code: MODULE_ID + '.gate.blocked', text: tmp.textContent.replace(/\s+/g, ' ').trim() });
      gateEl.innerHTML = '';
      gateEl.hidden = true;
      return;
    }
    if (html) api.status.clear(MODULE_ID + '.gate.blocked');
    gateEl.className = 'ctox-card consent-result';
    gateEl.innerHTML = html || '';
    gateEl.hidden = !html;
  }

  // Read the SHELL-wired grammar state straight from the pane DOM. Die Ansicht
  // steht auf dem EINEN Umschalter (data-pg-view = aktuelle Ansicht), nicht auf
  // einem gedrueckten Knopf eines Paares.
  function readGrammar() {
    return {
      search: (rail?.querySelector('[data-pg-search]')?.value || '').trim().toLowerCase(),
      view: rail?.querySelector('[data-pg-view]')?.dataset.pgView === 'list' ? 'list' : 'cards',
      band: rail?.querySelector('[data-pg-band][aria-selected="true"]')?.dataset.pgBand || 'all',
      status: rail?.querySelector('[data-pg-filter][data-pg-name="status"]')?.value || 'all',
    };
  }

  // Counts/footer via the shell handle when it has wired the pane, else plain
  // textContent (the shell wires asynchronously after mount).
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

  // Kein Lesezugriff (Core antwortet forbidden): Liste leer, Hinweis in der Zustandszone statt im Inhalt.
  function renderLockedCollection() {
    rowsCache = [];
    if (listEl) listEl.innerHTML = '';
    writeCounts({ all: 0, valid: 0, open: 0, ended: 0 });
    writeFooter('— ' + t('entries'));
    if (detailEl) { detailEl.hidden = true; detailEl.innerHTML = ''; }
    api.status.report({ level: 'warn', code: MODULE_ID + '.data.locked', text: t('dataLocked') + ' ' + t('dataLockedHint') });
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
      detailEl.setAttribute('data-context-record-type', 'consent');
      detailEl.setAttribute('data-context-label', (rec.purpose || refName(rec, 'subject', locale)));
    }
    if (titleEl) titleEl.textContent = rec ? (rec.purpose || refName(rec, 'subject', locale)) : t('newTitle');
  }

  function render() {
    if (locked) { renderLockedCollection(); return; }
    const g = readGrammar();
    const now = nowMs();
    const filtered = filterRows(rowsCache, g, now);
    if (listEl) listEl.innerHTML = renderRecordList(filtered, { view: g.view, selectedId, nowMs: now });
    writeCounts(countsFor(rowsCache, now));
    const scope = { all: t('bandAll'), valid: t('bandValid'), open: t('bandOpen'), ended: t('bandEnded') }[g.band] || t('bandAll');
    writeFooter(filtered.length + ' ' + t('entries') + ' · ' + scope);
    renderDetail();
  }

  let locked = false;
  function sortRows(rows) {
    return [...rows].sort((a, b) => (Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0)));
  }
  // Live-Abo (App-API 4.0): subscribe ruft sofort mit dem lokalen Stand und bei jeder Änderung.
  function subscribeRows() {
    try {
      return api.query(PRIMARY, {}).subscribe((rows) => {
        rowsCache = sortRows(Array.isArray(rows) ? rows : []);
        render();
      });
    } catch (error) {
      if (error?.code !== 'forbidden') throw error;
      locked = true;
      render();
      return () => {};
    }
  }

  // Selecting a record pre-fills the consent-check form (subject + purpose) as a
  // starting point and auto-reveals the read-only detail card.
  function fillForm(rec) {
    setField('subject_id', rec.subject_id || '');
    setField('purpose', rec.purpose || '');
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
    setGate('');
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
    setGate('');
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
      a.download = 'consent-records.json';
      a.rel = 'noopener';
      root?.appendChild(a);
      a.click();
      a.remove();
    } catch (e) {
      console.error('[consent] export failed:', e);
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
      try { parsed = JSON.parse(await file.text()); } catch { report('import.invalid', 'importInvalid', 'error'); return; }
      const items = Array.isArray(parsed) ? parsed : (parsed && typeof parsed === 'object' ? [parsed] : []);
      if (!items.length) { report('import.empty', 'importEmpty'); return; }
      const stamp = nowMs();
      let count = 0;
      for (const raw of items) {
        try { await api.put(PRIMARY, prepareImport(raw, stamp, String(count))).local; count += 1; }
        catch (e) { console.error('[consent] import failed:', e); }
      }
      report('import.done', 'imported', 'info', String(count));
    });
    input.click();
  }

  // Befehl über api.command: legt das Befehlsdokument an und liefert sofort (nie warten). Das Ergebnis (`done`)
  // ist die fachliche Antwort und erscheint im Prüfbereich; `failed`/lokal ungültig → Zustandszone.
  function runCommand(type, payload, onResult) {
    const code = MODULE_ID + '.command.' + type;
    const handle = api.command(type, { ...payload, record_id: payload.subject_id || MODULE_ID, client_context: { module_id: MODULE_ID } });
    let off = null;
    let finished = false;
    const stop = () => { finished = true; off?.(); off = null; };
    off = handle.status.subscribe((value) => {
      if (value?.status === 'failed') {
        api.status.report({ level: 'error', code, text: type + ': ' + (value.error || 'failed'), ref: handle.id });
        stop();
      } else if (value?.status === 'done') {
        api.status.clear(code);
        stop();
        onResult(value.result);
      }
    });
    if (finished) off?.();
    handle.local.catch((error) => {
      api.status.report({ level: 'error', code, text: type + ': ' + (error?.code || 'local') + ' ' + (error?.message || error), ref: handle.id });
    });
    disposers.push(stop);
    return handle;
  }

  // ats.consent.check — reads { subject_id, purpose? }, returns { ok, allowed, purpose }.
  function onCheckSubmit(event) {
    event.preventDefault();
    setGate('');
    const f = Object.fromEntries(new FormData(formEl).entries());
    const subject_id = String(f.subject_id || '').trim();
    if (!subject_id) { report('form.subject_required', 'subjectRequired'); return; }
    clear('form.subject_required');
    const purposeRaw = String(f.purpose || '').trim();
    const payload = { subject_id, purpose: purposeRaw || null };
    runCommand(CHECK_COMMAND, payload, (result) => renderCheckResult(result, subject_id, purposeRaw));
  }

  function renderCheckResult(result, subject_id, purposeRaw) {
    const blockers = collectBlockers(result);
    if (result?.ok === false || (Array.isArray(blockers) && blockers.length > 0)) {
      setGate('<strong>' + esc(t('checkFailed')) + '</strong>' + blockerList(blockers), 'block');
      return;
    }
    const allowed = result?.allowed === true || result?.data?.allowed === true;
    const checkedPurpose = result?.purpose ?? result?.data?.purpose ?? (purposeRaw || null);
    setGate(
      '<strong>' + esc(allowed ? t('consentPresent') : t('consentMissing')) + '</strong>'
      + fieldRows([
        [t('subject'), subject_id],
        [t('purpose'), checkedPurpose ?? t('existenceOnly')],
        [t('decision'), allowed ? t('allowed') : t('denied')],
      ]),
      allowed ? 'ok' : 'block'
    );
  }
  formEl?.addEventListener('submit', onCheckSubmit);

  // Betroffenenrechte: Art. 15 export / Art. 17 erase, both keyed by subject_id.
  async function dispatchSubject(commandType, subjectId, label) {
    setGate('');
    const subject_id = String(subjectId || '').trim();
    if (!subject_id) { report('form.subject_required', 'subjectRequired'); return; }
    clear('form.subject_required');
    // Art. 17 löscht app-übergreifend und endgültig: nur nach ausdrücklicher Bestätigung im App-Fenster.
    if (commandType === ERASE_COMMAND) {
      const confirmed = await api.shell.dialog({
        kind: 'confirm',
        title: t('eraseConfirmTitle'),
        text: t('eraseConfirmText').replace('{subject}', subject_id),
        confirm_label: t('eraseConfirm'),
        danger: true,
        default: false,
      });
      if (confirmed !== true) return;
    }
    runCommand(commandType, { subject_id }, (result) => {
      const blockers = collectBlockers(result);
      if (result?.ok === false || (Array.isArray(blockers) && blockers.length > 0)) {
        setGate('<strong>' + esc(label) + ' ' + esc(t('blocked')) + '.</strong>' + blockerList(blockers), 'block');
        return;
      }
      // Löschung: die Liste folgt über das Live-Abo, kein Nachladen.
      renderSubjectResult(commandType, label, subject_id, result);
    });
  }

  function renderSubjectResult(commandType, label, subjectId, result) {
    // Result keys are the native handler's exact shape (store.rs):
    //   export -> { record_count, collections{coll:[rec]}, audit_trail[] }
    //   erase  -> { erased_count, erased{coll:[id]} }
    const data = result?.data || result || {};
    let body = '';

    if (commandType === ERASE_COMMAND) {
      const erased = data.erased && typeof data.erased === 'object' ? data.erased : {};
      const count = data.erased_count ?? Object.values(erased).reduce((n, ids) => n + (Array.isArray(ids) ? ids.length : 0), 0);
      body += fieldRows([[t('subject'), subjectId], [t('deletedRecords'), count]]);
      const collEntries = Object.entries(erased).filter(([, ids]) => Array.isArray(ids) && ids.length);
      if (collEntries.length) {
        body += '<ul class="ats-blockers">'
          + collEntries.map(([coll, ids]) => '<li>' + esc(coll) + ': ' + esc(ids.length) + ' (' + esc(ids.join(', ')) + ')</li>').join('')
          + '</ul>';
      }
      setGate('<strong>' + esc(label) + ' ' + esc(t('executed')) + '.</strong>' + body, 'ok');
      return;
    }

    // EXPORT_COMMAND (Art. 15 Auskunft)
    const collections = data.collections && typeof data.collections === 'object' ? data.collections : {};
    const auditTrail = Array.isArray(data.audit_trail) ? data.audit_trail : [];
    const recordCount = data.record_count ?? Object.values(collections).reduce((n, recs) => n + (Array.isArray(recs) ? recs.length : 0), 0);
    body += fieldRows([[t('subject'), subjectId], [t('affectedRecords'), recordCount], [t('auditEntries'), auditTrail.length]]);
    const exportPayload = { subject_id: subjectId, record_count: recordCount, collections, audit_trail: auditTrail };
    let pretty;
    try { pretty = JSON.stringify(exportPayload, null, 2); } catch { pretty = String(exportPayload); }
    body += '<div>' + esc(t('exportLabel')) + ':</div><pre class="ctox-pre">' + esc(pretty) + '</pre>';
    setGate('<strong>' + esc(label) + ' ' + esc(t('executed')) + '.</strong>' + body, 'ok');
  }

  async function onSubjectSubmit(event) {
    event.preventDefault();
    const action = event.submitter?.getAttribute?.('data-subject-action');
    const f = Object.fromEntries(new FormData(subjectFormEl).entries());
    const subjectId = f.subject_id;
    if (action === EXPORT_COMMAND) await dispatchSubject(EXPORT_COMMAND, subjectId, t('exportArticle15'));
    else if (action === ERASE_COMMAND) await dispatchSubject(ERASE_COMMAND, subjectId, t('eraseArticle17'));
  }
  subjectFormEl?.addEventListener('submit', onSubjectSubmit);

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
  listEl?.addEventListener('click', onListClick);
  listEl?.addEventListener('keydown', onListKey);

  // Header + detail icon actions (create / import / export / collapse / per-record rights).
  function onAction(event) {
    const btn = event.target?.closest?.('[data-action]');
    if (!btn || !root?.contains(btn)) return;
    const action = btn.dataset.action;
    if (action === 'new') startNew();
    else if (action === 'import') importRecords();
    else if (action === 'export') exportRecords();
    else if (action === 'collapse-detail') { userCollapsed = true; renderDetail(); }
    else if (action === 'rights-export' || action === 'rights-erase') {
      const rec = selectedId ? rowsCache.find((r) => r.id === selectedId) : null;
      if (!rec?.subject_id) return;
      if (action === 'rights-export') dispatchSubject(EXPORT_COMMAND, rec.subject_id, t('exportArticle15'));
      else dispatchSubject(ERASE_COMMAND, rec.subject_id, t('eraseArticle17'));
    }
  }
  root?.addEventListener('click', onAction);

  // Betroffenenrechte form is hidden by default (is-rights-hidden on root); the
  // header toggle reveals it on demand — threads' data-toggle idiom.
  function onToggleRights() {
    const hidden = root?.classList.toggle('is-rights-hidden');
    toggleRightsEl?.setAttribute('aria-pressed', hidden ? 'false' : 'true');
  }
  toggleRightsEl?.addEventListener('click', onToggleRights);

  // Ein-Knopf-Umschalter Karten <-> Liste. Der Knopf ist eine AKTION: Icon,
  // aria-label und title benennen das Ziel des Klicks, einen Gedrueckt-Zustand
  // gibt es nicht.
  function syncViewToggle() {
    if (!viewToggleEl) return;
    const next = viewToggleEl.dataset.pgViewAlt === 'cards' ? 'cards' : 'list';
    const label = t(VIEW_ACTION_KEY[next]);
    viewToggleEl.innerHTML = VIEW_ICON[next];
    viewToggleEl.setAttribute('aria-label', label);
    viewToggleEl.setAttribute('title', label);
    viewToggleEl.removeAttribute('aria-pressed');
  }
  function onViewToggle() {
    if (!viewToggleEl) return;
    const current = viewToggleEl.dataset.pgView === 'list' ? 'list' : 'cards';
    viewToggleEl.dataset.pgView = current === 'list' ? 'cards' : 'list';
    viewToggleEl.dataset.pgViewAlt = current;
    syncViewToggle();
    render();
  }
  viewToggleEl?.addEventListener('click', onViewToggle);

  // Die generische Shell-Verdrahtung (shared/pane-grammar.js) setzt auf JEDEN
  // [data-pg-view]-Knopf ein aria-pressed="true". Fuer einen Aktionsknopf ist
  // das falsche Semantik (Screenreader melden einen Umschalter im Zustand
  // "gedrueckt"). Dieser Listener haengt an der SPALTE, laeuft also in der
  // Bubble-Phase garantiert nach beiden Knopf-Listenern, und raeumt es ab —
  // ohne Timer und ohne Annahme ueber die Registrierungsreihenfolge. Das
  // Klickziel wird bewusst NICHT geprueft: der Icon-Tausch ersetzt das
  // getroffene <svg>, das Ereignisziel haengt danach an keinem Elternknoten
  // mehr. Der Knopf darf das Attribut ohnehin nie tragen.
  function stripViewTogglePressed() {
    viewToggleEl?.removeAttribute('aria-pressed');
  }
  rail?.addEventListener('click', stripViewTogglePressed);
  syncViewToggle();

  // Re-render when the shell reports a grammar change (search / view / tray /
  // band). The event bubbles from the wired pane.
  const onGrammarChange = () => { render(); };
  rail?.addEventListener('ctox-pane-grammar-change', onGrammarChange);

  disposers.push(subscribeRows());
  disposers.push(api.shell.locale.subscribe((value) => {
    const next = langFrom(value);
    if (next === locale) return;
    locale = next;
    text = COPY[locale];
    root?.setAttribute('lang', locale);
    applyLabels();
    syncViewToggle();
    render();
  }));
  applyRestored(restored);

  // Snapshot für den Live-Remount: Auswahl, Ansicht, offene Formulareingaben.
  function snapshot() {
    const g = readGrammar();
    const form = formEl ? Object.fromEntries(new FormData(formEl).entries()) : {};
    const subjectForm = subjectFormEl ? Object.fromEntries(new FormData(subjectFormEl).entries()) : {};
    return {
      v: 1,
      selectedId,
      userCollapsed,
      search: rail?.querySelector('[data-pg-search]')?.value || '',
      view: g.view,
      rightsOpen: !root?.classList.contains('is-rights-hidden'),
      form: { subject_id: String(form.subject_id || ''), purpose: String(form.purpose || '') },
      subjectForm: { subject_id: String(subjectForm.subject_id || '') },
    };
  }

  function applyRestored(snap) {
    if (!snap || typeof snap !== 'object') return;
    if (typeof snap.selectedId === 'string') selectedId = snap.selectedId;
    if (typeof snap.userCollapsed === 'boolean') userCollapsed = snap.userCollapsed;
    const search = rail?.querySelector('[data-pg-search]');
    if (search && typeof snap.search === 'string') search.value = snap.search;
    if (snap.view === 'list' && viewToggleEl && viewToggleEl.dataset.pgView !== 'list') onViewToggle();
    if (snap.rightsOpen === true && root?.classList.contains('is-rights-hidden')) onToggleRights();
    if (snap.form && typeof snap.form === 'object') {
      setField('subject_id', snap.form.subject_id || '');
      setField('purpose', snap.form.purpose || '');
    }
    const subjectInput = subjectFormEl?.querySelector('[name="subject_id"]');
    if (subjectInput && typeof snap.subjectForm?.subject_id === 'string') subjectInput.value = snap.subjectForm.subject_id;
    applyListSelection();
    renderDetail();
  }

  return {
    unmount() {
      for (const dispose of disposers.splice(0)) {
        try { dispose(); } catch (error) { console.warn('[consent] unsubscribe failed', error); }
      }
      formEl?.removeEventListener('submit', onCheckSubmit);
      subjectFormEl?.removeEventListener('submit', onSubjectSubmit);
      toggleRightsEl?.removeEventListener('click', onToggleRights);
      viewToggleEl?.removeEventListener('click', onViewToggle);
      rail?.removeEventListener('click', stripViewTogglePressed);
      listEl?.removeEventListener('click', onListClick);
      listEl?.removeEventListener('keydown', onListKey);
      root?.removeEventListener('click', onAction);
      rail?.removeEventListener('ctox-pane-grammar-change', onGrammarChange);
      host.replaceChildren();
      delete host.dataset.atsModule;
    },
    snapshot,
  };
}

// Consent status → kit badge state (base.css .ctox-badge modifiers).
function badgeStateClass(status) {
  switch (status) {
    case 'active':
      return ' is-success';
    case 'withdrawn':
      return ' is-danger';
    case 'expired':
    case 'pending':
      return ' is-warning';
    default:
      return '';
  }
}

function fmtDate(ms) {
  const n = Number(ms);
  if (!Number.isFinite(n) || n <= 0) return '—';
  try { return new Date(n).toLocaleDateString(locale === 'en' ? 'en-US' : 'de-DE'); } catch { return String(n); }
}

// Rechtsgrundlage als Nutzerbegriff (Art. 6 DSGVO) statt Rohwert.
function legalBasisLabel(raw) {
  const v = String(raw || 'consent').toLowerCase();
  const map = { consent: 'basisConsent', contract: 'basisContract', legal_obligation: 'basisLegal', legitimate_interest: 'basisInterest' };
  return map[v] ? text[map[v]] : String(raw);
}

function fmtValidUntil(ms) {
  return Number.isFinite(Number(ms)) && Number(ms) > 0 ? fmtDate(ms) : text.unlimited;
}

// Key/value result rows inside the gate callout → kit .ctox-fields (base.css).
function fieldRows(pairs) {
  return '<dl class="ctox-fields">' + pairs
    .map(([label, value]) => '<dt>' + esc(label) + '</dt><dd>' + esc(value) + '</dd>')
    .join('') + '</dl>';
}

function collectBlockers(result) {
  const decision = result?.gate || result?.decision || null;
  const blockers = result?.blockers || decision?.blockers || result?.errors || null;
  if (Array.isArray(blockers)) return blockers.filter(Boolean);
  if (blockers) return [blockers];
  if (typeof result?.error === 'string' && result.error) return [result.error];
  if (result?.ok === false && result?.message) return [result.message];
  return [];
}

function blockerList(blockers) {
  if (!Array.isArray(blockers) || !blockers.length) return '';
  const items = blockers
    .map((b) => '<li>' + esc(typeof b === 'string' ? b : (b?.message || b?.reason || JSON.stringify(b))) + '</li>')
    .join('');
  return '<ul class="ats-blockers">' + items + '</ul>';
}

async function loadMarkup() {
  // index.html liefert der Service Worker versioniert (§16.15) – kein Cache-Buster; CSS lädt die Shell.
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
