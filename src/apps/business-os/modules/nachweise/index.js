import {
  CREDENTIAL_TYPES,
  credentialStatus,
  daysUntilExpiry,
  isDeploymentBlocking,
} from './core/credential.js';

const MOD_BUILD = '20260718-kit2';
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
    title: 'Nachweise', kicker: 'Nachweise', subtitle: 'Ablaufende, verifizierte Nachweise je Subjekt mit Einsatz-Gate und Leistungsnachweis-Freigabe.',
    subjectLabel: 'Subjekt', credentialTypeLabel: 'Nachweistyp', validUntilLabel: 'Gültig bis', deploymentSubjectLabel: 'Einsatz-Subjekt', requiredTypesLabel: 'Pflicht-Typen', recordLabel: 'Leistungsnachweis', rateLabel: 'Verrechnungssatz', clientLabel: 'Entleiher', signatureLabel: 'Signatur-Anfrage', collectionLabel: 'Collection',
    subjectPlaceholder: 'Subjekt-ID (Kandidat/Mitarbeiter)', issuerPlaceholder: 'Aussteller', addCredential: 'Nachweis hinzufügen',
    deploymentSubjectPlaceholder: 'Subjekt-ID für Einsatz-Prüfung', requiredTypesPlaceholder: 'Pflicht-Typen (kommagetrennt)',
    checkReadiness: 'Einsatzbereitschaft prüfen', recordPlaceholder: 'Leistungsnachweis-ID (Sign-off)',
    ratePlaceholder: 'Verrechnungssatz €/h (Pflicht für Abrechnung)', clientAccountPlaceholder: 'Entleiher-Account-ID (Rechnungsempfänger)',
    signaturePlaceholder: 'Signatur-Anfrage-ID (falls Entleiher-Signatur erzwungen)', collectionPlaceholder: 'Collection (optional, Standard: planning_time_records)',
    releaseBilling: 'Entleiher-Sign-off → Abrechnung freigeben', credentials: 'Nachweise', empty: 'Noch keine Nachweise erfasst.',
    commandOffline: 'Offline: Befehlsdienst nicht verfügbar.', checking: 'Prüfe Einsatzbereitschaft…', dispatchOffline: 'Offline: Befehl konnte nicht gesendet werden.',
    blockedReason: 'blockiert', notReady: 'Nicht einsatzbereit', ready: 'Einsatzbereit', databaseUnavailable: 'Datenbank nicht verfügbar.',
    databaseOffline: 'Offline: Datenbank nicht verfügbar.', subjectRequired: 'Subjekt-ID erforderlich.', saveFailed: 'Nachweis konnte nicht gespeichert werden.',
    recordRequired: 'Leistungsnachweis-ID erforderlich.', signoffRunning: 'Sign-off läuft…', signoffBlocked: 'Sign-off blockiert.',
    signoffSaved: 'Entleiher-Sign-off gespeichert.', performanceRecord: 'Leistungsnachweis', billingReleased: 'Abrechnung freigegeben',
    invoice: 'Rechnung', net: 'Netto', yes: 'ja', no: 'nein', subject: 'Subjekt', issuer: 'Aussteller',
    expiredAgo: 'abgelaufen vor', daysRemaining: 'noch', days: 'Tage', noExpiry: 'kein Ablauf', verifiedBy: 'verifiziert von',
    deploymentCheck: 'Einsatz prüfen', deploymentBlocking: 'Einsatz blockierend', credential: 'Nachweis',
    statusValid: 'gültig', statusExpiring: 'läuft ab', statusExpired: 'abgelaufen', statusNotYetValid: 'noch nicht gültig', statusUnverified: 'nicht verifiziert',
  },
  en: {
    title: 'Credentials', kicker: 'Credentials', subtitle: 'Expiring, verified credentials per subject with deployment gate and performance-record billing release.',
    subjectLabel: 'Subject', credentialTypeLabel: 'Credential type', validUntilLabel: 'Valid until', deploymentSubjectLabel: 'Deployment subject', requiredTypesLabel: 'Required types', recordLabel: 'Performance record', rateLabel: 'Charge rate', clientLabel: 'Client', signatureLabel: 'Signature request', collectionLabel: 'Collection',
    subjectPlaceholder: 'Subject ID (candidate/employee)', issuerPlaceholder: 'Issuer', addCredential: 'Add credential',
    deploymentSubjectPlaceholder: 'Subject ID for deployment check', requiredTypesPlaceholder: 'Required types (comma-separated)',
    checkReadiness: 'Check deployment readiness', recordPlaceholder: 'Performance record ID (sign-off)',
    ratePlaceholder: 'Charge rate €/h (required for billing)', clientAccountPlaceholder: 'Client account ID (invoice recipient)',
    signaturePlaceholder: 'Signature request ID (when client signature is enforced)', collectionPlaceholder: 'Collection (optional, default: planning_time_records)',
    releaseBilling: 'Client sign-off → release billing', credentials: 'credentials', empty: 'No credentials recorded yet.',
    commandOffline: 'Offline: command service unavailable.', checking: 'Checking deployment readiness…', dispatchOffline: 'Offline: command could not be sent.',
    blockedReason: 'blocked', notReady: 'Not ready for deployment', ready: 'Ready for deployment', databaseUnavailable: 'Database unavailable.',
    databaseOffline: 'Offline: database unavailable.', subjectRequired: 'Subject ID is required.', saveFailed: 'Credential could not be saved.',
    recordRequired: 'Performance record ID is required.', signoffRunning: 'Sign-off in progress…', signoffBlocked: 'Sign-off blocked.',
    signoffSaved: 'Client sign-off saved.', performanceRecord: 'Performance record', billingReleased: 'Billing released',
    invoice: 'Invoice', net: 'Net', yes: 'yes', no: 'no', subject: 'Subject', issuer: 'Issuer',
    expiredAgo: 'expired', daysRemaining: 'expires in', days: 'days', noExpiry: 'no expiry', verifiedBy: 'verified by',
    deploymentCheck: 'Check deployment', deploymentBlocking: 'Blocks deployment', credential: 'Credential',
    statusValid: 'valid', statusExpiring: 'expiring', statusExpired: 'expired', statusNotYetValid: 'not yet valid', statusUnverified: 'unverified',
  },
};

