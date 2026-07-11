# CTOX Business OS App Platform Refactoring Plan

Status: Presentation, Policy, Context/Delegation und Starter/Archetypen sind
weitgehend umgesetzt. Revision 15 ergänzt den verbindlichen Client-only-
App-Vertrag: neue Business-Apps dürfen weder Rust-Änderungen noch einen CTOX-
Recompile benötigen; Schema, Echtzeitsync, Freigaben und deklarative Aktionen
werden zur Laufzeit aus dem App-Paket registriert. Der Refactor bleibt offen,
Revision 15

Stand: 2026-07-11

Fortschrittsindikator nach requirement-by-requirement Audit vor Aufnahme des
Client-only-Tracks: ca. 84 % der technischen Umsetzung und ca. 75 % des
Gesamtplans inklusive menschlicher Freigaben und Produktmetadaten. Der neue
Runtime-App-Track wird nicht rückwirkend als erledigt gezählt. Revision 14 nahm den zuvor zu hoch bewerteten
34+2-Browsernachweis zurueck: API-gesteuertes Resize und Root-Overflow beweisen
weder echte Mausbedienung noch Pane-Nutzbarkeit, Mehrfachchat-Komposition,
vollstaendige Window-Header oder Mobile-Shell.
Revision 15 verankert diese Regeln zusaetzlich im Impeccable-Projektkontext,
im kanonischen App-Development-Skill und im separaten CTOX-Deploy-Skill; die
frische Installed-Abnahme bleibt bis zum laufenden Installer-/Browsernachweis
offen.

Scope: Business OS Shell, Presentation Runtime, App SDK, Design System,
Context Actions, Rechte, Delegation, Creator, Templates, Store, Release und
Migration aller Apps

## 0A. Produktinvariante: Apps sind Client-Pakete

Das Zielmodell entspricht einer nativen Desktop-App-Plattform: Eine neue App
wird als installierbares HTML-/CSS-/ESM-Paket entwickelt und gegen eine bereits
laufende CTOX-Instanz installiert. Der Entwickler beschreibt Datenmodell,
Darstellung, Sharing und optionale Aktionen deklarativ. Persistenz, Offline-
Journal, Recovery, WebRTC-Sync, Reconnect, Multi-Tab und Multiuser-Verteilung
werden von Business OS bereitgestellt.

Nicht verhandelbar:

- Eine reine Daten-App benötigt keine Änderung unter `src/core/`, keine neue
  Rust-Match-Arm, keine Wire-Fixture und keinen `cargo build`.
- Neue Collections aus `collections.schema.json` werden beim Installieren oder
  Aktualisieren einer App automatisch validiert und im laufenden Datenpfad
  registriert. Ein interner, supervisierter Peer-Reconcile ist zulässig; ein
  manueller Daemon-Neustart oder Recompile ist es nicht.
- Echtzeitsync ist für App-Collections standardmäßig aktiv. „Standardmäßig“
  bedeutet nicht „für jeden sichtbar“: Workspace-, Rollen- und Record-ACLs
  bestimmen weiterhin serverautoritativ, welche Nutzer replizieren dürfen.
- Normales CRUD läuft direkt über die shellgelieferten Collection-Handles.
  App-Code baut keinen HTTP-Endpunkt, keinen eigenen WebRTC-Peer und keine
  zweite Persistenzschicht.
- App-spezifische Aktionen dürfen als Manifest-/Action-Definition zur Laufzeit
  hinzukommen. Der native Router interpretiert geprüfte generische Primitive;
  er erhält nicht für jeden neuen Action-Namen kompilierten Rust-Code.
- Unbekannte Browser-Payloads sind niemals frei ausführbare Backend-Programme.
  Der Runtime-Interpreter akzeptiert nur registrierte Input-Schemas, erlaubte
  Collection-Operationen, explizite Permissions, Idempotency Keys und
  gegebenenfalls deklarative Saga-/Compensation-Schritte.
- Hostzugriff, externe Side Effects oder Spezialprotokolle bleiben optionale
  Erweiterungen. Sie müssen über vorhandene Capability-Adapter oder zukünftig
  sandboxed, zur Laufzeit installierbare Extensions laufen – nicht über einen
  stillen Vertrauenssprung vom Browser und nicht zwingend über einen CTOX-
  Binary-Recompile.

Der Default-Entwicklerpfad lautet damit:

```text
module.json + collections.schema.json + index.html/js/css
                         │
                         ▼
       Business OS installiert und validiert das Paket
                         │
        ┌────────────────┼────────────────┐
        ▼                ▼                ▼
  IndexedDB/WAL     WebRTC/SQLite    ACL/Multiuser
        │                │                │
        └────────────────┴────────────────┘
             reaktive App-Collections
```

### 0A.1 Runtime Data Contract

`module.json` erhält beziehungsweise normalisiert einen deklarativen
`data_runtime`-Block. Die exakte Fixture wird im Implementierungstrack
festgelegt; semantisch muss er mindestens abbilden:

- `sync: "realtime" | "local"`, Default `realtime`;
- `scope: "workspace" | "actor" | "record"`, mit sicherem Draft-/Release-
  Default;
- Collection-Read/Write-Absicht und Grant-Vorschlag;
- Konfliktstrategie, Retention und optionale Demand-Loading-Hinweise;
- deklarative Actions mit Input-Schema, Policy, Idempotenz und zulässigen
  Effekten.

Die Schema-Datei bleibt die Quelle für Records, Indizes und deklarative
Migrationen. Browser und Native lesen dasselbe kanonische JSON; App-Entwickler
pflegen keine zweite Rust-Repräsentation.

### 0A.2 Drei Action-Stufen

1. **Client CRUD (Default):** `insert`, `incrementalUpsert`, `patch`, Delete
   und reaktive Queries auf app-eigenen Collections. Automatisch offline- und
   realtimefähig.
2. **Deklarative Runtime Action:** serverautoritativ geprüfte Mutation,
   Bulk-Operation oder registrierte Saga aus sicheren generischen Primitiven.
   Die Definition wird mit der App installiert; kein Recompile.
3. **Privilegierte Extension:** externer Dienst, Hostzugriff oder spezialisierte
   native Fähigkeit. Explizite Capability, Review und Sandbox/Adapter nötig;
   niemals der Standard für eine normale Business-App.

### 0A.3 Abnahmekriterium

Auf einem bereits gestarteten Release-Binary wird eine bislang unbekannte App
mit neuer Collection und neuem deklarativem Action-Namen installiert. Ohne
Source-Änderung, Backend-Build oder Entwickler-Neustart müssen zwei echte,
verschieden berechtigte Browserprofile Folgendes beweisen:

1. App öffnet und Schema wird automatisch registriert.
2. Profil A schreibt offline; der Write ist journalisiert und lokal sichtbar.
3. Nach Reconnect erreicht er native SQLite und Profil B in Echtzeit.
4. Ein Nutzer ohne Grant erhält weder Pull noch Push-Zugriff.
5. Die deklarative Action ist idempotent, auditiert und projiziert ihren Status.
6. Reload, App-Update und Leaderwechsel erhalten Daten und Checkpoints.
7. Kein geändertes Rust-Artefakt und kein während des Tests ausgeführter
   `cargo build` sind Teil des Installationspfads.

## 0. Verifizierter Implementierungs- und Rollout-Status

Der Plattform-Refactor ist im gemeinsamen Worktree weitgehend implementiert,
aber noch nicht produktionsreif. Die frühere Revision 3 leitete den technischen
Abschluss aus Source-Gates und einem Browserlauf gegen Workspace-Assets ab;
Revision 4 widerlegte das durch einen fehlschlagenden installierten Langlauf.
Revision 5 isolierte QA-Lifecycle-, Mount- und Transportursachen. Revision 6
schloss erstmals den installierten 34-App-Cold-Lifecycle sowie den realen
Rechtsklick→Command-Bus→Native-Store→Queue-Pfad. Revision 7 bestätigt den
finalen ausgelieferten 34-App-Stand und schließt Zwei-Profil- sowie
Edit/Reload/Browser-/Service-Restart-Persistenz. Revision 8 schließt den echten
Nicht-Admin-Denial→Threads→Reviewer→Reauthorization-Pfad, den isolierten
Login/Logout-Gate und einen erneuten installierten 34-App-Lauf. Als technischer
Restblocker blieb die Komposition des persistenten Chat-Docks mit fokussierten
App-Windows. Revision 9 schließt auch diesen Punkt mit einem adaptiven
Bottom-/Side-/Compact-Dock-Vertrag, reproduzierbarem E2E und einem interaktiven
Lauf gegen die installierte V16-Version.
Revision 10 macht zusätzlich den realen Rechtsklick-/Delegationsflow bei je
10.000 historischen Commands, Threads, Messages und Notifications zum
verbindlichen Production-Smoke der Release-Matrix.
Revision 11 prüft die Plananforderungen erstmals gegen ihre jeweils geforderte
Breite. Dabei wurde insbesondere festgestellt, dass ein 34-Oberflächen-
Langlauf zwei Apps der Migrationsmatrix ausgelassen, zwei andere Desktop-
Surfaces eingesetzt und nur Open/Overflow/unbenannte Buttons geprüft hatte.
Auch die vorhandene 12-fache Design-Matrix war ein Design-Lab-, kein per-App-
Gate, und ihr Locale-Schalter übersetzte zunächst keinen sichtbaren Text.

Im Worktree erreicht:

- alle 34 Source-Apps bestehen den offiziellen App-Validator,
- alle 33 normalen Apps besitzen den kanonischen Presentation Contract und
  unterstützen `window`, `maximized` und `focus` bei mindestens 640 × 480,
- `desktop` ist die einzige verbliebene Full-Workspace-/Shell-Surface,
- Registry, eingebetteter Offline-Katalog, Creator, Validator und Skill-Dokus
  schreiben denselben Presentation Contract,
- Window/Maximized/Focus wechseln ohne Remount; Window Hosts liefern echte
  Left/Main/Right-Slots und sind CSS-Container,
- Exact `data.read`/`data.write`, Grant-Seeding, fail-closed Capabilities und
  Actor-Epoch-Revocation sind nativ implementiert,
- SemVer allein veröffentlicht keine App mehr; bestehende SemVer-Sichtbarkeit
  wird in Release-Records migriert,
- Install, Reinstall, Template-Install und Catalog-Update validieren im
  Staging und aktivieren atomar; nicht vertrauenswürdige Third-Party-Bundles
  bleiben bis zu einer Sandbox gesperrt,
- Context v2 erfasst App, Window, Surface, Pane, Collection, Entity, Field,
  Auswahl, Pointer und Deep Link; `ctx.contextActions.register/capture/dispatch`
  ist Teil des gepinnten Mount-Vertrags,
- Rechtsklick, ContextMenu-Taste und Shift+F10 benutzen denselben zentralen
  Renderer und Typed Command Bus,
- verweigerte delegierbare Daten-/App-Aktionen werden als
  `threads.ctox_approval.request` mit Reviewer und vollständigem Kontext
  persistierbar gemacht,
- 14 tote App-Kontextmenüs wurden entfernt; nur das fachliche Desktop-Menü ist
  als Shell-Ausnahme erlaubt,
- Light und Dark verwenden das kompakte, flache Operational-Instrument-System;
  Design Lab und 12-fache Width/Theme/Locale-Matrix sind reproduzierbar,
- der App-Development-Skill ist validiert und über
  `business_os.read_app_skill_resource` remote abrufbar.

Diese Liste beschreibt implementierte Plattformfähigkeiten, nicht mehr
automatisch den vollständigen DoD aller Apps. Eine App gilt erst nach der
app-spezifischen Evidence aus Abschnitt 8 als vollständig migriert.

Automatisierte Evidence:

- Business-OS-JS-Suite: 114 Tests grün,
- Source-App-Validator: 34/34 grün,
- Registry-/Launcher-Smoke: 35 eindeutige Launch Targets,
- Design-Matrix: 12/12 Renderings grün,
- nativer Exact-Collection-Grant-Test grün,
- Browser-E2E gegen Workspace-Assets für Window/Focus ohne Remount, Context v2,
  Typed Ask, Shift+F10 und Denial→Threads-Approval grün,
- gespeicherte Browser- und Design-Artefakte unter
  `output/playwright/business-os-refactor-final-all/` und
  `output/playwright/business-os-design-matrix/`.

Historischer installierter Revision-4-Baseline-Test vom 2026-07-10
(durch Revision 6 überholt):

- `ctox-real` wurde aus dem Worktree als Release gebaut, lokal installiert und
  über den realen Service auf `http://127.0.0.1:8765/` getestet.
- Der native Peer erreicht nach zwei Transportkorrekturen
  `replicationUp=true`: Signaling bevorzugt IPv4 mit IPv6-Fallback und der
  Default-ICE-Kandidat veröffentlicht eine routbare lokale Adresse statt
  `0.0.0.0`.
- Der strikte installierte QA-Lauf öffnete 32 von 34 Core-Apps; `outbound` und
  `buchhaltung` überschritten jeweils das 25-Sekunden-Zeitbudget.
- 19 App-Surfaces erzeugten Console-/Page-Error-Gruppen; insgesamt wurden 484
  Events erfasst. Dominant sind Timeouts für `masterChangesSince` und
  `masterWrite`, Multi-Tab-Query-Owner-Timeouts sowie geschlossene
  `BroadcastChannel`-Instanzen.
- Alle 34 nummerierten Screenshots wurden erzeugt. Der isolierte App-Creator
  öffnet in ungefähr 14 Sekunden; sein Langlauf-Fehler ist damit ein
  Lifecycle-/Transportproblem und kein reiner Creator-Mount-Defekt.
- Rechtsklick und Shift+F10 öffnen auf einem Notes-Record dasselbe zentrale
  Menü. Der v2-Payload enthält korrekte App-, Window-, Pane-, Record-, Pointer-
  und Deep-Link-Daten (`record_id = notes_seed_ops_review`); Collection und
  Field Path bleiben leer.
- Der dabei erzeugte `business_os.context.ask`-Command war im Browser-Dispatcher
  sichtbar, ließ sich über den nativen MCP-Zugriff auf `business_commands` aber
  nicht nachweisen. Damit ist Command-Persistenz/Delivery im installierten
  System nicht bestanden.
- Ein zweiter paralleler Browser kann trotz aufgebauter RTC-Verbindung in
  `masterChangesSince`-Timeouts laufen. Single-Browser-Erfolg ist deshalb kein
  ausreichendes Production Gate.

Evidence:

- `output/playwright/business-os-local-installed-20260710-final/business-os-qa-baseline.md`
- `output/playwright/business-os-local-installed-20260710-final/business-os-qa-baseline.json`
- Screenshots `01-ctox.png` bis `34-creator.png` im selben Verzeichnis

Fortschritt der Revision 5 am 2026-07-10:

- Der zuvor beobachtete Renderer-Runaway war zu einem wesentlichen Teil ein
  Fehler im QA-Lifecycle selbst: Der Runner rief die nicht vorhandene
  `windowManager.close`-Methode auf. Dadurch blieben alle Fenster, Mounts,
  Subscriptions und Demand-Queries des gesamten 34-App-Laufs aktiv. Der Runner
  verwendet jetzt `destroy`, wartet auf das tatsächliche `window:closed` und
  räumt auch nach fehlgeschlagenen Starts auf. Ein Node-seitiger Watchdog
  beendet einen wirklich blockierten Renderer reproduzierbar.
- Der Demand-Transport wiederholt serverseitiges `RATE_LIMITED` mit begrenztem
  Backoff und frischer Request-ID. Ein transienter ICE-`disconnected`-Zustand
  erhält 30 Sekunden Grace; `failed` und `closed` bleiben sofort terminal.
- Die generische native Business-Record-Projektion schreibt kanonisch
  verglichen in Batches statt jeden Datensatz einzeln und gibt den
  Datenbank-Lock zwischen Seiten frei. Die nächste native Revision reduziert
  die Seite von 250 auf 25 Datensätze und verwendet einen zusammengesetzten
  `(updated_at_ms, record_id)`-Cursor, damit gleiche Zeitstempel weder
  übersprungen noch endlos wiederholt werden.
- Threads-, Documents-, Customers-, Buchhaltung-, Support- und Calendar-Mounts
  rendern die Window-Surface jetzt vor Demand-Abfragen, optionalem Seed und
  Realtime-Setup. Der Explorer rendert ebenfalls sofort, schreibt seine
  Standardordner nur bei semantischen Änderungen und verwendet keine vom
  nativen SQL-Query-Stream nicht unterstützte `$ne`-Abfrage mehr.
