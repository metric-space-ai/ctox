const MATCHING_BUILD = '20260518-module-contract2';

export async function mount(ctx) {
  await ensureStyles();
  const dataSource = await import('./ui/businessOsDataSource.js');
  dataSource.setBusinessOsRawDatabase?.(ctx.db?.raw || null);
  ctx.host.innerHTML = await loadModuleMarkup();
  ctx.host.dataset.matchingModule = 'native';
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();
  await import(`./ui/businessOsControls.js?v=${MATCHING_BUILD}`);
  const matchingUi = await import(`./ui/index.js?v=${MATCHING_BUILD}`);
  await matchingUi.mountMatchingDashboard?.(ctx);
  return () => {
    try { window.teardownRxdbLiveUiSync?.(); } catch {}
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
