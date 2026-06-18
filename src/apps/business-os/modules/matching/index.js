const MATCHING_BUILD = '20260606-load-sync-feedback1';

export async function mount(ctx) {
  await ensureStyles();
  const dataSource = await import('./ui/businessOsDataSource.js');
  dataSource.setBusinessOsDatabaseContext?.(ctx);
  ctx.host.innerHTML = await loadModuleMarkup();
  ctx.host.dataset.matchingModule = 'native';
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();

  let disposed = false;
  const dashboardStartup = (async () => {
    if (ctx.matchingDefinition || globalThis.CTOX_MATCHING_DEFINITION) {
      const definitionModule = await import('./ui/matchingDefinition.js');
      if (disposed) return;
      definitionModule.setActiveMatchingDefinition?.(ctx.matchingDefinition || globalThis.CTOX_MATCHING_DEFINITION);
    }
    await import(`./ui/businessOsControls.js?v=${MATCHING_BUILD}`);
    if (disposed) return;
    const matchingUi = await import(`./ui/index.js?v=${MATCHING_BUILD}`);
    if (disposed) return;
    await matchingUi.mountMatchingDashboard?.(ctx);
  })().catch((error) => {
    if (disposed) return;
    console.error('[matching] dashboard startup failed:', error);
    ctx.notifications?.show?.({
      type: 'error',
      title: 'Matching konnte nicht geladen werden',
      message: String(error?.message || error),
      time: 9000
    });
  });

  return () => {
    disposed = true;
    try { window.teardownRxdbLiveUiSync?.(); } catch {}
    dashboardStartup.catch(() => {});
    ctx.host.replaceChildren();
    delete ctx.host.dataset.matchingModule;
  };
}

async function ensureStyles() {
  const href = `${new URL('./index.css', import.meta.url).pathname}?v=${MATCHING_BUILD}`;
  if (document.querySelector(`link[href="${href}"]`)) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  document.head.append(link);
}

async function loadModuleMarkup() {
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}
