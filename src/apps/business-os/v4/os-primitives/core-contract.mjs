// Origin: CTOX
// License: Apache-2.0
//
// Kernvertrag der App-Capabilities (CAP-1 + CAP-2 + CAP-3) als Daten, für die Aufrufprüfung in
// check-caps.mjs (--calls). Quelle (nur gelesen, git show):
//   CAP-1/2: Klon ~/.local/state/ctox-sync-v4/clones/cap, Zweig v4/cap-2 @ 83d979cf4,
//   src/core/app_capabilities/{registry.rs, schema.rs, caps/*.rs}
//   Doku: ~/.local/state/ctox-sync-v4/docs/befund-cap-1.md, befund-cap-2.md
//   („Parameter-/Ergebnisvertrag für die Ports“).
//   CAP-3: PR #831, Zweig origin/v4/cap-3 @ 4f511562e, src/core/app_capabilities/caps/{cap3.rs (ROUTES, contract()),
//   audit.rs, config.rs, secrets.rs, lock.rs, flow.rs, browser.rs, ingest.rs, coding.rs, mail.rs server_payload}
//   (serde-Structs mit deny_unknown_fields = `allowed`); Doku ~/.local/state/ctox-sync-v4/docs/befund-cap-3.md.
//
// JS-Aufrufpfad = caps/office.rs `call_path(route)`: `capability`, wenn deren letztes
// Segment gleich `function` ist (oder doc.html/query), sonst `capability.function`.
// Argumentform:
//   cap2  – flache Argumente; `wire_args` (caps/office.rs) hüllt sie intern in {payload}.
//           Felder = serde-Struct mit deny_unknown_fields; `fields: '*'` = native
//           Befehls-Payload (Roh-Passwort/Token/Pfad-Felder werden abgewiesen).
//   cap1  – KEINE Hülle durch wire_args: schema.rs FunctionSchema::validate erlaubt nur die
//           Schlüssel {payload, record_id, action}; die native Befehls-Payload steht in `payload`.
//   alias – registry.rs cap1(): ActionAdapter; wie cap1, zusätzlich `action` Pflicht und aus
//           der festen Liste (Legacy-Befehlstypen), kein generischer Aufruf.
// Widerspruch Doku/Code: befund-cap-2.md sagt „Flat JS-Argumente …; der CAP-1-payload-
// Umschlag ist intern“ – im Code gilt das nur für CAP-2-Routen. Für cap1/alias fordert der
// Decoder heute die Hülle. Die Prüfung meldet das als eigene Klasse HUELLE, nicht als
// Fachabweichung, bis der Vertrag entschieden ist.

export const CONTRACT_SOURCE = 'clones/cap v4/cap-2 @ 83d979cf4 + CAP-3 #831 v4/cap-3 @ 4f511562e (src/core/app_capabilities)';
const CAP3 = 'CAP-3 #831';
const CAP3B = 'CAP-3b #844';

const BLOB = { blob: 'blob', blob_id: 'blob' }; // serde alias blob_id -> blob

// §16.32 (Kern-Festlegung 15.10.2026, Strang CAPSIG): BlobRef = files.ref (sha256 des Inhalts).
// Die Primitive nehmen in `blob_id` (bzw. `blob`, office `changes`, Session-Felder) den BlobRef in der
// Form {ref:'<files.ref>'}; CAP-2 stellt von {sha256,size,transfer_id} auf {ref} um, Apps sehen nie eine
// transfer_id. Jeder vom Primitiv erzeugte Blob (office.render, blob.materialize, doc-Ergebnisse, web.fetch)
// erscheint in derselben Transaktion als files-Datensatz mit genau dieser ref und owner_app = aufrufende App
// (über api.files.get abrufbar). Rohdaten (z. B. Base64) legt der Handler vorher über ctx.files.put als
// files-Datensatz an und übergibt dessen ref. Die Aufrufprüfung prüft: steht in einem BlobRef-Feld ein
// Objektliteral, muss es genau den Schlüssel `ref` haben.
export const BLOB_REF_SOURCE = '§16.32';
export const BLOB_REF_KEYS = ['ref'];
export const BLOB_REF_FIELDS = ['blob', 'blob_id', 'changes'];

