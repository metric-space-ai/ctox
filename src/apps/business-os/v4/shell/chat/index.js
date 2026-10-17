// Shell v4 – Business-Chat (CHAT4): die Chatleiste der Shell-V2 (shared/business-chat.js, byte-gleich) im v4-Rahmen.
//
// Die V2-Abhängigkeiten von initBusinessChat({session, commandBus, db, sync, getActiveModule}) entstehen wie bei der
// Workjet-Brücke über createV2Compat (App 'shell'):
//   commandBus.dispatch          → compat.state.commandBus (api.command, Befehl business_os.chat.task); task_id der
//                                  V2-Quittung aus dem Befehlsergebnis des Empfängers
//   sync.startCollection         → compat.state.sync.startCollection (Liveness statt Pull)
//   db.raw.<collection>          → raw-db.js (Lesen über compat.state.db / api.query, Schreiben nur business_chats)
//   session                      → Akteur des Kerns (api.actor) als {authenticated, user:{id, name}}
//   getActiveModule              → das fokussierte Fenster des Rahmens (Aufrufer)
//   Anhänge (CHATATT.md)          → desktop_files/desktop_file_chunks als Schreibattrappen in raw-db.js → api.files.put,
//                                  BlobRef in business_chats.attachment_blobs (E30: lesbar für ctox);
//                                  sync.leaseCollection('desktop_file_chunks') als Griff ohne Netz; die Befehlsnutzlast
//                                  trägt statt der desktop_file-Referenz den BlobRef (kind 'blob', ref = sha256)
// Ein Rahmen hat höchstens eine Leiste; ein neuer Rahmen (Doppelpuffer) übernimmt sie im selben Schritt.
import { createV2Compat } from '../workjet/v2-compat.js'; // v4-allow: debt L-42
import { initBusinessChat } from '../../../shared/business-chat.js'; // v4-allow: debt L-42
import { createRawDb } from './raw-db.js';

// Collections aus dem Inventar (CHAT4.md). Schreiben nur die eigenen Chatverläufe.
export const CHAT_COLLECTIONS = Object.freeze([
  'business_chats', 'business_commands', 'ctox_queue_tasks', 'ctox_crew_members', 'ctox_harness_status', 'ctox_harness_events',
]);
export const CHAT_WRITABLE = Object.freeze(['business_chats']);

const CSS_ID = 'v4-chat-v2-css';
let mounted = null;

const V2_ATTACHMENT_KEYS = ['attachments', 'attachment_refs'];

// desktop_file-Referenz (business-chat.js stageChatAttachment) → v4-BlobRef. Ohne file_id/storage_collection, damit kein
// V2-Leser eine desktop_files-Zeile sucht, die es in v4 nicht gibt. Fehlt der Blob, scheitert das Senden.
function blobAttachmentRef(ref, db) {
  if (!ref || typeof ref !== 'object' || ref.kind !== 'desktop_file') return ref;
  const blob = db.blobForClientFile(ref.file_id);
  if (!blob) throw Object.assign(new Error(`Anhang ${ref.name || ref.file_id || ''} wurde nicht in der Dateiablage gespeichert.`), { code: 'unavailable' });
  return {
    kind: 'blob',
    ref: blob.ref,
    blob_id: blob.id,
    attachment_id: ref.attachment_id || '',
    name: ref.name || blob.name,
    mime_type: ref.mime_type || blob.mime,
    size_bytes: blob.size,
    content_hash: blob.ref,
    content_hash_scheme: ref.content_hash_scheme || 'sha256-bytes-v1',
    chat_id: ref.chat_id || '',
    message_id: ref.message_id || '',
    command_id: ref.command_id || '',
    content_state: 'available',
    blob_doc: `business_chats/${ref.chat_id || ''}`,
  };
}

