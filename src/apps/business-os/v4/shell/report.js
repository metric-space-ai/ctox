// Shell v4 – Meldeschleife, Shell-Seite (FR6b, M-06, Bedienkonzept §3a „Melden aus jedem Fenster“).
//
//   reportProblem({ appId, kind = 'bug' | 'wish', windowId? }) → Promise<{ report_id } | null>
//
// Öffnet im App-Fenster ein Blatt (dasselbe Fensterblatt wie api.shell.dialog, ../frame/window-sheet.js; nichts
// außerhalb des Fensters) mit zwei Schritten: „Was ist passiert?“ (bei 'wish': „Was wünschst du dir?“) und „Was hast
// du erwartet?“. Automatisch angehängt (client_context): App, Titel und Version, geöffneter Datensatz (falls im
// Fenster markiert), die aktuellen Fehler/Warnungen der Zustandszone, Fenstergröße, Zeitpunkt.
// Fensterbild: NICHT angehängt – ohne Fremdbibliothek gibt es im Browser keinen verlässlichen Weg (getDisplayMedia
// fragt den Nutzer, SVG-foreignObject verliert Bilder/Canvas/Stylesheets fremder Herkunft). Vermerkt in FR6b.md.
//
// Gespeichert wird wie heute in „Bugs & Features“ (modules/reports liest business_module_reports + ctox_bug_reports,
// geschrieben bisher von shared/business-reporter.js persistLocalBusinessReport): dieselben zwei Zeilen, über die
// App-API der Shell (api.put). Abbrechen speichert nichts. Fehler beim Speichern → Zoneneintrag `shell.report` im
// Fenster (einziger Ort für Zustände); Erfolg bleibt still.
//
// Der Rahmen (../frame/index.js) meldet sich mit installReporter(ctx) an; ctx.window(appId, windowId) liefert
// { appId, appTitle, title, ver, windowId, element, content, sheet, zone, focus } oder null, ctx.api() die
// Shell-App-API. Beim Shell-Tausch löst der alte Rahmen nur seine eigene Anmeldung.

let installed = null;

export const REPORT_CODE = 'shell.report';
export const REPORT_COLLECTIONS = Object.freeze({ reports: 'business_module_reports', bugs: 'ctox_bug_reports' });

const TEXT = Object.freeze({
  bug: { title: 'Problem melden', what: 'Was ist passiert?', next: 'Weiter' },
  wish: { title: 'Wunsch äußern', what: 'Was wünschst du dir?', next: 'Weiter' },
  expected: 'Was hast du erwartet?',
  send: 'Senden',
  failed: 'Meldung nicht gespeichert',
  hide: 'Ausblenden',
});

export function installReporter(ctx) {
  const mine = { ...ctx };
  installed = mine;
  return () => { if (installed === mine) installed = null; };
}

const newId = () => (globalThis.crypto?.randomUUID?.() || `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 10)}`);

// Geöffneter Datensatz: was die App im Fenster als ausgewählt markiert (gleiche Attribute wie das Kontextmenü).
export function openRecordOf(content) {
  if (!content?.querySelector) return null;
  const el = content.querySelector([
    '[data-context-record-id][aria-selected="true"]', '[data-record-id][aria-selected="true"]',
    '[data-context-record-id][aria-current]:not([aria-current="false"])', '[data-record-id][aria-current]:not([aria-current="false"])',
    '[data-context-record-id].is-selected', '[data-record-id].is-selected',
    '[data-context-record-id].is-active', '[data-record-id].is-active',
  ].join(','));
  if (!el) return null;
  const id = el.getAttribute('data-context-record-id') || el.getAttribute('data-record-id') || '';
  if (!id) return null;
  const type = el.getAttribute('data-context-record-type') || el.getAttribute('data-record-type') || '';
  const label = (el.getAttribute('data-context-record-label') || el.getAttribute('aria-label') || el.innerText || el.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 120);
  return { id, type, label };
}

export function recentZoneErrors(zone, limit = 5) {
  let list = [];
  try { list = zone?.entries?.() || []; } catch { list = []; }
  if (!Array.isArray(list)) list = [...(list || [])];
  return list.filter((e) => e && (e.level === 'error' || e.level === 'warn') && e.code !== REPORT_CODE)
    .sort((a, b) => (b.at || 0) - (a.at || 0)).slice(0, limit)
    .map((e) => ({ level: e.level, code: String(e.code), text: String(e.text ?? ''), source: String(e.source ?? '') }));
}

