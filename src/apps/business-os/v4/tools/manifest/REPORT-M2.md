# Manifest v4 – Bericht Strang M2 (generiert von gen-manifest.mjs, nicht von Hand ändern)

Quellen: module.json, collections.schema.json (Vorrang v4/schemas/<app_id>.collections.schema.json), handlers/*.js,
Code (commands_out, Trigger-Exporte, net.ingest), je App v4-manifest.json (commands_in, exports, Rechte, multi_instance, Fremdzugriffs-ops, Trigger),
ownership-baseline.json (eingefrorene Eigentümer-Zuordnung), Plattform-exports aus ~/.local/state/ctox-sync-v4/docs/v4-manifests/ctox-platform.manifest.json.
Prüfung: `node --test src/apps/business-os/v4/tools/manifest/gen-manifest.test.mjs`; `node src/apps/business-os/v4/tools/manifest/gen-manifest.mjs --check`.

## Zahlen

| Merkmal | Wert |
|---|---|
| Apps (modules + customer-modules) | 41 |
| Manifeste gesamt (Apps + shell) | 42 |
| Collections gesamt / App-eigen / ctox-platform | 236 / 221 / 15 |
| Handler gesamt (Apps mit Handlern) | 399 (29) |
| Legacy-Rest gesamt (ctx.cap.legacy) | 0 |
| Capabilities gesamt (verschieden, ohne legacy) | 71 |
| Trigger | 11 |
| command_rights-Einträge | 55 |
| foreign-Deklarationen (davon write / nur Code-Scan) | 293 (32 / 94) |
| **foreign ohne passendes exports (Soll 0)** | **0** |
| Plattformzugriffe auf exports beschränkt | 1 |
| Befehlstypen mit Handlern in mehreren Apps | 8 |
| eigene Collections ohne Schema | 0 |
| schema.js noch vorhanden / nicht ladbar | 3 / 0 |
| Shell-Dateien | 123 |
| Apps mit styles / write_guard-Collections | 41 / 1 |
| offene _todo gesamt | 149 |
| Deklarationsdateien v4-manifest.json | 40 |
| commands_out / commands_in | 34 / 21 |
| **commands ohne Gegenstück (Soll 0)** | **0** |
| ctx.command mit nicht auflösbarem Ziel | 0 |
| **UI-Befehle ohne Route (E40, Soll 0)** / mehrdeutig | **0** / 0 |
| Plattformroute gemeldet (PLATFORM_ROUTES_PENDING) / commands_in aus UI abgeleitet | 26 / 10 |
| net.ingest-Anmeldungen | 2 |
| Apps mit multi_instance:false | 40 |
| Icons: Rasterbild im Paket / nur SVG / generiert (MAN2) | 40 / 1 / 0 |

## Je App

| App | Handler | Legacy-Rest | Capabilities | Trigger | command_rights | foreign | exports | _todo |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| app-store | 0 | 0 | 0 | 0 | 0 | 7 | 0 | 4 |
| appsec-pentest | 35 | 0 | 7 | 0 | 0 | 3 | 0 | 3 |
| browser | 20 | 0 | 3 | 0 | 0 | 4 | 0 | 3 |
| buchhaltung | 0 | 0 | 0 | 0 | 0 | 1 | 7 | 3 |
| calendar | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 3 |
| coding-agents | 3 | 0 | 3 | 0 | 0 | 3 | 0 | 3 |
| consent | 5 | 0 | 3 | 0 | 1 | 12 | 1 | 3 |
| conversations | 0 | 0 | 0 | 0 | 0 | 5 | 3 | 4 |
| creator | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 3 |
| credentials | 3 | 0 | 3 | 0 | 1 | 1 | 0 | 3 |
| ctox | 22 | 0 | 9 | 0 | 5 | 4 | 16 | 4 |
| customers | 20 | 0 | 0 | 0 | 0 | 9 | 3 | 3 |
| cv-print-builder | 3 | 0 | 3 | 0 | 0 | 7 | 0 | 3 |
| desktop | 0 | 0 | 0 | 0 | 0 | 4 | 2 | 3 |
| documents | 13 | 0 | 8 | 0 | 0 | 9 | 3 | 3 |
| esign | 2 | 0 | 2 | 0 | 1 | 14 | 1 | 3 |
| explorer | 0 | 0 | 0 | 0 | 0 | 8 | 0 | 3 |
| file-viewer | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 3 |
| importer | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 3 |
| intake | 1 | 0 | 2 | 0 | 1 | 12 | 1 | 3 |
| interviews | 1 | 0 | 3 | 0 | 1 | 13 | 2 | 3 |
| invoices | 23 | 0 | 0 | 0 | 0 | 13 | 1 | 3 |
| iot | 22 | 0 | 6 | 3 | 1 | 1 | 0 | 3 |
| knowledge | 11 | 0 | 4 | 2 | 0 | 1 | 4 | 3 |
| kundenpipeline | 10 | 0 | 5 | 1 | 0 | 3 | 1 | 3 |
| mail | 7 | 0 | 9 | 0 | 0 | 15 | 0 | 3 |
| matching | 3 | 0 | 3 | 0 | 0 | 2 | 2 | 3 |
| nachweise | 3 | 0 | 2 | 0 | 1 | 12 | 1 | 3 |
| news | 8 | 0 | 7 | 0 | 0 | 1 | 0 | 3 |
| notes | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 3 |
| outbound | 60 | 0 | 13 | 2 | 1 | 14 | 8 | 12 |
| outbound-lead-generation | 28 | 0 | 18 | 1 | 0 | 5 | 9 | 13 |
| placements | 2 | 0 | 2 | 0 | 1 | 11 | 2 | 3 |
| reports | 0 | 0 | 0 | 0 | 0 | 6 | 0 | 3 |
| research | 6 | 0 | 7 | 0 | 0 | 11 | 2 | 3 |
| shiftflow | 0 | 0 | 0 | 0 | 0 | 2 | 1 | 3 |
| spreadsheets | 8 | 0 | 5 | 0 | 0 | 6 | 3 | 3 |
| submissions | 1 | 0 | 2 | 0 | 1 | 13 | 1 | 3 |
| support | 38 | 0 | 2 | 2 | 37 | 11 | 3 | 3 |
| threads | 27 | 0 | 4 | 0 | 3 | 21 | 0 | 4 |
| tickets | 14 | 0 | 2 | 0 | 0 | 4 | 5 | 3 |

## Legacy-Rest je App


## Capabilities gesamt (71)

| Capability | Apps |
|---|---|
| `agent.coding.cancel` | coding-agents |
| `agent.coding.models` | coding-agents |
| `agent.coding.turn` | coding-agents |
| `agent.flow.record` | ctox |
| `agent.queue.control` | ctox |
| `agent.queue.task.claim` | ctox |
| `agent.queue.task.delete` | ctox |
| `agent.queue.task.for_command` | ctox, research |
| `agent.queue.task.get` | ctox, research |
| `agent.queue.task.update` | ctox |
| `agent.task.cancel` | outbound-lead-generation |
| `agent.task.enqueue` | appsec-pentest, ctox, cv-print-builder, documents, iot, knowledge, kundenpipeline, news, outbound, outbound-lead-generation, research, spreadsheets, support, threads, tickets |
| `agent.task.find` | outbound-lead-generation |
| `agent.task.get` | appsec-pentest, ctox, cv-print-builder, documents, iot, knowledge, kundenpipeline, news, outbound, outbound-lead-generation, research, spreadsheets, support |
| `agent.task.set_metadata` | outbound-lead-generation |
| `agent.task.update` | outbound-lead-generation |
| `agent.task.workspace.read` | documents, knowledge, outbound-lead-generation, research |
| `artifact.write` | outbound |
| `audit.event` | consent, esign, intake, interviews, nachweise, outbound, placements, submissions, threads, tickets |
| `audit.query` | consent |
| `blob.materialize` | mail |
| `browser.auth_assist.request` | news, outbound, outbound-lead-generation |
| `browser.auth_assist_complete` | browser |
| `browser.automation.run` | outbound |
| `browser.op` | browser |
| `browser.session_stop` | browser |
| `command.get` | outbound-lead-generation |
| `config.get` | consent, esign, intake, interviews, nachweise, outbound-lead-generation, placements, submissions |
| `dataframe.window` | documents, knowledge, research |
| `directory.user` | threads |
| `doc.extract` | cv-print-builder, matching |
| `doc.html` | matching |
| `fence.claim` | outbound-lead-generation |
| `fence.release` | outbound-lead-generation |
| `files.get` | appsec-pentest, outbound, outbound-lead-generation |
| `files.list` | appsec-pentest |
| `files.put` | outbound-lead-generation |
| `identity.users.list` | kundenpipeline |
| `knowledge.query` | iot, outbound, research |
| `mail.channel.mailbox.link` | outbound |
| `mail.delivery_log.query` | outbound |
| `mail.send` | kundenpipeline, outbound, outbound-lead-generation |
| `mail.server.config.get` | kundenpipeline, mail |
| `mail.server.domain.delete` | mail |
| `mail.server.domain.save` | mail |
| `mail.server.runtime.save` | mail |
| `mail.server.user.delete` | mail |
| `mail.server.user.save` | mail |
| `model` | iot |
| `net.ingest.subscribe` | iot |
| `net.ingest.webhook.register` | iot, outbound |
| `office.edit.apply` | documents, spreadsheets |
| `office.edit.prepare` | documents, mail, spreadsheets |
| `office.render` | documents, mail, spreadsheets |
| `office.settings.save` | documents |
| `pentest.app_audit` | appsec-pentest |
| `pentest.run` | appsec-pentest |
| `policy.module_decision` | threads |
| `process.run` | appsec-pentest, outbound-lead-generation |
| `schedule.list` | news |
| `schedule.remove` | news |
| `schedule.set_enabled` | news |
| `schedule.upsert` | news |
| `secret.delete` | credentials |
| `secret.exists` | outbound-lead-generation |
| `secret.list` | credentials |
| `secret.put` | credentials |
| `secret.scope` | outbound-lead-generation |
| `secrets.exists` | outbound |
| `speech.stt.transcribe` | interviews |
| `web.fetch` | matching |

## Trigger

| App | Handler | Auslöser |
|---|---|---|
| iot | handlers/ctox.iot.rules.tick.js | every PT1M |
| iot | handlers/ctox.iot.signal.evaluate.js | on iot_attributes insert |
| iot | handlers/ctox.iot.signal.evaluate.js | on iot_attributes update |
| knowledge | handlers/knowledge.view.window.js | on knowledge_views insert |
| knowledge | handlers/knowledge.view.window.js | on knowledge_views update |
| kundenpipeline | handlers/kundenpipeline.mail.inbound.js | on communication_messages insert |
| outbound | handlers/outbound.scheduler.tick.js | every PT5M |
| outbound | handlers/outbound.tracking.register.js | every PT1H |
| outbound-lead-generation | handlers/outbound.contacts.email_validation_sweep.js | every PT1H |
| support | handlers/support.intake.project.js | on communication_messages insert |
| support | handlers/support.intake.project.js | on communication_messages update |

## App-übergreifende Befehle (§16.40)

| Absender | Ziel-App | Typ | Beleg |
|---|---|---|---|
| buchhaltung | ctox | `ctox.app.modify.request` | index.js:4373 |
| buchhaltung | ctox | `ctox.chat.task` | index.js:4373 |
| conversations | outbound | `outbound.message.approve` | index.js:1929 |
| conversations | outbound | `outbound.message.reject` | index.js:1935 |
| conversations | outbound | `outbound.reply.classify` | index.js:1944 |
| conversations | outbound | `outbound.reply.match` | index.js:1959 |
| desktop | ctox | `ctox.app.modify.request` | index.js:469 |
| desktop | ctox | `ctox.chat.task` | index.js:469 |
| documents | ctox | `ctox.app.modify.request` | index.js:658 |
| documents | ctox | `ctox.chat.task` | index.js:658 |
| knowledge | ctox | `ctox.app.modify.request` | index.js:2857 |
| knowledge | ctox | `ctox.chat.task` | index.js:2857 |
| mail | ctox | `ctox.chat.task` | index.js:2561 |
| mail | outbound | `outbound.campaign.status.set` | index.js:1131 |
| mail | outbound | `outbound.message.approve` | index.js:1144 |
| mail | outbound | `outbound.message.compose` | index.js:1258 |
| mail | outbound | `outbound.message.request_approval` | index.js:1142, index.js:1951 |
| mail | outbound | `outbound.message.send_approved` | index.js:1146, index.js:1951 |
| mail | outbound | `outbound.provider.reconcile` | index.js:1123 |
| mail | support | `support.conversation.open_from_thread` | index.js:2515, index.js:2516 |
| matching | ctox | `ctox.app.modify.request` | ui/businessOsControls.js:1175 |
| matching | ctox | `ctox.chat.task` | ui/businessOsControls.js:1175 |
| outbound | outbound-lead-generation | `outbound.research_source.reprobe` | handlers/_lib/owner-app.js:22 |
| outbound | outbound-lead-generation | `outbound.research_source.test` | handlers/_lib/owner-app.js:22 |
| outbound | outbound-lead-generation | `outbound.scrape.script.read` | handlers/_lib/olg-registry.js:167, v4-manifest.json; handlers/_lib/olg-registry.js requestLatestScript/requestTargetUpsert; customer-modules/outbound-lead-generation/OLG725.md (§16.40, §16.47 Nr. 4) |
| outbound | outbound-lead-generation | `outbound.scrape.target.upsert` | handlers/_lib/olg-registry.js:73, v4-manifest.json; handlers/_lib/olg-registry.js requestLatestScript/requestTargetUpsert; customer-modules/outbound-lead-generation/OLG725.md (§16.40, §16.47 Nr. 4) |
| outbound | outbound-lead-generation | `outbound.update_digest.send_now` | handlers/_lib/owner-app.js:22 |
| research | ctox | `ctox.app.modify.request` | index.js:4884 |
| research | ctox | `ctox.chat.task` | index.js:4884 |
| research | documents | `research.systematic.report.create` | index.js:2550, index.js:2551 |
| research | knowledge | `knowledge.book.ingest` | handlers/_lib/callback.js:212 |
| spreadsheets | ctox | `ctox.app.modify.request` | index.js:2225 |
| spreadsheets | ctox | `ctox.chat.task` | index.js:2225 |
| support | tickets | `ctox.ticket.local.create` | handlers/support.ticket.create_from_conversation.js:33 |

| Empfänger | Typ | from |
|---|---|---|
| ctox | `business_os.chat.task` | * |
| ctox | `ctox.app.modify.request` | * |
| ctox | `ctox.chat.task` | * |
| documents | `research.systematic.report.create` | research |
| knowledge | `knowledge.book.ingest` | research |
| outbound | `outbound.campaign.status.set` | mail |
| outbound | `outbound.message.approve` | conversations, mail |
| outbound | `outbound.message.compose` | mail |
| outbound | `outbound.message.reject` | conversations |
| outbound | `outbound.message.request_approval` | mail |
| outbound | `outbound.message.send_approved` | mail |
| outbound | `outbound.provider.reconcile` | mail |
| outbound | `outbound.reply.classify` | conversations |
| outbound | `outbound.reply.match` | conversations |
| outbound-lead-generation | `outbound.research_source.reprobe` | outbound |
| outbound-lead-generation | `outbound.research_source.test` | outbound |
| outbound-lead-generation | `outbound.scrape.script.read` | outbound |
| outbound-lead-generation | `outbound.scrape.target.upsert` | outbound |
| outbound-lead-generation | `outbound.update_digest.send_now` | outbound |
| support | `support.conversation.open_from_thread` | mail |
| tickets | `ctox.ticket.local.create` | support |

Ohne Gegenstück (0, Soll 0):

Nicht auflösbares Ziel (0):

Shell (SHELL_COMMANDS_OUT + UI-Scan v4/frame, v4/shell, v4/source):
- shell → ctox `business_os.chat.task`: v4/shell/chat/index.js (commandBus.dispatch → api.command; shared/business-chat.js Standardtyp business_os.chat.task, CHAT4.md)
- shell → ctox `ctox.chat.task`: v4/shell/app-host.js:500 (Menüpunkt ctox.ask „CTOX fragen…“ → Eingabeblatt → Standardbefehl ctox.chat.task)

### commands_in aus der UI-Nutzung abgeleitet (E40, je Absender, nie '*') (10)

- outbound ← conversations `outbound.message.approve`: modules/conversations/index.js:1929
- outbound ← conversations `outbound.message.reject`: modules/conversations/index.js:1935
- outbound ← conversations `outbound.reply.classify`: modules/conversations/index.js:1944
- outbound ← conversations `outbound.reply.match`: modules/conversations/index.js:1959
- outbound ← mail `outbound.campaign.status.set`: modules/mail/index.js:1131
- outbound ← mail `outbound.message.approve`: modules/mail/index.js:1144
- outbound ← mail `outbound.message.compose`: modules/mail/index.js:1258
- outbound ← mail `outbound.message.request_approval`: modules/mail/index.js:1142, modules/mail/index.js:1951
- outbound ← mail `outbound.message.send_approved`: modules/mail/index.js:1146, modules/mail/index.js:1951
- outbound ← mail `outbound.provider.reconcile`: modules/mail/index.js:1123

### UI-Befehle ohne Route: weder Plattformroute (src/core/business_os/business_command_inventory.json + Registry) noch Handler (E40, 0, Soll 0, rot)

keine

### Mehrdeutig: Handler in mehreren Apps (0, Soll 0, rot)

keine

### An den Kern gemeldet, noch keine Plattformroute (PLATFORM_ROUTES_PENDING, 26)

- `browser.capture.extract` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): browser modules/browser/index.js:1424
- `browser.http_auth.respond` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): browser modules/browser/index.js:627
- `browser.observer.grant` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): browser modules/browser/index.js:587
- `browser.observer.revoke` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): browser modules/browser/index.js:592
- `browser.upload.select` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): browser modules/browser/index.js:555
- `browser.webauthn.respond` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): browser modules/browser/index.js:636
- `business_os.data.modify` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): buchhaltung modules/buchhaltung/index.js:814
- `business_os.data.modify` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): research modules/research/index.js:4898
- `business_os.i18n.translate` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): matching modules/matching/ui/businessOsControls.js:789
- `ctox.browser_context.capture` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): browser modules/browser/index.js:1369
- `ctox.business_os.app.create` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): creator modules/creator/index.js:1025
- `ctox.business_os.app.create` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): desktop modules/desktop/appCommands.js:28
- `ctox.business_os.app.create` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): importer modules/importer/index.js:173
- `ctox.knowledge.runbooks.manage` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): documents modules/documents/index.js:3332
- `ctox.plan.step.retry` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): im UI-Scan keine Fundstelle als Befehlstyp
- `ctox.workjet.calendar.connect.v1` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): shell v4/shell/workjet/actions/calendar-connect.js:11
- `ctox.workjet.calendar.read.v1` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): shell v4/shell/workjet/actions/meeting.js:150
- `ctox.workjet.device.v1` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): shell v4/shell/workjet/actions/device.js:45
- `ctox.workjet.grok.v1` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): shell v4/shell/workjet/actions/instance.js:18
- `ctox.workjet.jour_fixe.speech.v1` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): shell v4/shell/workjet/actions/meeting.js:204
- `ctox.workjet.speech.dictation.v1` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): shell v4/shell/workjet/actions/meeting.js:28
- `ctox.workjet.speech.settings.v1` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): shell v4/shell/workjet/actions/meeting.js:105
- `external_sql.write` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): outbound-lead-generation customer-modules/outbound-lead-generation/index.js:10766
- `mail.series_email.import` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): mail modules/mail/index.js:2044
- `outbound-lead-generation.import` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): outbound-lead-generation customer-modules/outbound-lead-generation/index.js:8486
- `web_stack.auth_assist.request` (an den Kern gemeldet 16.10.2026, Übergabe-Abnahmeliste E40): ctox modules/ctox/index.js:4906

## foreign ohne passendes exports (0, Soll 0)

keine

## Plattformzugriffe auf exports beschränkt (1)

- app-store: app_drafts gewünscht read, gewährt – (nicht in Plattform-exports)

## Befehlstypen mit Handlern in mehreren Apps (8)

- `outbound.research_policy.publish`: outbound, outbound-lead-generation
- `outbound.research_source.auth_assist`: outbound, outbound-lead-generation
- `outbound.research_source.generate_adapter`: outbound, outbound-lead-generation
- `outbound.research_source.reprobe`: outbound, outbound-lead-generation
- `outbound.research_source.test`: outbound, outbound-lead-generation
- `outbound.research_source.upsert`: outbound, outbound-lead-generation
- `outbound.sellify.lookup`: outbound, outbound-lead-generation
- `outbound.update_digest.send_now`: outbound, outbound-lead-generation

## Offene _todo (app-spezifisch; die drei Standardpunkte rights/client_limit/app_ver stehen in jedem Manifest)

### app-store

- app_drafts: Plattform exportiert die Collection nicht – Zugriff (read) nicht deklariert.

### conversations

- schema.js ist noch Teil der Dateien; entfällt beim Port (§16.13).

### ctox

- schema.js ist noch Teil der Dateien; entfällt beim Port (§16.13).

### outbound

- Befehlstyp outbound.research_policy.publish hat Handler in mehreren Apps (outbound, outbound-lead-generation); der Core muss eindeutig routen.
- Befehlstyp outbound.research_source.auth_assist hat Handler in mehreren Apps (outbound, outbound-lead-generation); der Core muss eindeutig routen.
- Befehlstyp outbound.research_source.generate_adapter hat Handler in mehreren Apps (outbound, outbound-lead-generation); der Core muss eindeutig routen.
- Befehlstyp outbound.research_source.reprobe hat Handler in mehreren Apps (outbound, outbound-lead-generation); der Core muss eindeutig routen.
- Befehlstyp outbound.research_source.test hat Handler in mehreren Apps (outbound, outbound-lead-generation); der Core muss eindeutig routen.
- Befehlstyp outbound.research_source.upsert hat Handler in mehreren Apps (outbound, outbound-lead-generation); der Core muss eindeutig routen.
- Befehlstyp outbound.sellify.lookup hat Handler in mehreren Apps (outbound, outbound-lead-generation); der Core muss eindeutig routen.
- Befehlstyp outbound.update_digest.send_now hat Handler in mehreren Apps (outbound, outbound-lead-generation); der Core muss eindeutig routen.
- schema.js ist noch Teil der Dateien; entfällt beim Port (§16.13).

### outbound-lead-generation

- Befehlstyp outbound.research_policy.publish hat Handler in mehreren Apps (outbound, outbound-lead-generation); der Core muss eindeutig routen.
- Befehlstyp outbound.research_source.auth_assist hat Handler in mehreren Apps (outbound, outbound-lead-generation); der Core muss eindeutig routen.
- Befehlstyp outbound.research_source.generate_adapter hat Handler in mehreren Apps (outbound, outbound-lead-generation); der Core muss eindeutig routen.
- Befehlstyp outbound.research_source.reprobe hat Handler in mehreren Apps (outbound, outbound-lead-generation); der Core muss eindeutig routen.
- Befehlstyp outbound.research_source.test hat Handler in mehreren Apps (outbound, outbound-lead-generation); der Core muss eindeutig routen.
- Befehlstyp outbound.research_source.upsert hat Handler in mehreren Apps (outbound, outbound-lead-generation); der Core muss eindeutig routen.
- Befehlstyp outbound.sellify.lookup hat Handler in mehreren Apps (outbound, outbound-lead-generation); der Core muss eindeutig routen.
- Befehlstyp outbound.update_digest.send_now hat Handler in mehreren Apps (outbound, outbound-lead-generation); der Core muss eindeutig routen.
- module.json.entry zeigt auf local-modules/…; Kundeninstanzen tragen ggf. eine abweichende installierte module.json (~/.local/state/ctox-sync-v4/docs/thesen-olg-installiert-20261015/module.json) – vor Aktivierung abgleichen.
- Abnahme Nicht-Admin (V4-PORT.md §13) legt die Mindestrechte fest; Logo (layout.icon_svg) muss erhalten bleiben.

### shell

- files: Pfade relativ zu src/apps/business-os/; lab/-Prüfstände, Tests und .md sind ausgenommen.
- app.js/app.css/index.html und die übrigen shared-Dateien stehen laut Dateigrenze im Shell-Artefakt; Shell v4 ersetzt app.js (wird am Ende gelöscht) – Liste beim Rückbau kürzen.

### threads

- Projektions-Auslöser (W-4) noch nicht im Vertrag: siehe _proposed_triggers.

## Regeln dieses Strangs

1. customer-modules/* werden erfasst; gelöscht werden nur Manifeste mit `_generator` = gen-manifest.mjs, die nicht mehr erzeugt werden.
2. schema.js ist optional (Lade-Fehler landen im Drift-Bericht); Schemaquelle allein collections.schema.json bzw. v4/schemas/<app_id>.collections.schema.json; A9-`migrations` und Alt-`migration_strategies` werden gelesen.
3. Plattform-Collections gehören ctox-platform; Apps deklarieren sie nur gemäß Plattform-exports (business_module_catalog → business_apps/business_app_versions read); überschießende ops werden gekappt und gelistet.
4. handlers = vorhandene handlers/*.js ohne _-Präfix; capabilities_required = im Handler-Code aufgerufene ctx.cap-Pfade (ohne Präfix „ctx.cap.“, wie in den V4-HANDLERS-Vorschlägen) plus `legacy.<typ>` je Legacy-Typ; Handler-Dateien mit „test“ im Befehlstyp sind keine Testdateien.
5. Shell-Manifest: v4/{frame,shell,source}/** + v4/sw.js + Shell-Dateien laut Dateigrenze; lab/-Prüfstände, Tests und .md ausgenommen.
6. styles (§16.19 Nr. 6) = index.css der App (+ styles der Deklaration); write_guard (§16.21) nur, wo ein Rust-Schreibwächter belegt ist (OLG-Leads, outbound_field_review.rs:88).
7. commands_out (§16.40) aus ctx.command im Handler-Code (Literal/Konstante, module-Option, Weiterleitung command.type) und aus UI-Code (shared/context-chat.js; payload.module mit Handler-Typ der Ziel-App); commands_in aus v4-manifest.json plus je konkretem Absender aus der UI-Nutzung (E39/E40, nie '*'); jede Seite ohne Gegenstück ist ein Befund.
7a. UI-Befehle (E39/E40): Typ-Position von api.command bzw. app-lokaler Hüllfunktionen (transitiv); eigener Handler → eigene Route, Plattformroute (Inventur exact_control_types/control_prefixes + Registry ctox.app.*, ctox.coding.*) → keine Deklaration, genau eine fremde Handler-App → commands_out/commands_in, sonst rot (commands_unrouted/commands_out_ambiguous) außer PLATFORM_ROUTES_PENDING.
8. Trigger aus `export const triggers` im Handler oder v4-manifest.json; on-Trigger nur auf eigene oder foreign-watch-Collections. net.ingest-Zustelltypen bekommen command_rights {roles: []} (nur Core-Zustellung).