export const ROUTES = [
  // ---------------- CAP-2 (caps/office.rs ROUTES, caps/mail.rs ROUTES) ----------------
  { path: 'office.edit.prepare', kind: 'cap2', src: 'caps/office.rs:268 Prepare; BlobRef {ref} §16.32',
    allowed: ['kind', 'blob', 'blob_id', 'implemented_features'], required: ['kind', ['blob', 'blob_id']] },
  { path: 'office.edit.apply', kind: 'cap2', src: 'caps/office.rs:277 Apply (session {kind, source:{ref}, editor:{ref}}, changes {ref}, base_version = session.editor.ref; §16.32)',
    allowed: ['session', 'changes', 'base_version', 'implemented_features'], required: ['session', 'changes', 'base_version'] },
  { path: 'office.render', kind: 'cap2', src: 'caps/office.rs:294 Render (to: docx|xlsx|email_html; pdf nativ nicht unterstützt)',
    allowed: ['session', 'to'], required: ['session', 'to'] },
  { path: 'office.settings.save', kind: 'cap2', src: 'caps/office.rs route office.settings/save → ctox.office.settings.save', fields: '*' },
  { path: 'blob.materialize', kind: 'cap2', src: 'caps/blob.rs:104 BlobRequest', allowed: ['blob', 'blob_id'], required: [['blob', 'blob_id']] },
  { path: 'blob.export', kind: 'cap2', src: 'caps/blob.rs:104 BlobRequest', allowed: ['blob', 'blob_id'], required: [['blob', 'blob_id']] },
  { path: 'doc.extract', kind: 'cap2', src: 'caps/doc_extract.rs:5 Extract (kind: pdf|document|spreadsheet|text, ocr: bool; blob_id {ref} §16.32)',
    allowed: ['blob', 'blob_id', 'kind', 'ocr'], required: [['blob', 'blob_id']] },
  { path: 'doc.html', kind: 'cap2', src: 'caps/doc_extract.rs:21 HtmlQuery (blob_id {ref} §16.32)',
    allowed: ['blob', 'blob_id', 'selector', 'attribute', 'limit'], required: [['blob', 'blob_id'], 'selector'] },
  { path: 'web.fetch', kind: 'cap2', src: 'caps/doc_extract.rs:45 Fetch (Ergebnis Text-CAS, blob_id {ref} als files-Datensatz §16.32)', allowed: ['url', 'find'], required: ['url'] },
  { path: 'dataframe.window', kind: 'cap2', src: 'caps/doc_extract.rs:34 Window (filter/sort nur leer)',
    allowed: ['table', 'skip', 'offset', 'limit', 'filter', 'sort'], required: ['table', 'limit'] },
  { path: 'speech.transcribe', kind: 'cap2', src: 'caps/speech.rs:5 Transcribe', allowed: ['blob', 'blob_id', 'model', 'language'], required: [['blob', 'blob_id']] },
  { path: 'speech.synthesize', kind: 'cap2', src: 'caps/speech.rs:14 Synthesize', allowed: ['text', 'voice', 'format'], required: ['text'] },
  { path: 'speech.stt.transcribe', kind: 'cap2', src: 'caps/speech.rs:5 Transcribe (language nativ nicht unterstützt → invalid; blob_id {ref} §16.32)',
    allowed: ['blob', 'blob_id', 'model', 'language'], required: [['blob', 'blob_id']] },
  { path: 'mail.send', kind: 'cap2', src: 'caps/mail.rs:129 SendRequest (Ergebnis {message_id,status})',
    allowed: ['account_id', 'to', 'cc', 'subject', 'text', 'html', 'attachments', 'thread_ref', 'idempotency_key'],
    required: ['account_id', 'to', 'subject', 'idempotency_key'] },
  ...['test', 'sync', 'settings.save', 'pair.start', 'disconnect', 'jami.create', 'jami.export', 'mailbox.link']
    .map((fn) => ({ path: `mail.channel.${fn}`, kind: 'cap2', src: `caps/mail.rs route mail.channel/${fn}`, fields: '*' })),
  ...['runtime.save', 'domain.delete', 'user.delete']
    .map((fn) => ({ path: `mail.server.${fn}`, kind: 'cap2', src: `caps/mail.rs route mail.server/${fn}`, fields: '*' })),
  // CAP-3 #831 (caps/mail.rs server_payload, §16.28): Passwort nur aus dem Secret-Intake des Befehls,
  // DKIM-Privatschlüssel nur null (Core erzeugt), config.get ohne Argumente; Ergebnisse ohne Geheimfelder.
  { path: 'mail.server.config.get', kind: 'flat', src: `${CAP3} caps/mail.rs mail.server/config.get (keine Argumente)`,
    allowed: [], required: [] },
  { path: 'mail.server.user.save', kind: 'flat', src: `${CAP3} caps/mail.rs:483 UserSave (password {secret_intake:true}, command_id = eigener Befehl)`,
    allowed: ['command_id', 'username', 'owner_user_id', 'password', 'communication_account'],
    required: ['command_id', 'username', 'password'] },
  { path: 'mail.server.domain.save', kind: 'flat', src: `${CAP3} caps/mail.rs:520 domain.save (dkim_private_key nur null – Core erzeugt)`,
    fields: '*', values: { dkim_private_key: [null] } },
  { path: 'mail.read', kind: 'cap2', src: 'caps/mail.rs:339 ReadRequest', allowed: ['thread_key', 'limit'], required: ['thread_key'] },
  { path: 'mail.delivery_log.query', kind: 'cap2', src: 'caps/mail.rs:398 DeliveryQuery (Ergebnis {deliveries})',
    allowed: ['message_ids', 'since', 'limit'], required: [] },

  // ---------------- CAP-1 (caps/{task,queue,ticket,crew,coding,turn,schedule,knowledge}.rs) ----------------
  ...['abort_turn', 'block', 'capacity', 'pause', 'release', 'retry', 'task.delete', 'task.update']
    .map((fn) => ({ path: `agent.queue.${fn}`, kind: 'cap1', src: `caps/queue.rs agent.queue/${fn}` })),
  ...['delegate', 'company.research', 'contact_research', 'lead_qualification', 'report_bug', 'request', 'approve']
    .map((fn) => ({ path: `agent.task.${fn}`, kind: 'cap1', src: `caps/task.rs agent.task/${fn}` })),
  ...['local.create', 'approve', 'execute', 'close', 'verify', 'publish_clarification', 'request_clarification',
    'resolve_clarification', 'writeback_comment', 'create_from_conversation']
    .map((fn) => ({ path: `ctox.ticket.${fn}`, kind: 'cap1', src: `caps/ticket.rs ctox.ticket/${fn}` })),
  ...['assign', 'learning.confirm', 'learning.delete', 'learning.update', 'member.create', 'member.update', 'memory.update']
    .map((fn) => ({ path: `agent.crew.${fn}`, kind: 'cap1', src: `caps/crew.rs agent.crew/${fn}` })),
  // CAP-3 #831 (caps/coding.rs flat_envelope): flach; `model` und unbekannte Felder werden abgewiesen.
  { path: 'agent.coding.models', kind: 'flat', src: `${CAP3} caps/coding.rs flat_envelope models({module_id})`,
    allowed: ['module_id'], required: ['module_id'] },
  // CAP-3d (#844 ebddaa2e8, befund-cap-3.md „CAP-3d“ Punkt 4): §16.44-Form; V3-Form (module_id/faux/preset_id),
  // Mischformen und Hülle → invalid; ohne gebundenes #848 → unavailable.
  { path: 'agent.coding.turn', kind: 'flat', src: 'CAP-3d #844 ebddaa2e8 caps/coding.rs turn({app_id, draft_id, base_draft_rev, prompt, model?, turn_id?})',
    allowed: ['app_id', 'draft_id', 'base_draft_rev', 'prompt', 'model', 'turn_id'], required: ['app_id', 'draft_id', 'base_draft_rev', 'prompt'] },
  { path: 'agent.coding.cancel', kind: 'flat', src: 'CAP-3d #844 ebddaa2e8 caps/coding.rs cancel({turn_id})',
    allowed: ['turn_id'], required: ['turn_id'] },
  ...['submit', 'watch', 'cancel', 'input', 'capabilities', 'history', 'route.read.v1', 'route.read.v2',
    'route.capabilities.v1', 'route.capabilities.v2']
    .map((fn) => ({ path: `agent.turn.${fn}`, kind: 'cap1', src: `caps/turn.rs agent.turn/${fn}` })),
  // §16.30 Nr. 2+5: schedule vollständig, flache Argumente (wire_args hüllt intern).
  { path: 'schedule.upsert', kind: 'flat', src: '§16.30 schedule.upsert (caps/schedule.rs, native Payload)', fields: '*' },
  { path: 'schedule.list', kind: 'flat', src: '§16.30 schedule.list({name|thread_key})',
    allowed: ['name', 'thread_key'], required: [['name', 'thread_key']] },
  { path: 'schedule.remove', kind: 'flat', src: '§16.30 schedule.remove({task_id})', allowed: ['task_id'], required: ['task_id'] },
  { path: 'schedule.set_enabled', kind: 'flat', src: '§16.30 schedule.set_enabled({task_id, enabled})',
    allowed: ['task_id', 'enabled'], required: ['task_id', 'enabled'] },
  { path: 'knowledge.query', kind: 'flat', src: '§16.30 knowledge.query (caps/knowledge.rs, native Knowledge-Payload flach)', fields: '*' },

  // ---------------- §16.30 Nr. 2: agent.task als echtes Primitiv (flach) ----------------
  // enqueue({title, instruction, skill?, thread_key?, payload, priority?, idempotency_key?, on_complete?, on_failed?})
  // → {task_id}. on_complete/on_failed = Befehlstyp der EIGENEN App (Callback {task_id, status, result,
  // workspace_refs}, höchstens einmal je task_id). `action`/`record_id` nur für die Legacy-Aktionen (Übergang).
  { path: 'agent.task.enqueue', kind: 'flat', src: '§16.30 agent.task.enqueue (echtes Primitiv)',
    allowed: ['title', 'instruction', 'skill', 'thread_key', 'payload', 'priority', 'idempotency_key', 'on_complete',
      'on_failed', 'action', 'record_id'],
    required: [['instruction', 'action']] },
  { path: 'agent.task.get', kind: 'flat', src: '§16.30 agent.task.get({task_id})', allowed: ['task_id'], required: ['task_id'] },
  { path: 'agent.task.find', kind: 'flat', src: '§16.30 agent.task.find({idempotency_key|command_id})',
    allowed: ['idempotency_key', 'command_id'], required: [['idempotency_key', 'command_id']] },
  { path: 'agent.task.update', kind: 'flat', src: '§16.30 agent.task.update({task_id, …})', fields: '*' },
  { path: 'agent.task.cancel', kind: 'flat', src: '§16.30 agent.task.cancel({task_id, status_note?})',
    allowed: ['task_id', 'status_note'], required: ['task_id'] },
  { path: 'agent.task.set_metadata', kind: 'flat', src: '§16.30 agent.task.set_metadata({task_id, key, value})',
    allowed: ['task_id', 'key', 'value'], required: ['task_id', 'key', 'value'] },
  { path: 'agent.task.workspace.read', kind: 'flat', src: '§16.30 agent.task.workspace.read({task_id, path})',
    allowed: ['task_id', 'path'], required: ['task_id', 'path'] },
  { path: 'agent.queue.task.get', kind: 'flat', src: '§16.30 agent.queue.task.get({task_id})', allowed: ['task_id'], required: ['task_id'] },
  { path: 'agent.queue.task.for_command', kind: 'flat', src: '§16.30 agent.queue.task.for_command({command_id})',
    allowed: ['command_id'], required: ['command_id'] },
  { path: 'agent.queue.task.claim', kind: 'flat', src: '§16.30 agent.queue.task.claim({task_id, holder, route_status_in}) atomar',
    allowed: ['task_id', 'holder', 'route_status_in', 'now'], required: ['task_id', 'holder'] },

  // ---------------- CAP-3 #831 (caps/cap3.rs ROUTES + contract(), flache Argumente) ----------------
  { path: 'audit.event', kind: 'flat', src: `${CAP3} caps/audit.rs:14 Event`,
    allowed: ['collection', 'record_id', 'event_type', 'payload', 'observed_at_ms'], required: ['collection', 'event_type'] },
  { path: 'audit.query', kind: 'flat', src: `${CAP3} caps/audit.rs:25 Query (nur eigene App)`,
    allowed: ['command_type_prefix', 'collection', 'record_id', 'summary', 'order', 'limit'], required: [] },
  { path: 'config.get', kind: 'flat', src: `${CAP3} caps/config.rs:23 Get (Whitelist ALLOWED_KEYS, nur lesend)`,
    allowed: ['key'], required: ['key'],
    values: { key: ['CTO_EMAIL_ADDRESS', 'CTOX_BUSINESS_OS_AUE_REQUIRED_CREDENTIALS',
      'CTOX_BUSINESS_OS_REQUIRE_ENTLEIHER_SIGNATURE', 'CTOX_BUSINESS_OS_REQUIRE_LEGAL_BASIS_EVIDENCE'] } },
  // secrets.rs:26 Args; scope = eigener App-/Nutzer-Scope oder 'credentials' (Instanz, Rolle secrets.manage).
  // secret.put: value nur {secret_intake:true}, command_id = eigener Befehl.
  { path: 'secret.put', kind: 'flat', src: `${CAP3} caps/secrets.rs:26 Args put (value {secret_intake:true})`,
    allowed: ['command_id', 'name', 'value', 'scope', 'user', 'description'], required: ['command_id', 'name', 'value'] },
  { path: 'secret.list', kind: 'flat', src: `${CAP3} caps/secrets.rs:26 Args list → {scope, secrets[]}`,
    allowed: ['scope', 'user'], required: [] },
  ...['delete', 'exists'].map((fn) => ({ path: `secret.${fn}`, kind: 'flat', src: `${CAP3} caps/secrets.rs:26 Args ${fn}`,
    allowed: ['name', 'scope', 'user'], required: ['name'] })),
  { path: 'secrets.exists', kind: 'flat', src: `${CAP3} caps/secrets.rs:26 Args exists (Alias secret.exists)`,
    allowed: ['name', 'scope', 'user'], required: ['name'] },
  { path: 'secret.scope', kind: 'flat', src: `${CAP3} caps/secrets.rs scope({user?}) → {scope}; argumentlos erlaubt`,
    allowed: ['user'], required: [] },
  // lock.rs:18 Claim / :24 Release. Ergebnis claim: {token, fence, expires_at_ms}; belegt → {token:null,…} (nie null).
  ...['lock', 'fence'].flatMap((fam) => [
    { path: `${fam}.claim`, kind: 'flat', src: `${CAP3} caps/lock.rs:18 Claim → {token|null, fence, expires_at_ms}`,
      allowed: ['key', 'ttl_ms'], required: ['key'] },
    { path: `${fam}.release`, kind: 'flat', src: `${CAP3} caps/lock.rs:24 Release (conflict stale_fence)`,
      allowed: ['token'], required: ['token'] },
  ]),
  { path: 'agent.flow.record', kind: 'flat', src: `${CAP3} caps/flow.rs:9 Record (nur Plattform-Apps ctox/ctox-platform)`,
    allowed: ['event_kind', 'title', 'body_text', 'message_key', 'work_id', 'ticket_key', 'attempt_index', 'metadata'],
    required: ['event_kind', 'title'] },
  { path: 'browser.op', kind: 'flat', src: `${CAP3} caps/browser.rs:64 Op (op aus ALLOWED_OPS)`,
    allowed: ['session_id', 'tab_id', 'op', 'params'], required: ['session_id', 'op'],
    values: { op: ['navigate', 'reload', 'back', 'forward', 'tab_open', 'tab_activate', 'tab_close', 'dialog_respond',
      'permission_respond', 'clipboard_copy', 'clipboard_paste', 'clipboard_clear', 'credential_fill', 'nav_state'] } },
  ...['session_stop', 'auth_assist_complete'].map((fn) => ({ path: `browser.${fn}`, kind: 'flat',
    src: `${CAP3} caps/browser.rs:73 Session`, allowed: ['session_id'], required: ['session_id'] })),
  // Generische Seitenerfassung: urls (1..10, absolute http(s)) → {captures:[{url, final_url, status, mime, blob:{ref}, size}]}.
  // Kein query/extract_schema: Personen-Recherche ist App-Logik (E17), kein Core-Primitiv.
  { path: 'browser.capture', kind: 'flat', src: `${CAP3} caps/browser.rs:78 Capture({urls, timeout_ms?})`,
    allowed: ['urls', 'timeout_ms'], required: ['urls'] },
  // §16.38: command Pflicht (Handler im aktiven Manifest), kein deliver/realm.
  { path: 'net.ingest.webhook.register', kind: 'flat', src: `${CAP3} caps/ingest.rs:168 Register (§16.38) → {ingest_url, path, command_type}`,
    allowed: ['path', 'command', 'default_response', 'pass_headers', 'aliases'], required: ['path', 'command'] },
  { path: 'net.ingest.subscribe', kind: 'flat', src: `${CAP3} caps/ingest.rs:179 Subscribe`,
    allowed: ['endpoint', 'protocol', 'enabled', 'config'], required: ['endpoint', 'protocol'] },

  // ---------------- CAP-3b #844 (caps/files.rs, process.rs, directory.rs, browser.rs auth_assist) ----------------
  // Live-Verdrahtung + restliche OS-Primitive auf v4/integration (PR metric-space-ai/ctox#844, Zweig v4/cap-3b
  // @ b877621a6). files.* und Ingest-Zustellung bleiben bis zum Cutover `unavailable`/HTTP 503 (Handler müssen das
  // als Zone-Meldung behandeln, nicht abstürzen). browser.* läuft über den Core-autorisierten Peer-Eingang (§16.46).
  { path: 'files.put', kind: 'flat', src: `${CAP3B} caps/files.rs put (name + genau eins von text|base64; Ergebnis {id, ref}, ref=sha256; owner_app aus ctx)`,
    allowed: ['name', 'mime', 'text', 'base64'], required: ['name'] },
  { path: 'files.get', kind: 'flat', src: `${CAP3B} caps/files.rs get (genau eins von id|ref; text nur UTF-8 ≤512 KiB, nie base64)`,
    allowed: ['id', 'ref'], required: [['id', 'ref']] },
  { path: 'files.list', kind: 'flat', src: `${CAP3B} caps/files.rs list → {files:[files.get ohne text]}`, allowed: ['limit'], required: [] },
  { path: 'files.delete', kind: 'flat', src: `${CAP3B} caps/files.rs delete`, allowed: ['id'], required: ['id'] },
  { path: 'process.run', kind: 'flat', src: `${CAP3B} caps/process.rs run (program=Allowlist-Name business_os.process_allowlist; leeres env + genau env; cwd im App-Workspace; timeout)`,
    allowed: ['program', 'args', 'cwd_ref', 'stdin', 'stdin_ref', 'env', 'timeout_ms'], required: ['program'] },
  { path: 'browser.auth_assist.request', kind: 'flat', src: `${CAP3B} caps/browser.rs AuthAssistRequest (target_url serde-Alias von target_url_override; credential_ref ^ctox-secret://credentials/; Modul/Owner aus ctx)`,
    allowed: ['source_id', 'target_url_override', 'target_url', 'credential_ref', 'login_hint', 'preferred_auth_assist', 'requesting_task_id', 'source_module', 'command_path'],
    required: ['source_id'] },
  { path: 'directory.user', kind: 'flat', src: `${CAP3B} caps/directory.rs user → {found, user_id, display_name?, active}`, allowed: ['user_id'], required: ['user_id'] },
  { path: 'identity.users.list', kind: 'flat', src: `${CAP3B} caps/directory.rs users list → {users:[{user_id, display_name, role, active}]}`,
    allowed: ['active', 'roles', 'order'], required: [] },
  { path: 'policy.module_decision', kind: 'flat', src: `${CAP3B} caps/directory.rs policy (Akteur aus ctx) → {allowed, display_reason, reason_code}`,
    allowed: ['permission', 'module'], required: ['permission', 'module'], values: { permission: ['data.read', 'data.write', 'apps.modify'] } },
  { path: 'command.get', kind: 'flat', src: `${CAP3B} caps/directory.rs command.get (nur eigene App) → {found, command_id, command_type, module, record_id, status, payload}`,
    allowed: ['command_id'], required: ['command_id'] },

  // ---------------- Aktions-Aliase (registry.rs cap1()) ----------------
  { path: 'ctox.ticket.run', kind: 'alias', src: 'registry.rs cap1() ActionAdapter ctox.ticket/run',
    actions: ['local.create', 'approve', 'execute', 'close', 'verify', 'publish_clarification', 'request_clarification',
      'resolve_clarification', 'writeback_comment', 'create_from_conversation'] },
  { path: 'agent.queue.control', kind: 'alias', src: 'registry.rs cap1() ActionAdapter agent.queue/control',
    actions: ['abort_turn', 'block', 'capacity', 'pause', 'release', 'retry'] },
  // agent.task.enqueue: bis §16.30 Alias auf sieben Legacy-Befehle; jetzt echtes Primitiv (siehe oben, kind 'flat').
];

