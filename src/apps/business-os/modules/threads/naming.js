// Anzeigename eines Threads nach Projekt und Aufgabe (workjet#594 „Threads heißen wie Kennungen“).
//
// Nur vorhandene Felder, nichts erfunden. Kennungen (UUIDs, cmd_/thread_-IDs, Hashes, die eigenen IDs des Threads)
// sind nie ein Name; sie stehen nur in den Details. Rückfall-Reihenfolge (dokumentiert in V4-PORT.md §workjet#594):
//
//   Projekt  1. workjet_projects[project_id].name; project_id aus business_commands.payload.project_id
//               (verknüpfter Befehl) → user_threads.metadata.workjet_supervisor.project_id → source_record_id, wenn
//               source_record_type = workjet_project
//            2. user_threads.source_label (= Quellkontext-Label der Projektion), wenn keine Kennung
//            3. business_apps[source_module].title, sonst source_module
//   Aufgabe  1. user_threads.title  2. ctox_queue_tasks.title (verknüpfte Aufgabe)  3. business_commands.payload.title
//            4. user_threads.source_label  5. metadata.source_context.label  6. business_commands.payload.prompt
//               (erste Zeile, ≤ 120 Zeichen wie ctoxThreadTitle der Projektion)  7. user_threads.next_step
//            8. business_commands.command_type – nie roh: über COMMAND_TYPE_NAMES (längstes Präfix) wird daraus
//               Quelle + sprechende Ableitung („ctox.workjet.project.list“ → „Workjet · Projektliste“); die Quelle
//               ersetzt nur ein Projekt der Stufe 3 (App-Titel). Ohne Tabellentreffer keine Ableitung (weiter mit 9).
//            9. „Supervisor“ (metadata.workjet_supervisor vorhanden) bzw. „CTOX Arbeit“ (kind = ctox_task, Wortlaut
//               der Projektion) – sonst leer.
//   Jeder Kandidat wird übersprungen, wenn er eine Kennung ist oder dem Projekt gleicht. Enthält die Aufgabe das
//   Projekt bereits wörtlich, entfällt das Projekt.

const UUID_PART = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}/i;
const PREFIXED_ID = /^(cmd|thread|msg|message|approval|task|link|notif|notification|state|run|op)[_:][\w.:-]+$/i;
const HEX_ID = /^[0-9a-f]{16,}$/i;
// Rückfalltitel der Projektion: `CTOX Arbeit ${fallbackId}` (handlers/_projection.ctox_relevance.js ctoxThreadTitle).
const PROJECTION_FALLBACK = /^CTOX Arbeit\s+/;

export function isIdentifier(value, known = []) {
  const text = String(value ?? '').trim();
  if (!text) return true;
  if (known.some((id) => id && String(id).trim() === text)) return true;
  const core = text.replace(PROJECTION_FALLBACK, '');
  if (core !== text && isIdentifier(core, known)) return true;
  if (/\s/.test(core)) return false; // Sätze und Wortfolgen sind keine Kennungen
  return UUID_PART.test(core) || PREFIXED_ID.test(core) || HEX_ID.test(core);
}

// Feste Übersetzungstabelle für Befehlstyp-Präfixe (Stufe 8; dokumentiert in V4-PORT.md). Treffer nur an
// Segmentgrenzen, das längste Präfix gewinnt. [Präfix, Quelle, Ableitung de, Ableitung en] – leere Ableitung = nur Quelle.
export const COMMAND_TYPE_NAMES = [
  ['ctox.workjet.project.supervisor.turn', 'Workjet', 'Supervisor-Zug', 'Supervisor turn'],
  ['ctox.workjet.project.supervisor', 'Workjet', 'Supervisor', 'Supervisor'],
  ['ctox.workjet.project.list', 'Workjet', 'Projektliste', 'Project list'],
  ['ctox.workjet.project.kpis', 'Workjet', 'Projektkennzahlen', 'Project KPIs'],
  ['ctox.workjet.project.chat', 'Workjet', 'Projektchat', 'Project chat'],
  ['ctox.workjet.project.worker', 'Workjet', 'Projekt-Worker', 'Project workers'],
  ['ctox.workjet.project', 'Workjet', 'Projekt', 'Project'],
  ['ctox.workjet.session.transfer', 'Workjet', 'Sitzungsübergabe', 'Session transfer'],
  ['ctox.workjet.session', 'Workjet', 'Sitzung', 'Session'],
  ['ctox.workjet.computer', 'Workjet', 'Rechner', 'Computer'],
  ['ctox.workjet.providers', 'Workjet', 'Anbieter', 'Providers'],
  ['ctox.workjet.jour_fixe', 'Workjet', 'Jour fixe', 'Jour fixe'],
  ['ctox.workjet', 'Workjet', '', ''],
];

