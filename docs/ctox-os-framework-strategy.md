# CTOX OS Framework Strategy

Stand: 2026-07-05
Status: Strategiepapier

## Executive Summary

Das strategische Objekt ist nicht nur eine Datenbank und auch nicht nur ein
App-Framework. CTOX Business OS entwickelt sich zu einem lokalen CTOX OS: einem
ganzheitlichen Framework, mit dem Apps für User erstellt, installiert,
betrieben, aktualisiert und zurückgerollt werden können.

`ctox-rxdb` bleibt dabei load-bearing: Es ist die lokale Daten-, Sync- und
Runtime-Engine, ohne die das Framework nicht funktioniert. Es ist aber nicht
das gesamte Framework. Das Gesamtframework umfasst zusätzlich Shell,
App-Modell, Modul-Manifeste, Schema-Verträge, Command-Bus, File Plane,
Permissions, Release-/Rollback-Metadaten, Runtime-Projektionen,
Deployment-Gates und daemonseitige Governance.

Die strategische Abhängigkeit ist eindeutig: Die CTOX-OS-Ausrichtung fliegt
oder fällt mit `ctox-rxdb`. Wenn die Engine stabil, schnell, beobachtbar und
erweiterbar ist, kann CTOX OS viele kleine und große Apps lokal entwickeln,
deployen und miteinander vernetzen. Wenn die Engine instabil ist, wird das
OS-Modell an jeder höheren Schicht fragil.

Die strategische Richtung ist deshalb: CTOX Business OS als lokales CTOX OS
entwickeln, in dem App- und Deployment-Framework, Runtime, Datenebene, Policy
und User-Zugriff zusammenwirken. `ctox-rxdb` ist darin die zentrale
Runtime-Engine. Apps sollen nicht als lose Web-UIs entstehen, sondern als
Business-OS-Module mit klaren Verträgen für Daten, Dateien, Commands,
Permissions, Runtime-Zustand und Deployment.

Der wichtigste Architekturentscheid ist zweigeteilt:

1. CTOX OS wird als eigenes Produkt- und Architekturmodell geführt.
2. Die kritischen `ctox-rxdb`-Semantiken konvergieren schrittweise in einen
   gemeinsamen, hostunabhängigen Rust-Core, der nativ im Daemon und
   perspektivisch als WebAssembly im Browser genutzt werden kann.
3. `ctox-rxdb` wird nicht als abgeschlossene Datenbank betrachtet, sondern als
   OS-Runtime-Engine für App-Daten, Sync, Dateien, Commands, Reconnect,
   Deployment-Zustand und Runtime-Projektionen weiterentwickelt.

## Strategische These

CTOX Business OS ist die Ausprägung eines lokalen CTOX OS für
browserbedienbare Business-Apps. Das Ziel ist nicht nur, Daten zu
synchronisieren. Das Ziel ist, einen vollständigen App-Lifecycle für User
abzubilden:

- Apps erstellen oder anpassen.
- App-Module installieren.
- App-Schemas und Datenzugriff registrieren.
- App-Versionen ausrollen.
- Rechte und Datenzugriff prüfen.
- Commands und serverwirksame Mutationen ausführen.
- Dateien und Artefakte demand-basiert bereitstellen.
- Runtime-Zustand beobachten.
- Releases zurückrollen.
- Apps für User sichtbar und nutzbar machen.

Dieses CTOX OS muss dabei lokal-first, reaktiv, WebRTC-repliziert und
daemonautoritativ bleiben. Der Browser ist die Arbeitsoberfläche. Der CTOX
Daemon ist die Autorität für Persistenz, Policy, Projektionen, Command-
Ausführung und Audit. `ctox-rxdb` ist die Runtime-Engine, die diese Ebenen
zuverlässig koppelt.

Damit ist `ctox-rxdb` der technische Hebel für die Produktgeschwindigkeit:
Je besser die Engine App-Schemas, Collections, Commands, Files, Permissions,
Runtime-State und Deployment-Metadaten als gemeinsame OS-Primitiven trägt,
desto schneller können neue Apps entstehen und miteinander arbeiten. Die
Engine muss deshalb nicht nur Daten replizieren, sondern die wiederkehrenden
OS-Primitiven so robust bereitstellen, dass Apps darauf aufbauen können, ohne
eigene Infrastrukturwege zu erfinden.