- Der strikte installierte Lifecycle-Lauf
  `output/playwright/business-os-local-installed-20260710-lifecycle-v15/`
  erreichte 33/34 Apps ohne Renderer-Hänger. Einzig Explorer scheiterte noch
  an `SQLITE_QUERY_STREAM_UNSUPPORTED`; der anschließende installierte
  Zieltest `business-os-target-explorer-creator-v16/` besteht Explorer und App
  Creator ohne Console-/Page-Fehler.
- Der vollständige RxDB-JS-Lauf besteht mit 59 grünen Tests, 0 Fehlern und 2
  bewusst übersprungenen Cross-Process-Tests; der Modul-Conformance-Validator
  bleibt bei 34/34.

### Fortschritt der Revision 6 am 2026-07-10

- Das native Release-Binary wurde in einem isolierten Cargo-Target gebaut,
  atomar als `ctox-real` installiert und über den realen LaunchAgent gestartet.
  Die aktuell installierte SHA-256 ist
  `4f07ae948a2250ba068175bb4225716e96423a6a6b859f608a98650f4ad880c8`.
- Der strikte installierte Cold-Lifecycle
  `output/playwright/business-os-local-installed-20260710-full-cold-v25/`
  besteht erstmals alle 34 von 34 Apps. Jede App ist nach Öffnen und Schließen
  gesund; es gibt null Console-/Page-Fehler. Die einzige erfasste Console-Zeile
  ist eine nicht-fehlschlagende Lexical/Chrome-Contenteditable-Warnung.
- Der Lauf fand unmittelbar nach einem echten Service-Restart statt. Parallel
  projizierte der native Peer 10.650 Business Records. Der erste aktive Lauf
  sank von zuvor etwa 894 Sekunden auf 145,836 Sekunden; der folgende
  inkrementelle Lauf benötigte 7,091 Sekunden. Die UI blieb dabei bedienbar.
- Conversations, Shiftflow und IoT rendern vor ihrer ersten Datenhydration;
  Creator gibt seine Loading-Surface vor dem inneren Modul-Mount zurück.
  Coding Agents liest beim Mount nur Projektionen und erzeugt keine drei
  impliziten Provider-Status-Commands mehr.
- Statische `docs/`-Assets werden aus dem bereits gebündelten Release-Verzeichnis
  aufgelöst. Der vorherige Notes-Logo-404 liefert jetzt byteidentisch HTTP 200
  mit `Content-Type: image/png`.
- Der reale Rechtsklick auf `Operations Review` erfasst
  `notes_seed_ops_review`, Pane `left`, Windowed-Surface, Pointer `(372,254)`
  und den Deep Link im `business-os-context-v2`-Envelope. Die drei Modi
  `Daten ändern`, `Frage stellen` und `App ändern` tragen getrennte Impact-
  Semantik.
- Context Commands warten nur noch auf einen verbundenen Native-Peer, nicht
  auf den vollständigen Pull von 11.000+ historischen Commands. Die UI erhält
  nach erfolgreichem Native-Push eine lokale Quittung und übergibt die weitere
  Statusverfolgung an Chat/Tracker.
- Finaler E2E-Command:
  `cmd_66d2033b-6527-4a59-9182-a007d9679699`. Die UI quittierte in 1,458
  Sekunden, schloss das Menü und öffnete den Chat. Native Status: `accepted`.
  Queue-Link: `queue:system::19a5c3c28393d0a296703ca2`, Status `queued`,
  Thread `business-os/notes/notes_seed_ops_review`.
- Capability-, Access- und API-Token-Schlüssel werden rekursiv aus Queue-
  Prompt-Previews entfernt. Im finalen Queue-Prompt, Command-Audit-Kontext und
  Communication-Metadaten sind `capability_token` und Secret-Marker jeweils
  nullfach vorhanden; der vollständige autorisierte Command-Datensatz bleibt
  intern erhalten.
- Aktuelle Regressionsevidence: Command Bus 14/14 grün, Redaction-Rust-Test
  1/1 grün, RxDB-JS 61/61 grün,
  Module Conformance 34/34.
- Der spätere finale installierte Lauf
  `output/playwright/business-os-local-installed-20260710-full-final-v32/`
  bestätigt den ausgelieferten Stand nach Context-Command-, Redaction- und
  Sync-Bundle-Änderungen erneut mit 34/34 Apps, durchgehend `OK/OK` nach
  Öffnen/Schließen sowie null Console-/Page-Fehlern.
- Die Collection-Observable-Schicht behandelt einen transient vollen,
  weiterhin hart begrenzten Demand-Query-Queue nun als Backpressure: Initiale
  Snapshots werden mit gekapptem Backoff wiederholt und beim Unmount
  abgebrochen. Der Regressionstest reproduziert zwei `QUERY_QUEUE_LIMIT`-
  Antworten und prüft die anschließende korrekte Initialemission.
- `Credentials` wartet beim Mount nicht mehr auf den Command-Bus-Roundtrip für
  `ctox.secret.list`. Die Window-Surface rendert sofort, Hydrierung läuft im
  Hintergrund und verspätete Ergebnisse werden nach Unmount verworfen. Im
  installierten Zieltest sank die Öffnung von einem reproduzierten
  25-Sekunden-Timeout auf 106 ms.

Evidence:

- `output/playwright/business-os-local-installed-20260710-full-cold-v25/business-os-qa-baseline.md`
- `output/playwright/business-os-local-installed-20260710-full-cold-v25/business-os-qa-baseline.json`
- `output/playwright/business-os-local-installed-20260710-full-final-v32/business-os-qa-baseline.md`
- `output/playwright/business-os-local-installed-20260710-full-final-v32/business-os-qa-baseline.json`
- `output/playwright/business-os-context-e2e-v29-redacted.png`
- `output/playwright/business-os-multi-profile-v33/business-os-multi-profile-evidence.md`
- `output/playwright/business-os-multi-profile-v33/after-service-restart.png`

### Fortschritt der Revision 7 am 2026-07-10

- Der finale strikte installierte Lifecycle-Lauf
  `output/playwright/business-os-local-installed-20260710-full-final-v32/`
  besteht 34/34 Apps auf dem tatsächlich installierten Stand. Alle Apps sind
  nach Öffnen und Schließen gesund; Console- und Page-Errors: null.
- Zwei getrennte Clean Profiles booteten und öffneten Notes gleichzeitig.
  Profil A erstellte einen Record über die sichtbare App-UI; Profil B sah ihn
  nach 6.898 ms. Profil B editierte ihn über den sichtbaren Editor; Profil A
  sah den finalen Stand nach 2.839 ms. Beide Profile blieben fehlerfrei.
- Reload in Profil A (2.387 ms) und ein neu erzeugtes Clean Profile
  (5.031 ms) fanden denselben finalen Stand.
- Nach echtem `ctox stop`/`ctox start` und neuem Service-PID fand ein weiteres
  Clean Profile denselben Record nach 7.398 ms. `status.ok=true`, null
  fehlerhafte oder reconnectende Collections, null Browserfehler.
- Der temporäre Testrecord wurde anschließend als Tombstone synchronisiert;
  die Notes-Unsynced-Zahl fiel von eins auf null und ein weiteres Clean Profile
  bestätigte seine Abwesenheit.

Revision 7 war noch kein vollständig grünes Production Gate, weil echter
Denial→Approval und Login/Logout nicht bestanden waren. Revision 8 schließt
diese beiden technischen Nachweise.

### Fortschritt der Revision 8 am 2026-07-10

- Das Release-Binary mit SHA-256
  `8fe36cfbb3e6bb877c929ef22ad580eba9f60feb1d99fcac997490fba41df2a0`
  wurde atomar als `ctox-real` installiert und mit neuem Service-PID gestartet.
- Ein echter Nicht-Admin-Command auf `notes_seed_ops_review` wurde nativ mit
  `data.write`, `role_or_scope_denied` und `allowed=false` abgelehnt. Der
  Decision-Shape liefert konsistent `requires_approval=true` sowie
  `delegation.available=true` und `threads.ctox_approval.request`.
- Der finale Approval-Zyklus lief mit getrennter Requester-/Reviewer-Identität:
  Request-Command `cmd_7cd69ae2-73d0-4f67-ad9d-d3239f4d414b`, Approval
  `approval_5fc235a9-1232-404f-85e2-0ddc4448e87a`, Decision-Command
  `cmd_e112c7e3-1feb-45f8-9343-002e511886f0`, reautorisierter Ziel-Command
  `cmd_30e06f94-3312-4131-97cb-1ac54be9e36c` und Queue-Task
  `queue:system::5b0411f71bddfc54c95aa020`.
- Threads und Command Bus ziehen in einem Clean Profile keine 10.000+ alten
  Thread-, Notification-, Command- oder Queue-Dokumente mehr vollständig.
  Append-heavy Projektionen bleiben push-fähig und werden über begrenzte
  Demand-Queries geladen. Threads rendert fachliche Daten vor optionaler
  Command-/Task-Anreicherung und revalidiert Approval-Status per Primärschlüssel.
- Der installierte Lauf
  `output/playwright/business-os-local-installed-20260710-full-final-v35/`
  besteht 34/34 Apps, jede mit `OK/OK` und null Console-/Page-Fehlern; Threads
  öffnet in 24 ms. Nach dem finalen V13-Embed-Rebuild, atomarer Installation
  und echtem Service-Restart bestätigt
  `output/playwright/business-os-local-installed-20260710-embedded-final-v37/`
  denselben 34/34-Stand gegen das dauerhaft eingebettete Artefakt.
- Der isolierte Browser/Rust-Smoke `business-os-auth-scope-ui` besteht Login,
  authentifizierten Reload, Logout, gesperrten Reload, Protected-Access-Block,
  Cross-Scope-Storage-Denial und final ausgeloggten Zustand. Browserfehler, 404
  und Request Failures: jeweils null. Für die lokale Auto-Admin-Installation
  bleibt Auth gemäß Gate-Definition `not-applicable`.
- Die temporäre Requester-Session wurde widerrufen, der User deaktiviert und
  sein QA-Grant deaktiviert. Keine Capability- oder Session-Tokens wurden in
  Evidence-Artefakten gespeichert.

Evidence:

- `output/playwright/business-os-approval-v34/business-os-approval-evidence.md`
- `output/playwright/business-os-approval-v34/pending-approval.png`
- `output/playwright/business-os-approval-v34/approved-v13-final.png`
- `output/playwright/business-os-local-installed-20260710-full-final-v35/business-os-qa-baseline.md`
- `output/playwright/business-os-local-installed-20260710-full-final-v35/business-os-qa-baseline.json`
- `output/playwright/business-os-local-installed-20260710-embedded-final-v37/business-os-qa-baseline.md`
- `output/playwright/business-os-local-installed-20260710-embedded-final-v37/business-os-qa-baseline.json`
- `runtime/build/business-os-auth-scope-final-v36.json`

### Fortschritt der Revision 9 am 2026-07-10

- Business Chat publiziert einen versionierten Layout-Contract mit realer
  Bounding-Box. `shell-chat-composition.js` leitet daraus gemeinsam mit der
  manifestierten Mindestarbeitsfläche der offenen Windows den Modus ab:
  Bottom-Dock bei ausreichender Höhe, Side-Dock bei ausreichender Breite und
  Dock-only Compact, wenn beides nicht möglich ist.
- Der Window Manager unterstützt vierseitige Insets, reflowt Normal-,
  Maximized- und alle Snap-Zustände, stellt temporär verdrängte Geometrie nach
  Collapse wieder her und bewahrt deklarierte Mindestgrößen. Die Taskbar wird
  bei expandiertem Chat zu einer 44-px-Rail im reservierten rechten Bereich.
- Der Stylesheet-Loader vergleicht jetzt die vollständige URL inklusive
  Cache-Buster. `index.html`, `app.css`, `app.js` und dynamische Shell-Imports
  stehen gemeinsam auf `20260710-chat-dock-composition-v16`.
- `assert-shell-chat-composition.mjs` testet die echten Chat-, Window-Manager-
  und Taskbar-Komponenten in fünf Phasen: Side-Dock, Pointer-Aktion,
  Pointer-/Keyboard-Taskbar, Collapse/Restore, Maximized, Bottom-Snap und
  900×600 Compact. App-/Dock- und Taskbar-/Dock-Occlusion sind null; 640×480
  Mindestfläche bleibt erhalten.
- Die installierte V16-Version wurde auf `http://127.0.0.1:8765/` interaktiv
  geprüft. Das reale Threads-Window liegt links vom Side-Dock; der Button
  `Freigabe anfragen` ist laut `elementsFromPoint` das oberste Pointerziel und
  wurde zusätzlich per koordinatenbasiertem Browser-Pointer aktiviert.
- Die installierte Taskbar minimiert und restauriert per Enter. Bei 900×600
  aktiviert die Shell automatisch Dock-only Compact, hält das CTOX-Window bei
  490 px Höhe und meldet jeweils 0 px² Window-/Dock- und Taskbar-/Dock-Overlap.
- Release-Binary und aktive Managed-Installation tragen SHA-256
  `2d0597d6463731b4ac2e675bd0037ebf97179fc762ff2fef3cff84fff3d7abc5`;
  Source-, Release- und Runtime-Webassets wurden bytegleich verifiziert.

Evidence:

- `output/playwright/shell-chat-composition-2026-07-10T21-01-11-459Z/shell-chat-composition.json`
- `output/playwright/shell-chat-composition-2026-07-10T21-01-11-459Z/shell-chat-composition-expanded.png`
- `output/playwright/business-chat-behavior-2026-07-10T20-52-32-406Z/business-chat-behavior.json`
- installierte interaktive V16-Prüfung im aktuellen Codex-Browserlauf

### Fortschritt der Revision 10 am 2026-07-10

- `business-os-threads-rightclick-ui` und `business-os-threads-scale-ui` sind
  jetzt getrennte Teile der zentralen `businessOsProductionSmokeModes` und
  damit verpflichtende Release-Gates. Registry-Selbsttest und Browser-Runner
  verlangen dieselben Evidence-Keys, Mindestmengen und Zeitbudgets.
- Der isolierte Smoke erzeugt reproduzierbar je 10.000 historische
  `business_commands`, `user_threads`, `user_thread_messages` und
  `user_notifications` im nativen RxDB-Store. Die Threads-App lädt davon nur
  begrenzte Fenster: 200 Thread-Zeilen, 200 zugehörige Messages und 50
  Notifications statt unbounded Initial Pulls.
- Der separat bestandene Matrix-Lauf des Scale-Gates rendert 200 sichtbare
  Thread-Zeilen in 8.163 ms und damit unter dem 30-Sekunden-Budget. Der
  Fixture-Seed benötigte 4.359 ms; alle Browser-/Netzwerkfehlerzähler sind null.
- Der Smoke verwendet den kanonischen Kontextanker
  `data-context-record-id/type/label`. Für Record
  `threads_rightclick_record` wurden Pointer `(283,688)`, Entity, Deep Link
  und Record-ID in `business-os-context-v2`, Payload und Client Context
  nachgewiesen.
- Die drei sichtbaren Modi entsprechen dem Produktvertrag: `Daten ändern`
  und `App ändern` persistieren für den simulierten Benutzer ohne
  Selbst-Ausführungsrecht je einen `threads.ctox_approval.request` mit
  Reviewer; `Frage stellen` persistiert direkt einen
  `business_os.context.ask` über den Typed Command Bus. Daten- und
  App-Delegation verwenden getrennte Kontext-Records, damit fachlich erlaubte
  Pending-Approval-Deduplizierung nicht mit Command-Persistenz verwechselt wird.
- Der gemeinsame Matrixlauf führt beide Gates nacheinander grün aus; der
  Right-click-Slice benötigte 35,249 Sekunden, der Scale-Slice 42,268 Sekunden.
  Die native
  Denial→Threads→Reviewer→Reauthorization-Projektion bleibt bewusst das
  getrennte Gate aus Revision 8; der Right-click-Gate verantwortet UI-Modi,
  Reviewer-Steuerung, exakten Kontext und Command-Persistenz. WebRTC-/
  Advanced-Status blieb gesund; Console Errors, Warnings, 404, Request
  Failures und Cache Repairs: jeweils null.

