export const manifest = {
  id: 'creator',
  title: 'App Creator',
  glyph: '⚙️',
  defaultWidth: 1200,
  defaultHeight: 800,
};

export async function mount(container, ctx) {
  container.innerHTML = '<div style="padding: 20px; color: var(--muted);">Lade App Creator...</div>';

  let teardown = null;
  let disposed = false;
  Promise.resolve().then(async () => {
    const mod = await import('../../modules/creator/index.js');
    if (disposed) return;
    container.innerHTML = '';
    const moduleCtx = {
      ...ctx,
      host: container
    };
    const mountedTeardown = await mod.mount(moduleCtx);
    if (disposed) {
      try { mountedTeardown?.(); } catch {}
      return;
    }
    teardown = mountedTeardown;
  }).catch((error) => {
    if (disposed) return;
    console.error('[creator-app] failed to mount:', error);
    container.innerHTML = `<p style="padding:16px;color:var(--danger);font-size:12px;">Start fehlgeschlagen: ${error?.message || error}</p>`;
  });

  return () => {
    disposed = true;
    if (teardown) {
      try {
        teardown();
      } catch (err) {
        console.error('[creator-app] teardown failed:', err);
      }
    }
    container.innerHTML = '';
  };
}