## Aktuelle Architektur

Die aktuelle Implementierung zeigt bereits die Konturen eines lokalen CTOX OS.
Sie besteht nicht nur aus einer Datenebene, sondern aus mehreren
zusammenwirkenden Schichten.

### 1. Browser Shell und App Host

Die Business-OS-Shell öffnet CTOX DB über
`src/apps/business-os/shared/db.js`. Runtime-Apps erhalten Datenbank- und
Collection-Handles von der Shell. Sie importieren nicht upstream `rxdb` und
erzeugen keinen eigenen Sync-Pfad.

Die Shell ist damit mehr als ein UI-Container. Sie ist der App Host:

- Sie stellt App-Modulen Datenbank-Handles bereit.
- Sie kontrolliert, welche Collections sichtbar sind.
- Sie bindet Runtime-Apps an Business-OS-Konventionen.
- Sie hält Apps transportblind.
- Sie trennt App-Code von Sync-, Storage- und Policy-Mechanik.

### 2. App- und Modulmodell

Business-OS-Apps sind Module mit Manifesten, Collections, Runtime-Metadaten und
Permissions. Runtime-installierte Module bringen Schema-Verträge mit und werden
in die native Replikation aufgenommen. Modul-Katalog, Release-Metadaten,
Source Snapshots, Installationsstatus und Rollback-Ziele sind selbst Teil des
Business-OS-State.

Das ist der Framework-Kern: Eine App ist nicht nur ein Bündel aus HTML, JS und
CSS. Eine App ist ein Business-OS-Modul mit Datenmodell, File Plane, Commands,
Permissions, Release-Zustand und Runtime-Verhalten.

### 3. `ctox-rxdb` Runtime Engine

Die Browser-Engine liegt unter `src/apps/business-os/rxdb/src/` und stellt eine
CTOX-spezifische RxDB-ähnliche Oberfläche bereit:

- Collections, Queries, Counts, Inserts, Upserts und Subscriptions.
- IndexedDB-Storage mit Checkpoint-, LWT- und Replication-Origin-Semantik.
- Active-Collection-Tracking auf Basis echter Reads und Subscriptions.
- WebRTC-Replikation über einen gemeinsamen multiplexed Room.
- Query-Demand-Loading mit Sidecar-Metadaten und Window-Completeness.
- File-Demand-Loading mit Chunks, Ranges, Cancel und Deduplizierung.
- Browser-Diagnostics für Peer-, Protocol-, Checkpoint- und Transportzustand.

Die native Seite lebt in `src/core/rxdb/` und
`src/core/business_os/rxdb_peer.rs`. Der Native Peer öffnet die SQLite-basierte
CTOX-DB, registriert Business-OS-Collections, startet die WebRTC-Replikation
und betreibt Hintergrundloops für Projektionen, Commands, Runtime-Status,
Desktop-Dateien, Modul-Kataloge und weitere Business-OS-Flächen.

Der Native Peer ist supervised:

- Er besitzt einen Prozess-Lock.
- Er schreibt Heartbeats inklusive `replicationUp`.
- Er leitet Signaling-URLs und Tokens pro Reconnect frisch ab.
- Bring-up-Fehler sind fatal für den aktuellen Run und führen zum Respawn.
- Schema- oder Config-Änderungen werden als Neustartgrund behandelt.

Die Rust-WebRTC-Replikation unter
`src/core/rxdb/src/plugins/replication_webrtc/` betreibt den gemeinsamen
multiplexed Sync-Raum. Sie demultiplexed Frames nach Collection, verhandelt
Protocol, Peer-Rollen, Capabilities, Schema-Hashes und Checkpoints und
erzwingt Collection-, Write- und Document-Authorisierung.

### 4. Deployment- und Release-Modell

Deployment bedeutet im Business OS nicht nur, statische Dateien auszuliefern.
Deployment bedeutet, eine App als kontrolliertes Modul in den lokalen
Business-OS-Runtime-Vertrag einzubinden:

