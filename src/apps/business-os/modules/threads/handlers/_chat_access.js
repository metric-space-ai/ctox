// Workjet-Privatchat-Zugriffsschutz als threads-Fachlogik (Shell v4, Strang WJAPP).
// Ersetzt die frühere Core-Capability „workjet command_access_check“ (Strang WJAPP: Capability entfällt).
// ref: src/core/business_os/project_chats/privacy.rs @4794af03d
//
// Nur eigene Lesezugriffe über ctx.get auf foreign_collections (Eigentümer ctox bzw.
// ctox-platform, siehe V4-HANDLERS.md §6): workjet_project_chats, workjet_worker_profile_bindings,
// workjet_projects, business_commands. Keine Capability, kein Schreiben.
// Bewusst ohne Import aus _lib.js (keine Modulzyklen: _lib.preflight importiert diese Datei).

const CHATS = 'workjet_project_chats'; // ref: project_chats.rs:43
const PROFILE_BINDINGS = 'workjet_worker_profile_bindings'; // ref: worker_profile_bindings.rs:15
const PROJECTS = 'workjet_projects'; // ref: project_chats.rs:351
const COMMANDS = 'business_commands';
// Rust folgt Verknüpfungsketten unbegrenzt (Zyklus = Stapelüberlauf); hier fail-closed nach 8 Gliedern (U-W4).
const MAX_ASSOCIATION_DEPTH = 8;

export const ACCESS_UNAVAILABLE = 'Workjet chat is unavailable to this user or instance';
export const ACCESS_ADD_PARTICIPANT = 'Workjet chat cannot add another human participant';
export const ACCESS_ASSIGN_PARTICIPANT = 'Workjet chat cannot assign another human participant';

const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
const has = (v, k) => isObject(v) && Object.prototype.hasOwnProperty.call(v, k) && v[k] !== undefined;
const str = (v) => (typeof v === 'string' ? v : undefined);

// ref: privacy.rs:11-17
const isChatId = (value) => value.startsWith('workjet_private_') || value.startsWith('workjet_group_');
const isProfileBindingId = (value) => value.startsWith('workjet_profile_');

/**
 * Nur typisierte Hüllen-/Verweisfelder werden verfolgt; Nachrichtentext wird nie als
 * Identität gedeutet. ref: privacy.rs:19-51
 */
export function references(document, result = new Set()) {
  if (!isObject(document)) return result;
  for (const [key, value] of Object.entries(document)) {
    if (key === 'id' || key === 'record_id' || key === 'binding_id'
      || key.endsWith('chat_id') || key.endsWith('thread_id')) {
      if (typeof value === 'string' && (isChatId(value) || isProfileBindingId(value))) result.add(value);
    }
  }
  for (const field of ['payload', 'client_context', 'result', 'metadata', 'source_context', 'source']) {
    if (has(document, field)) references(document[field], result);
  }
  return result;
}

function isJourFixeCommand(kind) {
  return kind.startsWith('ctox.workjet.jour_fixe.');
}

/**
 * Projektbefehl der Workjet-Steuerung. ref: privacy.rs:53-78.
 * U-W2: `is_command(kind)` (project_chats.rs:81, native Typliste) ist hier auf das Präfix
 * verkürzt – jeder Typ mit Präfix ctox.workjet.project. oder ctox.workjet.jour_fixe. gilt als Projektbefehl (strenger, nie lockerer).
 */
function projectCommand(collection, document) {
  if (collection !== COMMANDS) return false;
  if (['id', 'command_id'].some((f) => (str(document?.[f]) ?? '').startsWith('workjet_project_native_'))) return true;
  const kind = str(document?.command_type);
  return kind !== undefined && (kind.startsWith('ctox.workjet.project.') || isJourFixeCommand(kind));
}

/** Verknüpfungskette Abbruch-Quittung → Zielbefehl. ref: privacy.rs:80-128 (nur business_commands erreichbar). */
function associations(collection, document) {
  const out = [];
  if (collection === COMMANDS
    && ['id', 'command_id'].some((f) => (str(document?.[f]) ?? '').startsWith('workjet_project_cancel_'))) {
    const target = str(document?.payload?.target_command_id) ?? '';
    const ok = target.startsWith('workjet_project_native_') || target.startsWith('workjet_crew_')
      || target.startsWith('ctox_delegate_');
    out.push([COMMANDS, ok ? target : '']);
  }
  return out;
}

/** ref: privacy.rs:130-137 */
export function hasRestrictedReference(collection, document) {
  if (projectCommand(collection, document)) return true;
  return references(document).size > 0 || associations(collection, document).length > 0;
}

function utf8Length(value) {
  let n = 0;
  for (const ch of value) {
    const cp = ch.codePointAt(0);
    n += cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
  }
  return n;
}

// char::is_control = Unicode Cc
const hasControl = (value) => /[\u0000-\u001f\u007f-\u009f]/.test(value);

async function load(store, collection, id) {
  if (typeof id !== 'string' || id === '') return null;
  const doc = await store.get(collection, id);
  return doc && doc._deleted !== true ? doc : null;
}

