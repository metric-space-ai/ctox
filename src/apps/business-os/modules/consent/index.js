
const MOD_BUILD = '20260620-ats9';
const MODULE_ID = 'consent';
const PRIMARY = 'business_consents';
const TITLE = 'consent';
const CHECK_COMMAND = 'ats.consent.check';
const EXPORT_COMMAND = 'ats.subject.export';
const ERASE_COMMAND = 'ats.subject.erase';

export async function mount(ctx) {
  await ensureStyles();
  ctx.host.innerHTML = await loadMarkup();
  ctx.host.dataset.atsModule = MODULE_ID;
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();
  const root = ctx.host.querySelector('[data-ats-root]');
  const listEl = root?.querySelector('[data-ats-list]');
  const countEl = root?.querySelector('[data-ats-count]');
  const formEl = root?.querySelector('[data-ats-form]');
  const subjectFormEl = root?.querySelector('[data-ats-subject-form]');
  const gateEl = root?.querySelector('[data-ats-gate]');
  const titleEl = root?.querySelector('[data-ats-title]');
  const subEl = root?.querySelector('[data-ats-sub]');
  if (titleEl) titleEl.textContent = ctx.manifest?.title || TITLE;
  if (subEl) subEl.textContent = ctx.manifest?.description || '';

  let rowsCache = [];
  const collection = () => { try { return ctx.db?.collection?.(PRIMARY) || ctx.db?.[PRIMARY] || null; } catch { return null; } };

  function setGate(html, kind) {
    if (!gateEl) return;
    gateEl.className = 'ats-gate' + (kind ? ' ats-gate--' + kind : '');
    gateEl.innerHTML = html || '';
  }

  async function render() {
    const col = collection();
    let rows = [];
    if (col?.find) {
      try { const docs = await col.find({ selector: {} }).exec(); rows = docs.map((d) => (typeof d.toJSON === 'function' ? d.toJSON() : d)).filter((r) => !r._deleted); }
      catch (e) { console.error('[consent] load failed:', e); }
    }
    rowsCache = rows;
    if (countEl) countEl.textContent = rows.length + ' Einträge';
    if (listEl) listEl.innerHTML = rows.length ? rows.map((r) => consentRow(r)).join('') : '<div class="ats-empty">Noch keine Einträge.</div>';
  }

  // ats.consent.check — reads { subject_id, purpose? }, returns { ok, allowed, purpose }.
  async function onCheckSubmit(event) {
    event.preventDefault();
    setGate('');
    const dispatch = ctx.commandBus?.dispatch;
    if (typeof dispatch !== 'function') { setGate('Offline: Befehlsdienst nicht verfügbar.', 'offline'); return; }
    const f = Object.fromEntries(new FormData(formEl).entries());
    const subject_id = String(f.subject_id || '').trim();
    if (!subject_id) { setGate('Subjekt-ID erforderlich.', 'block'); return; }
    const purposeRaw = String(f.purpose || '').trim();
    const payload = { subject_id, purpose: purposeRaw || null };

    let result;
    try { result = await dispatch.call(ctx.commandBus, { module: MODULE_ID, type: CHECK_COMMAND, command_type: CHECK_COMMAND, payload }); }
    catch (e) { console.error('[consent] check dispatch failed:', e); setGate('Offline: Befehl konnte nicht gesendet werden.', 'offline'); return; }

    const blockers = collectBlockers(result);
    if (result?.ok === false || (Array.isArray(blockers) && blockers.length > 0)) {
      setGate('<strong>Prüfung fehlgeschlagen.</strong>' + blockerList(blockers), 'block');
      return;
    }
    const allowed = result?.allowed === true || result?.data?.allowed === true;
    const checkedPurpose = result?.purpose ?? result?.data?.purpose ?? (purposeRaw || null);
    setGate(
      '<strong>' + (allowed ? 'Einwilligung vorhanden.' : 'Keine gültige Einwilligung.') + '</strong>'
      + '<div class="ats-result-row">Subjekt: ' + esc(subject_id) + '</div>'
      + '<div class="ats-result-row">Zweck: ' + esc(checkedPurpose ?? '— (nur Existenzprüfung)') + '</div>'
      + '<div class="ats-result-row">Entscheidung: ' + (allowed ? 'erlaubt' : 'verweigert') + '</div>',
      allowed ? 'ok' : 'block'
    );
  }
  formEl?.addEventListener('submit', onCheckSubmit);

  // Betroffenenrechte: Art. 15 export / Art. 17 erase, both keyed by subject_id.
  async function dispatchSubject(commandType, subjectId, label) {
    setGate('');
    const dispatch = ctx.commandBus?.dispatch;
    if (typeof dispatch !== 'function') { setGate('Offline: Befehlsdienst nicht verfügbar.', 'offline'); return; }
    const subject_id = String(subjectId || '').trim();
    if (!subject_id) { setGate('Subjekt-ID erforderlich.', 'block'); return; }

    let result;
    try { result = await dispatch.call(ctx.commandBus, { module: MODULE_ID, type: commandType, command_type: commandType, payload: { subject_id } }); }
    catch (e) { console.error('[consent] ' + commandType + ' dispatch failed:', e); setGate('Offline: Befehl konnte nicht gesendet werden.', 'offline'); return; }

    const blockers = collectBlockers(result);
    if (result?.ok === false || (Array.isArray(blockers) && blockers.length > 0)) {
      setGate('<strong>' + esc(label) + ' blockiert.</strong>' + blockerList(blockers), 'block');
      return;
    }
    renderSubjectResult(commandType, label, subject_id, result);
    if (commandType === ERASE_COMMAND) { try { await render(); } catch {} }
  }

  function renderSubjectResult(commandType, label, subjectId, result) {
    const data = result?.data || result || {};
    const exportPayload = data.export ?? data.record ?? data.records ?? data.subject ?? data.data ?? null;
    const erasedIds = data.erased_ids ?? data.deleted_ids ?? data.removed_ids ?? data.ids ?? null;
    const count = data.count ?? (Array.isArray(erasedIds) ? erasedIds.length : (Array.isArray(exportPayload) ? exportPayload.length : null));
    let body = '<div class="ats-result-row">Subjekt: ' + esc(subjectId) + '</div>';
    if (count != null) body += '<div class="ats-result-row">Betroffene Datensätze: ' + esc(count) + '</div>';
    if (Array.isArray(erasedIds) && erasedIds.length) {
      body += '<div class="ats-result-row">Gelöschte IDs:</div><ul class="ats-blockers">'
        + erasedIds.map((id) => '<li>' + esc(typeof id === 'string' ? id : (id?.id ?? JSON.stringify(id))) + '</li>').join('')
        + '</ul>';
    }
    if (commandType === EXPORT_COMMAND && exportPayload != null) {
      let pretty;
      try { pretty = JSON.stringify(exportPayload, null, 2); } catch { pretty = String(exportPayload); }
      body += '<div class="ats-result-row">Auskunft (Art. 15):</div><pre class="ats-export">' + esc(pretty) + '</pre>';
    }
    setGate('<strong>' + esc(label) + ' ausgeführt.</strong>' + body, 'ok');
  }

  async function onSubjectSubmit(event) {
    event.preventDefault();
    const action = event.submitter?.getAttribute?.('data-subject-action');
    const f = Object.fromEntries(new FormData(subjectFormEl).entries());
    const subjectId = f.subject_id;
    if (action === EXPORT_COMMAND) await dispatchSubject(EXPORT_COMMAND, subjectId, 'Auskunft (Art. 15)');
    else if (action === ERASE_COMMAND) await dispatchSubject(ERASE_COMMAND, subjectId, 'Löschen (Art. 17)');
  }
  subjectFormEl?.addEventListener('submit', onSubjectSubmit);

  // Per-row Betroffenenrechte buttons (delegated, mirrors placements' early_leave).
  async function onListClick(event) {
    const exportBtn = event.target?.closest?.('[data-subject-export]');
    const eraseBtn = event.target?.closest?.('[data-subject-erase]');
    if (exportBtn) { await dispatchSubject(EXPORT_COMMAND, exportBtn.getAttribute('data-subject-export'), 'Auskunft (Art. 15)'); return; }
    if (eraseBtn) { await dispatchSubject(ERASE_COMMAND, eraseBtn.getAttribute('data-subject-erase'), 'Löschen (Art. 17)'); }
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

const STATUS_LABEL = { active: 'gültig', withdrawn: 'widerrufen', expired: 'abgelaufen', pending: 'offen' };

function fmtDate(ms) {
  const n = Number(ms);
  if (!Number.isFinite(n) || n <= 0) return '—';
  try { return new Date(n).toLocaleDateString('de-DE'); } catch { return String(n); }
}

function consentRow(r) {
  const status = consentStatus(r);
  const subjectId = r.subject_id || '';
  const purpose = r.purpose || '—';
  const basis = r.legal_basis || 'consent';
  const validUntil = Number.isFinite(Number(r.expires_at_ms)) && Number(r.expires_at_ms) > 0 ? fmtDate(r.expires_at_ms) : '∞';
  const metaBits = [
    'Subjekt ' + esc(subjectId),
    'Rechtsgrundlage ' + esc(basis),
    'gültig bis ' + esc(validUntil),
    'erteilt ' + esc(fmtDate(r.granted_at_ms)),
  ];
  if (status === 'withdrawn') metaBits.push('widerrufen ' + esc(fmtDate(r.withdrawn_at_ms)));
  if (r.id) metaBits.push('#' + esc(r.id));
  return '<div class="ats-item ats-item--rich">'
    + '<div class="ats-item-main">'
    + '<span class="ats-badge ats-badge--' + esc(status) + '">' + esc(STATUS_LABEL[status] || status) + '</span>'
    + '<span class="ats-item-title">' + esc(purpose) + '</span>'
    + '<div class="ats-item-meta">' + metaBits.join(' · ') + '</div>'
    + '</div>'
    + '<div class="ats-item-actions">'
    + '<button type="button" class="ats-action" data-subject-export="' + esc(subjectId) + '" title="Recht auf Auskunft (DSGVO Art. 15)">Auskunft</button>'
    + '<button type="button" class="ats-action ats-action--danger" data-subject-erase="' + esc(subjectId) + '" title="Recht auf Löschung (DSGVO Art. 17)">Löschen</button>'
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
