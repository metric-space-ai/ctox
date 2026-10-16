# Aufruf gegen Kernvertrag: jeder Capability-Aufruf in den Handlern

Stand 15.10.2026. CAPA „Shell v4“. Ergänzung auf Orchestrator-Anweisung: nicht nur Namen,
auch die **Aufrufform** gegen den Kernvertrag prüfen. Die Handler selbst wurden **nicht**
geändert (eigener Strang des Orchestrators).

**Kernvertrag** = `befund-cap-2.md` („Parameter-/Ergebnisvertrag für die Ports“) plus der
tatsächliche Decoder im CAP-Klon (`~/.local/state/ctox-sync-v4/clones/cap`, Zweig
`v4/cap-2` @ 83d979cf4, nur per `git show` gelesen): `registry.rs` (Routen, Aktions-Aliase),
`schema.rs` (CAP-1-Argumentform), `caps/*.rs` (serde-Structs mit `deny_unknown_fields`).
Als Daten in `core-contract.mjs`; Prüfung `node check-caps.mjs --calls`, Tabelle
`node check-caps.mjs --calls-md CALL-CONTRACT.md`.

**Klassen.** ABWEICHUNG = Pfad in einer CAP-1/2-Familie, aber nicht im Vertrag, oder
unbekannte/fehlende Felder, oder Aktions-Alias ohne gültige `action` (Exit 1).
HUELLE = CAP-1-Route mit flachen Argumenten (siehe Widerspruch unten; Exit 1 nur mit
`--strict`). UNGEPRUEFT = Argument nicht statisch auswertbar. OHNE_VERTRAG = Familie hat
noch keinen Kernvertrag (kein CAP-1/2; informativ). OK = Pfad und Form passen.

**Widerspruch Doku/Code (Entscheidung nötig).** `befund-cap-2.md` sagt „Flat JS-Argumente …
der CAP-1-payload-Umschlag ist intern“. Im Code hüllt `wire_args` (caps/office.rs) aber
**nur CAP-2-Routen** in `{payload}`; CAP-1-Routen und die Aliase `ctox.ticket.run`,
`agent.queue.control`, `agent.task.enqueue` prüft `FunctionSchema::validate` (schema.rs)
auf genau die Schlüssel `{payload, record_id, action}`. Flache Aufrufe dieser Familien
scheitern heute mit `invalid $.args`. Entweder `wire_args` auf CAP-1 ausdehnen (dann sind
alle HUELLE-Zeilen OK) oder die Handler hüllen selbst.

**Semantische Lücke im Kern.** `agent.task.enqueue` ist im Kern **kein** generisches
Enqueue, sondern ein Alias auf sieben Legacy-Befehle (`delegate`, `company.research`,
`contact_research`, `lead_qualification`, `report_bug`, `request`, `approve`). Jeder
Handler, der eine eigene Aufgabe einreiht (`{title, instruction, skill, …}`), hat damit
keinen Kernvertrag — das ist der größte Einzelbefund und betrifft das OS-Primitiv
`agent.task` aus `CATALOG.md` direkt.

**Nachtrag CAP-3 (Strang CAP3A, 15.10.2026).** Kernvertrag ergänzt um CAP-3 #831 (Zweig `v4/cap-3`
@ 4f511562e, nur `git show`): `audit.event/query`, `config.get` (Schlüssel-Whitelist), `secret.put/list/delete/exists/scope`,
`secrets.exists`, `lock.*`/`fence.*` (belegt → `{token:null}`), `agent.flow.record` (nur Plattform), `browser.op/
session_stop/auth_assist_complete/capture({urls})`, `net.ingest.webhook.register` (`command` Pflicht, kein `deliver`),
`net.ingest.subscribe`, `agent.coding.models/turn` flach, `mail.server.user.save` (Passwort nur Intake) und
`mail.server.domain.save` (`dkim_private_key` nur `null`). Seit CAP-3 gilt die flache Argumentform für alle Routen
(§16.30 Nr. 5, `cap3.rs wire_args`); der obige Widerspruch und die `agent.task.enqueue`-Lücke sind damit erledigt.
`runtime_state.*`, `web.capture`, `research.person` sind laut Kern bewusst nicht gebaut (App-Logik, E17) und zählen
als ABWEICHUNG; die übrigen OHNE_VERTRAG-Zeilen sind die im Kern offenen Pfade (`OPEN_IN_CORE`). Neu prüft der
Scanner Literalwerte (`values`: `config.get.key`, `browser.op.op`, `mail.server.domain.save.dkim_private_key`) und
argumentlose Aufrufe (`secret.scope()`).

<!-- BEGIN GENERATED (check-caps.mjs --calls-md) -->
_Stand: Klon-HEAD 31d8c6148, Arbeitsbaum zum Zeitpunkt des Laufs (Handler ändern sich laufend; neu erzeugen mit --calls-md). Kernvertrag: clones/cap v4/cap-2 @ 83d979cf4 + CAP-3 #831 v4/cap-3 @ 4f511562e (src/core/app_capabilities). 199 Capability-Aufrufe in `modules/*/handlers` und `customer-modules/*/handlers` (ohne Tests)._

**Zählung:** ABWEICHUNG 0 · HUELLE 0 · UNGEPRUEFT 2 · OK 190 · OHNE_VERTRAG 7

### UNGEPRUEFT (2)

