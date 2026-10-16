// Shell v4 – Spaltenteiler im Inhaltsbereich eines Fensters (FR2-3; v2: setupModuleResizers in app.js).
//
// Vertrag der App (unverändert aus Shell v2, deklarativ im Markup):
//   <div data-resize-frame style="--x-left: 320px"> … <div class="ctox-column-resizer" data-resizer="left"
//        data-resizer-var="--x-left" data-resizer-min="200" data-resizer-max="600"></div> … </div>
//   (.ctox-row-resizer bzw. data-resizer="top|bottom" für Zeilen). Der Host verdrahtet jeden Griff mit
//   shared/resizer.js (CtoxResizer: Zeiger + Tastatur), auch Griffe, die die App erst nach mount rendert.
//
// Spaltengrammatik (Strang FR4, v3: app.js schedulePaneGrammarWiring): derselbe Lebenszyklus verdrahtet die
// Grammatik jeder `.ctox-pane` mit data-pg-* (Suche, Filterablage, Ansicht, Bänder; shared/pane-grammar.js
// autoWirePaneGrammar) im Inhaltsroot – nach jedem mount/Remount und für Panes, die die App später rendert.
// Idempotent über die DOM-Marke data-pg-wired: Panes, die die App selbst verdrahtet hat, bleiben unangetastet
// (gleicher Griff pane.__ctoxPaneGrammar, keine doppelten Listener).
//
// Persistenz: kernel.windowState je Fenster und App unter `cols:<app>:<fenster>` = {app, widths: {cssVar: px}}.
// Zusätzlich die zuletzt gezogene Breite je App unter `cols:<app>` als Startwert für neue Fenster derselben App.
//
//   createColumnResizers({ appId, windowId, windowState }) → {
//     attach(root)   nach jedem mount: gespeicherte Breiten vor dem ersten Bild setzen, Griffe + Grammatik verdrahten, beobachten
//     detach()       vor unmount / bei destroy: Griffe lösen, Beobachter trennen
//     widths()       aktuelle Breiten (Test)
//     ready          Promise: gespeicherte Breiten gelesen
//     forget()       Fensterschlüssel löschen (Fenster geschlossen)
//   }

import { CtoxResizer } from '../../shared/resizer.js';
import { autoWirePaneGrammar } from '../../shared/pane-grammar.js';
import { createPaneHeadSync } from '../frame/pane-heads.js';
import { createPaneNarrow } from '../frame/pane-narrow.js';

const HANDLE = '.ctox-column-resizer[data-resizer-var], .ctox-row-resizer[data-resizer-var]';
const SIDES = ['left', 'right', 'top', 'bottom'];
// Was eine (neue) Grammatik-Pane ankündigt; autoWirePaneGrammar prüft danach selbst .ctox-pane + Marke.
const GRAMMAR = '.ctox-pane, [data-pg-search], [data-pg-tray-toggle], [data-pg-view], [data-pg-band]';

export function createColumnResizers({ appId, windowId, windowState }) {
  const winKey = `cols:${appId}:${windowId}`;
  const appKey = `cols:${appId}`;
  let widths = {};
  const read = async (key) => { try { const v = await windowState?.get?.(key); return v && typeof v.widths === 'object' ? v.widths : null; } catch { return null; } };
  const ready = (async () => {
    widths = { ...((await read(winKey)) || (await read(appKey)) || {}) };
  })();
  let root = null;
  let observer = null;
  let pending = 0;
  let saveTimer = 0;
  const wired = new Map(); // handle → CtoxResizer
  // Spaltenköpfe eines Fensters gleich hoch (Strang VIS4): gleicher Lebenszyklus wie die Spaltenteiler.
  const heads = createPaneHeadSync();
  // Reiter einzeilig + Liste→Detail unter 600 px (Strang KIT1): gleicher Lebenszyklus.
  const narrow = createPaneNarrow();

  function persist() {
    clearTimeout(saveTimer);
    // Entprellt: ein Zug erzeugt viele Breiten, gespeichert wird der Endstand.
    saveTimer = setTimeout(() => {
      const value = { app: appId, widths: { ...widths } };
      Promise.resolve(windowState?.put?.(winKey, value)).catch(() => {});
      Promise.resolve(windowState?.put?.(appKey, value)).catch(() => {});
    }, 150);
  }

  function wire(handle) {
    const cssVar = handle.getAttribute('data-resizer-var');
    const container = handle.closest('[data-resize-frame]');
    if (!cssVar || !container || !root.contains(container)) return;
    const requested = handle.getAttribute('data-resizer');
    const side = SIDES.includes(requested) ? requested : 'left';
    const orientation = handle.getAttribute('data-resizer-orientation') === 'horizontal' || side === 'top' || side === 'bottom' ? 'horizontal' : 'vertical';
    const minWidth = Number.parseFloat(handle.getAttribute('data-resizer-min')) || 200;
    const maxWidth = Number.parseFloat(handle.getAttribute('data-resizer-max')) || 600;
    const saved = Number(widths[cssVar]);
    if (Number.isFinite(saved) && saved > 0) container.style.setProperty(cssVar, `${Math.round(Math.max(minWidth, Math.min(maxWidth, saved)))}px`);
    const resizer = new CtoxResizer({
      resizerEl: handle, containerEl: container, cssVar, side, orientation, minWidth, maxWidth,
      onResize: (w) => { widths[cssVar] = Math.round(w); persist(); },
    });
    wired.set(handle, resizer);
  }

  function scan() {
    pending = 0;
    if (!root) return;
    for (const [handle, r] of wired) if (!root.contains(handle)) { r.destroy?.(); wired.delete(handle); }
    for (const handle of root.querySelectorAll(HANDLE)) if (!wired.has(handle)) wire(handle);
    try { autoWirePaneGrammar(root); } catch (err) { console.warn('[pane-grammar]', err); }
  }

  return {
    ready,
    attach(appRoot) {
      this.detach();
      root = appRoot;
      scan();
      // Griffe und Grammatik-Panes, die die App später rendert (Datenabo, Ansichtswechsel), ebenfalls verdrahten.
      observer = new MutationObserver((records) => {
        if (pending) return;
        for (const rec of records) {
          for (const n of rec.addedNodes) if (n.nodeType === 1 && (n.matches(HANDLE) || n.querySelector(HANDLE) || n.matches(GRAMMAR) || n.querySelector(GRAMMAR))) { pending = requestAnimationFrame(scan); return; }
          if (wired.size) for (const n of rec.removedNodes) if (n.nodeType === 1) { pending = requestAnimationFrame(scan); return; }
        }
      });
      observer.observe(appRoot, { childList: true, subtree: true });
      heads.attach(appRoot);
      narrow.attach(appRoot);
    },
    detach() {
      narrow.detach();
      heads.detach();
      observer?.disconnect();
      observer = null;
      if (pending) cancelAnimationFrame(pending);
      pending = 0;
      for (const r of wired.values()) r.destroy?.();
      wired.clear();
      root = null;
    },
    widths: () => ({ ...widths }),
    flush() { if (saveTimer) { clearTimeout(saveTimer); saveTimer = 0; const value = { app: appId, widths: { ...widths } }; return Promise.all([windowState?.put?.(winKey, value), windowState?.put?.(appKey, value)]).catch(() => {}); } return Promise.resolve(); },
    forget() { clearTimeout(saveTimer); saveTimer = 0; return Promise.resolve(windowState?.delete?.(winKey)).catch(() => {}); },
  };
}