function withBlobAttachments(command, db) {
  const payload = command?.payload && typeof command.payload === 'object' ? command.payload : null;
  if (!payload || !V2_ATTACHMENT_KEYS.some((k) => Array.isArray(payload[k]) && payload[k].length)) return command;
  const nextPayload = { ...payload };
  for (const key of V2_ATTACHMENT_KEYS) if (Array.isArray(payload[key])) nextPayload[key] = payload[key].map((r) => blobAttachmentRef(r, db));
  const ctx = command.client_context && typeof command.client_context === 'object' ? command.client_context : null;
  return {
    ...command,
    payload: nextPayload,
    ...(ctx ? { client_context: { ...ctx, ...(ctx.attachment_storage ? { attachment_storage: 'files' } : {}) } } : {}),
  };
}

function sessionFromActor(compat) {
  const actor = compat.api?.actor || {};
  const id = String(actor.id || compat.state.session?.user_id || '').trim(); // v4-allow: debt L-42
  if (!id) return null;
  return Object.freeze({
    authenticated: true,
    user: Object.freeze({ id, name: String(actor.display_name || id), role: String(actor.role || '') }),
    kernel_instance_id: compat.state.session?.kernel_instance_id ?? null, // v4-allow: debt L-42
  });
}

/**
 * Montiert die V2-Chatleiste. Liefert den Griff oder null (ohne Akteur, Befehls- oder Lesepfad keine Leiste; der
 * Rahmen behält dann seine Crew-Leiste).
 * @param {{kernel: object, shellVer?: string, getActiveModule?: () => ({id: string, title?: string} | null), doc?: Document}} o
 */