/** owned_project(conn, id, owner, require_active=false). ref: project_chats.rs:344-362, required :406-413 */
async function ownsProject(store, projectId, owner) {
  if (typeof projectId !== 'string') return false;
  const id = projectId.trim();
  if (id === '' || utf8Length(id) > 128 || hasControl(id)) return false;
  const project = await load(store, PROJECTS, id);
  return Boolean(project) && project.owner_user_id === owner && project.is_deleted !== true;
}

/**
 * visible_in_store für business_commands-Dokumente. ref: privacy.rs:379-471.
 * U-W1: Identitäts-Alias (workjet_identity::owner_from_connection) ist Kern-Sache –
 * command.actor.id gilt als kanonische Nutzer-ID.
 */
async function visibleInStore(store, collection, document, userId) {
  if (userId === '') return false;
  const owner = userId;
  const refs = references(document);
  const isProject = projectCommand(collection, document);
  if (isProject) {
    // U-W3: Besprechungsbefehle lösen ihr Projekt über die native Meeting-Tabelle des Kerns auf
    // (privacy.rs:419-436); ohne diese Quelle hier fail-closed.
    if (isJourFixeCommand(str(document?.command_type) ?? '')) return false;
    if (!(await ownsProject(store, document?.payload?.project_id, owner))) return false;
  }
  for (const id of [...refs].sort()) {
    const chat = isChatId(id);
    const relation = await load(store, chat ? CHATS : PROFILE_BINDINGS, id);
    if (!relation || relation.owner_user_id !== owner || relation.is_deleted === true) return false;
    if (chat && !(await ownsProject(store, relation.project_id, owner))) return false;
  }
  return isProject || refs.size > 0;
}

/** ref: privacy.rs:245-263 (collect_constraints_with_reader) */
async function collectConstraints(store, collection, document, constraints, depth) {
  if (depth > MAX_ASSOCIATION_DEPTH) throw new Error('association chain too long');
  if (projectCommand(collection, document) || references(document).size > 0) constraints.push([collection, document]);
  for (const [related, id] of associations(collection, document)) {
    // canonical_association (privacy.rs:315-377): fehlender Zielbefehl ist ein Fehler, keine Freigabe.
    const target = await load(store, related, id);
    if (!target) throw new Error('referenced command is unavailable');
    await collectConstraints(store, related, target, constraints, depth + 1);
  }
}

/** Der Befehl als serialisiertes BusinessCommand (store.rs:575). */
export function commandDocument(command) {
  const module = str(command?.module) && command.module.trim() !== '' ? command.module : str(command?.app_id) ?? '';
  return {
    id: str(command?.id) ?? null,
    module,
    type: str(command?.type) ?? '',
    record_id: str(command?.record_id) ?? null,
    payload: command?.payload === undefined ? null : command.payload,
    client_context: command?.client_context === undefined ? null : command.client_context,
  };
}

/**
 * document_visible_to_actor(…, "business_commands", command): null = keine Workjet-Einschränkung,
 * true = sichtbar, false = verweigert. Lesefehler verweigern (Rust: `let Ok(..) else Some(false)`).
 * ref: privacy.rs:141-220, 265-292
 */
export async function commandVisibleToActor(store, document, userId) {
  if (!hasRestrictedReference(COMMANDS, document)) return null;
  const constraints = [];
  try {
    await collectConstraints(store, COMMANDS, document, constraints, 0);
  } catch {
    return false;
  }
  if (constraints.length === 0) return null;
  try {
    for (const [collection, doc] of constraints) {
      if (!(await visibleInStore(store, collection, doc, userId))) return false;
    }
  } catch {
    return false;
  }
  return true;
}

/**
 * project_chats::command_access_check – Vorspann jedes threads-Befehls.
 * Wirft {code:'forbidden'} mit den Rust-Texten. ref: privacy.rs:473-522
 */
export async function commandAccessCheck(store, command, authenticatedUser) {
  const document = commandDocument(command);
  const access = await commandVisibleToActor(store, document, authenticatedUser);
  if (access === false) throw { code: 'forbidden', message: ACCESS_UNAVAILABLE };
  if (access === null) return;
  // Privatchats Mensch↔Worker gewähren nie einem weiteren Menschen Zugang über die
  // generischen Zuständigen-/Erwähnungs-/Benachrichtigungsfelder.
  if (!document.type.startsWith('threads.')) return;
  const payload = isObject(document.payload) ? document.payload : {};
  for (const key of ['target_user_ids', 'participant_ids', 'watcher_user_ids', 'assignee_user_ids']) {
    const values = payload[key];
    if (Array.isArray(values) && !values.every((v) => v === authenticatedUser)) {
      throw { code: 'forbidden', message: ACCESS_ADD_PARTICIPANT };
    }
  }
  for (const key of ['target_user_id', 'assigned_user_id', 'user_id', 'reviewer_user_id']) {
    const value = payload[key];
    if (typeof value === 'string' && !(value === '' || value === authenticatedUser)) {
      throw { code: 'forbidden', message: ACCESS_ASSIGN_PARTICIPANT };
    }
  }
}
