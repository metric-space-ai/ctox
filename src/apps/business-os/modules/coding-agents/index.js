// App-API 4.0 (Shell v4): mount(root, api, {restored, route}) -> {unmount, snapshot}.
//
// Coding Agents = Übersicht aller Agent-Läufe (Strang AGENT, Bedienkonzept Fassung 4 §3, Owner-Mängel M-03/M-04/M-05).
// Der Agent arbeitet nicht mehr hier, sondern als Seitenleiste in der Code-Ansicht der jeweiligen App (§16.44: auf dem
// eigenen Entwurf, Ergebnis als Diff, Veröffentlichen per Knopf). Diese App listet die Läufe aus der Live-Collection
// coding_turns (neueste oben, laufende zuerst) und öffnet per Klick die Code-Ansicht der betreffenden App
// (api.shell.openSource(app_id); die Shell öffnet dort die Agent-Seitenleiste). Kein eigener Kopf-Schmuck: das App-Icon
// oben links ist der Standard-Icon-Header der Shell. Daten nur über api.query; keine Zustandstexte (Fehler → Zone).
// Deep-Link: api.shell.navigate('coding-agents', {app:'<id>'}) (Altform {moduleId}) filtert auf eine App.

const TURNS = 'coding_turns';
const APPS = 'business_apps';
const ACTIVE = new Set(['queued', 'running']);

// Texte: locales/<lang>.json (Deutsch eingebettet als Rückfall, damit die Liste ohne Netz sofort steht).
const DE = {
  app: 'Coding-Agenten', title: 'Agent-Läufe', search: 'Läufe suchen',
  empty: 'Noch keine Läufe – Start über „Code ansehen“ im App-Menü.',
  emptyFilter: 'Keine Läufe passen zur Suche.', open: 'Code-Ansicht von {app} öffnen', file: '1 Datei', files: '{n} Dateien',
  state: { queued: 'wartet', running: 'läuft', done: 'fertig', conflict: 'Patch bereit', cancelled: 'abgebrochen', failed: 'fehlgeschlagen' },
};
const LOCALES = new Map([['de', DE]]);
async function loadLocale(lang) {
  if (LOCALES.has(lang)) return LOCALES.get(lang);
  try {
    const json = await fetch(new URL(`./locales/${lang}.json`, import.meta.url)).then((r) => (r.ok ? r.json() : null));
    if (json && typeof json === 'object') { LOCALES.set(lang, { ...DE, ...json, state: { ...DE.state, ...(json.state || {}) } }); return LOCALES.get(lang); }
  } catch { /* Rückfall Deutsch */ }
  return DE;
}
const fill = (tpl, vars) => String(tpl).replace(/\{(\w+)\}/g, (_, k) => String(vars[k] ?? ''));
const langOf = (locale) => (String(locale || 'de').toLowerCase().startsWith('en') ? 'en' : 'de');

async function loadModuleMarkup() {
  // index.html bleibt die Markup-Quelle; der Service Worker liefert sie versioniert (kein Cache-Buster).
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}

const routeAppId = (event) => String(event?.params?.app || event?.params?.moduleId || '').trim();

/** Reine Sortierung/Filterung (auch für Tests): laufende zuerst, dann neueste. */
export function arrangeRuns(turns, { search = '', app = '', titles = new Map() } = {}) {
  const q = search.trim().toLowerCase();
  return (Array.isArray(turns) ? turns : [])
    .filter((t) => t && t.id && t.app_id && !t._deleted)
    .filter((t) => !app || t.app_id === app)
    .filter((t) => {
      if (!q) return true;
      const prompt = t.prompt ?? t.messages?.find((m) => m.role === 'user')?.text ?? '';
      return `${titles.get(t.app_id) ?? ''} ${t.app_id} ${prompt}`.toLowerCase().includes(q);
    })
    .sort((a, b) => (ACTIVE.has(b.state) - ACTIVE.has(a.state)) || ((b.created_at_ms ?? 0) - (a.created_at_ms ?? 0)));
}

function relTime(ms, lang) {
  if (!ms) return '';
  const d = new Date(ms);
  const sameDay = d.toDateString() === new Date().toDateString();
  return sameDay ? d.toLocaleTimeString(lang === 'en' ? 'en-GB' : 'de-DE', { hour: '2-digit', minute: '2-digit' })
    : d.toLocaleDateString(lang === 'en' ? 'en-GB' : 'de-DE', { day: '2-digit', month: '2-digit' });
}

