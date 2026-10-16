// Gemeinsamer Kontext-Chat-Baustein (Shell v4, Strang SKL-D): „Mit CTOX arbeiten / Frage / App ändern“.
//
// Ersetzt die acht Kopien des Rechtsklick-Kontextmenüs (documents, spreadsheets, buchhaltung, knowledge, research,
// matching, desktop, app-store). Reiner, app-neutraler Code: Menüeinträge, Kontext-Payload und Absenden über
// api.command. Die App übergibt nur ihren Kontext ({source_app, record_ref, selection, text, …}); Anweisungstext,
// Skill und Rückweg liegen bei der Chat-Eigentümer-App, die die beiden Standardbefehle anbietet:
//   - CONTEXT_CHAT_COMMAND   Kontext-Auftrag an CTOX (Modus data|ask)
//   - APP_MODIFY_COMMAND     App ändern (Modus app; Rechteprüfung im Handler, hier nur Darstellung)
// Kein DOM-Zugriff außer der Markup-Hilfe (liefert einen String), keine Collections, keine Sonderwege.

export const CONTEXT_CHAT_COMMAND = 'ctox.chat.task';
export const APP_MODIFY_COMMAND = 'ctox.app.modify.request';
/** Ziel-App des Sprungs nach dem Absenden (Eigentümerin des Chats). */
export const CONTEXT_CHAT_OWNER = 'ctox';

export const CONTEXT_CHAT_MODES = Object.freeze(['data', 'ask', 'app']);
export const CONTEXT_CHAT_TEXT_MAX = 8000;
const SELECTION_MAX = 1000;

const DEFAULT_LABELS = Object.freeze({
  data: 'Mit Daten arbeiten',
  ask: 'Frage beantworten',
  app: 'App modifizieren',
});

const text = (value) => (typeof value === 'string' ? value.trim() : (value == null ? '' : String(value).trim()));
const escapeDefault = (value) => String(value ?? '')
  .replaceAll('&', '&amp;')
  .replaceAll('<', '&lt;')
  .replaceAll('>', '&gt;')
  .replaceAll('"', '&quot;')
  .replaceAll("'", '&#39;');

/** Sichtbare Modi: data immer, ask optional, app nur mit Darstellungsrecht (Entscheidung im Handler). */
export function contextChatModes({ canModifyApp = false, includeAsk = true, labels = {} } = {}) {
  const pick = (mode) => ({ mode, label: text(labels[mode]) || DEFAULT_LABELS[mode] });
  return [pick('data'), ...(includeAsk ? [pick('ask')] : []), ...(canModifyApp ? [pick('app')] : [])];
}

/** Radiogruppe der Modi als Markup (erster Eintrag vorausgewählt). Name des Feldes: contextMode. */
export function contextChatModeMarkup(options = {}) {
  const escape = typeof options.escape === 'function' ? options.escape : escapeDefault;
  const cls = text(options.labelClass);
  return contextChatModes(options)
    .map((entry, index) => `<label${cls ? ` class="${escape(cls)}"` : ''}><input type="radio" name="contextMode" value="${escape(entry.mode)}"${index === 0 ? ' checked' : ''} /> ${escape(entry.label)}</label>`)
    .join('\n        ');
}

/** Gewählten Modus auf einen sichtbaren Modus zurückführen (app ohne Recht → data, ask ohne Angebot → data). */
export function normalizeContextChatMode(mode, { canModifyApp = false, includeAsk = true } = {}) {
  if (mode === 'app') return canModifyApp ? 'app' : 'data';
  if (mode === 'ask') return includeAsk ? 'ask' : 'data';
  return 'data';
}

/** Aktuelle Textauswahl im Fenster (gekappt), leer ohne Auswahl oder außerhalb des Browsers. */
export function currentSelectionText(win = globalThis) {
  try {
    return text(win?.getSelection?.()?.toString?.()).slice(0, SELECTION_MAX);
  } catch {
    return '';
  }
}

function plainContext(context) {
  if (!context || typeof context !== 'object' || Array.isArray(context)) return {};
  try {
    return JSON.parse(JSON.stringify(context));
  } catch {
    return {};
  }
}

/**
 * Kontext → Standardbefehl. Liefert {type, payload, record_id}; wirft {code:'invalid'} bei fehlender App oder
 * leerem/zu langem Text. payload: {source_app, record_ref, selection, text, mode, title?, thread_key?, context}.
 */
export function buildContextChatCommand(spec = {}) {
  const sourceApp = text(spec.source_app);
  if (!sourceApp) throw { code: 'invalid', message: 'source_app is required' };
  const message = text(spec.text);
  if (!message) throw { code: 'invalid', message: 'text is required' };
  if ([...message].length > CONTEXT_CHAT_TEXT_MAX) {
    throw { code: 'invalid', message: `text has more than ${CONTEXT_CHAT_TEXT_MAX} characters` };
  }
  const mode = normalizeContextChatMode(spec.mode, { canModifyApp: spec.canModifyApp === true, includeAsk: spec.includeAsk !== false });
  const recordRef = text(spec.record_ref);
  const payload = {
    source_app: sourceApp,
    record_ref: recordRef,
    selection: text(spec.selection).slice(0, SELECTION_MAX),
    text: message,
    mode,
    context: plainContext(spec.context),
  };
  const title = text(spec.title);
  if (title) payload.title = title;
  const threadKey = text(spec.thread_key);
  if (threadKey) payload.thread_key = threadKey;
  const type = mode === 'app' ? APP_MODIFY_COMMAND : CONTEXT_CHAT_COMMAND;
  return { type, payload, record_id: mode === 'app' ? sourceApp : (recordRef || sourceApp) };
}

/**
 * Absenden über die Standardschnittstelle: api.command(type, payload, {record_id}) und Sprung in die Chat-App
 * (Route command_id + chat_id). `send(type, payload, opts)` ersetzt api.command, wenn die App Befehle über einen
 * eigenen Status-Wrapper schickt. Liefert den CommandHandle (oder null, wenn der Wrapper keinen liefert).
 */
export function sendContextChat(api, spec, { send = null, navigate = true } = {}) {
  const { type, payload, record_id } = buildContextChatCommand(spec);
  const handle = typeof send === 'function'
    ? send(type, payload, { record_id })
    : api.command(type, payload, { record_id });
  const commandId = text(handle?.id);
  if (navigate && commandId && typeof api?.shell?.navigate === 'function') {
    api.shell.navigate(CONTEXT_CHAT_OWNER, { command_id: commandId, chat_id: commandId, source_app: payload.source_app, mode: payload.mode });
  }
  return handle ?? null;
}
