
const MOD_BUILD = '20260706-kit1';
const MODULE_ID = 'consent';
const PRIMARY = 'business_consents';
const TITLE = 'consent';
const CHECK_COMMAND = 'ats.consent.check';
const EXPORT_COMMAND = 'ats.subject.export';
const ERASE_COMMAND = 'ats.subject.erase';
const COPY = {
  de: {
    title: 'Einwilligungen', subjectLabel: 'Subjekt', purposeLabel: 'Zweck', rightsLabel: 'Betroffenenrechte', subjectPlaceholder: 'Subjekt-ID', purposePlaceholder: 'Zweck (optional — leer = nur Existenz)',
    checkConsent: 'Einwilligung prüfen', rightsSubjectPlaceholder: 'Subjekt-ID für Betroffenenrechte',
    exportArticle15: 'Auskunft (Art. 15)', eraseArticle17: 'Löschen (Art. 17)',
    exportTitle: 'Recht auf Auskunft (DSGVO Art. 15)', eraseTitle: 'Recht auf Löschung (DSGVO Art. 17)',
    entries: 'Einträge', entriesEmpty: 'Noch keine Einträge.', commandOffline: 'Offline: Befehlsdienst nicht verfügbar.',
    dataLocked: 'Datenzugriff gesperrt.',
    dataLockedHint: 'Die App ist installiert. Gib business_consents im App Store frei, um vorhandene Einwilligungen zu sehen.',
    subjectRequired: 'Subjekt-ID erforderlich.', dispatchOffline: 'Offline: Befehl konnte nicht gesendet werden.',
    checkFailed: 'Prüfung fehlgeschlagen.', consentPresent: 'Einwilligung vorhanden.', consentMissing: 'Keine gültige Einwilligung.',
    subject: 'Subjekt', purpose: 'Zweck', existenceOnly: '— (nur Existenzprüfung)', decision: 'Entscheidung',
    allowed: 'erlaubt', denied: 'verweigert', blocked: 'blockiert', executed: 'ausgeführt',
    deletedRecords: 'Gelöschte Datensätze', affectedRecords: 'Betroffene Datensätze', auditEntries: 'Audit-Einträge',
    exportLabel: 'Auskunft (Art. 15)', valid: 'gültig', withdrawn: 'widerrufen', expired: 'abgelaufen', open: 'offen',
    legalBasis: 'Rechtsgrundlage', validUntil: 'gültig bis', granted: 'erteilt', exportShort: 'Auskunft', eraseShort: 'Löschen',
  },
  en: {
    title: 'Consent', subjectLabel: 'Subject', purposeLabel: 'Purpose', rightsLabel: 'Data-subject rights', subjectPlaceholder: 'Subject ID', purposePlaceholder: 'Purpose (optional — blank checks existence only)',
    checkConsent: 'Check consent', rightsSubjectPlaceholder: 'Subject ID for data-subject rights',
    exportArticle15: 'Access request (Art. 15)', eraseArticle17: 'Erase (Art. 17)',
    exportTitle: 'Right of access (GDPR Art. 15)', eraseTitle: 'Right to erasure (GDPR Art. 17)',
    entries: 'entries', entriesEmpty: 'No entries yet.', commandOffline: 'Offline: command service unavailable.',
    dataLocked: 'Data access is locked.',
    dataLockedHint: 'The app is installed. Grant business_consents in the App Store to view existing consent records.',
    subjectRequired: 'Subject ID is required.', dispatchOffline: 'Offline: command could not be sent.',
    checkFailed: 'Consent check failed.', consentPresent: 'Valid consent exists.', consentMissing: 'No valid consent.',
    subject: 'Subject', purpose: 'Purpose', existenceOnly: '— (existence check only)', decision: 'Decision',
    allowed: 'allowed', denied: 'denied', blocked: 'blocked', executed: 'completed',
    deletedRecords: 'Deleted records', affectedRecords: 'Affected records', auditEntries: 'Audit entries',
    exportLabel: 'Access request (Art. 15)', valid: 'valid', withdrawn: 'withdrawn', expired: 'expired', open: 'pending',
    legalBasis: 'Legal basis', validUntil: 'valid until', granted: 'granted', exportShort: 'Access', eraseShort: 'Erase',
  },
};

