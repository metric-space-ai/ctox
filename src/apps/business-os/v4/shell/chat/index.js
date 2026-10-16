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
// Anhänge (desktop_file_chunks, sync.leaseCollection) sind nicht abgebildet (CHAT4.md, Fluchtweg).
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
      const receipt = await v2Bus.dispatch(command, options); // v4-allow: debt L-42
      const result = receipt?.result && typeof receipt.result === 'object' ? receipt.result : {};
      const taskId = String(receipt?.task_id || result.task_id || '').trim();
      return { ...receipt, task_id: taskId, execution_task_id: taskId, task_status: String(result.status || receipt?.status || '') };
    },
  } : null;
  const sync = compat.state.sync; // v4-allow: debt L-42
  if (!session || !db || !commandBus || !sync) { compat.dispose(); return null; }
  if (!doc.getElementById(CSS_ID)) {
    const link = doc.createElement('link');
    link.id = CSS_ID;
    link.rel = 'stylesheet';
    link.href = new URL('./chat-v2.css', import.meta.url).href;
    doc.head.appendChild(link); // vor installChatStyles: dieselbe Reihenfolge wie app.css vor der Laufzeitregel in V2
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
  const handle = {
    root,
    compat,
    get active() { return mounted === handle; },
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
