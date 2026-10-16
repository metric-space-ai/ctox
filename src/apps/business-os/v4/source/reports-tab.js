// Shell v4 – Reiter „Meldungen“ in der Code-Ansicht (Strang LOOP, Bedienkonzept §3a Schritt 2/3).
//
//   createReportsTab({ appId, api, canEdit, onAssign, onOpenTurn }) → { el, count, rows(), linked(draftId), destroy() }
//     api        App-API der Shell (query/patch auf business_module_reports, ../shell/report.js REPORT_COLLECTIONS)
//     canEdit()  apps.source.edit (ohne Recht: Liste lesbar, kein „Agent beauftragen“)
//     onAssign(report, no)   „Agent beauftragen“ (source-mode.js: Prompt, ctox.coding.turn §16.44, Verknüpfung)
//     onOpenTurn(turnId)     Lauf in der Agent-Seitenleiste zeigen
//
// Zeigt die offenen Meldungen (open | running) genau dieser App, älteste zuerst: Titel, Art, Melder, Datum, Status.
// Melder (Runde 2): eigene Meldung „Du“, sonst der Name aus der Nutzerliste der Instanz (USERS_COLLECTION, Spalten
// display_name je user_id/id, Eigentümer ctox, Shell-Deklaration SHELL_FOREIGN), Rückfall „Teammitglied“ – nie eine ID.
// mark(reportId) (api.shell.openSource(appId, {reportId})): Meldung markiert (aria-current, is-marked), in Sicht.
// Leer: ein Satz, was hier erscheint. Keine Zustandstexte; Fehler gehen in die Zone (Aufrufer).

import { REPORT_COLLECTIONS, LOOP_TEXT, sortReports, isOpenReport, reportNo } from '../shell/report.js';

export const USERS_COLLECTION = 'business_users';
export const REPORTER_FALLBACK = 'Teammitglied';
/** Anzeigename des Melders: „Du“ | Name aus der Nutzerliste | gespeicherter Name (keine ID) | „Teammitglied“. */
export function reporterLabel(r, { me = '', names = new Map() } = {}) {
  const id = String(r?.reporter_id ?? '');
  if (id && id === me) return 'Du';
  const fromDir = id ? names.get(id) : '';
  if (fromDir) return fromDir;
  const stored = String(r?.reporter_display_name || r?.client_context?.reporter_name || '').trim();
  return stored && stored !== id ? stored : REPORTER_FALLBACK;
}
/** Nutzerzeilen → Map user_id/id → display_name (nur nicht leere Namen, gelöschte ausgelassen). */
export function userNames(rows) {
  const out = new Map();
  for (const u of Array.isArray(rows) ? rows : []) {
    if (!u || u.is_deleted || u._deleted) continue;
    const name = String(u.display_name || u.name || '').trim();
    if (!name) continue;
    for (const k of [u.user_id, u.id]) if (typeof k === 'string' && k && !out.has(k)) out.set(k, name);
  }
  return out;
}

function h(tag, cls, text) {
  const n = document.createElement(tag);
  if (cls) n.className = cls;
  if (text !== undefined) n.textContent = text;
  return n;
}
const DATE = new Intl.DateTimeFormat('de-DE', { day: '2-digit', month: '2-digit', year: 'numeric' });
const dateOf = (ms) => (Number(ms) > 0 ? DATE.format(new Date(Number(ms))) : '');