// §16.30 Nr. 5: flache Argumente für alle Capability-Aufrufe (wire_args auch für CAP-1-Routen, Hülle intern).
// Ein flacher Aufruf einer cap1-Route ist damit OK statt HUELLE.
export const FLAT_ARGS_SOURCE = '§16.30';
// §16.30 Nr. 2: entfällt (Projektion ist live) – Aufrufe entfernen; die Prüfung meldet sie als ABWEICHUNG.
export const REMOVED = {
  'agent.queue.projection.refresh': 'entfällt (§16.30: Projektion ist live) – Aufruf entfernen',
  // CAP-3 #831 „Bewusst NICHT gebaut“ (App-Logik, E17):
  'runtime_state.read': 'kein Core-Primitiv (CAP-3 #831, E17: App-Logik) – Zustand in eigener App-Collection; Instanzkonfiguration nur lesend über config.get',
  'runtime_state.write': 'kein Core-Primitiv (CAP-3 #831, E17: App-Logik) – Zustand in eigener App-Collection',
  'web.capture': 'kein Core-Primitiv (CAP-3 #831, E17) – Seitenerfassung generisch über browser.capture({urls})',
  'research.person': 'kein Core-Primitiv (CAP-3 #831, E17: Personen-Recherche ist App-Logik) – agent.task.enqueue bzw. browser.capture({urls})',
};

