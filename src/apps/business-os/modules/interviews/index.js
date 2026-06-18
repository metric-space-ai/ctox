const MOD_BUILD = '20260618-ats1';
const PRIMARY = 'interview_meetings';

export async function mount(ctx) {
  await ensureStyles();
  ctx.host.innerHTML = await loadMarkup();
  ctx.host.dataset.atsModule = 'interviews';
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();
  const root = ctx.host.querySelector('[data-ats-root]');
  const listEl = root?.querySelector('[data-ats-list]');
  const countEl = root?.querySelector('[data-ats-count]');

  const collection = () => {
    try { return ctx.db?.collection?.(PRIMARY) || ctx.db?.[PRIMARY] || null; } catch { return null; }
  };

  async function render() {
    const col = collection();
    let rows = [];
    if (col?.find) {
      try {
        const docs = await col.find({ selector: {} }).exec();
        rows = docs.map((d) => (typeof d.toJSON === 'function' ? d.toJSON() : d)).filter((r) => !r._deleted);
      } catch (e) { console.error('[interviews] load failed:', e); }
    }
    if (countEl) countEl.textContent = rows.length + ' Einträge';
    if (!listEl) return;
    listEl.innerHTML = rows.length
      ? rows.map((r) => '<div class="ats-item">' + esc(r.id || '') + '</div>').join('')
      : '<div class="ats-empty">Noch keine Einträge.</div>';
  }

  let sub = null;
  const col = collection();
  if (col?.find) { try { sub = col.find({ selector: {} }).$?.subscribe?.(() => { render().catch(() => {}); }); } catch {} }
  await render();

  return () => {
    try { sub?.unsubscribe?.(); } catch {}
    ctx.host.replaceChildren();
    delete ctx.host.dataset.atsModule;
  };
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
  return String(v == null ? '' : v).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}
