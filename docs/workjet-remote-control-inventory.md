# Workjet vollständig per Agent fernsteuern: Befehlsinventar

Stand: 2026-10-08. Ziel: Ein Agent soll Workjet über den MCP-Server von CTOX bedienen können. Dieses Inventar beschreibt vorhandene Schnittstellen und mögliche Erweiterungen; es implementiert keine Werkzeuge und ist keine Abnahme der installierten Anwendung. Der native MCP-Abgleich gilt für CTOX-Main `450171de37ac60dddb3287225b711d6c3f235711`. Die Workjet-Protokollnamen stammen aus der ursprünglichen Bestandsaufnahme; deren vollständige Aktualität und installierte Erreichbarkeit sind hier nicht nachgewiesen.

Quellen:

- Workjet-Protokoll: `workjet/packages/contracts/src/rpc.ts` (`WS_METHODS`), `orchestration.ts` (Thread-Befehle), `project.ts`, `projectOverview.ts`, `settings.ts`
- CTOX-Befehlsebene: `src/core/business_os/command_plane.rs` und `store_workjet_*.rs`, Befehlstypen `ctox.workjet.*`
- MCP-Server: [`mcp_channel.rs`](../src/core/business_os/mcp_channel.rs) (`tool_descriptors()`), [`mcp_project_crew.rs`](../src/core/business_os/mcp_project_crew.rs), `mcp_workjet_*.rs`
- Autorisierung: [MCP-Sicherheitsvertrag](business-os-mcp-channel-v1-security-admin-guide.md); [native Worker-Dispatch-Verbindung](native-workjet-worker-dispatch.md)

Die Beschreibungen beziehen sich auf registrierten Quellcode. Welche Werkzeuge ein Client tatsächlich aufrufen darf, hängt zusätzlich von der installierten nativen Revision, der Gateway-Klassifikation und seinem aktuellen Grant ab. Eine Registrierung beweist weder Start noch Ergebnisrückgabe im Produkt.

Für den beauftragten Molecularity-Ablauf gelten feste Grenzen: Geheimnisse dürfen über diese MCP-Fernsteuerung nur als freigegebener Status erscheinen; Werte werden nicht gelesen, exportiert oder kopiert. Terminal-Input und Server-Updates sind aus diesem Auftrag ausgeschlossen. Ein zukünftiger Wrapper muss diese Grenzen nativ und am Gateway einhalten. Bestehende getrennt autorisierte Operator- und Secret-Store-Produktwege werden dadurch nicht erweitert oder aufgehoben.

## Kurzfassung

- Registriert sind unter anderem `business_os.start_project_task`, `business_os.cancel_project_task`, `business_os.start_crew_execution`, `business_os.remote_worker_admission`, `business_os.workjet_worker_dispatch`, `business_os.jour_fixe_read`, `business_os.jour_fixe_update` und `business_os.project_kpi`. Eine feste Gesamtzahl wird hier nicht behauptet; weitere Werkzeuge werden auch aus Modulaktionen erzeugt.
- Die Befehlsebene kennt weitere `ctox.workjet.*`-Befehle. Ein registrierter Domänen-Wrapper deckt nicht automatisch sämtliche Aktionen dieser Befehlsebene ab. Jour fixe ist teilweise angebunden; Projektaufträge können bereits direkt über MCP starten und abbrechen.
- Workjet-RPC und CTOX-MCP sind unterschiedliche Verträge. Ein RPC-Name in den Tabellen belegt keinen MCP-Aufruf. Die ursprünglichen Methodenzahlen sind für den aktuellen Workjet-Head nicht neu gemessen.
- Lücken sind vor allem: Threads und Nachrichten senden, Status lesen, Projekte anlegen und konfigurieren, Import, Einstellungen, Rechner-Zuweisung, Git/PR-Operationen, Kalender, Sprache, Provider-Gateway.

## Abgleich

Legende: **MCP vorhanden** = registrierter typisierter Wrapper im genannten CTOX-Quellstand. **Befehl; MCP fehlt** = ein interner Befehl ist benannt, aber kein passender Domänen-Wrapper in dieser Bestandsaufnahme. **fehlt** = kein passender Domänen-Wrapper identifiziert; dies behauptet nicht, dass die Funktion überhaupt nicht existiert. Generische `business_os.query_records`, `search_records` und `get_record` können erlaubte Datensätze lesen, ersetzen aber weder Domänenaktionen noch deren Autorisierung. Tabellen mit Workjet-RPC-Namen sind Kandidaten für den weiteren Abgleich, keine vollständige aktuelle RPC-Verifikation.