export function mountBusinessChat({ kernel, shellVer = 'dev', getActiveModule = () => null, doc = globalThis.document } = {}) {
  mounted?.unmount();
  let compat = null;
  try { compat = createV2Compat({ kernel, shellVer, appId: 'shell' }); } catch { return null; } // v4-allow: debt L-42
  const session = sessionFromActor(compat);
  const db = createRawDb({ compat, collections: CHAT_COLLECTIONS, writable: CHAT_WRITABLE });
  const v2Bus = compat.state.commandBus; // v4-allow: debt L-42
  // V2-Quittung (shared/command-bus.js commandReceipt) trägt task_id/task_status oben; im v4-Kern steht die Antwort des
  // Empfängers (ctox-Handler: {task_id, chat_id, status}) im Befehlsergebnis. Gleiche Felder, kein neuer Wert.
  const commandBus = v2Bus ? { // v4-allow: debt L-42
    async dispatch(command, options) {
      const receipt = await v2Bus.dispatch(withBlobAttachments(command, db), options); // v4-allow: debt L-42
      const result = receipt?.result && typeof receipt.result === 'object' ? receipt.result : {};
      const taskId = String(receipt?.task_id || result.task_id || '').trim();
      return { ...receipt, task_id: taskId, execution_task_id: taskId, task_status: String(result.status || receipt?.status || '') };
    },
  } : null;
  const v2Sync = compat.state.sync; // v4-allow: debt L-42
  // business-chat.js verlangt für Anhänge eine Leihe auf desktop_file_chunks (V2: Stücke replizieren). In v4 sammelt
  // raw-db.js die Stücke im Speicher; die Leihe ist ein Griff ohne Netz, jede andere Collection bleibt unbekannt.
  const sync = v2Sync ? Object.freeze({
    ...v2Sync,
    async leaseCollection(name) { // v4-allow: debt L-42 (V2-Name, von business-chat.js verlangt)
      if (name !== 'desktop_file_chunks' || !db?.raw?.desktop_file_chunks) {
        throw Object.assign(new Error(`business-chat: leaseCollection(${name}) ist in Shell v4 nicht abgebildet`), { code: 'forbidden' });
      }
      return Object.freeze({ collection: name, async release() {} });
    },
  }) : null;
  if (!session || !db || !commandBus || !sync) { compat.dispose(); return null; } // v4-allow: debt L-42
  if (!doc.getElementById(CSS_ID)) {
    const link = doc.createElement('link');
    link.id = CSS_ID;
    link.rel = 'stylesheet';
    link.href = new URL('./chat-v2.css', import.meta.url).href;
    doc.head.appendChild(link); // vor installChatStyles: dieselbe Reihenfolge wie app.css vor der Laufzeitregel in V2
    // v4-Korrekturen (kein V2-Abbild), siehe chat-v4.css; Spezifität über der Laufzeitregel, Reihenfolge egal.
    const fix = doc.createElement('link');
    fix.id = `${CSS_ID}-v4`;
    fix.rel = 'stylesheet';
    fix.href = new URL('./chat-v4.css', import.meta.url).href;
    doc.head.appendChild(fix);
  }
  doc.querySelector('[data-ctox-chat-root]')?.__ctoxChatCleanup?.();
  doc.querySelector('[data-ctox-chat-root]')?.remove();
  initBusinessChat({
    session,
    commandBus, // v4-allow: debt L-42
    db,
    sync,
    getActiveModule: () => {
      try { return getActiveModule() || null; } catch { return null; }
    },
  });
  const root = doc.querySelector('[data-ctox-chat-root]');
  if (!root) { compat.dispose(); return null; }
  // CSS-Kontext der Shell-V2 (chat-v2.css, generiert aus app.css u. a.) gilt in der Hülle; die Leiste steht fest (fixed),
  // die Hülle selbst hat keine Fläche. Gleiche Merkmale wie die Crew-Leiste des Rahmens (data-shell-style="ctox").
  const hull = doc.createElement('div');
  hull.className = 'v4f-chat';
  hull.dataset.v4ChatV2 = '';
  hull.dataset.shellStyle = 'ctox';
  doc.body.appendChild(hull);
  hull.appendChild(root);
  const view = doc.defaultView || globalThis;
  const handle = {
    root,
    compat,
    lastDraft: null,
    get active() { return mounted === handle; },
    // RCH2 (E39 Nr. 4): Entwurf einer App in der Leiste öffnen, nicht senden. Der Rahmen ruft das aus
    // api.shell.navigate('ctox', {chat_draft, command_type, payload, …}) und setzt source_module/source_title aus dem
    // rufenden Fenster. Weg wie V2 openBusinessChat: Ereignis ctox-business-os-chat-open an business-chat.js; der
    // Chat merkt sich command_type/payload (contextMeta) und schickt beim Senden genau diesen Befehl.
    openDraft(spec = {}) {
      if (mounted !== handle) return false;
      const source = String(spec.source_module || '').trim();
      if (!source) return false;
      const payload = spec.payload && typeof spec.payload === 'object' && !Array.isArray(spec.payload) ? spec.payload : null;
      const title = String(spec.title || '').trim();
      const detail = {
        module: source,
        source_module: source,
        source_title: String(spec.source_title || source),
        action: 'context-chat',
        reuseActive: false,
        draft: String(spec.draft ?? ''),
        ...(spec.command_type ? { command_type: String(spec.command_type) } : {}),
        ...(payload ? { payload } : {}),
        ...(payload?.thread_key ? { thread_key: String(payload.thread_key) } : {}),
        ...(spec.record_id ? { record_id: String(spec.record_id) } : {}),
        ...(title ? { title, command_title: title } : {}),
      };
      handle.lastDraft = Object.freeze(JSON.parse(JSON.stringify(detail)));
      view.dispatchEvent(new view.CustomEvent('ctox-business-os-chat-open', { detail }));
      return true;
    },
    unmount() {
      if (mounted !== handle) return;
      mounted = null;
      try { root.__ctoxChatCleanup?.(); } catch (e) { console.warn('[v4 chat] cleanup', e); }
      root.remove();
      hull.remove();
      compat.dispose();
    },
  };
  mounted = handle;
  return handle;
}

export function currentBusinessChat() { return mounted; }
