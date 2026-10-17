// A test app on the production module-context API. It never writes product data.
export async function mount(ctx) {
  const started = performance.now();
  const rows = await ctx.db.e2e_leads.find({ selector: {}, limit: 100000 }).exec();
  const panel = document.createElement('section'); panel.dataset.e2eApp = 'first-open';
  panel.replaceChildren(...rows.map(row => { const item = document.createElement('div'); item.dataset.id = row.id; item.textContent = row.id; return item; }));
  ctx.host.replaceChildren(panel);
  await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)));
  globalThis.__e2eAppResult = { openedAt: started, paintedAt: performance.now(), ids: rows.map(row => row.id), emptyRender: rows.length === 0,
    visible: panel.getBoundingClientRect().height > 0 && getComputedStyle(panel).visibility !== 'hidden' };
  return () => panel.remove();
}