Evidence des bestandenen isolierten Laufs:

- `business_os_threads_scale_commands=10000`
- `business_os_threads_scale_threads=10000`
- `business_os_threads_scale_messages=10000`
- `business_os_threads_scale_notifications=10000`
- `business_os_threads_scale_visible_thread_rows=200`
- `business_os_threads_scale_first_render_ms=8163`
- `business_os_threads_scale_budget_passed=1`
- `business_os_threads_rightclick_source_context_captured=1`
- `runtime/build/business-os-smoke-matrix-summary.json`

### Fortschritt und Auditkorrektur der Revision 11 am 2026-07-11

Neu belastbar umgesetzt:

- `business-os-app-inventory.mjs` leitet die Core-Matrix aus der kanonischen
  Registry und den tatsächlichen Modulverzeichnissen ab. Der Guard verlangt
  exakt 34 Core-Apps, darunter `desktop` als Shell-Surface und `invoices` als
  normale App. `explorer` und `code-editor` werden zusätzlich und ausdrücklich
  als Compatibility-Surfaces geführt; sie ersetzen keine Core-App mehr.
- Der installierte Parameter-Runner prüft für jede normale App ohne Remount
  `window -> maximized -> focus -> window`, Containerbreiten 640/960/1180,
  Warm-Mount-Zeit, vollständige Ready-Zeit, sichtbare Modusreaktion,
  Runtime-Health und globale Console-/Page-/Request-Fehler.
- Der vollständige Source-Overlay-Lauf gegen die installierte Shell umfasst
  34/34 Core-Apps plus zwei Compatibility-Surfaces. Stand V46: Warm Mount p95
  42 ms, maximale sichtbare Mount-Zeit 44 ms, Interaktion p95 36,5 ms und kein
  Presentation-/Overflow-Fehler. Ein anschließend gefundener Monaco-Warm-
  Reopen-Fehler wurde in V47 behoben. Der abschließende V50-Gesamtlauf ist mit
  Warm Mount p95 69 ms, Interaktion p95 42,1 ms, 34/34 Core-Apps, beiden
  Compatibility-Surfaces sowie null Console Errors, Warnings und Request
  Failures grün.
- `explorer` und `code-editor` verwenden jetzt echte Container Queries statt
  Viewport-Media-Queries. Beide bestehen bei 638 px realer Content-Breite ohne
  horizontalen Root-Overflow.
- `research` blockiert den sichtbaren Mount nicht mehr mit 250+750+1500-ms-
  Empty-Data-Retries. Der initiale Refresh läuft mount-token-gesichert im
  Hintergrund; verspätete Ergebnisse dürfen nach Unmount nicht mehr rendern.
  Gemessener Source-Overlay-Warm-Mount: 40 ms statt 624 bis 3375 ms.
- Der Windowed Host besitzt nun denselben auto-derived Loading-Shadow-Vertrag
  wie der Tab-Pfad. Stale Fetches werden über einen Window-Mount-Token
  verworfen; der Shadow wird nach Erfolg oder Fehler entfernt.
- Die Design-Matrix übersetzt sichtbaren DE-/EN-Text wirklich und wartet den
  stabilen Theme-Endzustand ab. Ein toleranzbasierter Pixel-Diff mit explizitem
  Baseline-Update, Diff-JSON und unveränderlicher Normalprüfung ist vorhanden.
- Ein ausführbarer Accessibility-Contract prüft alle zwölf Design-Zellen auf
  Namen, Focus, Kontrast, Reduced Motion und Overflow. Er fand reale Light-
  Theme-Kontrastfehler der Success-/Warning-Badges; die zentralen semantischen
  Tokens wurden korrigiert. Der aktuelle 12-Zellen-Lauf ist grün.
- CI und Release führen Screenshot-Matrix, Visual Diff und Accessibility-
  Contract nach der Browserinstallation aus und laden die Artefakte hoch.

Zum Zeitpunkt des Revision-11-Audits technisch offen:

1. Die 12-fache Design-Lab-Matrix ist Infrastruktur- und Primitive-Evidence,
   aber noch keine Light/Dark/DE/EN/Compact/Standard/Wide-Evidence für jede
   einzelne App.
2. Der parametrisierte Lifecycle-Runner deckt Presentation, Overflow,
   Performance und Fehlergesundheit ab. Die app-spezifischen DoD-Stories für
   echte Daten, Filter, Sortierung, Editieren, Signature Action, erlaubte und
   verweigerte Fachaktion, Reload und Wiederaufnahme fehlen noch als
   vollständige 34-App-Matrix.
3. Der Template Store enthält weiterhin nur die historischen Templates
   `matching` und `documents`. Die fünf kanonischen Archetypen mit getrennten
   Namespaces, Tests, Context-/Command-/Grant-Beispielen und Zuständen sind
   nicht implementiert.
4. Der Runtime Starter wird weiterhin aus großen Rust-Strings erzeugt;
   Creator, MCP/Prepare-App-Source und Template Store sind deshalb noch nicht
   nachweislich ein einziger kanonischer Starter-/Compilerpfad.
5. Der neue Windowed Loading Shadow benötigt nach dem Source-Guard noch einen
   Browser-/Release-Nachweis auf der neu gebauten Installation.
6. Die menschlichen Product-/Design-/Security-/Privacy-Signoffs und die fünf
   historischen Hypoport-IDs bleiben offen.

Aktuelle ausführbare Evidence:

- `output/playwright/business-os-local-installed-source-overlay-contract-v46/business-os-qa-baseline.json`
- `output/playwright/business-os-local-installed-source-overlay-contract-v50/business-os-qa-baseline.json`
- `output/playwright/business-os-research-source-performance-v41/business-os-qa-baseline.json`
- `output/playwright/business-os-code-editor-reopen-v47/business-os-qa-baseline.json`
- `output/playwright/business-os-design-matrix/design-matrix.json`
- `output/playwright/business-os-design-diff/visual-diff.json`
- `output/playwright/business-os-accessibility-contract.json`

### Fortschritt der Revision 12 am 2026-07-11

Der in Revision 11 geöffnete Starter-/Archetypen-Track ist geschlossen:

- Eine versionierte Source-of-Truth liegt unter
  `src/apps/business-os/app-starter/v2/`. Die früheren großen, unbenutzten
  v1-Rust-String-Templates wurden entfernt; Rust bindet die kanonischen Assets
  ausschließlich mit `include_str!` ein.
- Die fünf Archetypen `record-workbench`, `queue-workflow`,
  `editor-document`, `automation` und `timeline-thread` sind als benannte
  Template-Store-Einträge vorhanden. Jede erzeugte Instanz erhält eine eigene
  Collection-Namespace, eigene App-ID/Entry-Pfade, Schema- und Testartefakte.
- Creator, direkter MCP-Create, `prepare_app_source` und Template Store
  materialisieren denselben v2-Starter. Der frühere MCP-Skip wurde entfernt;
  `prepare_app_source` führt den offiziellen Validator jetzt synchron aus.
- Der historische HTTP-Template-Installer wurde entfernt. HTTP und RxDB-
  Command-Pfad verwenden nun denselben serverseitig autorisierten, gestagten,
  validierten und atomar aktivierten Store-Pfad.
- Der Starter enthält direkte CRUD-, Filter-, Sort-, Import-, Edit-,
  Signature-Action-, Command-Bus- und Context-v2-Beispiele sowie Empty-,
  Error-, Offline- und Permission→Delegation-Zustände. DE/EN, Light/Dark und
  Containerzustände für 640/960/1180 px sind ausführbar gegatet.
- `assert-app-starter.mjs` rendert und validiert alle fünf Artefakte. Der
  native Rust-Test installiert alle fünf Archetypen über das echte Staging und
  beweist zusätzlich getrennte Namespaces zweier Template-Instanzen.
- `assert-app-starter-browser.mjs` prüft 60 Browserzellen
  (5 Archetypen × 3 Breiten × 2 Themes × 2 Sprachen), fünf CRUD-/Command-/
  Context-Flows sowie fünf verweigerte Permission→Delegation-Flows. Das Gate
  läuft als Teil von `qa:design-gates` in CI und Release.
- Die visuelle Browserprüfung fand und behob zusätzlich einen nicht klickbaren
  Permission-Button durch Empty-State-Überlagerung sowie eine fehlerhafte
  vertikale Navigation und eine leere responsive Hauptfläche nach Auswahl.

Neue Evidence:

- `output/playwright/business-os-app-starter-v2/report.json`
- `output/playwright/business-os-app-starter-v2/*.png`
- `business_os::store::tests::canonical_template_archetypes_share_staged_runtime_starter`
- `npm --prefix src/apps/business-os test`
- `npm --prefix src/apps/business-os run qa:design-gates`
- `cargo check --bin ctox --no-default-features`

Nach Revision 12 technisch offen (historischer Stand vor Revision 13):

1. Die per-App-Matrix für Light/Dark, DE/EN, Compact/Standard/Wide und
   Accessibility ist für die 34 bestehenden Apps noch nicht vollständig.
2. Die 34 app-spezifischen Daten-, Filter-, Sortier-, Edit-, Signature-,
   Rechte-/Delegations-, Reload- und Wiederaufnahmestories fehlen noch als
   vollständige Evidence-Matrix.
3. Der neue Windowed Loading Shadow und alle aktuellen Sourceänderungen
   benötigen einen Production-Smoke auf einer neu gebauten Installation.
4. Product-/Design-/Security-/Privacy-Signoffs und die historische Zuordnung
   der fünf Hypoport-Pilot-IDs bleiben offen.

Weiterhin organisatorisch offen:

- Security-/Privacy- und Design-Signoff bleiben organisatorische Freigaben des
  benannten Product Owners. Die Source-Gates sind grün, die
  Signoff-Dokumente bleiben bis zur menschlichen Freigabe auf
  `pending-signoff`; der installierte Production-Gate-Status ist rot.
- Die historische Bezeichnung „fünf Hypoport-Apps“ ist im Repository weiterhin
  nicht auf konkrete IDs abgebildet. Alle 34 Apps besitzen zwar den
  Presentation Contract; ohne IDs kann die explizit verlangte Pilotkohorte
  jedoch nicht als solche gegen ihre vollständige DoD ausgewiesen werden.

### Fortschritt der Revision 13 am 2026-07-11

Revision 13 schliesst die bisher fehlende aktuelle Source-, Fachvertrags- und
Installed-Evidence, ohne die verbleibenden Langlauf- und Signoff-Risiken
wegzudefinieren:

- Der offizielle Source-Validator besteht 34/34 Apps. Fehlende kanonische
  Testeinstiege fuer Matching, Reports, Shiftflow, CTOX, Buchhaltung, Desktop,
  Knowledge und Research wurden ergaenzt.
- `qa/app-quality-contracts.json` beschreibt fuer alle 34 Core-Apps die
  Fachstory-Signale fuer Daten, Filter, Sortierung, Editieren, Signature
  Action, Context/Command, Permission/Delegation, Reload und Resume. Der
  strikte Auditor meldet 34 Apps und null unbelegte statische Signale, null
  Locale-, Container- oder Testluecken.
- Creator bietet die exakt fuenf kanonischen Archetypen explizit an und reicht
  den gewaehlten Archetyp in Manifest, Command-Payload und Client Context
  weiter. CTOX kann abgeschlossene oder blockierte Aufgaben direkt ueber den
  Typed Command Bus als Folgeauftrag fortsetzen; die Aktion haengt nicht von
  einem gemounteten Chat ab.
- Der Source-Browserlauf
  `output/playwright/business-os-source-full-en-dark-v73/` besteht 36/36
  Oberflaechen: 34 Core-Apps plus Files und Source Editor. Geprueft wurden
  Registry/App Store, 640/960/1180 px, Window/Maximized/Focus/Restore,
  Runtime-Health sowie Console- und Request-Fehler.
- Die Design-Gates bestehen die zwoelf Width/Theme/Locale-Zellen, Visual Diff,
  Accessibility, Reduced Motion, Focus, Kontrast und Overflow. Der kanonische
  Starter besteht zusaetzlich 60 Browserzellen und fuenf reale
  Permission-zu-Delegation-Flows.
- AppSec-Listen sind begrenzt und sekundaere Formulare als tastaturbedienbare
  Disclosures ausgefuehrt. Der sichtbare Interaktionswert sank im Zieltest von
  rund 482 ms auf 40,1 ms. Window-Maximize/Restore/Snap animiert keine
  Layout-Eigenschaften mehr ueber `transition: all`.
- Ein Release wurde direkt aus dem aktuellen Worktree mit `install.sh`
  gebaut und erfolgreich zu 100 % in
  `~/.local/lib/ctox/releases/v0.3.31-353-g846392ba0-dirty` aktiviert. Der
  installierte Creator enthaelt nachweislich den aktuellen Archetypenstand;
  der Business-OS-Webserver liefert den installierten Build auf Port 8765.
- Der sichtbare Installed-Smoke deckt alle 36 Oberflaechen in drei sauberen
  Chromium-Profilen ab. Batch A und B bestehen je 12/12. Batch C besteht
  10/12 im gemeinsamen Lauf; die dort durch Daten-/Browserlast verspäteten
  Apps Support und Tickets bestehen unmittelbar danach isoliert jeweils 1/1.
  Alle erfolgreichen Zieltests liegen innerhalb der Warm-Mount- und
  Interaktionsbudgets und melden null Console Errors.
- Zwei Versuche, alle 36 installierten Apps in genau einem sichtbaren
  Browserprozess zu sequenzieren, erzeugten nach CV Print Builder anhaltende
  Chrome-GPU-/Rendererlast. CV Print Builder selbst besteht isoliert
  reproduzierbar mit 39,6 ms Interaktionszeit; Documents besteht ebenfalls.
  Der Befund bleibt deshalb als Langlauf-/Umgebungsrisiko offen und wird nicht
  durch die drei gruenen Teilmatrizen ersetzt.
- Neu gestartete `ctox-real`-CLI-Prozesse blieben auf diesem Rechner bereits in
  macOS `_dyld_start` haengen, waehrend der installierte Service und der
  Business-OS-Webserver weiter gesund antworteten. Das ist kein SQLite- oder
  App-Mount-Stack, verhindert aber einen frischen CLI-Status-/Peer-Nachweis und
  bleibt als Installationsbefund offen.

Aktuelle Evidence:

- `output/playwright/business-os-source-full-en-dark-v73/business-os-qa-baseline.json`
- `output/playwright/business-os-app-quality-audit.json`
- `output/playwright/business-os-app-starter-v2/report.json`
- `output/playwright/business-os-appsec-window-perf-v67/business-os-qa-baseline.json`
- `output/playwright/business-os-installed-visible-batch-a-v76/business-os-qa-baseline.json`
- `output/playwright/business-os-installed-visible-batch-b-v76/business-os-qa-baseline.json`
- `output/playwright/business-os-installed-visible-batch-c-v76/business-os-qa-baseline.json`
- `output/playwright/business-os-installed-support-v76/business-os-qa-baseline.json`
- `output/playwright/business-os-installed-tickets-v76/business-os-qa-baseline.json`

Nach Revision 13 technisch offen:

1. Der installierte 34+2-Lifecycle muss nochmals in einem sauberen
   Ein-Prozess-Browserprofil ohne GPU-/Renderer-Runaway bestehen; der aktuelle
   vollstaendige Nachweis ist app-vollstaendig, aber auf drei Profile plus zwei
   isolierte Wiederholungen verteilt.
2. Die statischen 34-App-Fachvertraege muessen fuer risikoreiche Apps noch
   durch dynamische Stories mit echten Datenmutationen, erlaubter und
   verweigerter Aktion, Persistenz, Reload und Resume vertieft werden. Die
   bereits vorhandenen skalierten Context-/Approval- und Persistenz-E2Es
   bleiben gueltige Plattformnachweise, ersetzen aber nicht jede Fachvariante.
3. Der macOS-Loaderbefund fuer neue `ctox-real`-CLI-Prozesse muss isoliert und
   der frische Status-/Peer-Smoke wiederholbar gemacht werden.
4. Product-, Design-, Security- und Privacy-Signoff sowie die Zuordnung der
   fuenf historischen Hypoport-Pilot-IDs bleiben menschliche beziehungsweise
   produktseitige Restpunkte.

### Revision 14: interaktive Shell-Korrektur am 2026-07-11

