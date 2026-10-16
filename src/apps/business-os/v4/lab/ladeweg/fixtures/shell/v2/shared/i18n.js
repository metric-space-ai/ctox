// Muster aus heutigem shared/: Locale relativ zur Modul-URL der aufrufenden App laden.
export async function loadModuleMessages(moduleUrl, lang) {
  const r = await fetch(new URL(`./locales/${lang}.json`, moduleUrl));
  if (!r.ok) throw new Error(`locale ${r.status}`);
  return r.json();
}
