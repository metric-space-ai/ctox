// Lab-App crm 2.0.0: zweispaltig mit Spaltenteiler (v2-Markup), Kopf mit v2-Reservezonen, Container-Haltepunkte
// ohne eigene Container-Deklaration, onRoute-Ablage. Prüfobjekt für FR2 (Container, Zonen, Resizer, navigate).
// globalThis.__labMountDelayMs: Labor-Schalter für die Doppelpuffer-Messung (mount dauert künstlich länger).
export function mount(root, api, ctx = {}) {
  const delay = globalThis.__labMountDelayMs || 0;
  if (delay) return new Promise((resolve) => setTimeout(() => resolve(mountNow(root, api, ctx)), delay));
  return mountNow(root, api, ctx);
}
const CSS = `
[data-app="crm"] .crm-head { box-sizing: border-box; display: flex; align-items: center; height: var(--shell-v2-header-row-size, 61px);
  padding: 0 calc(var(--shell-v2-header-row-size, 37px) * 2) 0 calc(var(--shell-v2-icon-size, 80px) + 14px); border-bottom: 1px solid #e3e6ea; }
[data-app="crm"] .crm-head-r18 { box-sizing: border-box; height: 8px; padding: 0 var(--shell-v2-control-inset, 74px) 0 0; }
[data-app="crm"] .crm-split { display: grid; grid-template-columns: var(--crm-left, 220px) 6px minmax(0, 1fr); min-height: 300px; }
[data-app="crm"] .ctox-column-resizer { cursor: col-resize; background: #e3e6ea; }
[data-app="crm"] .crm-bp { --crm-bp: wide; }
@container business-app-window (max-width: 1024px) { [data-app="crm"] .crm-bp { --crm-bp: mid; } }
@container business-app-window (max-width: 768px) { [data-app="crm"] .crm-bp { --crm-bp: narrow; } }`;
// globalThis.__labGrammar: Spaltengrammatik (data-pg-*) im Markup, ohne eigenen autoWirePaneGrammar-Aufruf (Strang FR4:
// der Rahmen verdrahtet sie nach mount/Remount).
const GRAMMAR_PANE = `<section class="ctox-pane" data-crm-pg><input data-pg-search placeholder="Suche">
  <button type="button" data-pg-tray-toggle>Filter</button><div data-pg-tray hidden><select data-pg-filter data-pg-name="typ">
  <option value="all">Alle</option><option value="a">A</option></select><button type="button" data-pg-reset>Zurücksetzen</button></div>
  <div role="tablist"><button type="button" role="tab" data-pg-band="alle" aria-selected="true">Alle<span data-pg-count="alle"></span></button>
  <button type="button" role="tab" data-pg-band="offen" aria-selected="false">Offen<span data-pg-count="offen"></span></button></div></section>`;
function mountNow(root, api, ctx = {}) {
  const rows = Array.from({ length: 14 }, (_, i) => ({ id: 'crm-' + (i + 1), name: 'Kunden ' + (i + 1), qty: (i * 7) % 23 }));
  root.innerHTML = `<style>${CSS}</style><div class="lab-app" data-lab-app="crm" data-lab-ver="2.0.0" style="font:13px system-ui">
    <header class="crm-head" data-crm-head><b data-crm-title>Kunden</b></header><div class="crm-head-r18" data-crm-r18></div>
    <span class="crm-bp" data-crm-bp></span>
    <div class="crm-split" data-resize-frame data-crm-split>
      <aside data-crm-left style="padding:8px"><input data-lab-input placeholder="Notiz" style="width:100%;box-sizing:border-box;height:28px;padding:0 8px;border:1px solid #ccd;border-radius:5px"><code style="color:#789">2.0.0</code>${globalThis.__labGrammar ? GRAMMAR_PANE : ''}</aside>
      <div class="ctox-column-resizer" data-resizer="left" data-resizer-var="--crm-left" data-resizer-min="120" data-resizer-max="400"></div>
      <main style="padding:8px"><table style="width:100%;border-collapse:collapse">${rows.map((r) => `<tr style="border-bottom:1px solid #e3e6ea"><td style="padding:4px 2px">${r.id}</td><td>${r.name}</td><td style="text-align:right">${r.qty}</td></tr>`).join('')}</table></main>
    </div></div>`;
  const input = root.querySelector('[data-lab-input]');
  if (ctx.restored && typeof ctx.restored.note === 'string') input.value = ctx.restored.note;
  root.dataset.crmMountRoute = JSON.stringify(ctx.route ?? null);
  // globalThis.__labLateRoute: onRoute erst nach mount abonnieren (Start-Route darf trotzdem nicht verloren gehen).
  let offRoute = null;
  const sub = () => { offRoute = api.shell?.onRoute?.((ev) => { root.dataset.crmRoute = JSON.stringify(ev ?? null); root.dataset.crmRouteCount = String(Number(root.dataset.crmRouteCount || 0) + 1); }); };
  if (globalThis.__labLateRoute) setTimeout(sub, 60); else sub();
  api.shell?.setTitle?.('Kunden');
  return { unmount() { offRoute?.(); root.replaceChildren(); }, snapshot() { return { note: input.value }; } };
}