Eine sichtbare Abnahme waehrend des installierten Langlaufs hat gezeigt, dass
die bisherigen Presentation-Smokes zu technisch und zu wenig
benutzerorientiert waren. Der Screenshot des realen Zustands zeigte ein fast
vollflaechiges IoT-Fenster, mehrere alte QA-Chatfenster und eine nicht mehr
klar sichtbare beziehungsweise nicht frei nutzbare untere Arbeitsflaeche.
Damit gelten die folgenden Punkte ab sofort als verbindlicher Teil dieses
Plans und nicht als spaetere kosmetische Nacharbeit:

1. **Echtes Window-Handling statt Style-Mutation.** Jede der 35
   fensterfaehigen Oberflaechen wird ueber einen sichtbaren Launcher mit einem
   einzelnen Klick geoeffnet, am unteren und diagonalen Resize-Griff per
   echtem Maus-Drag verkleinert und vergroessert, maximiert, wiederhergestellt,
   minimiert, ueber die Taskbar wiederhergestellt und ueber den sichtbaren
   Close-Button geschlossen. `desktop` wird als Shell-Surface separat geprueft.
2. **Pane-Nutzbarkeit ist ein eigenes Gate.** Fruehere Fullscreen-Apps mit zwei
   oder drei Spalten muessen ihre linke Navigation, Hauptflaeche und rechte
   Kontext-/Detailflaeche in Window, Maximized und Mobile sinnvoll erhalten.
   Ein ausgeblendetes Pane braucht einen sichtbaren Drawer-/Stack- oder
   Zurueckweg. Root-Overflow allein ist keine Evidence.
3. **Chat bleibt Nutzerentscheidung.** Historische oder im Hintergrund
   abgeschlossene Chats duerfen ein vom Nutzer eingeklapptes Dock nicht
   selbststaendig wieder oeffnen und dadurch App-Fenster waehrend eines Drags
   reflowen. Der kompakte Chat-Button bleibt immer sichtbar.
4. **Mehrfachchat skaliert deterministisch.** Ein-/Ausblenden sowie die
   Zustaende mit 1, 6, 8, 12, 100 und 1000 Chats werden real geprueft. Chips
   und Overflow sind tastatur- und mausbedienbar, horizontal scrollbar und
   bleiben im Viewport. Im 340-px-Seitendock wird genau der aktive Chat
   gerendert; inaktive, ehemals absolut positionierte Chats duerfen nicht aus
   dem Viewport ragen.
5. **Vereinfachte Shell-Navigation.** Oben links bleibt nur das Startmenue-
   Symbol. Die separaten Symbole fuer Desktop und CTOX/Workflow sowie die dort
   obsolete Navigationsgruppe werden entfernt; Desktop und Apps bleiben ueber
   das Startmenue erreichbar.
6. **Single Click ist der Web-App-Standard.** App-Icons des Business-OS-
   Desktops starten mit einem Klick. Ein Doppelklick bleibt nur dort zulaessig,
   wo er fachlich einer Desktop-Dateiinteraktion entspricht, etwa im Explorer.
   Dragging eines Icons darf keinen App-Start ausloesen.
7. **Window-Header traegt den App-Lifecycle.** Direkt hinter App-Icon und
   App-Name stehen Version und Status wie privat, Team/public, Preview,
   modifiziert oder neu. Vor Minimize/Maximize/Close liegen sichtbare,
   berechtigungsgepruefte Aktionen fuer Source Editor und
   Versionen/Lifecycle/Rollback. Diese Aktionen verwenden dieselben nativen
   Permission- und Command-Pfade wie der bisherige Fullscreen-App-Bar.
8. **Responsive Shell mit Mobile-Untergrenze.** Die gesamte Business-OS-Shell,
   nicht nur App-Inhalte, funktioniert von Desktop bis zu einer expliziten
   mobilen Mindestbreite. Unterhalb der Mobile-Untergrenze wird nicht weiter
   verkleinert. Auf Mobile werden Fenster als bedienbare App-Sheets/
   Vollflaechen dargestellt, Startmenue, Headeraktionen, Zurueckweg, Chat und
   Taskbar bleiben erreichbar und Touch-Targets ausreichend gross.
9. **Visuelle Abnahme ist eigenstaendig.** Fuer alle Apps werden Screenshots
   nach dem echten Resize erzeugt. Zusaetzlich werden Initial-Shell, dichter
   Mehrfachchat, Side-Dock, 900-px-Shell und Mobile-Shell als eigene
   Bildartefakte geprueft. Leere, abgeschnittene, verrutschte oder durch
   andere Shell-Flaechen verdeckte Regionen sind harte Fehler.
10. **Ableton-Dichte ist ein Responsive-Vertrag.** Wiederkehrende Bedienung
    wie Import, Suche, Filter, Sortierung, Auswahl, Editieren, Export und
    Pane-Navigation verwendet eine flache, kompakte Operational-Chrome ohne
    generische Hero-Karten oder grossflaechige Buttons. Pro sichtbarem
    Arbeitsbereich darf hoechstens die fachlich einzigartige Automation als
    Hero-/Signature-Action Farbe und zusaetzlichen Raum beanspruchen. Auf
    kleinen Fenstern und Mobile werden Labels kontrolliert gekuerzt oder in
    Tooltips/Overflow ueberfuehrt; Funktion, Status und Rueckweg bleiben
    erhalten.
11. **Desktop-Labels bleiben rasterstabil.** Lange App-Namen belegen hoechstens
    zwei typografisch kontrollierte Zeilen innerhalb einer festen Icon-Zelle.
    Normale Wortgrenzen werden bevorzugt, untrennbare technische Namen duerfen
    kontrolliert umbrechen; danach greift Ellipsis/Clamping und der vollstaendige
    Name bleibt per Tooltip und Accessible Name verfuegbar. Kein Label darf
    Nachbar-Icons verschieben oder in die naechste Rasterzelle ragen.
12. **Creation- und Deploy-Skills bleiben synchron.** Root `PRODUCT.md`,
    `DESIGN.md` und `.impeccable/design.json`, der kanonische
    `business-os-app-module-development`-Skill sowie
    `/Users/michaelwelsch/Documents/ctox-business-os-deploy-skill/ctox/`
    enthalten denselben Operational-Density-, Signature-Automation-,
    Window-/Pane-/Mobile- und Browser-Evidence-Vertrag. Ein alter Deploy-Skill
    darf keine `full-workspace`-App oder generische Dashboard-Shell mehr
    erzeugen.

Bereits durch Revision 14 gefundene und in Bearbeitung befindliche Defekte:

- inaktive Mehrfachchat-Fenster behielten beim Wechsel in den Seitendock alte
  absolute X-Positionen und lagen ausserhalb des Viewports,
- historische beziehungsweise neue Hintergrundantworten oeffneten ein
  eingeklapptes Dock erneut und reduzierten waehrend der Arbeit die nutzbare
  Fensterhoehe,
- native Taskbar-Buttons behandelten Enter sowohl im eigenen Keydown-Handler
  als auch im synthetisierten Browser-Click und konnten doppelt toggeln,
- der bisherige 34+2-Runner verwendete fuer Presentation-Checks direkte
  Manager-/Style-Aufrufe und konnte echte Drag-, Launcher- und Controlfehler
  deshalb nicht ausschliessen,
- die 2-/3-Pane-Bedienbarkeit und Mobile-Shell waren nicht Teil des bisherigen
  per-App-Gates.

Revision-14-Evidence wird unter
`output/playwright/business-os-interactive-window-*` und
`output/playwright/shell-chat-composition-*` abgelegt. Der neue Lauf gilt erst
als abgeschlossen, wenn alle 35 Fenster-Apps plus Desktop/Shell gruen sind und
die Screenshots separat visuell geprueft wurden.

## 1. Executive Decision

CTOX Business OS wird von einer Sammlung historisch gewachsener Module zu einer
klar definierten App-Plattform refaktoriert.

Jede vorhandene App wird mindestens einmal vollständig gegen den neuen
Plattformvertrag geprüft, überarbeitet und neu abgenommen. Fachliche Logik,
RxDB-Schemas und funktionierende Workflows werden nicht pauschal neu
geschrieben. Erneuert werden die gemeinsamen Verträge für Präsentation,
responsive Darstellung, Standard-UI, Kontextaktionen, Command Bus,
Berechtigungen, Delegation, Themes, Loading und Tests.

Die bisherige Einteilung in `windowed` und `full-workspace` wird von der
App-Identität getrennt:

- Jede normale Business-App unterstützt eine responsive Windowed-Variante.
- Dieselbe App kann maximiert und optional in einem Focus-Modus laufen.
- Window, Maximized, Snap und Focus verwenden denselben Mount und Zustand.
- Minimized und die vorhandenen Snap-Zonen bleiben Window-Manager-Zustände.
- Nur echte Shell-Oberflächen wie der Desktop dürfen ausschließlich eine
  Vollflächen-Surface sein.

Normales lokales App-CRUD bleibt ein direkter RxDB-/Sync-Engine-Pfad. Es wird
nicht pauschal in Commands umgebaut. Der native Sync-Peer muss dafür exakte
`data.read`- und `data.write`-Entscheidungen erzwingen. Automationen,
Agentenaktionen, privilegierte Mutationen, App-Änderungen, Freigaben und
Delegationen laufen dagegen immer über den Typed Command Bus.

Die Migration erfolgt nicht als Big Bang. Zuerst werden der aktuelle Worktree
gesichert, der tatsächliche Ist-Stand inventarisiert und zwei parallele
Fundament-Tracks aufgebaut. Erst danach folgen Context Actions, Creator/Store,
Piloten und die App-Wellen.

## 2. Was Revision 2 gegenüber dem ersten Plan ändert

Das Review mit 33 Agents und adversarialen Gegenprüfungen hat folgende
Änderungen ausgelöst:

1. Der Plan unterscheidet jetzt zwischen `HEAD`, uncommitted Worktree und
   Zielzustand.
2. Acht der neun aktuell als Windowed sichtbaren Apps werden als in-flight
   Migration behandelt, nicht als bereits gelandete Plattformbasis.
3. Der bestehende Rechtsklick-v1-Vertrag wird erweitert und dual gelesen,
   nicht durch einen Greenfield-Vertrag ersetzt.
4. Der dominante Nicht-Command-Pfad, direkte RxDB-Schreibvorgänge, ist jetzt
   ausdrücklich entschieden und abgesichert.
5. Bereits vorhandene Release-, Update-, Rollback-, Session- und Peer-
   Mechanismen werden wiederverwendet, nicht neu gebaut.
6. Die bestehenden `.ctox-*`-Klassennamen bleiben stabil. Das Design System v2
   kuratiert Tokens und Verhalten, ohne eine zweite 34-App-Markup-Migration zu
   erzwingen.
7. Auto-derived Loading Shells aus `index.html` und `index.css` bleiben eine
   verbindliche Invariante.
8. Presentation-, Design- und Policy-Arbeit laufen teilweise parallel und
   treffen sich an expliziten Join Gates.
9. Visual Regression, Accessibility, Locale-/Width-Tests und der
   parametrisierte App-Contract-Test werden vor den Piloten als Infrastruktur
   gebaut.
10. Zwei frühe Risiken werden vorgezogen: Landing/Reconcile der Windowed-
    Arbeit und Delete-before-copy im Store-Install-Pfad.

### 2.1 Was Revision 4 nach dem installierten Browser-E2E ändert

1. „Source-Gates grün“ und „lokal installiert produktionsreif“ sind getrennte
   Zustände. Nur der erste ist erreicht.
2. Ein strikter QA-Modus darf keine Source-Manifeste in die installierte
   Registry injizieren und muss auf einen tatsächlich gesunden Shell-/Sync-
   Status warten.
3. Ein kompletter sequenzieller App-Lifecycle-Test und ein paralleler
   Zwei-Profil-Test werden zu Release-Gates.
4. Context-Payload, Browser-Dispatch, native Persistenz, Consumer-Ausführung
   und Approval-Delegation werden als fünf getrennte Prüfpunkte behandelt.
5. Transport-/Query-Owner-Fehler dürfen nicht als app-spezifische Mount-Fehler
   fehlklassifiziert werden.
6. Login/Logout kann erst abgenommen werden, wenn die Zielinstallation einen
   echten Auth-Gate besitzt; Auto-Admin im lokalen Profil ist dafür keine
   Evidence.

## 3. Verifizierter Ist-Stand

### 3.1 Modulbestand im aktuellen Worktree

Der aktuelle Bestand unter `src/apps/business-os/modules/` umfasst:

- 34 Module mit eigenem `index.js`
- 34 Module mit eigenem `index.css`
- ungefähr 61.670 Zeilen Modul-JavaScript
- ungefähr 27.259 Zeilen Modul-CSS
- 9 im Worktree als `windowed` deklarierte Module
- 23 im Worktree als `full-workspace` deklarierte Module
- 2 Module ohne eindeutige `layout.shell`-Angabe
- mindestens 15 Module mit `contextmenu`-Handlern irgendwo im Modulbaum

Die 34 Matrixzeilen und die Wellenpartition sind vollständig. Die
Hypoport-Kohorte ist im Repository nicht markiert.

### 3.2 HEAD versus uncommitted Windowed-Arbeit

Nur `browser` ist in `HEAD` bereits als Windowed-Modul etabliert. Folgende acht
Umstellungen existieren aktuell nur als uncommitted Worktree-Änderungen:

- `app-store`
- `appsec-pentest`
- `creator`
- `ctox`
- `knowledge`
- `reports`
- `threads`
- `tickets`

Diese Änderungen umfassen nicht nur `layout.shell = "windowed"`, sondern einen
de-facto Presentation Contract:

- `launch_kind = "desktop-app"`, teilweise auf Root- und Layout-Ebene
- `default_width` und `default_height`
- `min_width` und `min_height`
- Anpassungen an `app.js`, `window-manager.js`, Desktop Launcher, Registry und
  Guards

Phase 0 muss diese Arbeit zuerst einem Eigentümer zuordnen, gegen `HEAD`
reconcilen, testen und als separaten, reviewbaren Stand landen. Kein weiterer
Planabschnitt darf sie als dauerhaft vorhanden voraussetzen, bevor dieses Gate
erfüllt ist.

### 3.3 Bereits vorhandene Plattformbausteine

Der Refactor beginnt nicht auf leerer Fläche. Bereits vorhanden sind:

- ein 43-Felder-`mount(ctx)`-Vertrag, gepinnt durch
  `scripts/assert-module-context-contract.mjs` und dokumentiert in
  `docs/business-os-module-context.md`
- ein globales Shell-Kontextmenü mit Capture-Phase-Handler
- der dokumentierte v1-Rechtsklick-Contract in
  `src/apps/business-os/ARCHITECTURE.md`
- kanonische Attribute `data-context-record-id`,
  `data-context-record-type`, `data-context-label`
- Fallbacks über `data-*-id`, Element-ID und Pane-Klassennamen
- `selected_text`, `clicked_text`, Actor und `visible_scope` im bestehenden
  Context-/Command-Payload
- `threads.ctox_approval.request` inklusive nativer Re-Autorisierung bei der
  Freigabe
- ein Shared Context Menu mit Keyboard-Navigation innerhalb des geöffneten Menüs
- ein Design-System-Kit in `shared/base.css`, auf das alle 34 Module am
  2026-07-06 migriert wurden
- grüne Token- und Modul-Conformance-Gates
- Window Manager mit Minimize, Maximize, Restore und acht Snap-Zonen
- workspace- und actor-spezifische Fenstergeometrie in RxDB plus lokalem Cache
- ein Catalog-Update-Pfad mit Staging, atomarem Swap, Backup-Restore,
  Recovery-Versionen, Update-Badge und Rollback
- Release-Records, Data-Access-Review-Gate sowie getrennte Release-/Rollback-
  Rechte
- Session-Revocation und Peer-Revocation
- automatisch aus echtem `index.html` und `index.css` abgeleitete Loading Shells

### 3.4 Verifizierte Restlücken

Die Arbeit konzentriert sich auf folgende Deltas:

1. Gewöhnliche Collection-Reads und -Writes werden im nativen Sync-Pfad nicht
   gegen exakte `data.read`-/`data.write`-Grants geprüft.
2. Capability-Tokens prüfen Signatur und Ablauf, aber keine Actor Epoch, `jti`
   oder Revocation.
3. Der native Denial Path erzeugt einen Fehler und Audit, aber keinen
   delegierbaren Threads-Vorgang.
