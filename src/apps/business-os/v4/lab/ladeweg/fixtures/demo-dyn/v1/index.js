// Probe fuer berechnete Pfade (heutiges Muster in vielen Apps): Template-Literal + import.meta.url als Wert.
import { loadModuleMessages } from 'ctox:shared/i18n.js';
async function attempt(fn) { try { return (await fn()).title; } catch (e) { return `FEHLER:${e && e.message}`; } }
export async function mount(root, api, ctx = {}) {
  const lang = 'de';
  const a = await attempt(() => fetch(new URL(`./locales/${lang}.json`, import.meta.url)).then((r) => r.json()));
  const b = await attempt(() => loadModuleMessages(import.meta.url, lang));
  root.textContent = `${a}|${b}`;
  return { unmount() { root.textContent = ''; }, probe() { return { templateLiteral: a, metaUrlAsValue: b }; } };
}