- Manifest und Modulidentität.
- Collections und Schema-Kompatibilität.
- Installationsstatus.
- Release- und Rollback-Metadaten.
- Source Snapshots.
- Permission- und Data-Access-Entscheidungen.
- File-Plane-Konventionen.
- Command-Patterns für serverwirksame Mutationen.
- Runtime-Status und Projektionen.

Dieses Deployment-Modell ist usernah: Eine App soll für User installierbar,
nutzbar, aktualisierbar und im Fehlerfall zurückrollbar sein, ohne dass jede
App ihren eigenen Daten-, Rechte- oder Deployment-Pfad erfindet.

### 5. Datenpfad-Grenze

Business-OS-Daten laufen nicht über HTTP. HTTP darf statische Shell-Assets,
Bootstrap-Konfiguration, Status, Auth und explizite Control-Plane-Endpunkte
liefern. Collections, `business_commands`, Queue Tasks, Desktop-Dateien,
Chunks, Module Manifests und Runtime-Status gehören in den CTOX-DB/WebRTC-Pfad.

Diese Grenze ist produktstrategisch wichtig. Ein HTTP-Datenfallback würde
Fehler kaschieren, Policy-Grenzen aufweichen und pro Feature neue
Sondertransporte erzeugen.

## CTOX-OS-Modell

Das CTOX OS kann als strukturierte Virtualisierung einer lokalen
Arbeitsmaschine verstanden werden. Es virtualisiert nicht primär Pixel und
Mausbewegungen, sondern App-Zustände, Datenmodelle, Dateien, Commands, Rechte,
Runtime-Status und Deployment-Zustand.

Die virtuelle Maschinenoberfläche besteht aus:

- Datei-Metadaten, Chunks, Ranges und On-Demand-Streams.
- Durable Commands als policy-geprüfte Intent-Dokumente.
- Queue Tasks, Agentenläufen, Tickets, Approvals und Runtime-State.
- Modul-Katalogen, Source Snapshots, Release-Metadaten und Runtime-Schemas.
- Knowledge Tables, Users, Channels, Branding und Workspace-Projektionen.
- Browser-Runtime-State, der über Collections beobachtbar und steuerbar wird.

Der Browser wird dadurch zu einer lokalen, reaktiven Arbeitsoberfläche. Der
Daemon bleibt die autoritative Ausführungs- und Persistenzinstanz.
`ctox-rxdb` ist die Runtime-Engine, die diese Struktur zuverlässig
synchronisiert.

## Rolle von `ctox-rxdb`

`ctox-rxdb` ist die Engine, an der die CTOX-OS-Strategie hängt. Die Engine muss
die gemeinsamen OS-Primitiven tragen, damit App-Entwicklung und Deployment
nicht pro App neu erfunden werden.

Die Kernaufgaben der Engine sind:

- lokale reaktive App-Daten im Browser bereitstellen;
- daemonautoritatives State-Mirroring aus SQLite und CTOX Core ermöglichen;
- WebRTC-only Replikation ohne HTTP-Datenfallback sichern;
- Collections, Schemas und Runtime-installed Module konsistent integrieren;
- Commands als durable, policy-geprüfte Intents transportieren;
- Dateien, Chunks, Ranges und Artefakte demand-basiert verfügbar machen;
- Runtime-Status, Release-Zustand und Rollback-Ziele replizieren;
- Foreground- und Active-Collection-Priorisierung steuern;
- Reconnects, Checkpoints, Peer-Sessions und Native-Peer-Restarts korrekt
  behandeln;
- Idle-CPU, Query-Fallbacks, Writer-Locks und große Datenflüsse messbar
  kontrollieren.

Für die Produktstrategie bedeutet das: Jede Verbesserung an `ctox-rxdb`, die
Schemas, Commands, Dateien, Runtime-State, Deployment-Metadaten, Reconnects
oder Performance verlässlicher macht, erhöht direkt die Geschwindigkeit, mit
der CTOX OS neue Apps tragen kann. Jede Schwäche in diesen Bereichen wirkt
umgekehrt als Plattformgrenze.

## Engine-Entwicklungsrichtung

`ctox-rxdb` muss in Richtung einer OS-Runtime-Engine weiterentwickelt werden.
Die folgenden Entwicklungsachsen sind dafür entscheidend.