4. Window-Events für Chat-/Agentenaktionen können ohne gemountete Chat-Surface
   stumm verloren gehen, obwohl erfolgreiche Zustellung später in einem
   Command endet.
5. Der globale Context Handler verwendet `state.activeModule` und kann einen
   Klick in einem Windowed-Modul dem Hintergrundmodul zuordnen.
6. Dem v1-Kontext fehlen `field.path`, Pointer-Koordinaten und strukturierte
   Multi-Selection-IDs.
7. Viele der 15 lokalen Kontextmenüs sind durch den Capture-Handler bereits
   toter Legacy-Code; App Store, Desktop und native Editor-Surfaces bleiben
   echte Sonderpfade.
8. Container Queries sind nur in zwei Modulen vorhanden; der Window Host ist
   kein Container.
9. `full-workspace` wird noch durch Validator und Skill-Dokumente für
   runtime-installierte Apps erzwungen.
10. Das sanktionierte Kit selbst verwendet große Radien, Glass, Gradients und
    starke Schatten; einige Module besitzen zusätzliche harte Ausreißer.
11. Release-Records existieren, aber SemVer steuert weiterhin Sichtbarkeit in
    Native und Browser.
12. Release führt Validator, Smoke und E2E nicht aus.
13. `install_app_module` löscht bei Reinstall das bestehende Ziel vor dem Copy.
14. Install, Template Install und Update besitzen keine einheitliche
    Pre-Activation-Validation und keine Signaturprüfung.
15. Creator, MCP-Create, Prepare-App-Source und Template Store starten aus
    unterschiedlichen Quellen und validieren unterschiedlich.
16. Der App-Development-Skill ist für Remote-Agenten nicht als Ressource
    abrufbar.
17. Same-Origin-Module sind kein Sicherheitsboundary für offenen Third-Party-
    Code.
18. Visual-Diff-, Accessibility-, Reduced-Motion- und gerenderte Locale-/Width-
    Gates fehlen.

### 3.5 Reproduzierbare Evidence Anchors

| Bereich | Aktuelle Referenz |
| --- | --- |
| Collection Read Policy | `src/core/business_os/policy.rs::role_may_read_collection` |
| Record Read/Write Hooks | `src/core/business_os/threads.rs::may_replicate_document`, `may_accept_peer_write` |
| Capability Verification | `src/core/business_os/capability.rs::verify_capability_token` |
| Exact Grants | `src/core/business_os/store.rs::active_permission_grant_allows` |
| Release Visibility | `src/core/business_os/store.rs::projected_module_lifecycle` |
| Staged Update | `src/core/business_os/store.rs::update_module_to_catalog` |
| Gefährlicher Install-Pfad | `src/core/business_os/store.rs::install_app_module` |
| Global Context | `src/apps/business-os/app.js::handleGlobalContextMenu` |
| Context Detection | `src/apps/business-os/app.js::extractGlobalCtoxContext`, `detectRecordFromElement` |
| Windowed Mount | `src/apps/business-os/app.js::openWindowedModule` |
| Auto Loading Shell | `src/apps/business-os/app.js::applyLoadingShadow` |
| Window States | `src/apps/business-os/shared/window-manager.js` |
| Kit | `src/apps/business-os/shared/base.css` |
| Shell Tokens | `src/apps/business-os/app.css` |
| Conformance | `src/apps/business-os/scripts/assert-module-conformance.mjs` |
| App Validator | `src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs` |

## 4. Zielprinzipien

CTOX Business OS wird als kompaktes, zeitloses Arbeitsinstrument gestaltet.
Ableton Live dient als Referenz für Dichte, räumliches Gedächtnis, direkte
Interaktion und funktionalen Farbeinsatz, nicht als zu kopierende Oberfläche.

- Wiederkehrende Arbeit ist visuell leise und überall konsistent.
- Import, Suche, Filter, Sortierung, Auswahl, Editieren und Export verwenden
  dieselben Primitive und Positionierungsregeln.
- Jede App besitzt höchstens eine besonders präsente Signature Action pro
  sichtbarem Arbeitsbereich.
- Wiederkehrende Controls belegen nur ihre notwendige Groesse: kompakte
  Toolbar-Hoehen, kleine Status-Chips, icon-first bei knapper Breite und keine
  dekorativen Grosskarten. Touch-Modus vergroessert die Trefferflaeche, nicht
  die visuelle Masse des gesamten Layouts.
- Farbe zeigt Auswahl, Status, Semantik oder die einzigartige Automation an.
- Light und Dark sind zwei Renderings desselben räumlichen Systems.
- Die erste Ansicht zeigt echte Arbeit, keine Hero- oder Dashboard-Dekoration.
- Rechte, Sync, Ausführung und Fehlerzustand sind sichtbar und verständlich.
- Deutsch und Englisch werden in allen unterstützten Größen getestet.
- Bestehende belastbare Verträge werden erweitert, nicht parallel neu erfunden.

## 5. Zielarchitektur

### 5.1 Plattformschichten

```text
Native Policy, Sync Hooks, Store, Release und Audit
                       |
                Typed Command Bus
                       |
       App SDK und Context Action Service
                       |
       Presentation Runtime und Window Manager
                       |
        Bestehendes Kit, Tokens und Patterns
                       |
              Templates und App Creator
                       |
                  Business Apps
```

Keine App darf eine untere Schicht durch eine eigene HTTP-Datenbrücke,
UI-only-Rechteprüfung oder ad hoc gestartete Prozesse umgehen.

### 5.2 Presentation Contract

App-Identität und Präsentationsmodus werden getrennt. Ein möglicher finaler
Vertrag ist:

```json
{
  "presentation": {
    "default_mode": "window",
    "supported_modes": ["window", "maximized", "focus"],
    "initial_size": { "width": 1120, "height": 760 },
    "minimum_size": { "width": 640, "height": 480 },
    "multi_instance": false
  }
}
```

Die endgültigen Feldnamen werden in einem ADR festgelegt. Die Migration ist
nicht nur ein Mapping von `layout.shell`. Folgende bestehende Felder und
Aliase werden gleichzeitig aufgenommen:

- `layout.shell = full-workspace`
- `layout.shell = windowed`
- `layout.shell = desktop-window`
- `layout.full_workspace`
- `layout.fullFrame`
- `layout.launch_kind`
- Root-`launch_kind`
- `default_width`, `default_height`, `min_width`, `min_height`
- doppelte Werte in `module.json` und `modules/registry.json`

Der Compatibility Reader akzeptiert während der Migration Alt und Neu. Writer
schreiben nach dem Cutover nur noch den kanonischen Vertrag.

#### Präsentationsmodi und Window-Zustände

| Kategorie | Werte | Bedeutung |
| --- | --- | --- |
| App-Modus | `window`, `maximized`, optional `focus` | vom Manifest unterstützte Darstellung |
| Window-Zustand | `normal`, `minimized`, `maximized` | bestehender Window-Manager-State |
| Snap-State | acht vorhandene Snap-Zonen | bestehende Geometrievariante |
| Shell-Ausnahme | `shell-surface` | nur Desktop oder explizit beschlossene Root-Surface |

Regeln:

- Jede normale App unterstützt `window` und `maximized`.
- `focus` ist optional und fachlich begründet.
- Minimize, Maximize, Restore und Snap verwenden den vorhandenen Window Manager.
- Der Wechsel remountet die App nicht.
- `window` zu `focus` darf Auswahl, Scrollposition und Entwürfe nicht verlieren.
- Single-Window bleibt Standard. Multi-Instance ist opt-in und verwendet eine
  Instance-ID in Owner Key, Deep Link und Geometrie-Key.
- Unterhalb der manifestierten Mindestbreite verhindert der Window Manager
  weiteres Verkleinern. Responsive Verhalten unterhalb dieser Breite ist kein
  impliziter Vertrag.
- Touch ist entweder ein expliziter späterer Track oder ein dokumentiertes
  Nichtziel der ersten Migration.

#### Registry und Guards

`module.json`, `modules/registry.json`, eingebetteter Fallback Catalog,
`moduleActivationSignature`, Creator, Desktop Launcher und native Projektionen
werden in einem atomaren Contract-Schritt migriert.

Insbesondere werden gleichzeitig aktualisiert:

- `modules/desktop/registry-launch-smoke.mjs`
- `scripts/assert-module-context-contract.mjs`
- `scripts/assert-global-context-menu-policy.mjs`
- `scripts/validate-app-module.test.mjs`
- `module_static_check.mjs`
- die sieben Skill-Referenzen mit Full-Workspace-Mandat

Guards werden nicht gelöscht oder geschwächt. Sie werden auf den neuen Vertrag
umgestellt.

#### Deep Links und Wiederherstellung

- Ein Deep Link identifiziert App, Record und optional Window Instance.
- Öffnen eines Links fokussiert ein existierendes Window oder öffnet es neu.
- Windowed Links werden nicht auf einen Hintergrund-Tab-Hash umgeschrieben.
- Launch Args sind in Tab-, Window- und Focus-Modus identisch verfügbar.
- Vorhandene Geometrie wird weiterverwendet.
- Ob beim Neustart alle zuvor offenen Fenster automatisch wieder geöffnet
  werden, ist eine Phase-0-Entscheidung und keine bereits vorhandene Funktion.

### 5.3 Responsive Workbench

Der Window Host erhält `container-type: inline-size`. Apps reagieren auf ihren
Container, nicht auf die Browser-Viewport-Breite.

| Zustand | Richtwert | Darstellung |
| --- | --- | --- |
| Compact | 640 bis 759 px | eine Hauptfläche, Navigation und Inspector als Drawer |
| Standard | 760 bis 1199 px | Hauptfläche plus eine kontextabhängige Nebenfläche |
| Wide | ab 1200 px | linke Navigation, Zentrum und rechter Inspector gleichzeitig |

Die Pilotapps kalibrieren die finalen Grenzen. Apps dürfen andere Pane-
Proportionen verwenden, aber keine eigene responsive Grundmechanik bauen. Die
Shell liefert Pane-Stacking, Drawer-Übergang, Resizer, Scroll Ownership und
Layout-Persistenz.

Im Windowed-Pfad erhalten `ctx.left` und `ctx.right` keine bedeutungslosen
Throwaway-Divs mehr. Entweder liefert der Presentation Host echte Slots oder
der Vertrag markiert die Slots als nicht vorhanden und stellt die gemeinsame
Drawer-/Pane-API bereit.

### 5.4 Context Action Service v2 als Erweiterung von v1

Der heutige Contract bleibt während der Migration gültig:

- `module`
- `column`
- `record_type`
- `record_id`
- `label`
- `deep_link`
- `selected_text`
- `clicked_text`
- `visible_scope`

Die v2-Erweiterung ergänzt:

- exakte Window-/App-Attribution statt `state.activeModule`
- `surface_id` und `pane_id`
- Collection und Entity
- `field.path`
- strukturierte Multi-Selection-IDs
- Pointer-Koordinaten
- Presentation Mode und Window Instance

Apps können Targets explizit registrieren:

```js
ctx.contextActions.register(element, {
  surface: 'ticket-timeline',
  pane: 'center',
  entity: {
    collection: 'ctox_tickets',
    type: 'ticket',
    id: ticket.id,
    label: ticket.title
  },
  field: { path: 'status' },
  selection: () => ({ ids: selectedTicketIds }),
  actions: ['context.ask', 'data.modify', 'app.modify']
});
```

Der normalisierte v2-Kontext enthält mindestens:

```json
{
  "schema_version": "business-os-context-v2",
  "app_id": "tickets",
  "window_instance_id": "tickets:default",
  "surface_id": "ticket-timeline",
  "pane_id": "center",
  "presentation_mode": "window",
  "entity": {
    "collection": "ctox_tickets",
    "type": "ticket",
    "id": "ticket_123",
    "label": "Drucker ausgefallen"
  },
  "field": { "path": "status" },
  "selection": { "ids": ["ticket_123"], "text": "" },
  "pointer": { "x": 410, "y": 286 },
  "deep_link": "#tickets?record=ticket_123"
}
```

#### Migrationsregeln

- `data-context-record-id/-type/-label` werden automatisch in v2-Targets
  gemappt.
- `data-left-content` und `data-right-content` bleiben während der Migration
  Pane-Marker.
- Native und Browser-Consumer akzeptieren v1 und v2 parallel.
- V1-Ankünfte werden gezählt, bevor Fallbacks entfernt werden.
- `visible_scope` bleibt erhalten und wird nicht durch ein kleineres v2-Schema
  ersetzt.
- Der Window Host liefert App-ID und Window-ID; der Context Handler verwendet
  nicht mehr das Hintergrundmodul.
- `ctx.contextActions` erweitert den gepinnten Mount-Contract. Pin, Dokument,
  Marker und Tests werden im selben Commit aktualisiert.
- `ARCHITECTURE.md` wird mit dem Code im selben Schritt aktualisiert.

#### Menü-Interop

- Die Shell besitzt Rendering, Fokus, Positionierung und Keyboard Opening.
- Shift+F10 und ContextMenu-Key öffnen denselben Ablauf.
- Das bereits vorhandene In-Menu-Keyboard-Verhalten wird wiederverwendet.
- Window-Header-Context, App Context und Desktop Context fließen in dieselbe
  Registry.
- App Store, Desktop, Monaco, contenteditable, Documents und Spreadsheets
  erhalten dokumentierte Adapter, bevor lokale Menüs entfernt werden.
- Die 15 lokalen Handler werden rekursiv über alle Modul-JS-Dateien erfasst.
- Bereits preempteter Legacy-Code wird gelöscht, nicht als Neuimplementierung
  geschätzt.

### 5.5 CRUD, Commands und Agentenaktionen

Der Plattformvertrag unterscheidet zwei Pfade.

Für runtime-installierte Apps ist direkter Sync-CRUD der Normalfall. Ein neuer
fachlicher Button darf nicht automatisch bedeuten, dass ein Rust-Handler in
den CTOX-Daemon kompiliert werden muss. Der Typed Command Path teilt sich daher
in generische, zur Laufzeit registrierte App-Actions und bewusst native
Capabilities. Nur letztere dürfen eine plattformspezifische Implementierung
verlangen.

#### Direkter Sync-CRUD-Pfad

Geeignet für:

- manuelles Erstellen und Bearbeiten app-eigener Records
- lokale, offline-fähige Formularänderungen
- einfache Drag-/Drop- und Inline-Edits

Regeln:

- Browser schreibt über die shellgelieferte Collection.
- Native Pull-Hooks prüfen exaktes `data.read`.
- Native Push-Hooks prüfen exaktes `data.write`.
- Collection, Record, Actor und Grant werden auditiert, wenn die Policy dies
  verlangt.
- Ein Client darf gewöhnliche Collections nicht allein deshalb schreiben, weil
  sie nicht auf einer Deny-Liste stehen.

#### Typed Command Path

Pflicht für:

- `context.ask`
- agentische oder automatisierte Datenänderung
- Signature Automation
- serverseitige Jobs und Imports
- Bulk- und Cross-Collection-Mutationen
- App-Source-Änderung
- Release, Install, Rollback und Store-Aktionen
- Freigaben und Delegationen

Standardaktionen:

| Aktion | Semantik | Mindestberechtigung |
| --- | --- | --- |
| `context.ask` | lesende Frage im erfassten Scope | exaktes `data.read` |
| `data.modify` | agentische Änderung von Record oder Feld | exaktes `data.write` |
| `app.modify` | Änderung von App-Quelle oder Konfiguration | `apps.modify` |
| `action.delegate` | Übergabe an Reviewer oder CTOX | Thread-/Delegationsrecht |
| `automation.run` | app-spezifische Signature Action | Command-spezifische Policy |

Globale Window-Events dürfen UI öffnen, sind aber kein Zustellungskanal für
fachliche Aktionen. Module verwenden einen shellgelieferten Dispatcher, der
entweder einen Command persistiert oder einen sichtbaren Fehler zurückgibt. Die
Aktion darf nicht davon abhängen, ob Business Chat gemountet ist.

Runtime-App-Regel:

- `ctx.commandBus` akzeptiert app-spezifische Typen nur, wenn sie in der
  installierten Action Registry stehen oder bewusst als generische CTOX-
  Aufgabe geroutet werden.
- Die Registry stammt aus dem signierten/validierten App-Paket und wird ohne
  Binary-Rebuild aktualisiert.