// Die zwei Zeilen wie shared/business-reporter.js persistLocalBusinessReport (Bugs & Features liest beide).
export function buildReportRows({ id, kind, summary, expected, win, reporterId = '', now = Date.now(), record = null, errors = [] }) {
  const k = kind === 'wish' ? 'feature' : 'bug';
  const firstLine = String(summary).trim().split('\n')[0].trim();
  const title = firstLine.length > 80 ? `${firstLine.slice(0, 79)}…` : firstLine;
  const rect = win.element?.getBoundingClientRect?.();
  const clientContext = {
    source: 'shell-v4.report',
    app_id: win.appId,
    app_title: win.appTitle || '',
    window_title: win.title || '',
    app_ver: win.ver || '',
    window_id: win.windowId || '',
    record,
    recent_errors: errors,
    window_size: rect ? { w: Math.round(rect.width), h: Math.round(rect.height) } : null,
    window_image: null,
    report_delivery: { status: 'not_delegated', command_id: '', task_id: '' },
  };
  const report = {
    id, report_id: id, module_id: win.appId, kind: k, severity: 'medium', title, summary, expected,
    status: 'open', reporter_id: reporterId, ctox_command_id: '', task_id: '', inbound_channel: win.appId,
    client_context: clientContext, created_at_ms: now, updated_at_ms: now,
  };
  const bug = {
    id, title, status: 'open', module: win.appId, inbound_channel: win.appId, severity: 'medium', surface: 'business-os',
    description: summary, evidence: clientContext,
    payload: { kind: k, expected, ctox_command_id: '', task_id: '', delivery_status: 'not_delegated', change_summary: '', rollback_version_id: '' },
    created_at_ms: now, updated_at_ms: now,
  };
  return { report, bug };
}

export async function reportProblem({ appId, kind = 'bug', windowId = null } = {}) {
  const ctx = installed;
  if (!ctx) return null;
  const win = ctx.window(String(appId || ''), windowId ? String(windowId) : null);
  if (!win) return null;
  const k = kind === 'wish' ? 'wish' : 'bug';
  const copy = TEXT[k];
  win.focus?.();
  const summary = await win.sheet.dialog({ kind: 'prompt', title: copy.title, text: copy.what, multiline: true, required: true, confirm_label: copy.next });
  if (summary == null || !String(summary).trim()) return null;
  const expected = await win.sheet.dialog({ kind: 'prompt', title: copy.title, text: TEXT.expected, multiline: true, required: true, confirm_label: TEXT.send });
  if (expected == null || !String(expected).trim()) return null;
  const api = ctx.api?.() || null;
  const id = `report_${newId()}`;
  const { report, bug } = buildReportRows({
    id, kind: k, summary: String(summary).trim(), expected: String(expected).trim(), win,
    reporterId: String(api?.actor?.id || ''), record: openRecordOf(win.content), errors: recentZoneErrors(win.zone),
  });
  try {
    if (!api || typeof api.put !== 'function') throw new Error('Shell-App-API ohne put');
    const a = api.put(REPORT_COLLECTIONS.reports, report);
    const b = api.put(REPORT_COLLECTIONS.bugs, bug);
    await Promise.all([a?.local, b?.local]);
    win.zone?.clear?.(REPORT_CODE);
    return { report_id: id, report, bug };
  } catch (error) {
    win.zone?.report?.({ level: 'error', code: REPORT_CODE, text: `${TEXT.failed}: ${error?.message ?? error}`, source: 'shell' });
    return null;
  }
}

// ================= Meldeschleife Schritte 2–5 (Strang LOOP, Bedienkonzept §3a) =================
// Eine Meldung gehört zu genau einer App (business_module_reports.module_id = app_id). Die Code-Ansicht zeigt die
// offenen Meldungen der App (Reiter „Meldungen“, ../source/reports-tab.js), „Agent beauftragen“ sendet ctox.coding.turn
// (§16.44) auf den EIGENEN Entwurf und verknüpft die Meldung mit dem Lauf (Status running = „in Arbeit“, coding_turn_id),
// „Veröffentlichen“ schlägt die Versionsnotiz „behebt Meldung #N“ vor und setzt die genannten Meldungen auf behoben
// (Status completed + fixed_in_ver/fixed_note), die meldende Person sieht am Meldungs-Badge ihres Fensters dieser App
// genau einmal „Deine Meldung #N ist behoben“ (Aktion „Ansehen“); danach ist sie gelesen (reporter_seen_at_ms).
// Statuswerte wie „Bugs & Features“ (modules/reports: open | running | completed | blocked); Felder additiv (Schema
// additionalProperties: true). Alle Schreibungen über die App-API der Shell (api.patch, RFC-7396-Merge).