### 1. Instanz und Server

| Workjet-Funktion | Protokoll | Stand in CTOX |
|---|---|---|
| Status der Instanz lesen | `server.getConfig`, `server.probe`, `app.version` | MCP: `business_os.status` (nur Business-OS-Teil) |
| Einstellungen lesen und ändern | `server.getSettings`, `server.updateSettings` | Befehl: `ctox.runtime_settings.save`; MCP fehlt |
| Provider-Liste aktualisieren | `server.refreshProviders`, `server.updateProvider` | fehlt |
| Server aktualisieren | `server.updateServer(WithProgress)` | fehlt (bewusst nicht für Agenten? Entscheidung nötig) |
| Diagnose | `server.getProcessDiagnostics`, `getTraceDiagnostics`, `getUsageSummary` | fehlt |
| Tastenkürzel | `server.upsertKeybinding`, `removeKeybinding` | fehlt (niedrige Priorität) |

### 2. Projekte

| Workjet-Funktion | Protokoll | Stand in CTOX |
|---|---|---|
| Projekte auflisten | `projects.list`, `project.list`, `ctox.workjet.project.list` | Befehl; MCP fehlt |
| Projekt anlegen | `project.create`, `ctox.workjet.project.upsert` | Befehl; MCP fehlt |
| Nativen Auftrag in einem bestehenden eigenen Projekt starten | `business_os.start_project_task` | **MCP vorhanden**; `project_id`, `title`, `instruction`, `idempotency_key` |
| Eigenen nativen Projektauftrag abbrechen | `business_os.cancel_project_task` | **MCP vorhanden**; `target_command_id`, `idempotency_key`, optional `reason` |
| Projekt konfigurieren, Metadaten | `project.configure`, `project.meta.update` | Befehl (`project.upsert`); MCP fehlt |
| Projekt löschen | `project.delete` | fehlt (Löschung braucht Bestätigung) |
| Projektordner hinzufügen, entfernen | `projects.add`, `projects.remove` | fehlt |
| Dateien lesen, schreiben, suchen | `projects.readFile`, `writeFile`, `listEntries`, `searchEntries`, `searchContents` | fehlt; für Agenten wichtig |
| Projekt-Arbeitskopie | `ctox.workjet.working_copy.upsert` | Befehl; MCP fehlt |
| Projekt-Kacheln, Reihenfolge | `projectOverview` (`metric`, `link`, `text`) | fehlt (in Arbeit laut Koordinator: `project.gallery.order`) |

`start_project_task` benötigt keine erfundene Business-OS-App, Crew-Mitgliedschaft, externen Harness oder Ausführungsrechner. Native Projektbesitz- und Policy-Prüfungen bleiben verpflichtend. Dieselbe Actor-/Projekt-/Idempotenz-Identität verwendet denselben Command-/Task-Auftrag; ein geänderter Auftrag mit demselben Schlüssel wird verweigert. CTOX liefert `command_id`, `task_id` und Status. Wiederholung nach einer verlorenen Antwort verwendet den ursprünglichen Schlüssel. Der Abbruch prüft den Zielbesitz; er macht bereits eingetretene Nebenwirkungen nicht rückgängig. Diese Quellverträge sind noch kein hier ausgeführter Recovery-Nachweis.

### 3. Projekt-Supervisor und Projekt-Chat

| Workjet-Funktion | Protokoll | Stand in CTOX |
|---|---|---|
| Supervisor binden | `project.supervisor.bind`, `ctox.workjet.project.supervisor.bind` | Befehl; MCP fehlt |
| Supervisor-Turn senden | `project.supervisor.turn.submit`, `ctox.workjet.project.supervisor.turn.submit` | Befehl; MCP fehlt |
| Supervisor-Turn beobachten | `project.supervisor.turn.watch` | Befehl; MCP fehlt |
| Supervisor-Turn abbrechen | `project.supervisor.turn.cancel` | Befehl; MCP fehlt |
| Projekt-Chat anlegen, sicherstellen | `project.chat.create`, `ctox.workjet.project.chat.create`, `.chat.ensure` | Befehl; MCP fehlt |
| Worker zum Projekt hinzufügen, entfernen | `project.worker.add`, `ctox.workjet.project.worker.add/remove` | Befehl; MCP fehlt |
| Worker-Profil binden, lösen | `ctox.workjet.worker_profile.bind/unbind` | Befehl; MCP fehlt |