- Deklarative Daten-Actions verwenden einen generischen nativen Executor mit
  schema-validierten Inputs, exakten Grants, Effect Keys, Audit und optionaler
  Saga-Kompensation.
- Ein App-Command darf kein freies SQL, keinen Dateipfad und keinen beliebigen
  Prozessaufruf aus dem Browser transportieren.
- Native Spezialhandler bleiben für Core-Capabilities kompatibel, sind aber
  kein erforderlicher Erweiterungspunkt für gewöhnliche Apps.

Der bestehende Approval-Flow wird erweitert:

- `threads.ctox_approval.request` bleibt der kanonische Command.
- Die vorhandene Re-Autorisierung bei Approve bleibt bestehen.
- Neu ist die Konvergenz eines nativen Denials in einen delegierbaren Vorgang,
  sofern die Aktion delegierbar ist.
- Nicht delegierbare Denials bleiben harte Fehler mit Audit.

### 5.6 Serverautoritative Policy

#### Vorhanden und zu bewahren

- Permission-Vokabular für Collection- und Record-Scope
- explizite Grants in `business_permission_grants`
- Session-Revocation
- Peer-Revocation
- native Capability-Signaturen
- Data-Access-Review im Release-Flow
- Approve-time Re-Autorisierung

#### Zu ergänzen

- Pull-Hook verwendet exakte `data.read`-Entscheidung.
- Push-Hook verwendet exakte `data.write`-Entscheidung.
- Capability bindet Actor Epoch oder gleichwertigen Revocation State.
- Rollenänderung, Deaktivierung und Grant-Entzug erhöhen die Epoch oder
  widerrufen die Capability.
- Ungültige vorhandene Tokens sind fail-closed.
- Verhalten bei fehlendem Token und nicht erreichbarem Control Plane wird als
  typed Runtime Policy definiert, nicht über einen neuen Env-Schalter.
- Vor dem Enforcement werden bestehende Module, Rollen und Benutzer mit
  erforderlichen Grants migriert oder bewusst gelockt.
- Browser-Helper spiegeln Entscheidungen nur für UX.
- Der spätere Executor wird weiterhin nativ autorisiert.

Die maximale Revocation-Latenz wird in Phase 0 festgelegt und durch einen
Integrationstest belegt.

### 5.7 Release, Version, Install und Distribution

#### Vorhanden und wiederzuverwenden

- versionierte Release-Records
- Release- und Rollback-Status
- Data-Access-Review-Gate
- getrennte `apps.release`- und Rollback-Rechte
- Catalog Update mit Staging, atomarem Swap und Backup-Restore
- Recovery-Snapshot und Version Retention
- Update Detection und UI Badge

#### Konkrete Restarbeit

1. `install_app_module` übernimmt sofort das vorhandene Stage-Swap-Restore-
   Pattern. `remove_dir_all` vor erfolgreichem Copy wird entfernt.
2. Install, Reinstall, Template Install und Catalog Update verwenden dieselbe
   idempotente Staging-Hilfe.
3. Alle Aktivierungswege führen Manifest-, Schema-, Import-, Permission- und
   Source-Validation vor dem Swap aus.
4. Release führt Validator, Smoke und die für den Archetyp erforderlichen E2E-
   Gates aus.
5. Trusted Apps werden signiert und an eine veröffentlichte Revision gebunden.
6. Nicht vertrauenswürdige Third-Party-Apps benötigen eine Sandbox, bevor der
   Store geöffnet wird.
7. Sichtbarkeit stammt aus Release-/Lifecycle-Records, nicht aus SemVer.
8. Die SemVer-Regel wird gleichzeitig in Native, Browser-Mirror und Manifest-
   Lifecycle migriert.
9. Bestehende semver-abgeleitete Sichtbarkeit wird einmalig in explizite
   Release-/Lifecycle-Daten überführt.

Folgende Distributionsachsen bleiben explizit im Modell:

- `install_scope`: core, starter, store, internal, installed
- packaged Catalog
- installed-module Snapshot
- git-ignored `local-modules`-Drop
- Template Instantiation
- Remote/MCP App Creation

Jeder Kanal besitzt eine definierte Validation-, Trust- und Update-Policy.

### 5.8 Design System v2 ohne Klassen-Neustart

Alle 34 Module wurden gerade auf das bestehende Kit migriert. Revision 2 friert
die vorhandenen Namen ein. Es gibt keine zweite flächendeckende Umbenennung.

#### Bestehende Namen bleiben kanonisch

| Funktion | Kanonische bestehende Klasse |
| --- | --- |
| Workbench | `ctox-workspace` |
| Pane | `ctox-pane`, `ctox-pane-header`, `ctox-pane-body` |
| Toolbar | `ctox-toolbar` |
| Buttons | `ctox-button`, `ctox-icon-button` |
| Felder | `ctox-input`, `ctox-select`, `ctox-textarea` |
| Suche/Filter | `ctox-pane-search`, `ctox-pane-filter` |
| Tabelle | `ctox-table`, `ctox-table-wrap`, `ctox-table-sort` |
| Liste | `ctox-list`, `ctox-list-item` |
| Modal | `ctox-modal` |
| Context Menu | `ctox-context-menu` und Shell-Menu-Primitive |
| Status | `ctox-badge` |
| Empty State | `ctox-empty` |

Neue Namen wie `ctox-workbench`, `ctox-data-grid`, `ctox-dialog`,
`ctox-empty-state` oder `ctox-status` werden nicht parallel eingeführt. Falls
ein späterer Rename unvermeidbar ist, erfolgt er über Aliase, Deprecation-
Telemetry und ein eigenes Migrations-Gate.

#### Echte Ergänzungen

- Tool Group
- Tree
- gemeinsamer Drawer-/Inspector-Controller
- Error State
- Progress
- Run Control
- optionaler In-Pane-Data-Skeleton, klar getrennt von der App Loading Shell

#### Token- und Stilrichtung

- kompakt, präzise, flach und funktional
- getönte neutrale Flächen in Light und Dark
- verbundene Panes mit 1-px-Separatoren statt schwebender Karten
- 2 bis 4 px Radien für normale Controls
- Pill-Form nur für Tags, Chips und Status
- Schatten nur für Fenster, Popover, Kontextmenüs und notwendige Overlays
- keine dekorativen Gradients oder Glass-Flächen als Standard
- feste Density-Stufen und Row Heights
- Motion zwischen 150 und 250 ms nur für Zustandswechsel

Die Hauptarbeit beginnt in `app.css` und `shared/base.css`, nicht in 34
Einzelapps. Gleichzeitig werden harte Ausreißer wie `matching` und
`desktop` separat bereinigt.

Eine Retokenisierung aktualisiert gemeinsam:

- `app.css`
- `shared/base.css`
- `SHELL_TOKEN_NAMES` in `assert-module-conformance.mjs`
- Token-Regeln in `module_static_check.mjs`
- `references/design-guide.md`
- `DESIGN.md`

`DESIGN.md` beschreibt die kanonische visuelle Absicht. `design-guide.md`
bleibt der technische Agentenvertrag und verweist auf oder wird aus dem
kanonischen Stand aktualisiert. Es entstehen keine zwei widersprüchlichen
Designquellen.

#### Signature Action und Run Control

Jede App benennt ihre einzigartige Hauptautomation mit einem fachlichen Verb,
beispielsweise `Recherche starten` oder `Abrechnung erzeugen`.

Der Run Control bildet ab:

```text
Bereit -> Freigabe nötig -> Warteschlange -> Läuft -> Fertig
                                      \-> Fehlgeschlagen -> Wiederholen
```

Er zeigt Command-ID, Run-ID, Status, Fortschritt, Abbruch, Retry und
Ergebniszugang. Er ist kein dekorativer Primary Button.

### 5.9 Loading-Shell-Invariante

App-Level-Loading bleibt automatisch aus dem echten statischen Layout
abgeleitet:

- `index.html` bleibt für jede App und jedes Template verpflichtend.
- `index.css` beschreibt die reale initiale Geometrie.
- Die Shell leitet daraus die Loading Shadow ab.
- Windowed Mounts erhalten denselben Mechanismus wie Tab-/Full-Workspace-
  Mounts.
- Templates erzeugen keinen handgebauten Full-App-Skeleton.
- Ein Skeleton-Primitive ist ausschließlich für Datenplatzhalter nach dem Mount
  erlaubt.
- Validator und Archetyp-Fixtures prüfen diese Unterscheidung.

### 5.10 App-Archetypen und Templates

Es gibt fünf kanonische Archetypen:

1. **Record Workbench:** Liste, Suche, Filter, Tabelle, Inspector, Editieren.
2. **Queue/Workflow:** Inbox, Status, Detail, Freigabe, Übergabe.
3. **Editor/Document:** Explorer, Editor oder Canvas, Eigenschaften, Save State.
4. **Automation:** Eingangsdaten, Konfiguration, Run Control, Ergebnisse.
5. **Timeline/Thread:** Quellen, Verlauf, Kontext, Antwort und Delegation.

Planner, Secure Form, Store, Browser, Review und Multi-Pane sind Varianten
dieser fünf Archetypen, keine zusätzlichen ungebauten Templates.

Jeder Archetyp enthält:

- Windowed-, Maximized- und gegebenenfalls Focus-Darstellung
- Container-basierte responsive Zustände
- echte Command-Bus-Beispiele
- direkte CRUD-Beispiele mit Grants
- v1/v2 Context Targets
- erlaubte und verweigerte Rechtepfade
- bestehende Threads-Approval-Delegation
- Empty, Error, Offline und Permission States
- auto-derived App Loading Shell
- Light und Dark
- DE und EN
- Tests und Validator-Fixtures

Der Template Compiler schreibt App-ID, Collection-Namen, Schema-IDs, Imports,
Command Types, Persistenzschlüssel und Tests konsistent um. Mehrere Template-
Instanzen teilen niemals Collections oder lokale Schlüssel.

### 5.11 Creator und Agenten-Skills

Creator, MCP-Create, Prepare-App-Source, Template Store und externer Skill
werden auf einen kanonischen Starter zurückgeführt.

Heute zu beseitigende Divergenzen:

- MCP-sourced `ctox.business_os.app.create` überspringt die Starter-
  Materialisierung.
- `prepare_app_source` materialisiert, validiert aber nicht synchron wie der
  Creator-Pfad.
- Der Starter liegt als harte Rust-Strings vor.
- Template Store kopiert andere Quellen und validiert nicht gleichwertig.

Zielablauf:

1. fachliche Aufgabe und Benutzer bestimmen
2. Datenmodell und Grants bestimmen
3. Archetyp auswählen
4. Signature Action und Command definieren
5. Context Targets definieren
6. Presentation Modes und Mindestgröße bestimmen
7. aus dem kanonischen Starter erzeugen
8. statisch und im Browser validieren
9. als Draft installieren
10. Review und Release durchführen

Der App-Development-Skill wird als MCP Resource oder eigenständig
installierbares, versioniertes Paket ausgeliefert. Ein Host-lokaler Pfad ist
kein gültiger Remote-Contract.

## 6. Programmstruktur und Sequenzierung

Die folgenden Tracks sind nicht vollständig seriell.

```text
Phase 0: Baseline, Landing, Freeze und Entscheidungen
       |                         |
       v                         v
Track A: Policy/Release      Track B: Presentation/Design/QA
       |                         |
       +-----------+-------------+
                   v
        Phase 2: Context/Command Integration
                   v
        Phase 3: Creator, Templates, Store, Skills
                   v
        Phase 4: Piloten und Contract Cleanup
                   v
        Phase 5: App-Migrationswellen
                   v
        Phase 6: Legacy Removal und Release
```

Track A blockiert nicht den Beginn von Presentation oder Design. Track A muss
aber vor dem Exit von Phase 2 und vor jeder App-Migrationswelle grün sein.

### Phase 0: Baseline, Landing und Quick Wins

#### 0A: In-flight Windowed-Arbeit sichern

- Owner für die uncommitted Änderungen bestimmen.
- Diff aus App Manifests, Registry, `app.js`, Window Manager, Desktop Launcher,
  Tests und Docs als Einheit inventarisieren.
- doppelte Root-/Layout-`launch_kind`-Felder entscheiden.
- gegen aktuellen `HEAD` reconcilen.
- Tests ausführen und den Stand separat landen.
- erst danach Welle A als bestehende Kohorte behandeln.

#### 0B: Verifiziertes Gap-Inventar

- für jede Zielinvariante Status `vorhanden`, `teilweise`, `fehlend` erfassen
- Beweis aus Code und Test verlinken
- vorhandene Release-/Rollback-/Revocation-Arbeit nicht duplizieren
- Ist-Dokumentation gegen Code korrigieren, besonders
  `src/apps/business-os/ARCHITECTURE.md`

#### 0C: Früher Store-Quick-Win

- `install_app_module` auf die vorhandene Stage-Swap-Restore-Hilfe umstellen
- Reinstall idempotent machen
- Delete-before-copy als Guard-Test abdecken

#### 0D: Divergenz-Freeze

- die bestehenden 15 Contextmenu-Module rekursiv als schrumpfende Allowlist
  erfassen
- neue lokale Kontextmenüs in CI verbieten
- aktuelles app-spezifisches Standard-Komponenten-CSS als Baseline erfassen
- neue Full-Workspace-only-Apps und neue Primitive-Reimplementierungen sperren
- vorhandene Token-Gates weiterverwenden

#### 0E: Entscheidungen und Sizing

- fünf Hypoport-Modul-IDs benennen
- Accessibility-Ziel festlegen, empfohlen WCAG 2.2 AA
- Revocation-Latenzbudget festlegen
- Mindestfenstergröße bestätigen
- Focus-Chrome festlegen
- Touch als Scope oder Nichtziel entscheiden
- Warm-Mount- und Interaction-Latency-Budgets festlegen
- Auto-Restore offener Fenster entscheiden
- Design-Signoff-Owner benennen
- Third-Party-Trust-Modell entscheiden
- jede App mit S/M/L und Risiko bewerten
- Teamkapazität und maximal parallele High-Risk-Migrationen festlegen

Exit Gate:

- Windowed-Diff ist gelandet oder bewusst verworfen.
- Quick-Win verhindert Delete-before-copy.
- Gap-Inventar und Entscheidungen sind dokumentiert.
- Freeze-Guards sind grün.
- Piloten und Aufwand sind benannt.

### Track A: Policy und Release Hardening

- exakte `data.read`-Entscheidung im Pull-Hook
- exakte `data.write`-Entscheidung im Push-Hook
- Grant-Seeding und Locked-State-Migration für bestehende Apps
- Capability Epoch oder gleichwertige Revocation
- typed Fail-Closed-/Offline-Policy ohne neuen Env-Schalter
- Release-Record als Sichtbarkeitsquelle
- koordinierte Native-/Browser-/Manifest-Migration der SemVer-Regel
- Validator, Smoke und E2E als Release Gates
- Pre-Activation-Validation für alle Install-/Update-Wege
- Signatur für Trusted Apps oder Sandbox-Entscheidung

Track-A-Exit:

- negative Read-/Write-Tests bestehen nativ und im Browser.
- ungültiger oder widerrufener Actor verliert Zugriff innerhalb des Budgets.
- bestehende Benutzer besitzen korrekte Grants oder sichtbare Locked States.
- SemVer allein veröffentlicht keine App.
- alle Aktivierungswege sind stage-validiert und rollbackfähig.

### Track B1: Presentation Runtime

- finalen Presentation Contract festlegen
- Compatibility Reader für alle alten Felder implementieren
- in-flight `launch_kind`-Arbeit integrieren
- Registry und Guard-Duplikate atomar migrieren
- Window Host als CSS Container ausstatten
- echte Pane-/Drawer-Slots für Windowed Mounts liefern
- Focus Mode ergänzen
- bestehende Minimize-/Maximize-/Snap-/Geometriepfade bewahren
- Deep Links und Launch Args vereinheitlichen
- Windowed-attribution im Context Handler korrigieren
- auto-derived Loading Shell in Windowed Mount integrieren
- Full-Workspace-Validator und sieben Skill-Docs in diesem Track ändern, nicht
  erst am Projektende

Track-B1-Exit:

- Referenzapp läuft Windowed, Maximized und Focus ohne Remount.
- Compact, Standard und Wide reagieren auf Container Resize.
- Auswahl, Scrollposition und Entwurf bleiben erhalten.
- Deep Link fokussiert die korrekte App und Record-ID.
- Loading Shadow funktioniert im Windowed-Pfad.
- alle Presentation Guards sind grün.

