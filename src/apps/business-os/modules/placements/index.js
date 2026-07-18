const MOD_BUILD = '20260718-kit2';
const MODULE_ID = 'placements';
const PRIMARY = 'placements';
const TITLE = 'placements';
const CREATE_COMMAND = 'ats.placement.create';
const COPY = {
  de: {
    kicker: 'Vermittlungen', candidate: 'Kandidat-ID', client: 'Kunden-Account-ID', placementType: 'Vermittlungsart', directHire: 'Festanstellung (Personalvermittlung)', temporary: 'Arbeitnehmerüberlassung (Zeitarbeit)', qualifications: 'Pflicht-Qualifikationen (Komma)', fee: 'Honorar', guarantee: 'Garantie (Tage)', create: 'Anlegen', entries: 'Einträge', empty: 'Noch keine Einträge.', earlyLeaveBooked: 'Frühausstieg verbucht.', clawback: 'Clawback', credit: 'Gutschrift', offlineService: 'Offline: Befehlsdienst nicht verfügbar.', offlineSend: 'Offline: Befehl konnte nicht gesendet werden.', candidateRequired: 'Kandidat-ID erforderlich.', blocked: 'Blockiert.', placementCreated: 'Placement angelegt.', placement: 'Placement', feeInvoice: 'Honorar-Rechnung', days: 'Tage', invoice: 'Rechnung', cancellation: 'Storno', earlyLeave: 'Frühausstieg'
  },
  en: {
    kicker: 'Placements', candidate: 'Candidate ID', client: 'Client account ID', placementType: 'Placement type', directHire: 'Permanent placement', temporary: 'Temporary staffing', qualifications: 'Required qualifications (comma-separated)', fee: 'Fee', guarantee: 'Guarantee (days)', create: 'Create', entries: 'records', empty: 'No placements yet.', earlyLeaveBooked: 'Early leave recorded.', clawback: 'Clawback', credit: 'Credit note', offlineService: 'Offline: command service unavailable.', offlineSend: 'Offline: command could not be sent.', candidateRequired: 'Candidate ID is required.', blocked: 'Blocked.', placementCreated: 'Placement created.', placement: 'Placement', feeInvoice: 'Fee invoice', days: 'days', invoice: 'Invoice', cancellation: 'Cancellation', earlyLeave: 'Early leave'
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
  const subEl = root?.querySelector('[data-ats-sub]');
  if (titleEl) titleEl.textContent = ctx.manifest?.title || TITLE;
  if (subEl) subEl.textContent = ctx.manifest?.description || '';
  applyStaticCopy(root);

  let rowsCache = [];
  const collection = () => { try { return ctx.db?.collection?.(PRIMARY) || null; } catch { return null; } };

  // Gate callout → kit .ctox-callout variants (base.css).
  const GATE_VARIANTS = { ok: ' is-success', block: ' is-danger', offline: ' is-warning' };
  function setGate(html, kind) {
    if (!gateEl) return;
    gateEl.className = 'ctox-callout' + (GATE_VARIANTS[kind] || '');
    gateEl.innerHTML = html || '';
  }

  async function render() {
    const col = collection();
    let rows = [];
    if (col?.find) {
      try { const docs = await col.find({ selector: {} }).exec(); rows = docs.map((d) => (typeof d.toJSON === 'function' ? d.toJSON() : d)).filter((r) => !r._deleted); }
      catch (e) { console.error('[placements] load failed:', e); }
    }
    rowsCache = rows;
    if (countEl) countEl.textContent = `${rows.length} ${text.entries}`;
    if (listEl) listEl.innerHTML = rows.length
      ? rows.map((r) => placementRow(r)).join('')
      : `<div class="ctox-empty">${esc(text.empty)}</div>`;
  }

  async function onListClick(event) {
    const btn = event.target?.closest?.('[data-early-leave]');
    if (!btn) return;
    const placementId = btn.getAttribute('data-early-leave');
    if (!placementId) return;
    setGate('');
    try {
      const result = await ctx.commandBus?.dispatch?.({
        module: MODULE_ID,
        command_type: 'ats.placement.early_leave',
        payload: { placement_id: placementId, left_at_ms: Date.now() },
      });
      const cn = result?.credit_note_id ?? result?.data?.credit_note_id ?? null;
      const clawback = result?.clawback ?? result?.data?.clawback ?? null;
      setGate(
        `<strong>${esc(text.earlyLeaveBooked)}</strong>`
        + (clawback != null ? `<div class="ats-result-row">${esc(text.clawback)}: ` + esc(String(clawback)) + '</div>' : '')
        + (cn ? `<div class="ats-result-row">${esc(text.credit)}: ` + esc(cn) + '</div>' : ''),
        'ok',
      );
      await render();
    } catch (e) {
      console.error('[placements] early_leave dispatch failed:', e);
      setGate(text.offlineSend, 'offline');
    }
  }
  listEl?.addEventListener('click', onListClick);

  async function onSubmit(event) {
    event.preventDefault();
    setGate('');
    const dispatch = ctx.commandBus?.dispatch;
    if (typeof dispatch !== 'function') {
      setGate(text.offlineService, 'offline');
      return;
    }
    const data = new FormData(formEl);
    const f = Object.fromEntries(data.entries());
    const candidate_id = String(f.candidate_id || '').trim();
    if (!candidate_id) { setGate(text.candidateRequired, 'block'); return; }
    const placementType = String(f.placement_type || '').trim();
    const requiredTypes = String(f.required_types || '')
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean);
    const payload = {
      candidate_id,
      client_account_id: String(f.client_account_id || '').trim() || null,
      placement_type: placementType || null,
      required_types: requiredTypes.length ? requiredTypes : undefined,
      fee: f.fee === '' || f.fee == null ? null : Number(f.fee),
      guarantee_days: f.guarantee_days === '' || f.guarantee_days == null ? null : Number(f.guarantee_days),
    };

    let result;
    try {
      result = await ctx.commandBus?.dispatch?.({
        module: MODULE_ID,
        command_type: CREATE_COMMAND,
        payload,
      });
    } catch (e) {
      console.error('[placements] dispatch failed:', e);
      setGate(text.offlineSend, 'offline');
      return;
    }

    const decision = result?.gate || result?.decision || null;
    const blockers = result?.blockers || decision?.blockers || result?.errors || null;
    const blocked = result?.ok === false || result?.status === 'blocked' || decision?.status === 'blocked' || decision?.decision === 'block' || (Array.isArray(blockers) && blockers.length > 0);

    if (blocked) {
      const items = (Array.isArray(blockers) ? blockers : [blockers])
        .filter(Boolean)
        .map((b) => '<li>' + esc(typeof b === 'string' ? b : (b?.message || b?.reason || JSON.stringify(b))) + '</li>')
        .join('');
      setGate(`<strong>${esc(text.blocked)}</strong>` + (items ? '<ul class="ats-blockers">' + items + '</ul>' : ''), 'block');
      return;
    }

    const placementId = result?.placement_id ?? result?.data?.placement_id ?? null;
    const feeInvoiceId = result?.fee_invoice_id ?? result?.data?.fee_invoice_id ?? null;
    setGate(
      `<strong>${esc(text.placementCreated)}</strong>`
      + `<div class="ats-result-row">${esc(text.placement)}: ` + esc(placementId ?? '—') + '</div>'
      + `<div class="ats-result-row">${esc(text.feeInvoice)}: ` + esc(feeInvoiceId ?? '—') + '</div>',
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

  return () => { try { sub?.unsubscribe?.(); } catch {} formEl?.removeEventListener('submit', onSubmit); listEl?.removeEventListener('click', onListClick); ctx.host.replaceChildren(); delete ctx.host.dataset.atsModule; };
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
// Maps a placement status onto the kit badge states
// (confirmed/success, early_leave/warning, cancelled/danger, sonst neutral).
function statusBadgeClass(status) {
  if (status === 'confirmed') return 'is-success';
  if (status === 'early_leave') return 'is-warning';
  if (status === 'cancelled') return 'is-danger';
  return '';
}

function placementRow(r) {
  const status = String(r.status || 'confirmed');
  const active = status !== 'early_leave' && status !== 'cancelled';
  const fee = r.fee == null ? '—' : esc(String(r.fee));
  const badgeClass = ('ctox-badge ' + statusBadgeClass(status)).trim();
  return '<div class="ats-item ats-item--rich" data-id="' + esc(r.id || '') + '" data-context-record-id="' + esc(r.id || '') + '" data-context-record-type="placement" data-context-label="' + esc((r.candidate_id || '') + ' → ' + (r.client_account_id || '')) + '">'
    + '<div class="ats-item-main">'
    + '<span class="' + badgeClass + '" data-status="' + esc(status) + '">' + esc(status) + '</span> '
    + '<strong>' + esc(r.candidate_id || '—') + '</strong> &rarr; ' + esc(r.client_account_id || '—')
    + '</div>'
    + `<div class="ats-item-meta">${esc(text.fee)}: ` + fee + ` &middot; ${esc(text.guarantee)}: ` + esc(String(r.guarantee_days ?? '—')) + ` ${esc(text.days)}`
    + (r.fee_invoice_id ? ` &middot; ${esc(text.invoice)}: ` + esc(r.fee_invoice_id) : '')
    + (r.storno_credit_note_id ? ` &middot; ${esc(text.cancellation)}: ` + esc(r.storno_credit_note_id) : '')
    + '</div>'
    + (active ? '<button type="button" class="ctox-button ats-action" data-early-leave="' + esc(r.id || '') + '">' + esc(text.earlyLeave) + '</button>' : '')
    + '</div>';
}
function applyStaticCopy(root) {
  root.querySelectorAll('[data-copy]').forEach((node) => { node.textContent = text[node.dataset.copy] || node.textContent; });
  root.querySelectorAll('[data-copy-placeholder]').forEach((node) => { node.placeholder = text[node.dataset.copyPlaceholder] || node.placeholder; });
}
function esc(v) { return String(v == null ? '' : v).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;'); }