export async function mount(root, api, { restored, route } = {}) {
  const disposers = [];
  const st = {
    lang: langOf(api.shell.locale?.get?.()), turns: [], titles: new Map(),
    search: typeof restored?.search === 'string' ? restored.search : '',
    app: routeAppId(route) || (typeof restored?.app === 'string' ? restored.app : ''),
  };
  if (!root.dataset.app) root.dataset.app = api.app?.id || 'coding-agents';
  root.innerHTML = await loadModuleMarkup();
  const els = {
    title: root.querySelector('[data-ca-title]'), search: root.querySelector('[data-ca-search]'),
    list: root.querySelector('[data-ca-list]'), empty: root.querySelector('[data-ca-empty]'),
  };
  let texts = await loadLocale(st.lang);
  const t = () => texts;
  function applyTexts() {
    els.title.textContent = st.app ? `${t().title} · ${st.titles.get(st.app) ?? st.app}` : t().title;
    els.search.placeholder = t().search;
    els.search.setAttribute('aria-label', t().search);
  }
  function render() {
    applyTexts();
    const runs = arrangeRuns(st.turns, { search: st.search, app: st.app, titles: st.titles });
    els.list.replaceChildren();
    for (const run of runs) {
      const appTitle = st.titles.get(run.app_id) ?? run.app_id;
      const prompt = run.prompt ?? run.messages?.find((m) => m.role === 'user')?.text ?? '';
      const li = document.createElement('li');
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'coding-agents-run';
      btn.dataset.caRun = run.id;
      btn.dataset.app = run.app_id;
      btn.dataset.state = run.state ?? '';
      btn.title = fill(t().open, { app: appTitle });
      const head = document.createElement('span');
      head.className = 'coding-agents-run-head';
      const app = document.createElement('span');
      app.className = 'coding-agents-run-app';
      app.textContent = appTitle;
      const meta = document.createElement('span');
      meta.className = 'coding-agents-run-meta';
      const files = (run.changed_files ?? []).length;
      meta.textContent = [files ? (files === 1 ? t().file : fill(t().files, { n: files })) : '', relTime(run.created_at_ms, st.lang)].filter(Boolean).join(' · ');
      const mark = document.createElement('span');
      mark.className = 'coding-agents-run-mark';
      mark.setAttribute('role', 'img');
      mark.setAttribute('aria-label', t().state[run.state] ?? run.state ?? '');
      mark.title = t().state[run.state] ?? run.state ?? '';
      head.append(mark, app, meta);
      const text = document.createElement('span');
      text.className = 'coding-agents-run-prompt';
      text.textContent = prompt.split('\n')[0];
      btn.append(head, text);
      btn.addEventListener('click', () => api.shell.openSource(run.app_id));
      li.append(btn);
      els.list.append(li);
    }
    els.empty.hidden = runs.length > 0;
    els.empty.textContent = runs.length ? '' : (st.turns.length ? t().emptyFilter : t().empty);
  }
  els.search.value = st.search;
  els.search.addEventListener('input', () => { st.search = els.search.value; render(); });
  disposers.push(api.query(TURNS, {}).subscribe((rows) => { st.turns = Array.isArray(rows) ? rows : []; render(); }));
  disposers.push(api.query(APPS, {}).subscribe((rows) => {
    st.titles = new Map((Array.isArray(rows) ? rows : []).filter((r) => r?.id).map((r) => [r.id, r.title || r.id]));
    render();
  }));
  disposers.push(api.shell.onRoute((event) => { const id = routeAppId(event); if (id !== st.app) { st.app = id; render(); } }));
  if (api.shell.locale?.subscribe) {
    disposers.push(api.shell.locale.subscribe(async (l) => {
      const lang = langOf(l);
      if (lang === st.lang) return;
      st.lang = lang;
      texts = await loadLocale(lang);
      render();
      api.shell.setTitle?.(t().app);
    }));
  }
  api.shell.setTitle?.(t().app);
  render();
  return {
    unmount() {
      for (const dispose of disposers.splice(0)) { try { dispose(); } catch { /* still */ } }
      root.replaceChildren();
    },
    snapshot: () => ({ search: st.search, app: st.app }),
  };
}