### 1. Semantische Einheit

JavaScript- und Rust-Seite dürfen keine dauerhaft getrennten Wahrheiten über
Protocol, Schema Hashing, Query Fingerprints, Checkpoints, Frame Chunking,
LWT/LWW-Regeln oder Error Taxonomy behalten. Drift in diesen Regeln blockiert
das OS-Modell.

Ziel ist ein gemeinsamer Rust-Core für hostunabhängige Semantik, nativ im
Daemon und als WASM im Browser. Die Browser- und Daemon-Adapter bleiben für
IndexedDB, WebRTC, SQLite, Supervision und Policy verantwortlich.

### 2. App- und Deployment-Primitiven

Die Engine muss Deployment-Zustand genauso ernst nehmen wie klassische
Dokumentdaten. App-Manifest, Schema-Versionen, Installationsstatus,
Release-Metadaten, Rollback-Ziele, Source Snapshots, File Plane, Permissions
und Runtime-Status gehören in den stabilen OS-Vertrag.

Neue App-Typen sollen nicht zuerst neue Infrastruktur brauchen. Sie sollen auf
vorhandene Engine-Primitiven zurückgreifen.

### 3. Demand-first Datenflüsse

Das OS muss viele kleine und große Apps tragen können. Große Dateien,
Artefakte, Tabellen, Chunks und High-churn Collections dürfen deshalb nicht
eager oder unkontrolliert repliziert werden.

Die Engine braucht konsequentes Demand Loading, Active-Collection-Priorität,
Backpressure, Frame-Priorisierung, Eviction und klare Chunk-/Range-Semantik.

### 4. Lokale Robustheit ohne Cloud-Deployment-Zwang

User sollen Apps nutzen können, ohne dass jede App bei einem Cloud-Anbieter
deployed wird. Diese Stärke hängt an der lokalen CTOX-Instanz und an der
Browser/Daemon-Kopplung über `ctox-rxdb`.

Die Engine muss NAT-/Reconnect-/Restart-Szenarien, Peer-Session-Wechsel,
Signaling-Erneuerung, Token-Erneuerung und Native-Peer-Supervision als normale
Betriebszustände behandeln.

### 5. SQLite-Idle- und Performance-Disziplin

Frühere Idle-CPU-Probleme zeigen, dass Performance Teil der Produktstrategie
ist. CTOX OS kann nur dann dauerhaft lokal laufen, wenn die native Engine im
Idle ruhig bleibt und unter Last kontrolliert arbeitet.

SQLite darf kein permanenter Taktgeber werden. Polling muss budgetiert,
backoff-fähig und beobachtbar sein. Query-Fallbacks, Writer-Locks,
External-Poll-Wakeups und große Chunk-Flüsse müssen release-relevante Signale
bleiben.

### 6. Observability als Engine-Funktion

Die Engine muss ihren Zustand erklären können: Peer, Protocol, Collection,
Checkpoint, Demand Cache, File Streams, Native Peer, SQLite, Backpressure,
Errors, Reconnects und `replicationUp`. Ohne diese Sichtbarkeit ist ein lokales
OS schwer zu betreiben und schwer weiterzuentwickeln.

## Strategische Risiken

### 1. CTOX OS wird fälschlich als Datenbank behandelt

Das größte strategische Risiko ist, das System zu klein zu rahmen. Wenn
`ctox-rxdb` isoliert als Datenbank betrachtet wird, fehlen die entscheidenden
OS- und Framework-Fragen:

- Wie entsteht eine App?
- Wie wird sie für User installiert?
- Wie werden Schemas versioniert?
- Wie werden Rechte und Datenzugriff geprüft?
- Wie werden Commands standardisiert?
- Wie werden Dateien und Artefakte eingebunden?
- Wie werden Releases ausgerollt und zurückgerollt?
- Wie wird Runtime-Zustand beobachtbar?

CTOX OS muss diese Fragen als zusammenhängenden Vertrag beantworten. Die
Datenengine ist der Kern, aber nicht die vollständige Plattform.

