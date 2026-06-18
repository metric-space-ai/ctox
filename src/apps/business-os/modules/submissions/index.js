const MOD_BUILD = '20260618-ats3';
const MODULE_ID = 'submissions';
const PRIMARY = 'submissions';
const TITLE = "submissions";
const COMMAND_TYPE = 'ats.submission.present';

export async function mount(ctx) {
  await ensureStyles();
  ctx.host.innerHTML = await loadMarkup();
  ctx.host.dataset.atsModule = 'submissions';
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();
  const root = ctx.host.querySelector('[data-ats-root]');
  const listEl = root?.querySelector('[data-ats-list]');
  const countEl = root?.querySelector('[data-ats-count]');
  const formEl = root?.querySelector('[data-ats-form]');
  const gateEl = root?.querySelector('[data-ats-gate]');
  const statusEl = root?.querySelector('[data-ats-status]');
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
      catch (e) { console.error('[submissions] load failed:', e); }
    }
    rowsCache = rows;
    if (countEl) countEl.textContent = rows.length + ' Einträge';
    if (listEl) listEl.innerHTML = rows.length ? rows.map((r) => '<div class="ats-item">' + esc(r.id || '') + '</div>').join('') : '<div class="ats-empty">Noch keine Einträge.</div>';
  }

  function setGate(text) { if (gateEl) { gateEl.textContent = text || ''; gateEl.hidden = !text; } }
  function setStatus(text) { if (statusEl) { statusEl.textContent = text || ''; statusEl.hidden = !text; } }

  async function onSubmit(event) {
    event.preventDefault();
    setGate('');
    setStatus('');
    const data = new FormData(formEl);
    const f = Object.fromEntries(data.entries());
    const candidate_id = String(f.candidate_id || '').trim();
    const client_account_id = String(f.client_account_id || '').trim();
    if (!candidate_id || !client_account_id) {
      setGate('Kandidat-ID und Kunden-ID sind erforderlich.');
      return;
    }

    // Server-authoritative present: the native command owns the consent gate and
    // the double-submission entitlement check. We never write RxDB directly.
    if (typeof ctx.commandBus?.dispatch !== 'function') {
      setStatus('Offline: Server nicht erreichbar — bitte später erneut versuchen.');
      return;
    }

    setStatus('Wird übermittelt …');
    let outcome = null;
    try {
      outcome = await ctx.commandBus.dispatch({
        module: MODULE_ID,
        type: COMMAND_TYPE,
        command_type: COMMAND_TYPE,
        payload: { candidate_id, client_account_id },
      });
    } catch (e) {
      console.warn('[submissions] present dispatch failed:', e);
      setStatus('Offline: Befehl konnte nicht zugestellt werden.');
      return;
    }

    const result = outcome?.result ?? outcome ?? {};
    const allowed = result.allowed;
    const blockers = Array.isArray(result.blockers) ? result.blockers : [];
    if (allowed === false) {
      const reasons = blockers.map((b) => String(b?.reason || 'unknown')).join(', ');
      setGate('Blockiert: ' + (reasons || 'gate_denied'));
      setStatus('');
      return; // gate block — keep the form, not a success
    }

    const submissionId = result.submission_id || '';
    setStatus(submissionId ? ('Übermittelt — ID: ' + String(submissionId)) : 'Übermittelt.');
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