// CAP-3 #831 „Außerhalb des Auftrags/Katalogs offen“: Pfade, die der Kern weder gebaut noch verworfen hat.
// Sie bleiben OHNE_VERTRAG (informativ) mit diesem Hinweis statt des allgemeinen Texts.
export const OPEN_IN_CORE = {
  // CAP-3b #844 gebaut (caps/browser.rs, files.rs, process.rs, directory.rs) → jetzt echte Routen oben.
  // Vom Kern bewusst NICHT als OS-Primitiv gebaut (CAP-3b #844 "Not built"): App-Logik bzw. anderer Vertrag.
  'model.run': 'CAP-3b #844: kein OS-Primitiv – Modell kommt über die App-Laufzeit ctx.model (ctx.model(req), #830-Bindung), nicht über ctx.cap',
  'agent.command.settle': 'CAP-3b #844: kein OS-Primitiv – gehört zum Befehls-Lebenszyklus (Rückruf-Befehl/Ergebnis), nicht in die Fähigkeitsschicht',
  'pentest.run': 'CAP-3b #844: kein OS-Primitiv – App-Logik auf process.run (Engine über Allowlist-Namen); Port blockiert bis files.*-Cutover (State-Dir-Lesen)',
  'pentest.app_audit': 'CAP-3b #844: kein OS-Primitiv – App-Logik auf process.run (Business-OS-App-Audit über Allowlist-Namen)',
  'pentest.exploit_get': 'CAP-3b #844: kein OS-Primitiv – App-Logik; State-Dir-Datei über files.get (bis Cutover unavailable)', // v4-allow: code
  'pentest.exploit_list': 'CAP-3b #844: kein OS-Primitiv – App-Logik; State-Dir-Index über files.get (bis Cutover unavailable)', // v4-allow: code
  'pentest.exploit_verify': 'CAP-3b #844: kein OS-Primitiv – App-Logik auf process.run + files.get (bis Cutover unavailable)', // v4-allow: code
  // Außerhalb des CAP-3/3b-Auftrags, weder gebaut noch verworfen:
  'browser.automation.run': 'CAP-3b #844: im Kern offen (außerhalb des Auftrags), weder gebaut noch verworfen',
  'artifact.write': 'CAP-3b #844: im Kern offen (außerhalb des Auftrags), weder gebaut noch verworfen',
};