export async function mount(ctx) {
  await ensureStyles();
  ctx.host.innerHTML = await loadMarkup();
  ctx.host.dataset.atsModule = MODULE_ID;
  ctx.host.dataset.nachweiseModule = 'native';
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();

  const root = ctx.host.querySelector('[data-ats-root]');
  const locale = String(ctx.locale || document.documentElement.lang || 'de').toLowerCase().startsWith('en') ? 'en' : 'de';
  const copy = COPY[locale];
  const t = (key) => copy[key] || COPY.de[key] || key;
  root?.setAttribute('lang', locale);
  root?.querySelectorAll('[data-i18n]').forEach((node) => { node.textContent = t(node.dataset.i18n); });
  root?.querySelectorAll('[data-i18n-placeholder]').forEach((node) => { node.placeholder = t(node.dataset.i18nPlaceholder); });
  const listEl = root?.querySelector('[data-ats-list]');
  const countEl = root?.querySelector('[data-ats-count]');
  const formEl = root?.querySelector('[data-ats-form]');
  const gateFormEl = root?.querySelector('[data-ats-gate-form]');
  const signoffFormEl = root?.querySelector('[data-ats-signoff-form]');
  const gateEl = root?.querySelector('[data-ats-gate]');
  const titleEl = root?.querySelector('[data-ats-title]');
  const subEl = root?.querySelector('[data-ats-sub]');
  const typeSelect = root?.querySelector('[data-credential-type]');
  if (titleEl) titleEl.textContent = locale === 'en' ? t('title') : (ctx.manifest?.title || t('title'));
  if (subEl && ctx.manifest?.description && locale !== 'en') subEl.textContent = ctx.manifest.description;
  if (typeSelect) {
    typeSelect.innerHTML = CREDENTIAL_TYPES
      .map((type) => '<option value="' + esc(type.key) + '">' + esc(typeLabel(type.key, locale)) + '</option>')
      .join('');
  }

  const collection = () => {
    try { return ctx.db?.collection?.(PRIMARY) || null; } catch { return null; }
  };

  // Gate result → kit callout state (base.css .ctox-callout modifiers).
  const GATE_KINDS = { ok: 'is-success', block: 'is-danger', offline: 'is-warning' };
  function setGate(html, kind) {
    if (!gateEl) return;
    gateEl.className = 'ctox-callout' + (GATE_KINDS[kind] ? ' ' + GATE_KINDS[kind] : '');
    gateEl.innerHTML = html || '';
    gateEl.hidden = !html;
  }

  async function render() {
    const col = collection();
    let rows = [];
    if (col?.find) {
      try {
        const docs = await col.find({ selector: {} }).exec();
        rows = docs.map((d) => (typeof d.toJSON === 'function' ? d.toJSON() : d)).filter((r) => !r._deleted);
      } catch (e) { console.error('[nachweise] load failed:', e); }
    }
    if (countEl) countEl.textContent = rows.length + ' ' + t('credentials');
    if (listEl) {
      const now = Date.now();
      listEl.innerHTML = rows.length
        ? rows.map((r) => credentialRow(r, now, t, locale)).join('')
        : '<div class="ctox-empty">' + esc(t('empty')) + '</div>';
    }
  }

  // Per-row action: run the server-authoritative deployment gate for this
  // credential's subject (ats.deployment.check) — mirrors placements' early_leave.
  async function onListClick(event) {
    const btn = event.target?.closest?.('[data-deploy-check]');
    if (!btn) return;
    const subjectId = btn.getAttribute('data-deploy-check');
    if (!subjectId) return;
    await runDeploymentCheck(subjectId, []);
  }
  listEl?.addEventListener('click', onListClick);

  async function runDeploymentCheck(subjectId, requiredTypes) {
    const dispatch = ctx.commandBus?.dispatch;
    if (typeof dispatch !== 'function') { setGate(t('commandOffline'), 'offline'); return; }
    setGate(t('checking'));
    let result;
    try {
      const decision = await dispatch({
        module: MODULE_ID,
        command_type: DEPLOY_COMMAND,
        payload: { subject_id: subjectId, required_types: requiredTypes },
      });
      result = decision?.result || decision;
    } catch (e) {
      console.warn('[nachweise] deployment gate check failed:', e);
      setGate(t('dispatchOffline'), 'offline');
      return;
    }
    const blockers = result?.blockers || [];
    const ready = result?.ready === true && !(Array.isArray(blockers) && blockers.length);
    if (!ready) {
      const items = (Array.isArray(blockers) ? blockers : [blockers])
        .filter(Boolean)
        .map((b) => '<li>' + esc(typeLabel(b?.credential_type, locale) + ' — ' + (b?.reason || t('blockedReason'))) + '</li>')
        .join('');
      setGate(
        '<strong>✗ ' + esc(t('notReady')) + ': ' + esc(subjectId) + '</strong>'
        + (items ? '<ul class="ats-blockers">' + items + '</ul>' : ''),
        'block'
      );
      return;
    }
    setGate('<strong>✓ ' + esc(t('ready')) + ': ' + esc(subjectId) + '</strong>', 'ok');
  }

  // Create a credential (plain RxDB write — no native command for capture).
  async function onSubmit(event) {
    event.preventDefault();
    setGate('');
    const col = collection();
    if (!col?.insert) {
      ctx.notifications?.show?.({ type: 'error', title: t('title'), message: t('databaseUnavailable') });
      setGate(t('databaseOffline'), 'offline');
      return;
    }
    const data = new FormData(formEl);
    const f = Object.fromEntries(data.entries());
    const subjectId = String(f.subject_id || '').trim();
    if (!subjectId) { setGate(t('subjectRequired'), 'block'); return; }
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
      _deleted: false,
    };
    try {
      await col.insert(record);
      try { formEl.reset(); } catch {}
      await render();
    } catch (e) {
      console.error('[nachweise] insert failed:', e);
      setGate(t('saveFailed'), 'block');
    }
  }
  formEl?.addEventListener('submit', onSubmit);

  // Server-authoritative deployment-readiness gate (ats.deployment.check).
  async function onGateCheck(event) {
    event.preventDefault();
    const data = new FormData(gateFormEl);
    const subjectId = String(data.get('subject_id') || '').trim();
    if (!subjectId) { setGate(t('subjectRequired'), 'block'); return; }
    const requiredTypes = String(data.get('required_types') || '')
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean);
    await runDeploymentCheck(subjectId, requiredTypes);
  }
  gateFormEl?.addEventListener('submit', onGateCheck);

  // Entleiher sign-off → billing release (ats.leistungsnachweis.signoff).
  async function onSignoff(event) {
    event.preventDefault();
    setGate('');
    const dispatch = ctx.commandBus?.dispatch;
    if (typeof dispatch !== 'function') { setGate(t('commandOffline'), 'offline'); return; }
    const data = new FormData(signoffFormEl);
    const recordId = String(data.get('record_id') || '').trim();
    if (!recordId) { setGate(t('recordRequired'), 'block'); return; }
    const collectionName = String(data.get('collection') || '').trim();
    const payload = { record_id: recordId };
    if (collectionName) payload.collection = collectionName;
    // Billing-critical fields the native signoff handler reads (store.rs):
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
    setGate(t('signoffRunning'));
    let result;
    try {
      const decision = await dispatch({
        module: MODULE_ID,
        command_type: SIGNOFF_COMMAND,
        payload,
      });
      result = decision?.result || decision;
    } catch (e) {
      console.warn('[nachweise] signoff failed:', e);
      setGate(t('dispatchOffline'), 'offline');
      return;
    }
    const blockers = result?.blockers || result?.errors || null;
    const blocked = result?.ok === false || result?.status === 'blocked'
      || (Array.isArray(blockers) && blockers.length > 0);
    if (blocked) {
      const items = (Array.isArray(blockers) ? blockers : [blockers])
        .filter(Boolean)
        .map((b) => '<li>' + esc(typeof b === 'string' ? b : (b?.message || b?.reason || JSON.stringify(b))) + '</li>')
        .join('');
      setGate('<strong>' + esc(t('signoffBlocked')) + '</strong>' + (items ? '<ul class="ats-blockers">' + items + '</ul>' : ''), 'block');
      return;
    }
    const signedId = result?.record_id ?? recordId;
    const released = result?.billing_released === true;
    const invoiceId = result?.invoice_id ? String(result.invoice_id) : '';
    const netTotal = result?.net_total;
    setGate(
      '<strong>' + esc(t('signoffSaved')) + '</strong>'
      + '<div class="ats-result-row">' + esc(t('performanceRecord')) + ': ' + esc(signedId) + '</div>'
      + '<div class="ats-result-row">' + esc(t('billingReleased')) + ': ' + esc(released ? t('yes') : t('no')) + '</div>'
      + (invoiceId ? '<div class="ats-result-row">' + esc(t('invoice')) + ': ' + esc(invoiceId) + '</div>' : '')
      + (netTotal != null ? '<div class="ats-result-row">' + esc(t('net')) + ': ' + esc(String(netTotal)) + ' €</div>' : ''),
      'ok'
    );
    try { signoffFormEl.reset(); } catch {}
    await render();
  }
  signoffFormEl?.addEventListener('submit', onSignoff);

  let subscription = null;
  const col = collection();
  if (col?.find) {
    try { subscription = col.find({ selector: {} }).$?.subscribe?.(() => { render().catch(() => {}); }); }
    catch { /* live sync optional */ }
  }
  await render();

  return () => {
    try { subscription?.unsubscribe?.(); } catch {}
    formEl?.removeEventListener('submit', onSubmit);
    gateFormEl?.removeEventListener('submit', onGateCheck);
    signoffFormEl?.removeEventListener('submit', onSignoff);
    listEl?.removeEventListener('click', onListClick);
    ctx.host.replaceChildren();
    delete ctx.host.dataset.atsModule;
    delete ctx.host.dataset.nachweiseModule;
  };
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