Das Gegenstück ist ebenso wichtig: Das Framework darf die Engine nicht als
beliebige Infrastruktur behandeln. Die CTOX-OS-Strategie hängt an dieser
Engine. Framework-Features, die `ctox-rxdb` umgehen, schwächen die Plattform.

### 2. Semantischer Drift zwischen JavaScript und Rust

Heute existieren zentrale Regeln auf beiden Seiten der Datenebene. Das ist
kurzfristig praktikabel, erhöht aber die langfristige Fehleranfälligkeit.

Besonders driftanfällig sind:

- canonical JSON und Schema-Hashing;
- Protocol Payloads und Capability Validation;
- Query-Fingerprints und Demand-Window-Identität;
- Checkpoint-Epochen und Validity Keys;
- LWT-/LWW- und Replication-Origin-Regeln;
- Frame-Größen, Chunking, Ack und Resume;
- Error-Klassifikation in fatal, transient, peer lifecycle und transport IO;
- Active-Collection-Reactivation und Resync.

Drift in diesen Bereichen wirkt oft wie Netzwerkflakiness, Browser-Quirk,
Signaling-Fehler oder SQLite-Problem. Tatsächlich ist es dann ein
Engine-Konsistenzfehler.

### 3. SQLite als Idle-CPU- und Locking-Risiko

Frühere Idle-CPU-Probleme mit SQLite zeigen, dass die native Storage-Schicht
nicht als passives Detail betrachtet werden darf. SQLite ist Teil der Engine
und muss explizit auf Idle-Verhalten, Locking, Writer-Hotpaths,
Read-Concurrency und breite Query-Fallbacks hin gesteuert werden.

Der aktuelle Code enthält bereits wichtige Gegenmaßnahmen:

- WAL und separate Read-only-Verbindungen für file-backed Stores.
- `spawn_blocking` für blockierende `rusqlite`-Arbeit.
- Bulk Writes lesen nur betroffene IDs statt ganze Tabellen.
- Change-Erkennung über `update_hook`, Trigger und `__rxdb_changed_tables`.
- Keine per-Collection-Idle-Safety-Drains für file-backed SQLite.
- Database-wide External Poll mit Backoff bis in einen 30-Minuten-Standby.
- Runtime-Counter für Statement-Zeiten, Writer-Locks, Query-Fallbacks,
  External-Poll-Wakeups und Drain-Batches.

Diese Gegenmaßnahmen müssen als Architekturprinzipien erhalten bleiben. Jede
neue Projection, Replikationsfunktion oder Demand-Loading-Erweiterung braucht
eine explizite Idle-CPU- und SQLite-Hotpath-Bewertung.

### 4. App-spezifische Sonderwege

Wenn Apps eigene Sync-, Warm-up-, HTTP- oder Storage-Pfade einführen, verliert
Business OS den Framework-Vertrag. Apps sollen lokale Handles der Shell
verwenden und keine Transportlogik besitzen.

Das Risiko ist größer als nur technische Duplikation. App-Sonderwege würden das
entstehende Framework-Modell beschädigen: Schemas, Commands, Dateien,
Permissions, Runtime-Status und Projektionen würden nicht mehr als ein
einheitlicher Business-OS-Vertrag wirken, sondern als lose Sammlung von
Einzelfeatures.

### 5. Zu großer WASM-Schnitt

Ein vollständiger Browser-Port des heutigen Rust-Native-Peers wäre ein falscher
erster Schritt. Der aktuelle native Code enthält Host-Abhängigkeiten wie
SQLite, Tokio, `webrtc-rs`, Signaling, Supervision, Filesystem und
Daemon-Policy. Diese Mechanik gehört nicht unverändert in den Browser.

## Zielarchitektur

Die Zielarchitektur ist ein lokales CTOX OS mit `ctox-rxdb` als zentraler
Runtime-Engine.

