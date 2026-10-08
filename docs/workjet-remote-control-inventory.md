# Workjet vollständig per Agent fernsteuern: Befehlsinventar

Stand: 2026-10-08. Ziel: Ein Agent soll Workjet komplett über den MCP-Server von CTOX bedienen können, ohne Computer Use. Dieses Dokument listet, welche Workjet-Funktionen es gibt, ob es dafür heute einen MCP-Befehl gibt, und wo die Lücken liegen.

Quellen:

- Workjet-Protokoll: `workjet/packages/contracts/src/rpc.ts` (`WS_METHODS`), `orchestration.ts` (Thread-Befehle), `project.ts`, `projectOverview.ts`, `settings.ts`
- CTOX-Befehlsebene: `src/core/business_os/command_plane.rs` und `store_workjet_*.rs`, Befehlstypen `ctox.workjet.*`
- MCP-Server: `src/core/business_os/mcp_channel.rs` (`tool_descriptors()`, Zeile ~1449), `mcp_workjet_*.rs`

## Kurzfassung

- Der MCP-Server bietet heute 56 Werkzeuge. Vier davon sind Workjet-spezifisch: Worker-Dispatch, Jour fixe lesen und schreiben, KPI-Abfrage. Der Rest betrifft Business-OS-Apps, Datensätze, Freigaben, Crew-Ausführung und Meetings.
- Die Befehlsebene von CTOX kennt deutlich mehr. Für Projekte, Supervisor-Chats, Sitzungen, Rechner und Jour fixe gibt es `ctox.workjet.*`-Befehle. Diese sind nur über die UI und die interne Befehlsebene erreichbar, nicht über MCP.
- Das Workjet-Protokoll hat rund 160 RPC-Methoden (`rpc.ts`) und 9 Orchestrierungs-Methoden (`orchestration.ts`). Ein Teil gehört nur zur Oberfläche (Vorschau, Terminal, Ansichtszustand). Der Rest ist für die Fernsteuerung relevant.
- Lücken sind vor allem: Threads und Nachrichten senden, Status lesen, Projekte anlegen und konfigurieren, Import, Einstellungen, Rechner-Zuweisung, Git/PR-Operationen, Kalender, Sprache, Provider-Gateway.

## Abgleich

Legende: **MCP** = über MCP erreichbar. **Befehl** = `ctox.workjet.*`-Befehl existiert, aber kein MCP-Werkzeug. **fehlt** = weder noch.

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
| Projekt konfigurieren, Metadaten | `project.configure`, `project.meta.update` | Befehl (`project.upsert`); MCP fehlt |
| Projekt löschen | `project.delete` | fehlt (Löschung braucht Bestätigung) |
| Projektordner hinzufügen, entfernen | `projects.add`, `projects.remove` | fehlt |
| Dateien lesen, schreiben, suchen | `projects.readFile`, `writeFile`, `listEntries`, `searchEntries`, `searchContents` | fehlt; für Agenten wichtig |
| Projekt-Arbeitskopie | `ctox.workjet.working_copy.upsert` | Befehl; MCP fehlt |
| Projekt-Kacheln, Reihenfolge | `projectOverview` (`metric`, `link`, `text`) | fehlt (in Arbeit laut Koordinator: `project.gallery.order`) |

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
| Konten: API-Schlüssel, OAuth, entfernen | `addApiKeyAccount`, `oauthStart/Poll/Cancel`, `removeAccount`, `setGrant` | fehlt. Geheimnisse: nur mit eigener Freigabe-Regel |
| Routing, Start, Stopp | `updateRouting`, `start`, `stop`, `admit`, `infer` | fehlt |
| Provider-Abo | `ctox.provider_subscription.status`, `rotate`, `disconnect` | Befehl; MCP fehlt |

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
| Terminal öffnen, schreiben, lesen, schließen | `terminal.open`, `write`, `attach`, `close`, … | fehlt. Entscheidung nötig: Agenten-Shell über MCP ist ein eigener Risikobereich |
| Vorschau, Browser-Automation | `preview.*` (10), `previewAutomation.*` (3) | fehlt. Für Fernsteuerung optional, meist UI-only |

Empfehlung: Vorschau und Terminal-Darstellung ausklammern. Terminal-Eingaben nur mit eigenem Werkzeug und Freigabe.

### 13. Jour fixe, Kalender, Meetings, KPIs