function credentialRow(r, now, t, locale) {
  const status = credentialStatus(r, now);
  const blocking = isDeploymentBlocking(r, now);
  const days = daysUntilExpiry(r, now);
  const daysLabel = Number.isFinite(days)
    ? (days < 0 ? t('expiredAgo') + ' ' + Math.abs(days) + ' ' + t('days') : t('daysRemaining') + ' ' + days + ' ' + t('days'))
    : t('noExpiry');
  const subject = r.subject_id || '—';
  const meta = [
    t('subject') + ': ' + subject,
    r.issuer ? t('issuer') + ': ' + r.issuer : '',
    daysLabel,
    r.verified_by ? t('verifiedBy') + ' ' + r.verified_by : '',
    r.id ? 'ID: ' + r.id : '',
  ].filter(Boolean).join(' · ');
  const checkBtn = r.subject_id
    ? '<button type="button" class="ctox-button" data-deploy-check="' + esc(r.subject_id) + '">' + esc(t('deploymentCheck')) + '</button>'
    : '';
  const recordId = r.id || '';
  const contextLabel = typeLabel(r.credential_type, locale) + ' · ' + subject;
  return '<div class="ats-item ats-item--rich"'
    + ' data-context-record-id="' + esc(recordId) + '"'
    + ' data-context-record-type="nachweis"'
    + ' data-context-label="' + esc(contextLabel || recordId) + '">'
    + '<div class="ats-item-main">' + esc(typeLabel(r.credential_type, locale))
    + '<span class="ats-item-sub"> · ' + esc(subject) + '</span></div>'
    + '<div class="ats-item-side">'
    + '<span class="ctox-badge' + badgeStateClass(status) + '">' + esc(t({ valid: 'statusValid', expiring: 'statusExpiring', expired: 'statusExpired', not_yet_valid: 'statusNotYetValid', unverified: 'statusUnverified' }[status]) || status) + '</span>'
    + (blocking ? '<span class="ctox-badge is-danger">' + esc(t('deploymentBlocking')) + '</span>' : '')
    + checkBtn
    + '</div>'
    + '<div class="ats-item-meta">' + esc(meta) + '</div>'
    + '</div>';
}

function typeLabel(key, locale = 'de') {
  return (locale === 'en' ? TYPE_LABEL_EN.get(key) : TYPE_LABEL.get(key)) || key || COPY[locale]?.credential || COPY.de.credential;
}

async function ensureStyles() {
  const href = new URL('./index.css', import.meta.url).pathname + '?v=' + MOD_BUILD;
  if (document.querySelector('link[href="' + href + '"]')) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  document.head.append(link);
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
