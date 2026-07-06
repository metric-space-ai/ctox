const MOD_BUILD = '20260706-kit1';
const MODULE_ID = 'esign';
const PRIMARY = 'signature_requests';
const TITLE = 'esign';
const CREATE_COMMAND = 'ats.signature.request';
const SIGN_COMMAND = 'ats.signature.sign';
const SUBJECT_KINDS = ['arbeitsvertrag', 'vermittlungsvertrag', 'ueberlassungsvertrag'];

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
  const gateEl = root?.querySelector('[data-ats-gate]');
  const titleEl = root?.querySelector('[data-ats-title]');
  const subEl = root?.querySelector('[data-ats-sub]');
  if (titleEl) titleEl.textContent = ctx.manifest?.title || TITLE;
  if (subEl) subEl.textContent = ctx.manifest?.description || '';

  let rowsCache = [];
  const collection = () => { try { return ctx.db?.collection?.(PRIMARY) || null; } catch { return null; } };

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
      catch (e) { console.error('[esign] load failed:', e); }
    }
    rows.sort((a, b) => (b.updated_at_ms || 0) - (a.updated_at_ms || 0));
    rowsCache = rows;
    if (countEl) countEl.textContent = rows.length + ' Einträge';
    if (listEl) listEl.innerHTML = rows.length ? rows.map((r) => signatureRow(r)).join('') : '<div class="ctox-empty">Noch keine Einträge.</div>';
  }

  async function onListClick(event) {
    const btn = event.target?.closest?.('[data-sign]');
    if (!btn) return;
    const requestId = btn.getAttribute('data-sign');
    const signerId = btn.getAttribute('data-signer');
    if (!requestId || !signerId) return;
    setGate('');
    const dispatch = ctx.commandBus?.dispatch;
    if (typeof dispatch !== 'function') { setGate('Offline: Befehlsdienst nicht verfügbar.', 'offline'); return; }
    let result;
    try {
      result = await ctx.commandBus?.dispatch?.({
        module: MODULE_ID,
        type: SIGN_COMMAND,
        command_type: SIGN_COMMAND,
        payload: { request_id: requestId, signer_id: signerId },
      });
    } catch (e) {
      console.error('[esign] sign dispatch failed:', e);
      setGate('Offline: Befehl konnte nicht gesendet werden.', 'offline');
      return;
    }

    if (renderBlocked(result)) return;
    const reqId = result?.request_id ?? requestId;
    const status = result?.status ?? '';
    setGate(
      '<strong>Signatur erfasst.</strong>'
      + '<div class="ats-result-row">Anfrage: ' + esc(reqId) + '</div>'
      + '<div class="ats-result-row">Unterzeichner: ' + esc(signerId) + '</div>'
      + (status ? '<div class="ats-result-row">Status: ' + esc(status) + '</div>' : ''),
      'ok'
    );
    await render();
  }
  listEl?.addEventListener('click', onListClick);

  function renderBlocked(result) {
    const decision = result?.gate || result?.decision || null;
    const blockers = result?.blockers || decision?.blockers || result?.errors || null;
    const blocked = !result || result?.ok === false || result?.status === 'blocked'
      || decision?.status === 'blocked' || decision?.decision === 'block'
      || (Array.isArray(blockers) && blockers.length > 0);
    if (!blocked) return false;
    const items = (Array.isArray(blockers) ? blockers : [blockers])
      .filter(Boolean)
      .map((b) => '<li>' + esc(typeof b === 'string' ? b : (b?.message || b?.reason || JSON.stringify(b))) + '</li>')
      .join('');
    setGate('<strong>Blockiert.</strong>' + (items ? '<ul class="ats-blockers">' + items + '</ul>' : ''), 'block');
    return true;
  }

  async function onSubmit(event) {
    event.preventDefault();
    setGate('');
    const dispatch = ctx.commandBus?.dispatch;
    if (typeof dispatch !== 'function') { setGate('Offline: Befehlsdienst nicht verfügbar.', 'offline'); return; }
    const data = new FormData(formEl);
    const f = Object.fromEntries(data.entries());
    const document_id = String(f.document_id || '').trim();
    if (!document_id) { setGate('Dokument-ID erforderlich.', 'block'); return; }
    const subject_kind = SUBJECT_KINDS.includes(f.subject_kind) ? f.subject_kind : SUBJECT_KINDS[0];
    const signers = String(f.signers || '')
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean)
      .map((id) => ({ id, state: 'pending' }));
    const payload = { document_id, subject_kind, signers };

    let result;
    try {
      result = await ctx.commandBus?.dispatch?.({
        module: MODULE_ID,
        type: CREATE_COMMAND,
        command_type: CREATE_COMMAND,
        payload,
      });
    } catch (e) {
      console.error('[esign] dispatch failed:', e);
      setGate('Offline: Befehl konnte nicht gesendet werden.', 'offline');
      return;
    }

    if (renderBlocked(result)) return;
    const reqId = result?.request_id ?? result?.data?.request_id ?? null;
    const status = result?.status ?? result?.data?.status ?? null;
    setGate(
      '<strong>Signatur-Anfrage angelegt.</strong>'
      + '<div class="ats-result-row">Anfrage: ' + esc(reqId ?? '—') + '</div>'
      + '<div class="ats-result-row">Status: ' + esc(status ?? '—') + '</div>',
      'ok'
    );
    try { formEl.reset(); } catch {}
    await render();
  }
  formEl?.addEventListener('submit', onSubmit);

  let sub = null;
  const col = collection();
  if (col?.find) { try { sub = col.find({ selector: {} }).$?.subscribe?.(() => { render().catch(() => {}); }); } catch {} }
  await render();

  return () => {
    try { sub?.unsubscribe?.(); } catch {}
    formEl?.removeEventListener('submit', onSubmit);
    listEl?.removeEventListener('click', onListClick);
    ctx.host.replaceChildren();
    delete ctx.host.dataset.atsModule;
  };
}

