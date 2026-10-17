# Zuordnung: 131 Capabilities → OS-Primitiv / App-Logik / Plattform

Stand 15.10.2026. CAPA „Shell v4“. Erzeugt aus den Live-Manifesten unter
`../manifests/*.manifest.json` (Feld `capabilities_required`) und `catalog.mjs`.
Die Tabellen unten regeneriert `node gen-mapping.mjs`; Klassifikation und
Begründungen stehen in `catalog.mjs` bzw. `CATALOG.md`.

**Hinweis zur Zahl:** Der Auftrag nennt ~120 Capabilities; die Live-Manifeste führen
derzeit **131** eindeutige `capabilities_required` (eine parallele Manifest-Regeneration
hat die Code-Aufrufstellen wie `agent.task.find/get/update/set_metadata/cancel`,
`agent.queue.task.delete/set_status`, `fence.*`, `command.get`,
`outbound.field_status_witnesses.*` mit aufgenommen). Die Zuordnung ist über den
Live-Satz getrieben, damit jede einzelne genau einmal erscheint.

**Legende.** (a) ist bereits ein OS-Primitiv (Name im Katalog). (b) ist App-Logik: sie
muss aus dem Core in App-Handler über eigene Collections, getrieben von den Primitiven;
Spalten Fachlogik / Umsetzung / Aufwand (S/M/L) / betroffene Handler. (c) ist Plattform
(`ctox-platform`): generisches Core-Plumbing, das Core-Zustand spiegelt (App-Installation,
Rechte, Queue-/Command-Projektion) — gehört nicht einer App.

<!-- BEGIN GENERATED (gen-mapping.mjs) -->
_Generiert aus 73 eindeutigen `capabilities_required` über 28 Manifeste. Nicht von Hand ändern; `node gen-mapping.mjs`._

**Zählung:** OS-Primitiv 66 · App-Logik 5 · Plattform 2 · UNBEKANNT 0.

### (a) Bereits ein OS-Primitiv

