const MOD_BUILD = '20260717-kit2';
const MODULE_ID = 'esign';
const PRIMARY = 'signature_requests';
const TITLE = 'esign';
const CREATE_COMMAND = 'ats.signature.request';
const SIGN_COMMAND = 'ats.signature.sign';
const SUBJECT_KINDS = ['arbeitsvertrag', 'vermittlungsvertrag', 'ueberlassungsvertrag'];
const COPY = {
  de: {
    document: 'Dokument-ID', subjectKind: 'Vertragstyp', employment: 'Arbeitsvertrag', placement: 'Vermittlungsvertrag', staffing: 'Überlassungsvertrag', signers: 'Unterzeichner-IDs (Komma-getrennt)', create: 'Anlegen', entries: 'Einträge', empty: 'Noch keine Einträge.', offlineService: 'Offline: Befehlsdienst nicht verfügbar.', offlineSend: 'Offline: Befehl konnte nicht gesendet werden.', signatureCaptured: 'Signatur erfasst.', request: 'Anfrage', signer: 'Unterzeichner', status: 'Status', blocked: 'Blockiert.', documentRequired: 'Dokument-ID erforderlich.', requestCreated: 'Signatur-Anfrage angelegt.', sign: 'Signieren', artifact: 'Artefakt'
  },
  en: {
    document: 'Document ID', subjectKind: 'Agreement type', employment: 'Employment contract', placement: 'Placement agreement', staffing: 'Staffing agreement', signers: 'Signer IDs (comma-separated)', create: 'Create', entries: 'records', empty: 'No signature requests yet.', offlineService: 'Offline: command service unavailable.', offlineSend: 'Offline: command could not be sent.', signatureCaptured: 'Signature recorded.', request: 'Request', signer: 'Signer', status: 'Status', blocked: 'Blocked.', documentRequired: 'Document ID is required.', requestCreated: 'Signature request created.', sign: 'Sign', artifact: 'Artifact'
  }
};
let text = COPY.de;

export async function mount(ctx) {
  text = COPY[ctx.locale === 'en' ? 'en' : 'de'];
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
  if (titleEl) titleEl.textContent = ctx.manifest?.title || TITLE;
  applyStaticCopy(root);

  let rowsCache = [];
  const collection = () => { try { return ctx.db?.collection?.(PRIMARY) || null; } catch { return null; } };

  // Gate feedback renders in the kit callout; kinds map onto its variants.
  const GATE_VARIANTS = { ok: 'is-success', block: 'is-danger', offline: 'is-warning' };
  function setGate(html, kind) {
    if (!gateEl) return;
    gateEl.className = 'ctox-callout' + (GATE_VARIANTS[kind] ? ' ' + GATE_VARIANTS[kind] : '');
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
    if (countEl) countEl.textContent = `${rows.length} ${text.entries}`;
    if (listEl) listEl.innerHTML = rows.length ? rows.map((r) => signatureRow(r)).join('') : `<div class="ctox-empty">${esc(text.empty)}</div>`;
  }

  async function onListClick(event) {
    const btn = event.target?.closest?.('[data-sign]');
    if (!btn) return;
    const requestId = btn.getAttribute('data-sign');
    const signerId = btn.getAttribute('data-signer');
    if (!requestId || !signerId) return;
    setGate('');
    const dispatch = ctx.commandBus?.dispatch;
    if (typeof dispatch !== 'function') { setGate(text.offlineService, 'offline'); return; }
    let result;
    try {
      result = await ctx.commandBus?.dispatch?.({
        module: MODULE_ID,
        command_type: SIGN_COMMAND,
        payload: { request_id: requestId, signer_id: signerId },
      });
    } catch (e) {
      console.error('[esign] sign dispatch failed:', e);
      setGate(text.offlineSend, 'offline');
      return;
    }

    if (renderBlocked(result)) return;
    const reqId = result?.request_id ?? requestId;
    const status = result?.status ?? '';
    setGate(
      `<strong>${esc(text.signatureCaptured)}</strong>`
      + `<div class="ats-result-row">${esc(text.request)}: ` + esc(reqId) + '</div>'
      + `<div class="ats-result-row">${esc(text.signer)}: ` + esc(signerId) + '</div>'
      + (status ? `<div class="ats-result-row">${esc(text.status)}: ` + esc(status) + '</div>' : ''),
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
    setGate(`<strong>${esc(text.blocked)}</strong>` + (items ? '<ul class="ats-blockers">' + items + '</ul>' : ''), 'block');
    return true;
  }

  async function onSubmit(event) {
    event.preventDefault();
    setGate('');
    const dispatch = ctx.commandBus?.dispatch;
    if (typeof dispatch !== 'function') { setGate(text.offlineService, 'offline'); return; }
    const data = new FormData(formEl);
    const f = Object.fromEntries(data.entries());
    const document_id = String(f.document_id || '').trim();
    if (!document_id) { setGate(text.documentRequired, 'block'); return; }
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
        command_type: CREATE_COMMAND,
        payload,
      });
    } catch (e) {
      console.error('[esign] dispatch failed:', e);
      setGate(text.offlineSend, 'offline');
      return;
    }

    if (renderBlocked(result)) return;
    const reqId = result?.request_id ?? result?.data?.request_id ?? null;
    const status = result?.status ?? result?.data?.status ?? null;
    setGate(
      `<strong>${esc(text.requestCreated)}</strong>`
      + `<div class="ats-result-row">${esc(text.request)}: ` + esc(reqId ?? '—') + '</div>'
      + `<div class="ats-result-row">${esc(text.status)}: ` + esc(status ?? '—') + '</div>',
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
  metaParts.push(`${esc(text.request)}: ` + esc(r.id || '—'));
  if (total) metaParts.push(`${esc(text.signer)}: ` + signed + '/' + total);
  if (r.signed_artifact_id) metaParts.push(`${esc(text.artifact)}: ` + esc(r.signed_artifact_id));
  const meta = metaParts.join(' · ');

  const completed = status === 'completed' || status === 'declined' || status === 'expired';
  const actions = completed ? '' : signers
    .filter((s) => s && s.id && s.state !== 'signed' && s.state !== 'declined')
    .map((s) => '<button type="button" class="ctox-button ctox-button--sm" data-sign="' + esc(r.id || '') + '" data-signer="' + esc(s.id) + '">' + esc(text.sign) + ': ' + esc(s.id) + '</button>')
    .join('');

  const badgeClass = ('ctox-badge ' + statusBadgeClass(status)).trim();
  const recordId = r.id || r.document_id || '';
  const recordLabel = r.document_id || r.id || '—';
  return '<div class="ats-item"'
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

function applyStaticCopy(root) {
  root.querySelectorAll('[data-copy]').forEach((node) => { node.textContent = text[node.dataset.copy] || node.textContent; });
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