### 4. Threads und Turns

Das ist der wichtigste Block für den Molecularity-Ablauf: Parent-Thread anlegen, Nachricht senden, Status lesen.

| Workjet-Funktion | Protokoll | Stand in CTOX |
|---|---|---|
| Thread anlegen | `thread.create`, `orchestration.dispatchCommand` | fehlt (Befehl `ctox.workjet.session.create` deckt nur Sitzungen ab) |
| Turn starten (Nachricht senden) | `thread.turn.start` | fehlt |
| Turn unterbrechen | `thread.turn.interrupt` | fehlt |
| Freigabe beantworten | `thread.approval.respond` | fehlt |
| Rückfrage beantworten | `thread.user-input.respond` | fehlt |
| Thread-Status und Verlauf lesen | `orchestration.subscribeThread`, `getArchivedShellSnapshot`, `searchThreads` | fehlt (nur Stream, kein Snapshot per MCP) |
| Turn-Diff lesen | `orchestration.getTurnDiff`, `getFullThreadDiff` | fehlt |
| Thread archivieren, wiederherstellen, löschen | `thread.archive`, `unarchive`, `delete` | fehlt (Löschung: Bestätigung) |
| Anheften, Snooze, Settle | `thread.pin`, `thread.snooze`, `thread.settle` (plus Gegenstücke) | fehlt |
| Titel, Metadaten | `thread.meta.update` | fehlt |
| Sitzung setzen, stoppen | `thread.session.set`, `thread.session.stop` | Befehl (Sitzungen); MCP fehlt |
| Checkpoint zurücksetzen | `thread.checkpoint.revert` | fehlt |
| Verlauf importieren | `thread.history.import` | fehlt |
| Interaktionsmodus, Runtime-Modus | `thread.interaction-mode.set`, `thread.runtime-mode.set` | fehlt |
| Workjet-Konfiguration am Thread | `thread.workjet-config.set` | fehlt |

### 5. Mailbox, Delegation, Übergabe

| Workjet-Funktion | Protokoll | Stand in CTOX |
|---|---|---|
| Nachricht an Worker | `workjet.mailbox.sendMessage` | fehlt |
| Aufgabe delegieren | `workjet.mailbox.delegateTask` | fehlt |
| Antworten | `workjet.mailbox.reply` | fehlt |
| Review anfordern | `workjet.mailbox.requestReview` | fehlt |
| Delegation aktualisieren, neu zuweisen | `workjet.mailbox.updateDelegation`, `reassignDelegation` | fehlt |
| Übergabe senden, listen, annehmen | `workjet.mailbox.sendHandoff`, `listHandoffs`, `acceptHandoff` | fehlt |

### 6. Worker und Dispatch

| Workjet-Funktion | Protokoll | Stand in CTOX |
|---|---|---|
| Worker-Dispatch | `business_os.workjet_worker_dispatch` (action `dispatch`) | **MCP vorhanden** (nur für Supervisor-Sitzungen) |
| Quelle registrieren, widerrufen, abfragen, abschließen | derselbe MCP-Befehl (`register_source`, `revoke_source`, `poll`, `complete`) | **MCP vorhanden** (nur Owner/Admin) |
| Worker-Anfragen, Empfang, Antwort | `workjet.worker.requests`, `receive`, `respond`, `routeReserve`, `routeVerify`, `sourcePrepare`, `sourceConfirm`, `enrollComputer` | fehlt |
| Worker-Status und Liste | (aus Dispatch-Ergebnis ableitbar) | fehlt |

### 7. Rechner (Computer)

| Workjet-Funktion | Protokoll | Stand in CTOX |
|---|---|---|
| Rechner auflisten | `computer.list`, `ctox.workjet.computer.list` | Befehl; MCP fehlt |
| Rechner zuweisen, lösen | `computer.assign`, `computer.unassign` | Befehl; MCP fehlt |
| Endpunkt anlegen, abschalten, listen | `computer.endpoint.upsert`, `.disable`, `.list` | Befehl; MCP fehlt |
| Mesh-Übersicht, Peers, Widerruf | `workjet.mesh.roster`, `overview`, `revokePeer` | fehlt |
| Einladungen | `invite.create`, `invite.revoke` | fehlt (Code in `mobile_invites.rs`) |

### 8. Sitzungen, Übergabe zwischen Rechnern