| Capability | Primitiv | Apps |
|---|---|---|
| `agent.coding.cancel` | `agent.turn` | coding-agents, shell |
| `agent.coding.models` | `agent.turn` | coding-agents |
| `agent.coding.turn` | `agent.turn` | coding-agents, shell |
| `agent.queue.control` | `agent.queue` | ctox |
| `agent.queue.task.claim` | `agent.queue` | ctox |
| `agent.queue.task.delete` | `agent.queue` | ctox |
| `agent.queue.task.for_command` | `agent.queue` | ctox, research |
| `agent.queue.task.get` | `agent.queue` | ctox, research |
| `agent.queue.task.update` | `agent.queue` | ctox |
| `agent.task.cancel` | `agent.task` | outbound-lead-generation |
| `agent.task.enqueue` | `agent.task` | appsec-pentest, ctox, cv-print-builder, documents, iot, knowledge, kundenpipeline, news, outbound, outbound-lead-generation, research, spreadsheets, support, threads, tickets |
| `agent.task.find` | `agent.task` | outbound-lead-generation |
| `agent.task.get` | `agent.task` | appsec-pentest, ctox, cv-print-builder, documents, iot, knowledge, kundenpipeline, news, outbound, outbound-lead-generation, research, spreadsheets, support |
| `agent.task.set_metadata` | `agent.task` | outbound-lead-generation |
| `agent.task.update` | `agent.task` | outbound-lead-generation |
| `agent.task.workspace.read` | `agent.task` | documents, knowledge, outbound-lead-generation, research |
| `artifact.write` | `files` | outbound |
| `audit.event` | `audit` | consent, esign, intake, interviews, nachweise, outbound, placements, submissions, threads, tickets |
| `audit.query` | `audit` | consent |
| `blob.materialize` | `files` | mail |
| `browser.auth_assist.request` | `browser` | news, outbound, outbound-lead-generation |
| `browser.auth_assist_complete` | `browser` | browser |
| `browser.automation.run` | `browser` | outbound |
| `browser.op` | `browser` | browser |
| `browser.session_stop` | `browser` | browser |
| `config.get` | `config` | consent, esign, intake, interviews, nachweise, outbound-lead-generation, placements, submissions |
| `dataframe.window` | `dataframe` | documents, knowledge, research |
| `directory.user` | `directory` | threads |
| `doc.extract` | `doc` | cv-print-builder, matching |
| `doc.html` | `doc` | matching |
| `fence.claim` | `lock` | outbound-lead-generation |
| `fence.release` | `lock` | outbound-lead-generation |
| `files.get` | `files` | outbound, outbound-lead-generation |
| `files.put` | `files` | outbound-lead-generation |
| `identity.users.list` | `directory` | kundenpipeline |
| `knowledge.query` | `dataframe` | iot, outbound, research |
| `mail.channel.mailbox.link` | `mail.transport` | outbound |
| `mail.delivery_log.query` | `mail.transport` | outbound |
| `mail.send` | `mail.transport` | kundenpipeline, outbound, outbound-lead-generation |
| `mail.server.config.get` | `mail.transport` | kundenpipeline, mail |
| `mail.server.domain.delete` | `mail.transport` | mail |
| `mail.server.domain.save` | `mail.transport` | mail |
| `mail.server.runtime.save` | `mail.transport` | mail |
| `mail.server.user.delete` | `mail.transport` | mail |
| `mail.server.user.save` | `mail.transport` | mail |
| `model` | `model` | iot |
| `net.ingest.subscribe` | `net.ingest` | iot |
| `net.ingest.webhook.register` | `net.ingest` | iot, outbound |
| `office.edit.apply` | `doc` | documents, spreadsheets |
| `office.edit.prepare` | `doc` | documents, mail, spreadsheets |
| `office.render` | `doc` | documents, mail, spreadsheets |
| `office.settings.save` | `doc` | documents |
| `policy.module_decision` | `policy` | threads |
| `process.run` | `process.run` | outbound-lead-generation |
| `schedule.list` | `schedule` | news |
| `schedule.remove` | `schedule` | news |
| `schedule.set_enabled` | `schedule` | news |
| `schedule.upsert` | `schedule` | news |
| `secret.delete` | `secrets` | credentials |
| `secret.exists` | `secrets` | outbound-lead-generation |
| `secret.list` | `secrets` | credentials |
| `secret.put` | `secrets` | credentials |
| `secret.scope` | `secrets` | outbound-lead-generation |
| `secrets.exists` | `secrets` | outbound |
| `speech.stt.transcribe` | `speech` | interviews |
| `web.fetch` | `net.http` | matching |

### (b) App-Logik (muss aus dem Core in App-Handler)

| Capability | App | Fachlogik | Umsetzung in der App | Aufwand | Handler |
|---|---|---|---|---|---|
| `pentest.app_audit` | appsec-pentest | Business-OS-App-Audit | process.run (Allowlist-Name) | L | appsec-pentest/handlers/ctox.appsec.app.audit.js |
| `pentest.exploit_get` | appsec-pentest | Exploit-Skript lesen | files.get (State-Dir, Pfadprüfung im Handler) | M | appsec-pentest/handlers/ctox.appsec.exploit.get.js |
| `pentest.exploit_list` | appsec-pentest | Exploit-Index lesen | files.get (State-Dir-Index) | M | appsec-pentest/handlers/ctox.appsec.exploit.list.js |
| `pentest.exploit_verify` | appsec-pentest | Exploit erneut ausführen | process.run + files.get | L | appsec-pentest/handlers/ctox.appsec.exploit.verify.js |
| `pentest.run` | appsec-pentest | Pentest-Engine-Lauf | process.run (Allowlist-Name) + files; Port blockiert bis files.*-Cutover | L | appsec-pentest/handlers/_lib/args.js |

### (c) Plattform (ctox-platform, nicht App)

| Capability | Begründung | Apps |
|---|---|---|
| `agent.flow.record` | Harness-Prozess-Evidenz (harness_flow) | ctox |
| `command.get` | Lesen von business_commands (Plattform-Collection) | outbound-lead-generation |
<!-- END GENERATED -->