### Track B2: Design System und QA-Infrastruktur

- `PRODUCT.md` und `DESIGN.md` bestätigen
- Shell Tokens und `base.css` auf die kompakte Richtung kuratieren
- bestehende Klassennamen behalten
- harte Modulausreißer bereinigen
- Run Control und fehlende Primitive ergänzen
- Design Lab aufbauen
- Screenshot-Matrix für drei Breiten, zwei Themes und zwei Locales bauen
- Visual-Diff-Vergleich mit Baseline und Review-Artefakt bauen
- Keyboard-/Accessibility-Harness bauen
- Reduced-Motion-Plattformregel und Gate bauen
- parametrisierten per-App Contract-Test auf bestehendem Playwright-Harness bauen
- Performance-Messung für Warm Mount und Kerninteraktion ergänzen

Track-B2-Exit:

- jedes DoD-Gate besitzt ausführbares Tooling oder ist ausdrücklich manuell.
- manuelle visuelle Gates besitzen Owner und gespeichertes Signoff-Artefakt.
- kein neuer Klassen-Rename ist nötig.
- Light/Dark und Compact/Standard/Wide sind reproduzierbar prüfbar.

### Phase 2: Context, Commands und Delegation

Join Gate: Track A muss für Grant-Semantik und Revocation grün sein. Track B1
muss Windowed Attribution und Presentation Context liefern.

- v1/v2 Context Dual-Read implementieren
- `ctx.contextActions` in Mount-Contract, Doc und Guard aufnehmen
- Attribut-Mapping für bestehende v1-Targets
- Window-, Pane-, Field-, Pointer- und Multi-Selection-Kontext ergänzen
- ContextMenu-Key und Shift+F10 unterstützen
- Adapter für native Editor-/Spreadsheet-/Desktop-/Window-Header-Menüs
- stumme Event-Zustellung durch shellgelieferten Agent Dispatcher ersetzen
- `threads.ctox_approval.request` wiederverwenden
- delegierbare native Denials in Threads überführen
- Permission Decision, Scope und Resultat auditieren
- Legacy-Handler erst nach Coverage entfernen

Exit Gate:

- Rechtsklick auf Windowed Record wird der richtigen App zugeordnet.
- Record, Collection, Feld, Pointer und Auswahl stimmen im Command überein.
- erlaubte Direct-CRUD- und Command-Pfade funktionieren.
- verweigerte delegierbare Aktion erscheint dauerhaft in Threads.
- nicht gemounteter Chat kann keine Aktion mehr stumm verschlucken.
- Refresh erhält Delegation und Audit.

### Phase 3: Creator, Templates, Store und Skills

- kanonischen Starter aus einer Quelle herstellen
- Client-only-App-Vertrag und `data_runtime`-Manifest normalisieren
- Runtime Action Registry plus sicheren generischen Mutation-/Saga-Executor
  ohne per-App-Rust-Handler implementieren
- automatische Hot-Registrierung neuer App-Collections und Actions beim
  Install/Update implementieren
- Creator-, MCP- und Prepare-App-Source-Pfade angleichen
- synchronen Validator überall gleich ausführen
- fünf Archetypen und Varianten implementieren
- Template Compiler mit echtem Namespace-Rewrite
- `index.html`- und Loading-Shell-Invariante in Validator aufnehmen
- Presentation Contract in Creator und Skills lehren
- Full-Workspace-Mandat vollständig entfernen
- Remote Skill Resource ausliefern
- Draft, Review, Release, Update und Rollback in Store UI darstellen
- install_scope- und local-modules-Policy dokumentieren

Exit Gate:

- extern erzeugte App besteht ohne manuelle Reparatur alle Gates.
- eine neue realtimefähige Multiuser-App mit eigener Collection und eigener
  deklarativer Action läuft auf einem bestehenden Binary ohne Rust-Änderung,
  Recompile oder manuellen Daemon-Neustart.
- zwei Template-Instanzen besitzen getrennte Namespaces.
- Starter validiert sich selbst in Creator, MCP und Template Store.
- Remote-Agent kann alle Skill-Ressourcen lesen.
- Windowed App wird vom offiziellen Validator akzeptiert.

### Phase 4: Piloten und Contract Cleanup

Pilotkohorten:

1. die fünf benannten Hypoport-Apps
2. die gelandete Windowed-Kohorte
3. mindestens eine komplexe Drei-Pane-App
4. mindestens eine Editor-/Canvas-App mit Focus Mode
5. mindestens eine Automation mit Run Control

Eine Pilotapp wird genau einmal migriert. Nach der Pilotbereinigung werden alle
Piloten gegen den finalisierten Contract erneut gegated; sie erscheinen in
späteren Wellen nur noch als bereits erledigte Zeilen.

Exit Gate:

- fünf Archetypen oder ihre notwendigen Varianten sind validiert.
- keine Pilotkorrektur erfordert app-spezifische Plattformlogik.
- alle Piloten bestehen Production Gates nach der letzten Contract-Bereinigung.

### Phase 5: Migration aller Apps

#### Welle A: Windowed-/Presentation-Kohorte

- `app-store`
- `appsec-pentest`
- `browser`
- `creator`
- `ctox`
- `knowledge`
- `reports`
- `threads`
- `tickets`

Voraussetzung: Die acht in-flight Änderungen sind in Phase 0 gelandet. Bereits
als Pilot abgeschlossene Apps werden nicht erneut implementiert, nur erneut
gegated.

#### Welle B: kompakte Register und Workflows

- `consent`
- `credentials`
- `esign`
- `intake`
- `interviews`
- `nachweise`
- `placements`
- `submissions`

#### Welle C: operative Kernworkbenches

- `customers`
- `matching`
- `outbound`
- `research`
- `support`
- `conversations`
- `coding-agents`

#### Welle D: Fachsysteme und Planung

- `buchhaltung`
- `calendar`
- `invoices`
- `iot`
- `shiftflow`

#### Welle E: Dokument, Tabelle und Content

- `cv-print-builder`
- `documents`
- `notes`
- `spreadsheets`

#### Welle F: Shell-Ausnahme

- `desktop`

Zusätzlicher Compatibility Track:

- installierte Store-Snapshots
- `local-modules`
- kundenspezifische nicht im Core-Registry-Inventar enthaltene Apps

Sie werden nicht still durch `base.css` v2 restyled. Sie benötigen Alias-
Kompatibilität, Re-Release oder eine explizite Inkompatibilitätsmeldung.

Vor jeder Welle wird S/M/L-Sizing gegen verfügbare Teamkapazität bestätigt.
Maximal zwei High-Risk-Apps werden gleichzeitig migriert. Eine Welle schließt
erst, wenn ihre Apps und Compatibility-Fälle grün sind.

### Phase 6: Legacy Removal und Release

- v1 Context Fallback erst nach Null-Telemetrie entfernen
- tote lokale Kontextmenüs entfernen
- Event-only-Agent-Dispatcher entfernen
- alte Presentation-Felder nur nach vollständigem Compatibility-Nachweis
  entfernen
- deprecated Design-Aliase entfernen
- veraltete Starter und Skills entfernen
- widersprüchliche Dokumentation aktualisieren
- finalen Security-/Privacy-Signoff durchführen
- offenen Store nur nach Signatur- oder Sandbox-Gate freigeben

## 7. App-Migrationsmatrix

`Heute Worktree` beschreibt den bei Planrevision sichtbaren Checkout und ist
nicht gleichbedeutend mit `HEAD`. Alle normalen Apps zielen auf Window und
Maximized; Focus ist fachlich optional.

| App | Heute Worktree | Ziel | Kanonischer Archetyp | Variante / besondere Prüfung |
| --- | --- | --- | --- | --- |
| app-store | windowed, in-flight | window + maximized | Record Workbench | Store, atomare Installation, Release |
| appsec-pentest | windowed, in-flight | window + maximized | Queue/Workflow | Freigaben, lange Evidence |
| browser | windowed, committed | window + maximized + focus | Editor/Document | Browser-Variante, Security Boundary |
| buchhaltung | full-workspace | window + maximized | Record Workbench | Tabellen, DATEV, dichte Formulare |
| calendar | full-workspace | window + maximized | Queue/Workflow | Planner-Variante, Zeitraster, Drag |
| coding-agents | full-workspace | window + maximized | Queue/Workflow | Runs, Terminalstatus, Rechte |
| consent | full-workspace | window + maximized | Record Workbench | kompaktes Register |
| conversations | full-workspace | window + maximized | Timeline/Thread | Kanalfilter, lange Verläufe |
| creator | windowed, in-flight | window + maximized | Queue/Workflow | Starter und App Review |
| credentials | full-workspace | window + maximized | Record Workbench | Secure-Form-Variante, Write-only |
| ctox | windowed, in-flight | window + maximized | Queue/Workflow | Live Runs und Statusdichte |
| customers | full-workspace | window + maximized | Record Workbench | CRM und Auswahlpersistenz |
| cv-print-builder | full-workspace | window + maximized + focus | Editor/Document | PDF Preview und Split View |
| desktop | full-workspace | shell-surface | Shell-Ausnahme | Window Manager und Launcher |
| documents | unspecified | window + maximized + focus | Editor/Document | native Menüs, Loading Shell |
| esign | full-workspace | window + maximized | Queue/Workflow | Status und Freigabe |
| intake | full-workspace | window + maximized | Queue/Workflow | Import und Deduplizierung |
| interviews | full-workspace | window + maximized | Queue/Workflow | Scorecards und Termine |
| invoices | full-workspace | window + maximized | Record Workbench | Dokumente und Finanzstatus |
| iot | full-workspace | window + maximized + focus | Automation | Canvas-Variante, Widgets, Signalbaum |
| knowledge | windowed, in-flight | window + maximized + focus | Editor/Document | Data-Variante, Markdown, Dataframes |
| matching | full-workspace | window + maximized | Record Workbench | Multi-Pane-Variante, Design-Ausreißer |
| nachweise | full-workspace | window + maximized | Record Workbench | Ablaufstatus und Evidence |
| notes | full-workspace | window + maximized + focus | Editor/Document | native Textinteraktion |
| outbound | full-workspace | window + maximized | Automation | Import, Qualifizierung, Runs |
| placements | full-workspace | window + maximized | Queue/Workflow | Garantie- und Honorarstatus |
| reports | windowed, in-flight | window + maximized | Queue/Workflow | Review-Variante, Rollback, Evidence |
| research | full-workspace | window + maximized + focus | Automation | Quellen, Karten, Ergebnis |
| shiftflow | full-workspace | window + maximized | Queue/Workflow | Planner-Variante, Zeitplanung, Drag |
| spreadsheets | unspecified | window + maximized + focus | Editor/Document | Spreadsheet-Context-Interop |
| submissions | full-workspace | window + maximized | Queue/Workflow | Consent und Doppelvorstellung |
| support | full-workspace | window + maximized | Queue/Workflow | Timeline-Variante, SLA, Makros |
| threads | windowed, in-flight | window + maximized | Timeline/Thread | Approval, Denial Path, Delegation |
| tickets | windowed, in-flight | window + maximized | Queue/Workflow | Command-Bus-Referenz |

## 8. Definition of Done pro App

Eine App gilt erst als migriert, wenn alle zutreffenden Punkte erfüllt sind.

### Plattform

- [ ] kanonischer Presentation Contract oder geprüfter Compatibility-Eintrag
- [ ] ein Mount für Window, Maximized und optional Focus
- [ ] keine eigene HTTP-Datenbrücke
- [ ] keine app-eigene Standard-Persistenz außerhalb der vorgesehenen Stores
- [ ] sauberes Mount/Unmount ohne Listener- oder Subscription-Leak
- [ ] `index.html` bleibt repräsentativer statischer App-Aufbau
- [ ] App Loading Shell wird automatisch abgeleitet

### Responsive und Window Manager

- [ ] Compact bei der vereinbarten Mindestgröße nutzbar
- [ ] Standard und Wide ohne unerwarteten horizontalen Seitenscroll
- [ ] Pane-Reduktion nutzt gemeinsame Drawer-/Stack-Mechanik
- [ ] Resize erhält Auswahl und Entwurf
- [ ] Maximize, Restore, Minimize, Snap und Reload getestet
- [ ] Focus verliert keinen App-Zustand
- [ ] Deep Link öffnet oder fokussiert die richtige App und den Record

### Daten, Kontext und Aktionen

- [ ] direkte Reads besitzen native `data.read`-Entscheidung
- [ ] direkte Writes besitzen native `data.write`-Entscheidung
- [ ] fachliche Records als v1/v2 Context Targets registriert
- [ ] relevante Felder und Mehrfachauswahl erfasst
- [ ] Windowed Context trägt korrekte App- und Window-ID
- [ ] Agentenaktionen und Automationen über Typed Command Bus
- [ ] keine fachliche Aktion hängt von gemountetem Business Chat ab
- [ ] fehlende delegierbare Rechte führen über bestehenden Approval-Command zu Threads
- [ ] Audit enthält Scope, Permission Decision und Resultat

### Design

- [ ] bestehende kanonische Kit-Namen verwendet
- [ ] keine lokale Reimplementierung von Standard-UI
- [ ] fachliche Signature Action als Run Control
- [ ] Light und Dark über Screenshot-Matrix geprüft
- [ ] keine nicht genehmigten Glass-, Gradient- oder Card-Stacks
- [ ] Empty, Error, Offline und Permission States vollständig
- [ ] In-Pane-Skeleton klar von App Loading Shell getrennt
- [ ] DE und EN bei Compact, Standard und Wide geprüft

### Accessibility und Performance

- [ ] vollständige Tastaturbedienung
- [ ] sichtbarer Focus
- [ ] semantische Rollen und Labels
- [ ] Context Menu per Tastatur erreichbar
- [ ] festgelegtes Kontrastziel erfüllt
- [ ] Reduced Motion respektiert
- [ ] Warm Mount und Kerninteraktion liegen innerhalb der Phase-0-Budgets

### Browser-E2E

- [ ] App in frischem Profil öffnen
- [ ] echte Daten sichtbar
- [ ] Filter, Sortierung, manuelles Editieren und Persistenz prüfen
- [ ] Signature Action starten und Status verfolgen
- [ ] Rechtsklick auf Windowed Record und Feld prüfen
- [ ] erlaubte Aktion prüfen
- [ ] verweigerte Aktion und Delegation prüfen
- [ ] Reload und Wiederaufnahme prüfen
- [ ] keine Console Errors, Request Failures oder unerwartete 404

## 9. Test- und Release-Gates

### 9.1 Vorhandene Gates, die erweitert werden

- Manifest- und Schema-Validierung
- `assert-module-conformance.mjs`
- `module_static_check.mjs`
- Mount-Context-Contract-Guard
- Global-Context-Policy-Guard
- Module Registry-/Launcher-Smokes
- Business OS JS Suite
- native RxDB- und Policy-Tests
- Browser/Rust Smoke Matrix

### 9.2 Phase-0-Freeze-Gates

- keine neuen lokalen `contextmenu`-Handler außerhalb der schrumpfenden Allowlist
- rekursiver Scan aller Modul-JS-Dateien, nicht nur `index.js`
- keine neuen Full-Workspace-only-Apps
- keine neue lokale Standard-Komponentenfamilie
- keine neue Token-Neudefinition

### 9.3 Neue Infrastruktur vor den Piloten

1. **Screenshot Matrix Runner**
   Drei Containergrößen, Light/Dark, DE/EN, deterministische Fixtures.
2. **Visual Diff Runner**
   Vergleich gegen Baseline, tolerierte Abweichung, gespeichertes Artefakt.
3. **Accessibility Contract Runner**
   Keyboard, Focus, Rollen, Namen, Kontrast und Reduced Motion.
4. **Parametrisierter App Contract Runner**
   Mount, Resize, Deep Link, Context v2, CRUD Grant, Command, Denial,
   Delegation, Reload und Console-/Network-Health.
5. **Performance Probe**
   Warm Mount, Loading-Shadow-Sichtbarkeit und Kerninteraktionslatenz.

Wenn ein Gate nicht automatisierbar ist, benennt der Plan einen Owner und das
zu speichernde Signoff-Artefakt. `visuell abgenommen` ohne Owner und Evidence
ist kein Gate.