| Workjet-Funktion | Protokoll | Stand in CTOX |
|---|---|---|
| Sitzung anlegen, listen, löschen | `session.create`, `session.list`, `ctox.workjet.session.*` | Befehl; MCP fehlt |
| Sitzungs-Transfer starten, Status, Abbruch | `session.transfer.start`, `status`, `abort`, `ctox.workjet.session.transfer.*` | Befehl; MCP fehlt |

### 9. Import

| Workjet-Funktion | Protokoll | Stand in CTOX |
|---|---|---|
| Alt-Import prüfen, entscheiden | `workjet.legacyImport.inspect`, `decide` | fehlt |
| Sitzungs-Import prüfen, ausführen | `workjet.sessionImport.inspect`, `import` | fehlt |

Dieser Block ist für das Molecularity-Beispiel direkt nötig (importiertes Projekt zum Laufen bringen).

### 10. Provider-Gateway und Zugänge

| Workjet-Funktion | Protokoll | Stand in CTOX |
|---|---|---|
| Status, Katalog, Health, Nutzung | `workjet.providerGateway.status`, `catalog`, `scopedCatalog`, `health`, `usage` | fehlt |
| Modelle prüfen, entdecken, binden | `modelChecks`, `checkModels`, `discoverModels`, `bindModel` | fehlt |
| Konten: API-Schlüssel, OAuth, entfernen | `addApiKeyAccount`, `oauthStart/Poll/Cancel`, `removeAccount`, `setGrant` | Kein passender Workjet-MCP-Wrapper identifiziert; bestehende Produktaktionen und native Berechtigungen bleiben maßgeblich |
| Routing, Start, Stopp | `updateRouting`, `start`, `stop`, `admit`, `infer` | fehlt |
| Provider-Abo | `ctox.provider_subscription.status`, `rotate`, `disconnect` | Befehl; MCP fehlt |

Für die autorisierte Konten-Föderation kann ein Konto auf einem Computer oder standardmäßig auf CTOX liegen. Synchronisiert werden Existenz, Modelle und Health nach der geltenden Policy; das Secret bleibt am haltenden Knoten im Secret Store. Routing verwendet diesen Knoten. Dieses Inventar erklärt weder die Föderation noch eine neue Kontenverwaltung für implementiert. Modellvorschläge stammen ausschließlich aus dem von echten Provider-Listen gespeisten `llm.ctox.dev`-Katalog; Modell-IDs werden nicht geraten.

Das vorhandene Credential-Metadatenwerkzeug ist enger: Es erlaubt nur exakte, im aktuellen signierten Delegationskontext freigegebene Präsenzselektoren. Daraus folgen kein allgemeiner Zugriff auf Konten, Anzahl, Readiness oder Secret-Werte.

### 11. Git, Worktrees, Pull Requests, Review

| Workjet-Funktion | Protokoll | Stand in CTOX |
|---|---|---|
| Repository initialisieren, klonen, veröffentlichen | `vcs.init`, `sourceControl.cloneRepository`, `publishRepository`, `lookupRepository` | fehlt |
| Status, Refs, Wechsel, Pull | `vcs.refreshStatus`, `listRefs`, `createRef`, `switchRef`, `pull` | fehlt |
| Worktrees | `vcs.createWorktree`, `removeWorktree`, `workjet.worktrees.inspect` | fehlt |
| Stacked Action (commit, push, PR) | `git.runStackedAction` | fehlt |
| PR auflösen, vorbereiten | `git.resolvePullRequest`, `git.preparePullRequestThread` | fehlt |
| PR-Liste, Details, Aktivität, Diff | `pullRequests.list`, `listStats`, `detail`, `activity`, `diffFileContents` | fehlt |
| PR-Aktionen, Kommentare, Reviews | `pullRequests.runAction`, `update`, `comment`, `submitReview`, `replyToThread`, `setThreadResolution`, `requestReviewers`, `reviewerCandidates`, `setReaction` | fehlt |
| Diff-Vorschau | `review.getDiffPreview`, `review.getDiffFileContents` | fehlt |
| Quellcode-Snapshots (CTOX-intern) | `ctox.source.save`, `load`, `diff`, `commit`, `log`, `list_snapshots`, `rollback_snapshot` | **MCP fehlt** (Befehle vorhanden) |

### 12. Dateien, Terminal, Vorschau

