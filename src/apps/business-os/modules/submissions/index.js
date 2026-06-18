import { evaluateSubmissionGuard } from './core/submission.js';
const MOD_BUILD = '20260618-ats2';
const PRIMARY = 'submissions';
const TITLE = "submissions";

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

  async function onSubmit(event) {
    event.preventDefault();
    if (gateEl) gateEl.textContent = '';
    const col = collection();
    if (!col?.insert) { ctx.notifications?.show?.({ type: 'error', title: TITLE, message: 'Datenbank nicht verfügbar.' }); return; }
    const data = new FormData(formEl);
    const f = Object.fromEntries(data.entries());
    const now = Date.now();

    // Server-authoritative consent gate before presenting a candidate.
    try {
      const decision = await ctx.commandBus?.dispatch?.({
        module: 'submissions', type: 'ats.consent.check', command_type: 'ats.consent.check',
        payload: { subject_id: f.candidate_id, purpose: 'present_to_client' },
      });
      const allowed = decision?.result?.allowed ?? decision?.allowed;
      if (allowed === false) {
        if (gateEl) gateEl.textContent = 'Blockiert: keine gültige Einwilligung (present_to_client).';
        return;
      }
    } catch (e) { console.warn('[submissions] consent gate check skipped:', e); }
    // Local double-submission guard (mirrors the native check) via the engine core.
    const guard = evaluateSubmissionGuard({ candidate_id: f.candidate_id, client_account_id: f.client_account_id }, { existingSubmissions: rowsCache, hasConsent: true, nowMs: now });
    if (!guard.allowed) { if (gateEl) gateEl.textContent = 'Blockiert: ' + guard.blockers.map((b) => b.reason).join(', '); return; }
    const record = Object.assign({ id: 'subm_' + now + '_' + Math.round(now % 1e6), created_at_ms: now, updated_at_ms: now, _deleted: false }, { candidate_id: f.candidate_id, client_account_id: f.client_account_id, sent_at_ms: now, status: 'sent' });
    try { await col.insert(record); formEl.reset(); await render(); }
    catch (e) { console.error('[submissions] insert failed:', e); if (gateEl) gateEl.textContent = 'Fehler: ' + (e?.message || e); }
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
