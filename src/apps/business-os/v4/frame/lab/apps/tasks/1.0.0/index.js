// Lab-App tasks 1.0.0: dichte Tabelle + Eingabe, mount/unmount/snapshot nach App-API 4.0 §10.2a Nr. 1.
// globalThis.__labMountDelayMs: Labor-Schalter für die Doppelpuffer-Messung (mount dauert künstlich länger).
export function mount(root, api, ctx = {}) {
  const delay = globalThis.__labMountDelayMs || 0;
  if (delay) return new Promise((resolve) => setTimeout(() => resolve(mountNow(root, api, ctx)), delay));
  return mountNow(root, api, ctx);
}
function mountNow(root, api, ctx = {}) {
  const rows = Array.from({ length: 14 }, (_, i) => ({ id: 'tasks-' + (i + 1), name: 'Aufgaben ' + (i + 1), qty: (i * 7) % 23 }));
  root.innerHTML = `<div class="lab-app" data-lab-app="tasks" data-lab-ver="1.0.0" style="font:13px system-ui;padding:8px 10px">
    <div style="display:flex;gap:6px;margin-bottom:8px"><input data-lab-input placeholder="Notiz" style="flex:1;height:28px;padding:0 8px;border:1px solid #ccd;border-radius:5px"><code style="align-self:center;color:#789">1.0.0</code></div>
    <table style="width:100%;border-collapse:collapse">${rows.map((r) => `<tr style="border-bottom:1px solid #e3e6ea"><td style="padding:4px 2px">${r.id}</td><td>${r.name}</td><td style="text-align:right">${r.qty}</td></tr>`).join('')}</table></div>`;
  const input = root.querySelector('[data-lab-input]');
  if (ctx.restored && typeof ctx.restored.note === 'string') input.value = ctx.restored.note;
  api.shell?.setTitle?.('Aufgaben');
  return { unmount() { root.replaceChildren(); }, snapshot() { return { note: input.value }; } };
}
