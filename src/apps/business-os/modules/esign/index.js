
const MOD_BUILD = '20260618-ats2';
const MODULE_ID = 'esign';
const PRIMARY = 'signature_requests';
const TITLE = "esign";
const COMMAND_TYPE = 'ats.signature.request';
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
  const collection = () => { try { return ctx.db?.collection?.(PRIMARY) || ctx.db?.[PRIMARY] || null; } catch { return null; } };

  async function render() {
    const col = collection();
    let rows = [];
    if (col?.find) {
      try { const docs = await col.find({ selector: {} }).exec(); rows = docs.map((d) => (typeof d.toJSON === 'function' ? d.toJSON() : d)).filter((r) => !r._deleted); }
      catch (e) { console.error('[esign] load failed:', e); }
    }
    rowsCache = rows;
    if (countEl) countEl.textContent = rows.length + ' Einträge';
    if (listEl) listEl.innerHTML = rows.length ? rows.map((r) => '<div class="ats-item">' + esc(r.id || '') + '</div>').join('') : '<div class="ats-empty">Noch keine Einträge.</div>';
  }

  function setGate(html, kind) {
    if (!gateEl) return;
    gateEl.className = 'ats-gate' + (kind ? ' is-' + kind : '');
    gateEl.innerHTML = html || '';
  }

  async function onSubmit(event) {
    event.preventDefault();
    setGate('');
    const data = new FormData(formEl);
    const f = Object.fromEntries(data.entries());
    const document_id = String(f.document_id || '').trim();
    const subject_kind = SUBJECT_KINDS.includes(f.subject_kind) ? f.subject_kind : SUBJECT_KINDS[0];
    if (!document_id) { setGate('Dokument-ID erforderlich.', 'error'); return; }

    const payload = { document_id, subject_kind, signers: [] };
    let result = null;
    try {
      result = await ctx.commandBus?.dispatch?.({
        module: MODULE_ID,
        type: COMMAND_TYPE,
        command_type: COMMAND_TYPE,
        payload,
      });
    } catch (e) {
      console.error('[esign] dispatch failed:', e);
      setGate('Offline – Befehl konnte nicht gesendet werden.', 'error');
      return;
    }

    if (!result) { setGate('Offline – kein Ergebnis vom Server.', 'error'); return; }

    const blockers = Array.isArray(result.blockers) ? result.blockers
      : (Array.isArray(result.gate?.blockers) ? result.gate.blockers : []);
    const status = result.status || result.gate?.decision || '';
    const blocked = (status === 'blocked' || status === 'denied' || result.gate?.decision === 'block') || blockers.length > 0;

    if (blocked) {
      const items = blockers.length
        ? '<ul class="ats-blockers">' + blockers.map((b) => '<li>' + esc(typeof b === 'string' ? b : (b?.reason || b?.message || JSON.stringify(b))) + '</li>').join('') + '</ul>'
        : '';
      setGate('Gate blockiert' + (status ? ' (' + esc(status) + ')' : '') + items, 'error');
      return;
    }

    const reqId = result.request_id || result.id || '';
    setGate('Angelegt: ' + esc(reqId) + (status ? ' · ' + esc(status) : ''), 'ok');
    formEl.reset();
    await render();
  }
  formEl?.addEventListener('submit', onSubmit);

  let sub = null;
  const col = collection();
  if (col?.find) { try { sub = col.find({ selector: {} }).$?.subscribe?.(() => { render().catch(() => {}); }); } catch {} }
  await render();

  return () => { try { sub?.unsubscribe?.(); } catch {} formEl?.removeEventListener('submit', onSubmit); ctx.host.replaceChildren(); delete ctx.host.dataset.atsModule; };
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
