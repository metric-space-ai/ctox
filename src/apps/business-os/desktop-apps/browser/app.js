import { mount as mountBrowserModule } from '../../modules/browser/index.js?v=20260528-windowed-browser1';

export const manifest = {
  id: 'browser',
  title: 'Browser',
  glyph: '🌐',
  defaultWidth: 1120,
  defaultHeight: 760,
};

export async function mount(container, ctx = {}) {
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