```text
CTOX OS Framework
  -> App model and module conventions
  -> User-facing install and deployment lifecycle
  -> Manifest, schema, command, file, permission contracts
  -> Release, rollback, source snapshot, runtime status
  -> Shell-delivered app handles
  -> Local user access without cloud deployment provider

ctox-rxdb Runtime Engine
  -> local-first reactive data model
  -> WebRTC-only sync
  -> demand query and file loading
  -> active collection priority
  -> checkpoint and reconnect semantics

Gemeinsamer ctox-rxdb Core in Rust
  -> deterministische Engine-Semantiken
  -> native Nutzung im Daemon
  -> WASM-Nutzung im Browser

Browser Host Adapter
  -> IndexedDB
  -> WebRTC / WebSocket / BroadcastChannel
  -> Browser-Lifecycle, Quota, Shell-Facade
  -> Demand Cache, Eviction, Browser-Diagnostics
  -> App Handles, Module Runtime, Client Framework Contracts

Daemon Host Adapter
  -> SQLite
  -> webrtc-rs und Signaling
  -> Native-Peer-Supervision
  -> Policy, Projections, Commands, Runtime-Status
  -> Module Manifests, Install, Release, Rollback
  -> Server Framework Contracts
```

Business-OS-Apps entwickeln gegen stabile CTOX-OS-Konventionen, nicht gegen
rohe Storage- oder Transportdetails. Der gemeinsame `ctox-rxdb`-Core soll
hostunabhängige Engine-Regeln besitzen. Die Adapter kapseln Host-Mechanik,
aber bauen keine eigenen semantischen Wahrheiten auf.

## Rust/WASM-Strategie

Die strategische Richtung ist eine Rust-basierte gemeinsame Engine, die im
Browser über WebAssembly genutzt werden kann. Der erste Schritt ist jedoch
nicht ein vollständiger Rewrite der Browser-Datenebene.

Priorität haben kleine, deterministische und driftanfällige Funktionen:

1. Protocol Constants und Protocol Validation.
2. Canonical JSON und Schema Hashing.
3. Checkpoint Status und Checkpoint Validity Keys.
4. Query Fingerprinting und Demand-Window Identity.
5. Byte-korrektes Frame Budgeting und Chunk Splitting.
6. LWT-/LWW-/Replication-Origin Acceptance Rules.
7. Einheitliche Error Taxonomy.

Der Browseradapter bleibt substanziell. IndexedDB ist kein trivialer Adapter:
er muss Transaktionen, lokale Indizes, Pushability, Checkpoint-Iteration,
Master-Origin-Behandlung, Query-Pläne und Divergence-Schutz leisten. WebRTC,
BroadcastChannel, Quota und Browser-Lifecycle bleiben ebenfalls
browsernative Aufgaben.

Der Nutzen des gemeinsamen Rust/WASM-Cores liegt nicht in "weniger
JavaScript" als Selbstzweck. Der Nutzen liegt in weniger semantischem Drift,
besserer Testbarkeit und höherer Zuverlässigkeit der kritischen Engine-Regeln.

## Roadmap

### Phase 1: CTOX OS Boundary

Die CTOX-OS-Grenze wird explizit dokumentiert. Jede Regel wird entweder als
OS-/Framework-Vertrag, Engine-Semantik oder Host-Mechanik klassifiziert.

Ergebnis:

- Liste der CTOX-OS-Verträge für Apps, Module, Deployment und User-Lifecycle.
- Liste der Core-Semantiken.
- Liste der Browser-Adapter-Verantwortung.
- Liste der Daemon-Adapter-Verantwortung.
- Drift-Risikomatrix für JS/Rust-Doppelimplementierungen.

### Phase 2: Shared Core Skeleton

Ein kleiner hostunabhängiger Rust-Core wird angelegt. Er darf keine Abhängigkeit
auf `rusqlite`, `webrtc-rs`, native Filesystem-Pfade, Tokio-Multithreading oder
Browser-APIs haben.

Ergebnis:

- Native Rust-Nutzung im Daemon.
- WASM-Build für Browser-Integration.
- Corpus-Tests gegen bestehende JS/Rust-Fixtures.
- Erste ersetzte Drift-Semantik, etwa Protocol Validation oder Schema Hashing.

### Phase 2B: OS-Primitiven in der Engine festziehen

Die wiederkehrenden OS-Primitiven werden als Engine-Verträge beschrieben und
gegen App- und Deployment-Szenarien getestet.

Ergebnis:

