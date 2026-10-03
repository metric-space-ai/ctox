# CTOX Sync: Kern-Erneuerung und Ablösung

Stand: 2026-09-06. Dieses Dokument beschreibt den Arbeitsstand der genehmigten
Offensive. Es ist keine Produktionsfreigabe. Der neue native Kern ist noch nicht
an die Workjet-Ausführung angeschlossen; die alten Ausführungspfade sind daher
noch aktiv. Ihre Entfernung gehört ausdrücklich zur Abnahme.

Der portable Checkpoint-Vertrag für die spätere Ausführungsanbindung ist in
[ctox-sync-portable-checkpoint-contract.md](ctox-sync-portable-checkpoint-contract.md)
dokumentiert. Er verschärft das Manifest auf Version 2 und beschreibt die
Capture-/Restore-Grenze; er ersetzt noch keinen Codex-/Claude-Adapter und ist
keine Produktionsfreigabe.

## Aktueller Abnahmestand

Nachtrag 2026-09-08: Die unten protokollierten grünen Standalone-Sync-Tests
verwendeten WebRTC 0.20.5, während das CTOX-Binary 0.20.0-alpha.1 mit lokalem
ICE-Patch verwendete. Auch den separaten RxDB-Tests fehlte dieser Patch.
Die Ergebnisse bleiben historische Einzelbefunde, belegen aber keine Abnahme
des ausgelieferten Transports. Die Angleichung und erneute Prüfung sind in
[Native transport parity](ctox-sync-native-transport-parity-20260908.md)
dokumentiert. Die Vier-Prozess-Abnahme bleibt bis zum erfolgreichen Gegenbeleg rot.

Die separate Abnahme mit vier echten CTOX-Prozessen ist **rot**. Mit privaten
IPC-Verzeichnissen starten alle vier Hosts und beantworten die lokale
Identitätsprüfung. Die Signalisierungsdiagnose weist sechs Angebote, sechs
Antworten und zwölf ICE-Kandidaten nach (`host-private-ipc-cli-signal-counts.json`).
Trotzdem bestätigt der erste Voter innerhalb der unveränderten 15-Sekunden-Grenze
keine Mehrheit; die übrigen Voter haben keine nutzbare WebRTC-Kontrollroute.
Die vier Host-Stderr-Protokolle bleiben leer. Der Fehler ist damit hinter dem
Signalisierungsaustausch eingegrenzt, seine Ursache aber noch nicht bewiesen.
Aufnahme, Wiederverbindung, Wiederanlauf und Widerruf werden in dieser
Prozessabnahme noch nicht erreicht. Grüne Tests innerhalb eines Prozesses
ersetzen diese fehlende Abnahme nicht. Dieser Implementierungsstand wird auf
ausdrücklichen Nutzerwunsch auf `main` integriert; er ist keine Freigabe für
robustes SSH-/QR-Onboarding oder produktives Agent-Failover.

Die beiden generierten Workjet-Vertragsdateien sind mit `ca7dd885f` auf
Workjets `main` veröffentlicht. Sie wurden in einem sauberen Checkout von
`dbcf2072e` generiert; Paket-Typecheck, Host-Schema-Prüfung und der Driftcheck
aller fünf Ausgaben bestehen dort. Der bestehende, anderweitig veränderte
Workjet-Checkout wurde dafür nicht zusammengeführt oder bereinigt.

### Gemeinsamer nativer CLI-/Service-Host

Der abschließende vollständige Sync-Testlauf nach der IPC-Rechtekorrektur besteht
**74/74**, ohne Skips (`host-main-final-sync.json`). Darin sind fünf
Host-Transportprüfungen und alle 14 WebRTC-Szenarien enthalten; letztere benötigen
17,19 Sekunden. Dieser Lauf bleibt vom oben beschriebenen roten Vier-Prozess-Test
getrennt. Die Formatprüfung des Sync-Crates und des betroffenen CTOX-Host-Adapters
besteht ebenfalls. Clippy über alle Sync-Targets mit `-D warnings` besteht
abschließend in 25,60 Sekunden (`host-main-final-clippy.json`).

`src/core/sync_host/` verbindet den vorhandenen CTOX-Service und die neue lokale
`ctox sync`-CLI mit derselben `host_runtime`-Funktion. Der Adapter lädt die
unveränderlichen Host-Pins aus der vorhandenen Runtime-SQLite-Datenbank und
Schlüssel sowie gesonderte Transportkonfiguration aus dem verschlüsselten
Secret-Store. Ein gemeinsamer Prozess-Lock schützt die Stores bereits vor ihrem
Öffnen. Der Host startet den bestehenden nativen Sync-Lifecycle mit einem
eigenen leeren Kontroll-DB-Handle; Business-Collections werden auch dann
abgewiesen, wenn ihre Replikationsliste leer ist. Der tatsächliche IPC-Endpunkt
wird veröffentlicht, aktiv auf Identität/Protokoll geprüft und beim Ende der
Laufzeit entzogen. Der lokale Status behauptet ausschließlich Listener-Liveness.

Die öffentlich eingelesenen Host-Setup-Typen sind nun Teil der bestehenden
kanonischen Fixture mit generierten Rust-, TypeScript- und Effect-Definitionen.
Die bisherigen handgeschriebenen Rust-Typen entfallen. IPC-Version 1 und native
Kontrollversion 5 bleiben unverändert. Die Transportkonfiguration verlangt
explizite ICE-Einstellungen und eigene Signaling-Zugangsdaten; sie verwendet
keine Business-OS-Grant-Felder als Ersatz. Eine Suche nach einem alten Secret-Key
legt auf einer frischen Installation keine Business-Datenbank mehr an. Bereits
vorhandene Legacy-Key-Daten bleiben im bestehenden Migrationspfad.

Der erste CLI-Abnahmeversuch endete mit einer unlokalisierten Deadline
(`host-runtime-cli.json`). Der ergänzte Kontext lokalisierte den Abbruch beim
ersten `sync init` (`host-runtime-cli-diagnostic-fixed.json`). Ein separater
Prozess-Sample zeigte ausschließlich `_dyld_start`, also noch keinen CTOX-Code;
dieser Probeprozess beendete `init` anschließend erfolgreich nach 11,42 Sekunden.
Das erklärt einen beobachteten Startverzug, beweist aber nicht rückwirkend die
Ursache jedes Timeouts. Die CLI-Prüfung behält ihre 20-Sekunden-Startgrenze.

Die erste vollständige Sync-Prüfung mit generierten Host-Typen lief gleichzeitig
mit dem großen CTOX-Binary-Link (`host-contracts-sync.json`). Die ersten neun
Testprogramme bestanden; in der WebRTC-Prüfung bestanden 11/14 Szenarien.
Zwei Fehler zeigten bestätigte Aufnahme-Replays, während die Assertions einen
erstmaligen Applied-Beleg erwarteten; ein weiterer Lauf erreichte den erwarteten
Quorum-Zustand nicht. Die Diagnose enthält SQLite-Append-Zeiten bis etwa
1,9 Sekunden. Diese Lastbeobachtung ist kein Nachweis eines behobenen Fehlers.
Die Assertions und Deadlines bleiben unverändert. Clippy mit allen Targets und
`-D warnings` bestand anschließend (`host-contracts-clippy.json`). Der aktuelle
gemeinsame Stand enthält zusätzlich die unverändert übernommene Crew-Integration
von `ca1c0e362`; er erhält eigene Build- und Testnachweise.

Auf diesem gemeinsamen Stand besteht die Browser-RxDB-Suite **117/117**, ohne
Skips. Clippy über alle Sync-Targets mit `-D warnings` besteht ebenfalls
(`host-main-clippy.json`, 26,29 Sekunden). Der Workjet-Contract-Typecheck, die
drei vorhandenen IPC-Schema-Tests, die neue Host-Schema-Prüfung und der
Generator-Driftcheck für alle fünf Rust-/TypeScript-Ausgaben bestehen. Diese
Workjet-Prüfungen verwenden den kanonischen lokalen Checkout; eine neue
Desktop-/Mobile-Veröffentlichung ist damit nicht nachgewiesen.

Der vollständige gemeinsame Sync-Lauf besteht anschließend **73/73**, ohne
Skips (`host-main-sync.json`): 19 Unit-, 11 Cluster-, 5 Checkpoint-, 1 Effect-,
5 Konfigurations-, 4 Transport-, 2 IPC-, 9 Lifecycle-, 1 Store-Conformance-,
14 WebRTC-, 1 Membership- und 1 Workjet-Key-Test. Die 14 WebRTC-Szenarien benötigen
16,25 Sekunden; ihre bestehenden Assertions und Deadlines sind unverändert.
Während dieses Laufs läuft kein Cargo-Build parallel. Die separat gestartete
macOS-Signaturprüfung ist als zusätzliche Leseaktivität zu berücksichtigen.
Der gemeinsame CTOX-Binary-Build besteht (`host-main-binary.json`, 8m22s);
die 484 Warnungen entsprechen dem zuvor beobachteten Umfang.

Die CLI-Abnahme des ungekürzten 809-MiB-Dev-Binary scheitert dagegen weiterhin
an `sync init` vor der Host-Veröffentlichung (`host-main-cli.json`). Ein weiterer
Sample mit dem gleichen Legacy-Datensatz zeigt wieder ausschließlich
`_dyld_start` und noch keinen CTOX-Code (`host-legacy-probe.sample`). Die
macOS-Signaturprüfung bestätigt einen gültigen Binary. Der Testdatenträger ist
ein fast volles HFS+-SD-Medium. Die Diagnose ersetzt keine erfolgreiche
Prozessabnahme und keinen Nachweis der Startup-Performance eines Releases.

Eine separate, erneut signaturgeprüfte Testkopie ohne Debug-Symbole behält alle
23 geprüften Code-/Datensektionen unverändert (`host-test-binary.json`), reduziert
die Dateigröße aber nur von 848.006.056 auf 764.681.200 Bytes. Auch diese erste
Prozessabnahme scheitert beim Start (`host-main-cli-stripped.json`). Die Fixture
verwendet anschließend Symlinks statt Hardlinks, damit sie beim Anlegen und
Aufräumen ihrer Bundle-Marker den Link-Zähler des geprüften Binary nicht verändert.
Dieser Lauf erreicht Identitätsanlage, Legacy-Key-Migration, Workjet-Key-Import
und Konfiguration; der erste Host endet vor Veröffentlichung des Listeners.
Die ergänzte Diagnose zeigt `PermissionDenied` (`host-main-cli-start-diagnostic.json`).

Der Host hatte das IPC-Verzeichnis mit den umask-abhängigen Standardrechten von
`tempfile` erzeugt. Der Listener verlangt bereits private Verzeichnisrechte und
weist diesen Pfad korrekt ab. `local_host::private_ipc_directory` erzeugt das
Verzeichnis jetzt atomar mit angeforderten Rechten `0700`; der CTOX-Adapter nutzt
diese Kernfunktion. Die bisherige Rechteprüfung bleibt unverändert. Ein eigener
Test prüft Rechte und Akzeptanz durch den tatsächlichen Host-Lock. Die veraltete
Testkopie wurde nach bestätigtem Ende aller Nutzer entfernt; der ursprüngliche
geliehene Build-Target blieb erhalten. Der folgende Build entfernt die Symbole
bereits beim Linken des lokalen Test-Binary (`-Cstrip=symbols`); das ist keine
neue Runtime-Konfiguration und keine veröffentlichte Release-Abnahme.

Die gemeinsame Signaling-Fixture ist aus den bestehenden WebRTC-Tests
extrahiert; derselbe lokale Server wird für die separate Vier-Prozess-Abnahme
verwendet. Diese ruft das tatsächlich gebaute CTOX-Programm auf und prüft
Legacy-Key-Migration, importierten Workjet-Key, exklusiven Host-Besitz, aktuellen
Listener, Aufnahme, Reconnect, Worker-Neustart und Widerruf. Sie führt keinen
Coding-Harness aus. Aktuelle Endergebnisse werden nach Abschluss dieses Laufs
hier ergänzt; bisherige grüne Kernprüfungen ersetzen diese Abnahme nicht.

Offen bleiben produktive Signaling-Grants, vollständige CTOX-Service-Abnahme,
Windows-Listener, Workjets produktive SSH-/QR-Aufnahme und echte Harness-Ausführung.
Details und administrative CLI-Schritte stehen im
[Host-Vertrag](ctox-sync-worker-host-contract.md).

### Dauerhafte native Host-Pins

`host_config.rs` definiert die lokale native Konfiguration und speichert sie in
einer eigenen Tabelle der vorhandenen Host-Runtime-SQLite-Datenbank. Scope,
lokaler Node/Public-Key, Voter- oder Worker-Rolle, die drei Voter-Schlüssel und
ihre Fähigkeiten bleiben über Neustarts gebunden. Ein erneutes Speichern darf
diese Bindung nicht still verändern. Dafür braucht es einen geprüften
Migrationsweg; Raft-Zeitparameter können für den nächsten Start geändert werden.
Der Schreibvorgang verwendet eine unmittelbare SQLite-Transaktion, das Laden
validiert das Format und seine Invarianten erneut. Fremde Runtime-Tabellen
werden weder neu aufgebaut noch überschrieben.

Die bestehenden nativen Voter-/Worker-Attachments erhalten ihre Optionen aus
dieser Konfiguration und prüfen den lokalen Schlüssel. Routen starten leer und
werden weiterhin durch signierte Discovery ermittelt. Schlüsselmaterial,
Signaling-Token und aktive IPC-Endpunkte gehören nicht in diesen Datensatz.
Der gemeinsame Kontrollraum ist `ctox-execution:<scope>`; produktive
Business-OS-Räume werden nicht als Ausführungsnetz verwendet. Worker erhalten
keinen eigenen Raft-Store. Ihre lokalen Pins ersetzen keine Quorum-Aufnahme.

Der erste echte Neustartlauf zeigte eine unzulässige Kopplung: Das Ableiten des
IPC-Verzeichnisses aus dem vollständigen Datenpfad überschritt auf macOS die
Unix-Socket-Pfadlänge (`host-config-restart.json`). Der Host übergibt deshalb das
lokale private IPC-Verzeichnis getrennt vom dauerhaften Speicherpfad, wie es der
bestehende Listener bereits vorsieht. Dessen Rechte-, Besitzer- und
Exklusivitätsprüfungen bleiben erhalten; es gibt keinen Netzwerk-Fallback.

Die neue Neustart-Fixture fährt drei native Voter und einen Worker vollständig
herunter und rekonstruiert anschließend Schlüsselobjekte und Konfiguration aus
denselben Pins sowie die Voter aus denselben Raft-Stores. Sie prüft vorhandenen
Auftragsbesitz, unveränderte Mitgliedschaft, Replay, Widerruf und die Sperre
erhaltener Handles nach Shutdown über echte lokale WebRTC-Verbindungen.
Der gezielte Neustarttest besteht in 26,61 Sekunden, ohne Änderung der
60-Sekunden-Frist (`host-config-restart-receipt.json`). Ein vorheriger Lauf
scheiterte an der erwarteten Aufnahme-Receipt-Art, ohne deren Wert auszugeben
(`host-config-restart-ipc.json`). Die Assertion blieb erhalten und gibt jetzt den
Receipt aus. Der anschließende Erfolg beweist die Ursache dieses Zwischenfehlers
nicht. Die vollständige Sync-Suite besteht anschließend **65/65**, ohne Skips
(`host-config-sync.json`): 19 Unit-, 11 Cluster-, 5 Checkpoint-, 1 Effect-,
5 Host-Konfigurations-, 2 IPC-, 9 Lifecycle-, 1 Store-Conformance-, 10 echte
WebRTC-, 1 Membership- und 1 Workjet-Identitäts-Test. Die zehn WebRTC-Szenarien
benötigen zusammen 17,86 Sekunden, der Build 27,39 Sekunden. Die bestehenden
Assertions und Fristen sind unverändert. Clippy über alle Sync-Targets mit
`-D warnings` besteht in 12,93 Sekunden (`host-config-clippy.json`); Format- und
Diff-Prüfung bestehen ebenfalls. Browser-/Wire-Suite und CTOX-Gesamtcheck wurden
für diese ausschließlich native Konfigurationsänderung nicht erneut ausgeführt;
ihre unten genannten Ergebnisse gehören zum vorherigen Quellstand.

