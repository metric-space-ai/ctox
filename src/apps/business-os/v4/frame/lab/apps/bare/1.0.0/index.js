// Lab-App bare 1.0.0 (Strang ZONE2, K24 / Befund T3-1): eine App, die das Kit NICHT kennt – wie die K24-Probe.
// Eigene Formularzeile ganz oben: links ein Knopf (unter dem App-Icon), rechts „Agent“ (unter dem Knopfcluster),
// dazu ein absolut positionierter Knopf oben rechts über der Zeile (position: absolute in der App-Wurzel). Kein .ctox-pane-header,
// keine Zonen-Variablen. Jeder Klick zählt in globalThis.__bareClicks[name] (echte Maus im Labor).
export function mount(root) {
  root.innerHTML = `<div class="bare" style="font:13px system-ui;display:flex;flex-direction:column;gap:12px;padding:40px 16px 16px;height:100%;box-sizing:border-box;background:#fff;color:#1d2330">
    <form data-bare-form style="display:flex;gap:8px;flex-wrap:wrap">
      <button type="button" data-bare="left" style="padding:6px 12px;font:inherit">Links</button>
      <input data-bare-input aria-label="Tick" style="flex:1 1 200px;min-width:0;padding:6px 8px;font:inherit">
      <button type="button" data-bare="tick" style="padding:6px 12px;font:inherit">Tick</button>
      <button type="button" data-bare="right" style="padding:6px 12px;font:inherit">Agent</button>
    </form>
    <ul data-bare-list style="margin:0;padding:0;list-style:none"><li style="padding:6px 0;border-bottom:1px solid #ddd">Zeile 1</li></ul>
    <button type="button" data-bare="abs" style="position:absolute;top:6px;right:6px;padding:4px 10px;font:inherit">Abs</button>
  </div>`;
  globalThis.__bareClicks = globalThis.__bareClicks || {};
  const onClick = (ev) => { const b = ev.target.closest('[data-bare]'); if (b) globalThis.__bareClicks[b.dataset.bare] = (globalThis.__bareClicks[b.dataset.bare] || 0) + 1; };
  root.addEventListener('click', onClick);
  return { unmount() { root.removeEventListener('click', onClick); root.replaceChildren(); }, snapshot() { return null; } };
}