- stabiler Vertrag für App-Collections und Runtime-installed Schemas;
- stabiler Vertrag für `business_commands` als Intent-Bus;
- stabiler Vertrag für File Plane, Chunks, Ranges und Artefakte;
- stabiler Vertrag für Release-/Rollback-/Installationszustand;
- stabiler Vertrag für Runtime-Status und Projektionen;
- klare Abgrenzung zwischen Engine-Primitiv und App-spezifischer Logik.

### Phase 3: Drift-Reduktion in Hochrisiko-Regeln

Die Regeln mit größtem Divergenzrisiko werden in den gemeinsamen Core
verschoben.

Priorität:

- Checkpoint Validity;
- Frame Chunking;
- Query Fingerprinting;
- LWW-/Replication-Origin-Akzeptanz;
- Error Taxonomy.

Ergebnis:

- Weniger doppelte Semantik.
- Paritätstests prüfen Core-Verhalten statt getrennte Implementierungen.
- Reconnects, Resyncs und große Collection-Flüsse werden deterministischer.

### Phase 4: App- und Deployment-Verträge stabilisieren

Die App-Schicht wird als offizieller Teil der Framework-Strategie behandelt.
Ziel ist ein konsistentes Modell für Apps, Module und Deployment, nicht nur
eine Sammlung von Datenbankzugriffen.

Ergebnis:

- stabile App-Contracts für Datenbank-Handles und Collection-Nutzung;
- klare Regeln für runtime-installierte Schemas;
- standardisierte Command-Patterns für serverwirksame Mutationen;
- einheitliche File-Plane-Konventionen;
- definierte Permission- und Data-Access-Erwartungen;
- Installations-, Release-, Rollback- und Lifecycle-Metadaten als
  Framework-Bestandteil;
- Deployment-Gates für Schema-Kompatibilität, Rechte, Datenzugriff,
  Projektionen und Runtime-Status;
- Testfixtures, die App-Verträge und Datenebene gemeinsam prüfen.

### Phase 5: Performance- und Idle-Gates

SQLite- und Transport-Performance werden als Release-Gates definiert.

Ergebnis:

- Idle-CPU-Budget für Native Peer und SQLite-Poller.
- Writer-Lock- und Statement-Latenzbudgets.
- Query-Fallback-Budget mit Collection-/Operator-Attribution.
- Demand-only-Garantien für Chunk-Collections.
- Regression-Tests gegen per-Collection-Idle-Drains.

### Phase 6: CTOX-OS-first Feature Development

Neue Business-OS-Fähigkeiten werden bevorzugt als CTOX-OS-Fähigkeiten
implementiert, nicht als app-spezifische Sonderpfade. Wenn die Fähigkeit den
Datenpfad berührt, muss sie `ctox-rxdb` stärken statt umgehen.

Zielbereiche:

- Dateien und Blob-Chunks;
- Browser-Runtime-State;
- Agentenläufe und Queue Tasks;
- Module, Releases und Runtime-Schemas;
- Knowledge Tables und Ticket-Projektionen;
- Commands, Approvals und Audit Trails;
- MCP-/Agentenkanäle, soweit sie Business-OS-State berühren.

## Engineering Principles

1. CTOX Business OS ist ein lokales CTOX OS für Apps und Deployment.
2. `ctox-rxdb` ist die zentrale Runtime-Engine dieses OS.
3. Die CTOX-OS-Strategie fliegt oder fällt mit Robustheit, Performance und
   Evolvierbarkeit dieser Engine.
4. Der Daemon bleibt autoritativ für Policy, Persistenz und Ausführung.
5. Business-OS-Daten laufen nicht über HTTP-Fallbacks.
6. Browser-Apps bleiben transportblind und nutzen Shell-gelieferte Handles.
7. App-Deployment umfasst Manifest, Schema, Rechte, Commands, Files, Release,
   Rollback und Runtime-Status.
8. Shared Core reduziert semantischen Drift; Tests sichern die Migration.
9. SQLite-Idle-CPU ist ein Release-Risiko.
10. Polling ist nur akzeptabel, wenn es budgetiert, backoff-fähig und
   beobachtbar ist.
11. Große Datenflüsse müssen demand-, priority- und backpressure-fähig sein.
12. Reconnect, Restart und Partial Failure sind explizite Engine-Zustände.
13. Observability ist Teil des CTOX OS, nicht nachträgliche Diagnose.

