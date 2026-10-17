// Lab-App studio 1.0.0 (FR6a, nachgebaut nach Black Hole Studio, M-10/M-11/M-12): eigene Werkzeugleiste oben rechts
// (Einstellungen, „PNG speichern“, Sprache, Vollbild) und Titel oben links – genau dort, wo Icon und Knopfcluster der
// Shell schweben. Mit Kit (Standard): Werkzeugleiste im Pane-Kopf .ctox-pane-header, der Rahmen rückt sie aus den
// reservierten Zonen. globalThis.__labStudioNoKit = true: dieselbe Leiste OHNE Kit (Gegenprobe für G8: schneidet die
// Zonen). Kein Icon im Manifest (Ersatz-Icon der Shell). Vollbild-Knopf ruft api.shell.window.fullscreen().
export function mount(root, api) {
  const kit = !globalThis.__labStudioNoKit;
  root.innerHTML = `<div class="lab-app" data-lab-app="studio" style="font:13px system-ui;position:absolute;inset:0;display:grid;grid-template-rows:auto 1fr;background:#0e1420;color:#e8eef6">
    <div class="${kit ? 'ctox-pane-header' : 'studio-bar'}" data-studio-head style="display:block;padding:0 6px;box-sizing:border-box;border-bottom:1px solid #243049">
      <div data-studio-row1 style="display:flex;flex-wrap:wrap;align-items:center;justify-content:space-between;gap:4px 8px;min-height:44px;padding:9px 0;box-sizing:border-box">
        <div data-studio-title style="font-weight:600;white-space:nowrap;overflow:hidden;padding-left:2px">Schwarzes Loch · Szene 1</div>
        <div data-studio-tools style="display:flex;flex-wrap:wrap;gap:4px;align-items:center;margin-left:auto">
          <button style="height:26px;padding:0 8px;border:1px solid #34425e;border-radius:5px;background:#1c2638;color:#e8eef6" type="button" data-studio-settings title="Einstellungen">⚙</button>
          <button style="height:26px;padding:0 8px;border:1px solid #34425e;border-radius:5px;background:#1c2638;color:#e8eef6" type="button" data-studio-png>PNG speichern</button>
          <select data-studio-lang aria-label="Sprache" style="height:26px;background:#1c2638;color:#e8eef6;border:1px solid #34425e;border-radius:5px"><option>DE</option><option>EN</option></select>
          <button style="height:26px;padding:0 8px;border:1px solid #34425e;border-radius:5px;background:#1c2638;color:#e8eef6" type="button" data-studio-full title="Vollbild">⛶</button>
        </div>
      </div>
      <div data-studio-row2 style="height:34px;padding:4px 0;box-sizing:border-box"><input data-studio-search placeholder="Szenen suchen" style="display:block;width:100%;box-sizing:border-box;height:26px;background:#141c2b;color:#e8eef6;border:1px solid #34425e;border-radius:5px;padding:0 8px"></div>
      <div data-studio-row3 style="height:34px;padding:4px 0;box-sizing:border-box"><input data-studio-filter placeholder="Filter" style="display:block;width:100%;box-sizing:border-box;height:26px;background:#141c2b;color:#e8eef6;border:1px solid #34425e;border-radius:5px;padding:0 8px"></div>
    </div>
    <div data-studio-scene style="display:grid;place-items:center"><div style="width:160px;height:160px;border-radius:50%;background:radial-gradient(circle,#000 40%,#f0b56f 42%,#3a1d5c 60%,transparent 70%)"></div></div>
  </div>`;
  api.shell?.setTitle?.('Szene 1');
  const full = root.querySelector('[data-studio-full]');
  const onFull = async () => { globalThis.__studioFull = await api.shell?.window?.fullscreen?.(); };
  full.addEventListener('click', onFull);
  return { unmount() { full.removeEventListener('click', onFull); root.replaceChildren(); }, snapshot() { return null; } };
}
