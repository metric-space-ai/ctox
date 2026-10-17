// Labor-Fenster: minimales Chrome nach Entwurf §6/§7 (Kopf mit Steuerung, Inhalt, Zustandszone). Nur Labor –
// der echte Rahmen (v4/frame, shared/window-manager.js) ist im Umbau eines anderen Strangs; der Quelltext-Modus
// hängt nur an controls/content/zone und braucht dessen Klassen nicht.
const W = { error: 3, warn: 2, info: 1 };

export function createLabZone(windowEl) {
  const el = document.createElement('div');
  el.className = 'lab-zone';
  el.hidden = true;
  const line = document.createElement('button');
  line.type = 'button';
  line.className = 'lab-zone__line';
  // Aktion am Eintrag (z. B. „Auf aktuelle Version setzen“ bei stale_base): Vorschlag an status-zone.js (StatusItem.action).
  const act = document.createElement('button');
  act.type = 'button';
  act.className = 'lab-zone__act';
  act.dataset.zoneAction = '';
  act.hidden = true;
  el.append(line, act);
  const list = document.createElement('div');
  list.className = 'lab-zone__list';
  list.hidden = true;
  windowEl.append(list);
  const entries = new Map();
  const render = () => {
    let top = null;
    for (const e of entries.values()) if (!top || W[e.level] > W[top.level] || (W[e.level] === W[top.level] && e.at >= top.at)) top = e;
    el.hidden = !top;
    if (!top) { list.hidden = true; return; }
    el.dataset.level = top.level;
    line.textContent = top.text;
    line.title = top.text;
    act.hidden = !top.action;
    act.textContent = top.action?.label ?? '';
    act.dataset.code = top.code;
    act.onclick = top.action ? () => top.action.run() : null;
    list.replaceChildren(...[...entries.values()].map((e) => Object.assign(document.createElement('div'), { className: 'lab-zone__item', textContent: e.text })));
  };
  line.addEventListener('click', () => { list.hidden = !list.hidden; });
  return {
    el,
    report({ level = 'info', code, text, action }) { if (code) { entries.set(code, { level, code, text: String(text ?? code), action: action && typeof action.run === 'function' ? action : null, at: performance.now() }); render(); } },
    clear(code) { if (entries.delete(code)) render(); },
    entries: () => [...entries.values()].map((e) => ({ level: e.level, code: e.code, text: e.text, action: e.action?.label ?? null, at: e.at })),
  };
}

export function createLabWindow(desktop, { id, title, icon = '□', geometry }) {
  const el = document.createElement('section');
  el.className = 'lab-win';
  el.dataset.windowId = id;
  el.innerHTML = `<header class="lab-win__header"><span class="lab-win__icon"></span><span class="lab-win__title"></span><span class="lab-win__fill"></span><div class="lab-win__controls"><button type="button" class="lab-win__close" aria-label="Schließen">×</button></div></header><div class="lab-win__body"></div>`;
  el.querySelector('.lab-win__icon').textContent = icon;
  el.querySelector('.lab-win__title').textContent = title;
  const zone = createLabZone(el);
  el.append(zone.el);
  desktop.append(el);
  const place = (g) => Object.assign(el.style, { left: `${g.x}px`, top: `${g.y}px`, width: `${g.w}px`, height: `${g.h}px` });
  place(geometry);
  return { id, el, controls: el.querySelector('.lab-win__controls'), content: el.querySelector('.lab-win__body'), zone, place };
}
