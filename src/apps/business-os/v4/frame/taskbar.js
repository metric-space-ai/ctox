// Gruppierungslogik der App-Reiter (FR6b, ab FR6c ohne eigene Leiste). Die einzige Leiste ist die Reiterleiste der
// Shell-V2 oben in der Kopfleiste (topbar.js); sie rendert ihre Reiter aus buildEntries und wechselt per nextInGroup.
//
// - Eintrag = Gruppe: layout.presentation.taskbar_group (z. B. „crew“ – alle Crew-Chats unter EINEM Eintrag „Crew“),
//   sonst die App. Angeheftete Apps und ihre Fenster sind DERSELBE Eintrag.
//   Reihenfolge: angeheftete Einträge in Pin-Reihenfolge, danach laufende in Öffnungsreihenfolge.
// - Klick: ohne Fenster → App öffnen; ein Fenster → fokussieren bzw. minimieren; mehrere → reihum (nextInGroup).
// Keine DOM-Teile, keine Zustandstexte.

// Gruppenschlüssel und -titel einer App (gleiche Regel für Pins und Fenster).
export function taskGroupOf(app) {
  const pres = app?.layout?.presentation;
  const key = pres && typeof pres.taskbar_group === 'string' ? pres.taskbar_group.trim() : '';
  if (!key) return null;
  return { key, title: String(pres.taskbar_group_title || key.charAt(0).toLocaleUpperCase() + key.slice(1)) };
}

// Einträge: angeheftete (Pin-Reihenfolge) zuerst, dann laufende (Reihenfolge des ersten Fensters).
export function buildEntries({ pins = [], windows = [], appById = new Map() }) {
  const entries = new Map();
  const groupOf = (appId, w = null) => {
    const g = taskGroupOf(appById.get(appId));
    if (g) return g;
    if (w?.group) return { key: String(w.group), title: String(w.groupTitle || w.group) };
    return { key: String(appId), title: null };
  };
  const entryFor = (appId, w = null) => {
    const g = groupOf(appId, w);
    let e = entries.get(g.key);
    if (!e) {
      e = { key: g.key, appId, grouped: g.title != null, title: g.title ?? String(appById.get(appId)?.title || w?.title || appId), pins: [], windows: [] };
      entries.set(g.key, e);
    }
    return e;
  };
  for (const id of pins) if (appById.has(id)) entryFor(id).pins.push(id);
  for (const w of windows) entryFor(w.appId, w).windows.push(w);
  return [...entries.values()];
}

// Für ältere Aufrufer/Tests: Fenstergruppen ohne Pins.
export function groupWindows(windows, appById = new Map()) {
  return buildEntries({ windows, appById });
}

// Ziel eines Klicks auf einen Eintrag mit Fenstern: {id, toggle} (toggle = Fokussieren/Minimieren eines Einzelfensters).
export function nextInGroup(wins) {
  if (!wins.length) return null;
  if (wins.length === 1) return { id: wins[0].id, toggle: true };
  const at = wins.findIndex((w) => w.focused && w.state !== 'minimized');
  if (at >= 0) return { id: wins[(at + 1) % wins.length].id, toggle: false };
  const visible = wins.filter((w) => w.state !== 'minimized');
  const pool = visible.length ? visible : wins;
  return { id: pool.reduce((a, b) => ((b.z || 0) > (a.z || 0) ? b : a)).id, toggle: false };
}
