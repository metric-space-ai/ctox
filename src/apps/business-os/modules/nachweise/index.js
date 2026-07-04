import {
  CREDENTIAL_TYPES,
  credentialStatus,
  daysUntilExpiry,
  isDeploymentBlocking,
} from './core/credential.js';

const MOD_BUILD = '20260620-ats9';
const MODULE_ID = 'nachweise';
const PRIMARY = 'business_credentials';
const DEPLOY_COMMAND = 'ats.deployment.check';
const SIGNOFF_COMMAND = 'ats.leistungsnachweis.signoff';

const TYPE_LABEL = new Map(CREDENTIAL_TYPES.map((t) => [t.key, t.label]));

export async function mount(ctx) {
  await ensureStyles();
  ctx.host.innerHTML = await loadMarkup();
  ctx.host.dataset.atsModule = MODULE_ID;
  ctx.host.dataset.nachweiseModule = 'native';
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();

  const root = ctx.host.querySelector('[data-ats-root]');
  const listEl = root?.querySelector('[data-ats-list]');
  const countEl = root?.querySelector('[data-ats-count]');
  const formEl = root?.querySelector('[data-ats-form]');
  const gateFormEl = root?.querySelector('[data-ats-gate-form]');
  const signoffFormEl = root?.querySelector('[data-ats-signoff-form]');
  const gateEl = root?.querySelector('[data-ats-gate]');
  const titleEl = root?.querySelector('[data-ats-title]');
  const subEl = root?.querySelector('[data-ats-sub]');
  const typeSelect = root?.querySelector('[data-credential-type]');
  if (titleEl) titleEl.textContent = ctx.manifest?.title || 'Nachweise';
  if (subEl && ctx.manifest?.description) subEl.textContent = ctx.manifest.description;
  if (typeSelect) {
    typeSelect.innerHTML = CREDENTIAL_TYPES
      .map((type) => '<option value="' + esc(type.key) + '">' + esc(type.label) + '</option>')
      .join('');
  }

  const collection = () => {
    try { return ctx.db?.collection?.(PRIMARY) || null; } catch { return null; }
  };

  function setGate(html, kind) {
    if (!gateEl) return;
    gateEl.className = 'ats-gate' + (kind ? ' ats-gate--' + kind : '');
    gateEl.innerHTML = html || '';
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
    if (countEl) countEl.textContent = rows.length + ' Nachweise';
    if (listEl) {
      const now = Date.now();
      listEl.innerHTML = rows.length
        ? rows.map((r) => credentialRow(r, now)).join('')
        : '<div class="ats-empty">Noch keine Nachweise erfasst.</div>';
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
    if (typeof dispatch !== 'function') { setGate('Offline: Befehlsdienst nicht verfügbar.', 'offline'); return; }
    setGate('Prüfe Einsatzbereitschaft…');
    let result;
    try {
      const decision = await dispatch({
        module: MODULE_ID,
        type: DEPLOY_COMMAND,
        command_type: DEPLOY_COMMAND,
        payload: { subject_id: subjectId, required_types: requiredTypes },
      });
      result = decision?.result || decision;
    } catch (e) {
      console.warn('[nachweise] deployment gate check failed:', e);
      setGate('Offline: Befehl konnte nicht gesendet werden.', 'offline');
      return;
    }
    const blockers = result?.blockers || [];
    const ready = result?.ready === true && !(Array.isArray(blockers) && blockers.length);
    if (!ready) {
      const items = (Array.isArray(blockers) ? blockers : [blockers])
        .filter(Boolean)
        .map((b) => '<li>' + esc(typeLabel(b?.credential_type) + ' — ' + (b?.reason || 'blockiert')) + '</li>')
        .join('');
      setGate(
        '<strong>✗ Nicht einsatzbereit: ' + esc(subjectId) + '</strong>'
        + (items ? '<ul class="ats-blockers">' + items + '</ul>' : ''),
        'block'
      );
      return;
    }
    setGate('<strong>✓ Einsatzbereit: ' + esc(subjectId) + '</strong>', 'ok');
  }

  // Create a credential (plain RxDB write — no native command for capture).
  async function onSubmit(event) {
    event.preventDefault();
    setGate('');
    const col = collection();
    if (!col?.insert) {
      ctx.notifications?.show?.({ type: 'error', title: 'Nachweise', message: 'Datenbank nicht verfügbar.' });
      setGate('Offline: Datenbank nicht verfügbar.', 'offline');
      return;
    }
    const data = new FormData(formEl);
    const f = Object.fromEntries(data.entries());
    const subjectId = String(f.subject_id || '').trim();
    if (!subjectId) { setGate('Subjekt-ID erforderlich.', 'block'); return; }
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
      setGate('Nachweis konnte nicht gespeichert werden.', 'block');
    }
  }
  formEl?.addEventListener('submit', onSubmit);

  // Server-authoritative deployment-readiness gate (ats.deployment.check).
  async function onGateCheck(event) {
    event.preventDefault();
    const data = new FormData(gateFormEl);
    const subjectId = String(data.get('subject_id') || '').trim();
    if (!subjectId) { setGate('Subjekt-ID erforderlich.', 'block'); return; }
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
    if (typeof dispatch !== 'function') { setGate('Offline: Befehlsdienst nicht verfügbar.', 'offline'); return; }
    const data = new FormData(signoffFormEl);
    const recordId = String(data.get('record_id') || '').trim();
    if (!recordId) { setGate('Leistungsnachweis-ID erforderlich.', 'block'); return; }
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
    setGate('Sign-off läuft…');
    let result;
    try {
      const decision = await dispatch({
        module: MODULE_ID,
        type: SIGNOFF_COMMAND,
        command_type: SIGNOFF_COMMAND,
        payload,
      });
      result = decision?.result || decision;
    } catch (e) {
      console.warn('[nachweise] signoff failed:', e);
      setGate('Offline: Befehl konnte nicht gesendet werden.', 'offline');
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
      setGate('<strong>Sign-off blockiert.</strong>' + (items ? '<ul class="ats-blockers">' + items + '</ul>' : ''), 'block');
      return;
    }
    const signedId = result?.record_id ?? recordId;
    const released = result?.billing_released === true;
    const invoiceId = result?.invoice_id ? String(result.invoice_id) : '';
    const netTotal = result?.net_total;
    setGate(
      '<strong>Entleiher-Sign-off gespeichert.</strong>'
      + '<div class="ats-result-row">Leistungsnachweis: ' + esc(signedId) + '</div>'
      + '<div class="ats-result-row">Abrechnung freigegeben: ' + (released ? 'ja' : 'nein') + '</div>'
      + (invoiceId ? '<div class="ats-result-row">Rechnung: ' + esc(invoiceId) + '</div>' : '')
      + (netTotal != null ? '<div class="ats-result-row">Netto: ' + esc(String(netTotal)) + ' €</div>' : ''),
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

function credentialRow(r, now) {
  const status = credentialStatus(r, now);
  const blocking = isDeploymentBlocking(r, now);
  const days = daysUntilExpiry(r, now);
  const daysLabel = Number.isFinite(days)
    ? (days < 0 ? 'abgelaufen vor ' + Math.abs(days) + ' T.' : 'noch ' + days + ' Tage')
    : 'kein Ablauf';
  const subject = r.subject_id || '—';
  const meta = [
    'Subjekt: ' + subject,
    r.issuer ? 'Aussteller: ' + r.issuer : '',
    daysLabel,
    r.verified_by ? 'verifiziert von ' + r.verified_by : '',
    r.id ? 'ID: ' + r.id : '',
  ].filter(Boolean).join(' · ');
  const checkBtn = r.subject_id
    ? '<button type="button" class="ats-action" data-deploy-check="' + esc(r.subject_id) + '">Einsatz prüfen</button>'
    : '';
  const recordId = r.id || '';
  const contextLabel = typeLabel(r.credential_type) + ' · ' + subject;
  return '<div class="ats-item ats-item--rich"'
    + ' data-context-record-id="' + esc(recordId) + '"'
    + ' data-context-record-type="nachweis"'
    + ' data-context-label="' + esc(contextLabel || recordId) + '">'
    + '<div class="ats-item-main">' + esc(typeLabel(r.credential_type))
    + '<span class="ats-item-sub"> · ' + esc(subject) + '</span></div>'
    + '<div class="ats-item-side">'
    + '<span class="ats-badge ats-badge--' + esc(status) + '">' + esc(status) + '</span>'
    + (blocking ? '<span class="ats-badge ats-badge--blocking">Einsatz blockierend</span>' : '')
    + checkBtn
    + '</div>'
    + '<div class="ats-item-meta">' + esc(meta) + '</div>'
    + '</div>';
}

function typeLabel(key) {
  return TYPE_LABEL.get(key) || key || 'Nachweis';
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