Dieser vorherige Abnahmestand umfasste noch keinen produktiven Host-Anschluss.
Der nachfolgende CLI-/Service-Adapter ist oben beschrieben; Signaling-Zulassung
und sichtbare SSH-/QR-Aufnahme bleiben offen.
Es wird keine zertifizierte Harness-Wiederherstellung oder Migration behauptet.

### Wiedererkennung von Votern nach Adresswechsel

Bei der Prüfung des produktiven Host-Anschlusses zeigte sich eine notwendige
Vorarbeit: `ExecutionGroupOptions.routes` und `WorkerExecutionOptions.routes`
verlangten drei feste Signaling-Adressen. Der bestehende Reconnect übernahm nur
die geänderte lokale Adresse, aktualisierte aber die Routen zu anderen Votern
nicht. Diese Adressen als dauerhafte Host-Konfiguration zu speichern hätte
einen Neustartfehler festgeschrieben.

Die Route-Map ist jetzt ein optionaler Starthinweis. Native Peers im selben
admittierten Raum werden über `ctox.sync.authority.route.v1` geprüft. Dieser
native Kontrollaufruf verwendet den vorhandenen signierten Envelope mit eigenen
Request-/Reply-Kennungen und bindet Scope, Empfänger, frische Nonce und aktuelle
Signaling-Adresse. Eine Route wird erst nach einem gültigen Nachweis eines der
drei konfigurierten Voter-Schlüssel ersetzt; die Verbindung muss nach der Antwort
noch dieselbe und weiterhin admittiert sein. Worker-Schlüssel dürfen damit keine
Voter-Route übernehmen. Der Nachweis ruft keine fachliche Operation auf und
vergibt weder Mitgliedschaft noch Ausführungsberechtigung.

Die bisherige Pflicht zu vollständigen Adress-Maps und die ausschließlich daran
gebundene Discovery sind ersetzt. Auch die eigene ereignisbasierte Liste offener
Verbindungen entfällt: Beim späten Anhängen eines Workers fehlten darin bereits
geöffnete Kanäle. Die Discovery liest ihre aktuellen Verbindungen aus dem
Transport-Handler; Ereignisse wecken nur die Verarbeitung.

Da beide nativen Hosts jetzt Kandidaten entdecken, ist die frühere einseitige
Worker-Initiierung entfernt (`connect_worker_to_authority_peer` samt internem
Boolean-Sonderpfad). Voter und Worker verwenden dieselbe Regel: Nur die kleinere
Signaling-ID erzeugt ein Angebot. Der allgemeine native Browser-Responder bleibt
passiv. Die WebRTC-Fixture erfasst die tatsächlichen SDP-Angebote und prüft diese
Richtung zusätzlich zu den bisherigen Aufnahme-, Widerrufs- und Datengrenzen.

Es gibt weiterhin genau drei Voter. Bestätigte
Konfiguration, produktive Signaling-Grants, Anschluss an den CTOX-Service und
SSH-/QR-Abnahme sind damit noch nicht erledigt.

Die beiden neuen Signatur-/Replay-Prüfungen bestehen mit allen bisherigen
Kern-Unit-Tests (19/19, `voter-route-unit.json`). Der zusätzliche echte
WebRTC-Test besteht in 19,82 Sekunden (`voter-route-single-initiator.json`):
Start ohne Routen, Voter 3 unter neuer Adresse, anschließend Voter 2 stoppen.
Die wiederhergestellte Verbindung ist damit für die Mehrheit nötig. Der
bestehende Worker-Auftrag bleibt autorisiert; Wiederholung, Widerruf und
verweigerte Business-Datenzugriffe werden weiterhin geprüft. Der Test erfasst
auch die tatsächlichen SDP-Angebote und fordert die gemeinsame Initiator-Regel.
Die vollständige native RxDB-Suite besteht **422/422**: 389 Unit-Tests
(46,62 s), 31 Conformance-Tests, Error-Guard und Idle-Budget (8,59 s), ohne Skips.
Beleg: `voter-route-native.json`. Der Build benötigte auf dem gemeinsamen
externen Volume 4 Minuten 9 Sekunden; dies ist keine App-Laufzeitmessung.
Die vollständige Sync-Suite besteht **59/59**, ohne Skips: 19 Unit-, 11 Cluster-,
5 Checkpoint-, 1 Effect-, 2 IPC-, 9 Lifecycle-, 1 Store-Conformance-,
9 echte WebRTC-, 1 Membership- und 1 Workjet-Identitäts-Test. Die neun
WebRTC-Szenarien benötigen zusammen 17,37 Sekunden; Build 1 Minute.
Beleg: `voter-route-sync.json`. Die unveränderten Fristen bleiben erhalten.
Beide Crates bestehen Clippy über alle Targets mit `-D warnings`
(`voter-route-native-clippy.json`, 33,05 s; `voter-route-sync-clippy.json`,
52,38 s). Die Formatprüfungen und die fünf generierten Verträge sind konsistent.
Der aktuelle Wire-Daemon ist gebaut (`voter-route-wire-build.json`, 42,65 s).
Die Browser-/Wire-Suite besteht mit `--require-wire-daemon` **116/116**, ohne
Skips (`voter-route-js-wire.log`); Cross-Process-Wire 2,73 s, File-Fetch 1,89 s.
`cargo check --locked --bin ctox` mit dem bisherigen Dev-Profil besteht in
4 Minuten 21 Sekunden; unverändert 484 Bestandswarnungen, davon 478 im
CTOX-Binary. Belege: `voter-route-root-check.json` und
`voter-route-root-check.log`. Alle genannten Belege liegen unter
`/Volumes/tmp/dev-artifacts/ctox/sync-core-offensive/`.
Diese Prüfungen zertifizieren weder WAN-Betrieb noch produktive SSH-/QR-Aufnahme,
Harness-Wiederaufnahme oder die noch ausstehende Runtime-Migration.

Zwischenläufe bleiben im Nachweisverzeichnis erhalten: ein Borrow-Konflikt im
neuen Fixture, fehlende Worker-Routen beim späten Attachment und ein nicht
admittierter neuer Voter-Kanal vor Entfernung des Initiator-Sonderpfads. Eine
zwischenzeitlich fehlgeschlagene Admission-Assertion lieferte noch keinen
Receipt-Wert; ihre Diagnostik wurde ergänzt und die Assertion beibehalten.
Sie wird nicht ohne weiteren Nachweis einem bestimmten Fehler zugeordnet.

### Native Worker ohne Business-OS-Collections

Der gemeinsame NativeSync-Lifecycle erhält die Datenbankidentität jetzt explizit
über `NativeSyncOptions.database`. Eine leere replizierte Collection-Menge ist
gültig; fremde Datenbankzuordnungen werden vor dem Signaling-Aufbau abgewiesen.
Das alte repräsentative `RxWebRTCReplicationPool.collection`-Feld ist entfernt,
seine Aufrufer wählen die benannte Collection aus der öffentlichen Collection-Liste.

Reine Kontroll-Peers melden `collection: null` und ausdrücklich leere
Schema-/Checkpoint-Maps über den bestehenden Wire-Vertrag. Eine ausgelassene
Schema-Map eines schemafähigen Peers bleibt ein Fehler. Beide RxDB-Wahlrichtungen
registrieren nur für vom anderen Peer angebotene Collections Replikation:
keine Master-Streams oder Fork-Writer gegen einen reinen Kontroll-Peer.
Auch die frühere Ausnahme für höchstens eine Collection entfällt: Sie ließ
Schema-/Checkpoint-Maps weg und wurde vom datenlosen Koordinator korrekt als
ungültiger moderner Handshake abgewiesen. Moderne Peers vergleichen ihre
Schemas über die benannten Map-Einträge, unabhängig von ihrer jeweiligen
repräsentativen Collection. Die Fristen bleiben unverändert.
Die Workjet-/Reconnect-Fixture verwendet jetzt den Verbund 2/1/0/0 Collections
mit absichtlich unterschiedlichen Vertretern der beiden Daten-Peers.
Sie erzeugt für Worker und Koordinationsstimme
keine Business-Collections; ihre beiden Daten-Peers behalten die vorhandene
Collection für den Test auf verweigerte Business-Datenzugriffe.

Der neue vollständige native Lauf besteht **422/422**: 389 Unit-Tests,
31 Conformance-Tests, Error-Guard und Idle-Budget, ohne ignorierte Tests.
Beleg: `control-only-mixed-native.json`. Drei anfängliche Fehler lagen in
den Fixtures: getrennte Datenbanken im Multiplex-Aufbau, fehlender interner
Store beim Versuch, eine zweite Collection anzulegen, und die fehlende
gegenseitige Admission im neuen Mock. Der gemeinsame Collection-Testhelfer
verwendet jetzt eine explizit geteilte Datenbank; die Kontroll-Fixture bestätigt
ihre Admission. Die bestehenden Isolations- und Sicherheitsassertions bleiben
erhalten. Der zwischenzeitliche Compilerfehler im Testhelfer wurde behoben.
Die vollständige Sync-Suite besteht anschließend **56/56**, einschließlich
**8/8 echter WebRTC-Szenarien** (15,10 Sekunden), 9 Lifecycle-Tests und allen
11 Cluster-Fällen. Aufnahme und Reconnect prüfen dabei den Verbund 2/1/0/0. Beleg:
`control-only-mixed-sync.json`. Der zuvor reproduzierte Leader-Timeout des
datenlosen Koordinators ist damit nach Entfernung der Single-Collection-
Ausnahme behoben; dafür wurde keine Frist erhöht. Das ist getrennt von den
historischen Last-/Storage-Timeouts der vorherigen Main-Integration.

Beide Crates bestehen Clippy über alle Targets mit `-D warnings`;
die abschließende Korrektur ersetzt ausschließlich einen unnötigen Arc-Clone
im Test durch eine Slice-Referenz. Alle Formatprüfungen und die fünf generierten
Verträge bestehen. Der frisch gebaute Wire-Daemon besteht mit der vollständigen
JS-/Browser-Suite **116/116**, ohne Skips (`control-only-js-wire.log`).
`cargo check --locked --bin ctox` mit dem bisherigen Dev-Profil besteht in
56,38 Sekunden; der Root-Bestand meldet weiterhin 484 Warnungen.
Belege: `control-only-native-clippy-final.json`, `control-only-sync-clippy.json`,
`control-only-wire-build.json` und `control-only-root-check.json` unter
`/Volumes/tmp/dev-artifacts/ctox/sync-core-offensive/`.
Diese Funktionsprüfungen ersetzen keine WAN-, Harness- oder Performance-Abnahme.

Dieser Kernschritt liefert weiterhin keinen produktiven Signaling-Grant,
keine persistierte Host-Konfiguration und keine SSH-/QR-Produktabnahme.

### Integration auf GitHub-main