| Workjet-Funktion | Protokoll | Stand in CTOX |
|---|---|---|
| Jour fixe lesen | `business_os.jour_fixe_read` | **MCP vorhanden** |
| Jour fixe schreiben (inkl. narrate) | `business_os.jour_fixe_update` | **MCP vorhanden** |
| Jour fixe: Vorbereitung, Meeting starten und beenden, Transkript, Todos, Deck, Kommentare | `ctox.workjet.jour_fixe.*` (≈14 Befehle) | Befehl; MCP teilweise (nur die zwei Werkzeuge oben) |
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

Diese Befehle existieren in CTOX, sind aber nicht über MCP erreichbar: `ctox.coding.turn` (Coding-Sidecar), `ctox.module.*` (Installation, Rollback, Version), `ctox.app_store.*`, `ctox.secret.*`, `ctox.channel.*`, `ctox.mailserver.*`, `ctox.iot.*`, `ctox.appsec.*`, `ctox.crew.*`, `ctox.business_os.audit.*`, `ctox.business_os.backup.*`. Die meisten davon liegen außerhalb des Workjet-Ziels. Ausnahme: `ctox.secret.*` und `ctox.provider_subscription.*` brauchen eine bewusste Entscheidung, ob Agenten sie sehen dürfen.

## Lückenliste nach Reihenfolge des Molecularity-Ablaufs

1. **Instanz und Projekte**: `projects.list`, Projekt anlegen und konfigurieren, Supervisor binden, Supervisor-Turn senden und beobachten, Projekt-Chat sicherstellen.
2. **Import**: Sitzungs- und Alt-Import prüfen und ausführen.
3. **Threads und Parents**: Thread anlegen, Turn starten (Nachricht senden), Turn unterbrechen, Freigabe und Rückfrage beantworten, Status-Snapshot und Turn-Diff lesen, Archivieren und Anheften.
4. **Worker**: Worker starten (Dispatch besteht), Worker-Anfragen und Status lesen, Mailbox senden und antworten, Übergabe.
5. **Einstellungen**: Settings lesen und ändern, Provider-Status, Gateway-Status (ohne Geheimnisse).
6. **Rechner**: Liste, Zuweisung, Endpunkte, Mesh-Übersicht.
7. **Kalender und Sprache**: abhängig von den parallelen Threads.
8. **Übersicht**: Projektkacheln, KPI-Konfiguration, Jour-fixe-Listen.

Git/PR, Terminal und Vorschau sind eigene Blöcke. Sie kommen nach Punkt 8, sofern Michael sie für Agenten will.

## Gestaltungsvorschlag für den MCP-Server

- Ein typisiertes Werkzeug pro Domäne, zum Beispiel `workjet.project`, `workjet.thread`, `workjet.worker`, `workjet.computer`, `workjet.import`, mit einem `action`-Feld. Das folgt dem Muster von `mcp_workjet_jour_fixe.rs` und `mcp_workjet_kpis.rs`.
- Jede Aktion wird auf einen bestehenden `ctox.workjet.*`-Befehl oder auf eine neue Befehlsart der Befehlsebene abgebildet. Die Geschäftslogik bleibt in der Befehlsebene, der MCP-Teil ist nur Schnittstelle und Richtlinie.
- Jede Aktion läuft durch `enforce_business_os_mcp_policy`. Lesende Aktionen sind frei, schreibende brauchen die Rolle des Projekt-Supervisors oder Owner/Admin.
- Löschungen, Rechner-Widerruf und Geheimnisse sind nicht Teil der ersten Stufe, bis Michael sie freigibt.
- Jede Aktion bekommt einen Test gegen die Befehlsebene und einen Test der Richtlinie (erlaubt und verweigert).
- Generierte Verträge (`src/core/rxdb/tests/fixtures/*.json`) werden nicht von Hand geändert.

## Offene Entscheidungen für Michael

- Dürfen Agenten Geheimnisse und Provider-Konten sehen oder ändern? Vorschlag: nein, nur Status.
- Dürfen Agenten Terminal-Eingaben schicken? Vorschlag: nein, in der ersten Stufe.
- Server-Update über MCP? Vorschlag: nein.

## Bewusst gesetzte Grenzen (Stand 2026-10-08)

Diese Grenzen gelten als Default, bis Michael sie ändert. Sie lassen sich später lockern, jede Lockerung braucht einen eigenen PR mit Tests.

- **Geheimnisse und Provider-Konten:** Agenten sehen nur Status (vorhanden, lesbar, Anzahl, Gesundheit). Keine Schreib- oder Leseaktion auf Schlüssel, Tokens oder OAuth-Daten.
- **Terminal:** Keine Eingabe in Terminals über MCP.
- **Server-Update:** Kein Update über MCP.
