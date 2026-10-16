// Demo-App "demo" – Lebenszyklus nach App-API 4.0 (§10.2a Nr. 1).
import { helper, VERSION } from './util.js';
import { greet } from 'ctox:shared/greet.js';

let mountCount = 0; // Modulzustand: beweist, dass alte Module unberuehrt weiterlaufen
export const APP_VERSION = VERSION;
export function moduleState() { return { version: VERSION, mountCount, greet: greet('x') }; }

export async function mount(root, api, ctx = {}) {
  mountCount += 1;
  const [html, de] = await Promise.all([
    fetch(new URL('./index.html', import.meta.url)).then((r) => { if (!r.ok) throw new Error(`index.html ${r.status}`); return r.text(); }),
    fetch(new URL('./locales/de.json', import.meta.url)).then((r) => { if (!r.ok) throw new Error(`de.json ${r.status}`); return r.json(); }),
  ]);
  root.dataset.app = 'demo';
  root.innerHTML = html;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = new URL('./index.css', import.meta.url).href;
  const cssReady = new Promise((res, rej) => { link.onload = res; link.onerror = () => rej(new Error('index.css')); });
  root.prepend(link);
  const img = root.querySelector('img.demo-icon');
  img.src = new URL('./icon.svg', import.meta.url).href;
  await Promise.all([cssReady, img.decode()]);
  root.querySelector('[data-k="title"]').textContent = de.title;
  root.querySelector('[data-k="greet"]').textContent = greet(de.name);
  root.querySelector('[data-k="ver"]').textContent = `${VERSION} ${helper()}`;
  const input = root.querySelector('input[name="note"]');
  if (ctx.restored && typeof ctx.restored.note === 'string') input.value = ctx.restored.note;
  return {
    unmount() { link.remove(); root.innerHTML = ''; },
    snapshot() { return { note: input.value }; },
    probe() {
      const box = root.querySelector('.demo-box');
      return {
        version: VERSION,
        title: root.querySelector('[data-k="title"]').textContent,
        greet: root.querySelector('[data-k="greet"]').textContent,
        helper: helper(),
        cssColor: getComputedStyle(box).borderTopColor,
        cssBg: getComputedStyle(root.querySelector('.demo-badge')).backgroundImage,
        imgWidth: img.naturalWidth,
        note: input.value,
        moduleUrl: import.meta.url,
      };
    },
  };
}