### 9.4 Release-Gate

Ein Release benötigt:

- statischen Validator
- Archetyp-Smoke
- erforderliche E2E-Story
- Policy- und Grant-Negativtests
- Screenshot-/Visual-Diff-Artefakt
- Accessibility-Ergebnis
- Design-Signoff bei materieller UI-Änderung
- Source-/Manifest-Signatur für Trusted Store Apps
- funktionierendes Rollback-Ziel

### 9.5 Installiertes Production Gate

Die Source- und Workspace-Gates reichen nicht für einen Rollout. Vor einer
Produktionsfreigabe muss dieselbe gebaute und installierte Release-Version
folgende Stories in frischen Browserprofilen bestehen:

1. **Bootstrap:** Shell meldet `status.ok=true`, alle erforderlichen Peers sind
   aktiv und der native Peer meldet `replicationUp=true`.
2. **34-App-Langlauf:** jede Core-App öffnet innerhalb des festgelegten Budgets;
   danach funktionieren die zuerst geöffneten Apps weiterhin ohne Remount-
   oder Query-Owner-Schaden.
3. **Transport Health:** keine `masterChangesSince`-/`masterWrite`-Timeouts,
   keine unerwartet geschlossenen Broadcast Channels und keine anhaltenden
   Multi-Tab-Owner-Timeouts.
4. **Zwei Profile:** zwei parallele Clean Profiles können dieselben erlaubten
   Collections lesen und schreiben, ohne dass eines den Query-/Replication-
   Owner des anderen beschädigt.
5. **Command End-to-End:** UI-Aktion erzeugt einen typisierten Payload, der
   Command wird in `business_commands` persistiert, nativ konsumiert und sein
   finaler Status samt Audit ist über Browser und MCP auffindbar.
6. **Context Matrix:** Rechtsklick, ContextMenu-Key und Shift+F10 liefern für
   Record und Feld identischen App-/Window-/Pane-/Collection-/Entity-/Field-
   Kontext.
7. **Policy Matrix:** erlaubte Admin-Aktion, verweigerte Nicht-Admin-Aktion und
   delegierbarer Denial→Threads-Approval werden mit getrennten Benutzern
   getestet; Approve re-autorisiert nativ.
8. **Persistenz:** manuelles Editieren, Reload, Browserneustart und zweites
   Profil zeigen denselben erlaubten Datenstand.
9. **Auth:** Login, Logout, Session-Revocation und Clean-Profile-Zugriff werden
   gegen die reale Zielkonfiguration getestet. Für reine Local-Auto-Admin-
   Builds wird dieses Gate ausdrücklich als `not-applicable`, nicht als grün,
   protokolliert.
10. **Health Budget:** keine ungeklärten Console Errors, Page Errors, Request
    Failures oder unerwarteten 404; bekannte harmlose Meldungen benötigen eine
    enge, begründete Allowlist.
11. **Window/Dock Composition:** persistente Docks, Chat und App-Windows dürfen
    keine sichtbaren Controls anderer fokussierter Surfaces unbedienbar machen;
    Pointer-, Keyboard- und Taskbar-Focus werden mit überlappenden Surfaces
    getestet.

Gate 11 ist seit Revision 9 technisch grün: automatisierter Fünf-Phasen-E2E
und installierter V16-Lauf belegen Side-/Bottom-/Compact-Komposition, echten
Pointer auf der Threads-Freigabeaktion, Enter-Aktivierung der Taskbar sowie
null pointeraktive Overlap-Fläche bei 1280×720 und 900×600.

Der Runner speichert pro Release JSON, Markdown, Screenshots und Console-/
Network-Logs. Ein Timeout wird weiter untersucht und nicht durch eine größere
pauschale Wartezeit grün gerechnet.

## 10. Migrationsstrategie

- Bestehende Daten und Command Types bleiben zunächst kompatibel.
- Presentation und Context v2 werden additiv eingeführt.
- Legacy-Verwendung wird telemetriert.
- Apps werden wellenweise migriert.
- Gemeinsame Probleme werden in SDK, Policy oder Kit behoben, nicht kopiert.
- Businesslogik wird nur geändert, wenn ein Test oder Vertrag einen Fehler zeigt.
- Jede Migration besitzt eine Rückfallversion.
- Pilotapps werden nicht doppelt implementiert.
- Installierte und lokale Apps erhalten einen eigenen Compatibility Track.
- Keine Massenkonvertierung darf 34 Apps syntaktisch ändern und statisch als
  fertig markieren.
- Änderungen an `layout.shell` oder Presentation können aktive Module neu
  aktivieren; Rollout und User-Kommunikation berücksichtigen dies.

## 11. Risiken und Gegenmaßnahmen

| Risiko | Gegenmaßnahme |
| --- | --- |
| In-flight Windowed-Arbeit geht verloren | Phase-0-Landing vor jeder abhängigen Welle |
| UI-Refactor beschädigt Fachlogik | Daten- und Command-Tests vor UI-Migration einfrieren |
| Direct CRUD umgeht Grants | native Pull-/Push-Entscheidung pro Collection/Record |
| Chat-Event verschluckt Aktion | shellgelieferter persistierender Dispatcher |
| Windowed Context zeigt falsche App | Window-/Host-Attribution statt `state.activeModule` |
| Design System erzwingt zweite Markup-Migration | bestehende Kit-Namen einfrieren |
| Flat Design ändert alle Apps unkontrolliert | Shell-Token-Change plus Visual-Matrix und Ausreißerinventar |
| Loading Shell wird pro App neu gebaut | `index.html`-Derivation und Validator-Gate |
| Responsive Regeln werden app-spezifisch | Container-Layout als SDK und Contract-Test |
| Delegation umgeht Rechte | bestehende Approve-time Re-Autorisierung beibehalten |
| Grant-Enforcement sperrt Bestandsnutzer aus | Grant-Seeding und sichtbare Locked-State-Migration |
| Capability-Fail-Closed bricht Offline-Modus | typed Offline-Policy und Rollout-Test |
| Store-Reinstall verliert aktive App | früher Stage-Swap-Restore-Quick-Win |
| SemVer-Migration versteckt Apps | koordinierte Native-/Browser-/Manifest-Datenmigration |
| Third-Party-App gefährdet Same Origin | Signatur oder Sandbox vor offenem Store |
| DoD bleibt Papierliste | Tooling vor Piloten, Owner für manuelle Gates |
| Piloten werden doppelt migriert | migrate-once und Re-Gate-Regel |
| Ein Browser ist grün, weitere Profile verhungern | paralleler Zwei-Profil-Test mit getrennten Clean Profiles |
| Langlauf zerstört Query-/Replication-Owner | 34-App-Sequenz plus erneuter Zugriff auf frühe Apps als Release-Gate |
| UI zeigt einen Command, Native Store kennt ihn nicht | Command-E2E mit Browser-ID, nativer Store-Suche, Consumerstatus und Audit |
| Transportfehler erscheinen als App-Mountfehler | gemeinsame Transport-Telemetrie und Fehlerklassifikation vor App-Triage |
| Persistenter Chat verdeckt fokussierte App-Controls | Dock reserviert Workspace-Fläche oder nimmt am Window-Manager-Occlusion-/Z-Order-Modell teil; Overlap-E2E |

## 12. Erfolgskriterien

Der Refactor ist abgeschlossen, wenn:

1. jede normale Core-App responsive in einem Fenster nutzbar ist,
2. Maximized, Snap und optional Focus denselben Zustand weiterverwenden,
3. normales CRUD native Read-/Write-Grants erzwingt,
4. Agentenaktionen und Automationen als Commands persistieren,
5. jeder relevante Rechtsklick exakten App-, Window-, Record- und Feldkontext
   erzeugt,
6. fehlende delegierbare Rechte dauerhaft in Threads landen,
7. Light und Dark dieselbe kompakte, zeitlose Designsprache verwenden,
8. bestehende Kit-Namen stabil bleiben,
9. Loading Shells automatisch aus dem echten App-Aufbau entstehen,
10. Creator, Templates, Store und Agent-Skills denselben Starter verwenden,
11. Release und Aktivierung validiert, atomar, signiert oder sandboxed und
    rollbackfähig sind,
12. jede App das gemeinsame Browser-, Policy-, Visual- und Accessibility-Gate
    besteht,
13. installierte und lokale Apps eine dokumentierte Compatibility-Policy haben,
14. persistente Docks und Fenster sich nicht gegenseitig sichtbar, aber
    unbedienbar überlagern.
15. jede Fenster-App mit echtem Single-Click, Maus-Resize und sichtbaren
    Window-Controls bedienbar ist,
16. Zwei-/Drei-Pane-Apps in Window und Mobile fuer jedes reduzierte Pane einen
    sichtbaren Bedien- und Rueckweg behalten,
17. der Chat-Dock bei wachsender Chatanzahl scrollbar im Viewport bleibt und
    ein eingeklappter Dock nicht durch Hintergrundantworten geoeffnet wird,
18. jeder Window-Header Version, Lifecycle-/Sichtbarkeitsstatus, Source-Aktion
    und Versionskontrolle entsprechend den Rechten zeigt,
19. oben links nur das Startmenue verbleibt und App-Icons im Web mit einem
    Klick starten,
20. die Shell bis zur festgelegten Mobile-Untergrenze ohne verdeckte
    Navigation, Headeraktionen, Chat- oder Taskbar-Flaechen funktioniert.
21. alle Apps den Ableton-Dichtevertrag erfuellen: nur Signature-Automationen
    sind raeumlich/visuell dominant; wiederkehrende Controls bleiben in
    Window, Mobile und Touch kompakt, vollstaendig und erreichbar.
22. Desktop-App-Namen mit kurzen, langen und untrennbaren Labels bleiben in
    Desktop und Mobile innerhalb ihrer festen zweizeiligen Rasterzelle.
23. Impeccable-Preflight, lokaler App-Development-Skill und externer
    CTOX-Deploy-Skill schreiben und validieren denselben aktuellen
    App-Presentation-/Design-Vertrag.

Erfolgskriterium 14 ist mit Revision 9 erfüllt. Der zusätzliche skalierte
Context-/Threads-Nachweis ist mit Revision 10 dauerhaft in der Release-Matrix.
Revision 13 belegt Kriterien 7 und 10 sowie Teile der statischen Breite von
Kriterium 12. Revision 14 widerlegt jedoch die vollstaendige visuelle und
interaktive Freigabe: Kriterien 15 bis 20 sind neu verbindlich und noch nicht
vollstaendig gruen. Die Gesamtfreigabe bleibt technisch und wegen der
ausdruecklich menschlichen Signoffs offen.

## 13. Getroffene Entscheidungen und verbleibende Produktmetadaten

Die Phase-0-Entscheidungen sind in `PRODUCT.md`, `DESIGN.md`, der technischen
Baseline und den Manifesten festgeschrieben:

1. Die globale Mindestgröße ist 640 × 480; Apps dürfen größere Startgrößen,
   aber keine höhere Mindestgröße verlangen.
2. Focus behält eine kompakte 28-px-Chrome mit Rückweg in den Window-Modus.
3. Touch unterstützt die grundlegende Aktivierung, ist aber kein
   Optimierungsziel für dichte Workflows.
4. Auto-Restore und Multi-Instance sind für alle migrierten Apps zunächst
   deaktiviert (`auto_restore: false`, `multi_instance: false`).
5. Rollen- oder Grant-Änderungen wirken bei der nächsten nativen Entscheidung
   und spätestens innerhalb von fünf Sekunden; fehlende oder nicht prüfbare
   Tokens sind fail-closed und erzeugen einen typisierten Fehler.
6. Apps erhalten keine beliebige eigene Theme-Farbe. Der gemeinsame Akzent
   bleibt operativ; eine App darf damit genau ihre fachliche Signature Action
   hervorheben.
7. WCAG 2.2 AA, vollständige Tastaturbedienung und Reduced Motion sind
   verbindliche Ziele.
8. Product Owner ist der menschliche Design-Signoff-Owner; die 12-fache
   Screenshot-Matrix liegt unter
   `output/playwright/business-os-design-matrix/`.
9. Warm Mount p95 ≤ 500 ms und sichtbare Interaktionsantwort p95 ≤ 100 ms sind
   die Zielbudgets. Längere Arbeit wird als persistierter Command/Run gezeigt.
10. Trusted Apps benötigen revisionsgebundene Vertrauensevidence. Untrusted
    Third-Party-Apps bleiben bis zu einer echten Sandbox deaktiviert.
11. Keine der 34 vorhandenen Apps wurde archiviert. Alle besitzen den
    Presentation Contract; die vollständige app-spezifische Definition of Done
    ist laut Revision-11-Audit noch nicht für alle Apps gegated.
12. Hochriskante Migrationen werden mit höchstens zwei Apps parallel
    durchgeführt.

Die Chat-Dock-/Window-Komposition ist mit Revision 9 technisch geschlossen;
das skalierte Threads-/Context-Gate ist mit Revision 10 technisch geschlossen.
Revision 11 öffnete keine dieser belegten Fähigkeiten erneut, korrigierte aber
die Breite des Gesamtclaims. Revision 12 schließt die fünf Archetypen und den
vereinheitlichten Starterpfad. Revision 13 schliesst die aktuelle 34-App-
Fachvertragsmatrix und die app-vollstaendige sichtbare Installed-Matrix.
Offen bleiben die Ein-Prozess-Langlaufstabilitaet, dynamische Fachstory-Tiefe,
die menschlichen Signoffs und die Benennung der fünf historischen
Hypoport-Pilot-IDs.

## 14. Nächster Schritt: technische Resttracks, danach Signoff

Binary-Rollout, realer Context-Command-Pfad, paralleler Zwei-Profil-Test,
Persistenz, Nicht-Admin-Approval und isolierter Auth-Smoke bleiben belegt.
Revision 14 oeffnet Window/Dock-/Pane-/Mobile-Composition jedoch erneut. Die
Reihenfolge der verbleibenden Schritte ist:

1. Chat-Dock-Hydration, Mehrfachchat-Side-Dock und Taskbar-Tastaturpfad
   korrigieren und mit realen Toggle-/Scroll-/Skalierungsinteraktionen pruefen.
2. Shell-Navigation vereinfachen, Desktop-App-Launcher auf Single Click
   umstellen und die Window-Header um Version, Status, Source und
   Versionskontrolle erweitern.
3. Den 2-/3-Pane-Vertrag jeder frueheren Fullscreen-App bei 640/960/1180 px
   sowie Mobile pruefen; fehlende Drawer-/Stack-Rueckwege implementieren.
4. Die Shell-Mobile-Untergrenze festschreiben und Startmenue, Window-Sheet,
   Header, Chat und Taskbar auf Touch und Viewport-Fit abnehmen.
5. Danach den neuen interaktiven 35-App-Lauf und den 34+2-Installed-Lifecycle
   in einem installergebundenen Browserprozess vollstaendig bestehen lassen.
   Der externe Deploy-Skill besteht parallel seine eigene Test- und
   Validator-Suite gegen denselben Presentation-/Preflight-Vertrag.
6. Die statischen 34-App-Fachvertraege risikobasiert durch dynamische
   Datenmutation, erlaubte/verweigerte Aktion, Persistenz, Reload und Resume
   vertiefen und die Evidence je App verlinken.
7. Den macOS-`_dyld_start`-Befund neuer `ctox-real`-Prozesse beheben und danach
   Status, Native Peer und Service-Restart auf genau dem installierten Release
   erneut nachweisen.
8. Product-, Design-, Security- und Privacy-Signoff auf den tatsächlich
   neu gebauten und installierten Release-Artefakten durchführen und die vorgesehenen
   Signoff-Dokumente von `pending-signoff` auf eine menschlich bestätigte
   Entscheidung setzen. Vor dem Signoff müssen Release-Commit,
   `evidence_revision` und alle `source_hashes` neu eingefroren werden; der
   strikte aktuelle Check weist erwartungsgemäß sieben offene Controls,
   `reviewer/date/status` und veraltete Worktree-Hashes aus.
5. Local-Auto-Admin bleibt für die lokale Zielkonfiguration bei Auth
   `not-applicable`; der isolierte reale Login/Logout-Smoke bleibt Pflicht für
   den Auth-fähigen Build.
6. Die fehlende Hypoport-ID-Zuordnung bleibt separate Produktmetadatenpflege
   und blockiert die technische Stabilisierung nicht.