export function createReportsTab({ appId, api, canEdit = () => true, onAssign, onOpenTurn }) {
  const st = { rows: [], busy: new Set(), closed: false, listeners: new Set(), names: new Map(), marked: null, scrolled: null };
  const el = h('div', 'v4-src__reports');
  el.dataset.v4Reports = appId;
  el.setAttribute('role', 'list');
  el.setAttribute('aria-label', 'Meldungen');
  const me = () => api.actor?.id ?? '';
  const reporterOf = (r) => reporterLabel(r, { me: me(), names: st.names });

  function render() {
    el.replaceChildren();
    const open = st.rows.filter(isOpenReport);
    if (!open.length) {
      el.append(h('p', 'v4-src__reports-empty', 'Hier erscheinen Probleme und Wünsche, die zu dieser App gemeldet werden.'));
    }
    for (const r of open) {
      const no = reportNo(r, st.rows);
      const kind = r.kind === 'bug' ? 'bug' : 'feature';
      const status = String(r.status || 'open');
      const item = h('article', 'v4-src__report');
      item.setAttribute('role', 'listitem');
      item.dataset.reportId = r.id;
      item.dataset.reportNo = String(no ?? '');
      item.dataset.status = status;
      item.dataset.kind = kind;
      if (st.marked === r.id) { item.classList.add('is-marked'); item.setAttribute('aria-current', 'true'); }
      const head = h('div', 'v4-src__report-head');
      head.append(h('span', 'v4-src__report-no', `#${no ?? '?'}`), h('span', 'v4-src__report-title', r.title || ''));
      const meta = h('div', 'v4-src__report-meta');
      meta.append(
        h('span', `v4-src__report-kind is-${kind}`, LOOP_TEXT.kind[kind]),
        h('span', 'v4-src__report-by', reporterOf(r)),
        h('span', 'v4-src__report-date', dateOf(r.created_at_ms)),
        h('span', `v4-src__report-status is-${status}`, LOOP_TEXT.status[status] ?? status),
      );
      item.append(head, meta);
      const acts = h('div', 'v4-src__report-acts');
      if (status === 'running' && r.coding_turn_id) {
        const run = h('button', 'v4-src__link', 'Lauf ansehen');
        run.type = 'button';
        run.dataset.reportTurn = r.coding_turn_id;
        run.title = `Agent-Lauf ${r.coding_turn_id}`;
        run.addEventListener('click', () => onOpenTurn?.(r.coding_turn_id));
        acts.append(run);
      } else if (canEdit()) {
        const go = h('button', 'v4-src__btn v4-src__btn--small', 'Agent beauftragen');
        go.type = 'button';
        go.dataset.reportAssign = r.id;
        go.disabled = st.busy.has(r.id);
        go.addEventListener('click', async () => {
          if (st.busy.has(r.id)) return;
          st.busy.add(r.id);
          render();
          try { await onAssign?.(r, no); } finally { st.busy.delete(r.id); if (!st.closed) render(); }
        });
        acts.append(go);
      }
      if (acts.childNodes.length) item.append(acts);
      el.append(item);
    }
    for (const fn of st.listeners) fn(open.length);
    // Markierte Meldung einmal in Sicht holen (auch wenn die Zeile erst mit der Live-Abfrage ankommt).
    const hit = st.marked && st.scrolled !== st.marked ? el.querySelector(`[data-report-id="${CSS.escape(st.marked)}"]`) : null;
    if (hit) { st.scrolled = st.marked; hit.scrollIntoView?.({ block: 'nearest' }); }
  }

  let off = null;
  try {
    const lq = api.query(REPORT_COLLECTIONS.reports, { selector: { module_id: appId } });
    off = lq.subscribe((rows) => { if (st.closed) return; st.rows = sortReports(rows); render(); });
    st.rows = sortReports(lq.get?.() ?? []);
  } catch { st.rows = []; }
  let offUsers = null;
  try {
    const uq = api.query(USERS_COLLECTION, {});
    offUsers = uq.subscribe((rows) => { if (st.closed) return; st.names = userNames(rows); render(); });
    st.names = userNames(uq.get?.() ?? []);
  } catch { st.names = new Map(); }
  render();

  return {
    el,
    get count() { return st.rows.filter(isOpenReport).length; },
    onCount(fn) { st.listeners.add(fn); fn(st.rows.filter(isOpenReport).length); return () => st.listeners.delete(fn); },
    rows: () => st.rows.slice(),
    /** Offene Meldungen, die ein Agent-Lauf auf diesem Entwurf bearbeitet (Kandidaten für „behebt Meldung #N“). */
    linked(draftId) { return st.rows.filter((r) => r.status === 'running' && r.coding_turn_id && (!draftId || !r.coding_draft_id || r.coding_draft_id === draftId)); },
    render,
    /** Meldung markieren (null = keine); sie kommt einmal in Sicht. */
    mark(reportId) { st.marked = reportId ? String(reportId) : null; st.scrolled = null; render(); },
    get marked() { return st.marked; },
    destroy() { st.closed = true; off?.(); offUsers?.(); st.listeners.clear(); el.remove(); },
  };
}