export const FIXED_CODE = 'shell.report.fixed';
export const LOOP_TEXT = Object.freeze({
  kind: { bug: 'Problem', feature: 'Wunsch' },
  status: { open: 'Offen', running: 'In Arbeit', completed: 'Behoben', blocked: 'Blockiert' },
  fixed: (no) => `Deine Meldung #${no} ist behoben`,
  view: 'Ansehen',
});
const OPEN = new Set(['open', 'running', '']);

export const isOpenReport = (r) => !!r && r._deleted !== true && OPEN.has(String(r.status ?? 'open'));
const byCreated = (a, b) => (Number(a.created_at_ms) || 0) - (Number(b.created_at_ms) || 0) || String(a.id).localeCompare(String(b.id));

/** Alle Meldungen einer App in Meldereihenfolge (Grundlage der Nummer #N). */
export function appReports(rows, appId) {
  return (Array.isArray(rows) ? rows : []).filter((r) => r && r._deleted !== true && r.module_id === appId).sort(byCreated);
}
/** Meldungen in Meldereihenfolge (ohne Filter; die App-Auswahl trifft die Abfrage). */
export const sortReports = (rows) => (Array.isArray(rows) ? rows : []).filter((r) => r && r._deleted !== true).sort(byCreated);
/** Nummer #N einer Meldung innerhalb ihrer App: report_no, sonst Rang in Meldereihenfolge (1-basiert). */
export function reportNo(report, rows) {
  if (Number.isInteger(report?.report_no) && report.report_no > 0) return report.report_no;
  const i = appReports(rows, report?.module_id).findIndex((r) => r.id === report?.id);
  return i < 0 ? null : i + 1;
}