/** Quelle und sprechende Ableitung eines Befehlstyps aus COMMAND_TYPE_NAMES, sonst null (nichts erfunden). */
export function commandTypeName(commandType, lang = 'de') {
  const type = String(commandType ?? '').trim();
  if (!type) return null;
  let hit = null;
  for (const row of COMMAND_TYPE_NAMES) {
    if ((type === row[0] || type.startsWith(`${row[0]}.`)) && (!hit || row[0].length > hit[0].length)) hit = row;
  }
  if (!hit) return null;
  return { source: hit[1], action: (lang === 'en' ? hit[3] : hit[2]) || '' };
}

function firstLine(value, max = 120) {
  const line = String(value ?? '').split('\n').map((part) => part.trim()).find(Boolean) || '';
  const chars = [...line];
  return chars.length > max ? `${chars.slice(0, max).join('')}` : line;
}

function same(a, b) {
  return String(a || '').trim().toLowerCase() === String(b || '').trim().toLowerCase();
}

/**
 * @param {object} thread user_threads-Dokument
 * @param {object} lookups { command(id), task(id), project(id), appTitle(module) } – liefern Dokument/Text oder null
 * @param {object} [labels] { supervisor, ctoxWork, lang }
 * @returns {{ project: string, task: string, title: string, command: object|null, queueTask: object|null }}
 */
export function threadName(thread, lookups = {}, labels = {}) {
  const t = thread || {};
  const meta = t.metadata && typeof t.metadata === 'object' ? t.metadata : {};
  const sourceContext = meta.source_context && typeof meta.source_context === 'object' ? meta.source_context : {};
  const supervisor = meta.workjet_supervisor && typeof meta.workjet_supervisor === 'object' ? meta.workjet_supervisor : null;
  const recordType = String(t.source_record_type || '');
  const recordId = String(t.source_record_id || '');
  const fromCtox = String(t.source_module || '') === 'ctox';
  let command = fromCtox && recordType === 'command' ? lookups.command?.(recordId) || null : null;
  const queueTask = (fromCtox && ['queue_task', 'ctox_task', 'task'].includes(recordType) ? lookups.task?.(recordId) : null)
    || (command?.task_id ? lookups.task?.(command.task_id) : null) || null;
  if (!command && queueTask?.command_id) command = lookups.command?.(queueTask.command_id) || null;
  const known = [t.id, t.thread_id, recordId, command?.id, command?.command_id, queueTask?.id];
  const usable = (value) => (isIdentifier(value, known) ? '' : String(value).trim());

  const payload = command?.payload && typeof command.payload === 'object' ? command.payload : {};
  const projectId = payload.project_id || supervisor?.project_id || (recordType === 'workjet_project' ? recordId : '');
  let project = projectId ? usable(lookups.project?.(projectId)?.name) : '';
  if (!project) project = usable(t.source_label);
  const projectFromApp = !project;
  if (!project && t.source_module) project = usable(lookups.appTitle?.(t.source_module) || t.source_module);

  const candidates = [
    t.title,
    queueTask?.title,
    payload.title,
    t.source_label,
    sourceContext.label,
    firstLine(payload.prompt),
    t.next_step,
  ];
  let task = '';
  for (const candidate of candidates) {
    const value = usable(candidate);
    if (value && !same(value, project)) { task = firstLine(value, 400); break; }
  }
  // Stufe 8: der Befehlstyp nie roh als Name, nur über die feste Tabelle.
  const derived = task ? null : commandTypeName(command?.command_type || command?.type, labels.lang);
  if (derived) {
    if (projectFromApp || !project) project = derived.source;
    if (!same(derived.action, project)) task = derived.action;
  }
  if (!task && supervisor) task = labels.supervisor || 'Supervisor';
  if (!task && t.kind === 'ctox_task') task = labels.ctoxWork || 'CTOX Arbeit';
  if (project && task && task.toLowerCase().includes(project.toLowerCase())) project = '';
  const title = [project, task].filter(Boolean).join(' · ') || labels.thread || 'Thread';
  return { project, task, title, command, queueTask };
}

/** IDs der Befehle und Aufgaben, die `threadName` für diese Threads nachschlägt (für eine gezielte Live-Abfrage). */
export function namingLookupIds(threads, commands = [], tasks = []) {
  const commandIds = new Set();
  const taskIds = new Set();
  for (const t of threads || []) {
    if (String(t?.source_module || '') !== 'ctox') continue;
    const id = String(t.source_record_id || '').trim();
    if (!id) continue;
    if (t.source_record_type === 'command') commandIds.add(id);
    else if (['queue_task', 'ctox_task', 'task'].includes(t.source_record_type)) taskIds.add(id);
  }
  for (const c of commands || []) if (c?.task_id) taskIds.add(String(c.task_id));
  for (const q of tasks || []) if (q?.command_id) commandIds.add(String(q.command_id));
  return { commandIds: [...commandIds].sort(), taskIds: [...taskIds].sort() };
}