export async function mount(ctx) {
  await ensureStyles();
  ctx.host.innerHTML = await loadMarkup();
  ctx.host.dataset.atsModule = MODULE_ID;
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();
  const root = ctx.host.querySelector('[data-ats-root]');
  const locale = String(ctx.locale || document.documentElement.lang || 'de').toLowerCase().startsWith('en') ? 'en' : 'de';
  const copy = COPY[locale];
  const t = (key) => copy[key] || COPY.de[key] || key;
  root?.setAttribute('lang', locale);
  root?.querySelectorAll('[data-i18n]').forEach((node) => { node.textContent = t(node.dataset.i18n); });
  root?.querySelectorAll('[data-i18n-placeholder]').forEach((node) => { node.placeholder = t(node.dataset.i18nPlaceholder); });
  root?.querySelectorAll('[data-i18n-title]').forEach((node) => { node.title = t(node.dataset.i18nTitle); });
  const listEl = root?.querySelector('[data-ats-list]');
  const countEl = root?.querySelector('[data-ats-count]');
  const formEl = root?.querySelector('[data-ats-form]');
  const subjectFormEl = root?.querySelector('[data-ats-subject-form]');
  const gateEl = root?.querySelector('[data-ats-gate]');
  const titleEl = root?.querySelector('[data-ats-title]');
  const subEl = root?.querySelector('[data-ats-sub]');
  if (titleEl) titleEl.textContent = locale === 'en' ? t('title') : (ctx.manifest?.title || t('title'));
  if (subEl) subEl.textContent = ctx.manifest?.description || '';

  let rowsCache = [];
  const collection = () => { try { return ctx.db?.collection?.(PRIMARY) || null; } catch { return null; } };
  const canReadCollection = () => {
    const permissionCheck = ctx.permissions?.canReadCollection;
    return typeof permissionCheck !== 'function' || permissionCheck(PRIMARY) === true;
  };

  function isPermissionDenied(error) {
    return error?.code === 'CTOX_BUSINESS_OS_PERMISSION_DENIED'
      || error?.name === 'BusinessOsPermissionError';
  }

  function renderLockedCollection() {
    rowsCache = [];
    if (countEl) countEl.textContent = '— ' + t('entries');
    if (listEl) {
      listEl.innerHTML = '<div class="ctox-empty ctox-empty--locked" role="status">'
        + '<strong>' + esc(t('dataLocked')) + '</strong>'
        + '<span>' + esc(t('dataLockedHint')) + '</span>'
        + '</div>';
    }
  }

  function setGate(html, kind) {
    if (!gateEl) return;
    gateEl.className = 'ats-gate' + (kind ? ' ats-gate--' + kind : '');
    gateEl.innerHTML = html || '';
  }

  async function render() {
    if (!canReadCollection()) {
      renderLockedCollection();
      return;
    }
    const col = collection();
    let rows = [];
    if (col?.find) {
      try {
        const docs = await col.find({ selector: {} }).exec();
        rows = docs.map((d) => (typeof d.toJSON === 'function' ? d.toJSON() : d)).filter((r) => !r._deleted);
      } catch (error) {
        if (isPermissionDenied(error)) {
          renderLockedCollection();
          return;
        }
        console.error('[consent] load failed:', error);
      }
    }
    rowsCache = rows;
    if (countEl) countEl.textContent = rows.length + ' ' + t('entries');
    if (listEl) listEl.innerHTML = rows.length ? rows.map((r) => consentRow(r, t, locale)).join('') : '<div class="ctox-empty">' + esc(t('entriesEmpty')) + '</div>';
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
      + '<div class="ats-result-row">' + esc(t('subject')) + ': ' + esc(subject_id) + '</div>'
      + '<div class="ats-result-row">' + esc(t('purpose')) + ': ' + esc(checkedPurpose ?? t('existenceOnly')) + '</div>'
      + '<div class="ats-result-row">' + esc(t('decision')) + ': ' + esc(allowed ? t('allowed') : t('denied')) + '</div>',
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
    if (commandType === ERASE_COMMAND) { try { await render(); } catch {} }
  }

  function renderSubjectResult(commandType, label, subjectId, result) {
    // Result keys are the native handler's exact shape (store.rs):
    //   export -> { record_count, collections{coll:[rec]}, audit_trail[] }
    //   erase  -> { erased_count, erased{coll:[id]} }
    const data = result?.data || result || {};
    let body = '<div class="ats-result-row">' + esc(t('subject')) + ': ' + esc(subjectId) + '</div>';

    if (commandType === ERASE_COMMAND) {
      const erased = data.erased && typeof data.erased === 'object' ? data.erased : {};
      const count = data.erased_count ?? Object.values(erased).reduce((n, ids) => n + (Array.isArray(ids) ? ids.length : 0), 0);
      body += '<div class="ats-result-row">' + esc(t('deletedRecords')) + ': ' + esc(count) + '</div>';
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
    body += '<div class="ats-result-row">' + esc(t('affectedRecords')) + ': ' + esc(recordCount)
      + ' · ' + esc(t('auditEntries')) + ': ' + esc(auditTrail.length) + '</div>';
    const exportPayload = { subject_id: subjectId, record_count: recordCount, collections, audit_trail: auditTrail };
    let pretty;
    try { pretty = JSON.stringify(exportPayload, null, 2); } catch { pretty = String(exportPayload); }
    body += '<div class="ats-result-row">' + esc(t('exportLabel')) + ':</div><pre class="ats-export">' + esc(pretty) + '</pre>';
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

  // Per-row Betroffenenrechte buttons (delegated, mirrors placements' early_leave).
  async function onListClick(event) {
    const exportBtn = event.target?.closest?.('[data-subject-export]');
    const eraseBtn = event.target?.closest?.('[data-subject-erase]');
    if (exportBtn) { await dispatchSubject(EXPORT_COMMAND, exportBtn.getAttribute('data-subject-export'), t('exportArticle15')); return; }
    if (eraseBtn) { await dispatchSubject(ERASE_COMMAND, eraseBtn.getAttribute('data-subject-erase'), t('eraseArticle17')); }
  }
  listEl?.addEventListener('click', onListClick);

  let sub = null;
  const col = collection();
  if (col?.find) { try { sub = col.find({ selector: {} }).$?.subscribe?.(() => { render().catch(() => {}); }); } catch {} }
  await render();

  return () => {
    try { sub?.unsubscribe?.(); } catch {}
    formEl?.removeEventListener('submit', onCheckSubmit);
    subjectFormEl?.removeEventListener('submit', onSubjectSubmit);
    listEl?.removeEventListener('click', onListClick);
    ctx.host.replaceChildren();
    delete ctx.host.dataset.atsModule;
  };
}

// status mirrors core/consent.js isConsentValid: withdrawn > expired > active/granted.
function consentStatus(r) {
  const now = Date.now();
  if (Number.isFinite(Number(r.withdrawn_at_ms)) && Number(r.withdrawn_at_ms) > 0 && Number(r.withdrawn_at_ms) <= now) return 'withdrawn';
  if (Number.isFinite(Number(r.expires_at_ms)) && Number(r.expires_at_ms) > 0 && Number(r.expires_at_ms) <= now) return 'expired';
  const basis = String(r.legal_basis || '');
  const consentBasis = basis === '' || basis === 'consent' || basis === 'special_category_consent';
  if (consentBasis && !(Number.isFinite(Number(r.granted_at_ms)) && Number(r.granted_at_ms) > 0)) return 'pending';
  return 'active';
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

function fmtDate(ms, locale) {
  const n = Number(ms);
  if (!Number.isFinite(n) || n <= 0) return '—';
  try { return new Date(n).toLocaleDateString(locale === 'en' ? 'en-US' : 'de-DE'); } catch { return String(n); }
}

function consentRow(r, t, locale) {
  const status = consentStatus(r);
  const subjectId = r.subject_id || '';
  const purpose = r.purpose || '—';
  const basis = r.legal_basis || 'consent';
  const validUntil = Number.isFinite(Number(r.expires_at_ms)) && Number(r.expires_at_ms) > 0 ? fmtDate(r.expires_at_ms, locale) : '∞';
  const metaBits = [
    t('subject') + ' ' + esc(subjectId),
    t('legalBasis') + ' ' + esc(basis),
    t('validUntil') + ' ' + esc(validUntil),
    t('granted') + ' ' + esc(fmtDate(r.granted_at_ms, locale)),
  ];
  if (status === 'withdrawn') metaBits.push(t('withdrawn') + ' ' + esc(fmtDate(r.withdrawn_at_ms, locale)));
  if (r.id) metaBits.push('#' + esc(r.id));
  const ctxLabel = subjectId || r.id || '';
  return '<div class="ats-item ats-item--rich"'
    + ' data-context-record-id="' + esc(r.id || '') + '"'
    + ' data-context-record-type="consent"'
    + ' data-context-label="' + esc(ctxLabel) + '">'
    + '<div class="ats-item-main">'
    + '<span class="ctox-badge' + badgeStateClass(status) + '">' + esc(t({ active: 'valid', withdrawn: 'withdrawn', expired: 'expired', pending: 'open' }[status]) || status) + '</span>'
    + '<span class="ats-item-title">' + esc(purpose) + '</span>'
    + '<div class="ats-item-meta">' + metaBits.join(' · ') + '</div>'
    + '</div>'
    + '<div class="ats-item-actions">'
    + '<button type="button" class="ctox-button" data-subject-export="' + esc(subjectId) + '" title="' + esc(t('exportTitle')) + '">' + esc(t('exportShort')) + '</button>'
    + '<button type="button" class="ctox-button is-danger" data-subject-erase="' + esc(subjectId) + '" title="' + esc(t('eraseTitle')) + '">' + esc(t('eraseShort')) + '</button>'
    + '</div>'
    + '</div>';
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
  const html = await fetch(new URL('./index.html', import.meta.url)).then((r) => r.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((n) => n.remove());
  return doc.body.innerHTML;
}
function esc(v) { return String(v == null ? '' : v).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;'); }