| Workjet-Funktion | Protokoll | Stand in CTOX |
|---|---|---|
| Dateien | siehe Abschnitt 2 | fehlt |
| Terminal öffnen, schreiben, lesen, schließen | `terminal.open`, `write`, `attach`, `close`, … | kein MCP-Wrapper in diesem Umfang; insbesondere Terminal-Input ist ausdrücklich ausgeschlossen |
| Vorschau, Browser-Automation | `preview.*` (10), `previewAutomation.*` (3) | fehlt. Für Fernsteuerung optional, meist UI-only |

Ein generischer Terminal-Wrapper gehört nicht zum Molecularity-Auftrag. Zulässige Laufstatus-, Aktivitäts- und Ergebnisabfragen bleiben typisierte, policy-gebundene Leseoperationen; sie erteilen keine Terminal-Schreibrechte.

### 13. Jour fixe, Kalender, Meetings, KPIs

| Workjet-Funktion | Protokoll | Stand in CTOX |
|---|---|---|
| Jour fixe lesen | `business_os.jour_fixe_read` | **MCP vorhanden**: `readMeeting`, `readComments`, `readTranscript`; registrierter Supervisor mit gültiger Lease |
| Jour fixe schreiben | `business_os.jour_fixe_update` | **MCP vorhanden**: `updatePrepareDeck`, `updateProposeTodos`, `narrate`; registrierter Supervisor mit gültiger Lease |
| Weitere Jour-fixe-Aktionen: Meeting starten und beenden, Todos, Deck, Kommentare | `ctox.workjet.jour_fixe.*` | Interne Befehle; die zwei MCP-Wrapper oben erschließen nur ihre ausdrücklich typisierten Aktionen |
| KPI-Werte lesen, auflösen | `business_os.project_kpi` | **MCP vorhanden** (nur Supervisor) |
| KPI-Definitionen konfigurieren | `ctox.workjet.project.kpis.configure` | Befehl; MCP fehlt |
| Meetings planen, Status, Absage, Transkript, Löschen | `meeting.schedule`, `status`, `cancel`, `get_transcript`, `delete` | **MCP vorhanden** (CTOX-Meeting, nicht Kalender) |
| Kalenderkonten und Termine | `calendar_account`, `calendar_events` in CTOX | fehlt (in Arbeit laut Koordinator) |

### 14. Sprache

| Workjet-Funktion | Stand in CTOX |
|---|---|
| Sprachsitzung, STT/TTS-Rechner, Status | Code vorhanden (`speech` in 8 Dateien), kein MCP. In Arbeit laut Koordinator (`speech-status`) |

### 15. Entscheidungen, Lumas, Crew

| Workjet-Funktion | Protokoll | Stand in CTOX |
|---|---|---|
| Entscheidung anfordern, lesen | `decision_hub.request_decision`, `get_decision` | **MCP vorhanden** |
| Entscheidungs-Verbindungen | `workjet.decisionHub.listConnections`, `provisionConnection`, `probeConnection`, `disconnectConnection` | fehlt |
| Lumas-Konfiguration | (keine Protokollmethode gefunden) | In Arbeit laut Koordinator, nicht doppelt bauen |
| Crew-Ausführung | `business_os.start_crew_execution`, `claim`, `report`, `list`, `update_crew_plan`, `get_crew_context` | **MCP vorhanden** |

### 16. Modul-Apps (Business OS)

Dieser Bereich ist im MCP am vollständigsten: App anlegen, ändern, Quelldateien lesen und schreiben, validieren, Smoke-Test, Modul-Aktionen, Freigaben, Datensätze, Artefakte, Läufe, Designvorlagen. Details in `docs/business-os-mcp-channel-v1-security-admin-guide.md`.

### Weitere Befehle ohne MCP-Gegenstück (Befehlsebene)

Die ursprüngliche Bestandsaufnahme nennt außerdem diese internen Befehlsfamilien, für die sie keinen direkten, gleichnamigen MCP-Wrapper ausweist: `ctox.coding.turn` (Coding-Sidecar), `ctox.module.*` (Installation, Rollback, Version), `ctox.app_store.*`, `ctox.secret.*`, `ctox.channel.*`, `ctox.mailserver.*`, `ctox.iot.*`, `ctox.appsec.*`, `ctox.crew.*`, `ctox.business_os.audit.*`, `ctox.business_os.backup.*`. Die meisten liegen außerhalb des Workjet-Ziels. Manche Ziele sind über typisierte Vorschlags-/Ausführungswerkzeuge erreichbar, etwa die erlaubte App-Store-Installation; das Fehlen eines gleichnamigen Wrappers bedeutet kein allgemeines Verbot. Secret-Zugriff folgt dem bestehenden Vertrag und wird hier nicht freigegeben.

