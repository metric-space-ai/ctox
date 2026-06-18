const MODULE_ID = 'intake';
const MOD_BUILD = '20260618-ats2';
const PRIMARY = 'applications';
const TITLE = 'intake';
const COMMAND_TYPE = 'ats.intake.capture';

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
  const statusEl = root?.querySelector('[data-ats-status]');
  const titleEl = root?.querySelector('[data-ats-title]');
  const subEl = root?.querySelector('[data-ats-sub]');
  if (titleEl) titleEl.textContent = ctx.manifest?.title || TITLE;
  if (subEl) subEl.textContent = ctx.manifest?.description || '';

  let rowsCache = [];
  const collection = () => { try { return ctx.db?.collection?.(PRIMARY) || ctx.db?.[PRIMARY] || null; } catch { return null; } };

  function setStatus(html, kind) {
    if (!statusEl) return;
    statusEl.innerHTML = html || '';
    statusEl.dataset.kind = kind || '';
  }
  function setGate(html) { if (gateEl) gateEl.innerHTML = html || ''; }

  async function render() {
    const col = collection();
    let rows = [];
    if (col?.find) {
      try { const docs = await col.find({ selector: {} }).exec(); rows = docs.map((d) => (typeof d.toJSON === 'function' ? d.toJSON() : d)).filter((r) => !r._deleted); }
      catch (e) { console.error('[intake] load failed:', e); }
    }
    rowsCache = rows;
    if (countEl) countEl.textContent = rows.length + ' Einträge';
    if (listEl) listEl.innerHTML = rows.length ? rows.map((r) => '<div class="ats-item">' + esc(r.id || '') + '</div>').join('') : '<div class="ats-empty">Noch keine Einträge.</div>';
  }

  async function onSubmit(event) {
    event.preventDefault();
    setGate('');
    setStatus('');
    const data = new FormData(formEl);
    const f = Object.fromEntries(data.entries());
    const name = String(f.name == null ? '' : f.name).trim();
    const email = String(f.email == null ? '' : f.email).trim();
    const channel = String(f.channel == null ? '' : f.channel).trim();
    if (!name) { setGate('<span class="ats-block">Name ist erforderlich.</span>'); return; }

    const submitBtn = formEl?.querySelector('button[type="submit"]');
    if (submitBtn) submitBtn.disabled = true;
    try {
      const dispatch = ctx.commandBus?.dispatch;
      if (typeof dispatch !== 'function') {
        setStatus('<span class="ats-offline">Offline — Befehl nicht verfügbar.</span>', 'offline');
        return;
      }
      const result = await ctx.commandBus.dispatch({
        module: MODULE_ID,
        type: COMMAND_TYPE,
        command_type: COMMAND_TYPE,
        payload: { name, email, channel },
      });
      const gate = result?.gate || result?.decision || null;
      const blocked = result?.blocked === true || gate?.decision === 'block' || (Array.isArray(gate?.blockers) && gate.blockers.length > 0) || (Array.isArray(result?.blockers) && result.blockers.length > 0);
      const blockers = (Array.isArray(gate?.blockers) ? gate.blockers : (Array.isArray(result?.blockers) ? result.blockers : []));
      if (blocked) {
        const items = blockers.length ? blockers.map((b) => '<li>' + esc(typeof b === 'string' ? b : (b?.message || b?.reason || JSON.stringify(b))) + '</li>').join('') : '<li>' + esc(gate?.reason || 'Eingang blockiert.') + '</li>';
        setGate('<div class="ats-block"><strong>Gate blockiert:</strong><ul>' + items + '</ul></div>');
        return;
      }
      const appId = result?.application_id || '';
      const dedupeKey = result?.dedupe_key || '';
      setStatus('<span class="ats-ok">Angelegt: ' + esc(appId) + (dedupeKey ? ' <small>(' + esc(dedupeKey) + ')</small>' : '') + '</span>', 'ok');
      formEl.reset();
      await render();
    } catch (e) {
      console.error('[intake] dispatch failed:', e);
      setStatus('<span class="ats-offline">Offline — ' + esc(e?.message || String(e)) + '</span>', 'offline');
    } finally {
      if (submitBtn) submitBtn.disabled = false;
    }
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
