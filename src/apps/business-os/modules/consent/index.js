
const MOD_BUILD = '20260721-consent-ia';
const MODULE_ID = 'consent';
const PRIMARY = 'business_consents';
const CHECK_COMMAND = 'ats.consent.check';
const EXPORT_COMMAND = 'ats.subject.export';
const ERASE_COMMAND = 'ats.subject.erase';
const COPY = {
  de: {
    title: 'Einwilligungen', kicker: 'ATS', listTitle: 'Einwilligungen', newTitle: 'Neue Prüfung',
    newAction: 'Neue Prüfung', importAction: 'Importieren', exportAction: 'Exportieren',
    searchPlaceholder: 'Suchen...', closeDetail: 'Details schließen',
    bandAll: 'Alle', bandValid: 'Gültig', bandOpen: 'Offen', bandEnded: 'Erloschen',
    subjectLabel: 'Subjekt', purposeLabel: 'Zweck', rightsLabel: 'Betroffenenrechte', subjectPlaceholder: 'Subjekt-ID', purposePlaceholder: 'Zweck (optional — leer = nur Existenz)',
    checkConsent: 'Einwilligung prüfen', rightsSubjectPlaceholder: 'Subjekt-ID',
    exportArticle15: 'Auskunft (Art. 15)', eraseArticle17: 'Löschen (Art. 17)',
    exportTitle: 'Recht auf Auskunft (DSGVO Art. 15)', eraseTitle: 'Recht auf Löschung (DSGVO Art. 17)',
    entries: 'Einträge', entriesEmpty: 'Noch keine Einträge.', emptyFiltered: 'Kein Eintrag passt zum Filter.',
    statusAll: 'Alle Status', commandOffline: 'Offline: Befehlsdienst nicht verfügbar.',
    dataLocked: 'Datenzugriff gesperrt.',
    dataLockedHint: 'Die App ist installiert. Gib business_consents im App Store frei, um vorhandene Einwilligungen zu sehen.',
    databaseOffline: 'Offline: Datenbank nicht verfügbar.',
    imported: 'Importiert', importInvalid: 'Ungültige JSON-Datei.', importEmpty: 'Keine Datensätze in der Datei.',
    subjectRequired: 'Subjekt-ID erforderlich.', dispatchOffline: 'Offline: Befehl konnte nicht gesendet werden.',
    checkFailed: 'Prüfung fehlgeschlagen.', consentPresent: 'Einwilligung vorhanden.', consentMissing: 'Keine gültige Einwilligung.',
    subject: 'Subjekt', purpose: 'Zweck', existenceOnly: '— (nur Existenzprüfung)', decision: 'Entscheidung',
    allowed: 'erlaubt', denied: 'verweigert', blocked: 'blockiert', executed: 'ausgeführt',
    deletedRecords: 'Gelöschte Datensätze', affectedRecords: 'Betroffene Datensätze', auditEntries: 'Audit-Einträge',
    exportLabel: 'Auskunft (Art. 15)', valid: 'gültig', withdrawn: 'widerrufen', expired: 'abgelaufen', open: 'offen',
    legalBasis: 'Rechtsgrundlage', validUntil: 'gültig bis', granted: 'erteilt', exportShort: 'Auskunft', eraseShort: 'Löschen',
  },
  en: {
    title: 'Consent', kicker: 'ATS', listTitle: 'Consent', newTitle: 'New check',
    newAction: 'New check', importAction: 'Import', exportAction: 'Export',
    searchPlaceholder: 'Search...', closeDetail: 'Close details',
    bandAll: 'All', bandValid: 'Valid', bandOpen: 'Pending', bandEnded: 'Ended',
    subjectLabel: 'Subject', purposeLabel: 'Purpose', rightsLabel: 'Data-subject rights', subjectPlaceholder: 'Subject ID', purposePlaceholder: 'Purpose (optional — blank checks existence only)',
    checkConsent: 'Check consent', rightsSubjectPlaceholder: 'Subject ID',
    exportArticle15: 'Access request (Art. 15)', eraseArticle17: 'Erase (Art. 17)',
    exportTitle: 'Right of access (GDPR Art. 15)', eraseTitle: 'Right to erasure (GDPR Art. 17)',
    entries: 'entries', entriesEmpty: 'No entries yet.', emptyFiltered: 'No entry matches the filter.',
    statusAll: 'All statuses', commandOffline: 'Offline: command service unavailable.',
    dataLocked: 'Data access is locked.',
    dataLockedHint: 'The app is installed. Grant business_consents in the App Store to view existing consent records.',
    databaseOffline: 'Offline: database unavailable.',
    imported: 'Imported', importInvalid: 'Invalid JSON file.', importEmpty: 'No records in the file.',
    subjectRequired: 'Subject ID is required.', dispatchOffline: 'Offline: command could not be sent.',
    checkFailed: 'Consent check failed.', consentPresent: 'Valid consent exists.', consentMissing: 'No valid consent.',
    subject: 'Subject', purpose: 'Purpose', existenceOnly: '— (existence check only)', decision: 'Decision',
    allowed: 'allowed', denied: 'denied', blocked: 'blocked', executed: 'completed',
    deletedRecords: 'Deleted records', affectedRecords: 'Affected records', auditEntries: 'Audit entries',
    exportLabel: 'Access request (Art. 15)', valid: 'valid', withdrawn: 'withdrawn', expired: 'expired', open: 'pending',
    legalBasis: 'Legal basis', validUntil: 'valid until', granted: 'granted', exportShort: 'Access', eraseShort: 'Erase',
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

// A shard is a pure selector: title + ONE muted meta line. No inline expansion,
// no per-row buttons (design-guide "Canonical Column Grammar").
export function consentRow(r, opts = {}) {
  const view = opts.view === 'list' ? 'list' : 'cards';
  const selected = Boolean(opts.selected);
  const now = Number(opts.nowMs) || 0;
  const status = statusOf(r, now);
  const subject = r.subject_id || '—';
  const title = r.purpose || subject;
  const badge = '<span class="ctox-badge' + badgeStateClass(status) + '" data-status="' + esc(status) + '">' + esc(statusLabel(status)) + '</span>';
  const id = r.id || '';
  const attrs = ' class="ctox-list-item consent-row consent-row--' + view + (selected ? ' is-selected' : '') + '"'
    + ' role="button" tabindex="0" aria-selected="' + (selected ? 'true' : 'false') + '"'
    + ' data-context-record-id="' + esc(id) + '"'
    + ' data-context-record-type="consent"'
    + ' data-context-label="' + esc((title + ' · ' + subject) || id) + '"';
  if (view === 'list') {
    return '<div' + attrs + '><span class="consent-row-title">' + esc(title) + '</span>' + badge + '</div>';
  }
  const metaBits = [esc(text.kicker), esc(text.subject + ': ' + subject), esc(text.legalBasis + ': ' + (r.legal_basis || 'consent'))];
  return '<div' + attrs + '>'
    + '<div class="consent-row-head"><span class="consent-row-title">' + esc(title) + '</span>' + badge + '</div>'
    + '<div class="consent-row-meta">' + metaBits.join(' · ') + '</div>'
    + '</div>';
}

// The list body markup (shards or compact rows), or the empty state.
export function renderRecordList(rows, opts = {}) {
  const list = Array.isArray(rows) ? rows : [];
  if (!list.length) {
    return '<div class="ctox-empty"><strong>' + esc(opts.emptyText || text.entriesEmpty) + '</strong></div>';
  }
  return list.map((r) => consentRow(r, { view: opts.view, nowMs: opts.nowMs, selected: r.id && r.id === opts.selectedId })).join('');
}

// Read-only detail card for the selected consent record (auto-reveal target).
export function recordDetailHtml(r, nowMs) {
  const now = Number(nowMs) || 0;
  const status = statusOf(r, now);
  const subject = r.subject_id || '—';
  const title = r.purpose || subject;
  const rows = [];
  rows.push(field(text.subject, esc(subject)));
  rows.push(field(text.purpose, esc(r.purpose || text.existenceOnly)));
  rows.push(field(text.legalBasis, esc(r.legal_basis || 'consent')));
  rows.push(field(text.validUntil, esc(fmtValidUntil(r.expires_at_ms))));
  rows.push(field(text.granted, esc(fmtDate(r.granted_at_ms))));
  if (Number.isFinite(Number(r.withdrawn_at_ms)) && Number(r.withdrawn_at_ms) > 0) {
    rows.push(field(text.withdrawn, esc(fmtDate(r.withdrawn_at_ms))));
  }
  if (r.id) rows.push(field('ID', '<span class="ats-tag">' + esc(r.id) + '</span>'));
  const rightsIcon = r.subject_id
    ? '<button type="button" class="ctox-pane-icon" data-action="rights-export" aria-label="' + esc(text.exportTitle) + '" title="' + esc(text.exportTitle) + '"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3v12M12 15l-4-4M12 15l4-4M5 21h14"/></svg></button>'
      + '<button type="button" class="ctox-pane-icon" data-action="rights-erase" aria-label="' + esc(text.eraseTitle) + '" title="' + esc(text.eraseTitle) + '"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 7h16M9 7V5h6v2M6 7l1 13h10l1-13"/></svg></button>'
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
    _deleted: false,
  };
}

// ---------------------------------------------------------------------------
// Mount
// ---------------------------------------------------------------------------

export async function mount(ctx) {
  locale = String(ctx.locale || document.documentElement.lang || 'de').toLowerCase().startsWith('en') ? 'en' : 'de';
  text = COPY[locale];
  await ensureStyles();
  ctx.host.innerHTML = await loadMarkup();
  ctx.host.dataset.atsModule = MODULE_ID;
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();

  const root = ctx.host.querySelector('[data-ats-root]');
  root?.setAttribute('lang', locale);
  const t = (key) => text[key] || COPY.de[key] || key;
  root?.querySelectorAll('[data-i18n]').forEach((node) => { node.textContent = t(node.dataset.i18n); });
  root?.querySelectorAll('[data-i18n-placeholder]').forEach((node) => { node.placeholder = t(node.dataset.i18nPlaceholder); });
  root?.querySelectorAll('[data-i18n-title]').forEach((node) => { node.title = node.ariaLabel = t(node.dataset.i18nTitle); });

  const rail = root?.querySelector('.consent-rail');
  const listEl = root?.querySelector('[data-ats-list]');
  const detailEl = root?.querySelector('[data-ats-detail]');
  const formEl = root?.querySelector('[data-ats-form]');
  const subjectFormEl = root?.querySelector('[data-ats-subject-form]');
  const gateEl = root?.querySelector('[data-ats-gate]');
  const titleEl = root?.querySelector('[data-ats-title]');
  const subEl = root?.querySelector('[data-ats-sub]');
  const toggleRightsEl = root?.querySelector('[data-toggle-rights]');
  if (titleEl) titleEl.textContent = locale === 'en' ? t('title') : (ctx.manifest?.title || t('title'));
  if (subEl) subEl.textContent = ctx.manifest?.description || '';

  let rowsCache = [];
  let selectedId = null;
  let userCollapsed = false;
  const nowMs = () => Date.now();
  const collection = () => { try { return ctx.db?.collection?.(PRIMARY) || null; } catch { return null; } };
  const canReadCollection = () => {
    const permissionCheck = ctx.permissions?.canReadCollection;
    return typeof permissionCheck !== 'function' || permissionCheck(PRIMARY) === true;
  };
  function isPermissionDenied(error) {
    return error?.code === 'CTOX_BUSINESS_OS_PERMISSION_DENIED'
      || error?.name === 'BusinessOsPermissionError';
  }

  // Gate callout → kit .ctox-callout variants (base.css).
  const GATE_VARIANTS = { ok: ' is-success', block: ' is-danger', offline: ' is-warning' };
  function setGate(html, kind) {
    if (!gateEl) return;
    gateEl.className = 'ctox-callout' + (GATE_VARIANTS[kind] || '');
    gateEl.innerHTML = html || '';
  }

  // Read the SHELL-wired grammar state straight from the pane DOM.
  function readGrammar() {
    return {
      search: (rail?.querySelector('[data-pg-search]')?.value || '').trim().toLowerCase(),
      view: rail?.querySelector('[data-pg-view][aria-pressed="true"]')?.dataset.pgView || 'cards',
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

  function renderLockedCollection() {
    rowsCache = [];
    if (listEl) {
      listEl.innerHTML = '<div class="ctox-empty ctox-empty--locked" role="status">'
        + '<strong>' + esc(t('dataLocked')) + '</strong>'
        + '<span>' + esc(t('dataLockedHint')) + '</span>'
        + '</div>';
    }
    writeCounts({ all: 0, valid: 0, open: 0, ended: 0 });
    writeFooter('— ' + t('entries'));
    if (detailEl) { detailEl.hidden = true; detailEl.innerHTML = ''; }
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
      detailEl.setAttribute('data-context-label', (rec.purpose || rec.subject_id || rec.id || ''));
    }
    if (titleEl) titleEl.textContent = rec ? ((rec.purpose || rec.subject_id || '—')) : t('newTitle');
  }

  function render() {
    if (!canReadCollection()) { renderLockedCollection(); return; }
    const g = readGrammar();
    const now = nowMs();
    const filtered = filterRows(rowsCache, g, now);
    if (listEl) {
      const emptyText = rowsCache.length ? t('emptyFiltered') : t('entriesEmpty');
      listEl.innerHTML = renderRecordList(filtered, { view: g.view, selectedId, nowMs: now, emptyText });
    }
    writeCounts(countsFor(rowsCache, now));
    const scope = { all: t('bandAll'), valid: t('bandValid'), open: t('bandOpen'), ended: t('bandEnded') }[g.band] || t('bandAll');
    writeFooter(filtered.length + ' ' + t('entries') + ' · ' + scope);
    renderDetail();
  }

  async function load() {
    if (!canReadCollection()) { rowsCache = []; return; }
    const col = collection();
    let rows = [];
    if (col?.find) {
      try {
        const docs = await col.find({ selector: {} }).exec();
        rows = docs.map((d) => (typeof d.toJSON === 'function' ? d.toJSON() : d)).filter((r) => !r._deleted);
      } catch (error) {
        if (isPermissionDenied(error)) { rowsCache = []; return; }
        console.error('[consent] load failed:', error);
      }
    }
    rows.sort((a, b) => (Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0)));
    rowsCache = rows;
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
    const col = collection();
    if (!col?.upsert) { setGate(t('databaseOffline'), 'offline'); return; }
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'application/json,.json';
    input.addEventListener('change', async () => {
      const file = input.files && input.files[0];
      if (!file) return;
      let parsed;
      try { parsed = JSON.parse(await file.text()); } catch { setGate(t('importInvalid'), 'block'); return; }
      const items = Array.isArray(parsed) ? parsed : (parsed && typeof parsed === 'object' ? [parsed] : []);
      if (!items.length) { setGate(t('importEmpty'), 'block'); return; }
      const stamp = nowMs();
      let count = 0;
      for (const raw of items) {
        try { await col.upsert(prepareImport(raw, stamp, String(count))); count += 1; }
        catch (e) { console.error('[consent] import failed:', e); }
      }
      setGate('<strong>' + esc(t('imported')) + '</strong>: ' + count, 'ok');
      await load();
      render();
    });
    input.click();
  }

  // ats.consent.check — reads { subject_id, purpose? }, returns { ok, allowed, purpose }.
  async function onCheckSubmit(event) {
    event.preventDefault();
    setGate('');
    const dispatch = ctx.commandBus?.dispatch;
    if (typeof dispatch !== 'function') { setGate(t('commandOffline'), 'offline'); return; }
    const f = Object.fromEntries(new FormData(formEl).entries());
    const subject_id = String(f.subject_id || '').trim();
    if (!subject_id) { setGate(t('subjectRequired'), 'block'); return; }
    const purposeRaw = String(f.purpose || '').trim();
    const payload = { subject_id, purpose: purposeRaw || null };

    let result;
    try { result = await dispatch.call(ctx.commandBus, { module: MODULE_ID, command_type: CHECK_COMMAND, payload }); }
    catch (e) { console.error('[consent] check dispatch failed:', e); setGate(t('dispatchOffline'), 'offline'); return; }

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
    const dispatch = ctx.commandBus?.dispatch;
    if (typeof dispatch !== 'function') { setGate(t('commandOffline'), 'offline'); return; }
    const subject_id = String(subjectId || '').trim();
    if (!subject_id) { setGate(t('subjectRequired'), 'block'); return; }

    let result;
    try { result = await dispatch.call(ctx.commandBus, { module: MODULE_ID, type: commandType, command_type: commandType, payload: { subject_id } }); }
    catch (e) { console.error('[consent] ' + commandType + ' dispatch failed:', e); setGate(t('dispatchOffline'), 'offline'); return; }

    const blockers = collectBlockers(result);
    if (result?.ok === false || (Array.isArray(blockers) && blockers.length > 0)) {
      setGate('<strong>' + esc(label) + ' ' + esc(t('blocked')) + '.</strong>' + blockerList(blockers), 'block');
      return;
    }
    renderSubjectResult(commandType, label, subject_id, result);
    if (commandType === ERASE_COMMAND) { try { await load(); render(); } catch {} }
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

  // Re-render when the shell reports a grammar change (search / view / tray /
  // band). The event bubbles from the wired pane.
  const onGrammarChange = () => { render(); };
  rail?.addEventListener('ctox-pane-grammar-change', onGrammarChange);

  let subscription = null;
  const col = collection();
  if (col?.find) {
    try { subscription = col.find({ selector: {} }).$?.subscribe?.(() => { load().then(render).catch(() => {}); }); }
    catch { /* live sync optional */ }
  }
  await load();
  render();

  return () => {
    try { subscription?.unsubscribe?.(); } catch {}
    formEl?.removeEventListener('submit', onCheckSubmit);
    subjectFormEl?.removeEventListener('submit', onSubjectSubmit);
    toggleRightsEl?.removeEventListener('click', onToggleRights);
    listEl?.removeEventListener('click', onListClick);
    listEl?.removeEventListener('keydown', onListKey);
    root?.removeEventListener('click', onAction);
    rail?.removeEventListener('ctox-pane-grammar-change', onGrammarChange);
    ctx.host.replaceChildren();
    delete ctx.host.dataset.atsModule;
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

function fmtValidUntil(ms) {
  return Number.isFinite(Number(ms)) && Number(ms) > 0 ? fmtDate(ms) : '∞';
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

async function ensureStyles() {
  const href = new URL('./index.css', import.meta.url).pathname + '?v=' + MOD_BUILD;
  if (document.querySelector('link[href="' + href + '"]')) return;
  const link = document.createElement('link'); link.rel = 'stylesheet'; link.href = href; document.head.append(link);
}
async function loadMarkup() {
  const url = new URL('./index.html', import.meta.url).pathname + '?v=' + MOD_BUILD;
  const html = await fetch(url).then((r) => r.text());
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