## Lückenliste nach Reihenfolge des Molecularity-Ablaufs

1. **Instanz und Projekte**: `projects.list`, Projekt anlegen und konfigurieren, Supervisor binden, Supervisor-Turn senden und beobachten, Projekt-Chat sicherstellen.
2. **Import**: Sitzungs- und Alt-Import prüfen und ausführen.
3. **Threads und Parents**: Thread anlegen, Turn starten (Nachricht senden), Turn unterbrechen, Freigabe und Rückfrage beantworten, Status-Snapshot und Turn-Diff lesen, Archivieren und Anheften.
4. **Worker**: Worker starten (Dispatch besteht), Worker-Anfragen und Status lesen, Mailbox senden und antworten, Übergabe.
5. **Einstellungen**: Settings lesen und ändern, Provider-Status, Gateway-Status (ohne Geheimnisse).
6. **Rechner**: Liste, Zuweisung, Endpunkte, Mesh-Übersicht.
7. **Kalender und Sprache**: abhängig von den parallelen Threads.
8. **Übersicht**: Projektkacheln, KPI-Konfiguration, Jour-fixe-Listen.

Git/PR und Vorschau sind eigene Blöcke und benötigen einen gesonderten Auftrag. Terminal-Input und Server-Updates bleiben aus diesem Molecularity-Ablauf ausgeschlossen.

## Gestaltungsvorschlag für den MCP-Server

- Ein typisiertes Werkzeug pro Domäne, zum Beispiel `workjet.project`, `workjet.thread`, `workjet.worker`, `workjet.computer`, `workjet.import`, mit einem `action`-Feld. Das folgt dem Muster von `mcp_workjet_jour_fixe.rs` und `mcp_workjet_kpis.rs`.
- Jede Aktion wird auf einen bestehenden `ctox.workjet.*`-Befehl oder auf eine neue Befehlsart der Befehlsebene abgebildet. Die Geschäftslogik bleibt in der Befehlsebene, der MCP-Teil ist nur Schnittstelle und Richtlinie.
- Lesen und Schreiben bleiben beide policy- und grantgebunden. `enforce_business_os_mcp_policy` sowie die jeweilige native Actor-, Modul-, Collection-, Projektbesitz- und Lease-Prüfung entscheiden. Rollen allein erteilen keine pauschale Freigabe. Die tatsächliche Managed-Instanzidentität bleibt vom Tenant-Workspace getrennt; ein Wrapper darf keinen bestehenden Grant verbreitern oder Credential-Werte exportieren.
- Der Vorschlag erteilt keine zusätzlichen Rechte. Bestehende autorisierte Lösch-, Widerruf-, Konten- und Secret-Store-Produktwege behalten ihren jeweiligen Vertrag; neue Wrapper müssen dessen enge Ziele und Ablehnungsfälle erhalten.
- Jede Aktion bekommt einen Test gegen die Befehlsebene und einen Test der Richtlinie (erlaubt und verweigert).
- Wire-Verträge werden in den kanonischen Fixtures (`src/core/rxdb/tests/fixtures/*.json`) geändert und anschließend für beide Seiten regeneriert; generierte Ausgaben werden nicht von Hand editiert.

## Geltungsbereich und nächste Implementierung

Das Inventar setzt keine globalen Defaults oder zusätzlichen Vorab-Freigaben.
Die aktuellen Entscheidungen des Auftraggebers und die native Policy gelten.
Ein neuer Wrapper für Threads oder Konten ist ein eigener Implementierungsschritt;
dieses Dokument liefert dafür keine Rechte. Geheimnisse bleiben auf freigegebenen
Status beschränkt; Terminal-Input und Server-Updates sind keine nächsten Schritte
dieses Auftrags.

Für jede Ergänzung werden der konkrete Befehl, die Projekt-/Instanzidentität,
Idempotenz, Ergebnis-/Abbruchbeobachtung und erlaubte sowie verweigerte Fälle
am finalen Quell-Head geprüft. Nach dem normalen Merge folgt die Abnahme am
installierten Stand. Der hier beschriebene Quellenabgleich ersetzt weder diese
Abnahme noch den Nachweis eines vollständig fernsteuerbaren Workjet-Projekts.