| Datei:Zeile | Ist | Soll |
|---|---|---|
| `modules/ctox/handlers/ctox.task.delete.js:34` | `agent.queue.task.delete(commandId ? { task_id: taskId, command_id: commandId } : { task_id: taskId })` | {payload, record_id?} (caps/queue.rs agent.queue/task.delete) |
| `modules/threads/handlers/_lib.js:450` | `audit.event(event) [cap(ctx,[…])]` | {collection, record_id, event_type, payload, observed_at_ms} (CAP-3 #831 caps/audit.rs:14 Event) |

### OHNE_VERTRAG (7)

| Datei:Zeile | Ist | Soll |
|---|---|---|
| `modules/appsec-pentest/handlers/_lib/args.js:85` | `pentest.run({ args, command_type: command.type, state_dir_ref: payloadString(command.payload ?? {},…)` | CAP-3b #844: kein OS-Primitiv – App-Logik auf process.run (Engine über Allowlist-Namen); Port blockiert bis files.*-Cutover (State-Dir-Lesen) |
| `modules/appsec-pentest/handlers/ctox.appsec.app.audit.js:29` | `pentest.app_audit({ module_id: moduleId, args })` | CAP-3b #844: kein OS-Primitiv – App-Logik auf process.run (Business-OS-App-Audit über Allowlist-Namen) |
| `modules/appsec-pentest/handlers/ctox.appsec.exploit.get.js:17` | `pentest.exploit_get({ name, state_dir_ref: payloadString(p, 'state_dir') ?? null })` | CAP-3b #844: kein OS-Primitiv – App-Logik; State-Dir-Datei über files.get (bis Cutover unavailable) |
| `modules/appsec-pentest/handlers/ctox.appsec.exploit.list.js:12` | `pentest.exploit_list({ state_dir_ref: payloadString(command.payload ?? {}, 'state_dir') ?? null })` | CAP-3b #844: kein OS-Primitiv – App-Logik; State-Dir-Index über files.get (bis Cutover unavailable) |
| `modules/appsec-pentest/handlers/ctox.appsec.exploit.verify.js:26` | `pentest.exploit_verify({ state_dir_ref: payloadString(p, 'state_dir') ?? null, name: name ?? null, expect: exp…)` | CAP-3b #844: kein OS-Primitiv – App-Logik auf process.run + files.get (bis Cutover unavailable) |
| `modules/outbound/handlers/outbound.source.import.js:54` | `browser.automation.run({ source, timeout_ms: 180_000 })` | CAP-3b #844: im Kern offen (außerhalb des Auftrags), weder gebaut noch verworfen |
| `modules/outbound/handlers/outbound.source.import.js:103` | `artifact.write({ command_id: commandId, name: 'outbound_companies.json', json: { command_id: commandId…)` | CAP-3b #844: im Kern offen (außerhalb des Auftrags), weder gebaut noch verworfen |

### OK (190)

| Datei:Zeile | Ist | Soll |
|---|---|---|
| `customer-modules/outbound-lead-generation/handlers/_lib/contact-email-validation.js:407` | `fence.claim({ key: `outbound-research:${recordId}` })` | CAP-3 #831 caps/lock.rs:18 Claim → {token\|null, fence, expires_at_ms} |
| `customer-modules/outbound-lead-generation/handlers/_lib/contact-email-validation.js:433` | `fence.release({ token: fence.token })` | CAP-3 #831 caps/lock.rs:24 Release (conflict stale_fence) |
| `customer-modules/outbound-lead-generation/handlers/_lib/lead-research.js:111` | `agent.task.enqueue({ title: str(payload, ['title']) ?? title, instruction, skill: RESEARCH_SKILL, thread_k…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `customer-modules/outbound-lead-generation/handlers/_lib/lead-research.js:175` | `agent.task.get({ task_id: taskId })` | §16.30 agent.task.get({task_id}) |
| `customer-modules/outbound-lead-generation/handlers/_lib/person-research.js:183` | `agent.task.get({ task_id: taskId })` | §16.30 agent.task.get({task_id}) |
| `customer-modules/outbound-lead-generation/handlers/_lib/person-research.js:185` | `agent.task.cancel({ task_id: taskId, status_note: `Cancelled because a new person research command \`${ty…)` | §16.30 agent.task.cancel({task_id, status_note?}) |
| `customer-modules/outbound-lead-generation/handlers/_lib/person-research.js:257` | `agent.task.find({ idempotency_key: idempotencyKey })` | §16.30 agent.task.find({idempotency_key\|command_id}) |
| `customer-modules/outbound-lead-generation/handlers/_lib/person-research.js:263` | `agent.task.enqueue({ title, instruction: prompt, skill: GAP_CLOSURE_SKILL, thread_key: threadKey, priority…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `customer-modules/outbound-lead-generation/handlers/_lib/person-research.js:271` | `agent.task.set_metadata({ task_id: taskId, key: 'business_os_command_id', value: commandId })` | §16.30 agent.task.set_metadata({task_id, key, value}) |
| `customer-modules/outbound-lead-generation/handlers/_lib/person-research.js:275` | `agent.task.set_metadata({ task_id: taskId, key: GAP_METADATA_KEY, value: finalContract })` | §16.30 agent.task.set_metadata({task_id, key, value}) |
| `customer-modules/outbound-lead-generation/handlers/_lib/person-research.js:278` | `agent.task.update({ task_id: taskId, title, instruction: await buildGapClosurePrompt(ctx, finalContract),…)` | native Payload (§16.30 agent.task.update({task_id, …})) |
| `customer-modules/outbound-lead-generation/handlers/_lib/person-research.js:452` | `fence.claim({ key: `outbound-research:${guardRecordId}`, ttl_ms: FENCE_TTL_MS })` | CAP-3 #831 caps/lock.rs:18 Claim → {token\|null, fence, expires_at_ms} |
| `customer-modules/outbound-lead-generation/handlers/_lib/person-research.js:480` | `agent.task.enqueue({ title: `Personenrecherche: ${request.company}`, instruction, skill: PERSON_RESEARCH_S…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `customer-modules/outbound-lead-generation/handlers/_lib/person-research.js:502` | `fence.release({ token: fence.token })` | CAP-3 #831 caps/lock.rs:24 Release (conflict stale_fence) |
| `customer-modules/outbound-lead-generation/handlers/_lib/person-research.js:510` | `agent.task.get({ task_id: taskId })` | §16.30 agent.task.get({task_id}) |
| `customer-modules/outbound-lead-generation/handlers/_lib/person-research.js:541` | `fence.release({ token })` | CAP-3 #831 caps/lock.rs:24 Release (conflict stale_fence) |
| `customer-modules/outbound-lead-generation/handlers/_lib/policy-report.js:74` | `mail.send({ account_id: sender, to, subject, text: body, html: report.body_html ?? undefined, ide…)` | caps/mail.rs:129 SendRequest (Ergebnis {message_id,status}) |
| `customer-modules/outbound-lead-generation/handlers/_lib/research-source-adapter.js:461` | `agent.task.enqueue({ title: `Outbound Scraper Adapter erzeugen: ${label}`, instruction: prompt, skill: 'un…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `customer-modules/outbound-lead-generation/handlers/_lib/research-source-adapter.js:713` | `secret.scope()` | argumentlos (CAP-3 #831 caps/secrets.rs scope({user?}) → {scope}; argumentlos erlaubt) |
| `customer-modules/outbound-lead-generation/handlers/_lib/research-source-adapter.js:725` | `secret.exists({ scope, name: secretName })` | CAP-3 #831 caps/secrets.rs:26 Args exists |
| `customer-modules/outbound-lead-generation/handlers/_lib/research-source-adapter.js:834` | `browser.auth_assist.request({ source_id: sourceId, target_url_override: url, credential_ref: credentialRef, login_h…)` | CAP-3b #844 caps/browser.rs AuthAssistRequest (target_url serde-Alias von target_url_override; credential_ref ^ctox-secret://credentials/; Modul/Owner aus ctx) |
| `customer-modules/outbound-lead-generation/handlers/_lib/research-source-adapter.js:873` | `agent.task.get({ task_id: taskId })` | §16.30 agent.task.get({task_id}) |
| `customer-modules/outbound-lead-generation/handlers/_lib/research-writeback-sanitize.js:157` | `agent.task.workspace.read({ task_id: taskId, path: parts.join('/') })` | §16.30 agent.task.workspace.read({task_id, path}) |
| `customer-modules/outbound-lead-generation/handlers/_lib/research-writeback.js:85` | `fence.claim({ key: `outbound-research:${request.record_id}` })` | CAP-3 #831 caps/lock.rs:18 Claim → {token\|null, fence, expires_at_ms} |
| `customer-modules/outbound-lead-generation/handlers/_lib/research-writeback.js:90` | `fence.release({ token: fence.token })` | CAP-3 #831 caps/lock.rs:24 Release (conflict stale_fence) |
| `customer-modules/outbound-lead-generation/handlers/_lib/research-writeback.js:98` | `command.get({ command_id: request.research_command_id })` | CAP-3b #844 caps/directory.rs command.get (nur eigene App) → {found, command_id, command_type, module, record_id, status, payload} |
| `customer-modules/outbound-lead-generation/handlers/_lib/research-writeback.js:109` | `agent.task.get({ task_id: request.gap_task_id })` | §16.30 agent.task.get({task_id}) |
| `customer-modules/outbound-lead-generation/handlers/_lib/research-writeback.js:162` | `agent.task.find({ command_id: request.research_command_id })` | §16.30 agent.task.find({idempotency_key\|command_id}) |
| `customer-modules/outbound-lead-generation/handlers/_lib/scrape-registry.js:49` | `files.get({ ref })` | CAP-3b #844 caps/files.rs get (genau eins von id\|ref; text nur UTF-8 ≤512 KiB, nie base64) |
| `customer-modules/outbound-lead-generation/handlers/_lib/scrape-registry.js:67` | `files.put({ name, mime: meta?.content_type ?? 'application/octet-stream', text })` | CAP-3b #844 caps/files.rs put (name + genau eins von text\|base64; Ergebnis {id, ref}, ref=sha256; owner_app aus ctx) |
| `customer-modules/outbound-lead-generation/handlers/_lib/scrape-runner.js:265` | `process.run({ program, args, cwd_ref: `${target.workspace_dir}/runs/${runId}`, stdin: registered.bo…)` | CAP-3b #844 caps/process.rs run (program=Allowlist-Name business_os.process_allowlist; leeres env + genau env; cwd im App-Workspace; timeout) |
| `customer-modules/outbound-lead-generation/handlers/_lib/sellify-remarks.js:73` | `agent.task.enqueue({ title, instruction, thread_key: `business-os/outbound-lead-generation/sperrvermerk/${…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `customer-modules/outbound-lead-generation/handlers/_lib/sellify-remarks.js:95` | `agent.task.get({ task_id: taskId })` | §16.30 agent.task.get({task_id}) |
| `customer-modules/outbound-lead-generation/handlers/outbound.contacts.email_validate.js:12` | `fence.claim({ key: `email-validation:${recordId}` })` | CAP-3 #831 caps/lock.rs:18 Claim → {token\|null, fence, expires_at_ms} |
| `customer-modules/outbound-lead-generation/handlers/outbound.contacts.email_validate.js:19` | `fence.release({ token: fence.token })` | CAP-3 #831 caps/lock.rs:24 Release (conflict stale_fence) |
| `customer-modules/outbound-lead-generation/handlers/outbound.research.adapters.reconcile.done.js:37` | `agent.task.get({ task_id: taskId })` | §16.30 agent.task.get({task_id}) |
| `customer-modules/outbound-lead-generation/handlers/outbound.research.adapters.reconcile.js:309` | `secret.scope()` | argumentlos (CAP-3 #831 caps/secrets.rs scope({user?}) → {scope}; argumentlos erlaubt) |
| `customer-modules/outbound-lead-generation/handlers/outbound.research.adapters.reconcile.js:310` | `secret.exists({ scope, name: secretName })` | CAP-3 #831 caps/secrets.rs:26 Args exists |
| `customer-modules/outbound-lead-generation/handlers/outbound.research.adapters.reconcile.js:422` | `agent.task.enqueue({ instruction: typeof payload.instruction === 'string' ? payload.instruction : '', titl…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `customer-modules/outbound-lead-generation/handlers/outbound.update_digest.send_now.js:65` | `config.get({ key: 'CTO_EMAIL_ADDRESS' })` | CAP-3 #831 caps/config.rs:23 Get (Whitelist ALLOWED_KEYS, nur lesend) |
| `modules/appsec-pentest/handlers/ctox.appsec.finding.remediate.js:63` | `agent.task.enqueue({ title: `${severity}: ${findingTitle}`, instruction, skill: 'appsec-pentest/appsec-pen…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `modules/appsec-pentest/handlers/ctox.appsec.finding.remediate.js:107` | `agent.task.get({ task_id: taskId })` | §16.30 agent.task.get({task_id}) |
| `modules/browser/handlers/_lib/run.js:40` | `browser.op({ session_id: id, tab_id: tab \|\| null, op, params })` | CAP-3 #831 caps/browser.rs:64 Op (op aus ALLOWED_OPS) |
| `modules/browser/handlers/browser.clipboard.clear.js:13` | `browser.op({ session_id: id, tab_id: tabId(payload) \|\| null, op: 'clipboard_clear', params: {} })` | CAP-3 #831 caps/browser.rs:64 Op (op aus ALLOWED_OPS) |
| `modules/browser/handlers/browser.clipboard.paste.js:15` | `browser.op({ session_id: id, tab_id: tabId(payload) \|\| null, op: 'clipboard_paste', params: {} })` | CAP-3 #831 caps/browser.rs:64 Op (op aus ALLOWED_OPS) |
| `modules/browser/handlers/browser.credential.fill.js:16` | `browser.op({ session_id: id, tab_id: tabId(payload) \|\| null, op: 'credential_fill', params: { cred…)` | CAP-3 #831 caps/browser.rs:64 Op (op aus ALLOWED_OPS) |
| `modules/browser/handlers/browser.session.stop.js:9` | `browser.session_stop({ session_id: id })` | CAP-3 #831 caps/browser.rs:73 Session |
| `modules/browser/handlers/web_stack.auth_assist.complete.js:15` | `browser.auth_assist_complete({ session_id: id })` | CAP-3 #831 caps/browser.rs:73 Session |
| `modules/coding-agents/handlers/ctox.coding.models.js:12` | `agent.coding.models({ module_id: moduleId }) [callCap]` | CAP-3 #831 caps/coding.rs flat_envelope models({module_id}) |
| `modules/coding-agents/handlers/ctox.coding.turn.js:16` | `agent.coding.turn({ module_id: moduleId, prompt: typeof payload.prompt === 'string' ? payload.prompt : ''…) [callCap]` | CAP-3 #831 caps/coding.rs flat_envelope turn({module_id, prompt?, faux?, preset_id?}) |
| `modules/consent/handlers/_ats.js:232` | `audit.event({ collection: 'business_commands', record_id: typeof command?.record_id === 'string' ? …)` | CAP-3 #831 caps/audit.rs:14 Event |
| `modules/consent/handlers/_ats.js:254` | `config.get({ key })` | CAP-3 #831 caps/config.rs:23 Get (Whitelist ALLOWED_KEYS, nur lesend) |
| `modules/consent/handlers/ats.subject.export.js:25` | `audit.query({ command_type_prefix: 'business_os.ats.', summary: { candidate_id: subjectId }, order:…)` | CAP-3 #831 caps/audit.rs:25 Query (nur eigene App) |
| `modules/credentials/handlers/ctox.secret.delete.js:10` | `secret.delete({ name, ...scopeArgs(name) }) [callCap]` | CAP-3 #831 caps/secrets.rs:26 Args delete |
| `modules/credentials/handlers/ctox.secret.list.js:14` | `secret.list({ scope: INSTANCE_SCOPE }) [callCap]` | CAP-3 #831 caps/secrets.rs:26 Args list → {scope, secrets[]} |
| `modules/credentials/handlers/ctox.secret.list.js:15` | `secret.list({}) [callCap]` | CAP-3 #831 caps/secrets.rs:26 Args list → {scope, secrets[]} |
| `modules/credentials/handlers/ctox.secret.put.js:14` | `secret.put({ command_id: command.id, name, value: { secret_intake: true }, ...scopeArgs(name) }) [callCap]` | CAP-3 #831 caps/secrets.rs:26 Args put (value {secret_intake:true}) |
| `modules/ctox/handlers/_lib/cockpit.js:43` | `agent.flow.record({ event_kind: 'cockpit.control', title: command.type, message_key: taskId \|\| null, meta…)` | CAP-3 #831 caps/flow.rs:9 Record (nur Plattform-Apps ctox/ctox-platform) |
| `modules/ctox/handlers/_lib/cockpit.js:67` | `agent.flow.record({ event_kind: 'cockpit.control', title: command.type, message_key: taskId \|\| null, meta…)` | CAP-3 #831 caps/flow.rs:9 Record (nur Plattform-Apps ctox/ctox-platform) |
| `modules/ctox/handlers/_lib/cockpit.js:83` | `agent.queue.task.get({ task_id: taskId })` | §16.30 agent.queue.task.get({task_id}) |
| `modules/ctox/handlers/_lib/cockpit.js:96` | `agent.queue.control({ action: block ? 'block' : retry ? 'retry' : 'release', task_id: taskId, value: { rout…)` | registry.rs cap1() ActionAdapter agent.queue/control; flach (§16.30) |
| `modules/ctox/handlers/_lib/context-chat.js:175` | `agent.task.enqueue({ title, instruction: instruction.replace(/\s+$/, ''), skill: appModify ? APP_MODULE_SK…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `modules/ctox/handlers/_lib/context-chat.js:202` | `agent.task.get({ task_id: taskId })` | §16.30 agent.task.get({task_id}) |
| `modules/ctox/handlers/_lib/crew.js:191` | `agent.flow.record({ event_kind: 'crew.control', title: command.type, metadata: { actor, command_type: com…)` | CAP-3 #831 caps/flow.rs:9 Record (nur Plattform-Apps ctox/ctox-platform) |
| `modules/ctox/handlers/_lib/crew.js:203` | `agent.flow.record({ event_kind: 'crew.control', title: command.type, metadata: { actor, command_type: com…)` | CAP-3 #831 caps/flow.rs:9 Record (nur Plattform-Apps ctox/ctox-platform) |
| `modules/ctox/handlers/_lib/tasks.js:14` | `agent.queue.task.get({ task_id: taskId })` | §16.30 agent.queue.task.get({task_id}) |
| `modules/ctox/handlers/_lib/tasks.js:15` | `agent.queue.task.for_command({ command_id: taskId })` | §16.30 agent.queue.task.for_command({command_id}) |
| `modules/ctox/handlers/ctox.crew.assign.js:21` | `agent.queue.task.claim({ task_id: task, holder: member, route_status_in: ASSIGNABLE_ROUTE_STATUSES, })` | §16.30 agent.queue.task.claim({task_id, holder, route_status_in}) atomar |
| `modules/ctox/handlers/ctox.queue.capacity.js:13` | `agent.queue.control({ action: 'capacity', value: workers })` | registry.rs cap1() ActionAdapter agent.queue/control; flach (§16.30) |
| `modules/ctox/handlers/ctox.queue.pause.js:12` | `agent.queue.control({ action: 'pause', value })` | registry.rs cap1() ActionAdapter agent.queue/control; flach (§16.30) |
| `modules/ctox/handlers/ctox.task.delete.js:24` | `agent.queue.task.get({ task_id: taskId })` | §16.30 agent.queue.task.get({task_id}) |
| `modules/ctox/handlers/ctox.task.delete.js:30` | `agent.queue.task.update({ task_id: taskId, patch: { route_status: 'cancelled', status_note: reason } })` | caps/queue.rs agent.queue/task.update; flach (§16.30) |
| `modules/ctox/handlers/ctox.task.update.js:26` | `agent.queue.task.update({ task_id: taskId, patch: { title, prompt, priority, status_note: `business-os: edited …)` | caps/queue.rs agent.queue/task.update; flach (§16.30) |
| `modules/cv-print-builder/handlers/ctox.cv_print.apply_parse.failed.js:18` | `agent.task.get({ task_id: taskId }) [callCap]` | §16.30 agent.task.get({task_id}) |
| `modules/cv-print-builder/handlers/ctox.cv_print.apply_parse.js:124` | `doc.extract({ blob_id: { ref: blobRef }, kind: 'pdf' }) [callCap]` | caps/doc_extract.rs:5 Extract (kind: pdf\|document\|spreadsheet\|text, ocr: bool; blob_id {ref} §16.32) |
| `modules/cv-print-builder/handlers/ctox.cv_print.apply_parse.js:174` | `agent.task.enqueue({ title: firstStringField(payload, ['title']) ?? 'CV strukturieren', instruction: await…) [callCap]` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `modules/documents/handlers/_lib/automation.js:72` | `agent.task.get({ task_id: taskId })` | §16.30 agent.task.get({task_id}) |
| `modules/documents/handlers/_lib/document-runbook.js:44` | `agent.task.enqueue({ title: `${DOCUMENT_RUNBOOKS[runbookId]}: ${text(doc.title) \|\| documentId}`, instructi…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `modules/documents/handlers/_lib/office.js:169` | `office.edit.prepare({ kind: binding.kind, blob_id: { ref: sourceRef }, implemented_features: implementedFea…)` | caps/office.rs:268 Prepare; BlobRef {ref} §16.32 |
| `modules/documents/handlers/_lib/office.js:212` | `office.edit.apply({ session, changes: { ref: changesRef }, base_version: refOf(session.editor), implement…)` | caps/office.rs:277 Apply (session {kind, source:{ref}, editor:{ref}}, changes {ref}, base_version = session.editor.ref; §16.32) |
| `modules/documents/handlers/_lib/report-writeback.js:78` | `office.edit.prepare({ kind: 'document', blob_id: { ref: docxRef }, implemented_features: DOCUMENT_FEATURES, })` | caps/office.rs:268 Prepare; BlobRef {ref} §16.32 |
| `modules/documents/handlers/_lib/report.js:187` | `dataframe.window({ table: tableId, skip, limit: WINDOW_LIMIT })` | caps/doc_extract.rs:34 Window (filter/sort nur leer) |
| `modules/documents/handlers/ctox.office.settings.save.js:9` | `office.settings.save(command.payload ?? {})` | native Payload (caps/office.rs route office.settings/save → ctox.office.settings.save) |
| `modules/documents/handlers/office.document.export.js:28` | `office.render({ session: prepared.session, to })` | caps/office.rs:294 Render (to: docx\|xlsx\|email_html; pdf nativ nicht unterstützt) |
| `modules/documents/handlers/office.document.freeze_email_content.js:31` | `office.render({ session: prepared.session, to: 'email_html' })` | caps/office.rs:294 Render (to: docx\|xlsx\|email_html; pdf nativ nicht unterstützt) |
| `modules/documents/handlers/research.systematic.report.complete.js:28` | `agent.task.workspace.read({ task_id: taskId, path })` | §16.30 agent.task.workspace.read({task_id, path}) |
| `modules/documents/handlers/research.systematic.report.create.js:57` | `agent.task.enqueue({ title: `Word-Dokument: ${title}`, instruction, skill: REPORT_SKILL, thread_key: 'busi…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `modules/esign/handlers/_ats.js:232` | `audit.event({ collection: 'business_commands', record_id: typeof command?.record_id === 'string' ? …)` | CAP-3 #831 caps/audit.rs:14 Event |
| `modules/esign/handlers/_ats.js:254` | `config.get({ key })` | CAP-3 #831 caps/config.rs:23 Get (Whitelist ALLOWED_KEYS, nur lesend) |
| `modules/intake/handlers/_ats.js:232` | `audit.event({ collection: 'business_commands', record_id: typeof command?.record_id === 'string' ? …)` | CAP-3 #831 caps/audit.rs:14 Event |
| `modules/intake/handlers/_ats.js:254` | `config.get({ key })` | CAP-3 #831 caps/config.rs:23 Get (Whitelist ALLOWED_KEYS, nur lesend) |
| `modules/interviews/handlers/_ats.js:232` | `audit.event({ collection: 'business_commands', record_id: typeof command?.record_id === 'string' ? …)` | CAP-3 #831 caps/audit.rs:14 Event |
| `modules/interviews/handlers/_ats.js:254` | `config.get({ key })` | CAP-3 #831 caps/config.rs:23 Get (Whitelist ALLOWED_KEYS, nur lesend) |
| `modules/interviews/handlers/ats.interview.transcribe.js:41` | `speech.stt.transcribe({ blob_id: { ref }, model })` | caps/speech.rs:5 Transcribe (language nativ nicht unterstützt → invalid; blob_id {ref} §16.32) |
| `modules/iot/handlers/_agenttask.js:67` | `agent.task.get({ task_id: taskId })` | §16.30 agent.task.get({task_id}) |
| `modules/iot/handlers/_iotcore.js:673` | `agent.task.enqueue({ title: `IoT: ${rule.name ?? ruleset.name} on ${asset.id}`, instruction: onMatch.task_…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `modules/iot/handlers/_timeseries.js:21` | `knowledge.query({ argv: ['data', ...argv] })` | native Payload (§16.30 knowledge.query (caps/knowledge.rs, native Knowledge-Payload flach)) |
| `modules/iot/handlers/ctox.iot.agent.configure.js:24` | `net.ingest.subscribe({ endpoint: id, protocol: req.kind, enabled: req.enabled, config: data })` | CAP-3 #831 caps/ingest.rs:179 Subscribe |
| `modules/iot/handlers/ctox.iot.signal.evaluate.js:50` | `agent.task.enqueue({ title: `IoT-Auftrag ausgelöst: ${signalRef}`, instruction: seedPrompt(widget.action_p…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `modules/iot/handlers/ctox.iot.webhook.register.js:43` | `net.ingest.webhook.register({ path: INGEST_PATH, command: INGEST_COMMAND, pass_headers: [...PASS_HEADERS], default_…)` | CAP-3 #831 caps/ingest.rs:168 Register (§16.38) → {ingest_url, path, command_type} |
| `modules/knowledge/handlers/_lib/agent.js:111` | `agent.task.enqueue({ title, instruction, skill: KNOWLEDGE_SKILL, thread_key: text(p.thread_key) \|\| DEFAULT…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `modules/knowledge/handlers/_lib/callback.js:31` | `agent.task.get({ task_id: taskId })` | §16.30 agent.task.get({task_id}) |
| `modules/knowledge/handlers/_lib/callback.js:50` | `agent.task.workspace.read({ task_id: taskId, path })` | §16.30 agent.task.workspace.read({task_id, path}) |
| `modules/knowledge/handlers/knowledge.view.window.js:118` | `dataframe.window({ table, skip: view.skip, limit: view.limit })` | caps/doc_extract.rs:34 Window (filter/sort nur leer) |
| `modules/knowledge/handlers/knowledge.view.window.js:123` | `dataframe.window({ table, skip, limit: CORE_PAGE })` | caps/doc_extract.rs:34 Window (filter/sort nur leer) |
| `modules/kundenpipeline/handlers/_lib/hub.js:187` | `identity.users.list({ active: true, roles: ['chef', 'admin'], order: ['created_at_ms:asc', 'user_id:asc'], })` | CAP-3b #844 caps/directory.rs users list → {users:[{user_id, display_name, role, active}]} |
| `modules/kundenpipeline/handlers/kundenpipeline.auftrag.complete.js:18` | `agent.task.get({ task_id: payload.task_id })` | §16.30 agent.task.get({task_id}) |
| `modules/kundenpipeline/handlers/kundenpipeline.delegate.js:35` | `agent.task.enqueue({ title: `Kundenauftrag: ${kurz(titel, 60)}`, instruction: instruction.replace(/\n$/, '…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `modules/kundenpipeline/handlers/kundenpipeline.mail.inbound.js:35` | `agent.task.enqueue({ title: `Triage: ${kurz(betreff, 60)}`, instruction: instruction.replace(/\n$/, ''), s…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `modules/kundenpipeline/handlers/kundenpipeline.mail.send.js:31` | `mail.server.config.get()` | argumentlos (CAP-3 #831 caps/mail.rs mail.server/config.get (keine Argumente)) |
| `modules/kundenpipeline/handlers/kundenpipeline.mail.send.js:42` | `mail.send({ account_id: from, to: [to], subject, text: body, idempotency_key: `${command.id}:kund…)` | caps/mail.rs:129 SendRequest (Ergebnis {message_id,status}) |
| `modules/kundenpipeline/handlers/kundenpipeline.triage.failed.js:10` | `agent.task.get({ task_id: payload.task_id })` | §16.30 agent.task.get({task_id}) |
| `modules/kundenpipeline/handlers/kundenpipeline.triage.write.js:34` | `agent.task.get({ task_id: payload.task_id })` | §16.30 agent.task.get({task_id}) |
| `modules/mail/handlers/ctox.mailserver.delete_domain.js:12` | `mail.server.domain.delete({ domain_name: domainName }) [callCap]` | native Payload (caps/mail.rs route mail.server/domain.delete) |
| `modules/mail/handlers/ctox.mailserver.delete_user.js:15` | `mail.server.user.delete({ username }) [callCap]` | native Payload (caps/mail.rs route mail.server/user.delete) |
| `modules/mail/handlers/ctox.mailserver.get_config.js:11` | `mail.server.config.get({}) [callCap]` | CAP-3 #831 caps/mail.rs mail.server/config.get (keine Argumente) |
| `modules/mail/handlers/ctox.mailserver.save_domain.js:32` | `mail.server.domain.save({ ...domain, dkim_private_key: null, }) [callCap]` | native Payload (CAP-3 #831 caps/mail.rs:520 domain.save (dkim_private_key nur null – Core erzeugt)) |
| `modules/mail/handlers/ctox.mailserver.save_runtime.js:39` | `mail.server.config.get({}) [callCap]` | CAP-3 #831 caps/mail.rs mail.server/config.get (keine Argumente) |
| `modules/mail/handlers/ctox.mailserver.save_runtime.js:44` | `mail.server.runtime.save({ settings }) [callCap]` | native Payload (caps/mail.rs route mail.server/runtime.save) |
| `modules/mail/handlers/ctox.mailserver.save_user.js:24` | `mail.server.user.save({ command_id: command.id, password: { secret_intake: true }, username: user.username, o…) [callCap]` | CAP-3 #831 caps/mail.rs:483 UserSave (password {secret_intake:true}, command_id = eigener Befehl) |
| `modules/mail/handlers/mail.content.freeze.js:40` | `blob.materialize({ blob_id: ref }) [callCap]` | caps/blob.rs:104 BlobRequest |
| `modules/mail/handlers/mail.content.freeze.js:55` | `office.edit.prepare({ kind: 'document', blob_id: source }) [callCap]` | caps/office.rs:268 Prepare; BlobRef {ref} §16.32 |
| `modules/mail/handlers/mail.content.freeze.js:62` | `office.render({ session: { kind: 'document', source: canonical, editor }, to: 'email_html', }) [callCap]` | caps/office.rs:294 Render (to: docx\|xlsx\|email_html; pdf nativ nicht unterstützt) |
| `modules/matching/handlers/_lib/job.js:260` | `doc.html({ blob_id: { ref }, selector: q.selector })` | caps/doc_extract.rs:21 HtmlQuery (blob_id {ref} §16.32) |
| `modules/matching/handlers/_lib/job.js:263` | `doc.html({ blob_id: { ref }, selector: q.selector, attribute: q.attr, limit: 1 })` | caps/doc_extract.rs:21 HtmlQuery (blob_id {ref} §16.32) |
| `modules/matching/handlers/_lib/job.js:265` | `doc.html({ blob_id: { ref }, selector: q.selector, attribute: 'content', limit: 1 })` | caps/doc_extract.rs:21 HtmlQuery (blob_id {ref} §16.32) |
| `modules/matching/handlers/_lib/job.js:268` | `doc.html({ blob_id: { ref }, selector: q.selector, limit: 1 })` | caps/doc_extract.rs:21 HtmlQuery (blob_id {ref} §16.32) |
| `modules/matching/handlers/_lib/job.js:272` | `doc.html({ blob_id: { ref }, selector: q.selector, limit: 1 })` | caps/doc_extract.rs:21 HtmlQuery (blob_id {ref} §16.32) |
| `modules/matching/handlers/matching.source.parse_object.js:64` | `doc.extract({ blob_id: { ref }, kind: extractKind(name, mime), ocr: true })` | caps/doc_extract.rs:5 Extract (kind: pdf\|document\|spreadsheet\|text, ocr: bool; blob_id {ref} §16.32) |
| `modules/matching/handlers/matching.source.parse_requirement.js:32` | `web.fetch({ url })` | caps/doc_extract.rs:45 Fetch (Ergebnis Text-CAS, blob_id {ref} als files-Datensatz §16.32) |
| `modules/matching/handlers/matching.source.parse_requirement.js:49` | `doc.html({ blob_id: { ref: fragmentRef }, selector: 'body', limit: 1 })` | caps/doc_extract.rs:21 HtmlQuery (blob_id {ref} §16.32) |
| `modules/nachweise/handlers/_ats.js:232` | `audit.event({ collection: 'business_commands', record_id: typeof command?.record_id === 'string' ? …)` | CAP-3 #831 caps/audit.rs:14 Event |
| `modules/nachweise/handlers/_ats.js:254` | `config.get({ key })` | CAP-3 #831 caps/config.rs:23 Get (Whitelist ALLOWED_KEYS, nur lesend) |
| `modules/news/handlers/_lib/news.js:366` | `agent.task.enqueue({ title: taskTitle(command), instruction, skill: NEWS_APP_SKILL, thread_key: str(payloa…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `modules/news/handlers/_lib/news.js:400` | `agent.task.enqueue({ title: taskTitle(command), instruction, skill: authSource ? NEWS_APP_SKILL : SCRAPE_S…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `modules/news/handlers/_lib/news.js:437` | `agent.task.get({ task_id: taskId })` | §16.30 agent.task.get({task_id}) |
| `modules/news/handlers/news.source.auth_assist.js:32` | `browser.auth_assist.request({ source_id: authSource, target_url_override: null, credential_ref: null, login_hint: n…)` | CAP-3b #844 caps/browser.rs AuthAssistRequest (target_url serde-Alias von target_url_override; credential_ref ^ctox-secret://credentials/; Modul/Owner aus ctx) |
| `modules/news/handlers/news.topic.schedule.js:45` | `schedule.list({ name, thread_key: threadKey })` | §16.30 schedule.list({name\|thread_key}) |
| `modules/news/handlers/news.topic.schedule.js:50` | `schedule.remove({ task_id: existing.task_id })` | §16.30 schedule.remove({task_id}) |
| `modules/news/handlers/news.topic.schedule.js:51` | `schedule.set_enabled({ task_id: existing.task_id, enabled: false })` | §16.30 schedule.set_enabled({task_id, enabled}) |
| `modules/news/handlers/news.topic.schedule.js:75` | `schedule.upsert({ name, cron_expr: cronExpr.trim(), prompt: `${ROUTE_MARKER}${JSON.stringify(route)}`, …)` | native Payload (§16.30 schedule.upsert (caps/schedule.rs, native Payload)) |
| `modules/news/handlers/news.topic.schedule.js:86` | `schedule.set_enabled({ task_id: task?.task_id, enabled: true })` | §16.30 schedule.set_enabled({task_id, enabled}) |
| `modules/outbound/handlers/_lib/agent-callback.js:19` | `agent.task.get({ task_id: taskId })` | §16.30 agent.task.get({task_id}) |
| `modules/outbound/handlers/_lib/knowledge.js:83` | `knowledge.query({ argv })` | native Payload (§16.30 knowledge.query (caps/knowledge.rs, native Knowledge-Payload flach)) |
| `modules/outbound/handlers/_lib/olg-registry.js:157` | `files.get({ ref })` | CAP-3b #844 caps/files.rs get (genau eins von id\|ref; text nur UTF-8 ≤512 KiB, nie base64) |
| `modules/outbound/handlers/_lib/outbound.js:325` | `audit.event({ collection: 'outbound_approvals', record_id: approvalId, event_type: 'business_os.ext…)` | CAP-3 #831 caps/audit.rs:14 Event |
| `modules/outbound/handlers/_lib/reconcile.js:44` | `mail.delivery_log.query({ message_ids: ids })` | caps/mail.rs:398 DeliveryQuery (Ergebnis {deliveries}) |
| `modules/outbound/handlers/_lib/research-adapter.js:44` | `secrets.exists({ scope: CREDENTIAL_SCOPE, name: secretName })` | CAP-3 #831 caps/secrets.rs:26 Args exists (Alias secret.exists) |
| `modules/outbound/handlers/_lib/research-adapter.js:140` | `browser.auth_assist.request({ source_id: sourceId, target_url: url, credential_ref: secretName ? `ctox-secret://${C…)` | CAP-3b #844 caps/browser.rs AuthAssistRequest (target_url serde-Alias von target_url_override; credential_ref ^ctox-secret://credentials/; Modul/Owner aus ctx) |
| `modules/outbound/handlers/_lib/research-task.js:50` | `agent.task.enqueue({ title, instruction: researchInstruction(request), skill: RESEARCH_SKILL, thread_key: …)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `modules/outbound/handlers/_lib/research-task.js:70` | `agent.task.get({ task_id: taskId })` | §16.30 agent.task.get({task_id}) |
| `modules/outbound/handlers/_lib/scrape-generation.js:145` | `agent.task.enqueue({ title: `Outbound Scraper Adapter erzeugen: ${label}`, instruction: prompt, skill: 'un…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `modules/outbound/handlers/_lib/tracking.js:90` | `net.ingest.webhook.register({ path: TRACKING.ingestPath, command: TRACKING.eventCommand, default_response: pixelRes…)` | CAP-3 #831 caps/ingest.rs:168 Register (§16.38) → {ingest_url, path, command_type} |
| `modules/outbound/handlers/outbound.campaign.mailbox.link.js:34` | `mail.channel.mailbox.link({ account_key: accountKey, channel, address: mailboxAddress, provider, profile })` | native Payload (caps/mail.rs route mail.channel/mailbox.link) |
| `modules/outbound/handlers/outbound.campaign.setup.request.js:38` | `agent.task.enqueue({ title: `${TITLE} · ${name}`, instruction: instruction.replace(/\n$/, ''), skill: SETU…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `modules/outbound/handlers/outbound.message.send_approved.js:88` | `mail.send({ account_id: senderAccountId, to: [recipient], subject: str(message, ['subject']) ?? '…)` | caps/mail.rs:129 SendRequest (Ergebnis {message_id,status}) |
| `modules/outbound/handlers/outbound.pipeline.outreach_draft.js:45` | `agent.task.enqueue({ title: `Anschreiben generieren: ${companyName}`, instruction: instruction.replace(/\n…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `modules/outbound/handlers/outbound.research_source.generate_adapter.done.js:25` | `agent.task.get({ task_id: taskId })` | §16.30 agent.task.get({task_id}) |
| `modules/placements/handlers/_ats.js:232` | `audit.event({ collection: 'business_commands', record_id: typeof command?.record_id === 'string' ? …)` | CAP-3 #831 caps/audit.rs:14 Event |
| `modules/placements/handlers/_ats.js:254` | `config.get({ key })` | CAP-3 #831 caps/config.rs:23 Get (Whitelist ALLOWED_KEYS, nur lesend) |
| `modules/research/handlers/_lib/callback.js:21` | `agent.task.get({ task_id: taskId })` | §16.30 agent.task.get({task_id}) |
| `modules/research/handlers/_lib/callback.js:93` | `agent.task.workspace.read({ task_id: taskId, path })` | §16.30 agent.task.workspace.read({task_id, path}) |
| `modules/research/handlers/_lib/enqueue.js:104` | `agent.task.enqueue({ title: optional(p.title) ?? `Research · ${p.research_brief?.task_title}`, instruction…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `modules/research/handlers/_lib/enqueue.js:123` | `agent.task.enqueue({ title: optional(p.title) ?? `Knowledge aktualisieren · ${values.task_title}`, instruc…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `modules/research/handlers/_lib/systematic.js:42` | `agent.task.workspace.read({ task_id: taskId, path })` | §16.30 agent.task.workspace.read({task_id, path}) |
| `modules/research/handlers/_lib/systematic.js:283` | `agent.task.workspace.read({ task_id: taskId, path: 'validation/evidence-manifest.json' })` | §16.30 agent.task.workspace.read({task_id, path}) |
| `modules/research/handlers/_lib/systematic.js:304` | `agent.task.workspace.read({ task_id: taskId, path })` | §16.30 agent.task.workspace.read({task_id, path}) |
| `modules/research/handlers/_lib/systematic.js:332` | `knowledge.query({ argv, workspace_task_id: workspaceTaskId })` | native Payload (§16.30 knowledge.query (caps/knowledge.rs, native Knowledge-Payload flach)) |
| `modules/research/handlers/_lib/systematic.js:333` | `knowledge.query({ argv })` | native Payload (§16.30 knowledge.query (caps/knowledge.rs, native Knowledge-Payload flach)) |
| `modules/research/handlers/_lib/systematic.js:360` | `dataframe.window({ table: catalogId, skip, limit: ROW_PAGE })` | caps/doc_extract.rs:34 Window (filter/sort nur leer) |
| `modules/research/handlers/research.systematic.run.complete.js:23` | `agent.task.get({ task_id: p.task_id.trim() })` | §16.30 agent.task.get({task_id}) |
| `modules/research/handlers/research.systematic.run.complete.js:47` | `agent.queue.task.for_command({ command_id: commandId })` | §16.30 agent.queue.task.for_command({command_id}) |
| `modules/research/handlers/research.systematic.run.complete.js:48` | `agent.queue.task.get({ task_id: lookupId })` | §16.30 agent.queue.task.get({task_id}) |
| `modules/research/handlers/research.systematic.run.complete.js:68` | `agent.queue.task.get({ task_id: queueTask.message_key })` | §16.30 agent.queue.task.get({task_id}) |
| `modules/research/handlers/research.systematic.run.complete.js:69` | `agent.queue.task.for_command({ command_id: commandId })` | §16.30 agent.queue.task.for_command({command_id}) |
| `modules/spreadsheets/handlers/_lib/automation.js:52` | `agent.task.enqueue({ title: `${SPREADSHEET_RUNBOOKS[runbookId]}: ${text(sheet.title) \|\| sheetId}`, instruc…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `modules/spreadsheets/handlers/_lib/automation.js:93` | `agent.task.get({ task_id: taskId })` | §16.30 agent.task.get({task_id}) |
| `modules/spreadsheets/handlers/_lib/office.js:169` | `office.edit.prepare({ kind: binding.kind, blob_id: { ref: sourceRef }, implemented_features: implementedFea…)` | caps/office.rs:268 Prepare; BlobRef {ref} §16.32 |
| `modules/spreadsheets/handlers/_lib/office.js:212` | `office.edit.apply({ session, changes: { ref: changesRef }, base_version: refOf(session.editor), implement…)` | caps/office.rs:277 Apply (session {kind, source:{ref}, editor:{ref}}, changes {ref}, base_version = session.editor.ref; §16.32) |
| `modules/spreadsheets/handlers/office.spreadsheet.export.js:28` | `office.render({ session: prepared.session, to })` | caps/office.rs:294 Render (to: docx\|xlsx\|email_html; pdf nativ nicht unterstützt) |
| `modules/submissions/handlers/_ats.js:232` | `audit.event({ collection: 'business_commands', record_id: typeof command?.record_id === 'string' ? …)` | CAP-3 #831 caps/audit.rs:14 Event |
| `modules/submissions/handlers/_ats.js:254` | `config.get({ key })` | CAP-3 #831 caps/config.rs:23 Get (Whitelist ALLOWED_KEYS, nur lesend) |
| `modules/support/handlers/_agent-task.js:101` | `agent.task.enqueue({ title, instruction, skill: 'support/support-workflow', thread_key: `business-os/suppo…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `modules/support/handlers/_agent-task.js:197` | `agent.task.get({ task_id: taskId })` | §16.30 agent.task.get({task_id}) |
| `modules/threads/handlers/_lib.js:422` | `policy.module_decision({ permission, module }) [cap(ctx,[…])]` | CAP-3b #844 caps/directory.rs policy (Akteur aus ctx) → {allowed, display_reason, reason_code} |
| `modules/threads/handlers/_lib.js:441` | `directory.user({ user_id: id }) [cap(ctx,[…])]` | CAP-3b #844 caps/directory.rs user → {found, user_id, display_name?, active} |
| `modules/threads/handlers/threads.ai.request.js:61` | `agent.task.enqueue({ title, instruction, thread_key: threadKey, payload: taskPayload, idempotency_key: `th…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `modules/threads/handlers/threads.ctox_approval.approve.js:68` | `agent.task.enqueue({ title: typeof payload.title === 'string' && payload.title.trim() !== '' ? payload.tit…)` | §16.30 agent.task.enqueue (echtes Primitiv) |
| `modules/tickets/handlers/_tickets.js:253` | `audit.event({ collection: caseDoc \|\| entry.case_id ? COLLECTIONS.cases : COLLECTIONS.items, record_…) [callCap]` | CAP-3 #831 caps/audit.rs:14 Event |
| `modules/tickets/handlers/_tickets.js:371` | `agent.task.enqueue({ title: `Ticket-Rückschrieb ${request.operation} ${ticket.ticket_key}`, instruction, s…) [callCap]` | §16.30 agent.task.enqueue (echtes Primitiv) |

<!-- END GENERATED -->
