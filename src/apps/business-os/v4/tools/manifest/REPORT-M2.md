# Manifest v4 – Bericht Strang M2 (generiert von gen-manifest.mjs, nicht von Hand ändern)

Quellen: module.json, collections.schema.json (Vorrang v4/schemas/<app_id>.collections.schema.json), handlers/*.js,
Code (commands_out, Trigger-Exporte, net.ingest), je App v4-manifest.json (commands_in, exports, Rechte, multi_instance, Fremdzugriffs-ops, Trigger),
ownership-baseline.json (eingefrorene Eigentümer-Zuordnung), Plattform-exports aus src/core/business_os/v4/ctox-platform.manifest.json.
Prüfung: `node --test src/apps/business-os/v4/tools/manifest/gen-manifest.test.mjs`; `node src/apps/business-os/v4/tools/manifest/gen-manifest.mjs --check`.

## Zahlen

| Merkmal | Wert |
|---|---|
| Apps (modules + customer-modules) | 42 |
| Manifeste gesamt (Apps + shell) | 43 |
| Collections gesamt / App-eigen / ctox-platform | 242 / 226 / 16 |
| Handler gesamt (Apps mit Handlern) | 117 (16) |
| Legacy-Rest gesamt (ctx.cap.legacy) | 0 |
| Capabilities gesamt (verschieden, ohne legacy) | 41 |
| Trigger | 10 |
| command_rights-Einträge | 43 |
| foreign-Deklarationen (davon write / nur Code-Scan) | 299 (32 / 94) |
| **foreign ohne passendes exports (Soll 0)** | **0** |
| Plattformzugriffe auf exports beschränkt | 0 |
| Befehlstypen mit Handlern in mehreren Apps | 0 |
| eigene Collections ohne Schema | 0 |
| schema.js noch vorhanden / nicht ladbar | 3 / 0 |
| Shell-Dateien | 126 |
| Apps mit styles / write_guard-Collections | 41 / 1 |
| offene _todo gesamt | 185 |
| Deklarationsdateien v4-manifest.json | 41 |
| commands_out / commands_in | 20 / 7 |
| **commands ohne Gegenstück (Soll 0)** | **0** |
| ctx.command mit nicht auflösbarem Ziel | 1 |
| **UI-Befehle ohne Route (E40, Soll 0)** / mehrdeutig | **0** / 0 |
| Plattformroute gemeldet (PLATFORM_ROUTES_PENDING) / commands_in aus UI abgeleitet | 13 / 1 |
| net.ingest-Anmeldungen | 2 |
| Apps mit multi_instance:false | 41 |
| Icons: Rasterbild im Paket / nur SVG / generiert (MAN2) | 40 / 2 / 0 |

## Je App

| App | Handler | Legacy-Rest | Capabilities | Trigger | command_rights | foreign | exports | _todo |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| app-store | 0 | 0 | 0 | 0 | 0 | 8 | 0 | 3 |
| appsec-pentest | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 3 |
| browser | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 3 |
| buchhaltung | 0 | 0 | 0 | 0 | 0 | 1 | 7 | 3 |
| calendar | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 3 |
| coding-agents | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 3 |
| consent | 0 | 0 | 0 | 0 | 0 | 12 | 1 | 4 |
| conversations | 0 | 0 | 0 | 0 | 0 | 5 | 3 | 4 |
| creator | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 3 |
| credentials | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 4 |
| ctox | 6 | 0 | 6 | 0 | 2 | 4 | 16 | 7 |
| customers | 0 | 0 | 0 | 0 | 0 | 9 | 3 | 3 |
| cv-print-builder | 3 | 0 | 3 | 0 | 0 | 7 | 0 | 3 |
| desktop | 0 | 0 | 0 | 0 | 0 | 4 | 2 | 3 |
| documents | 8 | 0 | 6 | 0 | 0 | 9 | 3 | 3 |
| esign | 0 | 0 | 0 | 0 | 0 | 14 | 1 | 4 |
| explorer | 0 | 0 | 0 | 0 | 0 | 8 | 0 | 3 |
| file-viewer | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 3 |
| importer | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 3 |
| intake | 0 | 0 | 0 | 0 | 0 | 12 | 1 | 4 |
| interviews | 0 | 0 | 0 | 0 | 0 | 13 | 2 | 4 |
| invoices | 0 | 0 | 0 | 0 | 0 | 13 | 1 | 3 |
| iot | 14 | 0 | 5 | 3 | 1 | 1 | 0 | 3 |
| knowledge | 11 | 0 | 4 | 2 | 0 | 1 | 4 | 3 |
| kundenpipeline | 10 | 0 | 5 | 1 | 0 | 3 | 1 | 3 |
| mail | 1 | 0 | 3 | 0 | 0 | 15 | 0 | 3 |
| matching | 3 | 0 | 3 | 0 | 0 | 2 | 2 | 3 |
| nachweise | 0 | 0 | 0 | 0 | 0 | 12 | 1 | 4 |
| news | 6 | 0 | 2 | 0 | 0 | 1 | 0 | 3 |
| notes | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 3 |
| outbound | 17 | 0 | 11 | 1 | 1 | 14 | 8 | 6 |
| outbound-lead-generation | 18 | 0 | 17 | 1 | 0 | 5 | 9 | 5 |
| placements | 0 | 0 | 0 | 0 | 0 | 11 | 2 | 4 |
| reports | 0 | 0 | 0 | 0 | 0 | 6 | 0 | 3 |
| research | 6 | 0 | 7 | 0 | 0 | 11 | 2 | 3 |
| shiftflow | 0 | 0 | 0 | 0 | 0 | 2 | 1 | 3 |
| spreadsheets | 5 | 0 | 4 | 0 | 0 | 6 | 3 | 3 |
| submissions | 0 | 0 | 0 | 0 | 0 | 13 | 1 | 4 |
| support | 4 | 0 | 2 | 2 | 37 | 11 | 3 | 37 |
| threads | 4 | 0 | 3 | 0 | 1 | 21 | 1 | 6 |
| tickets | 0 | 0 | 0 | 0 | 0 | 4 | 5 | 3 |
| workjet | 1 | 0 | 0 | 0 | 1 | 4 | 0 | 4 |

## Legacy-Rest je App


## Capabilities gesamt (41)

| Capability | Apps |
|---|---|
| `agent.flow.record` | ctox |
| `agent.queue.control` | ctox |
| `agent.queue.task.for_command` | ctox, research |
| `agent.queue.task.get` | ctox, research |
| `agent.task.cancel` | outbound-lead-generation |
| `agent.task.enqueue` | ctox, cv-print-builder, documents, iot, knowledge, kundenpipeline, news, outbound, outbound-lead-generation, research, spreadsheets, support |
| `agent.task.find` | outbound-lead-generation |
| `agent.task.get` | ctox, cv-print-builder, documents, iot, knowledge, kundenpipeline, news, outbound, outbound-lead-generation, research, spreadsheets, support |
| `agent.task.set_metadata` | outbound-lead-generation |
| `agent.task.update` | outbound-lead-generation |
| `agent.task.workspace.read` | documents, knowledge, outbound-lead-generation, research |
| `artifact.write` | outbound |
| `audit.event` | outbound, threads |
| `blob.materialize` | mail |
| `browser.auth_assist.request` | outbound, outbound-lead-generation |
| `browser.automation.run` | outbound |
| `command.get` | outbound-lead-generation |
| `dataframe.window` | documents, knowledge, research |
| `directory.user` | threads |
| `doc.extract` | cv-print-builder, matching |
| `doc.html` | matching |
| `fence.claim` | outbound-lead-generation |
| `fence.release` | outbound-lead-generation |
| `files.get` | outbound, outbound-lead-generation |
| `files.put` | outbound-lead-generation |
| `identity.users.list` | kundenpipeline |
| `knowledge.query` | iot, outbound, research |
| `mail.delivery_log.query` | outbound |
| `mail.send` | kundenpipeline, outbound-lead-generation |
| `mail.server.config.get` | kundenpipeline |
| `model` | iot |
| `net.ingest.webhook.register` | iot, outbound |
| `office.edit.apply` | documents, spreadsheets |
| `office.edit.prepare` | documents, mail, spreadsheets |
| `office.render` | mail |
| `policy.module_decision` | threads |
| `process.run` | outbound-lead-generation |
| `secret.exists` | outbound-lead-generation |
| `secret.scope` | outbound-lead-generation |
| `secrets.exists` | outbound |
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
| outbound | handlers/outbound.tracking.register.js | every PT1H |
| outbound-lead-generation | handlers/outbound.contacts.email_validation_sweep.js | every PT1H |
| support | handlers/support.intake.project.js | on communication_messages insert |
| support | handlers/support.intake.project.js | on communication_messages update |

## App-übergreifende Befehle (§16.40)

| Absender | Ziel-App | Typ | Beleg |
|---|---|---|---|
| buchhaltung | ctox | `ctox.app.modify.request` | index.js:4389 |
| buchhaltung | ctox | `ctox.chat.task` | index.js:4389 |
| desktop | ctox | `ctox.app.modify.request` | index.js:469 |
| desktop | ctox | `ctox.chat.task` | index.js:469 |
| documents | ctox | `ctox.app.modify.request` | index.js:658 |
| documents | ctox | `ctox.chat.task` | index.js:658 |
| knowledge | ctox | `ctox.app.modify.request` | index.js:2857 |
| knowledge | ctox | `ctox.chat.task` | index.js:2857 |
| mail | ctox | `ctox.chat.task` | index.js:2563 |
| mail | outbound | `outbound.message.compose` | index.js:1259 |
| matching | ctox | `ctox.app.modify.request` | ui/businessOsControls.js:1175 |
| matching | ctox | `ctox.chat.task` | ui/businessOsControls.js:1175 |
| outbound | outbound-lead-generation | `outbound.scrape.script.read` | handlers/_lib/olg-registry.js:167, v4-manifest.json; handlers/_lib/olg-registry.js requestLatestScript/requestTargetUpsert; customer-modules/outbound-lead-generation/OLG725.md (§16.40, §16.47 Nr. 4) |
| outbound | outbound-lead-generation | `outbound.scrape.target.upsert` | handlers/_lib/olg-registry.js:73, v4-manifest.json; handlers/_lib/olg-registry.js requestLatestScript/requestTargetUpsert; customer-modules/outbound-lead-generation/OLG725.md (§16.40, §16.47 Nr. 4) |
| research | ctox | `ctox.app.modify.request` | index.js:4916 |
| research | ctox | `ctox.chat.task` | index.js:4916 |
| research | documents | `research.systematic.report.create` | index.js:2551, index.js:2552 |
| research | knowledge | `knowledge.book.ingest` | handlers/_lib/callback.js:212 |
| spreadsheets | ctox | `ctox.app.modify.request` | index.js:2225 |
| spreadsheets | ctox | `ctox.chat.task` | index.js:2225 |

| Empfänger | Typ | from |
|---|---|---|
| ctox | `ctox.app.modify.request` | * |
| ctox | `ctox.chat.task` | * |
| documents | `research.systematic.report.create` | research |
| knowledge | `knowledge.book.ingest` | research |
| outbound | `outbound.message.compose` | mail |
| outbound-lead-generation | `outbound.scrape.script.read` | outbound |
| outbound-lead-generation | `outbound.scrape.target.upsert` | outbound |

Ohne Gegenstück (0, Soll 0):

Nicht auflösbares Ziel (1):
- outbound handlers/_lib/owner-app.js:22 type=`command.type` module=`OWNER_APP`

Shell (SHELL_COMMANDS_OUT + UI-Scan v4/frame, v4/shell, v4/source):
- shell → ctox `ctox.chat.task`: v4/shell/app-host.js:500 (Menüpunkt ctox.ask „CTOX fragen…“ → Eingabeblatt → Standardbefehl ctox.chat.task)

### commands_in aus der UI-Nutzung abgeleitet (E40, je Absender, nie '*') (1)

- outbound ← mail `outbound.message.compose`: modules/mail/index.js:1259

### UI-Befehle ohne Route: weder Plattformroute (src/core/business_os/business_command_inventory.json + Registry) noch Handler (E40, 0, Soll 0, rot)

keine

### Mehrdeutig: Handler in mehreren Apps (0, Soll 0, rot)

keine

### An den Kern gemeldet, noch keine Plattformroute (PLATFORM_ROUTES_PENDING, 13)

- `browser.http_auth.respond` (V2-gleich ohne Handler, native Browser-Laufzeit-Handler folgen nach M7 (E44.1, CAP)): im UI-Scan keine Fundstelle als Befehlstyp
- `browser.observer.grant` (V2-gleich ohne Handler, native Browser-Laufzeit-Handler folgen nach M7 (E44.1, CAP)): im UI-Scan keine Fundstelle als Befehlstyp
- `browser.observer.revoke` (V2-gleich ohne Handler, native Browser-Laufzeit-Handler folgen nach M7 (E44.1, CAP)): im UI-Scan keine Fundstelle als Befehlstyp
- `browser.upload.select` (V2-gleich ohne Handler, native Browser-Laufzeit-Handler folgen nach M7 (E44.1, CAP)): im UI-Scan keine Fundstelle als Befehlstyp
- `browser.webauthn.respond` (V2-gleich ohne Handler, native Browser-Laufzeit-Handler folgen nach M7 (E44.1, CAP)): im UI-Scan keine Fundstelle als Befehlstyp
- `ctox.plan.step.retry` (E42: Registry-Route, sobald die E25-Routen von Transfer auf v4/integration liegen): im UI-Scan keine Fundstelle als Befehlstyp
- `ctox.workjet.calendar.connect.v1` (E42: Registry-Route, sobald die E25-Routen von Transfer auf v4/integration liegen): im UI-Scan keine Fundstelle als Befehlstyp
- `ctox.workjet.calendar.read.v1` (E42: Registry-Route, sobald die E25-Routen von Transfer auf v4/integration liegen): im UI-Scan keine Fundstelle als Befehlstyp
- `ctox.workjet.device.v1` (E42: Registry-Route, sobald die E25-Routen von Transfer auf v4/integration liegen): im UI-Scan keine Fundstelle als Befehlstyp
- `ctox.workjet.grok.v1` (E42: Registry-Route, sobald die E25-Routen von Transfer auf v4/integration liegen): im UI-Scan keine Fundstelle als Befehlstyp
- `ctox.workjet.jour_fixe.speech.v1` (E42: Registry-Route, sobald die E25-Routen von Transfer auf v4/integration liegen): im UI-Scan keine Fundstelle als Befehlstyp
- `ctox.workjet.speech.dictation.v1` (E42: Registry-Route, sobald die E25-Routen von Transfer auf v4/integration liegen): im UI-Scan keine Fundstelle als Befehlstyp
- `ctox.workjet.speech.settings.v1` (E42: Registry-Route, sobald die E25-Routen von Transfer auf v4/integration liegen): im UI-Scan keine Fundstelle als Befehlstyp

## foreign ohne passendes exports (0, Soll 0)

keine

## Plattformzugriffe auf exports beschränkt (0)


## Befehlstypen mit Handlern in mehreren Apps (0)


## Tote Handler: Typ ist Plattformroute der Kern-Inventur (E42.1, 282)

- `ctox.appsec.app.audit`: appsec-pentest handlers/ctox.appsec.app.audit.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.approval.grant`: appsec-pentest handlers/ctox.appsec.approval.grant.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.approval.request`: appsec-pentest handlers/ctox.appsec.approval.request.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.approval.revoke`: appsec-pentest handlers/ctox.appsec.approval.revoke.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.assessment.archive`: appsec-pentest handlers/ctox.appsec.assessment.archive.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.assessment.create`: appsec-pentest handlers/ctox.appsec.assessment.create.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.assessment.run`: appsec-pentest handlers/ctox.appsec.assessment.run.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.audit.run`: appsec-pentest handlers/ctox.appsec.audit.run.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.authz.build_matrix`: appsec-pentest handlers/ctox.appsec.authz.build_matrix.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.authz.credential_proof_from_evidence`: appsec-pentest handlers/ctox.appsec.authz.credential_proof_from_evidence.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.authz.credential_proof_template`: appsec-pentest handlers/ctox.appsec.authz.credential_proof_template.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.authz.plan`: appsec-pentest handlers/ctox.appsec.authz.plan.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.authz.preflight`: appsec-pentest handlers/ctox.appsec.authz.preflight.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.authz.run`: appsec-pentest handlers/ctox.appsec.authz.run.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.authz.status`: appsec-pentest handlers/ctox.appsec.authz.status.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.exploit.get`: appsec-pentest handlers/ctox.appsec.exploit.get.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.exploit.list`: appsec-pentest handlers/ctox.appsec.exploit.list.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.exploit.verify`: appsec-pentest handlers/ctox.appsec.exploit.verify.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.finding.remediate`: appsec-pentest handlers/ctox.appsec.finding.remediate.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.finding.remediate.complete`: appsec-pentest handlers/ctox.appsec.finding.remediate.complete.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.finding.remediate.failed`: appsec-pentest handlers/ctox.appsec.finding.remediate.failed.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.graph.build`: appsec-pentest handlers/ctox.appsec.graph.build.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.investigation.execute`: appsec-pentest handlers/ctox.appsec.investigation.execute.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.investigation.plan`: appsec-pentest handlers/ctox.appsec.investigation.plan.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.investigation.refute`: appsec-pentest handlers/ctox.appsec.investigation.refute.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.investigation.resolve`: appsec-pentest handlers/ctox.appsec.investigation.resolve.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.lab.create`: appsec-pentest handlers/ctox.appsec.lab.create.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.lab.run`: appsec-pentest handlers/ctox.appsec.lab.run.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.pipeline.rework`: appsec-pentest handlers/ctox.appsec.pipeline.rework.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.replay.baseline`: appsec-pentest handlers/ctox.appsec.replay.baseline.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.replay.investigations`: appsec-pentest handlers/ctox.appsec.replay.investigations.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.report.export`: appsec-pentest handlers/ctox.appsec.report.export.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.review`: appsec-pentest handlers/ctox.appsec.review.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.state.sync`: appsec-pentest handlers/ctox.appsec.state.sync.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.appsec.tools.doctor`: appsec-pentest handlers/ctox.appsec.tools.doctor.js – nicht im Manifest, der Typ geht an den Kern
- `browser.back`: browser handlers/browser.back.js – nicht im Manifest, der Typ geht an den Kern
- `browser.clipboard.clear`: browser handlers/browser.clipboard.clear.js – nicht im Manifest, der Typ geht an den Kern
- `browser.clipboard.copy`: browser handlers/browser.clipboard.copy.js – nicht im Manifest, der Typ geht an den Kern
- `browser.clipboard.paste`: browser handlers/browser.clipboard.paste.js – nicht im Manifest, der Typ geht an den Kern
- `browser.controller.acquire`: browser handlers/browser.controller.acquire.js – nicht im Manifest, der Typ geht an den Kern
- `browser.controller.release`: browser handlers/browser.controller.release.js – nicht im Manifest, der Typ geht an den Kern
- `browser.controller.renew`: browser handlers/browser.controller.renew.js – nicht im Manifest, der Typ geht an den Kern
- `browser.credential.fill`: browser handlers/browser.credential.fill.js – nicht im Manifest, der Typ geht an den Kern
- `browser.dialog.respond`: browser handlers/browser.dialog.respond.js – nicht im Manifest, der Typ geht an den Kern
- `browser.forward`: browser handlers/browser.forward.js – nicht im Manifest, der Typ geht an den Kern
- `browser.navigate`: browser handlers/browser.navigate.js – nicht im Manifest, der Typ geht an den Kern
- `browser.permission.respond`: browser handlers/browser.permission.respond.js – nicht im Manifest, der Typ geht an den Kern
- `browser.reload`: browser handlers/browser.reload.js – nicht im Manifest, der Typ geht an den Kern
- `browser.reset`: browser handlers/browser.reset.js – nicht im Manifest, der Typ geht an den Kern
- `browser.session.start`: browser handlers/browser.session.start.js – nicht im Manifest, der Typ geht an den Kern
- `browser.session.stop`: browser handlers/browser.session.stop.js – nicht im Manifest, der Typ geht an den Kern
- `browser.tab.activate`: browser handlers/browser.tab.activate.js – nicht im Manifest, der Typ geht an den Kern
- `browser.tab.close`: browser handlers/browser.tab.close.js – nicht im Manifest, der Typ geht an den Kern
- `browser.tab.open`: browser handlers/browser.tab.open.js – nicht im Manifest, der Typ geht an den Kern
- `web_stack.auth_assist.complete`: browser handlers/web_stack.auth_assist.complete.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.coding.cancel`: coding-agents handlers/ctox.coding.cancel.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.coding.models`: coding-agents handlers/ctox.coding.models.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.coding.turn`: coding-agents handlers/ctox.coding.turn.js – nicht im Manifest, der Typ geht an den Kern
- `ats.consent.check`: consent handlers/ats.consent.check.js – nicht im Manifest, der Typ geht an den Kern
- `ats.retention.due`: consent handlers/ats.retention.due.js – nicht im Manifest, der Typ geht an den Kern
- `ats.retention.purge`: consent handlers/ats.retention.purge.js – nicht im Manifest, der Typ geht an den Kern
- `ats.subject.erase`: consent handlers/ats.subject.erase.js – nicht im Manifest, der Typ geht an den Kern
- `ats.subject.export`: consent handlers/ats.subject.export.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.secret.delete`: credentials handlers/ctox.secret.delete.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.secret.list`: credentials handlers/ctox.secret.list.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.secret.put`: credentials handlers/ctox.secret.put.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.crew.assign`: ctox handlers/ctox.crew.assign.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.crew.learning.confirm`: ctox handlers/ctox.crew.learning.confirm.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.crew.learning.delete`: ctox handlers/ctox.crew.learning.delete.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.crew.learning.update`: ctox handlers/ctox.crew.learning.update.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.crew.member.create`: ctox handlers/ctox.crew.member.create.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.crew.member.update`: ctox handlers/ctox.crew.member.update.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.crew.memory.update`: ctox handlers/ctox.crew.memory.update.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.queue.abort_turn`: ctox handlers/ctox.queue.abort_turn.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.queue.block`: ctox handlers/ctox.queue.block.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.queue.capacity`: ctox handlers/ctox.queue.capacity.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.queue.pause`: ctox handlers/ctox.queue.pause.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.queue.release`: ctox handlers/ctox.queue.release.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.queue.retry`: ctox handlers/ctox.queue.retry.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.task.delete`: ctox handlers/ctox.task.delete.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.task.update`: ctox handlers/ctox.task.update.js – nicht im Manifest, der Typ geht an den Kern
- `customers.account.archive`: customers handlers/customers.account.archive.js – nicht im Manifest, der Typ geht an den Kern
- `customers.account.create`: customers handlers/customers.account.create.js – nicht im Manifest, der Typ geht an den Kern
- `customers.account.update`: customers handlers/customers.account.update.js – nicht im Manifest, der Typ geht an den Kern
- `customers.activity.record`: customers handlers/customers.activity.record.js – nicht im Manifest, der Typ geht an den Kern
- `customers.contact.archive`: customers handlers/customers.contact.archive.js – nicht im Manifest, der Typ geht an den Kern
- `customers.contact.create`: customers handlers/customers.contact.create.js – nicht im Manifest, der Typ geht an den Kern
- `customers.contact.update`: customers handlers/customers.contact.update.js – nicht im Manifest, der Typ geht an den Kern
- `customers.dedupe.resolve`: customers handlers/customers.dedupe.resolve.js – nicht im Manifest, der Typ geht an den Kern
- `customers.import.from_outbound`: customers handlers/customers.import.from_outbound.js – nicht im Manifest, der Typ geht an den Kern
- `customers.note.create`: customers handlers/customers.note.create.js – nicht im Manifest, der Typ geht an den Kern
- `customers.note.update`: customers handlers/customers.note.update.js – nicht im Manifest, der Typ geht an den Kern
- `customers.opportunity.close_lost`: customers handlers/customers.opportunity.close_lost.js – nicht im Manifest, der Typ geht an den Kern
- `customers.opportunity.close_won`: customers handlers/customers.opportunity.close_won.js – nicht im Manifest, der Typ geht an den Kern
- `customers.opportunity.create`: customers handlers/customers.opportunity.create.js – nicht im Manifest, der Typ geht an den Kern
- `customers.opportunity.move_stage`: customers handlers/customers.opportunity.move_stage.js – nicht im Manifest, der Typ geht an den Kern
- `customers.opportunity.update`: customers handlers/customers.opportunity.update.js – nicht im Manifest, der Typ geht an den Kern
- `customers.task.complete`: customers handlers/customers.task.complete.js – nicht im Manifest, der Typ geht an den Kern
- `customers.task.create`: customers handlers/customers.task.create.js – nicht im Manifest, der Typ geht an den Kern
- `customers.task.update`: customers handlers/customers.task.update.js – nicht im Manifest, der Typ geht an den Kern
- `customers.view.save`: customers handlers/customers.view.save.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.office.settings.save`: documents handlers/ctox.office.settings.save.js – nicht im Manifest, der Typ geht an den Kern
- `office.document.commit`: documents handlers/office.document.commit.js – nicht im Manifest, der Typ geht an den Kern
- `office.document.export`: documents handlers/office.document.export.js – nicht im Manifest, der Typ geht an den Kern
- `office.document.freeze_email_content`: documents handlers/office.document.freeze_email_content.js – nicht im Manifest, der Typ geht an den Kern
- `office.document.prepare`: documents handlers/office.document.prepare.js – nicht im Manifest, der Typ geht an den Kern
- `ats.signature.request`: esign handlers/ats.signature.request.js – nicht im Manifest, der Typ geht an den Kern
- `ats.signature.sign`: esign handlers/ats.signature.sign.js – nicht im Manifest, der Typ geht an den Kern
- `ats.intake.capture`: intake handlers/ats.intake.capture.js – nicht im Manifest, der Typ geht an den Kern
- `ats.interview.transcribe`: interviews handlers/ats.interview.transcribe.js – nicht im Manifest, der Typ geht an den Kern
- `invoices.dunning.letter.send`: invoices handlers/invoices.dunning.letter.send.js – nicht im Manifest, der Typ geht an den Kern
- `invoices.dunning.run`: invoices handlers/invoices.dunning.run.js – nicht im Manifest, der Typ geht an den Kern
- `invoices.import.from_outbound`: invoices handlers/invoices.import.from_outbound.js – nicht im Manifest, der Typ geht an den Kern
- `invoices.invoice.assign_payment_terms`: invoices handlers/invoices.invoice.assign_payment_terms.js – nicht im Manifest, der Typ geht an den Kern
- `invoices.invoice.cancel`: invoices handlers/invoices.invoice.cancel.js – nicht im Manifest, der Typ geht an den Kern
- `invoices.invoice.create`: invoices handlers/invoices.invoice.create.js – nicht im Manifest, der Typ geht an den Kern
- `invoices.invoice.create_credit_note`: invoices handlers/invoices.invoice.create_credit_note.js – nicht im Manifest, der Typ geht an den Kern
- `invoices.invoice.delete`: invoices handlers/invoices.invoice.delete.js – nicht im Manifest, der Typ geht an den Kern
- `invoices.invoice.post`: invoices handlers/invoices.invoice.post.js – nicht im Manifest, der Typ geht an den Kern
- `invoices.invoice.update`: invoices handlers/invoices.invoice.update.js – nicht im Manifest, der Typ geht an den Kern
- `invoices.line.create`: invoices handlers/invoices.line.create.js – nicht im Manifest, der Typ geht an den Kern
- `invoices.line.delete`: invoices handlers/invoices.line.delete.js – nicht im Manifest, der Typ geht an den Kern
- `invoices.line.update`: invoices handlers/invoices.line.update.js – nicht im Manifest, der Typ geht an den Kern
- `invoices.payment.allocate`: invoices handlers/invoices.payment.allocate.js – nicht im Manifest, der Typ geht an den Kern
- `invoices.payment.match_suggestions`: invoices handlers/invoices.payment.match_suggestions.js – nicht im Manifest, der Typ geht an den Kern
- `invoices.payment.unallocate`: invoices handlers/invoices.payment.unallocate.js – nicht im Manifest, der Typ geht an den Kern
- `invoices.proposal.approve`: invoices handlers/invoices.proposal.approve.js – nicht im Manifest, der Typ geht an den Kern
- `invoices.proposal.create`: invoices handlers/invoices.proposal.create.js – nicht im Manifest, der Typ geht an den Kern
- `invoices.proposal.reject`: invoices handlers/invoices.proposal.reject.js – nicht im Manifest, der Typ geht an den Kern
- `invoices.recurring.create`: invoices handlers/invoices.recurring.create.js – nicht im Manifest, der Typ geht an den Kern
- `invoices.recurring.pause`: invoices handlers/invoices.recurring.pause.js – nicht im Manifest, der Typ geht an den Kern
- `invoices.recurring.run`: invoices handlers/invoices.recurring.run.js – nicht im Manifest, der Typ geht an den Kern
- `invoices.recurring.update`: invoices handlers/invoices.recurring.update.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.iot.agent.configure`: iot handlers/ctox.iot.agent.configure.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.iot.alarm.update`: iot handlers/ctox.iot.alarm.update.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.iot.asset.delete`: iot handlers/ctox.iot.asset.delete.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.iot.asset.upsert`: iot handlers/ctox.iot.asset.upsert.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.iot.attribute.write`: iot handlers/ctox.iot.attribute.write.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.iot.datapoints.query`: iot handlers/ctox.iot.datapoints.query.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.iot.ruleset.save`: iot handlers/ctox.iot.ruleset.save.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.iot.ruleset.toggle`: iot handlers/ctox.iot.ruleset.toggle.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.mailserver.delete_domain`: mail handlers/ctox.mailserver.delete_domain.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.mailserver.delete_user`: mail handlers/ctox.mailserver.delete_user.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.mailserver.get_config`: mail handlers/ctox.mailserver.get_config.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.mailserver.save_domain`: mail handlers/ctox.mailserver.save_domain.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.mailserver.save_runtime`: mail handlers/ctox.mailserver.save_runtime.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.mailserver.save_user`: mail handlers/ctox.mailserver.save_user.js – nicht im Manifest, der Typ geht an den Kern
- `ats.credential.verify`: nachweise handlers/ats.credential.verify.js – nicht im Manifest, der Typ geht an den Kern
- `ats.deployment.check`: nachweise handlers/ats.deployment.check.js – nicht im Manifest, der Typ geht an den Kern
- `ats.leistungsnachweis.signoff`: nachweise handlers/ats.leistungsnachweis.signoff.js – nicht im Manifest, der Typ geht an den Kern
- `news.source.auth_assist`: news handlers/news.source.auth_assist.js – nicht im Manifest, der Typ geht an den Kern
- `news.topic.schedule`: news handlers/news.topic.schedule.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.audit.export`: outbound handlers/outbound.audit.export.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.campaign.apply_setup`: outbound handlers/outbound.campaign.apply_setup.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.campaign.briefing.update`: outbound handlers/outbound.campaign.briefing.update.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.campaign.mailbox.link`: outbound handlers/outbound.campaign.mailbox.link.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.campaign.status.set`: outbound handlers/outbound.campaign.status.set.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.dev.seed_test_data`: outbound handlers/outbound.dev.seed_test_data.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.draft.prepare`: outbound handlers/outbound.draft.prepare.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.engagement.assign_sender`: outbound handlers/outbound.engagement.assign_sender.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.engagement.close`: outbound handlers/outbound.engagement.close.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.engagement.create`: outbound handlers/outbound.engagement.create.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.engagement.reapply_sequence`: outbound handlers/outbound.engagement.reapply_sequence.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.engagement.resume`: outbound handlers/outbound.engagement.resume.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.letter_template.save`: outbound handlers/outbound.letter_template.save.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.message.approve`: outbound handlers/outbound.message.approve.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.message.cancel`: outbound handlers/outbound.message.cancel.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.message.pause`: outbound handlers/outbound.message.pause.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.message.prepare`: outbound handlers/outbound.message.prepare.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.message.reject`: outbound handlers/outbound.message.reject.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.message.request_approval`: outbound handlers/outbound.message.request_approval.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.message.request_changes`: outbound handlers/outbound.message.request_changes.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.message.resume`: outbound handlers/outbound.message.resume.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.message.send_approved`: outbound handlers/outbound.message.send_approved.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.message.update_draft`: outbound handlers/outbound.message.update_draft.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.pipeline.write_outreach_draft`: outbound handlers/outbound.pipeline.write_outreach_draft.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.provider.reconcile`: outbound handlers/outbound.provider.reconcile.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.reply.classify`: outbound handlers/outbound.reply.classify.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.reply.match`: outbound handlers/outbound.reply.match.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.research_policy.publish`: outbound handlers/outbound.research_policy.publish.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.research_source.auth_assist`: outbound handlers/outbound.research_source.auth_assist.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.research_source.generate_adapter`: outbound handlers/outbound.research_source.generate_adapter.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.research_source.registry_read`: outbound handlers/outbound.research_source.registry_read.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.research_source.reprobe`: outbound handlers/outbound.research_source.reprobe.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.research_source.test`: outbound handlers/outbound.research_source.test.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.research_source.upsert`: outbound handlers/outbound.research_source.upsert.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.scheduler.tick`: outbound handlers/outbound.scheduler.tick.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.scheduling.mark_booked`: outbound handlers/outbound.scheduling.mark_booked.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.scheduling.prepare`: outbound handlers/outbound.scheduling.prepare.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.scheduling.update_slots`: outbound handlers/outbound.scheduling.update_slots.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.sellify.lookup`: outbound handlers/outbound.sellify.lookup.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.sequence.save`: outbound handlers/outbound.sequence.save.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.skillbook.save`: outbound handlers/outbound.skillbook.save.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.skillbook.seed_defaults`: outbound handlers/outbound.skillbook.seed_defaults.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.update_digest.send_now`: outbound handlers/outbound.update_digest.send_now.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.lead.research_writeback`: outbound-lead-generation handlers/outbound.lead.research_writeback.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.research_policy.publish`: outbound-lead-generation handlers/outbound.research_policy.publish.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.research_source.auth_assist`: outbound-lead-generation handlers/outbound.research_source.auth_assist.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.research_source.generate_adapter`: outbound-lead-generation handlers/outbound.research_source.generate_adapter.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.research_source.reprobe`: outbound-lead-generation handlers/outbound.research_source.reprobe.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.research_source.test`: outbound-lead-generation handlers/outbound.research_source.test.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.research_source.upsert`: outbound-lead-generation handlers/outbound.research_source.upsert.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.sellify.lookup`: outbound-lead-generation handlers/outbound.sellify.lookup.js – nicht im Manifest, der Typ geht an den Kern
- `outbound.update_digest.send_now`: outbound-lead-generation handlers/outbound.update_digest.send_now.js – nicht im Manifest, der Typ geht an den Kern
- `web_stack.person_research`: outbound-lead-generation handlers/web_stack.person_research.js – nicht im Manifest, der Typ geht an den Kern
- `ats.placement.create`: placements handlers/ats.placement.create.js – nicht im Manifest, der Typ geht an den Kern
- `ats.placement.early_leave`: placements handlers/ats.placement.early_leave.js – nicht im Manifest, der Typ geht an den Kern
- `office.spreadsheet.commit`: spreadsheets handlers/office.spreadsheet.commit.js – nicht im Manifest, der Typ geht an den Kern
- `office.spreadsheet.export`: spreadsheets handlers/office.spreadsheet.export.js – nicht im Manifest, der Typ geht an den Kern
- `office.spreadsheet.prepare`: spreadsheets handlers/office.spreadsheet.prepare.js – nicht im Manifest, der Typ geht an den Kern
- `ats.submission.present`: submissions handlers/ats.submission.present.js – nicht im Manifest, der Typ geht an den Kern
- `support.agent.apply_suggestion`: support handlers/support.agent.apply_suggestion.js – nicht im Manifest, der Typ geht an den Kern
- `support.agent.reject_suggestion`: support handlers/support.agent.reject_suggestion.js – nicht im Manifest, der Typ geht an den Kern
- `support.agent.writeback`: support handlers/support.agent.writeback.js – nicht im Manifest, der Typ geht an den Kern
- `support.assignment_policy.upsert`: support handlers/support.assignment_policy.upsert.js – nicht im Manifest, der Typ geht an den Kern
- `support.automation_rule.upsert`: support handlers/support.automation_rule.upsert.js – nicht im Manifest, der Typ geht an den Kern
- `support.automation.evaluate`: support handlers/support.automation.evaluate.js – nicht im Manifest, der Typ geht an den Kern
- `support.bulk.assign`: support handlers/support.bulk.assign.js – nicht im Manifest, der Typ geht an den Kern
- `support.bulk.priority`: support handlers/support.bulk.priority.js – nicht im Manifest, der Typ geht an den Kern
- `support.bulk.resolve`: support handlers/support.bulk.resolve.js – nicht im Manifest, der Typ geht an den Kern
- `support.bulk.snooze`: support handlers/support.bulk.snooze.js – nicht im Manifest, der Typ geht an den Kern
- `support.bulk.status`: support handlers/support.bulk.status.js – nicht im Manifest, der Typ geht an den Kern
- `support.conversation.assign`: support handlers/support.conversation.assign.js – nicht im Manifest, der Typ geht an den Kern
- `support.conversation.claim`: support handlers/support.conversation.claim.js – nicht im Manifest, der Typ geht an den Kern
- `support.conversation.open_from_thread`: support handlers/support.conversation.open_from_thread.js – nicht im Manifest, der Typ geht an den Kern
- `support.conversation.priority`: support handlers/support.conversation.priority.js – nicht im Manifest, der Typ geht an den Kern
- `support.conversation.reopen`: support handlers/support.conversation.reopen.js – nicht im Manifest, der Typ geht an den Kern
- `support.conversation.resolve`: support handlers/support.conversation.resolve.js – nicht im Manifest, der Typ geht an den Kern
- `support.conversation.snooze`: support handlers/support.conversation.snooze.js – nicht im Manifest, der Typ geht an den Kern
- `support.conversation.status`: support handlers/support.conversation.status.js – nicht im Manifest, der Typ geht an den Kern
- `support.identity.link`: support handlers/support.identity.link.js – nicht im Manifest, der Typ geht an den Kern
- `support.inbox.upsert`: support handlers/support.inbox.upsert.js – nicht im Manifest, der Typ geht an den Kern
- `support.macro.run`: support handlers/support.macro.run.js – nicht im Manifest, der Typ geht an den Kern
- `support.macro.upsert`: support handlers/support.macro.upsert.js – nicht im Manifest, der Typ geht an den Kern
- `support.note.create`: support handlers/support.note.create.js – nicht im Manifest, der Typ geht an den Kern
- `support.reply.draft`: support handlers/support.reply.draft.js – nicht im Manifest, der Typ geht an den Kern
- `support.reply.send`: support handlers/support.reply.send.js – nicht im Manifest, der Typ geht an den Kern
- `support.reporting.rebuild_rollups`: support handlers/support.reporting.rebuild_rollups.js – nicht im Manifest, der Typ geht an den Kern
- `support.sla_policy.upsert`: support handlers/support.sla_policy.upsert.js – nicht im Manifest, der Typ geht an den Kern
- `support.sla.apply`: support handlers/support.sla.apply.js – nicht im Manifest, der Typ geht an den Kern
- `support.sla.recalculate`: support handlers/support.sla.recalculate.js – nicht im Manifest, der Typ geht an den Kern
- `support.ticket.create_from_conversation`: support handlers/support.ticket.create_from_conversation.js – nicht im Manifest, der Typ geht an den Kern
- `support.ticket.link`: support handlers/support.ticket.link.js – nicht im Manifest, der Typ geht an den Kern
- `support.view_filter.upsert`: support handlers/support.view_filter.upsert.js – nicht im Manifest, der Typ geht an den Kern
- `support.view.upsert`: support handlers/support.view.upsert.js – nicht im Manifest, der Typ geht an den Kern
- `threads.ai.request`: threads handlers/threads.ai.request.js – nicht im Manifest, der Typ geht an den Kern
- `threads.ctox_approval.approve`: threads handlers/threads.ctox_approval.approve.js – nicht im Manifest, der Typ geht an den Kern
- `threads.ctox_approval.cancel`: threads handlers/threads.ctox_approval.cancel.js – nicht im Manifest, der Typ geht an den Kern
- `threads.ctox_approval.edit`: threads handlers/threads.ctox_approval.edit.js – nicht im Manifest, der Typ geht an den Kern
- `threads.ctox_approval.reject`: threads handlers/threads.ctox_approval.reject.js – nicht im Manifest, der Typ geht an den Kern
- `threads.ctox_approval.request`: threads handlers/threads.ctox_approval.request.js – nicht im Manifest, der Typ geht an den Kern
- `threads.handoff.create`: threads handlers/threads.handoff.create.js – nicht im Manifest, der Typ geht an den Kern
- `threads.link.create`: threads handlers/threads.link.create.js – nicht im Manifest, der Typ geht an den Kern
- `threads.link.remove`: threads handlers/threads.link.remove.js – nicht im Manifest, der Typ geht an den Kern
- `threads.message.create`: threads handlers/threads.message.create.js – nicht im Manifest, der Typ geht an den Kern
- `threads.note.create`: threads handlers/threads.note.create.js – nicht im Manifest, der Typ geht an den Kern
- `threads.note.delete`: threads handlers/threads.note.delete.js – nicht im Manifest, der Typ geht an den Kern
- `threads.note.update`: threads handlers/threads.note.update.js – nicht im Manifest, der Typ geht an den Kern
- `threads.notification.dismiss`: threads handlers/threads.notification.dismiss.js – nicht im Manifest, der Typ geht an den Kern
- `threads.notification.mark_read`: threads handlers/threads.notification.mark_read.js – nicht im Manifest, der Typ geht an den Kern
- `threads.notification.preferences.update`: threads handlers/threads.notification.preferences.update.js – nicht im Manifest, der Typ geht an den Kern
- `threads.thread.archive`: threads handlers/threads.thread.archive.js – nicht im Manifest, der Typ geht an den Kern
- `threads.thread.assign`: threads handlers/threads.thread.assign.js – nicht im Manifest, der Typ geht an den Kern
- `threads.thread.claim`: threads handlers/threads.thread.claim.js – nicht im Manifest, der Typ geht an den Kern
- `threads.thread.snooze`: threads handlers/threads.thread.snooze.js – nicht im Manifest, der Typ geht an den Kern
- `threads.thread.state.update`: threads handlers/threads.thread.state.update.js – nicht im Manifest, der Typ geht an den Kern
- `threads.thread.unwatch`: threads handlers/threads.thread.unwatch.js – nicht im Manifest, der Typ geht an den Kern
- `threads.thread.watch`: threads handlers/threads.thread.watch.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.ticket.approve`: tickets handlers/ctox.ticket.approve.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.ticket.close`: tickets handlers/ctox.ticket.close.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.ticket.execute`: tickets handlers/ctox.ticket.execute.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.ticket.local.comment`: tickets handlers/ctox.ticket.local.comment.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.ticket.local.create`: tickets handlers/ctox.ticket.local.create.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.ticket.local.transition`: tickets handlers/ctox.ticket.local.transition.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.ticket.publish_clarification`: tickets handlers/ctox.ticket.publish_clarification.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.ticket.request_clarification`: tickets handlers/ctox.ticket.request_clarification.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.ticket.resolve_clarification`: tickets handlers/ctox.ticket.resolve_clarification.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.ticket.verify`: tickets handlers/ctox.ticket.verify.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.ticket.writeback_comment`: tickets handlers/ctox.ticket.writeback_comment.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.ticket.writeback_transition`: tickets handlers/ctox.ticket.writeback_transition.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.ticket.writeback.complete`: tickets handlers/ctox.ticket.writeback.complete.js – nicht im Manifest, der Typ geht an den Kern
- `ctox.ticket.writeback.failed`: tickets handlers/ctox.ticket.writeback.failed.js – nicht im Manifest, der Typ geht an den Kern

## Offene _todo (app-spezifisch; die drei Standardpunkte rights/client_limit/app_ver stehen in jedem Manifest)

### consent

- command_rights ats.*: kein passender Handler.

### conversations

- schema.js ist noch Teil der Dateien; entfällt beim Port (§16.13).

### credentials

- command_rights ctox.secret.*: kein passender Handler.

### ctox

- command_rights ctox.crew.*: kein passender Handler.
- command_rights ctox.queue.*: kein passender Handler.
- command_rights ctox.task.*: kein passender Handler.
- schema.js ist noch Teil der Dateien; entfällt beim Port (§16.13).

### esign

- command_rights ats.*: kein passender Handler.

### intake

- command_rights ats.*: kein passender Handler.

### interviews

- command_rights ats.*: kein passender Handler.

### nachweise

- command_rights ats.*: kein passender Handler.

### outbound

- Trigger-Handler handlers/outbound.scheduler.tick.js fehlt unter handlers/ (v4-manifest.json).
- schema.js ist noch Teil der Dateien; entfällt beim Port (§16.13).
- 1 ctx.command-Aufruf(e) mit nicht auflösbarem Ziel (handlers/_lib/owner-app.js:22): Ziel-App/Typ zur Laufzeit bestimmt – commands_out in v4-manifest.json deklarieren oder Ziel festlegen (§16.40).

### outbound-lead-generation

- module.json.entry zeigt auf local-modules/…; Kundeninstanzen tragen ggf. eine abweichende installierte module.json (~/.local/state/ctox-sync-v4/docs/thesen-olg-installiert-20261015/module.json) – vor Aktivierung abgleichen.
- Abnahme Nicht-Admin (V4-PORT.md §13) legt die Mindestrechte fest; Logo (layout.icon_svg) muss erhalten bleiben.

### placements

- command_rights ats.*: kein passender Handler.

### shell

- files: Pfade relativ zu src/apps/business-os/; lab/-Prüfstände, Tests und .md sind ausgenommen.
- app.js/app.css/index.html und die übrigen shared-Dateien stehen laut Dateigrenze im Shell-Artefakt; Shell v4 ersetzt app.js (wird am Ende gelöscht) – Liste beim Rückbau kürzen.

### submissions

- command_rights ats.*: kein passender Handler.

### support

- command_rights support.inbox.upsert (support.manage_inboxes): kein Handler-Datei.
- command_rights support.conversation.open_from_thread (support.triage): kein Handler-Datei.
- command_rights support.conversation.claim (support.triage): kein Handler-Datei.
- command_rights support.conversation.assign (support.assign): kein Handler-Datei.
- command_rights support.conversation.status (support.triage): kein Handler-Datei.
- command_rights support.conversation.priority (support.triage): kein Handler-Datei.
- command_rights support.conversation.snooze (support.triage): kein Handler-Datei.
- command_rights support.conversation.resolve (support.resolve): kein Handler-Datei.
- command_rights support.conversation.reopen (support.triage): kein Handler-Datei.
- command_rights support.identity.link (support.triage): kein Handler-Datei.
- command_rights support.note.create (support.triage): kein Handler-Datei.
- command_rights support.ticket.link (support.triage): kein Handler-Datei.
- command_rights support.ticket.create_from_conversation (support.triage): kein Handler-Datei.
- command_rights support.reply.draft (support.reply): kein Handler-Datei.
- command_rights support.reply.send (support.reply): kein Handler-Datei.
- command_rights support.view.upsert (support.triage): kein Handler-Datei.
- command_rights support.view_filter.upsert (support.triage): kein Handler-Datei.
- command_rights support.bulk.assign (support.assign): kein Handler-Datei.
- command_rights support.bulk.status (support.triage): kein Handler-Datei.
- command_rights support.bulk.priority (support.triage): kein Handler-Datei.
- command_rights support.bulk.snooze (support.triage): kein Handler-Datei.
- command_rights support.bulk.resolve (support.resolve): kein Handler-Datei.
- command_rights support.assignment_policy.upsert (support.assign): kein Handler-Datei.
- command_rights support.macro.upsert (support.manage_macros): kein Handler-Datei.
- command_rights support.macro.run (support.triage): kein Handler-Datei.
- command_rights support.automation_rule.upsert (support.manage_inboxes): kein Handler-Datei.
- command_rights support.automation.evaluate (support.triage): kein Handler-Datei.
- command_rights support.sla_policy.upsert (support.manage_sla): kein Handler-Datei.
- command_rights support.sla.apply (support.manage_sla): kein Handler-Datei.
- command_rights support.sla.recalculate (support.manage_sla): kein Handler-Datei.
- command_rights support.reporting.rebuild_rollups (support.manage_inboxes): kein Handler-Datei.
- command_rights support.agent.writeback (support.agent_request): kein Handler-Datei.
- command_rights support.agent.apply_suggestion (support.agent_apply): kein Handler-Datei.
- command_rights support.agent.reject_suggestion (support.agent_apply): kein Handler-Datei.

### threads

- command_rights threads.ctox_approval.approve: kein passender Handler.
- command_rights threads.ctox_approval.reject: kein passender Handler.
- Projektions-Auslöser (W-4) noch nicht im Vertrag: siehe _proposed_triggers.

### workjet

- index.js exportiert kein mount(); Einstieg klären.

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