export const ROUTE_BY_PATH = new Map(ROUTES.map((r) => [r.path, r]));

// Familien, die CAP-1/CAP-2/CAP-3 abdecken. Ein Aufruf in einer dieser Familien, dessen Pfad nicht
// im Vertrag steht, ist eine ABWEICHUNG. Aufrufe außerhalb dieser Familien haben (noch)
// keinen Kernvertrag (Klasse OHNE_VERTRAG, informativ).
export const COVERED_FAMILIES = [
  'office.', 'blob.', 'doc.', 'web.', 'dataframe.', 'speech.', 'mail.',
  'agent.queue.', 'agent.task.', 'agent.crew.', 'agent.coding.', 'agent.turn.',
  'ctox.ticket.', 'schedule.', 'knowledge.',
  // CAP-3 #831 (vollständige Familien; browser.* nur pfadweise, weil auth_assist.request/automation.run offen sind)
  'audit.', 'config.', 'secret.', 'secrets.', 'lock.', 'fence.', 'agent.flow.', 'net.ingest.', 'runtime_state.', 'research.',
  // CAP-3b #844: jetzt vollständige Familien (Fehlgriff → ABWEICHUNG statt OHNE_VERTRAG).
  'files.', 'process.', 'directory.', 'identity.users.', 'policy.', 'command.',
];

// Bekannte Umbenennungen Ist-Pfad → Soll-Pfad (aus dem Vergleich Handler ↔ Registry).
export const RENAMES = {
  'office.prepare': 'office.edit.prepare',
  'office.apply': 'office.edit.apply',
  'mail.delivery_log.latest': 'mail.delivery_log.query',
  'mail.server.delete': 'mail.server.domain.delete | mail.server.user.delete (nach kind)',
  'mail.account.upsert': 'mail.channel.mailbox.link (nur Kampagnen-Postfach; sonst kein Vertrag)',
  'agent.crew.member.upsert': 'agent.crew.member.create | agent.crew.member.update',
  'agent.crew.learning.write': 'agent.crew.learning.confirm | .update | .delete (nach op)',
  'agent.queue.task.set_status': 'agent.queue.task.update (Status im payload)',
  'schedule.ensure': 'schedule.upsert',
  'knowledge.command': 'knowledge.query',
  'files.materialize': 'files.get (CAP-3b #844/OLG)',
};
