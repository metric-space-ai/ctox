const MATCHING_BUILD = '20260718-ctox-kit-migration2';

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
    const controls = await import(`./ui/businessOsControls.js?v=${MATCHING_BUILD}`);
    controls.setBusinessOsRuntimeContext?.(ctx);
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
  const cssVersion = String(import.meta.url).split('?v=')[1] || MATCHING_BUILD;
  const cssHref = new URL('./index.css', import.meta.url).pathname + (cssVersion ? `?v=${cssVersion}` : '');
  let link = document.querySelector('link[data-matching-style]');
  if (!link) {
    link = document.createElement('link');
    link.rel = 'stylesheet';
    link.dataset.matchingStyle = 'true';
    document.head.append(link);
  }
  if (link.getAttribute('href') !== cssHref) link.href = cssHref;
}

async function loadModuleMarkup() {
  const markupVersion = String(import.meta.url).split('?v=')[1] || MATCHING_BUILD;
  const markupHref = new URL('./index.html', import.meta.url).pathname + (markupVersion ? `?v=${markupVersion}` : '');
  const html = await fetch(markupHref).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}