Der anschließend veröffentlichte Crew-Commit `b0e24d470` (#59) ist in den
Sync-Kandidaten übernommen. Seine neuen `DocumentReadPolicy`-Feldrechte bleiben
in Master-, Live- und Query-Antworten erhalten. Die Feldabfrage prüft zusätzlich
die Verbindungsgeneration: Eine alte Verbindung darf keine Administratorrechte
ihrer Ersatzverbindung übernehmen. Der bestehende Crew-Live-Test wurde auf die
generationsgebundenen Verbindungen umgestellt und um diesen Fall ergänzt.

Der aktuelle vollständige kombinierte native Lauf besteht **419/419**:
386 Unit-Tests, 31 Conformance-Tests, Error-Guard und Idle-Budget. Keine Fehler,
keine ignorierten Tests. Beleg: `main-combined-native-repeat.json` im externen
Nachweisverzeichnis. Der erste kombinierte Lauf fand einen Fehler beim Aufbau
des neuen Tests (alte Testverbindung vor Ersatz nicht geschlossen); die reguläre
Close-Operation wurde ergänzt, ohne Assertions zu entfernen. Beim erneuten Lauf
verzögerte macOS den Programmstart; die Prozessprobe zeigt `_dyld_start`, bevor
Rust-Tests liefen. Der Prozess lief anschließend regulär erfolgreich durch;
eine vorbereitete alternative Startmethode wurde nicht verwendet. Die eigentlichen
Tests benötigten 4,69 / 0,10 / 0,01 / 8,56 Sekunden. Build- und Programmstartzeiten
auf dem externen Volume sind keine Laufzeit- oder WAN-Performance-Abnahme.

`cargo check --locked --bin ctox` mit dem bisherigen Dev-Profil besteht auf der
Kombination (1 Minute 51 Sekunden, 484 Warnungen über alle beteiligten
Pakete). Der Build setzt den regulären `npm run build` des Pi-Sidecars voraus;
sein generiertes Bundle fehlt in einem frischen Checkout zunächst. Native und
Sync-Formatprüfungen sowie die fünf generierten Verträge sind konsistent.
Die kombinierte Sync-Abnahme umfasst nun **54/54 bestandene Tests**:
17 Kern-Tests, 11 Cluster-Tests und anschließend alle 26 übrigen Prüfungen,
darunter **8/8 echte WebRTC-Szenarien** (14,29 Sekunden). Es wurde nichts
übersprungen. Der erste parallele Cluster-Lauf scheiterte in 9 von 11 Fällen
an unbestätigten Ergebnissen innerhalb der unveränderten Fristen. Danach
bestanden der betroffene Einzeltest und alle 11 Cluster-Fälle erneut parallel
(30,82 Sekunden), mit demselben Testprogramm und ohne Quelländerung. Die übrigen
Targets wurden vollständig nachgeholt. Das beweist keine behobene Ursache der
ersten Timeouts; dieser Befund bleibt für die Last-/Performance-Abnahme offen.
Belege: main-combined-cluster-reconciliation.json und
main-combined-sync-remaining.json. Beide Crates bestehen anschließend Clippy
über alle Targets mit `-D warnings`; die formalen Korrekturen betreffen getrennte
Rustdoc-Kommentare und die Position des unveränderten Feldrechte-Testmoduls.

Die vollständige Browser-/Wire-Suite mit frisch aus der Kombination gebautem
Daemon besteht abschließend **116/116**, ohne übersprungene Tests. Der erste
kombinierte Lauf meldete erneut `timeout: ready` in der File-Fetch-Fixture.
Die unveränderte Einzelprüfung bestand danach mit 102 Chunks / 800 KB, ebenso
die vollständige Wiederholung. Es wurden keine Deadlines oder Prüfungen gelockert.
Diese Start-/Lastbefunde bleiben dokumentiert und sind keine Produktionsfreigabe.
Der Merge liefert den gemeinsamen Kern und die korrigierten nativen Verbindungen;
produktive SSH-/QR-Aufnahme, Ausführungs-Supervision, vollständige Portabilität,
Shell-Umstellung und koordinierte Datenmigration bleiben offen.

### Vor der Crew-Integration geprüfter Ausgangsstand

Die folgenden 416/54-Testergebnisse gehören zum lokalen Ausgangsstand vom
6. September. Für die Main-Integration wurden nur die eigenen Sync-Änderungen
auf GitHub-`main` `55d5de646591e5db95b88b1f8fadaabe703bd582` übernommen.
Der ursprüngliche Checkout bleibt erhalten: Er war 250 Commits vor und 162
Commits hinter diesem Remote-Stand und enthält laufende fremde Änderungen.
Verzeichnisvergleiche bestätigen identische native RxDB-Quellen, RxDB-Tests
und die vollständige Sync-Crate zwischen geprüftem Ausgangsstand und Integration.
Die Root-Abhängigkeiten werden aus dem Main-Lockfile neu aufgelöst; Browser-
Verbraucher und Hauptprogramm erhalten deshalb eigene Integrationsprüfungen.

Der aktuelle vollständige native RxDB-Lauf ist erstmals nach sämtlichen
Verbindungs-/Transferkorrekturen grün: **416/416 Tests**, davon 383 Unit-Tests,
31 Conformance-Tests, Error-Contract-Guard und Idle-Budget-Test. Kein Fehler,
kein ignorierter Test. Build: 2 Minuten 56 Sekunden; die Unit-Tests benötigen
6,06 Sekunden, Conformance 1,30 Sekunden, Error-Guard 0,04 Sekunden und
Idle-Budget 8,58 Sekunden. Der eigene Ressourcen-Guard hat nicht pausiert.
Beleg: `/Volumes/tmp/dev-artifacts/ctox/sync-core-offensive/native-generation-full-run.json`.
Der Stand umfasst die anschließend unten historisch beschriebenen Query-,
Sendewarteschlangen-, Offer- und DataChannel-Abbaukorrekturen. Die folgende
historische Kennzeichnung dieser Änderungen als noch ungeprüft ist damit
überholt. Anschließend besteht die vollständige Sync-Suite mit aktiviertem
WebRTC über alle Targets: **54/54 Tests**, ohne Fehler oder ignorierte Tests.
Enthalten sind 17 Kern-Tests, 11 Cluster-Tests, 5 Checkpoint-Tests,
1 Wirkungs-Replay, 2 IPC-Frame-Prüfungen, 7 Native-Lifecycle-Tests,
1 vollständige OpenRaft-Storage-Prüfung, **8 echte WebRTC-Szenarien**,
1 Worker-Mitgliedschaftstest und 1 Node/Rust-Identitätsimport. Die WebRTC-Fälle
benötigen 18,87 Sekunden; Buildzeit 3 Minuten 36 Sekunden. Auch der zuvor
fehlgeschlagene Worker-Reconnect und die aktuelle Mitgliedschaftsabfrage über
Unix-IPC und WebRTC bestehen jetzt. Belege: `sync-webrtc-full-run.json` und
`sync-webrtc-full-run.log` im selben externen Nachweisverzeichnis.
Die JS-Wire-Suite besteht am lokalen Ausgangsstand **115/115**, ohne übersprungene
Tests. Im ersten Lauf trat ein nicht erneut reproduzierter Fünf-Sekunden-Timeout
beim Start der File-Fetch-Fixture auf; die unveränderte Einzelprüfung und die
vollständige Wiederholung bestehen. Das ist kein Nachweis einer behobenen
Startup-Ursache. Ein Identifier-Guard wurde durch Anonymisierung von Kommentaren
erfüllt; GitHub-main enthielt bereits dieselbe Bereinigung. Schutztests und
Deadlines wurden nicht verändert. Der alte Root-Check wurde gezielt mit Exit
143 beendet und gilt nicht als bestanden. Die integrierten Prüfungen auf
GitHub-main und der produktive SSH-/QR-Aufnahmeablauf bleiben offen.
Ein grüner nativer Transporttest ist keine Abnahme dieser Produktabläufe.

## Historische Entwicklung und Zwischenprüfungen

Die nachstehenden Zwischenstände dokumentieren Ursache und Nachweis früherer
Korrekturen. Aussagen wie „noch offen“ gelten jeweils für diesen damaligen
Lauf; für den heutigen Freigabestand ist der vorstehende Abschnitt maßgeblich.

Neu ergänzt ist `workerMembership { nodeId }` im gemeinsamen lokalen IPC-
Vertrag. Die native Abfrage liest den aktuellen Eintrag einschließlich Widerruf
erst nach einer linearisierbaren Quorum-Prüfung. Ein Worker darf ausschließlich
seinen eigenen Eintrag abfragen; die Leseberechtigung eines widerrufenen Workers
erteilt weder Ausführungs- noch Stimmrechte. Historische Aufnahmebelege werden
nicht als aktueller Zustand ausgegeben. Die Authority-Kontrollversion ist dafür
5; IPC bleibt bei Version 1 mit additiver Operation. Alle fünf generierten
Rust-/TypeScript-/Effect-Dateien sind synchron geprüft. Die vorhandenen Workjet-
IPC-/Schema-Prüfungen bestehen erneut 9/9 (6,57 Sekunden); sie ersetzen nicht
die native Quorum-Abnahme.

Die vorhandenen signierten Drei-Peer-/Worker-Tests sind um aktuelle Mitgliedschaft,
historischen Aufnahme-Replay nach Widerruf, fremde Node-/Schlüsselabfragen,
Neustart, isolierten alten Leader und Shutdown erweitert. Der gezielte Cargo-
Testlauf ohne WebRTC besteht nach Wiederaufnahme **3/3 Tests** (20,87 Sekunden,
8 weitere Cluster-Tests herausgefiltert). Er verwendet signierte RPCs über den
Test-Bus, unabhängige dauerhafte Raft-Stores und direkten `AuthorityIpc`-Dispatch;
echte WebRTC-Verbindungen und Unix-Socket-Clients sind damit noch nicht geprüft.
Die gemeldeten 52 Minuten 4 Sekunden Buildzeit enthalten die lange koordinierte
SIGSTOP-Pause und sind keine Performance-Messung. Cargo und Speicherguard sind
terminal; anschließend waren 4,8 GiB frei. Der Beleg liegt unter
`/Volumes/tmp/dev-artifacts/ctox/sync-core-offensive/worker-membership-quorum-first-run.json`.
Anschließend besteht derselbe frisch gebaute Cluster-Testbinärstand ohne Filter
**11/11 Tests** (23,37 Sekunden). Darin enthalten sind die vorhandenen Unix-IPC-
und echten Workjet-IPC-Client-Fälle, Besitz-Fencing, mehrheitliche Autorisierung,
geschützte Checkpoints und gesperrte Übernahme bei unklarem externem Effekt.
Der Beleg liegt unter
`/Volumes/tmp/dev-artifacts/ctox/sync-core-offensive/worker-membership-full-cluster-run.json`.
Die vorhandene Workjet-Client-Fixture wurde anschließend um die neue Abfrage
erweitert: unbekannt vor Aufnahme, aktiv nach Aufnahme, widerrufen nach Revoke,
historischer Aufnahme-Replay bei weiterhin aktuellem Widerruf sowie Ablehnung
bei Quorumverlust und Host-Stopp. Dieselbe Read-Request-ID liefert jeweils den
aktuellen Zustand. Der echte Workjet-Client verwendet dabei die generierten
Schemas und den nativen privaten Unix-Socket. Dieser gezielte Fall besteht
**1/1** (11,10 Sekunden) mit unveränderten bisherigen Ausführungs-/Replay-
Assertions und unveränderter Dreißig-Sekunden-Deadline. Die Fixture wird zur
Laufzeit geladen; ein weiterer Rust-Build war nicht nötig. Beleg:
`/Volumes/tmp/dev-artifacts/ctox/sync-core-offensive/worker-membership-workjet-socket-run.json`.
Die separate echte WebRTC-Abnahme und der produktive Host-Anschluss bleiben
offen. Der
[Host-Vertrag für Worker-Aufnahme](ctox-sync-worker-host-contract.md) beschreibt
Operationen, Endpunkt-Lifecycle, unbekannte Ergebnisse und die noch fehlende
produktive SSH-/QR-Anbindung. `hello` bestätigt nur die lokale Listener-Identität.

Der neue Regressionstest
`delayed_disconnect_must_not_cancel_a_replacement_handshake` hält die
Disconnect-Bereinigung eines anderen Peers an deren Fork-Lifecycle-Await an.
Danach wird für den Zielpeer zuerst die alte Trennung und dann eine
Ersatzverbindung publiziert. Der reguläre Protokoll-/Token-Handshake bestätigt
die Ersatzverbindung, bevor die alte Bereinigung freigegeben wird. Ein
nachgeordneter Disconnect mit beobachteter Task-Beendigung dient als Barriere:
Die abschließende Assertion prüft, ob die neue Zulassung erhalten blieb.
Der Test scheiterte gegen den unveränderten Produktionscode exakt an der
Assertion `old queued disconnect erased the replacement's confirmed admission`
(0/1, 0,03 Sekunden Testlauf nach 20 Minuten 17 Sekunden Build). Die direkte
unveränderte Wiederholung desselben Testbinaries scheitert an derselben
Assertion in 0,29 Sekunden. Die Ersatzverbindung hatte ihren regulären
Handshake vorher erfolgreich abgeschlossen. Der Lifecycle-Fehler ist damit
kausal reproduziert; die frühere Einordnung als bloßer Verdacht ist überholt.
Der Test verlangt ausdrücklich, dass eine Verbindung während der Bereinigung
eines anderen Peers vorankommen kann. Er besteht im unten dokumentierten
Unit-Zwischenstand; die vollständige Abnahme bleibt offen. Der native Adapter gibt
Verbindungshandles aus Routing-ID und Generation aus. Pool-Zulassung,
Handshake-Aufgaben und Transfer-Abbruch verwenden dadurch die konkrete
Verbindung; Policy-Prüfungen behalten ihre separate Identität. Kontrollanfragen
lösen die konfigurierte Route jeweils auf eine aktuell offene Verbindung auf.
Die Ablösung von Fork-Replikationszuständen stoppt den alten Zustand vor dem
Start seines Nachfolgers, damit beide nicht gleichzeitig Checkpoints schreiben.
Der Regressionstest modelliert dieselbe Routing-ID mit zwei Generationen;
zusätzliche Tests prüfen verspätetes Schließen und Token-Schreiben über ein
altes Handle. Dieser Zwischenstand ist ausdrücklich keine Freigabe. Die
SSH-/QR-Produktintegration ist dadurch noch nicht hergestellt.

Der erste vollständige native Test-Build dieses Umbaus hat die Bibliothek
gebaut, scheiterte danach aber im Testprogramm an drei alten
`buffered_bytes(&String)`-Aufrufen im vorhandenen Backpressure-Test. Dieser
verwendet jetzt die aktuelle Verbindungstest-Fixture; High-/Low-Watermark-
Prüfungen und die Zwei-Sekunden-Deadline bleiben erhalten. Anschließend wurde
der native Unit-Testlauf erneut gestartet. Dieser besteht **378/379 Tests**
(14,65 Sekunden nach 9 Minuten 52 Sekunden Build). Die ursprüngliche
Disconnect-Regression, die Tests für alte Verbindungshandles, Fork-Ablösung
und Datei-Transfer-Abbruch bestehen. Der neue Query-Transfer-Test scheitert
an seiner Drei-Sekunden-Deadline; ein isolierter Lauf desselben unveränderten
Binaries scheitert ebenfalls (0/1, 3,40 Sekunden). Ursache im Quellcode:
Ein bereits vom Producer als abgebrochen markierter Frame wartet noch auf
den Datenpuffer, während ein erst beim Senden erkannter Abbruch ihn umgeht.
Beide Fälle schließen jetzt direkt mit einem leeren Abbruch-Frame ab. Ein
zusätzlicher Test prüft dies ohne Zeitfortschritt bereits beim ersten Poll.

Die anschließende Prüfung des nativen Abbaus fand zudem den noch ungebundenen
Timeout-Pfad `wait_for_send_capacity` → `remove_peer_with_error`. Dieser
prüft jetzt beim Warten und Entfernen die konkrete Sendewarteschlange;
eine verspätete alte Aufgabe darf weder den Nachfolger löschen noch dessen
Zustand als Buffer-Stall melden. Der Abbau nach einem erneuten Offer prüft
Generation und weiterhin ungeöffneten Zustand gemeinsam. Der bedingungslose
`remove_peer`-Helfer ist entfernt. Neue Tests decken diese Fälle ab. Diese
anschließenden Änderungen waren zu diesem Zeitpunkt noch nicht funktional abgenommen; der aktuelle vollständige 416er-Lauf bestätigt sie inzwischen.

Die weitere Quellprüfung fand denselben Prüf-/Bereinigungsabstand beim Ende
des primären DataChannels: Nach einer früheren Generationsprüfung konnten
Presence- und Backpressure-Maps bereits dem Nachfolger gehören. Registrierung
und Abschluss verwenden jetzt dieselbe Lifecycle-Sperre wie der Peer-Ersatz.
`finish_data_channel_generation` behandelt OnClose und Stream-Ende gemeinsam;
veraltete Aufgaben verändern den Nachfolger nicht. Nach asynchroner Frame-
Reassemblierung wird die Generation vor lokalen Kontrollzustandsänderungen
unter derselben Sperre erneut geprüft. Ein zusätzlicher Regressionstest hält
Presence und Pufferzustand einer Ersatzverbindung fest. Formatprüfung grün;
Der aktuelle vollständige native Testlauf bestätigt auch diesen Regressionstest.

Die echte WebRTC-Worker-Fixture prüft jetzt auch die aktuelle Mitgliedschaft
über den privaten Unix-Socket und signierte DataChannel-Anfragen: nach Aufnahme,
nach Wiederverbindung mit neuer Signaling-Adresse und nach Widerruf trotz
historischem Aufnahme-Replay. Fremde Abfragen und Abfragen nach Host-Stopp
bleiben abgelehnt. Die bisherigen Assertions und die 60-Sekunden-Gesamtdeadline
bleiben bestehen; diese Erweiterung ist noch nicht ausgeführt.

Die vollständige Suite einschließlich Integrationstests, der Sync-WebRTC-Lauf
und die JS-Wire-Prüfungen bleiben nötig. Der erste Unit-Testlauf ist unter
`/Volumes/tmp/dev-artifacts/ctox/sync-core-offensive/native-generation-units-first-run.json`
gesichert.
Die Wiederholung ist unter
`/Volumes/tmp/dev-artifacts/ctox/sync-core-offensive/delayed-disconnect-red-repeat.json`
gesichert. Die Formatprüfung besteht. Die vollständigen Rust-/JS-Suiten sind
für die anschließenden Korrekturen noch nicht erneut gelaufen. Die ursprüngliche
Disconnect-Regression ist grün; die Query-Korrektur und die zusätzlichen
Timeout-/Offer-Tests sind noch nicht funktional verifiziert.

Kapazitätsstand nach dem letzten Unit-Lauf: Beide Cargo-Läufe und der isolierte
Test sind beendet. Bei nur 2,2 GiB freiem Platz auf `/Volumes/tmp` wurde auf
Abstimmung mit der Workjet-/Speicherkoordination ausschließlich der ruhende
eigene `cargo-target/debug/incremental`-Cache entfernt (1,7 GiB laut `du`).
Tatsächlich frei waren danach 3,1 GiB; Programme und Evidenz blieben erhalten.
Weitere große Builds warten auf ausreichende Reserve für die parallel laufende
Archivprüfung. Die anschließenden Quellkorrekturen sind deshalb noch nicht
erneut gebaut oder getestet.

Nach Abschluss der koordinierten Archivserie und des Android-Buildfensters
wurden zusätzlich ausschließlich eigene lose Objektdateien beendeter nativer
Builds entfernt: 1.280 Objekte für das terminale Unit-Testprogramm und 5.888
Objekte für fünf abgeschlossene RxDB-Bibliotheksvarianten (zusammen 4,194 GiB
logisch). Fertige Bibliotheken, das Unit-Testprogramm und sämtliche Belege
blieben erhalten. Beide Bereinigungsläufe endeten erfolgreich; tatsächlich
frei waren anschließend 6,2 GiB bei gleichzeitig laufendem separatem Harness-
Build. Der begrenzte Authority-Test wurde anschließend im abgestimmten Fenster
fortgesetzt; sein Ergebnis steht am Anfang dieses Dokuments.

Am 6. September wurden bei unbenutztem eigenem Cargo-Lock weitere 45.255
lose Rust-Objekte aus diesem Target entfernt (9,097 GiB logisch). Für jedes
entfernte Objekt blieb das zugehörige fertige Link-Artefakt vorhanden; rlibs,
dylibs und Testprogramme wurden nicht gelöscht. Der Lauf endete mit Exit 0,
anschließend meldete `df` etwa 11 GiB frei. Die vorgemerkten Compilerfenster
bleiben trotz dieser Reserve auf dem langsamen Volume nacheinander geplant.

Die bestehenden Workjet-Anschlusspunkte wurden während des Builds erneut
quellseitig geprüft (noch keine neue Workjet-Implementierung):

Zusätzliche Anschlussprüfung: `apps/server/src/workjet/sync/WorkjetSyncIpc.ts`
enthält bereits den typisierten lokalen IPC-Client. Die aktuelle Suche nach
`requestSyncAuthority` in Workjets `apps/` und `packages/` findet jedoch nur
diese Definition und ihre Tests, keinen produktiven Aufrufer. Auf CTOX-Seite
startet `rxdb_peer.rs` die `NativeSyncSession`, ruft im produktiven
Business-OS-/Service-Pfad aber weder `attach_execution` noch `attach_worker`
auf. Für die Aufnahme sind deshalb sowohl der Host-Lifecycle mit bestätigter
Konfiguration als auch der Workjet-Aufrufer des vorhandenen IPC-Vertrags
anzuschließen; ein zusätzlicher IPC-Client oder eine neue HTTP-Mailbox würde
diese Lücke nicht schließen.

Die vorhandenen Workjet-Prüfungen `WorkjetSyncIpc.test.ts` und
`ctoxSync.test.ts` wurden auf diesem Stand erneut ausgeführt: **9/9 Tests in
2/2 Dateien bestanden** (Vite-Testlauf 29,32 Sekunden). Sie prüfen den lokalen
IPC-Client und die generierten Verträge, nicht den fehlenden produktiven
Aufrufer, eine SSH-/QR-Aufnahme oder den nativen Reconnect.

Plattformgrenze: `native_execution.rs::attach_worker` lehnt Nicht-Unix-Plattformen
derzeit ausdrücklich mit `Unsupported` ab; der gemeinsame `LocalIpcHost` ist ein
Unix-Socket-Host. Die Named-Pipe-Unterstützung des Workjet-IPC-Clients ist deshalb
noch kein Windows-Worker-Nachweis. Native Named-Pipe-Bindung, Zugriffskontrolle,
Stop-/Neustart-Lifecycle und die entsprechenden Aufnahme-/Ausführungstests
gehören weiterhin zur plattformübergreifenden Abnahme.

Ausführungsanschluss: `AuthorityIpc::execute(Create)` setzt den ersten Besitzer
auf den lokalen Knoten; `authority::State` akzeptiert `Create` nur, wenn Besitzer
und authentifizierter Actor übereinstimmen. Das schützt die Selbstübernahme,
ist aber noch kein produktiver Ablauf „Auftrag auf A anlegen, auf B ausführen“.
Dieser braucht einen über CTOX Sync zugestellten, autorisierten Auftrag an den
ausgewählten Worker und dessen bestätigte Annahme. Die vorhandene
`WorkerDispatch.ts` erzeugt lokale Worker-Threads/Worktrees im Environment des
Aufrufers; sie ist keine Aufnahme oder Auswahl eines Netzwerkrechners. Die
Actor-Prüfung darf beim Anschluss nicht durch Vertrauen in ein UI-Hostfeld
umgangen werden.

| Aufnahmeweg | Vorhandener aktiver beziehungsweise vorbereiteter Code | Fehlender Anschluss an diesen Kern |
| --- | --- | --- |
| Desktop-SSH | `apps/desktop/src/ssh/DesktopSshEnvironment.ts::make` delegiert an `packages/ssh/src/tunnel.ts::ensureEnvironment`. Dieser baut den Tunnel auf, stellt optional ein Remote-Pairing-Credential aus und liefert HTTP-/WS-Basis-URLs sowie Ports zurück. | Native Worker-Identität, bestätigte Netzwerkmitgliedschaft und Authority-IPC werden von diesem Rückgabepfad noch nicht aufgebaut. Die vorhandene SSH-Installation/Authentifizierung ist wiederzuverwenden. |
| Managed QR-/Link-Invite | `apps/server/src/ctox/WorkjetManagedDeviceInviteCoordinator.ts` reserviert Bindungen und koordiniert Device-Session- und Sync-Grant-Ausstellung über typisierte Ports. | Für `WorkjetManagedCtoxSyncInviteIssuer` finden sich in `apps/` und `packages/` nur Definition, Coordinator-Verwendung und Test-Bindung; eine produktive Issuer-Implementierung ist dort nicht angeschlossen. Die native Aufnahme muss die bestehende Bindung/Grant-Abwicklung integrieren. |

Die laufende Aufnahme-Diagnose trennt jetzt native Store-Operationen in
Blocking-Pool-Wartezeit, SQLite-Mutex-Wartezeit und Ausführung. Pro internem
Operationsnamen bleiben nur aggregierte Zähler und maximale Phasendauern im
Speicher; weder Request-Inhalte noch eine wachsende Ereignishistorie werden
gespeichert. Der Messbereich bleibt beim tatsächlichen Blocking-Auftrag,
auch wenn dessen asynchroner Aufrufer bereits abgebrochen ist. Der lokale
Authority-Adapter ergänzt Raft-Term, Log-/Apply-Stand, Quorum-ACK-Alter sowie
laufende/abgeschlossene/abgebrochene Write- und Read-Aufrufe. Diese Beobachtungen
sind keine Berechtigungsentscheidung und ändern weder Fristen noch Persistenz.
Der erste Lauf besteht 17/17 Kernprüfungen einschließlich der drei neuen
Diagnose-/Abbruchprüfungen. WebRTC besteht 6/8 in 51,80 Sekunden: beide
Worker-Aufnahmen gelangen, der Reconnect-Fall scheitert anschließend am
Create-Replay und der direkte Drei-Peer-Fall an der Takeover-Bestätigung.
Die neue Store-Diagnose war zunächst nur an Aufnahme-/Widerruffehlern
angebunden; diese späteren Fehler lieferten deshalb noch keine Phasenwerte.
Die Assertion-Ausgaben sind jetzt auch für Create, Reconnect-Validate,
Create-Replay und Takeover ergänzt. Der Folgelauf besteht 7/8 WebRTC-Prüfungen
in 29,84 Sekunden. Der Worker-Reconnect scheitert diesmal vor Validate: alle
drei Kanäle sind offen, aber dem Worker fehlt die reziproke Zulassung zu Voter 3.
Alle Voter melden Leader 1, Term 4 und Log-/Apply-Index 3; der Leader meldet
ein Quorum-ACK-Alter von 4 ms. Keine Store-Operation ist am Fehlerpunkt aktiv.
Die Aufnahmen sind in diesem Lauf bestätigt; diese fehlende Reconnect-Zulassung
ist daher nicht als fehlender Raft-Commit zu klassifizieren.

Die Phasenmessung zeigt trotzdem beträchtliche einzelne Store-Latenzen:
maximal 1.983 ms für `append` und 939 ms für `save_vote` auf Voter 1, während
dessen maximale Blocking-Pool-Wartezeit nur 2,47 ms beträgt. Das sind Maxima
seit Start und kein Nachweis, dass sie den späteren Reconnect-Abbruch verursachten.
Die damalige Quelle zeigte einen separaten Verdacht: Native Transport-Einträge
besaßen Generationen, die Connect-/Disconnect-Subscriber des Replikationspools
arbeiteten jedoch getrennt nur mit Peer-IDs. Dieser Verdacht wurde anschließend
mit der oben beschriebenen Disconnect-Regression reproduziert und korrigiert.
Der WebRTC-Gesamtlauf nach dieser Korrektur steht weiterhin aus.

Die vollständigen terminalen Ergebnisse liegen unter
`/Volumes/tmp/dev-artifacts/ctox/sync-core-offensive/store-diagnostics-first-run.json`
und `store-diagnostics-reconnect-run.json`. Die Formatprüfung besteht;
Clippy für diese Diagnoseänderung besteht über alle Targets mit `--features webrtc --all-targets -- -D warnings` (4 Minuten 41 Sekunden, Exit 0).

Der produktive Signaling-Quellstand wurde erneut geprüft:
`ctox-dev/cloudflare-signaling/src/index.js::roleBoundAuthContext` lässt nur
die native CTOX-Rolle und die ausdrücklich aufgeführten Browser-Rollen zu;
`scripts/assert-signaling-hardening.mjs::assertRuntimeRoleBoundAuthentication`
prüft die Ablehnung von `workjet_executor` mit beiden vorhandenen Credentials.
Für Worker-Onboarding fehlt ein eigener authentifizierter Aufnahmevertrag.
Eine bloße Erweiterung der Rollenliste oder Zuweisung des Browser-Credentials
würde diese Lücke nicht schließen. Die aktuelle Diagnose ändert diesen Guard
nicht und stellt keine produktive Signaling-Freigabe dar.

Erneute Quellprüfung am 6. September: `roleBoundAuthContext` bildet auf `/v2`
den gemeinsamen Namespace aus Browser- und Native-Credential-Hash;
`joinRoom` verwendet diesen Namespace zusammen mit dem Raum. Ein zusätzliches
Worker-Token gehört deshalb ohne expliziten Aufnahmevertrag nicht automatisch
in den vorhandenen Raum. Die künftige Transport-Aufnahme muss Namespace, Raum,
Worker-Identität und Laufzeitberechtigung ausdrücklich binden; sie ersetzt keine
bestätigte Raft-Mitgliedschaft oder Ausführungsautorität. Der bestehende
`node scripts/assert-signaling-hardening.mjs`-Lauf in ctox-dev besteht auf dem
erneut geprüften Quellstand. Vorhandene Role-Binding-/TURN-Änderungen wurden
nicht verändert; kein Signaling-Deploy fand in diesem Schritt statt.

Vor dieser Diagnose bestand der lokale Worker-Reconnect im gezielten echten WebRTC-Test **1/1**:
nach neuer Signaling-Adresse werden alle drei Kontrollverbindungen erneut
aufgebaut, während Identität, IPC-Endpunkt und Auftragsgeneration erhalten
bleiben. Die damaligen Lifecycle-Prüfungen bestanden **7/7**; der parallele
WebRTC-Lauf bestand jedoch nur **5/8**, mit zwei Aufnahmefehlern und einem
weiteren fehlgeschlagenen Drei-Peer-Auftrag. Die Aufnahme-Instabilität bleibt
offen. Clippy für diese vorherige Lifecycle-Änderung bestand über alle Targets mit `--features webrtc --all-targets -- -D warnings`.

Der vorherige Routing-Stand bestand 14/14 Kern-, zweimal 11/11 Authority-,
jeweils zweimal 7/7 Lifecycle-/WebRTC- und 3/3 Workjet-Vertragsprüfungen samt
Clippy. Das bleibt historische Evidenz und ersetzt die aktuelle rote
Gesamtabnahme nicht. Produktive SSH-/QR-Aufnahme, Discovery geänderter
Voter-Adressen und die Workjet-Executor-Anbindung fehlen weiterhin.

### Befunde und Verlauf der Routing-Korrektur

Ein neuer echter WebRTC-Test prüft jetzt den Reconnect eines bereits aufgenommenen
Workers: Signaling-Verbindung schließen, unter einer neuen Signaling-Adresse
wieder beitreten, zusätzlich sämtliche bisherigen Worker-DataChannels schließen
und über denselben lokalen IPC-Endpunkt denselben Auftrag mit unveränderter
Besitzergeneration autorisieren. Der erste Lauf scheiterte bereits an der
Worker-Aufnahme und war deshalb kein Reconnect-Nachweis. Die unveränderte
Wiederholung erreichte den Adresswechsel und scheiterte daran, dass kein neuer
P2P-Kanal entstand (27,00 Sekunden Testlauf). Der Host-Abbruch bei Änderung
der eigenen Signaling-Adresse ist jetzt entfernt; die Initiator-Entscheidung
liest jeweils die aktuelle Adresse. Schlüssel, Mitgliedschaft und IPC-Endpunkt
werden nicht ersetzt. Der erste Lauf gegen die Korrektur scheiterte erneut
bereits bei der Aufnahme. Die unveränderte Wiederholung bestand anschließend
den Reconnect samt Auftragserhalt, Replay-Schutz, Datensperre, Widerruf und
Shutdown (1/1, 24,57 Sekunden). Der Test ist jetzt um den nachgewiesenen neuen
Verbindungsaufbau und die erneute Protokollzulassung zu allen drei Votern
erweitert; Aufnahmefehler erhalten dieselbe Transportdiagnose wie Widerruffehler.
Der anschließende Gesamtlauf besteht Lifecycle 7/7, WebRTC jedoch nur 5/8
(19,57 Sekunden). Beide Worker-Fälle scheitern erneut schon beim Aufnahmeauftrag;
der direkte Drei-Peer-Test scheitert ebenfalls an einer fehlenden Bestätigung.
Die neue Diagnose zeigt für die Worker-Fixtures alle Voter mit Leader 1,
gegenseitig zugelassene Kontrollkanäle, drei offene DataChannels pro Peer und
keine ausstehenden Sendewarteschlangen oder ACKs am Fehlerpunkt. Ein bloßer
Sendestau ist damit nicht belegt; die Aufnahme-Instabilität bleibt offen.
Der erweiterte Drei-Kanal-Reconnect besteht im gezielten Lauf (1/1,
27,54 Sekunden): neue Kanäle und Protokollzulassung zu sämtlichen drei Votern,
gleicher IPC-Endpunkt, unveränderte Besitzer-ID/Generation sowie anschließender
Replay-Schutz, Datensperre und Widerruf. Diese Einzelabnahme ersetzt den roten
parallelen Gesamtlauf nicht. Clippy für diese Lifecycle-Änderung besteht über alle Targets mit `--features webrtc --all-targets -- -D warnings`.
Ein neues Identitätsprotokoll ist dafür
nicht nötig: `SignedTransport` bindet jeden Austausch bereits an Schlüssel,
Scope, Empfänger und frische Nonce. Die geänderte Adresse eines Voters wird
damit noch nicht automatisch gefunden; dessen Discovery bleibt offen.

Bei der Abstimmung mit der Workjet-Desktop-/Android-Abnahme wurde außerdem eine
separate Root-Lücke im aktuellen Code bestätigt: `validated_workspace_root_override`
ignoriert `CTOX_ROOT`, wenn `looks_like_ctox_root` den Pfad nicht als vollständigen
Source-/Bundle-Root erkennt. Ein isolierter Source-Root braucht neben `src/core/main.rs`
auch `Cargo.toml` und `contracts/history/creation-ledger.md`. Zugleich verwenden
`native_peer_lock_path` und `native_peer_heartbeat_path` noch `root/runtime`
statt des von den Datenbanken verwendeten `CTOX_STATE_ROOT`. Diese inkonsistente
Zuständigkeit ist nicht behoben. Für eine sichere Fixture müssen bis dahin der
erkannte Root und sein Runtime-Verzeichnis zusammen unter `/Volumes/tmp` liegen;
die tatsächlichen Statuspfade müssen vor dem Start geprüft werden. Die
Desktop-/Android-Abnahme hat diese Fixture-Anordnung inzwischen auch mit dem
installierten `ctox-real 0.3.22` anhand der Statuspfade bestätigt.

Die nächste Korrektur ersetzt jetzt die getrennten Routing-Pfade von
Authority-Voter und nicht stimmberechtigtem Worker. Der alte Worker probierte
jeden Voter genau einmal und gab nur dessen letzten Fehler zurück. Beim
Wahlübergang konnte er damit vor Ablauf seiner fünfsekündigen Frist abbrechen.
Voter konnten bereits beim vorübergehend fehlenden lokalen Leader-Hinweis
abbrechen. Beide verwenden nun dieselbe begrenzte Wiederherstellung: identische
Anfrage und Request-ID, ausschließlich gepinnte Voter, begrenzter Backoff nach
einer erfolglosen Runde, sofortige Rückgabe bestätigter Ergebnisse und
typisierter Ablehnungen. Ein Replay bleibt ausdrücklich ein Replay. Shutdown
verwirft auch eine währenddessen eintreffende Bestätigung.

Der Kontrollvertrag ist dafür auf Version 4 erhöht. `AuthorityFailure` wird
aus derselben Fixture für Rust und TypeScript generiert und trennt
Leader-Hinweis, vorübergehende Nichterreichbarkeit und Ablehnung. Unbekannte
Leader-IDs erweitern die Vertrauensgruppe nicht. Alte String-Antworten werden
nicht still interpretiert. Die fünf generierten Verträge sind reproduzierbar;
Workjets gezielte Vertragsprüfung besteht 3/3 Tests. Die sieben neuen
Rust-Routing-/Vertragstests bestehen ebenfalls (virtuelle Uhr, 0,00 Sekunden
Testlauf; 10 Minuten 29 Sekunden Build inklusive Tokio-Abhängigkeiten).
Der anschließende vollständige Lauf dieser betroffenen Suiten besteht 14/14
Kern-, 7/7 Lifecycle- und 7/7 WebRTC-Prüfungen. Die parallele Authority-Suite
scheitert jedoch in vier von elf Fällen (35,78 Sekunden): Worker-Aufnahme,
IPC-Effect-Replay, nicht stimmberechtigter Worker und vollständiger Peer-Neustart.
Mehrere Anfragen schöpfen die gesamte Frist aus; die verbleibende Instabilität
ist damit nicht allein durch den früheren vorzeitigen Routing-Abbruch erklärt.
Die unveränderte serielle Diagnose besteht 11/11 Tests in 53,26 Sekunden.
Sie ersetzt die ausstehende parallele Abnahme nicht. Die Fixture zeichnet
bei Fehlern jetzt zusätzlich RPC-Art, Leader-Stände, konfigurierte Raft-Zeiten
und betroffene Request-ID auf. Der parallele Diagnoselauf besteht 10/11 Tests;
der direkte signierte Validate-RPC an den zuvor gespeicherten Leader scheitert.
Die Diagnose zeigt bei allen drei Peers `None` als Leader, Append-Abbrüche nach
50–53 ms und Vote-Abbrüche nach 153 ms bei einer Fixture-Konfiguration von
50 ms Heartbeat und 150–300 ms Wahlfrist. Die erfolgreiche Propose-Antwort
benötigte in diesem Lauf 1.328 ms. Das ist konkrete Evidenz für einen
Wahlübergang unter den impliziten Fixture-Zeiten, keine Verletzung der
Berechtigungsprüfung. Die Fixture verwendet jetzt denselben bereits bestehenden
`AuthorityTiming`-Vertrag wie die nativen Hosts (250 ms, 1.500–3.000 ms),
statt OpenRafts implizite Standardzeiten. Die fünfsekündige Ausführungsfrist,
die Quorum-Anforderungen und die Sicherheitsassertions bleiben unverändert.
Die beiden parallelen Läufe mit diesem Host-Vertrag bestehen jeweils 11/11 Tests
in 20,17 beziehungsweise 20,37 Sekunden. Beim zweiten Lauf bestehen zusätzlich
Lifecycle 7/7 (0,72 Sekunden) und WebRTC 7/7 (17,64 Sekunden), nach bereits
7/7 und 7/7 gegen denselben Produktionscode im ersten kombinierten Lauf.
Clippy für die neue Routing-Änderung besteht über alle Targets mit
`-D warnings`. Der einzige Befund war die komplexe Tupel-Signatur der neuen
Testdiagnose; sie verwendet jetzt einen benannten Typ, ohne Lint-Ausnahme.
Die folgenden Transportergebnisse sind
historischer Ausgangsstand und keine umfassende Abnahme des neuen Routings.

Der aktuelle native RxDB-Stand besteht **408/408 Tests**. Nach der Queue-Korrektur
bestehen Native-Lifecycle und paralleles WebRTC jeweils **zweimal 7/7 Tests**.
Die Authority-Suite besteht in zwei parallelen Läufen jeweils **10/11**: zuerst
scheiterte der vollständige Peer-Neustart, danach die erneute Autorisierung des
nicht stimmberechtigten Workers nach einem Leader-Ausfall. Der Neustart-Test
bestand isoliert unverändert. Die Ausführungsautorität ist damit noch nicht
vollständig abgenommen. Der frühere serielle Lauf mit 24/25 ist lediglich
historische Evidenz; die aktuellen WebRTC-Fristen wurden nicht verändert.

Ein paralleler WebRTC-Lauf bestand nur **1 von 7 Prüfungen**. Der danach
vollständig erfasste Profiling-Lauf bestand **6/7** in 43,16 Sekunden; dort
scheiterte die Quorumprüfung nach der Übernahme im direkten Drei-Peer-Test.
Dieser Test verwendet noch eine eigene 150-ms-Heartbeat-Konfiguration, die
Host-Tests den gemeinsamen Timing-Vertrag. Beide Läufe zeigen Instabilität;
24/25 aus dem seriellen Lauf ist keine aktuelle Gesamtabnahme.

Das Profil liegt unter
`/Volumes/tmp/dev-artifacts/ctox/sync-core-offensive/profile-20260905-151832/`
zusammen mit Testausgabe und Exit-Code 101. Es beweist keinen einzelnen
Engpass. Offene DataChannels und bereite Kontrollrouten genügen noch nicht für
einen stabilen Authority-Betrieb. Warteschlangenalter von rund einer Sekunde
wurden im vorherigen Lauf beobachtet; die Ursache aller Fehler bleibt offen.

Bei der Codeprüfung wurde eine konkrete Kopplung im nativen Sender gefunden:
Der erste Aufruf wartete auf die gesamte Queue statt auf sein eigenes Ergebnis.
Sein Abbruch gab den Drain-Slot frei, weckte aber bereits wartende Aufrufe nicht.
Die laufende Korrektur lässt jeden Sender sein eigenes Ergebnis abwarten und
übergibt einen freigegebenen Slot an bestehende Wartende. Ein Yield nach jedem
Sendeergebnis verhindert, dass die Rückgabe eine fremde Folgetransfer-Ausführung
abbricht. Vier neue Regressionstests prüfen Ergebnisrückgabe, aktive Übergabe
sowie verspätete Freigabe und wartende Sender bei Ersatz einer Peer-Queue.
Der vorhandene First-Poll-Abbruchtest bleibt erhalten. Der gezielte native
Lauf besteht **47/47 Transporttests** (0,19 Sekunden; vorher 50 Minuten Build).
Der zusätzliche Test für einen bereits laufenden alten Drainer reproduzierte
anschließend den Verlust eines Pakets aus der Ersatz-Queue (0 statt 1 verbleibende
Frames). Die Entnahme prüft jetzt ebenfalls die konkrete Queue-Identität, auch
bei priorisierten Frames während einer Chunk-Übertragung. Der erweiterte Test
prüft beide Entnahmepfade. Die vollständige native RxDB-Suite besteht danach
**408/408 Tests** (375 Unit-, 31 Konformitäts-, ein Fehlervertrags- und ein
Idle-Budget-Test); auch die Formatprüfung ist grün. Gegen diesen Stand bestehen
**7/7 Native-Lifecycle- und 7/7 parallele WebRTC-Tests**. Der zuvor gestartete
Authority-Lauf bestand 10/11: `state_and_receipts_survive_all_peer_restarts`
scheiterte mit „execution paused: no authority leader“. Dieser In-Memory-Test
verwendet OpenRafts Standardzeiten statt der nativen Host-Konfiguration und
läuft nicht über die geänderte Queue. Der unveränderte fokussierte
Wiederholungslauf mit Backtrace bestand (1/1, 5,25 Sekunden); die Ursache des
parallelen Fehlers bleibt offen. Die WebRTC-Fristen wurden nicht geändert. Ein grüner WebRTC-Lauf ist noch kein wiederholter Stabilitätsnachweis.
Die Korrektur ist keine bereits bestätigte Ursache oder Behebung aller
parallelen Authority-Fehler.

Die vollständige JS-Suite endete mit **110 bestanden, 4 fehlgeschlagen, 0
übersprungen**. Die Fehler wurden einzeln untersucht:

- Im Command-Aufruferinventar fehlten die bestehenden Dispatch-Aufrufer File
  Viewer und Importer sowie der Projektionsleser des File Viewers. Diese
  nachgewiesenen Aufrufer sind ergänzt; der unveränderte Guard ist grün.
- Der Command-Typ-Generator nahm lokale `installed-modules/` in das versionierte
  Inventar auf. Ein isolierter Test reproduzierte die Verunreinigung mit einem
  Tenant-Command. Der Generator verwendet jetzt dieselbe Quell-/Laufzeitgrenze
  wie das Aufruferinventar; der neue Test ist an die reguläre Prüfung angeschlossen.
  Das erzeugte JSON entspricht wieder dem bisherigen kanonischen Bestand.
- Der Kundenkennungs-Guard findet sechs Diagnosekommentare in fünf Dateien mit
  paralleler beziehungsweise bestehender Arbeit. Diese Implementierungen bleiben
  unangetastet; der Guard ist weiterhin offen.
- Der Multi-Tab-Test erreichte im Einzeltest sämtliche fachlichen Assertions und
  endete mit Exit 0, benötigte inklusive Schließen aber mehr als fünf Minuten.
  Die reguläre Suite bricht nach 180 Sekunden ab. Der nachfolgende Diagnoselauf
  mit `DEBUG=pw:browser` endete unverändert mit Exit 0: vom Browserstart bis zum
  vollständigen Schließen rund 12 Sekunden, davon 106 ms Profilbereinigung.
  Das bestätigt intermittierendes Verhalten, belegt aber noch nicht die Ursache
  des langsamen Laufs. Die Zeitgrenzen und der Browser-Lifecycle bleiben unverändert.

Die JS-/Rust-Wire-Tests und ihr Runner verwenden jetzt einen gemeinsamen
Binär-Resolver. Mit `--wire-daemon PATH` wird ein konkreter Build gebunden;
ein fehlender expliziter Pfad fällt nicht auf einen anderen Build zurück.
Der Auswahltest ist grün. Beide Cross-Process-Tests bestehen gegen den gerade
gebauten Daemon im isolierten Cargo-Ziel: 5.000 Dokumente in 26 Wire-Chunks sowie
102 Datei-Chunks mit insgesamt 800 KB. Diese Fixtures ersetzen keine
Command-Performance- oder WAN-Abnahme.

Die aktuelle Clippy-Abnahme des Sync-Kerns umfasst dessen Timing-, Routing-
und Diagnoseänderungen. Das Endergebnis des separaten früheren RxDB-Clippy-Laufs
konnte nach Verlust seines Prozesshandles nicht wiederhergestellt werden;
dieser separate Lauf wird nicht als bestanden gewertet.

Für Benutzer gibt es weiterhin **keinen vollständig abgenommenen SSH- oder
QR/Link-Aufnahmeablauf in Workjet Desktop und Mobile**. Vorhanden sind native
Mitgliedschaft, der nicht stimmberechtigte Worker-Client und ein gemeinsamer
IPC-/Transport-Lifecycle. Produktive Aufnahme, authentifizierter Wiederbeitritt
mit geänderten Signaling-Adressen, tatsächliche Executor-Anbindung und eine
plattformübergreifende Ende-zu-Ende-Prüfung fehlen. Die Offensive befindet sich
überwiegend in Etappe 2; spätere Etappen enthalten erst einzelne Grundlagen.

### Laufende Korrektur der Authority-Zeiten

Der native Host verwendete implizit OpenRafts Standardkonfiguration. Im lokal
installierten OpenRaft 0.9.25 nutzt `check_is_leader` in `core/raft_core.rs`
`heartbeat_interval` auch als vollständige RPC-Frist für die Quorumprüfung.
Der Standard beträgt 50 ms, die Standard-Wahlfrist 150–300 ms. Die äußere
Fünf-Sekunden-Frist in `local_validate` verlängert diese innere Frist nicht.
Damit erklärt die Konfiguration, warum bereits kleine lokale Verzögerungen
Bestätigungen verhindern können; sie beweist noch nicht die Ursache jedes
beobachteten Fehlers.

`ExecutionGroupOptions` verlangt jetzt eine typisierte `AuthorityTiming` aus
der Host-Konfiguration. Der gemeinsame Standard ist 250 ms für Heartbeat und
Quorum-RPC sowie 1.500–3.000 ms für die Wahl. Diese Fristen lösen keine künstliche
Wartezeit auf dem erfolgreichen Pfad aus. Sie ändern die Fehlererkennung und
müssen deshalb mit Failover-Latenz getrennt gemessen werden. Das ist keine
WAN-Abnahme und keine Aufweichung der Command-Performance-Grenze.
Ein neuer Test prüft signierte RPCs mit 80 ms zusätzlicher Verzögerung,
anschließende Leader-Isolation und erfolgreiche Wahl durch die übrigen Stimmen.
Der erste neue Verzögerungstest ist fehlgeschlagen: Quorumprüfung ohne fremde
Bestätigung, trotz der 250-ms-Konfiguration. Build: 16 Minuten 45 Sekunden,
Test: 5,22 Sekunden. Die unveränderte Wiederholung bestand die ersten
Quorumprüfungen, scheiterte anschließend aber an einem Fehler im neuen Test:
Der Ersatz-Leader sollte den fremden Executor autorisieren. Der Kern lehnte
das korrekt ab. Der Test verwendet nun drei ausführungsfähige Stimmen, lässt
den jeweils betroffenen Leader seinen eigenen Auftrag autorisieren und prüft
nach der Partition einen neuen Auftrag der überlebenden Mehrheit. Zusätzlich
zeichnen die Test-RPCs begrenzt Laufzeit und letzte Phase auf, auch bei Abbruch.
Die Authority-, Native-Lifecycle- und WebRTC-Suiten werden jetzt mit einem
Buildjob und nacheinander laufenden Testfällen geprüft. Ihre interne
Nebenläufigkeit bleibt bestehen; parallele Testcluster und WAN-Performance
sind damit nicht abgenommen. Die lokale Systemlast lag zuletzt über 200.
Das erklärt weder pauschal den ersten Fehler noch erlaubt es eine grüne Abnahme.

### Bestätigte Lücke im produktiven Signaling-Vertrag

Der lokale ctox-dev-Quellstand enthält in
`cloudflare-signaling/src/index.js:33` keine Rolle `workjet_executor`.
Die bisherige Ableitung unbekannter oder fehlender Rollen aus dem Clientnamen
ist jetzt gelöscht. Die Browser-Credential-Rollen sind ausdrücklich aufgeführt;
eine zusätzliche Rolle erhält nicht automatisch dieses Credential.
`scripts/assert-signaling-hardening.mjs` wurde um explizite und fehlende Rollen,
Credential-Verwechslungen aller bestehenden Rollen sowie die Ablehnung von
`workjet_executor` im bisherigen Authentifizierungsvertrag erweitert. Der
unveränderte alte Code fiel dabei durch; nach der Korrektur ist der erweiterte
Guard grün. Das prüft die sichere Ablehnung, noch keine erfolgreiche Aufnahme.
Die native Workjet-Rolle aus dem neuen Rust-Vertrag ist daher noch nicht über diesen realen
Zulassungsweg integriert. Das Vier-Peer-Signaling-Fixture bildet diese Grenze
nicht ab; seine erfolgreiche Verbindung wäre kein Beleg für den Produktpfad.

Die Ablösung muss Worker-Identität, bestätigte Mitgliedschaft und
Signaling-Berechtigung gemeinsam binden. Eine bloße Ergänzung der Rollenliste
hätte zuvor den Browser-Credential-Zweig übernommen. Jetzt wird eine Rolle ohne
ausdrückliche Credential-Zuordnung abgewiesen; der Aufnahmevertrag muss diese
Bindung erst herstellen. Die Korrektur liegt im lokalen ctox-dev-Quellstand und ist noch nicht
ausgeliefert. Die eigentliche Worker-Aufnahme bleibt offen. Die SSH-Provisionierung in Workjet installiert derzeit eine
GUI-App; der vorhandene Headless-Einstieg `apps/server/src/cli/server.ts`
(`serveCommand`) und dessen Server-Lifecycle sind noch nicht an die native
Authority angebunden.

## Verbindliche Architekturentscheidungen

CTOX Sync verbindet Entwicklungs- und Produktionsoberflächen. RxDB/WebRTC besitzt
die Datenreplikation, aber seine Master/Fork-Wahl entscheidet nicht über die
Ausführungsberechtigung eines Coding-Agenten. Ein eigener Kontrollvertrag über
dieselbe Transportinfrastruktur entscheidet über Mitgliedschaft, Besitzer und
Generation. OpenRaft ist exakt auf **0.9.25** gepinnt und bleibt hinter dem Adapter
in `src/core/sync/src/authority/`.

Eine Gruppe besteht aus drei eigenen stimmberechtigten Peers; mindestens zwei
sind zugleich geeignete Ausführungs- und Datenrechner. Ohne Mehrheit gibt es keine
neue bestätigte Ausführungsberechtigung. Eine Koordinationsstimme bestätigt keine
Dateikopie. Vollständige Session-Portabilität umfasst Journal, Anhänge,
Arbeitsdateien und tatsächlich wiederherstellbaren Harness-Zustand. Codex und
Claude müssen dafür mit echten Export-/Import-/Resume-Abläufen zertifiziert werden.
Die automatische Übernahme externer CTOX-Produktionsdienste ist nicht Teil dieser
Offensive.

Jede fachliche Tatsache erhält einen Besitzer. Lokale Ausführungsjournale,
Projektionen und Caches dürfen bleiben; mehrere fachliche Writer nicht. Native
Laufzeiten und Clients werden gemeinsam aktualisiert. Bestehende Daten bleiben
erhalten; alte Stores dienen nach Umschaltung ausschließlich als unveränderliche
Sicherung. Ein Rückwechsel nach neuen Schreibvorgängen benötigt eine geprüfte
Wiederherstellung.

## Ausgangsstand und laufende fremde Änderungen

Die lokale Baseline liegt unter
`runtime/sync-core-offensive/baseline-20260904T202838Z/`: drei Git-Patches, Status
inklusive unversionierter Pfade sowie ein Manifest mit HEADs und SHA-256 der
veränderten versionierten Dateien. CTOX-HEAD war
`310fdd679f234e6f975656db6a8416616e2b7a55`. Die Patches sind keine Sicherung des
Inhalts unversionierter Dateien. Es wurden keine fremden Änderungen zurückgesetzt.
Die drei Checkouts enthielten bereits 71 / 190 / 80 veränderte versionierte
Dateien (CTOX / Workjet / ctox-dev). Diese Baseline ist lokal; sie wird wegen
fremder Arbeitsinhalte nicht als öffentlicher Patch eingecheckt.

Erneut ausgeführte Baseline-Prüfungen:

| Prüfung | Ergebnis vor der Umstellung |
|---|---|
| RxDB-JS `run-all.mjs` | 106 bestanden, 4 fehlgeschlagen, 2 übersprungen |
| Nativer RxDB-Testlauf | 365 bestanden, 1 fehlgeschlagen |
| Nativer einzelner Wiederholungstest | `create_database_and_add_collection` bestanden; paralleler globaler Zähler ist ein offener Befund |
| Workjet CodexSessionRuntime, OwnershipStore, HandoffSnapshot, CtoxBusinessOsShell | 4 Dateien / 49 Tests bestanden |
| Shell-V2-Vertragswächter | 23/37 Module konform; 14 mit Body-Overlays |

Die vier JS-Befunde waren `command-consumer-inventory`, `command-type-inventory`,
`customer-identifier-inventory` und unterschiedliche RxDB-Bundle-Import-URLs.
Die übersprungenen Wire-Prüfungen benötigten den noch nicht gebauten Wire-Daemon.
Historische Performance-Ergebnisse werden nicht als neue Abnahme übernommen.

Nach den bisherigen Änderungen erneut geprüft: nativer RxDB-Lauf vollständig
grün (368 Unit-Tests, 31 Conformance-Tests, Error-Guard und Idle-Budget-Test).
Der frühere globale Zählerbefund ist dabei nicht erneut aufgetreten; sein Test
wurde nicht geändert. Die vollständige JS-Suite hat jetzt **110 bestanden,
3 fehlgeschlagen, 0 übersprungen**. Die Bundle-Import-Abweichung ist behoben;
die beiden Cross-Process-Prüfungen liefen diesmal mit vorhandenem Wire-Daemon.
Die drei übrigen, bereits in der Baseline roten Befunde wurden konkretisiert:

- Consumer-Inventar: `modules/file-viewer/index.js` und
  `modules/importer/index.js` fehlen in der erwarteten Liste.
- Command-Typ-Inventar: generierte `business_command_inventory.json` weicht
  vom aktuellen Code ab.
- Stillgelegte Kundenidentität: aktive Referenzen in `store.rs`, `rxdb_peer.rs`,
  `shared/sync.js`, `modules/browser/index.js` und `install/mod.rs`.

Diese Befunde sind nicht durch Aufweichen der Inventarprüfungen freigegeben.


## Zuständigkeiten und Umstellungsstand

| Tatsache / Mechanismus | Verbindlicher Besitzer | Stand |
|---|---|---|
| Instanz, Gerät, bestätigte Mitgliedschaft | nativer Sync-Kern, bestätigtes Peer-Verzeichnis | drei feste Stimmen; zusätzliche Worker-Aufnahme/Widerruf im Raft-/IPC-Code; Prüfung und produktive Provisionierung offen |
| Business-Records, Replikationscheckpoints, Dateitransfer | vorhandener RxDB-/WebRTC-Datenpfad | aktiv; nicht durch Raft ersetzt |
| Auftragsbesitzer und Generation | bestätigte native Raft-State-Machine | implementiert und isoliert getestet; aktive Executor-Anbindung offen |
| Externe Wirkung mit unklarem Ergebnis | dauerhafte Wirkungs-ID und explizite Abklärung | Journalzustände vorhanden; Gateway-/Tool-Integration offen |
| Vollständiger Session-Checkpoint | unveränderlicher Manifest-/Blob-Store plus bestätigte Datenkopien | Store/Restore und signierte dauerhafte Kopierquittungen vorhanden; produktiver Transfer und Harness-Zertifizierung offen |
| Geladene Shell/App/Schema/Native-Kombination | gemeinsamer Runtime-Vertrag | generierte Datenstruktur vorhanden; Resolver und Hosts noch nicht umgestellt |
| Workjet-Journal und UI-Zustand | Journal/Projektion der oben genannten Fakten | bestehende Writer noch aktiv; Migration offen |

## Implementierter Kern und seine Grenzen

- `native.rs` übernimmt jetzt den produktiven Business-OS-Transportstart im
  Quellcode. Der bisherige lokale Spawn-/Timeout-Pfad in `rxdb_peer.rs` ist
  entfernt. Business OS liefert Collections und sämtliche bisherigen
  Berechtigungsprüfungen; der Kern besitzt Signaling, Connection-Handler und
  Replikationspool. Datenbank, Heartbeat, Projektionen und Commands bleiben beim
  Host. Der Lifecycle übernimmt die Signaling-Verbindung bereits vor dem Warten
  auf Raumfreigabe und räumt sie bei Fehler, Deadline oder Abbruch auf.
  Signaling-Empfänger und der vollständige Pool werden jetzt vor dem Raumbeitritt
  installiert. Ein voller Kern-Lauf hatte einen Peer ohne Log und Leader gezeigt;
  bei der Untersuchung waren Empfänger erst nach Join und Pool-Subscriber erst
  nach Handler-Start vorhanden. Der zusätzliche Test sendet unmittelbar beim
  Join eine erste SDP-Nachricht und prüft, dass sie den vorbereiteten Handler
  erreicht. Der neu ausgeführte vollständige Lauf besteht mit dieser Reihenfolge.
  Die produktive Authority-Gruppe und Workjet-Ausführung sind damit noch nicht angeschlossen;
  es wurde keine neue Laufzeit ausgeliefert.
- `src/core/sync/` enthält OpenRaft-Adapter, SQLite-Log und State-Machine mit
  FULL-synchronen Commits, persistierten Votes, Receipts und atomarer
  Snapshot-Installation. Die OpenRaft-Storage-Konformitätsprüfung läuft gegen
  diese Implementierung. Der neue Crate wird separat gebaut, wie `src/core/rxdb`.
- Eigentumsübernahme prüft Generation, geschützten Checkpoint, geeignete
  Datenrechner und offene externe Wirkungen. Eine lokale Projektion allein ist
  keine Autorisierung; deren Prüfung erfordert einen linearisierbaren Quorum-Read.
  Diese Prüfung ersetzt noch keine Sperre am tatsächlichen Gateway/Tool.
- Checkpoint-Schutz akzeptiert ausschließlich signierte Quittungen von mindestens
  zwei verschiedenen konfigurierten Daten-Peers. Auftrag, Session, Modellroute,
  Account-Referenz, Harness-Version, Generation, Manifest-Hash und Sequenz müssen
  zusammenpassen. Die Kopierquittung entsteht erst nach vollständiger Prüfung und
  Flush der lokalen Dateien und Verzeichniseinträge. Koordinationsstimmen,
  doppelte Kopien und vom Aufrufer behauptete Replica-Listen zählen nicht.
  Quittungen bleiben im persistenten Checkpoint erhalten. Der Kontrollvertrag ist
  seitdem ausdrücklich Version 2; die zusätzliche Worker-Mitgliedschaft hebt
  den aktuellen Kontrollvertrag auf Version 3. Ältere Kontrollversionen werden
  abgewiesen. Alte experimentelle
  Authority-Stores mit unsignierten Checkpoints brauchen eine geprüfte Migration;
  es gibt keine stille Aufwertung alter Replica-Listen zu Schutzbelegen.
  Unter Windows werden dauerhafte Quittungen bis zur Implementierung und Prüfung
  des Verzeichnis-Flush ausdrücklich abgelehnt.
- Kontrollnachrichten und Antworten sind mit Ed25519 signiert und an Instanz,
  Empfänger und Anfrage-Nonce gebunden. Signierte alte Antworten dürfen keine
  frische Anfrage bestätigen. Signaling-Adressen sind nur Routinghinweise.
- Workjets vorhandene Mesh-Identität speichert Ed25519 als Node/OpenSSL-PKCS#8 v1
  unter `workjet-mesh-ed25519-private-key` im ServerSecretStore. Der native Import
  akzeptiert dieses Format jetzt mit obligatorischem Abgleich gegen die
  bestätigte öffentliche Identität. Ein echter Node→Rust-Interop-Test prüft
  Identitätserhalt und Ablehnung falscher Schlüssel. Ein neuer Schlüssel ist für
  die Anbindung damit nicht nötig; produktive Provisionierung und die Ableitung
  des Scope aus der bestätigten CTOX-Raumzuordnung bleiben offen.
- `authority/webrtc.rs` bindet diesen Vertrag an die vorhandenen
  `WebRTCMessage`-/Auxiliary-Handler. Kein HTTP-Datenpfad wurde hinzugefügt.
  `NativeSyncSession::attach_execution` besitzt jetzt genau eine konfigurierte
  Authority-Gruppe: Raum und lokaler Schlüssel werden geprüft, der Receiver
  wird im tatsächlichen Pool registriert und ausschließlich konfigurierte native
  Peers werden verbunden. Shutdown beendet die Gruppe vor dem Transport.
  Allgemeine Browser-Erkennung und Host-Berechtigungsprüfungen bleiben getrennt.
  Kontrollnachrichten werden erst nach Aufnahmeprüfung und abgeschlossenem
  eigenen Protokoll-/Token-Handshake gesendet. Ein empfangener Peer-Probe allein
  genügt nicht. Der erste integrierte Lauf legte hier einen Start-Race offen;
  die zusätzliche Prüfung des abgeschlossenen eigenen Handshakes behebt ihn.
  Der Drei-Peer-Test für Auftrag, Partition, Übernahme und Rückkehr benutzt jetzt
  den tatsächlichen Pool und dessen Auxiliary-Dispatcher; der direkte
  Test-Receiver ist entfernt. Ein zusätzlicher Test startet drei vollständige
  Native-Session-/Authority-Lifecycles ohne manuelle Peer-Verbindung, bestätigt
  Auftragsbesitz und prüft die Ablehnung eines Business-Record-Reads über denselben
  Kanal. Beide verwenden weiterhin einen isolierten Signaling-Server mit
  Test-Aufnahmeprädikaten. Produktive Provisionierung, Business-OS-Geräteprüfung,
  Workjet-Executor-Anbindung und WAN-Abnahme bleiben offen.

- Wiederholte bestätigte Commands liefern ausdrücklich `Replayed`; sie sind
  keine neue Erlaubnis für eine externe Wirkung. Eine verlorene Antwort auf
  `BeginEffect` darf damit keinen zweiten Dispatch auslösen.
- Der lokale Checkpoint-Store prüft Blob- und Manifest-Hashes, Dateilängen,
  portable Pfade und Symlink-Ziele. Restore erfolgt in ein neues Verzeichnis,
  überschreibt kein existierendes Arbeitsverzeichnis und stoppt bei offenen
  externen Wirkungen. Die bisherigen Roundtrip-Fixtures sind synthetisch;
  sie zertifizieren weder Codex noch Claude.
- Gemeinsame Rust-/TypeScript-Strukturen werden aus
  `src/core/rxdb/tests/fixtures/ctox_execution_contract.json` generiert.
  Der Generator erzeugt außerdem Effect-Schemas zur Laufzeitvalidierung.
  `src/core/sync/src/ipc.rs` verarbeitet gerahmte lokale Streams mit Größenlimit,
  Fristen und aus der nativen Identität abgeleitetem Actor. Workjets
  `WorkjetSyncIpc.ts` prüft echte Unix-Socket-/Named-Pipe-Antworten, Schema,
  Protokoll und Anfrage-ID und beendet abgebrochene Verbindungen.
  `local_host.rs` bindet den Unix-Endpunkt in einem privaten Verzeichnis, hält
  einen exklusiven Prozess-Lock und akzeptiert ausschließlich Clients derselben
  lokalen Benutzeridentität. Maximal 32 Verbindungen bleiben aktiv. Ein zweiter
  Start darf den ersten Listener nicht ersetzen; ein vorhandener Socket wird
  nur nach explizit gescheiterter Verbindungsprobe als verwaist entfernt. Shutdown
  beendet alle Verbindungen; verspätetes Aufräumen prüft den Socket-Inode.
  Der native/Workjet-Vertragstest lädt den tatsächlichen TypeScript-Client aus dem
  Workjet-Checkout und prüft Quorumverlust und Host-Abschaltung über diesen Host.
  Der Anschluss an den produktiven Peer-Lifecycle, ProviderService, Gateway und
  Tool-Freigaben sowie die Windows-Listener-Implementierung fehlen weiterhin.
- `NativeExecutionGroup` besitzt jetzt auch den lokalen IPC-Listener. Ein
  gemeinsamer Supervisor überwacht Listener und Peer-Erkennung, beendet bei
  deren Ausfall die Raft-Autorität und schließt den Endpunkt. Der Host erhält
  den tatsächlichen Pfad über `ipc_endpoint()` und kann `wait_stopped()` zur
  Fehleranzeige beobachten. Ein belegter Endpunkt darf weder übernommen noch
  überschrieben werden; der fehlgeschlagene neue Attach schließt seine eigene
  native Session. Auch Drop der besitzenden Session beendet Autorität und IPC,
  obwohl andere Aufrufer noch Referenzen auf Gruppe und Node halten.
  Der echte Workjet-TypeScript-Client wurde jetzt über diesen vollständigen
  Drei-Peer-WebRTC-/IPC-Lifecycle geprüft: Auftrag, Freigabe, Command-Replay,
  Quorumverlust und Endpunkt-Shutdown. Ein separater Test erhält den bisherigen
  Nachweis, dass Shutdown auch bei weiter verfügbaren anderen Stimmen sperrt.
  Ein abgebrochener Listener-Task kann anschließend gefahrlos aufgeräumt werden;
  sein bereits abgeschlossenes JoinHandle wird nicht erneut gepollt.
  Dies ersetzt den getrennten Listener-Aufbau für konfigurierte native Gruppen,
  aber noch nicht die fehlende produktive Gruppenkonfiguration oder den
  Workjet-Ausführungspfad. Nicht-Unix-Plattformen ohne zertifizierten Listener
  lehnen einen solchen Attach ausdrücklich ab.
- Workjets ACP-Prozessstart besitzt jetzt wie Codex eine erzwungene Beendigung
  nach zwei Sekunden. Ein echter Prozess samt Kindprozess, die beide SIGTERM
  ignorieren, reproduzierte zuvor den Hänger und wird jetzt beim Scope-Ende
  beendet. Das ist noch keine Autoritätsüberwachung. Der Fall eines schon
  erfolgreich beendeten Elternprozesses mit weiterlaufenden Kindern sowie
  Claudes SDK-Prozessbeendigung brauchen eigene Umsetzung und Abnahme.

- Im aktiven Workjet-`ProviderService` ist der bisherige Ersatzstart vor dem
  Stoppen der alten Instanz entfernt. Start, Recovery, Turn-/Approval-Aufrufe,
  Stop und Rollback werden lokal pro Thread über Provider-Grenzen serialisiert.
  Andere Threads bleiben unabhängig. Stop-Fehler brechen den Wechsel ab; ein
  nach Stop weiterhin vom Adapter gemeldeter Prozess blockiert ihn ebenfalls.
  MCP-Freigaben werden vor Stop entzogen. Bestätigt gestoppte alte Bindings
  behalten ihren Resume-Zustand, auch wenn der anschließende Start scheitert.
  Diese lokale Serialisierung ersetzt keine bestätigte verteilte Autorität.
- Der Claude-Adapter hatte zusätzlich zwei stille Freigaben: `query.close()`-
  Fehler wurden geschluckt und ein erfolgreicher Exit gemeldet; der interne
  Ersatzstart ignorierte ebenfalls Stop-Fehler. Beide Pfade sind entfernt.
  Fehlgeschlagene Schließungen bleiben als Fehler-Session sichtbar und sperren
  weitere Turns; Stop ist wiederholbar. Erst erfolgreiches Close entfernt die
  Session. Gleichzeitige Close-Aufrufe für dieselbe Query werden serialisiert.
  Eine injizierte SDK-Close-Ausnahme reproduziert den alten Fehler. Dies ist
  noch kein Nachweis über das Ende aller echten SDK-Kindprozesse und noch keine
  Bindung an Raft-Generation oder Quorumverlust.

## Konkrete Ablöseliste

| Bisheriger aktiver Pfad / Aufrufer | Ersatz | Entfernungstest / Abnahme | Zustand |
|---|---|---|---|
| `shared/db.js`, zwei Imports in `shared/sync.js`; unterschiedliche Bundle-Versionen und Zeitstempel-Retry | `shared/rxdb-runtime.js`, genau ein Import-Promise und eine URL | Singleton-Smoke plus Data-Plane-Guard verbieten weitere direkte versionierte Imports | **ersetzt und entfernt** |
| Workjet Mailbox und HandoffSnapshot mit eingeschränkter Session-Zusammenfassung | nativer Sync-Lifecycle + typisiertes lokales IPC + vollständige Checkpoints | A→B mit echter Historie/Dateien/Harness-Resume; keine aktiven Mailbox-Aufrufer | aktiv, Ablösung offen |
| unverbundene Workjet Ownership-Resolver und lokale Statusreparatur | bestätigte Authority plus abgeleitete Projektion | Partition, alter Besitzer, doppelte Commands und Projektionsneubau | aktiv, Ablösung offen |
| Codex stiller Resume→Start-Fallback | Fehler weitergeben; ein neuer Thread erfordert einen ausdrücklichen Start | sechs Resume-Fehler führen ausschließlich `thread/resume` aus; expliziter Neustart separat geprüft | **Fallback entfernt**; vollständige Portabilitätszertifizierung offen |
| Workjet Ersatzstart vor altem Stop; geschluckte Stop-/Claude-Close-Fehler | serialisierte Session-Umstellung mit bestätigtem Adapter-Stop | Stop-Fehler, falsche Stop-Bestätigung, konkurrierende Starts/Recovery, Close-Fehler mit erneutem Stop | **stille Fortsetzung entfernt**; native Autoritätsanbindung und echte Prozessgruppen-Abnahme offen |
| native `rxdb_peer.rs` mit Business-OS-Projektion/Commands im Peer-Lifecycle | gemeinsamer Lifecycle mit Workjet-/Produktionsadaptern | beide Adapter über denselben Kern; Neustart-/Heartbeat-Guards bleiben grün | Business-OS-Transportstart an `NativeSyncSession` angeschlossen, lokaler Start-/Abbruchpfad entfernt; Workjet-Adapter und gesamte Etappenabnahme offen |
| Server-Shell-Auflösung mit stillen Source-/Archiv-Ersatzpfaden | kanonischer Release-Resolver, ausdrückliche Recovery | falsche Version, kaputter Slot, Release-Ausfall | aktiv, Ablösung offen |
| Workjet Guest-Zerstörung beim Ansichtswechsel | persistenter Instanz-Lifecycle; Suspend separat | warmer Wechsel ohne vollständigen Bootstrap, Mobile-Suspend/Resume | aktiv, Ablösung offen |
| doppelte fachliche Writer und alte Stores | Migration und ausschließlich neue Writer | Kopienmigration, Referenzbilanz, Wiederherstellungsprobe | aktiv, Ablösung offen |

## Priorität: Worker robust aufnehmen

SSH-Installation und QR-/Link-Aufnahme müssen in dieselbe bestätigte
Worker-Mitgliedschaft münden. Die Abnahme endet erst nach einem tatsächlichen
Coding-Auftrag auf dem hinzugefügten Rechner und erneuter Verbindung nach dessen
Neustart; Desktop und Mobile müssen denselben bestätigten Zustand anzeigen.
Verwaltungsfunktionen ersetzen diesen Ablauf nicht.

Quellcodebefund vom 2026-09-05 im Workjet-Checkout:

| Einstieg | Vorhandener Pfad | Fehlender Anschluss |
|---|---|---|
| SSH | `apps/desktop/src/provisioning/DesktopComputerProvisioner.ts`: Host-Key-/Systemprüfung, CTOX-Installation, optionale Workjet-Installation, CTOX-Registry | Workjet bleibt ungestartet; keine bestätigte Worker-Aufnahme und kein Ausführungstest |
| Mobile QR/Link | `apps/mobile/src/features/pairing/WorkjetDevicePairingProvider.tsx`: Business-OS-Invite importieren und Gerätebindung abwarten | Gerätebindung verleiht noch keine native Worker-Identität, Mitgliedschaft oder Ausführungsfähigkeit |
| Mobile SSH | `apps/web/src/components/settings/ComputerProvisioningSection.tsx` benötigt Desktop-Bridge-Methoden | Mobil erreichbare Installation über einen berechtigten, netzwerkfähigen Host fehlt in diesem Pfad |

Konkrete Kernhindernisse für diese Aufnahme:

1. `NativeExecutionGroup` ignorierte konfigurierte Routen mit Signaling-Rolle
   `workjet_executor`. Der Regressionstest zeigte trotz angenommenem Raum null
   Datenkanäle und keine Raft-Mehrheit. Die Discovery akzeptiert jetzt beide
   nativen Rollen; Schlüsselprüfung und Business-Datenberechtigungen bleiben
   erforderlich.
2. Die drei Raft-Stimmen bleiben fest konfiguriert. `State.workers` enthält
   jetzt separat mehrheitlich bestätigte Ausführungsrechner. `AdmitWorker` und
   `RevokeWorker` werden mit Journalquittung atomar gespeichert und in Snapshots
   übernommen. Zusätzliche Worker dürfen `Propose`/`Validate` senden, aber keine
   Raft-RPCs und keine Mitgliedschaftsänderungen. Die State-Machine prüft ihre
   aktuelle Berechtigung; Validate prüft sie nach dem Quorum-Read erneut.
   Widerrufene IDs bleiben gesperrt, auch nach Replay und Wiederaufnahme mit
   demselben Schlüssel unter einer neuen ID. Native IPC und generierte Rust-/TS-
   Verträge enthalten Aufnahme/Widerruf und getrennte Mitgliedschaftsquittungen.
   Der Kontrollvertrag ist dafür Version 3. Der signierte Vier-Rechner-Test
   besteht einschließlich Snapshot und Neustart; der zusätzliche reine
   Mitgliedschaftstest prüft ungültige IDs/Schlüssel, doppelte Identitäten,
   Replay nach Widerruf und erneute Aufnahme unter neuer ID. Die vollständige
   Cluster-Nachprüfung besteht jetzt mit 9/9 Tests, einschließlich verweigerter
   Aufnahme ohne Mehrheit. `WorkerAuthorityClient` ist jetzt als signierter
   Client ohne eigenes Raft implementiert; `AuthorityIpc` und der private Listener
   verwenden dafür dieselbe `ExecutionAuthority`-Schnittstelle wie Raft-Knoten.
   Sein Einzeltest besteht (0,86 s), einschließlich verlorener Commit-Antwort,
   Replay, Leader-Wechsel, Widerruf und Shutdown. `NativeExecutionGroup` und
   `NativeExecutionWorker` verwenden jetzt denselben generischen Start-/Stop-
   Supervisor. `NativeSyncSession::attach_worker` hängt den nicht stimmberechtigten
   Client an den tatsächlichen Pool und den privaten IPC-Listener; der echte
   Vier-Rechner-Test besteht in 2,91 s. Produktive
   Einladungen/Schlüsselbeweise und Administratorberechtigung sind nicht ersetzt.
   `connect_native_execution_peer` lässt weiterhin nur die kleinere Signaling-ID
   initiieren. Für Worker ist nun `connect_worker_to_authority_peer` ergänzt:
   Der Worker initiiert einseitig zu seinen konfigurierten Stimmen, die ihn
   selbst nicht entdecken. Der gemeinsame Supervisor wählt diese Regel nur bei
   einer Worker-Anbindung; die bestehende Voter-Verbindung behält ihre Regel.
   Der neue Vier-Rechner-Test setzt den Worker ausdrücklich auf die größte ID
   und prüft reale DataChannels, IPC, Aufnahme/Widerruf, gesperrte Business-Daten
   und das Erlöschen erhaltener Handles bei Shutdown. Dieser Test besteht;
   die bisherigen Native-/Voter-Szenarien werden erneut geprüft. Das ist keine
   Harness-Ausführung oder Desktop-/Mobile-Aufnahme: SSH-/QR-Produkteinstieg
   und wechselnde Signaling-IDs bleiben offen.
3. Der native RxDB-Protokoll-Handshake setzte `peerSession.role` fest auf
   `ctox_instance`. Das ist durch `NativePeerRole` aus dem gemeinsamen
   `webrtc-rxdb-protocol.json`-Vertrag ersetzt. `NativeSyncOptions.peer_role`
   legt den Wert vor dem Start fest; derselbe Wert gilt für Antworten und
   ausgehende Probes. Business OS wählt ausdrücklich `CtoxInstance`, Worker
   verwenden `WorkjetExecutor`. Discovery und Verbindungsaufbau verwenden
   denselben generierten Rollenparser. Produktive Worker-Identitätsprüfung
   und Provisionierung bleiben anzuschließen; die Rollenangabe gewährt keine
   Datenberechtigungen. Die beiden Rollen-Fixtures verlangen nun auch im
   Protokoll-Handshake den passenden Wert je Session.
4. Routen und öffentliche Schlüssel werden noch als fertige lokale Konfiguration
   übergeben. Einladung, Besitznachweis, bestätigter Einmalverbrauch und
   Wiederverbindung mit neuer Signaling-ID sind noch nicht
   als Produktablauf angeschlossen. Ein Anzeigename oder Raumtoken genügt nicht.

Für QR/Link muss der gesamte Aufnahmeablauf ohne direkten Netzwerkzugang zum
Ziel über Signaling/WebRTC funktionieren. Für SSH dient der Admin-Zugang der
Installation und dem Start desselben Worker-Runtimes. Erst nach bestätigter
Aufnahme und aktueller Kompatibilitäts-/Fähigkeitsprüfung darf die UI den
Rechner als ausführungsbereit anzeigen. Abbruch, doppelte Einladung, falscher
Schlüssel, fehlende Mehrheit, unterbrochene Installation und Wiederaufnahme
gehören in die End-to-End-Abnahme. Keiner dieser vollständigen Produktabläufe
ist bisher abgenommen.

## Abnahmen und nächste Integrationsschritte

1. Peer-Provisionierung und Transport-Lifecycle anschließen. Die signierte
   Kontrollübertragung ist sowohl simuliert als auch über echte lokale
   WebRTC-Verbindungen geprüft; WAN und Host-Room-Admission folgen.
2. Auftragsautorität an lokale IPC-Commands, Executor-Supervisoren und tatsächliche
   Gateway-/Tool-Grenzen binden. Ohne diesen Schritt ist Etappe 2 nicht abgenommen.
3. Die vorhandenen authentifizierten Kopierquittungen an den produktiven
   Dateitransfer anschließen. Vor Übernahme die Daten auf dem Ziel erneut prüfen;
   eine frühere Quittung ersetzt keinen verfügbaren Wiederherstellungsstand.
4. Codex/Claude vollständig exportieren, importieren und verifiziert fortsetzen;
   Credentials nur als berechtigungsgeprüfte Referenzen behandeln.
5. Shell-/Instanzresolver und warme Host-Lifecycles umstellen; ersetzte Pfade jeweils
   löschen. Bestehende Shell-Guard-Befunde nicht durch Ausnahmen verbergen.
6. Reale Bestände auf Kopien migrieren, Wiederherstellung proben, koordinierte
   Umschaltung und Entfernung alter ausführbarer Pfade durchführen.

Zusätzlich offen: Grenzwerte und Wachstum von Raft-Receipts/Wirkungsjournal,
Snapshot-Übertragung großer Bestände, Windows-Restore-Zertifizierung und realer
mobiler Suspend. Failover, WAN und große Sessions erhalten eigene Messreihen.
Die bestehenden lokalen Ziele bleiben Command-p50 <300 ms und kritische
Boot-Collections im p95 <5 s. Sie sind noch nicht neu nachgewiesen.

## Verifikation des neuen Kerns

Nach der nativen Worker-Anbindung: der echte Vier-Rechner-Test besteht einzeln
(2,91 s) und im anschließenden Regressionstest erneut. Der Regressionstest
besteht insgesamt mit 13/14 Szenarien: alle sieben Native-Lifecycle-Tests und
sechs von sieben WebRTC-Tests. `native_sessions_own_authority_without_granting_business_data_access`
scheitert nach bestätigtem Create beim Quorum-Read mit lediglich Stimme 1.
Clippy mit `--features webrtc --all-targets -- -D warnings` besteht.
Die Quorum-Assertion enthält jetzt zusätzlich Kanal-/Queue-/Admission-Zustand
aller drei Peers, und die sieben WebRTC-Szenarien werden erneut geprüft.
Dieser Stand ist keine Produktionsfreigabe.

Offener Transportbefund: `classify_send_frame` in
`connection_handler_rs.rs` priorisiert nur `ctoxProtocol` und `token` intrinsisch
hoch; `ctox.sync.authority.v3`-Anfragen laufen als normale Nachrichten durch
denselben ACK-serialisierten Drainer wie Business-Daten. Antworten sind hoch
priorisiert. Vor großen Checkpoint-/WAN-Tests ist die Verzögerung von
Koordinationsverkehr unter Datenlast nachzuweisen und gegebenenfalls die
Kontrollübertragung strukturell zu trennen. Das ist noch kein Beweis für die
Ursache des aktuellen Fehlers im ruhigen Voter-Szenario.

Aktueller Nachlauf nach Einführen der nativen Worker-Protokollrolle: **noch
nicht freigegeben**. Der Rollen-Vertragsgenerator und die Drift-Prüfung bestehen,
das Browser-Bundle ist mit esbuild 0.28.0 neu erzeugt (kanonische URL v329).
Die JS-Suite meldet 109 bestanden, dieselben drei Inventarfehler und zwei
übersprungene Cross-Process-Tests; der zuvor vorhandene Wire-Daemon fehlt nach
Bereinigung der Build-Artefakte und muss neu gebaut werden.
Der erste Kern-Nachlauf scheiterte in zwei WebRTC-Szenarien. Ein Peer blieb
ohne Raft-Log, obwohl alle Datenkanäle offen und aufgenommen waren; eine
Sendewarteschlange enthielt 248 ausstehende Frames. Im Sender lag zwischen
beanspruchtem Drain-Slot und Installation des Abbruchschutzes ein `yield_now`.
Dieser Yield liegt nun innerhalb des geschützten Drainers. Ein zusätzlicher
Test bricht den tatsächlichen ersten Send-Poll am Yield ab und verlangt den
freigegebenen Slot. Vollständige Nachprüfung steht aus; die vorherige grüne
Discovery-Abnahme unten ist kein Beleg für diese weiteren Änderungen.

Aktueller nativer RxDB-Lauf: 369/370 Unit-Tests bestanden. Darunter sind die
neuen Rollen- und Send-Abbruch-Prüfungen. `rate_limit_kicks_in_after_burst` ist
auch einzeln rot; die nachgelagerten Conformance-/Guard-Binaries liefen deshalb
nicht. Der Test führt 33 vollständige Queries aus, während der echte Token-Bucket
nach einer Sekunde auffüllt; der Einzeltest dauerte 1,63 s. Das ist ein konkreter
Zeitabhängigkeitsbefund, keine Freigabe oder Änderung des Rate-Limits.
Der Workjet-IPC-Testlauf nach Vertragserweiterung besteht mit 6/6 Tests,
einschließlich Aufnahme-, Replay- und Widerrufsquittungen; die
fünf generierten Vertragsdateien bestehen die Drift-Prüfung.
Die erste Prüfung des zusätzlichen Workers enthielt vertauschte Parameter
im vorhandenen Testhelfer `ownership(generation, node_id)`; nach Korrektur
besteht dieses Szenario. Im nächsten parallelen Cluster-Lauf scheiterte der
bisherige IPC-Wirkungstest. Seine Assertion zeigt jetzt den tatsächlichen
Rückgabewert an; keine Deadline oder Schutzbedingung wurde gelockert.
Der erneute parallele Lauf vor der Worker-Client-Anbindung bestand mit 9/9 Cluster-Tests in
6,12 s. Der zwischenzeitliche IPC-Ausfall bleibt als nicht reproduzierter
Befund offen; eine technische Ursache ist damit nicht nachgewiesen.
Zusätzlich bestand der reine Mitgliedschaftstest. Der folgende Kernlauf mit
`webrtc` bestand die sieben Signatur-/Host-Unit-Tests, scheiterte aber in allen
zehn Cluster-Szenarien an Leader-/Commit-/Gesamt-Deadlines. Nachgelagerte
WebRTC- und Storage-Konformitätstests liefen deshalb nicht. Der neue Client ist
damit nicht abgenommen. Ein gezielter Einzeltest läuft zur Eingrenzung; die
Deadlines bleiben unverändert.

Die Cluster-Testdatei wartete vorher mehrere Minuten vor dem Programmeinstieg.
Eine `sample`-Aufnahme zeigte ausschließlich `_dyld_start`, 112 KB Speicher und
keine Rust-Threads. Das 26-MB-Binary ist linker-signiert; es trägt keinen
Quarantäne-xattr. Gleichzeitig wurden etwa 6 MB/s auf dem SD-Volume und
9,3 GB belegter Swap gemessen. Ein anschließender isolierter SQLite-WAL/FULL-
Test mit 30 Commits auf derselben Ablage ergab p50 8,3 ms, p95 22,7 ms und
maximal 24,1 ms. Diese Beobachtungen ersetzen weder die Ursachenklärung der
Cluster-Fehler noch die eigentliche Performance-Fixture.

Build-Ablage: Nach neuer lokaler Vorgabe wurde der eigene gestoppte
`runtime/build/sync-core`-Target nach
`/Volumes/tmp/dev-artifacts/ctox/sync-core-offensive/cargo-target` verschoben;
der bisherige Pfad ist ein Symlink. Der Umzug ist bestätigt abgeschlossen,
der neue Mitgliedschafts-Build läuft dort. Kopierte Hardlinks sind noch zu
konsolidieren, sobald kein Build das Target benutzt. Das Standard-Cargo-Target
zeigt ebenfalls auf `/Volumes/tmp`. Quellcode liegt weiter
im kanonischen `main`-Checkout, ohne eigenen Worktree oder PR.
`diskutil info /Volumes/tmp` meldet Secure Digital / Journaled HFS+ mit
deaktivierter Ownership. Der Kaltbuild und die Migration zeigen lange
I/O-Wartezeiten. Messungen auf dieser temporären Ablage dürfen nicht als
erneute Abnahme der bisherigen lokalen Performance-Fixture ausgegeben werden;
deren Grenzwerte bleiben unverändert.

Abgeschlossene Prüfung vor der weiteren Rollen-/Senderänderung:

Der Kern-Testlauf mit aktiviertem `webrtc`-Feature umfasst jetzt 37 Tests:
3 Signatur-/Replay-Prüfungen, 4 lokale Host-Lifecycle-Prüfungen,
7 persistente Drei-Peer-Szenarien, 5 Checkpoint-
Prüfungen, 1 Wirkungs-Wiederholungstest, 2 Wire-Ablehnungsprüfungen, die vollständige
OpenRaft-Storage-Suite, 7 native Transport-Lifecycle-Prüfungen,
1 Node/Rust-Identitätsimport, 5 vollständige Native-Session-/Authority-Gruppenläufe
und 1 echter WebRTC-Ablauf mit Partition, Übernahme,
Rückkehr und Ablehnung von Browser-/unbekannten Rollen. Der WebRTC-Test benutzt
jetzt signierte Quittungen aus zwei unabhängig gespeicherten synthetischen
Checkpoints. Die generierten Rust-/TypeScript-Verträge werden gemeinsam geprüft.
Der vollständige Lauf ist nach der Worker-Discovery-Korrektur grün (37/37).
Der neue Worker-Rollen-Test scheiterte vorher mit null Datenkanälen und
Raft-Leader-Timeout; beide Rollen-Szenarien bestehen jetzt. Sie prüfen weiterhin
synthetische Raumaufnahme, keine produktive Worker-Mitgliedschaft.
Nach dieser Korrektur sind Clippy (`--all-targets -- -D warnings`) und die
Formatprüfung erneut grün. Der Daemon-Check besteht erneut mit 476 Warnungen;
sein lokaler Beleg liegt in
`runtime/sync-core-offensive/worker-discovery-check.log`.
Die sieben Transport-Prüfungen verwenden echte lokale Signaling-Sockets und prüfen Ablehnung,
Deadline, Abbruch während Raumfreigabe, expliziten Shutdown und Drop einer
laufenden Session, die Ablehnung einer Gruppe für einen anderen Raum sowie die
Verarbeitung der ersten Signaling-Nachricht unmittelbar beim Raumbeitritt.
Nach Shutdown bleibt die Host-Datenbank beschreibbar.
Clippy mit `-D warnings`, die Formatprüfung und alle fünf generierten
Wire-Verträge sind grün. Die vollständige JS-Suite wurde erneut ausgeführt:
110 bestanden, die gleichen drei Inventarprüfungen fehlgeschlagen, keine
übersprungen. Der vollständige Daemon-Check (`cargo check -p ctox -j 2`)
besteht mit 476 gemeldeten Warnungen; er ist keine warnungsfreie Freigabe.
Der native RxDB-Testlauf ist nach Freigabe der Cargo-Artefaktsperre vollständig
grün und wurde nach der Aufnahmeprüfung erneut ausgeführt: 368 Unit-Tests,
31 Conformance-Tests, Error-Guard und Idle-Budget-Test bestanden. Ein neuer
Schutztest verlangt beide Handshake-Richtungen und prüft Disconnect/Shutdown.
Die reale Daemon-/Client-Laufzeitabnahme steht weiterhin aus.
Nach der IPC-Lifecycle-Anbindung wurde der Daemon-Check erneut erfolgreich mit
476 Warnungen ausgeführt; der neue Kern besteht Clippy mit `-D warnings`.
Der native RxDB-Testlauf enthält zusätzlich die Prüfung, dass eine doppelte
Registrierung des Kontrollhandlers den ersten Besitzer nicht überschreibt.

Die beiden Crates verwenden zurzeit eigene Cargo-Lockfiles. Der isolierte neue
Kern löste WebRTC 0.20.5 auf, der bestehende RxDB-Lauf 0.20.0-alpha.1. Beide
bauen mit dem vorhandenen RxDB-Code; eine koordinierte Release-Konfiguration
muss diese Abhängigkeiten und die ausgelieferte Native-Version explizit vereinen.
Daraus folgt keine bereits erfolgte Runtime-Kompatibilitätsfreigabe.

Workjet: zuvor 64 Tests in CodexSessionRuntime, ProviderService, generierten
Sync-Verträgen, lokalem IPC und ACP-Prozessabschaltung bestanden. Die vorhandenen
ACP-Protokolltests bestanden zusätzlich. Beim aktuellen Statuscheck wurden
115 Tests in sechs Dateien erneut erfolgreich ausgeführt: Device-Verträge,
GuestManager, Desktop-IPC, tatsächlicher Preload, Geräte-Pairing-Ablauf und
Legacy-Konfigurationsmapping. Device-Antworten haben jetzt einen kanonischen
validierten Vertrag; der GuestManager lehnt fehlerhafte und zur Aktion unpassende
Antworten ab. Der Preload verwendet den tatsächlich registrierten Gerätekanal.
Die veralteten Test-Typnamen und das fehlende `managerThreadReference` im
Legacy-Mapping sind korrigiert; Schutzprüfungen bleiben erhalten.

Ein vollständig grüner Typecheck aller Workjet-Pakete ist weiterhin nicht
nachgewiesen. Zuletzt bekannte weitere Befunde betreffen fehlende Effect-Dienste
in `McpHttpServer.test.ts` und `ManagerTool.ts` sowie DesktopTestProfile.
Die gezielten grünen Tests ersetzen keine Desktop-/Mobile-End-to-End-Abnahme
und keine Produktionsfreigabe.

Nach der lokalen Executor-Umstellung: **111 Tests** in `ProviderService.test.ts`
und `ClaudeAdapter.test.ts` bestanden. Neue Szenarien prüfen Stop vor Start,
weitergereichte Stop-Fehler, falsche Stop-Bestätigung, Erhalt des Resume-Bindings
nach gescheitertem Ersatzstart, gleichzeitige Starts und Recovery bei weiter
bedienbaren anderen Threads sowie Claude-Close-Fehler mit gesperrtem Ersatz und
anschließendem erfolgreichen Stop. Der Claude-Test wurde vor der Änderung rot
reproduziert. Die vier geänderten TS-Dateien bestehen gezieltes Lint und Format.
Der Server-Paket-Typecheck wurde erneut ausgeführt: keine Diagnostik in diesen
Dateien; weiterhin TS2375/TS377004 wegen fehlender Effect-Dienste in den beiden
oben genannten MCP-Dateien. Keine Desktop-/Mobile- oder echte SDK-Prozessgruppen-
Abnahme aus diesen Adapter-Fixtures ableiten.


## Reproduzierbare Prüfkommandos


Alle Kommandos aus dem CTOX-Root, durch `greppy bash-smart --` ausführen:

```sh
cargo test --manifest-path src/core/sync/Cargo.toml --target-dir runtime/build/sync-core --features webrtc -j 2
node src/core/sync/tools/generate-contracts.mjs --check --workjet-root ../workjet
node src/apps/business-os/rxdb/tests/runtime-singleton-smoke.mjs
node src/apps/business-os/rxdb/tests/data-plane-guard-smoke.mjs
node src/apps/business-os/rxdb/tests/run-all.mjs
cargo test --manifest-path src/core/rxdb/Cargo.toml
node src/apps/business-os/scripts/assert-shell-v2-contract.mjs
```

Ein grüner Kern-Testlauf allein erfüllt weder die komplette Failover-Abnahme noch
die Shell-, Migrations- oder Production-Ready-Kriterien.
