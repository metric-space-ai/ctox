export const manifest = {
  id: 'browser',
  title: 'Browser',
  glyph: '🌐',
  defaultWidth: 1120,
  defaultHeight: 760,
};

export async function mount(container, ctx = {}) {
  // The desktop-app wrapper is loaded with the shell's APP_BUILD query. Carry
  // that revision into the real browser module so a release cannot keep
  // executing an older cached Browser implementation indefinitely.
  const browserModuleUrl = new URL('../../modules/browser/index.js', import.meta.url);
  browserModuleUrl.search = new URL(import.meta.url).search;
  const { mount: mountBrowserModule } = await import(browserModuleUrl.href);
  return mountBrowserModule({
    ...ctx,
    host: container,
    module: {
      id: 'browser',
      title: 'Browser',
      ...(ctx.module || {}),
    },
  });
}