## Nicht-Ziele

- Keine HTTP-Datenbridge für Business-OS-Collections.
- Keine Rückkehr zu upstream RxDB-Parität als Produktziel.
- Keine app-eigenen Sync- oder Storage-Pfade.
- Keine Reduktion des Projekts auf eine generische Datenbankbibliothek.
- Keine Framework-Features, die die `ctox-rxdb`-Engine als Datenpfad umgehen.
- Kein Browser-only Permission Gate für serverseitige Mutationen.
- Kein vollständiger WASM-Port des Native Peers als erster Schritt.
- Keine neuen Runtime-Env-Toggles für Produktionsverhalten.
- Kein Polling als Ersatz für saubere Change-Signale.
- Kein VNC/noVNC/CDP-Remote-Desktop als primärer Business-OS-Datenpfad.

## Erfolgskriterien

CTOX OS ist strategisch auf Kurs, wenn folgende Aussagen dauerhaft
erfüllbar sind:

- Business OS startet in einem frischen Browser ohne HTTP-Datenfallback.
- Der Native Peer zeigt `replicationUp` nur bei realer Datenebenenfähigkeit.
- Runtime-installed Collections integrieren sich schema- und policy-korrekt in
  den gemeinsamen Sync-Raum.
- Apps können für User installiert, aktualisiert und zurückgerollt werden, ohne
  app-eigene Daten- oder Deployment-Pfade zu benötigen.
- User können auf Apps zugreifen, ohne dass für deren Bereitstellung ein Cloud-
  Anbieter als Deployment-Ziel erforderlich ist.
- App-Deployment ist über Manifest, Schema, Release, Rollback, Datenzugriff
  und Runtime-Status als zusammenhängender CTOX-OS-Vertrag abbildbar.
- Foreground-Collections werden gegenüber Hintergrunddaten priorisiert.
- Demand-only Chunk-Collections bleiben im Idle ruhig.
- Große Query- und File-Fetches blockieren keine Control Frames und keine
  Command-Roundtrips.
- Browser-Reconnects und Native-Peer-Restarts verwenden gültige Checkpoints
  statt unnötiger Full Resyncs.
- SQLite bleibt im Idle messbar ruhig.
- Query-Fallbacks, Writer-Locks und External-Poll-Wakeups sind attribuiert und
  release-relevant.
- Kritische Semantiken wie Protocol Validation, Checkpoint Validity, Frame
  Chunking und Query Fingerprinting kommen aus gemeinsamem Core-Code.
- App-, Deployment-, File- und Command-Primitiven sind in der Engine stabil
  genug, dass neue Apps keine eigenen Infrastrukturpfade benötigen.
- Apps können neue Business-OS-Funktionen nutzen, ohne Transport- oder
  Storage-Mechanik zu kennen.
- Runtime-Apps folgen einem klaren Framework-Vertrag für Collections,
  Commands, Files, Permissions und Lifecycle.

## Schlussfolgerung

CTOX Business OS hat sich aus der ursprünglichen RxDB-Idee in eine eigene
Richtung entwickelt. Diese Richtung ist strategisch richtig: CTOX benötigt ein
lokales CTOX OS für Apps, Deployment, Vernetzung, User-Zugriff und
daemonautoritativ replizierte Business-OS-Anwendungen.

`ctox-rxdb` bleibt dafür entscheidend. Es ist die Runtime-Engine für lokale
Reaktivität, WebRTC-Sync, Demand Loading, Checkpoints, Reconnects und
persistente Browser/Daemon-Kopplung. Es darf nicht mit dem gesamten Framework
gleichgesetzt werden, aber die CTOX-OS-Ausrichtung fliegt oder fällt mit
dieser Engine.

Die nächste Entwicklungsstufe besteht darin, beide Ebenen sauber zu führen:
CTOX OS als Gesamtprodukt für App-Erstellung, Deployment, Vernetzung und
User-Zugriff, und `ctox-rxdb` als robuste, performante, semantisch
einheitliche Runtime-Engine darunter. An dieser Trennung und Kopplung
entscheidet sich, ob Business OS eine dauerhaft belastbare App-Plattform für
User wird.
