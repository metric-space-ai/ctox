import { CREDENTIAL_TYPES, credentialStatus, daysUntilExpiry } from './core/credential.js';

const CREDENTIALS_BUILD = '20260618-credentials1';

export async function mount(ctx) {
  await ensureStyles();
  ctx.host.innerHTML = await loadModuleMarkup();
  ctx.host.dataset.credentialsModule = 'native';
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();

  const root = ctx.host.querySelector('[data-credentials-root]');
  const listEl = root?.querySelector('[data-credentials-list]');
  const formEl = root?.querySelector('[data-credentials-form]');
  const typeSelect = root?.querySelector('[data-credential-type]');

  if (typeSelect) {
    typeSelect.innerHTML = CREDENTIAL_TYPES
      .map((type) => `<option value="${type.key}">${escapeHtml(type.label)}</option>`)
      .join('');
  }

  const collection = () => {
    try {
      return ctx.db?.collection?.('business_credentials') || ctx.db?.business_credentials || null;
    } catch {
      return null;
    }
  };

  async function render() {
    const col = collection();
    let rows = [];
    if (col?.find) {
      try {
        const docs = await col.find({ selector: {} }).exec();
        rows = docs.map((doc) => (typeof doc.toJSON === 'function' ? doc.toJSON() : doc));
      } catch (error) {
        console.error('[credentials] load failed:', error);
      }
    }
    rows = rows.filter((row) => !row._deleted);
    if (!listEl) return;
    if (!rows.length) {
      listEl.innerHTML = '<div class="credentials-empty">Noch keine Nachweise erfasst.</div>';
      return;
    }
    const now = Date.now();
    listEl.innerHTML = rows
      .map((row) => {
        const status = credentialStatus(row, now);
        const days = daysUntilExpiry(row, now);
        const daysLabel = Number.isFinite(days) ? `${days} Tage` : 'kein Ablauf';
        return `
          <div class="credentials-item">
            <div>
              <strong>${escapeHtml(row.credential_type || 'Nachweis')}</strong>
              <span style="opacity:.6"> · ${escapeHtml(row.subject_id || '')}</span>
              <div style="font-size:12px;opacity:.6">${escapeHtml(row.issuer || '')} · ${daysLabel}</div>
            </div>
            <span class="credentials-status" data-status="${status}">${status}</span>
          </div>`;
      })
      .join('');
  }

  async function onSubmit(event) {
    event.preventDefault();
    const col = collection();
    if (!col?.insert) {
      ctx.notifications?.show?.({ type: 'error', title: 'Nachweise', message: 'Datenbank nicht verfügbar.' });
      return;
    }
    const data = new FormData(formEl);
    const validUntil = String(data.get('valid_until') || '');
    const now = Date.now();
    const record = {
      id: `cred_${now}_${Math.round((now % 1e6))}`,
      subject_id: String(data.get('subject_id') || '').trim(),
      subject_type: 'candidate',
      credential_type: String(data.get('credential_type') || ''),
      issuer: String(data.get('issuer') || '').trim(),
      valid_until_ms: validUntil ? Date.parse(validUntil) : 0,
      verified: false,
      status: 'unverified',
      created_at_ms: now,
      updated_at_ms: now,
      _deleted: false,
    };
    try {
      await col.insert(record);
      formEl.reset();
      await render();
    } catch (error) {
      console.error('[credentials] insert failed:', error);
    }
  }

  formEl?.addEventListener('submit', onSubmit);

  // Server-authoritative deployment-readiness gate (ats.deployment.check).
  const gateForm = root?.querySelector('[data-credentials-gate-form]');
  const gateResult = root?.querySelector('[data-credentials-gate-result]');
  async function onGateCheck(event) {
    event.preventDefault();
    if (gateResult) gateResult.textContent = 'Prüfe…';
    const data = new FormData(gateForm);
    const subjectId = String(data.get('subject_id') || '').trim();
    const requiredTypes = String(data.get('required_types') || '')
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean);
    try {
      const decision = await ctx.commandBus?.dispatch?.({
        module: MODULE_ID,
        type: 'ats.deployment.check',
        command_type: 'ats.deployment.check',
        payload: { subject_id: subjectId, required_types: requiredTypes },
      });
      const result = decision?.result || decision;
      if (gateResult) {
        gateResult.textContent = result?.ready
          ? '✓ einsatzbereit'
          : '✗ blockiert: ' + (result?.blockers || []).map((b) => `${b.credential_type} (${b.reason})`).join(', ');
      }
    } catch (error) {
      if (gateResult) gateResult.textContent = 'Prüfung nicht verfügbar (Gate offline).';
      console.warn('[credentials] deployment gate check failed:', error);
    }
  }
  gateForm?.addEventListener('submit', onGateCheck);

  let subscription = null;
  const col = collection();
  if (col?.find) {
    try {
      subscription = col.find({ selector: {} }).$?.subscribe?.(() => { render().catch(() => {}); });
    } catch { /* live sync optional */ }
  }
  await render();

  return () => {
    try { subscription?.unsubscribe?.(); } catch {}
    formEl?.removeEventListener('submit', onSubmit);
    gateForm?.removeEventListener('submit', onGateCheck);
    ctx.host.replaceChildren();
    delete ctx.host.dataset.credentialsModule;
  };
}

async function ensureStyles() {
  const href = `${new URL('./index.css', import.meta.url).pathname}?v=${CREDENTIALS_BUILD}`;
  if (document.querySelector(`link[href="${href}"]`)) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  document.head.append(link);
}

async function loadModuleMarkup() {
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}

function escapeHtml(value) {
  return String(value == null ? '' : value)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}