/** Auftrag an den Agenten: Meldungstext + Erwartung + App/Version + letzte Zonenfehler (Meldung und jetzt). */
export function codingPromptFor(report, { no = null, appTitle = '', appVer = '', zoneErrors = [] } = {}) {
  const cc = report?.client_context || {};
  const kind = report?.kind === 'bug' ? 'Problem' : 'Wunsch';
  const lines = [`Meldung${no ? ` #${no}` : ''} (${kind}): ${report?.title || ''}`.trim(), ''];
  if (report?.summary && report.summary.trim() !== String(report?.title || '').replace(/…$/, '').trim()) lines.push(`Was passiert ist: ${report.summary}`);
  if (report?.expected) lines.push(`Erwartet: ${report.expected}`);
  lines.push(`App: ${appTitle || cc.app_title || report?.module_id || ''}${cc.app_ver || appVer ? ` · Version ${cc.app_ver || appVer}` : ''}${appVer && cc.app_ver && appVer !== cc.app_ver ? ` (aktiv: ${appVer})` : ''}`);
  if (cc.record?.id) lines.push(`Geöffneter Datensatz: ${cc.record.type ? `${cc.record.type} ` : ''}${cc.record.id}${cc.record.label ? ` („${cc.record.label}“)` : ''}`);
  const seen = new Set();
  const errs = [...(Array.isArray(cc.recent_errors) ? cc.recent_errors : []), ...zoneErrors]
    .filter((e) => e && e.code && !String(e.code).startsWith('agent.') && !String(e.code).startsWith(REPORT_CODE))
    .filter((e) => { const k = `${e.code}\u0000${e.text}`; if (seen.has(k)) return false; seen.add(k); return true; }).slice(0, 8);
  if (errs.length) { lines.push('', 'Letzte Fehler im Fenster:'); for (const e of errs) lines.push(`- ${e.level ?? 'error'} ${e.code}: ${e.text ?? ''}`); }
  lines.push('', 'Behebe das im Entwurf dieser App und halte die Änderung klein.');
  return lines.join('\n');
}

/** Versionsnotiz-Vorschlag „behebt Meldung #3“ bzw. „behebt Meldungen #3, #5“. */
export function fixNoteFor(nos) {
  const list = [...new Set((nos || []).filter((n) => Number.isInteger(n)))].sort((a, b) => a - b);
  if (!list.length) return '';
  return `behebt ${list.length === 1 ? 'Meldung' : 'Meldungen'} ${list.map((n) => `#${n}`).join(', ')}`;
}
/** Welche Nummern nennt die Notiz (#N als eigenes Token)? */
export function notedNos(note) {
  return new Set([...String(note ?? '').matchAll(/#(\d+)\b/g)].map((m) => Number(m[1])));
}

/** Meldung mit einem angenommenen Agent-Lauf verknüpfen (Schritt 3): „in Arbeit“ + turn_id + Befehl. */
export async function linkReportToTurn(api, report, { turn_id, command_id = '', draft_id = '', now = Date.now() }) {
  const delivery = { status: 'delegated', command_id: command_id || '', task_id: '', turn_id };
  const a = api.patch(REPORT_COLLECTIONS.reports, report.id, {
    status: 'running', ctox_command_id: command_id || '', coding_turn_id: turn_id, coding_draft_id: draft_id,
    client_context: { report_delivery: delivery }, updated_at_ms: now,
  });
  const b = api.patch(REPORT_COLLECTIONS.bugs, report.id, {
    status: 'running', payload: { ctox_command_id: command_id || '', delivery_status: 'delegated', coding_turn_id: turn_id }, updated_at_ms: now,
  });
  await Promise.all([a?.local, b?.local]);
}

/** Veröffentlichte Version behebt die Meldungen (Schritt 4). */
export async function markReportsFixed(api, reports, { ver, note = '', actorId = '', now = Date.now() }) {
  const writes = [];
  for (const r of reports) {
    writes.push(api.patch(REPORT_COLLECTIONS.reports, r.id, {
      status: 'completed', fixed_in_ver: String(ver ?? ''), fixed_note: note, fixed_at_ms: now, fixed_by: actorId, reporter_seen_at_ms: null, updated_at_ms: now,
    })?.local);
    writes.push(api.patch(REPORT_COLLECTIONS.bugs, r.id, {
      status: 'completed', payload: { change_summary: note, fixed_in_ver: String(ver ?? ''), delivery_status: 'completed' }, updated_at_ms: now,
    })?.local);
  }
  await Promise.all(writes);
}

/**
 * Rückmeldung am Meldungs-Badge (Schritt 5): je Fenster einer App. Meldet jede behobene, noch ungelesene Meldung der
 * meldenden Person genau einmal als info-Eintrag der Zone (Badge, Aktion „Ansehen“ zeigt Notiz und Version im
 * Fensterblatt). Ansehen oder stilles Auslaufen des Eintrags (info-Frist der Zone) = gelesen (reporter_seen_at_ms).
 * Räumt sich mit der Zone (Fenster zu, Shell-Tausch).
 */
export function watchFixedReports({ api, appId, zone, sheet = null, actorId = null }) {
  if (!api?.query || !zone?.report) return () => {};
  const me = actorId ?? api.actor?.id ?? '';
  if (!me) return () => {};
  let lq;
  try { lq = api.query(REPORT_COLLECTIONS.reports, { selector: { module_id: appId } }); } catch { return () => {}; }
  const shown = new Set();
  let rows = [];
  const markSeen = (r) => {
    try { api.patch(REPORT_COLLECTIONS.reports, r.id, { reporter_seen_at_ms: Date.now() }); } catch { /* still */ }
  };
  const apply = (next) => {
    rows = Array.isArray(next) ? next : [];
    for (const r of rows) {
      if (r.reporter_id !== me || r.status !== 'completed' || !r.fixed_in_ver || r.reporter_seen_at_ms || shown.has(r.id)) continue;
      shown.add(r.id);
      const no = reportNo(r, rows);
      const code = `${FIXED_CODE}:${r.id}`;
      zone.report({
        level: 'info', code, text: LOOP_TEXT.fixed(no), source: 'shell', ref: r.id,
        action: {
          label: LOOP_TEXT.view,
          run: async () => {
            markSeen(r);
            await sheet?.dialog?.({ kind: 'alert', title: LOOP_TEXT.fixed(no), text: `${r.title || ''}\n\nVersion ${r.fixed_in_ver}${r.fixed_note ? ` – ${r.fixed_note}` : ''}` });
            return true;
          },
        },
        dismiss: () => markSeen(r),
      });
    }
  };
  const off = lq.subscribe(apply);
  apply(lq.get?.());
  let detached = false;
  const destroyZone = zone.destroy;
  const detach = () => { if (detached) return; detached = true; off?.(); if (zone.destroy === wrapped) zone.destroy = destroyZone; };
  function wrapped(...a) { detach(); return destroyZone?.apply(zone, a); }
  if (typeof destroyZone === 'function') zone.destroy = wrapped;
  return detach;
}