// Maps a signature-request status onto the kit badge states
// (created/neutral, waiting/warning, completed/success, terminal-negative/danger).
function statusBadgeClass(status) {
  if (status === 'completed') return 'is-success';
  if (status === 'declined' || status === 'expired') return 'is-danger';
  if (status === 'sent' || status === 'partially_signed') return 'is-warning';
  return '';
}

function signatureRow(r) {
  const status = String(r.status || 'created');
  const signers = Array.isArray(r.signers) ? r.signers : [];
  const signed = signers.filter((s) => s && s.state === 'signed').length;
  const total = signers.length;
  const main = esc(r.document_id || r.id || '—')
    + (r.subject_kind ? ' · ' + esc(r.subject_kind) : '');
  const metaParts = [];
  metaParts.push('Anfrage: ' + esc(r.id || '—'));
  if (total) metaParts.push('Unterzeichner: ' + signed + '/' + total);
  if (r.signed_artifact_id) metaParts.push('Artefakt: ' + esc(r.signed_artifact_id));
  const meta = metaParts.join(' · ');

  const completed = status === 'completed' || status === 'declined' || status === 'expired';
  const actions = completed ? '' : signers
    .filter((s) => s && s.id && s.state !== 'signed' && s.state !== 'declined')
    .map((s) => '<button type="button" class="ctox-button" data-sign="' + esc(r.id || '') + '" data-signer="' + esc(s.id) + '">Signieren: ' + esc(s.id) + '</button>')
    .join('');

  const badgeClass = ('ctox-badge ' + statusBadgeClass(status)).trim();
  const recordId = r.id || r.document_id || '';
  const recordLabel = r.document_id || r.id || '—';
  return '<div class="ats-item ats-item--rich"'
    + ' data-context-record-id="' + esc(recordId) + '"'
    + ' data-context-record-type="esign_document"'
    + ' data-context-label="' + esc(recordLabel) + '">'
    + '<div class="ats-item-main">'
    + '<div><span class="' + badgeClass + '" data-status="' + esc(status) + '">' + esc(status) + '</span> ' + main + '</div>'
    + '<div class="ats-item-meta">' + meta + '</div>'
    + '</div>'
    + '<div class="ats-item-actions">' + actions + '</div>'
    + '</div>';
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
